<p align="center">
  <img src="docs/assets/kimo-logo.png" alt="kimo" width="160">
</p>

<h1 align="center">kimo</h1>

<p align="center"><b>A video editor an AI can drive.</b><br>
MCP-first. Every feature is a tool call. Written in Rust, runs on a 2 GB box.<br>
<a href="https://getkimo.tech"><b>getkimo.tech</b></a></p>

<p align="center">
  <img alt="license" src="https://img.shields.io/badge/license-MIT-F4F0E8?labelColor=0E0E12">
  <img alt="language" src="https://img.shields.io/badge/rust-core-FF5C28?labelColor=0E0E12">
  <img alt="tools" src="https://img.shields.io/badge/MCP%20tools-331%20(v3)-F4F0E8?labelColor=0E0E12">
  <img alt="fuzz" src="https://img.shields.io/badge/tool%20fuzz-9359%20calls%2C%200%20failures-FF5C28?labelColor=0E0E12">
</p>

---

## What it is

Development recipe: `lower_third` expands a title, optional subtitle, panel and accent into ordinary primitive operations. Expansion is deterministic and uses clip-local enter/exit keyframes; RV-11 batch supplies validation, dry-run, revision checks and one-step undo. No hidden renderer or export path.
Development recipes: `narration_section` preserves RV-8 transcript-bound caption timing and derives speech windows from chosen takes. `lower_third` expands a title, optional subtitle, panel and accent into ordinary primitive operations. Expansion is deterministic and uses clip-local enter/exit keyframes; RV-11 batch supplies validation, dry-run, revision checks and one-step undo. No hidden renderer or export path.
Scene tools (RV-11): `batch` runs many tool calls as one atomic, undoable step (validates all ops first; dry_run returns the real ids and a `revision`, and `expect_revision` rejects a drifted project). `recipe_list` / `recipe_describe` / `recipe_apply` expand named scenes (`lower_third`, `narration_section`, `title_card`, `stat_counter`, `product_reveal`, `explainer_diagram`) into ordinary tool calls run through `batch`. Stated limits: clips have no name field, so `<prefix>/<role>` labels are returned in the result `roles` map, not stored on clips; `stat_counter` pops its value in (scale and opacity) and does not count up, because text content cannot animate.

Export manifests (U3a): `export_manifest` hashes every clip (content minus names, the bytes of every file it reads, its transform-parent chain, the engine version) and every fixed-length chunk of the timeline; `explain_invalidation` compares the current project with an earlier manifest and names, per chunk, why it is stale (`clip_changed` with the changed parts, `asset_changed`, `settings`, `engine`, `clip_added`, `clip_removed`). Markers, track locks and names never invalidate. Both are read-only.

**v1.0.0.** 255 tools, fuzzed (9359 calls, 0 failures). `main` is the stable v1. The `v2` branch carries the v2 work (263 tools, fuzzed: 9549 calls, 0 failures; see patches/MANIFEST.md for the unit-by-unit changelog).

**v3 branch (2.0.0-dev.1, in development).** 331 tools, fuzzed (6497 calls, 0 failures). `kimo_info` reports the build: version, git commit, project schema version, enabled features, ffmpeg/ffprobe versions. Saved projects carry a `kimo` block (schema_version, created_with, last_saved_with; set `KIMO_NO_IDENTITY=1` to omit it) and export reports carry the same identity. `add_shape` alpha semantics changed: the fill alpha no longer folds into layer opacity. Fill alpha (`fill_alpha`), stroke alpha (`stroke_alpha`) and layer opacity are separate animatable properties, so a hollow ring (fill alpha 0) keeps its stroke. Invalid enum values now error (tool input only; saved projects with old values still load and render as before): add_text align, set_mask shape, add_particles kind, add_text_animator unit, add_transition kind/ease and set_blend mode used to be accepted silently; every rejection lists the allowed values. Old saved projects are unchanged (a project without the new properties renders as before). `add_dim_mask` dims everything outside (or inside) a box with a feathered, rounded rect mask (one solid layer plus one inverted mask, optional ease keys); `spotlight` is a pure wrapper that returns the same expansion by default (`apply:false`) and runs it as one undo group with `apply:true`. `fit_box_to_text` returns a text clip's measured ink rect plus padding (read-only); `callout_card` is a pure wrapper that builds a rounded card, text, optional badge and optional shadow around measured text, returns the expansion by default and applies it as one undo group with `apply:true`; text is never clipped or wrapped, too wide gives `text_too_wide`.

Chunked export jobs (RV-6): `export_job_start` renders picture-only chunks in the background (poll with `job_status`, stop with `cancel_job`, continue with `resume_job`) and joins them with one continuous audio stream. A chunk is reused only when its dependency key (see `export_manifest`) matches AND the chunk file's sha256 still verifies; the result lists which chunks were reused or rendered and why. `force:true` re-renders everything. Cancel keeps finished chunks and marks the unfinished one `.partial`; `resume_job` refuses with `resume_mismatch` naming the first differing dependency if the project changed. ETA stays null until 3 chunks have been measured.

kimo is a video editor with no GUI. You talk to it over the [Model Context Protocol](https://modelcontextprotocol.io). Cut, keyframe, grade, track, key, relight, simulate, caption, mix and export are all registered commands, and the MCP server's tool list is generated from that same registry. There is no feature a human can reach that an AI cannot.

The target is the surface of CapCut, After Effects, Premiere and DaVinci Resolve in one engine: editing, motion graphics, VFX, tracking and pro color. It is an early engine with honest tiers. Read [FEATURES.md](FEATURES.md) for what is solid, what is beta and what is not built.

- **MCP first.** Add a tool in the registry and it is in the MCP server, the fuzz test and the schema automatically.
- **CPU, small memory.** The renderer is CPU only and tuned for a 2 GB machine. GPU code is off by default with a CPU fallback.
- **Measured, not claimed.** Every "better than X" line in FEATURES.md has a stated baseline and a measured number, including the cases where we lost.
- **Few entry points.** `tools/list` returns 16 grouped tools (15.7 KB) instead of 255 (176 KB), measured 11.2x smaller (compact JSON, v1.0.0). Every individual tool is still callable by its own name.
- **Fuzzed.** 255 tools (v1.0.0) / 263 tools (v2), 9359 / 9549 calls, 0 failures, before anything is published.

## Run the v3 branch (build from source, no main merge needed)

The prebuilt downloads below are v1.0.0. To run the v3 branch (the tool count in the v3 paragraph above), clone it and build it. Everything in this section was run against a build of this branch.

**Prerequisites**

- Rust stable (install with [rustup](https://rustup.rs)) and a C linker (`build-essential` on Debian/Ubuntu).
- `ffmpeg` and `ffprobe` on PATH (`sudo apt install ffmpeg`; checked with ffmpeg 4.4.2). Decode, encode and probing go through them.
- About 2 GB of RAM is enough to run. The release build is the heavy part: use `CARGO_BUILD_JOBS=1` on a small box (measured 17 minutes for a cold release build of the whole workspace on a shared 2 CPU / 2 GB machine).
- Linux x86_64 is the tested platform. Other platforms: see the Windows and macOS notes below.

**Build**

```
git clone -b v3 https://github.com/naman-sharma12345/kimo.git
cd kimo
CARGO_BUILD_JOBS=1 cargo build --release --workspace --bins
ls target/release/kimo target/release/kimo-mcp
target/release/kimo --version          # prints version 2.0.0-dev.1 and the git commit
```

`kimo-mcp` is the MCP server (stdio). `kimo` is the command line client for the same tools.

**Check the MCP server answers**

```
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | target/release/kimo-mcp
```

The reply contains `"serverInfo":{"commit":"...","name":"kimo",...,"version":"2.0.0-dev.1"}`.

**Client config.** Use the absolute path of the binary you just built, for example `/home/you/kimo/target/release/kimo-mcp`.

- Claude Code: `claude mcp add --transport stdio kimo -- /home/you/kimo/target/release/kimo-mcp`
- Claude Desktop: add this to `claude_desktop_config.json` (Settings, Developer, Edit Config) and restart the app. The `env` block is optional; `KIMO_TOOLS=core` returns a short `tools/list`.

```json
{
  "mcpServers": {
    "kimo": {
      "command": "/home/you/kimo/target/release/kimo-mcp",
      "args": [],
      "env": { "KIMO_TOOLS": "core" }
    }
  }
}
```

- Cursor: put the same block in `~/.cursor/mcp.json` (all projects) or `.cursor/mcp.json` (one project), then switch the server on in Cursor's MCP settings.
- Generic MCP client (stdio): command = the absolute path, no arguments, optional env as above. Any client that registers a stdio server by command and uses the `mcpServers` shape takes the block as is.

The server itself (launch, `initialize`, `tools/list`) is verified on this build; the client config file locations come from each client's docs, not from a run of those apps here.

Codex and OpenCode use the formats shown under "Setup" below with the same path. `KIMO_TOOLS=core` (env var on the server entry) shrinks `tools/list`; see "Tool list profiles" below.

**Quickstart with the CLI.** State lives in a project file (`--project p.kimo`, created on first use). Every command below ran against this build; replace the media path with your own file.

```
K=target/release/kimo
$K -q call add_track --project p.kimo                                  # prints {"track_id":"track1"}
$K -q call import_media --project p.kimo --args '{"path":"/abs/path/in.mp4"}'     # returns asset_id, e.g. asset2
$K -q call add_clip --project p.kimo --args '{"track_id":"track1","source_type":"media","asset_id":"asset2","start":0,"duration":3}'
$K -q call add_title --project p.kimo --args '{"track_id":"track1","style":"lower_third","text":"Hello kimo","start":0.5,"duration":2}'
$K -q call render_frame --project p.kimo --args '{"time":1,"path":"/tmp/f.png"}'  # look at one frame
$K -q call describe_frame --project p.kimo --args '{"time":1}'                    # numbers instead of pixels: stats, histogram, clipping, visible clips
$K -q call contact_sheet --project p.kimo --args '{"path":"/tmp/sheet.png","times":[0,1,2]}'
$K export p.kimo out.mp4                                               # render the timeline
$K tools search "color grade"                                          # find tools by words
$K tools describe add_clip                                             # argument schema of one tool
```

Ids (`track1`, `asset2`) are what the previous call returned, so read them from the output. `export` checks free memory first and refuses with a clear message when the frame would not fit; set `KIMO_MEM_MB` to override its budget. Exit codes: 0 ok, 1 tool error, 2 usage, 3 preflight failed. `kimo --help` lists every subcommand.

## Install

**Download a binary (Linux x86_64, static, no dependencies except ffmpeg):**

Windows x64 also ships a per-user setup installer and portable ZIP, with the
same MCP server and bundled fonts. Download Windows assets from
[GitHub Releases](https://github.com/naman-sharma12345/kimo/releases), or from
a manual `release` workflow run's artifacts. See the
[Windows setup guide](packaging/windows/WINDOWS.md) for FFmpeg installation
and MCP configuration. To build locally, run
`powershell -ExecutionPolicy Bypass -File packaging/windows/build.ps1`
with Rust, Python 3, and Inno Setup 6 installed.

```
curl -LO https://github.com/naman-sharma12345/kimo/releases/download/v1.0.0/kimo-v1.0.0-linux-x86_64.tar.gz
tar xzf kimo-v1.0.0-linux-x86_64.tar.gz
./kimo-v1.0.0-linux-x86_64/kimo-mcp        # speaks MCP over stdin/stdout
```

The binary is about 10 MB and answers its first MCP request in about 2 ms (measured: `initialize` 2 ms, first `tools/list` 6 ms). It is statically linked (musl), so it runs on any x86_64 Linux. It needs `ffmpeg` and `ffprobe` on PATH for decode and encode.

**Windows x64:** download the installer (`kimo-1.0.0-windows-x86_64-setup.exe`) or the portable ZIP from the [Windows release](https://github.com/naman-sharma12345/kimo/releases/tag/v1.0.0-windows.1). The installer is unsigned, so Windows may show a SmartScreen warning; check the `.sha256` file next to it. See the [Windows setup guide](packaging/windows/WINDOWS.md).

macOS binaries are not shipped yet; build from source there (below).

**Build from source** (Rust stable):

```
cargo build --release                 # target/release/kimo-mcp
python3 examples/demo_grade.py        # graded 2-clip sequence with a transition, built only with MCP calls
python3 examples/effects_gallery.py   # renders every effect
```

## Setup: add kimo to your AI client

kimo is a local MCP server that speaks over stdin/stdout (the "stdio" transport). There is nothing to start by hand: your client launches `kimo-mcp` itself. You only need the path to the binary.

**1. Get the binary** (see Install above) and put it somewhere stable, for example:

```
mkdir -p ~/.local/bin && cp kimo-v1.0.0-linux-x86_64/kimo-mcp ~/.local/bin/
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | ~/.local/bin/kimo-mcp
```

The second command should print a JSON reply containing `"serverInfo":{"name":"kimo"`. Install `ffmpeg` and `ffprobe` too (`sudo apt install ffmpeg`, or your package manager).

**2. Add it to your client.** Replace `/home/you/.local/bin/kimo-mcp` with your absolute path. Config formats below were checked against each client's public docs on 2026-10-11; client configs change, so if one fails, check the linked page.

### Claude Code
([docs](https://code.claude.com/docs/en/mcp-servers))

```
claude mcp add --transport stdio kimo -- /home/you/.local/bin/kimo-mcp
```

Add `--scope user` to make it available in every project, or `--scope project` to write it to a shared `.mcp.json`. Check with `claude mcp list`, or type `/mcp` inside Claude Code. The same thing as a `.mcp.json` file in your project:

```json
{ "mcpServers": { "kimo": { "command": "/home/you/.local/bin/kimo-mcp", "args": [] } } }
```

### OpenAI Codex (CLI and IDE extension)
([docs](https://developers.openai.com/codex/mcp))

```
codex mcp add kimo -- /home/you/.local/bin/kimo-mcp
```

Or edit `~/.codex/config.toml` (or `.codex/config.toml` in a trusted project):

```toml
[mcp_servers.kimo]
command = "/home/you/.local/bin/kimo-mcp"
```

### Cursor
([docs](https://cursor.com/docs/context/mcp))

Create `~/.cursor/mcp.json` (all projects) or `.cursor/mcp.json` (one project):

```json
{ "mcpServers": { "kimo": { "command": "/home/you/.local/bin/kimo-mcp", "args": [] } } }
```

Then open Cursor's MCP settings and switch the `kimo` server on.

### OpenCode
([docs](https://opencode.ai/docs/mcp-servers/))

Put this in `opencode.json` in your project, or in `~/.config/opencode/opencode.json` for all projects. Note that OpenCode takes `command` as a list and uses `type: "local"`:

```json
{
  "$schema": "https://opencode.ai/config.json",
  "mcp": {
    "kimo": { "type": "local", "command": ["/home/you/.local/bin/kimo-mcp"], "enabled": true }
  }
}
```

### Any other MCP client
kimo is a standard MCP stdio server (protocol 2024-11-05, `initialize`, `tools/list`, `tools/call`). Any client that can register a stdio server by command works: give it the absolute path to `kimo-mcp` and no arguments. Many clients use the `mcpServers` JSON shape shown for Cursor; some (for example Zed) use a different key, so check your client's MCP docs for the exact file and format. Only the four clients above were checked against their docs.

Windows: use the path to `kimo-mcp.exe` (the installer puts it in `%LOCALAPPDATA%\\Programs\\kimo\\bin`; in JSON write backslashes doubled, see the [Windows guide](packaging/windows/WINDOWS.md)). The configs above are the same except for the path; the Claude Code, Codex, Cursor and OpenCode formats were checked on their docs, not tested on Windows here. macOS: build from source (below) and use the path to `target/release/kimo-mcp`.

**3. Try it.** Ask your assistant: "Use kimo to import /path/to/clip.mp4, add a title, and render a frame." Or drive it yourself with the Python client in `examples/mcp_client.py`.

### Tool list profiles and recommended client setup
`KIMO_TOOLS` picks what `tools/list` returns. Every one of the 268 tools can be called by its own name under every profile.

| Profile | `tools/list` size | Use when |
|---|---|---|
| `grouped` (default) | 17 entries, about 4.5 KB | small context windows. Call a group with `action` and `args`, find tools with `search_tools`. |
| `core` | 29 entries, about 19 KB | the 26 most used tools with full schemas, plus `search_tools`, `kimo_describe`, `kimo_enable_group`. |
| `flat` | 270 entries, about 185 KB | clients with large context or tool search of their own. |
| `audio,color_grade` (group names) | those groups listed flat, the rest grouped | you work in a few areas. |

`search_tools(query, limit=8, group?)` returns ranked tools, effects and transitions with the required args and a `call_as` hint, so one search reaches any tool. `kimo_enable_group(name)` lists a group's tools individually from then on and sends `notifications/tools/list_changed` (the server advertises `tools.listChanged`). `tests/discovery_metric.py` measures calls and bytes to the first correct call for 10 tasks under each profile.

Recommended: start with the default. If your client re-reads the list on every turn and has the room, use `core`. Set it like this:

```
claude mcp add --transport stdio --env KIMO_TOOLS=core kimo -- /home/you/.local/bin/kimo-mcp
```

```toml
[mcp_servers.kimo.env]
KIMO_TOOLS = "core"
```

For JSON configs add `"env": { "KIMO_TOOLS": "core" }` to the server entry (OpenCode: `"environment": { "KIMO_TOOLS": "core" }`).

## How an AI sees the tools

The default `tools/list` is grouped, so a client does not have to read 255 tool schemas up front.

| Group | What it covers |
|---|---|
| project, timeline, animate, effects | project and media, edits, keyframes and motion, effects and transitions |
| masks, text, graphics, vfx3d | masks and roto, text and captions, shapes and HUD, 3D, particles and fracture |
| tracking, color_grade, color_science, scopes | tracking and camera solve, grading, color management, measurement |
| audio, output | audio, render and export |
| search_tools, kimo_describe, kimo_enable_group | find a tool by keywords (also searches the effect and transition catalogs; kimo_search is the older, unlisted form), get one action's full arguments, list a group flat (also searches the effect and transition catalogs), get one action's full arguments |

Call a group with `{"action": "<tool name>", "args": {...}}`, for example `{"name": "vfx3d", "action": "add_scene3d", "args": {...}}`. Calling a tool by its own name (`add_scene3d`) still works, and `tools/list` with `{"mode": "flat"}` (or `KIMO_TOOLS=flat`) lists all 255. A new tool is placed in a group by name rules in `crates/kimo-core/src/toolgroups.rs`, and a unit test fails if one is left unassigned. `tests/group_proof.py` checks that every tool is in exactly one group and that grouped and direct calls give the same result.

## v3 tool listing (331 registry tools)

This is the list the v3 build registers (read from `tools/list` of the grouped server plus `kimo_describe group=<name>`, 331 tools in 14 groups; the grouped `tools/list` itself shows 14 group tools plus `search_tools`, `kimo_describe`, `kimo_enable_group`). The release tag `v3.0.0` points at commit `b7faad6`; the build still reports version `2.0.0-dev.1` from `kimo --version` and `kimo_info`. Get any tool's arguments with `kimo tools describe <name>` or `kimo_describe`, and find tools by words with `search_tools`.

| Group | Tools |
|---|---|
| `project` (35) | `project_set`, `import_media`, `get_project`, `save_project`, `load_project`, `kimo_info`, `cache_stats`, `cache_clear`, `cache_set_budget`, `undo`, `redo`, `history_list`, `checkpoint`, `restore`, `checkpoint_delete`, `batch`, `list_dependencies`, `collect_project`, `relink_media`, `ext_list`, `set_meta`, `find`, `read_pixels`, `import_pixels`, `project_diff`, `recipe_list`, `recipe_describe`, `recipe_apply`, `set_limits`, `get_limits`, `template_define`, `template_apply`, `template_list`, `template_describe`, `template_delete` |
| `timeline` (37) | `add_track`, `add_clip`, `set_speed_ramp`, `group_clips`, `ungroup_clip`, `set_time_mode`, `freeze_frame`, `move_clip`, `trim_clip`, `split_clip`, `delete_clip`, `set_speed`, `ripple_delete`, `ripple_trim`, `roll_edit`, `slip_clip`, `slide_clip`, `insert_gap`, `lift_range`, `duplicate_clip`, `loop_clip`, `distribute_clips`, `swap_clips`, `rate_stretch`, `add_marker`, `list_markers`, `remove_marker`, `snap_time`, `list_edit_points`, `set_track`, `delete_track`, `split_all_at`, `remove_silence_cuts`, `sync_clips`, `add_multicam`, `switch_angle`, `list_multicam` |
| `animate` (32) | `set_expression`, `set_keyframe_bezier`, `add_camera_shake`, `set_motion_blur`, `set_camera`, `set_camera_keyframe`, `set_corner_pin`, `set_property`, `set_keyframe`, `set_blend`, `set_motion_path`, `remove_motion_path`, `set_flow_blur`, `render_flow`, `get_curve`, `move_key`, `remove_key`, `copy_curve`, `simplify_curve`, `smooth_curve`, `track_beats`, `bind_beats`, `set_mesh_pin`, `apply_motion_preset`, `motion_extract`, `motion_diff`, `set_keyframes`, `sample_property`, `bake_property`, `add_null`, `set_parent`, `get_transform` |
| `effects` (15) | `list_transitions`, `add_adjustment_layer`, `add_speed_lines`, `add_glitch_confetti`, `set_echo`, `set_posterize_time`, `list_effects`, `add_effect`, `set_effect_param`, `remove_effect`, `add_transition`, `set_effect_mask`, `add_managed_linear_effect`, `set_effect_encoding`, `effect_describe` |
| `masks` (20) | `add_dim_mask`, `spotlight`, `fit_box_to_text`, `callout_card`, `set_mask`, `set_mask_keyframe`, `remove_mask`, `add_mask`, `update_mask`, `remove_mask_layer`, `reorder_mask`, `list_masks`, `set_mask_prop_key`, `set_mask_path_key`, `roto_propagate`, `set_track_matte`, `remove_track_matte`, `render_mask`, `track_matte`, `refine_matte` |
| `text` (23) | `add_text`, `set_text`, `import_srt`, `transcribe`, `set_text_style`, `add_text_animator`, `set_animator_keyframe`, `add_title`, `list_fonts`, `set_text_weight`, `set_text_path`, `add_captions`, `list_captions`, `set_caption_style`, `edit_caption`, `transcript_import`, `transcript_get`, `transcript_edit_word`, `transcript_set_take`, `transcript_coverage`, `transcript_to_captions`, `validate_captions`, `transcript_edit_preview` |
| `graphics` (20) | `add_box_expand`, `add_shape`, `add_detection_box`, `add_hud_frame`, `add_counter`, `add_prompt_box`, `add_modifier`, `list_modifiers`, `remove_modifier`, `list_shape_modifiers`, `add_shape_modifier`, `remove_shape_modifier`, `list_layer_styles`, `set_layer_style`, `remove_layer_style`, `add_path_shape`, `set_path`, `path_info`, `add_path_key`, `clear_path_keys` |
| `vfx3d` (15) | `add_particles`, `add_emitter`, `add_shard_burst`, `add_scene3d`, `simulate_physics`, `shards_project`, `set_light`, `light_rig`, `remove_light`, `list_lights`, `set_material`, `list_materials`, `remove_material`, `set_relight`, `preview_material` |
| `tracking` (25) | `track_long_gap`, `track_point`, `stabilize`, `track_planar`, `pin_layer`, `track_mesh`, `solve_camera`, `list_camera_solves`, `camera_solve_info`, `apply_camera_solve`, `grade_node_tracking`, `track_planar_anchored`, `track_features`, `track_set_info`, `fit_track_homography`, `fit_info`, `decompose_track_homography`, `track_jitter_metrics`, `corner_disagreement`, `track_feature_residuals`, `lk_residual_audit`, `ecc_refine_track`, `render_error_field`, `validate_planar_track`, `undistort_clip` |
| `color_grade` (22) | `preview_subject_match`, `match_grade`, `set_color_curve`, `pick_white_balance`, `match_color`, `inspect_asc_cdl`, `apply_asc_cdl`, `generate_lut_node`, `evaluate_grade_graph_samples`, `inspect_grade_graph`, `set_grade_graph`, `add_node`, `connect_nodes`, `disconnect_nodes`, `remove_grade_node`, `set_grade_output`, `set_grade_node`, `set_node_param`, `set_node_mask`, `create_shared_grade_graph`, `bind_shared_grade_graphs`, `remove_shared_grade_graph` |
| `color_science` (18) | `inspect_color_frame_metadata`, `inspect_color_delivery`, `inspect_color_pipeline`, `compare_aces2_reference`, `sample_linear_neutral`, `audit_color_output`, `inspect_float_media`, `set_hdr_mastering`, `set_color_pipeline`, `set_clip_input_color`, `validate_hdr_transfer_math`, `evaluate_hlg_reference_display`, `add_acescct_primaries`, `inspect_high_bit_delivery_contract`, `inspect_film_response`, `inspect_ocio_config`, `transform_ocio_samples`, `transform_ocio_view_samples` |
| `scopes` (12) | `get_scopes`, `compare_linear_exposure`, `render_gamut_excursions`, `analyze_exposure_sequence`, `render_exposure_false_color`, `analyze_exposure`, `analyze_color`, `render_scope`, `render_display_scope`, `measure_loudness`, `inspect_grade_scopes`, `set_grade_scopes` |
| `audio` (17) | `add_audio_clip`, `set_audio`, `set_audio_volume_key`, `detect_beats`, `set_audio_effect_key`, `audio_filter_response`, `audio_effect_describe`, `audio_probe`, `read_samples`, `import_samples`, `audio_transients`, `add_audio_effect`, `remove_audio_effect`, `set_audio_pan`, `normalize_loudness`, `auto_duck`, `audio_speech_intervals` |
| `output` (40) | `export_audio`, `export_gif`, `render_frame`, `export`, `join_export`, `export_alpha`, `export_aces2_reference_video`, `render_aces2_reference`, `render_float_tiff`, `render_float_exr`, `render_high_bit_still`, `export_high_bit`, `export_grade_lut`, `export_camera_solve`, `export_asc_cdl`, `export_color_pipeline_lut`, `contact_sheet`, `describe_frame`, `detect_cuts`, `diff_frames`, `burn_in`, `export_job_start`, `job_status`, `cancel_job`, `resume_job`, `export_manifest`, `explain_invalidation`, `inspect_export_preset`, `export_preset`, `lint_frame`, `match_events`, `render_ocio_display_view`, `preview_range`, `query_safe_margins`, `session_stats`, `solve_constraints`, `export_with_subtitles`, `compute_text_bounds`, `set_ocio_view_delivery`, `export_ocio_view_video` |

## Layout

- `crates/` - `kimo-core` (model, keyframes, command registry, tool groups, CPU renderer, effects, text, audio, tracking, color) and `kimo-mcp` (the MCP stdio server)
- `examples/` - Python client and demos
- `docs/` - design notes and reviews
- `tests/` - the tool fuzz runner

Status: early, CPU renderer. See [ROADMAP.md](ROADMAP.md) and [FEATURES.md](FEATURES.md).

## Contributing and the README rule

The README is updated with every product change. If a change adds, removes or regroups tools, or ships a new proof, the same commit updates this file.

MIT licensed.

### Speech and pause perception

`audio_speech_intervals` gives deterministic speech-like regions and safe pause cut advice from clips, tracks, the mix or files. It is an energy/ZCR/spectral VAD, not neural speech recognition. Default padding is 120 ms; suggestions never edit. Aggressiveness 2-3 rejects steady tones; 0-1 can mistake tone bursts for speech. Silence mode matches audio_probe 100 ms RMS windows. Waveform images are optional. Real-speech precision/recall has not yet been measured.

Guided workflow draft: [A reference film, one step at a time](docs/WORKFLOWS.md). Captured current calls are separated from stages waiting for new APIs.

Frame lint: `lint_frame` is read-only advisory checks, report only, not a design certification: `out_of_bounds`, `text_overflow`, `safe_zone`, `min_text_size`, `text_contrast`, `static_frame`, `invisible_layer`, `low_res_upscale`. Tags `lint:allow_bleed` and `lint:ignore=<code>` silence a check per clip and are counted in the summary; thresholds are echoed in `params_used`; unknown check names return `invalid_check` with the allowed list. `text_contrast` takes the text's source colour and layer opacity as the foreground and samples the rendered background with that text hidden, so gradient text, effects and strokes are not certified. See docs/FRAME_LINT.md.
