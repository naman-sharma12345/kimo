//! Layer (transform) motion blur, opt-in exact sampler (set_motion_blur samples / shutter_phase). Analytic check: a white 40x40 rect layer over
//! black translating at 24 px per frame; with a 180 degree centred shutter its left edge sweeps 12 px while open. Pixel column i under an edge
//! at e covers c_i(e) = clamp(i+1-e, 0, 1); the shutter average over e in [ea, eb] is (Hf(i+1-ea) - Hf(i+1-eb)) / (eb - ea) with
//! Hf(u) = 0 (u<0), u^2/2 (0<=u<=1), u-1/2 (u>1). The legacy sampler (fixed n = ceil(angle/22.5) clamped 4..16) stays the default.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::{json, Value};

const W: usize = 240; const H: usize = 60; const FPS: f64 = 24.0; const V: f64 = 576.0; const X0: f64 = 100.0; const ROW: usize = 30;
const SH180: f64 = 0.5 / FPS;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn session(moving: bool) -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": FPS}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.0, 0.0, 0.0], "start": 0, "duration": 2}));
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let cl = c(&mut s, "add_shape", json!({"track_id": t2, "shape": "rect", "width": 40, "height": 40, "fill": [1.0, 1.0, 1.0, 1.0], "start": 0, "duration": 2}))["clip_id"].as_str().unwrap().to_string();
    if moving { for (t, v) in [(0.0, 0.0), (1.0, V)] { c(&mut s, "set_keyframe", json!({"clip_id": cl, "property": "x", "time": t, "value": v, "ease": "linear"})); } }
    (s, cl)
}
fn frame(s: &Session, t: f64) -> Vec<f32> { let f = render_frame(&s.project, t, &Cache::new()).unwrap(); assert_eq!((f.w, f.h), (W, H)); f.d }
fn hf(u: f64) -> f64 { if u < 0.0 { 0.0 } else if u <= 1.0 { u * u / 2.0 } else { u - 0.5 } }
fn exact_left(i: usize, ea: f64, eb: f64) -> f64 { let c = i as f64 + 1.0; (hf(c - ea) - hf(c - eb)) / (eb - ea) }
fn edge_at(t: f64) -> f64 { X0 + V * t.clamp(0.0, 1.0) }
fn max_err(d: &[f32], t: f64, open: f64, sh: f64) -> (f64, usize) { let (ea, eb) = (edge_at(t + open), edge_at(t + open + sh)); let mut m = (0f64, 0usize);
    for i in (ea.floor() as usize - 3)..(eb.ceil() as usize + 3) { let e = (d[(ROW * W + i) * 4] as f64 - exact_left(i, ea, eb)).abs(); if e > m.0 { m = (e, i); } } m }
const TS: [f64; 3] = [0.05, 0.1, 0.1 + 0.37 / V];
fn worst(s: &Session, open: f64, sh: f64) -> (f64, usize, f64) { let mut w = (0f64, 0usize, 0f64); for t in TS { let (e, i) = max_err(&frame(s, t), t, open, sh); if e > w.0 { w = (e, i, t); } } w }

#[test] fn legacy_sampler_error_is_printed_and_unchanged_by_default() {
    // measurement, not a gate: today's default (180 deg -> 8 samples) on 12 px of travel
    let (mut s, cl) = session(true); let r = c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 180})); assert_eq!(r, json!({"shutter_angle": 180.0}), "legacy result must be unchanged");
    let w = worst(&s, -0.5 * SH180, SH180); println!("legacy 180 deg (8 samples): max |kimo - exact| = {:.3e} (col {} t={:.4})", w.0, w.1, w.2);
    assert!(s.project.tracks[1].clips[0].props.keys().all(|k| !k.starts_with("mb_")), "legacy call must not write opt-in props");
}

#[test] fn exact_sampler_within_1e3_at_auto_and_converges() {
    let mut errs = vec![];
    for n in [8usize, 16, 32, 64, 128, 256] { let (mut s, cl) = session(true); c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 180, "samples": n}));
        let w = worst(&s, -0.5 * SH180, SH180); println!("exact N={n:3}: max |kimo - exact| = {:.3e} (col {} t={:.4})", w.0, w.1, w.2); errs.push((n, w.0)); }
    for w in errs.windows(2) { assert!(w[1].1 <= w[0].1 * 1.5 + 1e-6, "not converging: N={} {:.3e} -> N={} {:.3e}", w[0].0, w[0].1, w[1].0, w[1].1); }
    let e256 = errs.last().unwrap().1; assert!(e256 < 1e-3, "N=256 max err {e256:.3e} >= 1e-3");
    let (mut s, cl) = session(true); let r = c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 180, "samples": 0})); assert_eq!(r["samples"].as_f64(), Some(0.0), "{r}");
    let w = worst(&s, -0.5 * SH180, SH180); println!("exact auto: max |kimo - exact| = {:.3e} (col {} t={:.4})", w.0, w.1, w.2); assert!(w.0 < 1e-3, "auto max err {:.3e}", w.0);
}

#[test] fn shutter_phase_is_ae_semantics() {
    for (ang, ph, open, sh) in [(180.0, 0.0, 0.0, SH180), (360.0, -180.0, -SH180, 2.0 * SH180), (90.0, -90.0, -0.25 / FPS, 0.25 / FPS)] {
        let (mut s, cl) = session(true); c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": ang, "shutter_phase": ph, "samples": 256}));
        let w = worst(&s, open, sh); assert!(w.0 < 1e-3, "angle {ang} phase {ph}: max err {:.3e} at col {} t={}", w.0, w.1, w.2);
    }
}

#[test] fn static_layer_with_exact_blur_is_bit_identical() {
    let (mut s, cl) = session(false); let off = frame(&s, 0.5);
    c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 360, "samples": 64, "shutter_phase": -90})); let on = frame(&s, 0.5);
    assert!(off.iter().zip(&on).all(|(a, b)| a.to_bits() == b.to_bits()), "static layer: exact blur on differs from off");
}

#[test] fn angle_zero_clears_and_refusals_leave_project_untouched() {
    let (mut s, cl) = session(true); let before = serde_json::to_string(&s.project).unwrap();
    c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 180, "samples": 32, "shutter_phase": 10})); c(&mut s, "set_motion_blur", json!({"clip_id": cl, "shutter_angle": 0}));
    assert_eq!(serde_json::to_string(&s.project).unwrap(), before, "angle 0 must leave the project exactly as before");
    for (a, want) in [(json!({"samples": 1}), "samples must be 0 (auto) or an integer 2..256"), (json!({"samples": 257}), "samples must be 0 (auto) or an integer 2..256"), (json!({"samples": 3.5}), "samples must be 0 (auto) or an integer 2..256"),
        (json!({"samples": -2}), "samples must be 0 (auto) or an integer 2..256"), (json!({"shutter_phase": 400}), "shutter_phase must be -360..360"), (json!({"shutter_phase": -361}), "shutter_phase must be -360..360")] {
        let mut q = a.clone(); q["clip_id"] = json!(cl); q["shutter_angle"] = json!(180);
        let e = dispatch(&mut s, "set_motion_blur", &q).expect_err(&format!("accepted {a}")); assert!(e.contains(want), "{a}: '{e}' lacks '{want}'");
        assert_eq!(serde_json::to_string(&s.project).unwrap(), before, "{a} changed the project");
    }
}
