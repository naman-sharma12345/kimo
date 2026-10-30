//! v7.3: apply_review_fixes -- batches lint_edit's machine-actionable fixes into one call, with dry_run.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_apply_review_fixes_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ffmpeg(args: &[&str]) { let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stdin(Stdio::null()).status().expect("ffmpeg"); assert!(st.success(), "ffmpeg {args:?}"); }
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }
fn broll(name: &str, dur: f64, grade: &str) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    let vf = if grade.is_empty() { "null".to_string() } else { grade.to_string() };
    ffmpeg(&["-f", "lavfi", "-i", &format!("testsrc2=s=360x640:r=30:d={dur}"), "-f", "lavfi", "-i", &format!("anoisesrc=a=0.004:d={dur}:r=48000"), "-vf", &vf,
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
fn portrait() -> Session { let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 360, "height": 640, "fps": 30})); ss }

/// A dissolve on a cut whose clips only touch dips to black (director_small.rs's own fixture shape);
/// lint_edit flags it as transition_from_black with a concrete fix: remove_transition. That fix has no
/// placeholder in it, so apply_review_fixes must actually remove the transition, not just report it.
#[test] fn applies_a_concrete_fix_and_the_transition_is_actually_gone() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": broll("arf1.mp4", 2.0, "")})), "asset_id");
    let b = sid(&call(&mut ss, "import_media", json!({"path": broll("arf2.mp4", 2.0, "hue=h=90")})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 2, "fit": "cover"}));
    let c2 = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": b, "start": 2, "duration": 2, "fit": "cover"})), "clip_id");
    call(&mut ss, "add_transition", json!({"clip_id": c2, "kind": "crossfade", "duration": 0.5}));
    assert!(ss.project.find_clip(&c2).unwrap().transition_in.is_some(), "fixture should start with a transition");

    let r = call(&mut ss, "apply_review_fixes", json!({}));
    assert_eq!(r["ok"], true, "{r}");
    let hit = r["applied"].as_array().unwrap().iter().find(|e| e["code"] == json!("transition_from_black"));
    assert!(hit.is_some(), "transition_from_black should have been applied: {r}");
    assert_eq!(hit.unwrap()["tool"], json!("remove_transition"), "{r}");
    assert!(ss.project.find_clip(&c2).unwrap().transition_in.is_none(), "the transition should actually be gone after apply_review_fixes: {r}");
}

/// dry_run must report the exact call it would make and change nothing.
#[test] fn dry_run_reports_without_mutating() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": broll("arf3.mp4", 2.0, "")})), "asset_id");
    let b = sid(&call(&mut ss, "import_media", json!({"path": broll("arf4.mp4", 2.0, "hue=h=90")})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 2, "fit": "cover"}));
    let c2 = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": b, "start": 2, "duration": 2, "fit": "cover"})), "clip_id");
    call(&mut ss, "add_transition", json!({"clip_id": c2, "kind": "crossfade", "duration": 0.5}));

    let r = call(&mut ss, "apply_review_fixes", json!({"dry_run": true}));
    assert_eq!(r["dry_run"], true);
    let hit = r["applied"].as_array().unwrap().iter().find(|e| e["code"] == json!("transition_from_black"));
    assert!(hit.is_some(), "{r}");
    assert!(hit.unwrap().get("would_call").is_some(), "dry_run entries must carry would_call, not apply: {r}");
    assert!(ss.project.find_clip(&c2).unwrap().transition_in.is_some(), "dry_run must not mutate the project");
}

/// A fix with a placeholder value (missing_glyph's fix asks for "<the text without those characters>")
/// must be reported under skipped_manual, never dispatched with the literal placeholder string.
#[test] fn a_placeholder_fix_is_skipped_as_manual_not_guessed_at() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let c = sid(&call(&mut ss, "add_text", json!({"track_id": t, "text": "look \u{1F525} here", "start": 0, "duration": 1, "size": 80})), "clip_id");
    let before = call(&mut ss, "get_project", json!({}));
    let _ = before;
    let text_before = ss.project.find_clip(&c).unwrap().clone();
    let r = call(&mut ss, "apply_review_fixes", json!({}));
    let manual = r["skipped_manual"].as_array().unwrap().iter().find(|e| e["code"] == json!("missing_glyph"));
    assert!(manual.is_some(), "missing_glyph should be skipped as manual (its fix is a placeholder): {r}");
    assert!(manual.unwrap()["reason"].as_str().unwrap().contains("human"), "{r}");
    assert!(r["applied"].as_array().unwrap().iter().all(|e| e["code"] != json!("missing_glyph")), "must not be in applied: {r}");
    // the clip's text must be untouched -- no placeholder string was ever dispatched to set_text
    assert_eq!(ss.project.find_clip(&c).unwrap().name, text_before.name);
}

/// `codes` scopes which problems are acted on; everything else is left alone (not even listed as skipped).
#[test] fn codes_filter_scopes_which_problems_are_touched() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": broll("arf5.mp4", 2.0, "")})), "asset_id");
    let b = sid(&call(&mut ss, "import_media", json!({"path": broll("arf6.mp4", 2.0, "hue=h=90")})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 2, "fit": "cover"}));
    let c2 = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": b, "start": 2, "duration": 2, "fit": "cover"})), "clip_id");
    call(&mut ss, "add_transition", json!({"clip_id": c2, "kind": "crossfade", "duration": 0.5}));

    let r = call(&mut ss, "apply_review_fixes", json!({"codes": ["font_substituted"]}));
    assert!(r["applied"].as_array().unwrap().iter().all(|e| e["code"] != json!("transition_from_black")), "scoped out of codes, must not be touched: {r}");
    assert!(r["skipped_manual"].as_array().unwrap().iter().all(|e| e["code"] != json!("transition_from_black")), "scoped-out problems aren't even listed as skipped: {r}");
    assert!(ss.project.find_clip(&c2).unwrap().transition_in.is_some(), "untouched because not in codes");
}

/// An empty/clean timeline: lint_edit erroring (e.g. nothing to check) must not be an apply_review_fixes failure.
#[test] fn an_empty_timeline_is_ok_not_an_error() {
    let mut ss = portrait();
    let r = call(&mut ss, "apply_review_fixes", json!({}));
    assert_eq!(r["ok"], true, "{r}");
    assert_eq!(r["problems_seen"], json!(0), "{r}");
}
