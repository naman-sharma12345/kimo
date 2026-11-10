//! P0: fill alpha, stroke alpha and layer opacity are three separate animatable properties. A hollow ring (fill alpha 0, stroke on) must show its stroke.
//! Fails first on the old code, where add_shape folded the fill alpha into the layer opacity and hid the stroke.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn scene(fill: Value, stroke: Value) -> (Session, Value) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let cl = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "width": 100.0, "height": 60.0, "fill": fill, "stroke_width": 10.0, "stroke_color": stroke, "start": 0, "duration": 2}))["clip_id"].clone(); (s, cl) }
fn px(s: &Session, t: f64, x: usize, y: usize) -> [f32; 4] { let f = render_frame(&s.project, t, &Cache::new()).unwrap(); let i = (y * f.w + x) * 4; [f.d[i], f.d[i + 1], f.d[i + 2], f.d[i + 3]] }
const EDGE: (usize, usize) = (110, 90); // on the left edge of the rect: stroke centre
const MID: (usize, usize) = (160, 90);
#[test] fn hollow_ring_shows_its_stroke() {
    let (s, _) = scene(json!([1.0, 0.0, 0.0, 0.0]), json!([0.0, 1.0, 0.0]));
    let e = px(&s, 0.5, EDGE.0, EDGE.1); let m = px(&s, 0.5, MID.0, MID.1);
    assert!(e[1] > 0.95 && e[0] < 0.05, "stroke must be solid green on a hollow ring, got {e:?}");
    assert!(m[0] < 0.01 && m[1] < 0.01, "hollow centre must show the (black) background, got {m:?}");
}
#[test] fn fill_stroke_and_opacity_compose_independently() {
    let (mut s, cl) = scene(json!([1.0, 0.0, 0.0, 0.5]), json!([0.0, 1.0, 0.0, 0.25]));
    let (e, m) = (px(&s, 0.5, EDGE.0, EDGE.1), px(&s, 0.5, MID.0, MID.1));
    assert!((m[0] - 0.5).abs() < 0.02, "fill alpha 0.5 over black: red {m:?}"); assert!((e[1] - 0.25).abs() < 0.03, "stroke alpha 0.25 over black: green {e:?}");
    c(&mut s, "set_property", json!({"clip_id": cl, "property": "opacity", "value": 0.5}));
    let (e2, m2) = (px(&s, 0.5, EDGE.0, EDGE.1), px(&s, 0.5, MID.0, MID.1));
    assert!((m2[0] - 0.25).abs() < 0.02, "layer opacity 0.5 x fill alpha 0.5 = 0.25: {m2:?}"); assert!((e2[1] - 0.125).abs() < 0.03, "layer opacity 0.5 x stroke alpha 0.25 = 0.125: {e2:?}");
    assert_eq!(c(&mut s, "get_project", json!({})).to_string().contains("fill_alpha"), true, "fill_alpha is a stored property");
}
#[test] fn fill_alpha_animates() {
    let (mut s, cl) = scene(json!([1.0, 0.0, 0.0, 1.0]), json!([0.0, 1.0, 0.0]));
    c(&mut s, "set_keyframe", json!({"clip_id": cl, "property": "fill_alpha", "time": 0.0, "value": 0.0})); c(&mut s, "set_keyframe", json!({"clip_id": cl, "property": "fill_alpha", "time": 1.0, "value": 1.0}));
    let (a, b, d) = (px(&s, 0.0, MID.0, MID.1), px(&s, 0.5, MID.0, MID.1), px(&s, 1.0, MID.0, MID.1));
    assert!(a[0] < 0.02 && (b[0] - 0.5).abs() < 0.06 && d[0] > 0.98, "fill_alpha 0 -> 1 keyframes: {a:?} {b:?} {d:?}");
    let e = px(&s, 0.0, EDGE.0, EDGE.1); assert!(e[1] > 0.95, "stroke stays solid while the fill fades in: {e:?}");
}
#[test] fn no_alpha_properties_means_old_behaviour() {
    // a project saved before this change has neither property; it must render exactly as an opaque shape
    let (s, _) = scene(json!([1.0, 0.0, 0.0]), json!([0.0, 1.0, 0.0])); assert!(!serde_json::to_string(&s.project).unwrap().contains("fill_alpha"), "opaque shapes do not store the new properties");
    let m = px(&s, 0.5, MID.0, MID.1); assert!(m[0] > 0.99);
}
#[test] fn strokeless_fill_alpha_matches_old_layer_opacity() {
    // Old code folded the fill alpha into layer opacity, so for a stroke-less ellipse "fill alpha 0.8" must equal "layer opacity 0.8, fill alpha 1" (proves stroke-less equivalence only).
    let mk = |alpha_in_fill: bool| { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 24}));
        let t0 = c(&mut s, "add_track", json!({}))["track_id"].clone(); c(&mut s, "add_shape", json!({"track_id": t0, "shape": "rect", "width": 120, "height": 70, "fill": [0.9, 0.3, 0.2, 1.0], "start": 0, "duration": 2}));
        let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
        let fill = if alpha_in_fill { json!([0.2, 0.8, 0.4, 0.8]) } else { json!([0.2, 0.8, 0.4, 1.0]) };
        let cl = c(&mut s, "add_shape", json!({"track_id": t, "shape": "ellipse", "width": 90, "height": 90, "fill": fill, "start": 0, "duration": 2}))["clip_id"].clone();
        if !alpha_in_fill { c(&mut s, "set_property", json!({"clip_id": cl, "property": "opacity", "value": 0.8})); } s };
    let (a, b) = (mk(true), mk(false)); let mut worst = 0f32;
    for t in [0.1, 0.5, 0.9] { let (fa, fb) = (render_frame(&a.project, t, &Cache::new()).unwrap(), render_frame(&b.project, t, &Cache::new()).unwrap()); for (x, y) in fa.d.iter().zip(fb.d.iter()) { worst = worst.max((x - y).abs()); } }
    eprintln!("max f32 diff (fill_alpha 0.8 vs layer opacity 0.8, stroke-less ellipse over a rect) = {worst:e}");
    assert!(worst <= 1.0 / 255.0, "max f32 diff {worst}");
}
