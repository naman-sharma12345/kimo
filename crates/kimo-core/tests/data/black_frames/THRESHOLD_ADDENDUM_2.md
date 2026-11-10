# THRESHOLD_ADDENDUM_2.md: tool default luma_max = 0.02, range tag and the 16/255 note (pre-registered)
Written 2026-10-18T18:23:48Z, before any black_frames result has been seen. THRESHOLD.md and THRESHOLD_ADDENDUM.md stay unedited; this file adds to them and wins where it differs. Task c4cfa30b, G10, on Pally's rules relayed 2026-10-18 (m:14b11f24 and m:d88de173; the second refines the first). Frozen from this timestamp: a miss is reported with its numbers; no line moves.

tool default luma_max = 0.02, same as fixture line; 0 is strict mode

## Why
Real encoded black sits at 1-3/255 (H.264 noise and dither), so a default of 0 misses a real black frame with one or two codes of noise. The fixtures prove the 0.02 line (5/255 is under it, 6/255 over; THRESHOLD_ADDENDUM.md section 3). luma_max stays an explicit argument (0..1); luma_max 0 is a valid strict mode (only exactly-black frames).

## Disclosure (a run before this file)
At 18:19:38Z, before Pally's two messages reached this task (they arrived at 18:22:21Z), a script built the tool at the OLD default (luma_max 0) and ran tests/black_frames.rs once; it ended at about 18:22Z. Its run log, console output and frame PNGs were moved UNREAD at 18:23Z into a sealed, read-only folder (sha256 list kept with the task notes) and have not been opened. Nothing in this file, and no line, was chosen from that run. The same rules and predictions apply to the next run, which is the result run.

## Re-predicted rows (the default is now L = 0.02, so "at the defaults" = "at L")
- (i) gap: unchanged, (24, 35, 12), max_luma 0.
- (ii) #080808: unchanged, no run.
- (a2) 1% and 3% logos: unchanged, no run.
- (d) fade, at the defaults: NOW one run (23, 35, length 13), max_luma 4/255 = 0.015686 (was (24, 35, 12) at default 0). At luma_max 0 (strict): (24, 35, 12), max_luma 0.
- (f) #040404 dot on black, at the defaults: NOW one run (0, 11, 12), max_luma 4/255 (was no run at default 0). At luma_max 0: no run.
- one-frame black clip, defaults written out, refusals: unchanged (the written-out defaults now carry luma_max 0.02).
- NEW test "default_luma_max_equals_the_threshold_line": calling with no luma_max reports params.luma_max = 0.02, equal to the line L in THRESHOLD.md and the test constant.
- NEW test "plain_020202_frame_strict_mode_vs_default": a plain full-frame #020202 solid for 1 s gives NO run at luma_max 0 and one run (0, 23, 24) at the default, max_luma 2/255 = 0.007843 (max_luma_255 2).

## Report fields added
- range_tags: one entry per media clip that overlaps the range, {clip_id, color_range}, color_range = "tv", "pc" or "unknown" (the file's declared tag as ffprobe reads it; anything else or missing = unknown). range_tag: the common value when all those clips agree, "mixed" when they differ, null when no media clip is in the range (solids, shapes and text carry no range).
- limited_range_suspect and note: a frame "sits flat at 16/255" when its darkest AND brightest pixel luma both lie in 15/255..17/255. Each stretch of min_run or more such consecutive frames that is not inside a reported run is listed in limited_range_suspect ({start_frame, end_frame, length}), and note is set to a sentence saying these frames sit flat at about 16/255, likely limited-range black read as full range, and are not near-black at this luma_max. With no such stretch, limited_range_suspect is [] and note is null. This makes the known limit (a limited-range file wrongly tagged full range) visible instead of silent.
- max_luma per run stays.

## Predictions for the new fields
- (g1) limited-range black tagged tv: range_tag "tv"; the black half decodes to 0 and is the run (12, 23, 12); limited_range_suspect [] and note null.
- (g2) the same samples tagged pc: range_tag "pc"; no run at the default; limited_range_suspect [(12, 23, 12)] (the black half decodes to code 16 everywhere) and note set.
- Every solid/shape fixture: range_tags [], range_tag null, limited_range_suspect [], note null (no fixture frame sits flat at 15-17/255: the darkest flat fixtures are codes 0, 2, 4, 5, 6 and 8).
