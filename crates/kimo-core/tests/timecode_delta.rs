//! Rolling/regular edits by timecode-delta text, not just raw seconds (FEATURES.md "rolling/regular edit by
//! timecode text helpers"). `delta` on ripple_trim, roll_edit, slip_clip and slide_clip now also accepts a string
//! like "+10" (seconds, default unit), "-5 frames" / "-5f" (frame count at the project's fps), case-insensitively,
//! with or without a space before the unit. A plain number keeps behaving exactly as before (same result, byte for
//! byte) so this is additive, not a breaking change.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).unwrap_err() }

fn session_at_fps(fps: f64) -> (Session, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 160, "height": 90, "fps": fps}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    (s, t)
}

fn clip(s: &mut Session, t: &Value, start: f64, dur: f64) -> String {
    c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.5,0.5,0.5], "start": start, "duration": dur}))["clip_id"].as_str().unwrap().to_string()
}

#[test]
fn ripple_trim_accepts_a_plain_seconds_string_matching_the_numeric_result() {
    let (mut s1, t1) = session_at_fps(10.0); let a1 = clip(&mut s1, &t1, 0.0, 2.0);
    let r1 = c(&mut s1, "ripple_trim", json!({"clip_id": a1, "edge": "tail", "delta": 0.5}));
    let (mut s2, t2) = session_at_fps(10.0); let a2 = clip(&mut s2, &t2, 0.0, 2.0);
    let r2 = c(&mut s2, "ripple_trim", json!({"clip_id": a2, "edge": "tail", "delta": "+0.5"}));
    assert_eq!(r1["duration"], r2["duration"]);
    assert!((r2["duration"].as_f64().unwrap() - 2.5).abs() < 1e-9);
}

#[test]
fn ripple_trim_accepts_a_frames_string_converted_by_project_fps() {
    let (mut s, t) = session_at_fps(10.0); let a = clip(&mut s, &t, 0.0, 2.0);
    // -5 frames at 10 fps == -0.5 s
    let r = c(&mut s, "ripple_trim", json!({"clip_id": a, "edge": "tail", "delta": "-5 frames"}));
    assert!((r["duration"].as_f64().unwrap() - 1.5).abs() < 1e-9, "got {:?}", r);
}

#[test]
fn roll_edit_accepts_a_compact_frames_suffix() {
    let (mut s, t) = session_at_fps(10.0);
    let a = clip(&mut s, &t, 0.0, 4.0); let b = clip(&mut s, &t, 4.0, 4.0);
    // "10f" at 10fps == 1.0s
    c(&mut s, "roll_edit", json!({"left_clip_id": a, "right_clip_id": b, "delta": "10f"}));
    let left_dur = s.project.tracks[0].clips.iter().find(|x| x.id == a).unwrap().duration;
    assert!((left_dur - 5.0).abs() < 1e-9, "left clip should have grown by 1.0s, got {left_dur}");
}

#[test]
fn slip_clip_accepts_seconds_unit_word() {
    let (mut s, t) = session_at_fps(25.0); let a = clip(&mut s, &t, 0.0, 2.0);
    let r = c(&mut s, "slip_clip", json!({"clip_id": a, "delta": "2 sec"}));
    assert!((r["in_point"].as_f64().unwrap() - 2.0).abs() < 1e-9);
}

#[test]
fn slide_clip_accepts_a_negative_frames_string() {
    let (mut s, t) = session_at_fps(24.0);
    let a = clip(&mut s, &t, 0.0, 2.0); let b = clip(&mut s, &t, 2.0, 2.0); let cc = clip(&mut s, &t, 4.0, 2.0);
    // -12 frames at 24fps == -0.5s
    c(&mut s, "slide_clip", json!({"clip_id": b, "delta": "-12 frames"}));
    let bstart = s.project.tracks[0].clips.iter().find(|x| x.id == b).unwrap().start;
    assert!((bstart - 1.5).abs() < 1e-9, "got {bstart}");
    let _ = (a, cc);
}

#[test]
fn a_malformed_delta_string_is_a_clear_error_not_a_panic() {
    let (mut s, t) = session_at_fps(30.0); let a = clip(&mut s, &t, 0.0, 2.0);
    let e = err(&mut s, "ripple_trim", json!({"clip_id": a, "edge": "tail", "delta": "banana"}));
    assert!(e.contains("delta"), "error should mention delta, got: {e}");
}

#[test]
fn an_unrecognized_unit_is_a_clear_error() {
    let (mut s, t) = session_at_fps(30.0); let a = clip(&mut s, &t, 0.0, 2.0);
    let e = err(&mut s, "ripple_trim", json!({"clip_id": a, "edge": "tail", "delta": "5 fortnights"}));
    assert!(e.to_lowercase().contains("unit") || e.to_lowercase().contains("fortnight"), "got: {e}");
}
