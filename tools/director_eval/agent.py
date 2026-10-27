#!/usr/bin/env python3
"""The director loop the eval drives: a model + kimo-mcp over stdio, one tool call at a time.

Two adapters, so the same loop runs a frontier model and a small one:
  anthropic  Messages API (ANTHROPIC_BASE_URL / ANTHROPIC_API_KEY, prompt caching on)
  openai     any OpenAI-compatible chat/completions endpoint (OPENAI_BASE_URL / OPENAI_API_KEY), e.g. Llama / Muse hosts
Everything is logged to <out>/log.jsonl (one line per model step and per tool call) and <out>/run.json (totals).
"""
import argparse, base64, json, os, re, sys, time
import requests
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp import Kimo

# $ per million tokens: input, output, cache read, cache write. Extend for your model.
PRICES = {
    "claude-opus-5-5": (4.0, 20.0, 0.20, 5.0), "claude-haiku-4-5": (1.0, 5.0, 0.10, 1.25), "claude-sonnet-5": (2.0, 10.0, 0.20, 2.5),
    # Muse Spark Open: the OpenRouter list price (2026-10-21); the Fireworks bill itself was not read
    "accounts/fireworks/models/muse-spark-open": (1.25, 4.25, 0.15, 1.25),
    "accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b": (None, None, None, None),
    # Muse Spark 1.3 Contributor, via OpenRouter: list price read 2026-10-26 off openrouter.ai/meta/muse-spark-1.3-contributor
    # (permaslug meta/muse-spark-1.3-contributor-20260902). cache_write not published there; priced at the input rate as a floor, not billed.
    "meta/muse-spark-1.3-contributor": (0.10, 0.20, 0.002, 0.10),
}
RESULT_CAP = int(os.environ.get("DIRECTOR_RESULT_CAP", "12000"))  # characters of a tool result the model sees
KEEP_IMAGES = 2  # only the newest images stay in the context

MAGIC = {"image/png": b"\x89PNG", "image/jpeg": b"\xff\xd8", "image/gif": b"GIF8", "image/webp": b"RIFF"}
def good_images(imgs):
    """Keep only image blocks whose bytes are the image they claim to be (a tool once handed back an mp4 as image/png)."""
    out = []
    for mt, d in imgs:
        try: head = base64.b64decode(d[:24] + "=" * (-len(d[:24]) % 4))
        except Exception: continue
        if any(head.startswith(m) for m in MAGIC.values()): out.append((mt if head.startswith(MAGIC.get(mt, b"?")) else next(k for k, m in MAGIC.items() if head.startswith(m)), d))
    return out

def cap(t):
    return t if len(t) <= RESULT_CAP else t[:RESULT_CAP] + f"\n[... {len(t) - RESULT_CAP} more characters cut by the client]"


_UNCALLABLE_RE_TOOL = re.compile(r'"tool"\s*:\s*"([a-zA-Z_][a-zA-Z0-9_]*)"')
_UNCALLABLE_RE_NAME = re.compile(r'"name"\s*:\s*"([a-zA-Z_][a-zA-Z0-9_]*)"')
_UNCALLABLE_RE_ACTION = re.compile(r'"action"\s*:\s*"([a-zA-Z_][a-zA-Z0-9_]*)"')
_UNCALLABLE_NOTE = (" For text/caption style after creation (font, color, size, stroke, box), delete_clip the clip and "
                    "recreate it with add_title's or add_text's own style args, then set_property for position/scale/"
                    "opacity/rotation only -- do not keep searching or describing it, you cannot call it here.")

def flag_uncallable_tools(name, out, names):
    """A curated tool profile (e.g. director mode's ~35 tools) only lets the model emit tool_calls for the
    functions declared in its own `tools` request array. lint_edit's own `fix.tool` field and search_tools'
    full-447-tool-registry matches can both name a real tool that is outside this profile -- the schema the
    model was given has no such function, so it cannot actually invoke it (confirmed: tools/call still
    dispatches it server-side, but the model's own tool-calling API blocks emitting an undeclared call), and
    a model driven this way (seen repeatedly on Muse Spark Open/1.3 Contributor) loops calling kimo_describe
    and search_tools on that name over and over instead of using the real, reachable fix. This flags exactly
    that case right where it would otherwise start a loop, rather than relying on the model to notice."""
    try:
        mentioned = set(_UNCALLABLE_RE_TOOL.findall(out))
        if name == "search_tools": mentioned |= set(_UNCALLABLE_RE_NAME.findall(out))
        if name == "kimo_describe":
            m = _UNCALLABLE_RE_ACTION.search(out)
            if m: mentioned.add(m.group(1))
    except Exception:
        return out
    missing = sorted(t for t in mentioned if t and t not in names)
    if not missing: return out
    return out + f"\n[harness: {', '.join(missing)} named above is not callable in this tool profile (not in your tools list)." + _UNCALLABLE_NOTE + "]"


RETRY_LOG = []  # provider retries this run, written to run.json
def post_retry(url, headers, body, tag, need_choices=False, tries=7):
    """POST with backoff on anything transient: connection errors, timeouts, 408/409/425/429, every 5xx, a 200 whose
    body is not JSON or has no choices, and provider error bodies that say model_error/overloaded. A 4xx that is
    about the request itself is raised at once (retrying it would only repeat it)."""
    last = ""
    for attempt in range(tries):
        try:
            r = requests.post(url, headers=headers, json=body, timeout=300)
        except requests.RequestException as e:
            last = f"{tag} transport: {e!r}"[:300]
        else:
            if r.status_code == 200:
                try:
                    j = r.json()
                    if not need_choices or (j.get("choices") and j["choices"][0].get("message") is not None): return r
                    last = f"{tag} 200 without choices: {r.text[:200]}"
                except ValueError: last = f"{tag} 200 not json: {r.text[:200]}"
            else:
                t = r.text[:500]; last = f"{tag} {r.status_code}: {t}"
                transient = r.status_code in (408, 409, 425, 429) or r.status_code >= 500 or any(w in t.lower() for w in ("model_error", "overloaded", "temporarily", "try again"))
                if not transient: raise RuntimeError(last)
        wait = min(60, 3 * 2 ** attempt); RETRY_LOG.append({"attempt": attempt + 1, "why": last[:200], "wait_s": wait, "at": round(time.time(), 1)})
        time.sleep(wait)
    raise RuntimeError(f"{last} (after {tries} tries)")

def mcp_to_fn_tools(tools):
    return [{"name": t["name"], "description": t.get("description", "")[:1024], "input_schema": t.get("inputSchema") or {"type": "object", "properties": {}}} for t in tools]

class Anthropic:
    def __init__(self, model, base, key, vision=True):
        self.model, self.base, self.key, self.vision = model, base.rstrip("/"), key, vision
    def step(self, system, tools, msgs):
        tl = [dict(t) for t in tools]; tl[-1] = dict(tl[-1], cache_control={"type": "ephemeral"})
        m = [dict(x) for x in msgs]
        # rolling cache breakpoint on the newest user turn
        for i in range(len(m) - 1, -1, -1):
            if m[i]["role"] == "user":
                c = m[i]["content"]; c = [{"type": "text", "text": c}] if isinstance(c, str) else [dict(b) for b in c]
                c[-1] = dict(c[-1], cache_control={"type": "ephemeral"}); m[i] = dict(m[i], content=c); break
        body = {"model": self.model, "max_tokens": 4096, "system": [{"type": "text", "text": system, "cache_control": {"type": "ephemeral"}}], "tools": tl, "messages": m}
        r = post_retry(f"{self.base}/v1/messages", {"x-api-key": self.key, "anthropic-version": "2023-06-01", "content-type": "application/json"}, body, "anthropic")
        j = r.json(); u = j.get("usage", {})
        calls = [(b["id"], b["name"], b.get("input") or {}) for b in j["content"] if b["type"] == "tool_use"]
        text = "".join(b.get("text", "") for b in j["content"] if b["type"] == "text")
        usage = {"in": u.get("input_tokens", 0), "out": u.get("output_tokens", 0), "cache_read": u.get("cache_read_input_tokens", 0), "cache_write": u.get("cache_creation_input_tokens", 0)}
        return {"role": "assistant", "content": j["content"]}, calls, text, usage
    def tool_results(self, results):
        content = []
        for cid, is_err, text, imgs in results:
            c = [{"type": "text", "text": text or "(empty)"}]
            if self.vision: c += [{"type": "image", "source": {"type": "base64", "media_type": mt, "data": d}} for mt, d in imgs]
            content.append({"type": "tool_result", "tool_use_id": cid, "content": c, "is_error": is_err})
        return [{"role": "user", "content": content}]
    def prune_images(self, msgs):
        seen = 0
        for m in reversed(msgs):
            if m["role"] != "user" or isinstance(m["content"], str): continue
            for b in m["content"]:
                if b.get("type") == "tool_result" and isinstance(b.get("content"), list):
                    keep = []
                    for x in b["content"]:
                        if x.get("type") == "image":
                            seen += 1
                            if seen > KEEP_IMAGES: keep.append({"type": "text", "text": "[older image dropped]"}); continue
                        keep.append(x)
                    b["content"] = keep

class OpenAICompat:
    def __init__(self, model, base, key, vision=True):
        self.model, self.base, self.key, self.vision = model, base.rstrip("/"), key, vision
    def step(self, system, tools, msgs):
        tl = [{"type": "function", "function": {"name": t["name"], "description": t["description"], "parameters": t["input_schema"]}} for t in tools]
        body = {"model": self.model, "max_tokens": 4096, "messages": [{"role": "system", "content": system}] + msgs, "tools": tl, "temperature": 0.2}
        r = post_retry(f"{self.base}/chat/completions", {"Authorization": f"Bearer {self.key}"}, body, "openai", need_choices=True)
        j = r.json(); msg = j["choices"][0]["message"]; u = j.get("usage", {})
        calls = []
        for tc in msg.get("tool_calls") or []:
            try: a = json.loads(tc["function"].get("arguments") or "{}")
            except ValueError: a = {"__unparsed__": tc["function"].get("arguments")}
            calls.append((tc["id"], tc["function"]["name"], a))
        keep = {"role": "assistant", "content": msg.get("content") or ""}
        if msg.get("tool_calls"): keep["tool_calls"] = [{"id": tc["id"], "type": "function", "function": {"name": tc["function"]["name"], "arguments": tc["function"].get("arguments") or "{}"}} for tc in msg["tool_calls"]]
        cached = (u.get("prompt_tokens_details") or {}).get("cached_tokens", 0) or 0
        usage = {"in": u.get("prompt_tokens", 0) - cached, "out": u.get("completion_tokens", 0), "cache_read": cached, "cache_write": 0}
        return keep, calls, msg.get("content") or "", usage
    def tool_results(self, results):
        out, imgs_all = [], []
        for cid, is_err, text, imgs in results:
            out.append({"role": "tool", "tool_call_id": cid, "content": ("ERROR: " if is_err else "") + (text or "(empty)")}); imgs_all += imgs
        if imgs_all and self.vision:
            out.append({"role": "user", "content": [{"type": "text", "text": "Images returned by the tool calls above:"}] + [{"type": "image_url", "image_url": {"url": f"data:{mt};base64,{d}"}} for mt, d in imgs_all]})
        return out
    def prune_images(self, msgs):
        seen = 0
        for m in reversed(msgs):
            if m["role"] == "user" and isinstance(m["content"], list):
                keep = []
                for x in m["content"]:
                    if x.get("type") == "image_url":
                        seen += 1
                        if seen > KEEP_IMAGES: keep.append({"type": "text", "text": "[older image dropped]"}); continue
                    keep.append(x)
                m["content"] = keep

def strip_images(msgs):
    for m in msgs:
        if isinstance(m.get("content"), list):
            nc = []
            for x in m["content"]:
                if x.get("type") in ("image", "image_url"): nc.append({"type": "text", "text": "[image dropped]"}); continue
                if x.get("type") == "tool_result" and isinstance(x.get("content"), list):
                    x["content"] = [y if y.get("type") != "image" else {"type": "text", "text": "[image dropped]"} for y in x["content"]]
                nc.append(x)
            m["content"] = nc

_TITLE_WORD_RE = re.compile(r"[A-Za-z']+")
BANNED_LABEL_WORDS = {"REVIEW", "REVIEWS", "VLOG", "VLOGS", "UNBOXING", "UNBOX", "TUTORIAL", "HAUL", "DIARY", "TOUR"}

# MT8 rule 4: minimum relative luminance for the one accent colour, so a dark accent (e.g. the
# (0.2,0.35,0.65) navy the unseen2 re-run picked) does not get waved through just because it matched
# across title/keyword_color/active_color -- matching each other is necessary but not sufficient.
ACCENT_MIN_LUMINANCE = 0.25

def brief_banned_words(brief_text):
    """Genre/category label from any quoted example title in the brief (e.g. 'BOTTLE REVIEW') -- the brief's
    example is always "<description> <GENRE>" (BOTTLE REVIEW, DESK VLOG), so only the trailing word is a
    category label; the rest is a product noun or description actually spoken in the clip. MT5 fix: this used
    to ban every word in the quoted example, which rejected an honest, on-brand title like "THE BEST BOTTLE"
    just because the brief's own instructional example happened to contain "BOTTLE" -- the banned list is for
    labels copied from the category/brief (REVIEW, VLOG, ...), not for nouns the clip is actually about."""
    words = set()
    for m in re.finditer(r'e\.g\.,?\s*"([^"]+)"', brief_text, re.IGNORECASE):
        example_words = _TITLE_WORD_RE.findall(m.group(1))
        if example_words:
            words.add(example_words[-1].upper())
    return words

def parse_color(v):
    """Best-effort normalize an add_title/auto_captions colour arg (hex string or [r,g,b(,a)] 0..1) to an
    (r,g,b) tuple 0..1, or None if it isn't one of those shapes."""
    if v is None: return None
    if isinstance(v, str):
        h = v.lstrip("#")
        if len(h) in (3, 4): h = "".join(c * 2 for c in h[:3])
        if len(h) >= 6:
            try: return tuple(int(h[i:i+2], 16) / 255.0 for i in (0, 2, 4))
            except ValueError: return None
        return None
    if isinstance(v, (list, tuple)) and len(v) >= 3:
        try: return tuple(float(x) for x in v[:3])
        except (TypeError, ValueError): return None
    return None

def colors_differ(a, b, tol=0.25):
    """True when two normalized colours are clearly not 'the same accent' (per-channel distance > tol)."""
    if a is None or b is None: return False  # can't compare -- not flagged as a mismatch, flagged as missing instead
    return any(abs(x - y) > tol for x, y in zip(a, b))

def taste_check(state, banned_words):
    """The Stage C harness-enforced check: did this export actually follow the 5 hard rules? Returns a list
    of plain-English violations (empty = compliant). Reads only the tool-call args the agent itself already
    captured as calls happened -- no project.json re-parse needed."""
    v = []
    title = state.get("title_text")
    if not title:
        v.append("no add_title call was made -- the short has no hook title at all")
    else:
        bad = {w for w in _TITLE_WORD_RE.findall(title.upper())} & banned_words
        if bad:
            v.append(f'the title "{title}" uses banned category word(s) {sorted(bad)} -- that\'s a label copied from the brief/category, not a claim or question taken from what was actually said')
        if state.get("title_style") not in ("impact", "kinetic"):
            v.append(f'add_title style is "{state.get("title_style")}", not "impact" or "kinetic"')
    if state.get("caption_tool") != "auto_captions":
        v.append("captions were set with caption_from_words, which has no keyword_color at all -- use auto_captions instead (pass words:[{text,start,end}] yourself if you do not trust recognition) so keyword styling actually exists")
    else:
        if not state.get("caption_keywords"):
            v.append("auto_captions was called without keywords:true -- no stress words are highlighted")
        if not state.get("caption_keyword_color"):
            v.append("auto_captions was called without a keyword_color -- the highlighted words have no accent colour")
    # MT3 fix (false positive): add_title's `accent` is the intentional one-accent arg (rule 4);
    # `color` is the base text fill, which rule 4 now asks to leave white/default. Prefer accent
    # when both are present so a model that correctly left color=white/default is not flagged.
    title_c = parse_color(state.get("title_accent")) or parse_color(state.get("title_color"))
    title_c_src = "accent" if parse_color(state.get("title_accent")) else "color"
    cap_c = parse_color(state.get("caption_keyword_color"))
    if title_c and cap_c and colors_differ(title_c, cap_c):
        v.append(f"the title's {title_c_src} {state.get('title_' + title_c_src)} and the caption keyword_color {state.get('caption_keyword_color')} are not the same accent -- pick one colour from the footage and reuse it on both")
    # MT3 rule (a): active_color (the currently-spoken-word highlight) must equal the one accent too, or be
    # white -- left unset it silently falls back to auto_captions' own yellow, mixing a second colour in.
    if state.get("caption_tool") == "auto_captions" and cap_c:
        active_v = state.get("caption_active_color")
        active_c = parse_color(active_v)
        white = (1.0, 1.0, 1.0)
        if active_v is None:
            v.append("auto_captions was called without active_color, so the spoken-word highlight stays at the tool's default yellow -- set active_color to the same accent as keyword_color (or to white)")
        elif active_c and title_c and colors_differ(active_c, title_c) and colors_differ(active_c, white):
            v.append(f"auto_captions active_color {active_v} is neither the accent {state.get('title_' + title_c_src)} nor white -- captions show a third colour mixed in")
    # MT8 rule 4: an accent too dark reads poorly as a keyword highlight against the black stroke every
    # caption already has (stroke_width 9 vs the title's own none) -- e.g. (0.2,0.35,0.65) measured
    # relative luminance ~0.07 on the unseen2 re-run and was flagged by nobody. Require >=0.25 so the
    # accent itself stays legible, independent of where the title sits.
    if title_c:
        lum = _rel_luminance(title_c)
        if lum < ACCENT_MIN_LUMINANCE:
            v.append(f"the chosen accent {state.get('title_' + title_c_src)} has relative luminance {lum:.2f} (rule 4 wants >= {ACCENT_MIN_LUMINANCE:g}) -- too dark to read clearly as a keyword highlight with a black stroke; pick a brighter colour from the footage")
    return v


def _rel_luminance(rgb01):
    """WCAG relative luminance from (r,g,b) each 0..1 (treated as sRGB, the common convention for UI/graphics colours)."""
    def lin(c):
        c = max(0.0, min(1.0, c))
        return c / 12.92 if c <= 0.03928 else ((c + 0.055) / 1.055) ** 2.4
    r, g, b = (lin(c) for c in rgb01)
    return 0.2126 * r + 0.7152 * g + 0.0722 * b

def _wcag_contrast(rgb_a, rgb_b):
    la, lb = _rel_luminance(rgb_a), _rel_luminance(rgb_b)
    lighter, darker = max(la, lb), min(la, lb)
    return (lighter + 0.05) / (darker + 0.05)

def legibility_check(k, state, out_dir):
    """MT3 rule (c): after export, sample the title's real on-screen frame region and check (1) contrast of
    the title's text colour against the mean background under it, and (2) overlap with the frame's strongest
    subject via lint_edit's own text_over_subject finding (ext/saliency_map.rs) -- no new Rust, just two
    existing MCP calls (compute_text_bounds, lint_edit) that are not on the director tool list the model
    sees but are still reachable directly from the harness, plus one render_frame with the title hidden so
    the sampled region is the true background, not a mix of background and the title's own pixels.
    Returns (violations: list[str], detail: dict) -- detail is for the report, not the model."""
    detail = {"ran": False}
    cid = state.get("title_clip_id")
    if not cid:
        return [], detail  # no title clip -- taste_check already flags "no add_title call"
    t_mid = (state.get("title_start") or 0.0) + (state.get("title_duration") or 4.0) / 2.0
    detail["ran"] = True; detail["clip_id"] = cid; detail["t"] = round(t_mid, 2)
    v = []
    # (1) contrast: real on-screen bbox from compute_text_bounds, real background pixels from a render with
    # the title hidden (so no text pixels are averaged into "background").
    try:
        rect_err, rect_out, _ = k.call("compute_text_bounds", {"clip_id": cid, "t": t_mid}, timeout=60)
        bg_path = os.path.join(out_dir, ".mt3_legibility_bg.png")
        bg_err, bg_out, _ = k.call("render_frame", {"time": t_mid, "path": bg_path, "hide": [cid], "inline": "never"}, timeout=60)
        if not rect_err and not bg_err:
            r = json.loads(rect_out)
            x, y, w, h = r["canvas_rect"]
            from PIL import Image
            im = Image.open(bg_path).convert("RGB")
            iw, ih = im.size
            x0, y0 = max(0, int(x)), max(0, int(y)); x1, y1 = min(iw, int(x + w)), min(ih, int(y + h))
            detail["bbox"] = [x0, y0, x1, y1]
            if x1 > x0 and y1 > y0:
                region = im.crop((x0, y0, x1, y1))
                arr = np.asarray(region, dtype=np.float64) / 255.0  # H,W,3 sRGB 0..1
                flat = arr.reshape(-1, 3)
                mean_rgb01 = tuple(flat.mean(axis=0))
                detail["bg_rgb_mean"] = [round(c, 3) for c in mean_rgb01]
                title_text_c = parse_color(state.get("title_color")) or (1.0, 1.0, 1.0)  # add_title's own text fill; white if unset (the tool's default)
                # MT4 fix: the bbox mean passed real_review's title at 9.98:1 even though the title visibly
                # crossed the lavender bottle, because most of the bbox sampled the dark wall beside the
                # bottle -- one average hid the worst patch. Take the 10th percentile of per-pixel contrast
                # across the whole bbox instead, so a bbox that is only PART light/dark still fails.
                lin = np.where(flat <= 0.03928, flat / 12.92, ((flat + 0.055) / 1.055) ** 2.4)
                lum = 0.2126 * lin[:, 0] + 0.7152 * lin[:, 1] + 0.0722 * lin[:, 2]
                la = _rel_luminance(title_text_c)
                lighter = np.maximum(la, lum); darker = np.minimum(la, lum)
                pixel_ratios = (lighter + 0.05) / (darker + 0.05)
                ratio_p10 = float(np.percentile(pixel_ratios, 10))
                ratio_mean = _wcag_contrast(title_text_c, mean_rgb01)
                detail["title_text_color"] = [round(c, 3) for c in title_text_c]
                detail["contrast_ratio_p10"] = round(ratio_p10, 2); detail["contrast_ratio_mean"] = round(ratio_mean, 2)
                ratio = ratio_p10
                if ratio < 3.0:
                    v.append(f"title text colour {state.get('title_color') or '[white, default]'} against the "
                             f"background under it is only {ratio:.2f}:1 contrast at the 10th percentile of the "
                             f"bbox (mean was {ratio_mean:.2f}:1 -- part of the background behind this title is "
                             f"much lighter/darker than the average; WCAG wants >=3:1 for large text everywhere "
                             f"under it, not just on average) -- it will be hard to read at a glance")
            else:
                detail["bbox_error"] = "bbox outside frame bounds"
        else:
            detail["tool_error"] = (rect_out if rect_err else bg_out)[:200]
    except Exception as e:
        detail["exception"] = f"{e}"[:200]
    # (2) subject overlap: lint_edit already computes this (ext/saliency_map.rs) -- just read its finding.
    try:
        le_err, le_out, _ = k.call("lint_edit", {}, timeout=120)
        if not le_err:
            probs = json.loads(le_out).get("problems", [])
            hit = next((p for p in probs if p.get("code") == "text_over_subject" and p.get("clip_id") == cid), None)
            detail["text_over_subject"] = hit
            if hit:
                fix = hit.get("fix") or {}
                suggested_y = (fix.get("args") or {}).get("value") if fix.get("tool") == "set_property" else (fix.get("then") or {}).get("args", {}).get("value")
                detail["suggested_y"] = suggested_y
                v.append(f"the title overlaps the frame's strongest subject (lint_edit text_over_subject: {hit.get('message')}) -- move it clear" + (f", e.g. set_property y={suggested_y}" if suggested_y is not None else ""))
    except Exception as e:
        detail.setdefault("exception", f"{e}"[:200])
    return v, detail


# MT4 rule: the title's vertical position has to be deterministic, not left to chance on a given run.
# real_review's "MY TWO BOTTLES" sat at add_title's own default position (vertical centre, y unset == 0
# pixels from centre per toolgroups.rs: "x/y in pixels from centre") and crossed the lavender bottle even
# though legibility_check's contrast measured fine (9.98:1 at the mean) and text_over_subject missed the
# overlap -- centre is where the subject/product usually is, colour was never the actual problem here. The
# vlog's title, moved to y=-400 (the top safe band, a clear/dark strip above the subject), read clean. This
# checks only the y coordinate; it does not judge whether the top band is actually clearer in a given shot
# (that needs a saliency read of the top band itself, which text_over_subject's reuse doesn't give us) --
# the smallest deterministic fix is: if the title is still near centre, push it to the same top band that
# already worked, and let the model's own judgement (and the next legibility/overlap check) override that
# only when render_frame actually shows the top band is busier.
TOP_BAND_Y = -400.0
NEAR_CENTER_Y = 250.0

def placement_check(state):
    """Returns a list of violations (empty = compliant). Fires independently of legibility_check's
    contrast/overlap result -- taste3 passed both of those for a title that still visibly crossed the
    bottle, because centring, not colour or overlap-detection, was the actual defect."""
    v = []
    if not state.get("title_clip_id"):
        return v
    y = state.get("title_y")
    if y is None:
        y = 0.0  # add_title's own default, never moved by set_property
    if abs(y) < NEAR_CENTER_Y:
        v.append(f"the title sits near vertical centre (y={y:g} pixels from centre, default/unmoved) -- "
                 f"that is usually where the subject/product is; move it to the top safe band with "
                 f"set_property (clip_id, property: \"y\", value: {TOP_BAND_Y:g}) unless render_frame shows "
                 f"the top band is busier than the centre in this shot")
    return v



# MT8 (2): safe-area check. compute_text_bounds gives the title's real on-screen bbox; query_safe_margins
# (ext/safe_margins.rs, a real read-only kimo tool -- "nothing is moved or rejected" per its own schema,
# not on the director tool list the model sees but reachable the same way compute_text_bounds/lint_edit
# already are) compares that bbox to the documented title-safe zone (90% inset) and the canvas itself, and
# hands back a suggested_shift. MT7's unseen2 case: the model moved the title to x=80 on its own after a
# legibility note, pushing bbox x1 to 760 against a 720px canvas -- compute_text_bounds/query_safe_margins
# together say so in numbers (worst_edge "right", worst_edge_px 76, outside_canvas_pct ~6%) instead of this
# only being caught by eye in a strip.
def safe_area_check(k, state, out_dir):
    """Returns (violations: list[str], detail: dict)."""
    detail = {"ran": False}
    cid = state.get("title_clip_id")
    if not cid:
        return [], detail
    t_mid = (state.get("title_start") or 0.0) + (state.get("title_duration") or 4.0) / 2.0
    v = []
    try:
        rect_err, rect_out, _ = k.call("compute_text_bounds", {"clip_id": cid, "t": t_mid}, timeout=60)
        if rect_err:
            detail["tool_error"] = rect_out[:200]
            return [], detail
        rect = json.loads(rect_out)["canvas_rect"]
        detail["ran"] = True; detail["canvas_rect"] = rect
        sm_err, sm_out, _ = k.call("query_safe_margins", {"profile": "title_safe", "rects": [{"name": "title", "rect": rect}]}, timeout=60)
        if sm_err:
            detail["tool_error"] = sm_out[:200]
            return [], detail
        sm = json.loads(sm_out)
        detail["query_safe_margins"] = sm
        outside_pct = sm.get("rects_outside_safe_max_pct") or 0.0
        overlap = (sm.get("overlaps") or [{}])[0]
        outside_canvas_pct = overlap.get("outside_canvas_pct") or 0.0
        if outside_pct > 0 or outside_canvas_pct > 0:
            # suggested_shift lives per-zone (overlap["zones"][*]["suggested_shift"]), not on the
            # overlap itself -- confirmed against the live tool's actual output, not assumed from its name.
            zones = overlap.get("zones") or []
            suggested = (zones[0].get("suggested_shift") if zones else None) or {}
            cur_x = state.get("title_x") or 0.0
            suggested_x = round(cur_x + (suggested.get("dx") or 0.0))
            detail["suggested_x"] = suggested_x
            off_canvas = f", {outside_canvas_pct:.1f}% of it off the canvas entirely" if outside_canvas_pct > 0 else ""
            v.append(f"the title's on-screen box {[round(c) for c in rect]} is {outside_pct:.1f}% outside the title-safe area{off_canvas} "
                     f"(worst edge: {sm.get('worst_edge')}, {sm.get('worst_edge_px', 0):.0f}px over) -- bring it back in frame, e.g. "
                     f"set_property (clip_id: \"{cid}\", property: \"x\", value: {suggested_x})")
    except Exception as e:
        detail["exception"] = f"{e}"[:200]
    return v, detail


# MT8 (3): accent consistency across every caption clip at final export, not just the args of whatever
# auto_captions call the harness happened to see last. keyword_color/active_color are not stored as a
# clip arg kimo echoes back -- they land as the "fill" of a text_fx animator (id starting "kw" for the
# keyword pop, id == "active" for the spoken-word highlight) baked into the saved project -- so this reads
# every clip on the caption track(s) straight from the live project the same way caption_y_values does.
# (MT8 unseen_fix.jpg: the last frame's "NEXT" was still red because an earlier auto_captions call's
# clip was never touched when a later call set a different colour elsewhere -- taste_state's own
# "last call wins" tracking could not see that.)
def caption_clip_colors(out_dir):
    proj = _project_state(out_dir)
    if not proj:
        return {}
    cap_tracks = {g.get("track_id") for g in (proj.get("editorial", {}).get("captions") or {}).values() if g.get("track_id")}
    if not cap_tracks:
        return {}
    out = {}
    for tr in proj.get("tracks", []):
        if tr.get("id") not in cap_tracks:
            continue
        for c in tr.get("clips", []):
            anims = ((c.get("text_fx") or {}).get("animators")) or []
            kw_fill = next((a.get("fill") for a in anims if isinstance(a.get("id"), str) and a["id"].startswith("kw") and a.get("fill")), None)
            active_fill = next((a.get("fill") for a in anims if a.get("id") == "active" and a.get("fill")), None)
            if kw_fill or active_fill:
                out[c.get("id")] = {"text": ((c.get("source") or {}).get("text") or "")[:24], "keyword_color": kw_fill, "active_color": active_fill}
    return out


def accent_consistency_check(state, out_dir):
    """Returns (violations: list[str], detail: dict). No live tool can recolour an existing caption
    clip's keyword/active fill after creation (set_property's PROPS are opacity/x/y/scale/rotation/
    rot_x/rot_y/pos_z/backface/fill_alpha/stroke_alpha only, set_text_style has no fill-colour arg,
    add_text_animator only appends a new animator, set_animator_keyframe's prop list has no "fill" --
    confirmed against this binary's live schemas, not assumed) -- so unlike the y/x position checks this
    one can only be flagged for the model to fix (by redoing the mismatched clip), never silently
    corrected by the harness."""
    detail = {"ran": False}
    title_c = parse_color(state.get("title_accent")) or parse_color(state.get("title_color"))
    if not title_c:
        return [], detail
    detail["ran"] = True
    clips = caption_clip_colors(out_dir)
    detail["clips"] = clips
    white = (1.0, 1.0, 1.0)
    mismatched = []
    for cid, c in clips.items():
        kw = parse_color(c.get("keyword_color"))
        if kw and colors_differ(kw, title_c):
            mismatched.append((cid, "keyword_color", c.get("keyword_color"), c.get("text")))
        act = parse_color(c.get("active_color"))
        if act and colors_differ(act, title_c) and colors_differ(act, white):
            mismatched.append((cid, "active_color", c.get("active_color"), c.get("text")))
    detail["mismatched"] = mismatched
    v = []
    if mismatched:
        sample = "; ".join(f'{cid} ("{txt}") {prop}={val}' for cid, prop, val, txt in mismatched[:5])
        more = f" (+{len(mismatched) - 5} more)" if len(mismatched) > 5 else ""
        v.append(f"{len(mismatched)} caption clip property/ies still do not match the one accent {state.get('title_accent') or state.get('title_color')} "
                 f"at final export -- {sample}{more} -- no tool can recolour an existing caption clip, so delete_clip + redo auto_captions for "
                 f"just that span with the same accent")
    return v, detail


def price(model, usage):
    p = PRICES.get(model)
    if not p or p[0] is None: return None
    return (usage["in"] * p[0] + usage["out"] * p[1] + usage["cache_read"] * p[2] + usage["cache_write"] * p[3]) / 1e6

# MT7 constants: the harness's own candidate title-y bands, and how close (pixels, in the same
# "pixels from centre" convention add_title/set_property use) a candidate has to sit to a live caption
# clip's y before it's skipped as "the caption band". 140px covers a caption block's own height plus
# some margin (captions run roughly 150-250px tall with box_width/line wrapping in review); it is not a
# measured saliency check, just keeping a candidate from sitting ON the caption text.
LEGIBILITY_Y_CANDIDATES = (-400.0, -300.0, -500.0, 250.0, 350.0)
CAPTION_BAND_MARGIN = 140.0


def _project_state(out_dir):
    """Best-effort read of the live project as a dict: the autosave kimo writes after every successful
    change if present (freshest -- mid-run, before any save_project/export), else the exported
    project.json. None if neither exists or parses."""
    for name in (".autosave.json", "project.json"):
        p = os.path.join(out_dir, name)
        if os.path.exists(p):
            try:
                return json.load(open(p))
            except Exception:
                continue
    return None


def caption_y_values(proj):
    """y (props.y.base, pixels from centre -- the same convention add_title/set_property use) of every
    clip on this project's caption track(s), read straight from the saved project via
    editorial.captions[*].track_id -> tracks[*].clips. [] when there are no captions yet, or the project
    can't be read -- find_passing_y treats that the same as 'no caption band known', not as a pass."""
    if not proj:
        return []
    cap_tracks = {g.get("track_id") for g in (proj.get("editorial", {}).get("captions") or {}).values() if g.get("track_id")}
    if not cap_tracks:
        return []
    ys = []
    for tr in proj.get("tracks", []):
        if tr.get("id") not in cap_tracks:
            continue
        for c in tr.get("clips", []):
            y = (((c.get("props") or {}).get("y") or {}).get("base"))
            if y is not None:
                try:
                    ys.append(float(y))
                except (TypeError, ValueError):
                    pass
    return ys


def _measure_contrast_at_y(k, cid, t_mid, out_dir, title_text_c, tag="", stroke_c=None):
    """The same real-pixel p10-contrast measurement legibility_check does for the live export (real
    on-screen bbox from compute_text_bounds, clean background from a render with the title hidden),
    factored out so find_passing_y can reuse it per-candidate without duplicating the maths or drifting
    from what the actual post-export check measures. Returns a dict; "ratio_p10" is absent (not zero) on
    any tool or measurement failure, so a failed measurement is never mistaken for a passing one."""
    detail = {}
    try:
        rect_err, rect_out, _ = k.call("compute_text_bounds", {"clip_id": cid, "t": t_mid}, timeout=60)
        bg_path = os.path.join(out_dir, f".mt7_candidate_bg{tag}.png")
        bg_err, bg_out, _ = k.call("render_frame", {"time": t_mid, "path": bg_path, "hide": [cid], "inline": "never"}, timeout=60)
        if rect_err or bg_err:
            detail["tool_error"] = (rect_out if rect_err else bg_out)[:200]
            return detail
        r = json.loads(rect_out)
        x, y, w, h = r["canvas_rect"]
        from PIL import Image
        im = Image.open(bg_path).convert("RGB")
        iw, ih = im.size
        x0, y0 = max(0, int(x)), max(0, int(y)); x1, y1 = min(iw, int(x + w)), min(ih, int(y + h))
        detail["bbox"] = [x0, y0, x1, y1]
        if x1 <= x0 or y1 <= y0:
            detail["bbox_error"] = "bbox outside frame bounds"
            return detail
        region = im.crop((x0, y0, x1, y1))
        arr = np.asarray(region, dtype=np.float64) / 255.0
        flat = arr.reshape(-1, 3)
        lin = np.where(flat <= 0.03928, flat / 12.92, ((flat + 0.055) / 1.055) ** 2.4)
        lum = 0.2126 * lin[:, 0] + 0.7152 * lin[:, 1] + 0.0722 * lin[:, 2]
        la = _rel_luminance(title_text_c)
        lighter = np.maximum(la, lum); darker = np.minimum(la, lum)
        ratios = (lighter + 0.05) / (darker + 0.05)
        mean_ratio = _wcag_contrast(title_text_c, tuple(flat.mean(axis=0)))
        if stroke_c is not None:
            # OUT1: with a stroke, a viewer reads the letter off whichever edge stands out more at that
            # local patch of background -- the fill colour or its outline -- so the controlling ratio per
            # sampled point is the BETTER of the two, not the fill alone (the add_title outline_width/
            # outline_color args; same Source::Text stroke path auto_captions already draws with).
            ls = _rel_luminance(stroke_c)
            lighter_s = np.maximum(ls, lum); darker_s = np.minimum(ls, lum)
            ratios = np.maximum(ratios, (lighter_s + 0.05) / (darker_s + 0.05))
            mean_ratio = max(mean_ratio, _wcag_contrast(stroke_c, tuple(flat.mean(axis=0))))
        detail["ratio_p10"] = round(float(np.percentile(ratios, 10)), 2)
        detail["ratio_mean"] = round(mean_ratio, 2)
    except Exception as e:
        detail["exception"] = f"{e}"[:200]
    return detail


def followup_needs_harness_fix(leg_violations, plc_violations):
    """MT7 (2): the single, testable signal that the model's corrective round did not actually land --
    whether that was a y no-op (set_property called with the same value), a garbled turn that produced no
    tool calls and never re-exported (MT6's unseen run), or a real re-export that still doesn't pass.
    True means the post-correction legibility/placement re-check still finds a problem, so the harness
    safety net should apply find_passing_y's own already-measured fix itself. False means whatever the
    model did (changed y, changed something else, or genuinely left it because it was already fine) is
    good enough -- a model can pass without touching y (e.g. it picked a different start time instead),
    so "y changed" is reported for the record but is not itself the pass/fail signal."""
    return bool(leg_violations or plc_violations)


def legibility_notable_violations(position_violations, passing_y):
    """MT8 (1): which position (legibility/placement) violations actually belong in the corrective note.
    All of them when there either were none, or a passing y was actually found for the model to use --
    [] when there WERE position violations but find_passing_y proved none of the tried positions fix it,
    so there is nothing real to tell the model to do and no reason to leave the door open for it to
    improvise a retitle/restyle instead (MT7's unseen2 case: an open-ended "try something else" note led
    the model to delete the title, write a new one, shift it off-screen, and switch the whole accent)."""
    if position_violations and passing_y is None:
        return []
    return position_violations


def find_passing_y(k, state, out_dir, candidates=LEGIBILITY_Y_CANDIDATES, pass_threshold=3.0):
    """MT7 (1): when legibility fails, find a y that actually PASSES the same p10-contrast method
    legibility_check uses, before telling the model anything -- rather than asking it to guess and hoping
    the guess works. (MT6: the model's one corrective round re-set the exact same y=-400 that had just
    failed -- a no-op -- then its next turn produced no tool calls at all, so nothing was ever
    re-measured.) Tries each candidate band in order, skipping any that sit on top of a live caption clip
    (read from the project, not guessed) within CAPTION_BAND_MARGIN px. Each non-skipped candidate is
    actually applied with set_property and actually measured -- this really moves the title clip. Leaves
    the clip at whichever y it ends on: the first passing candidate (stops there, that's the fix), or, if
    none pass, back at the ORIGINAL y it had when this was called (MT7 (3): no candidate passing means
    leave the edit as is, not stranded on a losing guess).
    Returns (passing_y: float|None, detail: dict). detail always carries candidates_tried (each
    {y, ratio_p10, ...} or {y, skipped_caption_band: True}), any_passed, best_y/best_ratio_p10 (the
    highest-contrast candidate tried even when none passed, for an honest best-effort note), and
    caption_y_values (what was read, so a report can show why a candidate was skipped)."""
    cid = state.get("title_clip_id")
    detail = {"candidates_tried": [], "any_passed": False, "best_y": None, "best_ratio_p10": None, "caption_y_values": []}
    if not cid:
        return None, detail
    orig_y = state.get("title_y")
    if orig_y is None:
        orig_y = 0.0
    t_mid = (state.get("title_start") or 0.0) + (state.get("title_duration") or 4.0) / 2.0
    title_text_c = parse_color(state.get("title_color")) or (1.0, 1.0, 1.0)
    cap_ys = caption_y_values(_project_state(out_dir))
    detail["caption_y_values"] = cap_ys
    passing_y = None
    best_y, best_ratio = None, None
    for y in candidates:
        if cap_ys and any(abs(y - cy) < CAPTION_BAND_MARGIN for cy in cap_ys):
            detail["candidates_tried"].append({"y": y, "skipped_caption_band": True})
            continue
        try:
            set_err, set_out, _ = k.call("set_property", {"clip_id": cid, "property": "y", "value": y}, timeout=30)
        except Exception as e:
            detail["candidates_tried"].append({"y": y, "set_property_error": f"{e}"[:200]})
            continue
        if set_err:
            detail["candidates_tried"].append({"y": y, "set_property_error": set_out[:200]})
            continue
        m = _measure_contrast_at_y(k, cid, t_mid, out_dir, title_text_c, tag=f"_{int(y)}")
        entry = {"y": y, **m}
        detail["candidates_tried"].append(entry)
        ratio = m.get("ratio_p10")
        if ratio is not None and (best_ratio is None or ratio > best_ratio):
            best_ratio = ratio; best_y = y
        if ratio is not None and ratio >= pass_threshold and passing_y is None:
            passing_y = y
            break  # first passing candidate wins -- leave the clip right here, don't keep testing
    detail["best_y"] = best_y
    detail["best_ratio_p10"] = best_ratio
    if passing_y is not None:
        detail["any_passed"] = True
        return passing_y, detail
    # MT7 (3): no candidate passed -- restore the title to the y it had before this search, rather than
    # stranding it on whichever losing candidate happened to run last.
    try:
        k.call("set_property", {"clip_id": cid, "property": "y", "value": orig_y}, timeout=30)
    except Exception:
        pass
    detail["restored_y"] = orig_y
    return None, detail


HARNESS_OUTLINE_WIDTH_AT_1280H = 6.0  # px of stroke at the preset's own k-scale; see titles.rs::add_title
HARNESS_OUTLINE_COLOR = (0.0, 0.0, 0.0)  # black, same default auto_captions draws its own stroke with


def apply_outline_fix(k, state, out_dir, outline_width=HARNESS_OUTLINE_WIDTH_AT_1280H, outline_color=HARNESS_OUTLINE_COLOR):
    """OUT1: find_passing_y already proved no y position reaches readable contrast against this
    background -- moving the title can't fix it, but an outline can. Deletes the title clip (add_title
    is a one-shot preset; there is no existing tool that adds a stroke to a clip after creation -- see
    MT8's note on set_property/set_text_style, same story here) and recreates it at the exact
    text/style/position/accent it already had, this time with add_title's own outline_width/outline_color
    (the same Source::Text stroke_width/stroke_color path auto_captions already draws every caption
    with), then re-exports and re-measures. Always returns a dict with "attempted"; when attempted,
    "ratio_p10_with_outline" is the stroke-aware measurement (_measure_contrast_at_y's own ratio, taking
    whichever of fill/stroke contrasts better against each sampled background patch -- the real
    perceptual claim an outline makes, not a guarantee the plain WCAG number itself crosses the
    threshold, since that number is defined on the fill colour alone and a stroke is a shape change, not
    a colour change)."""
    detail = {"attempted": False}
    cid = state.get("title_clip_id"); track_id = state.get("title_track_id")
    if not cid or not track_id or not state.get("title_text") or not state.get("title_style"):
        detail["skipped_reason"] = "missing title_clip_id/track_id/text/style in harness state; cannot safely recreate the title"
        return detail
    args = {"track_id": track_id, "style": state["title_style"], "text": state["title_text"],
            "start": state.get("title_start") or 0.0, "duration": state.get("title_duration") or 4.0,
            "outline_width": outline_width, "outline_color": list(outline_color)}
    if state.get("title_color"): args["color"] = state["title_color"]
    if state.get("title_accent"): args["accent"] = state["title_accent"]
    if state.get("title_subtitle"): args["subtitle"] = state["title_subtitle"]
    if state.get("title_font"): args["font"] = state["title_font"]
    try:
        de, do, _ = k.call("delete_clip", {"clip_id": cid}, timeout=30)
        if de:
            detail["delete_error"] = do[:300]; return detail
        detail["attempted"] = True
        ae, ao, _ = k.call("add_title", args, timeout=60)
        if ae:
            detail["add_title_error"] = ao[:300]; return detail
        new_cids = (json.loads(ao).get("clip_ids") or [])
        if not new_cids:
            detail["add_title_error"] = "no clip_ids returned"; return detail
        new_cid = new_cids[0]
        for prop, val in (("y", state.get("title_y")), ("x", state.get("title_x"))):
            if val is not None:
                k.call("set_property", {"clip_id": new_cid, "property": prop, "value": val}, timeout=30)
        state["title_clip_id"] = new_cid
        detail["new_clip_id"] = new_cid
        t_mid = (state.get("title_start") or 0.0) + (state.get("title_duration") or 4.0) / 2.0
        title_text_c = parse_color(state.get("title_color")) or (1.0, 1.0, 1.0)
        m = _measure_contrast_at_y(k, new_cid, t_mid, out_dir, title_text_c, tag="_outline", stroke_c=outline_color)
        detail["measurement"] = m
        detail["ratio_p10_with_outline"] = m.get("ratio_p10")
        se, so, _ = k.call("save_project", {"path": os.path.join(out_dir, "project.json")}, timeout=60)
        if se: detail["save_error"] = so[:300]
        ee, eo, _ = k.call("export", {"path": os.path.join(out_dir, "final.mp4"), "crf": 20}, timeout=180)
        detail["re_exported"] = not ee
        if ee: detail["export_error"] = eo[:300]
    except Exception as e:
        detail["exception"] = f"{e}"[:300]
    return detail


def run(a):
    os.makedirs(a.out, exist_ok=True); log = open(os.path.join(a.out, "log.jsonl"), "w")
    env = {"KIMO_SPECTATOR": "off"}
    if a.tools: env["KIMO_TOOLS"] = a.tools
    if a.fonts: env["KIMO_FONTS"] = a.fonts
    k = Kimo(a.kimo, env=env, cwd=a.out, log=os.path.join(a.out, "kimo.stderr"))
    tools = mcp_to_fn_tools(k.list_tools())
    if a.allow:
        allow = [x.strip() for x in a.allow.split(",") if x.strip()]; have = {t["name"] for t in tools}
        missing = [x for x in allow if x not in have]
        if missing: raise SystemExit(f"--allow names not served by this kimo-mcp: {missing}")
        tools = [t for t in tools if t["name"] in allow]
    system = open(a.system).read() if a.system else ""
    if a.doc:
        for d in a.doc.split(","): system += "\n\n# Reference\n\n" + open(d).read().replace("{footage_list}", ", ".join(os.path.join(a.footage, f) for f in sorted(os.listdir(a.footage)) if f.lower().endswith((".mp4", ".mov", ".m4a", ".wav", ".mp3")))).replace("{footage}", a.footage).replace("{out}", a.out)
    brief = open(a.brief).read().replace("{footage}", a.footage).replace("{out}", a.out)
    taste_state = {"title_text": None, "title_style": None, "title_color": None, "title_accent": None,
                   "title_clip_id": None, "title_start": None, "title_duration": None, "title_y": None, "title_x": None,
                   "title_track_id": None, "title_subtitle": None, "title_font": None,
                   "caption_tool": None, "caption_keywords": False, "caption_keyword_color": None, "caption_active_color": None}
    taste_banned_words = BANNED_LABEL_WORDS | brief_banned_words(brief)
    taste_corrections_used = 0
    taste_log = {"fired": False, "violations_found": [], "corrective_round": 0}
    taste_followup = {}  # MT8: "position" (y-axis legibility/placement) and/or "safe_area" (x-axis) pending fixups

    if a.adapter == "anthropic":
        m = Anthropic(a.model, os.environ["ANTHROPIC_BASE_URL"], os.environ["ANTHROPIC_API_KEY"], not a.no_vision)
    else:
        m = OpenAICompat(a.model, os.environ["OPENAI_BASE_URL"], os.environ["OPENAI_API_KEY"], not a.no_vision)
    msgs = [{"role": "user", "content": brief}]
    t0 = time.time(); tot = {"in": 0, "out": 0, "cache_read": 0, "cache_write": 0}; calls_n = errs = steps = 0; final_text = ""; stop = "end_turn"
    tool_time = 0.0; tool_chars = 0
    log.write(json.dumps({"ev": "start", "model": a.model, "tools_listed": len(tools), "tools_chars": len(json.dumps(tools)), "system_chars": len(system), "server": k.server}) + "\n")
    final = os.path.join(a.out, "final.mp4"); proj = os.path.join(a.out, "project.json")
    names = {t["name"] for t in tools}
    def is_export(name, args):
        act = args.get("action") if isinstance(args, dict) else None
        return name == "export" or (name == "output" and act == "export")
    def is_preview(name, args):
        act = args.get("action") if isinstance(args, dict) else None
        return name in ("render_frame", "contact_sheet") or (name == "output" and act in ("render_frame", "contact_sheet"))
    def export_ok():
        try: return os.path.getsize(final) > 20000 and os.path.getmtime(final) >= t0
        except OSError: return False
    def budget_line(phase):
        left_c = a.max_calls - calls_n; left_t = int(a.max_wall - (time.time() - t0))
        s = f"[harness] budget: {calls_n}/{a.max_calls} tool calls used, {int(time.time() - t0)}/{int(a.max_wall)} s used ({left_c} calls, {left_t} s left). final.mp4 exported: {'yes' if export_ok() else 'NO'}."
        if phase:
            s += (f" FINISH PHASE: stop polishing now. In order: (1) save_project {{path: \"{proj}\"}}, (2) export {{path: \"{final}\", crf: 20}}."
                  " Frame previews (render_frame, contact_sheet) are refused until final.mp4 exists. After the export you may review and re-export once if budget remains.")
        return s
    finish_at_calls = int(a.max_calls * a.finish_frac); finish_at_wall = a.max_wall * a.finish_frac
    in_finish = False; nudges = 0; finish_entered = None; harness_finished = False; refused = 0; restarts = 0
    precheck_passes = 0  # design-taste self-critique cap: at most 1 pre-export preview pass, so it can't loop
    autosave = os.path.join(a.out, ".autosave.json")
    while True:
        el = time.time() - t0
        hard_wall = a.max_wall if not in_finish or export_ok() else a.max_wall + a.finish_grace
        if el > hard_wall: stop = "wall"; break
        if calls_n >= a.max_calls + (0 if export_ok() else a.finish_extra_calls): stop = "max_calls"; break
        c = price(a.model, tot)
        if c is not None and c > a.max_cost: stop = "cost"; break
        m.prune_images(msgs)
        try: amsg, calls, text, usage = m.step(system, tools, msgs)
        except Exception as e:
            if "does not support image" in str(e) and m.vision:
                m.vision = False; strip_images(msgs); log.write(json.dumps({"ev": "harness", "what": "model refuses images; vision off, images stripped"}) + "\n"); continue
            stop = f"model_error: {e}"[:300]; break
        steps += 1
        for kk in tot: tot[kk] += usage[kk]
        msgs.append(amsg); final_text = text or final_text
        log.write(json.dumps({"ev": "step", "n": steps, "t": round(time.time() - t0, 1), "usage": usage, "text": text[:2000], "calls": [[n, args] for _, n, args in calls]}) + "\n"); log.flush()
        if not calls:
            if export_ok() or nudges >= 2: break
            nudges += 1; in_finish = True; finish_entered = finish_entered or round(time.time() - t0, 1)
            msgs.append({"role": "user", "content": f"You stopped without exporting. {final} does not exist yet, so there is no video. " + budget_line(True)})
            log.write(json.dumps({"ev": "harness", "what": "nudge: ended without export", "n": nudges}) + "\n"); continue
        results = []
        just_exported = False
        for cid, name, args in calls:
            calls_n += 1; ts = time.time()
            if in_finish and not export_ok() and is_preview(name, args):
                is_err, out, imgs = True, "refused by the harness: FINISH PHASE, export final.mp4 first (see budget line).", []; refused += 1
            else:
                try: is_err, out, imgs = k.call(name, args, timeout=a.call_timeout)
                except Exception as e:
                    is_err, out, imgs = True, f"client: {e}", []
                    if "server exited" in str(e) or "no answer" in str(e):
                        k.close(); k = Kimo(a.kimo, env=env, cwd=a.out, log=os.path.join(a.out, "kimo.stderr")); restarts += 1
                        if os.path.exists(autosave):
                            le, lo, _ = k.call("load_project", {"path": autosave}, timeout=120)
                            out += (" (the kimo server crashed and was restarted; the harness reloaded your project as of your last successful call. Clip ids are unchanged. Retry this call.)" if not le else f" (the kimo server was restarted; reloading your project failed: {lo[:200]})")
                        else: out += " (the kimo server was restarted; the project in memory is gone)"
                        log.write(json.dumps({"ev": "harness", "what": "kimo restarted", "autosave_loaded": os.path.exists(autosave)}) + "\n")
            if not is_err: out = flag_uncallable_tools(name, out, names)
            if not is_err and isinstance(args, dict):
                if name == "add_title":
                    taste_state["title_text"] = args.get("text"); taste_state["title_style"] = args.get("style")
                    taste_state["title_color"] = args.get("color"); taste_state["title_accent"] = args.get("accent")
                    taste_state["title_start"] = args.get("start", 0.0); taste_state["title_duration"] = args.get("duration", 4.0)
                    taste_state["title_x"] = args.get("x", 0.0)
                    taste_state["title_track_id"] = args.get("track_id"); taste_state["title_subtitle"] = args.get("subtitle")
                    taste_state["title_font"] = args.get("font")
                    try:
                        cids = json.loads(out).get("clip_ids") if not is_err else None
                        if cids: taste_state["title_clip_id"] = cids[0]
                    except Exception:
                        pass
                elif name == "auto_captions":
                    taste_state["caption_tool"] = "auto_captions"
                    taste_state["caption_keywords"] = bool(args.get("keywords"))
                    taste_state["caption_keyword_color"] = args.get("keyword_color")
                    taste_state["caption_active_color"] = args.get("active_color")
                elif name == "caption_from_words" and taste_state["caption_tool"] != "auto_captions":
                    taste_state["caption_tool"] = "caption_from_words"
                elif name == "set_property" and taste_state.get("title_clip_id") and args.get("clip_id") == taste_state["title_clip_id"] and args.get("property") == "y":
                    try: taste_state["title_y"] = float(args.get("value"))
                    except (TypeError, ValueError): pass
                elif name == "set_property" and taste_state.get("title_clip_id") and args.get("clip_id") == taste_state["title_clip_id"] and args.get("property") == "x":
                    try: taste_state["title_x"] = float(args.get("value"))
                    except (TypeError, ValueError): pass
                if is_export(name, args): just_exported = True
            if not is_err and not export_ok() and is_preview(name, args) and taste_state.get("title_text"):
                # Only counts once a title exists -- an early render_frame/contact_sheet used to sample an
                # accent colour off the raw footage (hard rule 4) is shot-selection, not the design-taste
                # self-critique pass design_taste.md section 8 describes; gating on title_text keeps that
                # early colour-pick free while still capping the real pre-export check at one.
                precheck_passes += 1
                if precheck_passes > 1:
                    out += ("\n[harness: that's your one pre-export design-taste check already used. Fix the single worst "
                            "issue you listed against the taste guide, then call export now -- no further contact_sheet/"
                            "render_frame calls before export.]")
            dt = time.time() - ts; tool_time += dt; errs += is_err; tool_chars += len(out)
            log.write(json.dumps({"ev": "call", "n": calls_n, "name": name, "args": args, "error": is_err, "dt": round(dt, 2), "chars": len(out), "images": len(imgs), "result": out[:1500]}) + "\n"); log.flush()
            gi = good_images(imgs)
            if len(gi) < len(imgs): out += f"\n[client: {len(imgs) - len(gi)} returned image block(s) were not valid images and were dropped]"
            results.append([cid, is_err, cap(out), gi])
        # autosave after any successful change, so a server crash (seen: OOM kill during export) loses nothing
        if any(not r[1] for r in results) and not all(is_preview(n, ar) for _, n, ar in calls):
            try:
                if "output" in names: k.call("project", {"action": "save_project", "args": {"path": autosave}}, timeout=60)
                else: k.call("save_project", {"path": autosave}, timeout=60)
            except Exception: pass
        if just_exported and export_ok() and taste_corrections_used < 1:
            taste_log["fired"] = True
            violations = taste_check(taste_state, taste_banned_words)
            leg_violations, leg_detail = legibility_check(k, taste_state, a.out)
            plc_violations = placement_check(taste_state)
            safe_violations, safe_detail = safe_area_check(k, taste_state, a.out)
            accent_violations, accent_detail = accent_consistency_check(taste_state, a.out)
            taste_log["legibility"] = leg_detail
            taste_log["safe_area"] = safe_detail
            taste_log["accent_consistency"] = accent_detail
            position_violations = leg_violations + plc_violations  # the y-axis issues find_passing_y can fix
            taste_log["violations_found"] = violations + position_violations + safe_violations + accent_violations
            find_detail = None
            passing_y = None
            if position_violations:
                # MT7 (1): find a y that actually passes BEFORE writing the note, instead of telling the
                # model a generic "-400" and hoping (MT6: that hope failed -- the model re-set the exact
                # same already-failing -400 and called it fixed).
                passing_y, find_detail = find_passing_y(k, taste_state, a.out)
                taste_log["find_passing_y"] = find_detail
            position_fixable = (passing_y is not None) if position_violations else True
            position_violations_for_note = legibility_notable_violations(position_violations, passing_y)
            if position_violations and not position_fixable:
                # MT8 (1): no candidate y reaches readable contrast on this background -- do NOT invite the
                # model to "fix" this by retitling or restyling (MT7's unseen2 case: the note left the door
                # open with "try a different start time... or accept this", and the model instead deleted
                # the title, wrote a new one, moved it off-screen (x=80) and switched the whole accent to an
                # unreadable navy -- none of which find_passing_y had found would help, and the corrective
                # round was spent reacting to that instead of anything real). The title/position the model
                # already has is left exactly as is (find_passing_y already restored title_y to what it was
                # before this search), and that is recorded honestly instead of pretending there is a fix.
                tried = len([c for c in find_detail["candidates_tried"] if "skipped_caption_band" not in c])
                taste_log["legibility_unfixable"] = {"reason": f"{tried} candidate title position(s) tried near the top/bottom safe bands; "
                                                                "none reached readable contrast against this background -- kept the original "
                                                                "title text and position rather than asking the model to retitle/restyle for it",
                                                      "violations_excluded_from_note": position_violations, "find_passing_y": find_detail}
                # OUT1: moving the title can't fix this background -- but add_title's own outline can
                # (same stroke path auto_captions already draws with). The harness applies it itself, right
                # here, rather than waiting on a corrective round the model was never even told about.
                outline_detail = apply_outline_fix(k, taste_state, a.out)
                taste_log["legibility_unfixable"]["outline_fix"] = outline_detail
            all_violations_for_note = violations + position_violations_for_note + safe_violations + accent_violations
            if all_violations_for_note:
                taste_corrections_used += 1; taste_log["corrective_round"] = taste_corrections_used
                note = ("\n\n[harness: design-taste check on your export found real problems against the hard rules -- "
                        "fix these, then export again. This is your only corrective round, so fix all of them now:\n- "
                        + "\n- ".join(all_violations_for_note) + "\nPick one accent colour from the footage and reuse it on the title and the captions.")
                if position_violations_for_note:
                    note += (f" For legibility/placement specifically: add_title has no stroke/shadow/position arg, so the fix is to "
                              "keep the title text white (color unset or [1,1,1]) with your one accent only in the `accent` "
                              f"arg, and set the title to this EXACT position -- set_property (clip_id: \"{taste_state.get('title_clip_id')}\", "
                              f"property: \"y\", value: {passing_y:g}). The harness already measured that position: "
                              f"{find_detail['best_ratio_p10']:.2f}:1 contrast there (your current spot measured below 3:1). "
                              "Use this exact value -- don't pick a different one.")
                    # MT7 (2): remember exactly what we asked for, so once the model's next turn lands (or
                    # doesn't) we can verify the fix actually happened rather than trusting its say-so.
                    taste_followup["position"] = {"snapshot_y": taste_state.get("title_y"), "passing_y": passing_y}
                if safe_violations:
                    # MT8 (2): same verify-then-apply pattern as position, on the x axis. A plain x=0 is
                    # the harness's own deterministic fallback if the model's turn doesn't land; the note
                    # itself offers the more precise measured suggested_x first.
                    taste_followup["safe_area"] = {"snapshot_x": taste_state.get("title_x")}
                note += "]"
                results[-1][2] += note
                log.write(json.dumps({"ev": "harness", "what": "taste check failed, corrective round fired", "violations": all_violations_for_note,
                                       "legibility": leg_detail, "placement": plc_violations, "safe_area": safe_detail, "accent_consistency": accent_detail,
                                       "find_passing_y": find_detail, "legibility_unfixable": taste_log.get("legibility_unfixable")}) + "\n")
            else:
                log.write(json.dumps({"ev": "harness", "what": "taste check passed, or only an unfixable legibility/placement issue remained (no corrective round)",
                                       "legibility": leg_detail, "placement": plc_violations, "safe_area": safe_detail, "accent_consistency": accent_detail,
                                       "legibility_unfixable": taste_log.get("legibility_unfixable")}) + "\n")
        if not in_finish and not export_ok() and (calls_n >= finish_at_calls or time.time() - t0 >= finish_at_wall):
            in_finish = True; finish_entered = round(time.time() - t0, 1)
            log.write(json.dumps({"ev": "harness", "what": "finish phase", "calls": calls_n, "t": finish_entered}) + "\n")
        results[-1][2] += "\n\n" + budget_line(in_finish and not export_ok())
        msgs += m.tool_results([tuple(r) for r in results])
    # MT7 (2): verify the one corrective round, if it fired on a legibility/placement violation, actually
    # landed -- a no-op (the model re-set the identical y) or a turn that ends without a second export both
    # leave the FIRST, flagged export sitting on disk looking fine to export_ok() (it only checks mtime/size,
    # not content), exactly what happened on MT6's unseen run: the model's corrective turn produced zero tool
    # calls (garbled completion), the main loop's own "if not calls: ... break" exited right there, and the
    # still-bad final.mp4 was never touched again. If the model's fix really worked, this changes nothing.
    followup_verify = {"checked": False}
    if taste_followup.get("position") or taste_followup.get("safe_area"):
        followup_verify["checked"] = True
        cur_leg_violations, cur_leg_detail = legibility_check(k, taste_state, a.out)
        cur_plc_violations = placement_check(taste_state)
        cur_safe_violations, cur_safe_detail = safe_area_check(k, taste_state, a.out)
        cur_accent_violations, cur_accent_detail = accent_consistency_check(taste_state, a.out)
        followup_verify["legibility_after"] = cur_leg_detail
        followup_verify["safe_area_after"] = cur_safe_detail
        followup_verify["accent_consistency_after"] = cur_accent_detail
        any_applied = False
        # --- position (y axis): unchanged MT7 behaviour, now gated on taste_followup["position"] specifically.
        if taste_followup.get("position"):
            y_before, y_after = taste_followup["position"]["snapshot_y"], taste_state.get("title_y")
            still_bad_pos = followup_needs_harness_fix(cur_leg_violations, cur_plc_violations)
            followup_verify.update({"y_before": y_before, "y_after": y_after, "y_changed": y_after != y_before,
                                     "still_violating": cur_leg_violations + cur_plc_violations, "position_passed": not still_bad_pos})
            if still_bad_pos:
                fix_y = taste_followup["position"].get("passing_y")
                if fix_y is not None:
                    log.write(json.dumps({"ev": "harness", "what": "corrective round did not land on position (no-op, garbled turn, or no re-export) -- harness applying the already-verified fix itself", "fix_y": fix_y}) + "\n"); log.flush()
                    try:
                        he2, ho2, _ = k.call("set_property", {"clip_id": taste_state["title_clip_id"], "property": "y", "value": fix_y}, timeout=30)
                        if not he2:
                            taste_state["title_y"] = fix_y; any_applied = True
                        followup_verify.update({"harness_fix_y": fix_y, "harness_set_property_error_y": ho2[:300] if he2 else None})
                    except Exception as e:
                        followup_verify["harness_fix_error_y"] = f"{e}"[:300]
                else:
                    # MT7 (3): find_passing_y already found no candidate that passes -- don't strand the edit on
                    # a losing guess of our own either; say so honestly and leave what the model left.
                    followup_verify["harness_fix_note_y"] = "no candidate y passed contrast when this was first checked; leaving the edit as the model left it"
        # --- safe area (x axis): MT8 (2). Re-check with the SAME tool (query_safe_margins); if still
        # outside, apply x=0 ourselves -- a plain, always-available centred fallback -- rather than the
        # measured suggested_x (which assumes the model didn't also change the title text/duration/clip
        # since the note was written, and a plain centre is the safer deterministic default here).
        if taste_followup.get("safe_area"):
            x_before, x_after = taste_followup["safe_area"]["snapshot_x"], taste_state.get("title_x")
            still_bad_safe = bool(cur_safe_violations)
            followup_verify.update({"x_before": x_before, "x_after": x_after, "x_changed": x_after != x_before, "safe_area_passed": not still_bad_safe})
            if still_bad_safe:
                log.write(json.dumps({"ev": "harness", "what": "corrective round did not land on safe-area (title still outside the safe area/canvas) -- harness setting x=0 itself"}) + "\n"); log.flush()
                try:
                    he4, ho4, _ = k.call("set_property", {"clip_id": taste_state["title_clip_id"], "property": "x", "value": 0}, timeout=30)
                    if not he4:
                        taste_state["title_x"] = 0.0; any_applied = True
                    followup_verify.update({"harness_fix_x": 0, "harness_set_property_error_x": ho4[:300] if he4 else None})
                except Exception as e:
                    followup_verify["harness_fix_error_x"] = f"{e}"[:300]
        # --- accent consistency: MT8 (3). No live tool can recolour an existing caption clip's keyword/
        # active fill (checked against this binary's real schemas -- see accent_consistency_check's own
        # docstring), so there is nothing for the harness to apply here; record the honest after-state only.
        followup_verify["accent_consistency_still_violating"] = cur_accent_violations
        if any_applied:
            try:
                if "output" in names:
                    k.call("project", {"action": "save_project", "args": {"path": proj}}, timeout=60)
                    he3, ho3, _ = k.call("output", {"action": "export", "args": {"path": final, "crf": 20}}, timeout=a.call_timeout)
                else:
                    k.call("save_project", {"path": proj}, timeout=60)
                    he3, ho3, _ = k.call("export", {"path": final, "crf": 20}, timeout=a.call_timeout)
                applied_ok = (not he3) and export_ok()
                followup_verify.update({"harness_applied_fix": applied_ok, "harness_export_error": ho3[:300] if he3 else None})
            except Exception as e:
                followup_verify.update({"harness_applied_fix": False, "harness_fix_error": f"{e}"[:300]})
        else:
            followup_verify["harness_applied_fix"] = False
    taste_log["followup_verify"] = followup_verify

    # the harness's own export: only when the model never produced final.mp4. Disclosed in run.json, never counted as the model's.
    if not export_ok():
        log.write(json.dumps({"ev": "harness", "what": "harness export start", "stop": stop}) + "\n"); log.flush()
        try:
            if "output" in names:
                k.call("project", {"action": "save_project", "args": {"path": proj}}, timeout=60)
                he, ho, _ = k.call("output", {"action": "export", "args": {"path": final, "crf": 20}}, timeout=a.call_timeout)
            else:
                k.call("save_project", {"path": proj}, timeout=60)
                he, ho, _ = k.call("export", {"path": final, "crf": 20}, timeout=a.call_timeout)
        except Exception as e: he, ho = True, f"client: {e}"
        harness_finished = export_ok()
        log.write(json.dumps({"ev": "harness", "what": "harness export end", "error": he, "ok": harness_finished, "result": str(ho)[:500]}) + "\n"); log.flush()
    wall = time.time() - t0; cost = price(a.model, tot)
    k.close()
    run = {"model": a.model, "adapter": a.adapter, "tools_profile": a.tools or "grouped", "stop": stop, "wall_s": round(wall, 1), "tool_s": round(tool_time, 1), "steps": steps, "calls": calls_n, "errors": errs,
           "tokens": tot, "cost_usd": None if cost is None else round(cost, 4), "result_chars": tool_chars, "final_text": final_text[:3000],
           "harness_finished": harness_finished, "kimo_restarts": restarts, "finish_phase_at_s": finish_entered, "nudges": nudges, "previews_refused": refused, "provider_retries": RETRY_LOG,
           "model_exported": export_ok() and not harness_finished, "taste_check": taste_log, "harness_applied_fix": bool(taste_log.get("followup_verify", {}).get("harness_applied_fix")) or bool(taste_log.get("legibility_unfixable", {}).get("outline_fix", {}).get("attempted"))}
    json.dump(run, open(os.path.join(a.out, "run.json"), "w"), indent=1); log.write(json.dumps({"ev": "end", **run}) + "\n"); log.close()
    print(json.dumps({k2: v for k2, v in run.items() if k2 != "final_text"}))
    return run

if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--kimo", required=True); ap.add_argument("--brief", required=True); ap.add_argument("--footage", required=True); ap.add_argument("--out", required=True)
    ap.add_argument("--model", required=True); ap.add_argument("--adapter", choices=["anthropic", "openai"], default="anthropic")
    ap.add_argument("--tools", default="", help="KIMO_TOOLS profile for the server (grouped when empty)")
    ap.add_argument("--system", default="", help="system prompt file"); ap.add_argument("--doc", default="", help="comma list of docs appended to the system prompt")
    ap.add_argument("--fonts", default=""); ap.add_argument("--no-vision", action="store_true")
    ap.add_argument("--max-calls", type=int, default=150); ap.add_argument("--max-wall", type=float, default=1500); ap.add_argument("--max-cost", type=float, default=4.0)
    ap.add_argument("--call-timeout", type=float, default=600)
    ap.add_argument("--finish-frac", type=float, default=0.72, help="share of the call or wall budget after which the finish phase starts")
    ap.add_argument("--finish-grace", type=float, default=420, help="extra seconds past max-wall allowed for the export once in the finish phase")
    ap.add_argument("--finish-extra-calls", type=int, default=12)
    ap.add_argument("--allow", default="", help="comma list: only these tools are shown to the model (curated profile)")
    run(ap.parse_args())
