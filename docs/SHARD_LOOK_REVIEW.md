# Shard-look review (opt-in, no default changes)

Base: 894543f. No new dependency or render IO.

## Controls on add_shard_burst config

- `material`: `legacy` (default), `glass`, `concrete`, `metal`.
- `size_bias`: default 1, 0.125..8. Log-uniform at 1; 2.8 makes fine chips more common.
- `size_scale`: default 1, 0.01..4. Reduces every fragment dimension. Shared between analytic and physics sizing. Does not change pane-cell geometry, which is governed by `plane.cells` and `plane.grading`.
- `env_reflection`: default 0, 0..1. World-space analytic sky/ground and softbox, evaluated once per facet using its normal and vector to the camera eye. Alpha remains exactly unchanged. Reflections pass through existing occlusion, lifetime fade and over compositing. Zero returns input RGB without arithmetic.
- `env_roughness`: 0..1, default 0. Widens and weakens the softbox. This is an analytic look, not a loaded HDRI or full microfacet BRDF.

Fine glass starting point:

```json
{"material":"glass","count":450,"size_min":2,"size_max":24,"size_bias":2.8,"size_scale":0.75,"thickness":[0.02,0.05],"env_reflection":0.85,"env_roughness":0.15,"brightness":1.0}
```

Glass has low coverage opacity, pale tint and glints; concrete is opaque diffuse with seeded facet variation; metal is opaque with tinted reflection and stronger glints. Concrete/metal do not refract the background. Texture-bearing panes retain texture mixing; reflection is suppressed before birth so the unbroken surface remains unlit by this look.

## Proven locally

- Four pure-function tests: default-size identity, zero-reflection identity, fine distribution, nonfinite/hostile controls, deterministic/orientation-sensitive environment.
- Five renderer tests: exact zero-reflection frame identity for all four materials, unchanged alpha and no changes outside shard coverage, repeatable analytic/physics renders, hostile controls, fully covered occluder suppresses reflection, evidence generation.
- Original 894543f legacy float RGBA render compared against this version with `cmp`: byte identical at 640x360, t=0.55, seed=21, 180 shards, blur off.
- Debug and release renderer tests pass. Release fixture at 640x360, blur off: about 29-39 ms per frame (includes float/PPM file writes). This is not a real-footage or production-speed benchmark.
- Visually inspected the six-panel comparison: fine glass, glass with/without reflection, concrete, metal and unchanged legacy.

Found existing focal-depth variable shadowing: `fl` becomes pane flatness (0 for a free burst), making falloff division invalid. Opt-in material shading uses the camera focal depth instead. Legacy retains its old expression for compatibility. This does not fix the old depth-of-field expression.

## Limits

Synthetic burst scene only. No source footage supplied here, so no claim about real-shot integration, tracked-pane continuity, or final SCUMBLE quality. Materials are selectable per burst; use separate bursts for mixed materials. Physics thickness defaults are unchanged. Tiny chips can still be lost below raster coverage, so increase size_min when rendering low resolution. New control sanitization does not replace the existing global sandbox for unrelated old hostile parameters.

## Reproduce

```sh
cargo test -p kimo-core --lib shard_look
SHARD_LOOK_EVIDENCE=/tmp/shards cargo test --release -p kimo-core --test shard_look_render -- --nocapture --test-threads=1
```
