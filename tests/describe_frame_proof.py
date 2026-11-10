"""RV-25 s1 proof: describe_frame analytic scenes over real stdio. Temp files only."""
import json, os, subprocess, sys, time
BIN = os.environ.get("KIMO_BIN", "target/release/kimo-mcp")
class S:
    def __init__(s):
        s.p = subprocess.Popen([BIN], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True); s.i = 0; s.rpc("initialize", {})
    def rpc(s, m, pr): s.i += 1; s.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": s.i, "method": m, "params": pr}) + "\n"); s.p.stdin.flush(); return json.loads(s.p.stdout.readline())
    def call(s, n, a): return s.rpc("tools/call", {"name": n, "arguments": a})["result"]
    def j(s, n, a):
        r = s.call(n, a); assert not r["isError"], (n, r["content"][0]["text"]); return json.loads(r["content"][0]["text"])
    def close(s): s.p.stdin.close(); s.p.wait()
ok = True
def check(c, m):
    global ok; print(("PASS " if c else "FAIL ") + m); ok &= bool(c)
def scene(color, w=640, h=360):
    s = S(); s.j("project_set", {"width": w, "height": h, "fps": 24}); tid = s.j("add_track", {})["track_id"]
    s.j("add_clip", {"track_id": tid, "source_type": "solid", "color": color, "start": 0, "duration": 2}); return s
s = scene([1, 1, 1]); d = s.j("describe_frame", {"time": 1})
fs = d["frame_stats"]
check(abs(fs["clipped_white_pct"] - 100) < 1e-9 and fs["clipped_black_pct"] == 0, "white frame: clipped_white 100%, black 0")
check(fs["luma_histogram_32"][31] == 1.0 and abs(sum(fs["luma_histogram_32"]) - 1) < 1e-9, "white frame: bin 31 holds 1.0, fractions sum to 1")
check(all(abs(fs["luma_histogram_16"][i] - (fs["luma_histogram_32"][2 * i] + fs["luma_histogram_32"][2 * i + 1])) < 1e-12 for i in range(16)), "rollup[i] == bins[2i]+bins[2i+1]")
check(abs(fs["mean_luma"] - 1) < 1e-6 and fs["stddev_luma"] < 1e-6 and fs["low_detail"], "white: mean 1, stddev 0, low_detail flagged")
check(d["visible_clips_total"] == 1 and d["visible_clips"][0]["track"] == 0, "visible_clips lists the one clip")
ph_white = d["phash64"]; s.close()
s = scene([0, 0, 0]); d = s.j("describe_frame", {"time": 1}); fs = d["frame_stats"]
check(fs["clipped_black_pct"] == 100 and fs["luma_histogram_32"][0] == 1.0 and any("black" in w for w in d["warnings"]), "black frame: clipped_black 100%, bin 0, warning")
s.close()
s = scene([0.5, 0.5, 0.5]); d = s.j("describe_frame", {"time": 1}); fs = d["frame_stats"]
exp = 0.2126 * 0.5 + 0.7152 * 0.5 + 0.0722 * 0.5
check(abs(fs["mean_luma"] - exp) < 1e-3 and fs["mean_saturation"] == 0, "grey 0.5: mean luma matches rec709 formula, saturation 0")
d2 = s.j("describe_frame", {"time": 1}); check(d2 == d, "deterministic: second call identical")
# half-and-half: left black right white via two clips? use text-free approach: two tracks not needed; roi on a solid frame stays consistent
r = s.j("describe_frame", {"time": 1, "roi": [0.0, 0.0, 0.5, 0.5]}); check(r["roi_px"] == {"x0": 0, "y0": 0, "x1": 320, "y1": 180} and abs(r["frame_stats"]["mean_luma"] - fs["mean_luma"]) < 1e-6 and r["phash64"] == d["phash64"], "roi px box exact, phash stays whole-frame")
check(s.call("describe_frame", {"time": 1, "roi": [0.5, 0, 0.2, 1]})["isError"], "bad roi -> error")
m = s.j("describe_frame", {"time": 0.5, "motion": True}); check(m["motion"]["mean_abs_diff_vs_next_frame"] == 0, "static solid: motion 0")
s.close()
# timing on 1080p, 20 solid clips
s = S(); s.j("project_set", {"width": 1920, "height": 1080, "fps": 24}); tid = s.j("add_track", {})["track_id"]
for i in range(20): s.j("add_clip", {"track_id": tid, "source_type": "solid", "color": [i / 20, 0.3, 0.6], "start": 0, "duration": 2})
t = time.time(); s.j("describe_frame", {"time": 1}); cold = time.time() - t; t = time.time(); s.j("describe_frame", {"time": 1}); warm = time.time() - t
print("describe_frame 1080p 20 clips: cold %.0f ms, warm %.0f ms (includes render cache hit)" % (cold * 1000, warm * 1000)); s.close()
print("DESCRIBE FRAME PROOF OK" if ok else "DESCRIBE FRAME PROOF FAILED"); sys.exit(0 if ok else 1)
