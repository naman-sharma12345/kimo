# Ranked native integration plan

2026-10-14. Base `273cf0b`. Documentation/specification only. No new tool implemented, no cargo, no fresh engine tests, no frame renders. Correctness unverified; premium pending. Main stays untouched. Executable work belongs on separate `instinct/wip-*` branches and cannot be called ready until unit tests AND inspected 1920x1080 frames pass.

## Ranked decision table

| Rank | Work unit | Candidate source lane | What it fixes | Integration cost/risk | Ready now? |
|---|---|---|---|---|---|
| 1 | Continuous fields + derivatives + curl | FastNoiseLite MIT; noise-rs MIT; Ashima MIT; MaterialX file-level BSD | Hash-noise crawling and absent reusable force fields | Medium: API, overflow/alias limits and numeric agreement | Spec only |
| 2 | Pose/projection split + near clipping | Native math; Three.js MIT camera reference | Eye moves during zoom; triangles pop at near plane | Medium: saved-scene compatibility is load-bearing | Spec only |
| 3 | Shared raw light/material adapter | Existing Kimo BRDF + Filament Apache math reference | Inconsistent highlights across Scene3D/effects | Medium-high: avoid silent legacy changes | Spec only |
| 4 | Fixed-step 2D/3D force simulation + sphere CCD | Parry Apache; Box2D MIT boundaries | Preset-bound motion, tunneling, age-dependent steps | High: deterministic caches and budgets | Spec only |
| 5 | Alpha/HDR oracle + explicit operators | Native oracle; tiny-skia BSD reference | Potential transparent-edge/HDR errors | Medium: prove contract before expanding modes | Spec only |
| 6 | Typed VFX graph with auxiliary channels | Independent architecture; restrictive references ideas-only | Grade-only DAG ceiling and limited channel reuse | High: ROI, cache lifetime, graph safety | Spec only |
| 7 | Optional UV transition primitives | gl-transitions per-file MIT/BSD | Missing coordinate transitions, not core realism | Low-medium per selected file | Spec only |
| 8 | Constraints/volumes/general geometry | PBD MIT then other fully reviewed candidates | Longer-horizon feature depth | High; avoid constrained-box overload | Defer |

Copying is not the recommended default where Kimo already has suitable math. Small attributable adapters/ports are preferable to importing competing engines. No runtimes outside native C/C++/Rust are proposed.

## Priority 1: coherent fields, not preset looks

### FIELD-1: deterministic sampled noise

Candidate source order:

1. FastNoiseLite MIT native Rust implementation: Perlin/simplex/cellular and domain-warp machinery behind one bounded Kimo field adapter. Targeted Perlin/FBM code inspection supports this choice; do not paste a 3000-line library straight into effects.rs. Review full dependency/notice and selected implementation paths before vendoring.
2. noise-rs MIT cellular implementation: candidate for explicit Worley metric/feature contracts; inspected prefix is not full F1/F2 correctness review.
3. Ashima/stegu MIT simplex and periodic derivative noise: candidate scalar port/reference; keep all Ashima/Gustavson notices, distinguish variants and license lineage. Its own periodic header mentions precision artifacts, so benchmark and continuity-test rather than assuming it is superior.
4. OSL/MaterialX BSD noise lineage for derivative semantics; MaterialX's particular noise file retains Sony BSD terms despite Apache root. Do not flatten these notices.

Proposed native API, not registered tools yet:

- `Field`: kind `perlin | simplex | worley`, dimensions 2/3, seed, frequency vector, coordinate transform, value convention, repeat period (only when supported), derivative request.
- `Fractal`: source, octaves, lacunarity, gain, normalization mode `none | amplitude_sum`, mode `fbm | ridged`; octave limit is an explicit computational limit, not a hidden artistic clamp.
- `DomainWarp`: vector of independent seeded fields, amplitude in coordinate units, operation ordering explicit.
- `FieldSample`: scalar value, gradient in coordinate units, optional cell ID/F1/F2 for cellular noise. Worley distance has boundary nondifferentiabilities: do not promise smooth derivatives at cell boundaries.
- `FieldMap`: spatial/temporal mapping `x,y,z,time`; time input in seconds, not frame number. Existing hash `noise()` remains compatible and is renamed/described as hash noise rather than silently changed.
- `FieldEvaluationBudget`: bounded work = dimensions x octaves x samples; finite input guards, coordinate-to-integer overflow guard, seed wrapping behavior. Memory budget and timeout errors must propagate, never return a black frame on failure.

Derivative scaling: if a field is evaluated at q=A p+b, return world gradient A^T grad_q. For a scalar frequency f, gradient scales by f. FBM derivatives include each octave frequency multiplier and amplitude. For 2D curl from scalar potential psi, use `(dpsi/dy, -dpsi/dx)`; for 3D curl from vector potential A, use `(dAz/dy-dAy/dz, dAx/dz-dAz/dx, dAy/dx-dAx/dy)`. These are mathematical definitions, not copied source. Near-zero divergence must be tested numerically; multiplying an unnormalized curl by spatially varying gain can destroy the divergence-free property.

Band limit: procedural fields must know pixel footprint/derivatives where available. Remove or fade octaves above the sampling Nyquist limit; record policy in output metadata so CPU references and previews agree. A generic sample with no footprint must disclose point sampling rather than claiming antialiased output.

Tests before any landing:

- Negative coordinates/lattice boundaries/seed changes; same sample independent of call order, worker count and frame order.
- Continuity of Perlin/simplex samples, analytic gradients vs central differences at several epsilons; period equality when periodic mode is supported. No assumption of strict [-1,1] without reference evidence for the selected variant.
- Worley F1/F2 checked against an independent generous-neighborhood brute-force oracle, including feature points near cell corners and all supported metrics.
- Curl divergence convergence as epsilon shrinks, stable finite force outputs, zero-amplitude identity, fractal normalization and octave budgets.
- 1080p real Kimo frames: field grayscale, normal-from-gradient, displacement over checker and slow subpixel advection at t=0, .125, .5, 1.0, 2.0; signed/HDR/alpha preserved. Contact sheets plus close crops inspected. Compare temporal sequences for crawling and aliasing. 320x180 is smoke only.

## Priority 2: projection/clipping and raw lights

### CAMERA-1: separate pose from projection

Keep current camera as explicit `legacy_frame_fov` projection to retain saved projects. New `pinhole_physical` stores camera world pose, sensor/filmback dimensions, focal length (mm), principal point and near plane separately. Vertical FOV = 2 atan(sensor_height / (2 focal_length)). Changing focal length must not alter camera pose. Three.js's inspected MIT camera methods are a small API/math reference; PBRT Apache thin-lens code is a later optical reference, not a dependency.

Near-plane clipping: transform triangles into camera coordinates, clip polygon against z>=near (Sutherland-Hodgman), interpolate position and all attributes at t=(near-zA)/(zB-zA), triangulate resulting 0/3/4-vertex polygon. Perspective-correct interpolation happens after clipping. Reject invalid near/far inputs; do not discard an entire triangle merely because one vertex is behind. Degenerate/clipped edges need tolerance policy and shared-edge tests.

Tests: two/one/no vertices inside, on-plane vertices, huge/small scales, reversed winding, rotating camera, clipping adjacent triangles without cracks, FOV zoom with fixed eye, backward compatible old scene pixel equality. 1080p contact sheets show near-plane crossing, dolly vs optical zoom and foreground/background parallax. Explicitly do not claim physical lens DOF from a capped screen-space blur.

### LIGHT-1: shared, raw material/light interface

Adapter first: Scene3D's separate specular lobe must not silently inherit a new BRDF. Add `shading_model: legacy | microfacet_v1`; share named materials and evaluated light state by explicit opt-in. Directional/point/spot, world units, radiometric convention, range/falloff and color encoding must be documented. Existing relight's coordinate system is frame-normalized and its ambient is artistic; conversions must not pretend these are calibrated photometric units.

Expose direction, position, color/intensity, falloff, cone, roughness, metalness, IOR, clearcoat and emissive as independent channels, with animatable scalar curves. Add area-light shape/size and environment-map sampling only as separately testable primitives. Do not add a compulsory cinematic rig or provider presets. Filament Apache BRDF functions are candidate small attributed ports; its precision macros and full GPU architecture are not required.

Tests: reciprocity and selected known BRDF samples, finite grazing response, roughness/metal sweeps, energy sanity checked under an explicit normalization convention, same material across relight/Scene3D fixtures, point/directional/spot shadow truth, default legacy identity. 1080p material sphere/grid contact sheets under matched lights, plus moving-light sequences. Measurements and visual premium are separate gates.

## Priority 3: raw force/simulation primitives

### SIM-1: field-driven particles and fixed-step trajectories

Data: initial state arrays `(position, velocity, birth, lifetime, mass, radius, orientation)`; emitter is an optional initializer, not the simulation itself. Independent `ForceField` nodes: constant acceleration, linear drag, attractor with explicit softening law, vortex, sampled vector/curl field and user-composed force sum. Units explicit; seed and field version persisted. Do not constrain particles to rigid 2D templates; coordinate dimension 2/3 is a declared property.

Integrator: fixed dt, deterministic sample times, semi-implicit Euler baseline; choose substep count from quality/budget setting, not particle age. Bake checkpoints or trajectories with identity including all force/collider data, deterministic eviction, and cancellation-safe output. Scrubbing/rendering a frame cannot change the state used by other frames. Trails and render sprites consume trajectories; they do not own dynamics.

Collision: swept sphere/plane first, then swept sphere/sphere with quadratic time-of-impact; Parry Apache cast-ball API provides useful status/witness boundaries. More complex convex CCD can use audited Parry/Box2D MIT ideas/ports only after full selected-path review. Separate penetration recovery from time-of-impact response. PositionBasedDynamics MIT distance constraints are a later reusable constraint primitive, not a promise of cloth/fluid parity.

Tests: analytic ballistic/drag references, timestep convergence, impulse/momentum on isolated contacts, tunneling at high velocity, simultaneous births/negative prewarm, seed stability, shuffled frame order, caches invalidated by force edit, resource exhaustion/cancel. 1080p rendered scenes show slow and fast particles, near-camera motion, collisions, trails and interacting force fields, inspected across time. No fluid/cloth claim until those solvers exist and pass their own suites.

## After the critical path

1. Alpha/HDR compositing oracle and explicit Porter-Duff modes. tiny-skia BSD is a useful taxonomy/premultiplied reference, not a signed-HDR implementation to copy blindly.
2. Multi-input VFX graph with RGBA/depth/normal/motion channel types, ROI/tile/lifetime budgets. Natron/Olive/Blender/GEGL are idea/spec-only architecture references; independent implementation must not transplant their code.
3. Transition math as optional low-priority ports after file notices: gl-transitions Directional/CrossZoom are inspected MIT examples. Not all 125 transitions have been reviewed. Native raw UV/progress functions beat another collection of mandatory preset stacks.
4. Motion Canvas/Manim timing semantics inform scheduling and subobject staggering; Kimo already has Bezier/spring math. Do not adopt TS/Python runtimes or duplicate easing engines.
5. Volumes, general geometry, fluid/cloth and GPU work remain later. No GPU activation or new build burden in this docs-only stage.

## License and clean-room gates

- A root MIT/Apache/BSD file is a lead, not blanket clearance. Check each copied file, embedded upstream lineage, generated tables, test assets, submodules, dependency license and NOTICE. Keep original notices and mark modifications. Record exact pin and SHA256 of imported files.
- GPL/LGPL/AGPL/MPL, custom/noncommercial, mixed/unresolved and no-license sources are ideas/spec-only under this sweep. No restrictive implementation code was read for this plan; only repository descriptions/root notices. A reader of restrictive implementation cannot certify their own adaptation clean-room by renaming variables or translating languages. Use independent public math/specs and a separate implementation/review record.
- Shadertoy adapter/host licenses do not grant rights to arbitrary shader content. Require each shader's original license and author provenance. No automatic scraping/importing of unknown-license shader art.
- zlib/Tomorrow/W3C-specific licenses may be usable in principle but are outside this task's explicit MIT/Apache/BSD copying lane. This plan does not silently widen that lane.

## Delivery/landing checklist

Each work unit gets its own branch, source ledger update, third-party notice if source is imported, registry/schema proof, independent unit oracle, actual 1080p render/contact-sheet inspection, warning delta, and performance/memory evidence on the constrained target. All pending executable work is WIP/unverified. No claim that the baseline 40 warnings remained unchanged can be made without a fresh build; docs add no compiler code but do not substitute for that check.
