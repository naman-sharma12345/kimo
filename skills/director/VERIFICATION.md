# Director expansion verification

Base: main 273cf0b88c1d6d3d5bceb9af16ca7a1a5ef02384, read from live remote.
Target: instinct/wip-director-3 (or WIP continuation only). Main is not modified.

Eleven further accepted sources, thirty-two total, with inspected source files,
commit pins, license observations and accepted/rejected principles in CREDITS.md.
No external code, prompt bodies, templates or assets vendored.

Checks on 2026-10-14:
- 90 standard-library unittest cases passed (38 first-pass + 31 second-pass + 21 third-pass).
- All v1/v2/v3 examples pass the dependency-free validator.
- Independent jsonschema Draft2020-12 schema self-check and all three examples pass.
- Python syntax compilation passed; git diff --check passed.
- Registry name checks use current base source, including split commands files.
- v2 example --batch emits the same eight transparent primitives with absolute
  paths, symbolic ID bindings and review-only flags.

No cargo, engine execution, generator calls, paid actions, video or visual output
was produced. The examples remain synthetic. Numeric lens/lighting/camera
contracts are not automatically applied to engine settings. Tests establish
structure and timing checks, not taste, lip-sync correctness or media presence.

Unknown provider capability and estimated timing produce explicit warnings.
Source capability tables remain unverified, never executed as instructions.

Revision: README heading is branch-neutral. Added optional coverage policy with warning-only checks, lens rationale and light-change explanation; seven new tests pass.

Third pass adds cue evidence, explicit offset checks, readability warning and review-state hygiene. Real engine frame renders are UNVERIFIED. No new Rust tool was implemented; do not stage this WIP for merge without the owner-required real-frame gate.
