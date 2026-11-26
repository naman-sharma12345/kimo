//! Caption files in every broadcast and web format through craft-libs' craft-captions (MIT OR Apache-2.0, vendored in
//! third_party/craft-libs): import_captions reads SubRip, WebVTT, Scenarist SCC (CEA-608 pop-on, roll-up, paint-on),
//! MacCaption MCC (CEA-608 and CEA-708), EBU STL, TTML / IMSC1, DFXP and ASS/SSA into caption text clips;
//! export_captions_as writes a caption track to any of them (SCC and MCC with drop-frame 29.97 timecode).
use super::kit::*;
use craft_captions::time::{Tick, TICKS_PER_SECOND};
use craft_captions::{Cue, Document, Format, WriteOptions};

fn fmt_of(name: &str) -> Option<Format> {
    Some(match name { "srt" => Format::Srt, "vtt" | "webvtt" => Format::WebVtt, "scc" => Format::Scc, "mcc" => Format::Mcc, "stl" => Format::Stl, "ttml" | "imsc" => Format::Ttml, "dfxp" => Format::Dfxp, "ass" | "ssa" => Format::Ass, _ => return None })
}
fn name_of(f: Format) -> &'static str { match f { Format::Srt => "srt", Format::WebVtt => "vtt", Format::Scc => "scc", Format::Mcc => "mcc", Format::Stl => "stl", Format::Ttml => "ttml", Format::Dfxp => "dfxp", Format::Ass => "ass", #[allow(unreachable_patterns)] _ => "other" } }
fn secs(t: Tick) -> f64 { t.0 as f64 / TICKS_PER_SECOND as f64 }
fn tk(s: f64) -> Tick { Tick((s * TICKS_PER_SECOND as f64).round() as i64) }

fn import_captions(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?.to_string(); let bytes = std::fs::read(&path).map_err(|e| format!("could not read {path}: {e}"))?;
    let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_lowercase());
    let fmt = match a.get("format").and_then(|v| v.as_str()) { Some(f) => fmt_of(f).ok_or_else(|| format!("unknown caption format '{f}'"))?, None => craft_captions::detect(&bytes, ext.as_deref()).ok_or("not a caption file kimo reads (srt, vtt, scc, mcc, stl, ttml, dfxp, ass)")? };
    let doc = craft_captions::parse(&bytes, fmt).map_err(|e| format!("could not read the {} file: {e}", name_of(fmt)))?;
    if doc.cues.is_empty() { return Err("the file has no captions".into()); }
    // kimo's caption styling path reads SRT: hand it the cues as SRT (times exact to the millisecond)
    let srt = craft_captions::write(&doc, Format::Srt, WriteOptions::default());
    let tmp = std::env::temp_dir().join(format!("kimo-captions-{}-{}.srt", std::process::id(), ss.project.next_id)); std::fs::write(&tmp, &srt).map_err(|e| e.to_string())?;
    let tid = match a.get("track_id").and_then(|v| v.as_str()) { Some(t) => t.to_string(), None => crate::commands::dispatch_nested(ss, "add_track", &json!({"name": "Captions"}))?["track_id"].as_str().unwrap_or_default().to_string() };
    let mut args = json!({"track_id": tid, "path": tmp.to_string_lossy()});
    for k in ["size", "color", "font", "stroke_width", "y"] { if let Some(v) = a.get(k) { args[k] = v.clone(); } }
    let r = crate::commands::dispatch_nested(ss, "import_srt", &args); let _ = std::fs::remove_file(&tmp); let r = r?;
    let speakers: std::collections::BTreeSet<&str> = doc.cues.iter().filter_map(|c| c.speaker.as_deref()).collect();
    Ok(json!({"format": name_of(fmt), "cues": doc.cues.len(), "track_id": tid, "speakers": speakers, "warnings": doc.warnings, "result": r}))
}

fn export_captions_as(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?.to_string();
    let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let fname = a.get("format").and_then(|v| v.as_str()).map(String::from).unwrap_or(ext);
    let fmt = fmt_of(&fname).ok_or_else(|| format!("format must be srt, vtt, scc, mcc, stl, ttml, dfxp or ass, got '{fname}'"))?;
    if std::path::Path::new(&path).exists() && !a.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false) { return Err(format!("{path} exists; pass overwrite: true to replace it")); }
    let p = &ss.project;
    // the caption track: given, a caption group's, or the only track of text clips
    let tid = match (a.get("track_id").and_then(|v| v.as_str()), a.get("caption_id").and_then(|v| v.as_str())) {
        (Some(t), _) => t.to_string(),
        (None, Some(c)) => p.editorial.captions.get(c).map(|g| g.track_id.clone()).ok_or_else(|| format!("no caption group '{c}'"))?,
        (None, None) => { let cands: Vec<&str> = p.tracks.iter().filter(|t| !t.clips.is_empty() && t.clips.iter().all(|c| matches!(c.source, crate::model::Source::Text { .. }))).map(|t| t.id.as_str()).collect();
            match cands.as_slice() { [one] => one.to_string(), [] => return Err("no track of text clips; give track_id".into()), many => return Err(format!("several text tracks ({}); give track_id", many.join(", "))) } }
    };
    let t = p.tracks.iter().find(|t| t.id == tid).ok_or_else(|| format!("no track '{tid}'"))?;
    let mut cues: Vec<Cue> = t.clips.iter().filter_map(|c| match &c.source { crate::model::Source::Text { text, .. } if !text.trim().is_empty() => Some(Cue { start: tk(c.start), end: tk(c.start + c.duration), text: text.trim().to_string(), speaker: None, id: None, settings: String::new() }), _ => None }).collect();
    cues.sort_by_key(|c| c.start.0);
    if cues.is_empty() { return Err(format!("track '{tid}' has no captions")); }
    let mut doc = Document::default(); doc.cues = cues;
    let fps = p.fps; let drop = a.get("drop_frame").and_then(|v| v.as_bool()).unwrap_or((fps - 29.97).abs() < 0.01 || (fps - 59.94).abs() < 0.01 || matches!(fmt, Format::Scc | Format::Mcc));
    let mut opts = WriteOptions::default(); opts.drop_frame = drop; opts.rate = Some(craft_captions::time::FrameRate::from_f64(fps.max(1.0)));
    let bytes = craft_captions::write(&doc, fmt, opts);
    std::fs::write(&path, &bytes).map_err(|e| format!("could not write {path}: {e}"))?;
    let mut notes: Vec<String> = vec![];
    if matches!(fmt, Format::Scc) { notes.push("SCC is CEA-608: 32 characters a line, at most 4 lines, Latin text; longer lines are wrapped or cut by the format".into()); }
    Ok(json!({"path": path, "format": name_of(fmt), "cues": doc.cues.len(), "bytes": bytes.len(), "drop_frame": drop, "notes": notes}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "import_captions", description: "Import a caption file as caption text clips on a track: SubRip .srt, WebVTT .vtt, Scenarist .scc (CEA-608 pop-on, roll-up and paint-on), MacCaption .mcc (CEA-608/708), EBU .stl, TTML/IMSC1 .ttml, .dfxp and ASS/SSA. Format detected from the content and extension. Style args as import_srt.", mutates: true,
        schema: obj(vec![("path", s("caption file")), ("format", json!({"type":"string","enum":["srt","vtt","scc","mcc","stl","ttml","dfxp","ass"]})), ("track_id", s("track for the captions (default: a new Captions track)")), ("size", n("text size px")), ("color", json!({"description":"text colour"})), ("font", s("font")), ("stroke_width", n("outline px")), ("y", n("vertical position"))], &["path"]),
        run: import_captions },
    Command { name: "export_captions_as", description: "Write a caption track (the text clips of track_id, a caption group, or the only text track) as SubRip, WebVTT, Scenarist SCC (CEA-608, drop-frame timecode), MacCaption MCC (608/708), EBU STL, TTML/IMSC1, DFXP or ASS. Format from the extension unless given.", mutates: false,
        schema: obj(vec![("path", s("output file")), ("format", json!({"type":"string","enum":["srt","vtt","scc","mcc","stl","ttml","dfxp","ass"]})), ("track_id", s("caption track")), ("caption_id", s("caption group")), ("drop_frame", json!({"type":"boolean","description":"SCC/MCC timecode (default: drop-frame)"})), ("overwrite", json!({"type":"boolean"}))], &["path"]),
        run: export_captions_as },
] }
