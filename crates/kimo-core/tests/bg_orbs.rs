//! V1c-D bg_orbs rows: centre intensity, falloff shapes, gaussian at d=sigma, add commutative, dither (amp 0 identical, seeded, bounded, banding measured), orbit and keys analytic,
//! tile seam (128 canvas == top-left of 256 canvas), 1 vs 4 threads, apply == expansion + undo, op counts.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn base_wh(w: u32, h: u32) -> (Session, String) {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width":w,"height":h,"fps":25})).unwrap();
    let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string(); (s, t)
}
fn orbs(extra: Value, orbs: Value) -> (Session, Value) {
    let (mut s, t) = base_wh(256, 256); let mut a = json!({"track":t,"vignette":0,"grain":0,"ceiling":1,"duration":5,"apply":true,"dither":0,"base":{"type":"solid","c1":[0,0,0]},"orbs":orbs});
    for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } let r = dispatch(&mut s, "bg_orbs", &a).unwrap_or_else(|e| panic!("bg_orbs: {e}")); (s, r)
}
fn frame(s: &Session, t: f64) -> Vec<f32> { kimo_core::render::render_frame(&s.project, t, &kimo_core::render::Cache::new()).unwrap().d }
fn px(d: &[f32], x: usize, y: usize, c: usize) -> f32 { d[(y * 256 + x) * 4 + c] }
fn bits(d: &[f32]) -> Vec<u32> { d.iter().map(|v| v.to_bits()).collect() }
const FALL: [&str; 3] = ["gaussian", "inverse_square", "smooth"];
const LSB: f32 = 1.0 / 255.0;
fn one(extra: Value) -> serde_json::Value { let mut o = json!({"cx":100.5,"cy":100.5,"radius":30,"color":[1,0.5,0],"intensity":1}); for (k, v) in extra.as_object().unwrap() { o[k] = v.clone(); } o }
#[test] fn centre_pixel_is_colour_times_intensity() {
    let (s, _) = orbs(json!({}), json!([one(json!({"intensity":0.5}))])); let d = frame(&s, 0.0);
    for (c, want) in [(0, 0.5), (1, 0.25), (2, 0.0)] { assert!((px(&d, 100, 100, c) - want).abs() <= LSB, "channel {c}: {}", px(&d, 100, 100, c)); }
}
#[test] fn gaussian_at_one_sigma_is_e_to_minus_half_and_every_falloff_is_monotone() {
    let (s, _) = orbs(json!({}), json!([one(json!({}))])); let d = frame(&s, 0.0);
    let got = px(&d, 130, 100, 0); assert!((got - (-0.5f32).exp()).abs() < 1e-3, "gaussian at d = sigma: {got}"); // sigma^2 -> sigma in the exponent would give e^-15
    for f in ["gaussian", "inverse_square", "smooth"] {
        let (s, _) = orbs(json!({}), json!([one(json!({"falloff":f}))])); let d = frame(&s, 0.0); let row: Vec<f32> = (100..160).map(|x| px(&d, x, 100, 0)).collect();
        for w in row.windows(2) { assert!(w[1] <= w[0] + 1e-6, "{f} falls off: {row:?}"); } assert!(row[0] > 0.99 && row[59] < row[0] * 0.5, "{f}");
        if f == "inverse_square" { assert!((px(&d, 130, 100, 0) - 0.5).abs() < 2e-3, "inverse square at d = sigma is 0.5"); }
        if f == "smooth" { assert!(px(&d, 130, 100, 0) < 2e-3, "smooth reaches 0 at sigma"); }
    }
}
#[test] fn add_is_commutative_for_three_orbs_in_any_order() {
    let o = [one(json!({"cx":60.5})), one(json!({"cx":120.5,"color":[0.1,0.4,1]})), one(json!({"cx":180.5,"cy":150.5,"falloff":"smooth"}))];
    let r = |v: Vec<Value>| bits(&frame(&orbs(json!({}), json!(v)).0, 0.5));
    let a = r(vec![o[0].clone(), o[1].clone(), o[2].clone()]); assert_eq!(a, r(vec![o[2].clone(), o[0].clone(), o[1].clone()])); assert_eq!(a, r(vec![o[1].clone(), o[2].clone(), o[0].clone()]));
    let (s, _) = orbs(json!({"combine":"screen"}), json!(o)); let d = frame(&s, 0.0); assert!(d.iter().all(|v| *v <= 1.0 + 1e-6), "screen never exceeds 1");
}
#[test] fn dither_zero_is_identical_seeded_repeatable_bounded_and_reduces_banding() {
    let grad = json!({"type":"gradient","c1":[0.20,0.20,0.20],"c2":[0.24,0.24,0.24]});
    let mk = |amp: f64, seed: u64| { let (mut s, t) = base_wh(256, 256); dispatch(&mut s, "bg_orbs", &json!({"track":t,"vignette":0,"grain":0,"ceiling":1,"duration":5,"apply":true,"dither":amp,"seed":seed,"base":grad,"orbs":[{"cx":-9000,"cy":0,"radius":1}]})).unwrap(); frame(&s, 0.0) };
    let plain = mk(0.0, 1); let (_, r0) = (0, { let (mut s, t) = base_wh(256, 256); dispatch(&mut s, "bg_orbs", &json!({"track":t,"vignette":0,"grain":0,"ceiling":1,"duration":5,"dither":0,"base":grad,"orbs":[{"cx":-9000,"cy":0,"radius":1}]})).unwrap() });
    assert!(!r0["expansion"].to_string().contains("noise"), "dither 0 adds no noise effect"); assert_eq!(bits(&plain), bits(&mk(0.0, 7)), "amp 0 ignores the seed");
    let d1 = mk(1.0, 5); assert_eq!(bits(&d1), bits(&mk(1.0, 5)), "seeded and repeatable"); assert_ne!(bits(&d1), bits(&mk(1.0, 6)), "another seed differs");
    let worst = plain.iter().zip(&d1).enumerate().filter(|(i, _)| i % 4 < 3).map(|(_, (a, b))| (a - b).abs()).fold(0f32, f32::max); assert!(worst <= 1.0 * LSB * 1.001, "max |delta| {worst} <= 1 LSB");
    // banding: per row, how far the x-average of the 8-bit quantised row is from the true value (rows are constant, so the dithered average should be closer)
    let err = |d: &[f32]| -> f32 { (0..256).map(|y| { let q: f32 = (0..256).map(|x| (px(d, x, y, 0) * 255.0).round() / 255.0).sum::<f32>() / 256.0; (q - px(&plain, 0, y, 0)).abs() }).sum::<f32>() / 256.0 };
    let (e0, e1) = (err(&plain), err(&d1)); eprintln!("banding error (mean |row average - true|, in LSB): none {:.4}, dither 1.0 {:.4}", e0 / LSB, e1 / LSB); assert!(e1 < e0 * 0.5, "dither reduces banding: {e0} -> {e1}");
}
#[test] fn orbit_moves_the_peak_analytically_and_keys_hit_their_positions() {
    let peak = |s: &Session, t: f64| { let d = frame(s, t); let mut best = (0usize, 0usize, -1f32); for y in 0..256 { for x in 0..256 { let v = px(&d, x, y, 0); if v > best.2 { best = (x, y, v); } } } (best.0, best.1) };
    // x = cx + rx*cos(2 pi rate t + phase): rate 0.25 Hz -> t=0 at cx+50, t=1 at cx, t=2 at cx-50
    let (s, _) = orbs(json!({}), json!([one(json!({"cx":100.5,"cy":100.5,"radius":20,"orbit":{"rx":50,"ry":0,"rate_hz":0.25,"phase":0}}))]));
    assert_eq!(peak(&s, 0.0), (150, 100)); assert_eq!(peak(&s, 1.0), (100, 100)); assert_eq!(peak(&s, 2.0), (50, 100));
    let (s, _) = orbs(json!({}), json!([one(json!({"radius":20,"keys":[{"t":0,"cx":40.5,"cy":50.5},{"t":2,"cx":200.5,"cy":150.5}]}))]));
    assert_eq!(peak(&s, 0.0), (40, 50)); assert_eq!(peak(&s, 1.0), (120, 100)); assert_eq!(peak(&s, 2.0), (200, 150));
}
#[test] fn tile_seam_a_128_canvas_equals_the_top_left_of_the_256_canvas_with_dither() {
    let spec = |t: &str| json!({"track":t,"vignette":0,"grain":0,"ceiling":1,"duration":5,"apply":true,"dither":1.0,"seed":3,"orbs":[one(json!({"radius":60})), one(json!({"cx":30.5,"cy":90.5,"color":[0.2,0.5,1]}))]});
    let (mut big, tb) = base_wh(256, 256); dispatch(&mut big, "bg_orbs", &spec(&tb)).unwrap(); let (mut sm, ts) = base_wh(128, 128); dispatch(&mut sm, "bg_orbs", &spec(&ts)).unwrap();
    let (b, m) = (frame(&big, 0.0), kimo_core::render::render_frame(&sm.project, 0.0, &kimo_core::render::Cache::new()).unwrap().d);
    for y in 0..128 { for x in 0..128 { for c in 0..4 { assert_eq!(m[(y * 128 + x) * 4 + c].to_bits(), px(&b, x, y, c).to_bits(), "pixel ({x},{y}) channel {c}"); } } }
}
#[test] fn apply_equals_the_expansion_by_hand_one_undo_restores_and_threads_agree() {
    let (mut s1, t) = base_wh(256, 256); let a = |ap: bool| json!({"track":t,"vignette":0,"grain":0,"ceiling":1,"duration":3,"apply":ap,"name":"glow1","tags":["hero"],"blend":"screen","opacity":0.8,"base":{"type":"gradient","c1":[0.1,0,0.2],"c2":[0,0,0]},"orbs":[one(json!({"orbit":{"rx":20,"ry":10,"rate_hz":0.5,"phase":1}})), one(json!({"cx":200.5,"keys":[{"t":0,"cx":10,"cy":10},{"t":1,"cx":90,"cy":50}]}))]});
    let proj0 = serde_json::to_string(&s1.project).unwrap(); let r = dispatch(&mut s1, "bg_orbs", &a(false)).unwrap(); assert_eq!(r["applied"], false); assert_eq!(serde_json::to_string(&s1.project).unwrap(), proj0, "dry run leaves the project byte-equal");
    dispatch(&mut s1, "batch", &json!({"ops":r["expansion"].clone(),"atomic":true})).unwrap();
    let (mut s2, _) = base_wh(256, 256); let r2 = dispatch(&mut s2, "bg_orbs", &a(true)).unwrap(); assert_eq!(r2["expansion"], r["expansion"]); assert_eq!(serde_json::to_string(&s1.project.tracks).unwrap(), serde_json::to_string(&s2.project.tracks).unwrap(), "apply == expansion by hand");
    kimo_core::par::set_threads(1); let f1 = frame(&s2, 0.7); kimo_core::par::set_threads(4); let f4 = frame(&s2, 0.7); kimo_core::par::set_threads(0); assert_eq!(bits(&f1), bits(&f4), "1 vs 4 threads");
    dispatch(&mut s2, "undo", &json!({})).unwrap(); assert_eq!(serde_json::to_string(&s2.project).unwrap(), proj0, "one undo restores");
}
#[test] fn sixteen_orbs_op_counts_are_measured_and_below_the_default_limit() {
    let v: Vec<Value> = (0..16).map(|i| one(json!({"cx":10.5 + 12.0 * i as f64,"falloff":FALL[i % 3],"orbit":{"rx":5,"ry":5,"rate_hz":0.1,"phase":0.5}}))).collect();
    let (_, r) = orbs(json!({"dither":1.0}), json!(v)); let (w, tot) = (r["estimate"]["expr_ops"].as_u64().unwrap(), r["estimate"]["expr_ops_total"].as_u64().unwrap());
    eprintln!("16 orbs: worst single expression {w} ops, total {tot} ops over {} effects (limit 512 per expression)", r["estimate"]["effects"]); assert!(w < 512 && r["estimate"]["effects"] == 17);
    for (i, e) in r["expansion"].as_array().unwrap().iter().filter(|o| o["tool"] == "add_effect").enumerate() { let p = kimo_core::pixel_expr::compile(e["args"]["strings"]["expr"].as_str().unwrap(), usize::MAX).unwrap(); assert!(p.ops as u64 <= w, "effect {i}"); }
}
#[test] fn premium_defaults_are_echoed_vignette_darkens_edges_and_no_pixel_reaches_255() {
    let (mut s, t) = base_wh(256, 256);
    let big = json!([{"cx":110,"cy":120,"radius":90,"color":[1,0.6,0.3],"intensity":2.0},{"cx":150,"cy":130,"radius":90,"color":[0.4,0.7,1],"intensity":2.0},{"cx":128,"cy":100,"radius":80,"color":[0.8,0.4,1],"intensity":2.0}]);
    let dry = dispatch(&mut s, "bg_orbs", &json!({"track":t,"duration":1,"combine":"add","orbs":big})).unwrap(); let p = &dry["params"];
    assert_eq!((p["vignette"].as_f64(), p["grain"].as_f64(), p["ceiling"].as_f64()), (Some(0.25), Some(0.02), Some(0.92)), "defaults echoed: {p}");
    dispatch(&mut s, "bg_orbs", &json!({"track":t,"duration":1,"combine":"add","orbs":big,"apply":true})).unwrap(); let f = frame(&s, 0.0);
    let hot = (0..256 * 256).filter(|i| (0..3).any(|j| (f[i * 4 + j] * 255.0).round() >= 255.0)).count(); let mx = (0..256 * 256).map(|i| f[i * 4].max(f[i * 4 + 1]).max(f[i * 4 + 2])).fold(0.0f32, f32::max);
    eprintln!("default stack: {hot} pixels at 255, peak {mx:.4}"); assert_eq!(hot, 0, "zero 255 pixels with the default ceiling"); assert!(mx <= 0.92 + 0.02, "peak {mx} (ceiling 0.92 plus the grain amplitude)");
    // vignette: a flat base, corners darker than the centre; vignette 0 flat
    let lum = |extra: Value| { let (mut s, t) = base_wh(256, 256); let mut a = json!({"track":t,"duration":1,"apply":true,"dither":0,"grain":0,"ceiling":1,"base":{"type":"solid","c1":[0.5,0.5,0.5]},"orbs":[{"cx":-9000,"cy":0,"radius":1}]}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } dispatch(&mut s, "bg_orbs", &a).unwrap(); let f = frame(&s, 0.0); (px(&f, 128, 128, 0), px(&f, 2, 2, 0)) };
    let (c0, e0) = lum(json!({"vignette":0})); assert_eq!(c0, e0, "vignette 0 is flat"); let (c1, e1) = lum(json!({"vignette":0.5})); eprintln!("vignette 0.5: centre {c1:.4} corner {e1:.4}"); assert!(e1 < c1 * 0.9, "corner {e1} must be darker than centre {c1}");
    for bad in [json!({"ceiling":0.4}), json!({"ceiling":1.5}), json!({"vignette":-1}), json!({"grain":0.6})] { let mut a = json!({"track":t,"duration":1,"orbs":[{"cx":10,"cy":10,"radius":5}]}); for (k, v) in bad.as_object().unwrap() { a[k] = v.clone(); } assert!(dispatch(&mut s, "bg_orbs", &a).is_err(), "{bad}"); }
}
#[test] fn depth_layers_far_mid_near_have_distinct_scale_alpha_and_parallax_and_mid_is_the_identity() {
    let mk = |layer: Option<&str>| { let mut o = json!({"cx":100.5,"cy":100.5,"radius":20,"color":[1,1,1],"intensity":1,"orbit":{"rx":10,"ry":20,"rate_hz":0.5,"phase":0}}); if let Some(l) = layer { o["layer"] = json!(l); } orbs(json!({}), json!([o])) };
    let (_, plain) = mk(None); let (sm, mid) = mk(Some("mid")); let (sf, far) = mk(Some("far")); let (sn, near) = mk(Some("near"));
    let e = |r: &Value| r["params"]["orbs"][0].clone(); assert_eq!(e(&plain)["radius"], e(&mid)["radius"], "mid is the identity"); assert_eq!(e(&plain)["intensity"], e(&mid)["intensity"]); assert_eq!(e(&plain)["orbit"], e(&mid)["orbit"]);
    let (rf, rm, rn) = (e(&far)["radius"].as_f64().unwrap(), e(&mid)["radius"].as_f64().unwrap(), e(&near)["radius"].as_f64().unwrap()); assert!(rf < rm && rm < rn, "radius far (small) < mid < near (large foreground): {rf} {rm} {rn}"); assert!((rf / rm - 0.5).abs() < 1e-9 && (rn / rm - 1.5).abs() < 1e-9);
    let (ix, im, iy) = (e(&far)["intensity"].as_f64().unwrap(), e(&mid)["intensity"].as_f64().unwrap(), e(&near)["intensity"].as_f64().unwrap()); assert!(ix < im && im < iy, "alpha far < mid < near"); assert!((ix / im - 0.55).abs() < 1e-9 && (iy / im - 2.8).abs() < 1e-9, "far 0.275 and near 1.4 when mid is 0.5");
    let (pf, pm, pn) = (e(&far)["orbit"]["rx"].as_f64().unwrap(), e(&mid)["orbit"]["rx"].as_f64().unwrap(), e(&near)["orbit"]["rx"].as_f64().unwrap()); assert!((pf / pm - 0.5).abs() < 1e-9 && (pn / pm - 1.0 / 0.6).abs() < 1e-9, "parallax ratios {pf} {pm} {pn}"); assert!((pf / pn - 0.3).abs() < 1e-9, "far : near = 0.3 : 1.0");
    let col = |o: &Value| -> Vec<f64> { o["color"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect() }; let (cf, cm) = (col(&e(&far)), col(&e(&mid))); let hz = [0.45, 0.5, 0.62]; eprintln!("aerial perspective: far colour {cf:?} from mid {cm:?}"); for k in 0..3 { assert!((cf[k] - (cm[k] * 0.7 + hz[k] * 0.3)).abs() < 1e-9, "far colour is 70% own + 30% haze, channel {k}: {cf:?} vs {cm:?}"); } assert_ne!(cf, cm); assert_eq!(e(&mid)["color"], e(&near)["color"], "mid and near colours are as given");
    assert_eq!(e(&far)["falloff"], "inverse_square", "far gets the long-tail falloff by default"); assert_eq!(e(&near)["falloff"], "gaussian");
    let peak = |s: &Session| { let f = frame(s, 0.0); (0..256 * 256).map(|i| f[i * 4]).fold(0.0f32, f32::max) }; let (a, b, c) = (peak(&sf), peak(&sm), peak(&sn)); eprintln!("layer centre peaks far {a:.3} mid {b:.3} near {c:.3}"); assert!(b >= a * 1.15 && c >= b * 1.15, "each depth step is at least 15% brighter: {a} {b} {c}");
    let (mut s, t) = base_wh(256, 256); let bad = dispatch(&mut s, "bg_orbs", &json!({"track":t,"duration":1,"orbs":[{"cx":1,"cy":1,"radius":5,"layer":"back"}]})).unwrap_err(); assert!(bad.contains("layer"), "{bad}");
}
#[test] fn multi_plane_sixteen_orbs_stay_under_the_op_limit_and_a_near_core_peaks_at_230_to_240() {
    const LAY: [&str; 3] = ["far", "mid", "near"];
    let v: Vec<Value> = (0..16).map(|i| one(json!({"cx":10.5 + 12.0 * i as f64,"layer":LAY[i % 3],"orbit":{"rx":5,"ry":5,"rate_hz":0.1,"phase":0.5}}))).collect();
    let (mut s, t) = base_wh(256, 256); let r = dispatch(&mut s, "bg_orbs", &json!({"track":t,"duration":2,"orbs":v})).unwrap(); // premium finishing defaults on
    let ex = r["expansion"].as_array().unwrap(); let w = r["estimate"]["expr_ops"].as_u64().unwrap(); eprintln!("16 layered orbs with the finishing stack: worst expression {w} ops, {} add_effect calls, {} ops in the batch", ex.iter().filter(|o| o["tool"] == "add_effect").count(), ex.len());
    assert!(w < 512, "worst expression {w}"); assert!(ex.len() < 500, "batch of {} calls must stay under the 500-call guard", ex.len());
    let (mut s2, _) = base_wh(256, 256); let t2 = s2.project.tracks[0].id.clone();
    dispatch(&mut s2, "bg_orbs", &json!({"track":t2,"duration":2,"base":{"type":"solid","c1":[0.02,0.02,0.04]},"orbs":[{"cx":128.5,"cy":128.5,"radius":60,"color":[1,1,1],"intensity":0.5,"layer":"near"}],"apply":true})).unwrap();
    let f = frame(&s2, 0.0); let mx = (0..256 * 256).map(|i| f[i * 4].max(f[i * 4 + 1]).max(f[i * 4 + 2])).fold(0.0f32, f32::max); let p8 = (mx * 255.0).round(); eprintln!("near core peak {p8} / 255 (230-240 wanted, never 255)");
    assert!((230.0..=240.0).contains(&p8), "near core peak {p8}");
}
#[test] fn dither_pattern_changes_every_frame_but_a_frame_is_repeatable() {
    // QA found adjacent frames bit-equal (a locked screen-door). The seed now carries the frame time.
    let grad = json!({"type":"gradient","c1":[0.20,0.20,0.20],"c2":[0.24,0.24,0.24]});
    let (mut s, t) = base_wh(256, 256); dispatch(&mut s, "bg_orbs", &json!({"track":t,"vignette":0,"grain":0,"ceiling":1,"duration":5,"apply":true,"dither":1.0,"seed":5,"base":grad,"orbs":[{"cx":-9000,"cy":0,"radius":1}]})).unwrap();
    let (a, a2, b, c) = (frame(&s, 0.0), frame(&s, 0.0), frame(&s, 0.01), frame(&s, 0.02)); assert_eq!(bits(&a), bits(&a2), "same frame repeats bit-equal");
    let (mut s0, t0) = base_wh(256, 256); dispatch(&mut s0, "bg_orbs", &json!({"track":t0,"vignette":0,"grain":0,"ceiling":1,"duration":5,"apply":true,"dither":0,"base":grad,"orbs":[{"cx":-9000,"cy":0,"radius":1}]})).unwrap(); let plain = frame(&s0, 0.0);
    let res = |f: &[f32]| -> Vec<f64> { (0..256 * 256).map(|i| (f[i * 4] - plain[i * 4]) as f64).collect() };
    let corr = |x: &[f64], y: &[f64]| { let n = x.len() as f64; let (mx, my) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n); let (mut sxy, mut sxx, mut syy) = (0.0, 0.0, 0.0); for i in 0..x.len() { sxy += (x[i] - mx) * (y[i] - my); sxx += (x[i] - mx).powi(2); syy += (y[i] - my).powi(2); } sxy / (sxx * syy).sqrt().max(1e-12) };
    let (ra, rb, rc) = (res(&a), res(&b), res(&c)); let (c1, c2) = (corr(&ra, &rb), corr(&rb, &rc)); eprintln!("dither residual correlation frame0-frame1 {c1:.3}, frame1-frame2 {c2:.3}");
    assert!(c1.abs() < 0.05 && c2.abs() < 0.05, "adjacent frames must not share a pattern: {c1} {c2}");
    // TPDF: zero mean, +-1 LSB peak, and triangular (P(|x| > 0.5 LSB) = 0.25, a uniform draw would give 0.5)
    let lsb = 1.0 / 255.0; let mean = ra.iter().sum::<f64>() / ra.len() as f64 / lsb; let frac = ra.iter().filter(|v| v.abs() > 0.5 * lsb).count() as f64 / ra.len() as f64; let peak = ra.iter().fold(0.0f64, |m, v| m.max(v.abs())) / lsb; eprintln!("dither residual mean {mean:.4} LSB, peak {peak:.3} LSB, P(|x|>0.5 LSB) {frac:.3}");
    assert!(mean.abs() < 0.03 && peak <= 1.001 && frac > 0.2 && frac < 0.3, "zero-mean triangular dither within 1 LSB: mean {mean} peak {peak} frac {frac}"); let diff = a.iter().zip(&b).enumerate().filter(|(i, (x, y))| i % 4 == 0 && x != y).count(); assert!(diff > 256 * 256 / 3, "adjacent frames differ in many pixels: {diff}");
}
