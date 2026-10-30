//! v7.3: `import_edl` -- a real editorial-interchange gap: kimo could export EDL-adjacent things but had no
//! way to bring a cut list IN. A standard CMX3600 EDL (the lowest-common-denominator format every NLE
//! speaks) or a simple single-video-track OTIO JSON timeline becomes real clips on the project's timeline:
//! each event's record in/out places and sizes the clip, its source in point offsets into the media, and
//! its reel/clip name (an EDL's `* FROM CLIP NAME:` comment when present, else the reel field; an OTIO
//! clip's own name) is resolved to an actual media file by an explicit `media_map`, a `media_dir` scan, or
//! a path the file already names outright. A name that resolves to nothing is reported under `unresolved`
//! with its record range (never silently dropped, never a hard failure for the whole import) -- the same
//! honest-gap reporting collect_project/relink_media already use for missing media.
use super::kit::*;
use crate::commands::{dispatch_nested, Session};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
struct Event { clip_name: String, source_in: f64, record_in: f64, record_out: f64 }

/// HH:MM:SS:FF or drop-frame HH:MM:SS;FF -> seconds at `fps`. None if it isn't that shape.
fn parse_tc(tok: &str, fps: f64) -> Option<f64> {
    let s = tok.replace(';', ":");
    let parts: Vec<&str> = s.split(':').collect();
    if parts.len() != 4 { return None; }
    let nums: Vec<i64> = parts.iter().map(|p| if p.len() == 2 && p.chars().all(|c| c.is_ascii_digit()) { p.parse().ok() } else { None }).collect::<Option<_>>()?;
    Some(nums[0] as f64 * 3600.0 + nums[1] as f64 * 60.0 + nums[2] as f64 + nums[3] as f64 / fps)
}

/// Parses a CMX3600 EDL's video (V/B) events. Audio-only events (A, A2, ...) are not timeline picture and
/// are skipped; a dissolve/wipe's extra transition-duration token is tolerated (ignored -- imported as a
/// cut, same record range) since kimo's timeline has no transition-duration concept to place it into.
fn parse_cmx3600(text: &str, fps: f64) -> Vec<Event> {
    // CMX3600 convention: a `* FROM CLIP NAME:` comment FOLLOWS the event it names, so it retroactively
    // overwrites the just-pushed event's reel-derived name, not a name queued for the next event.
    let mut events: Vec<Event> = vec![];
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() { continue; }
        if let Some(rest) = line.strip_prefix('*') {
            let rest = rest.trim();
            if let Some(idx) = rest.to_ascii_uppercase().find("FROM CLIP NAME") {
                if let Some(colon) = rest[idx..].find(':') {
                    let name = rest[idx + colon + 1..].trim().to_string();
                    if let Some(last) = events.last_mut() { last.clip_name = name; }
                }
            }
            continue;
        }
        let tokens: Vec<&str> = line.split_whitespace().collect();
        if tokens.len() < 8 || !tokens[0].chars().all(|c| c.is_ascii_digit()) { continue; } // not an event line (header, title, blank-ish)
        let last4 = &tokens[tokens.len() - 4..];
        let Some(tcs): Option<Vec<f64>> = last4.iter().map(|t| parse_tc(t, fps)).collect() else { continue };
        let track_type = tokens[2];
        if !track_type.to_ascii_uppercase().contains('V') { continue; } // audio-only event: not picture
        let reel = tokens[1].to_string();
        events.push(Event { clip_name: reel, source_in: tcs[0], record_in: tcs[2], record_out: tcs[3] });
    }
    events
}

/// Walks one OTIO video Track's children in order, accumulating record position as it goes (a Gap's
/// duration advances the position without producing a clip; anything else not a Clip -- a nested Stack, a
/// Transition -- is skipped, a real but accepted limitation for a first pass). start_time/duration are
/// OTIO RationalTime {value, rate}; value/rate is always seconds regardless of the track's own rate.
fn otio_time(v: &Value) -> Option<f64> { Some(v.get("value")?.as_f64()? / v.get("rate")?.as_f64()?) }
fn parse_otio(root: &Value) -> Result<Vec<Event>, String> {
    let stack = root.get("tracks").ok_or("otio: no top-level \"tracks\" field")?;
    let children = stack.get("children").and_then(|v| v.as_array()).ok_or("otio: tracks.children is not an array")?;
    let track = children.iter().find(|t| t.get("kind").and_then(|v| v.as_str()) == Some("Video")).ok_or("otio: no Video track found under tracks.children")?;
    let items = track.get("children").and_then(|v| v.as_array()).ok_or("otio: video track has no children array")?;
    let mut events = vec![]; let mut pos = 0.0;
    for item in items {
        let schema = item.get("OTIO_SCHEMA").and_then(|v| v.as_str()).unwrap_or("");
        let sr = item.get("source_range");
        let dur = sr.and_then(|r| r.get("duration")).and_then(otio_time).unwrap_or(0.0);
        if schema.starts_with("Clip") {
            let source_in = sr.and_then(|r| r.get("start_time")).and_then(otio_time).unwrap_or(0.0);
            let name = item.get("name").and_then(|v| v.as_str()).map(String::from)
                .or_else(|| item.pointer("/media_reference/target_url").and_then(|v| v.as_str()).map(String::from))
                .ok_or("otio: a Clip has neither a name nor media_reference.target_url to resolve media by")?;
            events.push(Event { clip_name: name, source_in, record_in: pos, record_out: pos + dur });
        }
        pos += dur; // Gap and anything else just advance the record position
    }
    Ok(events)
}

/// media_map (exact name match) first, then a media_dir scan (an exact filename, else a case-insensitive
/// stem match against every file in the directory), then the name itself if it is already a real path.
fn resolve_media(name: &str, media_map: &BTreeMap<String, String>, media_dir: Option<&Path>) -> Option<PathBuf> {
    if let Some(p) = media_map.get(name) { let p = PathBuf::from(p); if p.is_file() { return Some(p); } }
    if let Some(dir) = media_dir {
        let direct = dir.join(name);
        if direct.is_file() { return Some(direct); }
        let target_stem = Path::new(name).file_stem().map(|s| s.to_string_lossy().to_lowercase());
        if let Ok(rd) = std::fs::read_dir(dir) {
            let mut entries: Vec<_> = rd.flatten().collect(); entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let p = entry.path(); if !p.is_file() { continue; }
                let stem = p.file_stem().map(|s| s.to_string_lossy().to_lowercase());
                if stem == target_stem || p.file_name().map(|f| f.to_string_lossy().to_lowercase()) == Some(name.to_lowercase()) { return Some(p); }
            }
        }
    }
    let p = Path::new(name); if p.is_file() { return Some(p.to_path_buf()); }
    None
}

fn import_edl(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?;
    let text = std::fs::read_to_string(path).map_err(|e| format!("{e} ({path})"))?;
    let media_dir = a.get("media_dir").and_then(|v| v.as_str()).map(Path::new);
    let media_map: BTreeMap<String, String> = a.get("media_map").and_then(|v| v.as_object()).map(|m| m.iter().filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string()))).collect()).unwrap_or_default();
    let dry_run = a.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
    let fps = opt_num(a, "fps", ss.project.fps);

    let requested_format = a.get("format").and_then(|v| v.as_str());
    let trimmed = text.trim_start();
    let parsed_json: Option<Value> = if trimmed.starts_with('{') { serde_json::from_str(&text).ok() } else { None };
    let is_otio = parsed_json.as_ref().map(|v| v.get("OTIO_SCHEMA").is_some() || v.get("tracks").is_some()).unwrap_or(false);
    let format = match requested_format { Some(f) => f.to_string(), None => if is_otio { "otio".into() } else { "cmx3600".into() } };

    let mut events = match format.as_str() {
        "otio" => { let root = parsed_json.ok_or("format \"otio\" but the file is not valid JSON")?; parse_otio(&root)? }
        "cmx3600" => parse_cmx3600(&text, fps),
        other => return Err(format!("unknown format '{other}': expected \"cmx3600\" or \"otio\"")),
    };
    events.sort_by(|a, b| a.record_in.partial_cmp(&b.record_in).unwrap_or(std::cmp::Ordering::Equal));

    let mut resolved: Vec<(Event, PathBuf)> = vec![];
    let mut unresolved: Vec<Value> = vec![];
    for ev in &events {
        match resolve_media(&ev.clip_name, &media_map, media_dir) {
            Some(p) => resolved.push((ev.clone(), p)),
            None => unresolved.push(json!({"clip_name": ev.clip_name, "record_in": ev.record_in, "record_out": ev.record_out})),
        }
    }

    if dry_run {
        return Ok(json!({"dry_run": true, "format": format, "events_total": events.len(), "imported": resolved.len(), "unresolved": unresolved,
            "would_import": resolved.iter().map(|(ev, p)| json!({"clip_name": ev.clip_name, "path": p.to_string_lossy(), "start": ev.record_in, "duration": ev.record_out - ev.record_in, "in_point": ev.source_in})).collect::<Vec<_>>()}));
    }

    let track_id = match a.get("track_id").and_then(|v| v.as_str()) {
        Some(t) => { if !ss.project.tracks.iter().any(|tr| tr.id == t) { return Err(format!("no track '{t}'")); } t.to_string() }
        None => dispatch_nested(ss, "add_track", &json!({}))?["track_id"].as_str().unwrap().to_string(),
    };

    // one import_media per distinct resolved path, reusing an already-imported asset (by path) so the same
    // source file referenced by several events is not imported twice
    let mut asset_by_path: BTreeMap<String, String> = ss.project.assets.values().map(|asset| (asset.path.clone(), asset.id.clone())).collect();
    let mut imported = 0usize;
    for (ev, p) in &resolved {
        let path_str = p.to_string_lossy().to_string();
        let asset_id = match asset_by_path.get(&path_str) {
            Some(id) => id.clone(),
            None => { let id = dispatch_nested(ss, "import_media", &json!({"path": path_str}))?["asset_id"].as_str().unwrap().to_string(); asset_by_path.insert(path_str, id.clone()); id }
        };
        dispatch_nested(ss, "add_clip", &json!({"track_id": track_id, "source_type": "media", "asset_id": asset_id, "start": ev.record_in, "duration": ev.record_out - ev.record_in, "in_point": ev.source_in}))?;
        imported += 1;
    }

    Ok(json!({"format": format, "track_id": track_id, "events_total": events.len(), "imported": imported, "unresolved": unresolved}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "import_edl", description: "Import a CMX3600 EDL or a simple OTIO JSON timeline as real clips on the timeline. Format is auto-detected from content (valid JSON with an OTIO_SCHEMA/tracks field -> otio, else cmx3600) or set explicitly with `format`. Each event's record in/out places and sizes the resulting clip and its source in point offsets into the media; an EDL's `* FROM CLIP NAME:` comment names the source when present (else the reel field), an OTIO clip uses its own name (or media_reference.target_url). CMX3600: only V/B (video) events become clips; audio-only events are skipped, dissolve/wipe transitions import as a cut at the same record range (no transition-duration concept on this timeline). OTIO: the first Video track under tracks.children; a Gap advances position without a clip, anything else (nested stacks, transitions) is skipped -- a real, stated limitation. Each event's source name is resolved to a real file by `media_map` (exact name -> path) first, then a `media_dir` scan (exact filename, else case-insensitive stem match), then the name itself if it is already a real path; anything that resolves to nothing is reported under `unresolved` with its record range rather than silently dropped or failing the whole import. `track_id` places onto an existing track; omitted creates a new one. `dry_run` reports events_total/imported/unresolved and the exact clips that would be created, without creating a track, importing media, or adding any clip.", mutates: true,
        schema: obj(vec![("path", s("the .edl or .otio/.json file to import")), ("format", json!({"type":"string","enum":["cmx3600","otio"],"description":"override auto-detection"})),
            ("media_dir", s("directory to resolve clip/reel names against (exact filename, else case-insensitive stem match)")),
            ("media_map", json!({"type":"object","description":"explicit clip_name/reel -> file path overrides, checked before media_dir"})),
            ("track_id", s("existing track to place onto; default: a new track is created")),
            ("fps", n("frame rate for CMX3600 timecode math; default the project's own fps (OTIO carries its own rate per time value and ignores this)")),
            ("dry_run", json!({"type":"boolean","description":"report would-import/unresolved, create nothing"}))], &["path"]),
        run: import_edl },
] }
