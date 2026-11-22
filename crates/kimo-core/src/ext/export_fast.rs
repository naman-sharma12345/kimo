//! Fast export for cut-only timelines: plain media clips laid end to end (no effects, transforms, text, grades or
//! transitions), plus plain audio clips (only volume, fades and position set). Such an edit is exported in ONE ffmpeg
//! process instead of rendering every frame through kimo's compositor, so it runs as fast as ffmpeg itself:
//!
//! - **stream_copy**: every picture clip is a whole file (the last may be cut short), all with the same codec,
//!   profile, pixel format, codec headers, size and frame rate, matching the project. The files are joined with
//!   ffmpeg's concat demuxer and the video is COPIED (no re-encode: as fast as reading the files, no quality loss).
//!   One clip on its own is the same case. The audio is mixed and encoded -- or, when every clip keeps its own sound
//!   untouched and all files carry the same audio format, copied too: then this is exactly `ffmpeg -f concat -c copy`.
//! - **direct_encode**: anything else that is still cut-only (trimmed clips, mixed sizes, frame rates or codecs).
//!   ffmpeg decodes, conforms (the same fps and fit filters the renderer uses), joins and encodes with the same x264
//!   settings as the normal export, in one pass.
//!
//! Eligibility is deliberately strict and checked on the project's own JSON, so any field this module does not know
//! about that is not at its default (an effect, a mask, a transform, a grade, a speed change, text...) sends the
//! export down the normal render path, with the reason reported.
use serde_json::{json, Value};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode { Copy, Encode }

/// One picture clip, in timeline order.
#[derive(Clone, Debug, PartialEq)]
pub struct Seg {
    pub path: String, pub start: f64, pub inp: f64, pub dur: f64,
    /// frames this clip occupies on the timeline (from its rounded start frame to its rounded end frame)
    pub frames: u64,
    /// direct_encode only: colour retag applied first (the renderer's color_tag_sanity), the fps conform and the fit
    pub pre: String, pub fps_filter: String, pub fit: String,
}

/// One audio source: a picture clip's own sound (`seg`) or a separate audio clip.
#[derive(Clone, Debug, PartialEq)]
pub struct Aud { pub path: String, pub start: f64, pub inp: f64, pub dur: f64, pub vol: f64, pub fade_in: f64, pub fade_out: f64, pub seg: Option<usize> }

pub struct Plan {
    pub mode: Mode,
    pub video: Vec<Seg>,
    pub audio: Vec<Aud>,
    pub duration: f64,
    pub fps: f64,
    pub preset: Option<String>,
    /// direct_encode: why the video could not simply be copied
    pub why_not_copy: Option<String>,
    /// stream_copy only: the audio is copied from the files too (no mix, no encode), see `audio_copy_ok`
    pub audio_copy: bool,
}

/// A value at its "nothing applied" default: null, false, 0, "", "normal", empty list/map, or a property with no keys.
pub(crate) fn pristine(v: &Value) -> bool {
    match v {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::Number(n) => n.as_f64() == Some(0.0),
        Value::String(s) => s.is_empty() || s == "normal",
        Value::Array(a) => a.is_empty(),
        Value::Object(o) => o.iter().all(|(k, x)| k == "mode" && x == "normal" || pristine(x)),
    }
}
fn unkeyed(v: &Value) -> bool { v.get("keys").map(pristine).unwrap_or(true) && v.get("expr").map(pristine).unwrap_or(true) }
fn prop_is(v: &Value, base: f64) -> bool {
    v.get("base").and_then(|b| b.as_f64()).map(|b| (b - base).abs() < 1e-9).unwrap_or(false) && unkeyed(v)
}
const CLIP_FREE: &[&str] = &["id", "source", "start", "duration", "in_point", "name", "role", "tags", "color", "label", "audio", "props", "speed", "blend", "time_mode"];

/// What a simple clip carries: (volume, fade in, fade out, fit).
#[derive(Debug, PartialEq)]
pub(crate) struct Simple { pub vol: f64, pub fade_in: f64, pub fade_out: f64, pub fit: f64 }

/// Check one clip: transforms at identity, speed 1, normal blend and time, nothing else applied. Volume and fades
/// pass (ffmpeg mixes them exactly), and so does a constant `fit` (the renderer's own ffmpeg fit filter is reused);
/// every other setting must be untouched.
pub(crate) fn clip_simple(c: &Value) -> Option<Simple> {
    let o = c.as_object()?;
    for (k, v) in o { if !CLIP_FREE.contains(&k.as_str()) && !pristine(v) { return None; } }
    if c.get("speed").and_then(|s| s.as_f64()).map(|s| (s - 1.0).abs() > 1e-9).unwrap_or(false) { return None; }
    if c.get("blend").map(|b| !pristine(b)).unwrap_or(false) || c.get("time_mode").map(|t| !pristine(t)).unwrap_or(false) { return None; }
    let mut fit = 0.0;
    if let Some(pr) = c.get("props").and_then(|p| p.as_object()) {
        for (k, v) in pr {
            if k == "fit" { if !unkeyed(v) { return None; } fit = v.get("base").and_then(|b| b.as_f64()).unwrap_or(0.0); continue; }
            let base = match k.as_str() { "opacity" | "scale" | "reveal" => 1.0, _ => 0.0 };
            if !prop_is(v, base) { return None; }
        }
    }
    let au = c.get("audio")?;
    let vol = au.get("volume").and_then(|v| if unkeyed(v) { v.get("base")?.as_f64() } else { None })?;
    for (k, v) in au.as_object()? {
        match k.as_str() {
            "volume" | "fade_in" | "fade_out" | "mute" => {}
            "duck_gain" => if !prop_is(v, 1.0) { return None; },
            _ => if !pristine(v) { return None; },
        }
    }
    Some(Simple { vol, fade_in: au.get("fade_in").and_then(|x| x.as_f64()).unwrap_or(0.0), fade_out: au.get("fade_out").and_then(|x| x.as_f64()).unwrap_or(0.0), fit })
}

/// What ffprobe says about a file's first video stream: enough to know whether files can be joined without re-encoding.
#[derive(Clone, Debug, PartialEq)]
pub struct Probe { pub codec: String, pub profile: String, pub pix_fmt: String, pub width: u32, pub height: u32, pub rate: String, pub fps: f64, pub headers: Option<String>, pub has_audio: bool,
    pub duration: Option<f64>,
    /// the first audio stream: codec, profile, sample rate, channels, codec headers, duration
    pub audio: Option<(String, String, u64, u64, Option<String>, Option<f64>)> }

fn probe(path: &str) -> Option<Probe> {
    let o = std::process::Command::new("ffprobe").args(["-v", "error", "-show_data_hash", "sha256", "-show_entries", "stream=codec_type,codec_name,profile,pix_fmt,width,height,avg_frame_rate,extradata_hash,sample_rate,channels,duration", "-of", "json", path]).output().ok()?;
    parse_probe(&serde_json::from_slice(&o.stdout).ok()?)
}
fn parse_probe(v: &Value) -> Option<Probe> {
    let ss = v["streams"].as_array()?;
    let vs = ss.iter().find(|s| s["codec_type"] == "video")?;
    let rate = vs["avg_frame_rate"].as_str().unwrap_or("0/1").to_string();
    let (a, b) = rate.split_once('/').unwrap_or((&rate, "1"));
    let fps = a.parse::<f64>().ok()? / b.parse::<f64>().ok().filter(|x| *x > 0.0)?;
    let st = |s: &Value, k: &str| s[k].as_str().unwrap_or("").to_string();
    let num = |s: &Value, k: &str| s[k].as_f64().or_else(|| s[k].as_str().and_then(|x| x.parse().ok()));
    let au = ss.iter().find(|s| s["codec_type"] == "audio");
    Some(Probe { codec: st(vs, "codec_name"), profile: st(vs, "profile"), pix_fmt: st(vs, "pix_fmt"), width: vs["width"].as_u64()? as u32, height: vs["height"].as_u64()? as u32, rate: rate.clone(), fps,
        headers: vs["extradata_hash"].as_str().map(String::from), has_audio: au.is_some(), duration: num(vs, "duration"),
        audio: au.map(|a| (st(a, "codec_name"), st(a, "profile"), num(a, "sample_rate").unwrap_or(0.0) as u64, a["channels"].as_u64().unwrap_or(0), a["extradata_hash"].as_str().map(String::from), num(a, "duration"))) })
}

/// Can these picture clips be joined by copying their video streams? `Err` says why not.
/// `clips`: (probe, in-point, duration, the asset's full duration). Every clip must be a whole file except the last,
/// which may end early (the output is cut there); all must share codec, profile, pixel format and codec headers.
pub fn copy_ok(clips: &[(&Probe, f64, f64, f64)], w: u32, h: u32, fps: f64) -> Result<(), String> {
    let frame = 1.0 / fps.max(1.0);
    let first = clips.first().ok_or("no video clip")?.0;
    if !matches!(first.codec.as_str(), "h264" | "hevc" | "av1") { return Err(format!("the video is {}, not H.264/HEVC/AV1", first.codec)); }
    for (i, (pr, inp, dur, full)) in clips.iter().enumerate() {
        if pr.width != w || pr.height != h { return Err(format!("a clip is {}x{}, the project is {w}x{h}", pr.width, pr.height)); }
        if (pr.fps - fps).abs() > 0.01 { return Err(format!("a clip is {:.3} fps, the project is {fps}", pr.fps)); }
        if (pr.codec.as_str(), pr.profile.as_str(), pr.pix_fmt.as_str(), pr.rate.as_str()) != (first.codec.as_str(), first.profile.as_str(), first.pix_fmt.as_str(), first.rate.as_str()) { return Err("the clips were not encoded the same way (codec, profile, pixel format or frame rate differ)".into()); }
        if clips.len() > 1 && (pr.headers.is_none() || pr.headers != first.headers) { return Err("the clips' codec headers differ (different cameras or encoder settings)".into()); }
        if inp.abs() > frame / 2.0 { return Err("a clip is trimmed at its start".into()); }
        if i + 1 < clips.len() && (dur - full).abs() > frame { return Err("a clip is trimmed at its end".into()); }
    }
    Ok(())
}

/// With the video copied, can the audio be copied too (exactly ffmpeg's own concat)? Only when nothing is done to it:
/// every clip plays its own sound at full volume with no fades, there is no other audio, every clip is a whole file
/// (the last one too), all files carry the same audio format and headers, and in each file the sound and the picture
/// end within a frame of each other (so the files' own lengths, which the concat demuxer joins by, are the clips').
/// `clips`: (probe, clip duration, the asset's full duration).
pub fn audio_copy_ok(clips: &[(&Probe, f64, f64)], audio: &[Aud], fps: f64) -> bool {
    let frame = 1.0 / fps.max(1.0);
    if audio.len() != clips.len() || audio.iter().enumerate().any(|(k, a)| a.seg != Some(k) || (a.vol - 1.0).abs() > 1e-9 || a.fade_in > 0.0 || a.fade_out > 0.0) { return false; }
    let Some(first) = clips.first().and_then(|c| c.0.audio.clone()) else { return false };
    if first.4.is_none() || !matches!(first.0.as_str(), "aac" | "mp3" | "opus" | "ac3" | "eac3" | "alac" | "flac") { return false; }
    clips.iter().all(|(pr, dur, full)| {
        let Some(a) = &pr.audio else { return false };
        (a.0.as_str(), a.1.as_str(), a.2, a.3, &a.4) == (first.0.as_str(), first.1.as_str(), first.2, first.3, &first.4)
            && (dur - full).abs() <= frame
            && matches!((a.5, pr.duration), (Some(ad), Some(vd)) if (ad - vd).abs() <= frame)
    })
}

/// Is this project a cut-only timeline that ffmpeg can export in one process? Returns the plan, or why not.
pub fn plan(p: &crate::model::Project) -> Result<Plan, String> {
    let pj = serde_json::to_value(p).map_err(|e| e.to_string())?;
    if pj["color_pipeline"]["enabled"].as_bool().unwrap_or(false) { return Err("colour management is on".into()); }
    if !pristine(&pj["grade_scopes"]["timeline"]) || !pristine(&pj["grade_scopes"]["clip_bindings"]) { return Err("a grade is applied".into()); }
    if !p.links.is_empty() || !p.expr_scripts.is_empty() || !p.hooks.is_empty() { return Err("the project has live links, expressions or hooks".into()); }
    if !p.occluders.is_empty() || p.comp3d.is_some() || p.ocio_view_delivery.is_some() { return Err("the project uses occluders, 3D compositing or an OCIO view".into()); }
    if p.master_speech_level > 0.0 { return Err("master speech levelling is on (the full mixer applies it)".into()); }
    let tracks = pj["tracks"].as_array().ok_or("no tracks")?;
    if tracks.iter().any(|t| t["solo"].as_bool().unwrap_or(false)) { return Err("a track is soloed".into()); }
    let end = super::export_manifest::project_end(p);
    if end <= 0.0 { return Err("empty timeline".into()); }
    let fps = if p.fps > 0.0 { p.fps } else { return Err("the project has no frame rate".into()) };
    let frame = 1.0 / fps;

    // (clip, typed clip, asset path, asset duration, simple, muted)
    let mut pics = vec![]; let mut audio = vec![];
    for (ti, t) in tracks.iter().enumerate() {
        let muted = t["muted"].as_bool().unwrap_or(false);
        for (ci, c) in t["clips"].as_array().into_iter().flatten().enumerate() {
            if c["source"]["type"] != "media" { return Err(format!("clip {} is not plain media", c["id"])); }
            let asset = c["source"]["asset"].as_str().or(c["source"]["asset_id"].as_str()).ok_or("media clip without an asset")?;
            let a = &pj["assets"][asset];
            let sm = clip_simple(c).ok_or_else(|| format!("clip {} has effects, transforms or other settings", c["id"]))?;
            let typed = p.tracks.get(ti).and_then(|t| t.clips.get(ci)).ok_or("clip index mismatch")?;
            if crate::reframe::active(typed) { return Err(format!("clip {} is reframed", c["id"])); }
            let path = p.resolve(a["path"].as_str().ok_or("asset without a path")?);
            let (start, dur, inp) = (c["start"].as_f64().unwrap_or(0.0), c["duration"].as_f64().unwrap_or(0.0), c["in_point"].as_f64().unwrap_or(0.0));
            let silent = muted || c["audio"]["mute"].as_bool().unwrap_or(false);
            if a["width"].as_u64().unwrap_or(0) > 0 {
                if crate::render::is_still_path(&path) { return Err(format!("clip {} is a still image", c["id"])); }
                pics.push((typed.clone(), path, start, dur, inp, a["duration"].as_f64().unwrap_or(0.0), sm, silent));
            } else if !silent {
                audio.push(Aud { path, start, inp, dur, vol: sm.vol, fade_in: sm.fade_in, fade_out: sm.fade_out, seg: None });
            }
        }
    }
    if pics.is_empty() { return Err("no video clip".into()); }
    pics.sort_by(|a, b| a.2.total_cmp(&b.2));
    // the pictures must tile the whole edit: start at 0, no gaps, no overlaps, end where the edit ends
    let mut at = 0.0;
    for (_, _, start, dur, ..) in &pics {
        if (start - at).abs() > frame / 2.0 { return Err(if *start > at { "there is a gap between video clips".into() } else { "video clips overlap".into() }); }
        at = start + dur;
    }
    if (at - end).abs() > frame / 2.0 { return Err("the video clips do not cover the whole edit".into()); }

    let mut probes: HashMap<String, Probe> = HashMap::new();
    for (_, path, ..) in &pics { if !probes.contains_key(path) { probes.insert(path.clone(), probe(path).ok_or_else(|| format!("cannot read the video streams of {path}"))?); } }
    let copy = copy_ok(&pics.iter().map(|(_, path, _, dur, inp, full, ..)| (&probes[path], *inp, *dur, *full)).collect::<Vec<_>>(), p.width, p.height, fps);
    let mode = if copy.is_ok() { Mode::Copy } else { Mode::Encode };
    if mode == Mode::Encode {
        if p.linear_blend { return Err(format!("{}; and linear-light blending is on, which the full renderer applies", copy.as_ref().unwrap_err())); }
        for (_, path, ..) in &pics { if p.hdr_tonemap && crate::render::hdr_source(path).is_some() { return Err("an HDR clip needs tone mapping".into()); } }
    }

    let mut video = vec![];
    for (k, (typed, path, start, dur, inp, _, sm, silent)) in pics.iter().enumerate() {
        let frames = ((start + dur) * fps).round() as u64 - (start * fps).round() as u64;
        let pre = if p.color_tag_sanity && crate::render::mistagged_hd_source(path) { "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709,".to_string() } else { String::new() };
        video.push(Seg { path: path.clone(), start: *start, inp: *inp, dur: *dur, frames, pre,
            fps_filter: crate::render::conform_fps_filter(p, typed, path, 1.0, false, None), fit: crate::render::clip_fit_filter(p.width, p.height, sm.fit, p.linear()) });
        if !silent && probes[path].has_audio { audio.push(Aud { path: path.clone(), start: *start, inp: *inp, dur: *dur, vol: sm.vol, fade_in: sm.fade_in, fade_out: sm.fade_out, seg: Some(k) }); }
    }
    audio.sort_by(|a, b| a.seg.is_none().cmp(&b.seg.is_none()).then(a.start.total_cmp(&b.start)));
    let audio_copy = mode == Mode::Copy && audio_copy_ok(&pics.iter().map(|(_, path, _, dur, _, full, ..)| (&probes[path], *dur, *full)).collect::<Vec<_>>(), &audio, fps);
    Ok(Plan { mode, video, audio, duration: end, fps, preset: p.x264_preset.clone(), why_not_copy: copy.err(), audio_copy })
}

/// The concat demuxer list for stream_copy with several clips. `duration` pins each file's length to its clip, so
/// the next file starts exactly where the timeline says even when a file's audio runs a little longer than its video.
/// With the audio copied too the files keep their own lengths (exactly ffmpeg's concat), so copied sound never overlaps.
pub fn concat_list(pl: &Plan) -> String {
    pl.video.iter().map(|s| { let f = format!("file '{}'\n", s.path.replace('\'', "'\\''")); if pl.audio_copy { f } else { format!("{f}duration {:.6}\n", s.dur) } }).collect()
}

/// The ffmpeg argv for a plan. `list` is where `concat_list` was written (stream_copy with several clips only).
pub fn argv(pl: &Plan, out: &str, list: &str, crf: u32) -> Vec<String> {
    let mut a: Vec<String> = ["-y", "-v", "error", "-nostdin", "-progress", "pipe:1"].map(String::from).to_vec();
    let mut n_inputs = 0usize;
    let mut add = |a: &mut Vec<String>, pre: Vec<String>, path: &str| -> usize { a.extend(pre); a.push("-i".into()); a.push(path.into()); n_inputs += 1; n_inputs - 1 };
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut seg_in: Vec<usize> = vec![];
    match pl.mode {
        Mode::Copy if pl.video.len() == 1 => { let i = add(&mut a, vec![], &pl.video[0].path); by_path.insert(pl.video[0].path.clone(), i); }
        Mode::Copy => { add(&mut a, ["-f", "concat", "-safe", "0"].map(String::from).to_vec(), list); }
        Mode::Encode => for s in &pl.video {
            // seeked like the renderer's own decode; a little extra read so the fps filter always has its last frame
            seg_in.push(add(&mut a, vec!["-ss".into(), format!("{:.6}", s.inp), "-t".into(), format!("{:.6}", (s.frames + 2) as f64 / pl.fps)], &s.path));
        },
    }
    let mut graph = vec![];
    if pl.audio_copy {
        a.extend(["-map", "0:v:0", "-map", "0:a:0", "-c", "copy", "-movflags", "+faststart"].map(String::from));
        a.push(out.into());
        return a;
    }
    for (k, au) in pl.audio.iter().enumerate() {
        let (input, inp) = match (pl.mode, au.seg) {
            (Mode::Encode, Some(s)) => (seg_in[s], 0.0),
            _ => (*by_path.entry(au.path.clone()).or_insert_with(|| add(&mut a, vec![], &au.path)), au.inp),
        };
        let mut f = format!("[{input}:a]atrim=start={inp:.6}:duration={:.6},asetpts=PTS-STARTPTS,aresample=48000,volume={:.6}", au.dur, au.vol);
        if au.fade_in > 0.0 { f.push_str(&format!(",afade=t=in:st=0:d={:.6}", au.fade_in)); }
        if au.fade_out > 0.0 { f.push_str(&format!(",afade=t=out:st={:.6}:d={:.6}", (au.dur - au.fade_out).max(0.0), au.fade_out)); }
        let ms = (au.start * 1000.0).round() as u64;
        if ms > 0 { f.push_str(&format!(",adelay={ms}:all=1")); }
        f.push_str(&format!("[a{k}]"));
        graph.push(f);
    }
    let mixed = !graph.is_empty();
    if mixed {
        let labels: String = (0..graph.len()).map(|k| format!("[a{k}]")).collect();
        graph.push(format!("{labels}amix=inputs={}:normalize=0:duration=longest,apad,atrim=0:{:.6}[aout]", graph.len(), pl.duration));
    }
    if pl.mode == Mode::Encode {
        // the renderer's per-clip decode chain (decode() in render.rs), then its encoder's colour conversion
        for (k, s) in pl.video.iter().enumerate() {
            graph.push(format!("[{}:v]{}setpts=PTS/1,{},tpad=stop_mode=clone:stop=-1,trim=start_frame=0:end_frame={},setpts=PTS-STARTPTS,{},format=rgba[v{k}]", seg_in[k], s.pre, s.fps_filter, s.frames, s.fit));
        }
        let labels: String = (0..pl.video.len()).map(|k| format!("[v{k}]")).collect();
        graph.push(format!("{labels}concat=n={}:v=1:a=0,scale=out_color_matrix=bt709:out_range=tv,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709,format=yuv420p[vout]", pl.video.len()));
    }
    if !graph.is_empty() { a.extend(["-filter_complex".into(), graph.join(";")]); }
    match pl.mode {
        Mode::Copy => a.extend(["-map".into(), "0:v:0".into(), "-c:v".into(), "copy".into()]),
        Mode::Encode => {
            a.extend(["-map".into(), "[vout]".into(), "-c:v".into(), "libx264".into()]);
            if let Some(pr) = &pl.preset { a.extend(["-preset".into(), pr.clone()]); }
            a.extend(["-pix_fmt", "yuv420p", "-color_range", "tv", "-color_primaries", "bt709", "-color_trc", "bt709", "-colorspace", "bt709"].map(String::from));
            a.extend(["-crf".into(), crf.to_string(), "-r".into(), format!("{}", pl.fps)]);
        }
    }
    if mixed { a.extend(["-map".into(), "[aout]".into(), "-c:a".into(), "aac".into(), "-b:a".into(), "192k".into()]); } else { a.push("-an".into()); }
    a.extend(["-t".into(), format!("{:.6}", pl.duration), "-movflags".into(), "+faststart".into(), out.into()]);
    a
}

/// No tools of its own: export_job_start uses it.
pub fn commands() -> Vec<crate::commands::Command> { vec![] }

pub fn mode_name(pl: &Plan) -> &'static str { match pl.mode { Mode::Copy => "stream_copy", Mode::Encode => "direct_encode" } }

pub fn summary(pl: &Plan) -> Value {
    let mut v = json!({"mode": mode_name(pl), "video_clips": pl.video.len(), "audio_clips": pl.audio.len(), "duration": pl.duration});
    if let Some(w) = &pl.why_not_copy { v["why_not_stream_copy"] = json!(w); }
    v["audio"] = json!(if pl.audio_copy { "copied" } else if pl.audio.is_empty() { "none" } else { "mixed" });
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    fn base() -> Value {
        json!({"id":"c1","start":0.0,"duration":5.0,"in_point":0.0,"speed":1.0,"blend":"normal","time_mode":{"mode":"normal"},
            "source":{"type":"media","asset":"a1"},"effects":[],"mask":null,"transition_in":null,"motion_blur":0.0,
            "props":{"opacity":{"base":1.0,"keys":[],"expr":[]},"scale":{"base":1.0,"keys":[],"expr":[]},"x":{"base":0.0,"keys":[],"expr":[]}},
            "audio":{"volume":{"base":0.8,"keys":[],"expr":[]},"fade_in":0.5,"fade_out":0.0,"mute":false,"effects":[],"pitch":0.0,"duck_gain":{"base":1.0,"keys":[],"expr":[]}}})
    }
    #[test]
    fn pristine_and_simple_clips() {
        assert_eq!(clip_simple(&base()), Some(Simple { vol: 0.8, fade_in: 0.5, fade_out: 0.0, fit: 0.0 }));
        let mut fx = base(); fx["effects"] = json!([{"kind":"blur"}]); assert!(clip_simple(&fx).is_none(), "an effect needs a render");
        let mut sc = base(); sc["props"]["scale"]["base"] = json!(1.2); assert!(clip_simple(&sc).is_none(), "a transform needs a render");
        let mut ky = base(); ky["audio"]["volume"]["keys"] = json!([[0,1]]); assert!(clip_simple(&ky).is_none(), "keyed volume is not a plain volume");
        let mut sp = base(); sp["speed"] = json!(2.0); assert!(clip_simple(&sp).is_none());
        let mut unknown = base(); unknown["some_new_feature"] = json!({"on": true}); assert!(clip_simple(&unknown).is_none(), "anything unknown and set disqualifies");
        let mut fit = base(); fit["props"]["fit"] = json!({"base":1.0,"keys":[],"expr":[]}); assert_eq!(clip_simple(&fit).map(|s| s.fit), Some(1.0), "a constant fit is the renderer's own ffmpeg filter");
        let mut kfit = base(); kfit["props"]["fit"] = json!({"base":1.0,"keys":[[0,0]],"expr":[]}); assert!(clip_simple(&kfit).is_none(), "an animated fit is not");
    }

    fn pr(codec: &str, w: u32, h: u32, rate: &str, headers: &str) -> Probe {
        let (a, b) = rate.split_once('/').unwrap();
        Probe { codec: codec.into(), profile: "High".into(), pix_fmt: "yuv420p".into(), width: w, height: h, rate: rate.into(), fps: a.parse::<f64>().unwrap() / b.parse::<f64>().unwrap(), headers: Some(headers.into()), has_audio: true, duration: Some(10.0),
            audio: Some(("aac".into(), "LC".into(), 48000, 2, Some("SHA256:a".into()), Some(10.0))) }
    }
    #[test]
    fn copy_needs_whole_matching_files() {
        let a = pr("h264", 1920, 1080, "30/1", "x"); let b = pr("h264", 1920, 1080, "30/1", "x");
        assert!(copy_ok(&[(&a, 0.0, 10.0, 10.0), (&b, 0.0, 5.0, 5.0)], 1920, 1080, 30.0).is_ok(), "two whole, identical files");
        assert!(copy_ok(&[(&a, 0.0, 10.0, 10.0), (&b, 0.0, 3.0, 5.0)], 1920, 1080, 30.0).is_ok(), "the last clip may end early: the output is cut there");
        assert!(copy_ok(&[(&a, 0.0, 8.0, 10.0), (&b, 0.0, 5.0, 5.0)], 1920, 1080, 30.0).unwrap_err().contains("end"), "a middle clip cut short would leave its tail in");
        assert!(copy_ok(&[(&a, 2.0, 8.0, 10.0)], 1920, 1080, 30.0).unwrap_err().contains("start"), "an in-point needs a keyframe-exact cut");
        let other_cam = pr("h264", 1920, 1080, "30/1", "y");
        assert!(copy_ok(&[(&a, 0.0, 10.0, 10.0), (&other_cam, 0.0, 5.0, 5.0)], 1920, 1080, 30.0).unwrap_err().contains("headers"));
        assert!(copy_ok(&[(&a, 0.0, 10.0, 10.0), (&other_cam, 0.0, 5.0, 5.0)][..1], 1920, 1080, 30.0).is_ok(), "one clip has nothing to match");
        let small = pr("h264", 1280, 720, "30/1", "x");
        assert!(copy_ok(&[(&small, 0.0, 5.0, 5.0)], 1920, 1080, 30.0).is_err(), "a size change needs scaling");
        let fps25 = pr("h264", 1920, 1080, "25/1", "x");
        assert!(copy_ok(&[(&fps25, 0.0, 5.0, 5.0)], 1920, 1080, 30.0).is_err());
        let prores = pr("prores", 1920, 1080, "30/1", "x");
        assert!(copy_ok(&[(&prores, 0.0, 5.0, 5.0)], 1920, 1080, 30.0).is_err());
    }
    #[test]
    fn audio_is_copied_only_when_untouched_and_alike() {
        let a = pr("h264", 1920, 1080, "30/1", "x");
        let au = |k| aud("v.mp4", 0.0, 0.0, 10.0, Some(k));
        let clips = [(&a, 10.0, 10.0), (&a, 10.0, 10.0)];
        assert!(audio_copy_ok(&clips, &[au(0), au(1)], 30.0), "two whole clips, own sound untouched");
        assert!(!audio_copy_ok(&clips, &[au(0)], 30.0), "one clip muted: the gap needs silence");
        assert!(!audio_copy_ok(&clips, &[au(0), Aud { vol: 0.5, ..au(1) }], 30.0), "a volume change needs a mix");
        assert!(!audio_copy_ok(&clips, &[au(0), au(1), aud("music.wav", 0.0, 0.0, 20.0, None)], 30.0), "music needs a mix");
        assert!(!audio_copy_ok(&[(&a, 10.0, 10.0), (&a, 6.0, 10.0)], &[au(0), au(1)], 30.0), "a cut-short last clip would keep its sound past the cut");
        let mut mono = a.clone(); mono.audio.as_mut().unwrap().3 = 1;
        assert!(!audio_copy_ok(&[(&a, 10.0, 10.0), (&mono, 10.0, 10.0)], &[au(0), au(1)], 30.0), "different channel layouts");
        let mut long = a.clone(); long.audio.as_mut().unwrap().5 = Some(10.2);
        assert!(!audio_copy_ok(&[(&long, 10.0, 10.0)], &[au(0)], 30.0), "sound running past the picture would shift the next clip");
        let pl = Plan { mode: Mode::Copy, video: vec![seg("a.mp4", 0.0, 0.0, 10.0), seg("b.mp4", 10.0, 0.0, 10.0)], audio: vec![au(0), au(1)], duration: 20.0, fps: 30.0, preset: None, why_not_copy: None, audio_copy: true };
        let s = argv(&pl, "out.mp4", "list.txt", 18).join(" ");
        assert!(s.ends_with("-f concat -safe 0 -i list.txt -map 0:v:0 -map 0:a:0 -c copy -movflags +faststart out.mp4"), "exactly ffmpeg's own concat: {s}");
        assert_eq!(concat_list(&pl), "file 'a.mp4'\nfile 'b.mp4'\n", "the files keep their own lengths");
        assert_eq!(summary(&pl)["audio"], "copied");
    }
    #[test]
    fn probe_reads_headers_and_audio() {
        let v = json!({"streams":[{"codec_type":"video","codec_name":"h264","profile":"High","pix_fmt":"yuv420p","width":1280,"height":720,"avg_frame_rate":"30000/1001","extradata_hash":"SHA256:ab"},{"codec_type":"audio"}]});
        let p = parse_probe(&v).unwrap();
        assert_eq!((p.width, p.height, p.headers.as_deref(), p.has_audio), (1280, 720, Some("SHA256:ab"), true));
        assert!((p.fps - 29.97).abs() < 0.01);
        assert!(parse_probe(&json!({"streams":[{"codec_type":"audio"}]})).is_none(), "no video stream");
    }

    fn seg(path: &str, start: f64, inp: f64, dur: f64) -> Seg {
        Seg { path: path.into(), start, inp, dur, frames: (dur * 30.0).round() as u64, pre: String::new(), fps_filter: "fps=30".into(), fit: crate::render::clip_fit_filter(1920, 1080, 0.0, false) }
    }
    fn aud(path: &str, start: f64, inp: f64, dur: f64, seg: Option<usize>) -> Aud { Aud { path: path.into(), start, inp, dur, vol: 1.0, fade_in: 0.0, fade_out: 0.0, seg } }
    #[test]
    fn single_clip_copies_and_reads_the_file_once() {
        let pl = Plan { mode: Mode::Copy, video: vec![seg("v.mp4", 0.0, 0.0, 600.0)], audio: vec![aud("v.mp4", 0.0, 0.0, 600.0, Some(0)), Aud { vol: 0.5, fade_out: 2.0, ..aud("a1.wav", 2.0, 1.0, 300.0, None) }], duration: 600.0, fps: 30.0, preset: None, why_not_copy: None, audio_copy: false };
        let a = argv(&pl, "out.mp4", "list.txt", 18); let s = a.join(" ");
        assert!(s.contains("-map 0:v:0 -c:v copy"), "{s}");
        assert!(s.contains("[0:a]atrim=start=0.000000:duration=600.000000"), "the video's own audio is input 0: {s}");
        assert!(s.contains("[1:a]atrim=start=1.000000:duration=300.000000") && s.contains("volume=0.500000") && s.contains("adelay=2000:all=1") && s.contains("afade=t=out:st=298.000000:d=2.000000"), "{s}");
        assert!(s.contains("amix=inputs=2:normalize=0"), "{s}");
        assert_eq!(a.iter().filter(|x| *x == "-i").count(), 2, "the video file is read once");
        assert!(!s.contains("libx264") && !s.contains("concat"), "{s}");
        let silent = Plan { mode: Mode::Copy, video: vec![seg("v.mp4", 0.0, 0.0, 10.0)], audio: vec![], duration: 10.0, fps: 30.0, preset: None, why_not_copy: None, audio_copy: false };
        assert!(argv(&silent, "o.mp4", "l.txt", 18).contains(&"-an".to_string()));
    }
    #[test]
    fn several_whole_clips_join_with_the_concat_demuxer() {
        let pl = Plan { mode: Mode::Copy, video: vec![seg("a.mp4", 0.0, 0.0, 10.0), seg("it's.mp4", 10.0, 0.0, 5.0)], audio: vec![aud("a.mp4", 0.0, 0.0, 10.0, Some(0)), aud("it's.mp4", 10.0, 0.0, 5.0, Some(1))], duration: 15.0, fps: 30.0, preset: None, why_not_copy: None, audio_copy: false };
        let a = argv(&pl, "out.mp4", "list.txt", 18); let s = a.join(" ");
        assert!(s.contains("-f concat -safe 0 -i list.txt"), "{s}");
        assert!(s.contains("-map 0:v:0 -c:v copy") && !s.contains("libx264"), "{s}");
        assert!(s.contains("[1:a]atrim") && s.contains("[2:a]atrim") && s.contains("adelay=10000:all=1"), "each clip's sound placed at its start: {s}");
        assert_eq!(concat_list(&pl), "file 'a.mp4'\nduration 10.000000\nfile 'it'\\''s.mp4'\nduration 5.000000\n", "quotes escaped, durations pinned");
    }
    #[test]
    fn trimmed_or_mixed_clips_encode_in_one_ffmpeg() {
        let pl = Plan { mode: Mode::Encode, video: vec![seg("a.mp4", 0.0, 3.0, 4.0), seg("b.mov", 4.0, 0.0, 2.0)], audio: vec![aud("a.mp4", 0.0, 3.0, 4.0, Some(0)), aud("m.wav", 0.0, 0.0, 6.0, None)], duration: 6.0, fps: 30.0, preset: Some("slow".into()), why_not_copy: Some("a clip is trimmed at its start".into()), audio_copy: false };
        let a = argv(&pl, "out.mp4", "unused", 20); let s = a.join(" ");
        assert!(s.contains("-ss 3.000000 -t 4.066667 -i a.mp4"), "seeked input, like the renderer's decode: {s}");
        assert!(s.contains("[0:a]atrim=start=0.000000:duration=4.000000"), "the clip's sound comes from its already-seeked input: {s}");
        assert!(s.contains("[2:a]atrim"), "a separate audio file is its own input: {s}");
        assert!(s.contains("[0:v]setpts=PTS/1,fps=30,tpad=stop_mode=clone:stop=-1,trim=start_frame=0:end_frame=120,setpts=PTS-STARTPTS,scale=1920:1080:force_original_aspect_ratio=decrease"), "{s}");
        assert!(s.contains("[v0][v1]concat=n=2:v=1:a=0,scale=out_color_matrix=bt709:out_range=tv,setparams=range=limited:color_primaries=bt709:color_trc=bt709:colorspace=bt709,format=yuv420p[vout]"), "{s}");
        assert!(s.contains("-map [vout] -c:v libx264 -preset slow -pix_fmt yuv420p") && s.contains("-crf 20 -r 30"), "the normal export's encoder settings: {s}");
        assert!(!s.contains("-c:v copy"), "{s}");
        assert_eq!(summary(&pl)["mode"], "direct_encode");
    }
}
