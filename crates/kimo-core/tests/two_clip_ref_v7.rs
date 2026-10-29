//! TDD tests for the two-clip-reference gap named on FEATURES.md L469 (true arithmetic/calculations/
//! channel combiner/set_channels between THIS clip and ANOTHER referenced clip) and L481 (match_grain).
//! All three reuse the set_matte/displacement_map clip-reference pattern (layerfx.rs): a `ref_clip_id`
//! names the other clip, rendered alone at the same time (render.rs render_clip_alone) and hidden from
//! the main composite by default (hide_ref, same "hide" string convention as set_matte's hide_matte).
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(ss: &mut Session, n: &str, a: serde_json::Value) -> String { dispatch(ss, n, &a).unwrap_err() }

fn px(ss: &Session, x: u32, y: u32) -> [f32; 4] {
    let cache = Cache::new();
    let f = render_frame(&ss.project, 0.0, &cache).unwrap();
    let o = ((y as usize * f.w) + x as usize) * 4;
    [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]]
}

fn solid_clip(ss: &mut Session, t: &str, c: [f64; 3]) -> String {
    call(ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": c, "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string()
}

fn two_track_proj(w: u32, h: u32) -> (Session, String, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": w, "height": h, "fps": 25}));
    let t1 = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t2 = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    (ss, t1, t2)
}

// ---------- channel_combiner ----------

#[test]
fn channel_combiner_add_combines_two_different_clips() {
    let (mut ss, t1, t2) = two_track_proj(16, 16);
    let a = solid_clip(&mut ss, &t1, [0.2, 0.2, 0.2]);
    let b = solid_clip(&mut ss, &t2, [0.3, 0.3, 0.3]);
    call(&mut ss, "add_channel_combiner", json!({"clip_id": &a, "ref_clip_id": &b, "operation": "add"}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 0.5).abs() < 0.02, "add should combine 0.2 + 0.3 = 0.5, got {p_:?}");
}

#[test]
fn channel_combiner_multiply_combines_two_different_clips() {
    let (mut ss, t1, t2) = two_track_proj(16, 16);
    let a = solid_clip(&mut ss, &t1, [0.5, 0.5, 0.5]);
    let b = solid_clip(&mut ss, &t2, [0.4, 0.4, 0.4]);
    call(&mut ss, "add_channel_combiner", json!({"clip_id": &a, "ref_clip_id": &b, "operation": "multiply"}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 0.2).abs() < 0.02, "multiply should combine 0.5 * 0.4 = 0.2, got {p_:?}");
}

#[test]
fn channel_combiner_opacity_mixes_toward_the_original() {
    let (mut ss, t1, t2) = two_track_proj(16, 16);
    let a = solid_clip(&mut ss, &t1, [0.2, 0.2, 0.2]);
    let b = solid_clip(&mut ss, &t2, [0.8, 0.8, 0.8]);
    call(&mut ss, "add_channel_combiner", json!({"clip_id": &a, "ref_clip_id": &b, "operation": "add", "opacity": 0.0}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 0.2).abs() < 0.02, "opacity 0 should leave the clip's own colour untouched, got {p_:?}");
}

#[test]
fn channel_combiner_hides_the_reference_clip_from_the_composite() {
    // ref clip b is on a track ABOVE a (t2 is added after t1, so comes out on top in z-order);
    // without hide_ref defaulting on, b's own solid colour would show through on top of a.
    let (mut ss, t1, t2) = two_track_proj(16, 16);
    let a = solid_clip(&mut ss, &t1, [0.2, 0.2, 0.2]);
    let b = solid_clip(&mut ss, &t2, [0.9, 0.9, 0.9]);
    call(&mut ss, "add_channel_combiner", json!({"clip_id": &a, "ref_clip_id": &b, "operation": "add", "opacity": 0.0}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 0.9).abs() > 0.3, "the reference clip should be hidden from the main composite by default, not shown on top, got {p_:?}");
}

#[test]
fn channel_combiner_refuses_referencing_itself() {
    let (mut ss, t1, _t2) = two_track_proj(16, 16);
    let a = solid_clip(&mut ss, &t1, [0.2, 0.2, 0.2]);
    let e = err(&mut ss, "add_channel_combiner", json!({"clip_id": &a, "ref_clip_id": &a, "operation": "add"}));
    assert!(e.contains("another clip"), "referencing the clip itself should be refused, got: {e}");
}

// ---------- set_channels ----------

#[test]
fn set_channels_pulls_red_from_the_referenced_clip() {
    let (mut ss, t1, t2) = two_track_proj(16, 16);
    let a = solid_clip(&mut ss, &t1, [0.1, 0.1, 0.1]);
    let b = solid_clip(&mut ss, &t2, [0.9, 0.2, 0.2]);
    call(&mut ss, "add_set_channels", json!({"clip_id": &a, "ref_clip_id": &b, "r_source": "ref", "r_channel": "red"}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 0.9).abs() < 0.02, "red output channel should be pulled from the reference clip's red, got {p_:?}");
    assert!((p_[1] - 0.1).abs() < 0.02, "untouched green should stay this clip's own, got {p_:?}");
}

#[test]
fn set_channels_on_and_off_force_flat_channel_values() {
    let (mut ss, t1, _t2) = two_track_proj(16, 16);
    let a = solid_clip(&mut ss, &t1, [0.5, 0.5, 0.5]);
    call(&mut ss, "add_set_channels", json!({"clip_id": &a, "r_source": "on", "g_source": "off"}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 1.0).abs() < 0.02, "r_source on should force red to 1.0, got {p_:?}");
    assert!(p_[1].abs() < 0.02, "g_source off should force green to 0.0, got {p_:?}");
}

#[test]
fn set_channels_requires_ref_clip_id_when_a_source_is_ref() {
    let (mut ss, t1, _t2) = two_track_proj(16, 16);
    let a = solid_clip(&mut ss, &t1, [0.5, 0.5, 0.5]);
    let e = err(&mut ss, "add_set_channels", json!({"clip_id": &a, "r_source": "ref"}));
    assert!(e.contains("ref_clip_id"), "should refuse r_source=ref with no ref_clip_id, got: {e}");
}

#[test]
fn set_channels_swap_red_and_blue_is_a_real_channel_swap() {
    let (mut ss, t1, _t2) = two_track_proj(16, 16);
    let a = solid_clip(&mut ss, &t1, [0.8, 0.3, 0.1]);
    call(&mut ss, "add_set_channels", json!({"clip_id": &a, "r_source": "this", "r_channel": "blue", "b_source": "this", "b_channel": "red"}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 0.1).abs() < 0.02, "red output should now carry the original blue (0.1), got {p_:?}");
    assert!((p_[2] - 0.8).abs() < 0.02, "blue output should now carry the original red (0.8), got {p_:?}");
}

// ---------- match_grain ----------

#[test]
fn match_grain_is_listed_as_a_driven_clip_reference_effect() {
    assert!(kimo_core::layerfx::KINDS.contains(&"match_grain"));
    assert_eq!(kimo_core::layerfx::source_key("match_grain"), Some("ref_clip"));
}

#[test]
fn match_grain_adds_more_pixel_variance_when_the_reference_has_more_grain() {
    // a synthetic "grainy" reference (noise effect applied to a solid) should make match_grain add
    // more variance to the target than a perfectly flat reference does.
    let (mut ss1, t1, t2) = two_track_proj(32, 32);
    let target1 = solid_clip(&mut ss1, &t1, [0.5, 0.5, 0.5]);
    let grainy_ref = solid_clip(&mut ss1, &t2, [0.5, 0.5, 0.5]);
    call(&mut ss1, "add_effect", json!({"clip_id": &grainy_ref, "kind": "grain", "params": {"amount": 0.3}}));
    call(&mut ss1, "add_match_grain", json!({"clip_id": &target1, "ref_clip_id": &grainy_ref}));
    let cache1 = Cache::new();
    let f1 = render_frame(&ss1.project, 0.0, &cache1).unwrap();
    let var_grainy: f64 = { let m: f64 = f1.d.iter().step_by(4).map(|&v| v as f64).sum::<f64>() / (f1.w * f1.h) as f64;
        f1.d.iter().step_by(4).map(|&v| (v as f64 - m).powi(2)).sum::<f64>() / (f1.w * f1.h) as f64 };

    let (mut ss2, t1b, t2b) = two_track_proj(32, 32);
    let target2 = solid_clip(&mut ss2, &t1b, [0.5, 0.5, 0.5]);
    let flat_ref = solid_clip(&mut ss2, &t2b, [0.5, 0.5, 0.5]);
    call(&mut ss2, "add_match_grain", json!({"clip_id": &target2, "ref_clip_id": &flat_ref}));
    let cache2 = Cache::new();
    let f2 = render_frame(&ss2.project, 0.0, &cache2).unwrap();
    let var_flat: f64 = { let m: f64 = f2.d.iter().step_by(4).map(|&v| v as f64).sum::<f64>() / (f2.w * f2.h) as f64;
        f2.d.iter().step_by(4).map(|&v| (v as f64 - m).powi(2)).sum::<f64>() / (f2.w * f2.h) as f64 };

    assert!(var_grainy > var_flat * 2.0, "matching a grainy reference should add noticeably more variance than matching a flat one (grainy={var_grainy}, flat={var_flat})");
}

#[test]
fn match_grain_strength_zero_leaves_a_flat_reference_target_unchanged() {
    let (mut ss, t1, t2) = two_track_proj(16, 16);
    let target = solid_clip(&mut ss, &t1, [0.4, 0.4, 0.4]);
    let flat_ref = solid_clip(&mut ss, &t2, [0.5, 0.5, 0.5]);
    call(&mut ss, "add_match_grain", json!({"clip_id": &target, "ref_clip_id": &flat_ref, "strength": 0.0}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 0.4).abs() < 0.01, "a perfectly flat reference (no high-pass residual) with strength 0 should leave the target unchanged, got {p_:?}");
}
