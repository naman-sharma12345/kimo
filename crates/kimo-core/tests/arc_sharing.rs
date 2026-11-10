use kimo_core::commands::{dispatch, Session};
use serde_json::json;
use std::sync::Arc;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

#[test]
fn one_clip_edit_shares_the_other_1999_clips_with_the_undo_snapshot() {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let tr = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let mut first = String::new();
    for i in 0..2000 {
        let r = call(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "solid", "color": "#336699", "start": i as f64 * 0.1, "duration": 0.1}));
        if i == 0 { first = r["clip_id"].as_str().unwrap().to_string(); }
    }
    call(&mut ss, "set_property", json!({"clip_id": first, "property": "opacity", "value": 0.5}));
    let cur = &ss.project.tracks[0].clips;
    let prev = &ss.undo.last().expect("undo snapshot").tracks[0].clips;
    assert_eq!(cur.len(), 2000);
    assert_eq!(prev.len(), 2000);
    let shared = cur.iter().zip(prev.iter()).filter(|(a, b)| Arc::ptr_eq(a, b)).count();
    assert_eq!(shared, 1999, "exactly the edited clip must be unshared");
    assert!(cur.iter().filter(|c| Arc::strong_count(c) >= 2).count() == 1999);
    // undo restores the original value
    call(&mut ss, "undo", json!({}));
    assert!((ss.project.tracks[0].clips[0].props["opacity"].at(0.0) - 1.0).abs() < 1e-9);
}

fn shared_count(ss: &Session) -> (usize, usize) {
    let cur = &ss.project.tracks[0].clips; let prev = &ss.undo.last().expect("undo snapshot").tracks[0].clips;
    (cur.iter().zip(prev.iter()).filter(|(a, b)| Arc::ptr_eq(a, b)).count(), cur.len())
}
fn track_of(n: usize) -> (Session, Vec<String>) {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let tr = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let ids = (0..n).map(|i| call(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "solid", "color": "#336699", "start": i as f64 * 0.1, "duration": 0.1}))["clip_id"].as_str().unwrap().to_string()).collect();
    (ss, ids)
}

#[test]
fn ripple_trim_of_zero_does_not_unshare_the_clips_it_shifts() {
    let (mut ss, ids) = track_of(200);
    call(&mut ss, "ripple_trim", json!({"clip_id": ids[0], "edge": "tail", "delta": 0.0}));
    let (shared, n) = shared_count(&ss);
    assert_eq!((shared, n), (199, 200), "only the trimmed clip may be copied when the shift distance is 0");
}

#[test]
fn ripple_trim_that_moves_clips_unshares_exactly_the_moved_ones() {
    let (mut ss, ids) = track_of(200);
    call(&mut ss, "ripple_trim", json!({"clip_id": ids[150], "edge": "tail", "delta": 0.05}));
    let (shared, n) = shared_count(&ss);
    assert_eq!(n, 200);
    assert_eq!(shared, 150, "clips 0..149 untouched stay shared; the trimmed clip and the 49 after it are copied");
}
