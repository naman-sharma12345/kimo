"""Report-only render baseline (unit 8 step 0): median wall time of render_frame per scene over 5 runs, 640x360, writes only /tmp.
Usage: python3 tests/render_bench.py"""
import sys, time, statistics, os; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
def scene(name):
    k = Kimo(); C = k.call; C("project_set", width=640, height=360, fps=24)
    bg = C("add_track")["track_id"]; tr = C("add_track")["track_id"]
    C("add_shape", track_id=bg, shape="rect", width=640, height=360, fill=[0.2, 0.3, 0.5, 1], start=0, duration=4)
    if name == "text":
        c = C("add_text", track_id=tr, text="Kimo render baseline", start=0, duration=4, font="Montserrat", size=60, color=[1, 1, 1], align="center")["clip_id"]
    elif name == "blur":
        c = C("add_shape", track_id=tr, shape="ellipse", width=300, height=200, fill=[1, 0.5, 0.2, 1], start=0, duration=4)["clip_id"]; C("add_effect", clip_id=c, kind="blur", params={"radius": 20})
    elif name == "glass":
        c = C("add_shape", track_id=tr, shape="rect", width=300, height=200, fill=[1, 1, 1, 1], start=0, duration=4)["clip_id"]; C("add_effect", clip_id=c, kind="frosted_glass", params={"blur": 8, "slab_thickness": 80})
    elif name == "particles":
        C("add_particles", track_id=tr, kind="dust", start=0, duration=4, count=400, color=[1, 0.9, 0.8], size=2, seed=7)
    elif name == "shapes20":
        for i in range(20): C("add_shape", track_id=tr, shape="rect", width=80, height=60, fill=[1, 1, 1, 0.5], start=0, duration=4)
    return k
print("scene\tmedian_ms\tmin_ms\tmax_ms")
for n in ("empty", "text", "blur", "glass", "particles", "shapes20"):
    k = scene(n); ts = []
    for i in range(6):
        a = time.perf_counter(); k.call("render_frame", time=1.0, path=f"/tmp/rb_{n}.png"); ts.append((time.perf_counter() - a) * 1000)
    ts = ts[1:]; print(f"{n}\t{statistics.median(ts):.0f}\t{min(ts):.0f}\t{max(ts):.0f}")
