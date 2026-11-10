# Native OCIO CPU API (review-only implementation)

This is an opt-in Rust/C++ CPU adapter to installed OpenColorIO 2. It does not
replace Kimo's defaults, output transform, Python ACES reference, renderer, or
project model. Configs are not called ACES-certified or production-validated.

## Build

Supply compatible OCIO 2 headers and a shared library, then build with:

```
OCIO_INCLUDE_DIR=/opt/ocio/include OCIO_LIB_DIR=/opt/ocio/lib \
LD_LIBRARY_PATH=/opt/ocio/lib cargo test -p kimo-core --features native-ocio \
  --test color_ocio_config
```

`OCIO_INCLUDE_DIR` contains `OpenColorIO/`. The headers and shared library must
match. Tested with OCIO 2.5.1 headers and the 2.5.1 wheel shared library on
Linux x86_64. No OCIO dependency is downloaded by the build. Default builds need no
OCIO headers/library, keep legacy effects unchanged, and fail closed on explicit
OCIO effects. The C++ adapter uses exception containment and RAII. `cc` is the
only new build dependency. Existing extension discovery stays intact.

## Configs, transforms and tools

`color::ocio_config::Config::from_text/from_file` loads a UTF-8 config of at most
4 MiB using the real OCIO parser. `colorspaces()` includes inactive spaces.
`views()` lists configured views attached to each concrete display, including
shared views. Virtual display templates are not enumerated. The library version is inspectable. No global current
config is set. Search paths/environment variable definitions are cleared in the
local config. Environment-independent display listing uses the all-displays API.

Self-contained configs may contain matrices, log transforms, embedded LUTs,
groups and built-ins supported by the installed OCIO version. External
`FileTransform`s are rejected throughout colorspaces, looks, named transforms,
and view transforms (including nested groups). This is deliberately narrower
than *every arbitrary config*: configs needing external LUT/CLF/CDL files or
context variables need a later asset allowlist/resolver. Native parser resource
use is not a process sandbox. The input cap and catalog/depth limits do not
prove safety against every malicious YAML/OCIO parser input.

`Config::processor(source, destination)` creates a persistent CPU processor.
It retains its own lifetime independently of the config. `Processor::apply`
works on straight/unassociated RGBA float frames, permits signed and HDR RGB,
checks shape/finiteness/alpha, preserves alpha, and is transactional on errors.
A processor marked no-op performs no pixel writes (including signed zero).
Optimized CPU approximations are disabled. Repeated results are deterministic
on the tested build, not promised bit-identical across OCIO versions, compiler
flags or CPU architectures. The RGB-only operation intentionally ignores alpha
terms in OCIO transforms.

Auto-registered tools: `inspect_ocio_config`, `transform_ocio_samples`. No files
are written by these tools. They do not select a project output display view.

## Per-effect working-domain declarations

The enum `WorkingDomain` has `Current`, `SceneLinear`, `DisplayReferred`.
`DomainProcessor::new(config, current, linear, display, domain)` resolves the
required space and compiles both transforms **before** effect evaluation.
`apply` automatically sandwiches the effect and returns to the current space.
Unknown effect kinds fail; mixing the legacy encoding wrapper fails.

The existing `effects::apply` dispatch recognizes an explicit effect string
contract, without model/schema changes:

```
"ocio_domain": "display-referred"   # or scene-linear/current
"ocio_config": "/path/to/config.ocio"
"ocio_working_space": "ACEScg"
"ocio_linear_space": "ACEScg"
"ocio_display_space": "My encoded display space"
```

These strings are declarations supplied by the caller, not inferred metadata.
The caller must ensure the actual incoming frame is in `ocio_working_space`.
There is no renderer-wide propagation or stale-project-domain detection.
Without declarations, all existing effects retain their current behavior.
No automatic classification of existing effect kinds is made. A display-space
sandwich is an invertible colorspace transform, **not** an ACES display-view
rendering operator or a guaranteed invertible tone map. Configured display
spaces that clip/gamut-map may lose information on the round trip.

For realtime/export integration, cache a `DomainProcessor` in the caller.
The string-dispatch convenience path reloads/recompiles per call and is not
performance-qualified. No MCP effect setter is included; existing generic effect string configuration
may be used. The separate forward display-view API below does not change this
invertible effect-domain sandwich.

## Proven tests

The small test config is authored here, not an Academy shipping config or a
complete ACES output config. Its reference domain is AP1 scene-linear. The
ACEScct space uses the installed OCIO built-in encoding curve. DisplayGamma2 is
synthetic and explicitly not an ACES ODT.

Published ACEScct Equation 1 from
https://docs.acescentral.com/encodings/acescct/ supplies independent expectations
for signed/toe/breakpoint/gray/white/HDR samples. Maximum allowed encoded error
is 2e-6; round-trip tolerance is 1e-5. Tests cover listing, repeated transforms,
processor lifetime, bit-exact no-op and alpha, malformed/oversize/NUL/nested
external-file rejection, bad inputs, effect sandwiches, error rollback, string
dispatch, missing native feature and tool registration. They do not prove an
ACES output view, the full ACES 2 suite, hostile-parser sandboxing, GPU output,
render-wide integration, throughput or industry parity.

An optional integration test loads the official built-in ACES2 CG config when
`KIMO_TEST_NATIVE_ACES_CONFIG` points to its serialized .ocio file. It checks
space/view discovery and ACEScg-to-ACEScct gray, not its output display views.


## Forward display-view CPU pipeline

`Config::display_processor(source, display, view)` compiles an OCIO
DisplayViewTransform in the forward direction with configured looks ENABLED.
This is the full pipeline defined by that config/view (including ACES output
rendering where the config defines it), not a source-to-display colorspace
sandwich. It reuses transactional RGB-only Processor application, preserves
alpha and may outlive Config. Wrong display/view combinations fail.

`transform_ocio_view_samples` takes config_path, source, display, view and flat
rgba (1..4096 samples). Signed/HDR floats are allowed; alpha0..1. It returns
unquantized transformed samples, no project changes. Arbitrary source/output
semantics remain caller-declared, not inferred from names.

`render_ocio_display_view` is an explicit SDR still-preview path. Required fields:
config_path, source, display, view, time, path (.png), output_encoding=sRGB_SDR.
The project must be managed AP1/D60 scene-linear (working_gamut3); source must
represent those pixels in the selected config. Caller explicitly declares the
view produces sRGB SDR, never PQ/HLG. It renders one frame, bypasses the configured
project output transform, applies the forward view and writes an 8-bit PNG.
Out-of-range pixel count is reported; PNG quantizes/clips0..1. The config remains
self-contained, with external FileTransforms rejected. No video export, GPU,
HDR image encoding, display calibration, global config changes or defaults.

Native library2.5.1 proof uses its official built-in
cg-config-v4.0.0_aces-v2.0_ocio-v2.5 serialized config, sourceACEScg,
display "sRGB - Display", view "ACES 2.0 - SDR 100 nits (Rec.709)".
The real-footage still is SDR-origin, explicitly decoded sRGB then converted to
AP1 by Kimo before the selected display-view transform. Actual pixels inspected.
A separate1280x720 declared-AP1 float fixture matches Python DisplayViewTransform
from the SAME OCIO library exactly (3,686,400 RGBA samples). That is API parity,
not an independent ACES implementation, full conformance suite or camera proof.
Configured-look inclusion and wrong names/input rollback are regression-tested.
