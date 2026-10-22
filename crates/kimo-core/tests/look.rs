//! look: one-tap creator looks built from grade operations (teal & orange, golden hour, moody, clean bright, film, vintage,
//! noir, neon) with an intensity slider (0 = exact identity, blend is linear) and skin protection (skin tones keep their own
//! hue and take only the look's brightness). Every look keeps tone order (a grey ramp never inverts) and stays inside 0..1.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::json;
use std::collections::BTreeMap;

fn pr(kv: &[(&str, f64)]) -> Params { let mut p = Params::new(); for (k, v) in kv { p.insert(k.to_string(), Prop::new(*v)); } p }
fn run(p: &Params, f: &mut Frame) -> Result<(), String> { kimo_core::effects::apply("look", p, &BTreeMap::new(), f, 0.0, 0) }
fn frame(px: &[[f32; 3]]) -> Frame { let mut d = Vec::new(); for c in px { d.extend_from_slice(&[c[0], c[1], c[2], 1.0]); } Frame { w: px.len(), h: 1, d } }
fn look(preset: f64, intensity: f64, px: &[[f32; 3]]) -> Vec<[f32; 3]> { let mut f = frame(px); run(&pr(&[("preset", preset), ("intensity", intensity), ("protect_skin", 0.0)]), &mut f).unwrap_or_else(|e| panic!("look preset {preset}: {e}"));
    f.d.chunks_exact(4).map(|c| [c[0], c[1], c[2]]).collect() }
fn hsv(c: [f32; 3]) -> (f32, f32, f32) { let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2])); let d = mx - mn; if d <= 0.0 { return (0.0, 0.0, mx); }
    let h = if mx == c[0] { 60.0 * ((c[1] - c[2]) / d) } else if mx == c[1] { 60.0 * ((c[2] - c[0]) / d + 2.0) } else { 60.0 * ((c[0] - c[1]) / d + 4.0) }; (h.rem_euclid(360.0), d / mx, mx) }
fn from_hsv(h: f32, s: f32, v: f32) -> [f32; 3] { let c = v * s; let hp = h / 60.0; let x = c * (1.0 - (hp % 2.0 - 1.0).abs()); let m = v - c;
    let (r, g, b) = match hp as i32 { 0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x), 3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x) }; [r + m, g + m, b + m] }
fn luma(c: [f32; 3]) -> f32 { 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] }
fn hue_gap(a: f32, b: f32) -> f32 { let d = (a - b).rem_euclid(360.0); d.min(360.0 - d) }
/// 12 hues x 3 saturations x 4 values plus 8 greys
fn chart() -> Vec<[f32; 3]> { let mut v = Vec::new(); for h in 0..12 { for s in [0.3f32, 0.6, 0.9] { for val in [0.25f32, 0.5, 0.7, 0.9] { v.push(from_hsv(h as f32 * 30.0, s, val)); } } }
    for g in 0..8 { let x = g as f32 / 7.0; v.push([x, x, x]); } v }
fn mean_sat(px: &[[f32; 3]]) -> f32 { px.iter().map(|c| hsv(*c).1).sum::<f32>() / px.len() as f32 }
fn mean_luma(px: &[[f32; 3]]) -> f32 { px.iter().map(|c| luma(*c)).sum::<f32>() / px.len() as f32 }
fn diff(a: &[[f32; 3]], b: &[[f32; 3]]) -> f32 { a.iter().zip(b).map(|(x, y)| (0..3).map(|j| (x[j] - y[j]).abs()).sum::<f32>()).sum::<f32>() / (3 * a.len()) as f32 }
const PRESETS: usize = 8;

#[test] fn look_is_an_effect_kind_with_presets_and_intensity() {
    let mut ss = Session::new(); dispatch(&mut ss, "project_set", &json!({"width": 64, "height": 64, "fps": 25})).unwrap();
    let tr = dispatch(&mut ss, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let id = dispatch(&mut ss, "add_clip", &json!({"track_id": tr, "source_type": "solid", "color": [0.4, 0.5, 0.6], "start": 0, "duration": 1})).unwrap()["clip_id"].as_str().unwrap().to_string();
    let a = json!({"clip_id": id, "kind": "look", "params": {"preset": 0, "intensity": 0.8, "protect_skin": 0.5}});
    dispatch(&mut ss, "add_effect", &a).unwrap_or_else(|e| panic!("add_effect {a}: {e}"));
    let list = dispatch(&mut ss, "list_effects", &json!({})).unwrap().to_string();
    for k in ["\"look\"", "teal", "golden", "moody", "intensity", "protect_skin"] { assert!(list.contains(k), "list_effects lacks {k}"); }
}

#[test] fn teal_orange_splits_shadows_and_highlights_and_intensity_blends() {
    let c = chart(); let id = look(0.0, 0.0, &c); for (a, b) in c.iter().zip(&id) { assert_eq!(a, b, "intensity 0 must be an exact identity"); }
    let full = look(0.0, 1.0, &c); let half = look(0.0, 0.5, &c);
    for ((a, f), h) in c.iter().zip(&full).zip(&half) { for j in 0..3 { assert!((h[j] - (a[j] + 0.5 * (f[j] - a[j]))).abs() < 1e-5, "intensity 0.5 is the halfway blend"); } }
    let o = look(0.0, 1.0, &[[0.15, 0.15, 0.15], [0.8, 0.8, 0.8], [0.2, 0.35, 0.7]]);
    assert!(o[0][2] > o[0][0] + 0.02 && o[0][1] > o[0][0] + 0.01, "dark grey goes teal: {:?}", o[0]);
    assert!(o[1][0] > o[1][2] + 0.03, "light grey goes orange: {:?}", o[1]);
    let (h0, h1) = (hsv([0.2, 0.35, 0.7]).0, hsv(o[2]).0); assert!(h1 < h0 - 5.0 && h1 > 175.0, "blue is pulled toward teal: hue {h0} -> {h1}");
}

#[test] fn skin_protection_keeps_skin_hue() {
    let skin = [[0.45f32, 0.30, 0.24], [0.80, 0.58, 0.46], [0.62, 0.42, 0.33]];
    let run_p = |p: f64| { let mut f = frame(&skin); run(&pr(&[("preset", 0.0), ("intensity", 1.0), ("protect_skin", p)]), &mut f).unwrap(); f.d.chunks_exact(4).map(|c| [c[0], c[1], c[2]]).collect::<Vec<_>>() };
    let (open, kept) = (run_p(0.0), run_p(1.0));
    let worst = |o: &[[f32; 3]]| skin.iter().zip(o).map(|(a, b)| hue_gap(hsv(*a).0, hsv(*b).0)).fold(0.0f32, f32::max);
    assert!(worst(&open) > 4.0, "teal & orange moves unprotected skin hue: {}", worst(&open));
    assert!(worst(&kept) < 1.5, "protected skin keeps its hue: {} deg", worst(&kept));
    for (a, b) in open.iter().zip(&kept) { assert!((luma(*a) - luma(*b)).abs() < 0.01, "protected skin still takes the look's brightness"); }
    let blue = [[0.2f32, 0.35, 0.7]]; let mut f = frame(&blue); run(&pr(&[("preset", 0.0), ("intensity", 1.0), ("protect_skin", 1.0)]), &mut f).unwrap();
    assert!(hue_gap(hsv(blue[0]).0, hsv([f.d[0], f.d[1], f.d[2]]).0) > 5.0, "protection is for skin only: blue still moves");
}

#[test] fn every_look_keeps_tone_order_and_range_and_differs() {
    let ramp: Vec<[f32; 3]> = (0..256).map(|i| { let x = i as f32 / 255.0; [x, x, x] }).collect(); let c = chart(); let mut outs = Vec::new();
    for p in 0..PRESETS { let o = look(p as f64, 1.0, &ramp);
        for w in o.windows(2) { assert!(luma(w[1]) >= luma(w[0]) - 1e-5, "preset {p}: grey ramp inverts ({:?} -> {:?})", w[0], w[1]); }
        for v in o.iter().flatten() { assert!(v.is_finite() && *v >= -1e-6 && *v <= 1.0 + 1e-6, "preset {p}: value {v} outside 0..1"); }
        let oc = look(p as f64, 1.0, &c); assert!(diff(&c, &oc) > 0.01, "preset {p} changes the picture"); outs.push(oc); }
    for a in 0..PRESETS { for b in a + 1..PRESETS { assert!(diff(&outs[a], &outs[b]) > 0.008, "presets {a} and {b} are too alike"); } }
}

#[test] fn looks_do_what_their_names_say() {
    let c = chart(); let (s0, l0) = (mean_sat(&c), mean_luma(&c)); let g = |p: f64, px: [f32; 3]| look(p, 1.0, &[px])[0];
    let o = look(1.0, 1.0, &c); let m = g(1.0, [0.5, 0.5, 0.5]); assert!(m[0] - m[2] > 0.05, "golden hour warms grey: {m:?}"); assert!(mean_sat(&o) > s0 * 1.05, "golden hour is richer: {} vs {s0}", mean_sat(&o));
    let o = look(2.0, 1.0, &c); assert!(mean_sat(&o) < s0 * 0.8 && mean_luma(&o) < l0, "moody is darker and muted: sat {} luma {}", mean_sat(&o), mean_luma(&o)); assert!(g(2.0, [0.0; 3])[2] > 0.015, "moody lifts and cools black");
    let o = look(3.0, 1.0, &c); assert!(mean_luma(&o) > l0 + 0.03 && mean_sat(&o) >= s0, "clean bright is brighter: {} vs {l0}", mean_luma(&o));
    let w = g(3.0, [0.97, 0.97, 0.97]); let w1 = g(3.0, [1.0, 1.0, 1.0]); assert!(w1[1] <= 1.0 && w1[1] > w[1], "clean bright keeps near-whites apart");
    let (b, w) = (g(4.0, [0.0; 3]), g(4.0, [1.0; 3])); assert!(luma(b) > 0.02 && luma(w) < 0.99, "film fades black and rolls off white: {b:?} {w:?}");
    let o = look(5.0, 1.0, &c); assert!(mean_sat(&o) < s0 * 0.85 && luma(g(5.0, [0.0; 3])) > 0.03, "vintage is faded");
    for v in look(6.0, 1.0, &c) { assert!((v[0] - v[1]).abs() < 0.02 && (v[1] - v[2]).abs() < 0.02, "noir is (near) black and white: {v:?}"); }
    let o = look(7.0, 1.0, &c); let d = g(7.0, [0.2, 0.2, 0.2]); assert!(mean_sat(&o) > s0 * 1.1 && d[2] > d[1] + 0.02, "neon is saturated with blue-magenta shadows: {d:?}");
}

#[test] fn bad_values_are_refused_and_frame_untouched() {
    for kv in [vec![("preset", 8.0)], vec![("preset", 1.5)], vec![("preset", -1.0)], vec![("intensity", 1.2)], vec![("intensity", f64::NAN)], vec![("protect_skin", -0.1)]] {
        let c = chart(); let mut f = frame(&c); let before = f.d.clone(); let e = run(&pr(&kv), &mut f); assert!(e.is_err(), "{kv:?} must be refused"); assert_eq!(f.d, before, "{kv:?}: frame untouched"); }
}
