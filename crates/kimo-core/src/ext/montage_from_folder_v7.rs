//! make_montage_from_folder (FEATURES.md L561, the "templates" MCP-convenience line): scan a folder
//! non-recursively for recognized video/still files, import each in filename order, and lay them back to
//! back (optionally cross-fading) on a track -- a quick "drop clips/photos in a folder, get an edited
//! sequence" template, the Kimo analogue of a NLE's "insert all from bin" action.
//!
//! Stills have no native duration (import_media/probe reports 0 for them, the existing convention --
//! render.rs's own comment: "Undated still images keep duration=0, matching existing import semantics"),
//! so clip_duration is required whenever the folder holds any. Video/audio clips use their own probed
//! duration when clip_duration is omitted, or when clip_duration is omitted entirely; when clip_duration IS
//! given, it caps a longer clip but never stretches a shorter one (no speed changes here -- just trims).
//!
//! crossfade (default 0, hard cuts) overlaps each clip after the first with the one before it by that many
//! seconds and registers add_transition's "crossfade" kind on the incoming clip, the same mechanism a
//! person would reach for by hand.
use super::kit::*;

/// Video container extensions this template recognizes by name (import_media/ffprobe would accept more,
/// but these are the common camera/export containers a "folder of clips" actually contains).
const VIDEO_EXT: &[&str] = &["mp4", "mov", "mkv", "webm", "avi", "m4v"];

fn recognized_kind(path: &std::path::Path) -> Option<&'static str> {
    let ext = path.extension()?.to_str()?.to_lowercase();
    if VIDEO_EXT.contains(&ext.as_str()) { return Some("video"); }
    if crate::render::STILL_EXT.contains(&ext.as_str()) { return Some("still"); }
    None
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "make_montage_from_folder", description: "Scan a folder (not recursive) for recognized video/still files, import each in filename order, lay them back to back on a track. clip_duration is required if any still is present (stills have no native duration); for video it caps a longer clip but never stretches a shorter one. crossfade > 0 overlaps each clip via add_transition's crossfade kind.", mutates: true,
        schema: obj(vec![
            ("folder", s("absolute path to the directory to scan")),
            ("track_id", s("optional: an existing track to append to (after its last clip's own end, from `start`); omitted creates a new track")),
            ("start", n("timeline seconds to begin at; default 0 (ignored in effect when track_id already has clips past this point -- clips are placed in sequence from here, not merged with what exists)")),
            ("clip_duration", n("seconds each clip is trimmed to at most; required if the folder contains any still image. Omit to let video/audio clips keep their own full probed duration")),
            ("crossfade", n("seconds of overlap between consecutive clips, applied as add_transition's crossfade kind on the incoming clip; default 0 (hard cuts)")),
            ("fit", json!({"type":"string","enum":["contain","cover","stretch"],"description":"media framing passed to each add_clip; default cover"})),
        ], &["folder"]),
        run: run_montage },
] }

fn run_montage(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let folder = str_arg(a, "folder")?;
    let dir = std::path::Path::new(folder);
    if !dir.is_dir() { return Err(format!("make_montage_from_folder: '{folder}' is not a directory")); }
    let read = std::fs::read_dir(dir).map_err(|e| format!("make_montage_from_folder: cannot read '{folder}': {e}"))?;

    let mut files: Vec<std::path::PathBuf> = vec![];
    let mut skipped: Vec<String> = vec![];
    for entry in read {
        let Ok(entry) = entry else { continue };
        let path = entry.path();
        if !path.is_file() { continue; }
        if recognized_kind(&path).is_some() { files.push(path); }
        else { skipped.push(path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()); }
    }
    files.sort();
    if files.is_empty() {
        return Err(format!("make_montage_from_folder: no recognized media files in '{folder}' (looked for {}, {})",
            VIDEO_EXT.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(", "),
            crate::render::STILL_EXT.iter().map(|e| format!(".{e}")).collect::<Vec<_>>().join(", ")));
    }

    let clip_duration = a.get("clip_duration").and_then(|v| v.as_f64());
    if let Some(d) = clip_duration { if !(d > 0.0) { return Err("make_montage_from_folder: clip_duration must be > 0".into()); } }
    let crossfade = opt_num(a, "crossfade", 0.0);
    if crossfade < 0.0 { return Err("make_montage_from_folder: crossfade must be >= 0".into()); }
    let fit = a.get("fit").and_then(|v| v.as_str()).unwrap_or("cover").to_string();
    if !["contain", "cover", "stretch"].contains(&fit.as_str()) { return Err(format!("make_montage_from_folder: bad fit '{fit}'; use contain, cover or stretch")); }

    let track_id = match a.get("track_id").and_then(|v| v.as_str()) {
        Some(t) => t.to_string(),
        None => crate::commands::dispatch_nested(ss, "add_track", &json!({}))?["track_id"].as_str().unwrap().to_string(),
    };

    let mut cursor = opt_num(a, "start", 0.0);
    let mut clip_ids = vec![];
    for (i, path) in files.iter().enumerate() {
        let kind = recognized_kind(path).unwrap();
        let path_str = path.to_str().ok_or_else(|| format!("make_montage_from_folder: non-UTF8 path {path:?}"))?;
        let imported = crate::commands::dispatch_nested(ss, "import_media", &json!({"path": path_str}))
            .map_err(|e| format!("make_montage_from_folder: importing {path_str}: {e}"))?;
        let asset_id = imported["asset_id"].as_str().unwrap().to_string();
        let native_duration = imported["duration"].as_f64().unwrap_or(0.0);

        let duration = if kind == "still" {
            clip_duration.ok_or_else(|| format!("make_montage_from_folder: {path_str} is a still image (no native duration) -- clip_duration is required when the folder contains stills"))?
        } else if native_duration > 0.0 {
            match clip_duration { Some(cap) => native_duration.min(cap), None => native_duration }
        } else {
            clip_duration.ok_or_else(|| format!("make_montage_from_folder: {path_str} probed with no readable duration and clip_duration was not given"))?
        };

        let overlap = if i == 0 { 0.0 } else { crossfade.min(duration) };
        let start = cursor - overlap;
        let add = crate::commands::dispatch_nested(ss, "add_clip", &json!({
            "track_id": &track_id, "source_type": "media", "asset_id": &asset_id,
            "start": start, "duration": duration, "in_point": 0, "fit": &fit,
        })).map_err(|e| format!("make_montage_from_folder: placing {path_str}: {e}"))?;
        let clip_id = add["clip_id"].as_str().unwrap().to_string();
        if i > 0 && overlap > 0.0 {
            crate::commands::dispatch_nested(ss, "add_transition", &json!({"clip_id": &clip_id, "kind": "crossfade", "duration": overlap}))
                .map_err(|e| format!("make_montage_from_folder: crossfading {path_str}: {e}"))?;
        }
        clip_ids.push(clip_id);
        cursor = start + duration;
    }

    Ok(json!({"track_id": track_id, "clip_ids": clip_ids, "skipped": skipped, "end": cursor}))
}
