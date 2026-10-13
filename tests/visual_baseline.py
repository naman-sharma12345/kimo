#!/usr/bin/env python3
"""Baseline table (premium visual spec step 1) for callout, lint chips and spotlight: measured value of each numbered check vs the spec target, at 1920x1080,
plus a 1080p frame and a 4x crop per family. Drives a built kimo-mcp (KIMO_BIN); writes only under $TMPDIR/kimo-baseline-*, never into the repo.
Usage: KIMO_BIN=... python3 tests/visual_baseline.py [--out DIR] [callout|chips|spotlight ...]"""
import json, math, os, subprocess, sys, tempfile
import numpy as np
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import visual_pass as vp
W, H, U = 1920, 1080, 8  # u = H/135
args = [a for a in sys.argv[1:]]; OUT = None
if "--out" in args: i = args.index("--out"); OUT = args[i + 1]; del args[i:i + 2]
OUT = OUT or tempfile.mkdtemp(prefix="kimo-baseline-"); os.makedirs(OUT, exist_ok=True); vp.OUT = OUT
def px(path):
    d = subprocess.run(["ffmpeg", "-v", "error", "-i", path, "-f", "rawvideo", "-pix_fmt", "rgb24", "-"], capture_output=True).stdout
    return np.frombuffer(d, np.uint8).reshape(H, W, 3).astype(np.float64) / 255.0
def lin(c): return np.where(c <= 0.04045, c / 12.92, ((c + 0.055) / 1.055) ** 2.4)
def luma(a): l = lin(a); return 0.2126 * l[..., 0] + 0.7152 * l[..., 1] + 0.0722 * l[..., 2]
def cr(a, b): a, b = max(a, b), min(a, b); return (a + 0.05) / (b + 0.05)
def crop4(src, region, name):
    x, y, w, h = region; dst = os.path.join(OUT, name)
    subprocess.run(["ffmpeg", "-y", "-v", "error", "-i", src, "-vf", f"crop={w}:{h}:{x}:{y},scale={w*4}:{h*4}:flags=neighbor", dst], check=True); return dst
def scene(s):
    vp.base(s, W, H, True); t = s.j("add_track", {})["track_id"]
    return s.j("add_text", {"track_id": t, "text": "Quarterly results", "start": 0, "duration": 6, "size": 120, "y": -250})["clip_id"]
rows = []; files = []
def row(fam, check, measured, target, ok=None): rows.append((fam, check, measured, target, "-" if ok is None else ("meets" if ok else "misses")))
def callout():
    s = vp.S(); scene(s); bare = s.frame(1, "bare")[1]
    a = {"text": "Revenue up 24%", "badge": "NEW", "position": "bottom_center", "duration": 6, "apply": True}
    try: s.j("callout_card", a); row("callout", "defaults at 1080p (margin 48, title_safe 5% = 54 px)", "OK", "unit inside safe", True); s.j("undo", {})
    except RuntimeError as e: row("callout", "defaults at 1080p (margin 48, title_safe 5% = 54 px)", "callout_unfit: " + str(e).split("overflow_px")[1][:60], "unit inside safe, >= 4u inset", False)
    r = s.j("callout_card", dict(a, style={"margin": 64}))
    p = r["params"]; st = p["style"]; card, ink, badge = p["card_rect"], p["text_rect"], p["badge_rect"]
    f = s.frame(1, "callout")[1]; files.append(f)
    A, B = px(bare), px(f)
    row("callout", "body size px", st["size"], "36", st["size"] == 36)
    row("callout", "badge label px", st["badge_size"], "24", st["badge_size"] == 24)
    L, T, R, Bt = ink[0] - card[0], ink[1] - card[1], card[0] + card[2] - ink[0] - ink[2], card[1] + card[3] - ink[1] - ink[3]
    row("callout", "padding L/T/R/B px (from ink rect)", f"{L:.0f}/{T:.0f}/{R:.0f}/{Bt:.0f}", "24/16/24/16", (round(L), round(T), round(R), round(Bt)) == (24, 16, 24, 16))
    # corner radius: a per-row edge fit on a shadow:false render of the same card (the shadow gradient contaminates the corner pixels: the same card fits 10.0 with the
    # shadow on and 11.7 with it off; the method itself fits plain shapes of radius 6/12/20 as 5.7/11.6/19.6, a bias of about -0.4 px). Block rows k from the
    # bottom-left corner (the badge sits over the top-left): e_k = sum(1 - coverage) is the circle's x offset, r minimises sum_k (e_k - (r - sqrt(r^2 - (r-k-0.5)^2)))^2.
    s2 = vp.S(); scene(s2); A2 = px(s2.frame(1, "bare2")[1]); r2 = s2.j("callout_card", dict(a, style={"margin": 64, "shadow": False})); B2 = px(s2.frame(1, "noshadow")[1]); c2 = r2["params"]["card_rect"]; s2.close()
    L0, T0, Hh = int(c2[0]), int(c2[1]), int(round(c2[3])); N = 24; ks = np.arange(N)
    cov = (luma(B2) - luma(A2)) / (np.median(luma(B2)[T0 + Hh - 14:T0 + Hh - 8, L0 + 40:L0 + 90]) - luma(A2) + 1e-9)
    e = np.array([float((1 - np.clip(cov[T0 + Hh - 1 - k, L0:L0 + N], 0, 1)).sum()) for k in ks]); rs = np.arange(2.0, 30.0, 0.05)
    def model(r): d = np.clip(r - ks - 0.5, -r, r); return np.where(ks + 0.5 < r, r - np.sqrt(np.maximum(r * r - d * d, 0)), 0.0)
    sse = [float(((e - model(r)) ** 2).sum()) for r in rs]; r_fit = float(rs[int(np.argmin(sse))]) + 0.4
    row("callout", "corner radius px (set / per-row circle fit, shadow off, +0.4 method bias)", f"{st['corner_radius']:.0f} set / {r_fit:.1f} fit (rms {math.sqrt(min(sse)/N):.2f} px)", "12 (fit within 1 px)", abs(r_fit - 12) <= 1.0)
    below = int(card[1] + card[3] + 3 * U); band = lambda a: luma(a[below:below + 4, int(card[0] + 20):int(card[0] + card[2] - 20)]).mean()
    drop = 1 - band(B) / max(band(A), 1e-9)
    row("callout", "shadow: luminance drop 3u below card", f"{drop*100:.1f}% (shadow default: {st['shadow']})", "6-18% via 2 layers", 0.06 <= drop <= 0.18)
    gap = card[1] - (badge[1] + badge[3]); bh = badge[3]
    row("callout", "badge height / gap / h-pad px", f"{bh:.0f} / {gap:.0f} / {st['badge_padding']:.0f}", "28 / 8 / 12", (round(bh), round(gap), round(st['badge_padding'])) == (28, 8, 12))
    # p10 text contrast: text pixels (bright) vs the card fill actually rendered
    x0, y0_, x1, y1 = [int(v) for v in (ink[0], ink[1], ink[0] + ink[2], ink[1] + ink[3])]
    reg = B[y0_:y1, x0:x1]; L_ = luma(reg).ravel(); fill = np.median(luma(B[int(card[1]) + 3:int(card[1]) + 8, int(card[0]) + 30:int(card[0]) + 80]))
    txt = np.sort(L_)[int(len(L_) * 0.995):].mean()
    ratios = [cr(v, fill) for v in L_ if v > 0.5 * txt]; p10 = float(np.percentile(ratios, 10)) if ratios else float("nan")
    row("callout", "p10 text contrast", f"{p10:.1f}:1", ">= 7:1", p10 >= 7)
    row("callout", "unit inset from safe edge px", f"{H - (card[1] + card[3]):.0f} (margin {st['margin']:.0f})", ">= 32 (4u)", st["margin"] >= 32)
    files.append(crop4(f, [int(card[0]) - 40, int(badge[1]) - 20, int(card[2]) + 80, int(card[1] + card[3] - badge[1]) + 60], "callout_4x.png"))
    s.close()
def chips():
    s = vp.S(); t = scene(s); s.j("set_property", {"clip_id": t, "property": "x", "value": 760})
    r = s.j("lint_frame", {"time": 1, "checks": ["safe_zone"], "annotate": True}); fr = r["frames"][0]; a = fr["annotation"]
    f = a["path"]; files.append(f); B = px(f)
    c = a["chips"][0]["rect"]; box = a["boxes"][0]
    u = min(W, H) / 135; sev = box["severity"]
    # measure the chip from pixels: fill colour at its left-middle, text rows = pixels whose luma differs from the fill by > 0.35
    reg = B[c[1]:c[1] + c[3], c[0]:c[0] + c[2]]; fillc = reg[c[3] // 2, 3]; dif = np.abs(luma(reg) - luma(fillc[None, None, :])[0, 0]) > 0.35
    rows_ = np.where(dif[:, 6:-6].any(1))[0]; cols_ = np.where(dif[3:-3, :].any(0))[0]; cols_ = cols_[(cols_ > 5) & (cols_ < c[2] - 5)]
    ink_h = rows_.max() - rows_.min() + 1 if len(rows_) else 0
    row("chips", "label px (ink rows incl. descender; cap-height ~0.73 of size)", f"ink {ink_h} px high in a {c[3]} px chip", "20 px label (ink ~15-19)", 14 <= ink_h <= 21)
    padx = cols_.min() if len(cols_) else -1; pady = rows_.min() if len(rows_) else -1
    row("chips", "chip padding px (left of ink / top of ink)", f"{padx} / {pady} (chip {c[2]}x{c[3]})", "8 / 5 (plus glyph side bearing)", 7 <= padx <= 11 and 4 <= pady <= 9)
    row("chips", "stroke px", a["stroke_px"], "max(2, 0.25u=2) = 2", a["stroke_px"] == 2)
    sc = (int(round(v * 255)) for v in box["colour"]); hexs = "#%02X%02X%02X" % tuple(sc)
    row("chips", "stroke colour (kept 3:1-safe by ruling)", hexs, "ruling: keep accessible stroke", True)
    fh = "#%02X%02X%02X" % tuple(int(round(v * 255)) for v in fillc)
    row("chips", f"chip fill colour ({sev}), over bg", fh, "#F5A524 at .92 alpha over bg", None)
    fl = float(luma(fillc[None, None, :])[0, 0]); tl = luma(reg).reshape(-1); ext = float(tl[np.argmax(np.abs(tl - fl))])
    lo, hi = min(ext, fl), max(ext, fl)
    row("chips", "chip text contrast (text extreme vs fill)", f"{cr(hi, lo):.1f}:1", ">= 4.5:1", cr(hi, lo) >= 4.5)
    rr = box["rect"]; hit = c[0] < rr[0] + rr[2] and rr[0] < c[0] + c[2] and c[1] < rr[1] + rr[3] and rr[1] < c[1] + c[3]
    row("chips", "chip/rect overlaps", int(hit), "0", not hit)
    files.append(crop4(f, [max(0, c[0] - 40), max(0, c[1] - 40), 560, 240], "chips_4x.png"))
    s.close()
def spotlight():
    s = vp.S(); scene(s); bare = s.frame(1, "bare")[1]
    box = [440, 330, 1040, 360]  # wraps the whole headline with margin
    r = s.j("spotlight", {"target_box": box, "duration": 6, "apply": True}); p = r["params"]
    f = s.frame(1, "default")[1]; files.append(f); A, B = px(bare), px(f)
    row("spotlight", "defaults echoed", f"dim {p.get('dim_factor')}, feather {p.get('feather_px')}, corner {p.get('corner_radius')}", "dim .55-.70, feather 64.8 (0.06*min), corner >= 0.5*feather", False)
    inside = np.abs(B[box[1] + 20:box[1] + box[3] - 20, box[0] + 20:box[0] + box[2] - 20] - A[box[1] + 20:box[1] + box[3] - 20, box[0] + 20:box[0] + box[2] - 20]).max() * 255
    row("spotlight", "interior vs undimmed (max LSB)", f"{inside:.1f}", "<= 1", inside <= 1)
    s.j("undo", {}); s.j("spotlight", {"target_box": box, "dim_factor": 0.65, "feather_px": 64, "corner_radius": 32, "duration": 6, "apply": True})
    f2 = s.frame(1, "feather 64, corner 32")[1]; files.append(f2); C = px(f2)
    tint = np.array([0.02, 0.03, 0.09]); op = 0.65
    k = np.argmax(A, axis=-1)[..., None]; Ak = np.take_along_axis(A, k, -1)[..., 0]; Ck = np.take_along_axis(C, k, -1)[..., 0]; tk = tint[k[..., 0]]
    m = np.clip((Ak - Ck) / (op * np.maximum(Ak - tk, 1e-3)), 0, 1)  # dim coverage 0 inside, 1 far outside (needs bg well above the tint)
    row("spotlight", "interior vs undimmed (max LSB, whole hole)", f"{np.abs(C[box[1]:box[1]+box[3], box[0]:box[0]+box[2]] - A[box[1]:box[1]+box[3], box[0]:box[0]+box[2]]).max()*255:.2f}", "<= 1", np.abs(C[box[1]:box[1]+box[3], box[0]:box[0]+box[2]] - A[box[1]:box[1]+box[3], box[0]:box[0]+box[2]]).max()*255 <= 1.001)
    def ramp(line):
        v = np.clip(line, 0, 1); lo, hi = np.where(v > 0.1)[0], np.where(v > 0.9)[0]
        return float("nan") if not len(lo) or not len(hi) else float(abs(hi[0] - lo[0]) if v[0] < v[-1] else abs(lo[-1] - hi[-1]))
    cy, cx = box[1] + box[3] // 2, box[0] + box[2] // 2; band = 120
    L_ = ramp(m[cy, box[0] - band:box[0] + 2]); Rr = ramp(m[cy, box[0] + box[2] - 2:box[0] + box[2] + band][::-1][::-1]); Tt = ramp(m[box[1] - band:box[1] + 2, cx]); Bb = ramp(m[box[1] + box[3] - 2:box[1] + box[3] + band, cx])
    row("spotlight", "feather ramp 10-90% px (L/R/T/B), param 64", f"{L_:.0f}/{Rr:.0f}/{Tt:.0f}/{Bb:.0f}", "64 +-1 each", all(abs(v - 64) <= 1 for v in (L_, Rr, Tt, Bb)))
    # corner: diagonal from the corner centre-of-curvature outward vs straight ramp
    r0 = 32; ccx, ccy = box[0] + r0, box[1] + r0; diag = np.array([m[int(ccy - r0 / 1.4142 - t / 1.4142), int(ccx - r0 / 1.4142 - t / 1.4142)] for t in range(0, band)])
    dc = ramp(diag[::-1][::-1]) if False else None
    dd = np.where(diag > 0.1)[0]; de = np.where(diag > 0.9)[0]; cw_ = float(de[0] - dd[0]) if len(dd) and len(de) else float("nan")
    row("spotlight", "corner ramp 10-90 px (diagonal) vs straight, symmetry", f"{cw_:.0f} vs {np.nanmean([L_, Rr, Tt, Bb]):.0f} ({abs(cw_ - np.nanmean([L_, Rr, Tt, Bb])) / np.nanmean([L_, Rr, Tt, Bb]) * 100:.0f}% off)", "within 10%", abs(cw_ - np.nanmean([L_, Rr, Tt, Bb])) / np.nanmean([L_, Rr, Tt, Bb]) <= 0.10)
    prof = m[cy, box[0] - band:box[0]]; d = np.abs(np.diff(prof)).max()
    row("spotlight", "max adjacent step in ramp (% of full range)", f"{d*100:.2f}%", "<= 2.5%", d <= 0.025)
    out = B[60:140, 100:300]; o2 = A[60:140, 100:300]; rr = (out / np.maximum(o2, 1e-6)).reshape(-1, 3).mean(0)
    row("spotlight", "dim tint (rgb ratio of dimmed/undimmed)", "%.3f/%.3f/%.3f" % tuple(rr), "10% blue/warm shift, luma kept", abs(rr[0] - rr[2]) > 0.05)
    files.append(crop4(f2, [box[0] - 120, box[1] - 120, 320, 240], "spotlight_4x.png"))
    s.close()
FAM = {"callout": callout, "chips": chips, "spotlight": spotlight}
for n in (args or list(FAM)):
    try: FAM[n]()
    except Exception as e: rows.append((n, "ERROR", repr(e)[:200], "-", "-"))
md = "| family | check | measured (this build) | target | vs spec |\n|---|---|---|---|---|\n" + "\n".join("| %s | %s | %s | %s | %s |" % r for r in rows)
open(os.path.join(OUT, "baseline.md"), "w").write(md + "\n"); print(md); print("\nfiles:"); [print(" ", f) for f in files]; print("out:", OUT)
