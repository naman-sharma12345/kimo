from mcp_client import Kimo
k = Kimo(); k.call("project_set", width=320, height=180, fps=30)
A = k.call("import_media", path="/tmp/src_a.mp4")["asset_id"]
t = k.call("add_track")["track_id"]
ids = [k.call("add_clip", track_id=t, source_type="media", asset_id=A, start=i*2, duration=2)["clip_id"] for i in range(3)]
def pts(): return k.call("list_edit_points")["points"]
print(pts())
k.call("ripple_trim", clip_id=ids[0], edge="tail", delta=-0.5); print("ripple_trim", pts())
k.call("roll_edit", left_clip_id=ids[0], right_clip_id=ids[1], delta=0.25); print("roll", pts())
k.call("slide_clip", clip_id=ids[1], delta=0.25); print("slide", pts())
k.call("ripple_delete", clip_id=ids[1]); print("ripple_delete", pts())
k.call("lift_range", track_id=t, start=1.0, end=2.0, extract=True); print("extract", pts())
k.call("add_marker", time=1.0, name="hit"); print(k.call("snap_time", time=1.05))
print(k.call("split_all_at", at=0.5))
k.call("undo"); print("undo", pts())
