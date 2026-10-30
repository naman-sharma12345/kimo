//! v7.2 additions: self_review -- one-call orchestration of lint_edit + validate_timeline +
//! list_dependencies (missing only), with an optional contact sheet of flagged moments. Pure
//! composition: no new checking logic, so these tests mostly confirm the correlation/plumbing, not
//! lint_edit/validate_timeline's own checks (which have their own test files).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
/// A real media clip WITH an audible audio track (-14 LUFS-ish sine), so lint_edit's silent_mix
/// check -- a legitimate finding on a genuinely silent clip -- doesn't make "clean" scenes noisy.
fn media_fixture(name: &str) -> String {
    let p = format!("/tmp/kimo-selfreview-fixture-{}-{name}.mp4", std::process::id());
    assert!(std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc=size=64x48:rate=25:duration=3", "-f", "lavfi", "-i", "sine=frequency=440:duration=3",
               "-pix_fmt", "yuv420p", "-shortest", &p])
        .status().unwrap().success());
    p
}
/// A clean project with one real media clip (not a solid -- lint_edit's own top_media check only
/// counts Source::Media as "a picture", so a solid-only timeline always reads empty_frame; that is
/// lint_edit's existing, pre-existing behaviour, not something to route around in self_review).
fn clean_media_scene(tag: &str) -> (Session, String) {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 64, "height": 48, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let path = media_fixture(tag); // tag must be unique per call site -- two tests sharing one tag race on the same /tmp path (same pid) under real concurrency
    let aid = c(&mut ss, "import_media", json!({"path": path.clone()}))["asset_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": aid, "start": 0.0, "duration": 3.0}));
    (ss, path)
}

fn scene_with_a_gap() -> Session {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.1,0.1,0.1], "start": 0.0, "duration": 2.0}));
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2,0.2,0.2], "start": 5.0, "duration": 2.0})); // 3s gap
    ss
}

#[test] fn ok_is_the_and_of_all_three_sources_not_independent_of_them() {
    // Not asserting lint_edit finds zero problems on the fixture (loudness/dark-frame thresholds are
    // lint_edit's own business, not self_review's) -- only that self_review's own "ok" is exactly the
    // AND of the three sources it combines, which is the actual thing this tool is responsible for.
    let (mut ss, path) = clean_media_scene("ok-and");
    let r = c(&mut ss, "self_review", json!({}));
    assert_eq!(r["summary"]["timeline_issues"], 0, "{r}");
    assert_eq!(r["summary"]["missing_media"], 0, "{r}");
    let lint_ok = r["lint_edit"]["ok"].as_bool().unwrap_or(true); // true when lint_edit was skipped (empty timeline)
    let want_ok = lint_ok && r["validate_timeline"]["ok"] == true && r["missing_media"].as_array().unwrap().is_empty();
    assert_eq!(r["ok"], want_ok, "{r}");
    assert_eq!(r["summary"]["lint_problems"], r["lint_edit"]["problems"].as_array().map(|a| a.len()).unwrap_or(0), "{r}");
    std::fs::remove_file(&path).ok();
}

#[test] fn a_gap_from_validate_timeline_shows_up_in_the_merged_report() {
    let mut ss = scene_with_a_gap();
    let r = c(&mut ss, "self_review", json!({}));
    assert_eq!(r["ok"], false);
    assert_eq!(r["summary"]["timeline_issues"], 1);
    assert_eq!(r["validate_timeline"]["summary"]["gaps"], 1);
    assert!(r["flagged_times"].as_array().unwrap().len() >= 1, "the gap's boundary is a flagged moment");
}

#[test] fn empty_timeline_lint_edit_error_is_reported_not_a_self_review_failure() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    // No clips at all: lint_edit itself errors ("the timeline is empty"); validate_timeline is still fine.
    let r = c(&mut ss, "self_review", json!({})); // must not itself error
    assert_eq!(r["lint_edit"]["skipped"], true);
    assert!(r["lint_edit"]["reason"].as_str().unwrap().contains("empty"));
    assert_eq!(r["validate_timeline"]["ok"], true);
}

#[test] fn missing_media_from_list_dependencies_is_surfaced_and_ok_goes_false() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    // Import a real file, then delete it on disk so list_dependencies reports it missing.
    let p = format!("/tmp/kimo-selfreview-{}.mp4", std::process::id());
    assert!(std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","lavfi","-i","testsrc=size=64x48:rate=25:duration=1","-pix_fmt","yuv420p",&p]).status().unwrap().success());
    let aid = c(&mut ss, "import_media", json!({"path": p.clone()}))["asset_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": aid, "start": 0.0, "duration": 1.0}));
    std::fs::remove_file(&p).unwrap();
    let r = c(&mut ss, "self_review", json!({}));
    assert_eq!(r["ok"], false);
    assert_eq!(r["summary"]["missing_media"], 1);
    assert_eq!(r["missing_media"][0]["status"], "missing");
}

#[test] fn no_path_means_no_contact_sheet_rendered() {
    let mut ss = scene_with_a_gap();
    let r = c(&mut ss, "self_review", json!({}));
    assert_eq!(r["contact_sheet"]["rendered"], false);
}

#[test] fn path_given_renders_a_contact_sheet_of_flagged_moments() {
    let mut ss = scene_with_a_gap();
    let out = format!("/tmp/kimo-selfreview-sheet-{}.png", std::process::id());
    let r = c(&mut ss, "self_review", json!({"path": out.clone()}));
    assert_eq!(r["contact_sheet"]["rendered"], true);
    assert!(std::path::Path::new(&out).is_file());
    std::fs::remove_file(&out).ok();
}

#[test] fn path_given_but_nothing_flagged_skips_rendering() {
    let (mut ss, path) = clean_media_scene("skip-render");
    let out = format!("/tmp/kimo-selfreview-nosheet-{}.png", std::process::id());
    let r = c(&mut ss, "self_review", json!({"path": out.clone()}));
    assert_eq!(r["contact_sheet"]["rendered"], false, "{r}");
    assert!(!std::path::Path::new(&out).is_file());
    std::fs::remove_file(&path).ok();
}

#[test] fn is_read_only_on_the_project() {
    let mut ss = scene_with_a_gap();
    let h = serde_json::to_string(&ss.project).unwrap(); let u = ss.undo.len();
    c(&mut ss, "self_review", json!({}));
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), h); assert_eq!(ss.undo.len(), u);
}



