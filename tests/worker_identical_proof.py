"""Export with 2 workers (budget 1090 MB) vs 1 worker (budget 700 MB) must be byte-identical (planner buffers=4 for this whitelist scene at 1080p; no low_memory
in either case). Writes only to /tmp. Usage (repo root): python3 tests/worker_identical_proof.py"""
import os, sys, hashlib, subprocess; sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
def run(mb, out):
    os.environ["KIMO_MEM_MB"] = str(mb); k = Kimo(); C = k.call; C("project_set", width=1920, height=1080, fps=24)
    t = C("add_track")["track_id"]; C("add_clip", track_id=t, source_type="solid", color=[.1,.2,.4], start=0, duration=2)
    t2 = C("add_track")["track_id"]; s = C("add_shape", track_id=t2, shape="rect", width=500, height=300, fill=[.9,.5,.1,1], start=0, duration=2)["clip_id"]
    C("set_keyframe", clip_id=s, property="x", time=0.0, value=-400.0); C("set_keyframe", clip_id=s, property="x", time=2.0, value=400.0)
    r = C("export", path=out); k.p.terminate(); return hashlib.sha256(open(out, "rb").read()).hexdigest()
a = run(1090, "/tmp/wk2.mp4"); b = run(700, "/tmp/wk1.mp4"); print("2 workers", a[:16], "1 worker", b[:16]); print("PASS" if a == b else "FAIL"); sys.exit(a != b)
