#!/usr/bin/env python3
"""Per-frame reference analysis: every frame gets diff-vs-prev (cut/motion energy), global shift (phase corr), mean RGB, luma, saturation,
edge density, bright/dark fractions, and a cut flag. Output CSV + JSON shot segmentation. Usage: ref_analyze.py video out_prefix"""
import sys, subprocess, numpy as np, json, csv
v, out = sys.argv[1], sys.argv[2]
W, H = 320, 180
p = subprocess.Popen(["ffmpeg","-v","error","-i",v,"-vf",f"scale={W}:{H}","-f","rawvideo","-pix_fmt","rgb24","-"], stdout=subprocess.PIPE)
fs = W*H*3; rows = []; prev = None; pg = None; n = 0
win = np.outer(np.hanning(H), np.hanning(W))
while True:
    b = p.stdout.read(fs)
    if len(b) < fs: break
    f = np.frombuffer(b, np.uint8).reshape(H, W, 3).astype(np.float32)/255
    g = f.mean(2); mx = f.max(2); mn = f.min(2); sat = np.where(mx > 0, (mx-mn)/np.maximum(mx,1e-6), 0)
    gx = np.abs(np.diff(g, axis=1)).mean(); gy = np.abs(np.diff(g, axis=0)).mean()
    r = dict(n=n, t=round(n/25, 3), r=f[...,0].mean(), g=f[...,1].mean(), b=f[...,2].mean(), luma=g.mean(), sat=sat.mean(), edge=gx+gy, bright=(g>0.85).mean(), dark=(g<0.1).mean(), diff=0.0, sx=0.0, sy=0.0)
    if prev is not None:
        r["diff"] = float(np.abs(g-prev).mean())
        A = np.fft.fft2(prev*win); B = np.fft.fft2(g*win); R = A*np.conj(B); R /= np.abs(R)+1e-9; c = np.fft.ifft2(R).real
        y, x = np.unravel_index(c.argmax(), c.shape); y = y-H if y > H//2 else y; x = x-W if x > W//2 else x
        r["sx"], r["sy"] = float(-x*4), float(-y*4)  # px at 1280 wide
    prev = g; rows.append(r); n += 1
d = np.array([r["diff"] for r in rows]); med = np.median(d)
for i, r in enumerate(rows):
    r["cut"] = int(i > 0 and r["diff"] > max(0.08, 6*med) and (i < 2 or r["diff"] > 2.5*rows[i-1]["diff"]))
for r in rows:
    for k in r:
        if isinstance(r[k], float): r[k] = round(r[k], 4)
with open(out+".csv","w") as fh:
    w = csv.DictWriter(fh, fieldnames=list(rows[0])); w.writeheader(); w.writerows(rows)
cuts = [r["n"] for r in rows if r["cut"]]
shots = []; s = 0
for c in cuts + [len(rows)]:
    if c > s: seg = rows[s:c]; shots.append(dict(start=s, end=c, t0=s/25, t1=c/25, mean_diff=round(float(np.mean([x["diff"] for x in seg])),4), luma=round(float(np.mean([x["luma"] for x in seg])),3), peak_shift=round(float(max(abs(x["sx"])+abs(x["sy"]) for x in seg)),1))); s = c
json.dump(dict(frames=len(rows), cuts=cuts, shots=shots), open(out+".json","w"), indent=1)
print(len(rows), "frames", len(cuts), "cuts")
