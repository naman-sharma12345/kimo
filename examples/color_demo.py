from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=640, height=360, fps=30)
A = k.call("import_media", path="/tmp/src_a.mp4")["asset_id"]
t = k.call("add_track")["track_id"]
c = k.call("add_clip", track_id=t, source_type="media", asset_id=A, start=0, duration=2)["clip_id"]
k.call("render_frame", time=0.5, path="/tmp/c0.png")
for sc in ["waveform","parade","vectorscope","histogram"]: k.call("render_scope", time=0.5, scope=sc, path=f"/tmp/sc_{sc}.png")
k.call("add_effect", clip_id=c, kind="color_space", params={"from": 2, "to": 3})  # to ACEScct and back
k.call("add_effect", clip_id=c, kind="color_space", params={"from": 3, "to": 2})
k.call("add_effect", clip_id=c, kind="qualifier", params={"hue_center": 0, "hue_width": 60, "saturation": 0.0, "hue_shift": 120, "matte_blur": 2})
k.call("render_frame", time=0.5, path="/tmp/c1.png")
k.call("add_effect", clip_id=c, kind="power_window", params={"cx": 0.3, "cy": 0.5, "w": 0.4, "h": 0.6, "feather": 0.5, "exposure": -1.5, "invert": 1})
k.call("add_effect", clip_id=c, kind="film_print", params={})
k.call("add_effect", clip_id=c, kind="halation", params={})
k.call("add_effect", clip_id=c, kind="hue_curves", params={"hvs0": 1.8, "hvs1": 1.8, "hvh4": 40})
k.call("render_frame", time=0.5, path="/tmp/c2.png")
print(k.call("pick_white_balance", clip_id=c, time=0.5, x=0.45, y=0.45, w=0.1, h=0.1))
k.call("render_frame", time=0.5, path="/tmp/c3.png")
