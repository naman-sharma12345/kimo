# Contract to Kimo primitives

Source-grounded against `main` 273cf0b88c1d6d3d5bceb9af16ca7a1a5ef02384:
`crates/kimo-core/src/commands.rs`, `commands/tools_*.rs`, `ext/audio_probe.rs`, `ext/contact_sheet.rs`, `ext/lint_frame.rs`, `toolgroups.rs`, `crates/kimo-mcp/src/main.rs`.
The director layer does not add a Rust command. Check the connected server's tool descriptions before executing, because another branch may expose different schemas. Do not guess tool names or provider features.

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

The current source includes `audio_probe`, `contact_sheet`, `lint_frame`, `detect_beats`, `set_camera` and `set_camera_keyframe`. This work read their schemas, but did not build or run them. Confirm the connected binary has the same surface before use.

- `audio_probe {target:{mix:true},start,end}` reports mix facts; clip targets are clip-relative, not absolute project times. It does not transcribe lyrics.
- `contact_sheet {path,times:[...],at_cuts:true}` samples timeline frames, capped at 16 tiles. Inspect omitted cuts separately; one grid does not certify a whole film.
- `lint_frame {times:[...],checks:[...]}` is read-only advice, not a design or identity certification.
- `set_camera {cam_x,cam_y,cam_z,cam_pan,cam_tilt,cam_roll,cam_fov}` controls virtual camera geometry for eligible 3D layers, not a new view behind a flat plate.
- `set_camera_keyframe {property,time,value,ease}` uses **absolute timeline time**, unlike clip-local `set_keyframe`. Do not convert mm/meters to pixels/FOV without a grounded stage convention.

No native song transcription or generation provider is added by this work.
