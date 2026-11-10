//! TDD tests for the VR / 360 equirectangular effect suite (FEATURES.md L477): vr_rotate_sphere,
//! vr_convert_projection, vr_plane_to_sphere, vr_blur, vr_sharpen, vr_denoise, vr_glow,
//! vr_chromatic_aberration (crates/kimo-core/src/vr.rs).
//!
//! Two engine facts shaped how these are written, found by direct pixel probing before trusting any
//! assertion (not after):
//! 1. The final composited frame this engine renders is always fully opaque (alpha 1 everywhere,
//!    including background) -- checking alpha to detect "outside a shape / outside a patch" is wrong
//!    on this renderer; these tests check RGB against the background colour (black) instead.
//! 2. A clip's x/y position (set_property) is applied AFTER its own effects run -- add_effect sees the
//!    clip rendered at its native canvas-centred position, not its final on-screen position. So a test
//!    that needs content genuinely touching column 0 / column (w-1) of the buffer an effect actually
//!    runs on uses a full-canvas, zero-offset Source::Gradient (add_clip source_type "gradient", angle
//!    90) rather than translating a smaller shape to the edge with set_property -- the gradient is
//!    rendered to fill the whole w x h buffer at its native (untranslated) position, so column 0 and
//!    column w-1 of that buffer really are the gradient's two ends by construction.
//!
//! The defining VR property under test is the horizontal WRAP at the 360 seam (column 0 and column
//! w-1 are neighbours on the sphere), which the ordinary (non-VR) blur/glow/chromatic-aberration
//! effects do not have (they clamp) -- proven by running both the ordinary and the VR kind on
//! IDENTICAL content and comparing the result at the far edge.
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn px(ss: &Session, x: u32, y: u32) -> [f32; 4] {
    let cache = Cache::new();
    let f = render_frame(&ss.project, 0.0, &cache).unwrap();
    let o = ((y as usize * f.w) + x as usize) * 4;
    [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]]
}

fn new_proj(w: u32, h: u32) -> (Session, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": w, "height": h, "fps": 25}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    (ss, t)
}

/// A full-canvas left-to-right gradient (c1 at column 0, c2 at column w-1), zero offset, so an
/// effect sees the real buffer edges as the gradient's two ends.
fn gradient_clip(ss: &mut Session, t: &str, c1: [f64; 3], c2: [f64; 3]) -> String {
    call(ss, "add_clip", json!({"track_id": t, "source_type": "gradient", "color": c1, "color2": c2, "angle": 90.0, "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string()
}

#[test]
fn all_eight_vr_kinds_are_listed() {
    let r = call(&mut Session::new(), "list_effects", json!({}));
    let kinds: Vec<&str> = r.as_array().unwrap().iter().map(|v| v["kind"].as_str().unwrap()).collect();
    for k in ["vr_rotate_sphere", "vr_convert_projection", "vr_plane_to_sphere", "vr_blur", "vr_sharpen", "vr_denoise", "vr_glow", "vr_chromatic_aberration"] {
        assert!(kinds.contains(&k), "{k} should be a listed effect kind");
    }
}

#[test]
fn vr_blur_wraps_the_seam_where_ordinary_blur_clamps_it() {
    let (w, h) = (64u32, 32u32);
    let (mut ss_vr, t1) = new_proj(w, h);
    let c1 = gradient_clip(&mut ss_vr, &t1, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]); // red at col0 -> black at col w-1
    call(&mut ss_vr, "add_effect", json!({"clip_id": &c1, "kind": "vr_blur", "params": {"radius": 10}}));
    let vr_edge = px(&ss_vr, (w - 1) as u32, h / 2);

    let (mut ss_ord, t2) = new_proj(w, h);
    let c2 = gradient_clip(&mut ss_ord, &t2, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    call(&mut ss_ord, "add_effect", json!({"clip_id": &c2, "kind": "blur", "params": {"radius": 10}}));
    let ord_edge = px(&ss_ord, (w - 1) as u32, h / 2);

    assert!(vr_edge[0] > ord_edge[0] + 0.03, "vr_blur should pull red in from column 0 across the 360 seam into column {}, more than the clamping ordinary blur does: vr={:?} ordinary={:?}", w - 1, vr_edge, ord_edge);
}

#[test]
fn vr_blur_zero_radius_is_identity() {
    let (mut ss, t) = new_proj(32, 16);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 32, "height": 16, "fill": [0.3, 0.4, 0.5, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_blur", "params": {"radius": 0}}));
    let p_ = px(&ss, 16, 8);
    assert!((p_[0] - 0.3).abs() < 0.01 && (p_[1] - 0.4).abs() < 0.01 && (p_[2] - 0.5).abs() < 0.01, "radius 0 should be identity, got {p_:?}");
}

#[test]
fn vr_sharpen_leaves_a_flat_field_unchanged() {
    let (mut ss, t) = new_proj(32, 16);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 32, "height": 16, "fill": [0.4, 0.4, 0.4, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_sharpen", "params": {"amount": 1.5, "radius": 3}}));
    let p_ = px(&ss, 16, 8);
    assert!((p_[0] - 0.4).abs() < 0.01, "a flat field has no high-frequency content, unsharp mask should leave it alone, got {p_:?}");
}

#[test]
fn vr_denoise_amount_zero_is_identity() {
    // on a 32x16 canvas the default (unmoved) clip centre (16,8) already lands a small centred rect
    // right where it is sampled, so no positioning is needed here.
    let (mut ss, t) = new_proj(32, 16);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 4, "height": 4, "fill": [1.0, 0.0, 0.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_denoise", "params": {"amount": 0.0, "radius": 3}}));
    let p_ = px(&ss, 16, 8);
    assert!(p_[0] > 0.9, "amount 0 should be identity (small red patch still fully red at its centre), got {p_:?}");
}

#[test]
fn vr_denoise_amount_one_smooths_an_isolated_speck() {
    let (mut ss, t) = new_proj(32, 16);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 2, "height": 2, "fill": [1.0, 1.0, 1.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let before = px(&ss, 16, 8);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_denoise", "params": {"amount": 1.0, "radius": 4}}));
    let after = px(&ss, 16, 8);
    assert!(before[0] > 0.9, "expected white speck before denoise, got {before:?}");
    assert!(after[0] < before[0] - 0.2, "amount 1 should pull an isolated speck toward its surroundings, got {after:?} (was {before:?})");
}

#[test]
fn vr_glow_wraps_the_seam_where_ordinary_glow_clamps_it() {
    let (w, h) = (64u32, 32u32);
    let (mut ss_vr, t1) = new_proj(w, h);
    let c1 = gradient_clip(&mut ss_vr, &t1, [3.0, 3.0, 3.0], [0.0, 0.0, 0.0]); // over-threshold white at col0 -> black at col w-1
    call(&mut ss_vr, "add_effect", json!({"clip_id": &c1, "kind": "vr_glow", "params": {"amount": 1.0, "radius": 14, "threshold": 0.5}}));
    let vr_edge = px(&ss_vr, (w - 1) as u32, h / 2);

    let (mut ss_ord, t2) = new_proj(w, h);
    let c2 = gradient_clip(&mut ss_ord, &t2, [3.0, 3.0, 3.0], [0.0, 0.0, 0.0]);
    call(&mut ss_ord, "add_effect", json!({"clip_id": &c2, "kind": "glow", "params": {"amount": 1.0, "radius": 14, "threshold": 0.5}}));
    let ord_edge = px(&ss_ord, (w - 1) as u32, h / 2);

    assert!(vr_edge[0] > ord_edge[0] + 0.03, "vr_glow's bloom should wrap the 360 seam and brighten column {} more than the clamping ordinary glow does: vr={:?} ordinary={:?}", w - 1, vr_edge, ord_edge);
}

#[test]
fn vr_chromatic_aberration_wraps_the_seam_where_ordinary_does_not() {
    let (w, h) = (64u32, 32u32);
    let params = json!({"amount": 40.0, "radial_scale": 1.0, "r_scale": 1.0, "b_scale": 0.0, "cx": 0.0, "cy": 0.5});
    let (mut ss_vr, t1) = new_proj(w, h);
    let c1 = gradient_clip(&mut ss_vr, &t1, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]); // red at col0 -> black at col w-1
    call(&mut ss_vr, "add_effect", json!({"clip_id": &c1, "kind": "vr_chromatic_aberration", "params": params.clone()}));
    let vr_edge = px(&ss_vr, (w - 1) as u32, h / 2);

    let (mut ss_ord, t2) = new_proj(w, h);
    let c2 = gradient_clip(&mut ss_ord, &t2, [1.0, 0.0, 0.0], [0.0, 0.0, 0.0]);
    call(&mut ss_ord, "add_effect", json!({"clip_id": &c2, "kind": "chromatic_aberration", "params": params}));
    let ord_edge = px(&ss_ord, (w - 1) as u32, h / 2);

    assert!(vr_edge[0] > ord_edge[0] + 0.03, "vr_chromatic_aberration's radial red shift should wrap the seam and show up more at column {} than the clamping ordinary effect: vr={:?} ordinary={:?}", w - 1, vr_edge, ord_edge);
}

#[test]
fn vr_rotate_sphere_half_turn_yaw_brings_the_far_edge_to_the_centre() {
    // a left(red)-to-right(blue) gradient: before rotation the centre column is a red/blue mix;
    // after a 180deg yaw the centre column should sample from column 0 (lon wraps to the opposite
    // side), so it should become close to pure red.
    let (w, h) = (72u32, 36u32);
    let (mut ss, t) = new_proj(w, h);
    let c = gradient_clip(&mut ss, &t, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]);
    let before_centre = px(&ss, w / 2, h / 2);
    assert!(before_centre[0] < 0.7, "sanity: the centre of a red->blue gradient should not already be mostly red, got {before_centre:?}");
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_rotate_sphere", "params": {"yaw": 180.0, "pitch": 0.0, "roll": 0.0}}));
    let after_centre = px(&ss, w / 2, h / 2);
    assert!(after_centre[0] > 0.7, "a 180 degree yaw should bring column 0's red to the centre column, got {after_centre:?}");
}

#[test]
fn vr_rotate_sphere_zero_is_identity() {
    let (mut ss, t) = new_proj(32, 16);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 32, "height": 16, "fill": [0.2, 0.6, 0.9, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_rotate_sphere", "params": {"yaw": 0.0, "pitch": 0.0, "roll": 0.0}}));
    let p_ = px(&ss, 16, 8);
    assert!((p_[0] - 0.2).abs() < 0.02 && (p_[1] - 0.6).abs() < 0.02 && (p_[2] - 0.9).abs() < 0.02, "zero rotation should be (near-)identity, got {p_:?}");
}

#[test]
fn vr_convert_projection_equirect_to_fisheye_is_circular() {
    let (mut ss, t) = new_proj(64, 32);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 64, "height": 32, "fill": [0.5, 0.5, 0.5, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_convert_projection", "params": {"mode": 0.0, "fov": 180.0}}));
    let corner = px(&ss, 0, 0); // outside the fisheye circle -> should be plain background black
    let centre = px(&ss, 32, 16); // inside, at the very centre -> should be the source's grey
    assert!(corner[0] < 0.05 && corner[1] < 0.05 && corner[2] < 0.05, "a corner pixel should fall outside the fisheye circle (background black), got {corner:?}");
    assert!(centre[0] > 0.3, "the centre should be inside the fisheye circle and sampled from the grey source, got {centre:?}");
}

#[test]
fn vr_convert_projection_round_trips_the_centre_direction() {
    // equirect -> fisheye -> equirect should bring the centre pixel (straight-ahead direction) back
    // close to where it started (some blur from the double-resample is expected, exact is not).
    let (mut ss, t) = new_proj(64, 32);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 8, "height": 8, "fill": [0.0, 1.0, 0.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    // default centre (32,16) is already the canvas centre -- no positioning needed
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_convert_projection", "params": {"mode": 0.0, "fov": 180.0}}));
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_convert_projection", "params": {"mode": 1.0, "fov": 180.0}}));
    let centre = px(&ss, 32, 16);
    assert!(centre[1] > 0.3, "the green patch at the straight-ahead direction should round-trip back near the centre, got {centre:?}");
}

#[test]
fn vr_plane_to_sphere_places_content_only_inside_its_fov_patch() {
    let (mut ss, t) = new_proj(72, 36);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 72, "height": 36, "fill": [0.0, 0.5, 1.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_plane_to_sphere", "params": {"yaw": 0.0, "pitch": 0.0, "fov_h": 60.0}}));
    let centre = px(&ss, 36, 18); // straight ahead (yaw 0, pitch 0) -- should be inside the patch
    let far_side = px(&ss, 0, 18); // lon ~ -180, well outside a 60-degree-wide patch centred at 0
    assert!(centre[2] > 0.5, "straight-ahead direction should be inside the placed patch (source blue), got {centre:?}");
    assert!(far_side[0] < 0.05 && far_side[1] < 0.05 && far_side[2] < 0.05, "the far side of the sphere should be outside a narrow 60deg patch (background black), got {far_side:?}");
}

#[test]
fn vr_plane_to_sphere_respects_yaw_offset() {
    let (mut ss, t) = new_proj(72, 36);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 72, "height": 36, "fill": [1.0, 0.0, 1.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "vr_plane_to_sphere", "params": {"yaw": 90.0, "pitch": 0.0, "fov_h": 40.0}}));
    let straight_ahead = px(&ss, 36, 18); // lon 0 -- should now be OUTSIDE the patch (it moved to yaw 90)
    let quarter_turn = px(&ss, 54, 18); // lon ~90 degrees (3/4 of the way across a 72-wide frame maps to +90)
    assert!(straight_ahead[0] < 0.05 && straight_ahead[1] < 0.05 && straight_ahead[2] < 0.05, "after a 90deg yaw offset, the straight-ahead direction should no longer be inside the narrow patch (background black), got {straight_ahead:?}");
    assert!(quarter_turn[0] > 0.5, "the patch should have moved to the yaw=90 direction (source magenta), got {quarter_turn:?}");
}
