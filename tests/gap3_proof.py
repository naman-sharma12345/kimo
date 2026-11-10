"""Gap #3 proof: typed modifiers, shape modifiers, layer styles, all via MCP only. Run from repo root after cargo build --release."""
import sys, os; sys.path.insert(0, os.path.join(os.path.dirname(__file__), '..', 'examples'))
from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=640, height=360, fps=25)
names = {t["name"] for t in k.tools()}
for n in ["add_modifier","list_modifiers","remove_modifier","add_shape_modifier","remove_shape_modifier","list_shape_modifiers","set_layer_style","list_layer_styles","remove_layer_style"]: assert n in names, n
t = k.call("add_track")["track_id"]
k.call("add_clip", track_id=t, source_type="solid", color=[0.12,0.12,0.16], start=0, duration=2)
def shape(**kw):
    tt = k.call("add_track")["track_id"]; r = k.call("add_shape", track_id=tt, start=0, duration=2, **kw); return r.get("clip_id") or r["clip_ids"][0]
# shape modifiers
a = shape(shape="ellipse", width=120, height=120, fill=[0.9,0.3,0.4], stroke_width=0)
k.call("add_shape_modifier", clip_id=a, kind="repeater", params={"copies":5,"dx":60,"dy":0,"scale":0.85})
k.call("set_keyframe", clip_id=a, property="x", time=0, value=-200); 
b = shape(shape="ellipse", width=200, height=200, fill=[-1,0,0], stroke_width=10, stroke_color=[1,1,1])
e = k.call("add_shape_modifier", clip_id=b, kind="trim", params={"start":0,"end":0.5})["effect_id"]
k.call("add_shape_modifier", clip_id=b, kind="gradient_stroke", params={"angle":0})
c = shape(shape="rect", width=200, height=140, fill=[0.2,0.7,0.9], stroke_width=0)
k.call("add_shape_modifier", clip_id=c, kind="merge", params={"w":120,"h":120,"dx":60,"dy":0}, strings={"shape":"ellipse","mode":"subtract"})
k.call("add_shape_modifier", clip_id=c, kind="offset", params={"amount":-6})
d = shape(shape="rect", width=160, height=100, fill=[0.95,0.8,0.2], stroke_width=0, corner_radius=20)
# layer styles
k.call("set_layer_style", clip_id=d, style="bevel", params={"depth":2.5,"size":6})
k.call("set_layer_style", clip_id=d, style="inner_shadow")
k.call("set_layer_style", clip_id=d, style="stroke", params={"width":4,"g":0.3}, strings={"position":"outside"})
for tm in (0.0, 1.0): k.call("render_frame", time=tm, path=f"/tmp/gap3_{tm}.png")
# errors are loud
for bad in [dict(clip_id=a, kind="nope"), dict(clip_id=a, kind="merge", strings={"mode":"xx"}), dict(clip_id=a, kind="repeater", params={"copies":999})]:
    try: k.call("add_shape_modifier", **bad); raise SystemExit(f"should have failed: {bad}")
    except RuntimeError as ex: print("ok err:", str(ex)[:90])
try: k.call("set_layer_style", clip_id=d, style="stroke", strings={"position":"left"}); raise SystemExit("bad position accepted")
except RuntimeError as ex: print("ok err:", str(ex)[:90])
# typed modifiers
k.call("set_keyframe", clip_id=d, property="y", time=0.2, value=-100); k.call("set_keyframe", clip_id=d, property="y", time=0.6, value=0)
k.call("add_modifier", clip_id=d, property="y", kind="bounce", amp=0.4); k.call("add_modifier", clip_id=d, property="y", kind="bounce", amp=0.5)
r = k.call("add_modifier", clip_id=d, property="x", kind="wiggle", freq=3, amp=20); assert r["modifiers"] == ["wiggle"]
r2 = k.call("add_modifier", clip_id=c, property="x", kind="wiggle", freq=3, amp=20)
m1 = k.call("list_modifiers", clip_id=d, property="x")["modifiers"][0]["op"]; m2 = k.call("list_modifiers", clip_id=c, property="x")["modifiers"][0]["op"]
assert m1["seed"] != m2["seed"], "default seeds must differ per layer"
assert len(k.call("list_modifiers", clip_id=d, property="y")["modifiers"]) == 1, "same-kind replaced"
k.call("add_modifier", clip_id=d, property="y", kind="overshoot", amp=0.3)
for tm in (0.5, 0.7, 1.2): k.call("render_frame", time=tm, path=f"/tmp/gap3_y{tm}.png")
for bad in [dict(kind="wiggle", freq=1e9), dict(kind="bounce", amp=50), dict(kind="loop", mode="spin")]:
    try: k.call("add_modifier", clip_id=d, property="y", **bad); raise SystemExit(f"should fail {bad}")
    except RuntimeError as ex: print("ok err:", str(ex)[:90])
print(k.call("remove_modifier", clip_id=d, property="y", kind="bounce"))
print("PASS")
