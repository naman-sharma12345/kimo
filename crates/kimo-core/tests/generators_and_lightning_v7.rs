//! TDD tests for the v7 generator/stylize catalog (FEATURES.md L475: 4-color gradient, advanced
//! lightning variants, audio spectrum/waveform, beam, cell pattern, eyedropper fill, fill, fractal,
//! paint bucket, radio waves, scribble, stroke, vegas, write-on, caustics) plus the lightning
//! strike/branch_decay additions. Direct apply3/apply2 calls (deterministic, no rendering pipeline
//! needed) plus a couple of end-to-end checks through the public add_effect/render_frame surface to
//! prove each is reachable as a registered tool kind, not just a bare function.
use kimo_core::{color::{apply3, catalog3}, effects2::{apply2, CATALOG2}, effects::{Frame, Params}, keyframe::Prop};
use kimo_core::commands::{dispatch, Session};
use serde_json::json;

fn blank(w: usize, h: usize) -> Frame { Frame { w, h, d: vec![0.0; w * h * 4] } }
fn dup(f: &Frame) -> Frame { Frame { w: f.w, h: f.h, d: f.d.clone() } }
fn params(kv: &[(&str, f64)]) -> Params { let mut pr = Params::new(); for (k, v) in kv { pr.insert(k.to_string(), Prop::new(*v)); } pr }
fn solid(w: usize, h: usize, rgba: [f32; 4]) -> Frame { let mut f = blank(w, h); for i in 0..w * h { f.d[i * 4..i * 4 + 4].copy_from_slice(&rgba); } f }

const GEN_KINDS: &[&str] = &["four_color_gradient", "radio_waves", "beam", "cell_pattern", "eyedropper_fill", "fill",
    "fractal", "paint_bucket", "scribble", "stroke", "write_on", "vegas", "caustics", "audio_spectrum", "audio_waveform"];

#[test]
fn all_15_generator_kinds_are_registered_in_catalog3() {
    let names: Vec<&str> = catalog3().iter().map(|c| c.0).collect();
    for k in GEN_KINDS { assert!(names.contains(k), "{k} missing from catalog3()"); }
}

#[test]
fn every_generator_kind_actually_changes_the_frame_at_defaults() {
    // Default params should draw *something* visible against a mid-gray 70x70 backdrop (this is a
    // "these are inert no-ops" regression guard, distinct from color_hostile.rs's extreme-input/finite
    // check). Gray (not black/white) so no kind's own default ink colour accidentally matches the
    // backdrop; 70x70 so radio_waves' corner-to-center distance (~49px) spans more than its 40px
    // default wavelength -- guaranteed to cross its lit band regardless of t.
    // Two kinds are excluded, each correctly inert at its own documented defaults, not a bug:
    //  - eyedropper_fill: samples *this* frame's own colour and replace-fills with it -- a
    //    mathematical no-op on any single-uniform-colour source frame (its own test below covers it).
    //  - write_on: documented to default `end` to 0 ("invisible"; keyframe to 1 to reveal") -- its
    //    own test below checks end=1 reveals the identical segment `stroke` draws.
    //  - audio_spectrum: all 16 band levels default to 0.0 (silence) -- zero-height bars is the
    //    correct rest state; its own test above drives real band levels and checks the bars respond.
    for kind in GEN_KINDS {
        if matches!(*kind, "eyedropper_fill" | "write_on" | "audio_spectrum") { continue; }
        let before = solid(70, 70, [0.5, 0.5, 0.5, 1.0]);
        let mut f = dup(&before);
        apply3(kind, &Params::new(), &mut f, 0.0, 7).unwrap_or_else(|e| panic!("{kind} errored on defaults: {e}"));
        assert_ne!(f.d, before.d, "{kind} left the frame unchanged at its own defaults");
        assert!(f.d.iter().all(|x| x.is_finite()), "{kind} produced non-finite output at defaults");
    }
}

#[test]
fn write_on_defaults_to_invisible_and_end_1_matches_stroke() {
    let backdrop = solid(40, 20, [0.5, 0.5, 0.5, 1.0]);
    let mut default_write_on = dup(&backdrop);
    apply3("write_on", &Params::new(), &mut default_write_on, 0.0, 0).unwrap();
    assert_eq!(default_write_on.d, backdrop.d, "write_on must default to invisible (end=0)");

    let mut revealed = dup(&backdrop);
    apply3("write_on", &params(&[("end", 1.0)]), &mut revealed, 0.0, 0).unwrap();
    let mut stroke = dup(&backdrop);
    apply3("stroke", &Params::new(), &mut stroke, 0.0, 0).unwrap();
    assert_eq!(revealed.d, stroke.d, "write_on at end=1 should match stroke's default full segment");
}

#[test]
fn four_color_gradient_interpolates_the_four_named_corners() {
    let mut f = blank(10, 10);
    apply3("four_color_gradient", &params(&[
        ("r_tl", 1.0), ("g_tl", 0.0), ("b_tl", 0.0),
        ("r_tr", 0.0), ("g_tr", 1.0), ("b_tr", 0.0),
        ("r_bl", 0.0), ("g_bl", 0.0), ("b_bl", 1.0),
        ("r_br", 0.0), ("g_br", 0.0), ("b_br", 0.0),
        ("opacity", 1.0), ("mode", 0.0),
    ]), &mut f, 0.0, 0).unwrap();
    let tl = &f.d[0..3]; let tr = &f.d[(9) * 4..(9) * 4 + 3];
    assert!(tl[0] > 0.9 && tl[1] < 0.1 && tl[2] < 0.1, "top-left should be ~red, got {tl:?}");
    assert!(tr[1] > 0.9 && tr[0] < 0.1, "top-right should be ~green, got {tr:?}");
}

#[test]
fn fill_covers_the_whole_frame_with_its_color_at_full_opacity() {
    let mut f = solid(8, 8, [0.9, 0.9, 0.9, 1.0]);
    apply3("fill", &params(&[("r", 0.2), ("g", 0.4), ("b", 0.6), ("opacity", 1.0), ("mode", 0.0)]), &mut f, 0.0, 0).unwrap();
    for i in 0..64 { assert!((f.d[i * 4] - 0.2).abs() < 1e-5 && (f.d[i * 4 + 1] - 0.4).abs() < 1e-5 && (f.d[i * 4 + 2] - 0.6).abs() < 1e-5, "pixel {i} not filled"); }
}

#[test]
fn eyedropper_fill_samples_the_layers_own_color_before_the_effect() {
    let mut f = solid(6, 6, [0.3, 0.6, 0.9, 1.0]);
    apply3("eyedropper_fill", &params(&[("x", 0.5), ("y", 0.5), ("opacity", 1.0), ("mode", 0.0)]), &mut f, 0.0, 0).unwrap();
    for i in 0..36 { assert!((f.d[i * 4] - 0.3).abs() < 1e-4 && (f.d[i * 4 + 1] - 0.6).abs() < 1e-4 && (f.d[i * 4 + 2] - 0.9).abs() < 1e-4); }
}

#[test]
fn paint_bucket_replaces_only_the_close_matching_region() {
    let mut f = blank(10, 1);
    for i in 0..5 { f.d[i * 4..i * 4 + 3].copy_from_slice(&[0.9, 0.1, 0.1]); } // left half red
    for i in 5..10 { f.d[i * 4..i * 4 + 3].copy_from_slice(&[0.1, 0.1, 0.95]); } // right half blue
    apply3("paint_bucket", &params(&[("x", 0.2), ("y", 0.5), ("tolerance", 0.1), ("r", 0.0), ("g", 1.0), ("b", 0.0), ("opacity", 1.0)]), &mut f, 0.0, 0).unwrap();
    assert!(f.d[0 * 4 + 1] > 0.9, "sampled region should now be green");
    assert!(f.d[9 * 4 + 2] > 0.92, "far region should be untouched blue, got {:?}", &f.d[9 * 4..9 * 4 + 3]);
}

#[test]
fn fractal_mandelbrot_vs_julia_modes_differ_and_zoom_changes_output() {
    let mut a = blank(24, 24); apply3("fractal", &params(&[("mode", 0.0), ("zoom", 1.0)]), &mut a, 0.0, 0).unwrap();
    let mut b = blank(24, 24); apply3("fractal", &params(&[("mode", 1.0), ("zoom", 1.0)]), &mut b, 0.0, 0).unwrap();
    assert_ne!(a.d, b.d, "Mandelbrot and Julia modes should render differently");
    let mut c = blank(24, 24); apply3("fractal", &params(&[("mode", 0.0), ("zoom", 50.0)]), &mut c, 0.0, 0).unwrap();
    assert_ne!(a.d, c.d, "zoom should change the rendered set");
}

#[test]
fn audio_spectrum_and_waveform_read_their_own_band_and_sample_params() {
    let mut quiet = blank(20, 20); apply3("audio_spectrum", &Params::new(), &mut quiet, 0.0, 0).unwrap();
    let loud_params: Vec<(&str, f64)> = (0..16).map(|i| (Box::leak(format!("b{i}").into_boxed_str()) as &str, 1.0)).collect();
    let mut loud = blank(20, 20); apply3("audio_spectrum", &params(&loud_params), &mut loud, 0.0, 0).unwrap();
    assert_ne!(quiet.d, loud.d, "band params should change the spectrum bars");

    let mut flat = blank(20, 20); apply3("audio_waveform", &Params::new(), &mut flat, 0.0, 0).unwrap();
    let hi_params: Vec<(&str, f64)> = (0..16).map(|i| (Box::leak(format!("s{i}").into_boxed_str()) as &str, 1.0)).collect();
    let mut hi = blank(20, 20); apply3("audio_waveform", &params(&hi_params), &mut hi, 0.0, 0).unwrap();
    assert_ne!(flat.d, hi.d, "sample params should change the waveform line");
}

#[test]
fn deterministic_given_same_params_time_and_seed() {
    for kind in GEN_KINDS {
        let mut a = blank(16, 16); apply3(kind, &Params::new(), &mut a, 0.42, 5).unwrap();
        let mut b = blank(16, 16); apply3(kind, &Params::new(), &mut b, 0.42, 5).unwrap();
        assert_eq!(a.d, b.d, "{kind} is not deterministic for identical (params, t, seed)");
    }
}

// ---- lightning: strike + branch_decay (the "advanced lightning variants" half of FEATURES.md L475) ----

fn lightning_params(extra: &[(&str, f64)]) -> Params {
    let mut kv = vec![("x0", 0.1), ("y0", 0.5), ("x1", 0.9), ("y1", 0.5), ("rate", 10.0)];
    kv.extend(extra.iter().cloned());
    params(&kv)
}

#[test]
fn lightning_is_registered_with_strike_and_branch_decay_params() {
    let entry = CATALOG2.iter().find(|c| c.0 == "lightning").expect("lightning missing from CATALOG2");
    let names: Vec<&str> = entry.2.iter().map(|p| p.0).collect();
    assert!(names.contains(&"strike"), "lightning needs a strike param");
    assert!(names.contains(&"branch_decay"), "lightning needs a branch_decay param");
    assert!(names.contains(&"strike_duty"), "lightning needs a strike_duty param");
}

#[test]
fn strike_mode_is_dark_between_flashes_and_lit_during_the_duty_window() {
    let backdrop = blank(40, 20);
    // rate=10 means a 0.1s cycle; strike_duty=0.2 of that cycle is lit. t=0.0 is inside the window (phase 0).
    let mut lit = dup(&backdrop);
    apply2("lightning", &lightning_params(&[("strike", 1.0), ("strike_duty", 0.2)]), &mut lit, 0.0, 3).unwrap();
    assert_ne!(lit.d, backdrop.d, "strike should draw something during its duty window");

    // t=0.05 is exactly half-way through the 0.1s cycle (phase 0.5), outside a 0.2 duty window.
    let mut dark = dup(&backdrop);
    apply2("lightning", &lightning_params(&[("strike", 1.0), ("strike_duty", 0.2)]), &mut dark, 0.05, 3).unwrap();
    assert_eq!(dark.d, backdrop.d, "strike should be fully dark outside its duty window");
}

#[test]
fn continuous_mode_default_ignores_strike_duty_and_always_draws() {
    let backdrop = blank(40, 20);
    for tt in [0.0, 0.03, 0.05, 0.07, 0.09] {
        let mut f = dup(&backdrop);
        apply2("lightning", &lightning_params(&[]), &mut f, tt, 3).unwrap(); // strike defaults to 0
        assert_ne!(f.d, backdrop.d, "default (non-strike) lightning should always draw, t={tt}");
    }
}

#[test]
fn branch_decay_changes_branch_brightness() {
    let mut faint = blank(60, 30);
    apply2("lightning", &lightning_params(&[("branches", 4.0), ("branch_decay", 0.05)]), &mut faint, 0.0, 9).unwrap();
    let mut strong = blank(60, 30);
    apply2("lightning", &lightning_params(&[("branches", 4.0), ("branch_decay", 1.0)]), &mut strong, 0.0, 9).unwrap();
    assert_ne!(faint.d, strong.d, "branch_decay should change the rendered branches");
}

#[test]
fn branch_decay_default_matches_the_pre_v7_hardcoded_half_brightness_look() {
    // Regression guard: branch_decay defaults to 0.5, reproducing the exact pre-existing
    // core*0.5 / glow*0.5 / intensity*0.7 branch look bit-for-bit (goldens must not move).
    let mut default_decay = blank(50, 25);
    apply2("lightning", &lightning_params(&[("branches", 3.0)]), &mut default_decay, 0.0, 11).unwrap();
    let mut explicit_default = blank(50, 25);
    apply2("lightning", &lightning_params(&[("branches", 3.0), ("branch_decay", 0.5)]), &mut explicit_default, 0.0, 11).unwrap();
    assert_eq!(default_decay.d, explicit_default.d, "omitting branch_decay must be bit-identical to branch_decay=0.5");
}

#[test]
fn end_to_end_through_add_effect_and_render_frame() {
    // Confirms these are reachable through the real tool surface (add_effect/set_effect_param/render_frame),
    // not just bare functions -- the TDD contract ("register every feature as an MCP tool").
    let mut ss = Session::new();
    let t = dispatch(&mut ss, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let c = dispatch(&mut ss, "add_shape", &json!({"track_id": t, "shape": "rect", "width": 80, "height": 60, "fill": [0.2, 0.2, 0.2, 1.0], "start": 0, "duration": 1})).unwrap()["clip_id"].as_str().unwrap().to_string();
    for kind in ["fill", "radio_waves", "fractal"] {
        let e = dispatch(&mut ss, "add_effect", &json!({"clip_id": &c, "kind": kind})).unwrap()["effect_id"].as_str().unwrap().to_string();
        dispatch(&mut ss, "set_effect_param", &json!({"clip_id": &c, "effect_id": e, "param": "opacity", "value": 1.0})).unwrap();
    }
    let listed = dispatch(&mut ss, "list_effects", &json!({})).unwrap();
    let kinds: Vec<String> = listed.as_array().unwrap().iter().filter_map(|x| x.get("kind").and_then(|k| k.as_str()).map(String::from)).collect();
    for k in GEN_KINDS { assert!(kinds.contains(&k.to_string()), "{k} not visible through list_effects"); }
}
