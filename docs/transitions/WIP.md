# Raw software transitions, source-only WIP

Base 2977b73751f1c9124501503e3cba0b9c33cc9413, branch transitions. No main edits/PR/merge. No cargo, functional tests, benchmarks, warnings comparison or frame renders executed. Do not label unit-verified or visually ready.

## Provenance

Adaptation of MIT gl-transitions algorithms, pinned b2dfdcfbec079af4a05306d32b9366aa495c5155. SOURCES.json records exact raw URLs and byte-verified SHA256s. Root LICENSE and each of four file headers checked. Root notice retained in GL_TRANSITIONS_LICENSE.txt. Per-file authors: luma gre; directionalwarp pschroen; CrossZoom rectalogic, port gre, glfx.js/rectalogic easing lineage; displacement Travis Fischer, adapted from Robin Delaporte/Codrops. No assets bundled. Prior source exposure means this is NOT clean-room.

CrossZoom and displacement headers point to additional upstream lineage. Full upstream licensing/notice reconciliation remains pending before promotion of those ports. File-level MIT declarations alone are not a claim that every linked upstream asset/dependency has been cleared. No restrictive code inspected/copied.

## Contract and deliberate differences

Original Rust software sampler renders actual Frame pixels, no GPU/interpreter dependency. Pixel-center bilinear filtering and crossfade accumulate premultiplied RGBA, then return straight alpha. Signed HDR RGB remains unclipped; alpha0 interior pixels become black by declared policy. Endpoints bypass filtering and preserve input bytes, including hidden RGB. Caller supplies matching color domains; no automatic transfer conversion. Clamp/repeat/transparent borders explicit. Scalar maps use raw red, not alpha-weighted color or implicit luma conversion. Inputs max4096x4096, row cancellation checks, checked shape arithmetic, finite controls, RGB overflow returns error. MCP transition_render_raw returns frames, not a timeline adapter; large JSON buffers may hit existing request/response limits, so it is not yet a practical 1080p transport.

Luma: progress, smoothing; exact source comparison when smoothing=0. Directional warp: nonzero continuous direction normalized with safe scaling, smoothing, independent UV distortion. Cross zoom: explicit center, strength and 1..128 midpoint samples, linear dissolve. No source shader's jitter, implicit animated center, sinusoidal strength envelope or exponential easing. Displacement: scalar-map red minus neutral, independent x/y strength, linear progress. These are controllable primitives, not pixel-identical shader compatibility or presets.

## Required gates

1. Serial compile and functional test slot only after parent grants it. Suggested: CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test transition_raw -- --test-threads=1 . Compare warning diagnostics against same-profile baseline.
2. Expand sampler oracle tests for repeat/transparent borders, partial-alpha texture edges, odd dimensions, invalid buffers, overflow and timeout. Existing draft tests cover endpoints, transparent hidden color/HDR, luma hard threshold, displacement neutral, singular/extreme directions.
3. Actual Rust 1080p render: KIMO_TRANSITION_FRAMES=/tmp/kimo-transition-frames CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test transition_raw render_1080p_contact_sources -- --ignored --test-threads=1 . Produces PPM checker/gradient/alpha sources at nine times for all four operators. Assemble contact sheets and clips with ffmpeg, inspect pixels, attach to parent. PPM is display-clamped evidence only, not HDR-preservation proof. Add linear-float numerical fixtures separately.
4. UV adapter plus transition timeline integration still pending. Run real project alpha/texture/motion scenes before claiming renderer improvement.
5. Criterion only after compile+functional pass. No speed, alignment or SIMD claim.

## Crossfade and slide additions on cbc468b (source-only)

Crossfade is original direct per-pixel premultiplied linear interpolation of RGB and alpha, returning straight RGBA without clipping signed HDR. No bilinear resampling. Slide is original direct UV translation with caller-supplied from_offset and to_offset vectors: sample A at uv+t*from_offset, B at uv+(1-t)*to_offset, then linear premultiplied blend. No hidden easing, auto direction normalization, macro or preset. Slide is not the tiled ownership-based push and does not promise gutter-free rigid tiles; caller chooses clamp/repeat/transparent border. Both operators share exact endpoint bypass, alpha-zero interior black policy and finite validation. Offset values are finite; arithmetic overflow is rejected by sampler. No new third-party source imported.

Draft tests cover endpoints, unequal-alpha analytical blend, hidden RGB at alpha0, zero-offset equivalence, direct one-texel translations, all edge policies and nonfinite/extreme offsets. UNCOMPILED and UNEXECUTED. Existing four operators and render_1080p_contact_sources are intentionally unchanged; the new harness has a separate exact test name so the planned four-port run need not change.

After the existing four-port run and serial compile/functional gates, run:

```sh
KIMO_TRANSITION_FRAMES=/tmp/kimo-transition-extra CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test transition_raw render_1080p_crossfade_slide_sources -- --ignored --exact --test-threads=1
ffmpeg -y -pattern_type glob -i '/tmp/kimo-transition-extra/crossfade-*.ppm' -vf 'scale=480:270,tile=3x3' -frames:v 1 /tmp/crossfade-sheet.png
ffmpeg -y -pattern_type glob -i '/tmp/kimo-transition-extra/slide-*.ppm' -vf 'scale=480:270,tile=3x3' -frames:v 1 /tmp/slide-sheet.png
```

Keep original 1080p files for crops and endpoint comparison. Inspect actual sheet pixels before reporting ready. These are recipes, not rendered evidence. PPM is display-clamped; float numeric tests own HDR-preservation evidence.
