//! v10 Workstream 6: review hears the mix and sees the grade. Seeded mistakes (a -6 LUFS mix, a clipped clip, loud
//! music over speech, a shot that doesn't match its scene, a strong cast, a long unblurred slide) must each be found
//! with the call that fixes it, and clean projects must come back with no findings in that domain.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::f64::consts::TAU;

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn tmp(name: &str, ext: &str) -> String { format!("{}/kimo-q10-{}-{name}.{ext}", std::env::temp_dir().display().to_string().replace('\\', "/"), std::process::id()) }
fn base() -> Session { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 48, "fps": 25})); ss }
fn track(ss: &mut Session) -> String { c(ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string() }

/// Stereo 16-bit 48 kHz WAV from a sample function (seconds -> -1..1).
fn wav(path: &str, secs: f64, f: impl Fn(f64) -> f64) {
    let n = (secs * 48000.).round() as usize;
    let data = (n * 4) as u32; let mut b = Vec::with_capacity(44 + data as usize);
    b.extend(b"RIFF"); b.extend((36 + data).to_le_bytes()); b.extend(b"WAVEfmt "); b.extend(16u32.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(2u16.to_le_bytes());
    b.extend(48000u32.to_le_bytes()); b.extend((48000u32 * 4).to_le_bytes()); b.extend(4u16.to_le_bytes()); b.extend(16u16.to_le_bytes()); b.extend(b"data"); b.extend(data.to_le_bytes());
    for i in 0..n { let v = (f(i as f64 / 48000.).clamp(-1.0, 1.0) * i16::MAX as f64) as i16; b.extend(v.to_le_bytes()); b.extend(v.to_le_bytes()); }
    std::fs::write(path, b).unwrap();
}
fn amp(db: f64) -> f64 { 10f64.powf(db / 20.) }
fn tone(path: &str, db: f64, secs: f64) { let a = amp(db); wav(path, secs, |t| a * (TAU * 1000. * t).sin()); }
/// Speech-like: a 160 Hz voice with falling harmonics, in 0.5 s syllables with 0.3 s pauses.
fn talk(path: &str, db: f64, secs: f64) {
    let a = amp(db) / 1.6;
    wav(path, secs, |t| if (t % 0.8) < 0.5 { a * (1..=12).map(|k| (TAU * 160. * k as f64 * t).sin() / k as f64).sum::<f64>() } else { 0.0 });
}
/// A steady music bed: two sustained notes.
fn bed(path: &str, db: f64, secs: f64) { let a = amp(db) / 2.0; wav(path, secs, |t| a * ((TAU * 330. * t).sin() + (TAU * 440. * t).sin())); }

fn media(ss: &mut Session, t: &str, path: &str, start: f64, dur: f64) -> String {
    let a = c(ss, "import_media", json!({"path": path}))["asset_id"].clone();
    c(ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": start, "duration": dur}))["clip_id"].as_str().unwrap().to_string()
}
fn review(ss: &mut Session, args: Value) -> Vec<Value> { c(ss, "review", args)["findings"].as_array().unwrap().clone() }
fn find<'a>(f: &'a [Value], code: &str) -> Option<&'a Value> { f.iter().find(|x| x["code"] == code) }
fn codes(f: &[Value]) -> Vec<String> { f.iter().map(|x| x["code"].as_str().unwrap().to_string()).collect() }

// ------------------------------------------------------------------------------------------------ audio

#[test] fn a_minus_6_lufs_mix_is_flagged_with_finish_audio() {
    let mut ss = base(); let t = track(&mut ss);
    let p = tmp("loud", "wav"); tone(&p, -6.0, 6.0);
    media(&mut ss, &t, &p, 0.0, 6.0);
    let f = review(&mut ss, json!({"frames": 0, "domains": ["audio"]}));
    let x = find(&f, "mix_loudness_off_target").unwrap_or_else(|| panic!("{:?}", codes(&f)));
    let l = x["measured"]["integrated_lufs"].as_f64().unwrap();
    assert!((l + 6.0).abs() < 2.0, "measures about -6 LUFS: {x}");
    assert_eq!(x["fix"]["tool"], "finish_audio"); assert_eq!(x["fix"]["args"]["target_lufs"], -14.0);
    // a podcast target moves the goalposts
    let f = review(&mut ss, json!({"frames": 0, "domains": ["audio"], "platform": "podcast"}));
    assert_eq!(find(&f, "mix_loudness_off_target").unwrap()["fix"]["args"]["target_lufs"], -16.0);
    std::fs::remove_file(&p).ok();
}

#[test] fn a_mix_on_target_has_no_audio_findings() {
    let mut ss = base(); let t = track(&mut ss);
    let p = tmp("ontarget", "wav"); tone(&p, -14.0, 6.0);
    media(&mut ss, &t, &p, 0.0, 6.0);
    let f = review(&mut ss, json!({"frames": 0, "domains": ["audio"]}));
    assert!(f.is_empty(), "{f:?}");
    std::fs::remove_file(&p).ok();
}

#[test] fn a_clipped_clip_and_a_clip_driven_into_clipping_are_flagged() {
    let mut ss = base(); let t = track(&mut ss);
    let sq = tmp("square", "wav"); wav(&sq, 4.0, |t| if (TAU * 200. * t).sin() >= 0.0 { 1.0 } else { -1.0 });
    let hot = tmp("hot", "wav"); tone(&hot, -6.0, 4.0);
    let a = media(&mut ss, &t, &sq, 0.0, 4.0);
    let b = media(&mut ss, &t, &hot, 4.0, 4.0);
    c(&mut ss, "set_audio", json!({"clip_id": b, "volume": 3.0}));
    let f = review(&mut ss, json!({"frames": 0, "domains": ["audio"]}));
    for id in [&a, &b] {
        let x = f.iter().find(|x| x["code"] == "audio_clipping" && x["clip_id"] == json!(id)).unwrap_or_else(|| panic!("{id}: {f:?}"));
        assert_eq!(x["fix"]["tool"], "set_audio"); assert_eq!(x["fix"]["args"]["clip_id"], json!(id));
    }
    // the fix for the driven clip brings it back under full scale
    let fix = f.iter().find(|x| x["code"] == "audio_clipping" && x["clip_id"] == json!(b)).unwrap()["fix"]["args"].clone();
    c(&mut ss, "set_audio", fix);
    let f = review(&mut ss, json!({"frames": 0, "domains": ["audio"]}));
    assert!(!f.iter().any(|x| x["code"] == "audio_clipping" && x["clip_id"] == json!(b)), "{f:?}");
    for p in [sq, hot] { std::fs::remove_file(&p).ok(); }
}

#[test] fn loud_music_over_speech_is_flagged_with_duck_timeline_and_the_fix_works() {
    let mut ss = base(); let td = track(&mut ss); let tm = track(&mut ss);
    let v = tmp("voice", "wav"); talk(&v, -14.0, 8.0);
    let m = tmp("music", "wav"); bed(&m, -15.0, 8.0);
    media(&mut ss, &td, &v, 0.0, 8.0); media(&mut ss, &tm, &m, 0.0, 8.0);
    let f = review(&mut ss, json!({"frames": 0, "domains": ["audio"]}));
    let x = find(&f, "music_masks_speech").unwrap_or_else(|| panic!("{f:?}"));
    assert!(x["measured"]["speech_over_music_db"].as_f64().unwrap() < 10.0, "{x}");
    assert_eq!(x["fix"]["tool"], "duck_timeline");
    assert_eq!(x["fix"]["args"]["dialogue_tracks"], json!([td])); assert_eq!(x["fix"]["args"]["music_tracks"], json!([tm]));
    c(&mut ss, "duck_timeline", x["fix"]["args"].clone());
    let f = review(&mut ss, json!({"frames": 0, "domains": ["audio"]}));
    assert!(find(&f, "music_masks_speech").is_none(), "ducked: {f:?}");
    for p in [v, m] { std::fs::remove_file(&p).ok(); }
}

#[test] fn quiet_music_under_speech_is_clean() {
    let mut ss = base(); let td = track(&mut ss); let tm = track(&mut ss);
    let v = tmp("voice2", "wav"); talk(&v, -14.0, 8.0);
    let m = tmp("music2", "wav"); bed(&m, -36.0, 8.0);
    media(&mut ss, &td, &v, 0.0, 8.0); media(&mut ss, &tm, &m, 0.0, 8.0);
    let f = review(&mut ss, json!({"frames": 0, "domains": ["audio"]}));
    assert!(find(&f, "music_masks_speech").is_none(), "{f:?}");
    for p in [v, m] { std::fs::remove_file(&p).ok(); }
}

// ------------------------------------------------------------------------------------------------ colour

fn shot(name: &str, color: &str) -> String {
    let p = tmp(name, "mp4");
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("color=c={color}:size=64x48:rate=25:duration=2"), "-pix_fmt", "yuv420p", &p]).status().unwrap().success());
    p
}
fn shots(ss: &mut Session, colors: &[&str], tag: &str) -> (Vec<String>, Vec<String>) {
    let t = track(ss); let (mut ids, mut paths) = (vec![], vec![]);
    for (i, col) in colors.iter().enumerate() { let p = shot(&format!("{tag}{i}"), col); ids.push(media(ss, &t, &p, i as f64 * 2.0, 2.0)); paths.push(p); }
    (ids, paths)
}
fn colour_review(ss: &mut Session) -> Vec<Value> { review(ss, json!({"frames": 8, "domains": ["color"], "faces": false})) }

#[test] fn a_shot_that_does_not_match_its_scene_is_flagged_with_match_shot_look() {
    let mut ss = base();
    let (ids, paths) = shots(&mut ss, &["0x808080", "0x808080", "0x808080", "0x406080"], "mismatch");
    let f = colour_review(&mut ss);
    let x = find(&f, "shot_mismatch").unwrap_or_else(|| panic!("{f:?}"));
    assert_eq!(x["clip_id"], json!(ids[3]), "{x}");
    assert!(x["measured"]["de2000"].as_f64().unwrap() > 8.0);
    assert_eq!(x["fix"]["tool"], "match_shot_look"); assert_eq!(x["fix"]["args"]["target_clip_ids"], json!([ids[3]]));
    assert!(ids[..3].contains(&x["fix"]["args"]["reference_clip_ids"][0].as_str().unwrap().to_string()));
    assert_eq!(f.iter().filter(|x| x["code"] == "shot_mismatch").count(), 1, "only the odd one out: {f:?}");
    // a scene marker makes the odd shot a scene of its own: nothing to match
    c(&mut ss, "add_marker", json!({"time": 6.0, "name": "Scene 2"}));
    assert!(find(&colour_review(&mut ss), "shot_mismatch").is_none());
    for p in paths { std::fs::remove_file(&p).ok(); }
}

#[test] fn a_strong_cast_is_flagged_with_a_white_balance_pick() {
    let mut ss = base();
    let (ids, paths) = shots(&mut ss, &["0x7080a8", "0x7080a8"], "cast");
    let f = colour_review(&mut ss);
    let x = find(&f, "color_cast").unwrap_or_else(|| panic!("{f:?}"));
    assert!(ids.contains(&x["clip_id"].as_str().unwrap().to_string()));
    assert_eq!(x["fix"]["tool"], "pick_white_balance");
    for k in ["clip_id", "time", "x", "y", "w", "h"] { assert!(!x["fix"]["args"][k].is_null(), "{k}: {x}"); }
    assert!(find(&f, "shot_mismatch").is_none(), "the two shots match each other: {f:?}");
    for p in paths { std::fs::remove_file(&p).ok(); }
}

#[test] fn matching_neutral_shots_have_no_colour_findings() {
    let mut ss = base();
    let (_, paths) = shots(&mut ss, &["0x808080", "0x808080", "0x787878"], "clean");
    let f = colour_review(&mut ss);
    assert!(f.is_empty(), "{f:?}");
    for p in paths { std::fs::remove_file(&p).ok(); }
}

// ------------------------------------------------------------------------------------------------ transitions

#[test] fn a_long_unblurred_slide_is_flagged_and_a_short_blurred_one_is_clean() {
    let mut ss = base(); let t1 = track(&mut ss); let t2 = track(&mut ss);
    c(&mut ss, "add_clip", json!({"track_id": t1, "source_type": "solid", "color": [0.2, 0.3, 0.4], "start": 0.0, "duration": 3.0}));
    let b = c(&mut ss, "add_clip", json!({"track_id": t2, "source_type": "solid", "color": [0.5, 0.3, 0.2], "start": 1.5, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_transition", json!({"clip_id": b, "kind": "slide_left", "duration": 1.4}));
    let f = review(&mut ss, json!({"frames": 0, "domains": ["transitions"]}));
    assert_eq!(find(&f, "transition_rhythm").unwrap()["fix"]["tool"], "add_transition");
    assert_eq!(find(&f, "moving_transition_without_blur").unwrap()["fix"]["tool"], "set_motion_blur");
    c(&mut ss, "set_motion_blur", json!({"clip_id": b, "shutter_angle": 180}));
    c(&mut ss, "add_transition", json!({"clip_id": b, "kind": "slide_left", "duration": 0.5}));
    let f = review(&mut ss, json!({"frames": 0, "domains": ["transitions"]}));
    assert!(f.is_empty(), "{f:?}");
}
