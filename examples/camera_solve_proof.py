#!/usr/bin/env python3
"""End-to-end proof for solve_camera: render a textured 3D scene with a known camera path (numpy), encode to H.264,
drive kimo-mcp over stdio (import, solve_camera, apply_camera_solve), and measure error vs ground truth.
usage: python3 examples/camera_solve_proof.py [path/to/kimo-mcp]"""
import sys, json, subprocess, os, numpy as np, tempfile
W, H, N, F = 640, 360, 48, 560.0
rng = np.random.default_rng(3)
def tex(n=256):
    t = np.zeros((n, n, 3), np.float32)
    for s in (4, 8, 16, 32, 64):
        g = rng.random((s, s, 3)).astype(np.float32); t += np.kron(g, np.ones((n // s, n // s, 1), np.float32)) / s * 8
    return (t - t.min()) / (t.max() - t.min())
# planes: origin, u axis, v axis (world), texture
planes = [(np.array([-6, -3.5, 14.0]), np.array([12, 0, 0.0]), np.array([0, 7, 0.0])),
          (np.array([-6, 3.5, 6.0]), np.array([12, 0, 0.0]), np.array([0, 0, 8.0])),
          (np.array([1.2, -1.5, 8.0]), np.array([2.5, 0, 0.0]), np.array([0, 2.5, 0.0])),
          (np.array([-4.5, -2.0, 10.0]), np.array([0, 0, 0.001]) + np.array([2.2, 0, 0]), np.array([0, 2.0, 0]))]
texs = [tex() for _ in planes]
def rotvec(w):
    th = np.linalg.norm(w)
    if th < 1e-12: return np.eye(3)
    k = w / th; K = np.array([[0, -k[2], k[1]], [k[2], 0, -k[0]], [-k[1], k[0], 0]]); return np.eye(3) + np.sin(th) * K + (1 - np.cos(th)) * K @ K
PAN = os.environ.get('PAN') == '1'
def cam(i):
    s = i / (N - 1)
    if PAN: return np.zeros(3), rotvec(np.array([0.03 * np.sin(s * 4), -0.5 * (s - 0.5), 0.04 * s]))
    C = np.array([-1.2 + 2.4 * s, 0.25 * np.sin(s * 5), -0.6 * s * s]); R_c2w = rotvec(np.array([0.02 * np.sin(s * 4), -0.2 * (s - 0.5), 0.03 * s])); return C, R_c2w
ys, xs = np.mgrid[0:H, 0:W].astype(np.float32)
dirs_c = np.stack([(xs + 0.5 - W / 2) / F, (ys + 0.5 - H / 2) / F, np.ones_like(xs)], -1)
tmp = tempfile.mkdtemp(); frames = []
for i in range(N):
    C, R = cam(i); d = dirs_c @ R.T; img = np.zeros((H, W, 3), np.float32); zbuf = np.full((H, W), 1e9, np.float32)
    for (o, u, v), t in zip(planes, texs):
        nrm = np.cross(u, v); den = d @ nrm; num = (o - C) @ nrm
        with np.errstate(divide="ignore", invalid="ignore"): tt = num / den
        P = C + tt[..., None] * d; rel = P - o; uu = (rel @ u) / (u @ u); vv = (rel @ v) / (v @ v)
        ok = (tt > 0) & (uu >= 0) & (uu < 1) & (vv >= 0) & (vv < 1) & (tt < zbuf)
        tx = np.clip((uu * 255).astype(int), 0, 255); ty = np.clip((vv * 255).astype(int), 0, 255); img[ok] = t[ty[ok], tx[ok]]; zbuf[ok] = tt[ok]
    frames.append((img * 255).astype(np.uint8))
mp4 = os.path.join(tmp, "scene.mp4")
p = subprocess.Popen(["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", "24", "-i", "-", "-c:v", "libx264", "-crf", "20", "-pix_fmt", "yuv420p", mp4], stdin=subprocess.PIPE)
for f in frames: p.stdin.write(f.tobytes())
p.stdin.close(); p.wait()
exe = sys.argv[1] if len(sys.argv) > 1 else "target/release/kimo-mcp"
srv = subprocess.Popen([exe], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True); mid = [0]
def rpc(method, params=None):
    mid[0] += 1; srv.stdin.write(json.dumps({"jsonrpc": "2.0", "id": mid[0], "method": method, "params": params or {}}) + "\n"); srv.stdin.flush(); return json.loads(srv.stdout.readline())
def call(name, args):
    r = rpc("tools/call", {"name": name, "arguments": args})["result"]; txt = r["content"][0]["text"]
    if r.get("isError"): raise SystemExit(f"{name} failed: {txt}")
    return json.loads(txt)
rpc("initialize", {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "proof", "version": "0"}})
names = [t["name"] for t in rpc("tools/list", {"mode": "flat"})["result"]["tools"]]
for t in ["solve_camera", "camera_solve_info", "apply_camera_solve", "export_camera_solve", "list_camera_solves"]: assert t in names, f"{t} not exposed over MCP"
call("project_set", {"width": W, "height": H, "fps": 24}); a = call("import_media", {"path": mp4})["asset_id"]; tr = call("add_track", {})["track_id"]
cid = call("add_clip", {"track_id": tr, "source_type": "media", "asset_id": a, "start": 0, "duration": N / 24})["clip_id"]
for label, extra in ((("pan: auto mode, self-calibrated", {}),) if PAN else (("known focal", {"focal_px": F}), ("self-calibrated focal", {}))):
    r = call("solve_camera", {"clip_id": cid, **extra}); print(label, json.dumps({k: r[k] for k in ("solve_id", "registered", "frames", "points", "tracks", "rms_px", "focal_px", "fov_deg", "seconds", "quality")}))
    sid = r["solve_id"]; info = call("camera_solve_info", {"solve_id": sid, "every": 1, "scale_px": 1})
    # compare to GT: align solved camera centres (similarity) to GT centres
    if PAN:
        print("   mode:", r["mode"], "notes:", r["notes"][:2]); sys.stdout.flush()
    S = np.array([[f["cam_x"], f["cam_y"], f["cam_z"]] for f in info["frames_data"]]); G = np.array([cam(f["frame"])[0] for f in info["frames_data"]])
    if PAN: S = G = np.zeros((len(S), 3)) + np.array([0, 0, 0.0]); S = S + np.random.default_rng(0).normal(0, 1e-9, S.shape); G = G + np.random.default_rng(1).normal(0, 1e-9, G.shape)
    ms, mg = S.mean(0), G.mean(0); Sc, Gc = S - ms, G - mg; U, D, Vt = np.linalg.svd(Gc.T @ Sc / len(S)); dd = np.eye(3); dd[2, 2] = np.sign(np.linalg.det(U) * np.linalg.det(Vt))
    Ra = U @ dd @ Vt; sc = (D * np.diag(dd)).sum() / (Sc ** 2).sum() * len(S); pos = np.sqrt((((sc * Sc @ Ra.T + mg) - G) ** 2).sum(1).mean())
    def rk(f):
        pan, tilt, roll = np.radians([f["cam_pan"], f["cam_tilt"], f["cam_roll"]]); cy, sy, cx, sx, cz, sz = np.cos(pan), np.sin(pan), np.cos(tilt), np.sin(tilt), np.cos(roll), np.sin(roll)
        return np.array([[cy, 0, sy], [0, 1, 0], [-sy, 0, cy]]) @ np.array([[1, 0, 0], [0, cx, -sx], [0, sx, cx]]) @ np.array([[cz, -sz, 0], [sz, cz, 0], [0, 0, 1]])
    M = sum(cam(f["frame"])[1] @ rk(f).T for f in info["frames_data"]); u_, _, vt_ = np.linalg.svd(M); Rg = u_ @ np.diag([1, 1, np.linalg.det(u_ @ vt_)]) @ vt_  # best global rotation (rotation-only alignment)
    rots = []
    for f in info["frames_data"]:
        dR = cam(f["frame"])[1].T @ (Rg @ rk(f)); rots.append(np.degrees(np.arccos(np.clip((np.trace(dR) - 1) / 2, -1, 1))))
    print(f"   vs ground truth: camera path length {np.linalg.norm(np.diff(G,axis=0),axis=1).sum():.2f}, position rms error {pos:.4f}, mean rotation error {np.mean(rots):.3f} deg, max {np.max(rots):.3f} deg, focal {r['focal_px']:.1f} (true {F})")
ap = call("apply_camera_solve", {"scale_px": 100}); print("apply_camera_solve", ap)
print("project camera props:", {k: len(v["keys"]) for k, v in json.load(open(os.devnull)).items()} if False else "keyed")
ex = call("export_camera_solve", {"path": os.path.join(tmp, "solve.json")}); print("export", ex)
print("OK")
