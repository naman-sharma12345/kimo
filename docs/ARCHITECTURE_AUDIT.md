# kimo architecture audit (v2 prep)

Measured on the 2 GB dev box, release build. Every row has a number or a file reference; nothing here is a guess.

## Done

| # | Finding | Evidence | Fix | Result |
|---|---|---|---|---|
| 1 | `dispatch` rebuilt the whole tool table (about 260 tools, every JSON schema) on every call and found the tool by linear scan | `commands.rs` `dispatch` called `registry()`; empty project: 1.88 ms for a trivial read-only call | table built once (`OnceLock`) plus a name index | 0.11 ms (17x) |
| 2 | Every call, including read-only ones, cloned the whole project for undo/rollback | read-only `list_lights` at 3000 clips: 6.14 ms, growing with project size | clone only when the tool is marked mutating | 0.17 ms at 3000 clips (36x), flat |
| 3 | Every mutating call also serialised the whole project just to size the history cap | `serde_json::to_vec(&before)` on each mutation | estimate refreshed every 16 mutations (`Session.snap_sz`) | mutating `set_light` at 3000 clips: 20.1 ms to 11.0 ms |

Proofs: `tests/dispatch_latency_proof.py` (fails on the old code), plus undo/checkpoint/history/undo-coverage proofs and the fuzz run (260 tools, 9451 calls, 0 failures) unchanged.

Trade-off, stated plainly: a tool marked read-only that secretly mutates is no longer rolled back on a panic. `tests/undo_coverage_proof.py` is the guard that such a tool does not exist (it found one earlier, list_modifiers, now fixed).

## Open, ranked by value

1. **Undo is whole-project clones.** A mutating call still costs O(project): 11 ms at 3000 clips, history capped at 100 steps or about 256 MB. Fix: structural sharing (`Arc` per track and clip, copy-on-write) so a snapshot is O(changed). Risk: moderate, touches the model.
2. **Hidden global render state.** 19 `thread_local!` globals (scene lights, tilt, slab projector, post flag, backdrop, displacement field). Each is set by the renderer and read by an effect, so effects are not pure functions of their arguments. Two real bugs in this branch stack came from that (frosted_glass skipped on tilted layers; bloom/glow clipped by alpha). Fix: an explicit `RenderCtx` passed to effects.
3. **Three effect catalogs plus string-match dispatch.** `CATALOG`, `CATALOG2`, `catalog3()` and a `match kind` in `apply`; adding an effect means editing 3 to 4 places. Fix: one `Effect` registry entry (name, params, post/local flags, run).
4. **Source layout.** 1.3 MB of Rust in about 11k lines; `commands.rs` has lines up to 5868 characters and `effects2.rs` 3381. rustfmt and diffs are unusable on them. Fix: split the registry by domain (the `ext/` module pattern already exists) and format.
5. **Errors are plain strings.** The MCP surface returns text only. For an AI-first tool, a structured error (`code`, `param`, `message`, `hint`) lets an agent self-correct. Fix: add a typed error alongside the string, backward compatible.
6. **Per-layer full-canvas f32 RGBA.** Layers are rendered on full-canvas buffers, with a bbox crop only for a short list of local effects. Fix: widen the cropped path.

Not audited yet: colour pipeline internals (owned by the COLOR agent), export streaming, the timeline model beyond what the benchmark touched.

## Golden harness and fonts
The golden text scene depends on which fonts the engine finds. Font lookup tries KIMO_FONTS first, then directories relative to the executable, then ./fonts, ~/kimo/fonts and /usr/share/fonts. The harness (crates/kimo-core/tests/golden_frames.rs) sets KIMO_FONTS to the workspace fonts directory so results do not depend on where the test binary lives. A machine without the workspace fonts directory will produce different text hashes.

## CI note: tests that need ffmpeg
`tests/track_diag_proof.py` (needs ffmpeg, numpy, Pillow, a built `target/release/kimo-mcp`; ~60 s, writes only /tmp) and the `#[ignore]`d `auto_duck_leaves_clips_outside_the_dialogue_shared_with_the_undo_snapshot` Rust test (run with `--include-ignored`) need ffmpeg. The default Rust CI stays ffmpeg-free; run these in a job that installs ffmpeg.

## Build and test cost on the 2 GB box (measured Oct 12)
Box: 1982 MB RAM, one crate `kimo-core` (about 1.63 MB of Rust source in src/, longest line in commands.rs 5868 chars). Release profile is opt-level 3 with thin LTO; there is no dev/test profile tuning yet.

| Step | Wall time |
|---|---|
| `cargo check -p kimo-core --lib` (after touching lib.rs, whole crate rechecked) | 11 s |
| `cargo build --release` after editing commands.rs | 106 to 127 s |
| `cargo test --release -p kimo-core --lib --no-run` | 106 s |
| `cargo test --release -p kimo-core` (all of tests/*.rs, 73 integration test files, each links its own binary) | 25+ min (killed unfinished twice) |
| 5 chosen integration targets (golden_frames, registry_unique, tool_contracts, arc_sharing, error_table) | about 3 to 4 min |
| lib unit tests (126 pass, 3 ignored) run time | 4 s |

Reading: compile of the one crate is about 2 minutes, so the cost is linking many test binaries, not compiling. Run named `--test` targets, not `-p kimo-core`. Use `cargo check` (11 s) while editing. Peak memory was not measured (no /usr/bin/time on the box); free memory stayed above 750 MB during builds. Not yet tried: [profile.test] opt-level 1 / debug 0 / codegen-units, lld. A crate split (unit 4) is the structural fix.

## Dead-arg candidates (report only, tests/dead_arg_report.py, 144 s)
The tool calls each tool with its required args, then with one optional arg set to a non-default value, and flags the arg when neither the result nor get_project changes. 117 tools exercised, 137 not exercised (base call errors with generic args). 48 candidates. These are CANDIDATES, not confirmed dead: many are args whose effect only shows in rendered pixels (ease, mode, anchor, simulate_physics gravity/drag, set_expression min/max/step) or whose alt value equals the default. Confirming each needs a render or state diff per tool. List: add_text anchor; set_audio_volume_key ease; detect_beats min_gap; set_speed_ramp ease; add_particles size; set_echo decay/mode/spacing; set_expression decay/fps/max/min/mode/phase/step; add_text_animator delay/duration/out/unit; add_detection_box flash/impact_scale; add_prompt_box type_time; set_camera_keyframe ease; set_time_mode at; set_keyframe ease; add_mask mode; get_curve t0/t1; simplify_curve tolerance; smooth_curve sigma; simulate_physics collide/drag/friction/gravity/gravity_x/restitution/start/walls; shards_project min_alpha; save_project history_steps; distribute_clips gap; set_track index; analyze_color units; add_modifier decay/direction/mode; remove_modifier kind; track_set_info include_status.
Confirmed bug found by the earlier strict-arg pass: set_text_style ignored line_mask (fixed on arch-u16). Confirmed misuse: set_property was given time by 7 proof files (fixed, arch-u15).

## Proof harness hygiene
`tests/undo_coverage_proof.py` filled every required string arg with "x", so path-like args wrote a 1.2 MB LUT file `./x` into the repo root. Fixed (path-like args go to /tmp). `tests/run_proofs.sh` now fails the run when any proof leaves a stray file in the repo (checked with a stray-writing script). `tests/arc_property_proof.py` compares against an oracle binary built before the Arc flip and before central track-lock enforcement (u9l); with the stale /tmp/kimo-oracle it now fails at split_clip on a locked track (new binary errors, oracle allows). The oracle needs rebuilding from a post-u9l commit.

## Render baseline, unit 8 step 0 (tests/render_bench.py, Oct 12, 640x360, release build, one core of a 2 GB box)
Median of 5 `render_frame` calls at t=1.0 (includes MCP round trip and PNG write to /tmp), after one warm-up call.

| Scene | median ms | min | max |
|---|---|---|---|
| background rect only | 83 | 76 | 86 |
| + text (60 px) | 78 | 76 | 84 |
| + ellipse with blur 20 | 92 | 85 | 102 |
| + rect with frosted_glass | 96 | 92 | 105 |
| + 400 dust particles | 79 | 66 | 85 |
| + 20 translucent rects | 129 | 118 | 153 |

Reading: a near-empty frame already costs about 80 ms, and every scene sits within 20 ms of it except the 20-rect scene (+46 ms, about 2.3 ms per layer). So the fixed per-call cost (frame setup, PNG encode and write, IPC) dominates these small scenes; measure that split before optimizing any single effect. Not measured: 1080p, video-source decode, multi-frame export throughput.
