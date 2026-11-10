#!/usr/bin/env python3
"""Screen-shake offsets from the HUD's top-left corner bracket: integer-pixel corner (min x, min y of bright pixels in the corner window) per frame.
usage: bracket_track.py video t0 dur [thr=95] -> prints dx,dy relative to the median corner"""
import sys, numpy as np
sys.path.insert(0, __file__.rsplit('/',1)[0])
from marker_track import frames
def corner(video, t0, d, thr=95):
    fr = frames(video, t0, d); out = []
    for f in fr:
        w = f[0:100, 0:100] * 255; m = w > thr
        # bracket = long bright horizontal + vertical runs: take rows/cols with >=18 bright pixels
        rows = np.where(m.sum(1) >= 18)[0]; cols = np.where(m.sum(0) >= 18)[0]
        out.append((cols.min() if len(cols) else np.nan, rows.min() if len(rows) else np.nan))
    a = np.array(out, float); med = np.nanmedian(a, 0); return a - med, med
if __name__ == "__main__":
    v, t0, d = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]); thr = float(sys.argv[4]) if len(sys.argv) > 4 else 95
    a, med = corner(v, t0, d, thr); print("rest corner", med)
    for i, (x, y) in enumerate(a):
        if np.isnan(x) or abs(x) >= 2 or abs(y) >= 2: print(f"t={t0+i/25:.2f} dx={x:+.0f} dy={y:+.0f}")
