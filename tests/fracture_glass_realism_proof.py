"""fracture_glass realism harness (MCP only). usage: python3 tests/fracture_glass_realism_proof.py [tag] [key=value ...effect param overrides]
With PROJ set (or /tmp/plates/style02.json present) it loads that native project - the film agent's project with plate paths repointed,
needing the two mixkit plates. Otherwise it builds its own fixture project in /tmp: two lavfi plates (smptebars under testsrc2),
fracture_glass on the top clip with t0=8.3 so the default TIMES straddle the impact. Writes /tmp/fg_<tag>_*.png + a contact sheet."""
import sys, os, time, json; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples'))
from mcp_client import Kimo
from PIL import Image
tag = sys.argv[1] if len(sys.argv) > 1 else "gate"; ov = dict(a.split("=") for a in sys.argv[2:])
k = Kimo(); C = k.call
proj = os.environ.get("PROJ"); proj = proj if proj else ("/tmp/plates/style02.json" if os.path.exists("/tmp/plates/style02.json") else None)
if proj: C("load_project", path=proj)
else:
    import subprocess
    for n, src in (("a", "testsrc2=s=640x360:r=25:d=11"), ("b", "smptebars=s=640x360:r=25:d=11")):
        subprocess.run(["ffmpeg", "-v", "error", "-y", "-f", "lavfi", "-i", src, "-pix_fmt", "yuv420p", f"/tmp/fg_plate_{n}.mp4"], check=True)
    C("project_set", width=640, height=360, fps=25)
    tb = C("add_track")["track_id"]; C("add_clip", track_id=tb, source_type="media", asset_id=C("import_media", path="/tmp/fg_plate_b.mp4")["asset_id"], start=0, duration=11, fit="cover")
    tt = C("add_track")["track_id"]; cl = C("add_clip", track_id=tt, source_type="media", asset_id=C("import_media", path="/tmp/fg_plate_a.mp4")["asset_id"], start=0, duration=11, fit="cover")["clip_id"]
    C("add_effect", clip_id=cl, kind="fracture_glass", params={"t0": 8.3, "impact_x": 0.5, "impact_y": 0.5})
p = C("get_project"); fx = None
for t in p["tracks"]:
    for c in t["clips"]:
        for e in c["effects"]:
            if e["kind"] == "fracture_glass": fx = (c["id"], e["id"])
assert fx, "no fracture_glass effect in the project"
print("fx", fx)
for kx, v in ov.items(): C("set_effect_param", clip_id=fx[0], effect_id=fx[1], param=kx, value=float(v))
times = [float(x) for x in os.environ.get("TIMES", "8.34,8.42,8.52,8.66,8.84,9.1").split(",")]; ims = []
for i, x in enumerate(times):
    s = time.time(); pth = f"/tmp/fg_{tag}_{i}.png"; C("render_frame", time=x, path=pth); print("t", x, round(time.time() - s, 1), "s"); ims.append(Image.open(pth).convert("RGB"))
sheet = Image.new("RGB", (1920, 720)); [sheet.paste(im.resize((640, 360)), ((i % 3) * 640, (i // 3) * 360)) for i, im in enumerate(ims)]; sheet.save(f"/tmp/fg_{tag}_sheet.png")
print("fracture glass realism sheet OK", f"/tmp/fg_{tag}_sheet.png")
