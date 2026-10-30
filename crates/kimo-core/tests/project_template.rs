//! v7.4: project_template -- starter project templates (canvas + fps + named track layout), distinct from
//! template_define/apply (clip/op templates, a completely different pre-existing thing).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }

#[test] fn list_project_templates_names_every_template_with_canvas_fps_and_tracks() {
    let mut ss = Session::new();
    let r = c(&mut ss, "list_project_templates", json!({}));
    let templates = r["templates"].as_array().unwrap();
    assert!(templates.len() >= 4, "{r}");
    let vs = templates.iter().find(|t| t["name"] == "vertical_social").expect("vertical_social should be a template");
    assert_eq!(vs["width"], 1080); assert_eq!(vs["height"], 1920);
    assert!(vs["tracks"].as_array().unwrap().len() >= 2);
    assert!(!vs["about"].as_str().unwrap().is_empty());
}

#[test] fn create_from_template_sets_canvas_fps_and_creates_named_tracks_in_order() {
    let mut ss = Session::new();
    let r = c(&mut ss, "create_project_from_template", json!({"template": "podcast_interview"}));
    assert_eq!(r["width"], 1920); assert_eq!(r["height"], 1080); assert_eq!(r["fps"], 25.0, "{r}");
    let p = c(&mut ss, "get_project", json!({}));
    let names: Vec<String> = p["tracks"].as_array().unwrap().iter().map(|t| t["name"].as_str().unwrap().to_string()).collect();
    assert_eq!(names, vec!["Host", "Guest", "Screen Share", "Music"], "{names:?}");
    for t in p["tracks"].as_array().unwrap() { assert_eq!(t["clips"].as_array().unwrap().len(), 0); }
}

#[test] fn refuses_to_overwrite_an_existing_project_without_the_flag() {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 640, "height": 480, "fps": 24}));
    c(&mut ss, "add_track", json!({}));
    let e = dispatch(&mut ss, "create_project_from_template", &json!({"template": "vertical_social"})).unwrap_err();
    assert!(e.contains("overwrite"), "{e}");
    // nothing should have changed
    let p = c(&mut ss, "get_project", json!({}));
    assert_eq!(p["width"], 640);
}

#[test] fn overwrite_true_replaces_the_existing_layout() {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 640, "height": 480, "fps": 24}));
    let old_track = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_shape", json!({"track_id": old_track, "shape": "rect", "width": 5, "height": 5, "fill": [1,1,1,1], "start": 0.0, "duration": 1.0}));
    let r = c(&mut ss, "create_project_from_template", json!({"template": "square_social", "overwrite": true}));
    assert_eq!(r["width"], 1080); assert_eq!(r["height"], 1080);
    let p = c(&mut ss, "get_project", json!({}));
    assert!(!p["tracks"].as_array().unwrap().iter().any(|t| t["id"] == old_track), "old track should be gone: {p}");
}

#[test] fn unknown_template_is_a_clean_error_naming_valid_ones() {
    let mut ss = Session::new();
    let e = dispatch(&mut ss, "create_project_from_template", &json!({"template": "cinematic_epic"})).unwrap_err();
    assert!(e.contains("no template 'cinematic_epic'"), "{e}"); assert!(e.contains("vertical_social"), "{e}");
}

#[test] fn result_is_one_undo_step() {
    let mut ss = Session::new();
    let before = ss.undo.len();
    c(&mut ss, "create_project_from_template", json!({"template": "widescreen_16_9"}));
    assert_eq!(ss.undo.len(), before + 1);
    c(&mut ss, "undo", json!({}));
    let p = c(&mut ss, "get_project", json!({}));
    assert_eq!(p["tracks"].as_array().unwrap().len(), 0, "undo should revert the whole template application");
}

#[test] fn every_template_is_internally_consistent() {
    let mut ss = Session::new();
    let r = c(&mut ss, "list_project_templates", json!({}));
    for t in r["templates"].as_array().unwrap() {
        let name = t["name"].as_str().unwrap();
        let mut fresh = Session::new();
        let created = c(&mut fresh, "create_project_from_template", json!({"template": name}));
        assert_eq!(created["width"], t["width"], "{name}"); assert_eq!(created["height"], t["height"], "{name}");
        let tracks = created["tracks"].as_array().unwrap();
        assert_eq!(tracks.len(), t["tracks"].as_array().unwrap().len(), "{name}");
    }
}

