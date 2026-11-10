"""20s showcase built entirely through MCP calls: beat-synced cuts, grade, glow, particles, text, transitions, audio."""
import sys; from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=1280, height=720, fps=30, name="showcase")
A = k.call("import_media", path="/tmp/s_grad.mp4")["asset_id"]; B = k.call("import_media", path="/tmp/s_mandel.mp4")["asset_id"]; M = "/tmp/music.m4a"
Mm = k.call("import_media", path=M)  # audio-only asset is fine for mixing
beats = k.call("detect_beats", path=M, min_gap=0.9)["beats"]; print("beats", len(beats), "bpm", k.call("detect_beats", path=M)["bpm"])
tv1, tv2, tfx, ttx, tau = [k.call("add_track")["track_id"] for _ in range(5)]
# cut points on every 4th beat (~2s)
cuts = [0.0] + [b for i, b in enumerate(beats) if i % 4 == 3 and b < 19][:8] + [20.0]
srcs = [A, B]; prev = None; vclips = []
for i in range(len(cuts) - 1):
    st, en = cuts[i], cuts[i + 1]; tr = tv1 if i % 2 == 0 else tv2
    c = k.call("add_clip", track_id=tr, source_type="media", asset_id=srcs[i % 2], start=st - (0.4 if i else 0), duration=en - st + (0.4 if i else 0), in_point=i * 0.7)["clip_id"]
    k.call("set_audio", clip_id=c, mute=True)
    k.call("add_effect", clip_id=c, kind="curves", params={"amount": 0.7}); k.call("add_effect", clip_id=c, kind="teal_orange", params={"amount": 0.8})
    k.call("add_effect", clip_id=c, kind="glow", params={"amount": 0.5, "radius": 12, "threshold": 0.65}); k.call("add_effect", clip_id=c, kind="vignette", params={"amount": 0.5})
    k.call("add_effect", clip_id=c, kind="grain", params={"amount": 0.04})
    k.call("set_keyframe", clip_id=c, property="scale", time=0, value=1.0, ease="out"); k.call("set_keyframe", clip_id=c, property="scale", time=en - st + 0.4, value=1.12)
    if i: k.call("add_transition", clip_id=c, kind=["iris", "wipe_left", "zoom", "diagonal", "crossfade", "spin"][i % 6], duration=0.4)
    sh = k.call("add_effect", clip_id=c, kind="rgb_split", params={"amount": 0})["effect_id"]
    k.call("set_keyframe", clip_id=c, effect_id=sh, property="amount", time=0.4 if i else 0, value=14, ease="out"); k.call("set_keyframe", clip_id=c, effect_id=sh, property="amount", time=0.8 if i else 0.4, value=0)
em = k.call("add_particles", track_id=tfx, kind="embers", start=0, duration=20, count=260, color=[1, 0.6, 0.25], size=3)["clip_id"]
k.call("add_effect", clip_id=em, kind="glow", params={"amount": 1.0, "radius": 10, "threshold": 0.2})
title = k.call("add_text", track_id=ttx, text="k i n o", start=0.3, duration=4.2, size=150, font="/usr/share/fonts/truetype/noto/NotoSans-Light.ttf", stroke_width=0, shadow=6, letter_spacing=10)["clip_id"]
k.call("set_keyframe", clip_id=title, property="reveal", time=0, value=0); k.call("set_keyframe", clip_id=title, property="reveal", time=1.4, value=1, ease="out")
k.call("set_keyframe", clip_id=title, property="opacity", time=3.4, value=1); k.call("set_keyframe", clip_id=title, property="opacity", time=4.2, value=0)
k.call("add_effect", clip_id=title, kind="glow", params={"amount": 0.9, "radius": 14, "threshold": 0.4})
sub = k.call("add_text", track_id=ttx, text="an editor an AI can drive", start=2.0, duration=3.0, size=34, font="/usr/share/fonts/truetype/noto/NotoSans-Light.ttf", letter_spacing=4)["clip_id"]
k.call("set_property", clip_id=sub, property="y", value=110); k.call("set_keyframe", clip_id=sub, property="opacity", time=0, value=0); k.call("set_keyframe", clip_id=sub, property="opacity", time=0.8, value=1); k.call("set_keyframe", clip_id=sub, property="opacity", time=3.0, value=0)
mus = k.call("add_clip", track_id=tau, source_type="media", asset_id=Mm["asset_id"], start=0, duration=20)["clip_id"]
k.call("set_audio", clip_id=mus, fade_in=0.5, fade_out=1.5, compress=True)
lb = k.call("add_clip", track_id=tfx, source_type="solid", color=[0, 0, 0], start=0, duration=0.01)["clip_id"]  # placeholder removed below
k.call("delete_clip", clip_id=lb)
k.call("save_project", path="/tmp/showcase.kimo.json")
for t, n in [(1.2, 1), (4.5, 2), (9.0, 3), (15.0, 4)]: k.call("render_frame", time=t, path=f"/tmp/sc_{n}.png")
print(k.call("export", path="/tmp/showcase.mp4", crf=20))
