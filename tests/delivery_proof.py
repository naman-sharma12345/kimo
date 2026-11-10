#!/usr/bin/env python3
"""RV-7 proof over the real MCP binary (3 x 1080p videos): collect_project -> delete originals -> fresh process opens the copy with 0 missing and an
identical export; collect time/bytes and hashing throughput (cold vs mtime-cached) are printed."""
import argparse, hashlib, os, shutil, subprocess, sys, time, tempfile
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "examples"))
from mcp_client import Kimo
ap = argparse.ArgumentParser(); ap.add_argument("--binary", default="target/release/kimo-mcp"); ap.add_argument("--seconds", type=int, default=4); a = ap.parse_args()
root = Path(tempfile.mkdtemp(prefix="kimo_delivery_proof_")); src = root / "src media"; src.mkdir()
def md5(p): return subprocess.run(["ffmpeg", "-v", "error", "-i", str(p), "-map", "0:v", "-f", "framemd5", "-"], capture_output=True, text=True, check=True).stdout
def tree(d): return sorted((str(p.relative_to(d)), hashlib.sha256(p.read_bytes()).hexdigest()) for p in d.rglob("*") if p.is_file())
for i, pat in enumerate(["testsrc", "testsrc2", "smptebars"]):
    subprocess.run(["ffmpeg", "-v", "error", "-f", "lavfi", "-i", f"{pat}=size=1920x1080:rate=25:duration={a.seconds}", "-pix_fmt", "yuv420p", "-c:v", "libx264", "-preset", "ultrafast", str(src / f"v{i}.mp4")], check=True)
proj = root / "orig.kimo"
with Kimo(a.binary) as k:
    k.call("project_set", width=1920, height=1080, fps=25)
    for i in range(3):
        t = k.call("add_track")["track_id"]; asset = k.call("import_media", path=str(src / f"v{i}.mp4"))["asset_id"]
        k.call("add_clip", track_id=t, source_type="media", asset_id=asset, start=0, duration=a.seconds)
    k.call("save_project", path=str(proj)); orig_sha = hashlib.sha256(proj.read_bytes()).hexdigest()
    t0 = time.time(); l1 = k.call("list_dependencies", check_hash=True); cold = time.time() - t0
    t0 = time.time(); l2 = k.call("list_dependencies", check_hash=True); warm = time.time() - t0
    size = sum(i["size"] for i in l1["items"]); assert l1["summary"]["missing"] == 0 and l1 == l2
    print(f"hash: {size/1e6:.1f} MB cold {cold:.3f}s ({size/1e6/max(cold,1e-9):.0f} MB/s), mtime-cached {warm:.4f}s")
    d1 = root / "c1"; t0 = time.time(); r = k.call("collect_project", dest_dir=str(d1)); ct = time.time() - t0
    print(f"collect: {r['files']} files {r['bytes']/1e6:.1f} MB in {ct:.2f}s, missing={r['missing']}")
    d2 = root / "c2"; k.call("collect_project", dest_dir=str(d2)); assert tree(d1) == tree(d2), "collect not deterministic"
    ref = root / "ref.mp4"; k.call("export", path=str(ref))
assert hashlib.sha256(proj.read_bytes()).hexdigest() == orig_sha, "original project modified"
shutil.rmtree(src); env = dict(os.environ); env.pop("KIMO_FONTS", None)
with Kimo(a.binary) as k2:
    k2.call("load_project", path=str(d1 / "project.kimo")); l = k2.call("list_dependencies", check_hash=True); assert l["summary"]["missing"] == 0 and l["summary"]["changed"] == 0, l["summary"]
    out = root / "copy.mp4"; k2.call("export", path=str(out))
assert md5(ref) == md5(out), "collected export differs from the original"
print("PASS delivery_proof"); shutil.rmtree(root)
