# Raw lighting numerical interface (WIP)

Base 2977b73, instinct/wip-raw-lights. No main PR/merge authorized. No cargo, compilation, numeric execution, warning-delta measurement or 1080p render. Unverified; premium pending.

Original wrappers expose lighting_brdf_sample and lighting_incident_sample over EXISTING Kimo math. Raw normal/light/view/albedo/material and light-definition/surface-position are explicit. No preset rig or hidden setup. This is not a new physical renderer or a Scene3D material integration. BRDF currently has artistic normalization; incident units remain existing frame-normalized units, not calibrated photometry. No claim of energy conservation follows from these wrappers. Light/model adapter, world-unit contract, area lights, environment sampling and 1080p sphere-grid/animated-light proof remain outstanding.

Run after parent gives serial window:

```sh
set -o pipefail
CARGO_BUILD_JOBS=1 cargo test --profile gate -p kimo-core --test raw_lighting --test registry_lint --test registry_unique --test ext_registry -- --test-threads=1
```

No existing shading implementation changed. Tests compare wrappers to current math, finite front/back response, zero-direction rejection and disabled lights. That is an access/correctness smoke gate, not independent physical validation. Future renderer integration requires independent BRDF samples/energy convention, legacy pixel equality and actual inspected 1080p material/light contact sheets. Zero new warnings against the same-profile baseline; goldens reviewed separately, never auto-blessed.
