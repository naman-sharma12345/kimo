//! RV-27 lane A curves: set_keyframes (A1).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn scene() -> (Session, String) { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "start": 0.0, "duration": 30.0}))["clip_id"].as_str().unwrap().to_string(); (ss, id) }

const EASES: [&str; 4] = ["linear", "out", "in_out", "out_expo"];
#[test] fn five_hundred_keys_equal_five_hundred_single_calls_byte_for_byte() {
    let keys: Vec<Value> = (0..500).map(|i| { let t = ((i * 37) % 500) as f64 * 0.05; json!({"t": t, "v": (i as f64 * 0.37).sin() * 3.0 + 1.0, "ease": EASES[i % 4]}) }).collect();
    let (mut a, ca) = scene(); for k in &keys { c(&mut a, "set_keyframe", json!({"clip_id": ca, "property": "opacity", "time": k["t"], "value": k["v"], "ease": k["ease"]})); }
    let (mut b, cb) = scene(); let r = c(&mut b, "set_keyframes", json!({"clip_id": cb, "property": "opacity", "keys": keys})); assert_eq!(r["written"], 500);
    assert_eq!(pj(&a), pj(&b), "byte-equal project"); assert_eq!(b.undo.len(), scene().0.undo.len() + 1, "one undo step for the whole write");
    assert!(keys.iter().any(|k| k["v"].as_f64().unwrap() > 1.0), "values above 1 are stored unclamped");
}
#[test] fn bad_key_rolls_everything_back() {
    let (mut ss, id) = scene(); c(&mut ss, "set_keyframe", json!({"clip_id": id, "property": "x", "time": 1.0, "value": 5.0})); let h = pj(&ss); let u = ss.undo.len();
    for bad in [json!({"t": 1.0}), json!({"t": "a", "v": 1.0}), json!({"t": 1.0, "v": 1.0, "ease": "wobble"}), json!({"t": 1.0, "v": 1.0, "bez": [0.1, 0.2]}), json!({"t": 1.0, "v": 1.0, "hold": "yes"}), json!({"t": 1.0, "v": 1e300})] {
        let m = e(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "x", "keys": [{"t": 0.0, "v": 9.0}, {"t": 0.5, "v": 8.0}, bad]})); assert!(!m.is_empty());
        assert_eq!(pj(&ss), h, "{m}"); assert_eq!(ss.undo.len(), u); }
    assert!(e(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "x", "keys": []})).contains("at least one"));
    assert!(e(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "nope", "keys": [{"t": 0, "v": 1}]})).contains("Valid:"));
    assert!(e(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "x", "mode": "zap", "keys": [{"t": 0, "v": 1}]})).contains("Valid:"));
    assert!(e(&mut ss, "set_keyframes", json!({"clip_id": "nope", "property": "x", "keys": [{"t": 0, "v": 1}]})).contains("nope"));
    assert_eq!(pj(&ss), h);
}
#[test] fn modes_replace_all_merge_replace_range_and_unsorted_input() {
    let ks = |c: &Session, id: &str| -> Vec<(f64, f64)> { c.project.find_clip(id).unwrap().props["scale"].keys.iter().map(|k| (k.t, k.v)).collect() };
    let (mut ss, id) = scene(); c(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "scale", "keys": [{"t": 3.0, "v": 3.0}, {"t": 1.0, "v": -1.0}, {"t": 2.0, "v": 2.0}]})); assert_eq!(ks(&ss, &id), [(1.0, -1.0), (2.0, 2.0), (3.0, 3.0)], "sorted, negative scale kept");
    let r = c(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "scale", "mode": "merge", "keys": [{"t": 2.0, "v": 20.0}, {"t": 4.0, "v": 4.0}]})); assert_eq!(r["replaced"], 1, "merge overwrote the key at t=2"); assert_eq!(ks(&ss, &id), [(1.0, -1.0), (2.0, 20.0), (3.0, 3.0), (4.0, 4.0)]);
    let r = c(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "scale", "mode": "replace_range", "keys": [{"t": 1.5, "v": 0.0}, {"t": 3.5, "v": 9.0}]})); assert_eq!(r["replaced"], 2, "keys at t=2 and t=3 were removed"); assert_eq!(ks(&ss, &id), [(1.0, -1.0), (1.5, 0.0), (3.5, 9.0), (4.0, 4.0)]);
    c(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "scale", "keys": [{"t": 0.0, "v": 1.0, "hold": true, "bez": [0.2, -0.5, 0.8, 1.5]}]})); let k = &ss.project.find_clip(&id).unwrap().props["scale"].keys; assert_eq!(k.len(), 1); assert!(k[0].bez.is_some());
    c(&mut ss, "undo", json!({})); assert_eq!(ks(&ss, &id).len(), 4);
}
#[test] fn effect_params_are_keyed_too() {
    let (mut ss, id) = scene(); let fx = c(&mut ss, "add_effect", json!({"clip_id": id, "kind": "blur"}))["effect_id"].as_str().unwrap().to_string();
    let p = c(&mut ss, "get_project", json!({})); let _ = p; let m = dispatch(&mut ss, "set_keyframes", &json!({"clip_id": id, "effect_id": fx, "property": "radius", "keys": [{"t": 0.0, "v": 0.0}, {"t": 1.0, "v": 40.0}]})); assert!(m.is_ok(), "{m:?}");
    assert!(e(&mut ss, "set_keyframes", json!({"clip_id": id, "effect_id": "nope", "property": "radius", "keys": [{"t": 0, "v": 1}]})).contains("no such effect"));
}

fn keys_on(ss: &mut Session, id: &str, prop: &str, ks: Value) { c(ss, "set_keyframes", json!({"clip_id": id, "property": prop, "keys": ks})); }
#[test] fn sample_property_closed_form_and_ranges_without_baking() {
    let (mut ss, id) = scene(); keys_on(&mut ss, &id, "opacity", json!([{"t": 0.0, "v": 0.0}, {"t": 2.0, "v": 4.0}, {"t": 3.0, "v": -2.0, "hold": true}, {"t": 4.0, "v": 9.0}]));
    let h = pj(&ss); let u = ss.undo.len();
    let r = c(&mut ss, "sample_property", json!({"clip_id": id, "property": "opacity", "times": [0.0, 0.5, 1.0, 2.0, 2.5, 3.5, 10.0]}));
    let v: Vec<f64> = r["values"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect(); let want = [0.0, 1.0, 2.0, 4.0, 1.0, -2.0, 9.0];
    for (a, b) in v.iter().zip(want) { assert!((a - b).abs() < 1e-12, "{v:?}"); }
    assert_eq!(pj(&ss), h, "sampling does not bake or change anything"); assert_eq!(ss.undo.len(), u);
    let r = c(&mut ss, "sample_property", json!({"clip_id": id, "property": "opacity", "times": {"start": 0.0, "end": 2.0, "n": 5}})); assert_eq!(r["values"].as_array().unwrap().len(), 5); assert!((r["values"][1].as_f64().unwrap() - 1.0).abs() < 1e-12);
    // clip space vs timeline space: a clip starting at 5s
    let t = ss.project.tracks[0].id.clone(); let late = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 4, "height": 4, "start": 5.0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string(); keys_on(&mut ss, &late, "x", json!([{"t": 0.0, "v": 0.0}, {"t": 2.0, "v": 20.0}]));
    let r = c(&mut ss, "sample_property", json!({"clip_id": late, "property": "x", "times": [4.0, 6.0, 7.5], "space": "timeline"})); assert_eq!(r["values"], json!([0.0, 10.0, 20.0])); assert_eq!(r["active"], json!([false, true, false]));
    let r = c(&mut ss, "sample_property", json!({"clip_id": late, "property": "x", "times": [1.0], "space": "clip"})); assert_eq!(r["values"], json!([10.0]));
}
#[test] fn sample_property_errors_name_what_exists() {
    let (mut ss, id) = scene();
    assert!(e(&mut ss, "sample_property", json!({"clip_id": id, "property": "zzz", "times": [0.0]})).contains("Properties on this clip"));
    assert!(e(&mut ss, "sample_property", json!({"clip_id": id, "property": "x", "times": []})).contains("1 to"));
    assert!(e(&mut ss, "sample_property", json!({"clip_id": id, "property": "x", "times": [0.0], "space": "zzz"})).contains("Valid:"));
    assert!(e(&mut ss, "sample_property", json!({"clip_id": id, "property": "x", "times": {"start": 0, "end": 1, "n": 0}})).contains("times.n"));
    assert!(e(&mut ss, "sample_property", json!({"clip_id": "nope", "property": "x", "times": [0.0]})).contains("nope"));
}
#[test] fn sampled_scale_predicts_the_rendered_bbox_within_one_pixel() {
    use kimo_core::render;
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "fill": [1, 1, 1, 1], "start": 0.0, "duration": 4.0}))["clip_id"].as_str().unwrap().to_string();
    keys_on(&mut ss, &id, "scale", json!([{"t": 0.0, "v": 0.5}, {"t": 1.0, "v": 2.5, "ease": "out"}, {"t": 3.0, "v": 1.0, "ease": "in_out"}]));
    let times = [0.0, 0.2, 0.5, 1.0, 1.7, 2.4, 3.2]; let r = c(&mut ss, "sample_property", json!({"clip_id": id, "property": "scale", "times": times})); let cache = render::Cache::new();
    for (i, tm) in times.iter().enumerate() { let sc = r["values"][i].as_f64().unwrap(); let f = render::render_frame(&ss.project, *tm, &cache).unwrap();
        let (w, hgt) = (160usize, 90usize); let (mut x0, mut x1) = (usize::MAX, 0usize); for y in 0..hgt { for x in 0..w { if f.d[(y * w + x) * 4] > 0.5 { x0 = x0.min(x); x1 = x1.max(x); } } }
        let got = (x1 - x0 + 1) as f64; assert!((got - 20.0 * sc).abs() <= 1.0, "t={tm}: rendered width {got} vs predicted {} (scale {sc})", 20.0 * sc); }
}

#[test] fn sample_unkeyed_standard_props_return_renderer_defaults_and_subframe_times_work() {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "fill": [1, 1, 1, 1], "start": 0.0, "duration": 4.0}))["clip_id"].as_str().unwrap().to_string();
    for (k, v) in [("scale", 1.0), ("opacity", 1.0), ("x", 0.0), ("rotation", 0.0), ("fill_alpha", 1.0), ("pin_gain", 1.0), ("stroke_alpha", 1.0), ("reveal", 1.0), ("rot_x", 0.0), ("rot_y", 0.0), ("pos_z", 0.0), ("backface", 0.0), ("path", 0.0), ("flow_blur", 0.0), ("flow_quality", 0.0)] { let r = c(&mut ss, "sample_property", json!({"clip_id": id, "property": k, "times": [0.0, 1.3, 99.0]})); assert_eq!(r["values"], json!([v, v, v]), "{k}"); }
    assert!(e(&mut ss, "sample_property", json!({"clip_id": id, "property": "nothing_reads_this", "times": [0.0]})).contains("Properties on this clip"));
    keys_on(&mut ss, &id, "x", json!([{"t": 0.0, "v": 0.0}, {"t": 1.0, "v": 100.0}]));
    let r = c(&mut ss, "sample_property", json!({"clip_id": id, "property": "x", "times": [0.0, 0.0001, 0.25, 0.2501, 0.999999]})); let v: Vec<f64> = r["values"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    assert!((v[1] - 0.01).abs() < 1e-9 && (v[2] - 25.0).abs() < 1e-9 && (v[3] - 25.01).abs() < 1e-9 && (v[4] - 99.9999).abs() < 1e-9, "{v:?}");
}

fn get_keys(ss: &Session, id: &str, prop: &str) -> Vec<(f64, f64)> { ss.project.find_clip(id).unwrap().props.get(prop).unwrap().keys.iter().map(|k| (k.t, k.v)).collect() }
fn one(ss: &mut Session) -> String { c(ss, "project_set", json!({"width": 160, "height": 90, "fps": 25})); let t = c(ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 20, "fill": [1, 1, 1, 1], "start": 0.0, "duration": 4.0}))["clip_id"].as_str().unwrap().to_string() }
#[test] fn bake_turns_expressions_into_dense_keys_with_a_measured_error() {
    let mut ss = Session::new(); let id = one(&mut ss);
    keys_on(&mut ss, &id, "x", json!([{"t": 0.0, "v": 0.0}, {"t": 2.0, "v": 100.0, "ease": "in_out"}]));
    c(&mut ss, "set_expression", json!({"clip_id": id, "property": "x", "op": "sin", "freq": 2.0, "amp": 7.0}));
    assert_eq!(ss.project.find_clip(&id).unwrap().props["x"].expr.len(), 1, "setup: expression present");
    let times: Vec<f64> = (0..=40).map(|i| i as f64 * 0.05).collect(); let before = c(&mut ss, "sample_property", json!({"clip_id": id, "property": "x", "times": times, "space": "clip"}));
    let r = c(&mut ss, "bake_property", json!({"clip_id": id, "property": "x", "start": 0.0, "end": 2.0, "fps": 100}));
    assert_eq!(r["keys"], 201); assert_eq!(r["expressions_dropped"], 1); assert!(r["max_error"].as_f64().unwrap() < 0.05, "{r}");
    assert!(ss.project.find_clip(&id).unwrap().props["x"].expr.is_empty(), "expression gone, plain editable keys remain"); assert_eq!(get_keys(&ss, &id, "x").len(), 201);
    let after = c(&mut ss, "sample_property", json!({"clip_id": id, "property": "x", "times": times, "space": "clip"})); let me = r["max_error"].as_f64().unwrap();
    for i in 0..times.len() { let (a, b) = (before["values"][i].as_f64().unwrap(), after["values"][i].as_f64().unwrap()); assert!((a - b).abs() <= me + 1e-9, "t={}: {a} vs {b}", times[i]); }
    let r2 = c(&mut ss, "bake_property", json!({"clip_id": id, "property": "x", "start": 0.0, "end": 2.0, "n": 9})); assert!(r2["max_error"].as_f64().unwrap() > r["max_error"].as_f64().unwrap(), "coarser bake reports a larger error");
    c(&mut ss, "undo", json!({})); assert_eq!(get_keys(&ss, &id, "x").len(), 201, "undo reverts the coarse bake");
}
#[test] fn bake_errors_and_camera_and_unkeyed() {
    let mut ss = Session::new(); let id = one(&mut ss);
    assert!(e(&mut ss, "bake_property", json!({"clip_id": id, "property": "x", "start": 0.0, "end": 1.0, "fps": 10, "drop_expression": false})).contains("twice"));
    assert!(e(&mut ss, "bake_property", json!({"clip_id": id, "property": "x", "start": 0.0, "end": 1.0})).contains("fps"));
    assert!(e(&mut ss, "bake_property", json!({"clip_id": id, "property": "x", "start": 1.0, "end": 1.0, "fps": 10})).contains("greater"));
    assert!(e(&mut ss, "bake_property", json!({"clip_id": id, "property": "x", "start": 0.0, "end": 1.0, "n": 1})).contains("2 to"));
    assert!(e(&mut ss, "bake_property", json!({"clip_id": id, "property": "zzz", "start": 0.0, "end": 1.0, "n": 3})).contains("Properties on this clip"));
    let r = c(&mut ss, "bake_property", json!({"clip_id": id, "property": "scale", "start": 0.0, "end": 1.0, "n": 3})); assert_eq!(r["max_error"], 0.0); assert_eq!(get_keys(&ss, &id, "scale"), vec![(0.0, 1.0), (0.5, 1.0), (1.0, 1.0)]);
}
#[test] fn duplicate_timestamps_last_write_wins_in_one_call_and_merge_overwrites() {
    let mut ss = Session::new(); let id = one(&mut ss);
    let r = c(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "x", "keys": [{"t": 1.0, "v": 5.0}, {"t": 0.0, "v": 1.0}, {"t": 1.0, "v": 9.0}, {"t": 1.0000000001, "v": 11.0}]}));
    let g = get_keys(&ss, &id, "x"); assert!(g.len() == 2 && g[0] == (0.0, 1.0) && (g[1].0 - 1.0).abs() < 1e-6 && g[1].1 == 11.0, "{g:?} {r}");
    c(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "x", "mode": "merge", "keys": [{"t": 1.0, "v": -4.0}, {"t": 2.0, "v": 3.0}]}));
    assert_eq!(get_keys(&ss, &id, "x"), vec![(0.0, 1.0), (1.0, -4.0), (2.0, 3.0)]);
}

#[test] fn bake_flags_partial_ranges_is_atomic_and_reports_a_short_end() {
    let mut ss = Session::new(); let id = one(&mut ss);
    keys_on(&mut ss, &id, "x", json!([{"t": 0.0, "v": 0.0}, {"t": 4.0, "v": 100.0}]));
    c(&mut ss, "set_expression", json!({"clip_id": id, "property": "x", "op": "sin", "freq": 1.0, "amp": 5.0}));
    let r = c(&mut ss, "bake_property", json!({"clip_id": id, "property": "x", "start": 0.0, "end": 4.0, "fps": 10})); assert_eq!(r["outside_range_changed"], false, "{r}"); assert!(r.get("end_requested").is_none());
    c(&mut ss, "set_expression", json!({"clip_id": id, "property": "x", "op": "sin", "freq": 1.0, "amp": 5.0}));
    let r = c(&mut ss, "bake_property", json!({"clip_id": id, "property": "x", "start": 1.0, "end": 2.0, "fps": 10})); assert_eq!(r["outside_range_changed"], true, "{r}"); assert!(r["note"].as_str().unwrap().contains("outside") || r["note"].as_str().unwrap().contains("holds"));
    let r = c(&mut ss, "bake_property", json!({"clip_id": id, "property": "x", "start": 0.0, "end": 1.0, "fps": 2.5})); assert_eq!(r["end_requested"], 1.0); assert!((r["range"][1].as_f64().unwrap() - 0.8).abs() < 1e-9, "{r}");
    let before = serde_json::to_string(&ss.project).unwrap();
    for bad in [json!({"property": "pin_gain", "start": 0.0, "end": 1.0, "n": 1}), json!({"property": "pin_gain", "start": 1.0, "end": 0.0, "n": 3})] { let mut a = bad; a["clip_id"] = json!(id); assert!(dispatch(&mut ss, "bake_property", &a).is_err()); assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "an error must leave the project byte-equal (no default entry inserted)"); }
    c(&mut ss, "bake_property", json!({"clip_id": id, "property": "pin_gain", "start": 0.0, "end": 1.0, "n": 2})); assert!(ss.project.find_clip(&id).unwrap().props.contains_key("pin_gain"));
}

fn expr(ss: &mut Session, id: &str, prop: &str, extra: Value) -> Value { let mut a = json!({"clip_id": id, "property": prop}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } c(ss, "set_expression", a) }
fn vals(ss: &mut Session, id: &str, prop: &str, times: &[f64]) -> Vec<f64> { c(ss, "sample_property", json!({"clip_id": id, "property": prop, "times": times, "space": "clip"}))["values"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect() }
/// independent analytic forms (amplitude-phase for underdamped), not the engine's formula
fn spring_ref(k: f64, cc: f64, m: f64, from: f64, to: f64, t: f64) -> f64 {
    let w0 = (k / m).sqrt(); let z = cc / (2.0 * (k * m).sqrt()); let d = from - to;
    if (z - 1.0).abs() < 1e-12 { return to + d * (-w0 * t).exp() * (1.0 + w0 * t); }
    if z < 1.0 { let sg = z * w0; let wd = w0 * (1.0 - z * z).sqrt(); let a = d * w0 / wd; let phi = (sg / wd).atan(); return to + a * (-sg * t).exp() * (wd * t - phi).cos(); }
    let r = w0 * (z * z - 1.0).sqrt(); let (r1, r2) = (-z * w0 + r, -z * w0 - r); let c1 = d * r2 / (r2 - r1); let c2 = -d * r1 / (r2 - r1); to + c1 * (r1 * t).exp() + c2 * (r2 * t).exp()
}
#[test] fn spring_matches_the_analytic_solution_in_every_damping_regime_with_out_of_range_values() {
    let times: Vec<f64> = (0..60).map(|i| i as f64 * 0.05).collect();
    for (k, cc, m) in [(100.0, 6.0, 1.0), (100.0, 20.0, 1.0), (100.0, 50.0, 1.0), (30.0, 0.0, 2.0)] {
        let mut ss = Session::new(); let id = one(&mut ss);
        expr(&mut ss, &id, "scale", json!({"op": "spring", "stiffness": k, "damping": cc, "mass": m, "from": 3.0, "to": -1.0, "t0": 0.25}));
        let v = vals(&mut ss, &id, "scale", &times);
        for (i, t) in times.iter().enumerate() { let want = if *t <= 0.25 { 3.0 } else { spring_ref(k, cc, m, 3.0, -1.0, t - 0.25) }; assert!((v[i] - want).abs() < 1e-9, "k={k} c={cc} m={m} t={t}: {} vs {want}", v[i]); }
    }
}
#[test] fn bezier_matches_an_independent_solve_and_is_exact_on_symmetric_and_linear_curves() {
    let mut ss = Session::new(); let id = one(&mut ss);
    expr(&mut ss, &id, "x", json!({"op": "bezier", "x1": 0.42, "y1": 0.0, "x2": 0.58, "y2": 1.0, "from": 10.0, "to": 110.0, "duration": 2.0, "t0": 1.0}));
    let ts: Vec<f64> = (0..=20).map(|i| 1.0 + 2.0 * i as f64 / 20.0).collect(); let v = vals(&mut ss, &id, "x", &ts);
    for (i, t) in ts.iter().enumerate() { let u = (t - 1.0) / 2.0; let (x1, y1, x2, y2) = (0.42f64, 0.0f64, 0.58f64, 1.0f64); let mut s = u; for _ in 0..60 { let bx = 3.0 * x1 * s * (1.0 - s).powi(2) + 3.0 * x2 * s * s * (1.0 - s) + s.powi(3); let dx = 3.0 * x1 * (1.0 - s).powi(2) + 6.0 * (x2 - x1) * s * (1.0 - s) + 3.0 * (1.0 - x2) * s * s; if dx.abs() < 1e-12 { break; } s -= (bx - u) / dx; s = s.clamp(0.0, 1.0); }
        let by = 3.0 * y1 * s * (1.0 - s).powi(2) + 3.0 * y2 * s * s * (1.0 - s) + s.powi(3); assert!((v[i] - (10.0 + 100.0 * by)).abs() < 1e-7, "t={t}: {} vs {}", v[i], 10.0 + 100.0 * by); }
    assert!((v[10] - 60.0).abs() < 1e-9, "symmetric curve passes through the midpoint: {}", v[10]);
    expr(&mut ss, &id, "x", json!({"op": "clear"})); expr(&mut ss, &id, "x", json!({"op": "bezier", "x1": 0.0, "y1": 0.0, "x2": 1.0, "y2": 1.0, "from": 0.0, "to": 1.0, "duration": 1.0}));
    let v = vals(&mut ss, &id, "x", &[0.0, 0.1, 0.37, 0.5, 0.9, 1.0, 5.0]); for (a, b) in v.iter().zip([0.0, 0.1, 0.37, 0.5, 0.9, 1.0, 1.0]) { assert!((a - b).abs() < 1e-9, "linear bezier: {a} vs {b}"); }
}
#[test] fn noise_impulse_is_deterministic_across_runs_and_threads_and_composes_with_a_spring() {
    let times: Vec<f64> = (0..40).map(|i| i as f64 * 0.07).collect();
    fn run_n(seed: u64, times: Vec<f64>) -> Vec<f64> { let mut ss = Session::new(); let id = one(&mut ss); expr(&mut ss, &id, "x", json!({"op": "noise_impulse", "seed": seed, "freq": 5.0, "amp": 12.0, "decay": 1.5, "octaves": 3, "t0": 0.5})); vals(&mut ss, &id, "x", &times) }
    let run = |seed: u64| run_n(seed, times.clone());
    let base = run(7); let hs: Vec<_> = (0..4).map(|_| { let t2 = times.clone(); std::thread::spawn(move || run_n(7, t2)) }).collect(); for h in hs { assert_eq!(h.join().unwrap().iter().map(|x| x.to_bits()).collect::<Vec<_>>(), base.iter().map(|x| x.to_bits()).collect::<Vec<_>>(), "bit-identical across threads"); }
    assert_ne!(run(8), base, "seed changes the noise"); assert!(base.iter().zip(&times).filter(|(_, t)| **t < 0.5).all(|(v, _)| *v == 0.0), "zero before t0"); assert!(base.iter().any(|v| v.abs() > 0.1));
    // composition without a bake: noise_impulse then spring(mul) == noise * spring
    let mut ss = Session::new(); let id = one(&mut ss);
    expr(&mut ss, &id, "x", json!({"op": "noise_impulse", "seed": 7, "freq": 5.0, "amp": 12.0, "decay": 1.5, "octaves": 3, "t0": 0.5})); expr(&mut ss, &id, "x", json!({"op": "spring", "stiffness": 80.0, "damping": 5.0, "mass": 1.0, "from": 0.0, "to": 1.0, "combine": "mul"}));
    let both = vals(&mut ss, &id, "x", &times); let sp: Vec<f64> = times.iter().map(|t| spring_ref(80.0, 5.0, 1.0, 0.0, 1.0, *t)).collect();
    for i in 0..times.len() { assert!((both[i] - base[i] * sp[i]).abs() < 1e-9, "t={}: {} vs {}", times[i], both[i], base[i] * sp[i]); }
}
#[test] fn generators_bind_to_effect_params_camera_fields_and_unkeyed_props_and_error_clearly() {
    let mut ss = Session::new(); let id = one(&mut ss); let fx = c(&mut ss, "add_effect", json!({"clip_id": id, "kind": "blur"}))["effect_id"].as_str().unwrap().to_string();
    let pn = "radius".to_string();
    expr(&mut ss, &id, &pn, json!({"effect_id": fx, "op": "spring", "from": 0.0, "to": 5.0})); let r = c(&mut ss, "sample_property", json!({"clip_id": id, "property": pn, "effect_id": fx, "times": [0.0, 10.0], "space": "clip"})); assert_eq!(r["values"][0], 0.0); assert!((r["values"][1].as_f64().unwrap() - 5.0).abs() < 1e-6);
    ss.project.camera.insert("cam_x".into(), kimo_core::keyframe::Prop::new(0.0));
    c(&mut ss, "set_expression", json!({"camera": true, "property": "cam_x", "op": "bezier", "from": 2.0, "to": 8.0, "duration": 2.0})); let r = c(&mut ss, "sample_property", json!({"camera": true, "property": "cam_x", "times": [0.0, 2.0, 9.0]})); assert_eq!(r["values"], json!([2.0, 8.0, 8.0]));
    expr(&mut ss, &id, "opacity", json!({"op": "spring", "from": 3.0, "to": 3.0}));
    assert!(e(&mut ss, "set_expression", json!({"clip_id": id, "property": "x", "op": "spring", "mass": 0.0})).contains("mass"));
    assert!(e(&mut ss, "set_expression", json!({"clip_id": id, "property": "x", "op": "bezier", "duration": 0.0})).contains("duration"));
    assert!(e(&mut ss, "set_expression", json!({"clip_id": id, "property": "x", "op": "spring", "combine": "zzz"})).contains("Valid"));
    assert!(e(&mut ss, "set_expression", json!({"camera": true, "property": "nope", "op": "spring"})).contains("no camera field"));
    expr(&mut ss, &id, "path", json!({"op": "spring"})); // unkeyed standard prop is created from the renderer default
}
#[test] fn spring_in_render_and_sample_property_are_the_same_evaluator() {
    use kimo_core::render;
    let mut ss = Session::new(); let id = one(&mut ss); expr(&mut ss, &id, "scale", json!({"op": "spring", "stiffness": 60.0, "damping": 4.0, "from": 0.5, "to": 2.0}));
    let times = [0.0, 0.1, 0.3, 0.6, 1.0, 2.0]; let r = c(&mut ss, "sample_property", json!({"clip_id": id, "property": "scale", "times": times})); let cache = render::Cache::new();
    for (i, tm) in times.iter().enumerate() { let sc = r["values"][i].as_f64().unwrap(); let f = render::render_frame(&ss.project, *tm, &cache).unwrap(); let w = 160usize; let (mut x0, mut x1) = (usize::MAX, 0usize);
        for y in 0..90usize { for x in 0..w { if f.d[(y * w + x) * 4] > 0.5 { x0 = x0.min(x); x1 = x1.max(x); } } } let got = (x1 - x0 + 1) as f64; assert!((got - 20.0 * sc).abs() <= 1.0, "t={tm}: rendered {got} vs {}", 20.0 * sc); }
}

#[test] fn set_expression_echoes_the_effective_op_with_documented_defaults() {
    let mut ss = Session::new(); let id = one(&mut ss);
    let r = expr(&mut ss, &id, "x", json!({"op": "bezier"})); assert_eq!(r["applied"], json!({"op": "bezier", "x1": 0.25, "y1": 0.1, "x2": 0.25, "y2": 1.0, "from": 0.0, "to": 1.0, "t0": 0.0, "duration": 1.0, "combine": ""}));
    let r = expr(&mut ss, &id, "x", json!({"op": "spring"})); assert_eq!(r["applied"]["stiffness"], 100.0); assert_eq!(r["applied"]["damping"], 10.0); assert_eq!(r["applied"]["mass"], 1.0);
    assert!(c(&mut ss, "set_expression", json!({"clip_id": id, "property": "x", "op": "clear"}))["applied"].is_null());
}

// ---- A6: per-key tangent handles ----
use kimo_core::keyframe::{handles_to_bez, Ease, Handle, Key, Prop};
fn kk(t: f64, v: f64, out: Option<(f64, f64)>, inn: Option<(f64, f64)>) -> Key { Key { t, v, ease: Ease::Linear, bez: None, out: out.map(|(dt, dv)| Handle { dt, dv }), inn: inn.map(|(dt, dv)| Handle { dt, dv }) } }
fn prop2(a: Key, b: Key) -> Prop { let mut p = Prop::new(0.0); p.keys = vec![a, b]; p }
#[test] fn handles_round_trip_to_an_equivalent_bez_within_1e_12() {
    for (o, i, v0, v1) in [((0.4, 3.0), (0.7, 1.0), 2.0, 10.0), ((1.0, -4.0), (0.25, 6.0), -3.0, 5.0), ((0.1, 0.0), (0.1, 0.0), 0.0, 1.0)] {
        let (a, b) = (kk(1.0, v0, Some(o), None), kk(3.0, v1, None, Some(i))); assert!(handles_to_bez(&Key { out: None, ..a }, &b).is_none(), "needs both handles");
        let (a, b) = (kk(1.0, v0, Some(o), None), kk(3.0, v1, Some(o), Some(i))); let bz = handles_to_bez(&a, &b).unwrap();
        let hp = prop2(a, b); let mut bp = prop2(Key { out: None, inn: None, bez: Some(bz), ..a }, Key { out: None, inn: None, ..b });
        for n in 0..=100 { let t = 1.0 + 2.0 * n as f64 / 100.0; assert!((hp.at(t) - bp.at(t)).abs() < 1e-12, "t={t}: {} vs {}", hp.at(t), bp.at(t)); } bp.keys.clear();
    }
}
#[test] fn handles_overshoot_and_flat_segments_can_bump() {
    let hp = prop2(kk(0.0, 0.0, Some((0.5, 9.0)), None), kk(1.0, 1.0, None, Some((0.5, -9.0)))); let m = (0..=100).map(|n| hp.at(n as f64 / 100.0)).fold(f64::MIN, f64::max); assert!(m > 1.5, "overshoot preserved, got {m}");
    let flat = prop2(kk(0.0, 5.0, Some((0.3, 4.0)), None), kk(1.0, 5.0, None, Some((0.3, 0.0)))); assert!(flat.at(0.4) > 5.5, "a flat segment with a handle bumps: {}", flat.at(0.4)); assert_eq!(flat.at(0.0), 5.0); assert_eq!(flat.at(1.0), 5.0);
}
#[test] fn handles_win_over_bez_and_mirrored_handles_are_c1() {
    let mut a = kk(0.0, 0.0, Some((0.4, 2.0)), None); a.bez = Some([0.9, 0.0, 0.9, 1.0]); let b = kk(1.0, 1.0, None, Some((0.4, 0.5))); let with = prop2(a, b); let mut a2 = a; a2.bez = None; assert_eq!(with.at(0.37), prop2(a2, b).at(0.37), "handles win");
    // mirrored handles at the middle key: equal slope dv/dt on both sides
    let (k0, k1, k2) = (kk(0.0, 0.0, Some((0.3, 0.3)), None), kk(1.0, 2.0, Some((0.4, 1.2)), Some((0.4, 1.2))), kk(2.5, 0.5, None, Some((0.5, 0.1)))); let mut p = Prop::new(0.0); p.keys = vec![k0, k1, k2];
    let (sl, sr) = (k1.inn.unwrap().dv / k1.inn.unwrap().dt, k1.out.unwrap().dv / k1.out.unwrap().dt); assert!((sl - sr).abs() < 1e-9, "analytic handle slopes match");
    let h = 1e-5; let (dl, dr) = ((p.at(1.0) - p.at(1.0 - h)) / h, (p.at(1.0 + h) - p.at(1.0)) / h); assert!((dl - sl).abs() < 1e-3 && (dr - sr).abs() < 1e-3, "sampled slopes {dl} {dr} vs {sl}");
}
#[test] fn handle_tools_set_get_sample_bake_and_errors() {
    let (mut ss, id) = scene(); keys_on(&mut ss, &id, "x", json!([{"t": 0.0, "v": 0.0, "out": {"dt": 0.5, "dv": 8.0}}, {"t": 2.0, "v": 4.0, "in": {"dt": 0.5, "dv": -8.0}}]));
    let r = c(&mut ss, "get_curve", json!({"clip_id": id, "property": "x"})); assert_eq!(r["keys"][0]["out"], json!({"dt": 0.5, "dv": 8.0})); assert_eq!(r["keys"][1]["in"], json!({"dt": 0.5, "dv": -8.0})); assert!(r["keys"][0].get("in").is_none());
    let want = ss.project.find_clip(&id).unwrap().props["x"].at(0.7); let s = c(&mut ss, "sample_property", json!({"clip_id": id, "property": "x", "times": [0.7], "space": "clip"})); assert_eq!(s["values"][0].as_f64().unwrap(), want);
    let before = pj(&ss); c(&mut ss, "bake_property", json!({"clip_id": id, "property": "x", "start": 0.0, "end": 2.0, "n": 9})); let b = c(&mut ss, "sample_property", json!({"clip_id": id, "property": "x", "times": [0.7], "space": "clip"})); assert!((b["values"][0].as_f64().unwrap() - want).abs() < 0.2); let _ = before;
    for bad in [json!({"t": 0.0, "v": 1.0, "out": {"dt": -1.0, "dv": 0.0}}), json!({"t": 0.0, "v": 1.0, "out": {"dt": 1.0}}), json!({"t": 0.0, "v": 1.0, "in": "x"}), json!({"t": 0.0, "v": 1.0, "in": {"dt": "NaN", "dv": 0.0}})] { let h = pj(&ss); assert!(!e(&mut ss, "set_keyframes", json!({"clip_id": id, "property": "x", "keys": [bad]})).is_empty()); assert_eq!(pj(&ss), h); }
    // old shape: keys without handles serialize with no in/out fields
    let (mut s2, i2) = scene(); keys_on(&mut s2, &i2, "x", json!([{"t": 0.0, "v": 0.0}, {"t": 1.0, "v": 1.0}])); let j = pj(&s2); assert!(!j.contains("\"in\"") && !j.contains("\"out\""), "{j}");
}
