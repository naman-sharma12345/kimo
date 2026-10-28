//! TDD tests for FEATURES.md L239: data-driven animation (JSON/CSV), essential properties,
//! responsive time, and scripting hooks. (Presets is covered by the pre-existing apply_motion_preset,
//! not under test here.)
use kimo_core::commands::{dispatch, Session};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(ss: &mut Session, n: &str, a: serde_json::Value) -> String { dispatch(ss, n, &a).unwrap_err() }

fn setup() -> (Session, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 320, "height": 180, "fps": 30}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": "#336699", "start": 0, "duration": 10}))["clip_id"].as_str().unwrap().to_string();
    (ss, c)
}

// ---------------- animate_from_data ----------------

#[test]
fn json_rows_with_explicit_time_become_exact_keyframes() {
    let (mut ss, c) = setup();
    let r = call(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "opacity",
        "data": [{"t": 0.0, "v": 0.2}, {"t": 1.0, "v": 0.8}, {"t": 2.5, "v": 0.1}]}));
    assert_eq!(r["keys"], 3);
    let keys = &ss.project.find_clip(&c).unwrap().props["opacity"].keys;
    assert_eq!(keys.len(), 3);
    assert!((keys[0].t - 0.0).abs() < 1e-9 && (keys[0].v - 0.2).abs() < 1e-9);
    assert!((keys[1].t - 1.0).abs() < 1e-9 && (keys[1].v - 0.8).abs() < 1e-9);
    assert!((keys[2].t - 2.5).abs() < 1e-9 && (keys[2].v - 0.1).abs() < 1e-9);
}

#[test]
fn rows_without_a_time_field_are_spaced_evenly_across_the_span() {
    let (mut ss, c) = setup();
    call(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "x",
        "data": [{"v": 0.0}, {"v": 50.0}, {"v": 100.0}], "start": 0.0, "duration": 4.0}));
    let keys = &ss.project.find_clip(&c).unwrap().props["x"].keys;
    assert_eq!(keys.len(), 3);
    assert!((keys[0].t - 0.0).abs() < 1e-9);
    assert!((keys[1].t - 2.0).abs() < 1e-9, "middle row should land halfway: {}", keys[1].t);
    assert!((keys[2].t - 4.0).abs() < 1e-9);
}

#[test]
fn csv_text_is_parsed_with_a_header_row() {
    let (mut ss, c) = setup();
    let csv = "t,v\n0,10\n1,20\n2,30\n";
    let r = call(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "y", "csv": csv, "time_field": "t", "value_field": "v"}));
    assert_eq!(r["keys"], 3);
    let keys = &ss.project.find_clip(&c).unwrap().props["y"].keys;
    assert_eq!(keys.iter().map(|k| k.v).collect::<Vec<_>>(), vec![10.0, 20.0, 30.0]);
}

#[test]
fn csv_quoted_fields_with_embedded_commas_parse_correctly() {
    let (mut ss, c) = setup();
    let csv = "label,t,v\n\"hello, world\",0,5\nplain,1,15\n";
    let r = call(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "rotation", "csv": csv, "time_field": "t", "value_field": "v"}));
    assert_eq!(r["keys"], 2);
    let keys = &ss.project.find_clip(&c).unwrap().props["rotation"].keys;
    assert_eq!(keys.iter().map(|k| k.v).collect::<Vec<_>>(), vec![5.0, 15.0]);
}

#[test]
fn csv_path_reads_a_real_file_on_disk() {
    let (mut ss, c) = setup();
    let path = std::env::temp_dir().join(format!("kimo_v7_data_anim_{}.csv", std::process::id()));
    std::fs::write(&path, "frame,val\n0,1\n1,2\n2,3\n3,4\n").unwrap();
    let r = call(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "scale", "csv_path": path.to_str().unwrap(), "time_field": "frame", "value_field": "val"}));
    std::fs::remove_file(&path).ok();
    assert_eq!(r["keys"], 4);
}

#[test]
fn scale_and_offset_apply_a_linear_transform_to_every_value() {
    let (mut ss, c) = setup();
    call(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "opacity",
        "data": [{"t": 0.0, "v": 0.0}, {"t": 1.0, "v": 100.0}], "scale": 0.01, "offset": 0.0}));
    let keys = &ss.project.find_clip(&c).unwrap().props["opacity"].keys;
    assert!((keys[1].v - 1.0).abs() < 1e-9, "100 * 0.01 should be 1.0, got {}", keys[1].v);
}

#[test]
fn clear_existing_defaults_true_and_can_be_disabled_to_merge() {
    let (mut ss, c) = setup();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "opacity", "time": 5.0, "value": 0.9, "ease": "linear"}));
    call(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "opacity", "data": [{"t": 0.0, "v": 0.1}]}));
    assert_eq!(ss.project.find_clip(&c).unwrap().props["opacity"].keys.len(), 1, "clear_existing=true (default) should wipe the old key at t=5");

    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "x", "time": 5.0, "value": 42.0, "ease": "linear"}));
    call(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "x", "data": [{"t": 0.0, "v": 1.0}], "clear_existing": false}));
    assert_eq!(ss.project.find_clip(&c).unwrap().props["x"].keys.len(), 2, "clear_existing=false should merge/add alongside the old key");
}

#[test]
fn animates_an_effect_param_when_effect_id_is_given() {
    let (mut ss, c) = setup();
    let e = call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "exposure"}))["effect_id"].as_str().unwrap().to_string();
    let r = call(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "stops", "effect_id": &e, "data": [{"t": 0.0, "v": 1.0}, {"t": 1.0, "v": -1.0}]}));
    assert_eq!(r["keys"], 2);
}

#[test]
fn unknown_clip_property_without_effect_id_is_a_clear_error() {
    let (mut ss, c) = setup();
    let e = err(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "not_a_real_prop", "data": [{"t": 0.0, "v": 1.0}]}));
    assert!(e.contains("unknown clip property"), "{e}");
}

#[test]
fn giving_zero_or_two_data_sources_is_an_error() {
    let (mut ss, c) = setup();
    let e1 = err(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "opacity"}));
    assert!(e1.contains("exactly one of"), "{e1}");
    let e2 = err(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "opacity", "data": [], "csv": "t,v\n0,1\n"}));
    assert!(e2.contains("exactly one of"), "{e2}");
}

#[test]
fn array_of_arrays_data_needs_a_fields_list() {
    let (mut ss, c) = setup();
    let e = err(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "opacity", "data": [[0.0, 0.5]]}));
    assert!(e.contains("fields"), "{e}");
    let r = call(&mut ss, "animate_from_data", json!({"clip_id": &c, "property": "opacity", "data": [[0.0, 0.5], [1.0, 0.9]], "fields": ["t", "v"]}));
    assert_eq!(r["keys"], 2);
}

// ---------------- essential properties ----------------

#[test]
fn mark_list_and_clear_essential_properties() {
    let (mut ss, c) = setup();
    let r = call(&mut ss, "set_essential_properties", json!({"clip_id": &c, "properties": [{"property": "opacity", "label": "Fade"}, {"property": "x"}]}));
    assert_eq!(r["essential_properties"].as_array().unwrap().len(), 2);
    let listed = call(&mut ss, "list_essential_properties", json!({"clip_id": &c}));
    let props = listed["essential_properties"].as_array().unwrap();
    assert!(props.iter().any(|p| p["property"] == "opacity" && p["label"] == "Fade"));
    assert!(props.iter().any(|p| p["property"] == "x" && p["label"].is_null()));

    let cleared = call(&mut ss, "clear_essential_properties", json!({"clip_id": &c}));
    assert_eq!(cleared["removed"], 2);
    let listed2 = call(&mut ss, "list_essential_properties", json!({"clip_id": &c}));
    assert_eq!(listed2["essential_properties"].as_array().unwrap().len(), 0);
}

#[test]
fn essential_properties_on_an_effect_param_record_the_effect_id() {
    let (mut ss, c) = setup();
    let e = call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "exposure"}))["effect_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_essential_properties", json!({"clip_id": &c, "properties": [{"property": "stops", "effect_id": &e, "label": "Brightness"}]}));
    let listed = call(&mut ss, "list_essential_properties", json!({"clip_id": &c}));
    let p = &listed["essential_properties"][0];
    assert_eq!(p["property"], "stops"); assert_eq!(p["effect_id"], e); assert_eq!(p["label"], "Brightness");
}

#[test]
fn replace_true_default_clears_prior_marks_replace_false_merges() {
    let (mut ss, c) = setup();
    call(&mut ss, "set_essential_properties", json!({"clip_id": &c, "properties": [{"property": "opacity"}]}));
    call(&mut ss, "set_essential_properties", json!({"clip_id": &c, "properties": [{"property": "x"}]}));
    let listed = call(&mut ss, "list_essential_properties", json!({"clip_id": &c}));
    assert_eq!(listed["essential_properties"].as_array().unwrap().len(), 1, "default replace=true should drop the opacity mark");

    call(&mut ss, "set_essential_properties", json!({"clip_id": &c, "properties": [{"property": "y"}], "replace": false}));
    let listed2 = call(&mut ss, "list_essential_properties", json!({"clip_id": &c}));
    assert_eq!(listed2["essential_properties"].as_array().unwrap().len(), 2, "replace=false should merge");
}

#[test]
fn project_wide_list_finds_every_clip_with_essential_marks() {
    let (mut ss, c) = setup();
    call(&mut ss, "set_essential_properties", json!({"clip_id": &c, "properties": [{"property": "opacity"}]}));
    let listed = call(&mut ss, "list_essential_properties", json!({}));
    let clips = listed["clips"].as_array().unwrap();
    assert_eq!(clips.len(), 1);
    assert_eq!(clips[0]["clip_id"], c);
}

// ---------------- responsive time ----------------

#[test]
fn uniform_retime_with_no_protect_zones_scales_every_keyframe_proportionally() {
    let (mut ss, c) = setup();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "opacity", "time": 0.0, "value": 0.0, "ease": "linear"}));
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "opacity", "time": 10.0, "value": 1.0, "ease": "linear"}));
    let r = call(&mut ss, "apply_responsive_time", json!({"clip_id": &c, "new_duration": 20.0}));
    assert_eq!(r["keyframes_remapped"], 2);
    let keys = &ss.project.find_clip(&c).unwrap().props["opacity"].keys;
    assert!((keys[0].t - 0.0).abs() < 1e-9);
    assert!((keys[1].t - 20.0).abs() < 1e-9, "a key at the old end should land at the new end: {}", keys[1].t);
    assert!((ss.project.find_clip(&c).unwrap().duration - 20.0).abs() < 1e-9);
}

#[test]
fn protected_zones_keep_their_own_timing_while_the_middle_stretches() {
    let (mut ss, c) = setup();
    // old duration 10s: protect [0,1] and [9,10]; keys at 0 (start), 0.5 (in protect_in), 5 (middle), 9.5 (in protect_out), 10 (end)
    for (t, v) in [(0.0, 0.0), (0.5, 1.0), (5.0, 2.0), (9.5, 3.0), (10.0, 4.0)] {
        call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "opacity", "time": t, "value": v, "ease": "linear"}));
    }
    call(&mut ss, "apply_responsive_time", json!({"clip_id": &c, "new_duration": 20.0, "protect_in": 1.0, "protect_out": 1.0}));
    let keys = &ss.project.find_clip(&c).unwrap().props["opacity"].keys;
    let by_v = |v: f64| keys.iter().find(|k| (k.v - v).abs() < 1e-9).unwrap().t;
    assert!((by_v(0.0) - 0.0).abs() < 1e-9, "protect_in start unchanged");
    assert!((by_v(1.0) - 0.5).abs() < 1e-9, "key inside protect_in zone unchanged");
    assert!((by_v(3.0) - 19.5).abs() < 1e-9, "key inside protect_out zone slides to stay flush with the new end (20 - (10-9.5))");
    assert!((by_v(4.0) - 20.0).abs() < 1e-9, "protect_out end slides to the new end");
    let mid = by_v(2.0);
    assert!(mid > 1.0 && mid < 19.0, "middle key should have moved into the stretched middle span, got {mid}");
}

#[test]
fn effect_param_keyframes_are_remapped_too() {
    let (mut ss, c) = setup();
    let e = call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "exposure"}))["effect_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "stops", "effect_id": &e, "time": 10.0, "value": 2.0, "ease": "linear"}));
    call(&mut ss, "apply_responsive_time", json!({"clip_id": &c, "new_duration": 5.0}));
    let keys = &ss.project.find_clip(&c).unwrap().effects.iter().find(|x| x.id == e).unwrap().params["stops"].keys;
    assert!((keys[0].t - 5.0).abs() < 1e-9);
}

#[test]
fn protect_zones_exceeding_the_shorter_duration_are_rejected() {
    let (mut ss, c) = setup();
    let e = err(&mut ss, "apply_responsive_time", json!({"clip_id": &c, "new_duration": 3.0, "protect_in": 2.0, "protect_out": 2.0}));
    assert!(e.contains("exceeds"), "{e}");
}

// ---------------- scripting hooks ----------------

#[test]
fn register_list_remove_and_run_a_hook() {
    let (mut ss, c) = setup();
    call(&mut ss, "template_define", json!({"name": "fade_it", "params": [{"name": "clip", "type": "id"}], "ops": [
        {"tool": "set_keyframe", "args": {"clip_id": "{{clip}}", "property": "opacity", "time": 0.0, "value": 0.0, "ease": "linear"}}
    ]}));
    let reg = call(&mut ss, "register_hook", json!({"event": "before_render", "template": "fade_it", "args": {"clip": &c}}));
    assert_eq!(reg["count"], 1);

    let listed = call(&mut ss, "list_hooks", json!({"event": "before_render"}));
    assert_eq!(listed["hooks"].as_array().unwrap().len(), 1);

    let ran = call(&mut ss, "run_hooks", json!({"event": "before_render"}));
    assert_eq!(ran["ran"], 1);
    assert_eq!(ran["results"][0]["ok"], true);
    assert_eq!(ss.project.find_clip(&c).unwrap().props["opacity"].keys.len(), 1, "the hook's template_apply should have actually run");

    let rem = call(&mut ss, "remove_hook", json!({"event": "before_render", "index": 0}));
    assert_eq!(rem["remaining"], 0);
}

#[test]
fn running_an_event_with_no_hooks_is_a_harmless_no_op() {
    let (mut ss, _c) = setup();
    let r = call(&mut ss, "run_hooks", json!({"event": "nothing_registered_here"}));
    assert_eq!(r["ran"], 0);
}

#[test]
fn a_failing_hook_reports_its_own_error_without_stopping_the_rest() {
    let (mut ss, _c) = setup();
    call(&mut ss, "register_hook", json!({"event": "ev", "template": "does_not_exist", "args": {}}));
    call(&mut ss, "template_define", json!({"name": "noop_ok", "params": [], "ops": [{"tool": "list_effects", "args": {}}]}));
    call(&mut ss, "register_hook", json!({"event": "ev", "template": "noop_ok", "args": {}}));
    let r = call(&mut ss, "run_hooks", json!({"event": "ev"}));
    assert_eq!(r["ran"], 2);
    assert_eq!(r["results"][0]["ok"], false);
    assert_eq!(r["results"][1]["ok"], true);
}

#[test]
fn hooks_persist_through_save_and_load_project() {
    let (mut ss, _c) = setup();
    call(&mut ss, "template_define", json!({"name": "t1", "params": [], "ops": [{"tool": "list_effects", "args": {}}]}));
    call(&mut ss, "register_hook", json!({"event": "ev", "template": "t1", "args": {}}));
    let path = std::env::temp_dir().join(format!("kimo_v7_hooks_{}.kimo", std::process::id()));
    call(&mut ss, "save_project", json!({"path": path.to_str().unwrap()}));
    let mut ss2 = Session::new();
    call(&mut ss2, "load_project", json!({"path": path.to_str().unwrap()}));
    std::fs::remove_file(&path).ok();
    let listed = call(&mut ss2, "list_hooks", json!({"event": "ev"}));
    assert_eq!(listed["hooks"].as_array().unwrap().len(), 1);
}

// ---------------- old projects unaffected ----------------

#[test]
fn a_project_that_never_touched_any_of_these_features_serializes_with_no_new_fields() {
    let (mut ss, c) = setup();
    let path = std::env::temp_dir().join(format!("kimo_v7_plain_{}.kimo", std::process::id()));
    call(&mut ss, "save_project", json!({"path": path.to_str().unwrap()}));
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::remove_file(&path).ok();
    assert!(!text.contains("\"hooks\""), "hooks field must not appear when nothing registered a hook");
    let _ = c;
}
