//! v7.4: export_edl -- the write-side counterpart to v7.3's import_edl. v7.3 closed the "bring a cut list
//! IN" gap; this closes the matching "send a cut list OUT" gap the same v7.3 audit flagged and deliberately
//! left for later: a standard CMX3600 EDL, a simple single-video-track OTIO JSON timeline (the same shape
//! import_edl's own parse_otio reads, so export then import_edl round-trips), or a minimal but real FCPXML
//! project (resources + a sequence/spine of asset-clips, frame-exact rational times) -- the three
//! interchange formats every mainstream NLE (Premiere, Resolve, Final Cut, Avid) can open, so an edit made
//! here can leave kimo and continue life in someone else's tool, or come back from one of theirs.
//!
//! Only a media clip is representable on an EDL/OTIO/FCPXML cut list (a solid, gradient, shape, text or
//! precomp clip has no source file an external NLE could open); any non-media clip on the exported track is
//! skipped and reported under `skipped_non_media` rather than silently dropped or failing the export. A
//! clip played at a non-1.0 speed has no universal cross-NLE representation in any of the three formats
//! (CMX3600 has no concept of it at all; OTIO's own speed effects and FCPXML's conform-rate are each a much
//! bigger step kimo's own timeline model doesn't carry), so its exported source range covers the actual
//! footage consumed (`in_point` .. `in_point + duration * speed`) and the clip is listed under
//! `approximated_speed` -- re-importing it elsewhere plays that same footage span at normal speed, not at
//! kimo's retimed speed, a real and stated limitation rather than a silent wrong number.
use super::kit::*;
use crate::model::Source;

struct ExEvent { name: String, path: String, source_in: f64, source_out: f64, record_in: f64, record_out: f64, speed_approximated: bool }

/// Every media clip on `track_id` (or, when omitted, the one track in the project that actually has media
/// clips -- an error naming every track's media-clip count when there is more than one, rather than
/// guessing which one the caller meant), in record order, resolved to events ready for any of the three
/// serializers. Non-media clips are reported under `skipped_non_media`, not silently dropped.
fn gather(ss: &Session, a: &Value) -> Result<(String, Vec<ExEvent>, Vec<Value>), String> {
    let track_id = match a.get("track_id").and_then(|v| v.as_str()) {
        Some(t) => { if !ss.project.tracks.iter().any(|tr| tr.id == t) { return Err(format!("no track '{t}'")); } t.to_string() }
        None => {
            let with_media: Vec<(&str, usize)> = ss.project.tracks.iter().map(|t| (t.id.as_str(), t.clips.iter().filter(|c| matches!(c.source, Source::Media { .. })).count())).filter(|(_, n)| *n > 0).collect();
            match with_media.len() {
                0 => return Err("no track has any media clips to export".into()),
                1 => with_media[0].0.to_string(),
                _ => return Err(format!("more than one track has media clips; pass track_id. Candidates: {}", with_media.iter().map(|(id, n)| format!("{id} ({n} media clip{})", if *n == 1 { "" } else { "s" })).collect::<Vec<_>>().join(", "))),
            }
        }
    };
    let track = ss.project.tracks.iter().find(|t| t.id == track_id).unwrap();
    let mut clips: Vec<_> = track.clips.iter().collect();
    clips.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
    let mut events = vec![]; let mut skipped = vec![];
    for c in &clips {
        let Source::Media { asset } = &c.source else { skipped.push(json!({"clip_id": c.id, "name": c.name, "start": c.start, "duration": c.duration, "reason": "not a media clip (no source file to reference)"})); continue; };
        let Some(a) = ss.project.assets.get(asset) else { skipped.push(json!({"clip_id": c.id, "name": c.name, "reason": "media asset no longer in the project"})); continue; };
        let path = ss.project.resolve(&a.path);
        let name = c.name.clone().unwrap_or_else(|| std::path::Path::new(&path).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| path.clone()));
        let speed_approximated = (c.speed - 1.0).abs() > 1e-9;
        events.push(ExEvent { name, path, source_in: c.in_point, source_out: c.in_point + c.duration * c.speed, record_in: c.start, record_out: c.start + c.duration, speed_approximated });
    }
    Ok((track_id, events, skipped))
}

/// Frame-exact HH:MM:SS:FF, non-drop-frame. `fps` is rounded to the nearest whole frame rate for the
/// column math (CMX3600 timecode has no fractional-fps representation; drop-frame NTSC is not attempted,
/// matching import_edl's own parse_tc, which only ever treats ';' as a ':' rather than doing 29.97 drop math).
fn fmt_tc(seconds: f64, fps: f64) -> String {
    let fps_i = (fps.round() as i64).max(1);
    let total_frames = ((seconds.max(0.0)) * fps_i as f64).round() as i64;
    let ff = total_frames % fps_i; let total_secs = total_frames / fps_i;
    let ss = total_secs % 60; let total_mins = total_secs / 60; let mm = total_mins % 60; let hh = total_mins / 60;
    format!("{hh:02}:{mm:02}:{ss:02}:{ff:02}")
}

fn to_cmx3600(title: &str, events: &[ExEvent], fps: f64) -> String {
    let mut out = format!("TITLE: {title}\nFCM: NON-DROP FRAME\n\n");
    for (i, e) in events.iter().enumerate() {
        let n = i + 1;
        out += &format!("{n:03}  AX{n:02}     V     C        {} {} {} {}\n", fmt_tc(e.source_in, fps), fmt_tc(e.source_out, fps), fmt_tc(e.record_in, fps), fmt_tc(e.record_out, fps));
        out += &format!("* FROM CLIP NAME: {}\n", e.path);
    }
    out
}

fn to_otio(title: &str, events: &[ExEvent], fps: f64) -> Value {
    let rt = |seconds: f64| json!({"OTIO_SCHEMA": "RationalTime.1", "value": seconds * fps, "rate": fps});
    let mut children = vec![]; let mut pos = 0.0;
    for e in events {
        if e.record_in > pos + 1e-9 { children.push(json!({"OTIO_SCHEMA": "Gap.1", "source_range": {"OTIO_SCHEMA": "TimeRange.1", "start_time": rt(0.0), "duration": rt(e.record_in - pos)}})); }
        let dur = e.record_out - e.record_in;
        children.push(json!({"OTIO_SCHEMA": "Clip.2", "name": e.name,
            "media_reference": {"OTIO_SCHEMA": "ExternalReference.1", "target_url": e.path},
            "source_range": {"OTIO_SCHEMA": "TimeRange.1", "start_time": rt(e.source_in), "duration": rt(dur)}}));
        pos = e.record_out;
    }
    json!({"OTIO_SCHEMA": "Timeline.1", "name": title,
        "tracks": {"OTIO_SCHEMA": "Stack.1", "children": [{"OTIO_SCHEMA": "Track.1", "kind": "Video", "name": "V1", "children": children}]}})
}

fn xml_escape(s: &str) -> String { s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;") }
/// Frame-exact rational time string, the form FCPXML actually wants ("150/25s"), not a decimal seconds
/// string -- so every offset/duration/start lands exactly on a frame boundary of `fps` instead of drifting.
fn fcp_time(seconds: f64, fps_i: i64) -> String { let frames = (seconds.max(0.0) * fps_i as f64).round() as i64; format!("{frames}/{fps_i}s") }

fn to_fcpxml(title: &str, events: &[ExEvent], width: u32, height: u32, fps: f64) -> String {
    let fps_i = (fps.round() as i64).max(1);
    let mut assets = String::new(); let mut spine = String::new();
    for (i, e) in events.iter().enumerate() {
        let id = format!("a{}", i + 1);
        let dur = e.record_out - e.record_in;
        assets += &format!("    <asset id=\"{id}\" name=\"{name}\" src=\"file://{path}\" start=\"0s\" duration=\"{fulldur}\" hasVideo=\"1\" format=\"r0\"/>\n",
            id = id, name = xml_escape(&e.name), path = xml_escape(&e.path), fulldur = fcp_time(e.source_out.max(e.source_in + dur), fps_i));
        spine += &format!("        <asset-clip ref=\"{id}\" name=\"{name}\" offset=\"{offset}\" duration=\"{duration}\" start=\"{start}\"/>\n",
            id = id, name = xml_escape(&e.name), offset = fcp_time(e.record_in, fps_i), duration = fcp_time(dur, fps_i), start = fcp_time(e.source_in, fps_i));
    }
    let total = events.last().map(|e| e.record_out).unwrap_or(0.0);
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<fcpxml version=\"1.9\">\n  <resources>\n    <format id=\"r0\" name=\"kimo_export\" frameDuration=\"1/{fps_i}s\" width=\"{width}\" height=\"{height}\"/>\n{assets}  </resources>\n  <library>\n    <event name=\"{title_esc}\">\n      <project name=\"{title_esc}\">\n        <sequence format=\"r0\" duration=\"{total}\">\n          <spine>\n{spine}          </spine>\n        </sequence>\n      </project>\n    </event>\n  </library>\n</fcpxml>\n",
        fps_i = fps_i, width = width, height = height, assets = assets, title_esc = xml_escape(title), total = fcp_time(total, fps_i), spine = spine)
}

fn export_edl(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let format = a.get("format").and_then(|v| v.as_str()).unwrap_or("cmx3600").to_string();
    if !["cmx3600", "otio", "fcpxml"].contains(&format.as_str()) { return Err(format!("format must be cmx3600, otio or fcpxml, got '{format}'")); }
    let fps = opt_num(a, "fps", ss.project.fps);
    let title = a.get("title").and_then(|v| v.as_str()).unwrap_or_else(|| if ss.project.name.is_empty() { "kimo export" } else { &ss.project.name }).to_string();
    let (track_id, events, skipped) = gather(ss, a)?;
    if events.is_empty() { return Err(format!("track '{track_id}' has no media clips to export (skipped {}: not media)", skipped.len())); }
    let approximated_speed: Vec<Value> = events.iter().filter(|e| e.speed_approximated).map(|e| json!({"name": e.name, "record_in": e.record_in, "record_out": e.record_out})).collect();

    let content = match format.as_str() {
        "cmx3600" => to_cmx3600(&title, &events, fps),
        "otio" => serde_json::to_string_pretty(&to_otio(&title, &events, fps)).map_err(|e| format!("could not serialize OTIO JSON: {e}"))?,
        "fcpxml" => to_fcpxml(&title, &events, ss.project.width, ss.project.height, fps),
        _ => unreachable!(),
    };

    let mut result = json!({"format": format, "track_id": track_id, "events": events.len(), "skipped_non_media": skipped, "approximated_speed": approximated_speed, "content": content});
    if let Some(path) = a.get("path").and_then(|v| v.as_str()) {
        std::fs::write(path, &content).map_err(|e| format!("could not write {path}: {e}"))?;
        result["path"] = json!(path);
        result["content"] = Value::Null; // written to disk; do not also return the whole body inline
        result["content_bytes"] = json!(content.len());
    }
    Ok(result)
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "export_edl", description: "Export the timeline as an editorial-interchange cut list: CMX3600 EDL, a simple OTIO JSON timeline, or a minimal FCPXML project. The write-side counterpart to import_edl (v7.3 only brought a cut list in).", mutates: false,
        schema: obj(vec![
            ("format", json!({"type":"string","enum":["cmx3600","otio","fcpxml"],"description":"default cmx3600"})),
            ("track_id", s("which track to export; default: the one track in the project with media clips (an error names every candidate when more than one does)")),
            ("path", s("write the result to this file instead of only returning it as `content`")),
            ("title", s("sequence/project name; default the project's own name, else \"kimo export\"")),
            ("fps", n("frame rate for CMX3600/FCPXML timecode math; default the project's own fps (OTIO carries its own rate per time value and ignores this)"))], &[]),
        run: export_edl },
] }
