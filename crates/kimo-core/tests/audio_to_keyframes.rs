//! audio_to_keyframes (AE Convert Audio to Keyframes) on synthetic WAVs (48 kHz stereo, fps 10): silence, a loud tone and
//! a tone 20 dB quieter must map to min, max and the dB-proportional point between; attack, release and smoothing must
//! lag the envelope the way they promise; bands and channels must select what they name; keys outside the span survive.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

const SR: usize = 48000;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
/// 4 s stereo: per second segment (left amp, right amp, Hz).
fn wav(name: &str, segs: [(f32, f32, f32); 4]) -> String {
    let path = std::env::temp_dir().join(format!("kimo-a2k-{}-{name}.wav", std::process::id())); let mut pcm = Vec::with_capacity(SR * 8);
    for i in 0..SR * 4 { let (l, r, hz) = segs[i / SR]; let v = (std::f32::consts::TAU * hz * i as f32 / SR as f32).sin(); pcm.push(l * v); pcm.push(r * v); }
    kimo_core::audio::write_wav(path.to_str().unwrap(), &pcm).unwrap(); path.to_str().unwrap().to_string()
}
/// A 4 s solid on track 1 (the keyed clip) and the WAV as an audio clip at 0 s.
fn scene(path: &str) -> (Session, String, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 10}));
    let t = id(&c(&mut s, "add_track", json!({})), "track_id");
    let v = id(&c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [1, 0, 0], "start": 0, "duration": 4.0})), "clip_id");
    let a = id(&c(&mut s, "import_media", json!({"path": path})), "asset_id"); let au = id(&c(&mut s, "add_audio_clip", json!({"asset_id": a, "start": 0, "duration": 4.0})), "clip_id");
    (s, v, au)
}
fn val(s: &Session, clip: &str, prop: &str, t: f64) -> f64 { let c = s.project.find_clip(clip).unwrap(); let p = c.props.get(prop).unwrap_or_else(|| panic!("{prop} not keyed")); p.at(t) }
fn standard() -> String { wav("std", [(0.0, 0.0, 440.0), (0.8, 0.8, 440.0), (0.08, 0.08, 440.0), (0.0, 0.0, 440.0)]) }

#[test]
fn loudness_maps_silence_to_min_loud_to_max_and_quiet_in_between_by_db() {
    let (mut s, v, au) = scene(&standard());
    let r = c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "scale", "min": 1.0, "max": 1.5, "floor_db": -60}));
    assert_eq!(r["keys"], 41, "{r}");
    let (sil, loud, quiet, end) = (val(&s, &v, "scale", 0.5), val(&s, &v, "scale", 1.5), val(&s, &v, "scale", 2.5), val(&s, &v, "scale", 3.5));
    // rms 0.566 (-4.9 dB) is the ceiling; rms 0.0566 (-24.9 dB) sits at 35.1/55.1 = 0.637 of the -60..-4.9 dB span
    assert!((sil - 1.0).abs() < 1e-6 && (end - 1.0).abs() < 1e-6, "silence must sit at min: {sil} {end}");
    assert!((loud - 1.5).abs() < 0.01, "the loudest second must reach max: {loud}");
    assert!((quiet - (1.0 + 0.5 * 0.637)).abs() < 0.03, "a tone 20 dB down must land 0.637 of the way in dB: {quiet}");
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "scale", "min": 1.0, "max": 1.5, "response": "linear"}));
    let q = val(&s, &v, "scale", 2.5); assert!((q - 1.05).abs() < 0.01, "linear response: a tenth of the amplitude is a tenth of the range: {q}");
}

#[test]
fn attack_release_and_smoothing_lag_the_envelope() {
    let (mut s, v, au) = scene(&standard());
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "scale", "min": 1.0, "max": 1.5, "floor_db": -60}));
    let (raw_rise, raw_fall, raw_edge) = (val(&s, &v, "scale", 1.1), val(&s, &v, "scale", 2.1), val(&s, &v, "scale", 1.05));
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "scale", "min": 1.0, "max": 1.5, "floor_db": -60, "attack": 0.3}));
    let atk = val(&s, &v, "scale", 1.1); assert!(raw_rise > 1.45 && atk < 1.25 && atk > 1.05, "attack 0.3 s must slow the rise: raw {raw_rise}, with attack {atk}");
    let fall_atk = val(&s, &v, "scale", 2.1); assert!((fall_atk - raw_fall).abs() < 0.02, "attack alone must not slow the fall: {fall_atk} vs {raw_fall}");
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "scale", "min": 1.0, "max": 1.5, "floor_db": -60, "release": 0.3}));
    let rel = val(&s, &v, "scale", 2.1); assert!(raw_fall < 1.36 && rel > 1.40, "release 0.3 s must hold the level as it falls: raw {raw_fall}, with release {rel}");
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "scale", "min": 1.0, "max": 1.5, "floor_db": -60, "smoothing": 0.5}));
    let sm = val(&s, &v, "scale", 1.0); let raw_at_1 = { let (mut s2, v2, au2) = scene(&standard()); c(&mut s2, "audio_to_keyframes", json!({"clip_id": v2, "source_clip_id": au2, "property": "scale", "min": 1.0, "max": 1.5, "floor_db": -60})); val(&s2, &v2, "scale", 0.8) };
    assert!(sm > 1.1 && sm < 1.42 && raw_at_1 < 1.01, "smoothing 0.5 s must average across the onset: smoothed at 1.0 s {sm}, raw at 0.8 s {raw_at_1}");
    let _ = raw_edge;
}

#[test]
fn bands_and_channels_select_what_they_name() {
    let p = wav("bands", [(0.6, 0.6, 80.0), (0.6, 0.6, 6000.0), (0.0, 0.6, 440.0), (0.0, 0.0, 440.0)]);
    let (mut s, v, au) = scene(&p);
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "scale", "min": 1.0, "max": 1.5, "band": "low", "end": 2.0}));
    let (bass, treble) = (val(&s, &v, "scale", 0.5), val(&s, &v, "scale", 1.5)); assert!(bass > 1.45 && treble < 1.1, "band low: the 80 Hz second {bass}, the 6 kHz second {treble}");
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "scale", "min": 1.0, "max": 1.5, "band": "high", "end": 2.0}));
    let (bass, treble) = (val(&s, &v, "scale", 0.5), val(&s, &v, "scale", 1.5)); assert!(treble > 1.45 && bass < 1.1, "band high: the 80 Hz second {bass}, the 6 kHz second {treble}");
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "opacity", "min": 0.2, "max": 1.0, "channel": "left", "start": 2.0}));
    let l = val(&s, &v, "opacity", 2.5); assert!((l - 0.2).abs() < 0.02, "channel left: the third second is silent on the left: {l}");
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "opacity", "min": 0.2, "max": 1.0, "channel": "right", "start": 2.0}));
    let r = val(&s, &v, "opacity", 2.5); assert!((r - 1.0).abs() < 0.02, "channel right: the third second is the loudest on the right: {r}");
}

#[test]
fn effect_params_are_keyed_and_keys_outside_the_span_survive() {
    let (mut s, v, au) = scene(&standard());
    let e = id(&c(&mut s, "add_effect", json!({"clip_id": v, "kind": "blur"})), "effect_id");
    c(&mut s, "set_keyframe", json!({"clip_id": v, "property": "scale", "time": 3.9, "value": 2.0}));
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "radius", "effect_id": e, "max": 30.0}));
    let fx = s.project.find_clip(&v).unwrap().effects.iter().find(|x| x.id == e).unwrap().params.get("radius").unwrap().clone();
    assert!((fx.at(0.5) - 4.0).abs() < 1e-6 && (fx.at(1.5) - 30.0).abs() < 0.2, "blur radius from its default 4 at silence to 30 at the peak: {} {}", fx.at(0.5), fx.at(1.5));
    c(&mut s, "audio_to_keyframes", json!({"clip_id": v, "source_clip_id": au, "property": "scale", "max": 1.3, "end": 3.0}));
    let pr = s.project.find_clip(&v).unwrap().props.get("scale").unwrap().clone();
    assert!(pr.keys.iter().any(|k| (k.t - 3.9).abs() < 1e-9 && (k.v - 2.0).abs() < 1e-9), "the key at 3.9 s (outside 0..3) must survive");
    assert_eq!(pr.keys.iter().filter(|k| k.t <= 3.0 + 1e-9).count(), 31, "one key per frame over 0..3 s");
}

#[test]
fn bad_input_is_refused_without_changing_the_project() {
    let (mut s, v, au) = scene(&standard()); let before = serde_json::to_string(&s.project).unwrap();
    for (args, want) in [
        (json!({"clip_id": v, "property": "scale"}), "needs max"),
        (json!({"clip_id": v, "source_clip_id": au, "property": "wobble", "max": 1.0}), "unknown clip property"),
        (json!({"clip_id": v, "property": "scale", "max": 1.0}), "not a media clip"),
        (json!({"clip_id": v, "source_clip_id": au, "property": "scale", "max": 1.0, "band": "sub"}), "band must be"),
        (json!({"clip_id": v, "source_clip_id": au, "property": "scale", "max": 1.0, "window": 0}), "window must be"),
        (json!({"clip_id": v, "source_clip_id": au, "property": "scale", "max": 1.0, "start": 3, "end": 2}), "start/end"),
        (json!({"clip_id": v, "source_clip_id": au, "property": "scale", "max": 1.0, "gain": 2}), "unknown argument"),
        (json!({"clip_id": v, "source_clip_id": au, "property": "scale", "max": 1.0, "effect_id": "fx_nope"}), "no such effect"),
        (json!({"clip_id": v, "source_clip_id": au, "property": "scale", "max": 1.0, "ceiling_db": -70}), "must be above floor_db"),
    ] { let e = dispatch(&mut s, "audio_to_keyframes", &args).expect_err(&format!("{args} must be refused")); assert!(e.contains(want), "{args}: {e}"); assert_eq!(serde_json::to_string(&s.project).unwrap(), before, "{args} changed the project"); }
}
