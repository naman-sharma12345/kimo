//! v7.3: batch_normalize_loudness -- the write-side counterpart to find_by_loudness (v7.2): normalizes
//! every candidate clip to a target LUFS in one call, reusing measure_loudness/normalize_loudness per clip.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::f64::consts::TAU;

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }

/// Stereo 1 kHz sine at `db` dBFS, `secs` seconds (a peak of L dBFS reads approximately L LUFS, same
/// technique find_by_loudness.rs uses).
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
fn tmp(name: &str) -> String { format!("{}/kimo-bnl-{}-{name}.wav", std::env::temp_dir().display(), std::process::id()) }
fn media_clip(ss: &mut Session, t: &str, path: &str) -> String {
    let a = c(ss, "import_media", json!({"path": path}))["asset_id"].clone();
    c(ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string()
}

#[test] fn normalizes_every_media_clip_to_the_target() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let loud = tmp("loud"); tone_wav(&loud, -10.0, 1.0); let quiet = tmp("quiet"); tone_wav(&quiet, -30.0, 1.0);
    let id_loud = media_clip(&mut ss, &t, &loud); let id_quiet = media_clip(&mut ss, &t, &quiet);

    let r = c(&mut ss, "batch_normalize_loudness", json!({"target_lufs": -20.0}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["total_candidates"], 2);
    let applied_ids: Vec<&str> = r["applied"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert!(applied_ids.contains(&id_loud.as_str()), "{r}"); assert!(applied_ids.contains(&id_quiet.as_str()), "{r}");

    // normalize_loudness itself sets clip.audio.volume to the computed linear gain (measure_loudness
    // reads raw decoded PCM, unaffected by the volume prop -- its own documented "remeasure timeline"
    // caveat); confirm the gain was actually written onto both clips, not just reported.
    for id in [&id_loud, &id_quiet] {
        let entry = r["applied"].as_array().unwrap().iter().find(|e| e["id"] == json!(id)).unwrap_or_else(|| panic!("{id} missing from applied: {r}"));
        let gain_db = entry["gain_db"].as_f64().unwrap();
        let expected_gain = 10f64.powf(gain_db / 20.0);
        let actual_gain = ss.project.find_clip(id).unwrap().audio.volume.base;
        assert!((actual_gain - expected_gain).abs() < 1e-6, "{id}: volume.base should be the computed linear gain {expected_gain}, got {actual_gain}");
    }
}

#[test] fn a_clip_already_within_tolerance_is_left_alone() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let on_target = tmp("ontarget"); tone_wav(&on_target, -14.1, 1.0); // within default 0.5dB tolerance of -14
    let id = media_clip(&mut ss, &t, &on_target);
    let before_vol = ss.project.find_clip(&id).unwrap().audio.volume.base;

    let r = c(&mut ss, "batch_normalize_loudness", json!({}));
    assert_eq!(r["applied"].as_array().unwrap().len(), 0, "{r}");
    let on_target_ids: Vec<&str> = r["already_on_target"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert!(on_target_ids.contains(&id.as_str()), "{r}");
    assert_eq!(ss.project.find_clip(&id).unwrap().audio.volume.base, before_vol, "volume must be untouched");
}

#[test] fn dry_run_reports_without_mutating() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let loud = tmp("loud2"); tone_wav(&loud, -10.0, 1.0);
    let id = media_clip(&mut ss, &t, &loud);
    let before_vol = ss.project.find_clip(&id).unwrap().audio.volume.base;

    let r = c(&mut ss, "batch_normalize_loudness", json!({"target_lufs": -20.0, "dry_run": true}));
    assert_eq!(r["dry_run"], true);
    let entry = r["applied"].as_array().unwrap().iter().find(|e| e["id"] == json!(id)).unwrap();
    assert!(entry.get("would_gain_db").is_some(), "{r}");
    assert_eq!(ss.project.find_clip(&id).unwrap().audio.volume.base, before_vol, "dry_run must not mutate");
}

#[test] fn clip_ids_scopes_to_an_explicit_list_and_unknown_id_errors() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = tmp("a5"); tone_wav(&a, -10.0, 1.0); let b = tmp("b5"); tone_wav(&b, -10.0, 1.0);
    let ida = media_clip(&mut ss, &t, &a); let idb = media_clip(&mut ss, &t, &b);

    let r = c(&mut ss, "batch_normalize_loudness", json!({"clip_ids": [ida.clone()], "target_lufs": -20.0}));
    assert_eq!(r["total_candidates"], 1, "{r}");
    let _ = idb;

    let e = dispatch(&mut ss, "batch_normalize_loudness", &json!({"clip_ids": ["not_a_real_clip"]})).unwrap_err();
    assert!(e.contains("unknown clip_id"), "{e}");
}

#[test] fn too_short_for_a_reading_is_skipped_not_a_failure() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let short = tmp("short2"); tone_wav(&short, -10.0, 1.0);
    let a = c(&mut ss, "import_media", json!({"path": short}))["asset_id"].clone();
    let id = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 0.1}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "batch_normalize_loudness", json!({}));
    assert_eq!(r["ok"], true, "{r}");
    let skipped_ids: Vec<&str> = r["skipped"].as_array().unwrap().iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert!(skipped_ids.contains(&id.as_str()), "{r}");
}

#[test] fn track_id_scopes_the_batch_to_one_track() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t1 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = tmp("a6"); tone_wav(&a, -10.0, 1.0); let b = tmp("b6"); tone_wav(&b, -10.0, 1.0);
    media_clip(&mut ss, &t1, &a); media_clip(&mut ss, &t2, &b);
    let r = c(&mut ss, "batch_normalize_loudness", json!({"track_id": t1}));
    assert_eq!(r["total_candidates"], 1, "{r}");
}
