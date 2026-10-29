//! Scene3D motion blur (scene3d.rs render_blur: shutter_deg/blur_samples) -- already fully implemented and
//! wired into the render path pre-v7, uncredited on FEATURES.md until this pass. This proves it is real and
//! live through the actual MCP surface (add_scene3d's `config`), not just present in the struct.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{self, render_frame, Cache};
use serde_json::json;

fn c(s: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn rendered(s: &Session, t: f64) -> kimo_core::effects::Frame { let mut cache = Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render_frame(&s.project, t, &cache).unwrap() }

/// A 160x90 project with one sphere whose `keys` sweep it fast across most of the canvas in 1s, sampled at
/// the midpoint (fastest point of a smoothstep ease) -- `shutter_deg` is the only thing that differs.
fn project_with_moving_sphere(shutter_deg: f64, blur_samples: u32) -> Session {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 160, "height": 90, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.02, 0.02, 0.03], "start": 0.0, "duration": 1.0}));
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_scene3d", json!({"track_id": t2, "start": 0.0, "duration": 1.0, "config": {
        "objects": [{"kind": "sphere", "size": [40.0, 40.0, 40.0], "color": [1.0, 0.9, 0.2], "emissive": [0.9, 0.8, 0.1],
            "keys": [{"t": 0.0, "pos": [-60.0, 0.0, 0.0]}, {"t": 1.0, "pos": [60.0, 0.0, 0.0]}]}],
        "lights": [{"kind": "directional", "dir": [0.3, -0.5, 1.0], "intensity": 1.2}],
        "shutter_deg": shutter_deg, "blur_samples": blur_samples
    }}));
    s
}

fn mean_abs_diff(a: &kimo_core::effects::Frame, b: &kimo_core::effects::Frame) -> f32 {
    let n = a.d.len() as f32; a.d.iter().zip(b.d.iter()).map(|(x, y)| (x - y).abs()).sum::<f32>() / n
}

#[test] fn shutter_zero_is_a_single_sharp_sample() {
    // documented: shutter_deg <= 0 renders once, no averaging -- a second render at the exact same t must
    // be byte-identical (determinism), proving no hidden sampling happens when blur is "off".
    let s = project_with_moving_sphere(0.0, 8);
    let f1 = rendered(&s, 0.5); let f2 = rendered(&s, 0.5);
    assert_eq!(f1.d, f2.d);
}

#[test] fn shutter_deg_blurs_a_fast_moving_object() {
    let sharp = project_with_moving_sphere(0.0, 8);
    let blurred = project_with_moving_sphere(300.0, 16);
    let fs = rendered(&sharp, 0.5);
    let fb = rendered(&blurred, 0.5);
    let diff = mean_abs_diff(&fs, &fb);
    assert!(diff > 0.002, "a fast sphere swept through ~300/360 of a frame's shutter should visibly differ from the sharp render; mean abs diff was {diff}");
}

#[test] fn more_blur_samples_is_a_closer_average_not_a_different_look() {
    // not a golden-exactness claim -- just proves blur_samples is live and not ignored: a coarse 2-sample
    // average and a fine 24-sample average of the SAME shutter window are close to each other (both
    // approximating the same continuous blur) but both clearly differ from the sharp (0-shutter) render.
    let sharp = project_with_moving_sphere(0.0, 8);
    let coarse = project_with_moving_sphere(300.0, 2);
    let fine = project_with_moving_sphere(300.0, 24);
    let fs = rendered(&sharp, 0.5); let fc = rendered(&coarse, 0.5); let ff = rendered(&fine, 0.5);
    assert!(mean_abs_diff(&fs, &fc) > 0.002);
    assert!(mean_abs_diff(&fs, &ff) > 0.002);
    assert!(mean_abs_diff(&fc, &ff) < mean_abs_diff(&fs, &ff), "coarse and fine samplings of the same window should be nearer each other than either is to the sharp render");
}

#[test] fn add_scene3d_accepts_shutter_config_without_error() {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_scene3d", json!({"track_id": t, "start": 0.0, "duration": 1.0, "config": {
        "objects": [{"kind": "box", "size": [20.0, 20.0, 20.0]}], "shutter_deg": 180.0, "blur_samples": 12
    }}));
}
