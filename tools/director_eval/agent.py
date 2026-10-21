#!/usr/bin/env python3
"""The director loop the eval drives: a model + kimo-mcp over stdio, one tool call at a time.

Two adapters, so the same loop runs a frontier model and a small one:
  anthropic  Messages API (ANTHROPIC_BASE_URL / ANTHROPIC_API_KEY, prompt caching on)
  openai     any OpenAI-compatible chat/completions endpoint (OPENAI_BASE_URL / OPENAI_API_KEY), e.g. Llama / Muse hosts
Everything is logged to <out>/log.jsonl (one line per model step and per tool call) and <out>/run.json (totals).
"""
import argparse, base64, json, os, sys, time
import requests
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp import Kimo

# $ per million tokens: input, output, cache read, cache write. Extend for your model.
PRICES = {
    "claude-opus-5-5": (4.0, 20.0, 0.20, 5.0), "claude-haiku-4-5": (1.0, 5.0, 0.10, 1.25), "claude-sonnet-5": (2.0, 10.0, 0.20, 2.5),
    # Muse Spark Open: the OpenRouter list price (2026-10-21); the Fireworks bill itself was not read
    "accounts/fireworks/models/muse-spark-open": (1.25, 4.25, 0.15, 1.25),
    "accounts/fireworks/models/nemotron-lightning-3p5-30b-a3b": (None, None, None, None),
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

def price(model, usage):
    p = PRICES.get(model)
    if not p or p[0] is None: return None
    return (usage["in"] * p[0] + usage["out"] * p[1] + usage["cache_read"] * p[2] + usage["cache_write"] * p[3]) / 1e6

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
        if not in_finish and not export_ok() and (calls_n >= finish_at_calls or time.time() - t0 >= finish_at_wall):
            in_finish = True; finish_entered = round(time.time() - t0, 1)
            log.write(json.dumps({"ev": "harness", "what": "finish phase", "calls": calls_n, "t": finish_entered}) + "\n")
        results[-1][2] += "\n\n" + budget_line(in_finish and not export_ok())
        msgs += m.tool_results([tuple(r) for r in results])
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
           "model_exported": export_ok() and not harness_finished}
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
