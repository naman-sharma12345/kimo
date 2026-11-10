"""Beat-synced cuts: detect beats via MCP, cut on them, export with mixed audio."""
from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=640, height=360, fps=30)
src = "/tmp/beat.mp4"; a = k.call("import_media", path=src)["asset_id"]
beats = k.call("detect_beats", path=src, min_gap=0.3); print("bpm", round(beats["bpm"]), "beats", [round(b,2) for b in beats["beats"][:8]])
t = k.call("add_track")["track_id"]
c = k.call("add_clip", track_id=t, source_type="media", asset_id=a, start=0, duration=4)["clip_id"]
k.call("set_audio", clip_id=c, fade_in=0.3, fade_out=0.5, highpass=40, compress=True)
k.call("set_audio_volume_key", clip_id=c, time=0, value=0.4); k.call("set_audio_volume_key", clip_id=c, time=1.0, value=1.0)
fx = k.call("add_effect", clip_id=c, kind="exposure", params={"stops": 0})["effect_id"]
for b in beats["beats"]:
    if b < 4: k.call("set_keyframe", clip_id=c, effect_id=fx, property="stops", time=b, value=1.2, ease="out"); k.call("set_keyframe", clip_id=c, effect_id=fx, property="stops", time=b+0.2, value=0, ease="out")
print(k.call("export", path="/tmp/beat_out.mp4"))
