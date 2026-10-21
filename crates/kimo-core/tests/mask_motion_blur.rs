//! Mask motion blur (AE "motion blur" on animated mask shapes): the coverage of an animated mask is the box-filter integral of
//! its per-instant coverage over the shutter interval. Exact analytic check: a rectangle path translating at a constant 6 px per
//! frame, 180 degree shutter (3 px of travel while open). Under a vertical edge at position e, pixel column i covers
//! c_i(e) = clamp(i+1-e, 0, 1); its shutter average over e in [ea, eb] is (Hf(i+1-ea) - Hf(i+1-eb)) / (eb - ea) with
//! Hf(u) = 0 (u<0), u^2/2 (0<=u<=1), u-1/2 (u>1), the antiderivative of clamp(u,0,1). Hand values for the centred shutter at
//! t=0.5 s (edge 92.0, open 90.5 .. close 93.5; the mask is to the right of the edge): col 89 -> 0, col 90 -> 0.125/3 = 0.041667,
//! col 91 -> 1.0/3 = 0.333333, col 92 -> 2.0/3 = 0.666667, col 93 -> 2.875/3 = 0.958333, col 94 -> 1.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::masks::{self, Ctx, MaskLayer};
use serde_json::{json, Value};

const W: usize = 220; const H: usize = 60; const FPS: f64 = 24.0; const V: f64 = 144.0; // px per second = 6 px per frame
const X0: f64 = 20.0; const RW: f64 = 30.0; const Y0: f64 = 10.0; const Y1: f64 = 50.0; const ROW: usize = 30;

fn rect_pts(x: f64) -> Value { json!([{"x": x / W as f64, "y": Y0 / H as f64}, {"x": (x + RW) / W as f64, "y": Y0 / H as f64}, {"x": (x + RW) / W as f64, "y": Y1 / H as f64}, {"x": x / W as f64, "y": Y1 / H as f64}]) }
fn session(moving: bool) -> (Session, String, String) {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width": W, "height": H, "fps": FPS})).unwrap();
    let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
    let cl = dispatch(&mut s, "add_clip", &json!({"track_id": t, "source_type": "solid", "color": [0.5, 0.5, 0.5], "start": 0, "duration": 2})).unwrap()["clip_id"].as_str().unwrap().to_string();
    let m = dispatch(&mut s, "add_mask", &json!({"clip_id": cl, "kind": "path", "interp": "linear", "points": rect_pts(X0)})).unwrap()["mask_id"].as_str().unwrap().to_string();
    if moving { for (t, x) in [(0.0, X0), (1.0, X0 + V)] { dispatch(&mut s, "set_mask_path_key", &json!({"clip_id": cl, "mask_id": m, "time": t, "points": rect_pts(x)})).unwrap(); } }
    (s, cl, m)
}
fn layer(s: &Session) -> MaskLayer { s.project.tracks[0].clips[0].mask.as_ref().unwrap().layers[0].clone() }
fn cov(l: &MaskLayer, t: f64) -> Vec<f32> { let f = Frame { w: W, h: H, d: vec![0.0; W * H * 4] }; let refs = |_: &str| None; masks::layer_coverage(l, &f, t, &Ctx { refs: &refs, fps: FPS }) }
fn hf(u: f64) -> f64 { if u < 0.0 { 0.0 } else if u <= 1.0 { u * u / 2.0 } else { u - 0.5 } }
/// exact shutter average of column i under a leading (left) edge sweeping from ea to eb
fn exact_left(i: usize, ea: f64, eb: f64) -> f64 { let c = i as f64 + 1.0; (hf(c - ea) - hf(c - eb)) / (eb - ea) }
fn edge_at(t: f64) -> f64 { X0 + V * t.clamp(0.0, 1.0) }
/// max |kimo - exact| over the columns around the left edge on ROW, shutter [t+open, t+open+sh] (seconds)
fn max_err(c: &[f32], t: f64, open: f64, sh: f64) -> (f64, usize) { let (ea, eb) = (edge_at(t + open), edge_at(t + open + sh)); let mut m = (0f64, 0usize);
    for i in (ea.floor() as usize - 3)..(eb.ceil() as usize + 3) { let e = (c[ROW * W + i] as f64 - exact_left(i, ea, eb)).abs(); if e > m.0 { m = (e, i); } } m }
fn blur(s: &mut Session, cl: &str, m: &str, extra: Value) -> Result<Value, String> { let mut a = json!({"clip_id": cl, "mask_id": m}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } dispatch(s, "set_mask_motion_blur", &a) }
const SH180: f64 = 0.5 / FPS;

#[test] fn guard_unblurred_edge_is_exact_box_coverage() {
    // green today: no blur props -> the instant coverage, column 92 at edge 92.3 = 0.7 exactly in area terms
    let (s, _, _) = session(true); let t = (92.3 - X0) / V; let c = cov(&layer(&s), t);
    for (i, want) in [(91usize, 0.0f64), (92, 0.7), (93, 1.0), (94, 1.0)] { let got = c[ROW * W + i] as f64; assert!((got - want).abs() < 1e-5, "col {i}: got {got}, want {want}"); }
}

#[test] fn hand_values_centred_180() {
    let (mut s, cl, m) = session(true); blur(&mut s, &cl, &m, json!({"shutter_angle": 180, "samples": 64})).unwrap();
    let c = cov(&layer(&s), 0.5);
    for (i, want) in [(89usize, 0.0f64), (90, 0.125 / 3.0), (91, 1.0 / 3.0), (92, 2.0 / 3.0), (93, 2.875 / 3.0), (94, 1.0)] { let got = c[ROW * W + i] as f64; assert!((got - want).abs() < 1e-3, "col {i}: got {got}, want {want}"); }
}

#[test] fn analytic_ramp_within_1e3_at_32_and_converges() {
    let mut errs = vec![];
    for n in [4usize, 8, 16, 32, 64, 128, 256] {
        let (mut s, cl, m) = session(true); blur(&mut s, &cl, &m, json!({"shutter_angle": 180, "samples": n})).unwrap();
        let mut worst = (0f64, 0usize, 0f64);
        for t in [0.3, 0.5, 0.5 + 0.37 / V, 0.71] { let (e, i) = max_err(&cov(&layer(&s), t), t, -0.5 * SH180, SH180); if e > worst.0 { worst = (e, i, t); } }
        println!("N={n:3}: max |kimo - exact| = {:.3e} (col {} t={:.4})", worst.0, worst.1, worst.2); errs.push((n, worst.0));
    }
    for &(n, e) in &errs { if n >= 32 { assert!(e < 1e-3, "N={n}: max err {e:.3e} >= 1e-3"); } }
    for w in errs.windows(2) { assert!(w[1].1 <= w[0].1 * 1.5 + 1e-6, "not converging: N={} err {:.3e} -> N={} err {:.3e}", w[0].0, w[0].1, w[1].0, w[1].1); }
    let (e32, e256) = (errs[3].1, errs[6].1); assert!(e256 < 1e-4 && e256 <= e32 / 8.0, "N=256 err {e256:.3e} (N=32 {e32:.3e}): want < 1e-4 and <= N=32/8");
}

#[test] fn hidden_shutter_prop_now_integrates_correctly() {
    // the pre-existing undocumented layer props "shutter"/"samples" (set_mask_prop_key) take the same corrected integrator
    let (mut s, cl, m) = session(true);
    for (k, v) in [("shutter", 180.0), ("samples", 32.0)] { dispatch(&mut s, "set_mask_prop_key", &json!({"clip_id": cl, "mask_id": m, "property": k, "time": 0, "value": v})).unwrap(); }
    let (e, i) = max_err(&cov(&layer(&s), 0.5), 0.5, -0.5 * SH180, SH180); assert!(e < 1e-3, "hidden props, N=32: max err {e:.3e} at col {i}");
}

#[test] fn shutter_phase_and_angle_are_ae_semantics() {
    // phase 0: shutter opens AT the frame time (edge 92 -> 95 for 180 deg); 360 deg phase -180: [t - 1 frame/2, t + 1 frame/2]
    for (ang, ph, open, sh) in [(180.0, Some(0.0), 0.0, SH180), (360.0, Some(-180.0), -SH180, 2.0 * SH180), (90.0, None, -0.125 / FPS, 0.25 / FPS), (720.0, Some(-360.0), -2.0 * SH180, 4.0 * SH180)] {
        let (mut s, cl, m) = session(true); let mut a = json!({"shutter_angle": ang, "samples": 256}); if let Some(p) = ph { a["shutter_phase"] = json!(p); }
        blur(&mut s, &cl, &m, a).unwrap(); let (e, i) = max_err(&cov(&layer(&s), 0.5), 0.5, open, sh);
        assert!(e < 1e-3, "angle {ang} phase {ph:?}: max err {e:.3e} at col {i}");
    }
}

#[test] fn auto_samples_meet_the_same_bar() {
    let (mut s, cl, m) = session(true); let r = blur(&mut s, &cl, &m, json!({"shutter_angle": 180})).unwrap(); assert_eq!(r["samples"], json!(0), "{r}");
    let l = layer(&s); let (e, i) = max_err(&cov(&l, 0.5), 0.5, -0.5 * SH180, SH180); println!("auto: max err {e:.3e} at col {i}"); assert!(e < 1e-3, "auto: max err {e:.3e} at col {i}");
    // static mask on auto = exactly the unblurred coverage
    let (s2, _, _) = session(false); let mut l2 = layer(&s2); let off = cov(&l2, 0.5); l2.props = l.props.clone(); let on = cov(&l2, 0.5);
    assert!(off.iter().zip(&on).all(|(a, b)| a.to_bits() == b.to_bits()), "static mask on auto differs from off");
}

#[test] fn static_mask_with_blur_is_bit_identical() {
    for (ang, smp) in [(180.0, 32.0), (360.0, 7.0), (720.0, 0.0), (45.0, 3.0)] {
        let (mut s, cl, m) = session(false); let off = cov(&layer(&s), 0.5);
        blur(&mut s, &cl, &m, json!({"shutter_angle": ang, "samples": smp})).unwrap(); let on = cov(&layer(&s), 0.5);
        assert!(off.iter().zip(&on).all(|(a, b)| a.to_bits() == b.to_bits()), "static path, angle {ang} samples {smp}: blur on differs from off");
    }
    // a moving mask that is held (static) across the shutter window is exact too: hold keys
    let (mut s, cl, m) = session(false); for t in [0.0, 1.0] { dispatch(&mut s, "set_mask_path_key", &json!({"clip_id": cl, "mask_id": m, "time": t, "points": rect_pts(X0 + 3.3)})).unwrap(); }
    let off = cov(&layer(&s), 0.5); blur(&mut s, &cl, &m, json!({"shutter_angle": 180, "samples": 9})).unwrap(); let on = cov(&layer(&s), 0.5);
    assert!(off.iter().zip(&on).all(|(a, b)| a.to_bits() == b.to_bits()), "two identical keys: blur on differs from off");
    // a static rect (prop kind) as well
    let (mut s, cl, _) = session(false); let r = dispatch(&mut s, "add_mask", &json!({"clip_id": cl, "kind": "rect", "params": {"cx": 0.43, "cy": 0.5, "w": 0.2, "h": 0.5}})).unwrap()["mask_id"].as_str().unwrap().to_string();
    let get = |s: &Session| s.project.tracks[0].clips[0].mask.as_ref().unwrap().layers[1].clone(); let off = cov(&get(&s), 0.5);
    blur(&mut s, &cl, &r, json!({"shutter_angle": 270, "samples": 5})).unwrap(); let on = cov(&get(&s), 0.5);
    assert!(off.iter().zip(&on).all(|(a, b)| a.to_bits() == b.to_bits()), "static rect: blur on differs from off");
}

#[test] fn default_off_is_byte_identical_and_angle_zero_restores() {
    let (mut s, cl, m) = session(true); let before_json = serde_json::to_string(&s.project).unwrap(); let before = cov(&layer(&s), 0.5);
    // off: the instant coverage at t (the moving edge is hard, no in-between)
    let inst = masks::path_coverage(&masks::path_at(&layer(&s), 0.5), true, W, H, false, false, false);
    assert!(before.iter().zip(&inst).all(|(a, b)| a.to_bits() == b.to_bits()), "default (no blur) is not the instant coverage");
    blur(&mut s, &cl, &m, json!({"shutter_angle": 180, "samples": 16, "shutter_phase": -45})).unwrap();
    assert_ne!(serde_json::to_string(&s.project).unwrap(), before_json);
    let r = blur(&mut s, &cl, &m, json!({"shutter_angle": 0})).unwrap(); assert_eq!(r["shutter_angle"], json!(0.0), "{r}");
    assert_eq!(serde_json::to_string(&s.project).unwrap(), before_json, "angle 0 must leave the project exactly as before");
    let after = cov(&layer(&s), 0.5); assert!(before.iter().zip(&after).all(|(a, b)| a.to_bits() == b.to_bits()));
}

#[test] fn default_angle_follows_the_clip_shutter() {
    let (mut s, cl, m) = session(true); let r = blur(&mut s, &cl, &m, json!({})).unwrap(); assert_eq!(r["shutter_angle"], json!(180.0), "{r}"); assert_eq!(r["source"], json!("default"));
    dispatch(&mut s, "set_motion_blur", &json!({"clip_id": cl, "shutter_angle": 90})).unwrap();
    let r = blur(&mut s, &cl, &m, json!({"samples": 8})).unwrap(); assert_eq!(r["shutter_angle"], json!(90.0), "{r}"); assert_eq!(r["source"], json!("clip"));
}

#[test] fn refusals_leave_the_project_untouched() {
    let (mut s, cl, m) = session(true); let before = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [(json!({"shutter_angle": -1}), "shutter_angle must be 0..720"), (json!({"shutter_angle": 721}), "shutter_angle must be 0..720"),
        (json!({"shutter_angle": 180, "samples": 1}), "samples must be 0 (auto) or an integer 2..256"), (json!({"samples": 257}), "samples must be 0 (auto) or an integer 2..256"),
        (json!({"samples": 2.5}), "samples must be 0 (auto) or an integer 2..256"), (json!({"samples": -4}), "samples must be 0 (auto) or an integer 2..256"),
        (json!({"shutter_phase": 361}), "shutter_phase must be -360..360"), (json!({"shutter_phase": -400}), "shutter_phase must be -360..360")] {
        let e = blur(&mut s, &cl, &m, a.clone()).expect_err(&format!("accepted {a}")); assert!(e.contains(want), "{a}: error '{e}' lacks '{want}'");
        assert_eq!(serde_json::to_string(&s.project).unwrap(), before, "{a} changed the project");
    }
    let e = blur(&mut s, &cl, "mask_nope", json!({})).expect_err("unknown mask accepted"); assert!(e.contains("no such mask"), "{e}");
    let lm = dispatch(&mut s, "add_mask", &json!({"clip_id": cl, "kind": "luma"})).unwrap()["mask_id"].as_str().unwrap().to_string();
    let e = blur(&mut s, &cl, &lm, json!({"shutter_angle": 180})).expect_err("luma accepted"); assert!(e.contains("is a selection"), "{e}");
}

