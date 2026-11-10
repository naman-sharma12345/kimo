#!/usr/bin/env python3
"""Scorer v2 (stricter): a 16-frame vision judge anchored by two reference sheets in the same request
(BAD = the raw clips joined in order with no editing, fixed at 1-2; REFERENCE = Opus 5.5's edit, shown with no score),
a strict written rubric, 2 samples averaged, then
  composite_v2 = 0.65 * judge% + 0.35 * measured_v1   (judge% = (score-1)/9*100)
  and capped at 40 when the judge mean is <= 3 (a bad-looking edit cannot score well on loudness alone).
measured_v1 = score.py's measured composite (unchanged). Never inflates: v2 <= v1 whenever the judge is below 8.5/10."""
import base64, json, os, re, sys, concurrent.futures as cf
import requests
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import score as S
RUBRIC = """You are a strict senior short-form video editor judging a vertical short (9:16) made by an automatic editor from a creator's raw phone clips.
You get three contact sheets, each 16 frames evenly spaced in time (left to right, top to bottom):
  SHEET 1 = BAD ANCHOR: the raw clips simply joined in order, no editing at all. This is what a 1-2 looks like.
  SHEET 2 = REFERENCE: an edit of the same clips by the strongest editor in this test. Use it only for scale; it is not perfect.
  SHEET 3 = THE EDIT TO JUDGE.
Score SHEET 3 from 1 to 10 against this rubric, as a creator would judge it before posting:
  1-2  raw footage or broken (black, frozen, wrong framing), or barely different from SHEET 1
  3-4  minimal edit: clips in order with a few cuts or plain text; amateur; would not post
  5-6  a real edit with a clear structure and readable text, but plain, shaky or messy in places; postable with fixes
  7-8  good creator quality: strong hook/opening, clean pacing, styled readable captions/titles, consistent look
  9-10 top creator quality, nothing to fix
Penalise hard: motion-blurred or badly framed shots kept in, repeated near-identical shots, text that is cluttered, overlapping, cut off,
unreadable or contradicts the picture, garbled/nonsense transcript text shown on screen, no hook, a story that wanders, an ending on a dead shot.
Do not reward effort or quantity of graphics; reward what a viewer would see. If SHEET 3 is hardly better than SHEET 1, give it 1-3.
Answer with JSON only: {"score": <1-10 integer>, "vs_bad": "<one line>", "issues": [up to 4 short strings]}"""
def b64(p): return base64.b64encode(open(p, "rb").read()).decode()
def sheet(video, path):
    if not os.path.exists(path): S.contact_sheet(video, path, n=16, cols=4)
    return path
def judge_once(bad, ref, cand, model="claude-opus-5-5"):
    base = os.environ["ANTHROPIC_BASE_URL"]; key = os.environ["ANTHROPIC_API_KEY"]
    content = []
    for label, p in (("SHEET 1 (BAD ANCHOR)", bad), ("SHEET 2 (REFERENCE)", ref), ("SHEET 3 (THE EDIT TO JUDGE)", cand)):
        content += [{"type": "text", "text": label}, {"type": "image", "source": {"type": "base64", "media_type": "image/png", "data": b64(p)}}]
    content.append({"type": "text", "text": RUBRIC})
    r = requests.post(base.rstrip("/") + "/v1/messages", headers={"x-api-key": key, "anthropic-version": "2023-06-01"},
                      json={"model": model, "max_tokens": 800, "messages": [{"role": "user", "content": content}]}, timeout=240)
    if r.status_code != 200: return {"error": r.text[:300]}
    t = "".join(b.get("text", "") for b in r.json()["content"]); u = r.json().get("usage", {})
    m = re.search(r"\{.*\}", t, re.S)
    try: j = json.loads(m.group(0))
    except Exception: j = {"raw": t[:500]}
    j["cost_usd"] = round((u.get("input_tokens", 0) * 4 + u.get("output_tokens", 0) * 20) / 1e6, 4); return j
def score_v2(name, video, measured_v1, bad_sheet, ref_sheet, workdir, n=2):
    cand = sheet(video, os.path.join(workdir, f"sheet16_{name}.png"))
    with cf.ThreadPoolExecutor(n) as ex: js = list(ex.map(lambda _: judge_once(bad_sheet, ref_sheet, cand), range(n)))
    sc = [j["score"] for j in js if isinstance(j.get("score"), (int, float))]
    jm = sum(sc) / len(sc) if sc else None
    comp = None
    if jm is not None:
        jp = (jm - 1) / 9 * 100
        comp = 0.65 * jp + 0.35 * (measured_v1 if measured_v1 is not None else jp)
        if jm <= 3: comp = min(comp, 40.0)
        comp = round(comp, 1)
    return {"name": name, "judge_v2_mean": jm, "judge_v2_samples": sc, "composite_v2": comp, "measured_v1": measured_v1,
            "issues": (js[0].get("issues") if js else None), "vs_bad": (js[0].get("vs_bad") if js else None), "cost_usd": round(sum(j.get("cost_usd", 0) for j in js), 4)}
