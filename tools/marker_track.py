#!/usr/bin/env python3
"""Track a static HUD marker (template matched in a search window) per frame -> true screen-shake offsets, unaffected by moving content.
usage: marker_track.py video t0 dur tx ty tw th [sx sy sw sh]   template = frame at t0+dur (rest frame) crop (tx,ty,tw,th); search window default tx-60..tx+tw+60"""
import sys, subprocess, numpy as np
from scipy.signal import fftconvolve
def frames(v, t0, d, W=1280, H=720):
    p = subprocess.run(["ffmpeg","-v","error","-ss",str(t0),"-t",str(d),"-i",v,"-vf","fps=25","-f","rawvideo","-pix_fmt","gray","-"], capture_output=True).stdout
    return np.frombuffer(p, np.uint8).reshape(-1, H, W).astype(np.float32)/255
def track(v, t0, d, tx, ty, tw, th, rest_idx=-1, pad=60):
    fr = frames(v, t0, d); tpl = fr[rest_idx][ty:ty+th, tx:tx+tw]; tpl = tpl-tpl.mean(); n = np.sqrt((tpl**2).sum())+1e-9
    out = []
    for f in fr:
        y0, x0 = max(0, ty-pad), max(0, tx-pad); win = f[y0:ty+th+pad, x0:tx+tw+pad]
        num = fftconvolve(win, tpl[::-1, ::-1], mode="valid")
        ones = np.ones_like(tpl); s1 = fftconvolve(win, ones[::-1, ::-1], mode="valid"); s2 = fftconvolve(win**2, ones[::-1, ::-1], mode="valid")
        den = np.sqrt(np.maximum(s2 - s1**2/tpl.size, 1e-9))*n; c = num/den; iy, ix = np.unravel_index(c.argmax(), c.shape)
        def sub(a, b, cc): d = a-2*b+cc; return 0.5*(a-cc)/d if abs(d) > 1e-9 else 0.0
        fx = ix + (sub(c[iy, ix-1], c[iy, ix], c[iy, ix+1]) if 0 < ix < c.shape[1]-1 else 0); fy = iy + (sub(c[iy-1, ix], c[iy, ix], c[iy+1, ix]) if 0 < iy < c.shape[0]-1 else 0)
        out.append((x0+fx-tx, y0+fy-ty, float(c.max())))
    return np.array(out)
if __name__ == "__main__":
    v, t0, d = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]); tx, ty, tw, th = map(int, sys.argv[4:8])
    r = track(v, t0, d, tx, ty, tw, th)
    for i, (x, y, c) in enumerate(r):
        if abs(x) > 1.2 or abs(y) > 1.2: print(f"t={t0+i/25:.2f} dx={x:+.1f} dy={y:+.1f} ncc={c:.2f}")
