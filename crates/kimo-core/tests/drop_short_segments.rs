//! drop_short_segments (new, lane KF12): delete clips shorter than min_duration_s on a track and
//! ripple later clips left to close the gaps -- cleans up tiny false-start fragments a rough cut leaves
//! between real takes. All tests below must fail before the tool exists (the dispatch itself errors with
//! "unknown action").
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }

fn project() -> Session { let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 360, "height": 640, "fps": 30})); ss }

/// Lays clips of the given durations back to back (no gaps) on one track, solid-color source (no ffmpeg,
/// no asset) so these tests run fast; returns (track_id, clip_ids in order).
fn lay_clips(ss: &mut Session, durations: &[f64]) -> (String, Vec<String>) {
    let t = sid(&call(ss, "add_track", json!({})), "track_id");
    let mut start = 0.0;
    let mut ids = vec![];
    for d in durations {
        let c = sid(&call(ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.4], "start": start, "duration": d})), "clip_id");
        ids.push(c);
        start += d;
    }
    (t, ids)
}

fn track_clip_durations(ss: &Session, track_id: &str) -> Vec<f64> {
    let mut v: Vec<(f64, f64)> = ss.project.tracks.iter().find(|t| t.id == track_id).unwrap().clips.iter().map(|c| (c.start, c.duration)).collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    v.into_iter().map(|(_, d)| d).collect()
}
fn track_starts_have_no_gaps(ss: &Session, track_id: &str) -> bool {
    let mut v: Vec<(f64, f64)> = ss.project.tracks.iter().find(|t| t.id == track_id).unwrap().clips.iter().map(|c| (c.start, c.duration)).collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let mut expect = 0.0;
    for (st, d) in &v { if (st - expect).abs() > 1e-6 { return false; } expect = st + d; }
    true
}

#[test]
fn drops_short_clips_and_ripples_the_rest_with_no_gaps() {
    let mut ss = project();
    // 3s, 0.5s, 2s, 0.8s -- the 0.5s and 0.8s are under min 1.0 and should go.
    let (t, ids) = lay_clips(&mut ss, &[3.0, 0.5, 2.0, 0.8]);
    let r = call(&mut ss, "drop_short_segments", json!({"track_id": t, "min_duration_s": 1.0, "keep_first_and_last": false}));
    let dropped: Vec<String> = r["dropped"].as_array().unwrap().iter().map(|v| v["clip_id"].as_str().unwrap().to_string()).collect();
    assert_eq!(dropped, vec![ids[1].clone(), ids[3].clone()], "{r}");
    assert_eq!(r["kept_count"].as_u64().unwrap(), 2, "{r}");
    assert_eq!(r["total_before_s"].as_f64().unwrap(), 6.3, "{r}");
    assert!((r["total_after_s"].as_f64().unwrap() - 5.0).abs() < 1e-6, "{r}");
    // the remaining clips (3.0, 2.0) are rippled left with no gap
    let durs = track_clip_durations(&ss, &t);
    assert_eq!(durs, vec![3.0, 2.0], "{durs:?}");
    assert!(track_starts_have_no_gaps(&ss, &t), "gap left after ripple");
    // the kept clips are the original first and third clips, not new ones
    let remaining: Vec<String> = ss.project.tracks.iter().find(|tr| tr.id == t).unwrap().clips.iter().map(|c| c.id.clone()).collect();
    assert!(remaining.contains(&ids[0]), "{remaining:?}");
    assert!(remaining.contains(&ids[2]), "{remaining:?}");
}

#[test]
fn dry_run_reports_and_changes_nothing() {
    let mut ss = project();
    let (t, ids) = lay_clips(&mut ss, &[3.0, 0.5, 2.0, 0.8]);
    let before = serde_json::to_string(&ss.project).unwrap();
    let r = call(&mut ss, "drop_short_segments", json!({"track_id": t, "min_duration_s": 1.0, "keep_first_and_last": false, "dry_run": true}));
    assert_eq!(r["dry_run"], json!(true));
    let dropped: Vec<String> = r["dropped"].as_array().unwrap().iter().map(|v| v["clip_id"].as_str().unwrap().to_string()).collect();
    assert_eq!(dropped, vec![ids[1].clone(), ids[3].clone()], "{r}");
    assert_eq!(r["kept_count"].as_u64().unwrap(), 2, "{r}");
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "dry_run must not change the project: {r}");
}

#[test]
fn keep_first_and_last_protects_a_short_first_or_last_clip() {
    let mut ss = project();
    // first clip is short (0.4s), last clip is short (0.3s); a middle short clip (0.6s) still goes.
    let (t, ids) = lay_clips(&mut ss, &[0.4, 3.0, 0.6, 2.0, 0.3]);
    let r = call(&mut ss, "drop_short_segments", json!({"track_id": t, "min_duration_s": 1.0}));
    let dropped: Vec<String> = r["dropped"].as_array().unwrap().iter().map(|v| v["clip_id"].as_str().unwrap().to_string()).collect();
    assert_eq!(dropped, vec![ids[2].clone()], "first/last should survive, only the middle short clip drops: {r}");
    let durs = track_clip_durations(&ss, &t);
    assert_eq!(durs, vec![0.4, 3.0, 2.0, 0.3], "{durs:?}");
    assert!(track_starts_have_no_gaps(&ss, &t));
}

#[test]
fn undo_restores_the_timeline_exactly() {
    let mut ss = project();
    let (t, _ids) = lay_clips(&mut ss, &[3.0, 0.5, 2.0, 0.8]);
    let before = serde_json::to_string(&ss.project).unwrap();
    call(&mut ss, "drop_short_segments", json!({"track_id": t, "min_duration_s": 1.0, "keep_first_and_last": false}));
    assert_ne!(serde_json::to_string(&ss.project).unwrap(), before, "sanity: the call should have changed something");
    call(&mut ss, "undo", json!({}));
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "undo must restore the exact pre-drop timeline");
}

#[test]
fn bad_args_error() {
    let mut ss = project();
    let (t, _ids) = lay_clips(&mut ss, &[3.0, 0.5, 2.0]);
    assert!(dispatch(&mut ss, "drop_short_segments", &json!({"track_id": t, "min_duration_s": "soon"})).is_err(), "non-number min_duration_s must error");
    assert!(dispatch(&mut ss, "drop_short_segments", &json!({"track_id": t, "min_duration_s": 0.0})).is_err(), "min_duration_s must be > 0");
    assert!(dispatch(&mut ss, "drop_short_segments", &json!({"track_id": t, "min_duration_s": -1.0})).is_err(), "negative min_duration_s must error");
    assert!(dispatch(&mut ss, "drop_short_segments", &json!({"track_id": "no_such_track"})).is_err(), "unknown track_id must error");
    assert!(dispatch(&mut ss, "drop_short_segments", &json!({"track_id": t, "dry_run": "yes"})).is_err(), "non-boolean dry_run must error");
    assert!(dispatch(&mut ss, "drop_short_segments", &json!({"track_id": t, "keep_first_and_last": "yes"})).is_err(), "non-boolean keep_first_and_last must error");
    assert!(dispatch(&mut ss, "drop_short_segments", &json!({"track_id": 5})).is_err(), "non-string track_id must error");
}

#[test]
fn no_track_id_defaults_to_the_track_with_the_most_media_clips() {
    // drop_short_segments has no media-free way to build a Source::Media clip without an asset, so this
    // only proves the no-op shape: a non-media (solid) track is never picked as the default, so omitting
    // track_id with only solid tracks present is the same as "no track has a media clip".
    let mut ss = project();
    let (_t, _ids) = lay_clips(&mut ss, &[3.0, 0.5]);
    let r = dispatch(&mut ss, "drop_short_segments", &json!({"min_duration_s": 1.0}));
    assert!(r.is_err(), "no media clip anywhere: omitting track_id must error, not silently pick a solid track: {r:?}");
}
