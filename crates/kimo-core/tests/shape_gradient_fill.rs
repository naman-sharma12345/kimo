//! Gradient Fill on a shape layer (FEATURES.md L236 "shape layer operators: ... gradient fill ..."): the
//! existing Gradient Stroke modifier (shapemods.rs) blends colour along the stroke; this is its sibling for
//! the shape's own fill, same linear blend, independent colour pair, so a shape can carry a flat stroke over
//! a gradient fill (or vice versa) at once.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::render_frame;
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).unwrap_err() }
const W: usize = 40;
const H: usize = 40;

fn rect_project() -> (Session, String) {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "start": 0.0, "duration": 2.0, "width": 36.0, "height": 36.0, "fill": [1.0, 1.0, 1.0], "stroke_width": 0.0}))["clip_id"].as_str().unwrap().to_string();
    (s, id)
}
fn frame(s: &Session, t: f64) -> Vec<f32> { render_frame(&s.project, t, &s.cache).unwrap().d }
fn px(d: &[f32], x: usize, y: usize) -> [f32; 3] { let i = (y * W + x) * 4; [d[i], d[i + 1], d[i + 2]] }

#[test]
fn gradient_fill_blends_left_to_right_at_angle_90() {
    let (mut s, id) = rect_project();
    c(&mut s, "add_shape_modifier", json!({"clip_id": id, "kind": "gradient_fill", "params": {"r1": 1.0, "g1": 0.0, "b1": 0.0, "r2": 0.0, "g2": 0.0, "b2": 1.0, "angle": 0.0}}));
    let d = frame(&s, 0.0);
    let left = px(&d, 2, H / 2); let right = px(&d, W - 3, H / 2);
    assert!(left[0] > 0.7 && left[2] < 0.3, "left edge should be mostly red: {left:?}");
    assert!(right[2] > 0.7 && right[0] < 0.3, "right edge should be mostly blue: {right:?}");
}

#[test]
fn gradient_fill_leaves_a_flat_stroke_untouched() {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "start": 0.0, "duration": 2.0, "width": 36.0, "height": 36.0, "fill": [1.0, 1.0, 1.0], "stroke_width": 6.0, "stroke_color": [0.0, 1.0, 0.0]}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "add_shape_modifier", json!({"clip_id": id, "kind": "gradient_fill", "params": {"r1": 1.0, "g1": 0.0, "b1": 0.0, "r2": 0.0, "g2": 0.0, "b2": 1.0, "angle": 0.0}}));
    let d = frame(&s, 0.0);
    // the stroke ring sits near the rect's left edge (x=2..~5, centered square 36px in a 40px canvas): still flat green, not gradient-tinted
    let stroke_px = px(&d, 3, H / 2);
    assert!(stroke_px[1] > 0.7 && stroke_px[0] < 0.2 && stroke_px[2] < 0.2, "stroke must stay flat green, unaffected by gradient_fill: {stroke_px:?}");
}

#[test]
fn gradient_stroke_and_gradient_fill_are_independent() {
    let (mut s, id) = rect_project();
    c(&mut s, "set_property", json!({"clip_id": id, "property": "fill_alpha", "value": 1.0}));
    // give the rect a visible stroke so there's something to put a gradient_stroke on
    let (mut s2, id2) = {
        let mut s = Session::new();
        c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 25}));
        let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
        let id = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "start": 0.0, "duration": 2.0, "width": 36.0, "height": 36.0, "fill": [1.0, 1.0, 1.0], "stroke_width": 6.0, "stroke_color": [1.0, 1.0, 1.0]}))["clip_id"].as_str().unwrap().to_string();
        (s, id)
    };
    let _ = &mut s; let _ = &id; // keep the first session alive for symmetry with other tests in this file
    c(&mut s2, "add_shape_modifier", json!({"clip_id": id2, "kind": "gradient_stroke", "params": {"r1": 1.0, "g1": 0.0, "b1": 0.0, "r2": 0.0, "g2": 0.0, "b2": 1.0, "angle": 0.0}}));
    c(&mut s2, "add_shape_modifier", json!({"clip_id": id2, "kind": "gradient_fill", "params": {"r1": 0.0, "g1": 1.0, "b1": 0.0, "r2": 0.0, "g2": 1.0, "b2": 0.0, "angle": 0.0}}));
    let d = frame(&s2, 0.0);
    let fill_px = px(&d, W / 2, H / 2); // center: fill only, no stroke
    assert!(fill_px[1] > 0.7 && fill_px[0] < 0.2 && fill_px[2] < 0.2, "fill center should be the flat gradient_fill green (r1==r2,b1==b2 here): {fill_px:?}");
    let stroke_left = px(&d, 3, H / 2); // left edge: stroke ring, should read gradient_stroke's red end, not green
    assert!(stroke_left[0] > 0.6 && stroke_left[1] < 0.3, "left stroke should read gradient_stroke's red end, independent of gradient_fill: {stroke_left:?}");
}

#[test]
fn gradient_fill_rejects_an_unrelated_param_the_same_way_gradient_stroke_does() {
    let (mut s, id) = rect_project();
    let e = err(&mut s, "add_shape_modifier", json!({"clip_id": id, "kind": "gradient_fill", "params": {"copies": 3.0}}));
    assert!(e.contains("copies"), "{e}");
}

#[test]
fn gradient_fill_on_a_clip_with_no_fill_set_is_a_no_op_rendering() {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    // fill alpha 0 via a negative-sentinel "no fill": pass fill color then force fill_alpha 0 so nothing draws from fill.
    let id = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "start": 0.0, "duration": 2.0, "width": 10.0, "height": 10.0, "fill": [1.0, 1.0, 1.0, 0.0], "stroke_width": 0.0}))["clip_id"].as_str().unwrap().to_string();
    let before = render_frame(&s.project, 0.0, &s.cache).unwrap().d;
    c(&mut s, "add_shape_modifier", json!({"clip_id": id, "kind": "gradient_fill", "params": {"r1": 1.0, "g1": 0.0, "b1": 0.0, "r2": 0.0, "g2": 0.0, "b2": 1.0, "angle": 0.0}}));
    let after = render_frame(&s.project, 0.0, &s.cache).unwrap().d;
    assert_eq!(before, after, "a fully transparent fill must stay invisible regardless of gradient_fill");
}
