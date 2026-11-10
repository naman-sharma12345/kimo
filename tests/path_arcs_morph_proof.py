"""SVG arcs + path morph keyframes proof, MCP only. Run from repo root after cargo build --release."""
import sys, os, math; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples'))
from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=960, height=540, fps=25)
t0 = k.call("add_track")["track_id"]; k.call("add_clip", track_id=t0, source_type="solid", color=[0.1,0.1,0.14], start=0, duration=2)
def tr(): return k.call("add_track")["track_id"]
def mk(**kw): return k.call("add_path_shape", track_id=tr(), start=0, duration=2, **kw)
# 1. analytic: circle r=100 from two arcs: length ~ 2*pi*100
c = mk(d="M-100 0 A100 100 0 1 0 100 0 A100 100 0 1 0 -100 0 Z", fill=[0.9,0.4,0.3]); L = c["length_px"]; print("circle length", L); assert abs(L - 2*math.pi*100) < 2.0, L
bb = c["bbox"]; assert abs(bb["x0"]+100) < 0.5 and abs(bb["x1"]-100) < 0.5 and abs(bb["y0"]+100) < 0.5, bb
# 2. relative arc, glued flags ('a50 50 0 01100 0' = flags 0,1 then 100 0), semicircle length pi*50 + chord
s2 = mk(d="M0 0 a50 50 0 0 1 100 0 Z", fill=[0.3,0.8,0.5]); assert abs(s2["length_px"] - (math.pi*50 + 100)) < 1.5, s2["length_px"]
s3 = mk(d="M0 0 a50 50 0 01100 0 Z"); assert abs(s3["length_px"] - s2["length_px"]) < 1e-6, "glued flags"
# 3. radii too small get scaled up (spec): same semicircle
s4 = mk(d="M0 0 A10 10 0 0 1 100 0 Z"); assert abs(s4["length_px"] - s2["length_px"]) < 1.5, s4["length_px"]
# 4. rotated ellipse arc, large-arc vs small-arc choose different lengths
a_small = mk(d="M0 0 A80 40 30 0 1 90 20")["length_px"]; a_large = mk(d="M0 0 A80 40 30 1 1 90 20")["length_px"]; assert a_large > a_small * 1.5, (a_small, a_large)
# 5. exact-arc trim on the circle: stroke only
st = mk(d="M-100 0 A100 100 0 1 0 100 0 A100 100 0 1 0 -100 0 Z", fill=[-1,0,0], stroke_width=10, stroke_color=[1,1,1])
e = k.call("add_shape_modifier", clip_id=st["clip_id"], kind="trim", params={"start":0,"end":0.25})["effect_id"]
# 6. morph: rounded-ish blob A -> blob B (same structure), with ease, then trim still works
A = "M-80 0 C-80 -60 -40 -80 0 -80 C40 -80 80 -60 80 0 C80 60 40 80 0 80 C-40 80 -80 60 -80 0 Z"
B = "M-110 -20 C-100 -90 -20 -60 0 -110 C50 -60 120 -80 100 0 C90 70 30 40 0 60 C-50 100 -120 50 -110 -20 Z"
m = mk(d=A, fill=[0.3,0.6,0.95]); cid = m["clip_id"]
r = k.call("add_path_key", clip_id=cid, time=1.0, d=B, ease="in_out"); print(r); assert [x["time"] for x in r["keys"]] == [0.0, 1.0]
k.call("add_path_key", clip_id=cid, time=2.0, d=A)
assert k.call("path_info", clip_id=cid)["path_keys"] == 3
for (cl, x, y) in [(c["clip_id"], -330, -120), (s2["clip_id"], 150, -150), (st["clip_id"], 330, -120), (cid, 0, 130)]:
    k.call("set_property", clip_id=cl, property="x", value=x); k.call("set_property", clip_id=cl, property="y", value=y)
for tm in (0.0, 0.5, 1.0): k.call("render_frame", time=tm, path=f"/tmp/am_{tm}.png")
# frame difference proves the morph moves: compare renders at 0.0 vs 1.0
import hashlib; h = [hashlib.md5(open(f"/tmp/am_{t}.png","rb").read()).hexdigest() for t in (0.0,0.5,1.0)]; assert len(set(h)) == 3, "morph frames identical"
# 7. loud errors
for bad in [dict(clip_id=cid, time=0.5, d="M0 0 L10 10 Z"), dict(clip_id=cid, time=-1, d=A), dict(clip_id=cid, time=0.5, d=A, ease="bounce"), dict(clip_id=c["clip_id"].replace("clip","zz"), time=0.5, d=A)]:
    try: k.call("add_path_key", **bad); raise SystemExit(f"accepted {bad}")
    except RuntimeError as ex: print("ok err:", str(ex)[:110])
for bad in ["M0 0 A10 10 0 2 1 5 5", "M0 0 A10 10 0 0 1 5", "M0 0 A10 10 0 0 1 1e9 5"]:
    try: mk(d=bad); raise SystemExit(f"accepted {bad}")
    except RuntimeError as ex: print("ok err:", str(ex)[:90])
print(k.call("clear_path_keys", clip_id=cid)); assert k.call("path_info", clip_id=cid)["path_keys"] == 0
print("PASS")
