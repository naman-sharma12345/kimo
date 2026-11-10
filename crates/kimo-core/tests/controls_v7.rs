//! TDD tests for expression-control effects (FEATURES.md L473): slider/checkbox/angle/point/color/
//! dropdown controls -- identity effects whose own params are the value, fully keyframeable and
//! link()-able since they are ordinary effect params.
use kimo_core::commands::{dispatch, Session};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn setup() -> (Session, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": "#404040", "start": 0, "duration": 2}))["clip_id"].as_str().unwrap().to_string();
    (ss, c)
}

#[test]
fn every_control_kind_adds_seeds_and_round_trips_through_sample_property() {
    // Like every other effect in this project, a control's params start unset (add_effect does not
    // pre-populate a kind's catalog defaults into the clip -- it only stores what the caller gives it);
    // the documented default is what the catalog advertises as the neutral value, seeded explicitly here.
    let (mut ss, c) = setup();
    let expected: &[(&str, &[(&str, f64)])] = &[
        ("slider_control", &[("value", 7.0)]), ("checkbox_control", &[("value", 1.0)]), ("angle_control", &[("value", 45.0)]),
        ("point_control", &[("x", 3.0), ("y", 4.0)]), ("color_control", &[("r", 0.5), ("g", 0.5), ("b", 0.5), ("a", 1.0)]), ("dropdown_control", &[("index", 2.0)]),
    ];
    for (kind, params) in expected {
        let p_obj: serde_json::Map<String, serde_json::Value> = params.iter().map(|(k, v)| (k.to_string(), json!(v))).collect();
        let e = call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": kind, "params": serde_json::Value::Object(p_obj)}))["effect_id"].as_str().unwrap().to_string();
        for (p, want) in *params {
            let v = call(&mut ss, "sample_property", json!({"clip_id": &c, "property": p, "effect_id": &e, "times": [0.0]}))["values"][0].as_f64().unwrap();
            assert!((v - want).abs() < 1e-9, "{kind}.{p}: expected {want}, got {v}");
        }
        call(&mut ss, "remove_effect", json!({"clip_id": &c, "effect_id": e}));
    }
    // every clip's own frame still renders fine after all that add/remove churn
    let path = std::env::temp_dir().join(format!("kimo_v7_controls_{}.png", std::process::id()));
    call(&mut ss, "render_frame", json!({"time": 0.5, "path": path.to_str().unwrap()}));
    assert!(path.exists());
    std::fs::remove_file(&path).ok();
}

#[test]
fn slider_control_value_is_keyframeable_via_set_keyframe_with_no_prior_seed() {
    // set_keyframe (unlike sample_property) creates the Prop on first use, so a control never needs an
    // add_effect params seed if the caller is going straight to keyframing it.
    let (mut ss, c) = setup();
    let e = call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "slider_control"}))["effect_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "value", "effect_id": &e, "time": 0.0, "value": 10.0, "ease": "linear"}));
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "value", "effect_id": &e, "time": 2.0, "value": 20.0, "ease": "linear"}));
    let v1 = call(&mut ss, "sample_property", json!({"clip_id": &c, "property": "value", "effect_id": &e, "times": [1.0]}))["values"][0].as_f64().unwrap();
    assert!((v1 - 15.0).abs() < 1e-6);
}

#[test]
fn point_control_has_independent_x_and_y() {
    let (mut ss, c) = setup();
    let e = call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "point_control"}))["effect_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "x", "effect_id": &e, "time": 0.0, "value": 5.0, "ease": "linear"}));
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "y", "effect_id": &e, "time": 0.0, "value": -3.0, "ease": "linear"}));
    let x = call(&mut ss, "sample_property", json!({"clip_id": &c, "property": "x", "effect_id": &e, "times": [0.0]}))["values"][0].as_f64().unwrap();
    let y = call(&mut ss, "sample_property", json!({"clip_id": &c, "property": "y", "effect_id": &e, "times": [0.0]}))["values"][0].as_f64().unwrap();
    assert_eq!(x, 5.0); assert_eq!(y, -3.0);
}

#[test]
fn color_control_seeded_as_opaque_white_round_trips_every_channel() {
    let (mut ss, c) = setup();
    let e = call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "color_control", "params": {"r": 1.0, "g": 1.0, "b": 1.0, "a": 1.0}}))["effect_id"].as_str().unwrap().to_string();
    for chan in ["r", "g", "b", "a"] {
        let v = call(&mut ss, "sample_property", json!({"clip_id": &c, "property": chan, "effect_id": &e, "times": [0.0]}))["values"][0].as_f64().unwrap();
        assert_eq!(v, 1.0, "{chan} should read back the seeded 1.0 (opaque white)");
    }
}

#[test]
fn another_propertys_expression_script_can_link_to_a_control_value() {
    let (mut ss, c) = setup();
    let e = call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "slider_control"}))["effect_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "value", "effect_id": &e, "time": 0.0, "value": 42.0, "ease": "linear"}));
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": format!("link(\"{c}:{e}.value\") * 2")}));
    let x = call(&mut ss, "sample_property", json!({"clip_id": &c, "property": "x", "times": [0.0]}))["values"][0].as_f64().unwrap();
    assert!((x - 84.0).abs() < 1e-3, "x should read the slider_control's value (42) doubled, got {x}");
}

#[test]
fn checkbox_control_can_drive_a_ternary_expression() {
    let (mut ss, c) = setup();
    let e = call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "checkbox_control"}))["effect_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "value", "effect_id": &e, "time": 0.0, "value": 1.0, "ease": "linear"}));
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "opacity", "script": format!("link(\"{c}:{e}.value\") > 0 ? 1 : 0")}));
    let v = call(&mut ss, "sample_property", json!({"clip_id": &c, "property": "opacity", "times": [0.0]}))["values"][0].as_f64().unwrap();
    assert_eq!(v, 1.0);
}

#[test]
fn controls_appear_in_the_effect_registry() {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let list = call(&mut ss, "list_effects", json!({}));
    let arr = list.as_array().cloned().unwrap_or_else(|| list["effects"].as_array().cloned().unwrap());
    let names: Vec<String> = arr.iter().filter_map(|v| v.get("kind").and_then(|k| k.as_str()).map(String::from)).collect();
    for k in ["slider_control", "checkbox_control", "angle_control", "point_control", "color_control", "dropdown_control"] {
        assert!(names.iter().any(|n| n == k), "{k} missing from the registry");
    }
}
