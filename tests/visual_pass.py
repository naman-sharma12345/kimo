#!/usr/bin/env python3
"""Visual pass (pass 2 of the two-tier check). Renders real frames for each feature and writes one labelled contact sheet per feature, so a person (or an
agent that can look at images) inspects the actual pixels, not only the headless asserts.
Usage: KIMO_BIN=<kimo-mcp> python3 tests/visual_pass.py [--out DIR] [feature ...]     (default out: a fresh temp dir, never the repo)
Lanes add a scene with @scene("name") returning [(label, png_path), ...]; the harness stacks them into DIR/<name>.png and writes DIR/manifest.json
(feature, tools used, frame paths, sha256 of the sheet). It only drives the MCP binary; it never builds anything and never writes into the repo."""
import base64, hashlib, json, os, subprocess, sys, tempfile
BIN = os.environ.get("KIMO_BIN", "target/release/kimo-mcp")
ARGS = [a for a in sys.argv[1:]]; OUT = None
if "--out" in ARGS: i = ARGS.index("--out"); OUT = ARGS[i + 1]; del ARGS[i:i + 2]
OUT = OUT or tempfile.mkdtemp(prefix="visual_pass_"); os.makedirs(OUT, exist_ok=True)
class S:
    def __init__(s):
        s.cwd = tempfile.mkdtemp(prefix="vp_cwd_"); s.p = subprocess.Popen([os.path.abspath(BIN)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True, cwd=s.cwd); s.i = 0; s.used = []; s.rpc("initialize", {})
    def rpc(s, m, pr): s.i += 1; s.p.stdin.write(json.dumps({"jsonrpc": "2.0", "id": s.i, "method": m, "params": pr}) + "\n"); s.p.stdin.flush(); return json.loads(s.p.stdout.readline())
    def j(s, n, a):
        s.used.append(n); r = s.rpc("tools/call", {"name": n, "arguments": a})["result"]
        t = next(c["text"] for c in r["content"] if c["type"] == "text")
        if r.get("isError"): raise RuntimeError(f"{n}: {t}")
        return json.loads(t)
    def frame(s, t, label, **kw):
        s.n = getattr(s, "n", 0) + 1; p = os.path.join(OUT, "frames", f"{s.cwd.split('_')[-1]}_{s.n}.png"); os.makedirs(os.path.dirname(p), exist_ok=True)
        r = s.j("render_frame", dict(time=t, path=p, inline="never", **kw)); assert os.path.exists(p), r; return (label, p)
    def close(s): s.p.stdin.close(); s.p.wait()
def precheck(path, ref=None, want=None):
    """Machine pre-checks before a human/agent looks: right size, not blank, not a single flat colour, differs from `ref` render when given."""
    w, h = [int(x) for x in subprocess.run(["ffprobe", "-v", "error", "-show_entries", "stream=width,height", "-of", "csv=p=0:s=x", path], capture_output=True, text=True).stdout.strip().split("x")]
    d = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], capture_output=True).stdout
    uniq = len(set(d[i:i + 3] for i in range(0, len(d), 3 * 7)))
    r = {"size": [w, h], "size_ok": want is None or [w, h] == list(want), "not_blank": uniq > 8, "unique_colours_sampled": uniq}
    if ref: r["differs_from_no_feature"] = open(ref, "rb").read() != open(path, "rb").read() and subprocess.run(["cmp", "-s", ref, path]).returncode != 0
    return r
SCENES = {}
def scene(name):
    def d(f): SCENES[name] = f; return f
    return d
def base(s, w=640, h=360, bg=True):
    s.j("project_set", {"width": w, "height": h, "fps": 25}); t = s.j("add_track", {})["track_id"]
    if bg: s.j("add_clip", {"track_id": t, "source_type": "gradient", "color": [0.95, 0.55, 0.15], "color2": [0.1, 0.25, 0.7], "start": 0, "duration": 6})
    return t
def title(s, text="Quarterly results", x=0, y=-60, size=54):
    t = s.j("add_track", {})["track_id"]; return s.j("add_text", {"track_id": t, "text": text, "start": 0, "duration": 6, "size": size, "x": x, "y": y})["clip_id"]
@scene("spotlight")
def _spot(s):
    base(s); title(s); out = [s.frame(1, "no spotlight")]
    s.j("spotlight", {"target_box": [200, 120, 240, 110], "dim_factor": 0.7, "duration": 6, "apply": True}); out.append(s.frame(1, "dim .7, hard edge, box 200,120,240x110"))
    s.j("undo", {}); s.j("spotlight", {"target_box": [200, 120, 240, 110], "dim_factor": 0.7, "feather_px": 24, "corner_radius": 30, "duration": 6, "apply": True}); out.append(s.frame(1, "feather 24, corner 30"))
    s.j("undo", {}); s.j("spotlight", {"target_box": [200, 120, 240, 110], "dim_factor": 0.9, "feather_px": 6, "duration": 6, "ease_in": 1, "apply": True}); out.append(s.frame(0.25, "dim .9 with ease_in 1s, t=.25"))
    return out
@scene("callout_card")
def _callout(s):
    base(s); out = []
    s.j("callout_card", {"text": "Revenue up 24%", "position": "bottom_center", "duration": 6, "apply": True}); out.append(s.frame(1, "bottom_center, defaults"))
    s.j("undo", {}); s.j("callout_card", {"text": "Revenue up 24%\\nvs last quarter", "badge": "NEW", "position": "top_left", "style": {"size": 30, "shadow": {"dy": 6, "blur": 10}}, "duration": 6, "apply": True}); out.append(s.frame(1, "2 lines + badge + shadow, top_left"))
    s.j("undo", {}); tt = title(s, "Launch", -150, 20, 60)
    s.j("callout_card", {"text": "Ships Friday", "badge": "TIP", "position": "near:" + tt, "offset": [0, 20], "style": {"fill": [0.1, 0.4, 0.2, 0.95], "corner_radius": 24}, "duration": 6, "apply": True}); out.append(s.frame(1, "near:title, offset [0,20], custom style"))
    s.j("undo", {}); s.j("callout_card", {"text": "Ships Friday", "badge": "TIP", "position": "near:" + tt, "duration": 6, "apply": True}); out.append(s.frame(1, "near:title, default placement + badge"))
    return out
@scene("lint_overlay")
def _lint(s):
    base(s); tt = title(s, "Too close to edge", 0, 0, 48); s.j("set_property", {"clip_id": tt, "property": "x", "value": 260}); out = [s.frame(1, "text pushed right, no overlay")]
    r = s.j("lint_frame", {"time": 1, "checks": ["safe_zone", "out_of_bounds"], "annotate": True}); f = r["frames"][0]; ann = (f.get("annotation") or {}).get("path")
    assert ann and os.path.exists(ann) and f["findings"], ("lint_frame annotate gave no overlay/findings", r["summary"])
    out.append(("lint_frame annotate:true: %s" % ",".join(x["code"] for x in f["findings"]), ann))
    h = f["findings"][0]["fix_hint"]; s.j(h["tool"], h["args"]); r2 = s.j("lint_frame", {"time": 1, "checks": ["safe_zone", "out_of_bounds"], "annotate": True})
    out.append(s.frame(1, "after applying fix_hint (%d findings)" % len(r2["frames"][0]["findings"]))); return out
def sheet(frames, path):
    inp = []; filt = []
    for i, (lab, p) in enumerate(frames):
        inp += ["-i", p]; lab = lab.replace(":", "\\:").replace("'", "").replace(",", "\\,")
        filt.append(f"[{i}:v]scale=480:270,pad=480:296:0:26:color=0x202020,drawtext=text='{lab}':x=6:y=5:fontsize=15:fontcolor=white[v{i}]")
    cols = 2 if len(frames) > 1 else 1; rows = (len(frames) + cols - 1) // cols
    layout = "|".join(f"{(i % cols) * 480}_{(i // cols) * 296}" for i in range(len(frames)))
    fc = ";".join(filt) + ";" + "".join(f"[v{i}]" for i in range(len(frames))) + (f"xstack=inputs={len(frames)}:layout={layout}" if len(frames) > 1 else "null") + "[o]"
    r = subprocess.run(["ffmpeg", "-y", "-v", "error"] + inp + ["-filter_complex", fc, "-map", "[o]", "-frames:v", "1", path], capture_output=True, text=True)
    if r.returncode: raise RuntimeError(r.stderr[-600:])
if __name__ == "__main__":
    names = ARGS or list(SCENES); man = []; bad = 0
    for n in names:
        s = S()
        try:
            fr = SCENES[n](s); sp = os.path.join(OUT, n + ".png"); sheet(fr, sp)
            pc = [dict(label=l, path=p, sha256=hashlib.sha256(open(p, "rb").read()).hexdigest(), **precheck(p, fr[0][1] if i else None)) for i, (l, p) in enumerate(fr)]
            ok = all(c["not_blank"] and c["size_ok"] and c.get("differs_from_no_feature", True) for c in pc)
            man.append({"feature": n, "sheet": sp, "sha256": hashlib.sha256(open(sp, "rb").read()).hexdigest(), "git_head": subprocess.run(["git", "rev-parse", "--short", "HEAD"], capture_output=True, text=True).stdout.strip(), "machine_prechecks_pass": ok, "frames": pc, "tools": sorted(set(s.used))}); print("SHEET", n, sp, "prechecks", "PASS" if ok else "FAIL"); bad += (not ok)
        except Exception as e: bad += 1; print("FAIL", n, e)
        finally: s.close()
    json.dump(man, open(os.path.join(OUT, "manifest.json"), "w"), indent=1); print("out:", OUT); sys.exit(1 if bad else 0)
