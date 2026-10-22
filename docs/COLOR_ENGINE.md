# Color engine: measured status

Status reconciled against engine checkpoint `dd9a517` on 2026-10-11. Per-feature timings, hashes and test counts below are historical checkpoint evidence, not current benchmarks. Sections record changes in publication order; statements about an earlier gap apply at that checkpoint unless listed as a current limit. Later sections supersede earlier implementation details.

## Explicit managed transform (V2, not Resolve superiority)
`add_effect kind=color_managed` converts RGB primaries through derived RGB/XYZ matrices, Bradford white adaptation, and explicit transfer functions. Gamut IDs: Rec709=0, Rec2020=1, P3D65=2, ACES AP1=3, AP0=4. Transfers: linear=0, sRGB=1, gamma2.4=2, ACEScct=3, LogC3 EI800=4, SLog3=5, PQ=6, HLG scene=7, BT709 OETF=8.

Linear values and negatives are preserved until an explicitly selected output compression. PQ uses `reference_white` nits per linear 1.0 (default 100). HLG is the scene OETF inverse, NOT a display EOTF/OOTF. Use `tone_map=1` for luminance Reinhard and `gamut_compress=1` for constant-luminance chroma compression into 0..1. Neither is an ACES RRT/ODT or measured display rendering transform. ACEScct + AP1 is a supported explicit encoding, not a complete ACES pipeline. The enabled project pipeline stores scene-linear working RGB (default AP1), not ACEScct samples.

Tests: 25 gamut-pair inverses/neutral adaptation within 2e-12, PQ 100-nit reference/10,000-nit endpoint and 999 samples roundtrip <1e-12, HLG roundtrip <3e-8, negative scene preservation, compression luminance preservation, hostile/nonfinite parameters. These are math tests, not camera/chart certification. Render proof: 48-frame 640x360 HDR gradient, 2.239 s wall-clock including encoding. Synthetic only.

Sources used for constants:
- https://docs.acescentral.com/encodings/acescg/
- https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.2100-2-201807-S!!PDF-E.pdf

## Free-position curves (V2)
`set_color_curve clip_id channel points`: 2..32 increasing [x,y] pairs, channel m/r/g/b. Shape-preserving PCHIP, no monotone segment overshoot; endpoint linear extrapolation rather than input clipping. Saved as normal effect params and keyframable with existing MCP tools. `add_effect kind=point_curves` permits combined master/RGB curves with channel_count and channel_xN/channel_yN params.

Tests: nonuniform identity including negative/high values, 10,001 monotone samples, invalid/duplicate/nonfinite points. Proof: same 48-frame gradient with custom master curve, 1.448 s wall-clock. No claim of visual superiority from a gradient.

## Remaining
Measured film stock, camera-specific IDT/profile calibration, semantic/subject-aware shot matching, production OCIO integration and display-referred HLG OOTF remain out of scope. Opt-in float working decode/compositing, high-bit IO, full-covariance Lab matching, declared HDR scopes/metadata and optional pinned ACES2 reference paths are implemented below, with their individual caveats. Legacy effects remain display-referred. Existing color_space is transfer-only for compatibility.

## LUT precision/safety (V2)
The existing `lut` effect now uses domain-aware tetrahedral interpolation. Accepts 2..65-cube grids, DOMAIN_MIN/MAX and finite rows; rejects 1-point grids, invalid dimensions, nonfinite data and inconsistent rows. File cap 32 MiB, cache cap 8 entries, cache key includes size and mtime (avoids stale replacement). Existing old unbounded parser removed. Three unit tests cover domain identity over all tetrahedra (1000 samples, error <3e-7), vertices and malformed data. Proof: 48-frame 640x360 synthetic 33-cube grade, 0.607 s including encode.

## Quantitative scopes
`analyze_color` returns float RGB/luma range statistics, under/over-range counts, 16..4096-bin histograms, up-to-256-column waveform density, 128x128 BT709 CbCr density. No implicit HDR/nits interpretation. The render output's color state is the caller's responsibility, and these scope data describe that render, not unmodified source media. Alpha-zero samples ignored. Two unit tests cover range accounting and hostile/empty inputs. 230,400-pixel proof analyzed in 0.0314 s. Existing PNG scopes remain display-referred/unlabelled, not pro-scopes parity.

40 targeted real-clip hostile color calls passed without server death, including render-time bad LUT sizes/NaN and extreme curve counts/gamut ids. Generic fuzz prior: 3190 calls, zero crashes; initial media fixture setup failed, so those generic cases did not exercise real media ids. Reproducer: examples/color_engine_proof.py.

## Perceptual full-covariance shot matching (V2/BETA)
`match_grade` samples source/reference frames (optional normalized [x,y,w,h] regions) and builds a symmetric Gaussian optimal-transport matrix in normalized CIELAB. The frame must be sRGB/D65 display-referred. Uses all nine covariance terms rather than independent per-channel scales. Regularization controls flat/low-rank regions. Adds `lab_match` with existing parameter/keyframe persistence.

Three tests: 1000 Lab roundtrips <2e-12, full covariance transport <1e-10, flat covariance identity and invalid regularization. Synthetic warm/cool gradient test: 230400 samples/shot, match computed in about 0.12 s, 48-frame output about 2.52 s including encode. See examples/color_match_proof.py for render-space Delta E76 measurement. This proves this specific synthetic transfer, not semantic shot match, matching different subjects, camera accuracy, or Resolve superiority. Whole-scene statistics can contaminate the match with black canvas/background; select representative regions for differently framed shots. No skin/subject segmentation or temporal smoothing.

Lab equations/constants source: http://www.brucelindbloom.com/Eqn_Lab_to_XYZ.html

## Portable pointwise grade (V2)
`cdl` supports clamped slope/offset/power and weighted saturation (caller chooses input encoding). `export_grade_lut` freezes supported pointwise effects into a 17/33/65 grid .cube; rejects spatial effects and per-effect mask bindings rather than silently dropping them. Clip transforms, timeline masks and source pixels are intentionally not part of the exported grade. Includes DOMAIN_MIN/MAX for bounded extended domains. No CDL XML import/export or certified ASC conformance.

Synthetic 33-cube CDL roundtrip: bake 0.0122 s (35937 entries), 48-frame 640x360 export 0.602 s; 8-bit direct-vs-baked RGBA mean absolute error 0.00278 code values, max 1 code value. Unit tests cover known SOP result and invalid params. No measured film stock LUT included.

Synthetic Lab transfer: render-space mean Delta E76 against same-layout reference improved 27.1945 -> 0.8418. This is a warm/cool gradient pair, not different real scenes and not a perceptual study.

## Real-footage validation and sampling fix
On SCUMBLE footage (two-second segment, 640x360), added deliberate CDL cast/contrast/saturation distortion then matched the same reference frame. A floating-point frame-boundary bug in existing clip isolation sampled shifted clips one frame earlier (`3.8-3.0` underflow at 25fps), causing the first test to match different fast-cut frames and worsen Delta E76. Fixed color-tool isolation by zero-based clock + frame-boundary snap; out-of-clip sample times now fail. Corrected mean Delta E76 8.596 -> 2.447 (71.5% lower). 48-frame export 3.758 s; sampling+matching 0.472 s. This is known-distortion recovery, NOT different-shot creative matching or reference-tool parity.

Qualifier adds edge-aware guided matte refine, black/white cleanup and matte gamma, with bounded radii. SCUMBLE hue qualifier proof: 48 frames 640x360, 11.450 s; hue keys matching colors across the scene, not a semantic object/skin selector. Matte inspected. General color effects now reject nonfinite output rather than passing corrupt pixels into encode.

## Color blur hot-path fix
Replaced color qualifier/halation use of the generic per-pixel-allocation box blur with a color-only separable RGB cascade. Correct clamp extension, f64 running sums, bounded radius, one scratch allocation, alpha preserved. Constant-border and alpha invariance tests pass. Same actual SCUMBLE 50-frame qualifier export fell from 11.450 s to 2.868 s on this workspace (about 4x faster end-to-end, includes decode/encode). Recovery export 3.635 s, Delta E76 unchanged. Prior documents called the 2-second/25fps real proof 48 frames; corrected count is 50 frames. Synthetic 24fps proofs are 48 frames.

## Scene-linear grade (V2)
`linear_grade`: exposure/gains/offset, signed pivoted power contrast and gamut-aware luminance saturation without intermediate clipping. Explicit chain: sRGB Rec709 -> linear AP1 via color_managed, linear_grade gamut=3, linear AP1 -> display sRGB Rec709 with optional output gamut compression. Existing display grading is unchanged. Managed tone-map/compression now uses destination gamut's luminance coefficients (was always 709).

Tests: extended-negative/HDR identity, radiometric doubling for +1 stop, luminance preservation across all five gamuts. Real 50-frame 640x360 SCUMBLE proof: 2.824 s including decode/audio/encode. 33-cube portable bake of 3-effect chain 0.0483 s. Visually checked source/grade and clip. Historical proof used 8-bit source decode before the later opt-in project-managed/native-input path. Adding this explicit effect alone does NOT switch the compositor or export to a scene-linear/10-bit workflow; `set_color_pipeline` and `export_high_bit` select those newer paths explicitly.

Final generic MCP fuzz after adding an actual image fixture: 129 tools, 5283 calls, zero failures, with real clip/asset ids. Added a Rust test that renders every catalog color effect at extreme params and requires finite output or explicit error. Color unit/regression count: 26 (plus one existing unrelated engine test).

## Final edge checks
Sampling retains the original timeline clock (important for dynamic text and expressions), while snapping only floating frame-boundary errors. Real-footage recovery result unchanged after this refinement. Added transparent-region rejection, masked/spatial LUT rejection, and serialized grade roundtrip tests. Color/regression test count now 29. Tested real-footage export repeated at 3.735 s recovery / 2.995 s refined qualifier, stable Delta E76 2.447.

## Camera gamut inputs (V2)
Added ARRI Wide Gamut 3=5, Sony S-Gamut3=6, S-Gamut3.Cine=7 to managed/linear gamut IDs. Pair WG3 with LogC3 transfer 4 (EI800 only), Sony gamuts with SLog3 transfer 5. 64 gamut-pair neutral/inverse tests pass; camera log scene roundtrips <3e-6 f32. Real footage encoded into WG3/LogC3 then decoded back: 50-frame 640x360 export 2.632 s, 8-bit still comparison exactly identical (MAE=0). This tests encoding reversibility, NOT an actual camera recording or camera characterization. Camera input matrix sources:
- https://colour.readthedocs.io/en/stable/generated/colour.models.RGB_COLOURSPACE_ARRI_WIDE_GAMUT_3.html
- https://antlerpost.com/colour-spaces/SLog3.html

## Opt-in project color pipeline (BETA, shared renderer upgrade)
`set_color_pipeline settings`: enabled=false by default, so old project JSON/tools retain legacy decode, display-referred compositing and output. Enabled projects decode media through 16-bit RGBA, store f32 samples, convert media input to chosen scene-linear working gamut, convert procedural sRGB/Rec709 graphics, and composite in f32 linear. `set_clip_input_color` overrides source encoding for individual media clips. Precomps/matte rendering carry project settings. `export` and `render_frame` apply output conversion; HDR legacy 8-bit export is rejected in favor of `export_high_bit`.

`export_high_bit`: hevc10 = 10-bit HEVC, or ffv1_16 = lossless 16-bit RGB MKV. Explicit Rec2020/PQ output writes BT2020/PQ/container tags and uses BT2020 RGB-to-YUV matrix; HLG tagged similarly. PQ reference_white controls nits scaling. Caller-declared mastering-display/MaxCLL metadata is optional for Rec2020/PQ HEVC10 via `set_hdr_mastering`; never inferred. No dynamic metadata, official RRT/ODT, or HDR certification. FFV1 preserves 16-bit RGB, not alpha. PCM audio kept in MKV; AAC in MP4.

Real SCUMBLE 50-frame 640x360 pipeline proof: 16-bit FFV1 about 3.3 s, tagged PQ 10-bit HEVC 5.33 s. Metadata verified by ffprobe: gbrp16le/bits_per_raw_sample=16 and yuv420p10le/bt2020nc/smpte2084/bt2020. SDR preview inspected. Legacy-vs-high-precision decode differs ~1.44 8-bit levels mean (max40 on sharp edges) because ffmpeg 8/16-bit resize/color conversion differs. Legacy default is untouched.

Precision fixture: 1024-level 16-bit PNG decoded, composited and exported to FFV1 16-bit; all 1024 distinct levels retained. See examples/color_precision_proof.py. Pipeline tests cover absent-setting legacy default, invalid settings, 16-bit quantization and physically correct 50% white/black composite (linear .5 -> sRGB .735357). Full tool fuzz with real ids: 134 tools, 5425 calls, zero failures.

`keyer` matte_model 1 (Smith & Blinn linear colour difference, trimap + projection edges) takes a clean plate (strings.ref_clip clip or strings.aux_path image, per-pixel screen colour), garbage/core mattes (strings.garbage_mask/core_mask = mask layer ids, mode none), despill_mode average/double/limit/min with despill_bias and replace colour, and view 3 status (black/grey/white, yellow = opaque but despilled) / 4 raw screen matte; matte_model 0 is the legacy keyer unchanged.

`pick_key_color` sets a keyer's (or chroma_key's) r,g,b to the per-channel median of a region (default top-left 15 %) of the clip rendered without its keyers, adding a keyer if the clip has none. `chroma_key` space 1 keys on Rec.709 CbCr distance (luma ignored, so a shaded screen still keys); space 0 (default) is the old RGB distance.

`auto_color clip_id [time] [strength] [percentile] [stretch] [mids]` is Auto Balance (Resolve's "A"): per channel it takes the robust black and white points (percentile clipped at each end, default 0.5 %) of the isolated clip at 5 frames across it (or one `time`), maps them to their common level (their Rec.709 luma, so brightness is kept; `stretch` maps to 0 and 1), then neutralises mids with a per-channel gamma fitted to near-neutral pixels (luma 0.15..0.85, |CbCr| < 0.05 after the black/white map, gamma clamped 0.8..1.25, skipped below 1 % of pixels). The result is one primaries model-1 effect (lift = intercept, gain = slope + lift, gamma), so `set_color_wheels effect_id` edits it; strength 0 is an exact identity. Display-referred only; a flat frame (channel range < 0.02) is refused. It assumes the scene's extremes and near-greys should be neutral, so a deliberately tinted scene (sunset, neon) is pulled toward grey: use strength or match_grade there.

Limits: legacy color/HSL/keyers/film effects assume display encoding; explicitly wrap them with color_managed transforms or use linear_grade. Managed source optical-flow blur uses float working-linear neighbours; its tracking-quality gaps remain (see below). Lab match_grade and portable clip-grade bake require legacy/display project (avoid silently treating linear data as sRGB). No gamut-aware 3D physical light calibration or per-media metadata auto-detection; caller explicitly names input encoding. High-bit export currently serial (bounded memory, no GPU), and PNG preview is 8-bit SDR only when output configured SDR. The managed decode cache keeps float windows and creates a quantized compatibility copy lazily for legacy tracking APIs; managed decoding uses more memory than legacy.

### High-bit still / honest preview
`render_high_bit_still` writes configured encoded samples to true 16-bit RGBA PNG (rgba64be). Precision fixture keeps all 1024 input levels; 0.097 s, metadata checked. Ordinary 8-bit `render_frame` PNG refuses PQ/HLG project output so a raw PQ image is not mistaken for a good-looking SDR preview. Select SDR output for a display preview; use the high-bit still when you need encoded values. FFV1 retains RGB matrix tagging (gbr), while HEVC uses BT2020 YUV conversion. HLG output here is an OETF encoding; no display OOTF adaptation based on target peak luminance is implemented.

### Per-effect encoding wrapper
`set_effect_encoding` wraps a display-referred effect with working-linear -> requested gamut/transfer -> effect -> working-linear. Explicit only, no guessed color state. Changing working gamut or disabling the project makes old wrappers fail until reconfigured/cleared. Portable LUT bake rejects wrappers because it has no working-project context. Real 50-frame 640x360 SCUMBLE film_print in sRGB wrapper: 3.829 s including audio/encode; clip inspected. This improves workflow safety, not film-stock accuracy. Identity sandwich test <2e-7 f32; invalid settings/working-gamut changes rejected. No automatic pipeline-wide legacy-effect migration.

### Float optical-flow source neighbours
Managed flow_blur now reads both source frames from the high-precision cache, converts both to working linear before flow/radiance accumulation, and avoids a second decode transform. Managed projects no longer reject it. Quality/shutter bounded. Actual SCUMBLE 25-frame320x180 proof: 0.845s, inspected. Constant HDR/negative radiance invariance unit test passes. This fixes color precision, not optical-flow tracking quality: coarse Lucas-Kanade still has aperture/fast-motion gaps and no cut detector. The default smear mode also has occlusion halos; `set_flow_blur mode=vector` handles static-background occlusion (per-pixel static/moving test, central prev/next flow, forward splat over a revealed-background plate). Its vector neighbours (prev, cur, next) go through the same working-linear input conversion.

### Caller-declared static HDR metadata
`set_hdr_mastering` validates physical xy primaries/white, increasing min/max nits (max10000), MaxFALL<=MaxCLL and content-light bounds. Only enabled Rec2020/PQ HEVC10 accepts it; defaults remain absent, never invented. Explicit metadata becomes x265 mastering-display/content-light SEI, retained through audio mux. Synthetic declaration on actual SCUMBLE25-frame320x180 output:1.028s. ffprobe confirmed exact rational primaries/white, min50/10000=0.005nits, max10000000/10000=1000nits, MaxCLL1000/MaxFALL400. These are test fixture declarations, NOT measurements of this SDR-origin footage or actual mastering conditions. Two unit tests cover units/hostile declarations. Clearing is supported. Dynamic HDR metadata and certification remain absent.
Source for x265 units/syntax: https://mailman.videolan.org/pipermail/x265-devel/2017-February/010894.html

### Bounded high-precision cache
Managed decode now stores only f32 sample windows; the compatibility u8 tracking buffer is created once on demand, not for every render. Unit test proves no quantization allocation until get()/tracking request. At640x360,24frames: decoded cache110592000->88473600bytes (20% less before tracking copy). This is cache-byte accounting, not whole-process RSS. Window cap is min(256MiB,one-third sandbox memory); decode allocations preflight, windows shrink at high resolution, active-cache excess fails explicitly rather than OOM. At this checkpoint ramps requested whole-clip ranges; current spot/export preparation windows source ranges (see the later ramp sections). Budget reserves potential lazy compatibility copy. Memory for ffmpeg children still outside Rust accounting.

Real50-frame640x360 repeat: RGB16 export3.087s, PQ10export5.347s; inspected SDR comparison unchanged. Precision1024-level fixture retains1024 levels/maxerror0, export0.095s.137tools5499hostilecalls0failures, suite pass. At this checkpoint, very long ramps needed proxy/windowed retime. Current spot/export preparation uses bounded ramp windows; oversized whole-project preparation and required source spans can still fail explicitly.

## True float interchange and project-aware scopes
Managed EXR/PFM sources decode at native dimensions to float32 GBR(A), then use the engine's alpha-correct area fit for shrinking and bilinear fit for enlargement. This bypasses an observed swscale float-resize clipping boundary: native EXR retained -0.25..4, but ffmpeg scale to 32x32 clipped to 0..1. Integer video/PNG paths remain RGBA16 for exact existing precision (1024-level proof stays max code error 0).
TIFF RGB/RGBA unsigned8/16 or float16/32 uses a bounded native decoder, not ffmpeg (the installed ffmpeg rejected float32 RGBA TIFF). Only first page/top-left orientation/straight alpha supported. Associated-alpha TIFF is rejected rather than silently misinterpreted. No ICC/EXR chromaticity auto-detection: caller explicitly declares input gamut/transfer, use transfer=0 for scene-linear sources. Multi-part/deep EXR and image sequences are not implemented.
`render_float_exr` exports working scene-linear float32 RGBA, no output display transform. Standard chromaticities, adoptedNeutral and caller-declared whiteLuminance are embedded (see the metadata section below); no claim of official ACES interchange compliance. It preserves negative and >1 values; ordinary integer delivery necessarily bounds samples.
`analyze_color` managed mode now uses working-gamut XYZ Y weights, labels scene-linear domain, and omits the invalid BT709 CbCr grid. `units=nits` scales by caller-declared reference_white, not measured display/scene luminance; HLG display OOTF remains absent. PNG scopes require Rec709 SDR output and apply the output transform; HDR/wide-gamut scope requests fail with directions to quantitative scopes rather than emit mislabeled PNGs.
Proof: float EXR + float32 TIFF 16x16 -> 32x32 retain exact range -0.25..4.0 and 100-reference-white scope range -25..400. EXR export 0.140s; native TIFF->EXR 0.047s. Real SCUMBLE 640x360 frame EXR 0.545s; 50-frame RGB16 3.146s. SDR fixture and actual footage previews visually inspected. This is CPU float interchange/scopes beta, not HDR certification, an ACES RRT/ODT, or parity with established grading applications.

### Float interpolation color-state and EXR standard metadata
Native float EXR/PFM/TIFF inputs now convert from explicit clip/project input encoding into working-linear before alpha-correct fit. Cache marks these samples working-linear to avoid a second transfer decode; input gamut/transfer overrides participate in cache invalidation. Tested sRGB and gamma2.4 non-linear TIFF resize against analytic linear interpolation and changing a cached clip input override. The default integer video/PNG path still uses ffmpeg resize before input conversion; opt-in linear_resample, described below, converts native integer samples before engine fit.
EXR export now uses native Rust lossless PIZ float32 encoding and embeds standard `chromaticities` for the selected working gamut plus `adoptedNeutral` and `whiteLuminance` from declared reference white. It does not set `acesImageContainerFlag`, claim official ACES container compliance, or imply measured luminance. Reader metadata tests verify AP1/D60 values and white luminance 100. Source metadata is still not automatically trusted or selected over explicit project input settings.
Measured native metadata-bearing EXR export: synthetic EXR source 0.085s, TIFF source 0.0036s, real 640x360 frame 0.423s. Real 50-frame RGB16 repeated at 2.980s; visually inspected SDR footage preview intact. Float -0.25..4 range still exact after roundtrip.

### Float media inspection and decode guard
`inspect_float_media` reads EXR/TIFF headers without decoding pixels. EXR reports per-layer dimensions, channel types, sampling, data-window origin, deep status, file-declared chromaticities/white luminance, and candidate built-in gamut IDs within 1e-5 xy tolerance. Transfer is not guessed; candidates never change clip input state. EXR header byte reads are limited to 1MiB. Managed EXR decode now rejects multi-layer/deep/nonzero-origin/nonstandard or subsampled/non-float RGB(A) rather than silently interpreting the wrong layer. TIFF inspection reports first-page sample format, alpha/orientation and whether more pages exist. Does not apply ICC. All eight built-in gamut signatures verified against actual output header. 400 hostile inspection calls leave server alive. Real 640x360 EXR inspection0.024s; TIFF0.0016s. Clarification: native EXR compression is lossless PIZ (verified actual header), not ZIP as earlier described.

### Float-source area downsampling
Native float fit now integrates each destination pixel's source footprint when shrinking, while enlargement stays bilinear. Both paths accumulate premultiplied RGB/alpha then return straight RGBA. Signed HDR values remain unbounded; area weights do not introduce negative-lobe ringing. The default integer video/PNG path still uses ffmpeg resize; opt-in linear_resample applies engine area/bilinear fit after native integer conversion. Synthetic 256x256 alternating -0.25/4.0 EXR ->32x32 preserves exact mean1.875 in every pixel, export0.100s. Transparent colored sample shrinks without fringe; randomized 221 source/destination pairs verify finite output and mean preservation for shrinking. Visual contact sheet shows checker source and uniform tone-mapped result. Real frame EXR0.423s/50-frame RGB16 3.176s, preview visually inspected intact. Area filtering is a practical anti-aliasing improvement, not an assertion of ideal reconstruction or renderer parity.

### Full-float TIFF output
`render_float_tiff` writes working scene-linear RGBA32Float without applying display output transform or clipping signed/HDR RGB. TIFF ExtraSamples=2 explicitly marks unassociated alpha (valid0..1). ImageDescription records working-gamut ID and caller-declared reference white; nonnegative chromaticities get standard TIFF WhitePoint/PrimaryChromaticities rational tags. Gamuts with negative primary coordinates omit those unsigned-rational tags rather than encode wrong values; gamut still recorded in description. No ICC profile or automatic transfer inference. Header inspection exposes file description and chromaticities as untrusted file-declared data, never applied automatically. Unit roundtrip retains exact -0.25/4/1.25/0.5 samples and verifies float sample/alpha tags; invalid alpha is rejected. Real640x360 working-float TIFF render0.376s; independent tifffile numeric read matches same-frame EXR exactly.400 hostile tool calls leave server alive.

### Decode cache identity
Decode cache signature now includes fit settings, source path, file size and modification timestamp, as well as input encoding/project pipeline/timing. Changing contain/cover/stretch or replacing a source file triggers re-decode instead of rendering stale fitted samples. Test warms a TIFF cache, changes contain->stretch and replaces source dimensions/sample value, verifying new alpha coverage and HDR value2. File freshness uses filesystem stat, not content hashing: an externally replaced same-size file with deliberately preserved modification time may still need explicit cache reset.56total tests pass.

### Float alpha boundary and cancellation
Float fit/EXR output now reject nonfinite/out-of-range alpha rather than let negative or >1 coverage create invalid premultiplied interpolation. RGB remains signed/unbounded. Float fit checks cooperative cancellation once per output row. Tests include alpha-1/1.001/Inf/NaN rejection and f32 extreme RGB interpolation without overflow.57total tests pass. This validation does not claim arbitrary effects always preserve valid alpha; export errors expose invalid coverage.

### Opt-in integer-media working-linear resample
`set_color_pipeline` accepts `linear_resample` (defaultfalse, absent old projects stayfalse). Enabled mode decodes integer video/PNG at native dimensions into RGBA16, converts declared input transfer/gamut before float area/bilinear fit, and caches project-sized working-linear frames. It never routes integer precision through swscale float. Native frames stream one at a time; cache signature includes this option and input color/framing/source identity. Window size accounts for native dimensions. Current spot/export ramp preparation windows source indices and rejects required spans that cannot fit; full-range preparation can still exceed the budget. ffmpeg still handles YUV->RGB and temporal sampling; this is not a full independent codec/color-metadata interpretation.
CPU/memory tradeoff measured on real1280x720 source ->640x360:50-frame RGB16 default3.175s vslinear8.550s, frameEXR0.425s vs1.183s. SDR preview mean difference0.2558-bit codes, max32(edge pixels); side-by-side visually inspected intact framing/color cards. Correct interpolation ordering, not a claim of subjective superiority. Explicit opt-in protects current projects/performance. Integer PNG fixture verifies working-linear cache state and exact0.5 linear mean of black/white stripes;58total tests pass.

### Shared decoded source windows and active-media budgeting
Immutable decoded frames are now Arc-shared across clips only when the entire decode identity matches (source asset/path/stat, timing/speed, input encoding/pipeline, fit) and the available window covers requested indices. Effects/transforms remain per clip. Cache memory totals count each shared allocation once, including reserved lazy quantized copy. Different offsets/fit/encoding do not share. Interactive prefetch divides cache budget across active media clips; existing oversized windows are evicted when active count grows. Required sample ranges remain mandatory and fail if they cannot fit; whole-timeline and ramp preparation do not silently drop required frames.
Reproduced real four-layer720p same-source failure at256MiB cap (0.711s). Shared-window render now succeeds0.331s; four independent source offsets succeed0.918s via budgeted prefetch. Output720p frame visually inspected. Unit test proves same-window pointer sharing and fit-change separation.59tests pass. This is in-session window sharing, not persistent disk caching or unlimited parallel layers; real deadlines/required ranges still bounded by cache capacity.

### Exact integer input transfer table optimization
Native integer linear-resample now constructs a65536-entry f64 transfer table and one gamut matrix per decoded window, then indexes exact16-bit codes. Table normalizes through the samef32 arithmetic/reference-white rounding as general input conversion; not an approximate interpolated LUT. Exhaustive65536codes x9transfers x4gamuts reproduce general float input transform bit-for-bit, including white123.456. Table memory512KiB/window. Real50-frame640x360 linear-resample8.314s->5.904s(29%faster); frameEXR1.097s->0.742s. Still slower than default3.175s, opt-in staysoff. Preview visually inspected unchanged. No GPU/codec speed claim.

## Scene-linear exposure evidence and false color
`analyze_exposure` requires a managed working-linear project and returns exact luminance quantiles(min,0.1%,1%,10%,median,90%,99%,99.9%,max), declared-reference-white nits, stops from caller middlegray(default0.18), half-stop histogram-16..16, negativeRGB/declared-peak excursions, transparent/invalid counts and alpha-weighted mean. Quantiles use linear interpolation atq*(N-1) over visible pixels; alpha weighting only applies to the separately labeled mean. Nonpositive luminance has nullstop, no logofnegative. Excursion does not establish clipping or mastering; no automatic exposure change.
`render_exposure_false_color` outputs diagnosticSDRPNG with exact returned stop palette/legend. Purple=nonpositive, blue=deep shadows, cyan=-3..-1stops, green=-1..1, yellow=1..3, orange=3..6, red=6+, invalidmagenta/transparentblack. This is an analytical palette, not a camera-specific calibration. Uses actual working-gamutYweights. Real230400pixel frame analysis0.378s including decode; diagnosticPNG0.409s. Visually inspected cards/table/shadows map to expected bands. MedianY0.001943/99th0.448721 on this frame, no inference about unmodified source exposure.64tests pass including signed/HDR percentile, invalidalpha/transparent and palette boundaries;200hostile analysis calls leave server alive.

### Bounded exposure sequence diagnostics
`analyze_exposure_sequence` samples2..120evenly spaced rendered frames in caller[start,end), returns median/99th luminance, medianstops and adjacentmedianstop change plus callerthreshold events. Local windowed decode cache is reused across samples. This is sparse diagnostic evidence, not every-frame scan or automatic flicker/cut classification: movement/cuts/coverage/real illumination can all change median. No correction applied. Real12samples320x180 over2s0.376s, four threshold1stop events. Range/end/count test covers exclusiveend and rejects outofproject/badcount;66tests pass.

### Output-stage range audit
`audit_color_output` separately reports working-linear range, output-gamut-linear before tone/gamut mapping, and actual configured encoded-output range. Optional PQ ceiling report uses10000/referencewhite in output-linear, not arbitrarylinear1 threshold. Includes negative/above1RGB and alpha/nonfinite coverage. RGBlinearabove1 can be legitimateHDR; negative and10000nit excursions may be bounded byPQ encoding, so pre-encoding counts remain visible. This is not YUVlegal-range/broadcast compliance or mastering certification, and applies no correction. Tests distinguish pre/postPQ bounds, empty/legacy errors, and bit-exact encodedrange against actual pipeline output. Real640x360 audit includingdecode0.38s class.69tests pass.

## Explicit luminance-ray shoulder
`luminance_shoulder` effect takes explicit linear gamut,knee,peak,amount. MapsY above knee withknee+(peak-knee)*(1-exp(-(Y-knee)/(peak-knee))); belowknee identity, derivativecontinuous1 at knee, monotonic boundedY asymptote. Scales RGB uniformly soRGB ratios preserved, not a perceptual hue guarantee. Negative luminance untouched, alpha unchanged. Does not bound saturatedchannels or replace gamut mapping. No encoding/automatic project setting, ACESRRT/ODT/compliance claim. OfficialACES2 documentation describesJMh rendering and separate display encoding, unlike this simple analytical operator: https://docs.acescentral.com/system-components/output-transforms/ and https://docs.acescentral.com/system-components/output-transforms/technical-details/gamut-compression/ .
Stress proof: realfootage +3stopsgain then shoulder(knee0.75/peak1) retains coloredcard distinctions vs hard-clipped output; shadows intentionally remain lifted/noisy. Visually inspected pairedbefore/after and no 'good grade' claim.50-frame640x360RGB16 export4.317s.72tests pass including10000point monotonic/continuity, signed/ratio/alpha, hostileparams. Reproducer included.

### Working-gamut-bound linear effect helper
`add_managed_linear_effect` adds linear_grade/luminance_shoulder, plus the later white_adaptation/soft_gamut_ray additions, with explicit currentworkinggamut and persistentstate guard. Caller omitsgamut; helper validatesparameters before mutation. Guardrejects render after projectgamut/enable state changes or gamutparameter mutation/keyframing/expressions, rather than silently usingwrongweights. Re-add after deliberateworkinggamut change. Existingadd_effect and oldeffect semantics unchanged.73tests pass includingAP1injection/stalegamut/rejectedbadparams.

### PQ wrapper reference-white correctness
`set_effect_encoding` now stores projectreferencewhite in encodingwrapper and uses it in both conversion directions. Previously wrapper silentlyused100 even on a203referencewhite project, which gavePQ-domain effects thewrongabsolutelevels. Newwrapper guards rejectchangedprojectwhite until reconfigured. Oldserialized wrappers withoutwhite keep historical100 behavior; reconfigure to inheritprojectwhite. Clearing removeswhite marker too. Tests match wrappedPQ-effect bitexactly to explicit203white transform/effect/inverse chain and identityroundtrip.75tests pass. No automatic encodingguess or change to oldeffects.

### Export batch cache-budget fallback
Legacy parallel export previously requestednt*4frames regardless of per-active-media cache budget. Multipleindependent media windows/nestedgroups could therefore reject full export although singleframesfit. Exportnow retries only explicitcache-cap/budget errors withhalvedbatch down to1frame, clears partialcache between retries, requests lastactualbatchframe rather than extraendframe. Requiredframe stillfails if impossible, unrelatederrors propagate. Twonestedgroups/fourindependentoffset720p realmedia0.4sexport succeeds2.301s; regressiontest checks nestedexport. Perframeoutput/framing unchanged, threadparallelism can reduce under memorypressure.

### Declared-neutral linear sampling
`sample_linear_neutral` returns diagonal gains from caller-selected normalizedregion in managedworkinglinear. Channelwise trimmedmean(default10%each tail) excludesnonpositiveRGB/invalid/transparent samples, preserves sampledstatistic'sgamutY when neutralized. Requires>=4positivevisiblepixels, caps100gain, returns evidenceonly/noeffect. Callerdeclaresneutral; doesnotestimatephysicalilluminant/Bradfordadaptation or allsceneaccuracy. Outliertest verifiestrimmedneutralization, hostile/emptypatcherrors. Real1170pixel illustrativetablepatchsample0.190s; itsneutralitynotestablished and gainsnotapplied.78tests pass.

### Explicit xy Bradford white adaptation
`white_adaptation` effect applies caller-declaredsourcewhite/targetwhite CIExy Bradfordadaptation in explicitlineargamut; preserves signed/HDR/alpha, noencoding/clipping. Managedlinearhelper includes it withworkinggamutguard. Notautomaticimageilluminant estimation; partialamount blendsmatrices, notspectralinterpolation. Tests all8gamutsidentity/inversewhite adaptation and invalidxy/singularboundary. IllustrativeD65xy->D50xy real320x180render0.231s, visuallyinspected intactwarmshift, notclaim actualsceneilluminationD65/D50.80tests pass.
Bradford reference methodology inspected at http://www.brucelindbloom.com/Eqn_ChromAdapt.html and https://fujiwaratko.sakura.ne.jp/infosci/colorspace/bradford_e.html . Additionaldirecttest mapsdeclaredD65whiteXYZ todeclaredD50whiteXYZ(<1e-10), beyondinverseconsistency. Chromaticityroundingaffects matrices, no claim allpublishedroundedconstants arebitexact.

### Output gamut excursion overlay
`render_gamut_excursions` showsnegativeRGB(blue), above callerlinearceiling(red), both(magenta) in outputgamutlinear beforetone/gamutmapping overconfiguredSDRpreview. RequiresRec709SDRoutput to avoidrawPQ/HLGPNG; exactcounts/legend/stagereturned. RGBceiling1notuniversalHDRlimit; noillegalHDR/clipping/broadcastcertification. Real+3stopstress320x180diagnostic0.253s, red7917/blue1179pixels, actualpixelsinspectedexpectedcardhighlights/excursions.82tests pass withdistinctstagecolors/HDRpreviewrejection.
Managedlinearhelper now rejectsunknownparameter names beforemutation; a misspelledexposure cannot silentlycreateanignoredsetting.83tests pass, rollback/noeffectasserted.1260hostilecalls acrossnew7diagnostic/helpertools leftserveralive.

### Soft linear gamut-ray compression
`soft_gamut_ray` effect compresseschroma alongconstantYRGB ray nearRGBcube boundary: normalizedboundarydistance knee(default0.8), exponentialasymptote1. LinearYwithin0..ceiling retained; Youtside becomesclampedneutral, so caller musttone-mapfirst. Explicitgamut/ceiling, managedhelperguard. No perceptualhueguarantee/ACESmappingclaim and noautomaticprojectchange.10000randomsignedRGB cases verifyboundedRGB and clampedYpreservation; neutralidentitytest.85tests pass. Real5xsaturation stress320x180render0.084s(warmcache), pairedbefore/afterpixelsinspectedcardcolorsboundedwithoutthehardredchannelclip; notcreativequalityparityclaim.
Gamutrayfollow-up: fixedboundarydistanceinitialization toallowactualinsidecubedistance<knee, so innerchromaidentityis exact; earlyversionunnecessarilycompressedallnonneutralchroma. Regressiontest locksidentity, all8gamutsbounds and serializedmanagedhelper tested.87tests pass. Combinedlineareffectstress50-frame640x360RGB16 4.698s, auditshowsnoRGBexcursions; previewvisuallyinspectedbut intentionallyoverexposed/noisy, notcreativequalityclaim. Reproducer included.

## Optional pinned OpenColorIO ACES2 reference preview
`render_aces2_reference` is an opt-in reference-onlySDR100Rec709PNG from explicitlydeclaredACEScg/AP1scene-linear workingframe. Requirespython3withopencolorio==2.5.1 available to theserver; noautomaticinstall. Usespinnedbuiltin`cg-config-v4.0.0_aces-v2.0_ocio-v2.5`, display`sRGB - Display`, view`ACES 2.0 - SDR 100 nits (Rec.709)`. Bypassesourconfiguredoutput transform, no doubleencoding. Notprojectdefault, industrysuperiority, measuredmasteringorACEScertification. Missing/wrongdependencyfailsclearly. Optional CPU processor is now reused per explicit reference video export; not production HDR delivery. OCIOitselfnotvendoredhere.
Actual640x360referenceCPU0.109s; fullMCPdecode/bridge/PNG0.672s. IndependentdirectPyOpenColorIO vsbridgePNGmaxcodeerror0 on sameEXRinput; actualpixelsvisuallyinspected intactdarkerfilmiccards. Domainrejectiontests and optionalrealprocessorfinite/alpha testpassed withKIMO_TEST_OCIO=1. Sources: https://opencolorio.readthedocs.io/en/v2.5.1/releases/ocio%5F2%5F5.html ; https://github.com/AcademySoftwareFoundation/OpenColorIO-Config-ACES/releases/tag/v3.0.0 ; https://docs.acescentral.com/system-components/output-transforms/implementer-notes/ ; https://docs.acescentral.com/system-components/output-transforms/parameters/ . UpstreamOCIOlicense https://raw.githubusercontent.com/AcademySoftwareFoundation/OpenColorIO/main/LICENSE permitsredistributionwithnotice, noendorsement; installer/dependencyredistributors mustretainupstreamlicense. Currentimplementationdoesnotcopyupstreamalgorithmcode, callsinstalledprocessor.


## Persistent optional ACES2 reference video
`export_aces2_reference_video(path, crf=18)` writes an explicit video-only SDR MP4, using the same pinned OCIO view as the reference PNG. It never changes project output settings and does not include audio. Even nonzero dimensions required for 4:2:0. sRGB-encoded RGB is converted to limited-range BT709 YCbCr; tags explicitly declare BT709 primaries/matrix and IEC61966-2-1 transfer. This is a reference inspection path, not a Rec709 gamma2.4 broadcast master or HDR delivery.

One pinned CPU processor per export; bounded one-frame channels, 60-second frame deadline, cancellation checks every 100ms while waiting, process failure poisons the session, child kill/reap and worker joins on drop. Encoder RAII cleanup also covers unwind/cancellation. Rust memory preflight covers in-process buffers, not external OCIO Python allocations. No automatic dependency install. SDR and PQ reference processor repeated-frame parity tests pass; PQ is not exposed by this MP4 command.

Synthetic 10-frame 640x360 processor benchmark: persistent 1.240s versus per-frame startup 2.215s. Actual footage 50 frames at 640x360/25fps exported in 8.192s (2.0s video); ffprobe confirmed all 50 frames and explicit color tags. Sampled video pixels inspected: colored cards and interface intact, darker output expected from this view. Engine tests passed including killed-child/non-reuse/cleanup and invalid frame recovery. These are local CPU timings, not real-time or industry parity claims.

## High-bit export preflight and process cleanup
`export_high_bit` now rejects zero dimensions, empty/invalid timelines and nonfinite/nonpositive fps before spawning an encoder; HEVC4:2:0 rejects odd dimensions. RGBA16 conversion validates frame shape/overflow and memory preflight rather than returning truncated byte streams. Encoder RAII kills/reaps on cancellation or panic as well as ordinary errors. A subprocess unwind test verifies removal from /proc; tiny FFV1 fixture and existing full-range integer16 precision tests pass. Existing successful export/color behavior unchanged. At this historical checkpoint, synchronous ffmpeg pipe writes and final waits still had no independent deadline; this patch does not claim hard cancellation of a blocked encoder.

## Nested render failure propagation
Precomp source rendering no longer silently replaces a failed inner render with an empty/black frame. Inner errors mark the source invalid so the existing outer finite-pixel check returns an error. Regressions verify grouped scene-linear signed/HDR values equal the ungrouped frame bit-for-bit (no second input decode), and a nested invalid linear-grade exposure fails instead of a successful black export. Error details remain generic at the outer render; this is failure integrity, not nested diagnostic context.

## Very low alpha float resize
Float fitting now normalizes premultiplied RGB for every positive weighted alpha instead of zeroing straight RGB below 1e-8. This keeps finite signed/HDR radiance through legitimate low-opacity pixels. Exact zero alpha still returns zero RGB to avoid undefined hidden-color division. Regression constant radiance [4,-0.25,0.18] at alpha1e-9,1e-20,1e-38 survives downsample and enlargement; all16interchange tests pass. This change affects managed/native float fit only, not legacy integer decode.

## Legacy helper encoding gates
`pick_white_balance` and `match_color` now reject enabled scene-linear projects rather than sampling/clamping working-linear RGB and installing a legacy display-referred effect without an encoding wrapper. Managed neutral work uses `sample_linear_neutral` plus caller-selected `linear_grade` gains. Legacy picker also rejects invalid/zero-pixel regions; match strength must be0..1. The picker samples the isolated rendered clip including existing effects, now stated accurately. Existing valid legacy gains unchanged. Three helper regressions verify rejection before mutation and expected legacy gains.

## Explicit reference-relative exposure suggestion
`compare_linear_exposure(clip_id, ref_clip_id, time, ref_time, percentile=0.5)` samples two caller-selected managed scene-linear frames and returns an exposure-stop suggestion for `linear_grade`, without adding an effect. Statistic: positive luminance, alpha0.99..1, unweighted percentile with linear interpolation. Percentile0.1..0.9, minimum4samples per frame, suggestion bound20stops. No semantic match, physical exposure inference or automatic creative target. Changes in framing/subject/lighting can make the suggestion inappropriate. Synthetic4xreference returns exactly2stops across all8workinggamuts; zero/transparent reference and invalid percentile rejected.

## Optional processor diagnostics
OCIO stderr is drained concurrently but retained at most8KiB, then included in failure results after child cleanup. Missing dependency now reports its actual import error; simulated wrong version reports the pinned2.5.1 requirement. Missing-library MCP path fails in0.029s locally. Capture is bounded and never executes diagnostic text. Actual optional processor tests still pass; no dependency install or fallback transform.

## Declared Rec709 high-bit SDR conversion and tags
Enabled Rec709 output now explicitly uses BT709 limited-range RGB-to-YCbCr for HEVC10, and declares BT709 primaries/matrix. Supported exact transfer tags: linear, sRGB, BT709 OETF. Gamma2.4 and camera log encodings are not mislabeled as BT709 OETF. FFV1 RGB uses identity matrix and full range. FFmpeg option name for identity matrix is `rgb`, not the ffprobe output name `gbr`; corrected existing PQ/HLG FFV1 paths too. Legacy disabled pipeline unchanged.

FFV1 encoded sRGB fixture decodes RGB16 within2e-5 and probe verifies tags. Actual10-frame320x180HEVC SDR export0.547s with10-bit420, explicit tags and retained audio. First decoded video frame visually inspected, scattering panels/lightbeam intact. Contact-sheet generation failed on the0.4sclip, so verification used the decoded first frame instead. This is signal identity tagging, not broadcast gamma2.4 mastering or color calibration.

## PQ/HLG FFV1 regression evidence
PQ and scene-HLG FFV1 RGB16 now have direct encoder/decoder regressions, confirming BT2020 primaries, identity RGB matrix, their exact declared transfer and decoded RGB within1.6e-5 of the configured output function. Actual10-frame320x180PQ RGB16 export0.426s with203declaredreferencewhite, no mastering metadata; probe confirms fullrangeGBRP16/PQ/BT2020 and retainedPCM audio. This is tagged integer interchange, not measured HDR/HLG display rendering; HLG still lacks display OOTF.11pipeline tests pass.

## Common-domain optional reference difference
`compare_aces2_reference` requires AP1 working + configured Rec709/sRGB output, then compares both encoded float outputs before integer clipping. Reports visible unweighted RGB RMSE, mean signed difference, maximum absolute difference and configured outside0..1coverage. Not a quality ranking: unlike plain matrix/encoding, the official view performs display rendering. No output changes, auto-fitting or compliance claim.

Actual640x360footage fullMCP0.722s: RMSE[0.113652,0.114899,0.113648], maxdelta[0.168222,0.231398,0.168222],34configuredoutofrange pixels of230400visible. This explains the prior darker reference preview numerically, not a defect score. Tests reject incompatible output domains and verify finite metrics on signed/HDR samples.

## Reference frame preflight
Optional OCIO one-shot rejects malformed/nonfinite frames and alpha outside0..1 before starting Python; persistent apply enforces the same alpha boundary while retaining valid processor state after rejected input. High-bit export rejects subframe timelines that round to zero frames before encoder spawn. Signed/HDR RGB remains allowed.7OCIO+11pipeline tests pass with real optional processor.

## Lab sampling API hostile-input bounds
Lab sampling now validates frame shape/overflow and excludes nonfinite/out-of-range alpha, not only alpha<0.99. Transport validates finite source means/covariances, symmetry, nonnegative diagonal and minimum4samples before matrix powers. Effect API rejects malformed/nonfinite frames rather than processing partial RGBA chunks. These checks do not certify arbitrary covariance positive definiteness; eigenvalue regularization remains the existing stability fallback. Six matching regressions pass, including malformed dimensions and invalid source evidence.

## Machine-readable declared pipeline inspection
`inspect_color_pipeline` returns declared render/input/output domains, current settings, bound/wrapped effect contracts and stale project-gamut/reference-white flags. It includes supported high-bit formats/tag candidates and optional reference prerequisites. No render, media metadata inference, dependency availability claim or configuration mutation. The reference dependency is explicitly 'not checked'. Caller must still handle render validation and malformed wrapper fields. Default legacy state, fresh/stale bound effect and invalid gamut tests pass;12pipeline tests total.


## Sampling resource preflight
Declared-neutral sampling now reserves its three channel arrays against the Rust memory budget before allocation and uses exact region capacity. Lab sampling checks cancellation per row. These checks do not bound OCIO subprocess memory or certify real-time performance. The old top-level remaining-features list was reconciled with implemented opt-in features and current caveats, avoiding stale claims that completed IO/scopes were missing.

## Declared mastering geometry/precision checks
Caller-declared mastering white must lie within its primary triangle (nonnegative barycentric weights), independent of primary winding order. x265 serialization rejects min/max luminance that collapse to the same0.0001nit integer value. These checks prevent inconsistent declarations, not measured display validation. Three mastering tests and12pipeline tests pass. Existing P3/D65 test declaration unchanged.

## Still-image cache identity
Decoded windows now carry explicit `still` identity. A one-frame video window at frame0 no longer claims to cover every later timeline index; only actual still-image paths get that reuse. This prevents frozen first-frame output when decode budgets shrink video windows to one frame or a source ends early. Direct window regression plus actual shared-source/nested-export/float-interchange tests pass. This intentionally changes a public Rust `Frames` struct field, so external literal initializers must set `still`; MCP schemas unchanged.

## Fractional instant cache range
Normal playback instant samples request their floor frame rather than floor..ceil, avoiding false memory rejection when only one frame is needed. Flow blur explicitly retains the next-frame requirement and fails if the budget cannot hold it instead of pretending duplicate-frame flow. Range, reverse/freeze and speed-curve paths retain prior bounds. Actual KIMO_MEM_MB=3 /4active managed128x72clips: fractional.28frame now renders blue correctly; flowblur at the same time still returns active-range budget error. Five cache tests pass; blue output pixels inspected.

## Stalled reference process timeout regression
A real optional OCIO child is stopped with SIGSTOP in the test, then a private short-deadline invocation confirms timeout poisons the session, kills/reaps the stalled process and rejects reuse within2seconds. Production apply still uses60seconds and100ms cancellation/check intervals; no user-configurable bypass. Six library and7OCIO tests pass. This verifies stalled Python processor lifecycle, not synchronous ffmpeg write deadlines.

## Decoder cleanup on cancellation
Media ffmpeg decoder is now held by a kill/reap-on-drop guard, covering native float frame-loop cancellation and error paths before the existing explicit wait. A process unwind regression verifies reaping;7library,5cache and16interchange tests pass. Successful decode/cache behavior unchanged. At this historical checkpoint, synchronous output reads remained a separate blocked-IO deadline limitation.

## Header-only color delivery inspection
`inspect_color_delivery(path)` reads local exported file headers through ffprobe, returning codecs/pixel formats, declared color tags/static side data, audio streams and absent/unspecified color fields. It does not decode/count frames or infer missing encoding. Local-file-only, bounded1MiB probe stdout, 30s total capture deadline and process cleanup; cancellation checked at most100ms apart while waiting. Two fixture/path tests pass. ActualPQ RGB16/PCM deliverable inspected through MCP in0.103s with matching explicit tags. Tag presence is not proof pixels match, calibrated display/mastering or compliance.

## Per-media declared input inspection
Pipeline inspection now includes each media clip's effective base input gamut/transfer IDs, override/default provenance, invalid-ID flag and presence of animation that decode does not evaluate. This exposes the existing static declaration semantics without silently adding time-varying input transforms or inferring camera identity. Nested media included through flattened clip traversal.13pipeline tests pass, including distinctcamera input override and invalidID inspection.

## Delivery probe bounded IO deadline
Header inspection now captures stdout on a joined reader, retains at most1MiB and enforces a30second total stdout/process-exit deadline with100ms cancellation checks. Timeout/overflow/error kills and reaps the probe before joining. Tests exercise actual sleeping child reaping, oversized output, success and failed exit;9library+2delivery tests pass. This helper is for direct local commands whose stdout has no surviving descendant writers. At this historical checkpoint, general media decoder/encoder blocked IO was unchanged.

## Explicit frame-level HDR metadata inspection
`inspect_color_frame_metadata` adds a separate first-video-packet decode to read frame side data such as HEVC mastering-display and content-light SEI. It does not silently turn header-only inspection into frame decode. Some codecs/intervals return zero frames; missing metadata here does not prove whole-file absence. Bounded1MiB/30s capture applies. Actual10-frame320x180PQHEVC declaredP3/D65/1000nit/MaxCLL1000/FALL400 export0.623s: first frame confirms exact x265 rational mastering fields and content-light values. Three delivery tests pass including realHEVC export/SEI inspection. Declarations remain caller-supplied, not measured mastering.

## Mastering setter precision preflight
Quantized min/max distinction is now checked by mastering validation itself, so `set_hdr_mastering` rejects unrepresentable pairs before changing project state rather than waiting for x265 export. Four metadata tests pass, including no-mutation setter regression.

## Decode window phase determinism
Decode windows now seek only to the clip's declared in_point, run the same speed/fps resampling timebase, then trim by absolute output-frame index. Seeking independently to each window start reset fps cadence, so source30fps/project25fps could choose different pixels for frame19 depending on prior spot renders. Synthetic moving fixture reproduced meanchannel difference5.48/max255; corrected cold/sequential difference0. Exact transferred tandem960x540 cold.76 versus.24/.52/.76 sequence is byte-identical after correction. Regression also compares fullclip cache, nonzero in_point.13, speeds1/0.7/1.2 and legacy/managed-nativefloat.

Correctness tradeoff: later windows decode/resample from clip in_point, adding CPU work proportional to elapsed clip time. Returned frame buffers remain bounded, but this is not a performance optimization or fastseek guarantee. A future stable-timestamp seek optimization must preserve these exact frame tests. Same source-frame phase now holds for spot renders and full export.

## Window trim before spatial conversion
Absolute output-frame trim now runs immediately after stable fps resampling, before scale/pad and output pixel-format conversion. Skipped earlier frames still decode for cadence correctness but no longer need project-resolution conversion. Phase regression remains exact for legacy/nativefloat and speed/inpoint variants;5cache+16interchange tests pass. No quantitative speedup claim yet.

## Fractional/VFR phase regression
Stable-window tests now also use30000/1001fps and variable-rate selected-frame fixtures, comparing cold spot windows against the full decoded timeline at four times with nonzero in_point. Both phase tests pass. These confirm deterministic sampling for these fixtures, not every demuxer/camera timestamp condition.

## Reset trimmed output timestamps
Stable fps phase is retained, but trimmed output timestamps must then reset to0 before rawvideo output. Otherwise default ffmpeg output synchronization may duplicate initial window frames on later windows. Added setpts=PTS-STARTPTS after trim in integer/nativefloat paths. Independent actual footage segment windows at24/100/250/400 and256-framechunks now byte-identical to full raw resample. Late-window/full decode fixture passes. Corrected20s320x180baseline export measured below in follow-up benchmark; no speculative speedup claim.

## Bounded export lookahead amortization
Legacy/8bit export now requests up to256decoded frames of lookahead, clipped by the existing per-media/cache memory budget and timeline end. Spot render keeps24framewindow; frame phase/PTSreset unchanged. This amortizes prefix decode after the correctness fix without retaining full unbounded clips. Four phase tests including spot/export window exact parity and five cache tests pass.

Actual20s320x180500frameexport: fixed24framebaseline11.839s, bounded256lookahead3.810s (68%lesswalltime). All500decoded output frames are byte-identical, SHA256d17fcafa48e360c365c83198023c26ea5c8787badfa9414f806842e916fb73f4 for both exports. Earlier uncorrectedPTS prototype benchmark was rejected, not used as parity evidence. Larger lookahead may use more memory up to the unchanged cap; active-clip fallback still applies.

## High-bit/reference lookahead reuse
High-bit and optional reference video exports now use the same bounded256frame decode lookahead, while preserving their per-frame output transforms/processors. Regression compares spot/export windows across frame256 in legacy and native managed float modes. Actual10framePQFFV1RGB16 roundtrip matches the prior deliverable byte-for-byte (SHA2565ccc74399cd1e5afe71b42ec9cadf44af13ea2d85b18fdc705c7982b8c40a588). No new high-bit speedup claim from that short clip. Fourphase/13pipeline/7OCIOtests pass, real optional dependency enabled.

## Direct render/decode time validation
Direct Rust render and cache-preparation APIs reject NaN/infinite times and reversed ranges instead of silently casting them to frame indices or returning a plausible black frame. The finite whole-clip sentinel range remains supported. Two hostile and four phase tests pass. MCP numeric sanitizer already rejected nonfinite JSON numbers; this closes direct engine calls without altering valid sampling.

## Direct managed-effect frame contract
Managed conversion, linear grade, Bradford adaptation, luminance shoulder and gamut-ray APIs now share preflight for exact RGBA shape, finite RGB and alpha0..1. Signed and HDR RGB remain valid. Invalid frames fail before any channel writes, closing partial-chunk success and invalid-alpha passthrough in direct Rust calls. Existing valid pipeline/transform regressions still pass; legacy effects are not automatically changed.


## Empty-frame audit compatibility
Shared managed-effect preflight permits correctly empty zero-pixel frames, preserving the established output-audit empty-statistics contract. Malformed dimensions/data still fail. Full-suite regression caught this compatibility gap before completion; follow-up restores it rather than redefining the audit API.

## Static input encoding contract enforcement
Managed media input gamut/transfer now rejects keyframes or expression operations instead of silently reading only their base values. Integer16 lookup construction enforces the same static contract, and inspection flags unsupported animation. Existing static project defaults/clip overrides unchanged. This does not add time-varying camera transforms. Fourteen pipeline tests pass including animated input rejection.

## Color setter contract checks
MCP pipeline setter rejects unknown settings instead of silently ignoring typos; project JSON schema compatibility remains unchanged. Clip input override setter enforces its documented enabled-managed/media-only scope, so procedural clips cannot report a successful override that rendering ignores. Fourhelper+14pipeline tests pass, including no-mutation typo/domain rejection. High-bit tool description also now accurately names optional caller-declared static mastering support.

## Stale wrapper cleanup
`set_effect_encoding(clear=true)` works after the managed pipeline is disabled, so a stale wrapper can be removed without temporarily re-enabling color management. Adding/configuring a wrapper still requires enabled management. Five helper tests pass including disable-then-clear.

## Reusable actual-footage color contract smoke
`examples/color_contract_smoke.py source.mp4 [--ocio]` exercises legacy preview, managed native-fit workingEXR+inspection/scopes/audit, SDR RGB16 and PQ/HLG RGB16 headers. Optional reference compare/PNG runs only with explicit--ocio and installed pinned dependency; no installation. Structured timings/results saved as evidence.json. Actual320x18010frame footage run completed; output tags match declared SDR/PQ/HLG transfers andGBRP16, legacy/reference PNG pixels inspected. This is integration smoke, not universalcameraformat or qualitycertification.

## LUT declaration and read integrity
LUT file reads stop at32MiB+1 even if a file grows after stat. Duplicate domain declarations or domain declarations after data now reject rather than silently redefining sample normalization. Direct LUT effect frame validation precedes file access, matching finite RGBA/alpha contract. Five LUT and two portability tests pass; interpolation math and valid LUT values unchanged.

## LUT finite mix bounds
Effect intensity blending now uses f64 weighted interpolation and checks output conversion, avoiding f32 subtraction overflow for extreme finite signed input. Zero intensity preserves the input exactly, even at f32 limits. Six LUT tests pass. Normal-value output rounding may differ at the last float bit; no new gamut/encoding inference.

## Curve direct API preflight
Curve lookup returns NaN for nonfinite inputs instead of underflowing its interval index and panicking. Effect API rejects malformed RGBA/invalid alpha before processing. Finite curve interpolation/extrapolation unchanged;5curve+2portability tests pass. Callers of scalar lookup still own interpreting nonfinite output.

## Covariance positive-semidefinite evidence gate
Lab transport checks normalized2x2principal minors and determinant, rejecting clearly indefinite symmetric source/reference covariances before regularization. Near-zero numerical tolerance1e-10 permits roundoff, flat/rank-deficient samples remain supported by explicit regularization. Seven matching tests pass, including indefinite2x2 and determinant-only cases. This validates covariance shape, not semantic shot quality.

## Quantitative scope finite accounting
Scope arrays preflight memory and loops check cancellation. Invalid alpha and scaled/accumulation overflow are excluded with separate counters rather than contaminating JSON min/mean with infinities. Finite range width required, preventing [-MAX,+MAX] width overflow. Three scope and16interchange tests pass; ordinary scope bins/values unchanged. Overflow counter is evidence of an unusable declared numeric domain, not clipping or mastering measurement.

### CDL direct-API frame integrity
`cdl::apply` validates the complete RGBA shape, finite samples and alpha in 0..1 before changing any samples. Signed/HDR finite RGB remains accepted and follows the documented clamped SOP behavior; alpha is preserved. The pixel loop checks cooperative cancellation every 65536 pixels. Regression coverage includes malformed shape, late NaN/Inf (no partial mutation), invalid alpha and extended-range input. This is input safety, not ASC conformance certification.

### Audit and overlay allocation/cancellation
Output audits preflight their full-frame temporary allocation before cloning. SDR gamut overlays preflight both full-frame temporaries and reject malformed/nonfinite/invalid-alpha input before allocating them. Diagnostic range and overlay pixel loops check cancellation every 65536 pixels. Empty audit compatibility is unchanged. Five focused audit/overlay tests pass; these checks do not make range evidence broadcast certification.

### Cooperative cancellation in color effects
Shared frame preflight now checks the call guard before scanning input. Linear grade, managed conversion, point curves, white adaptation, gamut-ray compression, luminance shoulder and Lab transport check cancellation every 65536 pixels, as CDL already does. Lab transport also uses the shared finite/alpha frame contract. A dedicated isolated test sets cancellation, invokes all eight effects and verifies no mutation before the cancellation panic. At this historical checkpoint, this did not close native decoder streaming or encoder write timeout gaps.

### LUT parser bounded token memory
The .cube parser takes at most five tokens from each row before enforcing the existing row grammar, avoiding a huge temporary token vector on a single hostile line. Valid size declarations preflight and reserve the exact 3D table capacity. Parsing checks cancellation every 4096 lines and applying a LUT checks every 65536 pixels. Seven focused LUT tests cover valid vertices/domains, invalid headers, huge token rows and extended finite mix. TITLE remains ignored, not interpreted as executable content.

### Exposure quantile allocation integrity
Exposure analysis/comparison now allocate their declared maximum f64 sample capacity after memory preflight, instead of growing a Vec beyond that estimate on non-power-of-two frame sizes. Guard checks bracket quantile sorting (the sort itself is not interruptible). Six exposure behavior tests and an isolated budget test pass; insufficient capacity fails before allocating the sample vector.

### RGBA16 boundary alpha contract
The direct RGBA16 output packer validates finite RGBA and alpha0..1 before allocating output bytes, while finite signed/over-range RGB still clamps at this integer delivery boundary. Cancellation is checked every 65536 pixels during packing. Fifteen pipeline tests pass, including the exact extended-RGB clamp/quarter-alpha quantization result and invalid-alpha rejection. This is not a policy change for valid rendered frames.

### Lab transport control integrity
Direct Lab effect calls reject nonfinite strength/gamut-compress controls before mutation rather than allowing infinity to clamp or NaN comparisons to select a mode. Finite strength still follows the existing 0..1 clamp. Nine Lab tests pass, including both controls at NaN/positive/negative infinity. No matching-policy or default change.

### Direct managed-control rejection matrix
A reusable regression iterates the catalogs for managed conversion, linear grade, white adaptation, gamut ray, shoulder, curves, CDL and Lab matching. Each declared numeric control is set independently to NaN, positive infinity and negative infinity. All 138 combinations must return an error before writing any pixels. This complements JSON input sanitization and the existing extreme-finite/frame-shape tests, not random fuzzing or proof for every effect.

### Single-pass guarded frame preflight
The shared direct-effect preflight checks finite RGBA and alpha range in one pixel scan instead of two sample scans, checking cancellation every 65536 pixels during validation itself. A late-invalid test places bad alpha after the first cancellation-check boundary and verifies seven effects reject it without changing earlier pixels. Signed/HDR RGB and empty-frame behavior are unchanged.

### Current limits versus historical proofs
Earlier per-feature timings describe the implementation when measured. Current opt-in behavior includes caller-declared static HDR metadata, bounded header capture and native high-precision managed input described in the later sections. Historical SDR-origin footage proofs are not HDR camera/source validation. Current managed/native decode, high-bit/reference/legacy video encoders, diagnostic/still PNG encoders and high-bit mux use bounded process owners, as recorded below. Local synchronous file writes, legacy audio/probe paths and uninterruptible work between cooperative checkpoints remain separate limits; child RSS is not included in Rust memory accounting.

### Serialized managed delivery parity
A synthetic gradient project containing an AP1 working pipeline, bound linear grade, free-position curve, explicitly sRGB-wrapped CDL and Rec2020/PQ output with declared203nit reference white is serialized/restored. Pipeline inspection, working float pixels and packed RGBA16 output match exactly at three times. Three portability tests pass. This is same-engine project roundtrip evidence, not parity with another application or file-tag inference.

### Planar float boundary contract
Planar pack/unpack preserves signed/HDR finite RGB and tiny positive alpha, but rejects alpha outside0..1. Unpack preflights the full destination allocation; both plane loops check cancellation every65536pixels. Seventeen interchange tests pass including alpha roundtrip/rejection. Optional reference processing still has its own explicit dependency and frame checks; a dependency-absent test run is not live OCIO proof.

### Ordinary decoder bounded read deadline
Ordinary project-sized integer ffmpeg decode uses concurrent joined stdout capture with a separate256MiB hard ceiling and the checked requested frame-window byte count. Header capture retains its16MiB generic ceiling (delivery calls still use1MiB). Decode deadline is30s+2s/requestedframe capped at the call timeout; cancellation checks occur at most100ms apart while waiting. Partial frame output fails explicitly instead of truncating. At this historical checkpoint, native-fit/float streaming and encoder writes remained separate blocked-IO gaps. Actual500frame320x180 output decodedRGBA hash matches the phase-fixed baseline exactly; measured export4.274s, not a speedup claim. Twelve library/five cache/four phase/fifteen pipeline tests pass, including normal ffmpeg byte/hash parity and a stalled decode child timeout/reap regression.

### Seeded numeric conversion matrix
A deterministic stress regression covers all64gamut pairs with4096signed/HDRRGB samples and86016scene-transfer roundtrips (linear/sRGB/gamma2.4/ACEScct/LogC3/SLog3/BT709). A separate monotonicity/roundtrip sweep covers PQ/sceneHLG at five declared white scales and4097code steps. PQ black checks the formula's nonzero encoded black floor separately rather than demanding exact zero roundtrip. Both tests pass. Self-consistency evidence does not prove independent colorimetric accuracy, manufacturer conformance, HLG display rendering or certification.

### Persistent reference per-frame allocation
Optional OCIO apply preflights its serialized input buffer each time rather than relying only on construction-time memory availability, and checks cancellation during packing. Common-domain reference comparison preflights its configured-output clone. With the explicit live-test gate enabled, an isolated budget test rejects before queueing a frame and verifies the processor remains usable after the budget is restored. Pinned dependency remains optional and never installed automatically.

### Contract smoke assertions
The real-footage smoke now fails on wrong pipeline domains, RGB16 dimensions/format/precision, missing/incorrect SDR/PQ/HLG primaries/transfer/identity-matrix/full-range tags, missing PCM audio or color-tag gaps. It still records full evidence/timings. Explicit optional OCIO comparison/render remains selected by `--ocio`; no dependency install. Fresh320x18010frame runs passed these assertions on the current engine.

### Planar destination budget regression
An isolated regression places the memory budget one byte below the full planar destination allocation. Both pack/unpack must fail before allocating it; after restoring the budget, the same bytes roundtrip exactly. This tests the preflight boundary independently of the allocator's hard backstop and does not model every decoder-library allocation.

### Audit temporary budget regression
An isolated test sets budget one byte below an audit clone and one byte below both overlay clones. Each must fail before its temporary allocation; after restoring budget, both valid operations succeed. This covers the explicit temporary buffers, not every serializer/library allocation.

### Scope count conservation
A seeded signed/HDR/partly-transparent frame regression checks bins16/257/4096: every counted pixel belongs to exactly one histogram/under/over category per channel; waveform sum equals in-range luminance histogram sum; vectorscope sum equals counted visible pixels. This is count integrity for the declared BT709 display-domain fixture, not a vectorscope calibration claim or HDR chromaticity scope.

### Native streaming decode bounded reader
Native integer16/float decode reads raw frames on a joined worker with a rendezvous channel. At most two raw frames are held (main + worker); their memory is reserved on the main thread before spawning. Main polls with100ms cancellation checks under the existing frame-derived window deadline, fits/converts one source frame at a time, and checks process exit under that same deadline. Receiver drop, child kill/reap and worker join handle errors/cancellation; partial frames fail. Additional peak preflight includes raw/source/fitted/accumulated buffers. Actual native500frame320x180RGB16 output matches the pre-worker decoded bytes exactly (230400000bytes, SHA25603337a49cfe54e6dfdcd596becc06e9c12e0077b3c6c2ee4a881d146f0611d15); short50frameproof also exact. Large-source/small-output prefix re-decode can take minutes; this is blocked-read safety, not a speedup.16library/phase/lookahead/cache/interchange tests and164fullsuite tests passed with optionalOCIOenabled. At this historical checkpoint, encoder writes remained the independent blocked-IO gap.

### Native stream exit-state regressions
A decoder that emits a complete frame then stalls at process exit must hit the same deadline and be killed/reaped. A decoder that emits complete bytes then exits unsuccessfully must return an error rather than counting raw-byte receipt as success. Both real-process tests pass; eighteen library tests run with the optional OCIO gate enabled.

### Flow diagnostic exact pair and float preservation
Flow diagnostics explicitly prepare the displayed time pair, then verify cache coverage before accessing frames. Float caches are copied directly without lazy8bit quantization; integer caches retain normalization. Final displayed frame uses the previous frame rather than an out-of-range/clamped duplicate; freeze returns no-motion error. Tests cover forward/reverse/end/freeze and signed/HDR float cache values without quantization. Cache memory rejection remains an error, not fake zero flow. The visualization is diagnostic, not an optical-flow accuracy certification.

### High-bit/reference encoder write deadlines
High-bit and optional reference video exports use a joined stdin writer with one acknowledged frame in flight. Main-thread queue submission is nonblocking; write acknowledgements poll cancellation at100ms intervals under30s+2s/requestedframe capped at export timeout. Finish closes stdin and checks process exit under the same deadline. Early/nonzero exit is an error; a bounded8192byte stderr tail is preserved. Main preflights frame buffers before spawning; cleanup disconnects queue, kills/reaps child and joins writer. Real-process tests cover blocked stdin, early failure diagnostic, nonzero finish while writer blocked and normal decoded-byte parity. Actual20s320x180 final audio-muxed FFV1 output matches baseline decodedRGBA16 exactly (230400000bytes, SHA2563427321180b001d3450c45dd48c256ddc6ff9bc08a1b084349d24a2a2ae2bc14) and PCM audio exactly (3837952bytes, SHA256f7556bc20859cb966ad0a1a8030cd2c734ed4e10a02a8f960cff9641350e8667).172fullsuite tests passed with optionalOCIOenabled. At this historical checkpoint, legacy export encoder writes, image writers and later audio mux waits remained separate synchronous IO gaps; this patch does not close those.

### Encoder owner cleanup and unwind proof
Obsolete local synchronous encoder wrappers were removed after migration to the joined writer owner. The unwind regression now blocks that writer on stdin and panics, verifying the new owner kills/reaps the process and joins its worker promptly. This replaces the old wrapper test; it does not add a second encoder path.

### Encoder diagnostics and size regressions
A process emitting more than8192stderr bytes then failing retains only the final8192bytes and the final error marker. A wrong-sized frame fails before queueing; a later correctly-sized frame succeeds on the same writer. Both regressions pass, with twenty-five library tests running under the optional OCIO gate.

### Staged high-bit audio mux with bounded wait
High-bit audio mux uses an exclusively created, unique temporary directory and writes its final mux candidate there, keeping the target separate until success. A frame-derived deadline capped by export timeout bounds ffmpeg exit wait with100ms cancellation checks and an8192byte stderr tail. On failure/unwind the completed video-only output is restored before deleting temporary output/WAV. If restore fails, recoverable video/WAV stay in the temporary directory and the error identifies it. No-audio exports skip this path unchanged. Actual20s final FFV1/MKV decoded video and PCM are byte-identical to the previous final audio-muxed output;182fullsuite tests passed with optionalOCIOenabled. Tests cover failed mux, stalled process, restore failure, unwind and temp uniqueness. At this historical checkpoint, WAV writing/audio mix, legacy export and image writers remained separate synchronous IO gaps.

### Strict high-bit audio caps
High-bit export alone uses typed strict audio errors: no audio track is distinct from decode failure, invalid controls and resource limits. Legacy mix/decode APIs remain unchanged. Each timeline PCM/decode buffer is capped at the smaller of64MiB and1/16of the configured memory budget, reducing the core mix+raw+converted worst-case peak to192MiB (9.375% of a2GiB box), before renderer/process buffers and small probe overhead. The real20s proof uses24.576MB core peak and has exact prior final decoded-video/PCM hashes. Guard preflight still checks current live memory; hard caps do not reserve the box for audio. Mix/limiter loops check cancellation; ffmpeg capture has a bounded deadline. Local WAV writes remain synchronous even though memory/chunk cancellation is bounded.

### Legacy delivery and PNG writer bounded IO
Legacy8bit SDR export now uses the joined writer owner while keeping parallel batch ordering and the exact libx264 option list. Worker-panic cleanup reaps the encoder; an out-of-order completion test verifies writes remain frame-ordered. Legacy AAC mux uses the staged transaction with the original192kbitrate; old audio mix API and no-audio skip remain unchanged. Actual20s audio/noaudio decodedRGBA and AAC-decodedPCM match the earlier final deliverables exactly. `render_png`, matte/cutout PNG and flow PNG use the same one-frame writer owner and unchanged image options. Alpha-gradient PNG old/new encoded bytes and decoded pixels match exactly; an actual stuck child hits a deadline and is reaped.194fullsuite tests passed with optionalOCIOenabled. At that checkpoint, other color-tool image writers remained separate gaps; the next section records their migration. Audio analysis/legacy decode and local file writes remain separate gaps.


### Color-tool still writer coverage (120/121)
`render_aces2_reference`, `render_gamut_excursions` and `render_exposure_false_color` reuse the crate-visible one-frame PNG writer. `render_scope` and `render_high_bit_still` use the same joined encoder owner directly, preserving the scope raster's existing u8 truncation and the high-bit path's rgba64le input -> rgba64be PNG output. The configured high-bit output transform is unchanged. Command argument assertions cover both paths; tool names, schemas and default color behavior are unchanged. Scope dimensions/shape and allocation sizes are checked before raster buffers are allocated. A stuck child on each still path must hit the deadline and be killed/reaped; malformed byte counts and nonzero exit have explicit regressions.

Deliberate behavior change: a scope encoder that exits unsuccessfully now returns an error. Previously that writer ignored the exit status and could report success. Successful valid scope pixels are unchanged.

Real source footage rendered at320x180 produced exact old/new PNG file hashes for all three diagnostic/reference tools, all four scope kinds and a configured AP0/PQ16-bit still. The scope and high-bit proof also compared decoded RGBA8/RGBA16LE samples exactly. For the four scopes, file SHA256 values were waveform `ffe98824674418c15f89349b3e1f1165da50b124d67b913ad28bcbab8d51d980`, parade `b885fb2dda33ebb65422729c6d538ee25df99702ba7ae6c5c890f15c1bdec8ce`, vectorscope `a01ab497f3ea3ef04bdbdd518bc9448679f96d01cb4fa00f8b45ef54a0b7fb4f`, histogram `f048a72c81b4dc807693cb0081fea3b1d648f3bb648f932b2f2f2481fef6ae10`. The PQ still file SHA256 was `37bdc78943759a0b38db13ad892b782451efdb796968e0d53586bbdc251ce1d5`; its460800decoded bytes had SHA256 `51ec1570d949110bbe4a921ed5057000c53309f110f3d4d426ea313b8bbc85d3`. This is same-engine output parity on SDR-origin footage, not HDR source validation, display calibration or certification. The encoded PQ PNG is not an SDR preview.

198tests passed with the actual optional reference-test gate `KIMO_TEST_OCIO=1` and the pinned dependency available; dependency presence alone does not activate those test bodies. Publication fuzz covered157tools/6101calls with0failures. Timings were recorded per render, without a speedup claim. At that checkpoint, scope raster cancellation and optional reference lifecycle were separate review areas; the next sections record those changes. Encoder deadlines include waiting for ffmpeg file-output completion, but do not make direct Rust file writes interruptible. Legacy audio decode/analysis and local file IO remain separate review areas.


### Scope raster cooperative cancellation (123)
Scope PNG generation checks the call guard before work, every32source rows for waveform/parade/vectorscope, every65536histogram samples, every32histogram drawing bins and every32output rows. Scope formulas, sample selection, raster quantization, encoder arguments and tool schemas are unchanged. An isolated integration regression cancels all four kinds before work and verifies no output file or input mutation. Real-footage PNG file hashes and decoded RGBA8 samples match the121baseline exactly for all four kinds.199tests passed with the actual optional OCIO gate enabled. These are cooperative checkpoints, not a wall-clock bound for every operation between them.

### Optional reference invalid-output lifecycle (124)
The OCIO processor validates returned RGBA shape, finite RGB/alpha and alpha0..1 through the shared frame contract. Invalid output closes the input queue, kills/reaps the child, joins its workers and refuses reuse. Finite signed/HDR RGB remains untouched; alpha0,0.5and1 pass exactly. Invalid input still fails before queueing and leaves a valid processor usable. The production python command, pinned bridge/config, dependency requirements and opt-in/default behavior are unchanged.

Three dependency-free fake-child regressions cover nonfinite RGB, invalid alpha, truncated/stalled output cleanup and no reuse, plus exact extended-RGB/alpha echo after a recoverable input error. Actual pinned OCIO real-footage reference output file hashes were identical before/after: SDR PNG SHA256 `168fed5480361aae731343d938c1ace7990a3ad109cea45bb550830045b33b88`,1s/25frame video-only reference MP4 SHA256 `c12ae14b7bc00d2f15c59780848305a6203644e61193570c810f26f17943bc59`.202tests passed with `KIMO_TEST_OCIO=1` and the pinned dependency available; publication fuzz covered157tools/6109calls with0failures. The reference artifacts were visually checked as well. This is lifecycle and same-engine output parity evidence, not independent ACES/HDR certification or proof for every subprocess failure mode.


### Speed-ramp source windows and overrun policy (127/128)
Speed-ramped media now decodes only the requested source-frame range instead of the whole clip for each spot/export preparation. Requested display times map through the ramp into source indices; the speed1ffmpeg trim path remains anchored at the clip in-point. Cache `first`, coverage, media lookup and flow diagnostic indexing use that source domain consistently. The shared `Clip::source_index` adds1e-6to the source-frame position before flooring, avoiding a preceding-frame selection caused by tiny floating-point boundary error. Source-frame count follows consumed source time rather than output duration, fixing fast-ramp truncation. The same per-active-media cache budget clamps ramp lookahead. Ordinary constant-speed behavior and tool schemas are unchanged.

A ramp window whose requested source frames are absent returns an explicit error naming the clip, requested range and decoded range/count, with shorten-ramp/clip guidance. It does not silently freeze the last source frame. Missing speculative lookahead alone is not an error; an earlier valid window remains usable even if the clip would overrun later. Two regressions cover normal/reverse/freeze, slow/fast ramp cold/full/sequential-export parity, early valid windows, partial valid lookahead and late source overrun. Five real short-ramp PNG file hashes matched the previous whole-clip path exactly.204tests passed with the actual optional OCIO gate enabled on the128review tree;128publication fuzz covered157tools/6117calls with0failures.

Synthetic memory proof:3.5s/25fps2560x1440source and canvas, managedAP1/BT709input with native linear resampling, speed ramp0.5->1.4->0.5. Spot high-bit renders at0,0.96and3.4s succeeded in1.753,2.993and8.311s. Main-process high-water RSS reached381144KiB; final RSS208480KiB. These figures exclude ffmpeg child RSS. The old88frame request had a conservative decode estimate of10380902400bytes, exceeding the256MiB cap; the window budget admitted at most two source frames with235929600bytes estimated. This is a synthetic bounded-cache proof, not the production scene's total process-tree memory or a speedup claim. Late windows still decode source prefixes and can take seconds. Whole-project `prepare(None)` can still reject oversized full ranges; bounded spot/export preparation is the fixed path. The separate tracker source-index change is owned by the tracking module.

### Output-transform parallel experiment: no-op (130)
The warmed1080p high-bit still baseline was1.377s end-to-end; cold median2.074s. A scratch direct-call phase probe on the same2logicalCPU Xeon2.60GHz box measured warmed preparation0.00005..0.00010s, float render0.235..0.294s, configured output transform0.372..0.488s and RGBA16packing0.038..0.051s. Those timings include no PNG encoder work. Source was the same SDR-origin media fixture, AP1 working/BT709 input and configuredAP0/PQ output. The earlier baseline label calling gamut4Rec2020 was incorrect: gamut4is AP0; Rec2020 is gamut1. The measured settings were not changed to fit that label.

A separate scratch1080p synthetic valid-frame probe compared the sequential output transform with the same per-pixel matrix/PQ arithmetic split across two scoped standard-library workers. Six sequential samples were0.557688,0.538634,0.541903,0.526505,0.526978,0.535777s; parallel samples0.282176,0.267556,0.276241,0.284842,0.272897,0.283150s. Float output on the fixture matched exactly. This is roughly2x local transform compute, not an end-to-end still speedup or a broad benchmark.

The experiment was discarded. Main-process high-water RSS was99628KiB sequential versus99864KiB parallel,236KiB higher, failing the no-higher-peak gate. Sequential conversion also stops at the first overflowing output channel after mutating only the preceding pixel/channel prefix. Parallel chunks can mutate later pixels before that earlier failure, changing error-state semantics. A full extra preflight or transactional output buffer would be a different scope with added compute or memory. No production implementation, dependency, tool schema or default changed. No three-resolution PNG parity or publication speedup claim is made for the discarded experiment; the sequential path remains in use.

## Explicit ordinary-video delivery (COLOR132)
Ordinary H264 export uses explicit BT709 RGB-to-YUV coefficients and limited range at every resolution, with Rec709 primaries, BT709 matrix and exact transfer signaling (disabled display-sRGB projects use sRGB; enabled Rec709 output supports linear/sRGB/BT709 OETF). Other managed ordinary-output domains fail before encoder spawn; BT601 delivery is unsupported. High-bit non-Rec2020 HLG now fails closed like non-Rec2020 PQ. This does not change project rendering/defaults or make lossy H264/4:2:0 exact at chroma edges. Migration: legacy MP4 delivery pixels changed from implicit conversion; re-export older MP4s for the explicit contract.

## Opt-in `film_finish` (COLOR133)

This is a display-sRGB/Rec709 creative finishing effect, not a calibrated film
stock, ACES output transform or HDR mastering effect. It leaves old effects and
project/export defaults unchanged. Use `add_effect` with kind `film_finish` on
footage, an outer precomp clip, or an adjustment layer. Place it below titles if
the titles should stay clean. In a managed linear project, configure the existing
`set_effect_encoding` wrapper with Rec709 gamut0 and sRGB transfer1; never apply
it directly to log, PQ, HLG or unconverted scene-linear data.

Controls: bloom0.12, halation0.08, grain0.025, vignette0.12; threshold0.7,
knee0.15, radius0.018 (fraction of shorter output edge, bounded512px),
grain_size1.5 (output pixels), vignette_softness0.6. All four amounts0 is exact
identity. This preset is mild; shot-specific tuning is expected.

Order: highlight extraction from the PRE-GRAIN image, alpha-weighted linear-light
blur, neutral bloom plus red-biased highlight rim, vignette, then luminance-shaped
monochrome grain LAST. Alpha is unchanged; fully transparent pixels do not emit
light and their stored RGB is unchanged. RGB is not clipped, so later delivery can
still clip highlights. The rim is an artistic approximation, not physical
emulsion scattering. Grain size is in output pixels (not scene/world units).

Grain uses the existing render seed `round(t * fps)`. On an outer clip or adjustment
layer this is absolute timeline frame: a cold bounded frame matches the same full
export frame, including a clip that starts later. Inside a nested precomp the
existing renderer uses its local clock, so the seed is local, not absolute.
Apply finishing to the OUTER precomp/adjustment for absolute-frame behavior.
The proposed absolute-seed propagation follow-up is not part of COLOR133.

CPU-only. Three-pass separable box blur approximates smooth scattering; no GPU or
real-time playback claim. Effect validation and output use a temporary buffer;
invalid inputs/params and output overflow return errors without partial edits.

## Preview-only semantic shot matching

`preview_subject_match` fits the existing subject_match core to two explicit
clip/time samples and writes before/after PNGs. source_regions/ref_regions name
mask-layer IDs in skin/background/lighting slots. Absent regions are disabled;
source and reference must name the same nonempty set. This is caller-designated
semantic sampling, not skin detection or estimated illumination. Overlaps with
sum>1, empty/undersampled active regions, unknown keys and deleted IDs are errors.
Options: amount, three region_amount values (missing slots must be0),
regularization, contrast_amount, max_exposure_stops and three exposure_target
entries (reference, preserve_source, or explicit linear luminance).

First version requires identity clip transforms; rejects 3D/pins/corners,
precomp/adjustment, transition/echo/motion path/clipblur/posterized timing and
masked effects. Semantic masks are detached from clip visibility in a scratch
project, not allowed to crop the preview. Reference-matte layers are not supported.
Existing clip effects are included. Mask coverage uses clip-local time on the
untransformed isolated image. Disabled layers are rejected. Coverage numerical
noise within1e-6 of0/1 is bounded; meaningful invalid weights are rejected.
Managed working RGB is explicitly converted to Rec709/sRGB for analysis; samples
outside the subject_match [0,1] display domain fail, not silently clipped.

The project is unchanged. There is no stored recipe, renderer wiring, automatic
mask tracking or reference dependency at playback. This tool is a preview, not a
persistent shot-match effect. Soft masks and low amount are important: a hard
sample rectangle can make a visible grade boundary. Review actual pixels before
accepting a look. Fit statistics describe this sample pair, not a quality score.

## Explicit pointwise pipeline LUT bake

`export_color_pipeline_lut` adds an ordered explicit steps bake, separate from
existing clip-only export_grade_lut. fields: steps (1..64), path(.cube),
size17/33/65(default33), time(frozen parameter time, default0),
domain_min/domain_max (three finite numbers, default[0,0,0]/[1,1,1], within+-16).
Steps:
- {type:effect, kind:exposure, params:{stops:{base:0.5}}}; params use Kimo Prop
  JSON (base, optionalkeys/expr). Only supported pointwise kinds; wrapper encoding
  supported. Explicit color_managed steps name gamut/transfer conversions.
- {type:ocio, config_path:..., source:ACEScg, destination:ACEScct}
- {type:ocio_view, config_path:..., source:ACEScg, display:"sRGB - Display",
  view:"ACES 2.0 - SDR 100 nits (Rec.709)"}
OCIO steps require native-ocio and installed OCIO2. Self-contained configs only.
The chain order and source/output domains are caller declarations; the tool does
not read/apply project defaults or guess color encoding. Configured OCIO views
include looks. Existing project and export behavior unchanged.

A static3D RGB LUT cannot represent grain, bloom/halation, vignette, denoise,
matte-dependent grades, tracking, motion or automatic per-shot analysis. These
are rejected, not silently omitted. Animation freezes at the explicit time.
Alpha behavior is not encoded. Red changes fastest in standard.cube data order.
Output is finite and unbounded up to parser1e6 limit; inputs outside domain clamp
on Kimo reapplication. Other apps may clamp outputs or interpolate differently.

Tool reports deterministic4105pixel tetrahedral error metrics from the serialized
LUT against direct transform: max, p99, RMS. These probes are not an error bound
on every pixel. Uniform grids lose accuracy at steep nonlinear knees/log toes;
encoding/HDR domains may need a separate shaper workflow (not built here).
ActualOCIO2.5.1 measured17/33/65 ACEScct max0.07861/0.04254/0.01371;
ACES2sRGBview max0.08251/0.04506/0.03643 on declared0..1AP1domain.65view RMS
0.000740 and p990.002404. Affine +0.5stop max1.19e-7 atall3sizes. This is export/
parse/reapply evidence, not lossless transfer, display calibration, crossapp
conformance or an independent ACESimplementation. Review errors before reuse.

## Parallel color CPU pass

The existing par worker selection now splits immutable nativeOCIO CPU processor
application, LUT application, ordinary linear_grade and basic pointwise effects
into disjoint pixel ranges. Math/order within each pixel is unchanged. No shared
render/default/schema changes. Serial fallback for extreme linear-grade values
preserves its existing first-overflow partial-write behavior. NativeOCIO remains
transactional with perworker diagnostics and unchanged alpha; processors are not
edited while workers run. No dynamic-property mutation API is exposed here.

Real-film10 frame upscaled to1920x1080, releaseLinuxx86_64, OCIO2.5.1,
prepatch177506a vs patched8workers: ACES2view warm~1170->608ms (~1.93x),
LUT65apply~132->57ms (~2.31x), ordinarylineargrade~145->87ms (~1.66x),
saturation~6.7->5.2ms (~1.29x). Separate patched1worker comparisons included.
Exact rawRGBAfloat SHA256 matches prepatch/patched1/patched8 for all4paths.
Timing is thisbox/thisfixture, includes existingpreflight/clone work, excludes
filewrites, no realtime/crosshardwarepromise. LUTfirstcall parse slower thanwarm.

## Native ACEScct lift/gamma/gain/offset

`add_acescct_primaries` adds an opt-in effect to managed AP1/D60 scene-linear
clips (working_gamut3). Required: clip_id, config_path, linear_space, cct_space.
Caller declares the config's AP1linear/ACEScctspaces; names alone are not encoding
proof. NativeOCIO2 required, self-containedconfigs. No defaults/outputchanges.

Numeric params: lift_r/g/b(default0, -1..1exclusive), gamma_r/g/b(default1,
0.1..10), gain_r/g/b(default1,0.01..10), offset_r/g/b(default0,+-1), inverse0/1.
Exact Kimo formula in encodedACEScct: y=signedpow(x+lift*(1-x),1/gamma)*gain+offset.
Inverse: x=(signedpow((y-offset)/gain,gamma)-lift)/(1-lift).
This black-pivot/white-anchor lift, signedgamma, gain thenoffset is explicit,
NOT a promise of Resolve/Baselight wheel semantics or perceptual hue preservation.
Neutral controls are bit-exact identity after config/processor validation.

RealOCIO convertsAP1->ACEScct beforegrade, ACEScct->AP1after. Alpha unchanged;
invalid parameters/config and nonfinite/overflow outputs fail transactionally.
Boundlinear gamut3 prevents staleprojectworkingdomain use. Separate encoding or
OCIOdomain wrappers rejected; effect owns its sandwich. Config reloaded/recompiled
percall (not cached/throughput-qualified). RGBnotclipped, laterdeliverymayclip.

Knowninputs include signed/toe/HDRsamples; independentLGG math verified to2e-6,
forward/inverse to2e-5 float tolerances, repeatedoutputstable. ActualSDRfootage
explicitlydecoded sRGB->AP1, graded, viewed through officialnativeACES2sRGBview;
before/afterpixels visually inspected. No camera/HDR/displaycertification.

## ASC CDL .cc/.cdl interop

inspect_asc_cdl(path) parses bounded1MiBXML; apply_asc_cdl(clip_id,path,id?,encoding)
imports a selected correction as immutablepayload asc_cdl effect;
export_asc_cdl(path,source_path or clip_id+effect_id,id?) canonicalexports it.
Multiplecorrections require selectionID toapply/.ccsingleexport. Slope/offset/
power/saturation doubles,IDs andsupporteddescriptions survive semanticroundtrip;
XML whitespace/order/comments/namespaceversion are not preserved byteforbyte.

Strictprofile: .cc ColorCorrection or .cdl ColorDecisionList with inline
ColorDecision/ColorCorrection, SOPNode/Slope/Offset/Power, SatNode/Saturation,
Description plus InputDescription/ViewingDescription. Empty/duplicate IDs,
numericcount/nonfinite/rangeerrors, duplicatefields, unknownchildren/attributes,
externalreferences, DTD/entitydefinitions, mixednamespaces rejected. Namespace
none, urn:ASC:CDL:v1.01 orv1.2 accepted; canonicalexportv1.2. Metadata outside
thisprofile rejected rather than lost. Slope/sat>=0,power>0, valuesbounded100.

NativeOCIO2.5.1 CDLTransform uses explicit CDL_ASC clamping (librarydefault
NO_CLAMP is NOTused): clamp(input*slope+offset,0,1)^power, BT709weighted
luma/saturation, finalclamp0..1. Alpha unchanged. Existinglegacycdl effect is
unchanged. NoFileTransform assetpermissionbypass; nativeCDL processorbuilt from
strictparsedvalues. Applyrequiresnativefeature. Encodingcurrent applies directly
to callerdeclared currentencoding; encodingACEScct requiresmanagedAP1 and explicit
config_path/linear_space/cct_space, convertingAP1->ACEScct->AP1 throughnativeOCIO.
Domainnames callerdeclared, notinferred. Boundgamut guard+wrapperconflicts fail.
ImportedXMLfile notneededafterapply; ACEScctsandwich stillrequires itsconfigfile.

Knownsigned/clippedinputs match independentASC formula to2e-6; repeatednative
outputexact, semantic.cc/.cdl roundtrip tested, sourcefiledeletedthen effectexport
retainsvalues. RealSDRfootage decodedsRGB->AP1, importedCDL inACEScct, official
ACES2sRGBview before/afterpixels inspected. NoCDLofficialconformancecertificate,
Resolveparity, HDRcamera/displaycalibration orallXMLdialects claim.
Reference nativeclampsemantics:
https://opencolorio.readthedocs.io/en/latest/api/transforms.html
XMLinteropexamples (secondaryimplementation):
https://cdl-convert.readthedocs.io/en/latest/usage_cc.html

## Strict high-bit delivery contract

Disabled high-bit exports previously omitted conversion/tags and could use
FFmpeg's default RGB-to-YUV matrix. They now declare Rec709/sRGB with BT709
limited-range YUV for HEVC10, or full-range identity RGB for FFV1 16. Managed
Rec709/Rec2020 supports exact linear/sRGB/BT709, plus Rec2020 PQ/HLG. Other
gamut/transfer pairs fail BEFORE encoder spawn rather than exporting untagged.
Gamma2.4 is not mislabeled BT709. Select a supported output pair or use float
EXR/TIFF for working/log interchange.

`inspect_high_bit_delivery_contract(format)` exposes pipe/output bit depth and
tags. The RGBA64LE pipe uses 8 bytes/pixel and retains 16-bit quantization.
HEVC converts to 10-bit limited-range YUV420; FFV1 preserves 16-bit RGB. Neither
path calls to_rgba8. Integer delivery clamps 0..1, so a lossless codec does not
restore out-of-domain radiance. Chroma subsampling and HEVC quantization are not
lossless. Native OCIO display/view is not a stored project output declaration:
there is no automatic view-name-to-tag guess and no native view video tool here.

## Explicit native OCIO view video

`set_ocio_view_delivery(selection)` persists config_path/source/display/view,
input_gamut, output_gamut, output_transfer as a separate optional selection.
It requires managed scene-linear pixels and a matching working/input gamut;
the named OCIO source must represent that gamut and white point. Caller declares
the actual view output gamut/transfer. Neither display nor view names select tags.
Config must be self-contained and a real OCIO 2.5.1 display/view pair. Validation
is transactional. Export revalidates current working gamut/config, so changing
working gamut makes the old selection stale instead of silently converting it.

`export_ocio_view_video(path,format,crf)` streams rendered working pixels through
that native display processor including looks, WITHOUT the project output
transform, to the existing RGBA64LE delivery pipe. Formats: hevc10/ffv1_16.
Tags/conversion obey the strict delivery contract. Ordinary exports deliberately
ignore the optional view selection. Audio follows the shared high-bit path.
Existing project mastering metadata is not inherited: the new view may differ.
Configs are loaded fresh at export; replacing a config file changes the result.
Integer outputs clamp 0..1; the result reports excursion pixel count across
frames. No alpha channel is retained in the video codecs. Source gamut/white
and output encoding are declarations, not validated measurements of a custom
OCIO config. Math/reference agreement does not certify a display or HDR mastering.

## Film-style tone response

Explicit `add_effect` kinds: `film_soft_shoulder`, `film_luma_grain`. Params are
keyframable. `inspect_film_response` reports curve/density samples. This is a
parametric look, not measured stock. Shoulder is per-channel signal math:
for x>knee, knee+(peak-knee)*(1-exp(-(x-knee)/(peak-knee))). Identity otherwise.
C1 at knee, monotone, asymptotic; amount1 bounds positive output, partial amount
only blends. Negative signal is unchanged. It can shift hue; use the existing
scene-linear luminance_shoulder when preserving RGB ratios is desired.

Grain occupancy = highlight_density+(shadow_density-highlight_density)*
(1-clamp(luma,0,1))^falloff. Rec709 display-signal luma is used, shadow>=highlight
required. Independent deterministic uniform hash chooses occupied cells then
symmetric additive amplitude. The expected unclipped mean is zero, not the
exact mean of a finite frame. Output is unbounded and integer encoders may clip
negative grain or highlights, shifting the mean. `grain_size` makes blocks, not
filtered scanned texture. Alpha0 pixels skipped, alpha untouched; seed is outer
frame (nested precomp uses local frame seed), so time changes the pattern.

Native OCIO applies when explicitly chosen: add_effect strings
ocio_domain=display-referred, ocio_config=<self-containedconfig>,
 ocio_working_space=<actualcurrentspace>, ocio_linear_space=<linearsource>,
 ocio_display_space=<invertiblecolorspace>. Existing native OCIO processor
sandwich is reused; this is not an inverse ACES display/view and cannot undo
ODT clipping. Alternatively an explicit finishing workflow applies these effects
after a real forward native display transform. Existing exports, native view
video and legacy film_finish are unchanged. Only caller declarations set the
domain. Measured f32 shoulder rounding max2.981e-8 on100001samples0..10;
grain additive amplitude is bounded by amount before f32 rounding, tested
[-4,4] default delta<=0.0250003. These are sampled numerical checks, not a
calibration or all-finite-input proof.

## Per-clip grade graph model

A clip may persist `grade_graph`: nodes keyed by stable caller-supplied ID,
output ID (reserved `input` bypasses graph), cache_budget_mb. No graph leaves
legacy effects untouched. Graph input is AFTER the clip's existing effects;
output feeds its existing masks, geometry and composite. Adjustment-layer
nodes follow existing adjustment effects. Nested clips carry their own graphs.

Node JSON: id, kind (`grade`/`parallel`/`layer`), ordered inputs, weights (one
Prop per input), effect (grade only: id/kind/params/strings), optional MaskLayer,
bypass. Grade accepts one input. Parallel normalizes nonnegative0..1weights
and averages RGB; layer uses first input as base, then blends later RGB inputs
by their weights in order (first weight ignored). First input alpha is exact.
These are defined kimo mixers, not claimed Resolve mixer semantics. Mask/key
coverage is computed on first input and blends node output to it. Mask kinds:
rect/ellipse/path/linear/radial/luma/color/hsl/alpha/noise; no cross-clip mattes.
Mask props/path keys are fully editable. `grade_node_tracking` imports normalized
cx/cy/rot samples into rect/ellipse mask clip-local keys; not a tracking solve.

MCP lifecycle: add_node -> connect_nodes -> set_grade_output. Grade defaults
parent=input; mixers initially parked. Connections replace/append indexed
slots; disconnect removes slot and weight. Remove referenced nodes fails until
rewired. `set_grade_node`/`set_grade_graph` replace full JSON transactionally;
null graph clears it. Full Prop payloads expose base/keyframes/ease/bezier/expr,
effect strings expose explicit native OCIO domains or legacy wrappers. Node
bypass forwards first input. Incomplete reachable nodes error at render;
parked nodes may be edited, but cycles/missing references anywhere fail.

Supported grades are a conservative color list reported by inspect_grade_graph;
backdrop/spatial/matte-dependent effects not silently admitted. LUT nodes are
not yet included. Graph does not infer working encoding. Existing bound gamut
and wrapper reference-white checks apply to node effects before rendering.
`evaluate_grade_graph_samples` reports actual evaluation-local cache stats on
caller-declared float RGBA samples, not media decode. Serial exact parity and
native OCIO reference proofs apply within tested domains, not certification.

Cache computes each reachable node once, then frees frames after last consumer.
Default256MiB/max1024MiB,64nodes/16inputs. A graph needing a wider live frontier
errors; callers can raise budget or simplify graph. Budget is cached frames,
not all effect/mask scratch; global guards remain. No cross-frame result reuse,
so source/params/config/seed changes cannot hit stale frame results. Static
layer cache is disabled for graph-bearing clips. No group/shared/timeline node
system or UI added in this phase.

## Shared, group, timeline grades

Project grade_scopes persists graphs (named shared Graph definitions),
clip_bindings (clipID -> ordered graphIDs), groups (ordered id/clips/graphs/
bypass objects), timeline (ordered graphIDs). Full replacement via
set_grade_scopes is transactional, validates all graph/clip references and
managed bindings. create_shared_grade_graph starts identity unless fullJSON
provided; existing12node tools choose exactly one clip_id/shared_graph_id.
inspect/evaluate works for both. remove_shared_grade_graph refuses referenced
removal; bound clip deletion fails until its refs/membership are removed.
Registry capped128definitions/128groups/16graphrefs perchain; eachDAG existing
nodecachebudget applies independently. Definitions share, framepixels don't.

Render clock/order: clip legacyeffects, localDAG, sharedrefs, matchinggroups in
storedorder, then clipmask/transform/composite. Adjustmentgraphs grade backdrop
through existing adjustmentmask/opacity/duration. Group members are individual
clips, NOT groupedflattenedcomposites; multiplegroups stack deterministically.
Finaltimeline chain grades compositedworking pixels before declaredprojectoutput
or nativeview; it is not a displayfinish afterODT. ExplicitOCIO nodewrappers work.
Timeline clock/seed is compositiontime/frame; clip/group parameter keys use each
clip's localclock. Nestedcompositions/matte renders carry matching clip/group
bindings but timeline suppressed to avoid repeatedglobalgrade. Staticlayercache
is disabled for scopedclips, so live shared edits cannot hit cachedstalegrades.

No inferred membership/domain; IDs/order/bypass are explicitMCPdata. split_clip,
split_all_at, insert_gap inherit bindings/groupmembership on rightsegment;
other editorialcut operations can introduce freshunboundIDs, caller explicitly
assigns those. Deleting bound members requires clearingreferences first. Empty
scopes gives previouslinearpath. Testscover multipleclipsharededit, noncommuting
grouporder, clocks, adjustmentgraph, nestedtimelineonce, exactrealserialreference,
missingrefs/rollback, split/delete, nativeOCIO and JSONroundtrip. No groupcache,
looksuperiority or industrysemanticparity claim.

## LUT and spatial grading nodes

File .cube node: add_node kind=grade effect_kind=lut strings.path=<file>, numeric
intensity (0..1) editable/keyframable. Native OCIO wrapper strings remain explicit.
Inlinegenerated node: generate_lut_node with clip_id OR shared_graph_id, unique
node_id, optionalparent, steps (existing pointwise/nativeOCIO bake schema), size17
or33, frozen time/domain. Stores serialized .cube in strings.cube_text and reports
serialization/interpolation probe error. FullJSON can edit/replace/remove it.
path+cube_text together errors. Output not auto selected. Generated data self
contained; file nodes keep existing external size/mtime-cache limitation.

The pointwise bake rejects spatialchains; measured4096random+9boundary probes
are not all-pixel error bounds. Nonlinear nativegamma17cube has visible numerical
error nearblack; caller chooses33or existing65filebake if needed. Inputdomain
clamps; output may exceed0..1. No auto camerainference/lookcertification.

Blur/sharpen are grade effects but spatial: full branch frame processed, then
node mask blends with firstinput, alpha restored to exactfirstinput. No DAG
optimizer reorders them or assumes commutativity. Orderedserialbranch parity
against directeffect RGB is tested, as are maskedalpha preservation and
noncommutingblur/nonlinearhue order. Cacheevaluates sharedancestorsonce, effect
scratch outside cachedframe budget. Radius0..512, sharpenamount0..3, keyframed
invalidvalueserror. No backdropglows/compositor or arbitrary spatialeffects.
Curves/hue-vs-hue/hue-vs-sat/hue-vs-lum/log wheels were already supported and are
now tested for exactdirectgrade parity rather than duplicated with new math.
