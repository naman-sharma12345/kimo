# threed-foundation source scaffold

Authored on fetched v4 65a2b63. Additive branch delta, no legacy renderer changes. No Cargo.lock resolution, vendoring, build, tests, warning comparison or PNG execution in this lane. UNCOMPILED/UNEXECUTED. This is partial foundation scaffolding, not completed 3D engine or glTF scene support.

## Implemented source surfaces

Original bounded mesh position/triangle validation and inspect; raw affine parent hierarchy evaluation with cycle checks; deterministic median BVH with ray query and brute-force oracle; conservative plane/AABB frustum function; sphere/box/CSG SDF sampling; GGX/Smith/Schlick + Lambert/pi numerical BRDF; nine-coefficient Y-up diffuse irradiance SH evaluation; orthographic constant-color CPU reference raster returning Frame/depth; readback row-stride planner; optional async wgpu adapter probe scaffold; optional tobj triangle-position-only OBJ and gltf embedded buffer-only GLB parser scaffold.

MCP source registers nine raw commands: inspect_threed_backend, inspect_mesh_buffers, inspect_scene_matrices, raycast_mesh_raw, sample_sdf_raw, sample_pbr_raw, sample_diffuse_sh_raw, render_mesh_cpu_reference, import_mesh_raw. Existing registry goldens untouched: engine verification must audit nine new lines, counts and grouping. These first commands accept explicit raw values and inspect outputs, no presets. No persistent scene store yet; raw buffers/graphs in each call. API names/layout provisional until compiled/validated.

## Honest missing pieces

GPU render pipeline/attachments/readback/executor not implemented. Adapter capability probe is NOT GPU rendering. CPU is initial orthographic depth-domain0..1 constant-color reference; perspective clipping/interpolation, material/light integration, textures/transparent sorting, top-left raster rule, multisampling and encoder export timeline adapter pending. Image command writes PNG through existing helper, does not mutate project.

GLB importer deliberately rejects ordinary scene nodes/materials/skins/animations and non-position attributes, so currently supports buffer-only fixtures rather than normal user scenes. OBJ importer rejects nontriangular faces, UV/normals/external MTL rather than silently dropping them. Full scene ingestion and controlled texture resolver pending. Limits do not yet preflight every parser allocation. Buffer/scene state persistence, hashes/cache invalidation, paged buffer tools, BVH refit and persistent mesh binding pending. Public BVH ray API is intended for freshly built matching mesh only; stored-graph integrity/version validation still needed.

SDF polygonization/dual contour/QEF not present, only raw field sampling. CSG field is not universally exact Euclidean distance; no sphere-tracing safety claim. SDF expression size budget beyond depth check pending. SH projection, environmental fixtures and specular IBL not present. Diffuse evaluation is raw signed SH, not clamped or calibrated. Existing legacy lighting unchanged.

## Dependency/license gate

Optional exact direct candidates: wgpu30.0.1 (Rust>=1.87), gltf1.4.1, tobj4.0.5. Features mesh-import and gpu3d default off. Wgpu backend features target-specific Linux Vulkan/Windows DX12/macOS Metal. Actual feature closure and cross-target compatibility must be resolved before acceptance. Zlib/ISC/Unicode3 exceptions apply only to required wgpu dependencies, not all candidate packages. No copyleft branch selected. Preserve license/copyright/Unicode/Apache NOTICE/modification notices under vendor and ship in release archives. No vendoring/lock generation done here.

## First serial batch (only after box release)

```sh
rustc --version
cargo metadata --format-version 1 --features mesh-import,gpu3d > /tmp/threed-metadata.json
cargo tree -p kimo-core --features mesh-import,gpu3d --target x86_64-unknown-linux-gnu -e features > /tmp/threed-linux-features.txt
cargo tree -p kimo-core --features mesh-import,gpu3d --target x86_64-pc-windows-msvc -e features > /tmp/threed-windows-features.txt
# Pause affected dependency before build if resolved terms/toolchain fail review.
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test threed_foundation -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --features mesh-import --test threed_foundation -- --test-threads=1
CARGO_BUILD_JOBS=1 cargo check --profile gate -p kimo-core --features gpu3d
# Existing registry/readme suites will need new-entry-only audited updates, not blanket blessing.
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test registry_lint --test registry_unique --test ext_registry --test golden_frames --test readme_count -- --test-threads=1
KIMO_3D_FRAME=/tmp/threed-cpu1080.png CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test threed_foundation render_1080p_foundation_fixture -- --ignored --exact --test-threads=1
```

Compare normalized warning diagnostics to same-profile38 baseline, zero net-new. Actual PNG inspected before milestone claim. Current ignored harness renders one triangle only, not a complete multi-scene contact sheet; expand to depth-overlap/coverage/transform/import fixtures and contact sheets after numeric pass. Existing golden entries stay unchanged on default legacy path. GPU proof separately follows completion of pipeline/readback, not this probe scaffold. Criterion after functional gates only.
