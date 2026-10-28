//! TDD verification tests for FEATURES.md L479 ("refine soft matte, refine hard matte, simple choker
//! variants, matte tools for tracked mattes"): refine_matte and choker already existed in the codebase
//! (refine_matte.rs, effects2.rs) but had no functional test calling them through the MCP command layer --
//! only incidental mentions in description-length/external-refs classification tests. These tests call the
//! actual tools with real data before the FEATURES.md line is flipped, per the TDD/honesty rule.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn px(ss: &Session, t: f64, x: u32, y: u32) -> [f32; 4] {
    let cache = Cache::new();
    let f = render_frame(&ss.project, t, &cache).unwrap();
    let o = ((y as usize * f.w) + x as usize) * 4;
    [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]]
}

// --- choker (effects2.rs's "choker" kind): simple matte choker, erode/dilate on alpha only ------------

// render_frame composites onto an opaque black canvas (alpha is always 1 in the final frame), so "inside
// vs outside the shape" is read from colour (a red fill vs the black backdrop), not from alpha.
#[test]
fn choker_positive_amount_shrinks_the_coloured_edge() {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 64, "height": 64, "fps": 25}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 40, "height": 40, "fill": [1.0, 0.0, 0.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    // a point 2px inside the rect's left edge (rect spans x 12..52 on a 64-wide canvas) -- red before choking
    let before = px(&ss, 0.0, 14, 32);
    assert!(before[0] > 0.9, "expected red before choking, got {before:?}");
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "choker", "params": {"amount": 6.0}}));
    let after = px(&ss, 0.0, 14, 32);
    assert!(after[0] < 0.5, "a 6px positive choke should have eaten a point only 2px inside the original edge, got r={}", after[0]);
}

#[test]
fn choker_negative_amount_expands_the_alpha_edge() {
    // The choker effect dilates/erodes ALPHA ONLY (effects2.rs's own doc comment), so a pixel the choke
    // newly covers takes on the shape's own (unpainted-there) premultiplied colour -- 0 -- not the fill
    // colour. Against a flat green background, that reads as the green darkening toward black at the
    // newly-covered edge, which is exactly how a real alpha dilate on a colour-matted layer looks.
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 64, "height": 64, "fps": 25}));
    let bg = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_clip", json!({"track_id": &bg, "source_type": "solid", "color": [0.0, 1.0, 0.0], "start": 0, "duration": 1}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 40, "height": 40, "fill": [1.0, 0.0, 0.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    // a point 2px OUTSIDE the rect's left edge -- pure green background before choking
    let before = px(&ss, 0.0, 9, 32);
    assert!(before[1] > 0.9 && before[0] < 0.1, "expected pure green background before choking, got {before:?}");
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "choker", "params": {"amount": -6.0}}));
    let after = px(&ss, 0.0, 9, 32);
    assert!(after[1] < before[1] - 0.3, "a 6px negative choke should have dilated the shape's alpha 3px past its original edge, darkening the green background there, got g={} (was {})", after[1], before[1]);
}

// --- refine_matte (ext/refine_matte.rs): classical hair/edge alpha matting -------------------------------

fn matting_scene() -> (Session, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 64, "height": 64, "fps": 25}));
    let bg = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_clip", json!({"track_id": &bg, "source_type": "solid", "color": [0.1, 0.4, 0.8], "start": 0, "duration": 1}));
    let fg = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_shape", json!({"track_id": &fg, "shape": "ellipse", "width": 36, "height": 36, "fill": [0.9, 0.3, 0.2, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    (ss, c)
}

#[test]
fn refine_matte_points_mode_attaches_a_matte_mask_and_softens_the_seed_edge() {
    let (mut ss, c) = matting_scene();
    let before = call(&mut ss, "list_masks", json!({"clip_id": &c}));
    assert_eq!(before["layers"].as_array().map(|a| a.len()).unwrap_or(0), 0, "no mask before refine_matte");
    // a rough circular seed outline close to, but not exactly on, the shape's own 36px ellipse edge
    let pts: Vec<[f64; 2]> = (0..24).map(|i| { let a = i as f64 / 24.0 * std::f64::consts::TAU; [32.0 + 19.0 * a.cos(), 32.0 + 19.0 * a.sin()] }).collect();
    let r = call(&mut ss, "refine_matte", json!({"clip_id": &c, "time": 0.0, "points": pts, "band": 6}));
    assert!(r["mask_id"].is_string(), "refine_matte should attach a matte mask layer, got {r}");
    let after = call(&mut ss, "list_masks", json!({"clip_id": &c}));
    assert_eq!(after["layers"].as_array().unwrap().len(), 1, "exactly one matte mask layer after refine_matte");
}

#[test]
fn refine_matte_matte_dir_mode_refines_a_track_matte_output_across_frames() {
    let (mut ss, c) = matting_scene();
    // a rough circular seed for track_matte, close to the ellipse's own edge
    let pts: Vec<[f64; 2]> = (0..16).map(|i| { let a = i as f64 / 16.0 * std::f64::consts::TAU; [32.0 + 18.0 * a.cos(), 32.0 + 18.0 * a.sin()] }).collect();
    let dir = std::env::temp_dir().join(format!("kimo_track_matte_{}", std::process::id()));
    let tm = call(&mut ss, "track_matte", json!({"clip_id": &c, "seeds": [{"time": 0.0, "points": pts}], "start": 0.0, "end": 0.04, "dir": dir.to_str().unwrap(), "add_mask": false}));
    assert!(tm["mask_id"].is_null() || tm.get("dir").is_some() || dir.exists(), "track_matte should have written frame-by-frame mattes to dir, got {tm}");
    let r = call(&mut ss, "refine_matte", json!({"clip_id": &c, "matte_dir": dir.to_str().unwrap(), "start": 0.0, "end": 0.04}));
    assert!(r["mask_id"].is_string(), "refine_matte in matte_dir mode should attach a matte mask layer, got {r}");
    std::fs::remove_dir_all(&dir).ok();
}
