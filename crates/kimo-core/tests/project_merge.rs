//! v7 additions: project_merge -- apply clip-level changes between two project revisions onto the live
//! project. Mirrors project_diff's own test style (checkpoints as revisions, byte-identical undo checks).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }

/// Base project: one track, one clip "a".
fn scene() -> (Session, String, String) {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.1,0.1,0.1], "start": 0.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    (ss, t, a)
}

#[test] fn clean_fast_forward_adds_and_moves_with_no_conflicts() {
    let (mut ss, t, a) = scene();
    c(&mut ss, "checkpoint", json!({"name": "base"}));
    // "theirs": add a second clip, move the first.
    let b = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2,0.3,0.4], "start": 5.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "move_clip", json!({"clip_id": a, "start": 3.0}));
    c(&mut ss, "checkpoint", json!({"name": "theirs"}));
    c(&mut ss, "restore", json!({"name": "base"})); // current is back to base
    assert_eq!(c(&mut ss, "find", json!({}))["count"], 1, "current only has clip a again, at start 0");

    let r = c(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:theirs"}));
    assert_eq!(r["ok"], true); assert_eq!(r["applied"], true);
    let mut applied: Vec<String> = r["applied_clips"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect(); applied.sort();
    let mut want = vec![a.clone(), b.clone()]; want.sort(); assert_eq!(applied, want);
    assert_eq!(r["conflicts"].as_array().unwrap().len(), 0);
    let moved = c(&mut ss, "find", json!({"time_range": [3.0, 3.0]})); assert_eq!(moved["count"], 1, "a's moved start carried over");
    assert_eq!(c(&mut ss, "find", json!({}))["count"], 2, "b was added");
}

#[test] fn edit_edit_conflict_blocks_by_default_and_each_policy_resolves_it() {
    let (mut ss, _t, a) = scene();
    c(&mut ss, "checkpoint", json!({"name": "base"}));
    c(&mut ss, "move_clip", json!({"clip_id": a, "start": 5.0})); // theirs moves a to 5
    c(&mut ss, "checkpoint", json!({"name": "theirs"}));
    c(&mut ss, "restore", json!({"name": "base"}));
    c(&mut ss, "move_clip", json!({"clip_id": a, "start": 3.0})); // current independently moves a to 3
    let h = pj(&ss); let u = ss.undo.len();

    // default: fail -- nothing applied, conflict reported
    let r = c(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:theirs"}));
    assert_eq!(r["ok"], false); assert_eq!(r["applied"], false);
    assert_eq!(r["conflicts"][0]["kind"], "edit_edit_conflict"); assert_eq!(r["conflicts"][0]["id"], a);
    assert_eq!(pj(&ss), h, "a failed (conflicting) merge changes nothing");
    assert_eq!(ss.undo.len(), u, "a merge that applies nothing pushes no undo step (no_history)");

    // on_conflict: base -- current's own edit (start 3) wins, nothing applied, but call still succeeds
    let r2 = c(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:theirs", "on_conflict": "base"}));
    assert_eq!(r2["ok"], true); assert_eq!(r2["conflicts"][0]["kind"], "edit_edit_conflict");
    assert_eq!(c(&mut ss, "find", json!({"time_range": [3.0, 3.0]}))["count"], 1, "current's start=3 preserved");
    assert_eq!(pj(&ss), h, "on_conflict:\"base\" applied nothing (conflict was the only candidate change), so no undo is even needed");

    // on_conflict: theirs -- theirs' edit (start 5) wins
    let r3 = c(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:theirs", "on_conflict": "theirs"}));
    assert_eq!(r3["ok"], true); assert_eq!(r3["conflicts"][0]["kind"], "edit_edit_conflict"); assert_eq!(r3["conflicts_resolved_by"], "theirs");
    assert_eq!(c(&mut ss, "find", json!({"time_range": [5.0, 5.0]}))["count"], 1, "theirs' start=5 applied");
}

#[test] fn remove_modify_conflict_and_clean_removal() {
    let (mut ss, t, a) = scene();
    let b = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.5,0.5,0.5], "start": 4.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "checkpoint", json!({"name": "base"}));
    c(&mut ss, "delete_clip", json!({"clip_id": a})); // theirs removes a untouched
    c(&mut ss, "delete_clip", json!({"clip_id": b})); // theirs also removes b
    c(&mut ss, "checkpoint", json!({"name": "theirs"}));
    c(&mut ss, "restore", json!({"name": "base"}));
    c(&mut ss, "move_clip", json!({"clip_id": b, "start": 6.0})); // current independently edits b since base

    let r = c(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:theirs"}));
    assert_eq!(r["ok"], false, "b's remove_modify_conflict blocks the default policy");
    assert_eq!(r["conflicts"][0]["kind"], "remove_modify_conflict"); assert_eq!(r["conflicts"][0]["id"], b);

    let r2 = c(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:theirs", "on_conflict": "theirs"}));
    assert_eq!(r2["ok"], true);
    let removed: Vec<String> = r2["removed_clips"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(removed.contains(&a) && removed.contains(&b), "a removed cleanly, b removed because on_conflict=theirs");
    assert_eq!(c(&mut ss, "find", json!({}))["count"], 0);
}

#[test] fn dry_run_reports_without_changing_anything_and_identical_revisions_do_nothing() {
    let (mut ss, t, a) = scene();
    c(&mut ss, "checkpoint", json!({"name": "base"}));
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.9,0.1,0.1], "start": 9.0, "duration": 1.0}));
    c(&mut ss, "checkpoint", json!({"name": "theirs"}));
    c(&mut ss, "restore", json!({"name": "base"}));
    let h = pj(&ss); let u = ss.undo.len();
    let r = c(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:theirs", "dry_run": true}));
    assert_eq!(r["applied"], false); assert_eq!(r["would_apply"], 1); assert_eq!(r["would_remove"], 0);
    assert_eq!(pj(&ss), h, "dry_run changes nothing"); assert_eq!(ss.undo.len(), u);

    // merging a revision against itself: nothing to apply, no_history, call still succeeds
    let r2 = c(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:base"}));
    assert_eq!(r2["ok"], true); assert_eq!(r2["applied_clips"].as_array().unwrap().len(), 0); assert_eq!(r2["removed_clips"].as_array().unwrap().len(), 0);
    assert_eq!(pj(&ss), h); assert_eq!(ss.undo.len(), u);
    let _ = a; // keep binding; a is exercised via find/time_range elsewhere in this suite
}

#[test] fn bad_args_are_typed_errors_and_merge_is_itself_undoable() {
    let (mut ss, t, a) = scene();
    c(&mut ss, "checkpoint", json!({"name": "base"}));
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.0,0.9,0.0], "start": 8.0, "duration": 1.0}));
    c(&mut ss, "checkpoint", json!({"name": "theirs"}));
    c(&mut ss, "restore", json!({"name": "base"}));
    assert!(e(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:nope"})).contains("no checkpoint"));
    assert!(e(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:base", "on_conflict": "sideways"})).contains("on_conflict"));
    let before_count = c(&mut ss, "find", json!({}))["count"].clone();
    c(&mut ss, "project_merge", json!({"base": "checkpoint:base", "theirs": "checkpoint:theirs"}));
    assert_eq!(c(&mut ss, "find", json!({}))["count"].as_u64().unwrap(), before_count.as_u64().unwrap() + 1);
    c(&mut ss, "undo", json!({}));
    assert_eq!(c(&mut ss, "find", json!({}))["count"], before_count, "the merge itself is one ordinary undo step");
    let _ = a;
}
