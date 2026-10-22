# THRESHOLD_ADDENDUM_3.md: independence fixtures k1 and k2 (pre-registered)
Written 2026-10-18T18:44:44Z, before fixtures k1 and k2 exist in code, before they are built or run. THRESHOLD.md, THRESHOLD_ADDENDUM.md and THRESHOLD_ADDENDUM_2.md stay unedited; no line and no earlier prediction changes. Task f3a42cb5 (G10, successor to c4cfa30b), on Naman's message relayed 2026-10-19 00:11 IST (m:eac698de, pt 2). Frozen from this timestamp: a miss is reported with its numbers; nothing is tuned.

## Why
The overlap test (j) shows a black run that is also still in BOTH tools with the same frames. Two more fixtures show the tools are independent: one still but not black, one black but not still. Same rendering as every other fixture: 1920x1080, 24 fps, real engine renders. black_frames at its defaults (luma_max 0.02 = L, min_run 2); frozen_frames at its defaults (tolerance 0 = identical, min_run 2), with its rule as committed (each frame compared with its run's first frame by the largest |R, G or B| difference).

## k1: still but NOT black
Clip: one solid #c08040 (codes 192, 128, 64) over 0..1 s, nothing else: frames 0-23, all identical. Its luma is (0.2126*192 + 0.7152*128 + 0.0722*64)/255 = 0.5369, about 27x L.
- frozen_frames (defaults): one run (0, 23, length 24), max_diff 0.
- black_frames (defaults): no run (run_count 0, black_frames 0); also no run at luma_max 0.
- black_frames limited_range_suspect [] and note null (no frame sits flat at 15-17/255).

## k2: black but NOT still
Clip: a #000000 solid over 0..1 s, plus a #040404 solid on a higher track over each ODD frame only, frame k covered by (k/24 - 0.02 s .. k/24 + 0.02 s) for k = 1, 3, ..., 23 (the same +-0.02 s window as fixture (b)). Rendered frames: even frames are code 0 everywhere, odd frames code 4 everywhere; every pair of neighbouring frames differs by 4/255 = 0.015686 on every channel.
- black_frames (defaults): one run (0, 23, length 24), max_luma 4/255 = 0.015686 (max_luma_255 4); every frame is at or under L.
- black_frames at luma_max 0 (strict): no run (the even black frames are single frames between code-4 frames, each run of exact black has length 1 < min_run 2).
- frozen_frames (defaults, tolerance 0): no run (every frame differs from the previous run's first frame by 4/255, so every run has length 1 < min_run 2); run_count 0, held_frames 0.
- frozen_frames at tolerance 0.02 (5.1/255): one run (0, 23, length 24), max_diff 4/255. The tolerance, not the blackness, decides; stated so a reader sees the tools do not share a rule.

## What would count as a miss
Any row above that differs in run count, start, end, length or max (to 1e-6) is a miss and is reported as it is.
