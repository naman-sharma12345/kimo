//! TDD tests for 3 new effects (FEATURES.md L469): shift_channels, solid_composite, remove_color_matting.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn solid_clip(r: f64, g: f64, b: f64, a: f64) -> (Session, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 16, "height": 16, "fps": 25}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 16, "height": 16, "fill": [r, g, b, a], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    (ss, c)
}

fn px(ss: &Session) -> [f32; 4] {
    let cache = Cache::new(); let f = render_frame(&ss.project, 0.0, &cache).unwrap();
    let o = ((8 * f.w) + 8) * 4; [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]]
}

#[test]
fn shift_channels_swaps_red_and_blue() {
    let (mut ss, c) = solid_clip(0.8, 0.3, 0.1, 1.0);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "shift_channels", "params": {"take_r": 2.0, "take_g": 1.0, "take_b": 0.0, "take_a": 3.0}}));
    let p = px(&ss);
    assert!((p[0] - 0.1).abs() < 0.01 && (p[1] - 0.3).abs() < 0.01 && (p[2] - 0.8).abs() < 0.01, "expected R and B swapped (0.1, 0.3, 0.8), got {p:?}");
}

#[test]
fn shift_channels_rejects_an_out_of_range_source() {
    // shift_channels' own range check runs when the effect is actually applied (render time), not at
    // add_effect time -- add_effect only validates that the param NAMES are known, not their values.
    let (mut ss, c) = solid_clip(0.5, 0.5, 0.5, 1.0);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "shift_channels", "params": {"take_r": 42.0}}));
    let cache = Cache::new();
    let r = render_frame(&ss.project, 0.0, &cache);
    assert!(r.is_err(), "take_r: 42 is out of the 0..9 range and should be rejected at render time, got {}", if r.is_ok() { "Ok(frame)" } else { "err" });
}

#[test]
fn solid_composite_behind_fills_transparent_gaps_without_covering_the_layer() {
    // a half-opaque white layer over a solid_composite black fill (behind): the result should show the
    // white layer blended toward black exactly where its own alpha is less than 1, not replace it outright
    let (mut ss, c) = solid_clip(1.0, 1.0, 1.0, 0.4);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "solid_composite", "params": {"r": 0.0, "g": 0.0, "b": 0.0, "opacity": 1.0, "behind": 1.0}}));
    let p = px(&ss);
    // straight-alpha composite of white(a=0.4) over black: 1.0*0.4 + 0.0*0.6 = 0.4 on each channel, alpha -> 1.0
    assert!((p[0] - 0.4).abs() < 0.02 && (p[1] - 0.4).abs() < 0.02 && (p[2] - 0.4).abs() < 0.02, "expected ~0.4 on each channel (white blended over black fill), got {p:?}");
    assert!((p[3] - 1.0).abs() < 0.02, "the fill should raise alpha to opaque, got a={}", p[3]);
}

#[test]
fn remove_color_matting_undoes_a_known_composited_fringe() {
    // simulate footage that was keyed to alpha 0.5 and then composited over a known red backdrop before
    // being re-saved: observed = original*0.5 + bg*0.5. remove_color_matting with the same bg should
    // recover the true original colour INTO THE CLIP'S OWN straight-alpha storage; the clip's alpha is
    // untouched (still 0.5), so render_frame's own final composite (this clip, alone, over the project's
    // black canvas) blends it again -- the pixel actually observed is original*alpha, not original itself.
    let bg = [1.0, 0.0, 0.0]; let original = [0.2, 0.8, 0.4]; let alpha = 0.5;
    let observed: Vec<f64> = (0..3).map(|j| original[j] * alpha + bg[j] * (1.0 - alpha)).collect();
    let (mut ss, c) = solid_clip(observed[0], observed[1], observed[2], alpha);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "remove_color_matting", "params": {"r": bg[0], "g": bg[1], "b": bg[2]}}));
    let p = px(&ss);
    let expect: Vec<f64> = original.iter().map(|v| v * alpha).collect();
    assert!((p[0] as f64 - expect[0]).abs() < 0.02 && (p[1] as f64 - expect[1]).abs() < 0.02 && (p[2] as f64 - expect[2]).abs() < 0.02,
        "expected the recovered colour blended over black at alpha {alpha} ({expect:?}), got {p:?}");
}
