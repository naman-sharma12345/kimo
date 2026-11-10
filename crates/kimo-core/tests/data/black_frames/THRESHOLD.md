# THRESHOLD.md: fixture lines for G10 black_frames (pre-registered)
Written 2026-10-18T18:07:16Z, BEFORE the black_frames tool has been compiled or run even once, and before any fixture result exists. Worker 22078d41 (G10). From this timestamp both lines below are FROZEN: if a fixture misses its prediction, the miss and its numbers are reported; no retune, no rerun. This file is committed and pushed on its own, before the tool commit.

## What the tool defaults are (not thresholds)
black_frames' own defaults carry no tuned number: `luma_max` defaults to 0 (a pixel is black only when its BT.709 luma is exactly 0) and `min_fraction` defaults to 1 (every pixel of the frame must be black). Thresholds are arguments. The two lines below are the values the FIXTURES pass in, stated before any run.

## Line 1: luma_max L = 0.02 (5.1/255)
A pixel is black when its Rec.709 luma Y' = 0.2126 R' + 0.7152 G' + 0.0722 B' (8-bit output values / 255) is at or under L.
Rule: the same line Kimo already uses for "crushed". lint_frame `quality` counts a pixel as crushed when its luma is at or under `crush_below`, default 0.02 (crates/kimo-core/src/ext/frame_quality.rs: QParams default, `if y <= q.crush_below`). With luma_max = 0.02, black_frames' per-frame black share is by definition lint_frame's `crushed_frac` on the same frame; the test cross-checks the two on a real render. Not chosen from any run (none exists).
In 8-bit codes for a grey pixel: codes 0..5 are black at L (5/255 = 0.0196), code 6 and up are not (6/255 = 0.0235).

## Line 2: min_fraction F = 0.98
A frame is black when the share of black pixels is at or over F.
Rule: ffmpeg blackdetect's documented default `picture_black_ratio_th` = 0.98, so a station bug or logo up to 2% of the picture does not stop a black frame. Not chosen from any run.

## Fixtures and predictions (160x90, 24 fps, every frame a real engine render; nothing mocked)
Rendering facts used for the predictions (read from the code, not from any run): the canvas starts opaque black (render.rs render_frame_opts: "instead of opaque black"), a clip's opacity blends in output values `dst*(1-a) + src*a` (render.rs blend_region), and frames are read through to_rgba8, which rounds to the nearest 8-bit code.
- (i) a timeline gap: solid #c08040 at 0-1 s, nothing at 1-1.5 s, solid #40c080 at 1.5-2.5 s. Predicted at the DEFAULTS: exactly one run (24, 35, length 12), peak luma 0, min black share 1.0. Not near any line (the clips are luma 0.55 and 0.63).
- (ii) a dark shot that is not black: solid #080808 for 1 s (24 frames). Predicted: no run at the defaults and no run at L (8/255 = 0.0314 = 1.57 x L; 2.9 codes over the line). Close to the line: called that, not "pass with margin".
- (iii) a fade to black: solid #606060 (96/255) at 0-1 s with opacity keyed 1 at 0 s to 0 at 1 s (linear), then solid #000000 at 1-1.5 s. Frame k of the fade is code 96 - 4k (96 x (1 - k/24), exact). Predicted at L: frames 0-22 not black (frame 22 = 8/255 = 0.0314 = 1.57 x L, the nearest miss), frame 23 = 4/255 = 0.0157 = 0.78 x L is black, frames 24-35 are 0: one run (23, 35, length 13), peak luma 4/255. At the defaults: one run (24, 35, length 12). Close to the line: frame 23 sits 1.1 codes under L and frame 22 2.9 codes over; the fade steps 4 codes a frame and the line falls inside one step, so the run start is fixed by the step, not by the rounding.
- (iv) a black frame with a small logo bug (min_fraction): solid #000000 for 0.5 s with a white rect (add_shape rect, fill [1,1,1]) over it. Bug A 12x12 px = 144 px = 1.00% of 14400 -> black share 0.99; bug B 24x18 px = 432 px = 3.00% -> black share 0.97. Predicted at luma_max 0 and F: bug A frames are black (0.99 >= 0.98), bug B frames are not (0.97 < 0.98). At the default min_fraction 1 neither is black. Close to the line: each is 1 point of the picture from F. If the rect does not land on whole pixels (antialiased edges count as non-black at luma_max 0), the measured share moves by at most the edge pixels (under 0.6% for either rect), which keeps A over and B under F; the measured share is reported as it comes.

## What would count as a miss
Any run start, end or length different from the above, or a frame black or non-black against its prediction. A miss is reported with the numbers; neither line is moved after this file.
