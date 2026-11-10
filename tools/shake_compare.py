#!/usr/bin/env python3
"""Sub-pixel global shift per frame at full res (phase correlation + parabolic peak) for two clips, so shake timing can be compared frame by frame.
usage: shake_compare.py ref.mp4 ref_start dur kimo.mp4 kimo_start  -> prints table of frame, t, ref(sx,sy), kimo(sx,sy) and best lag"""
import sys, subprocess, numpy as np
def shifts(v, ss, d, W=640, H=360):
    p = subprocess.run(["ffmpeg","-v","error","-ss",str(ss),"-t",str(d),"-i",v,"-vf",f"scale={W}:{H},fps=25","-f","rawvideo","-pix_fmt","gray","-"], capture_output=True).stdout
    fr = np.frombuffer(p, np.uint8).reshape(-1, H, W).astype(np.float32)/255
    win = np.outer(np.hanning(H), np.hanning(W)); out = [(0.0, 0.0)]
    for i in range(1, len(fr)):
        A = np.fft.fft2(fr[i-1]*win); B = np.fft.fft2(fr[i]*win); R = A*np.conj(B); R /= np.abs(R)+1e-9; c = np.fft.ifft2(R).real
        y, x = np.unravel_index(c.argmax(), c.shape)
        def sub(cm, c0, cp): d = cm - 2*c0 + cp; return 0.5*(cm-cp)/d if abs(d) > 1e-9 else 0.0
        fx = x + sub(c[y, (x-1) % W], c[y, x], c[y, (x+1) % W]); fy = y + sub(c[(y-1) % H, x], c[y, x], c[(y+1) % H, x])
        fx = fx-W if fx > W/2 else fx; fy = fy-H if fy > H/2 else fy
        out.append((-fx*2, -fy*2))  # px at 1280
    return np.array(out)
if __name__ == "__main__":
    rv, rs, d, kv, ks = sys.argv[1], float(sys.argv[2]), float(sys.argv[3]), sys.argv[4], float(sys.argv[5])
    a, b = shifts(rv, rs, d), shifts(kv, ks, d); n = min(len(a), len(b)); a, b = a[:n], b[:n]
    ma, mb = np.hypot(*a.T), np.hypot(*b.T)
    best = max(range(-6, 7), key=lambda L: np.corrcoef(ma[max(0, L):n+min(0, L)], mb[max(0, -L):n-max(0, L)])[0, 1] if n > 12 else 0)
    print("best lag (kimo frames later than ref):", best)
    th = 6.0
    print("ref hits :", [round(rs + i/25, 2) for i in range(n) if ma[i] > th])
    print("kimo hits:", [round(ks + i/25, 2) for i in range(n) if mb[i] > th])
