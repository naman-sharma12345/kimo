#!/usr/bin/env python3
"""MCP-level proof for refine_matte on H.264 footage: textured fg with soft edge + hair strands over a textured bg, rough outline seed, measures alpha error vs ground truth.
usage: python3 examples/refine_matte_proof.py [kimo-mcp]; writes /tmp/matte_proof_{seed,alpha,gt,frame}.png"""
import sys, json, subprocess, os, numpy as np, tempfile
W, H = 640, 360; rng = np.random.default_rng(4)
def tex(base):
    t = np.zeros((H, W, 3), np.float32)
    for s in (6, 12, 24, 48): t += np.kron(rng.random((H // s + 1, W // s + 1, 3)).astype(np.float32), np.ones((s, s, 1), np.float32))[:H, :W] / 4
    return np.clip(np.array(base, np.float32) + 0.5 * (t - t.mean()), 0, 1)
fg, bg = tex([0.75, 0.35, 0.3]), tex([0.25, 0.45, 0.7]); ys, xs = np.mgrid[0:H, 0:W].astype(np.float32)
d = np.sqrt(((xs - 320) / 170) ** 2 + ((ys - 180) / 120) ** 2); gt = np.clip((1 - d) * 170 / 3 + 0.5, 0, 1)
for s in range(80):
    th = s / 80 * 6.2832
    for t in range(22):
        x, y = int(320 + 170 * (1 + t * 0.008) * np.cos(th)), int(180 + 120 * (1 + t * 0.008) * np.sin(th))
        if 0 <= x < W and 0 <= y < H: gt[y, x] = max(gt[y, x], 0.6 - t * 0.02)
img = gt[..., None] * fg + (1 - gt[..., None]) * bg; tmp = tempfile.mkdtemp(); mp4 = os.path.join(tmp, "m.mp4")
p = subprocess.Popen(["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", "24", "-i", "-", "-t", "0.2", "-c:v", "libx264", "-crf", "18", "-pix_fmt", "yuv420p", mp4], stdin=subprocess.PIPE)
for _ in range(5): p.stdin.write((img * 255).astype(np.uint8).tobytes())
p.stdin.close(); p.wait()
srv = subprocess.Popen([sys.argv[1] if len(sys.argv) > 1 else "target/release/kimo-mcp"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True); mid = [0]
def rpc(m, params=None): mid[0] += 1; srv.stdin.write(json.dumps({"jsonrpc": "2.0", "id": mid[0], "method": m, "params": params or {}}) + "\n"); srv.stdin.flush(); return json.loads(srv.stdout.readline())
def call(n, a):
    r = rpc("tools/call", {"name": n, "arguments": a})["result"]; t = r["content"][0]["text"]
    if r.get("isError"): raise SystemExit(f"{n} failed: {t}")
    return json.loads(t)
rpc("initialize", {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "p", "version": "0"}})
call("project_set", {"width": W, "height": H, "fps": 24}); a = call("import_media", {"path": mp4})["asset_id"]; tr = call("add_track", {})["track_id"]
cid = call("add_clip", {"track_id": tr, "source_type": "media", "asset_id": a, "start": 0, "duration": 0.2})["clip_id"]
th = np.linspace(0, 2 * np.pi, 72, endpoint=False); poly = [[float(322 + 168 * np.cos(t)), float(178 + 121 * np.sin(t))] for t in th]  # rough, ~3px off
r = call("refine_matte", {"clip_id": cid, "time": 0.0, "points": poly, "band": 10, "out_png": "/tmp/matte_proof_alpha.png", "fg_png": "/tmp/matte_proof_fg.png", **__import__("json").loads(__import__("os").environ.get("MATTE_FLAGS", "{}"))})
print("per_frame", r["per_frame"])
print(json.dumps({k: r[k] for k in ("method", "frames", "seconds", "per_frame", "mask_id")}))
raw = open(os.path.join(r["dir"], "matte.bin"), "rb").read()[16:]; al = np.frombuffer(raw, np.uint8)[: W * H].reshape(H, W) / 255.0
seed = (((xs - 322) / 168) ** 2 + ((ys - 178) / 121) ** 2 < 1).astype(np.float32)
err = lambda a: (np.abs(a - gt).mean() * 1000, np.abs(a - gt)[(gt > 0.02) & (gt < 0.98)].mean())
for name, a_ in (("rough seed", seed), ("refine_matte", al)): e = err(a_); print(f"{name}: MAE x1000 {e[0]:.2f}, mean error on partial-alpha px {e[1]:.3f}")
def save(path, a_): subprocess.run(["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "gray", "-s", f"{W}x{H}", "-i", "-", "-frames:v", "1", path], input=(np.clip(a_, 0, 1) * 255).astype(np.uint8).tobytes())
save("/tmp/matte_proof_seed.png", seed); save("/tmp/matte_proof_gt.png", gt)
assert err(al)[0] < err(seed)[0] * 0.8; print("OK")
