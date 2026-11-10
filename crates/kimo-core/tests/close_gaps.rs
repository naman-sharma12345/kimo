//! v7.5 feature: close_gaps -- a general-purpose ripple-close for ANY gap on a track (or the whole
//! timeline in one call), independent of why the gap is there. `ripple_delete` closes the one gap left
//! by deleting a single clip; `drop_short_segments` closes gaps left by removing clips under a duration
//! threshold. Neither helps after a manual delete, a trim, a move, or an import left several unrelated
//! gaps scattered across one or many tracks -- today that is a manual "select clip, note the gap, move
//! it, repeat" per gap, per track. close_gaps walks a track's clips in timeline order and snaps each
//! one to touch the clip before it (or to the project start, if include_leading_gap is set), across as
//! many tracks as you name -- or, by default, every unlocked track in the project -- in one call.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn try_c(ss: &mut Session, n: &str, v: Value) -> Result<Value, String> { dispatch(ss, n, &v) }

fn new_proj() -> Session {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    ss
}

fn add_track(ss: &mut Session) -> String { c(ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string() }

fn add_solid_clip(ss: &mut Session, track_id: &str, start: f64, duration: f64) -> String {
    c(ss, "add_clip", json!({"track_id": track_id, "source_type": "solid", "color": [0.3, 0.4, 0.5], "start": start, "duration": duration}))["clip_id"].as_str().unwrap().to_string()
}

fn clip_start(ss: &mut Session, clip_id: &str) -> f64 {
    let p = c(ss, "get_project", json!({}));
    for t in p["tracks"].as_array().unwrap() {
        for cl in t["clips"].as_array().unwrap() {
            if cl["id"] == clip_id { return cl["start"].as_f64().unwrap(); }
        }
    }
    panic!("clip {clip_id} not found");
}

#[test]
fn closes_a_single_gap_between_two_clips_on_one_track() {
    let mut ss = new_proj();
    let t = add_track(&mut ss);
    let a = add_solid_clip(&mut ss, &t, 0.0, 1.0);
    let b = add_solid_clip(&mut ss, &t, 3.0, 1.0); // 2s gap after a
    let r = c(&mut ss, "close_gaps", json!({"track_ids": [t]}));
    assert_eq!(r["total_gaps_closed"], 1, "{r}");
    assert!((r["total_shifted_seconds"].as_f64().unwrap() - 2.0).abs() < 1e-6, "{r}");
    assert_eq!(clip_start(&mut ss, &a), 0.0, "first clip must not move");
    assert_eq!(clip_start(&mut ss, &b), 1.0, "second clip should now touch the first clip's end");
}

#[test]
fn default_leading_gap_is_preserved_the_first_clip_does_not_move_to_zero() {
    let mut ss = new_proj();
    let t = add_track(&mut ss);
    let a = add_solid_clip(&mut ss, &t, 2.0, 1.0); // leading gap of 2s from project start
    let b = add_solid_clip(&mut ss, &t, 5.0, 1.0); // 2s gap after a
    c(&mut ss, "close_gaps", json!({"track_ids": [t]}));
    assert_eq!(clip_start(&mut ss, &a), 2.0, "leading gap must be left alone by default");
    assert_eq!(clip_start(&mut ss, &b), 3.0, "b should touch a's end (3.0), not snap to 0");
}

#[test]
fn include_leading_gap_true_also_closes_the_gap_from_project_start() {
    let mut ss = new_proj();
    let t = add_track(&mut ss);
    let a = add_solid_clip(&mut ss, &t, 2.0, 1.0);
    c(&mut ss, "close_gaps", json!({"track_ids": [t], "include_leading_gap": true}));
    assert_eq!(clip_start(&mut ss, &a), 0.0, "include_leading_gap should snap the first clip to 0");
}

#[test]
fn closes_multiple_gaps_in_one_call_across_two_tracks_when_track_ids_is_omitted() {
    let mut ss = new_proj();
    let t1 = add_track(&mut ss);
    let t2 = add_track(&mut ss);
    let a1 = add_solid_clip(&mut ss, &t1, 0.0, 1.0);
    let b1 = add_solid_clip(&mut ss, &t1, 4.0, 1.0);
    let a2 = add_solid_clip(&mut ss, &t2, 0.0, 2.0);
    let b2 = add_solid_clip(&mut ss, &t2, 5.0, 1.0);
    let r = c(&mut ss, "close_gaps", json!({})); // no track_ids: whole timeline, one call
    assert_eq!(r["total_gaps_closed"], 2, "{r}");
    assert_eq!(clip_start(&mut ss, &a1), 0.0);
    assert_eq!(clip_start(&mut ss, &b1), 1.0);
    assert_eq!(clip_start(&mut ss, &a2), 0.0);
    assert_eq!(clip_start(&mut ss, &b2), 2.0);
}

#[test]
fn a_locked_track_is_skipped_and_reported_when_track_ids_is_not_given() {
    let mut ss = new_proj();
    let t1 = add_track(&mut ss);
    let t2 = add_track(&mut ss);
    add_solid_clip(&mut ss, &t1, 0.0, 1.0);
    let b1 = add_solid_clip(&mut ss, &t1, 3.0, 1.0);
    let a2 = add_solid_clip(&mut ss, &t2, 0.0, 1.0);
    let b2 = add_solid_clip(&mut ss, &t2, 3.0, 1.0);
    c(&mut ss, "set_track", json!({"track_id": t2, "locked": true}));
    let r = c(&mut ss, "close_gaps", json!({}));
    assert_eq!(r["tracks_skipped_locked"].as_array().unwrap(), &vec![json!(t2)], "{r}");
    assert_eq!(clip_start(&mut ss, &b1), 1.0, "unlocked track still gets closed");
    assert_eq!(clip_start(&mut ss, &a2), 0.0, "locked track untouched");
    assert_eq!(clip_start(&mut ss, &b2), 3.0, "locked track untouched");
}

#[test]
fn explicitly_naming_a_locked_track_errors_instead_of_silently_skipping() {
    let mut ss = new_proj();
    let t = add_track(&mut ss);
    add_solid_clip(&mut ss, &t, 0.0, 1.0);
    c(&mut ss, "set_track", json!({"track_id": t, "locked": true}));
    let err = try_c(&mut ss, "close_gaps", json!({"track_ids": [t]})).unwrap_err();
    assert!(err.contains("locked"), "{err}");
}

#[test]
fn dry_run_reports_what_would_happen_and_changes_nothing() {
    let mut ss = new_proj();
    let t = add_track(&mut ss);
    let a = add_solid_clip(&mut ss, &t, 0.0, 1.0);
    let b = add_solid_clip(&mut ss, &t, 3.0, 1.0);
    let r = c(&mut ss, "close_gaps", json!({"track_ids": [t], "dry_run": true}));
    assert_eq!(r["dry_run"], true);
    assert_eq!(r["total_gaps_closed"], 1, "{r}");
    assert_eq!(clip_start(&mut ss, &a), 0.0);
    assert_eq!(clip_start(&mut ss, &b), 3.0, "dry_run must not move anything");
}

#[test]
fn min_gap_threshold_leaves_small_gaps_alone() {
    let mut ss = new_proj();
    let t = add_track(&mut ss);
    add_solid_clip(&mut ss, &t, 0.0, 1.0);
    let b = add_solid_clip(&mut ss, &t, 1.05, 1.0); // tiny 0.05s gap
    let c2 = add_solid_clip(&mut ss, &t, 5.0, 1.0); // real 2.95s gap after b
    let r = c(&mut ss, "close_gaps", json!({"track_ids": [t], "min_gap": 0.5}));
    assert_eq!(r["total_gaps_closed"], 1, "{r}: only the real gap should close");
    assert_eq!(clip_start(&mut ss, &b), 1.05, "tiny gap under the threshold stays put");
    assert_eq!(clip_start(&mut ss, &c2), 2.05, "later clip still closes up against b's real end");
}

#[test]
fn close_gaps_is_undoable() {
    let mut ss = new_proj();
    let t = add_track(&mut ss);
    add_solid_clip(&mut ss, &t, 0.0, 1.0);
    let b = add_solid_clip(&mut ss, &t, 3.0, 1.0);
    c(&mut ss, "close_gaps", json!({"track_ids": [t]}));
    assert_eq!(clip_start(&mut ss, &b), 1.0);
    c(&mut ss, "undo", json!({}));
    assert_eq!(clip_start(&mut ss, &b), 3.0, "undo should restore the gap");
}

#[test]
fn no_gaps_is_a_harmless_no_op() {
    let mut ss = new_proj();
    let t = add_track(&mut ss);
    add_solid_clip(&mut ss, &t, 0.0, 1.0);
    add_solid_clip(&mut ss, &t, 1.0, 1.0);
    let r = c(&mut ss, "close_gaps", json!({"track_ids": [t]}));
    assert_eq!(r["total_gaps_closed"], 0, "{r}");
}

#[test]
fn errors_on_an_unknown_track_id() {
    let mut ss = new_proj();
    let err = try_c(&mut ss, "close_gaps", json!({"track_ids": ["nope"]})).unwrap_err();
    assert!(err.to_lowercase().contains("track"), "{err}");
}
