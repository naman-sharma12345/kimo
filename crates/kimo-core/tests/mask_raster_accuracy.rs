//! Path mask rasterisation and feather accuracy (owner's gate 2). Reference B is EXACT: the bezier flattened to ~1e-7 px and
//! exact per-pixel area coverage in f64. Reference A (16x16 point supersampling) is printed too; its own error on edge pixels
//! is up to ~1/32, so it cannot certify 1e-4 and is not gated. Gate: precision=1 coverage within 1e-4 of B on curved shapes.
//! Feather: exact hand-computed ramps for inner/outer feather and linear/smooth falloff.
use kimo_core::masks::{path_coverage, PathPt};
type P = (f64, f64);
const W: usize = 640; const H: usize = 360;

fn pt(x: f64, y: f64, ix: f64, iy: f64, ox: f64, oy: f64) -> PathPt { PathPt { x: x / W as f64, y: y / H as f64, ix: ix / W as f64, iy: iy / H as f64, ox: ox / W as f64, oy: oy / H as f64, ..Default::default() } }
fn circle(cx: f64, cy: f64, r: f64) -> Vec<PathPt> { let k = 0.5522847498 * r; vec![pt(cx + r, cy, 0.0, -k, 0.0, k), pt(cx, cy + r, k, 0.0, -k, 0.0), pt(cx - r, cy, 0.0, k, 0.0, -k), pt(cx, cy - r, -k, 0.0, k, 0.0)] }
fn blob() -> Vec<PathPt> { vec![pt(200.0, 100.0, -60.0, 30.0, 70.0, -40.0), pt(420.0, 90.0, -50.0, -30.0, 80.0, 60.0), pt(470.0, 250.0, 40.0, -70.0, -90.0, 50.0),
    pt(320.0, 210.0, 60.0, 40.0, -70.0, -50.0), pt(180.0, 290.0, 50.0, 30.0, -40.0, -60.0), pt(150.0, 170.0, -10.0, 60.0, 20.0, -50.0)] }
/// reference flattening: 20000 uniform steps per segment (chord error ~1e-7 px for these sizes)
fn fine(pts: &[PathPt]) -> Vec<P> { let n = pts.len(); let mut v = vec![]; for i in 0..n { let (a, b) = (&pts[i], &pts[(i + 1) % n]);
    let c = [(a.x * W as f64, a.y * H as f64), ((a.x + a.ox) * W as f64, (a.y + a.oy) * H as f64), ((b.x + b.ix) * W as f64, (b.y + b.iy) * H as f64), (b.x * W as f64, b.y * H as f64)];
    for s in 0..20000 { let u = s as f64 / 20000.0; let w = 1.0 - u; v.push((w * w * w * c[0].0 + 3.0 * w * w * u * c[1].0 + 3.0 * w * u * u * c[2].0 + u * u * u * c[3].0, w * w * w * c[0].1 + 3.0 * w * w * u * c[1].1 + 3.0 * w * u * u * c[2].1 + u * u * u * c[3].1)); } } v }
/// exact area coverage (nonzero), f64 cell-area accumulation (independent of kimo's rasteriser)
fn exact(poly: &[P]) -> Vec<f64> {
    let mut acc = vec![0f64; (W + 2) * H]; let n = poly.len();
    for i in 0..n { let (a, b) = (poly[i], poly[(i + 1) % n]); if a.1 == b.1 { continue; }
        let (dir, p0, p1) = if a.1 < b.1 { (1.0, a, b) } else { (-1.0, b, a) }; let dxdy = (p1.0 - p0.0) / (p1.1 - p0.1);
        for y in (p0.1.max(0.0).floor() as usize)..((p1.1.min(H as f64)).ceil() as usize) { let (ya, yb) = (p0.1.max(y as f64), p1.1.min(y as f64 + 1.0)); if yb <= ya { continue; }
            let (xa, xb) = (p0.0 + (ya - p0.1) * dxdy, p0.0 + (yb - p0.1) * dxdy); let mut cuts = vec![(xa, ya)]; let (lo, hi) = (xa.min(xb), xa.max(xb)); let mut mids = vec![]; let mut xi = lo.floor() + 1.0;
            while xi < hi { mids.push((xi, p0.1 + (xi - p0.0) / dxdy)); xi += 1.0; } if xb < xa { mids.reverse(); } cuts.extend(mids); cuts.push((xb, yb));
            for s in cuts.windows(2) { let dy = (s[1].1 - s[0].1) * dir; let xm = 0.5 * (s[0].0 + s[1].0); let cx = xm.floor(); let ci = (cx.max(-1.0) + 1.0) as usize; if ci >= W + 1 { continue; } let fr = xm - cx;
                acc[y * (W + 2) + ci] += dy * (1.0 - fr); acc[y * (W + 2) + ci + 1] += dy * fr; } } }
    let mut out = vec![0f64; W * H]; for y in 0..H { let mut s = 0.0; for x in 0..W + 1 { s += acc[y * (W + 2) + x]; if x >= 1 { out[y * W + x - 1] = s.abs().min(1.0); } } } out
}
/// 16x16 point-sampled supersampling (reference A): per sub-row crossings of the fine polygon, even-odd fill
fn super16(poly: &[P]) -> Vec<f64> { let mut out = vec![0f64; W * H]; let n = poly.len();
    for sy in 0..H * 16 { let y = (sy as f64 + 0.5) / 16.0; let mut xs: Vec<f64> = vec![];
        for i in 0..n { let (a, b) = (poly[i], poly[(i + 1) % n]); if (a.1 > y) != (b.1 > y) { xs.push(a.0 + (y - a.1) * (b.0 - a.0) / (b.1 - a.1)); } }
        xs.sort_by(|p, q| p.partial_cmp(q).unwrap());
        for pair in xs.chunks(2) { if pair.len() < 2 { continue; } let (l, r) = (pair[0], pair[1]); let s0 = ((l * 16.0 - 0.5).ceil().max(0.0)) as usize; let s1 = ((r * 16.0 - 0.5).floor().min((W * 16 - 1) as f64)) as i64;
            for sx in s0 as i64..=s1 { out[(sy / 16) * W + sx as usize / 16] += 1.0 / 256.0; } } }
    out }
fn errs(k: &[f32], r: &[f64]) -> (f64, f64) { let mut mx = 0f64; let mut s = 0.0; let mut c = 0usize; for i in 0..k.len() { let e = (k[i] as f64 - r[i]).abs(); mx = mx.max(e); if r[i] > 0.0 && r[i] < 1.0 { s += e; c += 1; } } (mx, s / c.max(1) as f64) }

#[test] fn curved_masks_meet_1e4_in_precision_mode() {
    let mut fails = vec![];
    for (name, shape) in [("circle r=100", circle(320.0, 180.0, 100.0)), ("S-blob", blob())] {
        let f = fine(&shape); let (b, a) = (exact(&f), super16(&f));
        let def = path_coverage(&shape, true, W, H, false, false, false); let pre = path_coverage(&shape, true, W, H, false, true, false);
        let ((dbm, dba), (dam, _), (pbm, pba), (pam, _)) = (errs(&def, &b), errs(&def, &a), errs(&pre, &b), errs(&pre, &a));
        let (am, aa) = { let mut mx = 0f64; let mut s = 0.0; let mut c = 0; for i in 0..b.len() { let e = (a[i] - b[i]).abs(); mx = mx.max(e); if b[i] > 0.0 && b[i] < 1.0 { s += e; c += 1; } } (mx, s / c as f64) };
        println!("{name:12}: default vs exact max {dbm:.2e} mean(edge) {dba:.2e} | precise vs exact max {pbm:.2e} mean(edge) {pba:.2e} | vs 16x16: default {dam:.2e} precise {pam:.2e} | 16x16 vs exact max {am:.2e} mean {aa:.2e}");
        if pbm > 1e-4 { fails.push(format!("{name}: precise max error {pbm:.3e} > 1e-4")); }
    }
    assert!(fails.is_empty(), "{}", fails.join("\n"));
}

fn rect_canvas(fi: f64, fo: f64, f: f64) -> Vec<PathPt> { // 480 x 320 px rectangle on a 1280 x 720 canvas (px@720p scale = 1)
    let (w, h) = (1280.0, 720.0); let p = |x: f64, y: f64| PathPt { x: x / w, y: y / h, f, fi, fo, ..Default::default() };
    vec![p(400.0, 200.0), p(880.0, 200.0), p(880.0, 520.0), p(400.0, 520.0)] }
fn ss(x: f64) -> f64 { let t = x.clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

#[test] fn outer_feather_linear_and_smooth_ramps_are_exact() {
    let pts = rect_canvas(0.0, 20.0, 0.0);
    let lin = path_coverage(&pts, true, 1280, 720, false, false, true); let smo = path_coverage(&pts, true, 1280, 720, false, false, false);
    let row = 360usize;
    for k in 0..24usize { let x = 880 + k; let d = k as f64 + 0.5; // pixel centre distance to the right edge
        let (wl, ws) = ((1.0 - d / 20.0).max(0.0), 1.0 - ss(d / 20.0));
        assert!((lin[row * 1280 + x] as f64 - wl).abs() < 1e-6, "linear outer x={x}: {} want {wl}", lin[row * 1280 + x]);
        assert!((smo[row * 1280 + x] as f64 - ws).abs() < 1e-6, "smooth outer x={x}: {} want {ws}", smo[row * 1280 + x]); }
    for k in 0..30usize { assert_eq!(lin[row * 1280 + 879 - k], 1.0, "inside stays solid with outer-only feather"); }
}

#[test] fn inner_feather_ramp_is_exact_and_outside_stays_clear() {
    let pts = rect_canvas(10.0, 0.0, 0.0); let lin = path_coverage(&pts, true, 1280, 720, false, false, true); let row = 360usize;
    for k in 0..14usize { let x = 879 - k; let d = k as f64 + 0.5; let want = (d / 10.0).min(1.0);
        assert!((lin[row * 1280 + x] as f64 - want).abs() < 1e-6, "linear inner x={x}: {} want {want}", lin[row * 1280 + x]); }
    for k in 0..10usize { assert_eq!(lin[row * 1280 + 880 + k], 0.0, "outside stays clear with inner-only feather"); }
}

#[test] fn asymmetric_feather_spans_inner_plus_outer() {
    let pts = rect_canvas(6.0, 18.0, 0.0); let lin = path_coverage(&pts, true, 1280, 720, false, false, true); let row = 360usize;
    for k in -8i64..20 { let x = (880 + k) as usize; let sd = k as f64 + 0.5; let want = 1.0 - ((sd + 6.0) / 24.0).clamp(0.0, 1.0);
        assert!((lin[row * 1280 + x] as f64 - want).abs() < 1e-6, "x={x}: {} want {want}", lin[row * 1280 + x]); }
}

#[test] fn symmetric_feather_matches_the_old_f_path_in_the_new_engine() {
    // f = 20 (old symmetric feather) through the new engine with linear off and precision on equals a 10/10 inner/outer smooth ramp
    let a = path_coverage(&rect_canvas(0.0, 0.0, 20.0), true, 1280, 720, false, true, false); let b = path_coverage(&rect_canvas(10.0, 10.0, 0.0), true, 1280, 720, false, true, false);
    let mx = a.iter().zip(&b).map(|(x, y)| (x - y).abs()).fold(0.0f32, f32::max); assert!(mx < 1e-6, "max diff {mx}");
    // and the default engine keeps the historic f-feather on the same shape (unchanged code path)
    let old = path_coverage(&rect_canvas(0.0, 0.0, 20.0), true, 1280, 720, false, false, false); let row = 360usize;
    for k in -12i64..12 { let x = (880 + k) as usize; let sd = k as f64 + 0.5; let want = 1.0 - ss((sd + 10.0) / 20.0);
        assert!((old[row * 1280 + x] as f64 - want).abs() < 1e-5, "historic f path x={x}: {} want {want}", old[row * 1280 + x]); }
}

#[test] fn fi_fo_are_not_serialised_when_zero() {
    let p = PathPt { x: 0.5, y: 0.25, ..Default::default() }; let s = serde_json::to_string(&p).unwrap();
    assert_eq!(s, r#"{"x":0.5,"y":0.25,"ix":0.0,"iy":0.0,"ox":0.0,"oy":0.0,"f":0.0}"#);
    let q = PathPt { fi: 3.0, ..p }; assert!(serde_json::to_string(&q).unwrap().contains(r#""fi":3.0"#));
}
