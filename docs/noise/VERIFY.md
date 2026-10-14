# Verification request (not yet run)

No cargo was run in this lane. This workspace has no rustc/rustfmt/cargo executables. Python proof runner compiles as Python; staged whitespace check passes. Rust compilation, warnings, test outcomes and renders are all UNVERIFIED.

Do not merge this v4 branch into main. First gate is numeric/registry smoke, then real 1080p frames; no camera work before inspected noise sign-off.

## Proposed serial build window

Run in a clean checkout of this branch, with ENGINE-2's work stopped/completed by the parent (do not kill it):

```sh
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test coherent_noise -- --test-threads=1 2>&1 | tee /tmp/kimo-noise-numeric.log
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test registry_lint --test registry_unique --test ext_registry -- --test-threads=1 2>&1 | tee /tmp/kimo-noise-registry.log
CARGO_BUILD_JOBS=1 cargo build --profile gate -p kimo-mcp 2>&1 | tee /tmp/kimo-noise-build.log
python3 tests/noise_1080p.py --binary target/gate/kimo-mcp --output /tmp/kimo-noise-1080p
```

Use bash `set -o pipefail` so tee does not hide a test/build failure. Record current warning set, compare against the same-profile current-main 40-warning baseline; no new warning is permitted. The total alone is not a per-diagnostic comparison.

Before building: run rustfmt only on the three new Rust files if available, then inspect its diff. No formatting sweep over existing code.

## Golden registry changes (not blessed)

Adding an effect changes add_effect/list_effects-related schemas and adding sample_noise changes registry/module discovery. Existing golden_registry.txt has not been mechanically updated without a live run. A golden mismatch is expected and must be reviewed, not hidden. Run the golden harness only after numeric tests pass and review the exact schema/scene changes before accepting a new baseline. No golden frame/registry file has been changed in this package.

```sh
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test golden_frames -- --test-threads=1 2>&1 | tee /tmp/kimo-noise-golden.log
```

Do not use KIMO_UPDATE_GOLDEN=1 as a blanket fix. Stage only audited registry additions/expected changed tool schema entries after confirming old scene pixel hashes remain identical. 320x180 golden scenes are smoke only.

## Actual image review

Inspect perlin-contact-sheet.png, simplex-contact-sheet.png, both 1to1-crop.png files and the original 1920x1080 PNGs. Repeat t=0 in shuffled frame order must be byte-identical. Check spatial smoothness, diagonal/lattice seams, grain crawling during advection, clipping from display gain/bias and consistency across time. The raw simplex scale is intentionally low; gain 10 in the proof is an explicit display mapping, not intrinsic normalization. Record any clipping and do not relabel it as signal range failure. Numeric bounds/derivatives must pass separately.

The runner writes calls.json, server stderr and STATUS.txt; STATUS explicitly says visual inspection pending. Rendering files is not a sign-off. Correctness/premium labels remain pending until evidence is inspected.

The explicit extended gate checklist in FIELD_UNITS.md includes range invariants, seeded determinism, gradient continuity, solenoidal curl divergence convergence, and actual 1080p aliasing/banding/warp-seam inspection. None has passed by declaration.
