//! Keyframeable stereo pan (FEATURES.md L170): `Audio.pan` becomes a `Prop` like `volume`, animatable via a
//! dedicated `set_audio_pan_key` tool (same shape as `set_audio_volume_key`). `set_audio_pan` keeps setting the
//! static base value. Backward compat: a project saved under the old format (`"pan"` a bare number) must still
//! load -- `tests/data/v1_project.kimo` is exactly that fixture.
use kimo_core::commands::{dispatch, Session};
use kimo_core::model::Audio;
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value {
    dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}"))
}
fn track(s: &mut Session) -> String {
    c(s, "add_track", json!({}))["track_id"].as_str().unwrap().into()
}
fn fixture(name: &str, src: &str) -> String {
    let p = format!("/tmp/kimo-panfix-{}-{name}.wav", std::process::id());
    assert!(std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", src, "-ac", "2", &p])
        .status()
        .unwrap()
        .success());
    p
}
fn media(s: &mut Session, t: &str, path: &str, start: f64, duration: f64) -> String {
    let a = c(s, "import_media", json!({"path": path}))["asset_id"].clone();
    c(s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": start, "duration": duration}))["clip_id"].as_str().unwrap().into()
}

#[test]
fn set_audio_pan_key_registers_and_animates_pan_over_time() {
    let mut s = Session::new();
    let t = track(&mut s);
    let path = fixture("tone", "sine=frequency=1000:sample_rate=48000:duration=2");
    let cl = media(&mut s, &t, &path, 0.0, 2.0);
    // hard left at t=0, hard right at t=2, linear ramp between.
    c(&mut s, "set_audio_pan_key", json!({"clip_id": cl, "time": 0.0, "value": -1.0, "ease": "linear"}));
    c(&mut s, "set_audio_pan_key", json!({"clip_id": cl, "time": 2.0, "value": 1.0, "ease": "linear"}));
    let pr = s.project.find_clip(&cl).unwrap().audio.pan.clone();
    assert_eq!(pr.keys.len(), 2);
    assert!((pr.at(0.0) - (-1.0)).abs() < 1e-9);
    assert!((pr.at(1.0) - 0.0).abs() < 1e-9);
    assert!((pr.at(2.0) - 1.0).abs() < 1e-9);

    let pcm = kimo_core::audio::mix_strict(&s.project).unwrap();
    // early samples should be left-heavy, late samples right-heavy.
    let n = pcm.len() / 2;
    let early: f32 = pcm[0..2000].iter().map(|v| v.abs()).sum();
    let early_l: f32 = pcm[0..2000].iter().step_by(2).map(|v| v.abs()).sum();
    let early_r: f32 = pcm[1..2000].iter().step_by(2).map(|v| v.abs()).sum();
    assert!(early_l > early_r, "near t=0 (pan -1) left should dominate: L={early_l} R={early_r}");
    let tail = n * 2 - 2000;
    let late_l: f32 = pcm[tail..tail + 2000].iter().step_by(2).map(|v| v.abs()).sum();
    let late_r: f32 = pcm[tail + 1..tail + 2000].iter().step_by(2).map(|v| v.abs()).sum();
    assert!(late_r > late_l, "near t=2 (pan +1) right should dominate: L={late_l} R={late_r}");
    let _ = early;
    std::fs::remove_file(&path).ok();
}

#[test]
fn set_audio_pan_still_sets_static_base_unkeyframed() {
    let mut s = Session::new();
    let t = track(&mut s);
    let path = fixture("tone2", "sine=frequency=500:sample_rate=48000:duration=1");
    let cl = media(&mut s, &t, &path, 0.0, 1.0);
    c(&mut s, "set_audio_pan", json!({"clip_id": cl, "pan": -1.0}));
    let a = s.project.find_clip(&cl).unwrap().audio.pan.clone();
    assert_eq!(a.base, -1.0);
    assert!(a.keys.is_empty());
    std::fs::remove_file(&path).ok();
}

#[test]
fn legacy_plain_number_pan_still_deserializes() {
    // Old project format: Audio.pan was a bare f64. A project saved back when must still load.
    let legacy = json!({
        "lead": 0.0, "tail": 0.0, "effects": [], "pan": 0.35,
        "duck_gain": {"base": 1.0, "keys": [], "expr": []},
        "mute": false, "volume": {"base": 1.0, "keys": [], "expr": []},
        "fade_in": 0.0, "fade_out": 0.0, "pitch": 0.0, "highpass": 0.0, "lowpass": 0.0,
        "denoise": 0.0, "compress": false, "echo": 0.0, "reverse": false
    });
    let a: Audio = serde_json::from_value(legacy).expect("legacy bare-number pan must still deserialize");
    assert_eq!(a.pan.base, 0.35);
    assert!(a.pan.keys.is_empty());
}

#[test]
fn full_prop_pan_also_deserializes() {
    // New format: pan already a full Prop object (e.g. re-saved by this version).
    let modern = json!({
        "lead": 0.0, "tail": 0.0, "effects": [], "pan": {"base": -0.2, "keys": [{"t":1.0,"v":0.5,"ease":"linear"}], "expr": []},
        "duck_gain": {"base": 1.0, "keys": [], "expr": []},
        "mute": false, "volume": {"base": 1.0, "keys": [], "expr": []},
        "fade_in": 0.0, "fade_out": 0.0, "pitch": 0.0, "highpass": 0.0, "lowpass": 0.0,
        "denoise": 0.0, "compress": false, "echo": 0.0, "reverse": false
    });
    let a: Audio = serde_json::from_value(modern).expect("full Prop-shaped pan must deserialize");
    assert_eq!(a.pan.base, -0.2);
    assert_eq!(a.pan.keys.len(), 1);
}

#[test]
fn v1_fixture_project_with_bare_number_pan_still_loads() {
    let mut s = Session::new();
    c(&mut s, "load_project", json!({"path": concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/data/v1_project.kimo")}));
    // just confirms load_project didn't error (dispatch() unwraps); spot-check a clip's pan is a usable Prop now.
    for t in &s.project.tracks {
        for cl in &t.clips {
            assert!(cl.audio.pan.base.is_finite());
            let _: f64 = cl.audio.pan.at(0.5);
        }
    }
}
