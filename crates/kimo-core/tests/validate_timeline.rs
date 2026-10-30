//! v7.2 additions: validate_timeline -- structural health check (gaps, overlaps, zero/negative
//! duration, negative start), independent of lint_edit (platform lint) and list_dependencies (missing
//! media). Read-only: never mutates the project or pushes undo history.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }

fn base() -> Session { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); ss }
fn add(ss: &mut Session, t: &str, start: f64, dur: f64) -> String {
    c(ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.1,0.2,0.3], "start": start, "duration": dur}))["clip_id"].as_str().unwrap().to_string()
}

#[test] fn clean_butt_joined_timeline_is_ok_with_no_issues() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    add(&mut ss, &t, 0.0, 1.0); add(&mut ss, &t, 1.0, 1.0); add(&mut ss, &t, 2.0, 1.0);
    let h = pj(&ss); let u = ss.undo.len();
    let r = c(&mut ss, "validate_timeline", json!({}));
    assert_eq!(r["ok"], true);
    assert_eq!(r["summary"]["total"], 0);
    assert_eq!(r["issues"].as_array().unwrap().len(), 0);
    assert_eq!(pj(&ss), h, "read-only: project byte-identical"); assert_eq!(ss.undo.len(), u, "no undo entry pushed");
}

#[test] fn gap_between_clips_on_same_track_is_reported() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = add(&mut ss, &t, 0.0, 1.0); let b = add(&mut ss, &t, 3.0, 1.0); // 2s gap
    let r = c(&mut ss, "validate_timeline", json!({}));
    assert_eq!(r["ok"], false);
    assert_eq!(r["summary"]["gaps"], 1);
    assert_eq!(r["summary"]["total"], 1);
    let issue = &r["issues"][0];
    assert_eq!(issue["kind"], "gap");
    assert_eq!(issue["after"]["id"], a);
    assert_eq!(issue["before"]["id"], b);
    assert!((issue["gap_seconds"].as_f64().unwrap() - 2.0).abs() < 1e-6);
}

#[test] fn overlap_between_clips_on_same_track_is_reported() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    add(&mut ss, &t, 0.0, 2.0); add(&mut ss, &t, 1.0, 2.0); // overlaps by 1s
    let r = c(&mut ss, "validate_timeline", json!({}));
    assert_eq!(r["summary"]["overlaps"], 1);
    assert_eq!(r["issues"][0]["kind"], "overlap");
    assert!((r["issues"][0]["overlap_seconds"].as_f64().unwrap() - 1.0).abs() < 1e-6);
}

#[test] fn min_gap_seconds_filters_out_small_gaps() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    add(&mut ss, &t, 0.0, 1.0); add(&mut ss, &t, 1.02, 1.0); // 0.02s gap (e.g. sub-frame rounding)
    let r = c(&mut ss, "validate_timeline", json!({}));
    assert_eq!(r["summary"]["gaps"], 1, "default min_gap_seconds=0 reports any positive gap");
    let r2 = c(&mut ss, "validate_timeline", json!({"min_gap_seconds": 0.1}));
    assert_eq!(r2["ok"], true, "a gap under the threshold is ignored");
    assert_eq!(r2["summary"]["gaps"], 0);
}

#[test] fn tracks_are_independent_and_two_tracks_can_overlap_each_other_freely() {
    let mut ss = base();
    let t1 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    add(&mut ss, &t1, 0.0, 5.0); add(&mut ss, &t2, 0.0, 5.0); // same span, different tracks: fine
    let r = c(&mut ss, "validate_timeline", json!({}));
    assert_eq!(r["ok"], true, "overlap across different tracks is not flagged");
}

#[test] fn track_id_scopes_the_check_to_one_track() {
    let mut ss = base();
    let t1 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    add(&mut ss, &t1, 0.0, 1.0); add(&mut ss, &t1, 5.0, 1.0); // gap on t1
    add(&mut ss, &t2, 0.0, 1.0); add(&mut ss, &t2, 1.0, 1.0); // clean on t2
    let r1 = c(&mut ss, "validate_timeline", json!({"track_id": t1}));
    assert_eq!(r1["summary"]["gaps"], 1);
    let r2 = c(&mut ss, "validate_timeline", json!({"track_id": t2}));
    assert_eq!(r2["ok"], true);
}

#[test] fn negative_start_and_zero_or_negative_duration_are_reported() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    add(&mut ss, &t, 0.0, 1.0);
    // Reach into the project directly to construct otherwise-unreachable-through-tools bad states
    // (add_clip itself refuses negative start/duration -- this proves validate_timeline catches states
    // that arrive some other way, e.g. a hand-edited .kimo file loaded with load_project).
    {
        let tr = ss.project.tracks.iter_mut().find(|tr| tr.id == t).unwrap();
        let mut bad1 = (*tr.clips[0]).clone(); bad1.id = "bad_neg_start".into(); bad1.start = -2.0; bad1.duration = 1.0;
        let mut bad2 = (*tr.clips[0]).clone(); bad2.id = "bad_zero_dur".into(); bad2.start = 10.0; bad2.duration = 0.0;
        tr.clips.push(std::sync::Arc::new(bad1)); tr.clips.push(std::sync::Arc::new(bad2));
    }
    let r = c(&mut ss, "validate_timeline", json!({}));
    assert_eq!(r["summary"]["negative_start"], 1);
    assert_eq!(r["summary"]["zero_or_negative_duration"], 1);
    let kinds: Vec<&str> = r["issues"].as_array().unwrap().iter().map(|i| i["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"negative_start"));
    assert!(kinds.contains(&"zero_or_negative_duration"));
}

#[test] fn empty_project_and_empty_track_are_ok() {
    let mut ss = base();
    let r = c(&mut ss, "validate_timeline", json!({}));
    assert_eq!(r["ok"], true, "no tracks at all");
    c(&mut ss, "add_track", json!({}));
    let r2 = c(&mut ss, "validate_timeline", json!({}));
    assert_eq!(r2["ok"], true, "one track, no clips");
}

#[test] fn unmatched_plain_track_id_scopes_to_nothing_not_everything() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    add(&mut ss, &t, 0.0, 1.0); add(&mut ss, &t, 5.0, 1.0); // a real gap on the real track
    // A plain id that matches no track (not an @name lookup) scopes to nothing and finds nothing --
    // confirm it does NOT silently fall back to checking every track (which would hide the real gap
    // behind a false "ok", worse than an error).
    let r = c(&mut ss, "validate_timeline", json!({"track_id": "no_such_track_id"}));
    assert_eq!(r["ok"], true);
    assert_eq!(r["summary"]["total"], 0);
}

#[test] fn unknown_at_name_track_id_errors_like_find_does() {
    let mut ss = base();
    c(&mut ss, "add_track", json!({}));
    let e = dispatch(&mut ss, "validate_timeline", &json!({"track_id": "@nope"})).unwrap_err();
    assert!(e.contains("name_not_found") || e.contains("no track named"), "{e}");
}

