//! v7.2 additions: list_animated_properties -- discovery pass for what is keyframed on a clip, read-only.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }

fn scene() -> (Session, String) {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "start": 0.0, "duration": 10.0}))["clip_id"].as_str().unwrap().to_string();
    (ss, id)
}

#[test] fn empty_clip_has_nothing_animated() {
    let (mut ss, id) = scene();
    let h = pj(&ss); let u = ss.undo.len();
    let r = c(&mut ss, "list_animated_properties", json!({"clip_id": id}));
    assert_eq!(r["count"], 0);
    assert_eq!(r["animated"].as_array().unwrap().len(), 0);
    assert_eq!(pj(&ss), h, "read-only"); assert_eq!(ss.undo.len(), u, "no undo entry");
}

#[test] fn a_static_property_is_not_animated_a_keyframed_one_is() {
    let (mut ss, id) = scene();
    c(&mut ss, "set_property", json!({"clip_id": id, "property": "opacity", "value": 0.5})); // static, no keys
    c(&mut ss, "set_keyframe", json!({"clip_id": id, "property": "x", "time": 0.0, "value": 10.0}));
    c(&mut ss, "set_keyframe", json!({"clip_id": id, "property": "x", "time": 1.0, "value": 20.0}));
    let r = c(&mut ss, "list_animated_properties", json!({"clip_id": id}));
    assert_eq!(r["count"], 1, "only x has a key; opacity is static");
    let item = &r["animated"][0];
    assert_eq!(item["scope"], "clip");
    assert_eq!(item["property"], "x");
    assert_eq!(item["key_count"], 2);
}

#[test] fn effect_params_show_up_with_their_effect_id_and_kind() {
    let (mut ss, id) = scene();
    let fx = c(&mut ss, "add_effect", json!({"clip_id": id, "kind": "blur"}))["effect_id"].as_str().unwrap().to_string();
    c(&mut ss, "set_keyframe", json!({"clip_id": id, "effect_id": fx, "property": "radius", "time": 0.0, "value": 0.0}));
    let r = c(&mut ss, "list_animated_properties", json!({"clip_id": id}));
    assert_eq!(r["count"], 1);
    let item = &r["animated"][0];
    assert_eq!(item["scope"], "effect");
    assert_eq!(item["effect_id"], fx);
    assert_eq!(item["effect_kind"], "blur");
    assert_eq!(item["property"], "radius");
}

#[test] fn mixed_clip_and_effect_animation_both_listed() {
    let (mut ss, id) = scene();
    c(&mut ss, "set_keyframe", json!({"clip_id": id, "property": "scale", "time": 0.0, "value": 1.0}));
    let fx = c(&mut ss, "add_effect", json!({"clip_id": id, "kind": "blur"}))["effect_id"].as_str().unwrap().to_string();
    c(&mut ss, "set_keyframe", json!({"clip_id": id, "effect_id": fx, "property": "radius", "time": 0.0, "value": 5.0}));
    let r = c(&mut ss, "list_animated_properties", json!({"clip_id": id}));
    assert_eq!(r["count"], 2);
    let scopes: Vec<&str> = r["animated"].as_array().unwrap().iter().map(|i| i["scope"].as_str().unwrap()).collect();
    assert!(scopes.contains(&"clip")); assert!(scopes.contains(&"effect"));
}

#[test] fn unknown_clip_id_errors() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let m = dispatch(&mut ss, "list_animated_properties", &json!({"clip_id": "nope"})).unwrap_err();
    assert!(m.contains("nope"), "{m}");
}

