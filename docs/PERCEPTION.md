# Perception tools

These tools return observations and advice. They do not edit the timeline or create an undo entry.

## Frame sources

`diff_frames.a` and `.b` use `{time: seconds}` or `{frame: nonnegative_integer}`, never both. Source defaults to `composite`; `clip:<id>` isolates that clip and uses timeline time; `file:<path>` uses source time and source dimensions. Clip times must be inside the clip span. Files use the same decode, render and output color pipeline, not cached thumbnails. Different source dimensions produce `size_mismatch` before cropping or downscaling.

`region: [x,y,w,h]` is an integer-pixel crop before analysis. Default max analysis side is 480; `scale: 1` keeps full resolution. The shared box filter uses fixed-order f64 sums. Display-referred u8/255 RGB drives MSE, PSNR, maximum absolute delta and changed ratio. Changed means maximum channel delta strictly exceeds threshold (default 0.04). Rec709 luma uses 0.2126/0.7152/0.0722. SSIM uses 8x8 luma windows with population variance and C1=0.01^2, C2=0.03^2. The shared normalized RGB histogram has 8x8x8 bins and distance 0.5*L1. Identical PSNR is 99 dB. Heatmaps show channel absolute differences at gain 4, capped at 1, as PNG files and MCP image attachments. CLI JSON retains the path without inline image bytes.

## Event matching

`match_events` takes plain `a` and `b` arrays of `{t: seconds, ...}`. It keeps original indices even if times are unsorted. Offset is `b.t - a.t`. Nearest matching allows many `a` events to use the same `b`. One-to-one matching is a chronological two-pointer tolerance sweep, choosing the earliest b within tolerance and never crossing pairs. Nearest matching is a linear neighbor walk with earlier-b ties. Matching is O(N+M) on sorted vectors; unsorted inputs are stable-sorted first. Output buffers are preallocated and do not reallocate during matching. Tolerance is inclusive and defaults to one project frame. `max_offset_s` excludes distant pairs entirely. Statistics cover matched pairs; empty inputs return zero statistics. p90 uses nearest rank. `suggested_ops` returns boundary-change labels, not executable operations or permission to edit.

## Beat-synced cut verification

1. Run `audio_transients` in beat mode. Map beat times to `a:[{t:...}]`.
2. Run `detect_cuts` and map `cuts[].time` to `b:[{t:...}]`.
3. Run `match_events` with a one-frame tolerance and inspect signed offsets.
4. For out-of-tolerance pairs, choose actual clip move/trim operations and use `batch` dry-run before applying them. The advisory labels are not enough to identify a clip or authorize an edit.
5. Run `detect_cuts` again and re-match. Use `diff_frames` on frames before/after a boundary to verify a visible change rather than only a timing match.

This is composition through plain data, not a combined tool. Matching alone does not prove a cut is visually meaningful or a beat confident.

Nearest returns the nearest pair even outside tolerance so drift can be measured. Use `max_offset_s` to discard distant pairs. One-to-one pairs only inside tolerance, leaving disjoint windows as full dropouts. Duplicate timestamps are valid, ties preserve earliest chronological/original order. Empty inputs return cleanly with zero statistics and all available dropouts.

Cut analysis keeps only the previous/current analysis images plus per-frame histograms, mean luma and scores. It does not retain 20,000 full analysis images. Default fps is min(project fps,12), max side160. Sensitivity gives threshold=0.6-0.5*sensitivity; explicit threshold wins. A hard-cut candidate also exceeds mean(5 previous scores)*3+0.02. Minimum gap keeps the stronger candidate. Refinement samples one analysis step either side at project fps. Labels are advice: a single step is hard_cut, a monotone luma ramp of at least three moderate steps is dissolve, a spike returning within three analysis steps is flash. Timeline clip starts are returned separately, never substituted for content scores. Maximum 20,000 analysis frames; scores are decimated to at most 10,000 points.

Opposite-direction gradual ramps across a black trough merge into one `fade` interval with `dip:true`; hard cuts into black and back remain separate hard cuts. Adaptive motion suppression precedes flash labeling. A flash must change global content and return to the prior histogram/mean within three analysis steps. The tested translating periodic checkerboard produces zero cuts/flashes, but global pans can still look like cuts. There is no general high-motion immunity guarantee.

Optional `spatial:{grid:"4x4", min_cells_ratio:0.6}` (off by default; `1x1` also accepted) requires per-cell RGB histogram change above threshold in at least the selected fraction of cells. A tested object changing one quarter of cells is rejected at 0.6; a full hard cut passes. Inspect `return_scores` and compare boundary frames with `diff_frames` when motion could explain the score.

For a cut exactly equidistant between two transients, pass the cut as `a` and transients as `b` to nearest matching: the earlier transient wins. Example cut2.0, transients1.875/2.125 yields b_index0 and offset-0.125. Duplicate times retain earliest original index. One-to-one matching chooses the earliest event inside tolerance, not globally smallest distance.
