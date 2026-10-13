//! E1: the highlight shoulder. Additive light stacks used to clip flat at 255; the shoulder rolls them off below a ceiling with no plateau.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{apply, Frame, Params};
use serde_json::{json, Value};
fn sh(vals: &[[f32; 3]], start: f64, ceil: f64) -> Vec<[f32; 3]> {
    let mut f = Frame { w: vals.len(), h: 1, d: vals.iter().flat_map(|c| [c[0], c[1], c[2], 0.5]).collect() };
    let mut p = Params::new(); p.insert("start".into(), kimo_core::keyframe::Prop::new(start)); p.insert("ceiling".into(), kimo_core::keyframe::Prop::new(ceil)); apply("shoulder", &p, &Default::default(), &mut f, 0.0, 0).unwrap();
    assert!(f.d.chunks(4).all(|c| c[3] == 0.5), "alpha is untouched"); f.d.chunks(4).map(|c| [c[0], c[1], c[2]]).collect()
}
#[test] fn identity_below_start_slope_one_at_start_monotone_and_capped() {
    let ramp: Vec<[f32; 3]> = (0..=400).map(|i| { let v = i as f32 * 0.01; [v, v, v] }).collect(); let o = sh(&ramp, 0.8, 0.96);
    for i in 0..=80 { assert_eq!(o[i][0], ramp[i][0], "unchanged at or below start (i={i})"); }
    let slope = (o[82][0] - o[81][0]) / 0.01; assert!((slope - 1.0).abs() < 0.12 && slope < 1.0, "slope just above start {slope}");
    for i in 1..o.len() { if i <= 200 { assert!(o[i][0] > o[i - 1][0], "strictly increasing, no plateau, up to 2.0: i={i} {} vs {}", o[i][0], o[i - 1][0]); } else { assert!(o[i][0] >= o[i - 1][0] - 1e-6 && (o[i][0] - 0.96).abs() < 5e-3, "past 2.0 only f32 ulp noise next to the ceiling: i={i}"); } } assert!(o.iter().all(|c| c[0] < 0.96 + 1e-6), "never above the ceiling"); assert!(o[400][0] > 0.95, "approaches the ceiling");
    let d = (o[400][0] - o[300][0]) as f64; assert!(d > 0.0 || o[400][0] == o[300][0]);
}
#[test] fn hue_is_kept_and_degenerate_input_is_safe() {
    let o = sh(&[[2.0, 1.0, 0.5], [0.0; 3], [f32::NAN, 0.0, 0.0]], 0.8, 0.96); let r = o[0]; assert!((r[1] / r[0] - 0.5).abs() < 1e-5 && (r[2] / r[0] - 0.25).abs() < 1e-5, "rgb ratios kept {r:?}"); assert_eq!(o[1], [0.0; 3]);
    let _ = sh(&[[1.0; 3]], 5.0, -3.0); let _ = sh(&[[1.0; 3]], 0.8, 0.8);
}
fn frac_at_255(s: &Session) -> (f64, f64) { let f = kimo_core::render::render_frame(&s.project, 0.0, &kimo_core::render::Cache::new()).unwrap(); let n = f.w * f.h; let hot = (0..n).filter(|i| (0..3).any(|j| (f.d[i * 4 + j] * 255.0).round() >= 255.0)).count(); let mx = (0..n).map(|i| f.d[i * 4].max(f.d[i * 4 + 1]).max(f.d[i * 4 + 2])).fold(0.0f32, f32::max); (hot as f64 / n as f64, mx as f64) }
#[test] fn a_three_orb_additive_stack_blows_out_without_the_shoulder_and_not_with_it() {
    let mk = |shoulder: bool| { let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width":256,"height":256,"fps":25})).unwrap(); let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
        let orbs: Value = json!([{"cx":110,"cy":120,"radius":90,"color":[1,0.6,0.3],"intensity":2.0},{"cx":150,"cy":130,"radius":90,"color":[0.4,0.7,1],"intensity":2.0},{"cx":128,"cy":100,"radius":80,"color":[0.8,0.4,1],"intensity":2.0}]);
        dispatch(&mut s, "bg_orbs", &json!({"track":t,"duration":1,"combine":"add","dither":0,"vignette":0,"grain":0,"ceiling":if shoulder { 0.92 } else { 1.0 },"orbs":orbs,"apply":true})).unwrap();
        if false { let id = s.project.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| !c.effects.is_empty()).map(|c| c.id.clone()).unwrap(); dispatch(&mut s, "add_effect", &json!({"clip_id":id,"kind":"shoulder"})).unwrap(); } s };
    let (raw, rmax) = frac_at_255(&mk(false)); let (soft, smax) = frac_at_255(&mk(true));
    eprintln!("255 pixels: {:.3}% -> {:.3}%, peak {rmax:.3} -> {smax:.3}", raw * 100.0, soft * 100.0);
    assert!(raw > 0.005, "the fixture must blow out without the shoulder: {raw}"); assert!(soft <= 0.005, "with the shoulder at most 0.5% of pixels at 255: {soft}"); assert!(smax <= 0.92 + 1e-3, "peak {smax} must stay under the 0.92 default ceiling (235 of 255)");
}
#[test] fn a_saturated_additive_colour_keeps_its_chromaticity_and_stays_under_the_ceiling() {
    for c in [[0.05f32, 0.1, 3.0], [4.0, 0.3, 0.1], [0.2, 2.5, 0.4], [1.5, 1.4, 1.3]] { let o = sh(&[c], 0.8, 0.92)[0]; let mx = o[0].max(o[1]).max(o[2]);
        assert!(mx <= 0.92 + 1e-6, "{c:?} -> {o:?} passes the ceiling"); for j in 0..3 { for k in 0..3 { if c[k] > 0.0 && o[k] > 0.0 { assert!(((o[j] / o[k]) / (c[j] / c[k]) - 1.0).abs() < 1e-4, "{c:?} -> {o:?}: channel ratio {j}/{k} changed"); } } } }
    // saturation (max - min over max) of a strongly saturated input is unchanged, i.e. no pastel bleaching
    let o = sh(&[[4.0, 0.3, 0.1]], 0.8, 0.92)[0]; let s_in = (4.0f32 - 0.1) / 4.0; let s_out = (o[0] - o[2]) / o[0]; assert!((s_in - s_out).abs() < 1e-4, "saturation {s_in} -> {s_out}");
}
