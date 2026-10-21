# Director quickstart: an agent edits phone footage into a 9:16 short

This is the path an agent takes from raw vertical clips to an exported short, written from a full dry run on
2026-10-21: an MCP client (Claude Code) with `kimo-mcp` attached over stdio, the default grouped tool list,
no shell and no file tools. The run went import → best take → speech cuts → b-roll → stabilize → colour →
captions → title → audio levels → export → frame review → one fix → re-export in 9 min 25 s and 87 tool calls.
Every call below is one that worked in that run; the traps it hit are listed at the end with the way round each.

## 1. Wire it

```bash
claude mcp add --transport stdio kimo -- /abs/path/to/kimo-mcp            # Claude Code
# any stdio client: command = the absolute path, no arguments
```

- Leave `KIMO_TOOLS` unset (the grouped list, 17 entries, about 4.7 KB). Call a group as
  `{"action": "<tool>", "args": {...}}`, or call any tool by its own name. `kimo_describe {"group": "audio"}`
  lists a group's tools; `kimo_describe {"action": "stabilize"}` gives one tool's schema. A small model does
  better with this than with the 395-tool flat list.
- `KIMO_SPECTATOR=off` when nothing is watching. With the spectator app open every call shows up live.
- Images come back inline: `render_frame` and `contact_sheet` return a PNG content block the model can look at
  (cap with `max_inline_px`).

## 2. The edit, call by call

Times are timeline seconds unless a tool says clip-local.

| Step | Calls |
|---|---|
| Project | `project_set {width: 720, height: 1280, fps: 30}` then `import_media {path}` per clip (absolute paths). |
| Find the speech | `audio_speech_intervals {target: {path: ".../take2.mp4"}, aggressiveness: 1, min_silence_ms: 300, pad_ms: 120}` returns speech intervals and `cut_suggestions` with frame-aligned `start`/`end`. A second pass with `aggressiveness: 2, min_silence_ms: 100` finds the pauses between phrases (cut points for b-roll and captions). |
| Lay the take | `add_track {}`, `add_clip {track_id, source_type: "media", asset_id, start: 0, duration, fit: "cover"}`. |
| Cut the dead air | `remove_silence_cuts {track_id, ranges: [[s, e], ...]}` with the suggested ranges. The clip is split and rippled: its ids change, so call `find {track: "<track_id>"}` afterwards for the new ones. |
| B-roll | a second `add_track`, then `add_clip` per shot at the pause times, `in_point` for the part of the source you want, `fit: "cover"`. Mute or lower its audio with `set_audio {clip_id, volume: 0.12, fade_in, fade_out}`. |
| Stabilize | `stabilize {clip_id}` (warp engine, smooth motion, auto-scale crop). It keys x/y/scale/rotation on the clip at every frame and adds the crop mask, so stabilize after the clip's in-point and duration are final. |
| Colour | measure with `get_scopes {time}` (avg_rgb, luma). Match a shot to a good one with `match_grade {clip_id, ref_clip_id, time, ref_time, strength: 0.9}`, or fix a cast with `set_color_wheels {clip_id, gain: [r, g, b, 1], saturation}`. Re-measure with `get_scopes`. |
| Captions | `add_caption_cues {track_id, style: "boxed", cues: [{start, end, text}, ...]}`: the cues inline, no file needed, timed to the speech intervals. With a whisper CLI installed, `add_captions {source: "transcribe", path, out_srt}` transcribes instead. On 9:16 the presets wrap at 90% of the frame width. |
| Text | `add_text {track_id, text, start, duration, size, font}` then `set_property {clip_id, property: "y", value}` (pixels from the canvas centre, + is down). Background boxes: `set_text_style {clip_id, bg: [0,0,0,0.6], bg_pad: 16, bg_radius: 12, box_width: 0}` (`box_width: 0` makes the box hug the text). |
| Title | `add_title {track_id, style: "impact", text, start: 0, duration: 1.5}`. On 9:16 a word too wide for the frame is scaled to fit, and on a short title the reveal finishes by mid-duration. |
| Audio | dialogue: `set_audio {clip_id, highpass: 80}`, `add_audio_effect {clip_id, kind: "compressor", params: {threshold_db: -24, ratio: 4, attack_ms: 5, release_ms: 120}}`, then `normalize_loudness {clip_id, target_lufs: -14}`. If it refuses, the message names the loudest target that fits. Check the mix with `measure_loudness {}`. |
| Export | `export {path: ".../short_v1.mp4", crf: 20}`. 11.5 s at 720x1280 took 33 s (about 0.1 s per frame) and came out at 1.9 MB. |
| Review | `contact_sheet {path, times: [...], tile_px: 240, cols: 4}` and `render_frame {time, path}` on the timeline. To look at the exported file itself: `import_media` it, `add_clip` on a scratch track, `contact_sheet {isolate: [clip_id], ...}`, then `delete_track`. |
| Fix and re-export | change one thing (in the dry run: caption boxes over a busy shot), check with `contact_sheet`, `export` v2. |
| Save | `save_project {path}`. |

`batch {ops: [...], label, return: "minimal"}` runs many calls atomically with one undo step; `foreach` repeats an
op over a list (`{"foreach": {"var": "k", "in": ["clip30", "clip31"]}, "ops": [{"tool": "set_text_style", "args": {"clip_id": "{{k}}", ...}}]}`).

## 3. Traps and the way round

- **`get_project` is the whole project.** After one stabilize it was 57,000 characters, over a client's tool
  result limit. Use `find {track}` / `find {source_kind: "text"}`, `list_captions`, `list_effects`,
  `get_transform {clip_id}` and `get_scopes` for the parts you need.
- **`add_text` has no `y`.** Set it with `set_property` after; the warning on a stray `y` says so.
- **Keyframes** (`set_keyframes`): each key is an object `{t, v, ease?}` with `t` clip-local seconds; `ease` is one of
  `linear, in, out, in_out, hold, out_quart, out_expo, out_back, in_out_quint, ...` (not `ease_out`).
- **Audio effect params** are named per kind: compressor `threshold_db, ratio, attack_ms, release_ms, makeup_db`;
  eq `frequency, width, gain_db`; reverb `decay, delay_ms`; denoise `reduction_db, floor_db`. A wrong name lists them.
- **`export` returns no file size.** `list_dependencies` after importing the file shows its size, or check the
  bitrate budget: 16 MB over 60 s is about 2 Mb/s.
- **Speech-to-text needs the `whisper` CLI** (`pip install openai-whisper`). Without it, write the cues yourself
  from the script and time them to `audio_speech_intervals`.
- **Durations**: any decimal is fine (1.266667 s and 1.3334 s are 38 and 40 frames at 30 fps).
- **Exact argument names.** `start` (not `at`), `fps` (not `frame_rate`), `time` for a render. A wrong name is ignored with a
  warning, and when the call then fails the error names the ignored argument and the right one. A small model guessed all three.
- **`set_color_wheels` gain** is a multiplier per channel plus master, `[r, g, b, master]`, 1 = unchanged. `[0.15, 0.15, 0.05, 0.15]`
  is not "a little warmer", it is nearly black (a small model did exactly this and then cut the shot as "too dark").
  Re-measure with `get_scopes` after every grade.
- **Captions are a track of their own.** Put `add_caption_cues` on a new track above the picture, not on the talking-head track.
- **`session_stats`** counts calls made inside other tools (`remove_silence_cuts` calls `lift_range`).

## 4. Checklist before export

Run through this before every export. Each item is a mistake a dry-run agent made (most of them a small model).

1. **B-roll sits inside speech.** Every b-roll clip starts and ends inside an interval from `audio_speech_intervals`, on a
   track above the talking head, and the dialogue keeps playing under it. B-roll after the last word plays over silence.
2. **No grade crushed a shot.** For every graded clip, `get_scopes` average luma is near the talking head's (about 0.4) and
   never under 0.15. If a shot looks too dark after grading, undo the grade; do not delete the shot.
3. **Captions are on their own track,** above the picture, and `lint_frame` / `compute_text_bounds` puts them inside the
   platform safe zone (`query_safe_margins {profile: "tiktok"}`).
4. **The title is visible in its first half** and drawn in the preset's font: if `add_title` returned a `font_substituted`
   warning, install the fonts (below) or pass `font` explicitly.
5. **Loudness is measured, not guessed.** `measure_loudness` on the mix reads about -14 LUFS; if `normalize_loudness`
   refuses, use the target it names or the compressor call it gives.
6. **Frames were looked at.** `contact_sheet` across the cut points, after the last change.

**Fonts.** The title presets use OFL Google Fonts (Anton, Bebas Neue, Montserrat, Cormorant Garamond, Playfair Display,
Syne). The repo does not ship the .ttf files: run `bash fonts/fetch.sh` once. kimo finds `fonts/` when the build target is
inside the repo; with the target elsewhere, set `KIMO_FONTS=<repo>/fonts` in the MCP server's env.

## 5. Measured

The dry run's own clock (one 4-CPU box, Claude Code Opus as the director, 720x1280 30 fps, 5 clips, 11.5 s short):
9 min 25 s wall for the whole edit including the model's thinking, 87 tool calls, 9 errors (all recovered),
export of the 11.5 s short 33 s each time (the slowest call), stabilize 5 to 8 s on a 6 s clip (under 2 s on a 1.3 s one), contact sheets 3-4 s. Of the 9 min 25 s, about 2 min was spent inside tools; the rest was the model reading and deciding.
