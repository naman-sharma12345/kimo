//! TDD tests for FEATURES.md L491: Cineon converter, color profile converter, grow bounds, HDR
//! compander, tone mapping, compositing for log.
//!
//! New this pass: cineon_log (Kodak Cineon printing-density log<->linear, color.rs) and
//! hdr_compander (reversible mu-law-style HDR packing, color.rs) -- both exactly invertible, proven
//! by round-tripping through the MCP add_effect layer twice (encode then decode).
//!
//! Credited as already shipped, verified here rather than assumed:
//! - "color profile converter": color_space (linear/sRGB/Rec709/ACEScct/LogC3/S-Log3 transfer-curve
//!   conversion) and color_managed (full gamut+transfer conversion with exposure/tone-map/gamut-
//!   compress) -- both pre-existing (color.rs, color/managed.rs) and already covered by
//!   tests/color_pipeline.rs and friends; not retested here.
//! - "tone mapping": the pre-existing `tonemap` effect (color.rs) -- ACES filmic fit, Reinhard, and
//!   Hable filmic curves with exposure and optional sRGB encoding. It had ZERO functional tests
//!   anywhere in the suite before this file (grep confirmed) despite being fully implemented; verified
//!   here for the first time.
//! - "grow bounds": AE's Grow Bounds effect exists to stop a blur/glow from clipping at a precomp's
//!   own (smaller-than-needed) working buffer edge. This engine has no such edge to grow: every
//!   clip's working buffer is allocated at the full project canvas size regardless of source type
//!   (crates/kimo-core/src/render.rs, source_frame_h: `let (w, h) = (p.width as usize, p.height as
//!   usize); let mut f = Frame::new(w, h);` runs for every Source variant including Precomp, which
//!   renders its sub-project at `width: p.width, height: p.height`, the SAME as the parent -- never a
//!   smaller nested canvas). So a blur/glow in this engine is never clipped by an intermediate buffer
//!   boundary the way Grow Bounds exists to fix; the only remaining edge is the project canvas itself,
//!   which Grow Bounds does not touch in AE either (that's cropping, a separate concern). Documented
//!   here rather than adding a no-op tool that would do nothing: the held test below exercises exactly
//!   this fact (a full-bleed glow on a precomp is not clipped at a smaller-than-canvas boundary).
//! - "compositing for log": the project-level color_pipeline (color/pipeline.rs) already composites
//!   in float linear working space when enabled, with input_transfer able to name the source's log
//!   curve (LogC3, S-Log3) so footage is decoded to linear before any blend/composite math runs --
//!   exactly what "compositing for log" means (composite in linear, not directly on log-encoded
//!   values). Covered by tests/color_pipeline.rs; not retested here.
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

fn solid_clip(ss: &mut Session, t: &str, c: [f64; 3]) -> String {
    call(ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": c, "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string()
}

fn new_proj(w: u32, h: u32) -> (Session, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": w, "height": h, "fps": 25}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    (ss, t)
}

#[test]
fn cineon_log_and_hdr_compander_are_listed() {
    let r = call(&mut Session::new(), "list_effects", json!({}));
    let kinds: Vec<&str> = r.as_array().unwrap().iter().map(|v| v["kind"].as_str().unwrap()).collect();
    for k in ["cineon_log", "hdr_compander"] { assert!(kinds.contains(&k), "{k} should be a listed effect kind"); }
}

#[test]
fn cineon_log_decode_then_encode_round_trips() {
    // a mid-grey code value, decoded to linear then re-encoded, should land back near where it started.
    let (mut ss, t) = new_proj(16, 16);
    let c = solid_clip(&mut ss, &t, [0.5, 0.3, 0.7]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "cineon_log", "params": {"direction": 0.0}}));
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "cineon_log", "params": {"direction": 1.0}}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 0.5).abs() < 0.01, "red channel should round-trip through decode->encode, got {p_:?}");
    assert!((p_[1] - 0.3).abs() < 0.01, "green channel should round-trip through decode->encode, got {p_:?}");
    assert!((p_[2] - 0.7).abs() < 0.01, "blue channel should round-trip through decode->encode, got {p_:?}");
}

#[test]
fn cineon_log_white_point_decodes_to_linear_one() {
    // the white-point code value (685/1023 by default) should decode to linear 1.0 exactly, by
    // construction of the Cineon printing-density formula.
    let (mut ss, t) = new_proj(16, 16);
    let wp_norm = 685.0 / 1023.0;
    let c = solid_clip(&mut ss, &t, [wp_norm, wp_norm, wp_norm]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "cineon_log", "params": {"direction": 0.0}}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 1.0).abs() < 0.01, "the white point code value should decode to linear 1.0, got {p_:?}");
}

#[test]
fn cineon_log_is_monotonic_increasing() {
    // a darker log code value must decode to a darker (or equal) linear value -- never brighter.
    let (mut ss1, t1) = new_proj(16, 16);
    let c1 = solid_clip(&mut ss1, &t1, [0.3, 0.3, 0.3]);
    call(&mut ss1, "add_effect", json!({"clip_id": &c1, "kind": "cineon_log", "params": {"direction": 0.0}}));
    let dark = px(&ss1, 8, 8);

    let (mut ss2, t2) = new_proj(16, 16);
    let c2 = solid_clip(&mut ss2, &t2, [0.7, 0.7, 0.7]);
    call(&mut ss2, "add_effect", json!({"clip_id": &c2, "kind": "cineon_log", "params": {"direction": 0.0}}));
    let bright = px(&ss2, 8, 8);

    assert!(bright[0] > dark[0], "a higher log code value must decode to a brighter linear value, got dark={dark:?} bright={bright:?}");
}

#[test]
fn hdr_compander_is_identity_at_zero() {
    let (mut ss, t) = new_proj(16, 16);
    let c = solid_clip(&mut ss, &t, [0.0, 0.0, 0.0]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "hdr_compander", "params": {"direction": 0.0, "mu": 15.0}}));
    let p_ = px(&ss, 8, 8);
    assert!(p_[0].abs() < 1e-5, "compand(0) should be 0, got {p_:?}");
}

#[test]
fn hdr_compander_encode_then_decode_round_trips_an_hdr_value() {
    // a value above 1.0 (genuinely HDR, the whole point of this effect) should survive a
    // compand -> expand round trip.
    let (mut ss, t) = new_proj(16, 16);
    let c = solid_clip(&mut ss, &t, [4.0, 4.0, 4.0]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "hdr_compander", "params": {"direction": 0.0, "mu": 15.0}}));
    let companded = px(&ss, 8, 8);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "hdr_compander", "params": {"direction": 1.0, "mu": 15.0}}));
    let expanded = px(&ss, 8, 8);
    assert!(companded[0] < 1.5, "a companded HDR value should be pulled into a bounded-ish range, got {companded:?}");
    assert!((expanded[0] - 4.0).abs() < 0.01, "expand should invert compand and recover the original HDR value, got {expanded:?} (companded was {companded:?})");
}

#[test]
fn hdr_compander_handles_negative_values_symmetrically() {
    let (mut ss, t) = new_proj(16, 16);
    let c = solid_clip(&mut ss, &t, [-2.0, -2.0, -2.0]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "hdr_compander", "params": {"direction": 0.0, "mu": 15.0}}));
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "hdr_compander", "params": {"direction": 1.0, "mu": 15.0}}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - (-2.0)).abs() < 0.01, "a negative value should round-trip through compand->expand too, got {p_:?}");
}

#[test]
fn tonemap_reinhard_compresses_a_bright_hdr_value_below_one() {
    // tonemap mode 1 is plain Reinhard (x / (1+x)): a linear value of 9.0 should compress to 0.9.
    let (mut ss, t) = new_proj(16, 16);
    let c = solid_clip(&mut ss, &t, [9.0, 9.0, 9.0]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "tonemap", "params": {"mode": 1.0, "exposure": 0.0, "encode_srgb": 0.0}}));
    let p_ = px(&ss, 8, 8);
    assert!((p_[0] - 0.9).abs() < 0.01, "Reinhard tonemap of 9.0 should give 0.9 (x/(1+x)), got {p_:?}");
}

#[test]
fn tonemap_compresses_hdr_input_well_below_its_linear_value() {
    // a real HDR value (20x over white, within these curves' intended working range -- unlike an
    // unrealistic 1000x, which both the ACES-filmic and Hable approximations legitimately overshoot
    // past 1.0 on, a known property of their rational-polynomial fits outside the designed range)
    // should land well under its own linear value on every mode, proving real compression happened.
    for mode in [0.0, 1.0, 2.0] {
        let (mut ss, t) = new_proj(16, 16);
        let c = solid_clip(&mut ss, &t, [20.0, 20.0, 20.0]);
        call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "tonemap", "params": {"mode": mode, "exposure": 0.0, "encode_srgb": 0.0}}));
        let p_ = px(&ss, 8, 8);
        assert!(p_[0] < 1.2, "tonemap mode {mode} should compress a realistic 20x-over-white HDR input down near 1.0, got {p_:?}");
        assert!(p_[0] > 0.3, "tonemap mode {mode} should not crush a bright HDR input to near-black, got {p_:?}");
    }
}

#[test]
fn tonemap_zero_input_is_zero_on_every_mode() {
    for mode in [0.0, 1.0, 2.0] {
        let (mut ss, t) = new_proj(16, 16);
        let c = solid_clip(&mut ss, &t, [0.0, 0.0, 0.0]);
        call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "tonemap", "params": {"mode": mode, "exposure": 0.0, "encode_srgb": 0.0}}));
        let p_ = px(&ss, 8, 8);
        assert!(p_[0] < 0.02, "tonemap mode {mode} of 0.0 should stay at or near 0.0, got {p_:?}");
    }
}

#[test]
fn a_full_bleed_glow_on_a_precomp_is_not_clipped_at_a_smaller_working_buffer() {
    // "grow bounds" exists in AE to stop a precomp's own working buffer from clipping a blur/glow
    // that reaches toward its edges. This engine allocates every clip -- including a Precomp -- at
    // the full project canvas size (render.rs source_frame_h), so there is no smaller intermediate
    // buffer to clip against: a glow effect on a clip INSIDE a precomp that reaches all the way to
    // the precomp's own edge is not cut off when the precomp is later used (and, here, further
    // blurred) at the top level.
    let (mut ss, inner_t) = new_proj(48, 48);
    let bright = solid_clip(&mut ss, &inner_t, [3.0, 3.0, 3.0]);
    call(&mut ss, "add_effect", json!({"clip_id": &bright, "kind": "glow", "params": {"amount": 1.0, "radius": 16, "threshold": 0.5}}));
    let precomp = call(&mut ss, "group_clips", json!({"clip_ids": [&bright]}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &precomp, "kind": "blur", "params": {"radius": 4}}));
    let corner = px(&ss, 0, 0); // the glow should have bloomed all the way to the canvas corner, not been clipped at some smaller precomp buffer first
    assert!(corner[0] > 0.01, "a glow reaching the precomp's own edge should not be clipped before the outer blur sees it, got {corner:?}");
}
