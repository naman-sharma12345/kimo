# kimo

MCP-first, AI-first video editor in Rust. Every operation (cut, transform, keyframe, effect, transition, grade, track, mix, export) is a registered command. The MCP server's tool list is generated from that registry, so a feature cannot exist without being AI-callable. The AI can also render a frame to PNG and read video scopes, so it can look at its own work and iterate.

## Quick start
```
cargo build --release
python3 examples/demo_grade.py        # graded 2-clip sequence with transition, built only via MCP calls
python3 examples/effects_gallery.py   # renders every effect
```
Needs `ffmpeg` and `ffprobe` on PATH (decode/encode). 

## Use from an MCP client
```json
{ "mcpServers": { "kimo": { "command": "/path/to/kimo/target/release/kimo-mcp" } } }
```
Call `tools/list` to see everything. Start with `project_set`, `import_media`, `add_track`, `add_clip`, `add_effect`, `render_frame`, `export`.

## Tools (generated from the registry)
Timeline: project_set, import_media, add_track, add_clip, move/trim/split/delete_clip, set_speed, set_speed_ramp, undo/redo, save/load_project, get_project.
Look and motion: set_property, set_keyframe (any property or effect param, with easing), set_blend, set_mask + set_mask_keyframe, add_effect (list_effects), add_transition (list_transitions).
Images/stickers: import_media works with PNG/JPG (alpha kept); animate with transform keyframes.
Particles: add_particles (embers, sparks, snow, rain, dust). Export: export, export_gif, export_audio. Speech: transcribe (needs whisper CLI).
Text: add_text (typewriter, stroke, shadow), set_text, import_srt (captions).
Color: exposure, contrast, saturation, temperature, tint, color_wheels, curves, teal_orange, hsl, lut (.cube), color_isolate, get_scopes.
Vision: track_point (pin text to a moving surface), stabilize, chroma_key.
Audio: set_audio (fades, EQ, denoise, compressor, echo, pitch, reverse), set_audio_volume_key, detect_beats, export_audio.
VFX: add_adjustment_layer, add_shape (rect/ellipse/polygon/line), set_corner_pin, set_time_mode + freeze_frame, effects luma_key, ripple, lens_distortion, glitch_blocks, light_rays, drop_shadow, chromatic_aberration, flicker, halftone, edge_glow.
Output: render_frame, export.

## Layout
- `crates/kimo-core`: model, keyframes, command registry, CPU renderer, effects, text, audio, tracking
- `crates/kimo-mcp`: MCP stdio server
- `examples/`: Python client and demos

Status: early, CPU renderer. See ROADMAP.md.
