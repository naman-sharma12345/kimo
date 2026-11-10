"""Build a small graded sequence purely through MCP tool calls."""
import sys; from mcp_client import Kimo
k = Kimo(); print(len(k.tools()), "tools")
k.call("project_set", width=1280, height=720, fps=30)
a = k.call("import_media", path="/tmp/src_a.mp4")["asset_id"]
b = k.call("import_media", path="/tmp/src_b.mp4")["asset_id"]
t1, t2 = k.call("add_track")["track_id"], k.call("add_track")["track_id"]
c1 = k.call("add_clip", track_id=t1, source_type="media", asset_id=a, start=0, duration=3)["clip_id"]
c2 = k.call("add_clip", track_id=t2, source_type="media", asset_id=b, start=2.4, duration=3, speed=1.0)["clip_id"]
for c in (c1, c2):
    k.call("add_effect", clip_id=c, kind="exposure", params={"stops": 0.2})
    k.call("add_effect", clip_id=c, kind="curves", params={"amount": 0.8})
    k.call("add_effect", clip_id=c, kind="teal_orange", params={"amount": 0.9})
    k.call("add_effect", clip_id=c, kind="glow", params={"amount": 0.6, "radius": 14, "threshold": 0.6})
    k.call("add_effect", clip_id=c, kind="vignette", params={"amount": 0.6})
    k.call("add_effect", clip_id=c, kind="grain", params={"amount": 0.05})
fx = k.call("add_effect", clip_id=c2, kind="rgb_split", params={"amount": 0})["effect_id"]
k.call("set_keyframe", clip_id=c2, effect_id=fx, property="amount", time=0.0, value=24, ease="out")
k.call("set_keyframe", clip_id=c2, effect_id=fx, property="amount", time=0.6, value=0)
k.call("set_keyframe", clip_id=c1, property="scale", time=0, value=1.0, ease="in_out")
k.call("set_keyframe", clip_id=c1, property="scale", time=3, value=1.15)
k.call("add_transition", clip_id=c2, kind="iris", duration=0.6)
lb = k.call("add_effect", clip_id=c2, kind="letterbox", params={"ratio": 2.39})
k.call("save_project", path="/tmp/demo.kimo.json")
for t, n in [(1.0, "a"), (2.7, "mid"), (4.0, "b")]: k.call("render_frame", time=t, path=f"/tmp/demo_{n}.png")
print(k.call("export", path="/tmp/demo.mp4"))
