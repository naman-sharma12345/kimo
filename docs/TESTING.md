# Standard gate

`tests/gate_fast.sh` (every push) and `tests/gate_full.sh` (phase boundaries) wrap
`tests/gate_run.py`. The rust legs of both tiers are:

1. `cargo test --profile gate -p kimo-core` - ALL targets: lib, every integration
   test under `crates/kimo-core/tests/`, and doc tests. Never gate on `--lib` alone:
   a lib-only run lets integration and golden-hash regressions hide (on 2026-10-16 the
   hero_pipeline and sdf_contour_frames frame-hash tests drifted for weeks because
   those files were not listed in `tests/gate_manifest.txt`; the manifest's per-test
   rust steps are kept for per-suite timing, but the manifest is no longer what makes
   a test run - cargo discovers every target).
2. `cargo test --profile gate -p kimo-core --features mesh-import,flat-sah,material-textures`
   - every feature whose test binaries the machine can build and run, plus
   `cargo check -p kimo-core --features gpu3d` for compile coverage of the GPU path.
   Two exclusions, both deliberate: `native-ocio` requires OCIO 2 headers
   (`OCIO_INCLUDE_DIR`); `gpu3d` test binaries OOM small gate machines at link time
   (wgpu static link; measured SIGKILL at 1.5 GB free on a 2 GB box, 2026-10-16) - on a
   machine with the resources, add both to the test run. `--all-features` is NOT the
   command for these reasons.

Golden frame-hash tests (hero_pipeline, sdf_contour_frames, future render fixtures)
pin exact pixels. When a shading change legitimately alters output, re-pin only after
dumping the gated frames (the tests' `KIMO_*_FRAMES` env vars) and visually inspecting
them; the re-pin commit message must name the causing change and the old/new hashes.

Accepted approximation thresholds are documented at the assertion site, e.g. the
split-sum IBL vs Monte Carlo ceiling (5.49% relative at grazing dielectric, assert
band 7%) in `crates/kimo-core/src/threed/lighting_stage.rs`.
