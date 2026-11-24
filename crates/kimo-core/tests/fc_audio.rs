//! FilmCraft's audio DSP (crates/kimo-fc-audio) through add_audio_effect and kimo's audio mix: the effect runs in process
//! on the clip's decoded PCM, in the export and preview path.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn tone_clip(s: &mut Session) -> String {
    let p = std::env::temp_dir().join(format!("kimo-fcaudio-{}.wav", std::process::id())).to_string_lossy().to_string();
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=1000:sample_rate=48000:duration=1", "-ac", "2", &p]).status().unwrap().success());
    let t = c(s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = c(s, "import_media", json!({"path": p}))["asset_id"].clone();
    c(s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string()
}
fn rms(pcm: &[f32]) -> f64 { let mid = &pcm[pcm.len() / 4..pcm.len() * 3 / 4]; (mid.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / mid.len() as f64).sqrt() }

#[test]
fn filmcraft_effects_run_in_the_mix() {
    if std::process::Command::new("ffmpeg").arg("-version").output().is_err() { eprintln!("no ffmpeg; skipped"); return; }
    let mut s = Session::new(); let cl = tone_clip(&mut s);
    let base = rms(&kimo_core::audio::mix_strict(&s.project).unwrap()); assert!(base > 0.05, "the tone plays: {base}");
    let r = c(&mut s, "add_audio_effect", json!({"clip_id": cl, "kind": "amplify", "params": {"gain": 6.0}}));
    assert_eq!(r["effect"], "Amplify", "{r}");
    let loud = rms(&kimo_core::audio::mix_strict(&s.project).unwrap()); assert!((loud / base - 1.995).abs() < 0.1, "+6 dB doubles the level: {base} -> {loud}");
    c(&mut s, "undo", json!({}));
    c(&mut s, "add_audio_effect", json!({"clip_id": cl, "kind": "mute", "params": {"mute": 1}}));
    let quiet = rms(&kimo_core::audio::mix_strict(&s.project).unwrap()); assert!(quiet < 1e-4, "mute silences the clip: {quiet}");
}
