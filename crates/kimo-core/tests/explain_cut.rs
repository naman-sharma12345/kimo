//! v7.2 additions: explain_cut -- best-effort "why is there a cut here". No new detection logic: reads
//! transition_in, correlates with nearby markers (add_marker) and asks detect_cuts about a small window
//! around the boundary via dispatch_nested.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).unwrap_err() }

fn solid_fixture(name: &str, color: &str, secs: f64) -> String {
    let p = format!("/tmp/kimo-explaincut-{}-{name}.mp4", std::process::id());
    assert!(std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", &format!("color=c={color}:size=64x48:rate=25:duration={secs}"), "-pix_fmt", "yuv420p", &p])
        .status().unwrap().success());
    p
}

fn base() -> Session { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 48, "fps": 25})); ss }
fn import(ss: &mut Session, path: &str) -> String { c(ss, "import_media", json!({"path": path}))["asset_id"].as_str().unwrap().to_string() }

#[test] fn the_first_clip_on_a_track_has_no_incoming_cut_to_explain() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p = solid_fixture("first", "red", 1.0);
    let aid = import(&mut ss, &p);
    let cid = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": aid, "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "explain_cut", json!({"clip_id": cid}));
    assert_eq!(r["is_first_on_track"], true, "{r}");
    std::fs::remove_file(&p).ok();
}

#[test] fn unknown_clip_id_errors_plainly() {
    let mut ss = base();
    assert!(e(&mut ss, "explain_cut", json!({"clip_id": "nope"})).contains("no clip"));
}

#[test] fn an_explicit_transition_is_reported_and_named() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p1 = solid_fixture("tr1", "red", 1.0); let a1 = import(&mut ss, &p1);
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 0.0, "duration": 1.0}));
    let p2 = solid_fixture("tr2", "blue", 1.0); let a2 = import(&mut ss, &p2);
    let c2 = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a2, "start": 1.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_transition", json!({"clip_id": c2, "kind": "crossfade", "duration": 0.3}));
    let r = c(&mut ss, "explain_cut", json!({"clip_id": c2}));
    assert_eq!(r["has_incoming_transition"], true, "{r}");
    assert_eq!(r["transition_kind"], "crossfade", "{r}");
    assert!(r["explanation"].as_str().unwrap().contains("crossfade"), "{r}");
    std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

#[test] fn same_source_asset_as_the_previous_clip_is_flagged() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p = solid_fixture("same", "green", 2.0); let a = import(&mut ss, &p);
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a.clone(), "start": 0.0, "duration": 1.0, "in_point": 0.0}));
    let c2 = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 1.0, "duration": 1.0, "in_point": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "explain_cut", json!({"clip_id": c2}));
    assert_eq!(r["same_asset_as_previous"], true, "{r}");
    std::fs::remove_file(&p).ok();
}

#[test] fn different_source_assets_is_flagged_false() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p1 = solid_fixture("diff1", "red", 1.0); let a1 = import(&mut ss, &p1);
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 0.0, "duration": 1.0}));
    let p2 = solid_fixture("diff2", "blue", 1.0); let a2 = import(&mut ss, &p2);
    let c2 = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a2, "start": 1.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "explain_cut", json!({"clip_id": c2}));
    assert_eq!(r["same_asset_as_previous"], false, "{r}");
    std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

#[test] fn a_marker_near_the_cut_point_shows_up_and_is_named_in_the_explanation() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p1 = solid_fixture("mk1", "red", 1.0); let a1 = import(&mut ss, &p1);
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 0.0, "duration": 1.0}));
    let p2 = solid_fixture("mk2", "blue", 1.0); let a2 = import(&mut ss, &p2);
    let c2 = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a2, "start": 1.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_marker", json!({"time": 1.02, "name": "beat 3", "color": "blue"}));
    let r = c(&mut ss, "explain_cut", json!({"clip_id": c2, "window_seconds": 0.5}));
    let names: Vec<&str> = r["nearby_markers"].as_array().unwrap().iter().map(|m| m["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"beat 3"), "{r}");
    assert!(r["explanation"].as_str().unwrap().contains("beat 3"), "{r}");
}

#[test] fn a_tight_window_excludes_a_marker_that_is_too_far() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p1 = solid_fixture("far1", "red", 2.0); let a1 = import(&mut ss, &p1);
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 0.0, "duration": 2.0}));
    let p2 = solid_fixture("far2", "blue", 2.0); let a2 = import(&mut ss, &p2);
    let c2 = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a2, "start": 2.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_marker", json!({"time": 0.5, "name": "unrelated note", "color": "red"})); // far from the cut at t=2.0
    let r = c(&mut ss, "explain_cut", json!({"clip_id": c2, "window_seconds": 0.2}));
    assert!(r["nearby_markers"].as_array().unwrap().is_empty(), "{r}");
    std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

#[test] fn a_real_color_change_is_found_by_detect_cuts() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p1 = solid_fixture("vc1", "black", 1.0); let a1 = import(&mut ss, &p1);
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 0.0, "duration": 1.0}));
    let p2 = solid_fixture("vc2", "white", 1.0); let a2 = import(&mut ss, &p2);
    let c2 = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a2, "start": 1.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "explain_cut", json!({"clip_id": c2}));
    assert_eq!(r["detected_as_visual_cut"], true, "{r}");
    std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

#[test] fn no_visual_change_across_the_cut_is_reported_as_not_detected() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p1 = solid_fixture("nc1", "gray", 1.0); let a1 = import(&mut ss, &p1);
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 0.0, "duration": 1.0}));
    let p2 = solid_fixture("nc2", "gray", 1.0); let a2 = import(&mut ss, &p2); // identical colour: a cut with zero visual change
    let c2 = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a2, "start": 1.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "explain_cut", json!({"clip_id": c2}));
    assert_eq!(r["detected_as_visual_cut"], false, "{r}");
    assert!(r["explanation"].as_str().unwrap().contains("no strong visual change"), "{r}");
    std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

#[test] fn gap_before_is_reported_when_the_previous_clip_does_not_touch() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p1 = solid_fixture("gap1", "red", 1.0); let a1 = import(&mut ss, &p1);
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 0.0, "duration": 1.0}));
    let p2 = solid_fixture("gap2", "blue", 1.0); let a2 = import(&mut ss, &p2);
    let c2 = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a2, "start": 2.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string(); // 1s gap
    let r = c(&mut ss, "explain_cut", json!({"clip_id": c2}));
    assert!((r["gap_before_s"].as_f64().unwrap() - 1.0).abs() < 0.01, "{r}");
    std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

#[test] fn is_read_only_on_the_project() {
    let mut ss = base();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let p1 = solid_fixture("ro1", "red", 1.0); let a1 = import(&mut ss, &p1);
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a1, "start": 0.0, "duration": 1.0}));
    let p2 = solid_fixture("ro2", "blue", 1.0); let a2 = import(&mut ss, &p2);
    let c2 = c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a2, "start": 1.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let h = serde_json::to_string(&ss.project).unwrap(); let u = ss.undo.len();
    c(&mut ss, "explain_cut", json!({"clip_id": c2}));
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), h); assert_eq!(ss.undo.len(), u);
    std::fs::remove_file(&p1).ok(); std::fs::remove_file(&p2).ok();
}

