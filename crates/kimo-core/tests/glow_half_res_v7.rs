//! TDD tests for glow's half-res fast path (FEATURES.md L558 "u8/half-res fast path for glow/blur").
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn setup(w: u32, h: u32) -> (Session, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": w, "height": h, "fps": 25}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    // a bright square on a dark background -- gives glow something real to spread
    call(&mut ss, "add_clip", json!({"track_id": &t, "source_type": "solid", "color": "#050505", "start": 0, "duration": 1}));
    let t2 = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_shape", json!({"track_id": &t2, "shape": "rect", "width": 20, "height": 20, "fill": [3.0, 3.0, 3.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    (ss, c)
}

fn px(ss: &Session, x: u32, y: u32) -> [f32; 4] {
    let cache = Cache::new();
    let f = render_frame(&ss.project, 0.0, &cache).unwrap();
    let o = ((y as usize * f.w) + x as usize) * 4;
    [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]]
}

#[test]
fn half_res_defaults_off_and_is_byte_identical_to_omitting_the_param() {
    let (mut ss_a, c) = setup(128, 72);
    call(&mut ss_a, "add_effect", json!({"clip_id": &c, "kind": "glow", "params": {"mode": 1, "radius": 10, "amount": 1.0}}));
    let (mut ss_b, c2) = setup(128, 72);
    call(&mut ss_b, "add_effect", json!({"clip_id": &c2, "kind": "glow", "params": {"mode": 1, "radius": 10, "amount": 1.0, "half_res": 0}}));
    let pa = px(&ss_a, 40, 36); let pb = px(&ss_b, 40, 36);
    assert_eq!(pa, pb, "half_res: 0 must match the param being entirely absent");
}

#[test]
fn half_res_on_still_glows_but_differs_from_full_res() {
    let (mut ss_full, c) = setup(128, 72);
    call(&mut ss_full, "add_effect", json!({"clip_id": &c, "kind": "glow", "params": {"mode": 1, "radius": 10, "amount": 1.0}}));
    let (mut ss_half, c2) = setup(128, 72);
    call(&mut ss_half, "add_effect", json!({"clip_id": &c2, "kind": "glow", "params": {"mode": 1, "radius": 10, "amount": 1.0, "half_res": 1}}));
    // a point just outside the 20x20 square (centered on a 128x72 canvas -> roughly x 54..74, y 26..46)
    let bg_full = px(&ss_full, 80, 36);
    let bg_half = px(&ss_half, 80, 36);
    assert!(bg_full[0] > 0.04, "full-res glow should visibly brighten a point outside the square, got {bg_full:?}");
    assert!(bg_half[0] > 0.04, "half-res glow should also visibly brighten that point, got {bg_half:?}");
    assert_ne!(bg_full, bg_half, "half_res is a real distinct code path, not a silent no-op");
}

#[test]
fn half_res_runs_on_odd_dimensions_without_panicking() {
    let (mut ss, c) = setup(127, 73);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "glow", "params": {"mode": 1, "radius": 8, "amount": 1.0, "half_res": 1}}));
    let p = px(&ss, 10, 10);
    assert!(p[3] >= 0.0); // just proving it rendered without panicking
}

#[test]
fn half_res_is_keyframeable_like_any_effect_param() {
    let (mut ss, c) = setup(96, 54);
    let e = call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "glow", "params": {"mode": 1, "radius": 8, "amount": 1.0}}))["effect_id"].as_str().unwrap().to_string();
    call(&mut ss, "set_keyframe", json!({"clip_id": &c, "property": "half_res", "effect_id": &e, "time": 0.0, "value": 1.0, "ease": "linear"}));
    let v = call(&mut ss, "sample_property", json!({"clip_id": &c, "property": "half_res", "effect_id": &e, "times": [0.0]}))["values"][0].as_f64().unwrap();
    assert_eq!(v, 1.0);
}


#[test] #[ignore]
fn half_res_speedup_benchmark() {
    // Run: cargo test --release --test glow_half_res_v7 -- --ignored --nocapture half_res_speedup_benchmark
    for (w, h) in [(960u32, 540u32), (1920, 1080)] {
        let (mut ss_full, c) = setup(w, h);
        call(&mut ss_full, "add_effect", json!({"clip_id": &c, "kind": "glow", "params": {"mode": 1, "radius": 24, "amount": 1.0, "iterations": 6}}));
        let (mut ss_half, c2) = setup(w, h);
        call(&mut ss_half, "add_effect", json!({"clip_id": &c2, "kind": "glow", "params": {"mode": 1, "radius": 24, "amount": 1.0, "iterations": 6, "half_res": 1}}));
        let cache = Cache::new();
        let t0 = std::time::Instant::now(); for _ in 0..10 { std::hint::black_box(render_frame(&ss_full.project, 0.0, &cache).unwrap()); } let full_ms = t0.elapsed().as_secs_f64() * 100.0;
        let t0 = std::time::Instant::now(); for _ in 0..10 { std::hint::black_box(render_frame(&ss_half.project, 0.0, &cache).unwrap()); } let half_ms = t0.elapsed().as_secs_f64() * 100.0;
        eprintln!("GLOW {w}x{h}: full-res {full_ms:.2} ms/frame, half_res {half_ms:.2} ms/frame, speedup {:.1}x", full_ms / half_ms.max(0.001));
    }
}
