"""The silhouette cache (follow colliders / emit_follow) is shared across tool calls. Proof that it never serves a stale plane:
render, edit the clip, render again; the second frame must differ from the first and be byte-identical to the same edit rendered by a
fresh server that never cached anything. Also reports the repeat-frame speed-up. Usage: python3 tests/silhouette_cache_proof.py [plate.mp4]
Writes only under /tmp."""
import sys, time, hashlib, os; sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "examples")); from mcp_client import Kimo
plate = sys.argv[1] if len(sys.argv) > 1 else None
def scene(c):
    c("project_set", width=640, height=360, fps=25); t1 = c("add_track")["track_id"]; t2 = c("add_track")["track_id"]; t3 = c("add_track")["track_id"]
    if plate: c("add_clip", track_id=t1, source_type="media", asset_id=c("import_media", path=plate)["asset_id"], start=0, duration=3, fit="cover")
    else: c("add_clip", track_id=t1, source_type="solid", color=[0.1, 0.12, 0.2, 1], start=0, duration=3)
    tx = c("add_text", track_id=t2, text="KIMO", start=0, duration=3, size=200, anchor="center", x=0.5)["clip_id"]
    c("add_emitter", track_id=t3, start=0, duration=3, config=dict(shape="line", x=0.5, y=0.0, width=0.9, rate=120, life=2, speed=60, angle=90, gravity=1300, sprite="spark", size0=5, size1=3, color0=[1, .8, .4], color1=[1, .3, .1], additive=True, seed=3, bounce=0.4, prewarm=True, colliders=[dict(kind="layer", clip_id=tx, follow=True)]))
    return tx
def h(p): return hashlib.md5(open(p, "rb").read()).hexdigest()
k = Kimo(); c = k.call; tx = scene(c)
s = time.time(); c("render_frame", time=1.5, path="/tmp/sc_a.png"); cold = time.time() - s
s = time.time(); c("render_frame", time=1.5, path="/tmp/sc_a2.png"); warm = time.time() - s
assert h("/tmp/sc_a.png") == h("/tmp/sc_a2.png"), "cached repeat frame differs"
c("set_keyframe", clip_id=tx, property="scale", time=1.5, value=1.7); c("render_frame", time=1.5, path="/tmp/sc_b.png")
assert h("/tmp/sc_b.png") != h("/tmp/sc_a.png"), "edit did not change the frame (stale cache)"
k2 = Kimo(); c2 = k2.call; tx2 = scene(c2); c2("set_keyframe", clip_id=tx2, property="scale", time=1.5, value=1.7); c2("render_frame", time=1.5, path="/tmp/sc_b_fresh.png")
assert h("/tmp/sc_b.png") == h("/tmp/sc_b_fresh.png"), "edit after a cached render differs from a fresh server"
print(f"cold {cold:.2f}s, repeat {warm:.2f}s; edit after cache == fresh server, edit changed the frame")
print("silhouette cache proof OK")
