"""Text animation, mask reveal, LUT and HSL via MCP only."""
from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=960, height=540, fps=30)
a = k.call("import_media", path="/tmp/src_b.mp4")["asset_id"]
t1, t2 = k.call("add_track")["track_id"], k.call("add_track")["track_id"]
c = k.call("add_clip", track_id=t1, source_type="media", asset_id=a, start=0, duration=3)["clip_id"]
k.call("add_effect", clip_id=c, kind="lut", strings={"path": "/tmp/warm.cube"}, params={"intensity": 1})
k.call("add_effect", clip_id=c, kind="hsl", params={"blue_hue": 0.5, "sat": 0.1})
k.call("set_mask", clip_id=c, shape="ellipse", cx=0.5, cy=0.5, w=0.3, h=0.5, feather=0.05, invert=False)
k.call("set_mask_keyframe", clip_id=c, property="w", time=0, value=0.05, ease="out")
k.call("set_mask_keyframe", clip_id=c, property="w", time=2, value=1.4)
k.call("set_mask_keyframe", clip_id=c, property="h", time=0, value=0.05, ease="out")
k.call("set_mask_keyframe", clip_id=c, property="h", time=2, value=1.6)
tx = k.call("add_text", track_id=t2, text="k i n o", start=0.3, duration=2.7, size=110, stroke_width=2, stroke_color=[0,0,0], shadow=4)["clip_id"]
k.call("set_keyframe", clip_id=tx, property="reveal", time=0, value=0)
k.call("set_keyframe", clip_id=tx, property="reveal", time=1.2, value=1)
k.call("set_keyframe", clip_id=tx, property="scale", time=0, value=1.0)
k.call("set_keyframe", clip_id=tx, property="scale", time=2.7, value=1.12, ease="out")
k.call("add_effect", clip_id=tx, kind="glow", params={"amount": 0.8, "radius": 10, "threshold": 0.5})
for t, n in [(0.5, "1"), (1.2, "2"), (2.4, "3")]: k.call("render_frame", time=t, path=f"/tmp/tm_{n}.png")
