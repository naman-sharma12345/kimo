#!/usr/bin/env python3
"""Score one director run from what it produced: <out>/final.mp4 and <out>/project.json, against the footage's truth.json.

  python3 score.py --out RUN_DIR --footage FOOTAGE_SET_DIR --kimo /path/kimo-mcp [--judge]

Every check is 0..1 and measured, never self-reported: picture from decoded export frames, sound from ffmpeg ebur128 on the export,
timing from the saved project mapped onto the footage truth, text boxes from kimo's own compute_text_bounds.
The composite is the mean of the checks that apply to the set, x100. --judge adds a vision judge's 1-10 on a contact sheet (reported apart).
"""
import argparse, base64, io, json, math, os, re, subprocess, sys
import numpy as np
from PIL import Image
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from mcp import Kimo

FPS = 30.0

def run(cmd, t=300):
    return subprocess.run(["timeout", "-k", "5", str(t)] + cmd, capture_output=True, text=True)

def probe(path):
    r = run(["ffprobe", "-v", "error", "-show_entries", "stream=codec_type,width,height:format=duration", "-of", "json", path])
    if r.returncode: return None
    j = json.loads(r.stdout); v = [s for s in j.get("streams", []) if s.get("codec_type") == "video"]
    return {"duration": float(j["format"]["duration"]), "w": v[0]["width"] if v else 0, "h": v[0]["height"] if v else 0, "audio": any(s.get("codec_type") == "audio" for s in j["streams"])}

def frames(path, fps, w=90, h=160, t0=None, dur=None, gray=False):
    cmd = ["ffmpeg", "-v", "error"] + (["-ss", f"{t0:.3f}"] if t0 is not None else []) + ["-i", path] + (["-t", f"{dur:.3f}"] if dur else []) + \
          ["-vf", f"fps={fps},scale={w}:{h}", "-f", "rawvideo", "-pix_fmt", "gray" if gray else "rgb24", "-"]
    r = subprocess.run(["timeout", "-k", "5", "300"] + cmd, capture_output=True)
    ch = 1 if gray else 3; n = len(r.stdout) // (w * h * ch)
    return np.frombuffer(r.stdout[: n * w * h * ch], np.uint8).reshape(n, h, w, ch) if n else np.zeros((0, h, w, ch), np.uint8)

def loudness(path):
    r = run(["ffmpeg", "-nostats", "-i", path, "-filter_complex", "ebur128=peak=true", "-f", "null", "-"])
    s = r.stderr[r.stderr.rfind("Summary:"):]
    I = re.search(r"I:\s+(-?[\d.]+|-inf) LUFS", s); P = re.search(r"Peak:\s+(-?[\d.]+|-inf) dBFS", s)
    f = lambda m: (float(m.group(1)) if m and m.group(1) != "-inf" else None)
    return f(I), f(P)

def seg_rms_db(path, spans):
    """RMS dBFS of the export audio over a set of timeline spans."""
    r = subprocess.run(["timeout", "-k", "5", "120", "ffmpeg", "-v", "error", "-i", path, "-ac", "1", "-ar", "16000", "-f", "f32le", "-"], capture_output=True)
    a = np.frombuffer(r.stdout, np.float32); parts = [a[int(s * 16000):int(e * 16000)] for s, e in spans if e > s]
    x = np.concatenate(parts) if parts else np.zeros(1, np.float32)
    return 20 * math.log10(math.sqrt(float(np.mean(x ** 2))) + 1e-9)

def overlap(a, b): return max(0.0, min(a[1], b[1]) - max(a[0], b[0]))
def total_overlap(spans, ivs): return sum(overlap(s, i) for s in spans for i in ivs)
def merge(iv):
    out = []
    for s, e in sorted(iv):
        if out and s <= out[-1][1] + 1e-6: out[-1][1] = max(out[-1][1], e)
        else: out.append([s, e])
    return out

def lin(x, good, bad):
    """1 at good, 0 at bad, linear between (either direction)."""
    if good == bad: return 1.0 if x == good else 0.0
    return float(min(1, max(0, (x - bad) / (good - bad))))

class Timeline:
    def __init__(self, proj, truth):
        self.p = proj; self.truth = truth; self.assets = {k: os.path.basename(v.get("path", "")) for k, v in proj.get("assets", {}).items()}
        self.media, self.text, self.stray = [], [], []
        for ti, t in enumerate(proj.get("tracks", [])):
            if t.get("muted") and False: pass
            for c in t.get("clips", []):
                src = c.get("source", {}); st = float(c["start"]); du = float(c["duration"])
                rec = {"id": c["id"], "track": ti, "track_id": t["id"], "start": st, "end": st + du, "in": float(c.get("in_point", 0)), "speed": float(c.get("speed", 1) or 1), "clip": c}
                if src.get("type") == "media":
                    rec["file"] = self.assets.get(src.get("asset"), ""); rec["info"] = truth["clips"].get(rec["file"], {})
                    if rec["file"] not in truth["clips"]: self.stray.append(rec["file"]); continue  # e.g. the export re-imported for review
                    au = c.get("audio") or {}; vol = (au.get("volume") or {}).get("base", 1.0)
                    rec["audible"] = not au.get("mute") and vol > 0.01 and not t.get("muted")
                    op = (c.get("props", {}).get("opacity") or {}); rec["visible"] = (op.get("base", 1.0) > 0.05 or bool(op.get("keys"))) and not t.get("hidden")
                    if (proj.get("assets", {}).get(src.get("asset"), {}).get("width") or 0) <= 0: rec["visible"] = False  # audio-only: no picture
                    self.media.append(rec)
                elif src.get("type") == "text":
                    rec["text_str"] = src.get("text", ""); self.text.append(rec)
        self.end = max([m["end"] for m in self.media + self.text] + [0])

    def src_to_tl(self, m, s, e):
        a = max(s, m["in"]); b = min(e, m["in"] + (m["end"] - m["start"]) * m["speed"])
        if b <= a: return None
        return [m["start"] + (a - m["in"]) / m["speed"], m["start"] + (b - m["in"]) / m["speed"]]

    def speech(self, who=None):
        iv = []
        for m in self.media:
            inf = m["info"]
            if inf.get("kind") != "talk" or not m["audible"]: continue
            if who and inf.get("speaker") != who: continue
            for s, e in inf.get("speech", []):
                x = self.src_to_tl(m, s, e)
                if x: iv.append(x)
        return merge(iv)

    def unwanted_speech(self):
        iv = []
        for m in self.media:
            inf = m["info"]
            if inf.get("kind") != "talk" or not m["audible"]: continue
            bad = inf.get("speech", []) if inf.get("flub") else ([inf["false_start"]] if inf.get("false_start") else [])
            for s, e in bad:
                x = self.src_to_tl(m, s, e)
                if x: iv.append(x)
        return merge(iv)

    def word_gaps(self):
        g = []
        for m in self.media:
            if m["info"].get("kind") != "talk" or not m["audible"]: continue
            for t in m["info"].get("word_gaps", []):
                x = self.src_to_tl(m, t - 0.001, t + 0.001)
                if x: g.append(x[0])
        return sorted(g)

    def top_media(self, t):
        best = None
        for m in self.media:
            if m["visible"] and m["start"] <= t < m["end"] and (best is None or m["track"] >= best["track"]): best = m
        return best

    def picture_runs(self):
        """[(start, end, media_rec)] of what is on top, sampled on the frame grid."""
        runs = []; n = int(round(self.end * FPS))
        for i in range(n):
            t = (i + 0.5) / FPS; m = self.top_media(t); key = (m["id"] if m else None)
            if runs and runs[-1][3] == key: runs[-1][1] = (i + 1) / FPS
            else: runs.append([i / FPS, (i + 1) / FPS, m, key])
        return [(a, b, m) for a, b, m, _ in runs]

def jitter(fr):
    """High-frequency camera shake in pixels: phase-correlation shift between consecutive frames, minus its 9-frame moving average."""
    if len(fr) < 6: return None
    g = fr[..., 0].astype(np.float32); g = g[:, g.shape[1] // 8: -g.shape[1] // 8, g.shape[2] // 8: -g.shape[2] // 8]
    win = np.outer(np.hanning(g.shape[1]), np.hanning(g.shape[2]))
    sh = []
    for a, b in zip(g[:-1], g[1:]):
        A = np.fft.fft2((a - a.mean()) * win); Bf = np.fft.fft2((b - b.mean()) * win); R = A * np.conj(Bf); R /= np.abs(R) + 1e-9
        c = np.fft.ifft2(R).real; y, x = np.unravel_index(np.argmax(c), c.shape)
        if y > c.shape[0] // 2: y -= c.shape[0]
        if x > c.shape[1] // 2: x -= c.shape[1]
        sh.append((x, y))
    sh = np.array(sh, np.float32); pos = np.cumsum(sh, 0); k = 9
    if len(pos) < k: return float(np.mean(np.abs(sh)))
    ma = np.stack([np.convolve(pos[:, i], np.ones(k) / k, mode="same") for i in range(2)], 1)
    res = (pos - ma)[k // 2: -(k // 2)]
    return float(np.sqrt(np.mean(res ** 2)))

def norm_words(s): return re.findall(r"[a-z0-9']+", s.lower().replace("’", "'"))

def score(out, footage, kimo_bin, judge=False, judge_model="claude-opus-5-5"):
    truth = json.load(open(os.path.join(footage, "truth.json"))); br = truth["brief"]
    final = os.path.join(out, "final.mp4"); projp = os.path.join(out, "project.json")
    checks, notes = {}, {}
    pr = probe(final) if os.path.exists(final) else None
    finished = bool(pr and pr["duration"] > 1 and pr["w"] == 720 and pr["h"] == 1280)
    res = {"finished": finished, "checks": checks, "notes": notes}
    if not finished:
        # no usable deliverable (missing, broken, wrong size, or overwritten): nothing else is worth scoring
        notes["format"] = pr; res["composite"] = 0.0; return res
    d = pr["duration"]; notes["duration_s"] = round(d, 2)
    if br.get("duration"):  # a brief that leaves the length to the editor has none
        lo, hi = br["duration"]; checks["duration"] = 1.0 if lo <= d <= hi else lin(min(abs(d - lo), abs(d - hi)), 0, 3)
    # picture: black / blown frames
    fr = frames(final, 5); y = fr.astype(np.float32).mean(axis=3) / 255 if len(fr) else np.zeros((0, 1, 1))
    ts = np.arange(len(fr)) / 5.0; inner = (ts > 0.15) & (ts < d - 0.15)
    mean = y.mean(axis=(1, 2)) if len(fr) else np.zeros(0); clip_hi = (y > 0.97).mean(axis=(1, 2)) if len(fr) else np.zeros(0)
    bad = inner & ((mean < 0.08) | (mean > 0.92) | (clip_hi > 0.35))
    checks["exposure"] = lin(bad.sum() / max(1, inner.sum()), 0, 0.25); notes["bad_frames_at"] = [round(float(t), 1) for t in ts[bad]][:12]
    # sound
    I, P = loudness(final); notes["lufs"] = I; notes["true_peak_dbtp"] = P
    checks["loudness"] = 0.0 if I is None else (1.0 if abs(I + 14) <= 1 else lin(abs(I + 14), 1, 5))
    checks["peak"] = 0.0 if P is None else (1.0 if P <= -1.0 else lin(P, -1.0, 1.0))
    if not os.path.exists(projp):
        # the brief asks for project.json; without it nothing about the edit itself can be measured, so those checks score 0
        notes["project"] = "missing"
        want = (["broll_in_speech", "dead_air", "captions_sync", "cuts_on_rhythm"] + ([] if truth.get("auto_truth") else ["best_take", "captions_words"]) + (["speaker_balance"] if br.get("speakers") == 2 else [])) \
            if br.get("speech_driven") else (["cuts_on_rhythm"] + ([] if truth.get("auto_truth") else ["shots_used", "text_present"]))
        if any(c.get("shaky") for c in truth["clips"].values()): want.append("stabilized")
        if any(c.get("dark") or c.get("cast") or c.get("overexposed") for c in truth["clips"].values()): want.append("grade")
        for w in want + ["text_safe"]: checks[w] = 0.0
        res["composite"] = round(100 * float(np.mean(list(checks.values()))), 1)
        if judge: res["judge"] = judge_pass(final, out, open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "briefs", truth["set"] + ".txt")).read(), judge_model)
        return res
    proj = json.load(open(projp)); tl = Timeline(proj, truth)
    runs = tl.picture_runs(); cuts = [a for a, b, m in runs[1:]]
    used = {m["file"] for a, b, m in runs if m}; notes["shots"] = sorted(used); notes["cuts"] = [round(c, 2) for c in cuts]
    if br.get("speech_driven"):
        sp = tl.speech(); notes["speech_tl"] = [[round(a, 2), round(b, 2)] for a, b in sp]
        brolls = [(a, b) for a, b, m in runs if m and m["info"].get("kind") == "broll"]
        btot = sum(b - a for a, b in brolls)
        dil = merge([[s - 0.2, e + 0.2] for s, e in sp])
        if btot == 0 and truth.get("auto_truth"): notes["broll_in_speech"] = "n/a (no clip without speech on screen)"
        else: checks["broll_in_speech"] = 0.0 if btot == 0 else total_overlap(brolls, dil) / btot
        notes["broll_s"] = round(btot, 2)
        # rough cut: unwanted takes gone, no dead air
        if truth.get("auto_truth"): notes["n/a"] = ["duration", "best_take", "captions_words", "stabilized", "grade", "speaker_balance"]  # no ground truth for real footage
        else: uw = tl.unwanted_speech(); checks["best_take"] = 1.0 if not uw else lin(sum(b - a for a, b in uw), 0, 2.0)
        gaps = [b[0] - a[1] for a, b in zip(sp, sp[1:])]
        lead = sp[0][0] if sp else d; tail = (d - sp[-1][1]) if sp else d
        worst = max(gaps + [0]); notes["longest_gap_s"] = round(worst, 2); notes["lead_s"] = round(lead, 2); notes["tail_s"] = round(tail, 2)
        checks["dead_air"] = float(np.mean([lin(worst, 0.8, 2.0), lin(lead, 2.2, 4.0), lin(tail, 1.2, 3.0)]))
        # captions
        caps = [x for x in tl.text if not (x["start"] < 0.3 and any(w in x["text_str"].upper() for w in ("KIMO", "EMBER", "MAYA'S")) and len(norm_words(x["text_str"])) <= 3)]
        cspans = merge([[x["start"], x["end"]] for x in caps]); stot = sum(b - a for a, b in sp)
        cov = total_overlap(sp, cspans) / stot if stot else 0.0
        ctot = sum(b - a for a, b in cspans); prec = total_overlap(cspans, merge([[s - 0.4, e + 0.4] for s, e in sp])) / ctot if ctot else 0.0
        script = []; seen_files = set()
        for m in tl.media:
            if m["info"].get("kind") == "talk" and m["audible"] and not m["info"].get("flub") and m["file"] not in seen_files:
                seen_files.add(m["file"]); script += norm_words(" ".join(m["info"].get("sentences") or m["info"].get("lines") or []))
        cw = norm_words(" ".join(x["text_str"] for x in caps)); bag = {}
        for w in cw: bag[w] = bag.get(w, 0) + 1
        hit = 0
        for w in script:
            if bag.get(w, 0) > 0: hit += 1; bag[w] -= 1
        words = hit / len(script) if script else 0.0
        checks["captions_sync"] = float(np.mean([cov, prec]))
        if not truth.get("auto_truth"): checks["captions_words"] = words  # an auto transcript is not a script to hold captions to
        notes["captions"] = {"coverage": round(cov, 2), "precision": round(prec, 2), "word_recall": round(words, 2), "n": len(caps)}
        # cuts on pauses
        gapsw = tl.word_gaps(); edges = [x for s, e in sp for x in (s, e)]
        def on_pause(c):
            if any(s + 0.12 < c < e - 0.12 for s, e in sp) is False: return True  # in silence or at a phrase edge
            return min([abs(c - g) for g in gapsw] + [9]) <= 0.1 or min([abs(c - x) for x in edges] + [9]) <= 0.15
        checks["cuts_on_rhythm"] = (sum(on_pause(c) for c in cuts) / len(cuts)) if cuts else 0.0
        if br.get("speakers") == 2:
            a = tl.speech("Maya"); b = tl.speech("Sam")
            if a and b:
                da, db = seg_rms_db(final, a), seg_rms_db(final, b); notes["speaker_rms_db"] = {"Maya": round(da, 1), "Sam": round(db, 1)}
                checks["speaker_balance"] = lin(abs(da - db), 2.0, 6.0)
            else: checks["speaker_balance"] = 0.0; notes["speaker_balance"] = "a speaker is missing"
    else:
        music = [m for m in tl.media if m["info"].get("kind") == "music" and m["audible"]]
        if not music: checks["cuts_on_rhythm"] = 0.0; notes["music"] = "no audible music clip"
        else:
            mm = music[0]; beats = [x for x in (mm["start"] + (b - mm["in"]) / mm["speed"] for b in mm["info"]["beats"]) if mm["start"] - 1e-6 <= x <= mm["end"]]
            good = [min([abs(c - b) for b in beats] + [9]) <= 1.5 / FPS for c in cuts]
            checks["cuts_on_rhythm"] = (sum(good) / len(cuts)) if cuts else 0.0; notes["off_beat_cuts"] = [round(c, 2) for c, g in zip(cuts, good) if not g]
        if truth.get("auto_truth"): notes["n/a"] = ["shots_used", "text_present", "stabilized", "grade"]
        else: checks["shots_used"] = min(1.0, len([u for u in used if truth["clips"].get(u, {}).get("kind") == "broll"]) / 4)
        if not truth.get("auto_truth"):
            want = ["EMBER COFFEE", "FRESH ROAST DAILY", "@EMBERCOFFEE"]; have = " ".join(x["text_str"].upper().replace("\n", " ") for x in tl.text)
            checks["text_present"] = sum(w in re.sub(r"\s+", " ", have) for w in want) / len(want)
    # shaky clips stabilized (measured on the export against the raw source over the same span)
    st = []
    for a, b, m in runs:
        if not (m and m["info"].get("shaky")) or b - a < 0.4: continue
        src_t = m["in"] + (a - m["start"]) * m["speed"]
        je = jitter(frames(final, FPS, 180, 320, a + 1 / FPS, b - a - 2 / FPS)); jr = jitter(frames(os.path.join(footage, m["file"]), FPS, 180, 320, src_t + 1 / FPS, b - a - 2 / FPS))
        if je is not None and jr: st.append(je / jr); notes.setdefault("jitter", []).append({"at": round(a, 2), "export_px": round(je, 2), "raw_px": round(jr, 2)})
    if any(c.get("shaky") for c in truth["clips"].values()):
        checks["stabilized"] = float(np.mean([lin(r, 0.5, 0.9) for r in st])) if st else 0.0
    # graded shots: luma and cast where a defective shot is on screen
    gr = []
    for a, b, m in runs:
        inf = m["info"] if m else {}
        if not (inf.get("dark") or inf.get("cast") or inf.get("overexposed")) or b - a < 0.3: continue
        f = frames(final, 6, 90, 160, a + 0.05, b - a - 0.1).astype(np.float32) / 255
        if not len(f): continue
        rgb = f.reshape(-1, 3).mean(0); lum = float(0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]); cast = float(max(abs(rgb[0] - rgb[1]), abs(rgb[2] - rgb[1])))
        ok_l = 1.0 if 0.25 <= lum <= 0.68 else lin(min(abs(lum - 0.25), abs(lum - 0.68)), 0, 0.15); ok_c = lin(cast, 0.06, 0.16)
        gr.append(min(ok_l, ok_c)); notes.setdefault("graded", []).append({"file": m["file"], "at": round(a, 2), "luma": round(lum, 3), "cast": round(cast, 3)})
    if gr: checks["grade"] = float(np.mean(gr))
    # text inside the safe area, measured by kimo
    k = Kimo(kimo_bin)
    try:
        k.j("load_project", {"path": projp}); avoid = [z["rect"] for z in k.j("query_safe_margins", {"profile": "tiktok"})["zones"] if z["kind"] == "avoid"]
        ts_ = k.j("query_safe_margins", {"profile": "title_safe"}); ts_r = [z["rect"] for z in ts_["zones"] if z["kind"] == "safe"][0]
        sx0, sy0, sx1, sy1 = ts_r[0], ts_r[1], ts_r[0] + ts_r[2], ts_r[1] + ts_r[3]; inside = []; outs = []
        for x in tl.text:
            try: bb = k.j("compute_text_bounds", {"clip_id": x["id"], "t": round((x["end"] - x["start"]) / 2, 3)})
            except Exception as e: continue
            if not isinstance(bb, dict) or "canvas_rect" not in bb: continue  # a clip kimo cannot measure (no glyphs, empty text) is skipped
            rx, ry, rw, rh = bb["canvas_rect"]; ok = rx >= sx0 - 4 and ry >= sy0 - 4 and rx + rw <= sx1 + 4 and ry + rh <= sy1 + 4
            ok = ok and not any(overlap((rx, rx + rw), (z[0], z[0] + z[2])) > 4 and overlap((ry, ry + rh), (z[1], z[1] + z[3])) > 4 for z in avoid)  # clear of the platform UI (tiktok estimate)
            inside.append(ok)
            if not ok: outs.append({"clip": x["id"], "text": x["text_str"][:30], "rect": [round(v) for v in bb["canvas_rect"]]})
        checks["text_safe"] = (sum(inside) / len(inside)) if inside else 0.0; notes["text_out_of_safe"] = outs[:6]
    finally: k.close()
    res["composite"] = round(100 * float(np.mean(list(checks.values()))), 1)
    if judge: res["judge"] = judge_pass(final, out, open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "briefs", truth["set"] + ".txt")).read(), judge_model)
    return res

def contact_sheet(final, path, n=12, cols=4):
    d = probe(final)["duration"]; tiles = []
    for i in range(n):
        t = min(d - 0.05, d * (i + 0.5) / n)
        r = subprocess.run(["timeout", "-k", "5", "60", "ffmpeg", "-v", "error", "-ss", f"{t:.3f}", "-i", final, "-frames:v", "1", "-vf", "scale=180:320", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], capture_output=True)
        tiles.append(Image.frombytes("RGB", (180, 320), r.stdout[:180 * 320 * 3]) if len(r.stdout) >= 180 * 320 * 3 else Image.new("RGB", (180, 320), (255, 0, 255)))
    rows = math.ceil(n / cols); sh = Image.new("RGB", (cols * 180, rows * 320))
    for i, im in enumerate(tiles): sh.paste(im, ((i % cols) * 180, (i // cols) * 320))
    sh.save(path); return path

JUDGE_PROMPT = """You are judging a vertical short made by an automatic editor from the brief below. The image is a contact sheet: 12 frames
evenly spaced in time, left to right, top to bottom. Judge only what you can see: shot choice and order, colour and exposure,
stability blur, text legibility and placement (inside a phone's safe area, not cut off), and whether it looks like a finished,
creator-quality short that meets the brief. Answer with JSON only, no code fence: {"score": <1-10>, "issues": [up to 4 short strings]}.

BRIEF:
"""

def judge_pass(final, out, brief, model):
    import requests
    sheet = contact_sheet(final, os.path.join(out, "judge_sheet.png"))
    cap = json.load(open(os.environ["KIMO_EVAL_CAPABILITY"])) if os.path.exists(os.environ.get("KIMO_EVAL_CAPABILITY", "")) else {}
    base = os.environ.get("ANTHROPIC_BASE_URL") or (cap.get("base_url", "") + "/anthropic"); key = os.environ.get("ANTHROPIC_API_KEY") or cap.get("token")
    b64 = base64.b64encode(open(sheet, "rb").read()).decode()
    body = {"model": model, "max_tokens": 1200, "messages": [{"role": "user", "content": [{"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64}}, {"type": "text", "text": JUDGE_PROMPT + brief.split("DELIVER:")[0]}]}]}
    r = requests.post(base.rstrip("/") + "/v1/messages", headers={"x-api-key": key, "anthropic-version": "2023-06-01"}, json=body, timeout=180)
    if r.status_code != 200: return {"error": r.text[:300]}
    t = "".join(b.get("text", "") for b in r.json()["content"]); u = r.json().get("usage", {})
    m = re.search(r"\{.*\}", t, re.S)
    try: j = json.loads(m.group(0)) if m else {"raw": t}
    except ValueError: j = {"raw": t}
    j["model"] = model; j["cost_usd"] = round((u.get("input_tokens", 0) * 4 + u.get("output_tokens", 0) * 20) / 1e6, 4)
    return j

if __name__ == "__main__":
    ap = argparse.ArgumentParser(); ap.add_argument("--out", required=True); ap.add_argument("--footage", required=True); ap.add_argument("--kimo", required=True)
    ap.add_argument("--judge", action="store_true"); ap.add_argument("--judge-model", default="claude-opus-5-5")
    a = ap.parse_args(); r = score(a.out, a.footage, a.kimo, a.judge, a.judge_model)
    json.dump(r, open(os.path.join(a.out, "score.json"), "w"), indent=1); print(json.dumps(r, indent=1))
