# set_text_style and set_text_weight must both honor line_mask (same field, same pixels); revealed frame must not be clipped.
import sys, json; sys.path.insert(0, 'examples')
from mcp_client import Kimo
import numpy as np; from PIL import Image
def run(mode):
    k = Kimo(); k.call("project_set", width=960, height=540, fps=24)
    bg = k.call("add_track")["track_id"]; tr = k.call("add_track")["track_id"]
    k.call("add_shape", track_id=bg, shape="rect", width=960, height=540, fill=[0.08, 0.08, 0.1, 1], start=0, duration=4)
    c = k.call("add_text", track_id=tr, text="\u00c1gyjpq \u00c9\u00c0\u00c7 \u00c5jg\nQu\u00efck Br\u00f8wn \u0192gjy \u00d5\u00d1", start=0, duration=4, font="Montserrat", size=64, color=[1, 1, 1], align="center")["clip_id"]
    if mode == "weight": k.call("set_text_weight", clip_id=c, line_mask=True)
    st = dict(box_width=900)
    if mode == "style": st["line_mask"] = True
    k.call("set_text_style", clip_id=c, **st)
    a = k.call("add_text_animator", clip_id=c, unit="line", smoothness=0.3, y=60, opacity=0)["animator_id"]
    k.call("set_animator_keyframe", clip_id=c, animator_id=a, prop="offset", time=0.5, value=0, ease="linear")
    k.call("set_animator_keyframe", clip_id=c, animator_id=a, prop="offset", time=2.5, value=2.2, ease="linear")
    out = {}
    for t, n in ((0.8, "early"), (1.2, "mid"), (3.2, "full")):
        p = f"/tmp/tlm_{mode}_{n}.png"; k.call("render_frame", time=t, path=p); out[n] = np.array(Image.open(p).convert("RGB")).astype(int)
    s = json.dumps(k.call("get_project")); out["flag"] = '"line_mask": true' in s
    return out
none, w, s = run("none"), run("weight"), run("style")
d = lambda x, y: int((abs(x - y) > 0).any(2).sum())
for n in ("early", "mid", "full"): assert d(s[n], w[n]) == 0, f"style vs weight differ at {n}"
assert d(s["early"], none["early"]) > 1000 and d(s["mid"], none["mid"]) > 1000, "mask has no effect"
assert d(s["full"], none["full"]) == 0, "revealed frame is clipped (ascender/descender/accent band bug)"
assert w["flag"] and s["flag"] and not none["flag"], "flag not stored identically"
print("text line_mask proof OK")
