# kimo features

MCP-first, AI-first video editor in Rust. Every operation (cut, transform, keyframe, effect, transition, grade, track, mix, export) is a registered command. The MCP server's tool list is generated from that registry, so a feature cannot exist without being AI-callable. The AI can also render a frame to PNG and read video scopes, so it can look at its own work and iterate.

Legend: [x] shipped and tested, [~] partial / approximate, [ ] pending. Catalog counts below are reconciled to engine checkpoint `dd9a517` on 2026-10-11; use the live server (`tools/list`, `list_effects`) for later counts. Timings and fixture measurements are historical checkpoint evidence, not current benchmarks. Mixed checklist rows use [~] when only a subset is shipped; later sections state the limits.

## Quickstart
```
cargo build --release
python3 examples/demo_grade.py        # graded 2-clip sequence with transition, built only via MCP calls
python3 examples/effects_gallery.py   # renders every effect
```
Needs `ffmpeg` and `ffprobe` on PATH (decode/encode).

## Use from an MCP client
```json
{ "mcpServers": { "kimo": { "command": "/path/to/kimo/target/release/kimo-mcp" } } }
```
Call `tools/list` to see everything. Start with `project_set`, `import_media`, `add_track`, `add_clip`, `add_effect`, `render_frame`, `export`.


## 10 starter tools

A quick working set from README, not a measured usage ranking. Use `tools/list` for the live parameter schemas.

- `project_set`
- `import_media`
- `add_track`
- `add_clip`
- `add_effect`
- `render_frame`
- `export`
- `set_property`
- `set_keyframe`
- `list_effects`

## Module guide

Each module separates the capability checklist, parameter/control notes, limits, and proof records. Mixed source records keep their parameters, measurements and caveats together. Older checklists and later corrections are retained as written; the dated checkpoint is not a live capability guarantee.

- [Timeline and editing](#timeline-and-editing)
- [Transitions](#transitions)
- [Color and grading](#color-and-grading)
- [Audio](#audio)
- [Text and titles](#text-and-titles)
- [Compositing and motion](#compositing-and-motion)
- [Tracking and rotoscope](#tracking-and-rotoscope)
- [Particles, shards and glass](#particles-shards-and-glass)
- [3D and physics](#3d-and-physics)
- [Effects catalog](#effects-catalog)
- [Export and delivery](#export-and-delivery)
- [Performance](#performance)
- [Sandbox](#sandbox)
- [BETA and honest limits](#beta-and-honest-limits)

## Timeline and editing

### What it does

#### Timeline and trimming
- [x] multi-track timeline, add/move/trim/split/delete/duplicate clip
- [x] ripple delete, ripple trim (head/tail), roll edit, slip, slide
- [x] razor all (split_all_at), insert gap, lift / extract range, remove silence ranges
- [x] rate stretch, speed, speed ramp (time remap curve), reverse, freeze frame
- [x] markers (point/range, color, name), snap_time, list_edit_points
- [x] track lock, rename, reorder, delete
- [x] undo/redo, save/load project
- [~] group clips via precomps; pending: track targeting, sync lock, linked audio+video clips, link/unlink
- [~] nested compositions via group_clips / ungroup_clip; no editing grouped clips in place or group time remap
- [~] adjustment layers [x], track mattes [x], sequence settings per sequence, multiple sequences
- [~] designed title/lower-third presets via add_title; no MOGRT import or general parametric template system

### Parameters and controls

Use `tools/list` / `list_effects` for live schemas. Read the source parameter names, ranges and defaults alongside their evidence and limits in this module.

### Limits

- [ ] insert vs overwrite with auto-ripple on add_clip, three/four-point edit, source in/out marks
- [ ] multicam: sync by audio/timecode, angle switching, multicam to flat
- [ ] subclips, clip labels/colors, bins and metadata, search
- [ ] replace clip (keep effects), paste attributes, effect presets (save/apply), copy/paste keyframes
- [ ] auto reframe (cover-fit decode), scene edit detection, auto-cut to beats / to speech
- [ ] proxy workflow (generate proxy, toggle), offline media relink
- [ ] rolling/regular edit by timecode text ("+10", "-5 frames") helpers
- [ ] captions: SRT import [x], speech-to-text [ ] (needs whisper), caption styling presets [ ], burn-in [x via text]

### Proof level

Use the quality-tier audit and test notes in the BETA appendix; checklist marks are not a professional-tool parity claim.

## Transitions

### What it does

#### Transitions (31 registered kinds)
- [x] crossfade, wipes x4, slides x4, zoom, spin, flash, iris, blinds, checker, diagonal
- [x] dip black/white/color, whip left/right, light leak, glitch, zoom blur, clock, split h/v, noise dissolve, shatter / shatter_cells
- [~] 10 transition easings; no centered/start/end alignment control

### Parameters and controls

Use `tools/list` / `list_effects` for live schemas. Read the source parameter names, ranges and defaults alongside their evidence and limits in this module.

### Limits

- [ ] outgoing-clip push/slide-out, luma wipe from custom matte, page peel, morph cut-like, ripple dissolve, gradient wipe, barn doors

### Proof level

Use the quality-tier audit and test notes in the BETA appendix; checklist marks are not a professional-tool parity claim.

## Color and grading

### What it does

#### Color (Lumetri-style)
- [x] exposure, contrast, saturation, temperature, tint, color wheels (lift/gamma/gain), curves (luma S-curve), HSL secondary, LUT (.cube), teal/orange, vignette, color isolate, levels, brightness/contrast, channel mixer, gradient map, colorize, shadow/highlight, threshold
- [x] scopes (waveform/vectorscope/histogram data via get_scopes)
- [~] free-position master/RGB curves, hue curves, qualifier with per-effect masks and display-sRGB/D65 Lab shot match; pending: mask tracking binding, semantic match and automatic creative tone selection
- [~] opt-in scene-linear wide-gamut pipeline, explicit gamut/log transforms, supported pointwise grade LUT export, approximate film looks, optional pinned OCIO/ACES2 reference; no automatic camera IDT, measured film-stock calibration, production OCIO config import or certification

### Parameters and controls

Use `tools/list` / `list_effects` for live schemas. Read the source parameter names, ranges and defaults alongside their evidence and limits in this module.

### Limits

- [ ] denoise (temporal/spatial), sharpen [x] unsharp, dehaze, clarity, lens profile correction

### Proof level

#### LUT export / color bake (.cube 17/33/65) - BETA
Explicit ordered-steps .cube export, with native OCIO space/view steps and managed/declared transforms. Spatial, masked and implicit effects fail closed (not baked). Every export reports max/p99/RMS error against the source chain. Verified against a real OCIO 2.5.1: affine roundtrip max 1.19e-7. Nonlinear bakes carry a tetrahedral approximation error: ACEScct max 0.0137 at 65, ACES2 view max 0.0364 at 65 (RMS 0.00074). A baked LUT is an approximation, not the live chain. Source: COLOR workstream; reviewed here (build, 3 focused tests with the default build, fuzz 167 tools / 6449 calls / 0 failures).

- HDR validation (color/hdr_validation.rs + tools/hdr_validate.py, COLOR; diagnostics only, no transfer edits): independent BT.2100 PQ/HLG reference math validator, HLG OOTF evaluator (opt-in), PQ excursion/metadata diagnostics, whole-clip MaxCLL/MaxFALL audit. 7 integration + module tests. Measured limits (from COLOR's report, docs/HDR_VALIDATION.md): FFV1 RGB16 error sits at the 16-bit quantization floor (0.046 nit); HEVC 10-bit CRF 0 is NOT lossless-equivalent (p95 144 nits, max 331 nits on the fixture). Not independently re-run by MOTION beyond the test suite.

- Native OCIO (opt-in cargo feature `native-ocio`; config parse, list spaces/views, domain-sandwich effects, inspect_ocio_config + transform_ocio_samples tools; BETA, COLOR): the native CPU path was built and run by COLOR on Linux x86_64 against OCIO 2.5.1 (suite 293/293; official built-in ACES cg config loaded through the C++ adapter; real 1280x720 ACEScg to ACEScct transform with alpha exact; Python same-library parity max RGB delta 0; ACEScct published values matched to 2e-6). MOTION re-ran only the default build (config-parse/registry tests, fuzz 162 tools / 6242 calls / 0 failures); no OCIO library on this box. NOT claimed: ACES display-view rendering API, full ACES suite, GPU, hardware display, other platforms, production throughput, external LUT files, renderer-wide domain propagation (docs/OCIO_NATIVE.md).

- OCIO display/view transform (`render_ocio_display_view`, `transform_ocio_view_samples`; BETA, COLOR): forward OCIO DisplayViewTransform with looks enabled, self-contained ext module, no render/project/output changes. COLOR's evidence: completed native full suite 308/308 pass against OCIO 2.5.1 with the official ACES config set (an earlier 309 figure was withdrawn), 3.69M RGBA float samples through the full native view match the Python same-library view processor with max delta 0, one real rendered frame visually inspected. MOTION re-ran: default build + 1 integration test + fuzz 165 tools / 6356 calls / 0 failures; native path not re-run here. Limits: self-contained configs only (external LUTs rejected), CPU only, preview PNG only with an explicit sRGB_SDR declaration, managed AP1/D60 required, no HDR preview/encoding proof, no automatic output changes.

- Display scopes (`render_display_scope` tool: waveform, RGB parade, vectorscope, histogram PNG; Rec709/2020, IRE + nits graticules, PQ/HLG aware; BETA, COLOR): 2 integration tests pass here plus COLOR's reported 271/271 suite and exact known-value proofs (Rec709 red vector, PQ 100/1000/10000 nit). Limits: IRE is full-range RGB, not legal YUV; HLG nits are assumption-based; PNGs about 1 MB. docs/DISPLAY_SCOPES.md.

- Subject-aware shot match (color/subject_match.rs, COLOR; BETA, library only): matte-driven per-region Lab statistics matching with safety bounds, 16 tests, one real-photo proof (RMSE 0.0585 to 0.0193). Registered as a module only: NOT exposed as a tool or effect yet, no video/temporal validation, no automatic subject detection.

- `preview_subject_match` (level 1, preview only, no project mutation; BETA, COLOR): matte-guided per-region matching between two identity-geometry clips at explicit times, returns before/after preview + diagnostics + plan; absent regions disabled, overlapping weights rejected, fit frozen at call time. 4 tests pass here, fuzz 166 tools / 6419 calls / 0 failures. COLOR's real-pair proof on film10 frames: weighted log-luma error 1.331 to 1.180 (a modest gain, one pair). Level 2 (persistent subject_match effect in the render path) is NOT built and needs its own review and render-lane GO.

- `camera_idt` effect + chart-fit IDT profiles (color/idt.rs, COLOR; BETA): gamma + ridge 3x3 matrix fitted from chart patches, save/load kimo-idt-v1, identity defaults, alpha untouched. 9 tests; fit error 1e-6 on SYNTHETIC 24-patch data only: no chart-image detection, no white adaptation, no real camera validated, catalog-only (no MCP profile tool). Hint-driven encoding picker selects only on explicit hint plus pixel sanity.

#### add_acescct_primaries (ACEScct primaries) - BETA
Native AP1 to ACEScct to AP1 sandwich with per-channel primaries and an inverse, neutral identity, bound gamut 3. Explicit documented formula, NOT Resolve LGG parity. Conflict guard runs before the OCIO domain wrapper dispatch (v2: no double-domain interpretation) with an atomic rejection test. COLOR's proof: known math and inverse, real OCIO reference within 2e-6, determinism, neutral bit-exact incl. signed zero and HDR alpha. Reviewed here: build, cct_grade test, fuzz 169 tools / 6555 calls / 0 failures.

- ASC CDL files (BETA, COLOR): strict .cc/.cdl import (roxmltree, DTD/entities rejected), canonical semantic export, native OCIO CDLTransform with explicit ASC clamped style, inspect/apply/export tools, optional ACEScct sandwich with bound guard. Verified by focused tests (cdl_xml) and fuzz (172 tools, 0 failures); not yet checked against third-party grading-app CDL files.

## Audio

### What it does

#### Audio
- [x] per-clip volume + keyframes, fades, mute, pitch, high/low pass, compressor, echo, denoise flags, reverse, beat detection, export_audio

### Parameters and controls

Use `tools/list` / `list_effects` for live schemas. Read the source parameter names, ranges and defaults alongside their evidence and limits in this module.

### Limits

- [ ] essential-sound style: dialogue loudness normalize, auto-duck music under voice, EQ bands, de-esser, de-reverb, limiter, reverb, noise gate, stereo pan keyframes, surround, audio transitions (constant power/exponential)
- [ ] voiceover record, audio sync by waveform, loudness meter (LUFS) readout

### Proof level

Use the quality-tier audit and test notes in the BETA appendix; checklist marks are not a professional-tool parity claim.

## Text and titles

### What it does

#### Titles / text
- [x] text clips with typewriter, font size/color, keyframed transforms, masks
- [~] char/word/line animators (position, scale, rotation, opacity, blur, tracking), path text, title presets, stroke/shadow/background styles and font-weight control; pending: per-character color, unrestricted variable-font support, per-span rich text and 3D extrusion

### Parameters and controls

Use `tools/list` / `list_effects` for live schemas. Read the source parameter names, ranges and defaults alongside their evidence and limits in this module.

### Limits

Read the gaps, pending items and scope qualifications in this module's records, together with the BETA appendix.

### Proof level

- Text on path + randomized selector (V1): set_text_path (points or circle/ellipse/figure8/wave/arc/spiral, per-glyph tangent rotation, animated start progress, flip) and add_text_animator random=true seed=N (AE Randomize Order). Motion-blur sampling includes the path travel. Gaps: no per-character colour animator (text is one coverage mask, one colour), no "based on" modes (words/lines are available via unit), path margin/perpendicular offset not exposed.

- Text fixes (from external review): add_prompt_box text now sits inside its plate (measured: text left edge 273 px = plate left 244 + padding; was 174, outside the plate). add_text gains `x` and `anchor` (center|left|right) so x refers to the block center, left or right edge (measured left edge 145 px vs target 140, glyph side bearing). New `text::measure`. `font` accepts a family name or a file path everywhere (it already did; descriptions now say so). Not yet done: non-snake-case warnings, gentler speed-line/detection-box defaults.

- Review cleanups: text.rs non-snake-case warnings removed (pure rename, prompt box output unchanged); add_speed_lines default ray count 70 to 48 (gentler; pass count to restore). Detection-box default softening not done.

## Compositing and motion

### What it does

#### B. After Effects: compositing and motion graphics core
- [x] layers: video, image, text, shape (rect, ellipse, polygon, line), adjustment layer, solid-like via shapes
- [x] transform keyframes: position, scale, rotation, opacity with ease (linear, in, out, in_out, hold)
- [x] blend modes (set_blend), masks (animated, feather), corner pin, track point, stabilize
- [~] perspective 3D layers/project camera and Scene3D geometry with lights, depth of field and parenting/nulls; these are not unrestricted AE layer/lighting features
- [~] bezier timing handles, spatial paths/auto-orient, fixed expression-op stacks (wiggle/loop/sin/inertia/clamp/round); no free-form interpreter, index/valueAtTime linkage or expression-control objects
- [~] precomps on a local clock; pending: group time remap, collapse transformations, continuous rasterization
- [~] rendered-layer alpha/luma/inverted track mattes; pending: general set-matte effect and stencil/silhouette blend modes
- [~] beta grid mesh pin/tracking for smooth deformation; no puppet rig, general bezier warp/liquify or content-aware object removal
- [~] transform shutter blur, echo/frame blend, speed/time-stretch and posterize time; no flow interpolation/retime or per-effect blur
- [~] planar homography tracking/corner pin; pending: 3D camera solve and face tracking

#### Motion program (queued, requested: all motion features of Premiere, After Effects, Resolve Studio, plus kimo-exclusive)
Status honest: nothing below is done unless marked. Existing: motion blur (shutter-angle, per layer), point/ring tracker, stabilizer, speed ramp, particles, physics, Scene3D.
- [~] 1. Track mattes (alpha/luma/inverted) shipped; collapse transforms pending. DONE V1: precomps (group_clips / ungroup_clip: nested tracks on a local clock, group transform/effects/mask/opacity/keyframes apply to the whole group; gaps: no editing of grouped clips in place, no time remap of the group, no continuous rasterization, nested audio not mixed)
- [~] 2. Expressions V1 DONE: op stack per property/effect param (wiggle w/ octaves+seed, loop_in/out cycle|pingpong|offset|continue, sin, inertia, posterize_time, clamp, round) via set_expression; cubic-bezier key handles via set_keyframe_bezier (overshoot ok). Gaps: no free-form expression interpreter (linkage, valueAtTime, index), no per-axis 2D wiggle correlation, no handle UI/ separate in/out influence. Spatial paths with auto-orient shipped; text animators still need polish
- [~] 3. Echo V1 DONE (set_echo: composite/add/screen/average frame-blend ghosts, +/- time; set_posterize_time on whole clip). Gaps: no optical-flow interpolation/retime (forward source blur shipped separately), no time displacement, no rolling-shutter repair. Original: Echo, posterize time, frame blending, optical-flow interpolation and slow-mo, optical-flow motion blur for footage, time displacement, rolling-shutter repair
- [~] 5. Planar/homography tracker and limited occlusion reacquisition shipped; pending: mask tracking, 3D camera solve, face tracking, independently measured warp stabilizer parity, object removal
- [~] 6. Beta mesh pin/tracking shipped; pending: ML roto/segmentation, puppet rig, liquify, smart reframe, speed warp, optical-flow retime

- [x] impact landing for detection boxes (impact:true), add_box_expand (constant-speed expansion + 2-frame flash), add_camera_shake / impact_shake (damped spring, measured from SCUMBLE), add_hud_frame, dynamic text tokens {tc} {bar} {t} {pct}. V1: measured from one reference, not generalized beyond it; see examples/scumble_*.py for known diffs.

### Parameters and controls

- add_speed_lines (manga radial rays, flicker/spin/bg). Gaps: flat-colored rays only, no per-ray gradient or noise streak variant, CPU O(pixels x rays) (~0.1 s/frame at 70 rays).
- add_camera_shake now uses a measured per-frame kick table ([-19,-10],[-11,-6],[-4,-2] px @720p, discrete frames, same as the SCUMBLE reference; pass profile for others, profile='spring' for the old damped spring). Verified frame by frame with tools/bracket_track.py + examples/shake_align.py (kick frames and values identical to the reference at 4.12/4.60/5.08). Gaps: per-hit amplitude differences in the ref (6.04 hit is ~1.3x) and later hits 6.5-7.5 not yet verified because the HUD bracket is covered there.

- [x] 2b. Spatial motion paths (set_motion_path / remove_motion_path): bezier or through-points (auto-smooth Catmull-Rom) or presets (circle, ellipse, figure8, wave, arc, spiral), constant-speed arc-length progress via keyframable property `path` (ease, bezier handles, expressions work), auto-orient with offset and corner smoothing, correct under motion blur and echo. Gaps: no per-point speed handles (use eased `path` keys), no 3D paths, no path editing UI.

### Limits

- [ ] shape layer operators: fill, stroke, gradient fill, trim paths, repeater, merge/offset paths, zig zag, pucker/bloat, wiggle paths, round corners, twist; bezier paths
- [ ] roto: AI object matte / roto brush (needs ML model), mask tracking, refine edge
- [ ] light/color: layer styles (drop shadow [x as effect], inner/outer glow [x outer], bevel, stroke [x outline], gradient overlay)
- [ ] responsive time / presets, essential properties, data-driven animation (JSON/CSV), scripting hooks

- [ ] 4. Motion blur for Scene3D geometry and 3D layers, per-effect blur
- [ ] 7. Kimo-exclusive ideas: beat-driven motion (bind any property to audio::analyze beats), motion-by-prompt presets (AI writes the curve set), physics-driven layers [x via circle-proxy simulate_physics], motion "diff" tool (compare two clips' motion curves and match one to the other), auto motion-match to reference footage via tracker.
- [ ] Infra: HUD timecode as one dynamic text source (currently 2 clips per frame, slow export)

### Proof level

#### apply_motion_preset (motion presets and plain-language prompts)
21 named presets (pop_in, bounce_in, elastic_in, slide_in_*, whip_in, spin_in, zoom_punch, shake, glitch_jitter, wobble, float, pulse (bpm), heartbeat, fades, drop_out, drift_zoom) baked to dense per-frame keyframes on x/y/scale/rotation/opacity relative to the clip's current values. A keyword prompt matcher maps phrases like "bounce in from above, then shake" to presets in mention order (one entrance preset max) and the result lists what was applied. Tier: solid for the preset set; the prompt matcher is keyword-based, not an LLM, and returns the available names when nothing matches. Presets are hand-tuned curves, not yet cross-checked against CapCut's exact timing.

- Graph editor data tools (V1): get_curve (keys, ease, bezier handles + dense value and SPEED samples for a speed graph), move_key, remove_key, copy_curve (offset, time scale, value scale/offset, reverse, invert), simplify_curve (RDP to a tolerance, e.g. 101 tracker keys -> 45 within 1.0), smooth_curve (Gaussian). With set_keyframe_bezier and set_expression these cover the graph editor's data side. Gaps: no multi-property selection/batch edits, no tangent-mode presets (auto/flow/ease-in-out handle types), no value-graph overlay render.

- Beat-driven motion (kimo exclusive, V1): track_beats (BPM + beat grid from any audio/video asset: onset envelope, autocorrelation tempo, grid refinement) and bind_beats (keyframes any numeric property to hit on beats: pulse, kick, swing, flash curves; every-Nth beat; explicit beat lists). Measured on the SCUMBLE reference (true 125 BPM): estimate 127 BPM, beat phase about 90 ms late versus the known hit times, so for frame-tight sync pass explicit beats or an offset. Gaps: no downbeat/bar detection, no per-band (kick vs hat) envelopes, unreliable on rubato audio.

## Tracking and rotoscope

### What it does

#### Mesh tracking and mesh pin (curved / waving surfaces)
- `set_mesh_pin`: layer pinned onto an nx*ny vertex mesh, per-cell perspective, motion blur. Tier: solid (verified on a synthetic waving flag).
- `track_mesh`: engine-level 3-level image pyramid (coarse-to-fine, 4x/2x/1x) per-vertex photometric Gauss-Newton against the frame-0 patch, sequential with velocity prediction, aperture-safe ridge, neighbour-regularised. Tier: BETA+.
  Measured on a synthetic waving flag (9x6 grid, 100 frames, textured, motion blur): mean vertex error 3.3 px, median 2.2 px, p95 8.6 px (was 12.3 / 7.7 / 30 before pyramid + ridge). 40 of 54 vertices stay under 2 px. 3-4 vertices on horizontal stripes (aperture problem, x unconstrained) still slip 25-110 px late in the shot. Track 14 s for 100 frames.
  Known gaps: no per-vertex occlusion, no topology change, stripes-only regions drift along the stripe.

#### motion_extract / motion_diff (global motion match and compare)
`motion_extract` tracks the whole frame (inset quad, drift-free anchor tracker) and returns per-frame dx, dy, scale, rotation relative to the first frame; with `target_clip_id` it writes them as keyframes so text/graphics ride the footage (match-move), `invert` makes it a stabiliser, `gain` scales it. `motion_diff` compares two clips' global motion (RMS and correlation per channel, worst frame), e.g. a recreation vs its reference.
Measured on a synthetic handheld plate with known motion (100 frames): mean error dx 0.51 px, dy 0.73 px, scale 0.00 %, rotation 0.001 deg; max 1.1 px. Extract 7.3 s for 100 frames. Tier: BETA. Global similarity only: no parallax, no rolling shutter, no depth layers; a surface that changes appearance a lot over the shot (cuts, big occlusions) needs a restart range. Only synthetic ground truth so far, real handheld footage test still owed.

#### Long-gap tracking: track_long_gap (read-only) - BETA
Appearance-template tracking across occlusions (immutable seed template, whole-canvas multiscale search, ambiguity rejection, consecutive-frame reacquisition). NOT semantic re-ID. Workstream evidence: controlled 300-frame / 12 s real-footage blackout: zero invented boxes in the gap, +60 px displaced reacquisition within 2 frames, 5.55 px mean error after return, 3.61 s for 369 frames; natural faceocc2: 220/812 boxes, 6.60 px mean error, 7 reacquisitions, stays lost after frame 317 (rotation and appearance change). Limits: rotation, large appearance change and identical distractors. Reviewed here: build, 6 unit tests, fuzz 168 tools / 6512 calls / 0 failures.
Long-gap level 2 (opt-in `appearance_memory=true`, default false = v1 behavior; BETA): bank of 8 learned anchors, learns only from 3 spatially consistent strong observations and never while lost or tentative. Natural faceocc2: 528/812 boxes (v1 220), 9 reacquisitions past the old permanent-loss point, 8.15 px mean error, 0 boxes under 0.5 IoU vs ground truth (v1: 70/220 under 0.5, min 0.380, undersized boxes, not wrong-person locks; post-blackout 15/58 vs 0/56). Controlled blackout still clean (zero gap boxes, 3.24 px). Wording: v1 is center-accurate but boxes can shrink; memory mode gives tighter boxes and drift survival, opt-in because of contamination risk; neither is a general no-false-lock guarantee. Limit: frames 650+ at full cap still lost. Reviewed here: 8 unit tests, fuzz 168 tools / 6516 calls / 0 failures.

### Parameters and controls

- `track_planar` (homography, 4 corners) + `pin_layer` (perspective corner pin, keyframed, 3x3 supersampled AA edges, premultiplied) + motion blur on the pinned layer from the tracked motion + `light` brightness match.

### Limits

Read the gaps, pending items and scope qualifications in this module's records, together with the BETA appendix.

### Proof level

#### Plane-pose motion model (planar tracker core)
- `pose_predict`: constant-velocity extrapolation of a tracked plane in pose space (rotation vector + translation, plane size fixed), used to coast through occlusion and when the surface leaves the frame. Unit-tested exact (1e-12 px) on a synthetic rotating plane. Tier: BETA.
- Measured on the 3D overtake room: coast error at the end of the shot 1407 px -> 1179 px (the sign is fully off-frame there, so this is not visible in the composite). It does NOT extend how far the tracker survives: tracking still ends at about 3% visible, because the data is gone. Using the pose prediction as the always-on prior made tracking worse (noisy pose from 1-2 px quad noise), so it is only used while lost.
- Focal length is assumed 0.9 x frame width (env KIMO_F overrides, for tests). No focal self-calibration yet. Edge-on survival below about 3% visible is a data limit, not solved.

#### Real handheld footage test (motion_extract), honest result
Clip: Wikimedia Commons "Wzwz munich petuelpark 09d" (CC0, 540x720, 3.7 s, handheld tilt with a sudden bright burst filling the frame near the end). motion_extract measured a large move (dy 138 px, roll 13 deg, scale 0.72). Match-moved text sits on the rock through the shot, including the burst. Inverting the motion as a stabiliser cut the residual camera motion from std dx 24.5 / dy 36.8 px / rot 3.8 deg to dx 2.7 / dy 22.6 px / rot 0.89 deg, i.e. roll and sideways motion are mostly gone, but vertical motion is only reduced by ~40 %: the burst and heavy motion blur break the whole-frame track in the second half. Tier stays BETA. Needs a restart/segment mode and a robust exposure-jump gate before it can be called solid on real footage. Track 6.3 s for 93 frames at 540x720.

#### Planar tracker (mocha-style) - built, tiers

- Mocha/AE/Resolve-style algorithm (not validated against those products): sub-pixel planar track, perspective pin, occluder-robust (Tukey weights), exposure-invariant (zero-mean/unit-variance photometric model).

- Synthetic fixture measurement: anchor-relative by re-aligning to the frame-0 anchor each frame; coasts through full occlusion and re-acquires with a coarse search. Synthetic perspective move + h264 + 180deg blur + occluder: 0.33 px mean / 0.39 px p95 corner error vs ground truth (examples/planar_demo.py, tools/wall_plate.py).

- Shipped separately: beta mesh/grid tracking, optional reanchor segments and anchor/adaptive auto-mattes (tiers below). NOT YET: backward tracking from a middle frame, general multi-anchor management and grain/lens match on the pinned layer.

- Occluder auto-matte (`track_planar occluder_matte=true`): at render time the live frame is compared with the frame-0 surface (warped by the tracked quad, brightness-matched); wherever they differ the pinned layer is cut, with 2 px grow and feather. Tier: automated difference-matte workflow, not independently measured Mocha parity; limits: occluder with the same colour as the surface is not detected, persisted recipe rebuilds the anchor after reload, no temporal smoothing of the matte yet.

- Adaptive occluder matte (`occluder_mode=adaptive`, LIMITED-USE tier, measured on one real clip): rounded 0..8 plane-px min/max-envelope compare (default 1) (`occluder_tolerance`), chroma-dominant metric (`occluder_chroma`), island/hole cleanup by area (`occluder_min_area`), thin-structure cleanup (`occluder_thin`, two-stage so fragments are not merged), temporal median (`occluder_temporal`), grey closing (`occluder_close`, default off, did not fix notches connected to the occluder), persisted recipe that rebuilds after reload, follows constant clip speed and speed-ramp clips with the same frame selection as the renderer (decoder phase bug found and fixed on the real key). Real keyboard/finger result: finger protected, no flashes over all 110 frames, glyph islands and contour rings removed, residual small notches touching the finger edge at entry and mild edge scallop. NOT claimed: neutral or dark occluders (chroma 1 cannot cut them), universal matting, or a clean solid-key replacement. Synthetic leak 0.01% to 0.14%; synthetic wins did not transfer to the real clip, so only the real-clip numbers count.

- Rotoscope assist (`roto_propagate`, BETA, weak gain): fills a path mask between your keyframes with per-frame optical flow (robust affine fit to interior flow, forward from one key and backward from the next, falling back toward plain interpolation where the two chains disagree) and snaps the outline to the strongest edge within `snap` px. Measured: exact-GT synthetic (textured ellipse rotating and accelerating over real keyboard footage, mean IoU over held-out frames): keys 6 frames apart interpolation 0.837 vs roto 0.870 (snap 6), 10 apart 0.674 vs 0.708, 25 apart 0.141 vs 0.144 (flow chain not trusted, so it falls back; raising `trust` to 1.5 reaches 0.335 at 25 frames but drops 6-frame IoU to 0.72). Hand-traced real finger (keys 12 frames apart, one held-out frame): 0.732 interpolation vs 0.770 roto; that ground truth is hand-traced by eye (about +-5 px), so treat it as indicative only. Memory is half-resolution gray per segment (300 frame cap). Not a replacement for dense matting: gains over interpolation are small (+3 points) and fast large motion is unsolved. SCUMBLE independent real-finger 8-frame trace (keys .64/.96, held-out .80): interpolation IoU 0.727 vs roto 0.747 (+2 points, less background spill, mean snap 2.2 px), held-out fingertip and lower edge still visibly missed: NOT a production neutral matte even with 8-frame keys, and key spacing does not guarantee accuracy. Guided-filter refine (mask `refine` 4/8/16 px) on top of roto does not help on this crossing (IoU 0.745 vs 0.745/0.740/0.740, false px 556 vs 622/688/720): the error is shape/motion, not edge softness; hand-traced ground truth is about +-5 px on a ~40 px finger, so differences of 1-2 IoU points are inside the noise (examples/roto_refine_test.py).

- Speed ramps: planar tracking and the occluder matte read ramp clips at the displayed source frame (`Clip::source_index`, floor(source_time*fps+1e-6)); the pin itself is rendered from keyframes the tracker wrote at output times, so the pin path does not call source_index at render time and a ramp edit needs a re-track; decode is windowed (COLOR patches 127/128). Renderer-aligned tracker vs export max 1.41 px over 87 frames. Reverse/freeze time modes are not handled by the occluder.

- Off-frame prediction (planar): homography-extrapolated motion prior pulls corners toward the predicted quad in proportion to how much of the surface has left the frame. Overtake room test (sign leaves frame left), mean corner error vs ground truth: 33% visible 14.6 -> 5.6 px, 22% 18 -> 7.6 px, 12% 20 -> 14.6 px, 4% 61 -> 34 px. Track still declared lost at about 3% visible (frame 120 of 138). Not solved: true edge-on (width -> 0) needs a camera-motion model, not just a homography prior.

- Planar tracker robustness (real footage): quad validity gate (convex, bounded area/corner jump) and a confidence-drop rule (a sudden fall to under about 0.7-0.8 of the recent mean means occlusion: coast, do not follow). Real-footage case (SCUMBLE reference, overpass face 8.9-11.5 s): tracks the shake settle (about 15 px) at conf 0.87, then a full-frame white title slam covers the surface and the track holds the last good quad instead of latching onto the title (before the gate it diverged to -900 px). Does not re-acquire within that clip because the title stays up. Limit: real-footage coverage is one shot; no long handheld test yet.

## Particles, shards and glass

### What it does

#### Shard burst (add_shard_burst) - V2
3D glass-shard cloud generator: hundreds of prism shards, project-camera projection, internal depth sort, DOF softness, rim glints with dispersion, 4-sample motion blur. Gaps: background-aware screen-space displacement shipped (see below), not ray-traced refraction; no shard shadows/physical light calibration, not GPU.

#### fracture_glass (native effect, v1) - BETA
Procedural laminated-glass shatter, a normal clip effect (add_effect kind=fracture_glass), deterministic per seed, binds the live post-effects clip canvas (no re-grade, source time continues).
- Topology: irregular Voronoi cells from jittered rings around one impact point (t0, impact_x/y). Ring site counts step up outward (branching), plus `debris` fine fragments near the impact, so big shards and small debris mix. Crack edges zigzag and vary in width and brightness (dimmer near the centre, sparse glints).
- Dynamics: cracks grow outward at crack_speed (not all present at frame 1). Intact facets refract the live source with RGB dispersion. Cells release in a radial stagger (release_delay, release_spread, release_radius), then fall with gravity, push, 3D tumble about a random in-plane axis (perspective foreshortening, back face visible), slab thickness walls, and sub-frame motion blur (blur_samples x shutter_deg at `fps`). Fade is absolute from impact (fade_start, clear_time). Holes are transparent with a hole_dim black overlay, so the lower layer shows through.
Measured: 1280x720 about 0.23 s/frame intact, 0.4-0.65 s/frame while shards fall (5 blur samples, 2 vCPU).
Honest limits: single impact point; shards are flat polygons with a constant-thickness slab, no refraction through the slab or edge caustics; no shard-shard collisions or ground contact; glint is a fixed procedural light, not an environment; outer pane still reads as roughly even cells (no long hairline radial cracks); no spall or micro-cracks; look judged by eye on one contact sheet, not against a ground-truth reference.

### Parameters and controls

fracture_glass v1.1 (BETA): ring budget planner (rings always reach the frame edge, ring_growth is coarsened to fit max_cells, cap 2500, cost about N^2), edge tint params (edge_r/g/b, neutral-ish default), stronger per-shard tumble variance plus an initial per-shard tilt at release (pieces read as individuals), per-shard per-sample time jitter in motion blur (no stepped echoes), cracks masked out of holes (a released cell's cracks die with it) and optional `crack_clear` absolute fade for the remaining pane, glass material response (face_opacity, grazing-angle fresnel reflection, moving sheen streak, edge sheen). Limit: the effect cannot refract the layer below (it only sees its own canvas), so translucency is plain alpha over whatever is underneath.

fracture_glass v1.1.1: far slab face now shares face_opacity (it was an opaque grey polygon behind translucent faces), more ring jitter so the outer pane stops reading concentric.

- fracture_glass v1.2 (BETA): per-shard velocity decorrelation (`vel_noise` direction/speed/swirl, `release_jitter` lead/lag) so the burst is not a uniform funnel; shard faces use a lens/tilt-distorted, dispersed texture sample (`face_refraction`). Limit: the effect only sees its own canvas, so it cannot refract the layer beneath; size-dependent drag and shard collisions not done.

### Limits

Read the gaps, pending items and scope qualifications in this module's records, together with the BETA appendix.

### Proof level

#### SCUMBLE hero shatter recreation (examples/scumble_shatter.py) - V1 (recreation, not parity)
Composed from existing tools: keyboard bg with push-in and blur ramp, finger/key sprites, orange spark emitter, physics shard burst with camera fly-in and zoom-blur streaks, detection-box HUD. Gaps vs reference: finger/key/keyboard are drawn stand-ins (no real macro footage or 3D model), shards lack true refraction of the scene, no caustics, lighting is fake, no rigid-body interaction with the finger.

#### Background-aware refraction (shard burst refract:px) - V1
The shard pass writes a screen-space displacement field (tilt x thickness, stronger on side walls, motion-blurred with the shards); the compositor bends the canvas behind the layer before blending. Gaps: single offset (no per-channel dispersion of the background), displacement clamped, shard layer only (not a general refractive material for any layer), no caustics.

Shard geometry V1.5: 4-6 vertex irregular convex chunks with varied aspect and thickness. Gaps: still extruded polygons, not fractured volumes (no Voronoi-fractured solids, no bevel normals).

- Shard planes behind a moving occluder (`shard_burst` plane.occlusion_target + `occluder_expand`, LIMITED-USE tier): the shard plane consumes the target clip's adaptive matte in final screen space (scale, pos, rotation, motion path, 3D pose, project camera), evaluated on demand so track order and target opacity do not matter. Measured: synthetic magenta-over-skin 2476 -> 0 at target scale 1.12; real macro key shot, original order, 35 burst frames: interior finger protected, small boundary fringe remains, not a dense body mask. Neutral/dark occluders: `occluder_luma_gate` removes 100% core leak to 0.0% (false reject 0.0%) on COLOR's synthetic neutral fixture only, FAILS on real monochrome keyboard footage: gate-on cuts 5.2%/6.1%/1.7% of a clear key patch at 1/2/3 s (false rejects on printed key surface), gate-off cuts none; stays experimental, not neutral-occluder support. Mode 2 (`occluder_luma_gate=2`, brighter-than-surface only) cut that real false-reject to 0.0% at 0/1/2/3 s and keeps 0% core leak, 0% false reject on the synthetic fixture; on a REAL finger crossing over a dark key mode 2 FAILS the bar: skin leak 22.5% (1 s) and 38.7% (1.5 s) of bright skin ROI, plus 47% of dark surface wrongly cut on retreat (ghosting). Experimental, not usable for neutral occluders.

- Contact shadows for debris (`shard_burst` contact_shadow 0..1, shadow_reach, shadow_plane_z; BETA): each shard projects a soft shadow onto the receiving plane along the light vector, blur and opacity falling with height. Measured on a synthetic grey plate: shadows only darken (0 brightened px, 0 shard px changed), 1887 plate px darkened by mean 3/255 at t=1.0 strength 0.7. Not yet checked on a real shot; shadow direction follows the `light` vector and has not been validated against real footage lighting. Shadow respects the occluder matte (real key repro at 1.64, strength 0.6: 1632 plate px darkened, max 8.7 codes, 0 on the finger, 0 brightened; subtle on dark keys). Frame cost about +0 s on the synthetic scene (1.9 s on vs 2.3 s off, noise).
  CORRECTION (COLOR found it, fixed in this commit): before this fix the contact shadow was effectively only a thin halo around each shard, because the polygon signed distance had the wrong sign (two negatives cancelled), so the earlier measurements above (1887 darkened px / 1632 px on the real key) described that halo, not a grounded shadow, and ON vs OFF could render byte-identical in clean fixtures. film10 and earlier deliveries have no real contact shadows. After the fix (examples/shard_shadow_test.py): synthetic grey plate 76681 px darkened (mean 38.7 codes at strength 0.6, max 122), 0 brightened, finger preserved on the real key repro (40285 px darkened, mean 17.7, max 90 at strength 0.6; 2392 px differ upward by >1 code, under translucent shards, not investigated). 4 regression tests (contact_shadow_validation): ON differs from OFF, fades with height (0.51 near / 0.12 far peak alpha delta), reverses with light direction, exact zero when occluded. Still: fixed-z receiver, overlap summed and clamped, no real-light calibration; strength 0.6 is strong, tune down for subtle grounding.

- Shard motion blur controls (`shard_burst` shutter_deg default 180, 0 = off; blur_samples default 4, 8 with texture; BETA): the old fixed 4 time-samples left stepped ghost copies with visible wireframe outlines on fast shards. Measured on the real burst repro (1.28 s and 1.64 s, RMSE in the shard region vs a 32-sample reference): 4 samples 2.07/1.42, 8 samples 1.38/0.71, 16 samples 0.44/0.40. Cost is linear: 1.64 s frame 2.2 s at 4, 4.7 s at 16, 8.6 s at 32 (1280x720, one render_frame). Defaults unchanged so existing renders are byte-identical. Blur is time-sampled, not per-shard velocity streaks; sample count is manual (no automatic velocity-adaptive count yet).

- Shard trails (`shard_burst` trail 0..1, trail_length s default 0.12, trail_samples 2..24 default 8; BETA): faded earlier-time copies of every shard, drawn with the same shader and occluder behind the shard (older = fainter). Off by default; legacy renders byte-identical (md5 equal on the 1.64 s burst repro frame). Measured on that burst: trail 0.7 length 0.15 s changes 101,003 px, 0.0 diff in the finger region, costs about +8% render time in my run (21.1 s vs 19.5 s, first-frame decode included). Honest read: on this scene the shards are near-black over a dark keyboard so the trails are subtle; not judged on a bright plate, no per-shard velocity-based length, trails are time-lagged copies (not particle emitters).

- Shard looks (`shard_burst` material legacy|glass|concrete|metal, size_bias, size_scale, env_reflection, env_roughness; BETA, SHARD-LOOK workstream, 5 render tests + 9 module tests): opt-in only. Verified here: legacy configs render BYTE-IDENTICAL (md5 equal on the 1.28 s and 1.64 s burst repro frames before/after the patch); metal/glass on that burst change only the shard pixels (finger/background region diff 0.0) and render in 14-17 s/frame at 8 blur samples. Caveats from the authors: synthetic scene only, material is per burst not per shard, analytic sky/ground environment (not an HDRI), fine glass reads darker and more restrained than the legacy rims. No claim of photoreal glass.

- Backdrop access for effects (BETA, engine capability): an effect can declare `needs_backdrop` (effects2::needs_backdrop); the renderer then gives it a read-only copy of the composite below its layer via render::backdrop() (one frame copy, built only for clips carrying such an effect; thread-local so parallel export is safe). First user: fracture_glass `transmission` (0..1, default 0, so existing output is unchanged): shard faces mix in the real background, refracted with per-channel dispersion. Verified visually on the test rig (faces show the background instead of flat source chips); not yet verified on the film, and refraction is a distortion offset, not a ray-traced lens. Limit: only fracture_glass uses it so far.

- Backdrop fix: the backdrop is now set right before a layer's own effects, so it also works on precomp layers (nested render of children no longer clears it). Verified: a precomp-wrapped fracture_glass with transmission renders pixel-identical to the flat clip, and transmission 0.9 differs from 0.0 in the nested case.

## 3D and physics

### What it does

#### 3D suite status
Done: layer transforms, camera, shard-internal depth sort/DOF/motion blur. Scene3D additionally has materials, lights, optional shadows/AO/env and null parenting. Todo: general layer depth sorting, DOF/motion blur on flat 3D layers, extrusion and camera rigs (orbit, look-at).

#### Rigid-body physics (physics.rs, used by add_shard_burst physics:true) - V1
Deterministic 240 Hz baked sim: mass from size, linear/angular velocity, gravity, linear and angular drag, floor plane with restitution + Coulomb friction (spin from contact), body-body impulse collisions (sphere proxies, spatial-hash broadphase). Scrub-safe via cached trajectory. Gaps: sphere proxies not true convex hulls (no resting stability on faces), no joints/constraints/soft bodies/fluids, general-layer simulate_physics is a separate 480 Hz circle-proxy tool (see below), CPU.
Shard shading: Schlick fresnel and depth falloff params. Screen-space background displacement is applied by the compositor when refract is enabled. Gaps: no ray-traced refraction, caustics or area lights.

#### Scene3D (add_scene3d) - V1
Real geometry renderer: z-buffered triangles, box/rounded-box/cylinder/capsule/sphere/plane/null, parent transforms (nulls), keyframed pos/rot/scale per object, PBR-style materials (albedo, roughness, metal, emissive, opacity, Schlick fresnel, Blinn-Phong GGX-like spec), directional and point lights, ambient, 2x2 SSAA, depth-of-field from the depth buffer, transparent objects sorted back to front. Seen through the project camera, composes with shard bursts and everything else. Optional shadows/AO/procedural environment reflections are described below. Gaps: no global illumination or HDRI environment-map import, no textures/UVs, no mesh import (OBJ/glTF), no extrusion of text/shapes, no motion blur on scene geometry, transparency is per-object sorted (no per-pixel OIT), CPU only.

Scene3D additions (V1): shadows:true (1536 px shadow map per directional light, 3x3 PCF), ao:<strength> (screen-space ambient occlusion from the depth buffer), env:<strength> (procedural studio environment reflections with fresnel, driven by the lights). Known artifacts: faint facet banding on the curved capsule under shadowing (bias tuning), no soft area-light penumbra, shadows from opaque objects only, point-light shadows not done, env is procedural (no HDRI import).

#### simulate_physics (2D layer physics baked to keyframes)
Motion-graphics physics for layers: circle-proxy bodies (gravity, drag, restitution, friction, canvas walls, collisions, spin) simulated at 480 Hz and baked into dense x / y / rotation keyframes. Scrub-safe, editable in the graph editor, motion-blurs normally. Tier: solid for circle proxies; no polygon/concave collision shapes, no joints, no pins. Example: examples/physics_demo.py (14 shapes dropping and stacking, 5 s, 74 s render at 1280x720 with 180 degree motion blur).

### Parameters and controls

Use `tools/list` / `list_effects` for live schemas. Read the source parameter names, ranges and defaults alongside their evidence and limits in this module.

### Limits

Read the gaps, pending items and scope qualifications in this module's records, together with the BETA appendix.

### Proof level

Use the quality-tier audit and test notes in the BETA appendix; checklist marks are not a professional-tool parity claim.

## Effects catalog

### What it does

#### C. After Effects effect categories
(AE category -> kimo effect, based on Adobe's effect list)

#### Blur & Sharpen
- [x] Gaussian-style blur, directional blur, spin (radial) blur, zoom blur, sharpen (unsharp), tilt-shift (lens blur band)

#### Channel
- [x] channel mixer, invert, solid via gradient ramp

#### Color correction
- [x] brightness/contrast, curves (luma and free master/RGB), levels (master), exposure, hue/sat (HSL), tint, colorize, color balance via wheels, gamma/pedestal/gain, photo filter-like via temperature, shadow/highlight, vibrance-like via saturation

#### Distort
- [x] bulge, twirl, ripple, wave warp, turbulent displace, polar coordinates, offset, mirror, motion tile, flip (transform), lens distortion (optics compensation), kaleidoscope-style, corner pin, glitch blocks, pixel sort
- [~] beta mesh pin/tracking shipped; pending: bezier warp, general displacement map (using another layer), liquify, magnify, reshape, smear, spherize, warp (presets: arc, flag, fish...), warp stabilizer (stabilize exists via tracking) , detail-preserving upscale, transform effect, rolling shutter repair, CC (bend it, ripple pulse, page turn, lens, power pin, slant...), wave presets

#### Expression controls

#### Generate
- [x] checkerboard, grid, gradient ramp (linear/radial), fractal noise (via fractal_noise), lightning / energy beam, lens flare, circle/ellipse/rect via shape layers

#### Immersive / VR

#### Keying
- [x] chroma key (color, tolerance, softness), luma key, spill suppress, choker, feather alpha
- [~] keyer with screen gain/balance, clip black/white, despill and edge unmixing (synthetic tests, not Keylight parity); pending: color difference key, color range, difference matte, extract, inner/outer key, linear color key, spill suppressor for any color

#### Matte
- [x] matte choker (choker), feather alpha

#### Noise & Grain
- [x] film grain, noise via fractal noise, film damage (scratches, dust, flicker)

#### Perspective
- [x] corner pin, drop shadow (alpha), long shadow
- [~] project 3D camera shipped; pending: bevel alpha, bevel edges, radial shadow, CC cylinder/sphere/environment, extrude

#### Simulation
- [x] particles (embers, sparks, snow, rain, dust) via add_particles

#### Stylize
- [x] glow, outer glow, edge glow (neon), find edges, emboss, halftone (mosaic-like dots), pixelate (mosaic), posterize, threshold, strobe, scanlines, VHS, vignette, outline, sepia, invert

#### Text

#### Time
- [x] time remap via speed ramp, reverse, freeze
- [~] echo/frame blend, posterize time and forward optical-flow source blur shipped; pending: time displacement, flow interpolation/retime, slowmo and per-effect force blur

#### Transition effects (as effects, not clip transitions)

#### Utility
- [x] letterbox (cinema bars)

#### Obsolete / classic still used by pros
- [~] basic text, perspective 3D and path text shipped; pending: lens blur, lightning (classic), channel blur, bevel, bulge-style CC effects, reduce noise

#### Third-party style looks (Hollywood staples, implement natively)
- [~] light leaks, anamorphic streaks, lens flare, film grain, halation, bloom [glow], chromatic aberration, lens distortion, VHS, glitch
- [~] approximate halation, film-print and bleach-bypass effects shipped; pending: calibrated 35mm film emulation and film gate weave, day-for-night, volumetric light, fog / atmosphere, heat haze [x turbulent_displace], energy shield, hologram, screen replacement (corner pin + tracking), sky replacement (needs segmentation), digital clone/duplicate, speed-line, rain on lens, camera shake presets [x shake], rolling shutter, lens dirt

### Parameters and controls

- `silhouette_fill` (effect): flat-color silhouette of a clip's alpha, mix amount, optional outline ring. Tier: solid. Gap: no pattern fills, no auto-segmentation (mask comes from an RGBA cutout).

- `add_glitch_confetti` (generator): random flat squares + horizontal tear lines, reshuffled at flicker_fps. Tier: solid. Gap: no block displacement of the plate under it (use glitch_blocks).

- Masking stack (masks.rs): see docs/MASKING_ROADMAP.md for the full table and honest tiers (ML roto, mask tracking, per-point feather QUEUED).

### Limits

- [ ] camera lens blur with depth map / bokeh shapes, bilateral, channel blur, compound blur (using layer map), fast box variants, smart blur, reduce interlace flicker

- [ ] arithmetic, blend, calculations, channel combiner, compound arithmetic, minimax (erode/dilate, partial: choker), remove color matting, set channels, set matte, shift channels, solid composite

- [ ] levels per channel, color balance (HLS), black & white mixer, change to color, change color, selective color, tritone [x via gradient_map], leave color [x color_isolate], auto levels/contrast/color, Lumetri full, vibrance, CC tools (toner), color link, equalize, easy levels, output (clip), PS arbitrary map, color stabilizer, match color

- [ ] slider, checkbox, color, point, angle, layer, dropdown controls feeding any param

- [ ] 4-color gradient, advanced lightning variants (strike, branch decay), audio spectrum, audio waveform, beam (laser), cell pattern, eyedropper fill, fill, fractal (Mandelbrot/Julia), paint bucket, radio waves, scribble, stroke, vegas (animated outline), write-on, caustics

- [ ] VR to sphere, VR rotate sphere, VR blur/sharpen/denoise/glow, VR chromatic aberrations, VR plane to sphere, VR convert projection

- [ ] refine soft matte, refine hard matte, simple choker variants, matte tools for tracked mattes

- [ ] add grain with presets, match grain, remove grain, dust & scratches (cleanup), median, noise HLS, noise alpha, noise (additive/mono), motion/temporal denoise

- [ ] Particle Playground-style emitter with forces/gravity, particle trails, collisions, Foam / bubbles, Caustics / water surface, Card Dance, Shatter, Wave World, CC particle world/systems (fire, smoke, snow), cloth-ish cards, rigid-body sims (via integration with a physics crate), fluid/smoke sim, hair/rain on lens

- [ ] brush strokes, cartoon, color emboss, roughen edges, scatter, texturize, CC glass/kaleida/mr. mercury/blobbylize/burn film, CC threshold RGB, dither, tile mosaic, trace/vector stylize, sketch/pencil, oil paint, anaglyph

- [ ] numbers, timecode (source timecode burn-in)

- [ ] block dissolve, card wipe, gradient wipe, iris wipe, linear wipe, radial wipe, venetian blinds, CC glass wipe, CC grid wipe, CC image wipe, CC jaws, CC light wipe, CC line sweep, CC radial scale wipe, CC scale wipe, CC twister, CC warpo

- [ ] Cineon converter, color profile converter, grow bounds, HDR compander, apply color LUT [x as lut], tone mapping, compositing for log

### Proof level

- `denoise` effect (spatial, edge-aware, Rec709 Y'CbCr, alpha untouched, amount 0 = identity; BETA, from the DENOISE workstream, 11 module tests): 960x540 real frame + Gaussian noise sigma 12 codes: noisy 27.52 dB; denoise r2 33.80 dB (0.60 s incl. render); r3 luma .8 chroma .9 36.52 dB (0.74 s); OpenCV bilateral 36.28 dB (0.04 s), OpenCV NLM 32.71 dB (0.65 s). One soft macro frame, one noise model: a tie with a bilateral filter, not a claim of beating a production denoiser. No temporal pass in the effect path (the module supports neighbours; effects see one frame). examples/denoise_proof.py.

## Export and delivery

### What it does

#### Delivery
- [x] mp4 export (parallel, streaming), gif, audio export, still frame

### Parameters and controls

Native OCIO view video (explicit opt-in): `set_ocio_view_delivery` stores config,
source space, display/view, scene-linear input gamut, and actual output
gamut/transfer declaration. `export_ocio_view_video` applies the native OCIO
2.5.1 forward display processor including looks, bypasses project output, then
uses strict HEVC10/FFV1 16 delivery tags. No guessing from view names. Selection
must match the current working gamut and does not affect ordinary export.
Caller declares source gamut/white and view output encoding; tags do not prove
these declarations. Existing mastering metadata is NOT inherited. No native
HDR mastering certification. Integer outputs clip 0..1 and HEVC uses YUV420.

### Limits

- [ ] export presets (YouTube, TikTok, ProRes, DNxHR, image sequence, WebM/VP9, AV1, HEVC), render queue, background render, in/out range export, alpha export, burn-in timecode, watermark, chapter markers

### Proof level

#### Explicit ordinary-video delivery (COLOR132)
Ordinary H264 export now converts Rec709 RGB to BT709 limited-range YUV and writes matching primaries/matrix/range plus the exact linear/sRGB/BT709 transfer tag. Disabled projects retain display-sRGB rendering; managed ordinary export rejects other gamuts/transfers instead of leaving players to guess. BT601 delivery is unsupported. High-bit HLG, like PQ, requires Rec2020. Migration: legacy MP4 delivery pixels changed from implicit conversion; re-export older MP4s for the explicit contract. Project rendering/defaults are unchanged.

#### Export: start/end range + identity-transform fast path - BETA, measured
- `export` takes optional `start`/`end` seconds. Frame k renders at its absolute time, so range frames equal the same frames of a full export (verified at crf 0 by decoded frame md5: 30/30 frames identical) and audio is sliced from the same mix.
- Layers with an identity transform no longer copy the whole 1080p float frame (33 MB) before blending. Output is unchanged: a full export of the benchmark project is frame-md5-identical to the pre-change export.
- Measured (2 vCPU, 1080p30, 4 s project: image background with vignette+grain and 6 text layers, crf 0): full export 42.2 s to 32.0 s (1.32x); a 1 s range export takes 8.6 s.
- Per-frame cost is still dominated by full-frame float layers and the x264 encode; a static-layer cache is designed but NOT built (needs strict invalidation for time-varying effects and hash-verification against uncached output).

High-bit delivery contract (COLOR): disabled-project exports now explicitly use
Rec709/sRGB, BT709 limited-range YUV for HEVC10 or full-range identity RGB for
FFV1 16. This fixes the formerly untagged/default-matrix disabled HEVC path.
Managed high-bit exports accept Rec709/Rec2020 with exact linear/sRGB/BT709
transfer tags, plus Rec2020 PQ/HLG. Other gamuts and gamma2.4/log/ACEScct output
that previously exported untagged or partly tagged now ERROR before encoding.
Fix: select a supported delivery gamut/transfer, or use float EXR/TIFF for working
or log interchange. This is signal labeling/precision, not HDR display calibration.

- export_alpha (BETA): timeline or range with transparency as ProRes 4444 .mov (10-bit 4:4:4:4), WebM VP9 + alpha, or RGBA PNG sequence. Verified: all three decode clean, PNG sequence has alpha 0 at empty pixels and opaque text pixels, ffprobe reports prores yuva444p. Straight alpha, no audio, managed color pipeline must be off (errors otherwise). Named export presets are not done.

## Performance

### What it does

#### D. Infrastructure / performance
- [x] windowed streaming decode (24-frame window), eviction, parallel export, preview cache
- [~] CPU f32 renderer (about 2 fps at 720p for heavy stacks)
- [~] windowed tracking/stabilization shipped; pending: proxy-based preview and cached low-res scrubbing

### Parameters and controls

Use `tools/list` / `list_effects` for live schemas. Read the source parameter names, ranges and defaults alongside their evidence and limits in this module.

### Limits

- [ ] wgpu pipeline: WGSL effect kernels, GPU compositing, frame textures in VRAM, async readback, hardware encode via ffmpeg/NVENC
- [ ] u8/half-res fast path for glow/blur, SIMD, tile-based rendering, dirty-region cache
- [ ] expression evaluator (small interpreter) shared by all params
- [ ] project format v2 with compositions, versioned migrations
- [ ] MCP: batch tool (apply many ops atomically), describe_project summary for LLM context, templates (create_title, create_lower_third, create_trailer_titles, make_montage_from_folder)
- [ ] GUI (deferred by the user)

### Proof level

#### CPU parallelism (std threads, no new dependency) - BETA, measured
`kimo_core::par` splits work by disjoint rows/column strips across `available_parallelism()` threads (override: `KIMO_THREADS`). Every output element is computed by one thread with the same arithmetic as before, so results are bit-identical (FNV hash of all output floats equal to the pre-change build).
Measured on this 2-vCPU box (examples/bench_cpu.rs, examples/bench2.rs, old = build of e9c4762):
- box blur x3 passes, 1080p: 0.53-0.60 s old, 0.12-0.13 s on 1 thread (4.1-4.6x: no per-pixel allocs, cache-friendly vertical pass), 0.06-0.07 s on 2 threads (8.5-9.1x).
- flow_blur 720p: 0.334 s -> 0.201 s (1.7x); optical flow 720p q5: 0.098 -> 0.085 s (1.15x, solver dominated by serial resampling); blur_cov sigma 12: 0.017 -> 0.006 s (2.8x).
- denoise 720p (spatial+1 temporal neighbour): 0.444 s -> 0.381 s on 1 thread (precomputed spatial weights), 0.216 s on 2 threads (2.1x). Temporal pass still serial.
Limit: speedup is capped by core count (2 here). This is CPU only; no GPU code exists yet.

#### Parallel color paths (COLOR) - BETA, measured
Disjoint-pixel threading (shared `par` module) on four color paths: immutable OCIO apply, LUT, linear grade, basic per-pixel ops. COLOR's median 1080p reruns, 8 workers: OCIO 1170 to 608 ms (1.93x), LUT 132 to 57 ms (2.31x), linear grade 145 to 87 ms (1.66x), saturation 6.72 to 5.21 ms (1.29x). All four paths hash-identical before and after at 1 and 8 workers. Extreme inputs fall back to the serial path (preserves old partial-write behavior). Our box has 2 vCPUs, so expect at most about 2x here. Reviewed here: build, color_parallel test (1/2/4/8/16 threads), fuzz 168 / 6516 / 0 failures; COLOR reports full native suite 326/326.

Static text-layer cache (export only, BETA): text clips with constant text (no `{}` tokens), no text animators/fx, no keyframes or expressions on reveal or effect params, no mask/echo/posterize, and only pure-pixel effects (exposure, contrast, saturation, temperature, tint, color_wheels, curves, teal_orange, vignette, blur, glow, invert, sharpen, posterize, sepia, outline, outer_glow, long_shadow, choker, feather_alpha, silhouette_fill) are rendered once per export and rebuilt by pasting a bit-exact non-zero crop (grain and anything seeded or time-varying is never cached; single-frame renders never use it). Measured, same 4 s 1080p30 benchmark: full export 42.2 s (before this session) to 29.9 s (1.41x); output frame-md5-identical to the pre-change export (tests/export_bench.py). Gain is modest because source image copy, background effects and the x264 encode dominate; media/still/solid layers are not cached yet.

- Layer transform parallelism (export/render speed, bit-identical): the scale/rotate/translate resampler splits rows over the `par` threads. Measured on 2 vCPU, 8 scaled+rotated 1280x720 layers: 0.942 s to 0.558 s per frame (1.69x), output PNG md5 identical for KIMO_THREADS=1 and 2. Real film profile showed transform as the dominant stage.

Film finishing speed (COLOR): scoped f32 sRGB transfer math for film_finish
only, decode |input|<=1, encode |linear|<=16; f64 reference fallback outside.
Bloom extraction and main pass share one decoded pixel; coordinates and halo
luma are hoisted. Print_stock hoists stock curve constants and combines two
powers into one exp, still f64. Managed transfer math/defaults unchanged.
Measured, not mathematical all-input bounds: million-sample signed transfer
sweep max decode error2.060e-7, encode5.649e-7; finish extreme signed[-4,4]
parameter grid max7.153e-7, real1080p max2.384e-7. Print_stock real frame and
stock/pivot/light grid[-16,16] are bit-identical after f32 output rounding.
Release isolated real1080p median3 runs: finish1.37-1.47x, print_stock3.25-3.60x faster on
this host. Zero-strength identity, alpha, seed determinism unchanged. Tiny
non-bit-identical finish changes are opt-in effect math, not a global pipeline
speed mode. These samples do not certify all finite inputs or export speed.

- Review cleanups: text.rs non-snake-case warnings removed (pure rename, prompt box output unchanged); add_speed_lines default ray count 70 to 48 (gentler; pass count to restore). Detection-box default softening not done.

- Export presets (BETA): inspect_export_preset and export_preset with web-1080 (.mp4 H.264 CRF18 medium, +faststart, AAC 192k, Rec709 only, fit 1920x1080/1080x1920, no upscale, even dims, lanczos scaling in the encoder), master (.mkv FFV1 16-bit RGB lossless via export_high_bit ffv1_16, whole timeline) and alpha-prores (.mov ProRes 4444 yuva444p10le, transparent canvas). Extension enforced. Verified: web-1080 2560x1440 gives 1920x1080 h264 yuv420p tv bt709, faststart (moov first), decodes clean; fit rules unit-tested (odd, portrait, no upscale); ordinary export frame md5s identical before/after the shared-loop refactor. Limits: alpha-prores has no audio yet and needs the managed color pipeline off; web-1080 audio mux goes through the existing AAC path and moov is re-faststarted by a copy remux; not tested: COLOR's ffprobe tag matrix for alpha, master exact-u16 proof (reuses existing export_high).
- Alpha export BUG FIXED: the first export_alpha frames had premultiplied color at anti-aliased edges (edge pixels about half brightness, measured 122 vs 255 on orange text). The transparent compositor keeps premultiplied rgb; alpha deliveries now un-premultiply. Re-measured: edge rgb 255,128,26 equals the opaque text color for PNG and ProRes. Other blend modes on a fully transparent canvas are not alpha-correct and are untested.

## Sandbox

### What it does

#### Sandbox layer (guard.rs, built into every tool call)
- [x] V1: argument sanitizer (finite numbers, magnitude +-1e8, array/string size, nesting), project limits checked after every mutation with automatic rollback (canvas pixels, fps, clip count, timeline duration), memory budget via counting allocator (default 70% of RAM or cgroup limit; Frame allocations and the render loop fail the call instead of OOMing; hard backstop refuses allocations far above budget), cooperative cancel plus wall-clock time limit per call (default 600 s, export 7200 s) with a last-resort clean server exit if a call cannot be stopped, panics caught with project rollback. Env overrides: KIMO_MEM_MB, KIMO_TIMEOUT_S, KIMO_EXPORT_TIMEOUT_S, KIMO_MAX_PIXELS, KIMO_MAX_FPS, KIMO_MAX_CLIPS, KIMO_MAX_DURATION_S, KIMO_MAX_NUM. Test: tests/sandbox_test.py.

### Parameters and controls

Use `tools/list` / `list_effects` for live schemas. Read the source parameter names, ranges and defaults alongside their evidence and limits in this module.

### Limits

- Gaps (honest): in-process, not OS-level isolation (no seccomp/namespaces/cgroups; a native-code bug that corrupts memory still kills the server); memory accounting covers Rust heap only, not ffmpeg child processes or GPU memory; cancellation is checked per clip/frame and inside several color, scope and process-wait loops, but not every legacy effect or synchronous file operation is interruptible; ffmpeg children get no separate CPU/memory limits yet; file paths are not jailed (tools can read/write any path the user can).

### Proof level

Use the quality-tier audit and test notes in the BETA appendix; checklist marks are not a professional-tool parity claim.

## BETA and honest limits

The audit below preserves checkpoint wording. Read it alongside the module records, including their later additions and corrections, rather than treating any earlier pending item or limitation as a new status update. BETA, BETA+, LIMITED-USE, V1 and V2 labels remain exactly as supplied; no claim is promoted by this reorganization.

### Target, sources and design rule

Target: every useful Premiere Pro editing tool and the full After Effects effect range, each callable as an MCP tool.

Sources: Adobe AE effect list and category pages (helpx.adobe.com/after-effects/desktop/apply-effects-and-animation-presets/...), AE help index (compositing, expressions, 3D, tracking, time remap), Premiere help (nested sequences, color, proxies), CapCut feature pages (see README).

Design rule: one registry in `crates/kimo-core/src/commands.rs` + `edit.rs`; MCP lists straight from it. New feature = new command or effect catalog row, so AI can discover it with `list_effects` / `tools/list`.

### Original checklist grouping

#### A. Premiere Pro: editing

### E. Quality tiers (honest audit)
User bar: every feature best in class. Tiers: **PRO** = matches or beats the pro tool, **V2** = correct maths, missing refinements, **V1** = works, simple maths. Nothing is PRO until verified on real footage against the reference tool's behavior.

| Area | Tier | What is missing for PRO |
|---|---|---|
| Timeline edit tools (ripple/roll/slip/slide/lift/extract) | V2 | linked A/V, track targeting, sync locks, multi-track ripple |
| Color primaries / log wheels / curves / hue curves | V2 | float pipeline is f32 per-pixel and display-referred by default; opt-in scene-linear working space, 2..32-point free curves and explicit gamut mapping shipped; no calibrated camera/display/film pipeline |
| Color space transforms (ACEScct, LogC3, S-Log3, sRGB, Rec.709) | V2 | legacy color_space is transfer-only; color_managed supports primaries/log gamuts. No certified RRT/ODT or production OCIO config import; pinned reference is opt-in |
| Qualifier (HSL key) + Power window | V2 | bezier/per-effect masks and guided qualifier cleanup shipped; mask expansion/choke belongs to the mask stack. No mask tracking binding or universal temporal denoise |
| Scopes (waveform, parade, vectorscope, histogram) | V2 | quantitative working-linear/caller-declared-nits analysis shipped; PNG scopes remain SDR display-domain. No calibrated IRE/HDR display scope or CIE chromaticity |
| Film print, halation, bleach bypass | V1 | no measured print-stock LUT/density curves (Kodak 2383 etc.), halation is box-blur based |
| White balance picker, color match | V1 | full-covariance sRGB/D65 Lab optimal transport with sample regions shipped; no semantic match, mask-selected matching or camera calibration |
| Chroma key (old `chroma_key`) | V1 | superseded by `keyer` |
| Keyer (`keyer`: screen gain+balance, clip black/white, core shrink, edge soften, despill + luma replace, edge unmix, matte view) | V2 | per-pixel screen color variation map (clean plate), garbage matte binding, motion-adaptive matte, hair-detail refinement (guided filter), tested on synthetic screen only |
| Luma key | V1 | |
| Stabilization (two-pass: 24-patch robust similarity fit with outlier rejection, Gaussian path smoothing, lock mode, adaptive zoom, translation+rotation+scale, windowed decode) | V2 | no rolling-shutter correction, no 3D/perspective (homography) model, no inpainting of borders instead of zoom, no tripod/pan-aware modes. Measured: frame-to-frame jitter 9.8 -> 3.5 on a synthetic shaky clip |
| Tracking (ring of 9 patches, translation + rotation + scale, windowed decode, refit from frame 0 each frame) | V2 | this row describes the point/ring tracker; separate planar tracker has homography/reacquisition with limited-use tiers below. No per-patch confidence UI |
| Motion blur (`set_motion_blur`: shutter-angle multi-sample of x/y/scale/rotation, 4-16 samples) | V2 | no per-effect/param blur, forward source optical-flow blur is separate and has occlusion/fast-motion gaps; adjustment layers/text animators not covered |
| Renderer | V1 | alpha-correct bilinear + 2x2..4x4 supersampling on transforms (done); still CPU f32, no mipmaps/Lanczos for large downscales, no GPU |
| Transitions | V2 | 31 kinds, 10 easings, soft angled wipes, clock, split, iris, noise dissolve, dips, light leak, glitch, whip pan, zoom blur. Missing: outgoing-clip push/slide-out, luma-matte wipes from image, GPU-shader transitions |
| Particles | V2 (emitter) | add_emitter: 11 presets (incl. dust, smoke), forces, turbulence, attractor, floor bounce, trails, fade_in, sprites soft/hard/mote/puff/streak/ring/star/spark; no layer collisions, no texture sprites, no fluid sim |
| Text | V2 | kerned layout, word wrap, AE-style char/word/line animators with range selectors (6 shapes, keyframable) + 9 presets, stroke, blurred shadow, glow, gradient, rounded bg box. Missing: per-span rich text, unrestricted variable fonts, emoji; path text and font-weight control shipped |
| Masks | V2 | bezier path-shape keys and expansion shipped; no per-point feather, ML roto or automated mask tracking |
| Audio mixer | V1 | no EQ bands, no loudness normalize, no ducking |

- [x] HUD tools: add_detection_box, add_counter, add_prompt_box (V1)
- [x] shatter transition V2: radial + concentric spiderweb cracks that grow from an impact point, domain-warped irregular shards, impact flash and shockwave ring, shards of the outgoing frame tumble in true 3D perspective with refraction offset, edge glints and fade. Gaps: no depth-of-field blur on near shards, no secondary micro-fragment cascade, no sound. ("shatter_cells" keeps the old Voronoi reveal.)
- [x] 3D layer system V1 (three_d.rs): any layer becomes a perspective plane via rot_x, rot_y, rotation(z), pos_z, x, y, scale, backface; project camera via set_camera / set_camera_keyframe (cam_x/y/z, pan/tilt/roll, fov). Exact inverse perspective mapping with 2x2 supersampling. Shatter shards use the same Projector. Gaps: layers are composited in track order (no depth sort yet), no motion blur on 3D layers, no lights/shadows, no extruded 3D text, no mesh import.


### Tests
`python3 tests/tool_fuzz.py` reads the live MCP registry and exercises non-skipped tools with empty, null, wrong-type, negative, huge and real-id arguments, failing on crash, hang or malformed reply. The explicit skip set is export, export_gif, project_set, render_frame, export_audio, transcribe and download_stock, not every render/export tool. Call count varies with schemas; real-id coverage requires successful fixture setup.


### Fixes from repo issues #1-#7 (all verified)
- #1 add_clip fit: contain | cover | stretch (cover decodes at full res then center-crops).
- #2 shard DOF blur radius capped (10 px at 720) and dispersion fringe clamped, so defocus and glow no longer turn shards into fringed mosaics.
- #3 shard burst px units now scale with the SHORT edge/720 (portrait 1080x1920 gets 1.5x, not 2.67x).
- #4 export uses a work queue across cores (a slow shard frame no longer stalls the batch). Still CPU: real fix is a GPU path. Decode still serial between batches.
- #5 detect_beats / analyze: 11.6 ms resolution, onsets stamped at window centre, strongest-wins min_gap, bpm from autocorrelation. Test on a 140 BPM synthetic kick: first six onsets within 1-2 ms of truth, bpm 139.9.
- #6 add_counter hold default 0.
- #7 add_audio_clip (audio-only, never draws) and a lookahead peak limiter on export (ceiling -1.5 dBFS; tested: +80% gain input exports at -1.5 dBFS peak). Gap: sample-peak limiter, not oversampled true-peak.

- Detection box: default fill_opacity 0.08 to 0.05 (gentler) and the label tag width now uses measured text width (tag fits its text on the 720p check; the old 0.6 em guess could clip proportional fonts). Pass fill_opacity to restore.

- Alpha ProRes color fix (export_preset alpha-prores and export_alpha prores4444): they previously converted RGB to YUV with ffmpeg's default BT601 matrix and wrote no color tags (found by COLOR: Y12 2060 vs expected 2137). Now explicit RGB to BT709 limited-range conversion, colr atom written, and a ProRes bitstream metadata pass so ffprobe reports bt709 / iec61966-2-1 / bt709 / tv. Measured on RGB [0.2,0.6,230/255]: Y12 = 2136 (expected 2137, within 1 code). Input to the encoder is still RGBA8 (8-bit alpha and color precision, not the 16-bit pipe); the output is 10-bit ProRes 4444 (ffprobe reports a 12-bit pix_fmt, the decoder's container for it).

- Static shape cache (export only, bit-exact): constant shape layers (no keyframes/expressions, no mask/echo/posterize, effects from the same safe list as text) render once per export like static text. Verified frame-md5 identical on the film stand-in (10-12 s range). Measured effect: per-frame source-stage CPU 0.07 s to 0.026 s on that project; wall per frame 0.40 to 0.39 s (within noise), so this is a small win. Media/still layers are not cached.
Film-style response (explicit opt-in): film_soft_shoulder uses a per-channel
C1 exponential above knee, identity below; default knee0.75/peak1 maps1 to
0.90803014 and2 to0.99831551. Amount1 approaches peak without hard clipping;
partial amount is not bounded HDR delivery. RGB ratios/hue are not preserved.
film_luma_grain uses display-Rec709 luma and monotonically decreasing grain
occupancy: default shadow0.9/highlight0.08, falloff1.5. Independent seeded
amplitude is zero-mean before clipping, max0.025 default; output is NOT clipped.
Grain is monochrome and block-sized in output pixels, not a scanned film stock.
Generic add_effect exposes both; inspect_film_response exposes exact response.

On managed projects declare native OCIO effect-domain spaces explicitly for a
colorspace sandwich; do not invert an ACES display/view. For scene-linear
luminance/ray compression use existing luminance_shoulder. These new effects
may run after a real forward native display transform in an explicit finishing
workflow; ordinary export/nativeviewvideo is not automatically changed.
Measurements: 100001point shoulder sweep0..10 monotone/bounded, maxf32rounding
2.981e-8 vs f64curve on f32input. Grain512x512 occupancy at luma0.05/0.5/0.95
0.83850/0.36893/0.08911, within0.001 of declaredcurve; mean signednoise<1.44e-5.
Seed-repeat exact, transparent pixels/alpha/zeroamount exactidentity. Grain
sampling error is stochastic evidence, not an all-image occupancy guarantee.
No calibrated stock, ACES/look superiority or display certification claim.

## Gap #3: shape modifiers, layer styles, typed modifiers (all MCP)
- `add_modifier` / `list_modifiers` / `remove_modifier`: wiggle (per-layer derived seed), loop, bounce, overshoot on any property.
- `add_shape_modifier` / `remove_shape_modifier` / `list_shape_modifiers`: trim paths, repeater, offset paths, merge paths, gradient stroke (shape_* effects, keyframable).
- `set_layer_style` / `remove_layer_style` / `list_layer_styles`: inner shadow, bevel, stroke (inside/center/outside).
- Proof: `python3 tests/gap3_proof.py`.
Per-clip grading node DAG (explicit opt-in): grade nodes, normalized weighted
parallel RGB mixers and ordered opacity layer mixers. `input` is the rendered
clip after legacy effects, BEFORE clip masks/transforms. Serial graph matches
same ordered grades exactly. No graph = old path. Output selection is explicit;
parked nodes not rendered, all connections checked for cycles/missing IDs.
First-input alpha is retained; node masks blend RGB against first input. This
is a grading graph, NOT Fusion, shared/group/timeline grades or Resolve mixer
parity. Per-node mask/key coverage and imported tracker keyframes are supported;
tracking coordinates must come from an actual tracker, no node solver claimed.

MCP: inspect_grade_graph, set_grade_graph, add_node, connect_nodes,
disconnect_nodes, remove_grade_node, set_grade_output, set_grade_node,
set_node_param (full Prop animation/expression), set_node_mask,
grade_node_tracking and evaluate_grade_graph_samples (pixels/cache stats).
Complete node/graph JSON exposes strings/domain bindings, bypass, connection
order/weights, mask/path keys, parameters and cache budget; nothing GUI-only.

Node cache is evaluation-local: a shared dependency runs once per frame/eval,
Arc results evicted at last consumer. No persistent stale cross-frame cache.
Max64nodes/16inputs, default256MiB cached-frame budget configurable1..1024MiB;
over-budget fails transactionally, no quality downgrade. This budget measures
cached input/output frames, not effect/mask scratch or decode memory; global
memory guard remains active. Native OCIO per-effect strings work unchanged.
Managed stale-gamut/reference-white bindings checked before timeline render.
Tests: serial/real-frame exact parity, diamond single-evaluation/cache reuse,
parallel/layer weights, masks/keys, nativeOCIO parity, cycles/rollback/JSON
roundtrip, deterministic repeat and budget failures. No speed/hue/calibration
superiority claim.

## Gap #4: captions, audio effects, loudness, multicam (all MCP, BETA)
15 MCP tools: captions (add/list/style/edit/burn-in), audio (effects incl. Schroeder comb/allpass reverb, pan, loudness measure/normalize, ducking), multicam (sync/add/switch/list). Details and the tool list: docs/EDITORIAL_MCP.md. Measured: LUFS within 0.15 LU of FFmpeg loudnorm; audio sync recovers a 350 ms offset on real PCM.
Honest limits: sync is 5 ms RMS correlation, not sample-accurate; timecode labels are non-drop only; multicam uses the common overlap; pan is stereo balance; loudness uses sample peak, not true peak.

## Shared light and material model (BETA, all MCP)
Project-level lights (directional/point/spot, colour, intensity, falloff, cone, time keys) and named PBR materials (albedo, roughness, metal, ior, clearcoat, emissive; presets plastic, rubber, brushed_metal, chrome, gold, glass, concrete, wet_stone, car_paint, skin). Tools: set_light, remove_light, list_lights, set_material, remove_material, list_materials, set_relight, preview_material. First consumer: the `relight` effect, which builds a normal field from alpha and/or luma height and shades with GGX + Smith + Schlick, a clearcoat lobe and soft height-field self shadows, on any layer. Proof: tests/lighting_proof.py (moving the light moves the highlight, bad inputs rejected). Default render paths are unchanged (film frame md5s identical).
Honest limits: 2D layers only (a height-field relief, not real geometry); no environment/IBL reflections, so chrome and glass show only light highlights; no light-to-light shadows or bounce; self shadows are a 10-step height march; fracture_glass and shards do not read the shared lights yet (next step); sRGB-encoded values are shaded as given, not in linear light.
Adjust-stage CPU follow-up: film_finish caches the exact seeded grain lattice
and column interpolation terms once per frame. Color halo blur processes vertical
columns in64-column cache-friendly blocks, preserving each column's original
f64 running-sum update order. No math/transfer approximation added. Release
real1080p isolated median5: finish0.5443s->0.3730s (1.46x), exact float32bytes.
Prior concurrent run1.43x; host/export contention may differ. Small/signed/fractional
grain/seed grid and blocked-blur radius1..512 checks exact. Grain lattice adds
at most roughly one4bytevalue/pixel atgrain_size1 plus row/column terms.
Print_stock measured~0.112s/frame but unchanged in this follow-up; prior constants/
projection optimization already landed. No GPU/threading/default/output change.

- load_project no longer resets the id counter to 1000: it is set one past the largest id-like number in the loaded project (never below 1000), so resumed projects cannot overwrite existing clips/assets. Proof: tests/load_ids_proof.py.

## Camera solve, lens distortion, classical matting (SOLVE agent, BETA, all MCP)
Camera solve: KLT feature tracks, 2-view init, PnP, bundle adjustment (src sfm.rs, ext camera_solve.rs), with optional radial distortion k1/k2 and a rotation-only pan mode. refine_matte: classical (non-ML) trimap alpha matting.
Measured (author): synthetic solves 0.03-0.11 deg rotation error; real H.264 end-to-end 48/48 frames, 0.70 px RMS; distortion model takes rotation error 2.5 to 0.77 deg on a k1=-0.15 scene; pan mode on real video 48/48 frames, 0.012 deg mean error, focal 560.2 vs true 560 (examples/camera_solve_proof.py PAN=1); matting MAE x1000 14.04 to 6.67 on a synthetic hair test, 10.40 to 6.70 on real H.264 (compression noise limits it; examples/refine_matte_proof.py).
Honest limits: distortion is off by default when focal is unknown (k1 and focal are coupled); 0.77 deg is typical, not guaranteed (RANSAC sensitivity); no undistort tool, k1/k2 are reported only; matting needs a local fg/bg colour difference and is not ML; solves live in session memory only. I ran sfm_synthetic (1) and matting_synthetic (6) tests, film md5s unchanged and fuzz 229 tools / 8253 calls / 0 failures; I did not rerun the real-video proofs.

## Bezier paths (MCP)
- `add_path_shape` (SVG `d` or points+smooth), `set_path`, `path_info`. Trim uses exact arc length; offset, merge (path with path), repeater, gradient stroke, even-odd fill all work on paths.
- Proof: `python3 tests/bezier_paths_proof.py`.

- fracture_glass `scene_lights` (default 0, off = unchanged pixels) and `light_gain`: shades shard faces with the shared project lights (set_light). GGX specular on each shard's real 3D tumble normal with per-shard roughness 0.04-0.26, so highlights flash per shard with angle instead of a procedural sin-wave sheen. Needs at least one enabled light or it errors. Honest limits: specular only (no diffuse/thickness absorption), difference from the legacy look is modest at gentle light settings, no light-blocked shadows, per-shard thickness variation not done.
Node grading stage2: shared graph definitions, ordered clip refs, ordered
membership-based grade groups and a final timeline grade chain. Adjustment
layers can use local/shared graphs and keep their existing mask/opacity/duration.
Order: legacyeffects -> localgraph -> clipsharedrefs -> groups -> clipmask/
transform/composite -> timelinegraphs -> outputtransform. Clip/group clocks are
clip-local; timeline uses compositiontime. Nested/precomp/matte subrenders do
NOT apply the parent timeline chain again. Groups grade each member before
compositing, not a flattened grouped image. No inferred domains/conversion.

MCP: inspect_grade_scopes, set_grade_scopes (complete registry/membership/order/
bypass/timelineJSON), create_shared_grade_graph, bind_shared_grade_graphs,
remove_shared_grade_graph. All12node tools accept shared_graph_id instead of
clip_id, so every shared node/edge/param/key/mask/cache field stays editable.
Shareddefinition edits update all references. Results are NOT pixelcached across
clips. Referenced deletion errors until refs cleared; split_clip/split_all_at/
insert_gap copy cliprefs/group membership to the right segment. Other editorial
cut operations may create fresh unbound clip IDs: explicitly bind those IDs.
No-scope path preserves pixels; shared/group/timeline fixtures exact vs explicit
serialreference and deterministic. NativeOCIO bindings remain explicit. Stage2
is not Resolve shared-node/remote-grade semantic parity or a Fusion compositor.
## Path arcs and path morph keys (MCP)
- SVG `A`/`a` arcs in paths (endpoint to center conversion, radius scaling, glued flags).
- `add_path_key` / `clear_path_keys`: keyframe the whole path (per-point animation) with ease; keys must share the base path structure.
- Proof: `python3 tests/path_arcs_morph_proof.py`.

- Exact frame clock: media frame index now has a 1e-6 frame tolerance (media_index in render.rs). Before, times like 29/25 computed 28.99999999999996 and showed the previous source frame (a 1-frame plate lag against keyframed pins). Proof: tests/frame_clock_proof.py (98 exact frame times equal their epsilon renders); the film agent's phase_repro also reads exact_equals_previous false at 29/57/58. Not changed: speed-curve clips already had the tolerance; confetti/speedlines flicker steps use their own floor.

## Editorial follow-up (captions agent, BETA)
export_with_subtitles (selectable mov_text/SubRip streams next to burn-in, atomic output, range-relative cue times); measure_loudness true_peak=true (4x sinc reconstruction estimate, NOT certified, within 0.3 dB of FFmpeg ebur128 per the author); sync_clips drop_frame=true (29.97/59.94 DF labels, skipped labels rejected). Details in docs/EDITORIAL_MCP.md. I ran their MCP proofs (MP4/MKV streams, true peak, DF shift) and fuzz (240 tools / 8663 calls / 0 failures); the 0.3 dB figure is the author's.

Grade node widening: .cube LUT, blur and sharpen admitted. Existing RGB/point
curves, hue-vs-hue/sat/lum and log wheels already worked as grade nodes; added
exact direct-effect parity tests. File LUT uses strings.path; generated/persisted
LUT uses strings.cube_text (exactly one), intensity keyframable. generate_lut_node
bakes explicit pointwise/native OCIO chains to inline17/33 cubes, reports sampled
max/p99/RMS interpolation error, supports clip/shared targets. It does NOT auto
select output, infer domains or claim nonlinear LUTs lossless. Generated LUT
persisted with project, clamps input outside declared domain. Inline data parsed
each eval; file cache existing size/mtime bounded8entries.

Spatial nodes execute on each branch's full canvas in stated order, never moved
across mixers/qualifiers/curves. Blur/sharpen are NOT generally commutative with
nonlinear grades. Node RGB-only semantics preserve first-input alpha exactly;
this differs from alpha-blurring clip blur. Node masks blend after spatial op,
not before. Radius0..512, sharpenamount0..3 evaluated-time guards; effect scratch
still under globalmemory budget, not included in node cachedframecap.

## validate_planar_track (BETA, read-only diagnostic)
Re-tracks a plane from N independent anchor frames (each template built from the base track's own quad at that frame) and reports per-frame, per-corner px disagreement vs the base track (p50/p95/max, first flagged frame, full per-frame arrays for export). Measured on the licensed city plate (Mixkit 49844, 4 s, SHOWCASE fixture): base track confidence min 0.73 / mean 0.85, zero lost frames, yet anchored re-tracks disagree on the target quad (propagated from the tracked context quad) 0.9 px at frame 30, 3.0 at 40, 17.6 at 60, 46.9 at 90 (p50 17.3, p95 49.2, max 55.0; 56 of 75 tested frames over 3 px; 14.7 s for 100 frames). Frames before the first anchor (here 0-24) are reported as untested (tested=false, null distance), not as passes. Distances are on the target quad; context_max_px is reported separately. So a high-confidence track is flagged as unreliable. Usage: tests/validate_track_real_plate.py <plate.mp4>.
Honest limits: disagreement proves unreliability but does not say which track is right; agreement does NOT prove pixel lock (all anchors can share the same wrong, identical window); only forward re-tracks, no reverse or line-convergence constraint yet; this is a detector, not a better tracker. Constrained feature identities, hold-out patches and line constraints are not built.

## Track diagnostics (DIAG agent, BETA, read-only analysis, session-only storage)
11 MCP tools: track_features (raw tracks with train/heldout roles), fit_track_homography (soft-L1 LM, held-out residuals), decompose, jitter metrics, corner_disagreement, track_feature_residuals, lk_residual_audit, ecc_refine_track, render_error_field. See docs/TRACK_DIAGNOSTICS.md. Proof: tests/track_diag_proof.py on a synthetic plate with known corners (fit max corner error 0.28 px, ECC residual 0.018 px; I re-ran it on a freshly generated plate). Not run on the real city plate yet; features and fits live in session memory only. Run tests/track_diag_gen_plate.py from a scratch directory (it writes fr/, gt.json, plate.mp4 into the cwd).

- refine_matte rework (SOLVE): auto_bg (background colour model with flood-fill reclaim), exclude/include stroke polygons, bg_bounds colour-distance bounds (tool default true; core default off, set false on busy or similar-coloured backgrounds), foreground decontamination F=(C-(1-a)B)/a. Synthetic hair test (re-run by me): MAE x1000 14.04 to 6.67, partial-alpha error 0.398 to 0.094, passes. Wind-in-hair plate: visual judgment only, ~15k px reclaimed. Known flaw: ragged sleeve edges where shadow is near the wall colour.
- validate_planar_track `backward=true` (opt-in, about 6x slower: 94 s vs 14 s on 100 frames): also tracks backward in time from the last frame and each anchor (new planar::track_frames takes an explicit frame order; forward behaviour unchanged) and reports backward_max_target_corner_px separately. On the city plate the backward tracks started from the base track's end quad disagree 99 px at frame 0 falling to 32 px at frame 90: they inherit the base track's end-frame error, so this column is a stress signal, not a measurement of truth. Still no line-convergence or hold-out constraint.

- undistort_clip (SOLVE): removes radial Brown k1/k2 distortion from a clip (k1/k2 from a camera solve or explicit, H.264 crf 14 output, optional asset import). Measured by me via tests/undistort_proof.py on a synthetic k1 -0.15 / k2 0.02 clip: MAE vs ideal 0.0097 after undistort vs 0.0452 distorted (4.7x), coverage 1.0. Limits: the cargo round-trip test (undistort_roundtrip) was never compiled by the author and I could not complete it within my time cap either, so the MCP proof stands in; black-edge loss not measured separately; radial only (no tangential/p1p2).

- refine_matte chroma-weighted background distance (SOLVE; chroma 2.5x luminance, in the auto_bg/bg_bounds tool path only): cuff streaks reduced, hair halo cleaner; right sleeve/jacket edge still ragged, a partial fix. Synthetic matting test re-run by me: passes.

- Contract fixes: add_text `x` now applies without `anchor` (it was silently ignored unless anchor was also given); add_text color and add_shape fill/stroke accept [r,g,b,a]. Fill/text alpha multiplies the layer opacity; stroke alpha is not supported and add_shape returns a warning. Proof: tests/contract_fixes_proof.py (x=150 centres at 471 px, half-alpha red fill reads 128).

- track_planar_anchored (BETA): keyframe-anchored planar tracking. Verified quads at >=2 frames; each gap is tracked forward from the left anchor and backward from the right, blended with smoothstep, so the result passes exactly through every anchor and drift cannot exceed one gap. Reports per-gap one-sided closure error and flags gaps that need another anchor. Measured on the synthetic plate with known corners (tests/track_anchored_proof.py): unanchored max error 33.85 px / mean 14.19; 2 anchors 3.61 / 1.87; 3 anchors 2.91 / 1.22; 4 anchors 2.51 / 0.94. Limits: anchors are trusted as given, so it does not pick the right window on a repeated facade, the anchors do; not yet checked against real-plate truth; slower than a plain track (about 11 s for 40 frames at 640x400 with 2 anchors, because the backward leg often runs the coarse re-acquisition). Line-convergence constraints and constrained feature identities are NOT built.
- fracture_glass `realism` (0..1, default 0 = legacy pixel-identical): scales cell count up (x1+12r, cap 2500) and debris, per-shard drag/terminal-velocity gravity/flutter, size-dependent push, crack-arrival release timing with heavy tail, refractive see-through back faces, per-edge lit rims, soft contact shadows, tapered finer cracks. Set via add_effect/set_effect_param.
- fracture_glass `realism` honest note: published from the SHAPE track; realism=0 gated to the legacy brute-force path (default and 0 render identically on the city plate). Author caveats: a few large foreground slabs stay near camera in late frames; no hairline stress cracks. Fuzz 255 tools / 9359 calls / 0 failures.
- refine_matte (fix): bg_bounds and the thin-streak alpha closing now self-gate per frame on background roughness (bg_rms < 0.10, reported per frame with bounds_used). The synthetic hair proof regressed from 16.57 to 30.40 MAE x1000 with bounds on a textured background and is now 6.70 vs 10.40 for the rough seed. Real wind-in-hair plate (bg_rms 0.067) keeps bounds on and the cleaner wall-halo result. Limit: the 0.10 threshold is calibrated on two clips only; use bg_bounds=false on busy backgrounds. Sleeve/jacket errors from cream-on-pink shadow remain; use include/exclude strokes.
- fracture_glass `core_damp` (0..1, default 0 = pixel-identical, checked by md5 on 3 frames against the previous build) and `core_radius` (fraction of frame height, default 0.16): shards near the impact get lower launch speed and extra angular scatter, fading out by core_radius. Effect is modest: at 0.8 the centre keeps denser debris at 0.7-0.9 s after impact, but the large dark hole is not reduced because it is the backdrop revealed once shards leave, not shard density. The funnel/starburst look is only partly addressed; shard collisions and per-shard thickness are still not done.
- Particle emitter motion blur (BETA): add_emitter config `shutter_deg` (0..720, default 0 = off) and `blur_samples` (0 = auto). The system is integrated at several instants across the shutter interval centred on the frame time and the premultiplied buffers are averaged, i.e. the temporal integral of every sprite, force and speed, not a fake streak. Auto samples = fastest-particle travel over the shutter / (0.6 x smallest sprite), clamped 4..32 so there is no visible stepping. Measured at 1280x720, 300 particles/s, speed 1400: render 0.11 s off, 0.18 s at 180 degrees, 0.20 s at 360. Default off; the off path runs the same code as before but I did not md5-compare it against the previous binary. Limits: samples cost linearly; blur is per emitter, not per particle; fracture_glass and shard bursts already had their own shutter, scene3d still has none.
- 3D scene motion blur (BETA): add_scene3d config `shutter_deg` (0..720, default 0 = off, identical code path to before) and `blur_samples` (0 = 8, max 32). The whole scene is re-posed and re-rendered at several instants across the shutter and averaged in premultiplied space, so object animation, parenting and the project camera move all blur. Measured on a 1280x720 rbox scene: 0.19 s/frame off, 0.87 s at 8 samples (cost is linear in samples). Limits: it is sampling, not analytic, so a very fast move at 8 samples shows stepped copies at the shutter ends (raise blur_samples); no per-object shutter, one shutter for the whole layer; no rolling shutter.
