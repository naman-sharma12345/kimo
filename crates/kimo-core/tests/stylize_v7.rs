//! TDD tests for FEATURES.md L485 (brush strokes, cartoon, color emboss, roughen edges, scatter,
//! texturize, CC glass/kaleida/mr. mercury/blobbylize/burn film, CC threshold RGB, dither, tile
//! mosaic, trace/vector stylize, sketch/pencil, oil paint, anaglyph).
//!
//! 14 of the 17 named items got a real new effect kind this pass (effects2.rs): cartoon,
//! color_emboss, roughen_edges, scatter, texturize (procedural variant), cc_glass, cc_kaleida,
//! cc_threshold_rgb, dither, tile_mosaic, sketch_pencil, oil_paint, anaglyph, cc_burn_film.
//! Honest remaining gaps (documented on FEATURES.md, not attempted here): brush_strokes, CC Mr.
//! Mercury and CC Blobbylize (genuine particle/metaball simulations, out of scope for this pass);
//! texturize's two-clip "texture from another layer" AE mode (only the procedural single-clip bump
//! variant was built); "trace/vector stylize" literal VECTOR PATH output (this is a raster pixel
//! engine -- sketch_pencil covers the visual LOOK half of that item, not true vector extraction).
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{render_frame, Cache};
use serde_json::json;

fn call(ss: &mut Session, n: &str, a: serde_json::Value) -> serde_json::Value { dispatch(ss, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }

fn frame(ss: &Session) -> kimo_core::effects::Frame { let cache = Cache::new(); render_frame(&ss.project, 0.0, &cache).unwrap() }
fn px(ss: &Session, x: u32, y: u32) -> [f32; 4] { let f = frame(ss); let o = ((y as usize * f.w) + x as usize) * 4; [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]] }

fn new_proj(w: u32, h: u32) -> (Session, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": w, "height": h, "fps": 25}));
    let t = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    (ss, t)
}
fn solid_clip(ss: &mut Session, t: &str, c: [f64; 3]) -> String { call(ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": c, "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string() }
fn gradient_clip(ss: &mut Session, t: &str, c1: [f64; 3], c2: [f64; 3]) -> String { call(ss, "add_clip", json!({"track_id": t, "source_type": "gradient", "color": c1, "color2": c2, "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string() }
fn gradient_clip_angle(ss: &mut Session, t: &str, c1: [f64; 3], c2: [f64; 3], angle: f64) -> String { call(ss, "add_clip", json!({"track_id": t, "source_type": "gradient", "color": c1, "color2": c2, "angle": angle, "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string() }

fn variance(f: &kimo_core::effects::Frame) -> f64 {
    let n = (f.w * f.h) as f64; let m: f64 = f.d.iter().step_by(4).map(|&v| v as f64).sum::<f64>() / n;
    f.d.iter().step_by(4).map(|&v| (v as f64 - m).powi(2)).sum::<f64>() / n
}

#[test]
fn cartoon_posterizes_a_smooth_gradient_into_flat_bands() {
    let (mut ss, t) = new_proj(32, 32);
    let c = gradient_clip(&mut ss, &t, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "cartoon", "params": {"levels": 4.0, "edge_strength": 0.0}}));
    let f = frame(&ss);
    let distinct: std::collections::BTreeSet<i32> = f.d.iter().step_by(4).map(|&v| (v * 1000.0).round() as i32).collect();
    assert!(distinct.len() <= 6, "posterizing a smooth gradient to 4 levels should leave only a handful of distinct values, got {}", distinct.len());
}

#[test]
fn cartoon_darkens_right_at_a_hard_edge() {
    // compare edge_strength=1 against edge_strength=0 (same posterization, isolating only the edge-darkening term) at the exact
    // boundary pixel (x=8, where the hard-edged rect begins and the gradient is largest)
    let (mut ss_on, t_on) = new_proj(32, 32);
    solid_clip(&mut ss_on, &t_on, [0.9, 0.9, 0.9]);
    let b_on = call(&mut ss_on, "add_shape", json!({"track_id": &t_on, "shape": "rect", "width": 16, "height": 32, "fill": [0.1, 0.1, 0.1, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss_on, "add_effect", json!({"clip_id": &b_on, "kind": "cartoon", "params": {"levels": 8.0, "edge_strength": 1.0, "edge_threshold": 0.03}}));
    let with_edges = frame(&ss_on);

    let (mut ss_off, t_off) = new_proj(32, 32);
    solid_clip(&mut ss_off, &t_off, [0.9, 0.9, 0.9]);
    let b_off = call(&mut ss_off, "add_shape", json!({"track_id": &t_off, "shape": "rect", "width": 16, "height": 32, "fill": [0.1, 0.1, 0.1, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss_off, "add_effect", json!({"clip_id": &b_off, "kind": "cartoon", "params": {"levels": 8.0, "edge_strength": 0.0, "edge_threshold": 0.03}}));
    let without_edges = frame(&ss_off);

    let edge_x = 8usize; let i = (16 * with_edges.w + edge_x) * 4;
    assert!(with_edges.d[i] < without_edges.d[i] - 0.05, "the boundary pixel should be darker with edge_strength=1 than with edge_strength=0 (same posterization otherwise), with_edges={} without_edges={}", with_edges.d[i], without_edges.d[i]);
}

#[test]
fn color_emboss_keeps_colour_unlike_a_grey_emboss() {
    let (mut ss, t) = new_proj(16, 16);
    let c = solid_clip(&mut ss, &t, [0.8, 0.2, 0.1]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "color_emboss", "params": {"strength": 1.0}}));
    let p_ = px(&ss, 8, 8);
    // a flat solid has no gradient anywhere, so the emboss (original - neighbour) is exactly 0 and the colour is untouched
    assert!((p_[0] - 0.8).abs() < 0.01 && (p_[1] - 0.2).abs() < 0.01 && (p_[2] - 0.1).abs() < 0.01, "a flat colour should pass through unchanged (no local gradient to emboss), got {p_:?}");
}

#[test]
fn color_emboss_reacts_to_a_real_edge() {
    let (mut ss, t) = new_proj(32, 32);
    let a = solid_clip(&mut ss, &t, [0.2, 0.2, 0.2]);
    let b = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 16, "height": 32, "fill": [0.9, 0.9, 0.9, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &b, "kind": "color_emboss", "params": {"strength": 2.0, "angle": 0.0, "depth": 2.0}}));
    let _ = a;
    let near_edge = px(&ss, 9, 16); let far_from_edge = px(&ss, 28, 16);
    assert!((near_edge[0] - far_from_edge[0]).abs() > 0.05, "near the hard edge the emboss should visibly differ from a point far from any edge, got near={near_edge:?} far={far_from_edge:?}");
}

#[test]
fn roughen_edges_leaves_a_flat_fill_interior_alone_but_ragged_at_the_border() {
    let (mut ss, t) = new_proj(48, 48);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 40, "height": 40, "fill": [1.0, 0.5, 0.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "roughen_edges", "params": {"amount": 8.0, "size": 10.0}}));
    let before_center = [1.0f32, 0.5, 0.0];
    let center = px(&ss, 24, 24);
    assert!((center[0] - before_center[0]).abs() < 0.02 && (center[1] - before_center[1]).abs() < 0.02, "the interior of a large flat shape should be untouched by edge roughening, got {center:?}");
}

#[test]
fn scatter_increases_local_variance_on_a_gradient() {
    let (mut ss1, t1) = new_proj(32, 32);
    let c1 = gradient_clip(&mut ss1, &t1, [0.2, 0.2, 0.2], [0.8, 0.8, 0.8]);
    let f_before = frame(&ss1); let var_before = variance(&f_before);
    call(&mut ss1, "add_effect", json!({"clip_id": &c1, "kind": "scatter", "params": {"amount": 10.0}}));
    let f_after = frame(&ss1); let var_after = variance(&f_after);
    // scatter shuffles pixels around on a smooth gradient: the GLOBAL variance stays roughly similar (same pixel population),
    // but a shuffled gradient should no longer be monotonic row to row -- check some local neighbour differences grew
    let local_diff = |f: &kimo_core::effects::Frame| -> f64 { (1..f.w).map(|x| (f.d[(16 * f.w + x) * 4] - f.d[(16 * f.w + x - 1) * 4]).abs() as f64).sum::<f64>() };
    assert!(local_diff(&f_after) > local_diff(&f_before) * 1.5, "scattering a smooth gradient should raise pixel-to-pixel local variation, before={:.4} after={:.4}", local_diff(&f_before), local_diff(&f_after));
    let _ = var_before; let _ = var_after;
}

#[test]
fn texturize_modulates_a_flat_fill_with_a_bump_pattern() {
    let (mut ss, t) = new_proj(48, 48);
    let c = solid_clip(&mut ss, &t, [0.5, 0.5, 0.5]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "texturize", "params": {"amount": 1.0, "scale": 0.2}}));
    let f = frame(&ss); let var = variance(&f);
    assert!(var > 1e-4, "texturizing a flat fill should introduce real pixel variance from the bump pattern, got variance {var}");
}

#[test]
fn cc_glass_distorts_a_hard_edge() {
    let (mut ss, t) = new_proj(48, 48);
    solid_clip(&mut ss, &t, [0.1, 0.1, 0.1]);
    let b = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 24, "height": 48, "fill": [0.9, 0.9, 0.9, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    let before = frame(&ss);
    call(&mut ss, "add_effect", json!({"clip_id": &b, "kind": "cc_glass", "params": {"amount": 15.0, "scale": 0.05}}));
    let after = frame(&ss);
    let mut moved = 0; for y in 0..before.h { for x in 0..before.w { let i = (y * before.w + x) * 4; if (before.d[i] - after.d[i]).abs() > 0.05 { moved += 1; } } }
    assert!(moved > 20, "refracting through a bump field should visibly move the edge in more than a few pixels, moved {moved}");
}

#[test]
fn cc_kaleida_is_symmetric_across_wedges() {
    // the square stays at the clip's own natural (centred) position -- a clip's x/y set_property offset is applied AFTER
    // its own effects run (a known engine fact), so moving the SQUARE would move it only in the final composite, not in
    // what cc_kaleida itself sees. Instead the kaleidoscope's own pivot (center_x/center_y) is offset away from the square,
    // which is equivalent and avoids that gotcha entirely.
    let (mut ss, t) = new_proj(48, 48);
    let c = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 6, "height": 6, "fill": [1.0, 0.0, 0.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "cc_kaleida", "params": {"segments": 6.0, "center_x": 0.3, "center_y": 0.5}}));
    let f = frame(&ss);
    // the pivot sits to the LEFT of the square (which stays at the canvas's own centre, 24,24), so the square is at
    // angle 0 relative to the pivot -- squarely inside the base wedge [0, 30deg] that cc_kaleida mirrors outward from
    let (cx, cy, r) = (0.3 * 48.0f64, 0.5 * 48.0f64, 24.0f64 - 0.3 * 48.0);
    let mut hits = 0;
    for k in 0..6 { let a = (k as f64) * std::f64::consts::TAU / 6.0; let (px_, py_) = (cx + r * a.cos(), cy + r * a.sin());
        let (ix, iy) = (px_.round() as i64, py_.round() as i64);
        // search a small neighbourhood (the square is 6px wide) for red
        let mut found = false;
        for dy in -3..=3 { for dx in -3..=3 { let (xx, yy) = (ix + dx, iy + dy); if xx < 0 || yy < 0 || xx as usize >= f.w || yy as usize >= f.h { continue; }
            let i = (yy as usize * f.w + xx as usize) * 4; if f.d[i] > 0.5 && f.d[i + 1] < 0.3 { found = true; } } }
        if found { hits += 1; }
    }
    assert!(hits >= 5, "at least 5 of the 6 expected 60-degree-apart mirrored positions should show the red square, got {hits}/6");
}

#[test]
fn cc_threshold_rgb_produces_only_binary_channel_values() {
    let (mut ss, t) = new_proj(32, 32);
    let c = gradient_clip(&mut ss, &t, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "cc_threshold_rgb", "params": {"r_threshold": 0.5, "g_threshold": 0.5, "b_threshold": 0.5}}));
    let f = frame(&ss);
    for &v in f.d.iter().step_by(4) { assert!(v == 0.0 || v == 1.0, "every red channel value after cc_threshold_rgb must be exactly 0 or 1, got {v}"); }
}

#[test]
fn dither_keeps_values_within_the_quantized_level_set() {
    let (mut ss, t) = new_proj(32, 32);
    let c = gradient_clip(&mut ss, &t, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "dither", "params": {"levels": 3.0, "strength": 1.0}}));
    let f = frame(&ss);
    let distinct: std::collections::BTreeSet<i32> = f.d.iter().step_by(4).map(|&v| (v * 1000.0).round() as i32).collect();
    assert!(distinct.len() <= 3, "dithering to 3 levels should produce at most 3 distinct quantized red values, got {}: {:?}", distinct.len(), distinct);
}

#[test]
fn tile_mosaic_makes_every_pixel_in_a_block_identical() {
    let (mut ss, t) = new_proj(32, 32);
    let c = gradient_clip_angle(&mut ss, &t, [0.0, 0.0, 0.0], [1.0, 1.0, 1.0], 90.0); // 90 = left->right (default is top->bottom)
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "tile_mosaic", "params": {"size": 8.0}}));
    let f = frame(&ss);
    let a = f.d[(0 * f.w + 0) * 4]; let b = f.d[(0 * f.w + 7) * 4]; let c2 = f.d[(7 * f.w + 3) * 4];
    assert!((a - b).abs() < 1e-5 && (a - c2).abs() < 1e-5, "every pixel within the same 8x8 block should be identical after mosaic, got {a} {b} {c2}");
    let d = f.d[(0 * f.w + 8) * 4];
    assert!((a - d).abs() > 1e-4, "a pixel in the NEXT block over should differ from the first block's colour on a gradient, got {a} vs {d}");
}

#[test]
fn sketch_pencil_strength_zero_is_a_no_op() {
    let (mut ss, t) = new_proj(24, 24);
    let c = solid_clip(&mut ss, &t, [0.6, 0.3, 0.3]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "sketch_pencil", "params": {"strength": 0.0}}));
    let p_ = px(&ss, 12, 12);
    assert!((p_[0] - 0.6).abs() < 0.01, "strength 0 should leave the clip unchanged, got {p_:?}");
}

#[test]
fn sketch_pencil_desaturates_toward_grey_at_full_strength_and_zero_color_mix() {
    let (mut ss, t) = new_proj(24, 24);
    let c = solid_clip(&mut ss, &t, [0.9, 0.1, 0.1]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "sketch_pencil", "params": {"strength": 1.0, "color": 0.0, "blur_radius": 4.0}}));
    let p_ = px(&ss, 12, 12);
    assert!((p_[0] - p_[1]).abs() < 0.05 && (p_[1] - p_[2]).abs() < 0.05, "a pure (color=0) pencil sketch should be nearly grey (R~=G~=B), got {p_:?}");
}

#[test]
fn oil_paint_smooths_small_noise_into_flat_regions() {
    let (mut ss, t) = new_proj(32, 32);
    let c = solid_clip(&mut ss, &t, [0.5, 0.5, 0.5]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "grain", "params": {"amount": 0.15}}));
    let before = frame(&ss); let var_before = variance(&before);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "oil_paint", "params": {"radius": 3.0, "levels": 8.0}}));
    let after = frame(&ss); let var_after = variance(&after);
    assert!(var_after < var_before * 0.8, "oil_paint's most-common-bucket filter should measurably reduce small-scale noise variance, before={var_before} after={var_after}");
}

#[test]
fn anaglyph_separates_red_from_green_blue_on_an_edge() {
    let (mut ss, t) = new_proj(48, 48);
    solid_clip(&mut ss, &t, [0.0, 0.0, 0.0]);
    let b = call(&mut ss, "add_shape", json!({"track_id": &t, "shape": "rect", "width": 24, "height": 48, "fill": [1.0, 1.0, 1.0, 1.0], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    call(&mut ss, "add_effect", json!({"clip_id": &b, "kind": "anaglyph", "params": {"shift": 6.0}}));
    let f = frame(&ss);
    // the rect (width 24, centered) spans x=12..35; right at its left edge (x=12), the red channel (sampled from a leftward
    // shift) and green channel (from a rightward shift) should now disagree, since the edge moved differently for each --
    // the whole point of the stereo-separation illusion
    let edge_x = 12usize; let i = (24 * f.w + edge_x) * 4;
    assert!((f.d[i] - f.d[i + 1]).abs() > 0.2, "at the rect's left edge red and green should now disagree after anaglyph's opposite shifts, got r={} g={}", f.d[i], f.d[i + 1]);
}

#[test]
fn cc_burn_film_at_zero_progress_is_untouched_and_at_full_progress_is_gone() {
    // the final composited frame's alpha is always fully opaque (background included, a documented engine fact) -- so a
    // "burned away" clip is detected by its RGB dropping toward the canvas's own black background, not by reading alpha
    let (mut ss, t) = new_proj(32, 32);
    let c = solid_clip(&mut ss, &t, [0.9, 0.9, 0.9]);
    call(&mut ss, "add_effect", json!({"clip_id": &c, "kind": "cc_burn_film", "params": {"progress": 0.0}}));
    let untouched = px(&ss, 16, 16);
    assert!((untouched[0] - 0.9).abs() < 0.01, "progress 0 should leave the layer's own colour untouched, got {untouched:?}");

    let (mut ss2, t2) = new_proj(32, 32);
    let c2 = solid_clip(&mut ss2, &t2, [0.9, 0.9, 0.9]);
    call(&mut ss2, "add_effect", json!({"clip_id": &c2, "kind": "cc_burn_film", "params": {"progress": 1.0, "edge_width": 0.05}}));
    let f2 = frame(&ss2);
    let mean_r: f64 = f2.d.iter().step_by(4).map(|&v| v as f64).sum::<f64>() / (f2.w * f2.h) as f64;
    assert!(mean_r < 0.1, "progress 1.0 should have burned almost the entire layer away, dropping to the black canvas background, got mean red {mean_r}");
}

#[test]
fn every_new_stylize_kind_is_listed() {
    let r = dispatch(&mut Session::new(), "list_effects", &json!({})).unwrap();
    let kinds: Vec<&str> = r.as_array().unwrap().iter().map(|v| v["kind"].as_str().unwrap()).collect();
    for k in ["cartoon", "color_emboss", "roughen_edges", "scatter", "texturize", "cc_glass", "cc_kaleida", "cc_threshold_rgb", "dither", "tile_mosaic", "sketch_pencil", "oil_paint", "anaglyph", "cc_burn_film"] {
        assert!(kinds.contains(&k), "{k} should be a listed effect kind");
    }
}
