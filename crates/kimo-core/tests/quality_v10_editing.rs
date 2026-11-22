//! V10 Workstream 5: the core editing tools, measured. Every test builds a synthetic input with a KNOWN answer and
//! checks the tool's result against it, printing a `V10Q::` line with the before/after numbers:
//! - finish_audio: delivered integrated loudness within +-0.5 LU of the target and true peak <= -1 dBTP; the
//!   adaptive chain denoises the noisy clip only; a beat-driven music clip on a video track is called music.
//! - cuts: no click at any cut auto_rough_cut makes (the crossfade), measured against a hard splice of the same cut.
//! - match_shot_look: mean shot-to-shot CIEDE2000 < 3 on a colour chart shot under a warm cast and underexposed.
//! - auto_grade: skin-hue error < 5 deg on a cool-cast portrait stand-in, and its grey background back to neutral.
//! - captions: reading speed, line length, minimum/maximum duration, no orphans, timing snapped to the words.
//! - duck_timeline: eased (S-curve) ramps by default.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::caption_engine::*;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_q10_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ffmpeg(args: &[&str]) { let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stdin(Stdio::null()).status().expect("ffmpeg"); assert!(st.success(), "ffmpeg {args:?}"); }
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }
fn proj(w: u32, h: u32) -> Session { let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": w, "height": h, "fps": 30})); ss }

// ------------------------------------------------------------------------------------------------ audio fixtures
/// A talking-head stand-in: a voice-like tone (harmonics, syllable-rate wobble) in `speech` windows at `level`, over a
/// noise floor of amplitude `noise`, plus an optional steady 180 Hz hum (`hum`), so a splice in a pause is audible.
fn talk(name: &str, dur: f64, speech: &[(f64, f64)], level: f64, noise: f64, hum: f64) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    let gate: Vec<String> = speech.iter().map(|(s, e)| format!("between(t,{s},{e})")).collect();
    let a = format!("aevalsrc='({})*{level}*(0.25*sin(2*PI*140*t)+0.15*sin(2*PI*280*t+1)+0.1*sin(2*PI*420*t+2)+0.05*sin(2*PI*1900*t)+0.02*sin(2*PI*3300*t))*(0.55+0.45*sin(2*PI*4.3*t))+{hum}*sin(2*PI*180*t)':s=48000:d={dur}", gate.join("+"));
    ffmpeg(&["-f", "lavfi", "-i", &format!("testsrc2=s=320x180:r=30:d={dur}"), "-f", "lavfi", "-i", &a, "-f", "lavfi", "-i", &format!("anoisesrc=a={noise}:d={dur}:r=48000"),
        "-filter_complex", "[1:a][2:a]amix=inputs=2:normalize=0[a]", "-map", "0:v", "-map", "[a]", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "pcm_s16le", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
fn wav(name: &str, pcm: &[f32]) -> String { let p = dir().join(name); kimo_core::audio::write_wav(p.to_str().unwrap(), pcm).unwrap(); p.to_string_lossy().into_owned() }
/// A beat-driven music stand-in at 120 BPM: a 55 Hz kick on every beat, a bright noise hat on the off-beats and a
/// soft chord, i.e. energy well outside the voice band and a steady pulse.
fn music_pcm(dur: f64) -> Vec<f32> {
    let n = (dur * 48000.0) as usize; let mut seed = 12345u64; let mut prev = 0.0f64;
    (0..n).flat_map(|i| {
        let t = i as f64 / 48000.0; let tau = std::f64::consts::TAU;
        let ph = t % 0.5; let off = (t + 0.25) % 0.5;
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); let r = ((seed >> 33) as f64 / (1u64 << 31) as f64) - 1.0;
        let hp = r - prev; prev = r; // crude high-pass: a bright hat
        let x = 0.55 * (tau * 55.0 * t).sin() * (-14.0 * ph).exp() + 0.18 * hp * (-40.0 * off).exp() + 0.04 * ((tau * 220.0 * t).sin() + (tau * 277.2 * t).sin() + (tau * 329.6 * t).sin());
        let v = x as f32; [v, v]
    }).collect()
}

#[test] fn finish_audio_hits_the_loudness_target_and_the_true_peak_ceiling() {
    let mut ss = proj(320, 180); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let clean = sid(&call(&mut ss, "import_media", json!({"path": talk("fa_clean.mov", 4.0, &[(0.3, 1.6), (2.2, 3.7)], 1.0, 0.0005, 0.0)})), "asset_id");
    let noisy = sid(&call(&mut ss, "import_media", json!({"path": talk("fa_noisy.mov", 4.0, &[(0.3, 1.7), (2.3, 3.7)], 0.35, 0.03, 0.0)})), "asset_id");
    let c1 = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": clean, "start": 0, "duration": 4, "fit": "cover"})), "clip_id");
    let c2 = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": noisy, "start": 4, "duration": 4, "fit": "cover"})), "clip_id");
    let m = sid(&call(&mut ss, "import_media", json!({"path": wav("fa_bed.wav", &music_pcm(8.0))})), "asset_id");
    call(&mut ss, "add_audio_clip", json!({"asset_id": m, "start": 0, "duration": 8}));
    let raw = call(&mut ss, "measure_loudness", json!({"true_peak": true}));
    for target in [-14.0, -16.0] {
        let r = call(&mut ss, "finish_audio", json!({"target_lufs": target}));
        let got = call(&mut ss, "measure_loudness", json!({"true_peak": true}));
        let (l, tp) = (got["integrated_lufs"].as_f64().unwrap(), got["true_peak_dbfs"].as_f64().unwrap());
        eprintln!("V10Q:: finish_audio target {target}: raw mix {:.2} LUFS tp {:.2} -> delivered {l:.2} LUFS (error {:.2} LU), true peak {tp:.2} dBTP", raw["integrated_lufs"].as_f64().unwrap_or(f64::NAN), raw["true_peak_dbfs"].as_f64().unwrap_or(f64::NAN), l - target);
        assert!((l - target).abs() <= 0.5, "delivered {l:.2} LUFS, target {target}: {r}");
        assert!(tp <= -1.0, "true peak {tp:.2} dBTP above -1: {r}");
        // the chain is adaptive: the noisy clip is denoised, the clean one is not; both get the limiter; a re-run replaces it
        let chain = |cid: &str| r["dialogue_chains"].as_array().unwrap().iter().find(|c| c["clip_id"] == json!(cid)).map(|c| c["chain"].as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap().to_string()).collect::<Vec<_>>()).unwrap_or_default();
        assert!(chain(&c2).contains(&"denoise".to_string()), "noisy clip not denoised: {r}");
        assert!(!chain(&c1).contains(&"denoise".to_string()), "clean clip denoised for nothing: {r}");
        for c in [&c1, &c2] { assert!(chain(c).contains(&"limiter".to_string()) && chain(c).contains(&"deesser".to_string()) && chain(c).contains(&"compressor".to_string()), "{r}"); }
        let n_fx = ss.project.find_clip(&c2).unwrap().audio.effects.len(); assert_eq!(n_fx, chain(&c2).len(), "a re-run must replace finish_audio's chain, not stack it");
    }
}

#[test] fn finish_audio_calls_a_beat_driven_clip_on_a_video_track_music() {
    let p = dir().join("fa_musicvideo.mov");
    if !p.exists() { let w = wav("fa_mv.wav", &music_pcm(5.0)); ffmpeg(&["-f", "lavfi", "-i", "testsrc2=s=320x180:r=30:d=5", "-i", &w, "-map", "0:v", "-map", "1:a", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "pcm_s16le", "-shortest", p.to_str().unwrap()]); }
    let mut ss = proj(320, 180); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk("fa_mv_talk.mov", 4.0, &[(0.2, 3.8)], 1.0, 0.0005, 0.0)})), "asset_id");
    let b = sid(&call(&mut ss, "import_media", json!({"path": p.to_str().unwrap()})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 4, "fit": "cover"}));
    let cm = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": b, "start": 4, "duration": 5, "fit": "cover"})), "clip_id");
    let r = call(&mut ss, "finish_audio", json!({}));
    let cls = r["classification"].as_array().unwrap().iter().find(|c| c["clip_id"] == json!(cm)).cloned().unwrap();
    eprintln!("V10Q:: classification of a beat-driven music clip on a video track: {cls} (before V10: speech ratio only -> ambience, muted)");
    assert_eq!(cls["class"], json!("music"), "{r}");
    assert!(!ss.project.find_clip(&cm).unwrap().audio.mute, "a music clip must not be muted as ambience: {r}");
}

// ------------------------------------------------------------------------------------------------ cuts
/// Click score at each cut: the largest second difference of the mixed waveform within 3 ms of the cut, over the
/// 99th percentile of the second difference in the pause material just around it (6..60 ms either side): 1.0 = the
/// cut is no rougher than the room tone it sits in; a hard splice through a hum reads many times that.
fn click_scores(pcm: &[f32], cuts: &[f64]) -> Vec<f64> {
    let mono: Vec<f64> = pcm.chunks_exact(2).map(|p| 0.5 * (p[0] + p[1]) as f64).collect();
    let d2: Vec<f64> = (2..mono.len()).map(|i| (mono[i] - 2.0 * mono[i - 1] + mono[i - 2]).abs()).collect();
    cuts.iter().map(|c| {
        let i = ((c * 48000.0) as usize).saturating_sub(2);
        let mut around: Vec<f64> = (i.saturating_sub(2880)..i.saturating_sub(288)).chain(i + 288..(i + 2880).min(d2.len())).map(|k| d2[k]).collect(); around.sort_by(f64::total_cmp);
        let ref_v = around[(around.len() as f64 * 0.99) as usize].max(1e-9);
        d2[i.saturating_sub(144)..(i + 144).min(d2.len())].iter().cloned().fold(0.0, f64::max) / ref_v
    }).collect()
}
#[test] fn auto_rough_cut_pads_words_and_its_cuts_do_not_click() {
    let src = talk("cut_hum.mov", 6.5, &[(0.3, 1.5), (2.6, 3.6), (4.8, 6.0)], 1.0, 0.0005, 0.03);
    let run = |xfade: Option<f64>| -> (Vec<f64>, Value, Session) {
        let mut ss = proj(320, 180);
        let a = sid(&call(&mut ss, "import_media", json!({"path": src})), "asset_id");
        let mut args = json!({"asset_ids": [a]}); if let Some(x) = xfade { args["crossfade_ms"] = json!(x); }
        let r = call(&mut ss, "auto_rough_cut", args);
        let tid = sid(&r, "track_id");
        let cuts: Vec<f64> = ss.project.tracks.iter().find(|t| t.id == tid).unwrap().clips.iter().skip(1).map(|c| c.start).collect();
        assert!(cuts.len() >= 2, "the two pauses should be cut: {r}");
        let pcm = kimo_core::audio::mix(&ss.project);
        (click_scores(&pcm, &cuts), r, ss)
    };
    let (hard, _, _) = run(Some(0.0));
    let (soft, r, _) = run(None);
    eprintln!("V10Q:: click score at cuts (1.0 = like the material): hard splice {hard:.1?} -> with the {} ms crossfade {soft:.1?}", 8);
    assert!(soft.iter().all(|s| *s <= 2.0), "a cut still clicks: {soft:?} ({r})");
    assert!(hard.iter().any(|s| *s > 4.0), "the fixture should click when hard-spliced, or the measurement proves nothing: {hard:?}");
    // word padding: every kept segment keeps >= 80 ms before the word onset and >= 120 ms after its tail (VAD-relative)
    let segs = r["segments"].as_array().unwrap();
    for (s, (ws, we)) in segs.iter().zip([(0.3, 1.5), (2.6, 3.6), (4.8, 6.0)]) {
        let (a, b) = (s["source"][0].as_f64().unwrap(), s["source"][1].as_f64().unwrap());
        assert!(a <= ws - 0.01 || a <= 0.001, "onset {ws} clipped: segment starts {a}: {r}");
        assert!(b >= we + 0.12 || b >= 6.49, "tail {we} not padded: segment ends {b}: {r}");
    }
}

// ------------------------------------------------------------------------------------------------ colour
fn png(name: &str, w: usize, h: usize, px: impl Fn(usize, usize) -> [f64; 3]) -> String {
    let p = dir().join(name);
    let mut raw = Vec::with_capacity(w * h * 3); for y in 0..h { for x in 0..w { for v in px(x, y) { raw.push((v.clamp(0.0, 1.0) * 255.0).round() as u8); } } }
    let mut ch = Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{w}x{h}"), "-i", "-", "-frames:v", "1", p.to_str().unwrap()]).stdin(Stdio::piped()).spawn().expect("ffmpeg");
    use std::io::Write; ch.stdin.take().unwrap().write_all(&raw).unwrap(); assert!(ch.wait().unwrap().success());
    let v = dir().join(format!("{name}.mp4"));
    ffmpeg(&["-loop", "1", "-i", p.to_str().unwrap(), "-t", "2", "-r", "30", "-c:v", "libx264", "-preset", "ultrafast", "-crf", "4", "-pix_fmt", "yuv420p", v.to_str().unwrap()]);
    v.to_string_lossy().into_owned()
}
fn dec(v: f64) -> f64 { if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }
fn enc(v: f64) -> f64 { let v = v.clamp(0.0, 1.0); if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 } }
/// A cast and an exposure change applied the way a camera's white balance and shutter would: in linear light.
fn cast(c: [f64; 3], k: [f64; 3]) -> [f64; 3] { [0, 1, 2].map(|j| enc(dec(c[j]) * k[j])) }
fn lab(c: [f64; 3]) -> [f64; 3] {
    let l = c.map(dec);
    let (x, y, z) = (0.4124 * l[0] + 0.3576 * l[1] + 0.1805 * l[2], 0.2126 * l[0] + 0.7152 * l[1] + 0.0722 * l[2], 0.0193 * l[0] + 0.1192 * l[1] + 0.9505 * l[2]);
    let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
    let (fx, fy, fz) = (f(x / 0.95047), f(y), f(z / 1.08883));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}
/// CIEDE2000 (Sharma, Wu & Dalal 2005).
fn de2000(c1: [f64; 3], c2: [f64; 3]) -> f64 {
    let (l1, a1, b1) = (c1[0], c1[1], c1[2]); let (l2, a2, b2) = (c2[0], c2[1], c2[2]);
    let cb = (a1.hypot(b1) + a2.hypot(b2)) / 2.0; let g = 0.5 * (1.0 - (cb.powi(7) / (cb.powi(7) + 25f64.powi(7))).sqrt());
    let (a1p, a2p) = (a1 * (1.0 + g), a2 * (1.0 + g)); let (c1p, c2p) = (a1p.hypot(b1), a2p.hypot(b2));
    let hp = |b: f64, a: f64| if b == 0.0 && a == 0.0 { 0.0 } else { b.atan2(a).to_degrees().rem_euclid(360.0) };
    let (h1, h2) = (hp(b1, a1p), hp(b2, a2p));
    let dl = l2 - l1; let dc = c2p - c1p;
    let dh = if c1p * c2p == 0.0 { 0.0 } else if (h2 - h1).abs() <= 180.0 { h2 - h1 } else if h2 - h1 > 180.0 { h2 - h1 - 360.0 } else { h2 - h1 + 360.0 };
    let dhh = 2.0 * (c1p * c2p).sqrt() * (dh.to_radians() / 2.0).sin();
    let lb = (l1 + l2) / 2.0; let cbp = (c1p + c2p) / 2.0;
    let hb = if c1p * c2p == 0.0 { h1 + h2 } else if (h1 - h2).abs() <= 180.0 { (h1 + h2) / 2.0 } else if h1 + h2 < 360.0 { (h1 + h2 + 360.0) / 2.0 } else { (h1 + h2 - 360.0) / 2.0 };
    let t = 1.0 - 0.17 * (hb - 30.0).to_radians().cos() + 0.24 * (2.0 * hb).to_radians().cos() + 0.32 * (3.0 * hb + 6.0).to_radians().cos() - 0.20 * (4.0 * hb - 63.0).to_radians().cos();
    let dth = 30.0 * (-((hb - 275.0) / 25.0).powi(2)).exp(); let rc = 2.0 * (cbp.powi(7) / (cbp.powi(7) + 25f64.powi(7))).sqrt();
    let sl = 1.0 + 0.015 * (lb - 50.0).powi(2) / (20.0 + (lb - 50.0).powi(2)).sqrt(); let sc = 1.0 + 0.045 * cbp; let sh = 1.0 + 0.015 * cbp * t; let rt = -(2.0 * dth.to_radians()).sin() * rc;
    ((dl / sl).powi(2) + (dc / sc).powi(2) + (dhh / sh).powi(2) + rt * (dc / sc) * (dhh / sh)).sqrt()
}
/// Mean display RGB of the rendered timeline in a box (fractions of the frame) at time t.
fn sample(ss: &Session, t: f64, bx: [f64; 4]) -> [f64; 3] {
    let p = &ss.project; let mut cache = kimo_core::render::Cache::new(); kimo_core::render::prepare_at(p, &mut cache, Some(t)).unwrap();
    let f = kimo_core::render::render_frame(p, t, &cache).unwrap();
    let (x0, y0, x1, y1) = ((bx[0] * f.w as f64) as usize, (bx[1] * f.h as f64) as usize, ((bx[0] + bx[2]) * f.w as f64) as usize, ((bx[1] + bx[3]) * f.h as f64) as usize);
    let mut s = [0.0; 3]; let mut n = 0.0;
    for y in y0..y1 { for x in x0..x1 { let i = (y * f.w + x) * 4; for j in 0..3 { s[j] += f.d[i + j] as f64; } n += 1.0; } }
    s.map(|v| v / n)
}
fn hue709(c: [f64; 3]) -> f64 { let y = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]; ((c[0] - y) / 1.5748).atan2((c[2] - y) / 1.8556).to_degrees().rem_euclid(360.0) }
fn ang(a: f64, b: f64) -> f64 { let d = (a - b).rem_euclid(360.0); d.min(360.0 - d) }

const CHART: [[f64; 3]; 12] = [[0.2, 0.2, 0.2], [0.45, 0.45, 0.45], [0.7, 0.7, 0.7], [0.88, 0.88, 0.88], [0.80, 0.60, 0.48], [0.55, 0.38, 0.28],
    [0.70, 0.22, 0.20], [0.25, 0.58, 0.30], [0.22, 0.30, 0.68], [0.84, 0.74, 0.26], [0.26, 0.62, 0.68], [0.62, 0.30, 0.58]];
fn chart_px(k: [f64; 3]) -> impl Fn(usize, usize) -> [f64; 3] { move |x, y| { let i = (y / 120) * 4 + x / 160; cast(CHART[i.min(11)], k) } }
fn patch_box(i: usize) -> [f64; 4] { let (cx, cy) = ((i % 4) as f64 * 0.25 + 0.125, (i / 4) as f64 / 3.0 + 1.0 / 6.0); [cx - 0.06, cy - 0.08, 0.12, 0.16] }

#[test] fn match_shot_look_brings_a_cast_shot_within_3_de2000_of_the_reference() {
    let mut ss = proj(640, 360); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let r_a = sid(&call(&mut ss, "import_media", json!({"path": png("chart_ref.png", 640, 360, chart_px([1.0, 1.0, 1.0]))})), "asset_id");
    // the same scene shot warm (tungsten white balance) and a stop under
    let t_a = sid(&call(&mut ss, "import_media", json!({"path": png("chart_warm.png", 640, 360, chart_px([1.25 * 0.6, 0.6, 0.72 * 0.6]))})), "asset_id");
    let cref = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": r_a, "start": 0, "duration": 1, "fit": "cover"})), "clip_id");
    let ctgt = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": t_a, "start": 1, "duration": 1, "fit": "cover"})), "clip_id");
    let de = |ss: &Session| -> (f64, f64) { let v: Vec<f64> = (0..12).map(|i| de2000(lab(sample(ss, 0.5, patch_box(i))), lab(sample(ss, 1.5, patch_box(i))))).collect(); (v.iter().sum::<f64>() / 12.0, v.iter().cloned().fold(0.0, f64::max)) };
    let (before, before_max) = de(&ss);
    let r = call(&mut ss, "match_shot_look", json!({"reference_clip_ids": [cref.clone()], "target_clip_ids": [ctgt.clone()], "strength": 1.0}));
    let (after, after_max) = de(&ss);
    eprintln!("V10Q:: match_shot_look shot-to-shot CIEDE2000 over 12 patches: before mean {before:.2} (max {before_max:.2}) -> after mean {after:.2} (max {after_max:.2})");
    assert!(before > 8.0, "the fixture should start far apart: {before}");
    assert!(after < 3.0, "mean CIEDE2000 {after:.2} after matching: {r}");
    // a re-run replaces its own effect
    call(&mut ss, "match_shot_look", json!({"reference_clip_ids": [cref.clone()], "target_clip_ids": [ctgt.clone()], "strength": 1.0}));
    assert_eq!(ss.project.find_clip(&ctgt).unwrap().effects.iter().filter(|e| e.kind == "lab_match").count(), 1);
}

#[test] fn auto_grade_balances_in_linear_light_and_keeps_skin_on_its_hue() {
    // a portrait stand-in: a neutral grey wall (gradient), a skin-toned face block and two small coloured objects
    let scene = |x: usize, y: usize| -> [f64; 3] {
        let (fx, fy) = (x as f64 / 640.0, y as f64 / 360.0);
        if (0.38..0.62).contains(&fx) && (0.25..0.8).contains(&fy) { return [0.80, 0.60, 0.48]; }
        if (0.06..0.16).contains(&fx) && (0.7..0.9).contains(&fy) { return [0.70, 0.22, 0.20]; }
        if (0.84..0.94).contains(&fx) && (0.1..0.3).contains(&fy) { return [0.22, 0.45, 0.30]; }
        let g = 0.3 + 0.4 * fx; [g, g, g]
    };
    let mut ss = proj(640, 360); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let clean = sid(&call(&mut ss, "import_media", json!({"path": png("skin_clean.png", 640, 360, scene)})), "asset_id");
    let cool = sid(&call(&mut ss, "import_media", json!({"path": png("skin_cool.png", 640, 360, move |x, y| cast(scene(x, y), [0.78, 1.0, 1.25]))})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": clean, "start": 0, "duration": 1, "fit": "cover"}));
    let cc = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": cool, "start": 1, "duration": 1, "fit": "cover"})), "clip_id");
    let face = [0.42, 0.35, 0.16, 0.35]; let wall = [0.22, 0.05, 0.1, 0.15];
    let truth = hue709(sample(&ss, 0.5, face));
    let h_before = hue709(sample(&ss, 1.5, face)); let wall_before = sample(&ss, 1.5, wall);
    let r = call(&mut ss, "auto_grade", json!({"clip_ids": [cc]}));
    let h_after = hue709(sample(&ss, 1.5, face)); let w = sample(&ss, 1.5, wall);
    eprintln!("V10Q:: auto_grade skin-hue error vs the uncast shot: before {:.1} deg -> after {:.1} deg; grey wall cast (max |R-G|,|B-G|) {:.3} -> {:.3}",
        ang(h_before, truth), ang(h_after, truth), (wall_before[0] - wall_before[1]).abs().max((wall_before[2] - wall_before[1]).abs()), (w[0] - w[1]).abs().max((w[2] - w[1]).abs()));
    assert!(ang(h_after, truth) < 5.0, "skin hue {h_after:.1} vs {truth:.1}: {r}");
    assert!((w[0] - w[1]).abs() < 0.03 && (w[2] - w[1]).abs() < 0.03, "the grey wall is not neutral after: {w:?} ({r})");
}

// ------------------------------------------------------------------------------------------------ captions
/// A natural transcript with ASR-like timings: word length drives duration (~14 chars/s), commas and full stops leave
/// short pauses, and one long thinking pause sits mid-sentence.
fn transcript() -> Vec<EWord> {
    let text = "So the first thing we did, before anything else, was walk the whole site with the team. It took most of a morning. \
        Honestly I thought it would be a waste of time, but it changed how we planned every single room in the building. \
        We found two places where the light was wrong and one where the floor sloped toward the door. Fixing those early saved us weeks. \
        If you take one idea from this, it is that the walk comes first and the plan comes after it.";
    let mut t = 0.4; let mut out = vec![];
    for (i, w) in text.split_whitespace().enumerate() {
        let d = 0.06 * w.trim_matches(|c: char| !c.is_alphanumeric()).len() as f64 + 0.12;
        out.push(EWord { text: w.to_string(), start: t, end: t + d, conf: 0.95 });
        t += d + if w.ends_with('.') { 0.6 } else if w.ends_with(',') { 0.25 } else { 0.05 };
        if i == 40 { t += 1.0; }
    }
    out
}
struct Violations { long_line: usize, orphan_line: usize, orphan_unit: usize, fast: usize, short: usize, too_long: usize, unsnapped: usize, units: usize }
fn audit(words: &[EWord], timed: &[TimedUnit], max_chars: usize, max_cps: f64, min_d: f64, max_d: f64, lead: f64) -> Violations {
    let mut v = Violations { long_line: 0, orphan_line: 0, orphan_unit: 0, fast: 0, short: 0, too_long: 0, unsnapped: 0, units: timed.len() };
    for (k, tu) in timed.iter().enumerate() {
        let u = &tu.unit; let mut bounds = vec![u.first]; for b in &u.breaks_after { bounds.push(u.first + b + 1); } bounds.push(u.last + 1);
        for w in bounds.windows(2) { if span_chars(words, w[0], w[1] - 1) > max_chars { v.long_line += 1; } if w[1] - w[0] == 1 && u.last > u.first { v.orphan_line += 1; } }
        if u.last == u.first {
            let sep_before = k == 0 || words[u.first - 1].text.ends_with('.') || words[u.first].start - words[u.first - 1].end > 0.45;
            let sep_after = k + 1 == timed.len() || words[u.last].text.ends_with('.') || words[u.last + 1].start - words[u.last].end > 0.45;
            if !(sep_before && sep_after) { v.orphan_unit += 1; }
        }
        let dur = tu.end - tu.start;
        if reading_chars(words, u.first, u.last) as f64 / dur > max_cps + 1e-9 { v.fast += 1; }
        let capped_by_next = timed.get(k + 1).map_or(false, |n| (tu.end - n.start).abs() < 0.03);
        if dur < min_d - 1e-9 && !capped_by_next { v.short += 1; }
        if dur > max_d + 1e-9 { v.too_long += 1; }
        if tu.start > words[u.first].start + 1e-9 || tu.start < words[u.first].start - lead - 1e-9 || tu.end < words[u.last].end - 1e-9 { v.unsnapped += 1; }
    }
    v
}
#[test] fn captions_meet_reading_speed_line_length_duration_and_orphan_rules() {
    let w = transcript();
    let units = segment(&w, &SegmentSpec::default()).unwrap();
    let timed = time_units(&units, &w, &TimingSpec::default(), None);
    let v = audit(&w, &timed, 42, 20.0, 0.83, 7.0, 0.04);
    // the V8.12 rules on the same words (what every caller got before V10)
    let old_units = segment(&w, &SegmentSpec { max_chars_per_line: 20, phrase_aware: false, ..Default::default() }).unwrap();
    let old = time_units(&old_units, &w, &TimingSpec { min_duration_s: 0.2, reading_rules: false, ..Default::default() }, None);
    let o = audit(&w, &old, 42, 20.0, 0.83, 7.0, 0.04);
    eprintln!("V10Q:: captions on {} words: units {} -> {}; lines over 42 chars {} -> {}; one-word lines {} -> {}; orphan one-word cues {} -> {}; cues over 20 cps {} -> {}; cues under 0.83 s {} -> {}; over 7 s {} -> {}; unsnapped {} -> {}",
        w.len(), o.units, v.units, o.long_line, v.long_line, o.orphan_line, v.orphan_line, o.orphan_unit, v.orphan_unit, o.fast, v.fast, o.short, v.short, o.too_long, v.too_long, o.unsnapped, v.unsnapped);
    for tu in &timed { let u = &tu.unit; eprintln!("  [{:.2}-{:.2}] {}", tu.start, tu.end, compose_lines(&w, u)); }
    assert_eq!((v.long_line, v.orphan_line, v.orphan_unit, v.fast, v.short, v.too_long, v.unsnapped), (0, 0, 0, 0, 0, 0, 0), "caption rule violations");
    // lines read as phrases: no line ends on an article or possessive when the unit wraps
    for tu in &timed { let u = &tu.unit; for b in &u.breaks_after { let end = w[u.first + b].text.to_lowercase(); assert!(!["a", "an", "the", "my", "our", "of", "to"].contains(&end.as_str()), "line ends on '{end}': {}", compose_lines(&w, u)); } }
    for win in timed.windows(2) { assert!(win[0].end <= win[1].start + 1e-9, "overlapping cues"); }
}
fn compose_lines(w: &[EWord], u: &Unit) -> String {
    let mut s = String::new(); for i in u.first..=u.last { s.push_str(&w[i].text); if u.breaks_after.contains(&(i - u.first)) { s.push_str(" | "); } else { s.push(' '); } } s
}

// ------------------------------------------------------------------------------------------------ ducking
#[test] fn duck_timeline_ramps_are_eased_by_default() {
    let voice: Vec<f32> = (0..(6.0 * 48000.0) as usize).flat_map(|i| { let t = i as f64 / 48000.0; let tau = std::f64::consts::TAU;
        let x = if (2.0..4.0).contains(&t) { 0.4 * ((tau * 440.0 * t).sin() + 0.6 * (tau * 900.0 * t).sin() + 0.4 * (tau * 1700.0 * t).sin()) * (0.7 + 0.3 * (tau * 9.0 * t).sin()) } else { 0.0 }; [x as f32, x as f32] }).collect();
    let mut ss = Session::new();
    let va = call(&mut ss, "import_media", json!({"path": wav("duck_vo.wav", &voice)}))["asset_id"].clone();
    let d = call(&mut ss, "add_audio_clip", json!({"asset_id": va, "start": 0, "duration": 6}));
    let ma = call(&mut ss, "import_media", json!({"path": wav("duck_bed.wav", &music_pcm(6.0))}))["asset_id"].clone();
    let m = call(&mut ss, "add_audio_clip", json!({"asset_id": ma, "start": 0, "duration": 6}));
    call(&mut ss, "duck_timeline", json!({"dialogue_tracks": [d["track_id"]], "music_tracks": [m["track_id"]], "amount_db": -12.0}));
    let g = &ss.project.find_clip(m["clip_id"].as_str().unwrap()).unwrap().audio.duck_gain;
    assert!(g.keys.iter().all(|k| matches!(k.ease, kimo_core::keyframe::Ease::InOut)), "ramps should be eased: {:?}", g.keys);
    // the steepest change per 10 ms: eased 0.35 s / 0.8 s ramps against the old linear 0.1 s / 0.3 s
    let steepest = (0..600).map(|i| (g.at(i as f64 * 0.01 + 0.01) - g.at(i as f64 * 0.01)).abs()).fold(0.0, f64::max);
    let old_linear = (1.0 - 10f64.powf(-12.0 / 20.0)) * 0.01 / 0.1;
    eprintln!("V10Q:: duck_timeline steepest gain change per 10 ms: old linear 0.1 s attack {old_linear:.4} -> eased {steepest:.4}");
    assert!(steepest < old_linear, "ramps are no smoother than before");
    assert!(g.at(3.0) < 0.3 && g.at(5.5) > 0.95, "still ducks under the voice and recovers");
}
