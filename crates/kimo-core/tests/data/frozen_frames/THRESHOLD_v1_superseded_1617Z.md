# THRESHOLD.md: fixture iii tolerance for G9 frozen_frames
Written 2026-10-18T16:17:10Z, BEFORE the fixture iii result run below. Worker 9fedbf32.

Logged tolerance: 0.08 on the 0..1 scale (20.4 of 255), the `tolerance` argument of frozen_frames (largest |R,G,B| difference from the run's first frame).

Reasoning: fixture iii is a still gradient (160x90, 24 fps) under film_finish with grain 0.02 and bloom, halation and vignette 0. The grain is seeded by the timeline frame, so every frame differs, but the picture never changes. 0.08 = 4x the grain amount, as headroom for a luma-shaped grain of amount 0.02. It stays under every real change in fixtures i and ii (moving solids; a gradient sliding 0.5 px per frame that also reveals its edge), and the test asserts the pan reports nothing at this same tolerance.

Honest history (pre-threshold calibration, kept on record):
- 15:38Z: 0.08 was first written into the test as a guess, before any run, but it was not logged with reasoning.
- 16:09Z (b1) and 16:11Z (b2) PRE-THRESHOLD CALIBRATION runs on the original noise seed (timeline frames 0-23): grain max difference from the first frame 0.019608 (5 of 255). Tolerance 0: no run. Tolerance 0.08 (16:09Z) and 0.03 (16:11Z, the constant briefly tightened after seeing 5/255): one run 0-23. These came BEFORE the threshold was fixed and logged, so they are calibration, not the result.
- The result run uses a NEW noise seed: the same grainy still is read over timeline frames 120-143 (the clip runs 0-6 s and the window is 5-6 s), so film_finish draws different grain. It is run at tolerance 0 and at the logged 0.08 and reported as the fixture iii result.
