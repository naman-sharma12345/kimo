//! AAF (Avid Media Composer, Pro Tools, Premiere, Resolve) and OMF 2.0 (audio post) through craft-libs' craft-aaf
//! (MIT OR Apache-2.0, vendored in third_party/craft-libs). export_aaf writes the edit (picture and sound tracks, clip
//! gain, dissolves, markers; OMF is sound only) with the media referenced by path; import_aaf places a document's
//! first composition on new kimo tracks.
use super::kit::*;
use crate::model::Source as KSource;
use craft_aaf::model::{CClip, CItem, CKind, CMarker, CTrack, CTransition, Composition, Document, Gain, Source};
use craft_timeline::time::{FrameRate, Tick, TICKS_PER_SECOND};
use craft_timeline::Report;
use std::collections::HashMap;

fn tk(s: f64) -> Tick { Tick((s * TICKS_PER_SECOND as f64).round() as i64) }
fn secs(t: Tick) -> f64 { t.0 as f64 / TICKS_PER_SECOND as f64 }
fn notes(r: &Report, out: &mut Vec<String>) { for e in &r.entries { out.push(if e.count > 1 { format!("{} (x{})", e.message, e.count) } else { e.message.clone() }); } }
const ID: craft_aaf::Identity<'static> = craft_aaf::Identity { company: "kimo", product: "kimo", version: env!("CARGO_PKG_VERSION"), platform: std::env::consts::OS,
    product_id: [0x6b, 0x69, 0x6d, 0x6f, 0x2d, 0x61, 0x61, 0x66, 0x2d, 0x65, 0x78, 0x70, 0x6f, 0x72, 0x74, 0x31] };

fn to_doc(p: &crate::model::Project, out: &mut Vec<String>) -> Document {
    let rate = FrameRate::from_f64(if p.fps > 0.0 { p.fps } else { 30.0 });
    let mut doc = Document::default(); let mut src: HashMap<(String, bool), usize> = HashMap::new();
    let mut source = |doc: &mut Document, path: &str, a: &crate::model::Asset, pic: bool| -> usize {
        *src.entry((path.to_string(), pic)).or_insert_with(|| { doc.sources.push(Source { key: path.to_string(), name: std::path::Path::new(path).file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default(),
            kind: if pic { CKind::Picture } else { CKind::Sound }, path: Some(path.to_string()), channel: None, channels: if pic { 0 } else { 2 }, file_channels: if pic { 0 } else { 2 },
            width: a.width, height: a.height, frame_rate: FrameRate::from_f64(if a.fps > 0.0 { a.fps } else { p.fps.max(1.0) }), sample_rate: 48_000, bits: 16,
            length: if crate::render::is_still_path(path) && pic { Tick::ZERO } else { tk(a.duration) }, offset: Tick::ZERO, start_tc: None, tc_rate: rate, embedded: None, markers: vec![], nested: None }); doc.sources.len() - 1 })
    };
    // kimo keeps a dissolve as an overlap (outgoing clip runs past the cut on its track, incoming clip on a track above);
    // AAF keeps both on one track with the outgoing segment overlapping the transition. Put the incoming clip on the
    // outgoing clip's track: the outgoing length already includes the overlap.
    let tol = 0.5 / p.fps.max(1.0); let mut home: HashMap<&str, usize> = HashMap::new();
    for (ti, tr) in p.tracks.iter().enumerate() { for c in tr.clips.iter() {
        let Some(t) = c.transition_in.as_ref().filter(|t| ["crossfade", "dissolve", "dip_black", "dip_white"].contains(&t.kind.as_str())) else { continue };
        if let Some(oi) = p.tracks[..ti].iter().enumerate().rev().find_map(|(oi, ot)| ot.clips.iter().any(|o| o.start < c.start - tol && (o.start + o.duration - (c.start + t.duration)).abs() < tol).then_some(oi)) { home.insert(c.id.as_str(), oi); }
    } }
    let (mut vn, mut an) = (0u32, 0u32); let mut comp = Composition { name: if p.name.is_empty() { "kimo".into() } else { p.name.clone() }, aaf_mob_id: None, rate, sample_rate: 48_000, width: p.width, height: p.height, start_tc: 0, drop: false, tracks: vec![], markers: vec![] };
    for ti in 0..p.tracks.len() {
        let mut clips: Vec<&crate::model::Clip> = p.tracks.iter().enumerate().flat_map(|(si, st)| st.clips.iter().map(move |c| (si, &**c))).filter(|(si, c)| home.get(c.id.as_str()).copied().unwrap_or(*si) == ti).map(|(_, c)| c).collect();
        clips.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
        let mut pic = CTrack { kind: CKind::Picture, name: p.tracks[ti].name.clone(), number: 0, channels: 0, items: vec![] }; let mut snd = CTrack { kind: CKind::Sound, name: p.tracks[ti].name.clone(), number: 0, channels: 2, items: vec![] };
        let (mut tp, mut ts) = (0.0f64, 0.0f64);
        for c in clips {
            let KSource::Media { asset } = &c.source else { out.push(format!("\"{}\" is not media; AAF and OMF carry media clips only", c.name.clone().unwrap_or_else(|| c.id.clone()))); continue };
            let Some(a) = p.assets.get(asset) else { continue }; let path = p.resolve(&a.path); let audio_only = c.props.get("audio_only").map_or(false, |q| q.base >= 0.5) || a.width == 0; let still = crate::render::is_still_path(&path);
            let name = c.name.clone().unwrap_or_else(|| c.id.clone()); let vol = c.audio.volume.base;
            let tr_in = c.transition_in.as_ref().filter(|t| home.contains_key(c.id.as_str()) || ["crossfade", "dissolve", "dip_black", "dip_white"].contains(&t.kind.as_str())).filter(|t| t.duration > 0.0);
            let mut place = |trk: &mut CTrack, cursor: &mut f64, s: usize, kind_pic: bool| {
                let mut start = c.start;
                if let Some(t) = tr_in.filter(|_| *cursor > c.start + tol) { // overlaps the previous segment by the transition
                    let d = t.duration; trk.items.push(CItem::Transition(CTransition { len: tk(d), cut: Tick::ZERO, effect: if kind_pic { if t.kind == "dip_black" { "dip_to_black".into() } else { "cross_dissolve".into() } } else { "constant_power".into() } }));
                    start = c.start; *cursor = c.start; }
                else if start > *cursor + tol { trk.items.push(CItem::Filler(tk(start - *cursor))); *cursor = start; }
                trk.items.push(CItem::Clip(CClip { len: tk(c.duration), source: s, start: tk(c.in_point), gain: (!kind_pic && (vol - 1.0).abs() > 1e-6).then(|| Gain::Constant(vol)), name: name.clone() }));
                *cursor = start + c.duration;
            };
            if !audio_only { let s = source(&mut doc, &path, a, true); place(&mut pic, &mut tp, s, true); }
            if !still { let s = source(&mut doc, &path, a, false); place(&mut snd, &mut ts, s, false); }
            if (c.speed - 1.0).abs() > 1e-6 { out.push(format!("\"{name}\": speed {} is written at normal speed (AAF motion effects are not written)", c.speed)); }
        }
        if !pic.items.is_empty() { vn += 1; pic.number = vn; comp.tracks.push(pic); }
        if !snd.items.is_empty() { an += 1; snd.number = an; comp.tracks.push(snd); }
    }
    for m in &p.markers { comp.markers.push(CMarker { start: tk(m.time), duration: tk(m.duration), name: m.name.clone(), comment: String::new(), color: None }); }
    doc.compositions.push(comp); doc
}

fn export_aaf(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?.to_string(); let lower = path.to_lowercase();
    let fmt = a.get("format").and_then(|v| v.as_str()).map(String::from).unwrap_or_else(|| if lower.ends_with(".omf") { "omf".into() } else { "aaf".into() });
    if std::path::Path::new(&path).exists() && !a.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false) { return Err(format!("{path} exists; pass overwrite: true to replace it")); }
    let mut out = vec![]; let mut doc = to_doc(&ss.project, &mut out);
    if fmt == "omf" { let n = doc.compositions[0].tracks.len(); doc.compositions[0].tracks.retain(|t| t.kind == CKind::Sound); if doc.compositions[0].tracks.len() < n { out.push("OMF carries sound only: the picture tracks are not written".into()); } }
    if doc.compositions[0].tracks.is_empty() { return Err("no media clips to export".into()); }
    let mut rep = Report::default();
    let bytes = match fmt.as_str() { "aaf" => craft_aaf::aaf::write(&doc, &craft_aaf::aaf::WriteOptions { identity: ID, small_sectors: false }, &mut rep), "omf" => craft_aaf::omf::write(&doc, &ID, &mut rep), o => return Err(format!("format must be aaf or omf, got '{o}'")) }.map_err(|e| format!("{fmt} export failed: {e}"))?;
    notes(&rep, &mut out);
    std::fs::write(&path, &bytes).map_err(|e| format!("could not write {path}: {e}"))?;
    let c = &doc.compositions[0];
    Ok(json!({"path": path, "format": fmt, "bytes": bytes.len(), "picture_tracks": c.tracks.iter().filter(|t| t.kind == CKind::Picture).count(), "sound_tracks": c.tracks.iter().filter(|t| t.kind == CKind::Sound).count(), "sources": doc.sources.len(), "notes": out}))
}

fn import_aaf(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?.to_string(); let bytes = std::fs::read(&path).map_err(|e| format!("could not read {path}: {e}"))?;
    let mut rep = Report::default(); let omf = path.to_lowercase().ends_with(".omf") || bytes.starts_with(b"\xa4\x43\x4d\xa5") || !bytes.starts_with(&[0xd0, 0xcf, 0x11, 0xe0]);
    let doc = if omf { craft_aaf::omf::read(&bytes, &mut rep) } else { craft_aaf::aaf::read(&bytes, &mut rep) }.map_err(|e| format!("could not read the {} file: {e}", if omf { "OMF" } else { "AAF" }))?;
    let mut out = vec![]; notes(&rep, &mut out);
    let comp = doc.compositions.first().ok_or("the document has no composition")?; let offset = opt_num(a, "start", 0.0); let base = std::path::Path::new(&path).parent().map(|d| d.to_path_buf());
    let mut assets: HashMap<usize, Option<String>> = HashMap::new(); let mut pictures: Vec<(String, f64)> = vec![]; let (mut tracks, mut n) = (vec![], 0usize);
    let mut asset_of = |ss: &mut Session, i: usize, out: &mut Vec<String>| -> Option<String> {
        assets.entry(i).or_insert_with(|| { let s = doc.sources.get(i)?; if s.nested.is_some() { out.push(format!("nested sequence \"{}\" is not imported", s.name)); return None; }
            let Some(p) = s.path.clone() else { out.push(format!("\"{}\": embedded media is not imported", s.name)); return None };
            let p = if std::path::Path::new(&p).exists() { p } else { base.as_ref().map(|b| b.join(&p).to_string_lossy().to_string()).filter(|q| std::path::Path::new(q).exists()).unwrap_or(p) };
            match crate::commands::dispatch_nested(ss, "import_media", &json!({"path": p})) { Ok(r) => r["asset_id"].as_str().map(String::from), Err(e) => { out.push(format!("media {p} is offline ({e}); its clips are skipped")); None } } }).clone()
    };
    for kind in [CKind::Picture, CKind::Sound] { for t in comp.tracks.iter().filter(|t| t.kind == kind) {
        // place segments: a transition overlaps the segments around it (the outgoing one runs on under it)
        let mut placed: Vec<(f64, f64, &CClip, Option<(f64, String)>)> = vec![]; let mut cur = 0.0; let mut pending: Option<(f64, String)> = None;
        for it in &t.items { match it {
            CItem::Filler(l) => cur += secs(*l),
            CItem::Transition(tr) => { let d = secs(tr.len); if let Some(last) = placed.last_mut() { if last.1 > d { last.1 -= d; } } cur -= d; pending = Some((d, tr.effect.clone())); }
            CItem::Clip(c) => { placed.push((cur, secs(c.len), c, pending.take())); cur += secs(c.len); }
        } }
        let mut tid: Option<String> = None; let mut made: Vec<(String, Option<(f64, String)>)> = vec![];
        for (start, len, c, tr) in placed {
            let s = doc.sources.get(c.source); let key = s.map(|s| s.key.clone()).unwrap_or_default();
            if kind == CKind::Sound && pictures.iter().any(|(k, t0)| *k == key && (t0 - start).abs() < 0.02) { continue; } // its sound plays with the picture clip
            let Some(asset) = asset_of(ss, c.source, &mut out) else { continue };
            if tid.is_none() { tid = crate::commands::dispatch_nested(ss, "add_track", &json!({"name": t.name}))?["track_id"].as_str().map(String::from); tracks.extend(tid.clone()); }
            let src_in = secs(c.start - s.map_or(Tick::ZERO, |s| s.offset)).max(0.0);
            let r = if kind == CKind::Picture { pictures.push((key, start)); crate::commands::dispatch_nested(ss, "add_clip", &json!({"track_id": tid, "source_type": "media", "asset_id": asset, "start": start + offset, "duration": len, "in_point": src_in}))? }
                else { let vol = match &c.gain { Some(Gain::Constant(g)) => *g, _ => 1.0 }; crate::commands::dispatch_nested(ss, "add_audio_clip", &json!({"asset_id": asset, "track_id": tid, "start": start + offset, "duration": len, "in_point": src_in, "volume": vol}))? };
            if matches!(c.gain, Some(Gain::Varying { .. })) { out.push(format!("\"{}\": gain automation imported as its level only", c.name)); }
            n += 1; made.push((r["clip_id"].as_str().unwrap_or_default().to_string(), tr));
        }
        for (cid, tr) in made { if let Some((d, eff)) = tr { if kind == CKind::Picture { let k = if eff.contains("black") { "dip_black" } else { "crossfade" }; let _ = crate::commands::dispatch_nested(ss, "add_transition", &json!({"clip_id": cid, "kind": k, "duration": d})); } } }
    } }
    for m in &comp.markers { let _ = crate::commands::dispatch_nested(ss, "add_marker", &json!({"time": secs(m.start) + offset, "name": if m.name.is_empty() { &m.comment } else { &m.name }, "duration": secs(m.duration)})); }
    Ok(json!({"format": if omf { "omf" } else { "aaf" }, "composition": comp.name, "tracks": tracks, "clips": n, "markers": comp.markers.len(), "notes": out}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "export_aaf", description: "Export the edit as AAF (Edit Protocol: Avid Media Composer, Pro Tools, Premiere, Resolve) or OMF 2.0 (sound only, for audio post). Media clips with in points, linked sound, clip gain, dissolves and markers; media referenced by path. Format from the extension (.aaf/.omf) unless given.", mutates: false,
        schema: obj(vec![("path", s(".aaf or .omf file")), ("format", json!({"type":"string","enum":["aaf","omf"]})), ("overwrite", json!({"type":"boolean"}))], &["path"]),
        run: export_aaf },
    Command { name: "import_aaf", description: "Import an AAF or OMF document's first composition onto new kimo tracks: picture clips (with their linked sound), unlinked sound on audio tracks with clip gain, dissolves, markers. Media found by path (or next to the document); embedded media, nested sequences and effects are listed in notes.", mutates: true,
        schema: obj(vec![("path", s(".aaf or .omf file")), ("start", n("timeline seconds to place it at, default 0"))], &["path"]),
        run: import_aaf },
] }
