//! RV-28 C2: compute_text_bounds equals a real render's drawn extent (read-only).
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn bbox(ss: &Session) -> [f64; 4] { let fr = render::render_frame(&ss.project, 1.0, &render::Cache::new()).unwrap(); let (w, h) = (fr.w, fr.h); let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
    for y in 0..h { for x in 0..w { let i = (y * w + x) * 4; if fr.d[i] + fr.d[i + 1] + fr.d[i + 2] > 0.06 { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x + 1); y1 = y1.max(y + 1); } } } [x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64] }
fn scene(text: &str, font: &str, size: f64, stroke: f64, extra: Value) -> (Session, String) {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 1600, "height": 500, "fps": 25})); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let mut a = json!({"track_id": t, "text": text, "font": font, "size": size, "color": [1, 1, 1], "stroke_width": stroke, "stroke_color": [1, 1, 1], "start": 0.0, "duration": 3.0}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); }
    let id = c(&mut ss, "add_text", a)["clip_id"].as_str().unwrap().to_string(); (ss, id) }
#[test] fn clip_bounds_equal_the_rendered_pixels_within_1px() {
    for font in ["Montserrat", "DejaVuSans", "Inter", "Roboto", "Lato"] { for size in [32.0, 64.0, 96.0] { for (text, stroke) in [("Hello world", 0.0), ("Two\nlines here", 0.0), ("Outlined", 4.0)] {
        let (mut ss, id) = scene(text, font, size, stroke, json!({})); let before = pj(&ss);
        let r = c(&mut ss, "compute_text_bounds", json!({"clip_id": id, "t": 1.0})); assert_eq!(pj(&ss), before, "read-only");
        let want = bbox(&ss); let got: Vec<f64> = r["canvas_rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        for k in 0..4 { assert!((got[k] - want[k]).abs() <= (if stroke > 0.0 { 2.0 } else { 1.0 }) + if k >= 2 && stroke > 0.0 { 1.0 } else { 0.0 }, "{font} {size} {text:?} stroke {stroke}: tool {got:?} render {want:?}"); }
    } } }
}
#[test] fn moved_clip_and_standalone_agree_and_unknown_font_warns() {
    let (mut ss, id) = scene("Offset me", "Montserrat", 60.0, 0.0, json!({"x": 120.0})); c(&mut ss, "set_property", json!({"clip_id": id, "property": "y", "value": -40.0}));
    let a = c(&mut ss, "compute_text_bounds", json!({"clip_id": id, "t": 1.0})); let b = c(&mut ss, "compute_text_bounds", json!({"text": "Offset me", "font": "Montserrat", "size": 60.0, "x": 120.0, "y": -40.0}));
    for k in 0..4 { assert!((a["canvas_rect"][k].as_f64().unwrap() - b["canvas_rect"][k].as_f64().unwrap()).abs() < 1e-6, "{a} {b}"); }
    assert_eq!(b, c(&mut ss, "compute_text_bounds", json!({"text": "Offset me", "font": "Montserrat", "size": 60.0, "x": 120.0, "y": -40.0})), "deterministic");
    let w = c(&mut ss, "compute_text_bounds", json!({"text": "x", "font": "NoSuchFamilyZZ", "size": 20.0})); assert_eq!(w["font"]["substituted"], true); assert!(w["warnings"][0].as_str().unwrap().starts_with("font_substituted"));
    assert!(e(&mut ss, "compute_text_bounds", json!({"text": "x", "size": -1.0})).contains("out_of_range")); assert!(e(&mut ss, "compute_text_bounds", json!({"text": "x", "size": 20.0, "align": "up"})).contains("Valid:"));
}
