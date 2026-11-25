//! Timeline interchange with other editors through craft-libs' craft-timeline (MIT OR Apache-2.0, vendored in
//! third_party/craft-libs): export_timeline writes Final Cut Pro 7 XML (xmeml, what Premiere Pro imports and exports),
//! FCPXML 1.9-1.11 (Final Cut Pro, DaVinci Resolve) or OpenTimelineIO; import_timeline reads the same three and places
//! the edit on new kimo tracks. Time is exact on craft-timeline's ticks (every broadcast rate, 29.97 drop-frame).
//! Kimo's own layers with no equivalent (titles, shapes, effects) are listed in `notes`, never silently dropped.
use super::kit::*;
use crate::model::Source;
use craft_timeline::fx::Standard;
use craft_timeline::model::{
    AudioStreamInfo, Catalog, ClipId, ItemId, ItemKind, Label, Marker, MarkerId, MarkerKind, MediaClip, MediaInfo, MediaKind, MediaRef, Project as MProject, Sequence,
    SequenceSettings, Track as MTrack, TrackId, TrackItem, TrackKind, Transition, TransitionAlign, TransitionId, VideoStreamInfo,
};
use craft_timeline::time::{FrameRate, Tick, TICKS_PER_SECOND};
use craft_timeline::{fcp7, fcpxml, otio, ExportOptions, ImportOptions, Report};
use std::collections::HashMap;

const NS: &str = "kimo";
fn tk(s: f64) -> Tick { Tick((s * TICKS_PER_SECOND as f64).round() as i64) }
fn secs(t: Tick) -> f64 { t.0 as f64 / TICKS_PER_SECOND as f64 }
fn notes(r: &Report, out: &mut Vec<String>) { for e in &r.entries { out.push(if e.count > 1 { format!("{} (x{})", e.message, e.count) } else { e.message.clone() }); } }
fn is_still(path: &str) -> bool { crate::render::is_still_path(path) }

/// kimo's timeline as one craft-timeline sequence (picture on video tracks, sound on linked audio tracks).
fn to_model(p: &crate::model::Project, out: &mut Vec<String>) -> (MProject, ItemId) {
    let name = if p.name.is_empty() { "kimo" } else { p.name.as_str() };
    let mut m = MProject::new(name); let rate = FrameRate::from_f64(if p.fps > 0.0 { p.fps } else { 30.0 });
    let mut seq = Sequence::new(SequenceSettings { width: p.width, height: p.height, frame_rate: rate, ..Default::default() });
    let mut items: HashMap<String, ItemId> = HashMap::new();
    // kimo keeps a dissolve as an overlap: the outgoing clip runs on past the cut by the transition's length and the
    // incoming clip sits on a track above. Editors keep both on one track, cut at the incoming start, with the
    // transition using the outgoing clip's handle: move the incoming clip down and trim the outgoing one to the cut.
    let tol = 0.5 / p.fps.max(1.0);
    let mut home: HashMap<&str, usize> = HashMap::new(); let mut trim: HashMap<&str, f64> = HashMap::new();
    for (ti, tr) in p.tracks.iter().enumerate() { for c in tr.clips.iter() {
        let Some(t) = c.transition_in.as_ref().filter(|t| ["crossfade", "dissolve", "dip_black", "dip_white"].contains(&t.kind.as_str())) else { continue };
        let out_clip = p.tracks[..ti].iter().enumerate().rev().find_map(|(oi, ot)| ot.clips.iter().find(|o| o.start < c.start - tol && (o.start + o.duration - (c.start + t.duration)).abs() < tol && matches!(o.source, Source::Media { .. })).map(|o| (oi, o)));
        if let Some((oi, o)) = out_clip { home.insert(c.id.as_str(), oi); trim.insert(o.id.as_str(), c.start - o.start); }
    } }
    let placed: Vec<Vec<&crate::model::Clip>> = (0..p.tracks.len()).map(|ti| {
        let mut v: Vec<&crate::model::Clip> = p.tracks.iter().enumerate().flat_map(|(si, st)| st.clips.iter().map(move |c| (si, &**c))).filter(|(si, c)| home.get(c.id.as_str()).copied().unwrap_or(*si) == ti).map(|(_, c)| c).collect();
        v.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal)); v }).collect();
    for (ti, tr) in p.tracks.iter().enumerate() {
        let tname = if tr.name.is_empty() { format!("Track {}", ti + 1) } else { tr.name.clone() };
        let mut vt = MTrack::new(TrackId(m.alloc_id()), TrackKind::Video, tname.clone()); let mut at = MTrack::new(TrackId(m.alloc_id()), TrackKind::Audio, tname);
        let mut prev: Option<(f64, ClipId)> = None;
        for &c in &placed[ti] {
            let dur = trim.get(c.id.as_str()).copied().unwrap_or(c.duration);
            let label = c.name.clone().unwrap_or_else(|| c.id.clone());
            let (item, has_audio, has_video) = match &c.source {
                Source::Media { asset } => { let Some(a) = p.assets.get(asset) else { continue }; let path = p.resolve(&a.path); let still = is_still(&path);
                    let id = *items.entry(path.clone()).or_insert_with(|| {
                        let audio_only = a.width == 0 || a.height == 0;
                        let info = MediaInfo { name: std::path::Path::new(&path).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_else(|| path.clone()),
                            kind: if audio_only { MediaKind::AudioOnly } else if still { MediaKind::Still } else { MediaKind::Movie }, duration: tk(a.duration.max(c.in_point + c.duration * c.speed.abs())),
                            video: (!audio_only).then(|| VideoStreamInfo { width: a.width, height: a.height, frame_rate: FrameRate::from_f64(if a.fps > 0.0 { a.fps } else { p.fps.max(1.0) }), par: (1, 1) }),
                            audio_streams: if still { vec![] } else { vec![AudioStreamInfo { sample_rate: 48_000, channels: 2, bits_per_sample: None }] }, start_timecode: None };
                        let n = info.name.clone(); m.add_item(&n, Label::Iris, ItemKind::Media(Box::new(MediaClip { media: MediaRef::File { path: path.clone() }, info, interpret_frame_rate: None, markers: vec![], offline: false })), None) });
                    let audio_only = c.props.get("audio_only").map_or(false, |q| q.base >= 0.5) || a.width == 0;
                    (id, !still, !audio_only) }
                other => {
                    let what = serde_json::to_value(other).ok().and_then(|v| v.get("type").and_then(|t| t.as_str()).map(String::from)).unwrap_or_else(|| "layer".into());
                    out.push(format!("{what} \"{label}\" at {:.2} s has no equivalent in the document; it is written as a placeholder", c.start));
                    let id = m.add_item(&label, Label::Lavender, ItemKind::Uncarried { what, width: p.width, height: p.height, rate, duration: tk(dur) }, None); (id, false, true) }
            };
            if !c.effects.iter().all(|e| e.kind.starts_with("shape_")) { out.push(format!("\"{label}\": its kimo effects are not written (other editors would not know them)")); }
            let link = (has_audio && has_video).then(|| m.alloc_id());
            let reverse = matches!(c.time_mode, crate::model::TimeMode::Reverse);
            let gain = c.audio.volume.base; let gain_db = if gain > 1e-6 { 20.0 * gain.log10() } else { -96.0 };
            let mk = |m: &mut MProject, kind: TrackKind| TrackItem { id: ClipId(m.alloc_id()), item, name: label.clone(), label: Label::Iris, start: tk(c.start), duration: tk(dur), source_in: tk(c.in_point),
                speed: c.speed.abs().max(1e-6), reverse, enabled: true, link, group: None, effects: Standard.intrinsic(kind), markers: vec![], gain_db: if kind == TrackKind::Audio { gain_db } else { 0.0 },
                frame_hold: None, scale_to_frame: false, source_channels: vec![], ext: None };
            if has_video {
                let ti = mk(&mut m, TrackKind::Video); let vid = ti.id;
                if let Some(tr) = &c.transition_in {
                    let fx = match tr.kind.as_str() { "crossfade" | "dissolve" => Some("cross_dissolve"), "dip_black" => Some("dip_to_black"), "dip_white" => Some("dip_to_white"), _ => None };
                    match fx.and_then(|f| Standard.effect(f)) {
                        Some(def) => vt.transitions.push(Transition { id: TransitionId(m.alloc_id()), effect: def.instance(), start: tk(c.start), duration: tk(tr.duration),
                            from: prev.filter(|(end, _)| (end - c.start).abs() < 0.5 / p.fps.max(1.0)).map(|(_, id)| id), to: Some(vid), align: TransitionAlign::StartAtCut, reverse: false, ext: None }),
                        None => out.push(format!("transition '{}' into \"{label}\" is written as a cut", tr.kind)),
                    } }
                prev = Some((c.start + dur, vid)); vt.items.push(ti);
            }
            if has_audio { let ai = mk(&mut m, TrackKind::Audio); at.items.push(ai); }
        }
        if !vt.items.is_empty() { vt.sort(); seq.video_tracks.push(vt); }
        if !at.items.is_empty() { at.sort(); seq.audio_tracks.push(at); }
    }
    for mk in &p.markers { seq.markers.push(Marker { id: MarkerId(m.alloc_id()), start: tk(mk.time), duration: tk(mk.duration), name: mk.name.clone(), comment: String::new(), kind: MarkerKind::Comment, color: Label::Forest, url: String::new(), author: String::new(), ext: None }); }
    let sid = m.add_item(name, Label::Forest, ItemKind::Sequence(Box::new(seq)), None);
    (m, sid)
}

fn export_timeline(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?.to_string();
    let fmt = a.get("format").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| { let l = path.to_lowercase(); if l.ends_with(".fcpxml") { "fcpxml".into() } else if l.ends_with(".otio") { "otio".into() } else { "fcp7".into() } });
    if std::path::Path::new(&path).exists() && !a.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false) { return Err(format!("{path} exists; pass overwrite: true to replace it")); }
    if ss.project.tracks.iter().all(|t| t.clips.is_empty()) { return Err("the timeline is empty".into()); }
    let mut out = vec![]; let (m, sid) = to_model(&ss.project, &mut out); let mut rep = Report::default();
    let rel = std::path::Path::new(&path).parent().map(|d| d.to_string_lossy().to_string()); let relative = a.get("relative_paths").and_then(|v| v.as_bool()).unwrap_or(false);
    let opts = ExportOptions { relative_to: if relative { rel.as_deref() } else { None }, name: None, catalog: &Standard, namespace: NS, xmeml_version: 4, fcpxml_version: Default::default() };
    let text = match fmt.as_str() { "fcp7" | "xml" | "premiere" => fcp7::export(&m, sid, &opts, &mut rep), "fcpxml" => fcpxml::export(&m, sid, &opts, &mut rep), "otio" => otio::export(&m, sid, &opts, &mut rep),
        o => return Err(format!("format must be fcp7 (Premiere Pro XML), fcpxml or otio, got '{o}' (CMX 3600 EDL: export_edl)")) }.map_err(|e| format!("export failed: {e}"))?;
    notes(&rep, &mut out);
    std::fs::write(&path, &text).map_err(|e| format!("could not write {path}: {e}"))?;
    Ok(json!({"path": path, "format": fmt, "bytes": text.len(), "notes": out}))
}

fn import_timeline(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?.to_string();
    let bytes = std::fs::read(&path).map_err(|e| format!("could not read {path}: {e}"))?; let text = craft_timeline::decode_text(&bytes);
    let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_lowercase());
    let fmt = match a.get("format").and_then(|v| v.as_str()) { Some(f) => f.to_string(), None => match craft_timeline::detect_text(&bytes, ext.as_deref()) {
        Some(craft_timeline::Format::Fcp7Xml) => "fcp7".into(), Some(craft_timeline::Format::Fcpxml) => "fcpxml".into(), Some(craft_timeline::Format::Otio) => "otio".into(),
        Some(craft_timeline::Format::Edl) => return Err("this is a CMX 3600 EDL: use import_edl".into()), _ => return Err("not a Premiere/FCP7 XML, FCPXML or OpenTimelineIO document".into()) } };
    let base = std::path::Path::new(&path).parent().map(|d| d.to_string_lossy().to_string());
    let opts = ImportOptions { base_dir: base.as_deref(), name: None, catalog: &Standard, namespace: NS }; let mut rep = Report::default();
    let imp = match fmt.as_str() { "fcp7" | "xml" | "premiere" => fcp7::import(&text, &opts, &mut rep), "fcpxml" => fcpxml::import(&text, &opts, &mut rep), "otio" => otio::import(&text, &opts, &mut rep),
        o => return Err(format!("format must be fcp7, fcpxml or otio, got '{o}'")) }.map_err(|e| format!("could not read the {fmt} document: {e}"))?;
    let mut out = vec![]; notes(&rep, &mut out);
    let which = a.get("sequence").and_then(|v| v.as_str());
    let sid = imp.sequences.iter().copied().find(|s| which.map_or(true, |w| imp.project.item(*s).map_or(false, |i| i.name == w))).ok_or_else(|| format!("no sequence{} in the document", which.map(|w| format!(" named '{w}'")).unwrap_or_default()))?;
    let m = &imp.project; let seq = m.sequence(sid).ok_or("the sequence is missing")?; let offset = opt_num(a, "start", 0.0);
    let mut assets: HashMap<String, Option<String>> = HashMap::new(); let mut placed_links = std::collections::HashSet::new(); let (mut n_clips, mut tracks) = (0usize, vec![]);
    let mut asset_for = |ss: &mut Session, item: ItemId, out: &mut Vec<String>| -> Option<String> {
        let (_, mc, _) = m.resolve_media(item)?; let MediaRef::File { path } = &mc.media else { out.push(format!("generator \"{}\" is not imported", mc.info.name)); return None };
        assets.entry(path.clone()).or_insert_with(|| match crate::commands::dispatch_nested(ss, "import_media", &json!({"path": path})) { Ok(r) => r["asset_id"].as_str().map(String::from), Err(e) => { out.push(format!("media {path} is offline or unreadable ({e}); its clips are skipped")); None } }).clone()
    };
    for vt in &seq.video_tracks {
        let tid = crate::commands::dispatch_nested(ss, "add_track", &json!({"name": vt.name}))?["track_id"].as_str().unwrap_or_default().to_string(); tracks.push(tid.clone());
        let mut kimo_id: HashMap<ClipId, String> = HashMap::new();
        for it in &vt.items {
            if !it.enabled { continue; }
            let Some(asset) = asset_for(ss, it.item, &mut out) else { if !matches!(m.item(it.item).map(|i| &i.kind), Some(ItemKind::Media(_)) | Some(ItemKind::Subclip { .. })) { out.push(format!("\"{}\" ({}) is not media; skipped", it.name, m.item(it.item).map(|i| match &i.kind { ItemKind::Sequence(_) => "nested sequence", _ => "generator or title" }).unwrap_or("item"))); } continue };
            let r = crate::commands::dispatch_nested(ss, "add_clip", &json!({"track_id": tid, "source_type": "media", "asset_id": asset, "start": secs(it.start) + offset, "duration": secs(it.duration), "in_point": secs(it.source_in), "speed": it.speed}))?;
            let cid = r["clip_id"].as_str().unwrap_or_default().to_string(); n_clips += 1;
            if it.reverse { let _ = crate::commands::dispatch_nested(ss, "set_time_mode", &json!({"clip_id": cid, "mode": "reverse"})); }
            if let Some(l) = it.link { placed_links.insert(l); }
            let extra: Vec<&str> = it.effects.iter().filter(|e| !Standard.effect(&e.effect).map_or(false, |d| d.intrinsic)).map(|e| e.effect.as_str()).collect();
            if !extra.is_empty() { out.push(format!("\"{}\": effects not carried over: {}", it.name, extra.join(", "))); }
            kimo_id.insert(it.id, cid);
        }
        for t in &vt.transitions {
            let kind = match t.effect.effect.as_str() { "cross_dissolve" | "film_dissolve" | "additive_dissolve" | "non_additive_dissolve" => "crossfade", "dip_to_black" => "dip_black", "dip_to_white" => "dip_white", o => { out.push(format!("transition '{o}' became a crossfade")); "crossfade" } };
            if let Some(cid) = t.to.and_then(|c| kimo_id.get(&c)) { let _ = crate::commands::dispatch_nested(ss, "add_transition", &json!({"clip_id": cid, "kind": kind, "duration": secs(t.duration)})); }
        }
    }
    for at in &seq.audio_tracks {
        let mut tid: Option<String> = None;
        for it in &at.items {
            if !it.enabled || it.link.map_or(false, |l| placed_links.contains(&l)) { continue; } // its sound plays with the linked video clip
            let Some(asset) = asset_for(ss, it.item, &mut out) else { continue };
            if tid.is_none() { tid = crate::commands::dispatch_nested(ss, "add_track", &json!({"name": at.name}))?["track_id"].as_str().map(String::from); tracks.extend(tid.clone()); }
            crate::commands::dispatch_nested(ss, "add_audio_clip", &json!({"asset_id": asset, "track_id": tid, "start": secs(it.start) + offset, "duration": secs(it.duration), "in_point": secs(it.source_in), "volume": 10f64.powf(it.gain_db / 20.0)}))?; n_clips += 1;
        }
    }
    for mk in &seq.markers { let _ = crate::commands::dispatch_nested(ss, "add_marker", &json!({"time": secs(mk.start) + offset, "name": if mk.name.is_empty() { &mk.comment } else { &mk.name }, "duration": secs(mk.duration)})); }
    Ok(json!({"format": fmt, "sequence": m.item(sid).map(|i| i.name.clone()), "tracks": tracks, "clips": n_clips, "markers": seq.markers.len(), "notes": out}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "export_timeline", description: "Export the edit for another editor: Final Cut Pro 7 XML (format fcp7; what Premiere Pro imports), FCPXML 1.9-1.11 (Final Cut Pro, DaVinci Resolve) or OpenTimelineIO. Media clips with their in points, speed and reverse, linked sound, volume, crossfades and dips, markers. Titles, shapes and kimo effects have no equivalent and are listed in notes. Format from the extension (.xml, .fcpxml, .otio) unless given. For CMX 3600 EDL use export_edl.", mutates: false,
        schema: obj(vec![("path", s("file to write")), ("format", json!({"type":"string","enum":["fcp7","fcpxml","otio"]})), ("overwrite", json!({"type":"boolean"})), ("relative_paths", json!({"type":"boolean","description":"write media paths relative to the file where the format allows"}))], &["path"]),
        run: export_timeline },
    Command { name: "import_timeline", description: "Import an edit from another editor: Premiere Pro / Final Cut Pro 7 XML, FCPXML (Final Cut Pro, DaVinci Resolve) or OpenTimelineIO. Each video track becomes a kimo track of media clips (in points, speed, reverse), sound that is not linked to a picture goes on audio tracks, dissolves and dips become transitions, markers are added. Media is imported from the paths in the document (relative to it); offline media, nested sequences, generators and effects are listed in notes.", mutates: true,
        schema: obj(vec![("path", s(".xml, .fcpxml or .otio file")), ("format", json!({"type":"string","enum":["fcp7","fcpxml","otio"],"description":"default: detected"})), ("sequence", s("sequence name when the document has several (default: the first)")), ("start", n("timeline seconds to place it at, default 0"))], &["path"]),
        run: import_timeline },
] }
