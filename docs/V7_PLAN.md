# V7 Plan

Base: 65324e9bddbb063bac2aebe2fe323c43d2e83691 (main, post hackathon-scrub). FEATURES.md checklist is reconciled to
checkpoint dd9a517 (2026-10-11); v6.0.0 shipped a lot after that (transcript editing, proxy workflow, flash_series,
loudness curve/LRA, audio_spectrum, vignette shaping, film_burn, and more — see CHANGELOG.md). This audit re-checked
every `- [ ]` and `- [~]` line in FEATURES.md against the live registry (452 tools in golden_registry.txt), the
effect-kind catalog (~152 kinds dispatched in color.rs/effects.rs/effects2.rs/layerfx.rs), crates/kimo-core/src file
list, and the test suite (280 files).

Baseline (this audit, 65324e9, before any v7 commit): `cargo clean -p kimo-core --profile gate` then
`cargo test --profile gate --workspace -j2` under the build lock. Warnings: 35 (at the stated ceiling). Full counts
are appended below once the baseline run finishes (running in background; see build log).

## Headline finding

A large fraction of the "pending" list is already implemented and tested under names FEATURES.md doesn't mention
(the doc predates v6). Concretely, already DONE and just needing the checklist mark flipped (plus a quick
confirming test run, not new code):

- Captions: SRT import, **speech-to-text via local whisper.cpp** (tools/whisper/fetch.sh + auto_captions.rs),
  caption styling presets (pop/karaoke/clean/boxed/impact), burn-in — all shipped. Tests: auto_captions.rs,
  caption_confidence.rs, caption_legibility.rs, caption_validate.rs.
- Multicam: add_multicam, list_multicam, switch_angle, sync_clips tools exist.
- Auto reframe, scene edit detection, auto-cut to beats: auto_reframe, scene_split/detect_cuts,
  beat_cut/cut_to_beat/track_beats/bind_beats all exist and tested.
- Proxy workflow: shipped this cycle (top CHANGELOG line, docs/PROXY_WORKFLOW.md), offline relink too
  (proxy_relink/proxy_remove).
- Morph cut, push/slide wipes: add_morph_cut, add_push_wipe (push_wipe.rs test).
- Paste attributes, effect presets, templates: paste_attributes tool + template_apply/define/list/describe +
  title/lower-third preset system (templates.rs, template_examples.rs, title_creator_presets.rs).
- Essential-sound basics: normalize_loudness, auto_duck (duck_speech.rs test), EQ/compressor/deesser/reverb via
  add_audio_effect, stereo pan (set_audio_pan), LUFS loudness meter + LRA + momentary/short-term curves
  (measure_loudness), audio_spectrum.
- Audio sync by waveform: av_offset.rs (offset detection, tested).
- MCP batch tool: `batch` is a registered tool (batch.rs + batch_calc/batch_expand + 5 test files).
- Denoise / sharpen / dehaze: all three are registered color effect kinds with tests (dehaze.rs test; sharpen and
  denoise are marked [x] already / confirmed present).
- Blend modes: blend_modes_w3c.rs — W3C compositing blend modes implemented.

These get verified (not just assumed) before any FEATURES.md flip: each claimed-done item runs its named test
under the lock before I mark it [x].

## Genuinely missing or honestly partial (build targets, cheapest first)

| # | Item (FEATURES.md line) | Status | Size | Plan |
|---|---|---|---|---|
| 1 | Rolling/regular edit by timecode text helpers ("+10", "-5 frames") (L87) | missing | S | Small parser + MCP tool `nudge_edit`/arg on existing trim/roll tools accepting a timecode delta string. |
| 2 | Replace clip keeping effects (L84) | missing | S | New tool `replace_clip_media`: swap source, keep effect stack/keyframes, re-run duration fit. |
| 3 | Copy/paste keyframes (L84) | missing | S | New tool `copy_keyframes`/`paste_keyframes` (clip/property -> clip/property, with/without time offset). |
| 4 | Subclips, clip labels/colors, bins, metadata, search (L83) | partial | M | Clip-level label/color metadata exists (names.rs) — audit needs confirm; subclip (in/out-bound virtual clip referencing a parent asset) and bins/search over the project are new. |
| 5 | Insert vs overwrite with auto-ripple, 3/4-point edit, source in/out marks (L81) | partial | M | add_clip likely supports overwrite; insert+ripple and 3/4-point edit (mark in/out then edit) are new thin tools over existing ripple/insert primitives. |
| 6 | Noise gate, limiter, de-reverb, surround mix, audio transitions (constant-power/exponential) (L170) | partial | M | add_audio_effect kind enum gains `noise_gate`, `limiter`; de-reverb and surround are correctly honest [~] (see XL note below); crossfade curve shapes on audio clip edges are a smaller add. |
| 7 | Clarity, lens profile correction, bilateral/channel/compound/smart blur variants, reduce interlace flicker (L131, L467) | partial | M | `clarity` as a local-contrast effect kind (unsharp at larger radius/lower amount, masked); bilateral as a new denoise mode; dehaze/sharpen already there. Smart/compound/channel blur and full lens-profile-DB correction are closest-real + honest [~]. |
| 8 | Vibrance, selective-color refinements, color link, equalize (L471) | partial | M | `vibrance` effect kind (saturation curve weighted toward low-sat pixels) is a real, scoped add. Full Lumetri parity / CC toner are honest [~]. |
| 9 | Shape layer operators (fill/stroke/gradient fill/trim paths/repeater/zig-zag/pucker-bloat/wiggle paths/round corners/twist) (L236) | missing | L | Real subset buildable on top of existing path_tools/pathops/paths.rs (shape ops already does boolean + sdf); repeater/zig-zag/pucker-bloat/wiggle-paths are new path transforms. |
| 10 | Expression evaluator (free-form interpreter, index/valueAtTime linkage) (L559) | partial | L | set_expression already ships a fixed op-stack (wiggle/loop/sin/inertia/clamp/round) — a small real expression-language subset (arithmetic + the existing ops + valueAtTime) is buildable; a full general interpreter with external scripting is XL-honest-[~]. |
| 11 | Voiceover record, scripting hooks, data-driven animation (JSON/CSV), responsive time (L171, L239) | missing | L | No live mic I/O surface on this server (MCP/offline renderer) — voiceover record gets an honest [~] (import an existing audio file is already covered by add_audio_clip). Data-driven (CSV/JSON drives text/keyframes) is buildable as a real import tool. |
| 12 | Generator/stylize effect catalog gaps: 4-color gradient, lightning variants, audio spectrum visual, beam/laser, cell pattern, write-on, caustics, fractal (Mandelbrot/Julia), radio waves, scribble, brush strokes, cartoon, texturize, sketch/oil-paint/anaglyph, VR sphere tools, Particle Playground-style forces/fluid/cloth (L475, L477, L483, L485) | missing | XL | This is most of Adobe's Generate/Stylize/VR/Particle-Playground catalogs — dozens of distinct generator kernels. Real, tested subset per build cycle (I will ship the ones with clear, verifiable pixel math: 4-color gradient, write-on, cell pattern, fractal Mandelbrot/Julia, radio waves, scribble/stroke-path generators, sketch/oil-paint/anaglyph as deterministic filters); fluid/cloth/rigid-body sims beyond the existing particle/physics crate integration are honest [~] (closest real: extend simulate_physics / particle_simulate_raw with simple spring/drag forces, not a full fluid solver). |
| 13 | wgpu full pipeline: WGSL effect kernels for the 2D stack, GPU compositing, frame textures in VRAM, async readback, hardware encode via NVENC (L557) | partial | XL | 3D mesh rendering already has an offscreen wgpu path (offscreen_wgpu.rs, render_wgpu_pbr, render_scene3d_wgpu). Porting the whole 2D effect stack (152 kinds) to WGSL kernels and wiring NVENC hardware encode is a multi-week rewrite; this box also has no GPU. Honest [~]: document the existing wgpu 3D path, do not fake a 2D GPU compositor. |
| 14 | SIMD / u8 half-res fast path / dirty-region cache / tile-based rendering (L558) | missing | XL | Real perf engineering across the whole CPU f32 renderer; out of scope for a correctness-TDD pass without regressing goldens. Honest [~] with a measured starting number (current ~2fps/720p, from FEATURES.md) and no false claim of a rewrite. |
| 15 | Project format v2 with compositions, versioned migrations (L560) | missing | XL | A real schema/version migration system touches save/load for every project ever saved; needs careful backward-compat tests. Will attempt a real v1->v2 additive schema (new optional fields only, old projects load unchanged) rather than a full migration framework. |
| 16 | Puppet rig, liquify, general bezier warp, content-aware object removal, ML roto/segmentation (L213, L223, L237) | partial | XL | Beta mesh pin/tracking already exists (warper.rs/mesh_ingest etc.) — that's the real current ceiling without a licensed/trained ML segmentation model or licensed warp library. Honest [~], unchanged from today's doc language. |
| 17 | GUI (L562) | n/a | XL | Marked "(deferred by the user)" in FEATURES.md itself — kept deferred unless told otherwise; flagging to Naman rather than silently building a GUI given the scale of that alone. |
| 18 | MOGRT import / general parametric template system (L73) | missing | XL (licensed format) | Adobe's MOGRT is a closed, licensed format. Honest [~]: kimo's own JSON template system (template_define/apply) is the closest real substitute; no MOGRT parser without their SDK. |

## Build order

Cheapest-impact first, as instructed:
1. Verify-and-flip the "already done" items above (confirm each test passes, correct FEATURES.md marks) — fast,
   high item-count impact, zero new-code risk.
2. Items 1-3 (timecode nudge helpers, replace-clip, copy/paste keyframes) — small, self-contained, clear tests.
3. Items 6-8 (audio noise_gate/limiter, clarity/vibrance/bilateral effect kinds) — additive effect-kind work on
   existing dispatch tables, low regression risk (goldens are byte-pinned, new effects are opt-in).
4. Items 4-5 (subclips/bins/search, insert/overwrite/3-4-point edit) — medium, touches project model.
5. Item 9 (shape operators) and item 10 (expression subset) — larger, still bounded.
6. Item 11 (data-driven CSV/JSON + voiceover-record honest [~]).
7. Item 12 (generator/stylize subset) — ongoing, ship in small batches, rest honestly [~].
8. Items 13-18 — XL, last. Each gets its closest real implementation plus an honest, specific [~] limit line; none
   gets marked [x] without a passing test, per the honesty rule. I will flag items 17 (GUI, already deferred) and
   18 (MOGRT, licensed format) to Tab/Naman rather than silently building substitutes that look like the real thing.

## Rules carried into every commit
- TDD: failing test first, registered MCP command, golden frames unmoved, warnings <= 35, no binary/model/media
  committed, author/committer `Naman Sharma <naman-sharma12345@users.noreply.github.com>`.
- `cargo clean -p kimo-core` under the lock if the manifold CMake cache-path error recurs (hit once already during
  this audit's baseline build — the worktree path differs from /tmp/v6rel_A's cached cmake source path).


## Baseline (confirmed, 65324e9, before any v7 commit)

Full workspace suite (`cargo test --profile gate --workspace --no-fail-fast`, under the lock; needed one
`cargo clean -p kimo-core` first for the known manifold CMake cache-path mismatch against /tmp/v6rel_A's cache):

- 291 test binaries, 2023 tests passed, 0 unexpected failures.
- 2 known pre-existing failures, both already documented, neither a v7 regression:
  - `readme_count::readme_v3_tool_count_matches_the_registry` — the README no longer has a line containing
    "registered tools" (looks like a side effect of Naman's recent README redesign PRs). Pre-existing on 65324e9,
    before any v7 commit. Will restore a tool-count line in README as part of a small-item pass so this test
    passes again (or update the test if Naman wants the new README shape kept as-is — flagging, not deciding).
  - `robust_est.rs` doctest — the stated known pre-existing failure (a stray `License Agreement` string collides
    with the doctest's inferred fence). Per the mission brief, left alone.
- Warnings: 35 lib warnings (at the ceiling, matches FEATURES.md's "Warnings stay at 35, or lower" rule already
  being honored on main).
- golden_frames / recipe_frames: not separately isolated in this run (they're inside the full suite above and
  passed); will re-confirm their exact hashes (golden_frames 8959a9e4…, recipe_frames 02652276…) explicitly before
  the v7 wrap, and after every batch that could plausibly touch rendering.

## Steer received 2026-10-27 ~19:25 UTC

Naman confirmed (12:43 AM IST): "gui at last" — the GUI editor (crates/kimo-gui, currently a skeleton with demo
assets and a capabilities/default.json but **not a workspace member** — `Cargo.toml`'s `[workspace] members` lists
only kimo-core, kimo-mcp, kimo-cli) is now in scope, built as the very last item, after everything else. MOGRT:
no answer yet — keeping the JSON-template substitute with an honest [~] note, not attempting the closed format.
Item 17 above is un-deferred; item 18 unchanged. Build order otherwise unchanged: small items now.
