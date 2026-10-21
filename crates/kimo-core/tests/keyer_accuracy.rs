//! Keyer accuracy gates (lane K1). Synthetic plates with a KNOWN foreground F, KNOWN alpha and KNOWN screen K:
//! C = alpha*F + (1-alpha)*K (Porter-Duff over, straight colour). Alpha carries a solid core, a supersampled anti-aliased edge,
//! a 0.3-alpha motion-blur strip with ramps, and 1-px hair strands. The screen is flat or vignetted (0.8..1.2 of the flat screen).
//! Gates (never loosen): alpha MAE <= 0.005 on core (alpha==1) and background (alpha==0) pixels, <= 0.03 on edge pixels;
//! recovered foreground RMS <= 0.01 where alpha > 0.1; residual spill (key channel minus max of the others, clamped at 0, mean
//! where alpha > 0.1) <= 0.005. The vignetted screen must pass only with the clean plate on.
//! Method: linear colour-difference screen matte (Smith & Blinn, "Blue Screen Matting", SIGGRAPH 1996) against a per-pixel
//! screen colour, trimap from clip black/white, edge alpha by projection onto a propagated foreground colour.
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use kimo_core::keying::{apply_keyer, KeyerAux};

const W: usize = 256; const H: usize = 160;
const SCREEN: [f32; 3] = [0.10, 0.70, 0.15];

fn smoothstep(a: f32, b: f32, x: f32) -> f32 { let t = ((x - a) / (b - a)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

/// Known foreground colour field: hair (top), skin (middle), blue shirt (bottom), a grey sleeve on the right.
/// Low-frequency on purpose (region colours with slow drifts); stated in the report.
fn fg(x: f32, y: f32) -> [f32; 3] {
    let hair = [0.36 + 0.02 * (x / W as f32), 0.23, 0.13];
    let skin = [0.80, 0.60 + 0.02 * (y / H as f32), 0.49];
    let shirt = [0.20, 0.26, 0.62 - 0.02 * (x / W as f32)];
    let sleeve = [0.82, 0.82, 0.84];
    let a = smoothstep(52.0, 72.0, y); let b = smoothstep(118.0, 132.0, y); let s = smoothstep(160.0, 176.0, x);
    let mut c = [0.0; 3];
    for j in 0..3 { let v = hair[j] * (1.0 - a) + skin[j] * a; let v = v * (1.0 - b) + shirt[j] * b; c[j] = v * (1.0 - s) + sleeve[j] * s; }
    c
}

fn seg_dist(px: f32, py: f32, a: [f32; 2], b: [f32; 2]) -> f32 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]); let t = (((px - a[0]) * dx + (py - a[1]) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
    ((px - a[0] - t * dx).powi(2) + (py - a[1] - t * dy).powi(2)).sqrt()
}

/// Ground-truth alpha, 8x8 supersampled coverage per pixel.
fn alpha_true() -> Vec<f32> {
    let strands: Vec<([f32; 2], [f32; 2])> = (0..12).map(|k| { let ang = -2.6 + k as f32 * 0.19; let r0 = 30.0; let r1 = 30.0 + 18.0 + (k % 4) as f32 * 5.0;
        ([100.0 + r0 * ang.cos(), 50.0 + r0 * ang.sin()], [100.0 + r1 * ang.cos(), 50.0 + r1 * ang.sin()]) }).collect();
    let mut a = vec![0f32; W * H];
    for y in 0..H { for x in 0..W {
        let mut cov = 0.0f32; let mut hair = 0.0f32;
        for sy in 0..8 { for sx in 0..8 { let (px, py) = (x as f32 + (sx as f32 + 0.5) / 8.0, y as f32 + (sy as f32 + 0.5) / 8.0);
            let body = ((px - 100.0) / 52.0).powi(2) + ((py - 108.0) / 46.0).powi(2) <= 1.0;
            let head = ((px - 100.0).powi(2) + (py - 50.0).powi(2)).sqrt() <= 30.0;
            let sleeve = px >= 150.0 && px <= 200.0 && py >= 96.0 && py <= 140.0;
            if body || head || sleeve { cov += 1.0 / 64.0; }
            else if strands.iter().any(|(s0, s1)| seg_dist(px, py, *s0, *s1) <= 0.5) { hair += 1.0 / 64.0; } } }
        let mut v = cov + 0.8 * hair; // hair strands are semi-transparent (0.8 at full pixel coverage)
        // motion-blurred strip: 0.3 alpha with 6-px linear ramps, x 204..250, y 60..92
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        if fx >= 204.0 && fx <= 250.0 && fy >= 60.0 && fy <= 92.0 { let r = smoothstep(204.0, 210.0, fx).min(1.0 - smoothstep(244.0, 250.0, fx)); v = v.max(0.3 * r); }
        a[y * W + x] = v.clamp(0.0, 1.0);
    } }
    a
}

fn vignette(x: usize, y: usize) -> f32 { let (dx, dy) = ((x as f32 + 0.5) / W as f32 - 0.5, (y as f32 + 0.5) / H as f32 - 0.5); let r2 = (dx * dx + dy * dy) / 0.5; 1.2 - 0.4 * r2 }

struct Plate { c: Frame, k: Frame, a: Vec<f32>, f: Vec<[f32; 3]> }
fn plate(vignetted: bool, spill: f32) -> Plate {
    let a = alpha_true(); let mut c = Frame { w: W, h: H, d: vec![1.0; W * H * 4] }; let mut k = Frame { w: W, h: H, d: vec![1.0; W * H * 4] }; let mut fv = vec![[0f32; 3]; W * H];
    for y in 0..H { for x in 0..W { let i = y * W + x; let v = if vignetted { vignette(x, y) } else { 1.0 }; let mut f = fg(x as f32, y as f32);
        // optional green spill on the grey sleeve (a lit-by-the-screen subject): the key channel exceeds the others by `spill`
        if spill > 0.0 && f[0] > 0.8 { f[1] = f[0].max(f[2]) + spill; }
        fv[i] = f; for j in 0..3 { let kk = SCREEN[j] * v; k.d[i * 4 + j] = kk; c.d[i * 4 + j] = a[i] * f[j] + (1.0 - a[i]) * kk; } } }
    Plate { c, k, a, f: fv }
}

fn params(kv: &[(&str, f64)]) -> Params { let mut p = Params::new(); for (k, v) in kv { p.insert(k.to_string(), Prop::new(*v)); } p }
fn model1() -> Params { params(&[("r", SCREEN[0] as f64), ("g", SCREEN[1] as f64), ("b", SCREEN[2] as f64), ("matte_model", 1.0), ("edge_soften", 0.0), ("core_shrink", 0.0)]) }

#[derive(Debug)] struct Score { core_bg: f64, edge: f64, f_rms: f64, spill: f64, worst_edge: f64 }
fn score(p: &Plate, out: &Frame) -> Score {
    let (mut cb, mut ncb, mut e, mut ne, mut fr, mut nf, mut sp, mut worst) = (0f64, 0usize, 0f64, 0usize, 0f64, 0usize, 0f64, 0f64);
    for i in 0..W * H { let at = p.a[i]; let ag = out.d[i * 4 + 3];
        let err = (ag - at).abs() as f64; if at == 0.0 || at == 1.0 { cb += err; ncb += 1; } else { e += err; ne += 1; worst = worst.max(err); }
        if at > 0.1 { for j in 0..3 { fr += ((out.d[i * 4 + j] - p.f[i][j]) as f64).powi(2); } nf += 3; sp += (out.d[i * 4 + 1] - out.d[i * 4].max(out.d[i * 4 + 2])).max(0.0) as f64; } }
    Score { core_bg: cb / ncb as f64, edge: e / ne as f64, f_rms: (fr / nf as f64).sqrt(), spill: sp / (nf / 3) as f64, worst_edge: worst }
}
fn passes(s: &Score) -> bool { s.core_bg <= 0.005 && s.edge <= 0.03 && s.f_rms <= 0.01 && s.spill <= 0.005 }
fn run(p: &Plate, pr: &Params, aux: &KeyerAux) -> Frame { let mut f = Frame { w: W, h: H, d: p.c.d.clone() }; apply_keyer(pr, &mut f, 0.0, aux).expect("keyer"); assert!(f.d.iter().all(|v| v.is_finite()), "non-finite keyer output"); f }

#[test]
fn plate_is_what_it_says() {
    let p = plate(false, 0.0); let n_edge = p.a.iter().filter(|&&a| a > 0.0 && a < 1.0).count(); let n_strip = p.a.iter().filter(|&&a| (a - 0.3).abs() < 1e-6).count();
    let n_hair = p.a.iter().filter(|&&a| a > 0.0 && a < 0.8 + 1e-6).count();
    eprintln!("plate: edge px {n_edge}, 0.3 strip px {n_strip}, partial<=0.8 px {n_hair}");
    assert!(n_edge > 800 && n_strip > 500 && n_hair > 600);
    for i in 0..W * H { for j in 0..3 { let f = p.f[i][j]; assert!((0.0..=1.0).contains(&f)); } let f = p.f[i]; assert!(f[1] <= f[0].max(f[2]) + 1e-6, "test F must carry no spill"); }
}

#[test]
fn flat_screen_constant_key_meets_gates() {
    let p = plate(false, 0.0); let s = score(&p, &run(&p, &model1(), &KeyerAux::default()));
    eprintln!("flat screen model1: {s:?} (worst edge pixel error {:.4})", s.worst_edge);
    assert!(passes(&s), "flat screen gates failed: {s:?}");
}

#[test]
fn vignetted_screen_needs_the_clean_plate() {
    let p = plate(true, 0.0);
    let off = score(&p, &run(&p, &model1(), &KeyerAux::default()));
    eprintln!("vignetted, no clean plate: {off:?}");
    assert!(!passes(&off), "vignetted screen must NOT pass without a clean plate: {off:?}");
    let on = score(&p, &run(&p, &model1(), &KeyerAux { clean_plate: Some(&p.k), ..Default::default() }));
    eprintln!("vignetted, clean plate on: {on:?}");
    assert!(passes(&on), "vignetted screen with clean plate failed: {on:?}");
}

#[test]
fn flat_screen_with_clean_plate_also_meets_gates() {
    let p = plate(false, 0.0); let s = score(&p, &run(&p, &model1(), &KeyerAux { clean_plate: Some(&p.k), ..Default::default() }));
    eprintln!("flat screen, clean plate on: {s:?}");
    assert!(passes(&s), "{s:?}");
}

#[test]
fn spill_on_the_subject_is_removed_by_despill_limit() {
    // The sleeve is lit green (key channel = max(others) + 0.04). The test F is the SPILLED colour, so the F gate is measured
    // against the despilled truth instead: key channel limited to max(others).
    let p = plate(false, 0.04); let mut pr = model1(); pr.insert("despill_mode".into(), Prop::new(2.0)); pr.insert("clip_white".into(), Prop::new(0.9));
    let out = run(&p, &pr, &KeyerAux::default());
    let mut truth = Plate { c: Frame { w: W, h: H, d: vec![] }, k: Frame { w: W, h: H, d: vec![] }, a: p.a.clone(), f: p.f.clone() };
    for f in truth.f.iter_mut() { f[1] = f[1].min(f[0].max(f[2])); }
    let s = score(&truth, &out); eprintln!("spilled sleeve, despill limit: {s:?}");
    assert!(passes(&s), "{s:?}");
    // average mode leaves spill where the other two channels differ; measured, not gated
    let mut pa = pr.clone(); pa.insert("despill_mode".into(), Prop::new(0.0)); let sa = score(&truth, &run(&p, &pa, &KeyerAux::default())); eprintln!("spilled sleeve, despill average: {sa:?}");
}

#[test]
fn garbage_and_core_mattes_force_alpha() {
    let p = plate(false, 0.0); let mut garbage = vec![0f32; W * H]; let mut core = vec![0f32; W * H];
    for y in 0..H { for x in 0..W { if x < 20 { garbage[y * W + x] = 1.0; } if (90..110).contains(&x) && (100..120).contains(&y) { core[y * W + x] = 1.0; } } }
    // put junk in the garbage region (a tracking marker / light stand) and a screen-coloured hole in the core region
    let mut c = Frame { w: W, h: H, d: p.c.d.clone() }; for y in 10..30 { for x in 4..14 { let i = (y * W + x) * 4; c.d[i] = 0.9; c.d[i + 1] = 0.9; c.d[i + 2] = 0.9; } }
    for y in 104..116 { for x in 94..106 { let i = (y * W + x) * 4; for j in 0..3 { c.d[i + j] = SCREEN[j]; } } }
    let mut f = Frame { w: W, h: H, d: c.d.clone() }; apply_keyer(&model1(), &mut f, 0.0, &KeyerAux { garbage: Some(&garbage), core: Some(&core), ..Default::default() }).unwrap();
    for y in 0..H { for x in 0..W { let a = f.d[(y * W + x) * 4 + 3]; if x < 20 { assert_eq!(a, 0.0, "garbage at {x},{y}"); } if (90..110).contains(&x) && (100..120).contains(&y) { assert_eq!(a, 1.0, "core at {x},{y}"); } } }
}

#[test]
fn status_view_classifies_pixels() {
    let p = plate(false, 0.0); let mut pr = model1(); pr.insert("view".into(), Prop::new(3.0)); let out = run(&p, &pr, &KeyerAux::default());
    let mut res = model1(); res.insert("view".into(), Prop::new(0.0)); let r = run(&p, &res, &KeyerAux::default());
    for i in 0..W * H { let a = r.d[i * 4 + 3]; let px = &out.d[i * 4..i * 4 + 4]; assert_eq!(px[3], 1.0);
        if a == 0.0 { assert_eq!(&px[..3], &[0.0, 0.0, 0.0]); } else if a < 1.0 { assert_eq!(&px[..3], &[0.5, 0.5, 0.5]); } else { assert!(px[0] == 1.0 && px[1] == 1.0, "opaque shows white or yellow (spill flagged): {px:?}"); } }
}
