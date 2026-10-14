# Raw field units 2-5 (WIP, no cargo)

Extends original unit 1 on branch family `instinct/wip-noise-fields`, based on main 2977b73751f1c9124501503e3cba0b9c33cc9413. The full snapshot includes unit 1. It is not a main candidate, and no PR/merge to main is authorized. No copied source or lookup tables; prior permissive-source exposure remains disclosed in PLAN.md.

Separate operators, in requested order:

1. Worley: one independently hashed feature per unit square, Euclidean F1/F2, exact integer cell IDs, analytic radial gradients. Not differentiable at feature points/ties; zero subgradient chosen at feature, lexical cell tie break. Query a 7x7 neighborhood. F2<=sqrt(5): a feature in the containing cell is <=sqrt(2), one in a horizontal neighbor is <=sqrt(5); cells outside radius 3 have at least 3 coordinate distance. No 3x3-neighborhood assumption. Tests compare a much larger 17x17 oracle.
2. FBM: explicit Perlin/simplex base, seed per octave, lacunarity, signed octave gain, 1..16 work limit, optional sum-of-absolute-amplitudes normalization. Gradients include frequency chain factor. One octave is exactly base identity.
3. Domain warp: one explicit coordinate warp p+A*[W(seed,f*p),W(seed+1,f*p)], independent source and warp fields/seeds; apply full transpose-Jacobian chain rule. Zero amplitude is base identity. Not an automatic FBM stack.
4. Curl: 2D scalar potential rotation `(dpsi/dy,-dpsi/dx)` with constant potential amplitude and explicit frequency. Returns both vector components, no implicit normalization (normalizing spatially changes divergence). Tests finite-difference divergence and derivative scaling. 3D vector-potential curl is not implemented in this unit and is not claimed.

MCP: sample_field selects exactly one op with explicit config. Worley returns both distances/gradients/cell IDs; scalar FBM/warp return value/gradient; curl returns both vector components and null scalar gradient. field_operator effect selects the op from strings.field JSON; mapping frequency/offset/rates/gain/bias/mix are separate animatable effect parameters. Alpha unchanged, signed float math, transactional output on overflow/timeout/budget failure. The JSON config itself is static for this version: octave/warp/curl settings are not represented as animated channels yet. Point-sampled only: no band-limit/antialiasing claim.

No implementation is verified until Rust builds, numeric gates and actual 1080p rendering pass. Python runner syntax and whitespace pass here; Rust/cargo/rustfmt are unavailable. Zero-net-new-warning baseline is pending measured comparison, not inferred from source.

## Commands for the serial slot, after unit 1 numeric PASS

```sh
set -o pipefail
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --lib noise_fields::tests -- --test-threads=1 2>&1 | tee /tmp/kimo-field-local.log
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test coherent_noise --test noise_fields -- --test-threads=1 2>&1 | tee /tmp/kimo-fields-numeric.log
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test registry_lint --test registry_unique --test ext_registry -- --test-threads=1 2>&1 | tee /tmp/kimo-fields-registry.log
CARGO_BUILD_JOBS=1 cargo build --profile gate -p kimo-mcp 2>&1 | tee /tmp/kimo-fields-build.log
python3 tests/noise_1080p.py --binary target/gate/kimo-mcp --output /tmp/kimo-noise-1080p
python3 tests/noise_fields_1080p.py --binary target/gate/kimo-mcp --output /tmp/kimo-fields-1080p
```

Compare warnings per diagnostic against same-profile main, not only total count 40. Inspect exact 1080p originals, 1:1 crops and contact sheets for each type, across shuffled times. Repeated t=0 byte-equality is a numeric/render-repeat check, not visual approval. Preserve golden scene hashes; review expected registry additions separately rather than blindly updating golden files. All existing golden baselines are unchanged in this WIP.

No camera/projection work proceeds before noise visual sign-off. No fluid, 3D force simulation or premium-quality claim follows from a noise operator.
