# focus_series: pre-registered definitions and expectations

Written 2026-10-19T04:36:08Z, BEFORE the tool exists (no focus_series code is written, built or run at this time). Base: v6/eyes-wave2 = 2fd350c6.
Owner: Naman (WhatsApp m:c4ed8215 and m:90221be0, Oct 19 2026: "focus_series is approved as the one next feature").
Nothing below may be edited after the first focus_series run. A later change goes in a new dated file beside this one,
and every prediction here is scored as written (HIT / MISS), misses reported as misses.

## What the tool measures (fixed here, before code)

- Every sampled project frame (every `step`-th frame, default 1, in `start`..`end` of `source` = timeline or clip:<id>) is
  rendered at full resolution through the engine (same render path as black_frames / frozen_frames).
- Luma: Rec.709 Y' = 0.2126 R' + 0.7152 G' + 0.0722 B' of the full-range output codes, on the 0..255 code scale.
- `lap_var`: population variance of the 4-neighbour Laplacian L = Y'(x-1,y) + Y'(x+1,y) + Y'(x,y-1) + Y'(x,y+1) - 4 Y'(x,y),
  over interior pixels only (the 1-pixel frame border has no full neighbourhood and is skipped). Units: (code units)^2.
- `tenengrad`: mean Sobel gradient magnitude sqrt(Gx^2 + Gy^2) (3x3 Sobel, weights 1-2-1) over the same interior pixels. Code units.
- Both for the whole frame and for up to 16 `rects` [x, y, w, h] in frame pixels (a rect counts only its pixels that are
  interior to the frame; a rect with none reads null, never 0).
- Raw numbers only. No "blurry"/"sharp" verdict. The only threshold is an optional argument `soft_max` (a lap_var line):
  when given, runs of at least `min_run` (default 2) sampled frames with lap_var <= soft_max are listed. Not given = no runs.
- Summary: sharpest and softest sampled frame by lap_var (ties go to the earliest frame), min / median / max of each score.

## Synthetic clip S (the blur ramp)

ffmpeg lavfi, 160x90, 24 fps, 3.0 s (72 frames), ffv1 lossless: a black/white checkerboard of 8x8 px squares
(geq lum = if(mod(floor(X/8)+floor(Y/8),2),255,0), gray -> yuv444p). Kimo project 160x90 at 24 fps, one media clip at 0 for 3 s,
effect `blur` on it with `radius` keyed linear 0 at t = 0.5 s and 10 at t = 2.5 s (held 0 before, 10 after).

Engine facts read from the source BEFORE predicting (src/effects.rs, src/keyframe.rs; nothing run): the radius is truncated
to an integer (`as usize`); box_blur3 uses effective box radius r = 0 when radius < 1, else max(radius / 2, 1) (integer);
keyframes default to linear ease. So r per frame (radius = 10 * (f/24 - 0.5) / 2 inside the ramp):
r0 frames 0-16, r1 frames 17-31, r2 frames 32-40, r3 frames 41-50, r4 frames 51-59, r5 frames 60-71.

Predictions on S, call focus_series {} (whole clip, step 1):
- S1 monotone: lap_var(f+1) <= lap_var(f) for every f in 0..70; the same for tenengrad.
- S2 plateaus: frames with the same effective r give bit-identical lap_var and tenengrad; exactly 6 distinct lap_var values.
- S3 strict fall: at each change of r, both lap_var and tenengrad strictly fall (5 strict falls each).
- S4 exact plateau frames (depends on the linear-ease reading above): the 6 plateaus are exactly 0-16, 17-31, 32-40, 41-50, 51-59, 60-71.
- S5 sharpest = frame 0; softest = frame 60 (earliest frame of the last plateau, ties go to the earliest).
- S6 step: step 2 returns frames 0, 2, 4, ..., 70 (36 samples) with values equal to the step-1 values at those frames.
- S7 soft_max = the softest frame's lap_var (read from the same output): exactly one run, frames 60-71 (12 frames).
- S8 the frame count is 72 and the measured size is 160x90.

## Flat and rect fixtures

- F1 flat: a solid #808080 clip: lap_var = 0 and tenengrad = 0 exactly, whole frame and any rect.
- F2 rects: ffmpeg 160x90, 1 s: left half (x < 80) the same 8 px checker, right half (x >= 80) flat grey 128. Rect A
  [8, 8, 64, 74] (inside the checker half) and rect B [88, 8, 64, 74] (inside the flat half): A lap_var > 0 and A tenengrad > 0;
  B lap_var = 0 and B tenengrad = 0 exactly; A > B on both.
- F3 a rect fully outside the frame ([200, 200, 10, 10]) reads null for both scores, not 0.

## Real clip R (the second half of the verification pair)

/home/user/kimo_real_clip/jaja-ad_24s_freeze240-263_black480-491.mp4: 1920x1080, 24 fps, 576 frames, lossless x264
(made 2026-10-18 21:31Z for the G9/G10 real-footage test; see files/kimo_eyes/W2/real_footage/PREREG.md): frames 240-263 are
all source frame 240 (bit-identical when decoded), frames 480-491 are pure black. Kimo project 1920x1080 24 fps, one media clip
at 0 for 24 s. Call focus_series {} (whole clip, step 1, no soft_max). Predictions:
- R1 frames 480-491: lap_var = 0 and tenengrad = 0 exactly; softest = frame 480.
- R2 frames 240-263: bit-identical lap_var and tenengrad (one value each, repeated 24 times).
- R3 frames 239 and 264 differ from the frame-240 value on lap_var (the source differs there by max 228/255 and 231/255).
- R4 every frame outside 480-491 has lap_var > 0.
- No prediction is made about magnitudes or about which frame is the sharpest on R.
