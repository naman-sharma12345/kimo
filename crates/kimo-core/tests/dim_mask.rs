//! V1c-A: add_dim_mask / spotlight rows on a 256x256 canvas.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn base() -> Session {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":256,"height":256,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let g = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[0.9,0.2,0.1],"color2":[0.1,0.4,0.9],"start":0,"duration":5}));
    let _ = g;
    s
}
fn frame(s: &Session) -> Vec<f32> { render::render_frame(&s.project, 1.0, &render::Cache::new()).unwrap().d }
fn px(f: &[f32], x: usize, y: usize, ch: usize) -> f32 { f[(y * 256 + x) * 4 + ch] }
const BOX: [f64; 4] = [96., 96., 64., 64.];
fn with_dim(dim: f64, feather: f64) -> (Session, Vec<f32>, Vec<f32>, Value) {
    let mut s = base();
    let plain = frame(&s);
    let r = c(&mut s, "add_dim_mask", json!({"target_box":BOX,"dim_factor":dim,"feather_px":feather,"duration":5}));
    let f = frame(&s);
    (s, plain, f, r)
}
#[test]
fn inside_untouched_outside_dimmed() {
    let (_s, plain, f, _) = with_dim(0.5, 0.0);
    for (x, y) in [(110, 110), (128, 128), (150, 140)] { for ch in 0..3 { assert_eq!(px(&plain, x, y, ch), px(&f, x, y, ch), "inside ({x},{y}) ch{ch}"); } }
    for (x, y) in [(10, 10), (240, 30), (30, 200), (200, 240)] { for ch in 0..3 { let tint = [0.02f32, 0.03, 0.09][ch]; let want = px(&plain, x, y, ch) * 0.5 + 0.5 * tint; assert!((px(&f, x, y, ch) - want).abs() <= 1.0 / 255.0, "outside ({x},{y}) ch{ch}: {} vs {want}", px(&f, x, y, ch)); } }
}
#[test]
fn dim_zero_is_bit_identical() {
    let (_s, plain, f, _) = with_dim(0.0, 12.0);
    assert_eq!(plain.iter().map(|v| v.to_bits()).collect::<Vec<_>>(), f.iter().map(|v| v.to_bits()).collect::<Vec<_>>());
}
#[test]
fn feather_ramp_monotonic_and_symmetric() {
    let (_s, plain, f, _) = with_dim(1.0, 16.0);
    // darkness (1 - out/base) along the row y=128 crossing the left edge x=96 and the mirrored right edge x=160
    let dark = |x: usize| 1.0 - px(&f, x, 128, 0) / px(&plain, x, 128, 0).max(1e-6);
    let left: Vec<f32> = (70..=122).map(dark).collect();
    for w in left.windows(2) { assert!(w[1] <= w[0] + 1e-4, "left ramp must fall monotonically into the hole: {w:?}"); }
    let right: Vec<f32> = (134..=186).map(dark).collect();
    for w in right.windows(2) { assert!(w[1] >= w[0] - 1e-4, "right ramp must rise monotonically out of the hole: {w:?}"); }
    for d in 0..24 { let (l, r) = (dark(95 - d), dark(160 + d)); assert!((l - r).abs() < 0.02, "asymmetric at offset {d}: {l} vs {r}"); }
    // the dim layer is the dark blue-shifted tint (0.02, 0.03, 0.09), so full darkness is 1 - tint/base, not 1
    assert!(dark(60) > 0.9 && dark(128) < 0.03);
}
#[test]
fn feather_param_is_the_10_90_ramp_width_with_a_clean_profile_and_a_bit_equal_hole() {
    // premium spec (the 2.5% step cap holds for ramps >= 38 px: smootherstep peak slope is 0.951 / ramp per px): feather_px = 10-90% ramp width in px, smootherstep, ramp grows outward only, max adjacent step <= 2.5%
    let (_s, plain, f, _) = with_dim(1.0, 40.0);
    let tint = 0.02f32;
    let cov = |x: usize| (px(&plain, x, 128, 0) - px(&f, x, 128, 0)) / (px(&plain, x, 128, 0) - tint).max(1e-6); // 0 in the hole, 1 far outside
    let xs: Vec<usize> = (8..=96).rev().collect(); // walking left from the hole edge at x=96
    let at = |q: f32| xs.iter().position(|&x| cov(x) >= q).map(|i| xs[i] as f32);
    let w = at(0.1).unwrap() - at(0.9).unwrap();
    assert!((w.abs() - 40.0).abs() <= 1.0, "10-90 ramp width {w} px must equal feather_px 40 within 1 px");
    let max_step = xs.windows(2).map(|p| (cov(p[1]) - cov(p[0])).abs()).fold(0.0f32, f32::max);
    assert!(max_step <= 0.025, "max adjacent step {max_step} must be <= 2.5%");
    for x in 96..160 { assert_eq!(px(&plain, x, 128, 0), px(&f, x, 128, 0), "hole pixel x={x} must be bit-equal (the ramp never enters the hole)"); }
}
#[test]
fn box_clipped_to_canvas_and_typed_errors() {
    let mut s = base();
    let r = c(&mut s, "add_dim_mask", json!({"target_box":[-50., -50., 100., 100.],"dim_factor":0.5}));
    assert_eq!(r["params"]["clipped_box"], json!([0.0, 0.0, 50.0, 50.0]));
    for bad in [json!({"target_box":[0,0,0,10]}), json!({"target_box":[0,0,-5,10]}), json!({"target_box":[300,300,10,10]}), json!({"target_box":[0,0,10]}), json!({"target_box":[0,0,10,10],"feather_px":300}), json!({"target_box":[0,0,10,10],"dim_factor":1.5}), json!({})] {
        let before = serde_json::to_vec(&s.project).unwrap();
        let e = dispatch(&mut s, "add_dim_mask", &bad).unwrap_err();
        assert!(e.contains("fix_hint:") || e.starts_with("missing"), "{bad}: {e}");
        assert_eq!(before, serde_json::to_vec(&s.project).unwrap(), "a rejected call must not change the project");
    }
    let e = kimo_core::commands::dispatch_ex(&mut s, "add_dim_mask", &json!({"target_box":[0,0,0,10]})).unwrap_err();
    assert_eq!(e.code, "out_of_range");
    // JSON cannot carry a NaN number; the string form and a null-free path must both be rejected
    let nan = dispatch(&mut s, "add_dim_mask", &json!({"target_box":[0,0,10,10],"dim_factor":"NaN"}));
    assert!(nan.is_err());
}
#[test]
fn spotlight_expansion_equals_apply_and_one_undo() {
    let mut a = base();
    let mut b = base();
    let args = json!({"target_box":BOX,"dim_factor":0.4,"feather_px":8,"corner_radius":6,"ease_in":0.5,"ease_out":0.5,"duration":4});
    let before = serde_json::to_vec(&a.project).unwrap();
    let dry = c(&mut a, "spotlight", args.clone());
    assert_eq!(dry["applied"], false);
    assert_eq!(before, serde_json::to_vec(&a.project).unwrap(), "apply:false leaves the project unchanged");
    let hist = c(&mut a, "history_list", json!({}));
    let mut applied = args.clone(); applied["apply"] = json!(true);
    let r = c(&mut a, "spotlight", applied);
    assert_eq!(r["expansion"], dry["expansion"]);
    for step in dry["expansion"].as_array().unwrap() { c(&mut b, step["tool"].as_str().unwrap(), step["args"].clone()); }
    assert_eq!(serde_json::to_vec(&a.project).unwrap(), serde_json::to_vec(&b.project).unwrap(), "apply:true == applying the expansion by hand");
    assert_ne!(hist, c(&mut a, "history_list", json!({})));
    c(&mut a, "undo", json!({}));
    assert_eq!(before, serde_json::to_vec(&a.project).unwrap(), "one undo removes the whole spotlight");
}
#[test]
fn threads_1_vs_4_byte_equal() {
    let mut out = vec![];
    for th in ["1", "4"] { std::env::set_var("KIMO_THREADS", th); let (_s, _p, f, _) = with_dim(0.7, 20.0); out.push(f.iter().map(|v| v.to_bits()).collect::<Vec<_>>()); }
    assert_eq!(out[0], out[1]);
}
