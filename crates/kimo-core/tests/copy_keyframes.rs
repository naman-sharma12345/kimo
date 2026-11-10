//! `copy_keyframes`: copy one property's keyframes (base value, keys, eases, expression ops) from one clip onto one
//! or more other clips -- graph-editor copy/paste keyframes (FEATURES.md L84, alongside the already-shipped
//! paste_attributes and the template system as "effect presets"). Source can be a clip transform property or an
//! effect param; `time_offset` shifts every copied key; `replace` (default true) clears the destination's existing
//! keys first, false merges (overwrites only keys at matching times, keeps the rest).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).unwrap_err() }

fn session() -> (Session, Value) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 160, "height": 90, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    (s, t)
}
fn clip(s: &mut Session, t: &Value, start: f64) -> String {
    c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.5,0.5,0.5], "start": start, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string()
}

#[test]
fn copies_keys_and_ease_onto_another_clip_by_default() {
    let (mut s, t) = session();
    let a = clip(&mut s, &t, 0.0); let b = clip(&mut s, &t, 5.0);
    c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "x", "time": 0.0, "value": -50.0, "ease": "in_out"}));
    c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "x", "time": 1.0, "value": 50.0}));
    let r = c(&mut s, "copy_keyframes", json!({"from_clip_id": a, "from_property": "x", "to_clip_ids": [b]}));
    assert_eq!(r["pasted_onto"], json!([b]));
    let bp = s.project.find_clip(&b).unwrap().props.get("x").unwrap();
    assert_eq!(bp.keys.len(), 2);
    assert!((bp.keys[0].t - 0.0).abs() < 1e-9 && (bp.keys[0].v - (-50.0)).abs() < 1e-9);
    assert!((bp.keys[1].t - 1.0).abs() < 1e-9 && (bp.keys[1].v - 50.0).abs() < 1e-9);
}

#[test]
fn time_offset_shifts_every_copied_key() {
    let (mut s, t) = session();
    let a = clip(&mut s, &t, 0.0); let b = clip(&mut s, &t, 0.0);
    c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "opacity", "time": 0.2, "value": 0.1}));
    c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "opacity", "time": 1.2, "value": 0.9}));
    c(&mut s, "copy_keyframes", json!({"from_clip_id": a, "from_property": "opacity", "to_clip_ids": [b], "time_offset": 0.5}));
    let bp = s.project.find_clip(&b).unwrap().props.get("opacity").unwrap();
    assert!((bp.keys[0].t - 0.7).abs() < 1e-9); assert!((bp.keys[1].t - 1.7).abs() < 1e-9);
}

#[test]
fn replace_false_merges_instead_of_clearing() {
    let (mut s, t) = session();
    let a = clip(&mut s, &t, 0.0); let b = clip(&mut s, &t, 0.0);
    c(&mut s, "set_keyframe", json!({"clip_id": b, "property": "scale", "time": 0.5, "value": 2.0}));
    c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "scale", "time": 0.0, "value": 0.5}));
    c(&mut s, "copy_keyframes", json!({"from_clip_id": a, "from_property": "scale", "to_clip_ids": [b], "replace": false}));
    let bp = s.project.find_clip(&b).unwrap().props.get("scale").unwrap();
    // both the pre-existing 0.5s key and the copied 0.0s key should be present
    let times: Vec<f64> = bp.keys.iter().map(|k| k.t).collect();
    assert!(times.iter().any(|t| (t - 0.5).abs() < 1e-9), "{times:?}");
    assert!(times.iter().any(|t| (t - 0.0).abs() < 1e-9), "{times:?}");
}

#[test]
fn can_copy_an_effect_param_onto_another_clips_effect() {
    let (mut s, t) = session();
    let a = clip(&mut s, &t, 0.0); let b = clip(&mut s, &t, 0.0);
    let ea = c(&mut s, "add_effect", json!({"clip_id": a, "kind": "blur", "params": {"radius": 0.0}}))["effect_id"].as_str().unwrap().to_string();
    let eb = c(&mut s, "add_effect", json!({"clip_id": b, "kind": "blur", "params": {"radius": 0.0}}))["effect_id"].as_str().unwrap().to_string();
    c(&mut s, "set_keyframe", json!({"clip_id": a, "effect_id": ea, "property": "radius", "time": 0.0, "value": 0.0}));
    c(&mut s, "set_keyframe", json!({"clip_id": a, "effect_id": ea, "property": "radius", "time": 1.0, "value": 10.0}));
    c(&mut s, "copy_keyframes", json!({"from_clip_id": a, "from_property": "radius", "from_effect_id": ea, "to_clip_ids": [b], "to_effect_id": eb}));
    let bc = s.project.find_clip(&b).unwrap();
    let p = bc.effects.iter().find(|e| e.id == eb).unwrap().params.get("radius").unwrap();
    assert_eq!(p.keys.len(), 2);
}

#[test]
fn can_rename_the_destination_property() {
    let (mut s, t) = session();
    let a = clip(&mut s, &t, 0.0); let b = clip(&mut s, &t, 0.0);
    c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "x", "time": 0.0, "value": 10.0}));
    c(&mut s, "copy_keyframes", json!({"from_clip_id": a, "from_property": "x", "to_clip_ids": [b], "to_property": "y"}));
    let yp = s.project.find_clip(&b).unwrap().props.get("y").unwrap();
    assert_eq!(yp.keys.len(), 1); assert!((yp.keys[0].v - 10.0).abs() < 1e-9);
    // x (the default transform prop, never touched by this paste) keeps its default, un-keyed state
    assert!(s.project.find_clip(&b).unwrap().props.get("x").unwrap().keys.is_empty());
}

#[test]
fn errors_when_the_source_property_has_no_keyframes() {
    let (mut s, t) = session();
    let a = clip(&mut s, &t, 0.0); let b = clip(&mut s, &t, 0.0);
    let e = err(&mut s, "copy_keyframes", json!({"from_clip_id": a, "from_property": "x", "to_clip_ids": [b]}));
    assert!(e.to_lowercase().contains("keyframe") || e.to_lowercase().contains("no such property"), "got: {e}");
}

#[test]
fn errors_on_empty_destination_list() {
    let (mut s, t) = session();
    let a = clip(&mut s, &t, 0.0);
    c(&mut s, "set_keyframe", json!({"clip_id": a, "property": "x", "time": 0.0, "value": 10.0}));
    let e = err(&mut s, "copy_keyframes", json!({"from_clip_id": a, "from_property": "x", "to_clip_ids": []}));
    assert!(e.to_lowercase().contains("to_clip_ids"), "got: {e}");
}
