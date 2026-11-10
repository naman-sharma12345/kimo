# Explicit camera and clipping primitives (WIP)

Base 2977b73751f1c9124501503e3cba0b9c33cc9413, instinct/wip-camera. No main merge or PR authorized. Original native math, no third-party code. No cargo, no Rust compilation/tests/renders/warning measurement. Correctness unverified; premium pending.

New raw MCP commands camera_project_points and camera_clip_triangle expose explicit world eye + pan/tilt/roll, sensor height/focal length and near plane independently. Projection never changes eye. Clipping operates in camera coordinates and interpolates affine attribute channels before perspective correction; zero/one/two triangle output. Tests cover focal/pose separation, clipping topology, attributes, on-plane deduplication and invalid values.

Important: this first unit DOES NOT replace the legacy project camera or Scene3D renderer. Therefore it does NOT yet fix the existing saved-scene near-plane popping or FOV coupling. It supplies isolated raw primitives and test targets for a later adapter. Integrating it needs an explicit camera-model version in persisted project schema, legacy pixel-equivalence proof, scene view transformation and clipped triangle rasterization. Do not call math-tool completion a renderer fix.

Serial window commands:

```sh
set -o pipefail
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test camera_projection -- --test-threads=1 2>&1 | tee /tmp/camera-numeric.log
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test registry_lint --test registry_unique --test ext_registry -- --test-threads=1 2>&1 | tee /tmp/camera-registry.log
```

After adapter work, require real 1080p render/contact-sheet proof for near-plane crossings, adjacent triangles/shared edges, winding, rotating cameras, dolly vs focal zoom and matched legacy frames. No adapter/render proof is included yet. No golden baseline is auto-updated. Warning diagnostics must be compared against same-profile main, zero new warnings allowed.
