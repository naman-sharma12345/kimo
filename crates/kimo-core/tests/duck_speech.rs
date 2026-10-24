//! Speech-aware ducking (Premiere "Auto Ducking" / Resolve "Fairlight ducking" with dialogue detection): auto_duck can
//! now detect the dialogue with kimo's own VAD (detect: "speech") instead of raw RMS, so room tone, hiss or breaths on the
//! VO track above the threshold no longer hold the music down between lines; and `hold` keeps the duck down for N seconds
//! after the voice stops so the music does not pump up between words (both detect modes). The rendered mix is measured:
//! the music really dips by amount_db while the voice talks and is back at full level between lines.
//! Defaults (no detect, no hold) are the old RMS behaviour, bit for bit.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
const SR: f64 = 48000.0;

/// Voice-like bursts (440/900/1700 Hz partials, 9 Hz syllable wobble, as tests/vad.rs) at `speech_db` over a white
/// noise floor at `floor_db` (0 = none), `dur` seconds, stereo.
fn voice(dur: f64, bursts: &[(f64, f64)], speech_db: f64, floor_db: Option<f64>) -> Vec<f32> {
    let mut rng = 19u64; let gain = 10f64.powf(speech_db / 20.0);
    let noise = floor_db.map_or(0.0, |d| 10f64.powf(d / 20.0) * 3f64.sqrt());
    let tau = 2.0 * std::f64::consts::PI;
    (0..(dur * SR) as usize).flat_map(|i| {
        rng ^= rng << 13; rng ^= rng >> 7; rng ^= rng << 17;
        let n = (rng as f64 / u64::MAX as f64 * 2.0 - 1.0) * noise; let t = i as f64 / SR;
        let x = if bursts.iter().any(|(a, b)| t >= *a && t < *b) {
            ((tau * 440.0 * t).sin() + 0.6 * (tau * 900.0 * t).sin() + 0.4 * (tau * 1700.0 * t).sin()) * gain * (0.7 + 0.3 * (tau * 9.0 * t).sin())
        } else { 0.0 };
        let v = (x + n) as f32; [v, v]
    }).collect()
}

fn wav(name: &str, pcm: &[f32]) -> String {
    // Unique per call, not just per process: `name` is only "music" or "vo{len}", and every test in
    // this file uses an 8s duration, so all tests shared the exact same two file paths and raced on
    // them under the default parallel test runner (one test's write landing between another's write
    // and decode) -- the two "hold" tests reproducibly saw a sibling test's audio. A per-call counter
    // makes each temp file path unique regardless of scheduling.
    static N: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = N.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("kimo-duck-{name}-{}-{n}.wav", std::process::id()));
    kimo_core::audio::write_wav(p.to_str().unwrap(), pcm).unwrap(); p.to_str().unwrap().to_string()
}

/// music = 220 Hz sine at 0.2 for 8 s on one track, the VO wav on another; returns (session, music track, vo track, music clip).
fn scene(vo: &[f32]) -> (Session, Value, Value, String) {
    let music: Vec<f32> = (0..(8.0 * SR) as usize).flat_map(|i| { let v = ((i as f64 / SR) * 2.0 * std::f64::consts::PI * 220.0).sin() as f32 * 0.2; [v, v] }).collect();
    let (pm, pv) = (wav("music", &music), wav(&format!("vo{}", vo.len()), vo));
    let mut s = Session::new();
    let am = c(&mut s, "import_media", json!({"path":pm}))["asset_id"].clone();
    let av = c(&mut s, "import_media", json!({"path":pv}))["asset_id"].clone();
    let rm = c(&mut s, "add_audio_clip", json!({"asset_id":am,"start":0,"duration":8}));
    let rv = c(&mut s, "add_audio_clip", json!({"asset_id":av,"start":0,"duration":8}));
    let clip = rm["clip_id"].as_str().unwrap().to_string();
    (s, rm["track_id"].clone(), rv["track_id"].clone(), clip)
}

fn duck(s: &Session, clip: &str, t: f64) -> f64 { s.project.find_clip(clip).unwrap().audio.duck_gain.at(t) }

/// Goertzel amplitude of `f` Hz on the left channel over [t0, t1).
fn amp(pcm: &[f32], f: f64, t0: f64, t1: f64) -> f64 {
    let (a, b) = ((t0 * SR) as usize, (t1 * SR) as usize);
    let w = 2.0 * std::f64::consts::PI * f / SR; let (mut re, mut im) = (0.0, 0.0);
    for i in a..b { let v = pcm[i * 2] as f64; re += v * (w * i as f64).cos(); im += v * (w * i as f64).sin(); }
    2.0 * (re * re + im * im).sqrt() / (b - a) as f64
}

const G12: f64 = 0.251188643150958; // 10^(-12/20)

#[test]
fn speech_detection_ignores_room_tone_that_rms_ducks_on() {
    // two lines of dialogue over -30 dBFS hiss: the hiss alone is above the -35 dB RMS threshold
    let vo = voice(8.0, &[(1.0, 2.0), (4.0, 5.5)], -20.0, Some(-30.0));
    let (mut s, tm, tv, m) = scene(&vo);
    c(&mut s, "auto_duck", json!({"track":tm,"under":tv,"threshold_db":-35,"amount_db":-12}));
    assert!(duck(&s, &m, 3.0) < 0.3, "old RMS mode ducks on the hiss between lines: {}", duck(&s, &m, 3.0));
    let r = c(&mut s, "auto_duck", json!({"track":tm,"under":tv,"detect":"speech","amount_db":-12}));
    assert_eq!(r["mode"], json!("speech"), "{r}");
    let w = r["windows_used"].as_array().unwrap(); assert_eq!(w.len(), 2, "two lines found: {r}");
    for (row, (a, b)) in w.iter().zip([(1.0, 2.0), (4.0, 5.5)]) {
        let (x, y) = (row[0].as_f64().unwrap(), row[1].as_f64().unwrap());
        assert!(x <= a && x > a - 0.3 && y >= b && y < b + 0.5, "window {row} around line {a}..{b}");
    }
    for t in [1.5, 4.75] { assert!((duck(&s, &m, t) - G12).abs() < 1e-12, "ducked -12 dB at {t}: {}", duck(&s, &m, t)); }
    for t in [3.0, 7.5] { assert!((duck(&s, &m, t) - 1.0).abs() < 1e-12, "full music between lines at {t}: {}", duck(&s, &m, t)); }
    // the rendered mix really dips: 220 Hz (music only; the voice has no 220 Hz) measured in and out of the lines
    let pcm = kimo_core::audio::mix(&s.project);
    let (on, off) = (amp(&pcm, 220.0, 1.3, 1.7), amp(&pcm, 220.0, 2.8, 3.2));
    let dip = 20.0 * (on / off).log10();
    assert!((dip + 12.0).abs() < 0.25, "measured music dip {dip:.3} dB (on {on:.4}, off {off:.4})");
    let strict = kimo_core::audio::mix_strict(&s.project).unwrap();
    let dip2 = 20.0 * (amp(&strict, 220.0, 1.3, 1.7) / amp(&strict, 220.0, 2.8, 3.2)).log10();
    assert!((dip2 + 12.0).abs() < 0.25, "mix_strict dip {dip2:.3} dB");
}

#[test]
fn hold_bridges_the_gap_between_words_in_speech_mode() {
    let vo = voice(8.0, &[(1.0, 2.0), (2.9, 3.5)], -20.0, None);
    let (mut s, tm, tv, m) = scene(&vo);
    let r = c(&mut s, "auto_duck", json!({"track":tm,"under":tv,"detect":"speech","amount_db":-12}));
    assert_eq!(r["windows_used"].as_array().unwrap().len(), 2, "{r}");
    let gap = (r["windows_used"][0][1].as_f64().unwrap() + r["windows_used"][1][0].as_f64().unwrap()) / 2.0;
    assert!(duck(&s, &m, gap) > G12 + 0.2, "without hold the music swells between the words: {}", duck(&s, &m, gap));
    let r = c(&mut s, "auto_duck", json!({"track":tm,"under":tv,"detect":"speech","amount_db":-12,"hold":0.6}));
    assert_eq!(r["windows_used"].as_array().unwrap().len(), 1, "hold merges the two lines: {r}");
    assert!((duck(&s, &m, gap) - G12).abs() < 1e-12, "held down through the gap: {}", duck(&s, &m, gap));
    assert_eq!(r["hold"], json!(0.6));
}

#[test]
fn hold_bridges_the_gap_in_rms_mode_too() {
    let vo = voice(8.0, &[(1.0, 2.0), (2.4, 3.0)], -20.0, None);
    let (mut s, tm, tv, m) = scene(&vo);
    c(&mut s, "auto_duck", json!({"track":tm,"under":tv,"threshold_db":-40,"amount_db":-12}));
    let free = duck(&s, &m, 2.25);
    assert!(free > 0.5, "RMS without hold releases in a 0.4 s gap: {free}");
    c(&mut s, "auto_duck", json!({"track":tm,"under":tv,"threshold_db":-40,"amount_db":-12,"hold":0.5}));
    let held = duck(&s, &m, 2.25);
    assert!((held - G12).abs() < 1e-3, "RMS with hold 0.5 stays down: {held}");
    assert!((duck(&s, &m, 3.3) - G12).abs() < 1e-3, "hold also delays the release after the last word: {}", duck(&s, &m, 3.3));
    assert!(duck(&s, &m, 4.5) > 0.95, "then releases (word ends 3.0 + hold 0.5 + release): {}", duck(&s, &m, 4.5));
}

#[test]
fn defaults_are_the_old_rms_envelope_bit_for_bit() {
    let vo = voice(8.0, &[(1.0, 2.0), (4.0, 5.5)], -20.0, Some(-50.0));
    let (mut s, tm, tv, m) = scene(&vo);
    c(&mut s, "auto_duck", json!({"track":tm,"under":tv,"amount_db":-18}));
    let old = serde_json::to_string(&s.project.find_clip(&m).unwrap().audio.duck_gain).unwrap();
    c(&mut s, "auto_duck", json!({"track":tm,"under":tv,"amount_db":-18,"detect":"rms","hold":0}));
    assert_eq!(serde_json::to_string(&s.project.find_clip(&m).unwrap().audio.duck_gain).unwrap(), old);
}

#[test]
fn refusals_name_the_argument_and_change_nothing() {
    let vo = voice(8.0, &[(1.0, 2.0)], -20.0, None);
    let (mut s, tm, tv, _m) = scene(&vo);
    let j0 = serde_json::to_string(&s.project).unwrap();
    for (args, word) in [
        (json!({"track":tm,"under":tv,"detect":"loud"}), "detect"),
        (json!({"track":tm,"under":tv,"hold":-1}), "hold"),
        (json!({"track":tm,"under":tv,"hold":9}), "hold"),
        (json!({"track":tm,"detect":"speech"}), "under"),
        (json!({"track":tm,"under":tv,"detect":"speech","speech_windows":[[1,2]]}), "speech_windows"),
    ] {
        let e = dispatch(&mut s, "auto_duck", &args).unwrap_err();
        assert!(e.contains(word), "{args}: {e}");
        assert_eq!(serde_json::to_string(&s.project).unwrap(), j0, "{args} changed the project");
    }
}

#[test]
fn registry_entry_is_short_and_spectator_gui_routes_it_to_the_mixer() {
    let reg = kimo_core::commands::registry();
    let cmd = reg.iter().find(|c| c.name == "auto_duck").unwrap();
    assert!(cmd.description.len() < 150, "{} chars: {}", cmd.description.len(), cmd.description);
    let props = &cmd.schema["properties"];
    assert_eq!(props["detect"]["enum"], json!(["rms", "speech"]));
    assert!(props["hold"].is_object());
    assert_eq!(kimo_core::toolgroups::group_of("auto_duck"), "audio");
    let router = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../kimo-gui/ui/router.js")).unwrap();
    assert!(router.contains("auto_duck"), "spectator router shows the duck (detect/hold) on the music track");
}
