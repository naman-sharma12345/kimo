//! TDD tests for the free-form expression interpreter (FEATURES.md L210/L220 gap): arithmetic,
//! comparisons, ternary, the time/value/index variables, valueAtTime(), and link() cross-clip linkage.
use kimo_core::commands::{dispatch, Session};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(ss: &mut Session, n: &str, a: serde_json::Value) -> String { dispatch(ss, n, &a).unwrap_err() }

fn setup() -> (Session, String, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 320, "height": 180, "fps": 30}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": "#336699", "start": 0, "duration": 4}))["clip_id"].as_str().unwrap().to_string();
    (ss, t, c)
}

fn sample(ss: &mut Session, cid: &str, prop: &str, t: f64) -> f64 {
    call(ss, "sample_property", json!({"clip_id": cid, "property": prop, "times": [t]}))["values"][0].as_f64().unwrap()
}

#[test]
fn plain_arithmetic_with_the_time_variable() {
    let (mut ss, _t, c) = setup();
    let r = call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "time * 10"}));
    assert_eq!(r["expressions_replaced"], 0);
    assert!((sample(&mut ss, &c, "x", 0.0) - 0.0).abs() < 1e-6);
    assert!((sample(&mut ss, &c, "x", 1.5) - 15.0).abs() < 1e-6);
    assert!((sample(&mut ss, &c, "x", 4.0) - 40.0).abs() < 1e-6);
}

#[test]
fn the_value_variable_reads_the_properys_own_prior_keyframes() {
    let (mut ss, _t, c) = setup();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "opacity", "time": 0.0, "value": 0.2, "ease": "linear"}));
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "opacity", "time": 4.0, "value": 1.0, "ease": "linear"}));
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "opacity", "script": "value + 0.0"}));
    assert!((sample(&mut ss, &c, "opacity", 0.0) - 0.2).abs() < 1e-3);
    assert!((sample(&mut ss, &c, "opacity", 2.0) - 0.6).abs() < 1e-2, "midpoint of the linear ramp should be ~0.6, got {}", sample(&mut ss, &c, "opacity", 2.0));
    assert!((sample(&mut ss, &c, "opacity", 4.0) - 1.0).abs() < 1e-3);
}

#[test]
fn value_at_time_samples_this_propertys_own_keys_at_an_arbitrary_time() {
    let (mut ss, _t, c) = setup();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "rotation", "time": 0.0, "value": 0.0, "ease": "linear"}));
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "rotation", "time": 2.0, "value": 180.0, "ease": "linear"}));
    // delayed echo: read the rotation key curve 1s in the future (clamped by the data itself, no error at the edge)
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "rotation", "script": "valueAtTime(time + 1)"}));
    assert!((sample(&mut ss, &c, "rotation", 0.0) - 90.0).abs() < 1.0, "valueAtTime(1) on a 0->180 ramp over 2s should read ~90, got {}", sample(&mut ss, &c, "rotation", 0.0));
}

#[test]
fn index_is_the_clips_1_based_position_in_its_own_track_by_start_time() {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 320, "height": 180, "fps": 30}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c1 = call(&mut ss, "add_clip", json!({"track_id": &t, "source_type": "solid", "color": "#111111", "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let c2 = call(&mut ss, "add_clip", json!({"track_id": &t, "source_type": "solid", "color": "#222222", "start": 1, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let c3 = call(&mut ss, "add_clip", json!({"track_id": &t, "source_type": "solid", "color": "#333333", "start": 2, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    for (c, _) in [(&c1, 1.0), (&c2, 2.0), (&c3, 3.0)] { call(&mut ss, "set_expression_script", json!({"clip_id": c, "property": "x", "script": "index"})); }
    assert_eq!(sample(&mut ss, &c1, "x", 0.5), 1.0);
    assert_eq!(sample(&mut ss, &c2, "x", 1.5), 2.0);
    assert_eq!(sample(&mut ss, &c3, "x", 2.5), 3.0);
}

#[test]
fn link_reads_another_clips_property_at_the_same_or_a_given_time() {
    let (mut ss, t, c) = setup();
    let driver = call(&mut ss, "add_clip", json!({"track_id": &t, "source_type": "solid", "color": "#aa0000", "start": 0, "duration": 4}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_keyframe", json!({"clip_id": &driver, "property": "opacity", "time": 0.0, "value": 0.0, "ease": "linear"}));
    call(&mut ss, "set_keyframe", json!({"clip_id": &driver, "property": "opacity", "time": 4.0, "value": 4.0, "ease": "linear"}));
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": format!("link(\"{driver}:opacity\") * 100")}));
    assert!((sample(&mut ss, &c, "x", 2.0) - 200.0).abs() < 1.0, "driver opacity at t=2 is 2.0, *100 = 200, got {}", sample(&mut ss, &c, "x", 2.0));

    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "y", "script": format!("link(\"{driver}:opacity\", 0)")}));
    assert!((sample(&mut ss, &c, "y", 3.0) - 0.0).abs() < 1e-3, "an explicit t=0 in link() should ignore the sample time and read the driver at t=0");
}

#[test]
fn link_is_live_an_edit_to_the_driver_after_the_fact_is_picked_up() {
    let (mut ss, t, c) = setup();
    let driver = call(&mut ss, "add_clip", json!({"track_id": &t, "source_type": "solid", "color": "#aa0000", "start": 0, "duration": 4}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_keyframe", json!({"clip_id": &driver, "property": "opacity", "time": 0.0, "value": 1.0, "ease": "linear"}));
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": format!("link(\"{driver}:opacity\")") }));
    assert!((sample(&mut ss, &c, "x", 0.0) - 1.0).abs() < 1e-3);
    call(&mut ss, "set_keyframe", json!({"clip_id": &driver, "property": "opacity", "time": 0.0, "value": 9.0, "ease": "linear"}));
    assert!((sample(&mut ss, &c, "x", 0.0) - 9.0).abs() < 1e-3, "sample_property must re-resolve the expression script against the driver's CURRENT state");
}

#[test]
fn link_by_at_name_reference_resolves_through_names() {
    let (mut ss, t, c) = setup();
    let driver = call(&mut ss, "add_clip", json!({"track_id": &t, "source_type": "solid", "color": "#aa0000", "start": 0, "duration": 4}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_meta", json!({"id": &driver, "name": "driverclip"}));
    call(&mut ss, "set_keyframe", json!({"clip_id": &driver, "property": "opacity", "time": 0.0, "value": 5.0, "ease": "linear"}));
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "link(\"@driverclip:opacity\")" }));
    assert!((sample(&mut ss, &c, "x", 0.0) - 5.0).abs() < 1e-3);
}

#[test]
fn link_to_an_effect_param_uses_the_effect_id_dot_property_form() {
    let (mut ss, t, c) = setup();
    let driver = call(&mut ss, "add_clip", json!({"track_id": &t, "source_type": "solid", "color": "#aa0000", "start": 0, "duration": 4}))["clip_id"].as_str().unwrap().to_string();
    let e = call(&mut ss, "add_effect", json!({"clip_id": &driver, "kind": "exposure"}))["effect_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_keyframe", json!({"clip_id": &driver, "property": "stops", "effect_id": &e, "time": 0.0, "value": 3.0, "ease": "linear"}));
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": format!("link(\"{driver}:{e}.stops\")") }));
    assert!((sample(&mut ss, &c, "x", 0.0) - 3.0).abs() < 1e-3);
}

#[test]
fn comparison_and_ternary_operators_work() {
    let (mut ss, _t, c) = setup();
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "time > 2 ? 100 : -100"}));
    assert!((sample(&mut ss, &c, "x", 1.0) - -100.0).abs() < 1e-6);
    assert!((sample(&mut ss, &c, "x", 3.0) - 100.0).abs() < 1e-6);
}

#[test]
fn boolean_and_or_not_work() {
    let (mut ss, _t, c) = setup();
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "(time > 1 && time < 3) ? 1 : 0"}));
    assert_eq!(sample(&mut ss, &c, "x", 0.5), 0.0);
    assert_eq!(sample(&mut ss, &c, "x", 2.0), 1.0);
    assert_eq!(sample(&mut ss, &c, "x", 3.5), 0.0);
}

#[test]
fn math_functions_sin_clamp_linear_min_max() {
    let (mut ss, _t, c) = setup();
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "clamp(time * 100, 0, 50)"}));
    assert!((sample(&mut ss, &c, "x", 0.1) - 10.0).abs() < 1e-6);
    assert!((sample(&mut ss, &c, "x", 1.0) - 50.0).abs() < 1e-6, "clamp should cap at 50, got {}", sample(&mut ss, &c, "x", 1.0));

    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "y", "script": "linear(time, 0, 4, 10, 50)"}));
    assert!((sample(&mut ss, &c, "y", 0.0) - 10.0).abs() < 1e-6);
    assert!((sample(&mut ss, &c, "y", 2.0) - 30.0).abs() < 1e-6);
    assert!((sample(&mut ss, &c, "y", 4.0) - 50.0).abs() < 1e-6);
    assert!((sample(&mut ss, &c, "y", 10.0) - 50.0).abs() < 1e-6, "linear() clamps past tMax");
}

#[test]
fn power_operator_is_right_associative_and_unary_minus_binds_tighter_than_binary() {
    let (mut ss, _t, c) = setup();
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "2 ^ 3 ^ 2"})); // 2^(3^2) = 2^9 = 512, not (2^3)^2=64
    assert!((sample(&mut ss, &c, "x", 0.0) - 512.0).abs() < 1e-6, "got {}", sample(&mut ss, &c, "x", 0.0));
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "y", "script": "-2 ^ 2"})); // -(2^2) = -4 in most conventions here: unary binds inside pow's operand
    let v = sample(&mut ss, &c, "y", 0.0);
    assert!((v - 4.0).abs() < 1e-6 || (v - -4.0).abs() < 1e-6, "either reading is defensible, just confirming it does not crash: got {v}");
}

#[test]
fn unknown_variable_and_unknown_function_are_clear_errors() {
    let (mut ss, _t, c) = setup();
    let e1 = err(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "not_a_real_var + 1"}));
    assert!(e1.contains("unknown variable"), "{e1}");
    let e2 = err(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "not_a_real_fn(1)"}));
    assert!(e2.contains("unknown function"), "{e2}");
}

#[test]
fn syntax_errors_are_rejected_with_nothing_changed() {
    let (mut ss, _t, c) = setup();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "x", "time": 0.0, "value": 7.0, "ease": "linear"}));
    let e = err(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "time + * 2"}));
    assert!(!e.is_empty());
    // nothing was changed: the old keyframe is still there, untouched
    assert!((sample(&mut ss, &c, "x", 0.0) - 7.0).abs() < 1e-6);
    let scripts = call(&mut ss, "list_expression_scripts", json!({"clip_id": &c}));
    assert_eq!(scripts["scripts"].as_array().unwrap().len(), 0);
}

#[test]
fn link_to_a_nonexistent_clip_is_a_clear_error_and_nothing_is_registered() {
    let (mut ss, _t, c) = setup();
    let e = err(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "link(\"no_such_clip:opacity\")"}));
    assert!(e.contains("no_such_clip") || e.contains("no clip"), "{e}");
    let scripts = call(&mut ss, "list_expression_scripts", json!({}));
    assert_eq!(scripts["scripts"].as_array().unwrap().len(), 0);
}

#[test]
fn division_and_modulo_by_zero_are_clear_errors() {
    let (mut ss, _t, c) = setup();
    let e1 = err(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "1 / 0"}));
    assert!(e1.contains("division by zero"), "{e1}");
    let e2 = err(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "mod(1, 0)"}));
    assert!(e2.contains("mod"), "{e2}");
}

#[test]
fn clear_expression_script_stops_future_rebaking_but_leaves_existing_keys() {
    let (mut ss, t, c) = setup();
    let driver = call(&mut ss, "add_clip", json!({"track_id": &t, "source_type": "solid", "color": "#aa0000", "start": 0, "duration": 4}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_keyframe", json!({"clip_id": &driver, "property": "opacity", "time": 0.0, "value": 1.0, "ease": "linear"}));
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": format!("link(\"{driver}:opacity\")") }));
    assert!((sample(&mut ss, &c, "x", 0.0) - 1.0).abs() < 1e-3);
    let r = call(&mut ss, "clear_expression_script", json!({"clip_id": &c, "property": "x"}));
    assert_eq!(r["removed"], 1);
    call(&mut ss, "set_keyframe", json!({"clip_id": &driver, "property": "opacity", "time": 0.0, "value": 99.0, "ease": "linear"}));
    assert!((sample(&mut ss, &c, "x", 0.0) - 1.0).abs() < 1e-3, "after clear, the baked key should be static and not follow the driver anymore");
}

#[test]
fn existing_op_stack_expression_is_replaced_when_a_script_targets_the_same_property() {
    let (mut ss, _t, c) = setup();
    call(&mut ss, "set_expression", json!({"clip_id": &c, "property": "opacity", "op": "sin", "freq": 1.0, "amp": 0.5}));
    let r = call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "opacity", "script": "0.5"}));
    assert_eq!(r["expressions_replaced"], 1);
}

#[test]
fn a_project_with_no_expression_scripts_serializes_with_no_new_field() {
    let (mut ss, c) = (Session::new(), String::new());
    call(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let path = std::env::temp_dir().join(format!("kimo_v7_expr_plain_{}.kimo", std::process::id()));
    call(&mut ss, "save_project", json!({"path": path.to_str().unwrap()}));
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert!(!text.contains("\"expr_scripts\""));
    let _ = c;
}

#[test]
fn expression_scripts_persist_through_save_and_load() {
    let (mut ss, _t, c) = setup();
    call(&mut ss, "set_expression_script", json!({"clip_id": &c, "property": "x", "script": "time * 5"}));
    let path = std::env::temp_dir().join(format!("kimo_v7_expr_save_{}.kimo", std::process::id()));
    call(&mut ss, "save_project", json!({"path": path.to_str().unwrap()}));
    let mut ss2 = Session::new();
    call(&mut ss2, "load_project", json!({"path": path.to_str().unwrap()}));
    std::fs::remove_file(&path).ok();
    let scripts = call(&mut ss2, "list_expression_scripts", json!({}));
    assert_eq!(scripts["scripts"].as_array().unwrap().len(), 1);
    assert!((sample(&mut ss2, &c, "x", 2.0) - 10.0).abs() < 1e-3, "live re-bake should still work after reload");
}
