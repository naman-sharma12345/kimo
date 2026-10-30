//! v7.2 additions: get_curve, move_key and remove_key gain an optional effect_id so an effect's own
//! keyframed param can be read/moved/deleted the same way a clip's transform property can -- before
//! this they only ever reached `c.props`, even though set_keyframe has always been able to target an
//! effect param (effect_id + property). Backward compatible: effect_id omitted behaves exactly as
//! before (every existing call site/test is clip-props-only and untouched).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }

fn scene() -> (Session, String, String) {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "start": 0.0, "duration": 10.0}))["clip_id"].as_str().unwrap().to_string();
    let fx = c(&mut ss, "add_effect", json!({"clip_id": id, "kind": "blur"}))["effect_id"].as_str().unwrap().to_string();
    c(&mut ss, "set_keyframe", json!({"clip_id": id, "effect_id": fx, "property": "radius", "time": 0.0, "value": 0.0}));
    c(&mut ss, "set_keyframe", json!({"clip_id": id, "effect_id": fx, "property": "radius", "time": 1.0, "value": 40.0}));
    (ss, id, fx)
}

#[test] fn get_curve_reads_an_effect_param_with_effect_id() {
    let (mut ss, id, fx) = scene();
    let r = c(&mut ss, "get_curve", json!({"clip_id": id, "effect_id": fx, "property": "radius"}));
    let keys = r["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[0]["time"], 0.0); assert_eq!(keys[0]["value"], 0.0);
    assert_eq!(keys[1]["time"], 1.0); assert_eq!(keys[1]["value"], 40.0);
}

#[test] fn get_curve_without_effect_id_still_only_reaches_clip_props() {
    let (mut ss, id, _fx) = scene();
    c(&mut ss, "set_keyframe", json!({"clip_id": id, "property": "opacity", "time": 0.0, "value": 1.0}));
    // "radius" exists only on the effect, not as a clip transform prop -- omitting effect_id must not
    // silently find it there (no cross-talk between the two namespaces).
    let m = e(&mut ss, "get_curve", json!({"clip_id": id, "property": "radius"}));
    assert!(m.contains("no such property"), "{m}");
    let r = c(&mut ss, "get_curve", json!({"clip_id": id, "property": "opacity"}));
    assert_eq!(r["keys"].as_array().unwrap().len(), 1, "clip prop path is unchanged");
}

#[test] fn get_curve_unknown_effect_id_errors() {
    let (mut ss, id, _fx) = scene();
    let m = e(&mut ss, "get_curve", json!({"clip_id": id, "effect_id": "nope", "property": "radius"}));
    assert!(m.contains("no such effect"), "{m}");
}

#[test] fn move_key_moves_an_effect_params_keyframe() {
    let (mut ss, id, fx) = scene();
    let r = c(&mut ss, "move_key", json!({"clip_id": id, "effect_id": fx, "property": "radius", "time": 1.0, "new_time": 2.0, "new_value": 80.0}));
    assert_eq!(r["ok"], true);
    let curve = c(&mut ss, "get_curve", json!({"clip_id": id, "effect_id": fx, "property": "radius"}));
    let keys = curve["keys"].as_array().unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(keys[1]["time"], 2.0); assert_eq!(keys[1]["value"], 80.0);
}

#[test] fn remove_key_deletes_an_effect_params_keyframe() {
    let (mut ss, id, fx) = scene();
    let r = c(&mut ss, "remove_key", json!({"clip_id": id, "effect_id": fx, "property": "radius", "time": 1.0}));
    assert_eq!(r["keys"], 1);
    let curve = c(&mut ss, "get_curve", json!({"clip_id": id, "effect_id": fx, "property": "radius"}));
    assert_eq!(curve["keys"].as_array().unwrap().len(), 1, "only the t=0 key remains");
    let m = e(&mut ss, "remove_key", json!({"clip_id": id, "effect_id": fx, "property": "radius", "time": 1.0}));
    assert!(m.contains("no key at that time"));
}

#[test] fn move_and_remove_key_unknown_effect_id_errors() {
    let (mut ss, id, _fx) = scene();
    assert!(e(&mut ss, "move_key", json!({"clip_id": id, "effect_id": "nope", "property": "radius", "time": 0.0})).contains("no such effect"));
    assert!(e(&mut ss, "remove_key", json!({"clip_id": id, "effect_id": "nope", "property": "radius", "time": 0.0})).contains("no such effect"));
}
