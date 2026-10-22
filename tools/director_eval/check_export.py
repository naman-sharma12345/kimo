#!/usr/bin/env python3
"""Is this export a watchable video? duration, streams, black share, frozen share, mean luma over 10 samples."""
import json, subprocess, sys, re
def run(c, t=180): return subprocess.run(["timeout", "-k", "5", str(t)] + c, capture_output=True, text=True, stdin=subprocess.DEVNULL)
def check(p):
    r = run(["ffprobe", "-v", "error", "-show_entries", "stream=codec_type,codec_name,width,height:format=duration,size", "-of", "json", p], 60)
    try: j = json.loads(r.stdout)
    except Exception: return {"ok": False, "why": "ffprobe failed: " + r.stderr[:200]}
    st = j.get("streams", []); fmt = j.get("format", {})
    v = [s for s in st if s["codec_type"] == "video"]; a = [s for s in st if s["codec_type"] == "audio"]
    dur = float(fmt.get("duration") or 0); out = {"duration": round(dur, 2), "size_mb": round(int(fmt.get("size", 0)) / 1e6, 2), "video": v[0] if v else None, "audio": bool(a)}
    if not v or dur < 5: out.update(ok=False, why="no video or under 5 s"); return out
    r = run(["ffmpeg", "-v", "info", "-i", p, "-vf", "blackdetect=d=0.3:pix_th=0.08,freezedetect=n=0.002:d=2,signalstats,metadata=print:key=lavfi.signalstats.YAVG:file=-", "-an", "-f", "null", "-"], 400)
    blacks = sum(float(x) for x in re.findall(r"black_duration:([\d.]+)", r.stderr))
    freezes = sum(float(x) for x in re.findall(r"freeze_duration: ?([\d.]+)", r.stderr))
    ys = [float(x) for x in re.findall(r"YAVG=([\d.]+)", r.stdout)]
    out.update(black_share=round(blacks / dur, 3), frozen_share=round(freezes / dur, 3), mean_luma=round(sum(ys) / len(ys), 1) if ys else None, frames=len(ys))
    bad = []
    if out["black_share"] > 0.15: bad.append("black %.0f%%" % (100 * out["black_share"]))
    if out["frozen_share"] > 0.5: bad.append("frozen %.0f%%" % (100 * out["frozen_share"]))
    if ys and out["mean_luma"] < 25: bad.append("too dark")
    out.update(ok=not bad, why=", ".join(bad) or "ok"); return out
if __name__ == "__main__":
    for p in sys.argv[1:]: print(json.dumps({"path": p, **check(p)}))
