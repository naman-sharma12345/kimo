//! v7.2 additions: find_by_loudness -- whole-project loudness query in one call (the north star's own
//! example: "all clips under -20 LUFS"). Reuses measure_loudness per clip through dispatch_nested rather
//! than a second implementation.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::f64::consts::TAU;

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }

/// Stereo 1 kHz sine at `db` dBFS, `secs` seconds (a peak of L dBFS reads approximately L LUFS, same
/// technique as tests/loudness_ext.rs).
fn tone_wav(path: &str, db: f64, secs: f64) {
    let n = (secs * 48000.).round() as usize;
    let amp = 10f64.powf(db / 20.) * i16::MAX as f64;
    let mut frames = Vec::with_capacity(n);
    for i in 0..n { let v = (amp * (TAU * 1000. * i as f64 / 48000.).sin()) as i16; frames.push((v, v)); }
    let data = (frames.len() * 4) as u32; let mut b = Vec::with_capacity(44 + data as usize);
    b.extend(b"RIFF"); b.extend((36 + data).to_le_bytes()); b.extend(b"WAVEfmt "); b.extend(16u32.to_le_bytes()); b.extend(1u16.to_le_bytes()); b.extend(2u16.to_le_bytes());
    b.extend(48000u32.to_le_bytes()); b.extend((48000u32 * 4).to_le_bytes()); b.extend(4u16.to_le_bytes()); b.extend(16u16.to_le_bytes()); b.extend(b"data"); b.extend(data.to_le_bytes());
    for (l, r) in frames { b.extend(l.to_le_bytes()); b.extend(r.to_le_bytes()); }
    std::fs::write(path, b).unwrap();
}
fn tmp(name: &str) -> String { format!("{}/kimo-fbl-{}-{name}.wav", std::env::temp_dir().display(), std::process::id()) }

/// A clip of `path` (1s duration) on track `t`.
fn media_clip(ss: &mut Session, t: &str, path: &str) -> String {
    let a = c(ss, "import_media", json!({"path": path}))["asset_id"].clone();
    c(ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string()
}

#[test] fn finds_clips_under_a_max_lufs_threshold() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let loud = tmp("loud"); tone_wav(&loud, -10.0, 1.0); let quiet = tmp("quiet"); tone_wav(&quiet, -30.0, 1.0);
    let id_loud = media_clip(&mut ss, &t, &loud); let id_quiet = media_clip(&mut ss, &t, &quiet);

    let r = c(&mut ss, "find_by_loudness", json!({"max_lufs": -20.0}));
    assert_eq!(r["filtered"], true);
    let ids: Vec<&str> = r["matches"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert!(ids.contains(&id_quiet.as_str()), "{ids:?}");
    assert!(!ids.contains(&id_loud.as_str()), "{ids:?}");
    assert_eq!(r["total_media_clips"], 2);
}

#[test] fn min_and_max_lufs_together_bound_a_range() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = tmp("a"); tone_wav(&a, -5.0, 1.0); let b = tmp("b"); tone_wav(&b, -25.0, 1.0); let d = tmp("d"); tone_wav(&d, -45.0, 1.0);
    let ia = media_clip(&mut ss, &t, &a); let ib = media_clip(&mut ss, &t, &b); let id = media_clip(&mut ss, &t, &d);
    let r = c(&mut ss, "find_by_loudness", json!({"min_lufs": -30.0, "max_lufs": -20.0}));
    let ids: Vec<&str> = r["matches"].as_array().unwrap().iter().map(|m| m["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec![ib.as_str()], "only the middle clip falls in [-30,-20]: {ia} too loud, {id} too quiet");
}

#[test] fn no_range_given_returns_every_media_clips_reading_unfiltered() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = tmp("a2"); tone_wav(&a, -10.0, 1.0); let b = tmp("b2"); tone_wav(&b, -30.0, 1.0);
    media_clip(&mut ss, &t, &a); media_clip(&mut ss, &t, &b);
    let r = c(&mut ss, "find_by_loudness", json!({}));
    assert_eq!(r["filtered"], false);
    assert_eq!(r["match_count"], 2, "both clips come back as a per-clip readout, nothing excluded");
}

#[test] fn non_media_clips_are_not_candidates_at_all() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.1,0.1,0.1], "start": 0.0, "duration": 2.0}));
    let r = c(&mut ss, "find_by_loudness", json!({}));
    assert_eq!(r["total_media_clips"], 0);
    assert_eq!(r["match_count"], 0);
}

#[test] fn too_short_for_an_integrated_reading_is_skipped_not_a_match_or_a_failure() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let short = tmp("short"); tone_wav(&short, -10.0, 1.0); // 1s file
    let a = c(&mut ss, "import_media", json!({"path": short}))["asset_id"].clone();
    // 0.1s clip duration: under measure_loudness's 0.4s minimum for an integrated reading.
    let id = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 0.1}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "find_by_loudness", json!({}));
    assert_eq!(r["total_media_clips"], 1);
    assert_eq!(r["match_count"], 0);
    assert_eq!(r["skipped"], 1);
    assert_eq!(r["skipped_clips"][0]["id"], id);
}

#[test] fn track_id_scopes_the_query_to_one_track() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t1 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = tmp("a3"); tone_wav(&a, -10.0, 1.0); let b = tmp("b3"); tone_wav(&b, -30.0, 1.0);
    media_clip(&mut ss, &t1, &a); media_clip(&mut ss, &t2, &b);
    let r = c(&mut ss, "find_by_loudness", json!({"track_id": t1}));
    assert_eq!(r["total_media_clips"], 1);
}

#[test] fn min_greater_than_max_errors() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let e = dispatch(&mut ss, "find_by_loudness", &json!({"min_lufs": -10.0, "max_lufs": -20.0})).unwrap_err();
    assert!(e.contains("must be <="), "{e}");
}

#[test] fn is_read_only() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = tmp("a4"); tone_wav(&a, -10.0, 1.0); media_clip(&mut ss, &t, &a);
    let h = serde_json::to_string(&ss.project).unwrap(); let u = ss.undo.len();
    c(&mut ss, "find_by_loudness", json!({}));
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), h); assert_eq!(ss.undo.len(), u);
}

