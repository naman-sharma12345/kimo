//! v7.4: reframe_timeline -- a real, persistent, undoable whole-timeline aspect conversion. auto_reframe
//! (v7.2) is single-clip; export_variants (v7.3) touches every clip but restores everything after one
//! export. This is the first tool that actually changes the project's own canvas and keeps the result.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn clip_json(ss: &mut Session, clip_id: &str) -> Value { let p = c(ss, "get_project", json!({})); for t in p["tracks"].as_array().unwrap() { for cl in t["clips"].as_array().unwrap() { if cl["id"] == clip_id { return cl.clone(); } } } panic!("no such clip {clip_id} in project: {p}") }
fn tmp(tag: &str) -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("kimo_reframe_{tag}_{}", std::process::id())); let _ = std::fs::remove_dir_all(&d); std::fs::create_dir_all(&d).unwrap(); d }
fn make_media(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    let status = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=green:s=64x36:d=5:r=25", "-pix_fmt", "yuv420p"]).arg(&p).status().unwrap();
    assert!(status.success()); p
}

fn project_with_media(dir: &std::path::Path) -> (Session, String, String) {
    let m = make_media(dir, "a.mp4");
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 1920, "height": 1080, "fps": 25}));
    let aid = c(&mut ss, "import_media", json!({"path": m.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let tr = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let clip = c(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "media", "asset_id": aid, "start": 0.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    (ss, tr, clip)
}

#[test] fn converting_to_9_16_actually_and_permanently_changes_the_canvas() {
    let dir = tmp("basic"); let (mut ss, _tr, clip) = project_with_media(&dir);
    let r = c(&mut ss, "reframe_timeline", json!({"aspect": "9:16"}));
    assert_eq!(r["to"]["width"], 1080); assert_eq!(r["to"]["height"], 1920, "{r}");
    let p = c(&mut ss, "get_project", json!({}));
    assert_eq!(p["width"], 1080); assert_eq!(p["height"], 1920, "canvas change must stick, not be restored like export_variants");
    let cd = clip_json(&mut ss, &clip);
    assert!(cd["props"].get("reframe_x").is_some() || cd["props"].get("reframe_y").is_some(), "clip should have been keyed by auto_reframe: {cd}");
}

#[test] fn undo_reverses_the_whole_conversion_in_one_step() {
    let dir = tmp("undo"); let (mut ss, _tr, clip) = project_with_media(&dir);
    let before_depth = ss.undo.len();
    c(&mut ss, "reframe_timeline", json!({"aspect": "1:1"}));
    assert_eq!(ss.undo.len(), before_depth + 1, "reframe_timeline must be exactly one undo step");
    c(&mut ss, "undo", json!({}));
    let p = c(&mut ss, "get_project", json!({}));
    assert_eq!(p["width"], 1920); assert_eq!(p["height"], 1080);
    let cd = clip_json(&mut ss, &clip);
    assert!(cd["props"].get("reframe_x").is_none(), "reframe window must be undone too: {cd}");
}

#[test] fn auto_reframe_false_resizes_the_canvas_only_and_leaves_clip_fit_alone() {
    let dir = tmp("noreframe"); let (mut ss, _tr, clip) = project_with_media(&dir);
    let r = c(&mut ss, "reframe_timeline", json!({"aspect": "9:16", "auto_reframe": false}));
    assert_eq!(r["reframed"].as_array().unwrap().len(), 0, "{r}");
    let cd = clip_json(&mut ss, &clip);
    assert!(cd["props"].get("reframe_x").is_none());
}

#[test] fn a_clip_already_manually_framed_is_left_alone_unless_overwrite_existing() {
    let dir = tmp("manual"); let (mut ss, _tr, clip) = project_with_media(&dir);
    c(&mut ss, "auto_reframe", json!({"clip_id": clip, "x": 0.2, "y": 0.3}));
    let r = c(&mut ss, "reframe_timeline", json!({"aspect": "9:16"}));
    let skipped = r["skipped_already_framed"].as_array().unwrap();
    assert_eq!(skipped.len(), 1, "{r}"); assert_eq!(skipped[0], clip);
    let cd = clip_json(&mut ss, &clip);
    assert_eq!(cd["props"]["reframe_x"]["base"], 0.2, "the manual framing must survive untouched: {cd}");

    let r2 = c(&mut ss, "reframe_timeline", json!({"aspect": "1:1", "overwrite_existing": true}));
    assert_eq!(r2["reframed"].as_array().unwrap().len(), 1, "{r2}");
}

#[test] fn dry_run_reports_without_touching_the_project() {
    let dir = tmp("dryrun"); let (mut ss, _tr, _clip) = project_with_media(&dir);
    let before = c(&mut ss, "get_project", json!({}));
    let r = c(&mut ss, "reframe_timeline", json!({"aspect": "9:16", "dry_run": true}));
    assert_eq!(r["dry_run"], true); assert_eq!(r["would_reframe"].as_array().unwrap().len(), 1, "{r}");
    let after = c(&mut ss, "get_project", json!({}));
    assert_eq!(before["width"], after["width"]); assert_eq!(before["height"], after["height"]);
}

#[test] fn an_already_matching_aspect_reframes_nothing() {
    let dir = tmp("samesize"); let (mut ss, _tr, clip) = project_with_media(&dir);
    // project is already 1920x1080 == 16:9
    let r = c(&mut ss, "reframe_timeline", json!({"aspect": "16:9"}));
    assert_eq!(r["aspect_unchanged"], true, "{r}");
    assert_eq!(r["reframed"].as_array().unwrap().len(), 0, "{r}");
    let cd = clip_json(&mut ss, &clip);
    assert!(cd["props"].get("reframe_x").is_none());
}

#[test] fn explicit_width_height_works_without_a_named_aspect() {
    let dir = tmp("explicit"); let (mut ss, _tr, _clip) = project_with_media(&dir);
    let r = c(&mut ss, "reframe_timeline", json!({"width": 800, "height": 800}));
    assert_eq!(r["to"]["width"], 800); assert_eq!(r["to"]["height"], 800);
}

#[test] fn non_media_clips_are_never_touched() {
    let dir = tmp("nonmedia"); let (mut ss, tr, _clip) = project_with_media(&dir);
    let shape = c(&mut ss, "add_clip", json!({"track_id": tr, "source_type": "solid", "color": [1.0, 0.0, 0.0], "start": 3.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut ss, "reframe_timeline", json!({"aspect": "9:16"}));
    assert_eq!(r["candidates"], 1, "only the media clip should count: {r}");
    let cd = clip_json(&mut ss, &shape);
    assert!(cd["props"].get("reframe_x").is_none());
}

#[test] fn track_id_restricts_which_clips_get_reframed() {
    let dir = tmp("trackfilter"); let m = make_media(&dir, "b.mp4");
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 1920, "height": 1080, "fps": 25}));
    let aid = c(&mut ss, "import_media", json!({"path": m.to_str().unwrap()}))["asset_id"].as_str().unwrap().to_string();
    let t1 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c1 = c(&mut ss, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": aid, "start": 0.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    let c2 = c(&mut ss, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": aid, "start": 0.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "reframe_timeline", json!({"aspect": "9:16", "track_id": t1}));
    let cd1 = clip_json(&mut ss, &c1); assert!(cd1["props"].get("reframe_x").is_some());
    let cd2 = clip_json(&mut ss, &c2); assert!(cd2["props"].get("reframe_x").is_none(), "track_id must restrict scope: {cd2}");
}

#[test] fn bad_aspect_name_is_a_clean_error() {
    let dir = tmp("badaspect"); let (mut ss, _tr, _clip) = project_with_media(&dir);
    let e = dispatch(&mut ss, "reframe_timeline", &json!({"aspect": "21:9"})).unwrap_err();
    assert!(e.contains("unknown aspect"), "{e}");
}

#[test] fn no_size_given_is_a_clean_error() {
    let dir = tmp("nosize"); let (mut ss, _tr, _clip) = project_with_media(&dir);
    let e = dispatch(&mut ss, "reframe_timeline", &json!({})).unwrap_err();
    assert!(e.contains("give aspect") || e.contains("width"), "{e}");
}

