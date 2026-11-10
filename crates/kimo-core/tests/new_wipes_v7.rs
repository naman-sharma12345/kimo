//! TDD tests for two new transition kinds (FEATURES.md L489): block_dissolve (discrete random-order
//! block reveal, distinct from noise_dissolve's smooth per-pixel noise) and card_wipe (a left-to-right
//! "dealt" column reveal with a brief per-column horizontal squeeze standing in for AE's 3D card turn).
//!
//! Transition test pattern (matching film_burn.rs/push_wipe.rs): two tracks, the "outgoing" solid colour
//! on the bottom track spanning the whole window, the "incoming" solid colour on the top track with
//! `transition_in` attached -- the top clip's mask/op genuinely composites over the always-visible bottom
//! clip. Same-track sequential clips do NOT give a true outgoing/incoming blend in this engine (confirmed
//! by probing even the pre-existing "crossfade" kind on adjacent non-overlapping clips: it fades the
//! incoming clip in from black, not from the prior clip's content, because nothing else is "live" there).
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn scene(kind: &str, extra: serde_json::Value) -> Session {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 64, "height": 64, "fps": 25}));
    let bottom = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_clip", json!({"track_id": &bottom, "source_type": "solid", "color": [0.0, 1.0, 0.0], "start": 0, "duration": 2.0}));
    let top = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let b = call(&mut ss, "add_clip", json!({"track_id": &top, "source_type": "solid", "color": [1.0, 0.0, 0.0], "start": 0, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    let mut args = json!({"clip_id": &b, "kind": kind, "duration": 1.0}); let o = args.as_object_mut().unwrap();
    for (k, v) in extra.as_object().unwrap() { o.insert(k.clone(), v.clone()); }
    call(&mut ss, "add_transition", args);
    ss
}

fn frame_rgb_counts(ss: &Session, t: f64) -> (usize, usize) {
    let cache = Cache::new(); let f = render_frame(&ss.project, t, &cache).unwrap(); let (mut green, mut red) = (0, 0);
    for i in 0..f.w * f.h { let o = i * 4; if f.d[o + 1] > 0.5 && f.d[o] < 0.5 { green += 1; } if f.d[o] > 0.5 && f.d[o + 1] < 0.5 { red += 1; } }
    (green, red)
}

#[test]
fn block_dissolve_is_listed_and_reveals_in_discrete_blocks_not_a_smooth_edge() {
    let r = dispatch(&mut Session::new(), "list_transitions", &json!({})).unwrap();
    assert!(r.as_array().unwrap().iter().any(|v| v == "block_dissolve"), "block_dissolve should be a listed transition kind, got {r}");
    let ss = scene("block_dissolve", json!({"count": 8, "softness": 0.05}));
    let (g0, r0) = frame_rgb_counts(&ss, 0.0); // transition start: should be basically all green (outgoing, from the bottom track)
    let (g_mid, r_mid) = frame_rgb_counts(&ss, 0.5); // transition midpoint: a real mix of both colours, in blocks
    let (g1, r1) = frame_rgb_counts(&ss, 1.0); // transition end: should be basically all red (incoming, fully revealed)
    assert!(g0 > r0 * 4, "near the transition's start it should read mostly green, got green={g0} red={r0}");
    assert!(g_mid > 50 && r_mid > 50, "mid-transition should show a real patchwork of both colours (discrete blocks at different reveal times), got green={g_mid} red={r_mid}");
    assert!(r1 > g1 * 4, "after the transition it should read mostly red, got green={g1} red={r1}");
}

#[test]
fn card_wipe_is_listed_and_deals_columns_left_to_right() {
    let r = dispatch(&mut Session::new(), "list_transitions", &json!({})).unwrap();
    assert!(r.as_array().unwrap().iter().any(|v| v == "card_wipe"), "card_wipe should be a listed transition kind, got {r}");
    let ss = scene("card_wipe", json!({"count": 6, "softness": 0.05}));
    let cache = Cache::new();
    let f = render_frame(&ss.project, 0.5, &cache).unwrap(); // partway through the deal (confirmed by direct pixel probe: x=2 is red, x=w-4 is still green at this point)
    let left_red = f.d[2 * 4] > 0.5; // near x=2, the first column, should have flipped to red by now
    let right_red = f.d[(f.w - 4) * 4] > 0.5; // near the far right edge, the last column, should still be green
    assert!(left_red, "card_wipe should reveal its leftmost column before its rightmost one (dealt left to right)");
    assert!(!right_red, "the rightmost column should not have flipped yet at this point in the deal");
}
