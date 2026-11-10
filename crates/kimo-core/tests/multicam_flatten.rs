//! multicam to flat (FEATURES.md "multicam: sync by audio/timecode, angle switching, multicam to flat" --
//! the sync/switching halves already shipped as sync_clips/add_multicam/switch_angle/list_multicam; this
//! covers the remaining "to flat" piece: flatten_multicam).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, Ordering};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).unwrap_err() }
fn pj(s: &Session) -> Value { serde_json::to_value(&s.project).unwrap() }

static N: AtomicU64 = AtomicU64::new(0);
fn fixture(name: &str) -> String {
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = format!("/tmp/kimo-mcflat-{}-{n}-{name}.wav", std::process::id());
    assert!(std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=440:sample_rate=48000:duration=3", "-ac", "2", &p])
        .status().unwrap().success());
    p
}

fn setup_multicam() -> (Session, String, String) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 64, "height": 48, "fps": 25}));
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let asset_a = c(&mut s, "import_media", json!({"path": fixture("a")}))["asset_id"].as_str().unwrap().to_string();
    let asset_b = c(&mut s, "import_media", json!({"path": fixture("b")}))["asset_id"].as_str().unwrap().to_string();
    let clip_a = c(&mut s, "add_clip", json!({"track_id": t1, "source_type":"media", "asset_id": asset_a, "start": 0.0, "duration": 3.0}))["clip_id"].as_str().unwrap().to_string();
    let clip_b = c(&mut s, "add_clip", json!({"track_id": t2, "source_type":"media", "asset_id": asset_b, "start": 0.0, "duration": 3.0}))["clip_id"].as_str().unwrap().to_string();
    let m = c(&mut s, "add_multicam", json!({"angles": [clip_a, clip_b]}));
    let id = m["multicam_id"].as_str().unwrap().to_string();
    let track_id = m["track_id"].as_str().unwrap().to_string();
    (s, id, track_id)
}

#[test]
fn flatten_removes_the_group_but_keeps_the_tracks_clips() {
    let (mut s, id, track_id) = setup_multicam();
    c(&mut s, "switch_angle", json!({"multicam_id": id, "t": 1.5, "angle": 1}));
    let before_clips = c(&mut s, "find", json!({"track": track_id}))["matches"].as_array().unwrap().len();
    let r = c(&mut s, "flatten_multicam", json!({"multicam_id": id}));
    assert_eq!(r["track_id"], track_id);
    assert_eq!(r["clip_count"], before_clips as i64);
    let after_clips = c(&mut s, "find", json!({"track": track_id}))["matches"].as_array().unwrap().len();
    assert_eq!(after_clips, before_clips, "flatten must not touch the track's clips");
    let groups = c(&mut s, "list_multicam", json!({}));
    assert!(groups.get(&id).is_none(), "the flattened group must be gone from list_multicam");
}

#[test]
fn switch_angle_on_a_flattened_group_is_rejected() {
    let (mut s, id, _track_id) = setup_multicam();
    c(&mut s, "flatten_multicam", json!({"multicam_id": id}));
    let e = err(&mut s, "switch_angle", json!({"multicam_id": id, "t": 0.5, "angle": 1}));
    assert!(e.contains("no multicam"), "{e}");
}

#[test]
fn flatten_can_rename_the_track() {
    let (mut s, id, track_id) = setup_multicam();
    c(&mut s, "flatten_multicam", json!({"multicam_id": id, "track_name": "Final Cut A"}));
    let tracks = c(&mut s, "get_project", json!({}))["tracks"].as_array().unwrap().clone();
    let tr = tracks.iter().find(|t| t["id"] == track_id).unwrap();
    assert_eq!(tr["name"], "Final Cut A");
}

#[test]
fn flatten_unknown_group_errors_and_mutates_nothing() {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 64, "height": 48, "fps": 25}));
    let before = pj(&s);
    let e = err(&mut s, "flatten_multicam", json!({"multicam_id": "no-such-group"}));
    assert!(e.contains("no multicam"), "{e}");
    assert_eq!(pj(&s), before);
}

#[test]
fn flatten_is_byte_exact_undo_redo() {
    let (mut s, id, _track_id) = setup_multicam();
    let before = pj(&s);
    c(&mut s, "flatten_multicam", json!({"multicam_id": id}));
    let after = pj(&s);
    assert_ne!(before, after);
    c(&mut s, "undo", json!({}));
    assert_eq!(pj(&s), before, "undo must restore the multicam group");
    c(&mut s, "redo", json!({}));
    assert_eq!(pj(&s), after);
}
