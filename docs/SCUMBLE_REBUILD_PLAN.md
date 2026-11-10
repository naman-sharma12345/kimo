# SCUMBLE rebuild plan (frame-level)

Source: per-frame analysis of the reference (1551 frames, 25 fps, 62.04 s) by `tools/ref_analyze.py`.
Data: `docs/reference/refmap.csv` (every frame: diff vs previous, global shift in px, mean RGB, luma, saturation, edge density, bright/dark fractions, cut flag), `refmap.json` (72 cuts, 73 shots), `scumble_shot_strip.jpg` (first frame of every shot).

## What the numbers say
- 73 shots. Pace: 0-12 s slow (4 shots, 3 s average), 12-23 s medium (30 shots, 0.3-1.8 s), 23-31 s one 7.7 s node-graph/tunnel shot, 31-38 s one 7.3 s calm prompt/photo shot, 38-54 s rapid fire (32 shots of 0.1-0.7 s, 50-54 s is a 3-6 frame strobe sequence), 54-62 s end card (mascot, text build).
- Beat grid: cuts and shakes sit on a 0.48 s (125 BPM) grid up to 8 s, then on 0.12 s (quarter beat) multiples in the fast sections (cut frames 1255, 1258, 1261, 1264 ... all 3 frames apart).
- Camera shift (phase correlation) spikes mark shake, zooms and whip cuts: frames 11-73 (344 px), 391-435 (536 px, speed zoom), 583-775 (444 px, tunnel), 1159 (576 px), 1213-1255 (460 px).
- Luma by section: dark 0-8 s (0.1-0.2), bright photo 8.9-11 (0.5), mid grey 11-23 (0.3-0.5), dark UI/tunnel 23-31 (0.13-0.15), bright 31-38 (0.48), high-key flashes 38-54 (0.4-0.69), dark end card (0.22).

## Scene list (timing from the cut map) with tool status
EXISTS = in kimo now and usable. PART = engine has it, shot needs tweaks. MISSING = new tool.

| Time (s) | Shot | Tools needed | Status |
|---|---|---|---|
| 0-0.44 | keyboard macro, orange key | media plate, grain | EXISTS (plate = ref crop) |
| 0.44-2.9 | glass shatter on kebab, boxes in debris | add_shard_burst, add_detection_box, glass refraction | PART |
| 2.9-3.7 | cards on dark table | add_scene3d | PART |
| 3.7-8.0 | HUD, boxes land, beat shake | add_hud_frame, add_detection_box impact, add_camera_shake | PART (datamosh tear + scanline sweep MISSING) |
| 8.0-8.9 | box expansion + flash | add_box_expand, precomp | PART |
| 8.9-9.7 | photo with boxes, threshold-ink flash frame | threshold effect, media plate | EXISTS |
| 9.7-10.6 | DRAW ME A BOX | text animator, outline text, stroke, ghost-scale echo | PART (3D extruded heavy text MISSING) |
| 10.6-11.8 | FILL UP A SPOT (box fills with photo, text pop) | scan-wipe fill, add_box_expand, text pop | PART |
| 11.8-12.5 | yellow silhouette flashes (3 shots, 6 frames each) | silhouette_fill effect, add_glitch_confetti | DONE (see examples/scumble_silhouettes.py) |
| 12.5-13.7 | silhouette on speed lines, tiled glitch | speed_lines generator, rgb_split, glitch_blocks | MISSING (speed_lines) |
| 13.7-15.6 | NO MONTHLY BILL / NO LOCK-IN SHOP stamp, flamingos | rubber-stamp text, rotation jitter, colored cutout | MISSING (stamp text) |
| 15.6-17.4 | orange silhouette bursting with rays | speed_lines, silhouette fill | MISSING |
| 17.4-19.0 | mosaic tiles of cutouts, ghost SCUMBLE title | tile_mosaic generator, text animator | MISSING (tile_mosaic) |
| 19.0-20.4 | SCUMBLE SCUMBLE NEVER STOP, MASK IT cracked glass | stacked text with outline/gradient, crack overlay | PART (crack overlay MISSING) |
| 20.4-23.1 | MOVE IT / TAKE IT, cursor | text pop, cursor sprite with click | MISSING (cursor) |
| 23.1-31.0 | particle tunnel into node-graph UI, 33 NODES 57 LINKS counters | add_emitter, zoom, add_counter, node_graph renderer | MISSING (node_graph, tunnel preset) |
| 31.0-38.3 | typed prompt box, ONE LINE., 064 to 105 TOOLS | add_prompt_box, add_counter, look grade | EXISTS |
| 38.3-43.5 | ice skater: rgb-split echo trail, MASK IT/MOVE IT/TAKE IT/EVERY EDIT'S A LAYER | add_text, set_echo (add mode), rgb_split, silhouette fill | PART |
| 43.5-45.3 | photo to B&W, MY OWN / THE CODE IS / OPEN SOURCE | grade, text | EXISTS |
| 45.3-48.5 | layers panel UI, speed-line SCUMBLE tags | ui_panel renderer, speed_lines | MISSING (ui_panel) |
| 48.5-50.2 | dot silhouette, mosaic, glitch | halftone, tile_mosaic, pixel_sort | PART |
| 50.2-54.0 | strobe sequence: SCUMBLE in 14 style variants (outline, shatter, pixelate, comic burst, halftone woman, yellow dotted, blue silhouette, chrome glitch) | text style variants, pixelate, shard, halftone, kaleidoscope | PART (comic burst needs speed_lines) |
| 54.0-56.1 | triple-tile ghost photo, split panes | tile_mosaic, mirror | PART |
| 56.1-62.0 | mascot + SCUMBLE card text build, link, detection box on logo | vector mascot, add_title build, add_detection_box | PART (mascot MISSING as asset: trace from ref) |

## Missing tools, minimal build order (each is small and reused by 3+ scenes)
1. `add_speed_lines` (radial manga burst, animated, any colors): scenes 12.5, 15.6, 45.3, 50.2. About 1 h.
2. `add_silhouette_fill` (alpha-key a plate by color/luma, solid or pattern fill, outline, confetti pixels): scenes 11.8, 12.5, 15.6, 38.3, 48.5. About 2 h.
3. `add_tile_mosaic` (grid of tinted tiles with per-tile plates, staggered pop-in): scenes 17.4, 48.5, 54.0. About 1.5 h.
4. `add_stamp_text` (rubber stamp, rotation, ink texture, jitter): scene 13.7. About 1 h.
5. `add_cursor` (arrow sprite, path, click ripple): scenes 20.4, 31.0, 47. About 0.5 h.
6. `add_node_graph` + `add_ui_panel` (procedural node-link graph with animated links, counters; layer list panel): scenes 23.1, 45.3. About 3 h.
7. `add_crack_overlay` (glass crack mask with displacement), `datamosh_tear` effect, `scanline_sweep`: scenes 4, 19.0. About 1.5 h.
8. Extruded/outlined heavy kinetic text (`set_text_style` extrude + stroke + gradient): scenes 9.7, 19.0, 50.2. About 1.5 h.
9. Tunnel preset (emitter + zoom_blur + chroma) for 23.1-31.0. About 1 h.

## Fastest path through the shots (order of building)
1. Tools 1, 2, 8 first (they unlock 9 of 25 scenes), then build scenes 9.7-12.5, 15.6-17.4, 19.0-20.4.
2. Tools 3, 4, 5, then scenes 12.5-15.6, 17.4-19.0, 20.4-23.1.
3. Tool 6 + 9, scene 23.1-31.0 and 45.3-48.5.
4. Tool 7, scenes 3.7-8.9 polish and 19.0 cracks.
5. Strobe sequence 50.2-54 last: it is 24 shots of 3-6 frames, built as one script with a style table.
6. Assemble: one master timeline script per scene, beat grid from `refmap.csv` cut frames, then global grade, grain and export.

## Honest limits
- Photographic plates (woman, kebab, flamingos, skater, traffic, keyboard) cannot be generated by the engine, so they are reference crops, as agreed. Everything on top (HUD, boxes, text, fx, shake, UI, mosaics, transitions) is rendered by kimo.
- Render speed: about 2.6 s/frame for heavy shots at 720p on 2 cores. A 62 s export is 40 min at that rate; cached rasters, sub-rect layers and a wgpu blend path are the speed items still to do.
- The frame map records cuts and global motion; per-shot object motion (individual boxes, text paths) is still measured shot by shot when that shot is built.
