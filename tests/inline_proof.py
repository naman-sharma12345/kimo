"""RV-25 s1 proof: render_frame inline image blocks over real stdio. Writes only under a temp dir."""
import base64, hashlib, json, os, struct, subprocess, sys, tempfile
BIN = os.environ.get("KIMO_BIN", "target/release/kimo-mcp"); CLI = os.environ.get("KIMO_CLI", "target/release/kimo")
T = tempfile.mkdtemp(prefix="inline_proof_")

class S:
    def __init__(self, env=None):
        e = dict(os.environ); e.update(env or {}); self.i = 0
        self.p = subprocess.Popen([BIN], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, env=e); self.rpc("initialize", {})
    def rpc(self, m, pr):
        self.i += 1; self.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": self.i, "method": m, "params": pr}) + "\n"); self.p.stdin.flush()
        return json.loads(self.p.stdout.readline())
    def call(self, n, a): return self.rpc("tools/call", {"name": n, "arguments": a})["result"]
    def close(self): self.p.stdin.close(); self.p.wait()

def dims(png): w, h = struct.unpack(">II", png[16:24]); return w, h
def imgs(r): return [c for c in r["content"] if c["type"] == "image"]
def notes(r): return [c["text"] for c in r["content"][1:] if c["type"] == "text"]
def scene(s, w, h):
    s.call("project_set", {"width": w, "height": h, "fps": 24})
    t = s.call("add_track", {}); tid = json.loads(t["content"][0]["text"])["track_id"]
    s.call("add_clip", {"track_id": tid, "source_type": "solid", "color": [0.2, 0.4, 0.8], "start": 0, "duration": 2})

ok = True
def check(c, msg):
    global ok; print(("PASS " if c else "FAIL ") + msg); ok &= bool(c)

# H1: small frame, scale==1 png -> block bytes equal file
s = S(); scene(s, 320, 180); f = f"{T}/a.png"
r = s.call("render_frame", {"time": 0.5, "path": f}); b = imgs(r); fb = open(f, "rb").read()
check(len(b) == 1 and b[0]["mimeType"] == "image/png" and base64.b64decode(b[0]["data"]) == fb, "scale==1 png block equals file bytes")
check(r["structuredContent"]["inline"]["scale"] == 1.0 and r["structuredContent"]["sha256"] == hashlib.sha256(fb).hexdigest(), "structured sha256 + scale 1")
check(json.loads(r["content"][0]["text"]) == {"path": f}, "text result unchanged")
# B has no blocks after A
r2 = s.call("project_info", {}) if False else s.call("list_effects", {}); check(not imgs(r2) and "structuredContent" not in r2, "call after image has no blocks")
# error call after media: bad time -> no blocks
r3 = s.call("render_frame", {"time": "x", "path": f}); check(r3["isError"] and not imgs(r3), "error call has no blocks")
# never / downscale
r4 = s.call("render_frame", {"time": 0.5, "path": f, "inline": "never"}); check(not imgs(r4) and r4["content"][0]["text"] == r["content"][0]["text"], "inline never: no block, same text")
r5 = s.call("render_frame", {"time": 0.5, "path": f, "max_inline_px": 128}); b5 = imgs(r5)
check(len(b5) == 1 and max(dims(base64.b64decode(b5[0]["data"]))) == 128 and dims(open(f, "rb").read()) == (320, 180), "max_inline_px 128: inline long edge 128, disk file still 320x180")
s.close()
# KIMO_INLINE_IMAGES=0
s = S({"KIMO_INLINE_IMAGES": "0"}); scene(s, 320, 180); r = s.call("render_frame", {"time": 0.5, "path": f}); check(not imgs(r), "KIMO_INLINE_IMAGES=0: no block"); s.close()
# cap: forced tiny cap via env hook
s = S({"KIMO_INLINE_CAP_BYTES": "100"}); scene(s, 320, 180); r = s.call("render_frame", {"time": 0.5, "path": f})
check(not imgs(r) and not r["isError"] and any("inline omitted" in n for n in notes(r)), "forced tiny cap: block omitted + note, result ok"); s.close()
# ffmpeg missing (downscale needs it): block omitted + note, result still ok
s = S({"KIMO_INLINE_FFMPEG": "/nonexistent/ffmpeg"}); scene(s, 320, 180); r = s.call("render_frame", {"time": 0.5, "path": f, "max_inline_px": 128})
check(not r["isError"] and not imgs(r) and any("inline omitted" in n for n in notes(r)) and os.path.getsize(f) > 0, "ffmpeg missing: block omitted + note, result ok, file written"); s.close()
# CLI has no base64
if os.path.exists(CLI):
    o = subprocess.run([CLI, "call", "render_frame", "--args", json.dumps({"time": 0.5, "path": f})], capture_output=True, text=True)
    check(f in o.stdout and "base64" not in o.stdout and "iVBOR" not in o.stdout, "CLI call output has path, no base64")
print("INLINE PROOF OK" if ok else "INLINE PROOF FAILED"); sys.exit(0 if ok else 1)
