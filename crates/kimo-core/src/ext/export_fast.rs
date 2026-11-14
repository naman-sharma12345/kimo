//! Fast export for simple timelines: one untouched video clip covering the whole edit plus untouched audio clips
//! (only volume, fades and position set) is exported by COPYING the video stream and mixing/encoding only the audio,
//! instead of re-rendering every frame. A 10-minute talking head with three WAVs takes about as long as reading the
//! file, not tens of minutes. Eligibility is deliberately strict and checked on the project's own JSON, so any field
//! this module does not know about that is not at its default (an effect, a mask, a transform, a grade, a speed change,
//! text...) sends the export down the normal render path. The result is the same picture, bit for bit, as the source.
use serde_json::{json, Value};

pub struct Plan {
    pub video: String,
    pub duration: f64,
    /// (path, timeline start, source in-point, duration, volume, fade_in, fade_out)
    pub audio: Vec<(String, f64, f64, f64, f64, f64, f64)>,
}

/// A value at its "nothing applied" default: null, false, 0, "", "normal", empty list/map, or a property with no keys.
fn pristine(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty() || s == "normal",
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.iter().all(|(k, x)| k == "mode" && x == "normal" || pristine(x)),
    }
}
fn prop_is(v: &Value, base: f64) -> bool {
    v.get("base").and_then(|b| b.as_f64()).map(|b| (b - base).abs() < 1e-9).unwrap_or(false)
        && v.get("keys").map(pristine).unwrap_or(true) && v.get("expr").map(pristine).unwrap_or(true)
}
const CLIP_FREE: &[&str] = &["id", "source", "start", "duration", "in_point", "name", "role", "tags", "color", "label", "audio", "props", "speed", "blend", "time_mode"];

/// Check one clip: transforms at identity, speed 1, normal blend and time, nothing else applied. `allow_audio` lets
/// volume and fades through (they are mixed exactly by ffmpeg); every other audio setting must be untouched.
fn clip_simple(c: &Value) -> Option<(f64, f64, f64)> {
    let o = c.as_object()?;
    for (k, v) in o { if !CLIP_FREE.contains(&k.as_str()) && !pristine(v) { return None; } }
    if c.get("speed").and_then(|s| s.as_f64()).map(|s| (s - 1.0).abs() > 1e-9).unwrap_or(false) { return None; }
    if c.get("blend").map(|b| !pristine(b)).unwrap_or(false) || c.get("time_mode").map(|t| !pristine(t)).unwrap_or(false) { return None; }
    if let Some(pr) = c.get("props").and_then(|p| p.as_object()) {
        for (k, v) in pr {
            let base = match k.as_str() { "opacity" | "scale" | "reveal" => 1.0, _ => 0.0 };
            if !prop_is(v, base) { return None; }
        }
    }
    let au = c.get("audio")?;
    let vol = au.get("volume").and_then(|v| if v.get("keys").map(pristine).unwrap_or(true) && v.get("expr").map(pristine).unwrap_or(true) { v.get("base")?.as_f64() } else { None })?;
    for (k, v) in au.as_object()? {
        match k.as_str() {
            "volume" | "fade_in" | "fade_out" | "mute" => {}
            "duck_gain" => if !prop_is(v, 1.0) { return None; },
            _ => if !pristine(v) { return None; },
        }
    }
    Some((vol, au.get("fade_in").and_then(|x| x.as_f64()).unwrap_or(0.0), au.get("fade_out").and_then(|x| x.as_f64()).unwrap_or(0.0)))
}

fn probe_video(path: &str) -> Option<(String, u32, u32, f64, bool)> {
    let o = std::process::Command::new("ffprobe").args(["-v", "error", "-show_entries", "stream=codec_type,codec_name,width,height,avg_frame_rate", "-of", "json", path]).output().ok()?;
    let v: Value = serde_json::from_slice(&o.stdout).ok()?;
    let ss = v["streams"].as_array()?;
    let vs = ss.iter().find(|s| s["codec_type"] == "video")?;
    let fr = vs["avg_frame_rate"].as_str().unwrap_or("0/1");
    let (a, b) = fr.split_once('/').unwrap_or((fr, "1"));
    let fps = a.parse::<f64>().ok()? / b.parse::<f64>().ok().filter(|x| *x > 0.0)?;
    Some((vs["codec_name"].as_str()?.to_string(), vs["width"].as_u64()? as u32, vs["height"].as_u64()? as u32, fps, ss.iter().any(|s| s["codec_type"] == "audio")))
}

/// Is this project a "one video + audio" timeline that can be stream-copied? Returns the plan, or why not (for the result).
pub fn plan(p: &crate::model::Project) -> Result<Plan, String> {
    let pj = serde_json::to_value(p).map_err(|e| e.to_string())?;
    if pj["color_pipeline"]["enabled"].as_bool().unwrap_or(false) { return Err("colour management is on".into()); }
    if !pristine(&pj["grade_scopes"]["timeline"]) || !pristine(&pj["grade_scopes"]["clip_bindings"]) { return Err("a grade is applied".into()); }
    let tracks = pj["tracks"].as_array().ok_or("no tracks")?;
    if tracks.iter().any(|t| t["solo"].as_bool().unwrap_or(false)) { return Err("a track is soloed".into()); }
    let end = super::export_manifest::project_end(p);
    if end <= 0.0 { return Err("empty timeline".into()); }
    let (mut video, mut audio) = (None::<(Value, f64)>, vec![]);
    for t in tracks {
        let muted = t["muted"].as_bool().unwrap_or(false);
        for c in t["clips"].as_array().into_iter().flatten() {
            if c["source"]["type"] != "media" { return Err(format!("clip {} is not plain media", c["id"])); }
            let asset = c["source"]["asset"].as_str().or(c["source"]["asset_id"].as_str()).ok_or("media clip without an asset")?;
            let a = &pj["assets"][asset];
            let is_video = a["width"].as_u64().unwrap_or(0) > 0;
            let (vol, fi, fo) = clip_simple(c).ok_or_else(|| format!("clip {} has effects, transforms or other settings", c["id"]))?;
            let path = p.resolve(a["path"].as_str().ok_or("asset without a path")?);
            let (start, dur, inp) = (c["start"].as_f64().unwrap_or(0.0), c["duration"].as_f64().unwrap_or(0.0), c["in_point"].as_f64().unwrap_or(0.0));
            if is_video {
                if video.is_some() { return Err("more than one picture clip".into()); }
                if start.abs() > 1e-6 || inp.abs() > 1e-6 || (dur - end).abs() > 0.5 / p.fps.max(1.0) { return Err("the video clip does not cover the whole edit from its first frame".into()); }
                video = Some((c.clone(), vol));
                let muted_audio = muted || c["audio"]["mute"].as_bool().unwrap_or(false);
                if !muted_audio { audio.push((path.clone(), start, inp, dur, vol, fi, fo)); }
                let (codec, w, h, fps, has_audio) = probe_video(&path).ok_or("cannot read the video's streams")?;
                if !matches!(codec.as_str(), "h264" | "hevc" | "av1") { return Err(format!("the video is {codec}, not H.264/HEVC/AV1")); }
                if w != p.width || h != p.height { return Err(format!("the video is {w}x{h}, the project is {}x{}", p.width, p.height)); }
                if (fps - p.fps).abs() > 0.01 { return Err(format!("the video is {fps:.3} fps, the project is {}", p.fps)); }
                if !has_audio && !muted_audio { audio.pop(); }
            } else if !muted && !c["audio"]["mute"].as_bool().unwrap_or(false) {
                audio.push((path, start, inp, dur, vol, fi, fo));
            }
        }
    }
    let (vc, _) = video.ok_or("no video clip")?;
    let vpath = p.resolve(pj["assets"][vc["source"]["asset"].as_str().or(vc["source"]["asset_id"].as_str()).unwrap_or("")]["path"].as_str().unwrap_or(""));
    Ok(Plan { video: vpath, duration: end, audio })
}

/// The ffmpeg argv for a plan: the video stream copied, every audio clip trimmed, delayed, levelled and mixed.
pub fn argv(pl: &Plan, out: &str) -> Vec<String> {
    let mut a: Vec<String> = vec!["-y".into(), "-v".into(), "error".into(), "-nostdin".into(), "-progress".into(), "pipe:1".into(), "-i".into(), pl.video.clone()];
    let mut inputs: Vec<usize> = vec![];
    for (i, (path, ..)) in pl.audio.iter().enumerate() {
        if *path == pl.video && i == 0 { inputs.push(0); continue; }
        a.push("-i".into()); a.push(path.clone()); inputs.push(a.iter().filter(|x| *x == "-i").count() - 1);
    }
    let mut graph = vec![];
    for (k, ((_, start, inp, dur, vol, fi, fo), input)) in pl.audio.iter().zip(&inputs).enumerate() {
        let mut f = format!("[{input}:a]atrim=start={inp:.6}:duration={dur:.6},asetpts=PTS-STARTPTS,aresample=48000,volume={vol:.6}");
        if *fi > 0.0 { f.push_str(&format!(",afade=t=in:st=0:d={fi:.6}")); }
        if *fo > 0.0 { f.push_str(&format!(",afade=t=out:st={:.6}:d={fo:.6}", (dur - fo).max(0.0))); }
        let ms = (start * 1000.0).round() as u64;
        if ms > 0 { f.push_str(&format!(",adelay={ms}:all=1")); }
        f.push_str(&format!("[a{k}]"));
        graph.push(f);
    }
    a.extend(["-map".into(), "0:v:0".into()]);
    if !graph.is_empty() {
        let labels: String = (0..graph.len()).map(|k| format!("[a{k}]")).collect();
        graph.push(format!("{labels}amix=inputs={}:normalize=0:duration=longest,apad,atrim=0:{:.6}[aout]", graph.len(), pl.duration));
        a.extend(["-filter_complex".into(), graph.join(";"), "-map".into(), "[aout]".into(), "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()]);
    } else { a.push("-an".into()); }
    a.extend(["-c:v".into(), "copy".into(), "-t".into(), format!("{:.6}", pl.duration), "-movflags".into(), "+faststart".into(), out.into()]);
    a
}

/// No tools of its own: export_job_start uses it.
pub fn commands() -> Vec<crate::commands::Command> { vec![] }

pub fn summary(pl: &Plan) -> Value { json!({"mode": "stream_copy", "video": pl.video, "audio_clips": pl.audio.len(), "duration": pl.duration}) }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn pristine_and_simple_clips() {
        let base = json!({"id":"c1","start":0.0,"duration":5.0,"in_point":0.0,"speed":1.0,"blend":"normal","time_mode":{"mode":"normal"},
            "source":{"type":"media","asset":"a1"},"effects":[],"mask":null,"transition_in":null,"motion_blur":0.0,
            "props":{"opacity":{"base":1.0,"keys":[],"expr":[]},"scale":{"base":1.0,"keys":[],"expr":[]},"x":{"base":0.0,"keys":[],"expr":[]}},
            "audio":{"volume":{"base":0.8,"keys":[],"expr":[]},"fade_in":0.5,"fade_out":0.0,"mute":false,"effects":[],"pitch":0.0,"duck_gain":{"base":1.0,"keys":[],"expr":[]}}});
        assert_eq!(clip_simple(&base), Some((0.8, 0.5, 0.0)));
        let mut fx = base.clone(); fx["effects"] = json!([{"kind":"blur"}]); assert!(clip_simple(&fx).is_none(), "an effect needs a render");
        let mut sc = base.clone(); sc["props"]["scale"]["base"] = json!(1.2); assert!(clip_simple(&sc).is_none(), "a transform needs a render");
        let mut ky = base.clone(); ky["audio"]["volume"]["keys"] = json!([[0,1]]); assert!(clip_simple(&ky).is_none(), "keyed volume is not a plain volume");
        let mut sp = base.clone(); sp["speed"] = json!(2.0); assert!(clip_simple(&sp).is_none());
        let mut unknown = base.clone(); unknown["some_new_feature"] = json!({"on": true}); assert!(clip_simple(&unknown).is_none(), "anything unknown and set disqualifies");
    }
    #[test]
    fn argv_copies_video_and_mixes_audio() {
        let pl = Plan { video: "v.mp4".into(), duration: 600.0, audio: vec![("v.mp4".into(), 0.0, 0.0, 600.0, 1.0, 0.0, 0.0), ("a1.wav".into(), 2.0, 1.0, 300.0, 0.5, 0.0, 2.0)] };
        let a = argv(&pl, "out.mp4");
        let s = a.join(" ");
        assert!(s.contains("-c:v copy"), "{s}");
        assert!(s.contains("[0:a]atrim=start=0.000000:duration=600.000000"), "the video's own audio is input 0: {s}");
        assert!(s.contains("[1:a]atrim=start=1.000000:duration=300.000000") && s.contains("volume=0.500000") && s.contains("adelay=2000:all=1") && s.contains("afade=t=out:st=298.000000:d=2.000000"), "{s}");
        assert!(s.contains("amix=inputs=2:normalize=0"), "{s}");
        assert_eq!(a.iter().filter(|x| *x == "-i").count(), 2, "the video file is read once");
        let silent = Plan { video: "v.mp4".into(), duration: 10.0, audio: vec![] };
        assert!(argv(&silent, "o.mp4").contains(&"-an".to_string()));
    }
}
