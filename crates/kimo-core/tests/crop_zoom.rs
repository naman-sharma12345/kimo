//! V1c-B crop_zoom_keyframes: the window at t=0/mid/end equals the analytic from/to/eased interpolation, aspect rule, clamp, determinism, apply == ops by hand.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::curves::{ease_at, xform_to_window};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene() -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":256,"height":256,"fps":24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[0.2,0.5,0.8],"start":0,"duration":4}))["clip_id"].as_str().unwrap().to_string(); (s, id)
}
fn win_at(s: &Session, id: &str, t: f64) -> [f64; 4] { let cl = s.project.find_clip(id).unwrap(); let g = |k: &str| cl.props[k].at(t); xform_to_window(g("scale"), g("x"), g("y"), 256., 256.) }
fn near(a: [f64; 4], b: [f64; 4], what: &str) { for i in 0..4 { assert!((a[i] - b[i]).abs() < 1e-6, "{what}: {a:?} vs {b:?}"); } }
const FROM: [f64; 4] = [0., 0., 256., 256.]; const TO: [f64; 4] = [64., 96., 64., 64.];
fn lerp(e: f64) -> [f64; 4] { [0, 1, 2, 3].map(|i| FROM[i] + (TO[i] - FROM[i]) * e) }
fn zoom(s: &mut Session, id: &str, easing: Value) -> Value { c(s, "crop_zoom_keyframes", json!({"clip_id":id,"from_box":FROM,"to_box":TO,"duration":1.0,"easing":easing})) }
#[test] fn window_at_start_mid_end_equals_the_analytic_interpolation() {
    let (mut s, id) = scene(); let r = zoom(&mut s, &id, json!("linear"));
    near(win_at(&s, &id, 0.), FROM, "t=0"); near(win_at(&s, &id, 0.5), lerp(0.5), "mid"); near(win_at(&s, &id, 1.0), TO, "end");
    near(win_at(&s, &id, 0.25), lerp(0.25), "quarter");
    let e = r["effective_box_at"].as_array().unwrap(); assert_eq!(e.len(), 3); for (i, w) in [FROM, lerp(0.5), TO].iter().enumerate() { let b: Vec<f64> = e[i]["box"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect(); near([b[0], b[1], b[2], b[3]], *w, "reported"); }
    assert_eq!(r["clamped"], false); assert!(r["fix_hint"].is_null());
}
#[test] fn in_out_is_monotonic_and_matches_the_eased_interpolation() {
    let (mut s, id) = scene(); zoom(&mut s, &id, json!("in_out"));
    let mut prev = 0.; for i in 0..=24 { let t = i as f64 / 24.; let w = win_at(&s, &id, t); let e = ease_at(&json!("in_out"), t).unwrap(); near(w, lerp(e), &format!("frame {i}")); assert!(w[2] <= 256. + 1e-9); let z = 256. / w[2]; assert!(z >= prev - 1e-12, "zoom monotonic"); prev = z; }
}
/// Independent CSS cubic-bezier reference (bisection on x(t), then y) to cross-check the shared evaluator.
fn css(b: [f64; 4], x: f64) -> f64 { let f = |t: f64, a: f64, c2: f64| 3. * (1. - t).powi(2) * t * a + 3. * (1. - t) * t * t * c2 + t.powi(3); let (mut lo, mut hi) = (0., 1.); for _ in 0..80 { let m = (lo + hi) / 2.; if f(m, b[0], b[2]) < x { lo = m } else { hi = m } } f((lo + hi) / 2., b[1], b[3]) }
#[test] fn cubic_bezier_matches_the_reference_table() {
    let b = [0.25, 0.1, 0.25, 1.0]; let (mut s, id) = scene(); zoom(&mut s, &id, json!(b));
    for i in 0..=24 { let u = i as f64 / 24.; near(win_at(&s, &id, u), lerp(css(b, u)), &format!("bezier frame {i}")); }
    assert!((css(b, 0.5) - 0.8024).abs() < 1e-3, "reference sanity: CSS ease at 0.5 is about 0.802");

}
#[test] fn bad_bezier_x_is_rejected() { let (mut s, id) = scene(); let e = dispatch(&mut s, "crop_zoom_keyframes", &json!({"clip_id":id,"from_box":FROM,"to_box":TO,"duration":1.0,"easing":[1.5,0,0.5,1]})).unwrap_err(); assert!(e.starts_with("invalid_ease"), "{e}"); }
#[test] fn different_aspect_target_pads_and_is_reported() {
    let (mut s, id) = scene(); let r = c(&mut s, "crop_zoom_keyframes", json!({"clip_id":id,"from_box":FROM,"to_box":[64.,100.,128.,64.],"duration":1.0,"easing":"linear","clamp":false}));
    near(win_at(&s, &id, 1.0), [64., 68., 128., 128.], "grown to canvas aspect around the centre"); let h = &r["fix_hint"]; assert_eq!(h["kind"], "none"); assert!(h["why"].as_str().unwrap().contains("pad"), "{h}"); assert!(h["why"].as_str().unwrap().contains("2.000"));
}
#[test] fn source_bounds_clamp_keeps_the_window_inside() {
    let (mut s, id) = scene(); let r = c(&mut s, "crop_zoom_keyframes", json!({"clip_id":id,"from_box":FROM,"to_box":[200.,200.,100.,100.],"duration":1.0,"easing":"linear"}));
    let w = win_at(&s, &id, 1.0); near(w, [156., 156., 100., 100.], "clamped end"); assert_eq!(r["clamped"], true);
    for i in 0..=24 { let w = win_at(&s, &id, i as f64 / 24.); assert!(w[0] >= -1e-9 && w[1] >= -1e-9 && w[0] + w[2] <= 256. + 1e-9 && w[1] + w[3] <= 256. + 1e-9, "frame {i} {w:?}"); }
    let (mut s, id) = scene(); let r = c(&mut s, "crop_zoom_keyframes", json!({"clip_id":id,"from_box":FROM,"to_box":[200.,200.,100.,100.],"duration":1.0,"easing":"linear","clamp":false})); near(win_at(&s, &id, 1.0), [200., 200., 100., 100.], "unclamped end"); assert_eq!(r["clamped"], false);
}
fn frame_hash(s: &mut Session, t: f64) -> u64 { use std::hash::{Hash, Hasher}; kimo_core::render::prepare_at(&s.project, &mut s.cache, Some(t)).unwrap(); let mut f = kimo_core::render::render_frame(&s.project, t, &s.cache).unwrap(); kimo_core::color::pipeline::output(&s.project, &mut f).unwrap(); let mut h = std::collections::hash_map::DefaultHasher::new(); for v in &f.d { v.to_bits().hash(&mut h); } h.finish() }
fn gscene() -> (Session, String) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width":256,"height":256,"fps":24})); let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[0,0,0],"color2":[1,0.5,0.2],"start":0,"duration":4}))["clip_id"].as_str().unwrap().to_string(); (s, id) }
#[test] fn frame_is_bit_equal_twice_and_across_threads_and_actually_zooms() {
    let (mut s, id) = gscene(); let before = frame_hash(&mut s, 0.5); zoom(&mut s, &id, json!("in_out"));
    let a = frame_hash(&mut s, 0.5); let b = frame_hash(&mut s, 0.5); assert_eq!(a, b, "same frame twice"); assert_ne!(a, before, "the zoom changes the frame");
    kimo_core::par::set_threads(1); let t1 = frame_hash(&mut s, 0.5); kimo_core::par::set_threads(4); let t4 = frame_hash(&mut s, 0.5); kimo_core::par::set_threads(0); assert_eq!(t1, t4, "1 vs 4 threads"); assert_eq!(t1, a);
}
#[test] fn apply_false_returns_ops_and_apply_true_equals_applying_them_by_hand() {
    let args = |ap: bool| json!({"clip_id":"clip2","from_box":FROM,"to_box":TO,"duration":1.0,"easing":"out","apply":ap});
    let (mut s1, id) = scene(); assert_eq!(id, "clip2"); let proj0 = serde_json::to_string(&s1.project).unwrap();
    let r = c(&mut s1, "crop_zoom_keyframes", args(false)); assert_eq!(r["applied"], false); assert_eq!(serde_json::to_string(&s1.project).unwrap(), proj0, "apply:false does not touch the project");
    for op in r["ops"].as_array().unwrap() { c(&mut s1, op["tool"].as_str().unwrap(), op["args"].clone()); }
    let (mut s2, _) = scene(); c(&mut s2, "crop_zoom_keyframes", args(true));
    assert_eq!(serde_json::to_string(&s1.project.tracks).unwrap(), serde_json::to_string(&s2.project.tracks).unwrap(), "apply:true == ops by hand");
    c(&mut s2, "undo", json!({})); assert_eq!(serde_json::to_string(&s2.project).unwrap(), proj0, "one undo step restores the project");
}
#[test] fn ui_zoom_returns_the_expansion_and_apply_equals_it_by_hand() {
    let a = |ap: bool| json!({"clip_id":"clip2","target_box":TO,"duration":1.0,"hold":0.5,"easing":"in_out","apply":ap});
    let (mut s1, _) = scene(); let proj0 = serde_json::to_string(&s1.project).unwrap(); let r = c(&mut s1, "ui_zoom", a(false)); assert_eq!(r["applied"], false); assert_eq!(serde_json::to_string(&s1.project).unwrap(), proj0, "default changes nothing");
    let ops = r["expansion"].as_array().unwrap().clone(); assert_eq!(ops.len(), 2); assert!(ops.iter().all(|o| o["tool"] == "crop_zoom_keyframes")); assert_eq!(r["params_used"]["full_box"], json!([0, 0, 256, 256]));
    for op in &ops { c(&mut s1, op["tool"].as_str().unwrap(), op["args"].clone()); }
    let (mut s2, id) = scene(); c(&mut s2, "ui_zoom", a(true));
    assert_eq!(serde_json::to_string(&s1.project.tracks).unwrap(), serde_json::to_string(&s2.project.tracks).unwrap(), "apply:true == expansion by hand");
    near(win_at(&s2, &id, 0.), FROM, "start full frame"); near(win_at(&s2, &id, 1.0), TO, "at target"); near(win_at(&s2, &id, 1.25), TO, "hold"); near(win_at(&s2, &id, 1.5), TO, "hold end"); near(win_at(&s2, &id, 2.5), FROM, "back to full");
    c(&mut s2, "undo", json!({})); assert_eq!(serde_json::to_string(&s2.project).unwrap(), proj0, "one undo step");
}
#[test] fn bad_arguments_fail_with_typed_errors_before_any_key_is_written() {
    let (mut s, _) = scene(); let proj0 = serde_json::to_string(&s.project).unwrap();
    for (tool, args, code) in [("ui_zoom", json!({"clip_id":"clip2","target_box":[0,0,-5,10],"duration":1.0}), "invalid_box"), ("ui_zoom", json!({"clip_id":"clip2","target_box":TO,"duration":0}), "out_of_range"), ("ui_zoom", json!({"clip_id":"clip2","target_box":TO,"duration":1.0,"easing":"wobble"}), "invalid_ease"),
        ("ui_zoom", json!({"clip_id":"clip2","target_box":TO,"duration":1.0,"hold":-1}), "out_of_range"), ("crop_zoom_keyframes", json!({"clip_id":"clip2","from_box":FROM,"to_box":TO,"duration":100.0}), "limit_exceeded"), ("ui_zoom", json!({"clip_id":"clip2","target_box":TO,"duration":1.0,"easing":"in_out","apply":true,"back_duration":0}), "out_of_range")] {
        let e = dispatch(&mut s, tool, &args).unwrap_err(); assert!(e.starts_with(code), "{tool}: {e}"); assert_eq!(serde_json::to_string(&s.project).unwrap(), proj0, "{tool}: nothing written"); }
}
