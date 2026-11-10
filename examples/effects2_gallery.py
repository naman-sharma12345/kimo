import subprocess, sys
from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=320, height=180, fps=30)
A = k.call("import_media", path="/tmp/src_b.mp4")["asset_id"]
names = [e["kind"] for e in k.call("list_effects")]
new = ["twirl","bulge","polar","kaleidoscope","turbulent_displace","motion_tile","spin_blur","directional_blur","tilt_shift","fractal_noise","gradient_ramp","lightning","lens_flare","anamorphic_streak","gradient_map","levels","emboss","find_edges","pixel_sort","vhs","film_damage","colorize","threshold","channel_mixer","checkerboard","grid","strobe","flip","offset","shadow_highlight","brightness_contrast","choker","feather_alpha","spill_suppress","outline","outer_glow","long_shadow"]
print(len(names), "effects")
extra = {"strobe": {"duty": 1.0}, "choker": {}, }
for i, n in enumerate(new):
    kk = Kimo() if False else k
    t = kk.call("add_track")["track_id"]
    c = kk.call("add_clip", track_id=t, source_type="media", asset_id=A, start=0, duration=1)["clip_id"]
    r = kk.call("add_effect", clip_id=c, kind=n, params=extra.get(n, {}))
    kk.call("render_frame", time=0.4, path=f"/tmp/fx2_{i:02d}.png")
    kk.call("delete_track", track_id=t)
print("done")
