# Raw UV primitives (WIP)

Base 2977b73, instinct/wip-uv-primitives. Original math, no copied shader/library code. Survey context: gl-transitions MIT Directional/CrossZoom demonstrated coordinate push and radial sampling as useful raw building blocks, but this unit does not port either source implementation or claim identical output. No imported third-party source means no false dependency attribution is added.

MCP effect_uv_sample exposes raw push coordinate pairs/ownership weights and one radial UV sample. Direction magnitude remains continuous, unlike sign-snapped directional shader behavior. Endpoints explicitly choose source/destination. Caller owns sampling, edge policy, blend progress, weights and temporal easing. It is not a finished transition effect, zoom-blur renderer, artistic preset or performance optimization.

No cargo/compilation/test execution/renderer/1080p evidence. Tests are draft endpoints, zero/continuous direction magnitude, radial identity and invalid inputs. Numeric gate then renderer adapter and actual 1080p checkerboard/alpha/edge/motion contact sheets are required. Zero net-new-warning comparison pending. Main untouched, no main PR/merge. Primitive-only WIP is not a visual improvement claim.

```sh
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test uv_math --test registry_lint --test registry_unique --test ext_registry -- --test-threads=1
```

## Raw mapping extension (still uncompiled)

Planar picks two distinct axes 0..2 and returns raw unbounded coordinates. Polar maps xy to (atan2(y,x)/tau+.5, radius). Cylindrical is Y-axis: (atan2(z,x)/tau+.5, y). Spherical is Y-up: azimuth from xz, inclination acos(y/r)/pi, computed after max-component scaling to avoid radius overflow and subnormal direction loss. No implicit rotation, scale, repeat, aspect, camera or projection presets. Caller owns transforms and wrapping. Azimuth is canonical [0,1), including both signed-zero sides of the negative-X seam at u=0. Negative-X angular seam is explicit; zero radius/poles return declared fallback u=.5 with singular=true. Seam discontinuities and singular derivatives are mathematical properties, not hidden smoothing. No texture sampler, renderer or bandlimit is bundled.

Tests draft cardinal axes/poles/origin/seam, raw planar magnitudes, scale-invariant spherical coordinates and invalid axis handling. Real 1080p checker/texture render proof remains pending. No macro wrapper or preset is introduced.

## Finite-output review delta

Successful mapping results must be finite. Polar radius overflow returns an error, never an infinite radius. Cylindrical inherits this conservative polar check. Spherical max-component scaling handles even MAX and minimum-subnormal finite directions; zero scale returns before any division. Its normalized radius is at least 1 for a nonzero input, and acos input is clamped. Origin and poles remain explicit singular flags, not NaN sentinels. No near-pole epsilon or hidden smoothing is introduced.

Added draft tests cover signed-zero origins/poles/seams, wrap-equivalent seam sides, minimum subnormals, MAX directions, radius-overflow rejection and every nonfinite input axis. These Rust tests remain UNCOMPILED and UNEXECUTED. Criterion benchmarks are deferred until compilation and functional tests pass; no timing or speed claim is made.
