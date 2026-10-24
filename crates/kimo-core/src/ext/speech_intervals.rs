//! RV-26 thin adapter; local targets duplicated pending shared audio_targets reconciliation.
use super::kit::*;
use crate::{
    model::Source,
    perception::vad::{self, Params},
};
fn number(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) {
        None => Ok(d),
        Some(v) => v
            .as_f64()
            .filter(|x| x.is_finite())
            .ok_or_else(|| crate::errors::arg_err(a, k, "finite number")),
    }
}
/// KR2: counted PER-THREAD without a new thread-local static - see render::ANALYSIS_PREPARE_CALLS's doc
/// comment for why (the task bars adding one, and this engine has a real history of thread-local
/// render-state bugs) and why keying a shared `Mutex<HashMap<ThreadId,_>>` by the calling thread is
/// just as safe here next to Rust's one-thread-per-test harness. Pins how many times this function's
/// real audio decode (an ffmpeg-backed strict_decode/mix_strict call) actually runs, independent of
/// how many times `run` is called - see `run`'s pcm-memo, which skips this entirely on a cache hit.
pub static SPEECH_DECODE_CALLS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<std::thread::ThreadId, usize>>> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::HashMap::new()));
pub fn speech_decode_call_count() -> usize { *SPEECH_DECODE_CALLS.lock().unwrap().get(&std::thread::current().id()).unwrap_or(&0) }
fn decode(ss: &Session, target: &Value) -> Result<Vec<f32>, String> {
    *SPEECH_DECODE_CALLS.lock().unwrap().entry(std::thread::current().id()).or_insert(0) += 1;
    if target == "mix" || target.is_null() || target.get("mix") == Some(&json!(true)) {
        return crate::audio::mix_strict(&ss.project).map_err(|e| e.to_string());
    }
    if let Some(id) = target.get("clip_id").and_then(Value::as_str) {
        let c = ss
            .project
            .find_clip(id)
            .ok_or_else(|| format!("no clip '{id}'"))?;
        let Source::Media { asset } = &c.source else {
            return Err("no audio: media clip required".into());
        };
        let a = ss.project.assets.get(asset).ok_or("no asset")?;
        return crate::audio::strict_decode(&ss.project.resolve(&a.path), c)
            .map_err(|e| e.to_string());
    }
    if let Some(id) = target.get("track_id").and_then(Value::as_str) {
        let tr = ss
            .project
            .tracks
            .iter()
            .find(|t| t.id == id)
            .ok_or_else(|| format!("no track '{id}'"))?;
        let mut p = ss.project.clone();
        p.tracks = vec![tr.clone()];
        return crate::audio::mix_strict(&p).map_err(|e| e.to_string());
    }
    let path = target
        .as_str()
        .or_else(|| target.get("path").and_then(Value::as_str))
        .ok_or("target must be mix or {clip_id}, {track_id}, {path}")?;
    let path = ss.project.resolve(path);
    let raw = capture(
        std::process::Command::new("ffprobe").args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
            &path,
        ]),
        1024 * 1024,
        std::time::Duration::from_secs(30),
    )?;
    let v: Value = serde_json::from_slice(&raw).map_err(|e| e.to_string())?;
    let duration = v["format"]["duration"]
        .as_str()
        .and_then(|s| s.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v > 0.)
        .ok_or("cannot read audio duration")?;
    let mut c:crate::model::Clip=serde_json::from_value(json!({"id":"vad-path","source":{"type":"media","asset":"vad-path"},"start":0.,"duration":duration,"in_point":0.,"speed":1.,"props":{},"effects":[],"blend":"normal"})).map_err(|e|e.to_string())?;
    c.audio.mute = false;
    crate::audio::strict_decode(&path, &c).map_err(|e| e.to_string())
}

/// A conservative content fingerprint of everything `decode` could possibly read for ANY target:
/// the whole project, serialized. Correct by construction (anything decode's match arms could look
/// at - tracks, clips, assets, their paths - is inside `ss.project`), at the cost of being more
/// conservative than strictly necessary (an edit to a clip on a different, unrelated track also
/// invalidates the cache). That's the right trade here: decode() is the expensive part (an
/// ffmpeg-backed strict_decode/mix_strict), building and hashing this string is not, and a correct
/// cache miss costs exactly what every call cost before this cache existed - a correct cache HIT is
/// the only thing being purchased. None only when serialization itself fails (never observed; a
/// serialization failure makes the cache a permanent, harmless miss instead of a risk).
fn project_fingerprint(ss: &Session) -> Option<u64> {
    use std::hash::{Hash, Hasher};
    let s = serde_json::to_string(&ss.project).ok()?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    s.hash(&mut h);
    Some(h.finish())
}
/// Decodes once per (target, project-state) pair in this session and reuses the PCM on every later
/// call with the SAME target while the project is byte-identical - e.g. lint_edit's own
/// probe-then-final pair (director_tools.rs `speech()`, aggressiveness 2 then 1) for whichever
/// track turns out to hold the dialogue, which used to pay this function's real decode twice for
/// the one track that matters. Keyed on the target's own JSON (not just a track/clip id) so a
/// `path`/`{path}` target and a `mix` target never collide. Bounded to a few entries; cleared
/// instead of LRU-evicted past that (see Session::audio_decode_cache).
fn decode_memo(ss: &mut Session, target: &Value) -> Result<std::sync::Arc<Vec<f32>>, String> {
    let key = project_fingerprint(ss).and_then(|fp| serde_json::to_string(target).ok().map(|t| (t, fp)));
    if let Some(k) = &key {
        if let Some(hit) = ss.audio_decode_cache.get(k) {
            return Ok(hit.clone());
        }
    }
    let pcm = std::sync::Arc::new(decode(ss, target)?);
    if let Some(k) = key {
        if ss.audio_decode_cache.len() >= 8 {
            ss.audio_decode_cache.clear(); // simple bound, not an LRU: see the field's own doc comment
        }
        ss.audio_decode_cache.insert(k, pcm.clone());
    }
    Ok(pcm)
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let mut p = Params::default();
    p.mode = match a.get("mode") {
        None => "vad",
        Some(v) => v.as_str().ok_or("mode must be vad|silence")?,
    }
    .into();
    let ag = number(a, "aggressiveness", 1.)?;
    if !(0. ..=3.).contains(&ag) || ag.fract() != 0. {
        return Err("aggressiveness must be an integer 0..3".into());
    }
    p.aggressiveness = ag as u8;
    p.min_speech_ms = number(a, "min_speech_ms", 200.)?;
    p.min_silence_ms = number(a, "min_silence_ms", 300.)?;
    p.pad_ms = number(a, "pad_ms", 120.)?;
    p.hangover_ms = number(a, "hangover_ms", 200.)?;
    p.threshold_db = number(a, "threshold_db", -60.)?;
    p.merge_gap_ms = number(a, "merge_gap_ms", 0.)?;
    let max = number(a, "max_items", 1000.)?;
    if max.fract() != 0. || !(1. ..=100000.).contains(&max) {
        return Err("max_items must be an integer 1..100000".into());
    }
    p.max_items = max as usize;
    p.check()?;
    let image = match a.get("image") {
        None => "none",
        Some(v) => v.as_str().ok_or("image must be none|waveform")?,
    };
    if !["none", "waveform"].contains(&image) {
        return Err("image must be none|waveform".into());
    }
    let start = number(a, "start", 0.)?;
    let requested_end = a.get("end").map(|_| number(a, "end", 0.)).transpose()?;
    if start < 0. || requested_end.map_or(false, |e| e <= start) {
        return Err("range must satisfy 0 <= start < end".into());
    }
    let target = a.get("target").cloned().unwrap_or(json!("mix"));
    if target.as_object().map_or(false, |m| m.len() != 1) {
        return Err("target needs exactly one of clip_id|track_id|mix|path".into());
    }
    let pcm = decode_memo(ss, &target)?;
    let total = pcm.len() as f64 / 96000.;
    if start >= total {
        return Err("range start exceeds audio duration".into());
    }
    let end = requested_end.unwrap_or(total).min(total);
    let i0 = (start * 48000.) as usize * 2;
    let i1 = ((end * 48000.) as usize * 2).min(pcm.len());
    if i1 <= i0 {
        return Err("range selects no samples".into());
    }
    let start=i0 as f64/96000.; // actual sample boundary, not requested fractional seconds
    let x = &pcm[i0..i1];
    let mut result = vad::analyze(x, &p)?;
    for s in &mut result.speech {
        s.start += start;
        s.end += start;
    }
    for s in &mut result.silence {
        s.start += start;
        s.end += start;
    }
    for s in &mut result.cut_suggestions {
        s.start += start;
        s.end += start;
    }
    let mut out = serde_json::to_value(&result).map_err(|e| e.to_string())?;
    for key in ["speech","silence","cut_suggestions"] {if let Some(rows)=out[key].as_array_mut(){for row in rows{crate::perception::timebase::annotate(row,ss.project.fps,key!="speech")}}}
    out["timebase"]=crate::perception::timebase::meta(ss.project.fps);
    out["range_samples"]=json!({"start_sample":i0/2,"end_sample":i1/2,"sample_count":(i1-i0)/2});
    out["params"]["requested_start"] = json!(a.get("start").and_then(Value::as_f64).unwrap_or(0.));
    out["params"]["target"] = target;
    out["params"]["start"] = json!(start);
    out["params"]["end"] = json!(start + result.duration_s);
    out["params"]["image"] = json!(image);
    if image == "waveform" {
        let mut hash = 0xcbf29ce484222325u64;
        for v in x {
            for b in v.to_bits().to_le_bytes() {
                hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
            }
        }
        let path = std::env::temp_dir().join(format!(
            "kimo-vad-waveform-{}-{hash:016x}.png",
            std::process::id()
        ));
        let mut f = crate::effects::Frame::new(640, 160);
        for px in f.d.chunks_mut(4) {
            px.copy_from_slice(&[0.06, 0.06, 0.08, 1.])
        }
        for col in 0..640 {
            crate::guard::check();
            let (a, b) = (
                col * x.len() / 640,
                ((col + 1) * x.len() / 640)
                    .max(col * x.len() / 640 + 1)
                    .min(x.len()),
            );
            let peak = x[a..b]
                .iter()
                .map(|v| v.abs())
                .fold(0f32, f32::max)
                .clamp(0., 1.);
            let h = (peak * 72.) as usize;
            for y in 80 - h..=80 + h {
                let i = (y * 640 + col) * 4;
                f.d[i..i + 4].copy_from_slice(&[0.2, 0.8, 0.5, 1.])
            }
        }
        let path = path.to_string_lossy().to_string();
        crate::render::write_image(&f, &path)?;
        ss.media.push(crate::commands::Media {
            path: path.clone(),
            w: 640,
            h: 160,
            mime: "image/png",
        });
        out["waveform"] = json!({"path":path,"width":640,"height":160});
    }
    Ok(out)
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"audio_speech_intervals",mutates:false,description:"Find speech intervals and pauses with a lightweight energy/ZCR/spectral voice detector, or silences by the audio_probe RMS rule. Detect speech voice pauses with lightweight energy ZCR spectral VAD (NOT neural recognition), or silence using the same 100ms RMS rule as audio_probe. Stereo 48k engine decode -> mono 16k, 20ms features, percentile noise floor, two-state hysteresis and hangover. Aggressiveness 2..3 rejects steady tones; 0..1 can classify a tone burst as speech (reported limitation). Speech/cut times include requested padding, start/end are target-relative seconds; cut_suggestions are advice only and never edit. target: mix, {clip_id}, {track_id}, path or {path}. Missing audio/decode failure are errors. max_items caps result arrays, count is before truncation. Exact per-channel 48k PCM start_sample/end_sample/sample_count and start_ms/end_ms accompany seconds. Speech frame_aligned snaps outward, silence/cuts inward; exclusive ends, frame sample indices follow round(frame*48000/fps). Sample indices are authoritative; target-relative, not absolute project timeline for a clip. Requested range snaps to actual PCM samples. synthetic validation only; real-speech recall and reverb tails below the floor not established. Optional waveform image.",schema:obj(vec![("target",json!({"description":"mix, path, or exactly one clip_id|track_id|mix|path object","oneOf":[{"type":"string"},{"type":"object"}]})),("start",n("range start seconds")),("end",n("range end seconds")),("mode",json!({"type":"string","enum":["vad","silence"]})),("aggressiveness",n("integer 0..3; default 1")),("min_speech_ms",n("minimum speech duration, default 200 ms")),("min_silence_ms",n("minimum pause, default 300 ms")),("pad_ms",n("speech edge padding, default 120 ms")),("hangover_ms",n("speech trailing hangover, default 200 ms")),("threshold_db",n("silence-mode RMS threshold, default -60 dBFS")),("merge_gap_ms",n("merge nearby speech intervals, default 0 ms")),("max_items",n("array cap, default 1000")),("image",json!({"type":"string","enum":["none","waveform"]}))],&[]),run}]
}
