//! v7 MOGRT substitute (FEATURES.md L562-neighbourhood, "MOGRT substitute" line): export_template_package /
//! import_template_package bundle a template_define'd template with set_essential_properties marks into one
//! JSON file, Kimo's own open substitute for Adobe's closed .mogrt format.
use kimo_core::commands::{dispatch, Session};
use serde_json::json;

static TEST_SEQ: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
fn unique_dir(tag: &str) -> std::path::PathBuf {
    // cargo runs tests in this binary on parallel threads of ONE process, so process::id() alone collides;
    // an atomic counter plus the tag makes every call's directory unique regardless of test order.
    let n = TEST_SEQ.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("kimo_mogrt_{tag}_{}_{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}
fn setup_with_template(tag: &str) -> (Session, String, std::path::PathBuf) {
    let mut ss = Session::new();
    dispatch(&mut ss, "project_set", &json!({"width": 320, "height": 180, "fps": 25})).unwrap();
    let tr = dispatch(&mut ss, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let clip_id = dispatch(&mut ss, "add_shape", &json!({"track_id": tr, "shape": "rect", "start": 0.0, "duration": 2.0, "color": [1.0,0.0,0.0]})).unwrap()["clip_id"].as_str().unwrap().to_string();
    dispatch(&mut ss, "set_meta", &json!({"id": clip_id, "role": "headline_bg"})).unwrap();
    dispatch(&mut ss, "set_essential_properties", &json!({"clip_id": clip_id, "properties": [{"property": "opacity", "label": "Background opacity"}, {"property": "x", "label": "X position"}]})).unwrap();
    dispatch(&mut ss, "template_define", &json!({"name": "lower_third", "params": [{"name": "title", "type": "string", "default": "Title"}], "ops": [{"tool": "add_track", "args": {}}]})).unwrap();
    let dir = unique_dir(tag);
    (ss, clip_id, dir)
}

#[test]
fn export_bundles_template_and_essential_marks_into_one_json_file() {
    let (mut ss, clip_id, dir) = setup_with_template("export_bundle");
    let path = dir.join("lower_third.ktpl.json");
    let r = dispatch(&mut ss, "export_template_package", &json!({"name": "lower_third", "path": path.to_str().unwrap()})).unwrap();
    assert!(path.exists(), "export should write a file");
    assert_eq!(r["template_name"], "lower_third");
    assert_eq!(r["essential_clip_count"], 1);
    let txt = std::fs::read_to_string(&path).unwrap();
    let v: serde_json::Value = serde_json::from_str(&txt).unwrap();
    assert_eq!(v["kimo_template_package_version"], 1);
    assert_eq!(v["template"]["name"], "lower_third");
    let essential = v["essential_properties"].as_array().unwrap();
    assert_eq!(essential.len(), 1);
    assert_eq!(essential[0]["clip_id"], clip_id);
    assert_eq!(essential[0]["role"], "headline_bg");
    let tags: Vec<&str> = essential[0]["essential_tags"].as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    assert!(tags.iter().any(|t| t.starts_with("essential:opacity")));
    assert!(tags.iter().any(|t| t.starts_with("essential:x")));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn export_unknown_template_errors_with_a_useful_message() {
    let (mut ss, _clip_id, dir) = setup_with_template("export_unknown");
    let path = dir.join("x.json");
    let err = dispatch(&mut ss, "export_template_package", &json!({"name": "not_a_real_template", "path": path.to_str().unwrap()})).unwrap_err();
    assert!(err.contains("lower_third"), "the error should name what templates DO exist: {err}");
    assert!(!path.exists(), "a failed export should not write a partial file");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn round_trip_into_a_fresh_project_reapplies_essential_marks_by_role() {
    let (mut ss, _clip_id, dir) = setup_with_template("roundtrip_fresh");
    let path = dir.join("lower_third.ktpl.json");
    dispatch(&mut ss, "export_template_package", &json!({"name": "lower_third", "path": path.to_str().unwrap()})).unwrap();

    // A brand new project/session -- as if the package were handed to someone else.
    let mut fresh = Session::new();
    dispatch(&mut fresh, "project_set", &json!({"width": 320, "height": 180, "fps": 25})).unwrap();
    assert!(dispatch(&mut fresh, "template_apply", &json!({"name": "lower_third", "args": {}})).is_err(), "sanity: the template must not exist yet");
    let r = dispatch(&mut fresh, "import_template_package", &json!({"path": path.to_str().unwrap()})).unwrap();
    assert_eq!(r["template_name"], "lower_third");
    // Now the template is usable.
    dispatch(&mut fresh, "template_apply", &json!({"name": "lower_third", "args": {"title": "Breaking News"}})).unwrap();
    // No clip with role headline_bg exists in the fresh project yet (the template's ops were empty in this fixture),
    // so the essential marks should come back as unmatched rather than silently vanish.
    assert_eq!(r["essential_reapplied"], 0);
    let unmatched = r["essential_unmatched"].as_array().unwrap();
    assert_eq!(unmatched.len(), 1);
    assert_eq!(unmatched[0]["role"], "headline_bg");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn round_trip_into_the_same_project_reapplies_by_role_to_a_new_clip() {
    let (mut ss, orig_clip_id, dir) = setup_with_template("roundtrip_same");
    let path = dir.join("lower_third.ktpl.json");
    dispatch(&mut ss, "export_template_package", &json!({"name": "lower_third", "path": path.to_str().unwrap()})).unwrap();
    // Simulate the original role-tagged layer being gone (deleted/replaced) so role matching is unambiguous:
    // clear its role, leaving exactly one "headline_bg"-rolled clip (the new one below) in the project.
    dispatch(&mut ss, "set_meta", &json!({"id": orig_clip_id, "role": serde_json::Value::Null})).unwrap();
    // Add a second clip in the SAME project carrying the same role (simulating a fresh template_apply
    // instance that recreated the role-tagged layer) but with no essential marks of its own yet.
    let tr2 = dispatch(&mut ss, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let new_clip = dispatch(&mut ss, "add_shape", &json!({"track_id": tr2, "shape": "rect", "start": 0.0, "duration": 2.0})).unwrap()["clip_id"].as_str().unwrap().to_string();
    dispatch(&mut ss, "set_meta", &json!({"id": new_clip, "role": "headline_bg"})).unwrap();
    assert_ne!(new_clip, orig_clip_id);
    let r = dispatch(&mut ss, "import_template_package", &json!({"path": path.to_str().unwrap()})).unwrap();
    assert_eq!(r["essential_reapplied"], 1, "should match the new clip by role: {r}");
    let listed = dispatch(&mut ss, "list_essential_properties", &json!({"clip_id": new_clip})).unwrap();
    assert_eq!(listed["essential_properties"].as_array().unwrap().len(), 2);
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn import_rejects_a_file_that_is_not_a_kimo_template_package() {
    let mut ss = Session::new();
    dispatch(&mut ss, "project_set", &json!({"width": 320, "height": 180, "fps": 25})).unwrap();
    let dir = std::env::temp_dir().join(format!("kimo_mogrt_bad_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("not_a_package.json");
    std::fs::write(&path, r#"{"hello": "world"}"#).unwrap();
    let err = dispatch(&mut ss, "import_template_package", &json!({"path": path.to_str().unwrap()})).unwrap_err();
    assert!(err.contains("kimo_template_package"), "{err}");
    std::fs::remove_dir_all(&dir).ok();
}
