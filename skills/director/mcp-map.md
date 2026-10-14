# Contract to Kimo primitives

Source-grounded against `main` 425a2e87c8b8add4b5a118d77c60ca5cf2f7a052:
`crates/kimo-core/src/commands.rs`, `toolgroups.rs`, `crates/kimo-mcp/src/main.rs`.
This branch does not add a Rust command. Check the connected server's tool descriptions before executing, because another branch may expose different schemas. Do not guess tool names or provider features.

| Plan intent | Existing primitive | Important boundary |
|---|---|---|
| Canvas | project_set {width,height,fps,name} | Canvas, not color certification |
| Completed plate | import_media {path} | Probe duration before use; no generation |
| Layer | add_track {} | Resolve returned track_id, don't assume IDs |
| Edit interval | add_clip {track_id,source_type:"media",asset_id,start,duration,in_point,fit} | Absolute timeline start, media source offset |
| Song master | import_media then add_audio_clip {asset_id,start,duration} | One master, not repeated per scene |
| Transform | set_property {clip_id,property,value} | Digital transform is not physical camera motion |
| Animation | set_keyframe {clip_id,property,time,value,ease} | time is **clip-local**, not project time |
| Look | list_effects then add_effect | Use actual effect parameters; no hidden grade preset |
| Inspect | render_frame {time,path} | Look at pixels; sampling alone misses temporal failures |
| Delivery | export {path,crf,start,end} | Verify actual video/audio duration after export |

MCP discovery in this snapshot is `kimo_search` and `kimo_describe`, then grouped dispatch. Inspect server-returned schemas for wrapper argument shape. The emitted batch names raw command-registry operations and requires the client to choose the advertised wrapper. It is not a JSON-RPC request, does not resolve IDs, and must not be sent blindly as a single MCP tool.

The Python `--batch` path emits only project/import/track/clip/audio operations with named symbolic return bindings. It accepts a completed scene output path, not imagined source footage. Paths are resolved against `--media-root` (default current directory). Check filesystem existence, duration, identity and permission outside the planner. A consumer binds `$asset_0` and `$video_track` from actual return values; these strings are NOT engine IDs. The compiler never silently translates a camera move into flat scaling and never spends money.

Later branches may have audio_probe, contact_sheet, lint_frame or beat analysis. They are optional discovery targets, not a baseline claim in this snapshot. No native song transcription or generation provider is added by this work.
