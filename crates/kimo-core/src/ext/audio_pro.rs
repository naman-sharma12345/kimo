//! Audio finishing on FilmCraft's audio engine (crates/kimo-fc-audio): loudness_report (ITU-R BS.1770-4 / EBU R128:
//! integrated, short-term and momentary maxima, loudness range, true peak, against delivery targets), remix_music
//! (retime a music clip to a length by cutting at musically similar beats), set_track_mix (a mixer strip per track:
//! gain, balance and insert effects) and export_audio_as (WAV, FLAC, MP3, AAC, Opus; stereo or 5.1; optional
//! loudness normalisation with a true-peak ceiling).
use super::kit::*;
use crate::model::Source;
use kimo_fc_audio::channels::{convert, Layout, Mixdown};
use kimo_fc_audio::LoudnessMeter;

const SR: usize = crate::audio::SR;
const TARGETS: &[(&str, f64, f64)] = &[("youtube", -14.0, -1.0), ("spotify", -14.0, -1.0), ("apple_music", -16.0, -1.0), ("podcast", -16.0, -1.0), ("tiktok", -14.0, -1.0), ("broadcast_ebu", -23.0, -1.0), ("broadcast_atsc", -24.0, -2.0), ("netflix", -27.0, -2.0)];

fn measure(pcm: &[f32]) -> LoudnessMeter { let mut m = LoudnessMeter::new(SR as f64, 2); for chunk in pcm.chunks(48000 * 2) { m.process_interleaved(chunk); } m }
fn r1(v: f64) -> Value { if v.is_finite() { json!((v * 10.0).round() / 10.0) } else { Value::Null } }

fn loudness_report(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let p = &ss.project; let mut pcm = crate::audio::mix(p);
    let (t0, t1) = (opt_num(a, "start", 0.0).max(0.0), opt_num(a, "end", p.duration()));
    let (i0, i1) = (((t0 * SR as f64) as usize * 2).min(pcm.len()), ((t1 * SR as f64) as usize * 2).min(pcm.len())); pcm = pcm[i0..i1.max(i0)].to_vec();
    if pcm.is_empty() { return Err("no audio in that range".into()); }
    let m = measure(&pcm);
    let target = a.get("target").and_then(|v| v.as_str()).unwrap_or("youtube");
    let (tl, tp) = TARGETS.iter().find(|t| t.0 == target).map(|t| (t.1, t.2)).or_else(|| a.get("target_lufs").and_then(|v| v.as_f64()).map(|l| (l, -1.0))).ok_or_else(|| format!("target must be one of {}", TARGETS.iter().map(|t| t.0).collect::<Vec<_>>().join(", ")))?;
    let (i, peak) = (m.integrated(), m.true_peak_dbtp());
    let mut issues = vec![];
    if i.is_finite() && (i - tl).abs() > 1.0 { issues.push(format!("integrated {:.1} LUFS is {:.1} LU {} the {target} target {tl} LUFS: normalize with export_audio_as normalize_lufs={tl} or finish_audio", i, (i - tl).abs(), if i > tl { "above" } else { "below" })); }
    if peak > tp { issues.push(format!("true peak {peak:.1} dBTP is over the {tp} dBTP ceiling")); }
    Ok(json!({"integrated_lufs": r1(i), "loudness_range_lu": r1(m.loudness_range()), "max_momentary_lufs": r1(m.max_momentary()), "max_short_term_lufs": r1(m.max_short_term()), "true_peak_dbtp": r1(peak),
        "target": {"name": target, "lufs": tl, "true_peak_dbtp": tp}, "ok": issues.is_empty(), "issues": issues, "standard": "ITU-R BS.1770-4 / EBU R128 (FilmCraft's meter)"}))
}

fn write_wav_f32(path: &std::path::Path, planar: &[Vec<f32>]) -> Result<(), String> {
    let ch = planar.len(); let n = planar.first().map_or(0, |c| c.len()); let data = (n * ch * 4) as u32;
    let mut b = Vec::with_capacity(44 + data as usize); b.extend_from_slice(b"RIFF"); b.extend_from_slice(&(36 + data).to_le_bytes()); b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes()); b.extend_from_slice(&3u16.to_le_bytes()); b.extend_from_slice(&(ch as u16).to_le_bytes()); b.extend_from_slice(&(SR as u32).to_le_bytes());
    b.extend_from_slice(&((SR * ch * 4) as u32).to_le_bytes()); b.extend_from_slice(&((ch * 4) as u16).to_le_bytes()); b.extend_from_slice(&32u16.to_le_bytes()); b.extend_from_slice(b"data"); b.extend_from_slice(&data.to_le_bytes());
    for i in 0..n { for c in planar { b.extend_from_slice(&c[i].to_le_bytes()); } }
    std::fs::write(path, b).map_err(|e| format!("could not write {}: {e}", path.display()))
}

fn remix_music(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = str_arg(a, "clip_id")?.to_string(); let target = num_arg(a, "duration")?; if !(target > 1.0) { return Err("duration must be over 1 second".into()); }
    let c = ss.project.find_clip(&cid).ok_or_else(|| format!("no clip '{cid}'"))?.clone();
    let Source::Media { asset } = &c.source else { return Err("remix_music needs a media clip (music)".into()) };
    let a0 = ss.project.assets.get(asset).ok_or("the clip's media is missing")?.clone(); let path = ss.project.resolve(&a0.path);
    // the whole piece, decoded at 1x with no clip effects
    let mut whole = c.clone(); whole.in_point = 0.0; whole.duration = a0.duration; whole.speed = 1.0; whole.time_remap = None; whole.audio.effects.clear();
    let pcm = crate::audio::decode_clip(&path, &whole); if pcm.len() < SR * 4 { return Err("the clip has no audio (or under 2 seconds of it)".into()); }
    let l: Vec<f32> = pcm.chunks_exact(2).map(|f| f[0]).collect(); let r: Vec<f32> = pcm.chunks_exact(2).map(|f| f[1]).collect();
    let an = kimo_fc_audio::remix::analyze(&[&l, &r], SR as u32).map_err(|e| format!("could not analyse the music: {e:?}"))?;
    let params = kimo_fc_audio::remix::Params { segments: opt_num(a, "segments", 50.0).clamp(0.0, 100.0), variations: opt_num(a, "variations", 50.0).clamp(0.0, 100.0) };
    let n = (target * SR as f64).round() as i64; let plan = kimo_fc_audio::remix::plan(&an, n, params).map_err(|e| format!("could not plan a {target:.1} s remix: {e:?}"))?;
    let out = kimo_fc_audio::remix::render(&plan, 0, n as usize, 2, &mut |src, len| { let s = src.max(0) as usize; [&l, &r].iter().map(|ch| (0..len).map(|i| ch.get(s + i).copied().unwrap_or(0.0)).collect()).collect() });
    let dir = std::path::Path::new(&path).parent().map(|d| d.to_path_buf()).unwrap_or_else(std::env::temp_dir);
    let stem = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "music".into());
    let outp = dir.join(format!("{stem}_remix_{:.0}s.wav", target)); write_wav_f32(&outp, &out)?;
    let aid = crate::commands::dispatch_nested(ss, "import_media", &json!({"path": outp.to_string_lossy()}))?["asset_id"].as_str().unwrap_or_default().to_string();
    let cl = ss.project.find_clip_mut(&cid).ok_or("clip vanished")?; cl.source = Source::Media { asset: aid.clone() }; cl.in_point = 0.0; cl.duration = target; cl.speed = 1.0;
    Ok(json!({"clip_id": cid, "asset_id": aid, "path": outp.to_string_lossy(), "duration": target, "pieces": plan.pieces.len(), "joints_s": plan.joints().iter().map(|j| (*j as f64 / SR as f64 * 100.0).round() / 100.0).collect::<Vec<_>>()}))
}

fn set_track_mix(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let tid = str_arg(a, "track_id")?.to_string(); if !ss.project.tracks.iter().any(|t| t.id == tid) { return Err(format!("no track '{tid}'")); }
    let add = match a.get("add_effect") { Some(e) => Some(crate::ext::editorial_tools::dsp_effect_from(e.get("kind").and_then(|k| k.as_str()).ok_or("add_effect needs kind")?, &e.get("params").cloned().unwrap_or(json!({})))?), None => None };
    let m = ss.project.track_mix.entry(tid.clone()).or_default();
    if let Some(g) = a.get("gain_db").and_then(|v| v.as_f64()) { if !(-96.0..=24.0).contains(&g) { return Err("gain_db must be -96..24".into()); } m.gain_db = g; }
    if let Some(p) = a.get("pan").and_then(|v| v.as_f64()) { if !(-1.0..=1.0).contains(&p) { return Err("pan must be -1 (left) .. 1 (right)".into()); } m.pan = p; }
    if a.get("clear_effects").and_then(|v| v.as_bool()).unwrap_or(false) { m.effects.clear(); }
    if let Some(e) = add { m.effects.push(e); }
    let out = json!({"track_id": tid, "gain_db": m.gain_db, "pan": m.pan, "effects": m.effects});
    if m.gain_db == 0.0 && m.pan == 0.0 && m.effects.is_empty() { ss.project.track_mix.remove(&tid); }
    Ok(out)
}

fn export_audio_as(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?.to_string(); let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if std::path::Path::new(&path).exists() && !a.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false) { return Err(format!("{path} exists; pass overwrite: true to replace it")); }
    let codec: &[&str] = match ext.as_str() { "wav" => &["-c:a", "pcm_s24le"], "flac" => &["-c:a", "flac"], "mp3" => &["-c:a", "libmp3lame", "-b:a", "320k"], "m4a" | "aac" => &["-c:a", "aac", "-b:a", "256k"], "opus" | "ogg" => &["-c:a", "libopus", "-b:a", "192k"],
        o => return Err(format!("the extension must be .wav, .flac, .mp3, .m4a or .opus, got '.{o}'")) };
    let ch = a.get("channels").and_then(|v| v.as_u64()).unwrap_or(2); if ch != 2 && ch != 6 { return Err("channels must be 2 (stereo) or 6 (5.1)".into()); }
    if ch == 6 && ext == "mp3" { return Err("MP3 is stereo only; use .wav, .flac, .m4a or .opus for 5.1".into()); }
    let mut pcm = crate::audio::mix(&ss.project); if pcm.is_empty() { return Err("the timeline has no audio".into()); }
    let mut gain_db = 0.0;
    if let Some(target) = a.get("normalize_lufs").and_then(|v| v.as_f64()) {
        let i = measure(&pcm).integrated(); if !i.is_finite() { return Err("the mix is silent; nothing to normalize".into()); }
        gain_db = target - i; let g = 10f64.powf(gain_db / 20.0) as f32; for v in pcm.iter_mut() { *v *= g; }
        let ceiling = opt_num(a, "true_peak", -1.0); crate::audio::limit(&mut pcm, ceiling - 0.3); // sample-peak limiter a little under the true-peak ceiling
    }
    let l: Vec<f32> = pcm.chunks_exact(2).map(|f| f[0]).collect(); let r: Vec<f32> = pcm.chunks_exact(2).map(|f| f[1]).collect();
    let planar = if ch == 6 { convert(&[&l, &r], Layout::Stereo, Layout::Surround51, Mixdown::FrontRear) } else { vec![l, r] };
    let tmp = std::env::temp_dir().join(format!("kimo-audio-export-{}.wav", std::process::id())); write_wav_f32(&tmp, &planar)?;
    let mut cmd = std::process::Command::new("ffmpeg"); cmd.args(["-v", "error", "-y", "-i"]).arg(&tmp).args(codec);
    if ch == 6 { cmd.args(["-channel_layout", "5.1"]); } cmd.arg(&path);
    let st = cmd.status().map_err(|e| format!("ffmpeg: {e}")); let _ = std::fs::remove_file(&tmp); if !st?.success() { return Err(format!("ffmpeg could not write {path}")); }
    let after = measure(&pcm);
    Ok(json!({"path": path, "channels": ch, "duration": (pcm.len() / 2) as f64 / SR as f64, "gain_db": r1(gain_db), "integrated_lufs": r1(after.integrated()), "true_peak_dbtp": r1(after.true_peak_dbtp())}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "loudness_report", description: "Measure the mix to ITU-R BS.1770-4 / EBU R128 (integrated, loudness range, max momentary and short-term, true peak) against a delivery target (youtube, spotify, apple_music, podcast, tiktok, broadcast_ebu, broadcast_atsc, netflix, or target_lufs) and list what is off.", mutates: false,
        schema: obj(vec![("target", json!({"type":"string","enum":TARGETS.iter().map(|t| t.0).collect::<Vec<_>>()})), ("target_lufs", n("custom integrated target")), ("start", n("s")), ("end", n("s"))], &[]),
        run: loudness_report },
    Command { name: "remix_music", description: "Retime a music clip to an exact duration by cutting it at musically similar beat boundaries and crossfading (Premiere's Remix): writes <name>_remix_<secs>s.wav next to the music and puts it on the clip. segments/variations 0..100 trade fewer, longer pieces against more, shorter ones.", mutates: true,
        schema: obj(vec![("clip_id", s("music clip")), ("duration", n("target seconds")), ("segments", n("0..100, default 50")), ("variations", n("0..100, default 50"))], &["clip_id","duration"]),
        run: remix_music },
    Command { name: "set_track_mix", description: "A track's mixer strip: gain_db (-96..24), pan (-1 left .. 1 right) and insert effects (add_effect {kind, params} with any FilmCraft DSP kind from add_audio_effect: parametric_eq, compressor_dsp, mastering, ...), applied to the sum of the track's clips before the master. clear_effects removes them.", mutates: true,
        schema: obj(vec![("track_id", s("track")), ("gain_db", n("dB")), ("pan", n("-1..1")), ("add_effect", json!({"type":"object","description":"{kind, params}"})), ("clear_effects", json!({"type":"boolean"}))], &["track_id"]),
        run: set_track_mix },
    Command { name: "export_audio_as", description: "Export the mix as .wav (24-bit), .flac, .mp3 (320k), .m4a (AAC 256k) or .opus; channels 2 or 6 (5.1 upmix, ITU-R BS.775); normalize_lufs (e.g. -14 for YouTube, -16 for podcasts, -23 for EBU broadcast) with a true_peak ceiling (default -1 dBTP).", mutates: false,
        schema: obj(vec![("path", s("output file; the extension picks the format")), ("channels", n("2 or 6")), ("normalize_lufs", n("integrated loudness target")), ("true_peak", n("dBTP ceiling with normalize_lufs, default -1")), ("overwrite", json!({"type":"boolean"}))], &["path"]),
        run: export_audio_as },
] }
