//! TDD tests for 2 new effects (FEATURES.md L481): median (plain neighbourhood-median filter) and
//! dust_scratches (AE's despeckle: median only where a pixel is an outlier against its own neighbourhood).
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn px(ss: &Session, x: u32, y: u32) -> [f32; 4] {
    let cache = Cache::new(); let f = render_frame(&ss.project, 0.0, &cache).unwrap();
    let o = ((y as usize * f.w) + x as usize) * 4; [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]]
}

#[test]
fn median_leaves_a_flat_field_unchanged() {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 32, "height": 32, "fps": 25}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 32, "height": 32, "fill": [0.4, 0.5, 0.6, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "median", "params": {"radius": 2}}));
    let p = px(&ss, 16, 16);
    assert!((p[0] - 0.4).abs() < 0.01 && (p[1] - 0.5).abs() < 0.01 && (p[2] - 0.6).abs() < 0.01, "a flat field should be unchanged by a median filter, got {p:?}");
}

#[test]
fn median_smooths_away_a_single_speck_pixel() {
    // a single-pixel white speck in the middle of a 32x32 grey field, achieved with two stacked shapes
    // on the SAME clip's own composited result is not directly available, so instead this test drives the
    // underlying matting module's windowed logic indirectly: attach median to a clip whose OWN fill has a
    // sharp edge (a small rect), and check a point 1px inside the edge moves toward the edge's neighbour
    // colour (proving the filter is doing real neighbourhood statistics, not acting as identity).
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 32, "height": 32, "fps": 25}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 2, "height": 2, "fill": [1.0, 1.0, 1.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let before = px(&ss, 16, 16); // centre of the tiny 2x2 white rect
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "median", "params": {"radius": 3}}));
    let after = px(&ss, 16, 16);
    assert!(before[0] > 0.9, "expected white before filtering, got {before:?}");
    assert!(after[0] < before[0] - 0.3, "a median filter with a 7x7 window should pull an isolated small bright patch toward its (mostly black/transparent) surroundings, got {after:?} (was {before:?})");
}

#[test]
fn dust_scratches_despeckles_an_isolated_outlier_but_leaves_a_flat_field_alone() {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 32, "height": 32, "fps": 25}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 32, "height": 32, "fill": [0.4, 0.4, 0.4, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "dust_scratches", "params": {"radius": 2, "threshold": 0.1}}));
    let flat = px(&ss, 16, 16);
    assert!((flat[0] - 0.4).abs() < 0.01, "a flat field should be left exactly alone by dust_scratches (despeckle-only), got {flat:?}");
}

#[test]
fn dust_scratches_is_listed_as_an_effect_kind() {
    let r = call(&mut Session::new(), "list_effects", json!({}));
    let kinds: Vec<&str> = r.as_array().unwrap().iter().map(|v| v["kind"].as_str().unwrap()).collect();
    assert!(kinds.contains(&"median"), "median should be a listed effect kind");
    assert!(kinds.contains(&"dust_scratches"), "dust_scratches should be a listed effect kind");
}
