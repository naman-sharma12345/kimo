//! E8: the two house easings as named presets, analytic rows against reference values of the cubic-bezier.
use kimo_core::keyframe::{bezier_ease, named_bezier};
use kimo_core::commands::{dispatch, Session};
use serde_json::json;
#[test] fn presets_hit_their_reference_values() {
    let out = named_bezier("ui_ease_out").unwrap(); let io = named_bezier("ui_ease_in_out").unwrap(); assert_eq!(out, [0.16, 1.0, 0.3, 1.0]); assert_eq!(io, [0.65, 0.0, 0.35, 1.0]); assert!(named_bezier("bounce").is_none());
    for b in [out, io] { assert!(bezier_ease(b, 0.0).abs() < 1e-9 && (bezier_ease(b, 1.0) - 1.0).abs() < 1e-9); }
    assert!((bezier_ease(io, 0.5) - 0.5).abs() < 1e-6, "ease-in-out is symmetric: {}", bezier_ease(io, 0.5));
    let y = bezier_ease(out, 0.1); assert!(y > 0.40 && y < 0.60, "ease-out is front-loaded: y(0.1) = {y}"); eprintln!("ui_ease_out y(0.1)={y:.4} y(0.5)={:.4}; ui_ease_in_out y(0.25)={:.4}", bezier_ease(out, 0.5), bezier_ease(io, 0.25));
    assert!(bezier_ease(out, 0.5) > 0.95, "settled by the midpoint"); assert!(bezier_ease(io, 0.25) < 0.2, "slow start");
    let mut prev = 0.0; for i in 0..=100 { let v = bezier_ease(io, i as f64 / 100.0); assert!(v >= prev - 1e-12, "monotone"); prev = v; }
}
fn fixture() -> (Session, String) { let mut s = Session::new(); let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string(); let c = dispatch(&mut s, "add_clip", &json!({"track_id":t,"source_type":"solid","start":0,"duration":2})).unwrap()["clip_id"].as_str().unwrap().to_string();
    for (tm, v) in [(0.0, 0.0), (1.0, 1.0)] { dispatch(&mut s, "set_keyframe", &json!({"clip_id":c,"property":"opacity","time":tm,"value":v})).unwrap(); } (s, c) }
#[test] fn tool_accepts_a_preset_and_rejects_both_unknown_and_neither() {
    let (mut s, c) = fixture(); dispatch(&mut s, "set_keyframe_bezier", &json!({"clip_id":c,"property":"opacity","time":0.0,"preset":"ui_ease_out"})).unwrap();
    for bad in [json!({"preset":"ui_ease_out","bezier":[0,0,1,1]}), json!({"preset":"nope"}), json!({}), json!({"bezier":[1,2,3]})] { let mut a = json!({"clip_id":c,"property":"opacity","time":0.0}); for (k, v) in bad.as_object().unwrap() { a[k] = v.clone(); } assert!(dispatch(&mut s, "set_keyframe_bezier", &a).is_err(), "{bad}"); }
}
