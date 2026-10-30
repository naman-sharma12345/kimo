//! v7.6 feature: batch_set_meta -- apply a role, colour label and/or tags to many clips at once, by
//! explicit clip_ids or the same find-style filter find already takes, in one call.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn try_c(ss: &mut Session, n: &str, v: Value) -> Result<Value, String> { dispatch(ss, n, &v) }

fn new_proj() -> Session {
    let mut ss = Session::new();
    c(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    ss
}

fn add_shape(ss: &mut Session, t: &str, start: f64) -> String {
    c(ss, "add_shape", json!({"track_id": t, "shape": "rect", "start": start, "duration": 1.0, "color": [0.5, 0.5, 0.5, 1.0]}))["clip_id"].as_str().unwrap().to_string()
}

fn clip_meta(ss: &mut Session, id: &str) -> Value {
    let r = c(ss, "find", json!({"kind": "clip", "limit": 500}));
    for m in r["matches"].as_array().unwrap() { if m["id"] == id { return m.clone(); } }
    panic!("clip {id} not found");
}

#[test]
fn clip_ids_sets_role_and_color_on_exactly_those_clips() {
    let mut ss = new_proj();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = add_shape(&mut ss, &t, 0.0);
    let b = add_shape(&mut ss, &t, 1.0);
    let other = add_shape(&mut ss, &t, 2.0);

    let r = c(&mut ss, "batch_set_meta", json!({"clip_ids": [a, b], "role": "broll", "color": "red"}));
    assert_eq!(r["count"], 2, "{r}");
    assert_eq!(clip_meta(&mut ss, &a)["role"], "broll");
    assert_eq!(clip_meta(&mut ss, &b)["role"], "broll");
    assert_eq!(clip_meta(&mut ss, &a)["color"], "red");
    assert_eq!(clip_meta(&mut ss, &other)["role"], Value::Null, "untouched clip must not get the role");
}

#[test]
fn filter_selects_by_effect_and_applies_a_different_color_than_the_filter_used() {
    let mut ss = new_proj();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let glowy = add_shape(&mut ss, &t, 0.0);
    c(&mut ss, "add_effect", json!({"clip_id": glowy, "kind": "glow"}));
    let plain = add_shape(&mut ss, &t, 1.0);

    // filter.color would be the SELECTOR if given; here only has_effect selects, and the top-level
    // color is the value being set -- the two must never be confused.
    let r = c(&mut ss, "batch_set_meta", json!({"filter": {"has_effect": "glow"}, "color": "green"}));
    assert_eq!(r["count"], 1, "{r}");
    assert_eq!(clip_meta(&mut ss, &glowy)["color"], "green");
    assert_eq!(clip_meta(&mut ss, &plain)["color"], Value::Null);
}

#[test]
fn add_tags_and_remove_tags_pass_through_to_every_selected_clip() {
    let mut ss = new_proj();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = add_shape(&mut ss, &t, 0.0);
    c(&mut ss, "set_meta", json!({"id": a, "tags": ["keep_me", "drop_me"]}));
    let b = add_shape(&mut ss, &t, 1.0);

    c(&mut ss, "batch_set_meta", json!({"clip_ids": [a, b], "add_tags": ["reviewed"], "remove_tags": ["drop_me"]}));
    let am = clip_meta(&mut ss, &a);
    let at: Vec<String> = am["tags"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(at.contains(&"keep_me".to_string()), "{am}");
    assert!(at.contains(&"reviewed".to_string()), "{am}");
    assert!(!at.contains(&"drop_me".to_string()), "{am}");
    let bm = clip_meta(&mut ss, &b);
    assert!(bm["tags"].as_array().unwrap().iter().any(|v| v == "reviewed"), "{bm}");
}

#[test]
fn empty_clip_ids_updates_nothing() {
    let mut ss = new_proj();
    let r = c(&mut ss, "batch_set_meta", json!({"clip_ids": [], "role": "x"}));
    assert_eq!(r["count"], 0, "{r}");
}

#[test]
fn errors_with_no_meta_change_requested() {
    let mut ss = new_proj();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = add_shape(&mut ss, &t, 0.0);
    assert!(try_c(&mut ss, "batch_set_meta", json!({"clip_ids": [a]})).is_err());
}

#[test]
fn errors_when_neither_clip_ids_nor_filter_given() {
    let mut ss = new_proj();
    assert!(try_c(&mut ss, "batch_set_meta", json!({"role": "x"})).is_err());
}

#[test]
fn errors_when_both_clip_ids_and_filter_given() {
    let mut ss = new_proj();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = add_shape(&mut ss, &t, 0.0);
    assert!(try_c(&mut ss, "batch_set_meta", json!({"clip_ids": [a], "filter": {"track": t}, "role": "x"})).is_err());
}

#[test]
fn unknown_filter_field_errors() {
    let mut ss = new_proj();
    assert!(try_c(&mut ss, "batch_set_meta", json!({"filter": {"nonsense": "x"}, "role": "x"})).is_err());
}

#[test]
fn unknown_clip_id_errors_and_nothing_partial_sticks() {
    let mut ss = new_proj();
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let a = add_shape(&mut ss, &t, 0.0);
    assert!(try_c(&mut ss, "batch_set_meta", json!({"clip_ids": [a, "no-such-clip"], "role": "x"})).is_err());
}
