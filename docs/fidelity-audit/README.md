# Visual-fidelity audit and source survey (WIP)

Base: main 273cf0b, 2026-10-14. No engine code changes, no cargo, no main update.

1. [Gap audit](GAP_AUDIT.md): current source, likely failure mechanisms and evidence.
2. [Repository survey](REPOSITORY_SURVEY.md): 68 live pinned repositories, 16 targeted code/header inspections, not 68 end-to-end reviews.
3. [Ranked native port plan](PORT_PLAN.md): noise, camera/lights and field simulation first, with numeric and 1080p render gates.
4. [Kimo source excerpts](SOURCE_EVIDENCE.md): original checkpoint evidence, not external source code.
5. [Verification and limits](VERIFICATION.md): what was and was not run.

Machine-readable provenance: survey.json and inspected-sources.json. Root-license observations in license-evidence/ preserve the license text as research evidence with whitespace normalized; original and normalized hashes are in survey.json; this directory is not a third-party-code notice file and does not claim code was imported.

Correctness: fresh execution unverified. Premium: pending. 320x180 hashes are smoke only; actual visual sign-off requires inspected 1080p contact sheets/sequences.
