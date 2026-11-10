"""VFX layer smoke test: shapes, adjustment layer, corner pin, luma key, ripple, glitch, time reverse."""
from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=640, height=360, fps=30)
A = k.call("import_media", path="/tmp/src_a.mp4")["asset_id"]; B = k.call("import_media", path="/tmp/src_b.mp4")["asset_id"]; S = k.call("import_media", path="/tmp/sticker.png")["asset_id"]
tr = [k.call("add_track")["track_id"] for _ in range(6)]
base = k.call("add_clip", track_id=tr[0], source_type="media", asset_id=B, start=0, duration=3)["clip_id"]
k.call("set_time_mode", clip_id=base, mode="reverse")
st = k.call("add_clip", track_id=tr[1], source_type="media", asset_id=S, start=0, duration=3)["clip_id"]
k.call("set_property", clip_id=st, property="scale", value=0.8)
k.call("set_corner_pin", clip_id=st, corners=[[40, 30], [-20, -10], [-60, 40], [30, -50]])
k.call("add_effect", clip_id=st, kind="drop_shadow", params={"dx": 10, "dy": 12, "blur": 10, "opacity": 0.7})
ring = k.call("add_shape", track_id=tr[2], shape="polygon", sides=6, width=160, height=160, fill=[0, 0, 0], stroke_width=6, stroke_color=[0.2, 1, 1], start=0, duration=3)["clip_id"]
k.call("add_effect", clip_id=ring, kind="glow", params={"amount": 1.4, "radius": 12, "threshold": 0.3})
k.call("set_keyframe", clip_id=ring, property="rotation", time=0, value=0); k.call("set_keyframe", clip_id=ring, property="rotation", time=3, value=180)
k.call("set_property", clip_id=ring, property="x", value=-190); k.call("set_property", clip_id=ring, property="y", value=-90)
bar = k.call("add_shape", track_id=tr[2], shape="rect", width=220, height=36, corner_radius=18, fill=[1, 0.3, 0.1], start=0, duration=3)["clip_id"]
k.call("set_property", clip_id=bar, property="y", value=130); k.call("add_effect", clip_id=bar, kind="glow", params={"amount": 0.8})
adj = k.call("add_adjustment_layer", track_id=tr[5], start=0, duration=3)["clip_id"]
k.call("add_effect", clip_id=adj, kind="chromatic_aberration", params={"amount": 5}); k.call("add_effect", clip_id=adj, kind="light_rays", params={"amount": 0.7, "cx": 0.8, "cy": 0.1})
k.call("add_effect", clip_id=adj, kind="teal_orange", params={"amount": 0.7}); k.call("add_effect", clip_id=adj, kind="vignette", params={"amount": 0.5})
gl = k.call("add_effect", clip_id=adj, kind="glitch_blocks", params={"amount": 0, "slices": 14})["effect_id"]
k.call("set_keyframe", clip_id=adj, effect_id=gl, property="amount", time=1.9, value=0, ease="hold"); k.call("set_keyframe", clip_id=adj, effect_id=gl, property="amount", time=2.0, value=40, ease="hold"); k.call("set_keyframe", clip_id=adj, effect_id=gl, property="amount", time=2.4, value=0)
rp = k.call("add_clip", track_id=tr[3], source_type="media", asset_id=A, start=1, duration=2)["clip_id"]
k.call("add_effect", clip_id=rp, kind="luma_key", params={"threshold": 0.35, "softness": 0.1}); k.call("add_effect", clip_id=rp, kind="ripple", params={"amplitude": 10})
k.call("set_property", clip_id=rp, property="scale", value=0.4); k.call("set_property", clip_id=rp, property="y", value=-100); k.call("set_property", clip_id=rp, property="x", value=200)
for i, t in enumerate([0.3, 1.2, 2.1, 2.7]): k.call("render_frame", time=t, path=f"/tmp/vfx_{i}.png")
print("ok")
