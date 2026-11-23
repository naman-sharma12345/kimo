//! separate_stems: vocals and instrumental of a song (or of any clip's audio), as two new audio assets, optionally laid on
//! the timeline in sync with the clip. On-device Spleeter 2-stem models (vision/stems.rs), fetched on first use.
//! The vocal stem is what lyric timing and "music under speech" work need: caption_from_words / auto_captions on the
//! vocals asset align or transcribe lyrics without the beat in the way.
use super::kit::*;

fn write_wav_f32(path: &std::path::Path, stereo: &[f32], sr: u32) -> Result<(), String> {
    use std::io::Write;
    let data = (stereo.len() * 4) as u32; let mut f = std::io::BufWriter::new(std::fs::File::create(path).map_err(|e| e.to_string())?);
    let mut h = Vec::with_capacity(44);
    h.extend_from_slice(b"RIFF"); h.extend_from_slice(&(36 + data).to_le_bytes()); h.extend_from_slice(b"WAVEfmt ");
    h.extend_from_slice(&16u32.to_le_bytes()); h.extend_from_slice(&3u16.to_le_bytes()); h.extend_from_slice(&2u16.to_le_bytes());
    h.extend_from_slice(&sr.to_le_bytes()); h.extend_from_slice(&(sr * 8).to_le_bytes()); h.extend_from_slice(&8u16.to_le_bytes()); h.extend_from_slice(&32u16.to_le_bytes());
    h.extend_from_slice(b"data"); h.extend_from_slice(&data.to_le_bytes());
    f.write_all(&h).map_err(|e| e.to_string())?; for v in stereo { f.write_all(&v.to_le_bytes()).map_err(|e| e.to_string())?; } f.flush().map_err(|e| e.to_string())
}

#[cfg(feature = "vision")]
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let clip = a.get("clip_id").and_then(|v| v.as_str()).map(String::from);
    let (path, in0, dur, place) = if let Some(cid) = &clip {
        let c = ss.project.find_clip(cid).ok_or_else(|| format!("no clip '{cid}'"))?.clone();
        let crate::model::Source::Media { asset } = &c.source else { return Err(format!("{cid} is not a media clip")) };
        let p = ss.project.assets.get(asset).map(|x| x.path.clone()).ok_or("the clip's asset is missing")?;
        (p, c.in_point, c.duration * c.speed.abs().max(1e-6), Some((c.start, c.duration)))
    } else {
        let p = if let Some(id) = a.get("asset_id").and_then(|v| v.as_str()) { ss.project.assets.get(id).map(|x| x.path.clone()).ok_or_else(|| format!("no asset '{id}'"))? } else { str_arg(a, "path").map_err(|_| "give clip_id, asset_id or path".to_string())?.to_string() };
        (p, opt_num(a, "start", 0.0).max(0.0), opt_num(a, "duration", 600.0), None)
    };
    let dur = dur.clamp(1.0, 900.0);
    let sr = crate::vision::stems::SR;
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-v", "error", "-ss", &format!("{in0}"), "-t", &format!("{dur}"), "-i", &path, "-vn", "-ac", "2", "-ar", &sr.to_string(), "-f", "f32le", "-"]);
    let raw = crate::color::process::decode_capture(&mut cmd, ((dur * sr as f64) as usize + sr) * 8, std::time::Duration::from_secs(300))?;
    let pcm: Vec<f32> = raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
    if pcm.len() < sr { return Err("no audio to separate".into()); }
    let t0 = std::time::Instant::now();
    let (voc, acc) = crate::vision::stems::separate(&pcm)?;
    let stem = std::path::Path::new(&path).file_stem().and_then(|s| s.to_str()).unwrap_or("audio").to_string();
    let dir = a.get("dir").and_then(|v| v.as_str()).map(std::path::PathBuf::from).unwrap_or_else(|| std::env::temp_dir().join("kimo_stems"));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let tag = format!("{stem}_{}_{}", (in0 * 1000.0).round() as u64, (dur * 1000.0).round() as u64);
    let (pv, pa) = (dir.join(format!("{tag}_vocals.wav")), dir.join(format!("{tag}_instrumental.wav")));
    write_wav_f32(&pv, &voc, sr as u32)?; write_wav_f32(&pa, &acc, sr as u32)?;
    let mut ids = vec![];
    for p in [&pv, &pa] { ids.push(crate::commands::dispatch_nested(ss, "import_media", &json!({"path": p.to_string_lossy()}))?["asset_id"].as_str().unwrap_or("").to_string()); }
    let mut out = json!({"vocals": {"asset_id": ids[0], "path": pv.to_string_lossy()}, "instrumental": {"asset_id": ids[1], "path": pa.to_string_lossy()}, "seconds_of_audio": (dur * 100.0).round() / 100.0, "took_s": (t0.elapsed().as_secs_f64() * 10.0).round() / 10.0,
        "note": "lyrics: caption_from_words or auto_captions on the vocals asset; a cleaner music bed: the instrumental"});
    if let (Some((start, cdur)), true) = (place, a.get("add_to_timeline").and_then(|v| v.as_bool()).unwrap_or(true)) {
        let mut placed = serde_json::Map::new();
        for (name, id) in [("vocals", &ids[0]), ("instrumental", &ids[1])] {
            let t = crate::commands::dispatch_nested(ss, "add_track", &json!({"name": format!("Stem: {name}")}))?["track_id"].as_str().unwrap_or("").to_string();
            let c = crate::commands::dispatch_nested(ss, "add_audio_clip", &json!({"asset_id": id, "track_id": t, "start": start, "duration": cdur}))?;
            placed.insert(name.into(), json!({"track_id": t, "clip_id": c["clip_id"]}));
        }
        if a.get("mute_original").and_then(|v| v.as_bool()).unwrap_or(true) { if let Some(c) = clip.as_ref().and_then(|id| ss.project.find_clip_mut(id)) { c.audio.mute = true; } }
        out["placed"] = Value::Object(placed);
    }
    Ok(out)
}
#[cfg(not(feature = "vision"))]
fn run(_ss: &mut Session, _a: &Value) -> Result<Value, String> { Err("this build has no on-device models (the vision feature is off)".into()) }

pub fn commands() -> Vec<Command> {
    vec![Command { name: "separate_stems", description: "Split a song (or any clip's audio) into vocals and instrumental with on-device Spleeter models (about 40 MB, fetched on first use). With clip_id the two stems go on new tracks in sync with the clip and the clip's own audio is muted (mute_original:false keeps it). Then: lyric timing on the vocals (caption_from_words / auto_captions), or the instrumental as a bed under dialogue.", mutates: true,
        schema: obj(vec![("clip_id", s("clip whose audio to split")), ("asset_id", s("or an asset")), ("path", s("or a file")), ("start", n("seconds into the asset/file, default 0")), ("duration", n("seconds, default the whole clip / 600; max 900")),
            ("add_to_timeline", json!({"type":"boolean","description":"with clip_id: place both stems in sync (default true)"})), ("mute_original", json!({"type":"boolean","description":"mute the clip's own audio after placing the stems (default true)"})), ("dir", s("folder for the WAVs (default: temp)"))], &[]),
        run }]
}
