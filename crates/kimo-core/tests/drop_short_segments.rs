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


#[test]
fn ripple_closes_a_real_float_drift_gap_after_two_non_adjacent_drops() {
    // Reproduces the KF13 bug (found by KF12 on real auto_rough_cut footage): real clip boundaries carry
    // ~1ms float drift (e.g. one clip ends at 31.8670s, the next's own recorded start is 31.8660s -- not
    // bit-identical "touching"), and ripple_delete's shift_after (edit.rs, EPS=1e-6) is far tighter than
    // that drift, so the clip immediately after an already-shifted drop can fail its own
    // `start >= cut - EPS` check and never move, leaving a real gap (a black hold on export). This test
    // lays clips with exactly that ~1ms drift at TWO separate boundaries (not adjacent drops: a kept clip
    // of real size sits between them) and must fail on 9f9e070 (the gap is left open after both drops).
    let mut ss = project();
    let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    // A(3.0) B(0.5, short/dropped) C(2.0) D(0.8, short/dropped) E(1.5), each later boundary drifted
    // 0.001s EARLY relative to where the previous clip's own recorded end says it should be -- the same
    // direction and magnitude KF12 measured on the real footage.
    let drift = 0.001;
    let a_start = 0.0; let a_dur = 3.0;
    let b_start = a_start + a_dur; let b_dur = 0.5; // B starts exactly where A ends: no drift on a KEPT boundary
    let b_end = b_start + b_dur;
    let c_start = b_end - drift; let c_dur = 2.0; // drift right after the FIRST dropped clip (B)
    let c_end = c_start + c_dur;
    let d_start = c_end; let d_dur = 0.8; // C->D boundary: no drift (D is itself about to be dropped)
    let d_end = d_start + d_dur;
    let e_start = d_end - drift; let e_dur = 1.5; // drift right after the SECOND dropped clip (D)

    let mut ids = vec![];
    for (st, d) in [(a_start, a_dur), (b_start, b_dur), (c_start, c_dur), (d_start, d_dur), (e_start, e_dur)] {
        ids.push(sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.3, 0.4], "start": st, "duration": d})), "clip_id"));
    }
    let (a_id, b_id, c_id, d_id, e_id) = (ids[0].clone(), ids[1].clone(), ids[2].clone(), ids[3].clone(), ids[4].clone());

    let total_before = a_dur + b_dur + c_dur + d_dur + e_dur; // 7.8
    let r = call(&mut ss, "drop_short_segments", json!({"track_id": t, "min_duration_s": 1.0, "keep_first_and_last": true}));
    let dropped: Vec<String> = r["dropped"].as_array().unwrap().iter().map(|v| v["clip_id"].as_str().unwrap().to_string()).collect();
    assert_eq!(dropped, vec![b_id.clone(), d_id.clone()], "two non-adjacent short clips, both dropped: {r}");
    assert_eq!(r["kept_count"].as_u64().unwrap(), 3, "{r}");
    assert!((r["total_before_s"].as_f64().unwrap() - total_before).abs() < 1e-6, "{r}");
    let expected_after = a_dur + c_dur + e_dur; // 6.5
    assert!((r["total_after_s"].as_f64().unwrap() - expected_after).abs() < 1e-6, "{r}");

    // The real assertion: every surviving clip must start within 1 frame (1/30s here) of where the
    // previous one ends -- no residual gap, whatever the drop order or float drift was.
    let one_frame = 1.0 / 30.0;
    let mut v: Vec<(f64, f64)> = ss.project.tracks.iter().find(|tr| tr.id == t).unwrap().clips.iter().map(|c| (c.start, c.duration)).collect();
    v.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
    assert_eq!(v.len(), 3, "{v:?}");
    let mut expect_start = v[0].0; // first kept clip keeps its own start
    for (i, (st, d)) in v.iter().enumerate() {
        assert!((st - expect_start).abs() <= one_frame, "clip {i} starts at {st}, expected ~{expect_start} (gap of {})", st - expect_start);
        expect_start = st + d;
    }
    // And the export-relevant total: the LAST clip's end must match total_after_s within a frame --
    // this is exactly the number that was wrong before the fix (47.867s vs total_after_s 47.133s on the
    // real 6-clip footage: a 0.733s unclosed gap).
    let last_end = v.last().unwrap().0 + v.last().unwrap().1;
    assert!((last_end - expected_after).abs() <= one_frame, "export-relevant end {last_end} vs total_after_s {expected_after}");

    // The original clip identities survive (ripple, not recreate) and nothing before the first drop moved.
    assert!(v.iter().map(|_| ()).count() > 0);
    let remaining: Vec<String> = ss.project.tracks.iter().find(|tr| tr.id == t).unwrap().clips.iter().map(|c| c.id.clone()).collect();
    assert!(remaining.contains(&a_id) && remaining.contains(&c_id) && remaining.contains(&e_id), "{remaining:?}");
    let a_after = ss.project.find_clip(&a_id).unwrap();
    assert_eq!(a_after.start, a_start, "the clip before the first drop must never move");
}
