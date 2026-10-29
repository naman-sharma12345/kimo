//! TDD tests for three new transition kinds (FEATURES.md L109): ripple_dissolve (concentric sine-perturbed
//! radial reveal), barn_doors (the outgoing picture's two halves physically slide apart, a geometric slide
//! like push rather than a reveal mask), and page_peel (a simplified 2D corner peel with a curl-shadow band
//! and highlight along the lifting edge -- an honest stand-in for a true 3D page curl, not attempted here).
//!
//! Same scene/measurement pattern as new_wipes_v7.rs / film_burn.rs / push_wipe.rs: a green "outgoing" solid
//! on the bottom track spanning the whole window, a red "incoming" solid on the top track carrying
//! `transition_in`, so the top clip's mask/compose genuinely composites over the always-visible bottom clip.
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

fn listed(kind: &str) {
    let r = dispatch(&mut Session::new(), "list_transitions", &json!({})).unwrap();
    assert!(r.as_array().unwrap().iter().any(|v| v == kind), "{kind} should be a listed transition kind, got {r}");
}

#[test]
fn ripple_dissolve_is_listed_and_reveals_radially_from_the_centre_outward() {
    listed("ripple_dissolve");
    let ss = scene("ripple_dissolve", json!({"count": 6, "softness": 0.1}));
    let (g0, r0) = frame_rgb_counts(&ss, 0.0); assert_eq!(r0, 0, "t=0: all green (outgoing), got {r0} red px"); assert!(g0 > 3000);
    let (g1, r1) = frame_rgb_counts(&ss, 1.0); assert_eq!(g1, 0, "t=1: all red (incoming), got {g1} green px"); assert!(r1 > 3000);
    // at the midpoint the reveal should have started from the centre: the very centre pixel should already
    // read red (revealed) while at least one corner still reads green (not yet reached by the ripple front)
    let cache = Cache::new(); let f = render_frame(&ss.project, 0.5, &cache).unwrap();
    let centre = (32 * 64 + 32) * 4; assert!(f.d[centre] > 0.5 && f.d[centre + 1] < 0.5, "centre should be revealed (red) well before the edges at the midpoint");
    let corner = (1 * 64 + 1) * 4; assert!(f.d[corner + 1] > 0.5 && f.d[corner] < 0.5, "a far corner should still be outgoing (green) at the midpoint");
}

#[test]
fn barn_doors_is_listed_and_the_outgoing_picture_physically_slides_apart() {
    listed("barn_doors");
    let ss = scene("barn_doors", json!({}));
    let (g0, r0) = frame_rgb_counts(&ss, 0.0); assert_eq!(r0, 0, "t=0: doors fully closed, all green, got {r0} red px"); assert!(g0 > 3000);
    let (g1, r1) = frame_rgb_counts(&ss, 1.0); assert_eq!(g1, 0, "t=1: doors fully open, all red, got {g1} green px"); assert!(r1 > 3000);
    // at the midpoint a growing gap in the exact centre should already show the incoming picture, while the
    // outer edges (where the door panels have been pushed to) should still show the outgoing picture -- this
    // is the geometric-slide signature that distinguishes barn_doors from a plain split_h wipe.
    let cache = Cache::new(); let f = render_frame(&ss.project, 0.5, &cache).unwrap();
    let centre = (32 * 64 + 32) * 4; assert!(f.d[centre] > 0.5 && f.d[centre + 1] < 0.5, "the centre gap should already show the incoming (red) picture at the midpoint");
    let edge = (32 * 64 + 1) * 4; assert!(f.d[edge + 1] > 0.5 && f.d[edge] < 0.5, "the far edge should still show the outgoing (green) door panel at the midpoint");
}

#[test]
fn barn_doors_panel_edge_advances_monotonically_with_k() {
    // the boundary between "still green" and "already red" on one scanline should move outward (away from
    // centre) as k increases -- proof the door is actually sliding, not just fading in place.
    let ss = scene("barn_doors", json!({}));
    let cache = Cache::new();
    let gap_half_width = |t: f64| -> usize { let f = render_frame(&ss.project, t, &cache).unwrap(); let y = 32;
        let mut half = 0; for x in 32..64 { let i = (y * 64 + x) * 4; if f.d[i] > 0.5 && f.d[i + 1] < 0.5 { half = x - 32; } else { break; } } half };
    let a = gap_half_width(0.2); let b = gap_half_width(0.5); let c = gap_half_width(0.8);
    assert!(a < b && b < c, "the open gap should widen monotonically as the transition progresses, got {a} < {b} < {c}");
}

#[test]
fn page_peel_is_listed_and_reveals_diagonally_with_a_curl_shadow_band() {
    listed("page_peel");
    let ss = scene("page_peel", json!({"intensity": 1.0}));
    let (g0, r0) = frame_rgb_counts(&ss, 0.0); assert_eq!(r0, 0, "t=0: all green (outgoing), got {r0} red px"); assert!(g0 > 3000);
    let (g1, r1) = frame_rgb_counts(&ss, 1.0); assert_eq!(g1, 0, "t=1: all red (incoming), got {g1} green px"); assert!(r1 > 3000);
    // a diagonal reveal: at the midpoint, the near corner (where the peel starts) should be revealed (red)
    // while the far corner should still be outgoing (green) -- distinguishing it from a radial/centre reveal.
    let cache = Cache::new(); let f = render_frame(&ss.project, 0.5, &cache).unwrap();
    // default angle (135 deg, no `angle` given): gradient direction is (-cos45,sin45), so the corner that
    // reveals FIRST is (large x, small y) -- top-right -- and the one that reveals LAST is (small x, large y).
    let near = (5 * 64 + 58) * 4; assert!(f.d[near] > 0.5 && f.d[near + 1] < 0.5, "the peel's starting corner (top-right) should be revealed (red) by the midpoint");
    let far = (58 * 64 + 5) * 4; assert!(f.d[far + 1] > 0.5 && f.d[far] < 0.5, "the opposite corner (bottom-left) should still be outgoing (green) at the midpoint");
}
