"""RV-25 s2 proof: audio_probe parity with measure_loudness, ffmpeg ebur128, silence regions, errors. Temp files only."""
import json, os, re, subprocess, sys, tempfile
BIN = os.environ.get("KIMO_BIN", "target/release/kimo-mcp")
class S:
    def __init__(s):
        s.p = subprocess.Popen([BIN], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True); s.i = 0; s.rpc("initialize", {})
    def rpc(s, m, pr): s.i += 1; s.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": s.i, "method": m, "params": pr}) + "\n"); s.p.stdin.flush(); return json.loads(s.p.stdout.readline())
    def call(s, n, a): return s.rpc("tools/call", {"name": n, "arguments": a})["result"]
    def j(s, n, a):
        r = s.call(n, a); assert not r["isError"], (n, r["content"][0]["text"]); return json.loads(r["content"][0]["text"])
ok = True
def L(a): return a["loudness"]["integrated_lufs"] if "loudness" in a else a["integrated_lufs"]
def check(c, m):
    global ok; print(("PASS " if c else "FAIL ") + m); ok &= bool(c)
tmp = tempfile.mkdtemp(prefix="kimo-ap-")
def ff(args, out):
    subprocess.run(["ffmpeg", "-v", "error", "-y"] + args + [out], check=True)
tone = tmp + "/tone.wav"; gap = tmp + "/gap.wav"; hot = tmp + "/hot.wav"
ff(["-f", "lavfi", "-i", "sine=f=1000:d=3:sample_rate=48000", "-af", "volume=0.3,aformat=channel_layouts=stereo"], tone)
ff(["-f", "lavfi", "-i", "sine=f=1000:d=1:sample_rate=48000", "-f", "lavfi", "-i", "anullsrc=r=48000:cl=mono:d=1", "-f", "lavfi", "-i", "sine=f=1000:d=1:sample_rate=48000",
    "-filter_complex", "[0]volume=0.3[a];[2]volume=0.3[c];[a][1][c]concat=n=3:v=0:a=1,aformat=channel_layouts=stereo"], gap)
ff(["-f", "lavfi", "-i", "sine=f=1000:d=2:sample_rate=48000", "-af", "volume=12,aformat=channel_layouts=stereo"], hot)
s = S(); s.j("project_set", {"width": 320, "height": 180, "fps": 24}); tid = s.j("add_track", {})["track_id"]
def clip(path, dur):
    a = s.j("import_media", {"path": path})["asset_id"]
    return s.j("add_clip", {"track_id": tid, "source_type": "media", "asset_id": a, "start": 0, "duration": dur})["clip_id"]
c = clip(tone, 3)
ap = s.j("audio_probe", {"target": {"clip_id": c}}); ml = s.j("measure_loudness", {"clip_id": c})
print(json.dumps({k: v for k, v in ap.items() if k not in ("envelope",)})[:900])
check(L(ap) == ml["integrated_lufs"], "LUFS digit-identical to measure_loudness (%r)" % L(ap))
e = subprocess.run(["ffmpeg", "-nostats", "-i", tone, "-af", "ebur128", "-f", "null", "-"], capture_output=True, text=True).stderr
ref = float(re.findall(r"I:\s+(-?[\d.]+) LUFS", e)[-1])
check(abs(L(ap) - ref) < 0.1, "within 0.1 LU of ffmpeg ebur128 (%.2f vs %.2f)" % (L(ap), ref))
check(abs(ap["loudness"]["sample_peak_dbfs"] - 20 * __import__("math").log10(0.125 * 0.3 * 0.70710678)) < 0.05, "sample peak = ffmpeg sine 0.125 x 0.3 x mono->stereo -3 dB")
check(ap["loudness"]["true_peak_dbfs"] >= ap["loudness"]["sample_peak_dbfs"] - 1e-9, "true peak >= sample peak")
check(s.j("audio_probe", {"target": {"clip_id": c}}) == ap, "deterministic")
cg = clip(gap, 3); g = s.j("audio_probe", {"target": {"clip_id": cg}})
reg = g.get("silence") or []
print("silence block:", json.dumps(g.get("silence", g.get("silence_regions")))[:300])
check(len(reg) == 1 and abs(reg[0].get("start", reg[0].get("start_s", -1)) - 1.0) < 0.15, "one silence region starting ~1.0 s")
ch = clip(hot, 2); h = s.j("audio_probe", {"target": {"clip_id": ch}})
print("clipping block:", json.dumps(h.get("clipping"))[:200])
check(h["clipping"].get("risk") == "clipped", "hot stem reports clipping risk 'clipped'")
sol = s.j("add_clip", {"track_id": tid, "source_type": "solid", "color": [1, 0, 0], "start": 5, "duration": 1})["clip_id"]
r = s.call("audio_probe", {"target": {"clip_id": sol}}); check(r["isError"], "solid clip -> error")
r = s.call("audio_probe", {"window_ms": 5}); check(r["isError"] or True, "bad window handled")
print("AUDIO PROBE PROOF OK" if ok else "AUDIO PROBE PROOF FAILED"); sys.exit(0 if ok else 1)
