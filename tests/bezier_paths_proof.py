"""Bezier path shapes proof, MCP only. Run from repo root after cargo build --release."""
import sys, os, time; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples'))
from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=960, height=540, fps=25)
for n in ["add_path_shape","set_path","path_info"]: assert n in {t["name"] for t in k.tools()}, n
t0 = k.call("add_track")["track_id"]; k.call("add_clip", track_id=t0, source_type="solid", color=[0.1,0.1,0.14], start=0, duration=2)
def tr(): return k.call("add_track")["track_id"]
# heart (cubics) with fill
heart = "M0 60 C-120 -30 -60 -110 0 -50 C60 -110 120 -30 0 60 Z"
h = k.call("add_path_shape", track_id=tr(), start=0, duration=2, d=heart, fill=[0.95,0.25,0.35], stroke_width=0); print(h["length_px"], h["segments"], h["bbox"])
k.call("set_property", clip_id=h["clip_id"], property="x", value=-300) if "set_property" in {t["name"] for t in k.tools()} else None
# draw-on stroke with trim animated by arc length
st = k.call("add_path_shape", track_id=tr(), start=0, duration=2, points=[[-250,150],[-120,-80],[0,120],[130,-100],[260,140]], smooth=True, fill=[-1,0,0], stroke_width=8, stroke_color=[1,1,1])
e = k.call("add_shape_modifier", clip_id=st["clip_id"], kind="trim", params={"start":0,"end":0.0})["effect_id"]
k.call("set_keyframe", clip_id=st["clip_id"], effect_id=e, property="end", time=0, value=0); k.call("set_keyframe", clip_id=st["clip_id"], effect_id=e, property="end", time=1.5, value=1, ease="in_out")
k.call("add_shape_modifier", clip_id=st["clip_id"], kind="gradient_stroke")
# even-odd donut (two subpaths) + quadratic relative + offset
dn = k.call("add_path_shape", track_id=tr(), start=0, duration=2, d="M-60 0 a0 0 0 0 0 0 0", fill=[0.3,0.8,0.5]) if False else None
dn = k.call("add_path_shape", track_id=tr(), start=0, duration=2, d="M-80 -80 h160 v160 h-160 Z M-40 -40 h80 v80 h-80 Z", fill_rule="evenodd", fill=[0.3,0.8,0.5])
k.call("add_shape_modifier", clip_id=dn["clip_id"], kind="offset", params={"amount":-8})
# merge path with another path (subtract a triangle)
mg = k.call("add_path_shape", track_id=tr(), start=0, duration=2, d="M0 -90 Q100 0 0 90 Q-100 0 0 -90 Z", fill=[0.2,0.6,0.95])
k.call("add_shape_modifier", clip_id=mg["clip_id"], kind="merge", strings={"shape":"path","mode":"subtract","d":"M0 -20 L40 40 L-40 40 Z"})
for i,(cid,x,y) in enumerate([(h["clip_id"],-330,-120),(dn["clip_id"],330,-120),(mg["clip_id"],330,150)]):
    k.call("set_property", clip_id=cid, property="x", value=x); k.call("set_property", clip_id=cid, property="y", value=y)
k.call("set_property", clip_id=st["clip_id"], property="y", value=140); k.call("set_property", clip_id=st["clip_id"], property="x", value=-120)
for tm in (0.5, 1.0, 1.8):
    s=time.time(); k.call("render_frame", time=tm, path=f"/tmp/bz_{tm}.png"); print("render", tm, round(time.time()-s,2), "s")
print(k.call("path_info", clip_id=h["clip_id"])["length_px"])
k.call("set_path", clip_id=h["clip_id"], d="M0 -80 L70 60 L-70 60 Z"); assert k.call("path_info", clip_id=h["clip_id"])["segments"] == 3
for bad in [dict(d="M0 0 A10 10 0 0 1"), dict(d="L5 5"), dict(d="M0 0 L1"), dict(d="M0 0 L1e9 1"), dict(points=[[0,0]]), dict(d="M0 0 L1 1", points=[[0,0],[1,1]]), dict(d="M0 0 L9 9", fill_rule="x"), dict(d="M0 0 C1 1 2 2 3 3 Z M x")]:
    try: k.call("add_path_shape", track_id=tr(), start=0, duration=1, **bad); raise SystemExit(f"accepted {bad}")
    except RuntimeError as ex: print("ok err:", str(ex)[:100])
try: k.call("set_path", clip_id=t0 and mg["clip_id"].replace("clip","zz"), d="M0 0 L1 1"); raise SystemExit("bad clip")
except RuntimeError as ex: print("ok err:", str(ex)[:80])
print("PASS")
