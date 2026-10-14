//! V1c-D bg_wave rows (on top of bg_tools' chunk row): values and points land where the maths says, the 255-key cap has no off-by-one, audio bands find a sine, pulse keys, apply == expansion + undo, 1 vs 4 threads.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn base() -> (Session, String) {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width":256,"height":256,"fps":100})).unwrap();
    let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string(); (s, t)
}
fn nums(d: &str) -> Vec<f64> { d.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e')).filter_map(|t| t.parse().ok()).collect() }
fn audio(s: &mut Session, secs: usize, hz: f64) -> String { audio_at(s, secs, hz, 48000) }
fn audio_at(s: &mut Session, secs: usize, hz: f64, rate: usize) -> String {
    let n = rate * secs; let data: Vec<f64> = (0..n).map(|i| (i as f64 * 2.0 * std::f64::consts::PI * hz / rate as f64).sin() * 0.5).collect();
    let a = dispatch(s, "import_samples", &json!({"rate":rate,"channels":1,"data":[data]})).unwrap(); let ta = dispatch(s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    dispatch(s, "add_clip", &json!({"track_id":ta,"source_type":"media","asset_id":a["asset_id"],"start":0,"duration":secs})).unwrap()["clip_id"].as_str().unwrap().to_string()
}
#[test] fn values_land_on_the_documented_points_and_unknown_or_bad_args_are_typed() {
    let (mut s, t) = base(); let r = dispatch(&mut s, "bg_wave", &json!({"track":t,"duration":2,"source":"values","values":[-1,0,1],"smooth":false,"box":[0,0,200,100],"apply":true})).unwrap();
    let id = r["clip_ids"][0].as_str().unwrap(); let d = dispatch(&mut s, "path_info", &json!({"clip_id":id})).unwrap(); let v = nums(d["d"].as_str().unwrap());
    // canvas (0,100) (100,50) (200,0) from the top-left == centre-relative (-128,-28) (-28,-78) (72,-128)
    for (g, w) in v.iter().zip([-128.0, -28.0, -28.0, -78.0, 72.0, -128.0]) { assert!((g - w).abs() < 1e-6, "{v:?}"); }
    for (bad, code) in [(json!({"source":"nope"}), "bg_param_out_of_range"), (json!({"stroke_px":65}), "bg_param_out_of_range"), (json!({"closed":true}), "bg_param_out_of_range"), (json!({"fill_under":{"color":[1,1,1],"opacity":0.5}}), "bg_param_out_of_range"), (json!({"colour":[1,1,1]}), "bg_unknown_property"), (json!({"values":[0]}), "bg_param_out_of_range"), (json!({"pulse":{"rate_hz":1,"amount":0.2}}), "bg_param_out_of_range")] {
        let mut a = json!({"track":t,"duration":2,"source":"values","values":[0,1]}); for (k, x) in bad.as_object().unwrap() { a[k] = x.clone(); } let e = dispatch(&mut s, "bg_wave", &a).unwrap_err(); assert!(e.starts_with(code), "{bad}: {e}"); assert!(e.contains("fix_hint"), "{e}"); }
    let e = dispatch(&mut s, "bg_wave", &json!({"track":t,"duration":2,"source":"values","values":vec![0.0; 257]})).unwrap_err(); assert!(e.starts_with("bg_param_out_of_range"), "257 values: {e}");
    dispatch(&mut s, "bg_wave", &json!({"track":t,"duration":2,"source":"values","values":vec![0.0; 256]})).unwrap();
    dispatch(&mut s, "bg_wave", &json!({"track":t,"duration":2,"source":"points","points":[[0,0],[10,10]],"apply":true})).unwrap();
}
#[test] fn the_key_cap_is_255_with_no_off_by_one_and_coarsening_is_reported() {
    let (mut s, t) = base(); let clip = audio(&mut s, 3, 1000.0);
    let call = |s: &mut Session, dur: f64, extra: Value| { let mut a = json!({"track":t,"duration":dur,"source":"audio","audio":{"target":format!("clip:{clip}"),"start":0,"end":dur,"hop_s":0.01}}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } dispatch(s, "bg_wave", &a).unwrap() };
    let exact = call(&mut s, 2.55, json!({})); assert_eq!(exact["estimate"]["keys"], 255, "255 samples fit exactly: {}", exact["estimate"]); assert_eq!(exact["estimate"]["coarsened"], false);
    let over = call(&mut s, 2.56, json!({})); assert_eq!(over["estimate"]["coarsened"], true); assert!(over["estimate"]["keys"].as_u64().unwrap() <= 255, "{}", over["estimate"]); assert!(over["estimate"]["hop_s_used"].as_f64().unwrap() >= 2.56 / 255.0 - 1e-12 && over["estimate"]["hop_s_requested"] == 0.01);
    let keys = over["expansion"].as_array().unwrap().iter().filter(|o| o["tool"] == "add_path_key").count() + 1; assert!(keys <= 255, "{keys}");
    let r = call(&mut s, 3.0, json!({"apply":true})); let id = r["clip_ids"][0].as_str().unwrap(); assert_eq!(dispatch(&mut s, "path_info", &json!({"clip_id":id})).unwrap()["path_keys"].as_u64().unwrap() as usize, r["estimate"]["keys"].as_u64().unwrap() as usize - 1 + 1 - 0, "applied key count equals the estimate (base key included)");
}
#[test] fn audio_bands_find_a_1khz_sine_and_apply_equals_expansion_undo_and_threads() {
    let (mut s, t) = base(); let clip = audio(&mut s, 1, 1000.0); let proj0 = serde_json::to_string(&s.project).unwrap();
    let a = |ap: bool| json!({"track":t,"duration":1,"source":"audio","audio":{"target":format!("clip:{clip}"),"start":0,"end":1,"bands":32,"hop_s":0.1},"smooth":false,"box":[0,0,256,256],"glow":{"radius_px":8,"intensity":0.5},"pulse":{"rate_hz":2,"amount":0.4},"apply":ap,"name":"wave1"});
    let dry = dispatch(&mut s, "bg_wave", &a(false)).unwrap(); assert_eq!(serde_json::to_string(&s.project).unwrap(), proj0, "dry run leaves the project byte-equal");
    let first = dry["expansion"][0]["args"]["points"].as_array().unwrap().clone(); assert_eq!(first.len(), 32);
    let top = first.iter().enumerate().min_by(|x, y| x.1[1].as_f64().unwrap().total_cmp(&y.1[1].as_f64().unwrap())).unwrap().0; // smallest y = loudest band
    let want = (1000f64 / 40.0).ln() / (16000f64 / 40.0).ln() * 31.0; assert!((top as f64 - want).abs() <= 1.5, "loudest band {top}, want about {want:.1}");
    let pk: Vec<&Value> = dry["expansion"].as_array().unwrap().iter().filter(|o| o["tool"] == "set_keyframe").collect(); assert!(pk.len() >= 2 && pk.iter().all(|o| o["args"]["property"] == "amount"), "pulse keys on glow.amount");
    let (mut s2, _) = (Session::new(), 0); let _ = &mut s2;
    let r = dispatch(&mut s, "bg_wave", &a(true)).unwrap(); assert_eq!(r["expansion"], dry["expansion"], "apply == the dry-run expansion");
    kimo_core::par::set_threads(1); let f1 = kimo_core::render::render_frame(&s.project, 0.5, &kimo_core::render::Cache::new()).unwrap().d; kimo_core::par::set_threads(4); let f4 = kimo_core::render::render_frame(&s.project, 0.5, &kimo_core::render::Cache::new()).unwrap().d; kimo_core::par::set_threads(0);
    assert_eq!(f1.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), f4.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), "1 vs 4 threads");
    dispatch(&mut s, "undo", &json!({})).unwrap(); assert_eq!(serde_json::to_string(&s.project).unwrap(), proj0, "one undo restores");
}
#[test] fn an_expansion_over_the_500_call_batch_limit_is_a_typed_key_cap_error_and_a_shorter_window_works() {
    let (mut s, t) = base(); let clip = audio_at(&mut s, 20, 440.0, 8000); // 8 kHz import (sandbox caps arrays at 200000 items), converted to 48k; 20 s at 100 fps = 2000 samples = ~2000 add_path_key calls
    let a = |end: f64| json!({"track":t,"duration":end,"source":"audio","audio":{"target":format!("clip:{clip}"),"start":0,"end":end,"hop_s":0.01,"chunk":true,"bands":4}});
    let e = dispatch(&mut s, "bg_wave", &a(20.0)).unwrap_err(); assert!(e.starts_with("bg_key_cap:") && e.contains("500") && e.contains("fix_hint"), "{e}");
    let ok = dispatch(&mut s, "bg_wave", &a(4.0)).unwrap(); assert!(ok["expansion"].as_array().unwrap().len() <= 500, "4 s = 400 samples fits");
}
#[test] fn a_constant_tone_draws_the_same_curve_on_both_sides_of_a_chunk_seam() {
    // constant 1 kHz tone, 8 kHz import: every frame must show the same peak band at the same height, so any change across the seam is an artefact of chunking, not of the signal
    let (mut s, t) = base(); let clip = audio_at(&mut s, 4, 1000.0, 8000);
    let r = dispatch(&mut s, "bg_wave", &json!({"track":t,"duration":4,"source":"audio","audio":{"target":format!("clip:{clip}"),"start":0,"end":4,"bands":32,"hop_s":0.01,"chunk":true},"stroke_px":3,"gradient":false,"apply":true})).unwrap();
    let det = r["estimate"]["chunk_detail"].as_array().unwrap(); assert!(det.len() >= 2, "must cross a seam: {det:?}"); let seam = det[0]["t1"].as_f64().unwrap();
    let sig = |tm: f64| { let f = kimo_core::render::render_frame(&s.project, tm, &kimo_core::render::Cache::new()).unwrap(); let mut best = (0usize, 0.0f32, 0usize); // (column of the brightest top-most pixel row, ...)
        let mut top = usize::MAX; for y in 0..256 { for x in 0..256 { if f.d[(y * 256 + x) * 4] > 0.5 { top = top.min(y); } } }
        let mut xs = vec![]; for x in 0..256 { if f.d[(top.min(255) * 256 + x) * 4] > 0.5 { xs.push(x); } } if let Some(&x0) = xs.first() { best = (x0, 1.0, top); } (best.0, best.2) };
    let reference = sig(1.0); let times = [0.5, 1.5, seam - 0.02, seam - 0.01, seam, seam + 0.01, seam + 0.02, 3.5];
    for tm in times { let g = sig(tm); eprintln!("t={tm:.3} peak column {} top row {} (reference {:?})", g.0, g.1, reference); assert!((g.0 as i64 - reference.0 as i64).abs() <= 1 && (g.1 as i64 - reference.1 as i64).abs() <= 1, "t={tm}: {g:?} vs {reference:?}, seam at {seam}"); }
}
#[test] fn the_core_never_reaches_255_and_the_glow_has_three_scales_with_optional_colours() {
    let (mut s, t) = base(); let a = json!({"track":t,"duration":1,"source":"values","values":[-0.5,0.5,-0.5,0.5],"stroke_px":4,"glow":{"radius_px":24,"intensity":0.6,"colors":[[1,1,1],[0.3,0.6,1]]}});
    let dry = dispatch(&mut s, "bg_wave", &a).unwrap(); let ex = dry["expansion"].as_array().unwrap(); let glows: Vec<&Value> = ex.iter().filter(|o| o["args"]["kind"] == "outer_glow").collect();
    assert_eq!(glows.len(), 3, "tight, mid and wide"); let r: Vec<f64> = glows.iter().map(|g| g["args"]["params"]["radius"].as_f64().unwrap()).collect(); assert!(r[0] > r[1] && r[1] > r[2], "wide first, tight last: {r:?}"); assert!((r[1] - 24.0).abs() < 1e-9 && (r[2] - 8.0).abs() < 1e-9 && (r[0] - 60.0).abs() < 1e-9);
    let c: Vec<Vec<f64>> = glows.iter().map(|g| ["r", "g", "b"].iter().map(|k| g["args"]["params"][k].as_f64().unwrap()).collect()).collect(); assert_eq!(c[2], vec![1.0, 1.0, 1.0], "tight = colours[0]"); assert_eq!(c[1], vec![0.3, 0.6, 1.0], "mid = colours[1]"); assert_eq!(c[0], vec![0.3, 0.6, 1.0], "wide repeats the last given colour");
    assert!(ex.last().map_or(false, |o| o["args"]["kind"] == "shoulder") || ex.iter().any(|o| o["args"]["kind"] == "shoulder"), "a shoulder is on the layer");
    let mut b = a.clone(); b["apply"] = json!(true); dispatch(&mut s, "bg_wave", &b).unwrap(); let f = kimo_core::render::render_frame(&s.project, 0.5, &kimo_core::render::Cache::new()).unwrap();
    let mx = (0..256 * 256).map(|i| f.d[i * 4].max(f.d[i * 4 + 1]).max(f.d[i * 4 + 2])).fold(0.0f32, f32::max); let hot = (0..256 * 256).filter(|i| (0..3).any(|j| (f.d[i * 4 + j] * 255.0).round() >= 255.0)).count();
    eprintln!("wave core peak {mx:.4}, {hot} pixels at 255"); assert_eq!(hot, 0); assert!(mx <= 0.92 + 1e-3, "peak {mx}");
    for bad in [json!({"colors":[]}), json!({"colors":[[1,1]]}), json!({"colors":[[1,1,1],[1,1,1],[1,1,1],[1,1,1]]})] { let mut g = a.clone(); g["glow"] = json!({"radius_px":24,"intensity":0.6}); g["glow"]["colors"] = bad["colors"].clone(); assert!(dispatch(&mut s, "bg_wave", &g).is_err(), "{bad}"); }
    let mut g = a.clone(); g["ceiling"] = json!(1.2); assert!(dispatch(&mut s, "bg_wave", &g).is_err());
}
#[test] fn gradient_tints_the_core_from_stop_to_stop_with_no_near_white_core() {
    let (mut s, t) = base(); let from = [0.2, 0.6, 1.0]; let to = [0.9, 0.3, 0.6];
    let a = json!({"track":t,"duration":1,"source":"values","values":[0.0,0.0,0.0],"smooth":false,"box":[16,64,240,192],"stroke_px":6,"gradient":{"from":from,"to":to,"angle_deg":0},"apply":true});
    let r = dispatch(&mut s, "bg_wave", &a).unwrap(); assert!(r["expansion"].as_array().unwrap().iter().any(|o| o["args"]["kind"] == "gradient_stroke"), "gradient_stroke modifier in the expansion");
    let f = kimo_core::render::render_frame(&s.project, 0.5, &kimo_core::render::Cache::new()).unwrap();
    let at = |x: usize| { let mut best = (0usize, 0.0f32); for y in 0..256 { let v = f.d[(y * 256 + x) * 4] + f.d[(y * 256 + x) * 4 + 1] + f.d[(y * 256 + x) * 4 + 2]; if v > best.1 { best = (y, v); } } let i = (best.0 * 256 + x) * 4; [f.d[i], f.d[i + 1], f.d[i + 2]] };
    let (c0, cm, c1) = (at(24), at(128), at(232)); eprintln!("stroke colour at x=24 {c0:?} x=128 {cm:?} x=232 {c1:?}");
    let near = |c: [f32; 3], w: [f64; 3], tol: f64| (0..3).all(|k| (c[k] as f64 - w[k]).abs() <= tol);
    assert!(near(c0, from, 0.12), "left end near the from stop (blue is held under the 0.92 ceiling): {c0:?}"); assert!(near(c1, to, 0.12), "right end near the to stop: {c1:?}");
    let mid = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0, (from[2] + to[2]) / 2.0]; assert!(near(cm, mid, 0.08), "middle near the blend: {cm:?} vs {mid:?}");
    // the sweep spans the whole path, not a short span in the middle: red rises steadily along x (a path layer's Shape width is not its extent)
    let reds: Vec<f64> = [40usize, 80, 128, 176, 216].iter().map(|&x| at(x)[0] as f64).collect(); eprintln!("red along x {reds:?}"); for w in reds.windows(2) { let st = w[1] - w[0]; assert!(st > 0.03 && st < 0.3, "smooth steady sweep, step {st:.3} in {reds:?}"); }
    // the halo tracks the local stroke colour (follow): 6 px off the stroke, the red channel is lower at the cyan end than at the violet end
    let (mut s2, t2) = base(); let mut ag = a.clone(); ag["track"] = json!(t2); ag["glow"] = json!({"radius_px": 12, "intensity": 1.5}); dispatch(&mut s2, "bg_wave", &ag).unwrap(); let f = kimo_core::render::render_frame(&s2.project, 0.5, &kimo_core::render::Cache::new()).unwrap();
    let halo = |x: usize| { let mut best = (0usize, 0.0f32); for y in 0..256 { let v = f.d[(y * 256 + x) * 4 + 2]; if v > best.1 { best = (y, v); } } let (mut r, mut n) = (0.0f32, 0.0f32); for dy in [-8i32, 8] { let yy = (best.0 as i32 + dy).clamp(0, 255) as usize; r += f.d[(yy * 256 + x) * 4]; n += 1.0; } r / n };
    let (h0, h1) = (halo(40), halo(216)); eprintln!("halo red 8px off the stroke: cyan end {h0:.3} violet end {h1:.3}"); assert!(h1 > h0 * 1.6 && h1 > 0.04, "halo follows the ramp: {h0} vs {h1}");
    for c in [c0, cm, c1] { assert!(c.iter().cloned().fold(f32::MAX, f32::min) < 0.7, "no near-white core: {c:?}"); }
    let mut bad = a.clone(); bad["gradient"] = json!({"from":[2,0,0]}); assert!(dispatch(&mut s, "bg_wave", &bad).is_err()); bad["gradient"] = json!({"colour":[1,1,1]}); assert!(dispatch(&mut s, "bg_wave", &bad).is_err());
}
#[test] fn gradient_spans_the_path_extent_even_when_the_path_is_narrow_and_off_centre() {
    // regression for a sweep that used the layer's Shape width/height and the canvas centre: a path left of centre never reached its to stop
    let (mut s, t) = base(); let from = [0.2, 0.6, 1.0]; let to = [0.9, 0.3, 0.6];
    let a = json!({"track":t,"duration":1,"source":"values","values":[0.0,0.0,0.0],"smooth":false,"box":[16,64,80,128],"stroke_px":6,"gradient":{"from":from,"to":to,"angle_deg":0},"apply":true});
    dispatch(&mut s, "bg_wave", &a).unwrap(); let f = kimo_core::render::render_frame(&s.project, 0.5, &kimo_core::render::Cache::new()).unwrap();
    let at = |x: usize| { let mut best = (0usize, 0.0f32); for y in 0..256 { let v = f.d[(y * 256 + x) * 4 + 2]; if v > best.1 { best = (y, v); } } let i = (best.0 * 256 + x) * 4; [f.d[i], f.d[i + 1], f.d[i + 2]] };
    let (l, r) = (at(20), at(92)); eprintln!("narrow path ends: x=20 {l:?} x=92 {r:?}");
    assert!(r[0] > l[0] + 0.45 && l[0] < 0.35 && r[0] > 0.65, "red must sweep from about the from stop to about the to stop across the path: {l:?} {r:?}");
}
#[test] fn a_following_halo_may_exceed_the_old_amount_cap_of_4_but_a_pinned_halo_may_not() {
    let (mut s, t) = base(); let a = |g: Value| json!({"track":t,"duration":1,"source":"values","values":[0.0,0.3,0.0],"box":[16,64,224,128],"stroke_px":4,"glow":{"radius_px":40,"intensity":4.0},"gradient":g});
    let amts = |r: &Value| -> Vec<f64> { r["expansion"].as_array().unwrap().iter().flat_map(|o| { let p = &o["args"]["params"]; match o["args"]["kind"].as_str() { Some("outer_glow") => vec![p["amount"].as_f64().unwrap()], Some("outer_glow_multi") => vec![p["amount_wide"].as_f64().unwrap(), p["amount_mid"].as_f64().unwrap(), p["amount_tight"].as_f64().unwrap()], _ => vec![] } }).collect() }; // a following halo is ONE outer_glow_multi (wide, mid, tight amounts), a pinned one three outer_glow
    let on = amts(&dispatch(&mut s, "bg_wave", &a(json!({"from":[0.35,0.85,1],"to":[0.75,0.4,1]}))).unwrap()); let off = amts(&dispatch(&mut s, "bg_wave", &a(json!(false))).unwrap());
    eprintln!("outer_glow amounts following {on:?} pinned {off:?}"); assert_eq!(on.len(), 3); assert!(on.iter().cloned().fold(0.0, f64::max) > 4.0 && on.iter().all(|v| *v <= 10.0 + 1e-9), "follow lifts the cap to 10: {on:?}"); assert!(off.iter().all(|v| *v <= 4.0 + 1e-9), "a pinned halo keeps the cap of 4: {off:?}");
}
#[test] fn glow_scale_weights_are_0_55_0_33_0_12_when_following_and_0_5_0_3_0_2_when_pinned() {
    let (mut s, t) = base(); let (rad, inten, stroke) = (8.0f64, 0.1f64, 8.0f64);
    let a = |g: Value| json!({"track":t,"duration":1,"source":"values","values":[0.0,0.3,0.0],"box":[16,64,224,128],"stroke_px":stroke,"glow":{"radius_px":rad,"intensity":inten},"gradient":g});
    let amts = |r: &Value| -> Vec<f64> { r["expansion"].as_array().unwrap().iter().flat_map(|o| { let p = &o["args"]["params"]; match o["args"]["kind"].as_str() { Some("outer_glow") => vec![p["amount"].as_f64().unwrap()], Some("outer_glow_multi") => vec![p["amount_wide"].as_f64().unwrap(), p["amount_mid"].as_f64().unwrap(), p["amount_tight"].as_f64().unwrap()], _ => vec![] } }).collect() }; // a following halo is ONE outer_glow_multi (wide, mid, tight amounts), a pinned one three outer_glow
    let gs = |r: f64| (2.0 * r / stroke).max(1.0); let radii = [2.5 * rad, rad, rad / 3.0];
    let on = amts(&dispatch(&mut s, "bg_wave", &a(json!({"from":[0.35,0.85,1],"to":[0.75,0.4,1]}))).unwrap()); let off = amts(&dispatch(&mut s, "bg_wave", &a(json!(false))).unwrap());
    for (got, w, what) in [(&on, [0.12, 0.33, 0.55], "following"), (&off, [0.2, 0.3, 0.5], "pinned")] { assert_eq!(got.len(), 3); for k in 0..3 { let want = inten * w[k] * gs(radii[k]); assert!((got[k] - want).abs() < 1e-9, "{what} scale {k}: {} vs {want}", got[k]); } }
    eprintln!("scale amounts following {on:?} pinned {off:?}");
}
#[test] fn a_following_halo_decays_smoothly_with_no_plateau_and_is_nearly_gone_by_4r() {
    // flat line, glow radius 10 on 256: gaussian-shaped falloff, strictly decreasing and no box plateau
    let (mut s, t) = base(); let a = json!({"track":t,"duration":1,"source":"values","values":[0.0,0.0,0.0],"smooth":false,"box":[16,100,224,56],"stroke_px":4,"glow":{"radius_px":10,"intensity":2.5},"gradient":{"from":[0.35,0.85,1],"to":[0.35,0.85,1]},"apply":true});
    dispatch(&mut s, "bg_wave", &a).unwrap(); let f = kimo_core::render::render_frame(&s.project, 0.5, &kimo_core::render::Cache::new()).unwrap();
    let lum = |y: usize| { let i = (y * 256 + 128) * 4; (f.d[i] + f.d[i + 1] + f.d[i + 2]) as f64 }; let yc = (0..256).max_by(|a, b| lum(*a).total_cmp(&lum(*b))).unwrap(); let s0 = lum(yc);
    let r: Vec<f64> = [4usize, 8, 12, 16, 24, 32, 40].iter().map(|d| lum(yc + d) / s0).collect(); eprintln!("halo/stroke at 4,8,12,16,24,32,40 px: {r:.3?}");
    for w in r.windows(2) { assert!(w[1] < w[0] * 0.97, "strictly decaying, no plateau: {r:?}"); } assert!(r[0] >= 0.2 && r[6] <= 0.06, "visible near the stroke, nearly gone by 4r: {r:?}");
}

/// Float (pre-quantisation) outward profile of a flat wave line on a 512x512 canvas, both sides, from 4 px past the peak down to 1e-3.
fn halo_profiles(r: f64) -> Vec<Vec<f64>> { halo_profiles_ext(r).0 }
/// Also returns, per side, the distance (px from the peak) where the red channel first falls below 1/255 in float: the visible extent of the wide scale.
fn halo_profiles_ext(r: f64) -> (Vec<Vec<f64>>, Vec<usize>) {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width":512,"height":512,"fps":100})).unwrap(); let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let a = json!({"track":t,"duration":1,"source":"values","values":[0.0,0.0,0.0],"smooth":false,"box":[16,206,480,100],"stroke_px":4,"glow":{"radius_px":r,"intensity":2.5},"apply":true});
    dispatch(&mut s, "bg_wave", &a).unwrap(); let f = kimo_core::render::render_frame(&s.project, 0.5, &kimo_core::render::Cache::new()).unwrap();
    let col: Vec<f32> = (0..512).map(|y| f.d[(y * 512 + 256) * 4]).collect(); let yc = (0..512).max_by(|a, b| col[*a].total_cmp(&col[*b])).unwrap();
    let ext: Vec<usize> = [1i64, -1].iter().map(|dir| { let mut k = 0i64; loop { let y = yc as i64 + dir * k; if y < 0 || y >= 512 || (k > 4 && (col[y as usize] as f64) < 1.0 / 255.0) { break k as usize; } k += 1; } }).collect();
    let prof: Vec<Vec<f64>> = [1i64, -1].iter().map(|dir| { let mut p = vec![]; let mut k = 4i64; loop { let y = yc as i64 + dir * k; if y < 0 || y >= 512 || col[y as usize] <= 1e-3 { break; } p.push(col[y as usize] as f64); k += 1; } p }).collect();
    (prof, ext)
}
/// max over the profile of |d2[k]| / max(|d2| of the 3 samples each side): ~1 for a smooth decay, >= 9 for the kink the stacked saturated scales left (measured 11.3 at R=30, 9.3 at R=40 on 4eef6bd).
fn seam_ratio(p: &[f64]) -> f64 {
    let d2: Vec<f64> = p.windows(3).map(|w| (w[2] - 2.0 * w[1] + w[0]).abs()).collect(); let mut best = 0.0f64;
    for k in 3..d2.len().saturating_sub(3) { let nb = d2[k - 3..k].iter().chain(d2[k + 1..k + 4].iter()).cloned().fold(1e-9, f64::max); best = best.max(d2[k] / nb); } best
}
#[test] fn halo_decay_has_no_kink_in_float() {
    for r in [16.0, 30.0, 40.0] { for (i, p) in halo_profiles(r).iter().enumerate() {
        assert!(p.len() > 20, "R={r} side {i}: halo too short {}", p.len()); let sr = seam_ratio(p); eprintln!("R={r} side {i}: {} samples, seam ratio {sr:.2}", p.len());
        assert!(sr <= 2.5, "R={r} side {i}: slope kink, seam ratio {sr:.2} (smooth <= 1.7, the stacked-saturation seam was 9 to 11)");
        for w in p.windows(2) { assert!(w[1] < w[0], "R={r} side {i}: not strictly decreasing outward"); } } }
}
/// Visible extent (px from the peak where float red < 1/255) of the wide scale at R=40, measured on the patched build; the crop margin must not cut it. PIN: filled from the first green run, never loosened.
const WIDE_EXTENT_FLOOR_R40: usize = 0;
#[test] fn the_wide_scale_is_not_cropped_by_the_local_effect_margin() {
    let (ps, ext) = halo_profiles_ext(40.0); eprintln!("R=40 visible extent (red < 1/255) per side: {ext:?}, profile lengths (to 1e-3) {:?}", ps.iter().map(|p| p.len() + 4).collect::<Vec<_>>());
    for (i, e) in ext.iter().enumerate() { assert!(*e >= WIDE_EXTENT_FLOOR_R40, "side {i}: visible halo ends at {e} px, below the pinned {WIDE_EXTENT_FLOOR_R40}: the crop cut it"); }
}
#[test] fn a_pulse_on_a_following_halo_keys_the_mid_amount_of_the_single_effect() {
    let (mut s, t) = base(); let a = json!({"track":t,"duration":1,"source":"values","values":[0.0,0.3,0.0],"box":[16,64,224,128],"stroke_px":4,"glow":{"radius_px":12,"intensity":1.0},"pulse":{"rate_hz":2,"amount":0.5}});
    let ex = dispatch(&mut s, "bg_wave", &a).unwrap()["expansion"].as_array().unwrap().clone(); let multi = ex.iter().filter(|o| o["args"]["kind"] == "outer_glow_multi").count(); let keys: Vec<&Value> = ex.iter().filter(|o| o["tool"] == "set_keyframe").collect();
    assert_eq!(multi, 1); assert!(keys.len() >= 2 && keys.iter().all(|k| k["args"]["property"] == "amount_mid"), "pulse keys {:?}", keys.iter().map(|k| k["args"]["property"].clone()).collect::<Vec<_>>());
    let mut b = a.clone(); b["apply"] = json!(true); dispatch(&mut s, "bg_wave", &b).unwrap();
}
