//! Delivery-stage extras that sit next to export_preset but aren't presets themselves: a burned-in
//! timecode readout, a watermark overlay, and real container chapter markers written into an already
//! exported file. All three reuse existing primitives (text/shape clips, the timeline's own `markers`
//! list, ffmpeg metadata remux) rather than inventing new Source/effect kinds.
use super::kit::*;
use crate::commands::dispatch_nested as call;
use crate::model::{Source, Clip, TimeMode, Audio, default_props};
use std::process::Command as Cmd;

fn opt_num(a: &Value, k: &str, d: f64) -> f64 { a.get(k).and_then(|v| v.as_f64()).unwrap_or(d) }
fn opt_str<'a>(a: &'a Value, k: &str, d: &'a str) -> &'a str { a.get(k).and_then(|v| v.as_str()).unwrap_or(d) }
fn opt_bool(a: &Value, k: &str, d: bool) -> bool { a.get(k).and_then(|v| v.as_bool()).unwrap_or(d) }
fn rgb(a: &Value, k: &str, d: [f64; 3]) -> [f64; 3] { a.get(k).and_then(|v| v.as_array()).filter(|v| v.len() >= 3).map(|v| [v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0), v[2].as_f64().unwrap_or(0.0)]).unwrap_or(d) }

/// Corner/center anchor shared by burn_in_timecode and add_watermark: a fraction-of-canvas margin from
/// the chosen edge(s), resolved to the clip-centre x/y this engine's `x`/`y` properties expect (0,0 = canvas centre).
fn anchor_xy(position: &str, cw: f64, ch: f64, half_w: f64, half_h: f64, margin: f64) -> Result<(f64, f64), String> {
    let mx = margin * cw; let my = margin * ch;
    Ok(match position {
        "top_left" => (-cw / 2.0 + half_w + mx, -ch / 2.0 + half_h + my),
        "top_right" => (cw / 2.0 - half_w - mx, -ch / 2.0 + half_h + my),
        "bottom_left" => (-cw / 2.0 + half_w + mx, ch / 2.0 - half_h - my),
        "bottom_right" => (cw / 2.0 - half_w - mx, ch / 2.0 - half_h - my),
        "center" => (0.0, 0.0),
        _ => return Err(format!("position must be top_left, top_right, bottom_left, bottom_right or center, got '{position}'")),
    })
}

/// A single dynamic-text clip spanning the whole range, resolved to the exact running timecode AT RENDER
/// TIME, every frame -- not a pre-baked clip per frame (the engine already has this mechanism: render.rs'
/// `dyn_text`, used by every Source::Text clip, already carries a non-drop `{tc:S}` token; this tool adds
/// the drop-frame counterpart `{tcd:S}` next to it, reusing timecode::label's real SMPTE drop-frame math,
/// and is the one-call wrapper that places/sizes/positions it). No frame cap: the clip is the same one
/// object whether the range is a second or an hour.
fn burn_in_timecode(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let track_id = str_arg(a, "track_id")?.to_string();
    let start = opt_num(a, "start", 0.0);
    let duration = opt_num(a, "duration", ss.project.duration() - start);
    if duration <= 0.0 { return Err("duration must be > 0".into()); }
    let fps = ss.project.fps;
    let drop = opt_bool(a, "drop_frame", false);
    let tc_start = opt_num(a, "timecode_start", 0.0);
    if tc_start < 0.0 { return Err("timecode_start must be >= 0".into()); }
    // Validate up front (wrong fps for drop_frame, etc) with the same parser the render-time token uses,
    // so a bad combination errors right away instead of silently drawing a dashed placeholder later.
    let first_label = crate::timecode::label(tc_start, fps, drop)?;
    let last_label = crate::timecode::label(tc_start + duration, fps, drop)?;
    let (size, color) = (opt_num(a, "size", 36.0), rgb(a, "color", [1.0, 1.0, 1.0]));
    let font = opt_str(a, "font", "SpaceMono-Bold").to_string(); // monospace: label width stays constant frame to frame
    let position = opt_str(a, "position", "bottom_left").to_string();
    let margin = opt_num(a, "margin", 0.03);
    let bg_opacity = opt_num(a, "background_opacity", 0.55);
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);

    let tw = crate::text::measure(&first_label, &font, size, 0.0, 0.0, "center", cw).map(|v| v.0).unwrap_or(first_label.chars().count() as f64 * size * 0.6);
    let (plate_w, plate_h) = (tw + size * 0.8, size * 1.6);
    let (px, py) = anchor_xy(&position, cw, ch, plate_w / 2.0, plate_h / 2.0, margin)?;

    let mut clips = vec![];
    if bg_opacity > 0.0 {
        let mut plate = Clip {
            id: ss.project.gen_id("clip"), source: Source::Shape { shape: "rect".into(), width: plate_w, height: plate_h, fill: [0.0, 0.0, 0.0], stroke_width: 0.0, stroke_color: [0.0, 0.0, 0.0], sides: 4, corner_radius: 4.0 },
            start, duration, in_point: 0.0, speed: 1.0, speed_curve: None, time_remap: None, speed_blend: Default::default(), props: default_props(), grade_graph: None, effects: vec![], blend: "normal".into(), mask: None, time_mode: TimeMode::Normal, corners: None, audio: Audio::default(), motion_blur: 0.0, echo: None, motion_path: None, posterize_fps: 0.0, text_fx: None, parent: None, inherit: Default::default(), name: None, role: None, tags: vec![], transition_in: None,
        };
        plate.props.get_mut("x").unwrap().base = px; plate.props.get_mut("y").unwrap().base = py; plate.props.get_mut("opacity").unwrap().base = bg_opacity;
        clips.push(plate);
    }
    // `dyn_text` resolves `{tc:S}`/`{tcd:S}` as (clip.start + local_time) + S, so S = tc_start - start makes
    // the displayed clock read exactly tc_start at the clip's own first frame regardless of where `start` sits.
    let offset = tc_start - start;
    let token = if drop { format!("{{tcd:{offset}}}") } else { format!("{{tc:{offset}}}") };
    let mut clock = Clip {
        id: ss.project.gen_id("clip"), source: Source::Text { text: token, font, size, color, letter_spacing: 0.0, stroke_width: 0.0, stroke_color: [0.0; 3], shadow: 0.0, align: "center".into() },
        start, duration, in_point: 0.0, speed: 1.0, speed_curve: None, time_remap: None, speed_blend: Default::default(), props: default_props(), grade_graph: None, effects: vec![], blend: "normal".into(), mask: None, time_mode: TimeMode::Normal, corners: None, audio: Audio::default(), motion_blur: 0.0, echo: None, motion_path: None, posterize_fps: 0.0, text_fx: None, parent: None, inherit: Default::default(), name: None, role: None, tags: vec![], transition_in: None,
    };
    clock.props.get_mut("x").unwrap().base = px; clock.props.get_mut("y").unwrap().base = py;
    clips.push(clock);

    let ids: Vec<String> = clips.iter().map(|c| c.id.clone()).collect();
    ss.project.tracks.iter_mut().find(|t| t.id == track_id).ok_or("no such track")?.clips.extend(clips.into_iter().map(std::sync::Arc::new));
    Ok(json!({"clip_ids": ids, "first_label": first_label, "last_label": last_label}))
}

/// A text or logo-image watermark pinned to a screen corner for the given range (default: the whole
/// timeline) -- a thin, named wrapper over add_clip/import_media (the same composite-over-a-higher-track
/// mechanism add_pip documents finding no dedicated tool for) so "put my logo in the corner" has one call
/// instead of five.
fn add_watermark(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let track_id = str_arg(a, "track_id")?.to_string();
    let start = opt_num(a, "start", 0.0);
    let duration = opt_num(a, "duration", ss.project.duration() - start);
    if duration <= 0.0 { return Err("duration must be > 0".into()); }
    let position = opt_str(a, "position", "bottom_right").to_string();
    let opacity = opt_num(a, "opacity", 0.6);
    if !(0.0..=1.0).contains(&opacity) { return Err("opacity must be in [0, 1]".into()); }
    let margin = opt_num(a, "margin", 0.03);
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);
    let text = a.get("text").and_then(|v| v.as_str());
    let image = a.get("image").and_then(|v| v.as_str());
    let clip_id = match (text, image) {
        (Some(t), None) => {
            let size = opt_num(a, "size", 28.0);
            let color = rgb(a, "color", [1.0, 1.0, 1.0]);
            let font = opt_str(a, "font", "Inter").to_string();
            let tw = crate::text::measure(t, &font, size, 0.0, 0.0, "center", cw).map(|v| v.0).unwrap_or(t.chars().count() as f64 * size * 0.6);
            let (px, py) = anchor_xy(&position, cw, ch, tw / 2.0, size * 0.7, margin)?;
            let cid = call(ss, "add_text", &json!({"track_id": track_id, "text": t, "start": start, "duration": duration, "size": size, "color": color, "font": font}))?["clip_id"].as_str().ok_or("add_text returned no clip_id")?.to_string();
            call(ss, "set_property", &json!({"clip_id": cid, "property": "x", "value": px}))?;
            call(ss, "set_property", &json!({"clip_id": cid, "property": "y", "value": py}))?;
            call(ss, "set_property", &json!({"clip_id": cid, "property": "opacity", "value": opacity}))?;
            cid
        }
        (None, Some(src)) => {
            let width_frac = opt_num(a, "size", 0.18);
            if !(width_frac > 0.0 && width_frac <= 1.0) { return Err("size (fraction of canvas width) must be in (0, 1]".into()); }
            let asset_id: String = match ss.project.find_clip(src) {
                Some(c) => match &c.source { Source::Media { asset } => asset.clone(), _ => return Err(format!("clip '{src}' is not a media clip; image must be a media clip id or a file path")) },
                None => call(ss, "import_media", &json!({"path": src}))?["asset_id"].as_str().ok_or("import_media returned no asset_id")?.to_string(),
            };
            let asset = ss.project.assets.get(&asset_id).ok_or("imported asset not found")?;
            let (aw, ah) = (asset.width.max(1) as f64, asset.height.max(1) as f64);
            // Media clips carry no width/height prop (only `scale`, uniform, applied after `fit` sizes the
            // clip to the canvas -- see add_pip.rs). `fit: contain` fits the asset's own aspect inside the
            // full canvas first (fitted_w <= cw, fitted_h <= ch, one of them exactly matching); the uniform
            // scale that turns that fitted size into exactly `width_frac * cw` wide (any source aspect,
            // any canvas aspect) is solved for here rather than promising a size the engine can't address directly.
            let fit_scale = (cw / aw).min(ch / ah);
            let fitted_w = aw * fit_scale;
            let w = width_frac * cw; let h = w * ah / aw;
            let clip_scale = w / fitted_w;
            let (px, py) = anchor_xy(&position, cw, ch, w / 2.0, h / 2.0, margin)?;
            let cid = call(ss, "add_clip", &json!({"track_id": track_id, "source_type": "media", "asset_id": asset_id, "start": start, "duration": duration, "fit": "contain"}))?["clip_id"].as_str().ok_or("add_clip returned no clip_id")?.to_string();
            call(ss, "set_property", &json!({"clip_id": cid, "property": "scale", "value": clip_scale}))?;
            call(ss, "set_property", &json!({"clip_id": cid, "property": "x", "value": px}))?;
            call(ss, "set_property", &json!({"clip_id": cid, "property": "y", "value": py}))?;
            call(ss, "set_property", &json!({"clip_id": cid, "property": "opacity", "value": opacity}))?;
            cid
        }
        (Some(_), Some(_)) => return Err("add_watermark: give text or image, not both".into()),
        (None, None) => return Err("add_watermark: give text or image".into()),
    };
    Ok(json!({"clip_id": clip_id}))
}

/// Write the timeline's own `markers` (add_marker/list_markers) into `path`'s container as real chapters
/// (ffmpeg metadata remux, stream copy -- same in-place-swap shape as export_presets' remux_faststart),
/// so a player's chapter list shows them, not just this editor's timeline. Chapters need at least 2
/// markers (a chapter is a span, not a point); ordered by time, each chapter runs to the next marker's
/// time, the last to the file's own probed duration.
fn mux_chapters(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?;
    if !std::path::Path::new(path).exists() { return Err(format!("mux_chapters: no such file '{path}' -- export it first")); }
    let mut markers: Vec<&crate::model::Marker> = ss.project.markers.iter().collect();
    markers.sort_by(|a, b| a.time.partial_cmp(&b.time).unwrap_or(std::cmp::Ordering::Equal));
    if markers.len() < 2 { return Err(format!("mux_chapters needs at least 2 timeline markers (add_marker) to form chapter spans; found {}", markers.len())); }
    let probe = Cmd::new("ffprobe").args(["-v", "error", "-show_entries", "format=duration", "-of", "default=noprint_wrappers=1:nokey=1", path]).output().map_err(|e| format!("ffprobe: {e}"))?;
    let dur_s: f64 = String::from_utf8_lossy(&probe.stdout).trim().parse().map_err(|_| "ffprobe returned no duration".to_string())?;
    let dur_ms = (dur_s * 1000.0).round() as i64;
    let mut meta = String::from(";FFMETADATA1\n");
    for (i, m) in markers.iter().enumerate() {
        let start_ms = (m.time * 1000.0).round() as i64;
        let end_ms = markers.get(i + 1).map(|n| (n.time * 1000.0).round() as i64).unwrap_or(dur_ms).max(start_ms + 1);
        let title = m.name.replace('\\', "\\\\").replace('=', "\\=").replace(';', "\\;").replace('#', "\\#").replace('\n', "\\\n");
        meta.push_str(&format!("[CHAPTER]\nTIMEBASE=1/1000\nSTART={start_ms}\nEND={end_ms}\ntitle={title}\n"));
    }
    let tmp_meta = format!("{path}.chapters.meta.tmp");
    std::fs::write(&tmp_meta, &meta).map_err(|e| e.to_string())?;
    let ext = std::path::Path::new(path).extension().and_then(|e| e.to_str()).unwrap_or("mp4");
    let tmp_out = format!("{path}.chapters.tmp.{ext}");
    let out = Cmd::new("ffmpeg").args(["-v", "error", "-y", "-i", path, "-i", &tmp_meta, "-map_metadata", "1", "-map", "0", "-codec", "copy"]).arg(&tmp_out).output().map_err(|e| format!("ffmpeg: {e}"))?;
    let _ = std::fs::remove_file(&tmp_meta);
    if !out.status.success() { let _ = std::fs::remove_file(&tmp_out); return Err(format!("mux_chapters: ffmpeg failed: {}", String::from_utf8_lossy(&out.stderr).trim())); }
    std::fs::rename(&tmp_out, path).map_err(|e| e.to_string())?;
    Ok(json!({"path": path, "chapters": markers.len()}))
}

pub fn commands() -> Vec<Command> { vec![
Command { name: "burn_in_timecode", description: "Burn a continuously advancing SMPTE timecode readout into the picture. One dynamic-text clip over `start`..`start+duration`, resolved to the exact HH:MM:SS:FF (or HH:MM:SS;FF for drop_frame) at render time every frame -- not a soft overlay, and not a clip per frame; no range-length limit. `timecode_start` offsets the displayed clock from a non-zero source timecode (e.g. a tape start) without moving the burn's own position on this timeline.",
    mutates: true,
    schema: obj(vec![("track_id", s("track to add the burn clips to")), ("start", n("timeline start s, default 0")), ("duration", n("duration s, default: rest of the timeline")), ("drop_frame", json!({"type":"boolean","description":"29.97/59.94 drop-frame labels (needs project fps 30000/1001 or 60000/1001); default false (non-drop, needs integer fps)"})), ("timecode_start", n("displayed clock's own start offset in s, default 0")), ("position", s("top_left|top_right|bottom_left|bottom_right|center, default bottom_left")), ("size", n("font size px, default 36")), ("color", json!({"type":"array","description":"RGB 0-1, default white"})), ("font", s("default SpaceMono-Bold")), ("margin", n("fraction of canvas from the edge, default 0.03")), ("background_opacity", n("backing plate opacity 0 disables it, default 0.55"))], &["track_id"]),
    run: burn_in_timecode },
Command { name: "add_watermark", description: "Pin a text or logo-image watermark to a screen corner (or center). Default range is the whole timeline; a named one-call wrapper over add_text/add_clip + set_property, the mechanism add_pip documents as the only way to do this before now. Give `text` or `image` (a media clip id or a file path to import), not both.",
    mutates: true,
    schema: obj(vec![("track_id", s("track to add the watermark to")), ("text", s("watermark text (give this or image)")), ("image", s("logo image: an existing media clip id, or a file path to import (give this or text)")), ("start", n("timeline start s, default 0")), ("duration", n("duration s, default: rest of the timeline")), ("position", s("top_left|top_right|bottom_left|bottom_right|center, default bottom_right")), ("opacity", n("0-1, default 0.6")), ("size", n("text: font size px (default 28); image: fraction of canvas width (default 0.18)")), ("color", json!({"type":"array","description":"text RGB 0-1, default white"})), ("font", s("text only, default Inter")), ("margin", n("fraction of canvas from the edge, default 0.03"))], &["track_id"]),
    run: add_watermark },
Command { name: "mux_chapters", description: "Write this timeline's own markers (add_marker) into an already-exported file as real container chapters. A player's own chapter list shows them, not just this editor's timeline (ffmpeg metadata remux, stream copy -- the file's picture/audio bytes are untouched). Needs at least 2 markers (a chapter is a span to the next marker, or to the file's end for the last one).",
    mutates: false,
    schema: obj(vec![("path", s("path of an already-exported video file to add chapters to, in place"))], &["path"]),
    run: mux_chapters },
] }
