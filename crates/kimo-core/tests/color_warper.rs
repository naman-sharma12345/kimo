//! color_warper (Resolve Color Warper, hue-saturation): up to four pins, each picks a colour (hue, saturation) with a hue and
//! saturation reach and moves it to another hue/saturation. Pixels move by the Gaussian-weighted pin moves in polar hue-sat
//! (overlapping pins share, never add up past one move), greys stay put (moves fade out below saturation 0.1), Rec.709 luma
//! is kept (keep_luma), alpha kept; no pin moved or amount 0 is an exact identity; show_matte views the pins' reach.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::json;
use std::collections::BTreeMap;

fn pr(kv: &[(&str, f64)]) -> Params { let mut p = Params::new(); for (k, v) in kv { p.insert(k.to_string(), Prop::new(*v)); } p }
fn run(p: &Params, f: &mut Frame) -> Result<(), String> { kimo_core::effects::apply("color_warper", p, &BTreeMap::new(), f, 0.0, 0) }
fn frame(px: &[[f32; 3]]) -> Frame { let mut d = Vec::new(); for c in px { d.extend_from_slice(&[c[0], c[1], c[2], 0.75]); } Frame { w: px.len(), h: 1, d } }
fn warp(kv: &[(&str, f64)], px: &[[f32; 3]]) -> Vec<[f32; 3]> { let mut f = frame(px); run(&pr(kv), &mut f).unwrap_or_else(|e| panic!("color_warper {kv:?}: {e}"));
    for a in f.d.chunks_exact(4) { assert_eq!(a[3], 0.75, "alpha kept"); } f.d.chunks_exact(4).map(|c| [c[0], c[1], c[2]]).collect() }
fn hsv(c: [f32; 3]) -> (f32, f32, f32) { let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2])); let d = mx - mn; if d <= 0.0 { return (0.0, 0.0, mx); }
    let h = if mx == c[0] { 60.0 * ((c[1] - c[2]) / d) } else if mx == c[1] { 60.0 * ((c[2] - c[0]) / d + 2.0) } else { 60.0 * ((c[0] - c[1]) / d + 4.0) }; (h.rem_euclid(360.0), d / mx, mx) }
fn from_hsv(h: f32, s: f32, v: f32) -> [f32; 3] { let c = v * s; let hp = h.rem_euclid(360.0) / 60.0; let x = c * (1.0 - (hp % 2.0 - 1.0).abs()); let m = v - c;
    let (r, g, b) = match hp as i32 { 0 => (c, x, 0.0), 1 => (x, c, 0.0), 2 => (0.0, c, x), 3 => (0.0, x, c), 4 => (x, 0.0, c), _ => (c, 0.0, x) }; [r + m, g + m, b + m] }
fn luma(c: [f32; 3]) -> f32 { 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] }
fn gap(a: f32, b: f32) -> f32 { let d = (a - b).rem_euclid(360.0); d.min(360.0 - d) }
const RED_TO_BLUE: [(&str, f64); 6] = [("p1_hue", 0.0), ("p1_sat", 0.8), ("p1_to_hue", 240.0), ("p1_to_sat", 0.8), ("p1_hue_range", 20.0), ("p1_sat_range", 0.4)];

#[test] fn color_warper_is_an_effect_kind_and_identity_when_unmoved() {
    let mut ss = Session::new(); dispatch(&mut ss, "project_set", &json!({"width": 64, "height": 64, "fps": 25})).unwrap();
    let tr = dispatch(&mut ss, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let id = dispatch(&mut ss, "add_clip", &json!({"track_id": tr, "source_type": "solid", "color": [0.8, 0.2, 0.2], "start": 0, "duration": 1})).unwrap()["clip_id"].as_str().unwrap().to_string();
    let a = json!({"clip_id": id, "kind": "color_warper", "params": {"p1_hue": 0, "p1_sat": 0.8, "p1_to_hue": 240, "p1_to_sat": 0.8}});
    dispatch(&mut ss, "add_effect", &a).unwrap_or_else(|e| panic!("add_effect {a}: {e}"));
    let px: Vec<[f32; 3]> = (0..72).map(|i| from_hsv(i as f32 * 5.0, 0.2 + (i % 4) as f32 * 0.25, 0.3 + (i % 3) as f32 * 0.3)).collect();
    assert_eq!(warp(&[], &px), px, "no pin moved: exact identity");
    let mut kv = RED_TO_BLUE.to_vec(); kv.push(("amount", 0.0)); assert_eq!(warp(&kv, &px), px, "amount 0: exact identity");
}

#[test] fn a_pin_moves_its_colour_and_spares_the_rest() {
    let red = from_hsv(0.0, 0.8, 0.8); let orange = from_hsv(30.0, 0.8, 0.8); let green = from_hsv(120.0, 0.8, 0.6); let blue = from_hsv(240.0, 0.7, 0.7);
    let px = [red, orange, green, blue, [0.5, 0.5, 0.5], [1.0, 1.0, 1.0], [0.0, 0.0, 0.0]]; let o = warp(&RED_TO_BLUE, &px);
    let (h, s, _) = hsv(o[0]); assert!(gap(h, 240.0) < 3.0 && (s - 0.8).abs() < 0.06, "red lands on blue: hue {h} sat {s}");
    assert!((luma(o[0]) - luma(red)).abs() < 0.005, "luma kept: {} -> {}", luma(red), luma(o[0]));
    let moved = gap(hsv(o[1]).0, 30.0); assert!(moved > 10.0 && moved < 80.0, "orange, 1.5 reaches away, moves part way: {moved} deg");
    assert!(gap(hsv(o[2]).0, 120.0) < 1.0 && gap(hsv(o[3]).0, 240.0) < 1.0, "green and blue stay: {:?} {:?}", hsv(o[2]), hsv(o[3]));
    for k in 4..7 { assert_eq!(o[k], px[k], "greys, white and black are untouched"); }
}

#[test] fn the_warp_is_smooth_and_pins_share() {
    let sweep: Vec<[f32; 3]> = (0..360).map(|i| from_hsv(i as f32, 0.8, 0.8)).collect(); let o = warp(&RED_TO_BLUE, &sweep);
    let mut worst = 0.0f32; for k in 0..360 { let j = (k + 1) % 360; worst = worst.max(gap(hsv(o[k]).0, hsv(o[j]).0)); } assert!(worst < 6.0, "no tears in the hue sweep: worst step {worst} deg");
    let sats: Vec<[f32; 3]> = (0..100).map(|i| from_hsv(0.0, i as f32 / 99.0, 0.8)).collect(); let o = warp(&RED_TO_BLUE, &sats);
    for k in 1..100 { let (a, b) = (o[k - 1], o[k]); let d = ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt(); assert!(d < 0.08, "no tears along saturation at {k}: {d}"); }
    let mut kv = RED_TO_BLUE.to_vec(); kv.extend_from_slice(&[("p2_hue", 0.0), ("p2_sat", 0.8), ("p2_to_hue", 240.0), ("p2_to_sat", 0.8), ("p2_hue_range", 20.0), ("p2_sat_range", 0.4)]);
    let (h, _, _) = hsv(warp(&kv, &[from_hsv(0.0, 0.8, 0.8)])[0]); assert!(gap(h, 240.0) < 3.0, "two identical pins share the move (no double move): hue {h}");
}

#[test] fn a_saturation_pin_mutes_only_its_hue() {
    let kv = [("p3_hue", 120.0), ("p3_sat", 0.7), ("p3_to_hue", 120.0), ("p3_to_sat", 0.2), ("p3_hue_range", 30.0), ("p3_sat_range", 0.5)];
    let (green, red) = (from_hsv(120.0, 0.7, 0.6), from_hsv(0.0, 0.7, 0.6)); let o = warp(&kv, &[green, red]);
    assert!(hsv(o[0]).1 < 0.3 && gap(hsv(o[0]).0, 120.0) < 2.0, "green is muted, same hue: {:?}", hsv(o[0]));
    assert!((hsv(o[1]).1 - 0.7).abs() < 0.01 && gap(hsv(o[1]).0, 0.0) < 0.5, "red keeps its saturation: {:?}", hsv(o[1]));
    let mut f = frame(&[green, [0.4, 0.4, 0.4]]); let mut m = kv.to_vec(); m.push(("show_matte", 1.0)); run(&pr(&m), &mut f).unwrap();
    assert!(f.d[0] > 0.9 && f.d[4] == 0.0, "show_matte: white at the pin, black on grey ({}, {})", f.d[0], f.d[4]);
}

#[test] fn bad_values_are_refused_and_frame_untouched() {
    for kv in [vec![("p1_hue_range", 0.0)], vec![("p2_sat", 1.5)], vec![("p1_to_sat", -0.1)], vec![("amount", 2.0)], vec![("p4_to_hue", f64::NAN)], vec![("p1_sat_range", -1.0)]] {
        let px = [from_hsv(10.0, 0.7, 0.7), [0.3, 0.3, 0.3]]; let mut f = frame(&px); let before = f.d.clone(); assert!(run(&pr(&kv), &mut f).is_err(), "{kv:?} must be refused"); assert_eq!(f.d, before, "{kv:?}: untouched"); }
}
