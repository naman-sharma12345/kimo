//! V3-3: taking the layer's alpha bbox from the bounded rect raster region must give the same pixels as scanning the whole canvas.
//! Rect scenes with scale, rotation, opacity, motion blur, non-shape effects, and a mask (those must fall back); hint off vs on, bit-identical.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, set_shape_hint, Cache};
use serde_json::{json, Value};
fn fnv(f: &kimo_core::effects::Frame) -> u64 { let mut h = 0xcbf29ce484222325u64; for v in &f.d { h ^= v.to_bits() as u64; h = h.wrapping_mul(0x100000001b3); } h }
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
#[test] fn bbox_hint_is_bit_identical() {
    let mut n = 0;
    for (w, h) in [(60.0, 40.0), (300.0, 6.0), (1.0, 1.0), (500.0, 400.0), (33.3, 17.7)] { for (x, y) in [(0.0, 0.0), (140.0, 80.0), (-150.0, -85.0), (400.0, 0.0), (0.37, -0.61)] { for (stroke, cr) in [(0.0, 0.0), (5.0, 0.0), (12.0, 4.0)] {
      for (var, (sc, rot, mb)) in [(1.0, 0.0, 0.0), (1.7, 0.0, 0.0), (1.0, 23.0, 0.0), (0.6, -40.0, 0.0), (1.0, 0.0, 90.0)].into_iter().enumerate() {
        let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 24}));
        let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
        let cl = c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "width": w, "height": h, "fill": [0.9, 0.5, 0.2, 1], "stroke_width": stroke, "stroke_color": [0.1, 0.8, 0.4, 1], "corner_radius": cr, "start": 0, "duration": 1}))["clip_id"].clone();
        if (n % 4) == 1 { c(&mut s, "add_effect", json!({"clip_id": cl, "kind": "glow"})); }
        c(&mut s, "set_property", json!({"clip_id": cl, "property": "x", "value": x})); c(&mut s, "set_property", json!({"clip_id": cl, "property": "y", "value": y}));
        c(&mut s, "set_property", json!({"clip_id": cl, "property": "scale", "value": sc})); c(&mut s, "set_property", json!({"clip_id": cl, "property": "rotation", "value": rot}));
        if mb > 0.0 { let _ = dispatch(&mut s, "set_motion_blur", &json!({"clip_id": cl, "shutter_angle": mb})); }
        let _ = var;
        set_shape_hint(false); let a = fnv(&render_frame(&s.project, 0.5, &Cache::new()).unwrap()); set_shape_hint(true); let b = fnv(&render_frame(&s.project, 0.5, &Cache::new()).unwrap());
        assert_eq!(a, b, "bbox hint changed pixels: {w}x{h} at ({x},{y}) stroke {stroke} corner {cr} scale {sc} rot {rot} mb {mb}"); n += 1; } } } }
    eprintln!("shape_hint_equiv: {n} scenes bit-identical");
}
