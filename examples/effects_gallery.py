"""Render every effect in list_effects on one frame via MCP; used as a smoke test."""
import sys, subprocess; from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=320, height=180, fps=30)
a = k.call("import_media", path="/tmp/src_b.mp4")["asset_id"]
t = k.call("add_track")["track_id"]
skip = {"lut", "letterbox"}
names = []
for e in k.call("list_effects"):
    if e["kind"] in skip: continue
    t_ = k.call("add_track")["track_id"]  # isolate: one clip per effect, time-sliced
    c = k.call("add_clip", track_id=t_, source_type="media", asset_id=a, start=len(names), duration=1)["clip_id"]
    k.call("add_effect", clip_id=c, kind=e["kind"], params={}); names.append(e["kind"])
for i, n in enumerate(names): k.call("render_frame", time=i + 0.5, path=f"/tmp/fx_{i:02d}.png")
print(len(names), "effects rendered OK"); open("/tmp/fx_names.txt", "w").write("\n".join(names))
