//! v7.5 feature: duck_timeline -- auto_duck takes exactly one (music `track`, dialogue `under`) pair per
//! call; a timeline with two dialogue tracks (e.g. two speakers on separate lavs) and two music beds needs
//! four separate auto_duck calls to get every pair covered, and a line said on EITHER dialogue track must
//! duck BOTH music tracks, which auto_duck's own per-pair model cannot express at all. duck_timeline names
//! every dialogue track and every music track once and ducks every music track under the UNION of speech
//! across all named dialogue tracks, in one call. It is additive, not a reimplementation: it runs kimo's
//! own VAD once per dialogue track (same detector auto_duck's own detect:"speech" mode uses) and hands the
//! combined windows to auto_duck itself via its existing speech_windows path -- auto_duck's ducking math
//! is not duplicated here.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn try_c(s: &mut Session, n: &str, a: Value) -> Result<Value, String> { dispatch(s, n, &a) }
const SR: f64 = 48000.0;

/// Voice-like bursts (same shape as tests/duck_speech.rs's `voice`) at `speech_db` over `dur` seconds, stereo, silent elsewhere.
fn voice(dur: f64, bursts: &[(f64, f64)], speech_db: f64) -> Vec<f32> {
    let gain = 10f64.powf(speech_db / 20.0);
    let tau = 2.0 * std::f64::consts::PI;
    (0..(dur * SR) as usize).flat_map(|i| {
        let t = i as f64 / SR;
        let x = if bursts.iter().any(|(a, b)| t >= *a && t < *b) {
            ((tau * 440.0 * t).sin() + 0.6 * (tau * 900.0 * t).sin() + 0.4 * (tau * 1700.0 * t).sin()) * gain * (0.7 + 0.3 * (tau * 9.0 * t).sin())
        } else { 0.0 };
        let v = x as f32; [v, v]
    }).collect()
}
fn music(dur: f64) -> Vec<f32> { (0..(dur * SR) as usize).flat_map(|i| { let v = ((i as f64 / SR) * 2.0 * std::f64::consts::PI * 220.0).sin() as f32 * 0.2; [v, v] }).collect() }

fn wav(name: &str, pcm: &[f32]) -> String {
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("kimo-ducktl-{name}-{}-{n}.wav", std::process::id()));
    kimo_core::audio::write_wav(p.to_str().unwrap(), pcm).unwrap(); p.to_str().unwrap().to_string()
}

fn audio_clip(s: &mut Session, name: &str, pcm: &[f32], dur: f64) -> (String, String) {
    let path = wav(name, pcm);
    let asset = c(s, "import_media", json!({"path": path}))["asset_id"].clone();
    let r = c(s, "add_audio_clip", json!({"asset_id": asset, "start": 0, "duration": dur}));
    (r["track_id"].as_str().unwrap().to_string(), r["clip_id"].as_str().unwrap().to_string())
}

fn duck_at(s: &Session, clip: &str, t: f64) -> f64 { s.project.find_clip(clip).unwrap().audio.duck_gain.at(t) }

#[test]
fn ducks_one_music_track_under_speech_from_either_of_two_dialogue_tracks() {
    let mut s = Session::new();
    let dur = 8.0;
    let (d1, _) = audio_clip(&mut s, "voA", &voice(dur, &[(1.0, 3.0)], -6.0), dur);
    let (d2, _) = audio_clip(&mut s, "voB", &voice(dur, &[(5.0, 7.0)], -6.0), dur);
    let (m1, mclip) = audio_clip(&mut s, "music", &music(dur), dur);
    let _ = (&d1, &d2, &m1);
    let r = c(&mut s, "duck_timeline", json!({"dialogue_tracks": [d1, d2], "music_tracks": [m1], "amount_db": -12.0}));
    assert_eq!(r["music_tracks_ducked"], 1, "{r}");
    assert_eq!(r["dialogue_tracks_used"], 2, "{r}");
    // deep in speaker A's burst: music ducked
    assert!(duck_at(&s, &mclip, 2.0) < 0.5, "music should duck under speaker A: {}", duck_at(&s, &mclip, 2.0));
    // between the two bursts: no speech anywhere, music back up
    assert!(duck_at(&s, &mclip, 4.0) > 0.9, "music should be back at full level between the two speakers: {}", duck_at(&s, &mclip, 4.0));
    // deep in speaker B's burst: music ducked too, even though B is a DIFFERENT track than A
    assert!(duck_at(&s, &mclip, 6.0) < 0.5, "music should duck under speaker B too: {}", duck_at(&s, &mclip, 6.0));
}

#[test]
fn ducks_every_named_music_track_in_one_call() {
    let mut s = Session::new();
    let dur = 6.0;
    let (d1, _) = audio_clip(&mut s, "vo", &voice(dur, &[(1.0, 4.0)], -6.0), dur);
    let (m1, c1) = audio_clip(&mut s, "musicA", &music(dur), dur);
    let (m2, c2) = audio_clip(&mut s, "musicB", &music(dur), dur);
    let r = c(&mut s, "duck_timeline", json!({"dialogue_tracks": [d1], "music_tracks": [m1, m2], "amount_db": -12.0}));
    assert_eq!(r["music_tracks_ducked"], 2, "{r}");
    assert!(duck_at(&s, &c1, 2.0) < 0.5, "music track A must duck");
    assert!(duck_at(&s, &c2, 2.0) < 0.5, "music track B must duck too, same call");
}

#[test]
fn errors_when_a_track_is_named_as_both_dialogue_and_music() {
    let mut s = Session::new();
    let dur = 3.0;
    let (t, _) = audio_clip(&mut s, "x", &voice(dur, &[(0.5, 1.0)], -6.0), dur);
    let err = try_c(&mut s, "duck_timeline", json!({"dialogue_tracks": [t.clone()], "music_tracks": [t]})).unwrap_err();
    assert!(err.contains("both") || err.contains("overlap"), "{err}");
}

#[test]
fn errors_on_empty_dialogue_or_music_lists() {
    let mut s = Session::new();
    let dur = 3.0;
    let (t, _) = audio_clip(&mut s, "x", &voice(dur, &[(0.5, 1.0)], -6.0), dur);
    assert!(try_c(&mut s, "duck_timeline", json!({"dialogue_tracks": [], "music_tracks": [t.clone()]})).is_err());
    assert!(try_c(&mut s, "duck_timeline", json!({"dialogue_tracks": [t], "music_tracks": []})).is_err());
}

#[test]
fn errors_on_an_unknown_track_id() {
    let mut s = Session::new();
    let dur = 3.0;
    let (t, _) = audio_clip(&mut s, "x", &voice(dur, &[(0.5, 1.0)], -6.0), dur);
    let err = try_c(&mut s, "duck_timeline", json!({"dialogue_tracks": [t], "music_tracks": ["nope"]})).unwrap_err();
    assert!(err.to_lowercase().contains("track"), "{err}");
}

#[test]
fn duck_timeline_is_undoable() {
    let mut s = Session::new();
    let dur = 4.0;
    let (d1, _) = audio_clip(&mut s, "vo", &voice(dur, &[(1.0, 2.5)], -6.0), dur);
    let (m1, mclip) = audio_clip(&mut s, "music", &music(dur), dur);
    c(&mut s, "duck_timeline", json!({"dialogue_tracks": [d1], "music_tracks": [m1], "amount_db": -12.0}));
    assert!(duck_at(&s, &mclip, 1.5) < 0.5);
    c(&mut s, "undo", json!({}));
    assert!((duck_at(&s, &mclip, 1.5) - 1.0).abs() < 1e-6, "undo should clear the ducking");
}
