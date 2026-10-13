//! P2 failing-first rows: detect_cuts clip-boundary fields, audio_transients strength_db/band/nearest_cut_dt,
//! audio_speech_intervals confidence/rms_db/gaps/fix_hint. RED until P2 lands: keys do not exist yet.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn cut_project() -> (Session, String, String) {
    let mut ss = Session::new();
    let c = |ss: &mut Session, n: &str, a: Value| dispatch(ss, n, &a).unwrap();
    c(&mut ss, "project_set", json!({"width": 256, "height": 256, "fps": 12}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let left = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [1, 0, 0], "start": 0, "duration": 2}))["clip_id"].as_str().unwrap().to_string();
    let right = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0, 0, 1], "start": 2, "duration": 2}))["clip_id"].as_str().unwrap().to_string();
    (ss, left, right)
}

#[test]
fn detect_cuts_boundary_fields() {
    let (mut ss, left, right) = cut_project();
    let mut outs = vec![];
    for n in [1, 4] {
        kimo_core::par::set_threads(n);
        outs.push(dispatch(&mut ss, "detect_cuts", &json!({"fps": 12})).unwrap());
    }
    kimo_core::par::set_threads(0);
    assert_eq!(serde_json::to_vec(&outs[0]).unwrap(), serde_json::to_vec(&outs[1]).unwrap());
    let cuts = outs[0]["cuts"].as_array().unwrap();
    assert_eq!(cuts.len(), 1, "one hard cut at the red/blue boundary: {cuts:?}");
    let cut = &cuts[0];
    assert!((cut["time"].as_f64().unwrap() - 2.0).abs() <= 1.0 / 12.0, "cut within 1 frame of t=2");
    assert_eq!(cut["frame"].as_u64().unwrap(), 24);
    assert!(cut["score"].as_f64().unwrap() > 0.0);
    assert_eq!(cut["kind"], "hard_cut");
    assert_eq!(cut["left_clip"], left, "left_clip (P2) missing");
    assert_eq!(cut["right_clip"], right, "right_clip (P2) missing");
    assert_eq!(cut["on_clip_boundary"], true);
    assert!(cut["nearest_boundary_dt"].as_f64().expect("nearest_boundary_dt (P2) missing").abs() <= 1e-9);
}

fn click_wav(times: &[f64], seconds: f64) -> String {
    let mut pcm = vec![0f32; (seconds * 48000.) as usize * 2];
    for t in times {
        let i = (t * 48000.).round() as usize;
        pcm[2 * i] = 0.9;
        pcm[2 * i + 1] = 0.9;
    }
    let path = std::env::temp_dir().join(format!("kimo-p2-{}.wav", std::process::id()));
    kimo_core::audio::write_wav(path.to_str().unwrap(), &pcm).unwrap();
    path.to_str().unwrap().to_string()
}

#[test]
fn audio_transients_strength_db_band_nearest_cut() {
    let (mut ss, _l, _r) = cut_project();
    let path = click_wav(&[0.5, 1.5, 2.5, 3.5], 4.0);
    let c = |ss: &mut Session, n: &str, a: Value| dispatch(ss, n, &a).unwrap();
    let asset = c(&mut ss, "import_media", &json!({"path": path}))["asset_id"].as_str().unwrap().to_string();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": asset, "start": 0, "duration": 4}));
    let r = dispatch(&mut ss, "audio_transients", &json!({"target": "mix"})).unwrap();
    let events = r["transients"].as_array().unwrap();
    assert_eq!(events.len(), 4);
    for e in events {
        assert!(e["strength_db"].as_f64().expect("strength_db (P2) missing").is_finite());
        assert!(["low", "mid", "high"].contains(&e["band"].as_str().expect("band (P2) missing")), "band low|mid|high");
        let dt = e["nearest_cut_dt"].as_f64().expect("nearest_cut_dt (P2) missing");
        assert!((e["t"].as_f64().unwrap() - 2.0 - dt).abs() <= 2.0 + 1e-9);
    }
    let ev2 = &events[2];
    assert!((ev2["nearest_cut_dt"].as_f64().unwrap() - 0.5).abs() < 0.01, "click at 2.5 sits 0.5 s after the cut");
    std::fs::remove_file(path).unwrap();
}

fn speech_like_wav() -> String {
    // 0.3 s burst, 0.8 s pause, 0.3 s burst: amplitude-modulated harmonic signal, speech-ish ZCR/energy.
    let mut pcm = vec![0f32; 2 * 48000 * 2];
    for i in 0..48000 * 2 {
        let t = i as f64 / 48000.;
        let on = (0.2..0.5).contains(&t) || (1.3..1.6).contains(&t);
        let env = if on { 0.08 * (0.7 + 0.3 * (2.0 * std::f64::consts::PI * 9.0 * t).sin()) } else { 0.0 };
        let x = (env * ((2.0 * std::f64::consts::PI * 440.0 * t).sin() + 0.6 * (2.0 * std::f64::consts::PI * 900.0 * t).sin() + 0.4 * (2.0 * std::f64::consts::PI * 1700.0 * t).sin())) as f32;
        pcm[2 * i] = x;
        pcm[2 * i + 1] = x;
    }
    let path = std::env::temp_dir().join(format!("kimo-p2-speech-{}.wav", std::process::id()));
    kimo_core::audio::write_wav(path.to_str().unwrap(), &pcm).unwrap();
    path.to_str().unwrap().to_string()
}

#[test]
fn speech_intervals_confidence_rms_gaps_fix_hint() {
    let mut ss = Session::new();
    dispatch(&mut ss, "project_set", &json!({"width": 256, "height": 256, "fps": 24})).unwrap();
    let path = speech_like_wav();
    let r = dispatch(&mut ss, "audio_speech_intervals", &json!({"target": {"path": path}, "max_pause": 0.5, "pad_ms": 0, "hangover_ms": 0})).unwrap();
    let intervals = r["speech"].as_array().unwrap();
    assert_eq!(intervals.len(), 2, "two bursts: {intervals:?}");
    for iv in intervals {
        assert!(iv["confidence"].as_f64().expect("confidence (P2) missing") > 0.0);
        assert!(iv["rms_db"].as_f64().expect("rms_db (P2) missing").is_finite());
    }
    let gaps = r["gaps"].as_array().expect("gaps list (P2) missing");
    assert_eq!(gaps.len(), 1, "one 0.8 s pause between bursts");
    assert!((gaps[0]["duration"].as_f64().unwrap() - 0.8).abs() < 0.15);
    assert!(gaps[0]["fix_hint"].is_object(), "fix_hint (P2) missing");
    assert!(gaps[0]["fix_hint"]["why"].as_str().unwrap().contains("0.5"));
    std::fs::remove_file(path).unwrap();
}
