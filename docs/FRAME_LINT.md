# Advisory frame lint

`lint_frame` reports possible problems. It never changes the project, creates an undo entry, rejects an edit or blocks export. It is not a design certification.

Implemented checks: `out_of_bounds`, `invisible_layer`, `low_res_upscale`, `static_frame`, `text_contrast`, `min_text_size`, `safe_zone`, `text_overflow`, `text_overlap`, `text_over_foreground`, `caption_over_foreground`, `caption_style_consistency`, plus the opt-in, info-only `optical_balance` (not in the default set). Asking for an unsupported check returns `invalid_check`; absence of those findings does not mean the text was checked.

Choose one selector: `time`, `times`, or `range: {start,end,step}`. The default range step is 0.5 seconds. The default and hard cap is 120 sampled frames. `params.max_frames` may lower it. Too many samples return `too_many_frames` before rendering.

Thresholds are defaults, not rules. `params` supports max_static_s (3), mse_threshold (0), changed_ratio_threshold (0), pixel_threshold (0.04), opacity_epsilon (0.001), upscale_ratio (1.5) max_frames (120), contrast_normal (4.5), contrast_large (3.0), large_cap_px (24) and min_cap_px (20 at 1080p). All effective values are echoed in params_used. Unknown parameters fail rather than being ignored. `delivery_height` scales reported text ink pixels from project to delivery height. Thresholds are 1080p-equivalent, so changing delivery height alone does not change the finding decision. It does not change layer geometry.

Layer geometry uses renderer-resolved parent transforms and source extents, not the full-canvas quad of a small shape. These are layer bounds, not effect-expanded ink. Rotated layer bounds are axis-aligned envelopes. Masks, blur, shadows, 3D tilt and text ink are not certified by those bounds. Low-resolution upscale uses the drawn bounds, so rotation can increase its ratio.

`lint:allow_bleed` suppresses out_of_bounds on that clip. `lint:ignore=<code>` suppresses that code. Every suppression is counted and listed with the layer and tag. A suppression is not a successful check.

Adjacent sampled layer hits merge into intervals, with the largest measured overflow/upscale (or lowest opacity) recorded as worst. Static intervals use the existing diff_frames metric implementation. They say only that sampled frames were unchanged for longer than max_static_s; audio presence is reported as context, not a claim of narration or an intentional hold.

`annotate` defaults true. A finding frame gets a PNG with orange-red boxes, a file path, and inline image delivery over MCP. Annotations do not edit the project. max_findings defaults 200 and limits returned findings across the call; summary counts and omitted_findings retain visibility of truncation. Each frame reports measured elapsed_ms, which is not deterministic; finding data is deterministic.

Text geometry comes from C2 compute_text_bounds static ink and lines, with stroke growth and parent transform. It excludes shadow, glow and text background boxes. Text animator/path/italic deformation is not measured. The cap-height metric is capital-H outline ink from C1 measure_text in the same font/style, multiplied by resolved layer scale. It is not the font size or the total multiline height.

Contrast hides the selected text via an N1 render-view copy and samples the display-referred background under its visible ink bbox. It uses WCAG sRGB linearization, layer-opacity compositing, and min/p10/mean ratios with spread and worst pixel location. Gradient text, blend modes, grade/effects, stroke and animator visibility can differ from the source color; this is a heuristic, not rendered-glyph certification. Busy backgrounds are described by the distribution, not guaranteed legible. Normal threshold defaults 4.5; capital-H >=24px at 1080p equivalent uses 3.0.

text_overflow compares the transformed ink bbox against the canvas only; own-box overflow and wrapping correctness are not checked. safe_zone compares the same bbox against C3 safe/avoid rects. Platform profiles retain estimated, as_of and basis and ask for current-app review; their geometry is not a verified platform policy.

## Text layout checks (V4-1 P3)

`text_overlap` fires when two visible text layers' ink rectangles (compute_text_bounds) intersect by at least `overlap_min_px` (1) on both axes. `lint:ignore=text_overlap` on either layer suppresses it (counted).

`text_over_foreground` and `caption_over_foreground` fire when text ink overlaps a foreground layer drawn below it. A layer above the text is occlusion and is not reported. A candidate is a visible (opacity > 0.05) non-text layer with measurable bounds. These are background and never fire: a layer tagged `lint:background` or with role `background`; the bottom-most visible non-text layer when it covers at least 95% of the canvas and has no matte; a solid or gradient generator covering at least 95% of the canvas with no matte. Everything else, including a full-frame layer above another full-frame layer, is foreground. Layers that are the hidden source of a matte are not candidates.

Overlap is `overlap_area / ink_area`. Layers with a mask or matte are measured by rendering only that layer (and its matte source) with transparency and counting alpha above 0.5 under the ink (`basis` is `matte` or `mask`, `subject_bbox` is the alpha bbox); other layers use their drawn bounds (`basis: rect`). Thresholds: `min_overlap` 0.10 for rect, `min_matte_overlap` 0.02 for matte/mask. A text's own backing plate is excluded and listed in `summary.excluded_layers` with its basis: `tag` (`lint:backing_of=<text id>`), `auto_backing` (a shape or solid directly below the text, adjacent in draw order, whose bounds contain the ink with at least 1px margin), or `own_box` (the text has a background box). A plate that only partly overlaps is not a backing. A clip counts as a caption when its role is `caption`, it has the tag `caption`, or it belongs to an editorial caption group; a caption fires only `caption_over_foreground`. Findings per text are ordered by overlap, then draw order, then id, capped at 3 with `more` on the last. `lint:ignore=<code>` on the text or `lint:allow_overlap` on the covered layer suppresses (counted). These are bounds/alpha heuristics, not subject detection.

`caption_style_consistency` compares each visible caption cue's font, size, color, letter spacing, stroke, shadow and align with the most common tuple in its caption group (ties go to the earliest cue). `lint:allow_style_variation` on a cue suppresses it (counted in `suppressed`).

`optical_balance` (opt-in) reports the area-weighted centroid of visible text ink rectangles and its offset from canvas centre as a fraction. Info only.

`text_contrast` now also reports `p5_ratio`, `p50_ratio` and `background_spread` (max minus min background luma under the ink). Each text-hidden render, and each matte/mask alpha render, counts against `render_budget` (default 64 per call). When it is exhausted the check is skipped, not guessed, and listed in `summary.skipped_checks` with its reason. Hiding text renders a cloned in-memory project; the live project is unchanged.
