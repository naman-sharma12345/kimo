# Roadmap

Updated 2026-10-23 against `v6-all` at `98d1bdf`.

Build a dependable, programmable editing, motion, VFX, color and audio system that
outperforms established tools on measured end-to-end workflows. Feature count is
not a quality or parity claim.

The [professional editor gap analysis](docs/PRO_EDITOR_GAP_ANALYSIS.md) contains
source evidence, current competitor references, 27 work packages, dependencies,
acceptance criteria and proposed benchmarks. It is a source audit; runtime
behavior and performance were not revalidated for this assessment.

## Existing foundations

- Shared MCP/CLI registry; atomic batches, dry runs and revision checks.
- Timeline edits, nested compositions, keyframes, expressions and motion tools.
- Multicam sync/switching, transcripts, captions and local ASR orchestration.
- Tracking, classical/temporal mattes, masks, particles and multiple 3D paths.
- Managed color, grade graphs, scopes and high-bit export.
- Undo/checkpoints, copy-on-write clips, saves and optional persisted history.
- Dependency collection/relinking, caches and chunked render jobs.
- A read-only spectator and substantial regression/golden test infrastructure.

These features have varying restrictions and proof levels. Historical checklists
contain outdated gaps; consult source and observed tests before planning work.

## Next: trust and complete workflows

1. **Baseline/CI (F01/F04):** current-source Windows/Linux validation, truthful
   capabilities and reproducible performance/quality fixtures.
2. **Transcript edit application (F05), initial implementation:** phrase search,
   revision-checked atomic picture/audio deletes, caption/marker updates and undo.
   [Usage and support limits](docs/TRANSCRIPT_EDITING.md). Expand beyond explicitly
   bound normal-speed media after workflow qualification.
3. **Recovery/time (F02/F03):** durable journal/autosave, migrations, rational
   frame/sample timing, VFR mapping and linked A/V invariants.
4. **Media/playback (F06/F07/F09/F10):** proxies, streaming playback, bounded
   interchange and EXR/PNG sequence ingest.
5. **Render/color (F08/F16):** render contracts/IR, selective caching, alpha/color
   consistency and GPU hot paths with CPU reference parity.
6. **Creator workflow (F11/F12/F14/F19):** multilingual shaping, optional human
   review/control, promptable temporal mattes and complete audio routing.

F06 initial implementation: [preview proxy workflow](docs/PROXY_WORKFLOW.md) now supports validated synchronous generation, preview switching, integrity checks, fallback, relinking and undoable persisted bindings. Background processing, VFR timestamp maps, broader codec/color support, throughput qualification and a media-management UI remain open.

## Broader professional parity

Advance tracking/roto, restoration, HDR finishing, reusable compositing/motion,
3D integration, media conform and durable multi-output rendering. Build a full
interactive editor, collaboration and extension ecosystem on those foundations.
Specialist immersive, studio and simulation workflows need separate qualification.

## Differentiation and release standard

Prioritize revision-safe instructions, preservation constraints, linked aspect/
language variants, review evidence, searchable media/subjects and reproducible
packages. Extend existing batch/QC/dependency/manifest tools.

Keep commands discoverable, old projects readable, CPU rendering reproducible and
feature gates explicit. Run the applicable [test gates](docs/TESTING.md), add
real-footage workflow qualification, and follow the gap analysis comparison
protocol before publishing superiority claims.
