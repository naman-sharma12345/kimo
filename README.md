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
  <img alt="tools" src="https://img.shields.io/badge/MCP%20tools-255-F4F0E8?labelColor=0E0E12">
  <img alt="fuzz" src="https://img.shields.io/badge/tool%20fuzz-9359%20calls%2C%200%20failures-FF5C28?labelColor=0E0E12">
</p>

---

## What it is

**v1.0.0.** 255 tools, fuzzed (9359 calls, 0 failures). `main` is the stable v1; new work lands on feature branches.

kimo is a video editor with no GUI. You talk to it over the [Model Context Protocol](https://modelcontextprotocol.io). Cut, keyframe, grade, track, key, relight, simulate, caption, mix and export are all registered commands, and the MCP server's tool list is generated from that same registry. There is no feature a human can reach that an AI cannot.

The target is the surface of CapCut, After Effects, Premiere and DaVinci Resolve in one engine: editing, motion graphics, VFX, tracking and pro color. It is an early engine with honest tiers. Read [FEATURES.md](FEATURES.md) for what is solid, what is beta and what is not built.

- **MCP first.** Add a tool in the registry and it is in the MCP server, the fuzz test and the schema automatically.
- **CPU, small memory.** The renderer is CPU only and tuned for a 2 GB machine. GPU code is off by default with a CPU fallback.
- **Measured, not claimed.** Every "better than X" line in FEATURES.md has a stated baseline and a measured number, including the cases where we lost.
- **Few entry points.** `tools/list` returns 16 grouped tools (15.7 KB) instead of 255 (176 KB), measured 11.2x smaller (compact JSON, v1.0.0). Every individual tool is still callable by its own name.
- **Fuzzed.** 255 tools, 9359 calls, 0 failures, before anything is published.

## Director planning layer (this branch)

[The agent-side director skill](skills/director/SKILL.md) turns a brief into editable
scene and shot contracts: song phrase budgets, varied coverage, optics/light/material
choices, separate still/motion prompts and selective lip-sync eligibility. It is a
planning aid, not a media generator or a new MCP endpoint.

```bash
python3 skills/director/validate_plan.py skills/director/example.plan.json
python3 -m unittest discover -s skills/director -p 'test_*.py'
python3 skills/director/validate_plan.py plan.json --batch --media-root /absolute/project
```

The last command emits a **review-only** assembly batch using existing registry
primitives. Resolve symbolic IDs from real command results and inspect actual media
before applying it. [Schema](skills/director/contract.schema.json),
[MCP map](skills/director/mcp-map.md), [research credits](skills/director/CREDITS.md).
The included plan is a synthetic test fixture, not a rendered showcase.

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

### Grouped or flat tool list
By default the server lists 16 grouped tools (about 15.7 KB) so small context windows are not flooded; call a group with `action` and `args`, or use `kimo_search` to find a tool by intent and `kimo_describe` to see its arguments. Every one of the 255 tools can still be called by its own name. To list all 255 individually, set `KIMO_TOOLS=flat` in the server's environment:

```
claude mcp add --transport stdio --env KIMO_TOOLS=flat kimo -- /home/you/.local/bin/kimo-mcp
```

```toml
[mcp_servers.kimo.env]
KIMO_TOOLS = "flat"
```

For JSON configs add `"env": { "KIMO_TOOLS": "flat" }` to the server entry (OpenCode: `"environment": { "KIMO_TOOLS": "flat" }`).

### Troubleshooting
- Server shows as failed: run the `initialize` line above by hand; if that works, the path in your config is wrong (use an absolute path).
- Tools that read or write video fail: `ffmpeg` and `ffprobe` are not on the PATH your client launches with.
- A long export times out in your client: raise the client's MCP tool timeout; `KIMO_TIMEOUT_S` sets the server-side limit per tool call.

## Work made with kimo

These are renders from the engine, driven by tool calls. Some are from the film we are building on it. Where something is still rough, it says so.

### Fracture glass

Glass shards with physical motion, refraction, a shutter-correct motion blur and a damped core so the break starts from the point of impact. Shown on a city plate, top row before core damping, bottom row after. The dark hole where the glass breaks can now be filled with `hole_fill` (default off): a defocused, dimmed copy of the pane (depth behind the glass), or with `hole_source=1` the layers below. The fill is a blur, not a rebuilt scene, so it reads as out-of-focus depth only.

![fracture glass](docs/media/fracture-glass.jpg)

Left, default hole. Right, `hole_fill=1` (same frame, 1280x720 city plate, t=1.0 s):

![hole fill](docs/media/fracture-hole-fill.jpg)

### Frosted glass (backdrop effect)

`frosted_glass` turns any layer's alpha into a glass pane: inside it shows the composite below, blurred, tinted and refracted at a bevel, with a rim light. It runs after the layer's own position and scale, so a sliding panel frosts what is really under it. Frame from a 5 s 1280x720 clip (export took 15.0 s on the 2 GB box); the title sits on the pane. `magnify` turns the pane into a magnifier lens. Limits: not for 3D-tilted layers; the refraction is a bevel offset, not a ray-traced lens; frosting is a 3-pass box blur, not a Gaussian.

![frosted glass](docs/media/frosted-glass.jpg)

### Particles that hit things

Emitters take `colliders` (circle, rect or plane, analytic and deterministic) that sparks bounce off with `bounce` and `friction`, plus a `vortex` swirl force. Left to right: no obstacle, a circle and a floor plane, a vortex with a floor (same seed). Colliders can also be another layer's alpha silhouette (`kind: layer`, `clip_id`), so sparks bounce off a title or shape; the silhouette is read at the current frame by default; `follow: true` on the collider uses the silhouette at each step's own time (0.1 s bins), so an animating title plows particles ahead of it. Limits: particles do not collide with each other; thin rects can be tunnelled by very fast particles (60 integration steps per second).

![particle physics](docs/media/particle-physics.jpg)

### Particles from a layer

An emitter with `shape: layer` and `emit_clip` spawns on that clip's alpha silhouette (`emit_edge` for the outline only), so embers can rise off a title's letters; sparks can also bounce off it (`colliders` kind `layer`). Shown: a 5 s 1280x720 title clip, export 18.8 s. With `emit_follow` each particle spawns on the silhouette as it was at its own birth time (0.1 s bins), so a moving or animating title leaves a trail of its past positions; without it the silhouette at the current frame is used. Limit: bins are 0.1 s wide, so a very fast layer steps between bins, and each bin costs one clip render.

![embers from title outline](docs/media/title-embers.jpg)

### Motion blur on particles and on 3D scenes

Both take `shutter_deg` and `blur_samples`, default off. The scene is sampled at several instants across the shutter and averaged in premultiplied space. 3D scene blur re-poses the whole scene per sample, so object animation, parenting and camera moves all blur. Measured on a 1280x720 scene: 0.19 s per frame off, 0.87 s at 8 samples. At 8 samples a very fast move shows stepped copies, so raise `blur_samples`.

![particle motion blur](docs/media/particle-motion-blur.jpg)
![scene3d motion blur](docs/media/scene3d-motion-blur.jpg)

### Tracking and stabilizing

Point, planar and mesh trackers, feature tracking with homography fits, long-gap tracking, and diagnostics that report jitter and residuals. The frames below are a synthetic test plate (top: a label pinned to a moving surface, bottom: the stabilized result), so they show the mechanism, not a real-footage result. A confidence score is not pixel lock, and the validation tools can show a track is unreliable, not that it is true. We say that in every report.

![tracking and stabilizing](docs/media/tracking-stabilize.jpg)

### Why kimo

- **Built for an AI, not retrofitted.** 255 tools, each one a registered command with a schema. Nothing in the editor is reachable only by mouse. `tools/list` is grouped into 16 entries (15.7 KB instead of 176 KB, 11.2x smaller), with `kimo_search` to find a tool by intent and `kimo_describe` for its arguments, so a small context window can still drive all of it.
- **Tested the way a server should be.** A fuzzer calls every tool with generated arguments before anything is published: 255 tools, 9359 calls, 0 failures. Features ship with a proof that can fail, for example frosted glass reduces pane detail from 114444 to 4473 on a test plate, and a moving collider leaves a corridor of 88 pixels behind it where a frozen one leaves 163.
- **Physical effects, not presets.** Fracture glass with refraction, shutter-correct motion blur and a damped impact core. Frosted glass that frosts what is really under a moving pane, with a magnifier lens. Particles that collide with circles, planes and the moving silhouette of a layer, and that are born on that silhouette.
- **Real tracking, honestly reported.** Point, planar, mesh and feature tracking, stabilization and camera solve, with diagnostics that say when a track is unreliable instead of a confidence number that looks good.
- **Pro colour.** ACES and ACEScct, ASC CDL, OCIO, LUTs, HDR, grade node graphs, scopes and film response, in a float pipeline with 16-bit, EXR and TIFF output.
- **Small and fast to start.** A static 10 MB binary that answers its first MCP request in 2 ms, CPU only, tuned for a 2 GB machine. A 5 s 1280x720 shot with a video plate, a sliding title, collider rain and embers exports in about 40 s on that box.
- **Honest about limits.** Every feature has a tier in [FEATURES.md](FEATURES.md), with its baseline, its measured number and the cases where it lost.

## What is inside

| Area | Tools |
|---|---|
| Timeline and edit | project_set, import_media, add_track, add_clip, move/trim/split/delete, ripple_delete, ripple_trim, roll_edit, slip_clip, slide_clip, markers, multicam, sync_clips, undo/redo, save/load |
| Look and motion | set_property, set_keyframe (any property, with easing and bezier), expressions, motion paths, motion presets, speed ramps, freeze frame, flow blur, camera shake |
| Effects and transitions | add_effect, add_transition, blend modes, effect masks, adjustment layers |
| Masks and roto | bezier masks, mask keyframes, roto propagation, track mattes, refine_matte, chroma_key |
| Text and graphics | add_text with animators and paths, captions, titles, HUD frames, counters, detection boxes, shapes and path shapes with modifiers, layer styles |
| 3D and relight | add_scene3d, camera, lights, materials, relight, shard bursts, fracture glass |
| Particles and physics | add_emitter, add_particles, simulate_physics |
| Tracking and vision | track_point, track_planar, track_mesh, track_features, stabilize, solve_camera, undistort_clip, validation tools |
| Color | primaries, wheels, curves, HSL, LUTs, ACES and ACEScct, ASC CDL, OCIO, HDR, grade node graphs, scopes, film response, match_grade |
| Audio | fades, EQ, denoise, compressor, echo, pitch, pan, loudness, auto_duck, beat detection and binding |
| Output | render_frame, export, export_alpha, export_gif, export_audio, export_preset, high bit depth, float EXR and TIFF |

The full, current tool list is whatever `tools/list` returns.

## How an AI sees the tools

The default `tools/list` is grouped, so a client does not have to read 255 tool schemas up front.

| Group | What it covers |
|---|---|
| project, timeline, animate, effects | project and media, edits, keyframes and motion, effects and transitions |
| masks, text, graphics, vfx3d | masks and roto, text and captions, shapes and HUD, 3D, particles and fracture |
| tracking, color_grade, color_science, scopes | tracking and camera solve, grading, color management, measurement |
| audio, output | audio, render and export |
| kimo_search, kimo_describe | find an action by keywords (also searches the effect and transition catalogs), get one action's full arguments |

Call a group with `{"action": "<tool name>", "args": {...}}`, for example `{"name": "vfx3d", "action": "add_scene3d", "args": {...}}`. Calling a tool by its own name (`add_scene3d`) still works, and `tools/list` with `{"mode": "flat"}` (or `KIMO_TOOLS=flat`) lists all 255. A new tool is placed in a group by name rules in `crates/kimo-core/src/toolgroups.rs`, and a unit test fails if one is left unassigned. `tests/group_proof.py` checks that every tool is in exactly one group and that grouped and direct calls give the same result.

## Layout

- `crates/` - `kimo-core` (model, keyframes, command registry, tool groups, CPU renderer, effects, text, audio, tracking, color) and `kimo-mcp` (the MCP stdio server)
- `examples/` - Python client and demos
- `docs/` - design notes and reviews
- `tests/` - the tool fuzz runner

Status: early, CPU renderer. See [ROADMAP.md](ROADMAP.md) and [FEATURES.md](FEATURES.md).

## Contributing and the README rule

The README is updated with every product change. If a change adds, removes or regroups tools, or ships a new proof, the same commit updates this file.

MIT licensed.
