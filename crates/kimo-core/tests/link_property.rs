//! link_property (After Effects pickwhip): one clip's property is driven by another clip's property (or effect param) as
//! value = driver * multiplier + offset, optionally delayed. The link follows the driver's keys, easing and expressions.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
fn setup() -> (Session, String, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 160, "height": 90, "fps": 10}));
    let t1 = id(&c(&mut s, "add_track", json!({})), "track_id"); let t2 = id(&c(&mut s, "add_track", json!({})), "track_id");
    let mover = id(&c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "solid", "color": [1.0, 0.0, 0.0], "start": 0.0, "duration": 4.0})), "clip_id");
    let title = id(&c(&mut s, "add_clip", json!({"track_id": t2, "source_type": "solid", "color": [1.0, 1.0, 1.0], "start": 1.0, "duration": 3.0})), "clip_id");
    c(&mut s, "set_keyframe", json!({"clip_id": mover, "property": "x", "time": 0.0, "value": -40.0}));
    c(&mut s, "set_keyframe", json!({"clip_id": mover, "property": "x", "time": 4.0, "value": 40.0, "ease": "in_out"}));
    (s, mover, title)
}
fn sample(s: &mut Session, clip: &str, prop: &str, ts: &[f64]) -> Vec<f64> { c(s, "sample_property", json!({"clip_id": clip, "property": prop, "times": ts}))["values"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect() }

#[test]
fn a_title_follows_a_moving_clip() {
    let (mut s, mover, title) = setup();
    let r = dispatch(&mut s, "link_property", &json!({"clip_id": title, "property": "x", "driver_clip_id": mover, "driver_property": "x", "offset": 10.0}));
    assert!(r.is_ok(), "link_property must exist: {}", r.unwrap_err());
    let ts = [1.0, 1.55, 2.0, 2.73, 3.5, 3.95];
    let d = sample(&mut s, &mover, "x", &ts); let v = sample(&mut s, &title, "x", &ts);
    for i in 0..ts.len() { assert!((v[i] - (d[i] + 10.0)).abs() < 0.05, "t {}: title x {} follows mover x {} + 10", ts[i], v[i], d[i]); }
}

#[test]
fn b_multiplier_cross_property_and_delay() {
    let (mut s, mover, title) = setup();
    // scale follows the mover's x: scale = 1 + x * 0.01
    c(&mut s, "link_property", json!({"clip_id": title, "property": "scale", "driver_clip_id": mover, "driver_property": "x", "multiplier": 0.01, "offset": 1.0}));
    let ts = [1.0, 2.0, 3.0, 3.9]; let d = sample(&mut s, &mover, "x", &ts); let v = sample(&mut s, &title, "scale", &ts);
    for i in 0..ts.len() { assert!((v[i] - (1.0 + 0.01 * d[i])).abs() < 0.002, "t {}: scale {} = 1 + 0.01 * {}", ts[i], v[i], d[i]); }
    // delay: the follower trails the driver by 0.5 s (AE valueAtTime(time - 0.5))
    c(&mut s, "link_property", json!({"clip_id": title, "property": "y", "driver_clip_id": mover, "driver_property": "x", "delay": 0.5}));
    let d = sample(&mut s, &mover, "x", &[1.5, 2.5, 3.0]); let v = sample(&mut s, &title, "y", &[2.0, 3.0, 3.5]);
    for i in 0..3 { assert!((v[i] - d[i]).abs() < 0.05, "delayed follow {} vs {}", v[i], d[i]); }
}

#[test]
fn c_relinking_follows_new_driver_keys_and_bad_links_are_refused() {
    let (mut s, mover, title) = setup();
    c(&mut s, "link_property", json!({"clip_id": title, "property": "x", "driver_clip_id": mover, "driver_property": "x"}));
    c(&mut s, "set_keyframe", json!({"clip_id": mover, "property": "x", "time": 2.0, "value": 70.0}));
    c(&mut s, "link_property", json!({"clip_id": title, "property": "x", "driver_clip_id": mover, "driver_property": "x"}));
    let v = sample(&mut s, &title, "x", &[2.0]); assert!((v[0] - 70.0).abs() < 0.05, "relink picks up the driver's new key: {v:?}");
    assert!(dispatch(&mut s, "link_property", &json!({"clip_id": title, "property": "x", "driver_clip_id": title, "driver_property": "x"})).is_err(), "a property cannot drive itself");
    assert!(dispatch(&mut s, "link_property", &json!({"clip_id": title, "property": "x", "driver_clip_id": "nope", "driver_property": "x"})).is_err());
    assert!(dispatch(&mut s, "link_property", &json!({"clip_id": title, "property": "x", "driver_clip_id": mover, "driver_property": "x", "multiplier": "big"})).is_err());
}


#[test]
fn d_live_link_follows_driver_edits_without_relinking() {
    let (mut s, mover, title) = setup();
    // opacity follows the mover's x (multiplier+offset still apply): opacity = 0.5 + 0.01 * x
    c(&mut s, "link_property", json!({"clip_id": title, "property": "opacity", "driver_clip_id": mover, "driver_property": "x", "multiplier": 0.01, "offset": 0.5}));
    // Edit the DRIVER's keyframes directly -- no second link_property call. A live link must pick this up;
    // a baked one (the old bug) keeps showing whatever the driver was worth at the moment link_property ran.
    c(&mut s, "set_keyframe", json!({"clip_id": mover, "property": "x", "time": 2.0, "value": 20.0}));
    let d = sample(&mut s, &mover, "x", &[2.0])[0];
    let v = sample(&mut s, &title, "opacity", &[2.0])[0];
    assert!((v - (0.5 + 0.01 * d)).abs() < 0.01, "sample_property: opacity {v} must equal 0.5 + 0.01 * {d} (the driver's NEW value) right after editing the driver, with no re-link");

    // Same story at actual render time (not just sample_property): render the title clip alone (transparent
    // canvas, so its own alpha equals its own opacity exactly) before vs after editing the driver again.
    let cache = kimo_core::render::Cache::new();
    let before = kimo_core::render::render_clip_alone(&s.project, &title, 2.0, &cache).expect("renders");
    let alpha_before = before.d[(before.w / 2 + (before.h / 2) * before.w) * 4 + 3];
    c(&mut s, "set_keyframe", json!({"clip_id": mover, "property": "x", "time": 2.0, "value": -30.0}));
    let after = kimo_core::render::render_clip_alone(&s.project, &title, 2.0, &cache).expect("renders");
    let alpha_after = after.d[(after.w / 2 + (after.h / 2) * after.w) * 4 + 3];
    let expect_before = 0.5 + 0.01 * 20.0; let expect_after = 0.5 + 0.01 * -30.0;
    assert!((alpha_before - expect_before).abs() < 0.02, "render before second edit: alpha {alpha_before} ~= {expect_before}");
    assert!((alpha_after - expect_after).abs() < 0.02, "render after editing the driver again (no relink): alpha {alpha_after} ~= {expect_after}, not stuck at {alpha_before}");
}
