# Matte-guided shot matching: review-only prototype

Base: ee9f90871ebe472233a8bc51cc493210cb9e61d5. New files only.
This is not registered in MCP and does not change the renderer or defaults.

## Contract

`subject_match::statistics`, `fit` and `MatchPlan::apply` take straight RGBA
float frames declared as display-sRGB/D65, with finite channels in 0..1.
The caller supplies one `[skin, background, lighting]` weight triple per pixel.
Weights must be finite in 0..1 and sum to at most 1. The remainder is untouched.
The lighting region is caller-selected pixels representing lighting, not an
estimate of physical illumination. These are three independent region matches,
not a decomposition into reflectance and illumination.

Skin and background may come from existing masks or a segmentation model,
but this prototype does not detect skin, validate a mask's semantic accuracy,
track a face or resolve occlusion. Source and reference may have different
sizes and different mattes. A reused plan needs new tracked mattes per frame.

Each region reports complete alpha-weighted Lab mean/covariance and
log2 linear-luminance mean/variance. Welford accumulation is row-major,
without sampling, random seeds or threading. Fully transparent pixels do not
contribute; fractional alpha contributes proportionally. Kish effective sample
size prevents a single heavily weighted sample from passing as a rich region.
Enabled regions require at least four effective samples on both shots.
Missing regions fail rather than borrowing a global correction. Disable them
explicitly with `region_amount = 0`.

Chroma matches the full 2x2 covariance of Lab a/b using the existing
regularized optimal transport solver. L is intentionally excluded from that
transport. Exposure and contrast instead match log linear luminance. Each
region can target the reference, preserve source exposure, use an explicit
geometric mean linear luminance, or offset reference exposure in stops.
A bounded log contrast ratio avoids flat-sample noise explosions.

Soft region masks blend Lab deltas with the untouched remainder. Global and
per-region amount both apply. Zero amount is a bit-exact no-op after complete
input preflight. Alpha, fully transparent pixels, disabled and unassigned
regions are untouched. Exact identical-shot statistics use an identity path.
Input preflight catches shape overflow, nonfinite channels and invalid weights
before in-place mutation. All fitting controls are validated even at zero amount.
Cooperative guard checks support the existing cancellation model; a cancellation
panic during an in-place pass can leave partial output, so callers need the
existing dispatch rollback or must discard the partially modified frame.

SDR gamut compression preserves exact D65 linear luminance while reducing
chroma. Per-pixel exposure changes are bounded (default 4 stops, maximum 8),
and output luminance cannot exceed 1. True black stays black. Consequently
creative targets, sample moments and covariance are not guaranteed exact after
soft-mask blending, safety bounds, gamut compression, the black floor, or clipping.
This is neither HDR/scene-linear processing nor camera calibration. It makes
no creative automatic target selection, skin-tone correctness or industry
parity claim.

## Integration review

The module itself already uses `crate::{color, effects, guard}` and is ready to
be registered with `pub mod subject_match;` in color.rs by the engine owner.
The isolated integration test/example use `#[path]` and re-export the core
modules at their crate root, so they prove the new module without changing
lib.rs/color.rs. A future MCP wrapper needs an explicit matte source and
exposure policy; do not silently treat a hue qualifier as automatic skin detection.
No schema, registry, render.rs, dependency or default was changed.

## Proven results

- `cargo test -p kimo-core --test subject_match --test color_matching`:
  16 new focused tests plus 9 existing Lab tests passed (25/25).
- New tests cover identity, repeatable output, weighted means/covariance,
  weighted log lighting moments, alpha exclusion/preservation, independent
  regions, soft blends, disabled/missing regions, different frame sizes,
  creative linear targets and stop offsets, safety bounds, invalid shapes,
  nonfinite/out-of-range control and pixel matrices, and bounded outputs at
  extreme valid controls.
- The real-image proof runs the exact Rust implementation on the 512x512 NASA
  astronaut photograph with manually drawn face/background/lighting mattes.
  Source distortion is synthetic and known, on that same photograph. This is
  recovery evidence, not unseen-shot semantic detection or temporal stability.
- Observed release fit: 93.026 ms; apply: 124.053 ms on this workspace, one run.
  No general performance or video render-time claim.
- Float display-RGB RMSE against the reference: source 0.0585089851,
  matched 0.0193302755, creative face +0.35 stops 0.0224475419.
  The creative score is not quality ranking; the target is intentionally changed.
- Matched log-luminance mean errors: skin -0.01154 stops, background -0.00670,
  lighting -0.00913. Chroma mean Euclidean errors in native Lab units:
  skin 0.54853, background 1.03416, lighting 0.04942.
- Actual pixels inspected in comparison.png and regions.png. Matching visibly
  removes much of the cast and raises the face toward reference; the creative
  variant brightens only the face region. Residual background differences and
  manual-matte edge limitations remain. No automatic video test was run.

## Reproduce

`cargo test -p kimo-core --test subject_match --test color_matching`

Install Python numpy, Pillow, scikit-image. Then run from repo root:
`python3 examples/subject_match_proof.py /downloads/subject-match-proof`

The script creates source/reference/matte float buffers, executes the Rust
release proof example, and writes comparison PNGs and raw Rust statistics.

Image provenance and public-domain statement:
https://scikit-image.org/docs/0.26.x/api/skimage.data.html#skimage.data.astronaut
