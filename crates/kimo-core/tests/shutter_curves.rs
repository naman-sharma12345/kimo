//! Shutter curves (shutter efficiency, Nuke / RenderMan / Arnold): the blurred value is the curve-weighted temporal average
//! sum_k w(s_k) c(t_k) / sum_k w(s_k), s = normalized shutter position. Analytic check for masks and layers: a straight edge moving at
//! constant speed, reference = the same weighted average integrated in f64 with 200000 midpoints (independent of kimo), gate 1e-3 at N=256.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::masks::{self, Ctx};
use kimo_core::render::{render_frame, Cache};
use serde_json::{json, Value};

const FPS: f64 = 24.0; const SH180: f64 = 0.5 / FPS;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn curve_w(name: &str, pts: &[(f64, f64)], s: f64) -> f64 { match name { "box" => 1.0, "triangle" => 1.0 - (2.0 * s - 1.0).abs(), "gaussian" => (-0.5 * ((s - 0.5) * 6.0).powi(2)).exp(),
    _ => { if s <= pts[0].0 { return pts[0].1; } for w in pts.windows(2) { if s <= w[1].0 { let u = (s - w[0].0) / (w[1].0 - w[0].0); return w[0].1 + (w[1].1 - w[0].1) * u; } } pts[pts.len() - 1].1 } } }
/// reference: weighted shutter average of column i under a leading edge moving e(s) = ea + (eb - ea) s
fn reference(name: &str, pts: &[(f64, f64)], i: usize, ea: f64, eb: f64) -> f64 { let m = 200_000; let (mut num, mut den) = (0.0, 0.0);
    for k in 0..m { let s = (k as f64 + 0.5) / m as f64; let w = curve_w(name, pts, s); let e = ea + (eb - ea) * s; num += w * (i as f64 + 1.0 - e).clamp(0.0, 1.0); den += w; } num / den }
const CUSTOM: [(f64, f64); 3] = [(0.0, 0.0), (0.2, 1.0), (1.0, 0.3)];
fn curves() -> Vec<(&'static str, Value)> { vec![("triangle", json!({"shutter_curve": "triangle"})), ("gaussian", json!({"shutter_curve": "gaussian"})), ("custom", json!({"shutter_curve": "custom", "curve_points": [[0.0, 0.0], [0.2, 1.0], [1.0, 0.3]]}))] }
fn merge(a: &mut Value, b: &Value) { for (k, v) in b.as_object().unwrap() { a[k] = v.clone(); } }

// ---- masks: rect path translating 6 px per frame (3 px under a 180 degree shutter)
const MW: usize = 220; const MH: usize = 60; const MV: f64 = 144.0; const MX0: f64 = 20.0;
fn rect_pts(x: f64) -> Value { json!([{"x": x / MW as f64, "y": 10.0 / MH as f64}, {"x": (x + 30.0) / MW as f64, "y": 10.0 / MH as f64}, {"x": (x + 30.0) / MW as f64, "y": 50.0 / MH as f64}, {"x": x / MW as f64, "y": 50.0 / MH as f64}]) }
fn mask_session() -> (Session, String, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": MW, "height": MH, "fps": FPS})); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let cl = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.5, 0.5, 0.5], "start": 0, "duration": 2}))["clip_id"].as_str().unwrap().to_string();
    let m = c(&mut s, "add_mask", json!({"clip_id": cl, "kind": "path", "interp": "linear", "points": rect_pts(MX0)}))["mask_id"].as_str().unwrap().to_string();
    for (t, x) in [(0.0, MX0), (1.0, MX0 + MV)] { c(&mut s, "set_mask_path_key", json!({"clip_id": cl, "mask_id": m, "time": t, "points": rect_pts(x)})); } (s, cl, m) }
fn mask_cov(s: &Session, t: f64) -> Vec<f32> { let l = s.project.tracks[0].clips[0].mask.as_ref().unwrap().layers[0].clone(); let f = Frame { w: MW, h: MH, d: vec![0.0; MW * MH * 4] }; let refs = |_: &str| None; masks::layer_coverage(&l, &f, t, &Ctx { refs: &refs, fps: FPS }) }

#[test] fn mask_curves_match_the_weighted_convolution() {
    for (name, args) in curves() { for n in [16usize, 64, 256] {
        let (mut s, cl, m) = mask_session(); let mut a = json!({"clip_id": cl, "mask_id": m, "shutter_angle": 180, "samples": n}); merge(&mut a, &args); c(&mut s, "set_mask_motion_blur", a);
        let mut worst = (0f64, 0usize, 0f64);
        for t in [0.3, 0.5 + 0.37 / MV] { let cv = mask_cov(&s, t); let (ea, eb) = (MX0 + MV * (t - 0.5 * SH180), MX0 + MV * (t + 0.5 * SH180));
            for i in (ea.floor() as usize - 2)..(eb.ceil() as usize + 2) { let e = (cv[30 * MW + i] as f64 - reference(name, &CUSTOM, i, ea, eb)).abs(); if e > worst.0 { worst = (e, i, t); } } }
        println!("mask {name:8} N={n:3}: max |kimo - ref| = {:.3e} (col {} t={:.4})", worst.0, worst.1, worst.2);
        if n == 256 { assert!(worst.0 < 1e-3, "mask {name} N=256: {:.3e}", worst.0); }
    } }
}

// ---- layers: white 40x40 rect over black, 24 px per frame (12 px under 180 degrees)
const LW: usize = 240; const LH: usize = 60; const LV: f64 = 576.0; const LX0: f64 = 100.0;
fn layer_session() -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": LW, "height": LH, "fps": FPS}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.0, 0.0, 0.0], "start": 0, "duration": 2}));
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let cl = c(&mut s, "add_shape", json!({"track_id": t2, "shape": "rect", "width": 40, "height": 40, "fill": [1.0, 1.0, 1.0, 1.0], "start": 0, "duration": 2}))["clip_id"].as_str().unwrap().to_string();
    for (t, v) in [(0.0, 0.0), (1.0, LV)] { c(&mut s, "set_keyframe", json!({"clip_id": cl, "property": "x", "time": t, "value": v, "ease": "linear"})); } (s, cl) }

#[test] fn layer_curves_match_the_weighted_convolution() {
    for (name, args) in curves() { for n in [16usize, 64, 256] {
        let (mut s, cl) = layer_session(); let mut a = json!({"clip_id": cl, "shutter_angle": 180, "samples": n}); merge(&mut a, &args); c(&mut s, "set_motion_blur", a);
        let mut worst = (0f64, 0usize, 0f64);
        for t in [0.05, 0.1 + 0.37 / LV] { let f = render_frame(&s.project, t, &Cache::new()).unwrap(); let (ea, eb) = (LX0 + LV * (t - 0.5 * SH180), LX0 + LV * (t + 0.5 * SH180));
            for i in (ea.floor() as usize - 2)..(eb.ceil() as usize + 2) { let e = (f.d[(30 * LW + i) * 4] as f64 - reference(name, &CUSTOM, i, ea, eb)).abs(); if e > worst.0 { worst = (e, i, t); } } }
        println!("layer {name:8} N={n:3}: max |kimo - ref| = {:.3e} (col {} t={:.4})", worst.0, worst.1, worst.2);
        if n == 256 { assert!(worst.0 < 1e-3, "layer {name} N=256: {:.3e}", worst.0); }
    } }
}

#[test] fn box_curve_is_the_default_and_writes_nothing() {
    let (mut s, cl) = layer_session(); c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 180, "samples": 32})); let a = serde_json::to_string(&s.project).unwrap(); let fa = render_frame(&s.project, 0.1, &Cache::new()).unwrap();
    c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 180, "shutter_curve": "triangle"})); c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 180, "shutter_curve": "box"}));
    assert_eq!(serde_json::to_string(&s.project).unwrap(), a, "box must equal no curve"); let fb = render_frame(&s.project, 0.1, &Cache::new()).unwrap();
    assert!(fa.d.iter().zip(&fb.d).all(|(x, y)| x.to_bits() == y.to_bits()));
    let (mut s, cl, m) = mask_session(); c(&mut s, "set_mask_motion_blur", json!({"clip_id": cl, "mask_id": m, "shutter_angle": 180, "samples": 32})); let a = serde_json::to_string(&s.project).unwrap();
    c(&mut s, "set_mask_motion_blur", json!({"clip_id": cl, "mask_id": m, "shutter_angle": 180, "samples": 32, "shutter_curve": "gaussian"})); c(&mut s, "set_mask_motion_blur", json!({"clip_id": cl, "mask_id": m, "shutter_angle": 180, "samples": 32, "shutter_curve": "box"}));
    assert_eq!(serde_json::to_string(&s.project).unwrap(), a, "mask: box must equal no curve");
}

#[test] fn curve_refusals_leave_the_project_untouched() {
    let bad: Vec<(Value, &str)> = vec![(json!({"shutter_curve": "zigzag"}), "shutter_curve must be box, triangle, gaussian or custom"),
        (json!({"shutter_curve": "custom"}), "curve_points must be"), (json!({"shutter_curve": "custom", "curve_points": [[0.5, 1.0]]}), "curve_points must be"),
        (json!({"shutter_curve": "custom", "curve_points": [[0.0, 1.0], [1.5, 1.0]]}), "curve_points must be"), (json!({"shutter_curve": "custom", "curve_points": [[0.0, 0.0], [1.0, 0.0]]}), "curve_points must be"),
        (json!({"shutter_curve": "custom", "curve_points": [[0.0, 1.0], [1.0, -0.5]]}), "curve_points must be")];
    let (mut s, cl) = layer_session(); let before = serde_json::to_string(&s.project).unwrap();
    for (a, want) in &bad { let mut q = json!({"clip_id": cl, "shutter_angle": 180}); merge(&mut q, a); let e = dispatch(&mut s, "set_motion_blur", &q).expect_err(&format!("layer accepted {a}")); assert!(e.contains(want), "{a}: '{e}'"); }
    assert_eq!(serde_json::to_string(&s.project).unwrap(), before);
    let (mut s, cl, m) = mask_session(); let before = serde_json::to_string(&s.project).unwrap();
    for (a, want) in &bad { let mut q = json!({"clip_id": cl, "mask_id": m, "shutter_angle": 180}); merge(&mut q, a); let e = dispatch(&mut s, "set_mask_motion_blur", &q).expect_err(&format!("mask accepted {a}")); assert!(e.contains(want), "{a}: '{e}'"); }
    assert_eq!(serde_json::to_string(&s.project).unwrap(), before);
}
