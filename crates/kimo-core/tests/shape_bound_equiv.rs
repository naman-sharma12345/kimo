//! V3-3: rasterizing a rect/ellipse only inside its bounding box (+ margin) must be bit-identical to rasterizing the whole canvas. Sizes, aspect ratios,
//! positions (including partly/fully off-canvas), corner radius, strokes and anti-aliased fractional offsets. Each scene is rendered with the bound off and on.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, set_shape_bound, Cache};
use serde_json::{json, Value};
fn fnv(f: &kimo_core::effects::Frame) -> u64 { let mut h = 0xcbf29ce484222325u64; for v in &f.d { h ^= v.to_bits() as u64; h = h.wrapping_mul(0x100000001b3); } h }
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
#[test] fn bounded_shape_raster_is_bit_identical() {
    let mut n = 0;
    for shape in ["rect", "ellipse"] { for (w, h) in [(60.0, 40.0), (300.0, 6.0), (4.0, 170.0), (1.0, 1.0), (500.0, 400.0), (33.3, 17.7)] { for (x, y) in [(0.0, 0.0), (140.0, 80.0), (-150.0, -85.0), (400.0, 0.0), (0.37, -0.61)] { for (stroke, cr) in [(0.0, 0.0), (5.0, 0.0), (0.0, 8.0), (12.0, 4.0)] {
        let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 24}));
        let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
        let cl = c(&mut s, "add_shape", json!({"track_id": t, "shape": shape, "width": w, "height": h, "fill": [0.9, 0.5, 0.2, 1], "stroke_width": stroke, "stroke_color": [0.1, 0.8, 0.4, 1], "corner_radius": cr, "start": 0, "duration": 1}))["clip_id"].clone();
        if (n % 3) == 1 { c(&mut s, "add_effect", json!({"clip_id": cl, "kind": "glow"})); } // non-shape effects must not disable (or change) the bounded raster
        c(&mut s, "set_property", json!({"clip_id": cl, "property": "x", "value": x})); c(&mut s, "set_property", json!({"clip_id": cl, "property": "y", "value": y}));
        set_shape_bound(false); let a = fnv(&render_frame(&s.project, 0.5, &Cache::new()).unwrap()); set_shape_bound(true); let b = fnv(&render_frame(&s.project, 0.5, &Cache::new()).unwrap());
        assert_eq!(a, b, "bounded raster changed pixels: {shape} {w}x{h} at ({x},{y}) stroke {stroke} corner {cr}"); n += 1; } } } }
    eprintln!("shape_bound_equiv: {n} scenes bit-identical");
}
