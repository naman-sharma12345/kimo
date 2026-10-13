//! An open path (no Z) must stroke only the drawn segments; the implicit closing chord belongs to the fill, never the stroke (found by the bg_wave visual pass).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn alpha_at(extra: Value, x: usize, y: usize) -> f32 {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width":256,"height":256,"fps":25})).unwrap(); let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let mut a = json!({"track_id":t,"start":0,"duration":2,"stroke_width":4,"stroke_color":[1,1,1],"fill":[-1,0,0]}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); }
    dispatch(&mut s, "add_path_shape", &a).unwrap(); let f = kimo_core::render::render_frame(&s.project, 0.0, &kimo_core::render::Cache::new()).unwrap(); f.d[(y * 256 + x) * 4]
}
// canvas centre is (128,128): V shape (-60,0) (0,-60) (60,0); the closing chord would run along y = 128 between x = 68 and x = 188
#[test] fn open_stroke_has_no_chord_but_keeps_its_drawn_segments() {
    let v = json!({"d":"M-60 0 L0 -60 L60 0"});
    assert!(alpha_at(v.clone(), 128, 127) < 0.01, "no stroke on the implicit closing chord: {}", alpha_at(v.clone(), 128, 127));
    assert!(alpha_at(v.clone(), 100, 127) < 0.01 && alpha_at(v.clone(), 150, 128) < 0.01);
    assert!(alpha_at(v.clone(), 128, 68) > 0.9, "the apex of the drawn stroke is there: {}", alpha_at(v.clone(), 128, 68));
    assert!(alpha_at(v.clone(), 98, 98) > 0.5, "the drawn left leg is there: {}", alpha_at(v.clone(), 98, 98));
}
#[test] fn explicit_z_still_strokes_the_closing_edge_and_a_filled_open_path_still_fills_to_the_chord() {
    assert!(alpha_at(json!({"d":"M-60 0 L0 -60 L60 0 Z"}), 128, 127) > 0.5, "Z closes the stroke");
    assert!(alpha_at(json!({"d":"M-60 0 L0 -60 L60 0","fill":[1,1,1],"stroke_width":0}), 128, 100) > 0.99, "an open path still fills as if closed");
    assert!(alpha_at(json!({"d":"M-60 0 L0 -60 L60 0","fill":[1,1,1],"stroke_width":0}), 128, 150) < 0.01, "and not beyond the chord");
}
