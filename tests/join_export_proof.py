"""join_export proof: picture-only ranges joined with ONE continuous audio stream have no seam gaps, unlike per-range AAC.
Writes only under /tmp. Usage (repo root): python3 tests/join_export_proof.py"""
import sys, os, subprocess, struct, json, math
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), '..', 'examples')); from mcp_client import Kimo
T = "/tmp/jx"; os.makedirs(T, exist_ok=True)
subprocess.run(["ffmpeg","-v","error","-y","-f","lavfi","-i","sine=f=440:d=4","-ar","48000","-ac","2",f"{T}/tone.wav"], check=True)
import sys as _s
SRC=_s.argv[1] if len(_s.argv)>1 else "sine"
if SRC=="music": subprocess.run(["ffmpeg","-v","error","-y","-f","lavfi","-i","aevalsrc=0.3*sin(2*PI*220*t)+0.2*sin(2*PI*277*t)+0.15*sin(2*PI*330*t)*(1+0.5*sin(2*PI*3*t)):d=4:s=48000","-ac","2",f"{T}/tone.wav"],check=True)
k = Kimo(); C = k.call; C("project_set", width=320, height=180, fps=24)
t = C("add_track")["track_id"]; C("add_clip", track_id=t, source_type="solid", color=[.2,.3,.5], start=0, duration=4)
ta = C("add_track")["track_id"]; a = C("import_media", path=f"{T}/tone.wav")["asset_id"]; C("add_clip", track_id=ta, source_type="media", asset_id=a, start=0, duration=4)
RANGES = [(0,1.5),(1.5,3),(3,4)]
for i,(s,e) in enumerate(RANGES):
    C("export", path=f"{T}/pic{i}.mp4", start=s, end=e, audio=False); C("export", path=f"{T}/aud{i}.mp4", start=s, end=e)
C("export", path=f"{T}/full.mp4")
r = C("join_export", parts=[f"{T}/pic{i}.mp4" for i in range(3)], path=f"{T}/joined.mp4"); print("join", r)
def pcm(path):
    o = subprocess.run(["ffmpeg","-v","error","-i",path,"-vn","-f","f32le","-ac","1","-ar","48000","-"],capture_output=True).stdout
    return struct.unpack("<%df" % (len(o)//4), o)
def probe(path, sel):
    o = subprocess.run(["ffprobe","-v","error","-select_streams",sel,"-show_entries","stream=duration,start_time","-of","json",path],capture_output=True,text=True).stdout
    return json.loads(o)["streams"][0]
# old way: concat of per-range audio-bearing files
open(f"{T}/l.txt","w").write("".join(f"file '{T}/aud{i}.mp4'\n" for i in range(3)))
subprocess.run(["ffmpeg","-v","error","-y","-f","concat","-safe","0","-i",f"{T}/l.txt","-c","copy",f"{T}/perrange.mp4"],check=True)
def worst_seam_dip(x, ref, seams):
    """Min over seams and +-5 ms offsets of 2.5 ms-window RMS of x relative to the same window of the single full export; 1.0 = same energy, no dip."""
    w = 120; rms = lambda y, i: math.sqrt(sum(v*v for v in y[i:i+w])/w)+1e-9
    return min(rms(x, int(s*48000)+d)/rms(ref, int(s*48000)+d) for s in seams for d in range(-240, 241, 24))
seams = [1.5, 3.0]
J, F, P = pcm(f"{T}/joined.mp4"), pcm(f"{T}/full.mp4"), pcm(f"{T}/perrange.mp4")
dj, dp = worst_seam_dip(J, F, seams), worst_seam_dip(P, F, seams)
print(f"seam energy ratio vs full export (min over +-5 ms; 1.0 = no dip): joined {dj:.3f}  per-range aac concat {dp:.3f}")
n = min(len(J), len(F)); diff = max(abs(J[i]-F[i]) for i in range(0, n, 7)); print(f"max |joined - single full export| = {diff:.5f}, lengths {len(J)} vs {len(F)}")
vd = float(probe(f"{T}/joined.mp4","v:0")["duration"]); ad = float(probe(f"{T}/joined.mp4","a:0")["duration"]); print(f"A/V duration diff {abs(vd-ad)*1000:.1f} ms (1 frame = 41.7 ms)")
# (c) export_range guarantee: a ranged export (start,end) keeps exact picture+audio span and equals the slice of the full export's audio
C("export", path=f"{T}/range.mp4", start=1.0, end=2.5)
R = pcm(f"{T}/range.mp4"); s0 = int(1.0*48000); seg = F[s0:s0+len(R)]
rd = max(abs(R[i]-seg[i]) for i in range(2048, min(len(R), len(seg)) - 2048, 7)); rv = float(probe(f"{T}/range.mp4","v:0")["duration"]); ra = float(probe(f"{T}/range.mp4","a:0")["duration"])
print(f"(c) range 1.0-2.5: video {rv:.3f} s, audio {ra:.3f} s, max |range - slice of full audio| = {rd:.4f} (interior, 2048 samples at each edge excluded: AAC priming/overlap)")
okc = abs(rv-1.5) < 1e-3 and abs(ra-rv) < 1/24 and rd < 0.01
ok = okc and dj > 0.97 and diff < 0.02 and abs(vd-ad) < 1/24 and dj > dp
print("PASS" if ok else "FAIL"); sys.exit(0 if ok else 1)
