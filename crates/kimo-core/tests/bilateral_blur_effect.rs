//! `bilateral_blur` registered as its own MCP-callable effect kind (FEATURES.md L131), not only living inside
//! `denoise`. TDD via the public add_effect/render_frame surface.
use kimo_core::commands::{dispatch, registry_ref, Session};
use kimo_core::render;
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value {
    dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}"))
}

#[test]
fn bilateral_blur_is_a_listed_effect_and_mcp_registered_tool() {
    let kinds: Vec<String> = {
        let mut s = Session::new();
        c(&mut s, "list_effects", json!({}))
            .as_array().unwrap().iter().map(|e| e["kind"].as_str().unwrap().to_string()).collect()
    };
    assert!(kinds.contains(&"bilateral_blur".to_string()), "bilateral_blur missing from list_effects: {kinds:?}");
    assert!(registry_ref().iter().any(|cmd| cmd.name == "add_effect"), "add_effect tool should exist (carries bilateral_blur as a kind)");
}

#[test]
fn add_effect_bilateral_blur_renders_and_amount_zero_matches_no_effect() {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 64, "height": 48, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let clip = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "width": 60, "height": 44, "fill": [0.8, 0.3, 0.2, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let cache = render::Cache::new();
    let before = render::render_frame(&s.project, 0.5, &cache).unwrap();

    let eid = c(&mut s, "add_effect", json!({"clip_id": clip, "kind": "bilateral_blur"}))["effect_id"].as_str().unwrap().to_string();
    c(&mut s, "set_effect_param", json!({"clip_id": clip, "effect_id": eid, "param": "amount", "value": 0.0}));
    let cache2 = render::Cache::new();
    let zero_amount = render::render_frame(&s.project, 0.5, &cache2).unwrap();
    assert_eq!(before.d, zero_amount.d, "amount=0 must be a bit-exact no-op, same as denoise's contract");

    c(&mut s, "set_effect_param", json!({"clip_id": clip, "effect_id": eid, "param": "amount", "value": 1.0}));
    c(&mut s, "set_effect_param", json!({"clip_id": clip, "effect_id": eid, "param": "radius", "value": 6.0}));
    c(&mut s, "set_effect_param", json!({"clip_id": clip, "effect_id": eid, "param": "smoothness", "value": 0.7}));
    let cache3 = render::Cache::new();
    let blurred = render::render_frame(&s.project, 0.5, &cache3).unwrap();
    assert_ne!(before.d, blurred.d, "a nonzero amount should actually change pixels");
    assert!(blurred.d.iter().all(|v| v.is_finite()));
}

#[test]
fn unknown_param_is_rejected_same_as_any_other_effect() {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 32, "height": 32, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let clip = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "fill": [1.0, 1.0, 1.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let err = dispatch(&mut s, "add_effect", &json!({"clip_id": clip, "kind": "bilateral_blur", "params": {"not_a_real_param": 1.0}})).unwrap_err();
    assert!(err.contains("unknown param"), "expected an unknown-param error, got: {err}");
}
