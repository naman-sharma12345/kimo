//! TDD for v7's 3 new emitter presets (bubbles, foam, card_dance) and the new `card` sprite kind.
//! Direct against kimo_core::particles -- no MCP round trip needed, so these run in milliseconds.
use kimo_core::effects::Frame;
use kimo_core::particles::{preset, render, Emitter};

fn emit(cfg: serde_json::Value, t: f64, w: usize, h: usize) -> Frame {
    let e: Emitter = serde_json::from_value(cfg).expect("valid emitter config");
    let mut f = Frame::try_new(w, h).unwrap();
    render(&e, &mut f, t);
    f
}

fn nonzero_alpha_px(f: &Frame) -> usize { f.d.chunks_exact(4).filter(|p| p[3] > 0.01).count() }

#[test]
fn bubbles_preset_parses_and_renders_particles() {
    let cfg = preset("bubbles").unwrap();
    let f = emit(cfg, 2.0, 320, 240);
    assert!(nonzero_alpha_px(&f) > 0, "bubbles preset should put visible pixels on screen");
}

#[test]
fn bubbles_rise_not_fall() {
    // buoyancy: negative gravity means a single bubble born at t=0 should be higher (smaller y in screen space) as it ages.
    // prewarm:false + a very low rate isolates particle i=0 (born at t=0) as the only one alive across the sampled window,
    // so this measures one bubble's own trajectory rather than a steady-state population's.
    let mut cfg = preset("bubbles").unwrap();
    cfg["prewarm"] = serde_json::json!(false);
    cfg["rate"] = serde_json::json!(0.1);
    cfg["life_var"] = serde_json::json!(0.0);
    cfg["width"] = serde_json::json!(0.0);
    let early = emit(cfg.clone(), 0.3, 320, 240);
    let late = emit(cfg, 2.5, 320, 240);
    let centroid_y = |f: &Frame| -> f64 {
        let (mut sy, mut sa) = (0.0, 0.0);
        for y in 0..f.h { for x in 0..f.w { let a = f.d[(y * f.w + x) * 4 + 3] as f64; sy += y as f64 * a; sa += a; } }
        if sa > 0.0 { sy / sa } else { f.h as f64 }
    };
    let (cy_early, cy_late) = (centroid_y(&early), centroid_y(&late));
    assert!(cy_late < cy_early, "bubble should move up over its own life: early={cy_early} late={cy_late}");
}

#[test]
fn foam_preset_parses_and_hugs_the_surface_line() {
    let cfg = preset("foam").unwrap();
    let f = emit(cfg, 0.5, 320, 240);
    assert!(nonzero_alpha_px(&f) > 0, "foam preset should render flecks");
    // foam's short life (0.6s) and emission at y=0.85 means mass should stay near that band, not spread the whole frame.
    let band_lo = (0.70 * f.h as f64) as usize;
    let band_hi = f.h; // allow drift toward the bottom edge
    let (mut in_band, mut total) = (0usize, 0usize);
    for y in 0..f.h { for x in 0..f.w { if f.d[(y * f.w + x) * 4 + 3] > 0.01 {
        total += 1; if y >= band_lo && y < band_hi { in_band += 1; } } } }
    assert!(total > 0);
    assert!(in_band as f64 / total as f64 > 0.8, "foam should stay near the surface band: {in_band}/{total}");
}

#[test]
fn card_dance_preset_uses_card_sprite_and_renders() {
    let cfg = preset("card_dance").unwrap();
    assert_eq!(cfg["sprite"], "card");
    let f = emit(cfg, 1.0, 320, 240);
    assert!(nonzero_alpha_px(&f) > 0, "card_dance preset should render card sprites");
}

#[test]
fn card_sprite_flip_phase_changes_footprint_over_time() {
    // The `card` sprite's half-width is scaled by |cos(card_phase)|, a function of (t, seed). A single stationary
    // card sampled at different t should visibly change its on-screen footprint (edge-on thin vs. face-on wide)
    // as the flip phase sweeps -- that is the whole point of the new sprite kind, so pin it with a direct measurement.
    let cfg = serde_json::json!({"shape":"point","x":0.5,"y":0.5,"rate":1,"life":100.0,"speed":0.0,"angle":0,"spread":0,
        "size0":80,"size1":80,"color0":[1,1,1],"color1":[1,1,1],"alpha0":1,"alpha1":1,"sprite":"card","seed":7});
    let widths: Vec<usize> = (0..12).map(|i| {
        let t = i as f64 * 0.17;
        let f = emit(cfg.clone(), t, 320, 240);
        let mut min_x = f.w; let mut max_x = 0usize;
        for y in 0..f.h { for x in 0..f.w { if f.d[(y * f.w + x) * 4 + 3] > 0.05 { min_x = min_x.min(x); max_x = max_x.max(x); } } }
        if max_x >= min_x { max_x - min_x } else { 0 }
    }).collect();
    let (wmin, wmax) = (*widths.iter().min().unwrap(), *widths.iter().max().unwrap());
    assert!(wmax > 0, "card sprite should render something: {widths:?}");
    assert!(wmax > wmin * 2, "card flip should sweep footprint width widely: {widths:?}");
}

#[test]
fn card_sprite_is_flatter_than_a_soft_circle_at_face_on_phase() {
    // At a face-on phase (card_phase near +-1), the card should be noticeably wider than tall (a rectangle),
    // unlike the circular "soft" sprite at the same radius -- the real distinguishing property of a new sprite kind.
    let base = serde_json::json!({"shape":"point","x":0.5,"y":0.5,"rate":1,"life":100.0,"speed":0.0,"angle":0,"spread":0,
        "size0":80,"size1":80,"color0":[1,1,1],"color1":[1,1,1],"alpha0":1,"alpha1":1,"seed":7});
    let extent = |sprite: &str, t: f64| -> (usize, usize) {
        let mut cfg = base.clone(); cfg["sprite"] = serde_json::json!(sprite);
        let f = emit(cfg, t, 320, 240);
        let (mut min_x, mut max_x, mut min_y, mut max_y) = (f.w, 0usize, f.h, 0usize);
        for y in 0..f.h { for x in 0..f.w { if f.d[(y * f.w + x) * 4 + 3] > 0.05 { min_x = min_x.min(x); max_x = max_x.max(x); min_y = min_y.min(y); max_y = max_y.max(y); } } }
        (max_x.saturating_sub(min_x), max_y.saturating_sub(min_y))
    };
    // scan t to find a near-face-on phase for seed 7 (card_phase = cos(t*4.5 + hash*tau) near +-1)
    let mut best_t = 0.0; let mut best_w = 0usize;
    for i in 0..200 { let t = i as f64 * 0.05; let (w, _) = extent("card", t); if w > best_w { best_w = w; best_t = t; } }
    let (card_w, card_h) = extent("card", best_t);
    let (soft_w, soft_h) = extent("soft", 0.0);
    assert!(card_w as f64 / card_h.max(1) as f64 > 1.3, "face-on card should read as a rectangle, not a circle: w={card_w} h={card_h}");
    assert!((soft_w as f64 / soft_h.max(1) as f64 - 1.0).abs() < 0.3, "soft sprite stays circular (sanity check on the measurement): w={soft_w} h={soft_h}");
}

#[test]
fn unknown_preset_still_rejected_and_error_lists_new_names() {
    let e = preset("not_a_real_preset").unwrap_err();
    assert!(e.contains("bubbles") && e.contains("foam") && e.contains("card_dance"), "error should list the new presets: {e}");
}
