<p align="center">
  <img src="docs/assets/kimo-logo.png" alt="kimo" width="160">
</p>

<h1 align="center">kimo</h1>

<p align="center"><b>A video editor an AI can drive.</b><br>
MCP-first. Every feature is a tool call. Written in Rust, CPU renderer, runs on a 2 GB box.<br>
<a href="https://getkimo.tech"><b>getkimo.tech</b></a></p>

<p align="center">
  <img alt="license" src="https://img.shields.io/badge/license-MIT-F4F0E8?labelColor=0E0E12">
  <img alt="language" src="https://img.shields.io/badge/rust-core-FF5C28?labelColor=0E0E12">
  <img alt="tools" src="https://img.shields.io/badge/MCP%20tools-327-F4F0E8?labelColor=0E0E12">
  <img alt="fuzz" src="https://img.shields.io/badge/tool%20fuzz-6497%20calls%2C%200%20failures-FF5C28?labelColor=0E0E12">
</p>

---

## What it is

kimo is a video editor with no GUI. You talk to it over the [Model Context Protocol](https://modelcontextprotocol.io). Cut, keyframe, grade, track, key, relight, simulate, caption, mix and export are all registered commands, and the MCP server's tool list is generated from that same registry. There is no feature a human can reach that an AI cannot.

The target is the surface of CapCut, After Effects, Premiere and DaVinci Resolve in one engine: editing, motion graphics, VFX, tracking and pro color. It is an early engine with honest tiers. [FEATURES.md](FEATURES.md) lists what is solid, what is beta and what is not built.

## Capabilities

327 registered tools in 14 groups (full list [below](#tool-listing-327-tools)):

| Area | Groups | Examples |
|---|---|---|
| Editing | `project`, `timeline` | tracks, clips, trims, ripple and roll edits, speed ramps, multicam, markers, atomic `batch` with undo |
| Motion | `animate`, `effects` | keyframes and Bezier easing, expressions, camera and shake, corner pin, transitions, adjustment layers |
| Compositing | `masks`, `graphics` | mask layers and roto, shapes, HUD frames, counters, boxes |
| Text | `text` | titles, captions, SRT import, text animators |
| VFX | `vfx3d` | particles, emitters, shard bursts, 3D scene, physics, lights and materials |
| Tracking | `tracking` | point, planar and mesh tracking, stabilize, camera solve |
| Color | `color_grade`, `color_science`, `scopes` | grade graphs, curves, ASC CDL, LUTs, ACES and OCIO tools, scopes and exposure analysis |
| Audio | `audio` | clips, effects, beats, loudness, ducking, speech intervals |
| Output | `output` | frame and video export, chunked export jobs, GIF, EXR, manifests, frame lint, contact sheets |

## Architecture

- `crates/kimo-core` holds the project model, keyframes, the command registry, tool groups, the CPU renderer, effects, text, audio, tracking and color.
- `crates/kimo-mcp` is the MCP stdio server. Its tool list is generated from the registry.
- `kimo` is a command line client for the same tools. The fuzz runner and the schema come from the same registry too.
- Rendering and decoding: the renderer is CPU only. GPU code is off by default with a CPU fallback. Decode, encode and probing go through `ffmpeg` and `ffprobe`.

Add a tool to the registry and it appears in the MCP server, the CLI, the fuzz test and the schema. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/EXTENDING.md](docs/EXTENDING.md).

## Principles

- **MCP first.** Every tool is callable by name. The default `tools/list` is grouped so a client does not read every schema up front.
- **CPU, small memory.** Tuned for a 2 GB machine. A cold release build of the whole workspace took 17 minutes on a shared 2 CPU / 2 GB machine (`CARGO_BUILD_JOBS=1`).
- **Measured, not claimed.** Every "better than X" line in FEATURES.md has a stated baseline and a measured number, including the cases where we lost.
- **Fuzzed.** The v3 notes record 6497 fuzz calls with 0 failures at 327 tools. These numbers come from the project notes. They were not re-run for this README.

## Install

**Status of prebuilt downloads.** The v3.0.0 Linux (`.deb` and portable `tar.gz`) and Windows x64 assets are listed on the [Releases page](https://github.com/naman-sharma12345/kimo/releases) once they exist there. Windows x64 is built by a GitHub Actions runner and is not verified until the runner output exists. Until you see assets attached to a release, build from source. Older v1.0.0 downloads are a different, 255-tool build.

### Build from source (Linux x86_64 is the tested platform)

Everything in this section was run against a build of the v3 branch.

**Prerequisites**

- Rust stable (install with [rustup](https://rustup.rs)) and a C linker (`build-essential` on Debian/Ubuntu).
- `ffmpeg` and `ffprobe` on PATH (`sudo apt install ffmpeg`; checked with ffmpeg 4.4.2). Decode, encode and probing go through them.
- About 2 GB of RAM is enough to run. The release build is the heavy part: use `CARGO_BUILD_JOBS=1` on a small box (measured 17 minutes for a cold release build of the whole workspace on a shared 2 CPU / 2 GB machine).
- Linux x86_64 is the tested platform. Other platforms: see Verification status below.

**Build**

```
git clone https://github.com/naman-sharma12345/kimo.git
cd kimo
CARGO_BUILD_JOBS=1 cargo build --release --workspace --bins
ls target/release/kimo target/release/kimo-mcp
target/release/kimo --version          # prints version the version in Cargo.toml and the git commit
```

`kimo-mcp` is the MCP server (stdio). `kimo` is the command line client for the same tools.

**Check the MCP server answers**

```
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | target/release/kimo-mcp
```

The reply contains `"serverInfo":{"commit":"...","name":"kimo",...,"version":"the version in Cargo.toml"}`.

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


## Setup: add kimo to your AI client

kimo is a local MCP server that speaks over stdin/stdout (the "stdio" transport). There is nothing to start by hand: your client launches `kimo-mcp` itself. You only need the path to the binary.

**1. Get the binary** (build from source, see Install above) and put it somewhere stable, for example:

```
mkdir -p ~/.local/bin && cp /path/to/kimo/target/release/kimo-mcp ~/.local/bin/
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


### Tool list profiles
`KIMO_TOOLS` picks what `tools/list` returns. Every tool can be called by its own name under every profile.

| Profile | `tools/list` entries | Size | Use when |
|---|---|---|---|
| `grouped` (default) | 17 | 4,467 bytes | small context windows. Call a group with `action` and `args`; find tools with `search_tools`. |
| `core` | 29 | 20,369 bytes | the most used tools with full schemas, plus `search_tools`, `kimo_describe`, `kimo_enable_group`. |
| `flat` | 329 | 254,503 bytes | clients with large context or their own tool search. Every tool listed with its schema. |
| group names, e.g. `audio,color_grade` | varies | varies | those groups listed flat, the rest grouped. |

Entries and sizes were measured on the v3.0.0 static Linux release build (`initialize` then `tools/list`, compact JSON, no client). The 329 in `flat` is the 327 registry tools plus two discovery tools, `search_tools` and `kimo_describe`. `kimo_enable_group` is listed in `grouped` and `core` but not in `flat`. The 327 count in the badge and the tool listing below is the registry (`golden_registry.txt`, 327 lines). `search_tools(query, limit=8, group?)` returns ranked tools, effects and transitions with the required args and a `call_as` hint.

Set it like this: `claude mcp add --transport stdio --env KIMO_TOOLS=core kimo -- /home/you/.local/bin/kimo-mcp`, or `[mcp_servers.kimo.env] KIMO_TOOLS = "core"` for Codex. For JSON configs add `"env": { "KIMO_TOOLS": "core" }` (OpenCode: `"environment": { ... }`).

## How an AI sees the tools

Call a group with `{"action": "<tool name>", "args": {...}}`, for example `{"name": "vfx3d", "action": "add_scene3d", "args": {...}}`. Calling a tool by its own name also works.

## Tool listing (327 tools)

This is the list the v3 build registers (read from `tools/list` of the grouped server plus `kimo_describe group=<name>`, 327 tools in 14 groups; the grouped `tools/list` itself shows 14 group tools plus `search_tools`, `kimo_describe`, `kimo_enable_group`). `kimo --version` and `kimo_info` report the workspace version, which is 3.0.0 from this release. Get any tool's arguments with `kimo tools describe <name>` or `kimo_describe`, and find tools by words with `search_tools`.

| Group | Tools |
|---|---|
| `project` (35) | `project_set`, `import_media`, `get_project`, `save_project`, `load_project`, `kimo_info`, `cache_stats`, `cache_clear`, `cache_set_budget`, `undo`, `redo`, `history_list`, `checkpoint`, `restore`, `checkpoint_delete`, `batch`, `list_dependencies`, `collect_project`, `relink_media`, `ext_list`, `set_meta`, `find`, `read_pixels`, `import_pixels`, `project_diff`, `recipe_list`, `recipe_describe`, `recipe_apply`, `set_limits`, `get_limits`, `template_define`, `template_apply`, `template_list`, `template_describe`, `template_delete` |
| `timeline` (37) | `add_track`, `add_clip`, `set_speed_ramp`, `group_clips`, `ungroup_clip`, `set_time_mode`, `freeze_frame`, `move_clip`, `trim_clip`, `split_clip`, `delete_clip`, `set_speed`, `ripple_delete`, `ripple_trim`, `roll_edit`, `slip_clip`, `slide_clip`, `insert_gap`, `lift_range`, `duplicate_clip`, `loop_clip`, `distribute_clips`, `swap_clips`, `rate_stretch`, `add_marker`, `list_markers`, `remove_marker`, `snap_time`, `list_edit_points`, `set_track`, `delete_track`, `split_all_at`, `remove_silence_cuts`, `sync_clips`, `add_multicam`, `switch_angle`, `list_multicam` |
| `animate` (32) | `set_expression`, `set_keyframe_bezier`, `add_camera_shake`, `set_motion_blur`, `set_camera`, `set_camera_keyframe`, `set_corner_pin`, `set_property`, `set_keyframe`, `set_blend`, `set_motion_path`, `remove_motion_path`, `set_flow_blur`, `render_flow`, `get_curve`, `move_key`, `remove_key`, `copy_curve`, `simplify_curve`, `smooth_curve`, `track_beats`, `bind_beats`, `set_mesh_pin`, `apply_motion_preset`, `motion_extract`, `motion_diff`, `set_keyframes`, `sample_property`, `bake_property`, `add_null`, `set_parent`, `get_transform` |
| `effects` (15) | `list_transitions`, `add_adjustment_layer`, `add_speed_lines`, `add_glitch_confetti`, `set_echo`, `set_posterize_time`, `list_effects`, `add_effect`, `set_effect_param`, `remove_effect`, `add_transition`, `set_effect_mask`, `add_managed_linear_effect`, `set_effect_encoding`, `effect_describe` |
| `masks` (16) | `set_mask`, `set_mask_keyframe`, `remove_mask`, `add_mask`, `update_mask`, `remove_mask_layer`, `reorder_mask`, `list_masks`, `set_mask_prop_key`, `set_mask_path_key`, `roto_propagate`, `set_track_matte`, `remove_track_matte`, `render_mask`, `track_matte`, `refine_matte` |
| `text` (23) | `add_text`, `set_text`, `import_srt`, `transcribe`, `set_text_style`, `add_text_animator`, `set_animator_keyframe`, `add_title`, `list_fonts`, `set_text_weight`, `set_text_path`, `add_captions`, `list_captions`, `set_caption_style`, `edit_caption`, `transcript_import`, `transcript_get`, `transcript_edit_word`, `transcript_set_take`, `transcript_coverage`, `transcript_to_captions`, `validate_captions`, `transcript_edit_preview` |
| `graphics` (20) | `add_box_expand`, `add_shape`, `add_detection_box`, `add_hud_frame`, `add_counter`, `add_prompt_box`, `add_modifier`, `list_modifiers`, `remove_modifier`, `list_shape_modifiers`, `add_shape_modifier`, `remove_shape_modifier`, `list_layer_styles`, `set_layer_style`, `remove_layer_style`, `add_path_shape`, `set_path`, `path_info`, `add_path_key`, `clear_path_keys` |
| `vfx3d` (15) | `add_particles`, `add_emitter`, `add_shard_burst`, `add_scene3d`, `simulate_physics`, `shards_project`, `set_light`, `light_rig`, `remove_light`, `list_lights`, `set_material`, `list_materials`, `remove_material`, `set_relight`, `preview_material` |
| `tracking` (25) | `track_long_gap`, `track_point`, `stabilize`, `track_planar`, `pin_layer`, `track_mesh`, `solve_camera`, `list_camera_solves`, `camera_solve_info`, `apply_camera_solve`, `grade_node_tracking`, `track_planar_anchored`, `track_features`, `track_set_info`, `fit_track_homography`, `fit_info`, `decompose_track_homography`, `track_jitter_metrics`, `corner_disagreement`, `track_feature_residuals`, `lk_residual_audit`, `ecc_refine_track`, `render_error_field`, `validate_planar_track`, `undistort_clip` |
| `color_grade` (22) | `preview_subject_match`, `match_grade`, `set_color_curve`, `pick_white_balance`, `match_color`, `inspect_asc_cdl`, `apply_asc_cdl`, `generate_lut_node`, `evaluate_grade_graph_samples`, `inspect_grade_graph`, `set_grade_graph`, `add_node`, `connect_nodes`, `disconnect_nodes`, `remove_grade_node`, `set_grade_output`, `set_grade_node`, `set_node_param`, `set_node_mask`, `create_shared_grade_graph`, `bind_shared_grade_graphs`, `remove_shared_grade_graph` |
| `color_science` (18) | `inspect_color_frame_metadata`, `inspect_color_delivery`, `inspect_color_pipeline`, `compare_aces2_reference`, `sample_linear_neutral`, `audit_color_output`, `inspect_float_media`, `set_hdr_mastering`, `set_color_pipeline`, `set_clip_input_color`, `validate_hdr_transfer_math`, `evaluate_hlg_reference_display`, `add_acescct_primaries`, `inspect_high_bit_delivery_contract`, `inspect_film_response`, `inspect_ocio_config`, `transform_ocio_samples`, `transform_ocio_view_samples` |
| `scopes` (12) | `get_scopes`, `compare_linear_exposure`, `render_gamut_excursions`, `analyze_exposure_sequence`, `render_exposure_false_color`, `analyze_exposure`, `analyze_color`, `render_scope`, `render_display_scope`, `measure_loudness`, `inspect_grade_scopes`, `set_grade_scopes` |
| `audio` (17) | `add_audio_clip`, `set_audio`, `set_audio_volume_key`, `detect_beats`, `set_audio_effect_key`, `audio_filter_response`, `audio_effect_describe`, `audio_probe`, `read_samples`, `import_samples`, `audio_transients`, `add_audio_effect`, `remove_audio_effect`, `set_audio_pan`, `normalize_loudness`, `auto_duck`, `audio_speech_intervals` |
| `output` (40) | `export_audio`, `export_gif`, `render_frame`, `export`, `join_export`, `export_alpha`, `export_aces2_reference_video`, `render_aces2_reference`, `render_float_tiff`, `render_float_exr`, `render_high_bit_still`, `export_high_bit`, `export_grade_lut`, `export_camera_solve`, `export_asc_cdl`, `export_color_pipeline_lut`, `contact_sheet`, `describe_frame`, `detect_cuts`, `diff_frames`, `burn_in`, `export_job_start`, `job_status`, `cancel_job`, `resume_job`, `export_manifest`, `explain_invalidation`, `inspect_export_preset`, `export_preset`, `lint_frame`, `match_events`, `render_ocio_display_view`, `preview_range`, `query_safe_margins`, `session_stats`, `solve_constraints`, `export_with_subtitles`, `compute_text_bounds`, `set_ocio_view_delivery`, `export_ocio_view_video` |

## Verification status

- Registry: 327 tools. The v3.0.0 static build lists 329 entries in `flat` mode: those 327 plus `search_tools` and `kimo_describe`. The golden registry file `crates/kimo-core/tests/golden/golden_registry.txt` has 327 lines.
- Fuzz: 6497 calls, 0 failures, as recorded in the v3 notes. Not re-run for this README.
- Windows x64: unverified until a GitHub Actions runner has produced and smoke-tested the installer and ZIP.
- macOS: no binaries; build from source.
- Per-feature limits and change notes: [docs/DEV_NOTES.md](docs/DEV_NOTES.md). Feature tiers: [FEATURES.md](FEATURES.md). Plans: [ROADMAP.md](ROADMAP.md).

## Contributing and the README rule

The README is updated with every product change. If a change adds, removes or regroups tools, or ships a new proof, the same commit updates this file.

MIT licensed.

## Coherent fields (feature-branch WIP)

This feature branch adds original raw 2D Perlin/simplex fields, analytic gradients,
MCP point sampling and an animatable noise effect. It is not part of the finalized
v3 release. Compilation, numeric tests, warning delta and real 1080p visual
sign-off are pending. No quality or production-readiness claim is made.
See [the noise plan](docs/noise/PLAN.md) and [verification commands](docs/noise/VERIFY.md).

The next WIP field units add explicit Euclidean Worley F1/F2, FBM, domain warp and
2D potential curl. These are independently selected raw operators, not presets.
They are also uncompiled and unrendered here. See [field units](docs/noise/FIELD_UNITS.md).
