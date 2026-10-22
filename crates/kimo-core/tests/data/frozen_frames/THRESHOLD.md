# THRESHOLD.md: fixture iii tolerance for G9 frozen_frames (v2, rule-derived)
Written 2026-10-18T16:22:34Z, BEFORE any fixture iii result run on the new seed and before the negative control has ever run. Worker c98afaa7 (successor of 9fedbf32). From this timestamp the tolerance is FROZEN: if the new seed or the negative control fails, the fail and its numbers are reported; no retune, no rerun.
v1 of this file (16:17:10Z, 0.08, "4x the grain amount" headroom, no derivation) is superseded and kept as THRESHOLD_v1_superseded_1617Z.md.

## The value
tolerance T = 6.5/255 = 0.025490196 (0..1 scale), written in the test as `6.5 / 255.0`.

## The rule: the largest difference the fixture's noise can produce by construction, plus quantisation headroom, and nothing more
Fixture iii is a still gradient (160x90, 24 fps, colour [0.9,0.35,0.1] to [0.1,0.35,0.9]) under film_finish with grain 0.02 and bloom, halation and vignette 0. The picture never changes; only the grain differs per frame.
1. Grain amplitude (crates/kimo-core/src/color/finish.rs): the lattice value `noise(x,y,seed)` is `(v>>40)/2^24 - 0.5`, so it lies in [-0.5, 0.5). The per-pixel grain is a smoothstep-bilinear blend of four lattice values (weights in [0,1] summing to 1), so it stays in [-0.5, 0.5). It is multiplied by `grain * (0.2 + 0.8*sqrt(4l(1-l)))`, whose luma weight is at most 1 (at l = 0.5). So |grain added to a pixel| <= 0.5 * 0.02 = 0.01, the same on R, G and B (monochrome), added last. With bloom/halation/vignette 0 nothing else in the frame depends on the frame.
2. Two frames of the hold: the seed is the timeline frame (render.rs:458 `seed = (t * p.fps).round()`), so frame k and the run's first frame carry independent grain. Their pre-quantisation difference is at most 0.01 + 0.01 = 0.02 = 5.1/255.
3. Quantisation: frozen_frames reads frames through `Image::from_frame` -> `to_rgba8` (round to nearest 8-bit, no dither). Two values at most 5.1 counts apart round to integers at most floor(5.1) + 1 = 6 counts apart. The values stay inside 0.1..0.9 +- 0.01, so clamping never applies (it could only shrink a difference anyway).
4. So the largest difference the noise can ever produce inside the hold is 6/255 = 0.023529, and 7/255 is impossible from noise alone. T is the midpoint 6.5/255: every noise-only difference (<= 6 counts) is at or under T, every difference of 7 counts or more is over it, and the half count of margin on each side keeps the f64 comparison of k/255 values away from the boundary.

## Why not 0.02, 0.03, 0.05 (or 0.08)
- 0.02 (= 5.1 counts) is under the bound: a 6-count difference is reachable by construction (5.1 counts of grain + rounding), so a hold could break on a seed where that happens. The 5/255 seen at 16:09-16:12Z is one sample, not the bound.
- 0.03 (= 7.65 counts) and 0.05 (= 12.75 counts) are above the bound: they also accept 7-count (0.03) or 7-12-count (0.05) differences, which the noise can never produce, so they make the tool blind to real changes of that size for no reason.
- 0.08 (= 20.4 counts, the 15:38Z draft guess) accepts up to 20 counts, over 3x the bound.

## What T still catches (the smallest visible change)
- A frame leaves the hold when any pixel channel differs from the run's first frame by 7 counts or more (2.75% of full scale).
- Under this grain, a real change of D counts at a pixel can be partly cancelled by up to 6 counts of noise, so D >= 13 counts (0.051) is caught for certain at that pixel; 7-12 counts is caught unless the noise cancels it at every changed pixel. On clean (grain-free) frames the floor is 7 counts.

## Negative control (stated before it runs)
The same grainy still, with ONE frame (timeline frame 132, inside the 120-143 window) replaced by the same gradient + the same film_finish shifted DOWN 8 px (8 of 90 rows, 8.9% of the height) on a one-frame clip on a second track; the base clip shows through the 8 uncovered top rows. On a gradient spanning 90 rows that is about 8 x 0.8 x 255 / 89 = 18 counts on R and B across rows 8-89, above the 13-count certain-catch floor. The amount was picked from that floor, not from any run.
PASS iff, at the same T over frames 120-143, the hold breaks at frame 132: runs [(120,131,12), (133,143,11)]. The difference of frame 132 from frame 131 is also reported (frozen_frames on frames 131-132 at tolerance 1). Interior-only numbers (rows away from the shift boundary) are measured from the rendered PNGs and reported beside it.

## Fixture iii pass criteria (frozen)
On frames 120-143: tolerance 0 -> no run; tolerance T -> exactly one run (120, 143, 24) with 0 < max_diff <= T.
Also recorded, as in the draft: the pan (fixture ii) at T (asserted [] in the draft test, which held at 0.03 and 0.08 on the pre-threshold runs; the pan reveals a moving black edge, which drives most of its frame-to-frame difference).

## Seeds (the grain seed is the timeline frame)
- PRE-THRESHOLD CALIBRATION seed set: timeline frames 0-23 (seeds 0..23). Runs: 16:09Z (b1, tolerance 0.08) and 16:11-16:12Z (b1 rerun/b2, tolerance 0.03): grain max difference 0.019608 (5/255), tol 0 no run, tol 0.08/0.03 one run 0-23. These came before any threshold rule was written and NEVER count as the gated result.
- RESULT seed set (fresh, never run before this file): timeline frames 120-143 (seeds 120..143), window 5-6 s of a 6 s clip.

## Honest history
- 15:38Z: 0.08 written into the test as a guess, before any run, with no reasoning logged.
- 16:09Z, 16:11-16:12Z: the calibration runs above (seed 0-23); the constant was briefly set to 0.03 after seeing 5/255.
- 16:17:10Z: v1 THRESHOLD.md logged 0.08 ("4x grain") without a derivation; superseded by this file at Pally's rule (m:aed5b3e3).
- 2026-10-18T16:22:34Z: this file. T = 6.5/255 by the rule above.
