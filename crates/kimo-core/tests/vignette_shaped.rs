//! vignette: shaped (centre, roundness, squareness, size, aspect, feather, colour) and tracked (strings.source_clip / source_mask),
//! read off a flat grey 160x90 frame through an adjustment layer: the vignette's darkening is the drop from 0.5.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 90;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
fn tr(s: &mut Session) -> String { id(&c(s, "add_track", json!({})), "track_id") }
/// grey footage on the bottom track; returns (session, footage clip, adjustment layer on top)
fn scene() -> (Session, String, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let t = tr(&mut s); let foot = id(&c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.5, 0.5, 0.5], "start": 0, "duration": 2.0})), "clip_id");
    let ta = tr(&mut s); let adj = id(&c(&mut s, "add_adjustment_layer", json!({"track_id": ta, "start": 0, "duration": 2})), "clip_id"); (s, foot, adj)
}
fn vig(s: &mut Session, clip: &str, params: Value, strings: Value) -> Result<Value, String> { dispatch(s, "add_effect", &json!({"clip_id": clip, "kind": "vignette", "params": params, "strings": strings})).map_err(|e| e.to_string()) }
fn frame(s: &Session, t: f64) -> Result<Frame, String> { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).map_err(|e| e.to_string())?; render::render_frame(&s.project, t, &cache).map_err(|e| e.to_string()) }
fn g(f: &Frame, x: usize, y: usize) -> f32 { f.d[(y * f.w + x) * 4 + 1] }
fn near(got: f32, want: f32, tol: f32, what: &str) { assert!((got - want).abs() <= tol, "{what}: want {want}, got {got:.4}"); }

#[test]
fn centre_moves_the_clear_spot() {
    let (mut s, _f, adj) = scene();
    vig(&mut s, &adj, json!({"amount": 1.0, "feather": 0.4, "center_x": 0.2}), json!({})).unwrap();
    let f = frame(&s, 0.5).unwrap();
    // the clear spot sits at x = 32: the left edge is 32 px from it (untouched), the right edge 127 px (fully dark)
    near(g(&f, 32, 45), 0.5, 0.002, "new centre x 32"); near(g(&f, 0, 45), 0.5, 0.002, "left edge, 32 px from the centre");
    assert!(g(&f, 159, 45) < 0.05, "right edge, 127 px from the centre, must be dark: {}", g(&f, 159, 45));
}

#[test]
fn roundness_one_is_a_true_circle_and_zero_follows_the_frame() {
    let (mut s, _f, adj) = scene();
    let e = id(&vig(&mut s, &adj, json!({"amount": 1.0, "feather": 0.8, "roundness": 1.0}), json!({})).unwrap(), "effect_id");
    let f = frame(&s, 0.5).unwrap(); let (hz, vt) = (g(&f, 120, 45), g(&f, 80, 85));
    assert!(hz < 0.49, "40 px right of centre is inside the falloff: {hz}"); near(vt, hz, 0.003, "40 px below = 40 px right on a circle");
    c(&mut s, "set_effect_param", json!({"clip_id": adj, "effect_id": e, "param": "roundness", "value": 0.0}));
    let f = frame(&s, 0.5).unwrap(); assert!(g(&f, 80, 85) < g(&f, 120, 45) - 0.02, "frame-shaped: 40 px down reaches further out than 40 px across on a 16:9 frame");
}

#[test]
fn squareness_keeps_the_corners_lit_longer_and_size_aspect_scale_it() {
    let (mut s, _f, adj) = scene();
    let e = id(&vig(&mut s, &adj, json!({"amount": 1.0, "feather": 0.6, "squareness": 0.0}), json!({})).unwrap(), "effect_id");
    let f0 = frame(&s, 0.5).unwrap();
    c(&mut s, "set_effect_param", json!({"clip_id": adj, "effect_id": e, "param": "squareness", "value": 1.0}));
    let f1 = frame(&s, 0.5).unwrap();
    assert!(g(&f1, 140, 79) > g(&f0, 140, 79) + 0.03, "diagonal point brighter with squareness 1: {} vs {}", g(&f1, 140, 79), g(&f0, 140, 79));
    near(g(&f1, 150, 45), g(&f0, 150, 45), 0.01, "on-axis point the same");
    c(&mut s, "set_effect_param", json!({"clip_id": adj, "effect_id": e, "param": "squareness", "value": 0.0}));
    c(&mut s, "set_effect_param", json!({"clip_id": adj, "effect_id": e, "param": "size", "value": 2.0}));
    let f2 = frame(&s, 0.5).unwrap(); near(g(&f2, 150, 45), 0.5, 0.002, "size 2 pushes the falloff off the frame");
    c(&mut s, "set_effect_param", json!({"clip_id": adj, "effect_id": e, "param": "size", "value": 1.0}));
    c(&mut s, "set_effect_param", json!({"clip_id": adj, "effect_id": e, "param": "aspect", "value": 0.5}));
    let f3 = frame(&s, 0.5).unwrap(); assert!(g(&f3, 150, 45) < g(&f0, 150, 45) - 0.05, "aspect 0.5 narrows it: {} vs {}", g(&f3, 150, 45), g(&f0, 150, 45));
}

#[test]
fn colour_tints_the_edge_instead_of_darkening() {
    let (mut s, _f, adj) = scene();
    vig(&mut s, &adj, json!({"amount": 1.0, "feather": 0.3, "center_x": 0.0, "color_r": 1.0, "color_g": 0.0, "color_b": 0.0}), json!({})).unwrap();
    let f = frame(&s, 0.5).unwrap(); let i = (45 * W + 159) * 4;
    near(f.d[i], 1.0, 0.01, "far edge red"); near(f.d[i + 1], 0.0, 0.01, "far edge green"); near(f.d[1 + 45 * W * 4], 0.5, 0.002, "centre untouched");
}

#[test]
fn centre_follows_a_tracked_clip() {
    let (mut s, _f, adj) = scene();
    let tt = tr(&mut s); let trk = id(&c(&mut s, "add_shape", json!({"track_id": tt, "shape": "rect", "width": 4, "height": 4, "fill": "#ffffff", "start": 0, "duration": 2})), "clip_id");
    c(&mut s, "set_property", json!({"clip_id": trk, "property": "opacity", "value": 0.0}));
    c(&mut s, "set_keyframe", json!({"clip_id": trk, "property": "x", "time": 0.0, "value": 0.0, "ease": "linear"})); c(&mut s, "set_keyframe", json!({"clip_id": trk, "property": "x", "time": 2.0, "value": 64.0, "ease": "linear"}));
    vig(&mut s, &adj, json!({"amount": 1.0, "feather": 0.3, "roundness": 1.0, "size": 0.5, "center_x": 0.5}), json!({"source_clip": trk})).unwrap();
    let f0 = frame(&s, 0.0).unwrap(); let f1 = frame(&s, 1.0).unwrap();
    near(g(&f0, 80, 45), 0.5, 0.002, "t 0: clear at x 80"); near(g(&f1, 112, 45), 0.5, 0.002, "t 1: clear spot moved 32 px with the tracker");
    assert!(g(&f1, 60, 45) < g(&f0, 60, 45) - 0.05, "x 60 darker once the spot has moved away: {} vs {}", g(&f1, 60, 45), g(&f0, 60, 45));
}

#[test]
fn centre_follows_a_mask_layer() {
    let (mut s, foot, adj) = scene();
    let m = id(&c(&mut s, "add_mask", json!({"clip_id": foot, "kind": "ellipse", "mode": "none", "params": {"cx": 0.25, "cy": 0.5, "w": 0.1, "h": 0.1}})), "mask_id");
    vig(&mut s, &adj, json!({"amount": 1.0, "feather": 0.3, "roundness": 1.0, "size": 0.5}), json!({"source_mask": m})).unwrap();
    let f = frame(&s, 0.5).unwrap(); near(g(&f, 40, 45), 0.5, 0.002, "clear at the mask's centre x 40"); assert!(g(&f, 80, 45) < 0.45, "frame centre now in the falloff: {}", g(&f, 80, 45));
    // a path mask: the centre is the mean of its points
    let (mut s, foot, adj) = scene();
    let m = id(&c(&mut s, "add_mask", json!({"clip_id": foot, "kind": "path", "mode": "none", "points": [{"x": 0.6, "y": 0.4}, {"x": 0.8, "y": 0.4}, {"x": 0.8, "y": 0.6}, {"x": 0.6, "y": 0.6}]})), "mask_id");
    vig(&mut s, &adj, json!({"amount": 1.0, "feather": 0.3, "roundness": 1.0, "size": 0.5}), json!({"source_mask": m})).unwrap();
    let f = frame(&s, 0.5).unwrap(); near(g(&f, 112, 45), 0.5, 0.002, "clear at the path's centre x 112");
}

#[test]
fn default_vignette_is_unchanged_and_bad_sources_are_named() {
    let (mut s, _f, adj) = scene();
    vig(&mut s, &adj, json!({"amount": 0.5, "softness": 0.5}), json!({})).unwrap();
    let f = frame(&s, 0.5).unwrap();
    // the original formula at the right edge centre: dx 0.9875, r = dx/1.414, v = 1 - 0.5*((r - 0.5)/0.5)^2
    let r = (159.0f32 / 160.0 - 0.5) * 2.0 / 1.414; let v = 1.0 - 0.5 * ((r - 0.5) / 0.5).powi(2); near(g(&f, 159, 45), 0.5 * v, 0.002, "original falloff");
    let (mut s, _f, adj) = scene(); vig(&mut s, &adj, json!({"amount": 1.0}), json!({"source_clip": "nope"})).unwrap();
    let e = match frame(&s, 0.5) { Err(e) => e, Ok(_) => panic!("unknown source_clip must fail the render") }; assert!(e.contains("vignette") && e.contains("nope"), "{e}");
    let (mut s, _f, adj) = scene(); vig(&mut s, &adj, json!({"amount": 1.0}), json!({"source_mask": "nomask"})).unwrap();
    let e = match frame(&s, 0.5) { Err(e) => e, Ok(_) => panic!("unknown source_mask must fail the render") }; assert!(e.contains("nomask"), "{e}");
}
