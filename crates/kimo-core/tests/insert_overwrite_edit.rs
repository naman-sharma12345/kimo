//! Insert vs overwrite on add_clip, three/four-point edit, source in/out marks (FEATURES.md
//! "insert vs overwrite with auto-ripple on add_clip, three/four-point edit, source in/out marks").
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).unwrap_err() }
fn pj(s: &Session) -> Value { serde_json::to_value(&s.project).unwrap() }

fn fixture(name: &str, dur: f64) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    // Every test calls `project()` -> `fixture(...)`, and cargo runs tests in this binary on multiple threads
    // concurrently, so pid+name alone collided across tests writing the same path at once (a torn moov atom).
    let n = N.fetch_add(1, Ordering::Relaxed);
    let p = format!("/tmp/kimo-io-edit-{}-{n}-{name}.mp4", std::process::id());
    assert!(std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("testsrc=size=64x48:rate=25:duration={dur}"), "-pix_fmt", "yuv420p", &p])
        .status().unwrap().success());
    p
}

fn project() -> (Session, String) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 64, "height": 48, "fps": 25}));
    let asset = c(&mut s, "import_media", json!({"path": fixture("a", 10.0)}))["asset_id"].as_str().unwrap().to_string();
    (s, asset)
}

// ---------- mode: default (unchanged) ----------

#[test]
fn add_clip_with_no_mode_behaves_exactly_as_before_even_when_it_would_overlap() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 0.0, "duration": 5.0}));
    // No mode: overlapping placement is accepted as a raw append, same as always -- two clips, both untouched.
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 2.0, "duration": 5.0}));
    let clips = c(&mut s, "find", json!({"track": t}))["matches"].as_array().unwrap().clone();
    assert_eq!(clips.len(), 2);
    assert_eq!(clips[0]["start"], 0.0); assert_eq!(clips[0]["end"], 5.0);
    assert_eq!(clips[1]["start"], 2.0); assert_eq!(clips[1]["end"], 7.0);
}

// ---------- mode: insert ----------

#[test]
fn mode_insert_ripples_later_clips_on_the_target_track() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 0.0, "duration": 3.0}));
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 3.0, "duration": 3.0}));
    // Insert a 2s clip at t=3: the second clip should shift to start=5, nothing overwritten.
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 3.0, "duration": 2.0, "mode": "insert"}));
    let clips = c(&mut s, "find", json!({"track": t}))["matches"].as_array().unwrap().clone();
    assert_eq!(clips.len(), 3);
    let mut starts: Vec<f64> = clips.iter().map(|c| c["start"].as_f64().unwrap()).collect(); starts.sort_by(|a,b| a.partial_cmp(b).unwrap());
    assert_eq!(starts, vec![0.0, 3.0, 5.0]);
}

#[test]
fn mode_insert_splits_a_clip_spanning_the_insertion_point() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 0.0, "duration": 6.0}));
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 3.0, "duration": 1.0, "mode": "insert"}));
    let clips = c(&mut s, "find", json!({"track": t}))["matches"].as_array().unwrap().clone();
    // original 0..6 clip splits at 3 into 0..3 and 4..7 (shifted by 1s), plus the new 3..4 clip = 3 clips total.
    assert_eq!(clips.len(), 3);
}

#[test]
fn mode_insert_with_ripple_tracks_keeps_a_second_track_in_sync() {
    let (mut s, asset) = project();
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t1, "source_type":"media", "asset_id": asset, "start": 2.0, "duration": 3.0}));
    c(&mut s, "add_clip", json!({"track_id": t2, "source_type":"media", "asset_id": asset, "start": 2.0, "duration": 3.0}));
    c(&mut s, "add_clip", json!({"track_id": t1, "source_type":"solid", "color": [1.0,0.0,0.0], "start": 0.0, "duration": 1.0, "mode": "insert", "ripple_tracks": [t2]}));
    let on = |s: &mut Session, tid: &str| c(s, "find", json!({"track": tid}))["matches"].as_array().unwrap().iter().map(|c| c["start"].as_f64().unwrap()).filter(|x| *x > 0.5).collect::<Vec<_>>();
    assert_eq!(on(&mut s, &t1), vec![3.0]);
    assert_eq!(on(&mut s, &t2), vec![3.0], "ripple_tracks must shift the second track's clip to stay in sync");
}

#[test]
fn ripple_tracks_without_mode_insert_is_rejected() {
    let (mut s, asset) = project();
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let e = err(&mut s, "add_clip", json!({"track_id": t1, "source_type":"media", "asset_id": asset, "start": 0.0, "duration": 1.0, "ripple_tracks": [t2]}));
    assert!(e.contains("ripple_tracks only applies to mode"), "{e}");
}

// ---------- mode: overwrite ----------

#[test]
fn mode_overwrite_clears_the_destination_span_without_shifting_anything() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 0.0, "duration": 5.0}));
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 5.0, "duration": 5.0}));
    // Overwrite [2, 4) with a solid: the first clip spans clean across [2,4) so it splits into a 0..2 head and a
    // 4..5 tail (lift_range's own behaviour -- nothing in [2,4) survives), the new clip fills exactly [2,4), and
    // the second clip at start=5 is untouched (no ripple at all: that's the point of overwrite).
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"solid", "color":[0.0,1.0,0.0], "start": 2.0, "duration": 2.0, "mode": "overwrite"}));
    let clips = c(&mut s, "find", json!({"track": t}))["matches"].as_array().unwrap().clone();
    assert_eq!(clips.len(), 4, "0..2, 2..4 (new), 4..5 (tail), 5..10 untouched");
    let mut spans: Vec<(f64,f64)> = clips.iter().map(|c| (c["start"].as_f64().unwrap(), c["end"].as_f64().unwrap())).collect(); spans.sort_by(|a,b| a.partial_cmp(b).unwrap());
    assert_eq!(spans, vec![(0.0,2.0), (2.0,4.0), (4.0,5.0), (5.0,10.0)], "overwrite must not ripple the later clip, and must leave no gap or overlap");
}

#[test]
fn mode_overwrite_removes_a_fully_covered_clip() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 1.0, "duration": 1.0}));
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 0.0, "duration": 5.0, "mode": "overwrite"}));
    let clips = c(&mut s, "find", json!({"track": t}))["matches"].as_array().unwrap().clone();
    assert_eq!(clips.len(), 1, "the fully-covered 1..2 clip must be removed by the overwrite, not left overlapping");
}

// ---------- atomicity: a validation error after the mode mutation must not leave a partial change ----------

#[test]
fn mode_insert_failing_asset_validation_leaves_the_project_unmutated() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 3.0, "duration": 3.0}));
    let before = pj(&s);
    let e = err(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": "no-such-asset", "start": 3.0, "duration": 1.0, "mode": "insert"}));
    assert!(e.contains("no asset"), "{e}");
    assert_eq!(pj(&s), before, "a bad asset_id must not leave the insert-ripple applied: add_clip must be all-or-nothing");
}

#[test]
fn mode_insert_failing_on_a_bad_ripple_track_leaves_the_first_tracks_ripple_undone() {
    let (mut s, asset) = project();
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t1, "source_type":"media", "asset_id": asset, "start": 2.0, "duration": 3.0}));
    let before = pj(&s);
    let e = err(&mut s, "add_clip", json!({"track_id": t1, "source_type":"media", "asset_id": asset, "start": 0.0, "duration": 1.0, "mode": "insert", "ripple_tracks": ["no-such-track"]}));
    assert!(e.contains("no such track") || e.contains("no track"), "{e}");
    assert_eq!(pj(&s), before, "the ripple on t1 must be undone when a later ripple_tracks entry fails: all-or-nothing across the whole ripple set");
}

#[test]
fn mode_requires_a_recognised_value() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let e = err(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 0.0, "duration": 1.0, "mode": "bogus"}));
    assert!(e.contains("mode must be insert or overwrite"), "{e}");
}

// ---------- source marks ----------

#[test]
fn mark_source_sets_and_merges_in_and_out_independently() {
    let (mut s, asset) = project();
    c(&mut s, "mark_source", json!({"asset_id": asset, "in_point": 1.0}));
    assert_eq!(c(&mut s, "get_source_marks", json!({"asset_id": asset})), json!({"asset_id": asset, "in_point": 1.0, "out_point": null}));
    c(&mut s, "mark_source", json!({"asset_id": asset, "out_point": 4.0}));
    assert_eq!(c(&mut s, "get_source_marks", json!({"asset_id": asset})), json!({"asset_id": asset, "in_point": 1.0, "out_point": 4.0}), "setting out_point must not clobber the existing in_point");
}

#[test]
fn mark_source_rejects_out_before_in_and_past_asset_duration() {
    let (mut s, asset) = project();
    c(&mut s, "mark_source", json!({"asset_id": asset, "in_point": 5.0}));
    let e = err(&mut s, "mark_source", json!({"asset_id": asset, "out_point": 4.0}));
    assert!(e.contains("must be >"), "{e}");
    let e2 = err(&mut s, "mark_source", json!({"asset_id": asset, "out_point": 999.0}));
    assert!(e2.contains("past the asset's known duration"), "{e2}");
}

#[test]
fn clear_source_marks_one_or_all() {
    let (mut s, asset) = project();
    let asset2 = c(&mut s, "import_media", json!({"path": fixture("b", 3.0)}))["asset_id"].as_str().unwrap().to_string();
    c(&mut s, "mark_source", json!({"asset_id": asset, "in_point": 1.0, "out_point": 2.0}));
    c(&mut s, "mark_source", json!({"asset_id": asset2, "in_point": 0.0, "out_point": 1.0}));
    c(&mut s, "clear_source_marks", json!({"asset_id": asset}));
    assert_eq!(c(&mut s, "get_source_marks", json!({"asset_id": asset}))["in_point"], Value::Null);
    assert_eq!(c(&mut s, "get_source_marks", json!({"asset_id": asset2}))["in_point"], 0.0, "clearing one asset's marks must not touch another's");
    c(&mut s, "clear_source_marks", json!({}));
    let all = c(&mut s, "get_source_marks", json!({}))["marks"].as_array().unwrap().clone();
    assert!(all.is_empty(), "clear with no asset_id must clear every mark");
}

// ---------- edit_points: three-point edit ----------

#[test]
fn three_point_edit_derives_timeline_out_from_source_span() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let r = c(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "source_in": 1.0, "source_out": 4.0, "timeline_in": 2.0}));
    assert_eq!(r["timeline_out"], 5.0); assert_eq!(r["speed"], 1.0); assert_eq!(r["point_count"], 3);
    let clips = c(&mut s, "find", json!({"track": t}))["matches"].as_array().unwrap().clone();
    assert_eq!(clips.len(), 1); assert_eq!(clips[0]["start"], 2.0); assert_eq!(clips[0]["end"], 5.0);
}

#[test]
fn three_point_edit_derives_timeline_in_from_timeline_out() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let r = c(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "source_in": 1.0, "source_out": 3.0, "timeline_out": 10.0}));
    assert_eq!(r["timeline_in"], 8.0);
}

#[test]
fn three_point_edit_derives_source_out_from_timeline_span() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let r = c(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "source_in": 2.0, "timeline_in": 0.0, "timeline_out": 3.0}));
    assert_eq!(r["source_out"], 5.0);
}

#[test]
fn three_point_edit_derives_source_in_from_timeline_span() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let r = c(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "source_out": 6.0, "timeline_in": 1.0, "timeline_out": 4.0}));
    assert_eq!(r["source_in"], 3.0);
}

#[test]
fn edit_points_uses_source_marks_when_source_points_are_omitted() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "mark_source", json!({"asset_id": asset, "in_point": 2.0, "out_point": 5.0}));
    let r = c(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "timeline_in": 0.0}));
    assert_eq!(r["source_in"], 2.0); assert_eq!(r["source_out"], 5.0); assert_eq!(r["timeline_out"], 3.0);
}

#[test]
fn edit_points_explicit_source_points_override_marks() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "mark_source", json!({"asset_id": asset, "in_point": 2.0, "out_point": 5.0}));
    let r = c(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "source_in": 0.0, "source_out": 1.0, "timeline_in": 0.0}));
    assert_eq!(r["source_in"], 0.0); assert_eq!(r["source_out"], 1.0);
}

#[test]
fn edit_points_with_fewer_than_three_points_is_a_clear_error() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let e = err(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "timeline_in": 0.0}));
    assert!(e.contains("needs 3 of"), "{e}");
}

// ---------- edit_points: four-point edit ----------

#[test]
fn four_point_edit_with_matching_spans_uses_speed_1() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let r = c(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "source_in": 0.0, "source_out": 2.0, "timeline_in": 0.0, "timeline_out": 2.0}));
    assert_eq!(r["speed"], 1.0);
}

#[test]
fn four_point_edit_with_mismatched_spans_errors_without_fit() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let e = err(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "source_in": 0.0, "source_out": 4.0, "timeline_in": 0.0, "timeline_out": 2.0}));
    assert!(e.contains("fit:true"), "{e}");
}

#[test]
fn four_point_edit_with_fit_adjusts_speed_to_make_the_source_span_cover_the_timeline_span() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    // 4s of source crammed into 2s of timeline: speed 2.0 (plays twice as fast).
    let r = c(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "source_in": 0.0, "source_out": 4.0, "timeline_in": 0.0, "timeline_out": 2.0, "fit": true}));
    assert_eq!(r["speed"], 2.0);
    let clips = c(&mut s, "find", json!({"track": t}))["matches"].as_array().unwrap().clone();
    assert_eq!(clips[0]["end"], 2.0, "the clip occupies exactly the timeline span, not the source span");
}

#[test]
fn edit_points_mode_insert_ripples_like_add_clip_does() {
    let (mut s, asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type":"media", "asset_id": asset, "start": 0.0, "duration": 5.0}));
    c(&mut s, "edit_points", json!({"track_id": t, "asset_id": asset, "source_in": 0.0, "source_out": 1.0, "timeline_in": 0.0, "timeline_out": 1.0, "mode": "insert"}));
    let clips = c(&mut s, "find", json!({"track": t}))["matches"].as_array().unwrap().clone();
    let mut starts: Vec<f64> = clips.iter().map(|c| c["start"].as_f64().unwrap()).collect(); starts.sort_by(|a,b| a.partial_cmp(b).unwrap());
    assert_eq!(starts, vec![0.0, 1.0], "mode:insert on edit_points must ripple the existing clip forward, same as add_clip");
}

#[test]
fn edit_points_unknown_asset_is_a_clear_error_and_mutates_nothing() {
    let (mut s, _asset) = project();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let before = pj(&s);
    let e = err(&mut s, "edit_points", json!({"track_id": t, "asset_id": "nope", "source_in": 0.0, "source_out": 1.0, "timeline_in": 0.0}));
    assert!(e.contains("no asset"), "{e}");
    assert_eq!(pj(&s), before);
}
