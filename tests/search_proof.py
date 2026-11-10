"""kimo_search ranking checks: each query must put the expected action (or effect/transition kind) in the top results."""
import sys, os
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "examples"))
from mcp_client import Kimo
k = Kimo()
CASES = [("add text title", "add_text", 3), ("add text title", "add_title", 3), ("slow motion ramp", "set_speed_ramp", 3), ("stabilize shaky footage", "stabilize", 2),
 ("import video file", "import_media", 1), ("export mp4", "export", 2), ("pin text to moving surface", "track_planar", 4), ("glass shatter", "add_shard_burst", 3),
 ("cut clip in two", "split_clip", 3), ("fade audio", "set_audio", 3), ("3d scene motion blur shutter", "add_scene3d", 3), ("normalize loudness", "normalize_loudness", 2),
 ("captions from speech", "add_captions", 4), ("track a point", "track_point", 2)]
FX = [("remove green screen", "chroma_key"), ("vignette", "vignette"), ("film grain", "grain"), ("whip pan transition", "whip_left"), ("teal orange grade", "teal_orange")]
bad = 0
for q, want, top in CASES:
    r = [m["action"] for m in k.call("kimo_search", query=q, limit=top)["matches"]]
    ok = want in r; bad += not ok; print("ok  " if ok else "MISS", q, "->", want, "| top:", r)
for q, want in FX:
    r = [m["kind"] for m in k.call("kimo_search", query=q)["effects_and_transitions"][:4]]
    ok = want in r; bad += not ok; print("ok  " if ok else "MISS", q, "->", want, "| top:", r)
print(f"{len(CASES)+len(FX)-bad}/{len(CASES)+len(FX)} pass"); sys.exit(1 if bad else 0)
