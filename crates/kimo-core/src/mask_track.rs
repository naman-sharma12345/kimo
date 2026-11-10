//! Mask tracker (After Effects "Track Mask", Mocha-style): track the image region inside a path mask and move the whole
//! mask with it. Two stages per frame:
//!  1. initialiser: Shi-Tomasi features inside the region, pyramidal KLT with a forward/backward check (sfm.rs), and a robust
//!     frame-to-frame motion fit with RANSAC (robust_est.rs, ported from OpenCV) composed onto the previous frame's transform;
//!  2. drift-free refinement (kimo's own): Gauss-Newton / Levenberg-Marquardt on the photometric error between the
//!     REFERENCE frame's region pixels and the current frame, in the chosen motion model, with Catmull-Rom sampling and its
//!     analytic gradient, joint gain/bias (exposure) compensation and Huber IRLS weights against occluders. Because every frame
//!     is aligned to the reference frame, errors do not accumulate along the chain.
//! Coordinates: pixel units, pixel (i, j) covers [i, i+1) x [j, j+1); an image sample img[j*w+i] sits at (i+0.5, j+0.5).

use crate::robust_est::{self, Model};
use crate::trackmath::{self, H3, P};
use crate::masks::PathPt;

#[derive(Clone, Debug)]
pub struct Opts { pub max_features: usize, pub ransac_px: f64, pub refine: bool, pub expand: f64, pub seed: u64,
    /// test hook: replace this fraction of the tracked feature destinations by random positions before RANSAC (robustness gate)
    pub inject_outlier_frac: f64 }
impl Default for Opts { fn default() -> Self { Opts { max_features: 300, ransac_px: 1.5, refine: true, expand: 0.0, seed: 0x5eed_7a5c, inject_outlier_frac: 0.0 } } }

#[derive(Clone, Debug)]
pub struct FrameStat { pub frame: usize, pub features: usize, pub inliers: usize, pub rms: f64, pub refined: bool }
pub struct RegionTrack { pub h: Vec<H3>, pub stats: Vec<FrameStat>, pub warnings: Vec<String> }

pub const IDENTITY: H3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn inside(poly: &[P], x: f64, y: f64) -> bool { let n = poly.len(); let mut c = false; let mut j = n - 1; for i in 0..n { let (a, b) = (poly[i], poly[j]); if ((a.1 > y) != (b.1 > y)) && (x < (b.0 - a.0) * (y - a.1) / (b.1 - a.1) + a.0) { c = !c; } j = i; } c }
fn edge_dist(poly: &[P], x: f64, y: f64) -> f64 { let n = poly.len(); let mut d = f64::MAX; for i in 0..n { let (a, b) = (poly[i], poly[(i + 1) % n]); let (ex, ey) = (b.0 - a.0, b.1 - a.1); let l2 = (ex * ex + ey * ey).max(1e-18); let t = (((x - a.0) * ex + (y - a.1) * ey) / l2).clamp(0.0, 1.0); d = d.min((x - a.0 - t * ex).hypot(y - a.1 - t * ey)); } d }
/// Signed distance to the polygon outline (negative inside).
pub fn signed_dist(poly: &[P], x: f64, y: f64) -> f64 { let d = edge_dist(poly, x, y); if inside(poly, x, y) { -d } else { d } }
fn map_poly(h: &H3, poly: &[P]) -> Vec<P> { poly.iter().map(|p| trackmath::apply(h, *p).unwrap_or(*p)).collect() }

/// Catmull-Rom sample of a w*h image at pixel-unit position (x, y) with its analytic gradient. None near the border.
#[inline]
pub fn sample_cr(img: &[f32], w: usize, h: usize, x: f64, y: f64) -> Option<(f64, f64, f64)> {
    let (fx, fy) = (x - 0.5, y - 0.5); let (ix, iy) = (fx.floor(), fy.floor());
    if ix < 1.0 || iy < 1.0 || ix + 2.0 > (w - 1) as f64 || iy + 2.0 > (h - 1) as f64 { return None; }
    let (tx, ty) = (fx - ix, fy - iy); let (ix, iy) = (ix as usize, iy as usize);
    let wts = |t: f64| { let (t2, t3) = (t * t, t * t * t); ([0.5 * (-t3 + 2.0 * t2 - t), 0.5 * (3.0 * t3 - 5.0 * t2 + 2.0), 0.5 * (-3.0 * t3 + 4.0 * t2 + t), 0.5 * (t3 - t2)],
        [0.5 * (-3.0 * t2 + 4.0 * t - 1.0), 0.5 * (9.0 * t2 - 10.0 * t), 0.5 * (-9.0 * t2 + 8.0 * t + 1.0), 0.5 * (3.0 * t2 - 2.0 * t)]) };
    let ((wx, dwx), (wy, dwy)) = (wts(tx), wts(ty)); let (mut v, mut gx, mut gy) = (0.0, 0.0, 0.0);
    for j in 0..4 { let row = (iy + j - 1) * w; let (mut rv, mut rd) = (0.0, 0.0);
        for i in 0..4 { let p = img[row + ix + i - 1] as f64; rv += wx[i] * p; rd += dwx[i] * p; }
        v += wy[j] * rv; gx += wy[j] * rd; gy += dwy[j] * rv; }
    Some((v, gx, gy))
}

/// Parameterisation of a motion model in normalised coordinates u = (x - c) / s.
struct Param { m: Model, c: P, s: f64 }
impl Param {
    fn n(&self) -> usize { match self.m { Model::Translation => 2, Model::Similarity => 4, Model::Affine => 6, Model::Perspective => 8 } }
    fn nmat(&self) -> H3 { [[1.0 / self.s, 0.0, -self.c.0 / self.s], [0.0, 1.0 / self.s, -self.c.1 / self.s], [0.0, 0.0, 1.0]] }
    fn nimat(&self) -> H3 { [[self.s, 0.0, self.c.0], [0.0, self.s, self.c.1], [0.0, 0.0, 1.0]] }
    fn hn(&self, p: &[f64]) -> H3 { match self.m {
        Model::Translation => [[1.0, 0.0, p[0]], [0.0, 1.0, p[1]], [0.0, 0.0, 1.0]],
        Model::Similarity => [[1.0 + p[0], -p[1], p[2]], [p[1], 1.0 + p[0], p[3]], [0.0, 0.0, 1.0]],
        Model::Affine => [[1.0 + p[0], p[1], p[2]], [p[3], 1.0 + p[4], p[5]], [0.0, 0.0, 1.0]],
        Model::Perspective => [[1.0 + p[0], p[1], p[2]], [p[3], 1.0 + p[4], p[5]], [p[6], p[7], 1.0]] } }
    /// warped normalised point and d(w)/dp rows (x row, y row)
    fn warp_jac(&self, p: &[f64], u: P, jx: &mut [f64; 8], jy: &mut [f64; 8]) -> P {
        let (ux, uy) = u;
        match self.m {
            Model::Translation => { jx[0] = 1.0; jx[1] = 0.0; jy[0] = 0.0; jy[1] = 1.0; (ux + p[0], uy + p[1]) }
            Model::Similarity => { jx[..4].copy_from_slice(&[ux, -uy, 1.0, 0.0]); jy[..4].copy_from_slice(&[uy, ux, 0.0, 1.0]); ((1.0 + p[0]) * ux - p[1] * uy + p[2], p[1] * ux + (1.0 + p[0]) * uy + p[3]) }
            Model::Affine => { jx[..6].copy_from_slice(&[ux, uy, 1.0, 0.0, 0.0, 0.0]); jy[..6].copy_from_slice(&[0.0, 0.0, 0.0, ux, uy, 1.0]); ((1.0 + p[0]) * ux + p[1] * uy + p[2], p[3] * ux + (1.0 + p[4]) * uy + p[5]) }
            Model::Perspective => { let d = p[6] * ux + p[7] * uy + 1.0; let wx = ((1.0 + p[0]) * ux + p[1] * uy + p[2]) / d; let wy = (p[3] * ux + (1.0 + p[4]) * uy + p[5]) / d;
                jx.copy_from_slice(&[ux / d, uy / d, 1.0 / d, 0.0, 0.0, 0.0, -wx * ux / d, -wx * uy / d]); jy.copy_from_slice(&[0.0, 0.0, 0.0, ux / d, uy / d, 1.0 / d, -wy * ux / d, -wy * uy / d]); (wx, wy) }
        }
    }
}

/// Reference pixels of the region: pixel-space position, normalised coordinate, template value and template gradient
/// (central differences on the pre-smoothed reference; the pixel's own value is not in its gradient, so template noise does
/// not correlate with the residual).
struct RefSet { x: Vec<P>, u: Vec<P>, t: Vec<f64>, g: Vec<P>, corners: [P; 4] }

/// Separable Gaussian, sigma 1 px (radius 3): both the reference and every frame are smoothed the same way before the photometric
/// refinement (suppresses sensor noise and the interpolator's high-frequency error; isotropic, so it introduces no shift).
pub fn smooth1(img: &[f32], w: usize, h: usize) -> Vec<f32> {
    let k: Vec<f32> = (-3i32..=3).map(|i| (-(i * i) as f32 / 2.0).exp()).collect(); let ks: f32 = k.iter().sum(); let k: Vec<f32> = k.iter().map(|v| v / ks).collect();
    let mut t = vec![0f32; w * h]; for y in 0..h { for x in 0..w { let mut a = 0f32; for (j, kv) in k.iter().enumerate() { let xx = (x as i64 + j as i64 - 3).clamp(0, w as i64 - 1) as usize; a += kv * img[y * w + xx]; } t[y * w + x] = a; } }
    let mut o = vec![0f32; w * h]; for y in 0..h { for x in 0..w { let mut a = 0f32; for (j, kv) in k.iter().enumerate() { let yy = (y as i64 + j as i64 - 3).clamp(0, h as i64 - 1) as usize; a += kv * t[yy * w + x]; } o[y * w + x] = a; } }
    o
}

fn solve_sym(a: &mut Vec<Vec<f64>>, b: &mut Vec<f64>) -> bool { trackmath::solve_linear(a, b) }

/// Photometric refinement of `hp` (reference -> frame) against the smoothed frame `img`, inverse compositional with joint
/// gain/bias and Huber IRLS, Levenberg-Marquardt damped. Returns (H, rms, inlier pixels).
fn refine(par: &Param, rs: &RefSet, img: &[f32], w: usize, h: usize, hp: &H3) -> Option<(H3, f64, usize)> {
    let n = par.n(); let np = n + 2; let zero = [0f64; 8]; let mut jx = [0f64; 8]; let mut jy = [0f64; 8];
    // steepest-descent rows at the identity warp (inverse compositional: computed once, on the template)
    let sd: Vec<[f64; 8]> = (0..rs.u.len()).map(|i| { par.warp_jac(&zero[..n], rs.u[i], &mut jx, &mut jy); let mut r = [0f64; 8]; for j in 0..n { r[j] = par.s * (rs.g[i].0 * jx[j] + rs.g[i].1 * jy[j]); } r }).collect();
    let eval = |hh: &H3, ga: f64, gb: f64| -> (Vec<f64>, Vec<bool>) { let mut r = vec![0.0; rs.x.len()]; let mut ok = vec![false; rs.x.len()];
        for i in 0..rs.x.len() { if let Some(q) = trackmath::apply(hh, rs.x[i]) { if let Some((v, _, _)) = sample_cr(img, w, h, q.0, q.1) { r[i] = v - (ga * rs.t[i] + gb); ok[i] = true; } } } (r, ok) };
    let huber = |r: &[f64], ok: &[bool]| -> f64 { let mut a: Vec<f64> = r.iter().zip(ok).filter(|(_, o)| **o).map(|(x, _)| x.abs()).collect(); if a.is_empty() { return 1.0; }
        let m = a.len() / 2; a.select_nth_unstable_by(m, |x, y| x.partial_cmp(y).unwrap()); (1.4826 * a[m]).max(0.25) * 1.345 };
    let cost = |r: &[f64], ok: &[bool], k: f64| -> f64 { r.iter().zip(ok).filter(|(_, o)| **o).map(|(x, _)| { let a = x.abs(); if a <= k { 0.5 * a * a } else { k * (a - 0.5 * k) } }).sum::<f64>() };
    let corner_move = |h1: &H3, h2: &H3| -> f64 { rs.corners.iter().map(|c| { let a = trackmath::apply(h1, *c).unwrap_or(*c); let b = trackmath::apply(h2, *c).unwrap_or(*c); (a.0 - b.0).hypot(a.1 - b.1) }).fold(0.0, f64::max) };
    let (nm, nim) = (par.nmat(), par.nimat());
    let (mut hc, mut ga, mut gb) = (*hp, 1.0f64, 0.0f64); let (mut r, mut ok) = eval(&hc, ga, gb); let mut lambda = 1e-6;
    for _it in 0..60 {
        let k = huber(&r, &ok); let cur = cost(&r, &ok, k);
        let mut ata = vec![vec![0.0; np]; np]; let mut atb = vec![0.0; np]; let mut row = [0f64; 10];
        for i in 0..rs.x.len() { if !ok[i] { continue; }
            for j in 0..n { row[j] = ga * sd[i][j]; } row[n] = rs.t[i]; row[n + 1] = 1.0;
            let a = r[i].abs(); let wgt = if a <= k { 1.0 } else { k / a };
            for a_ in 0..np { let ra = wgt * row[a_]; atb[a_] += ra * r[i]; for b_ in a_..np { ata[a_][b_] += ra * row[b_]; } } }
        for a_ in 0..np { for b_ in 0..a_ { ata[a_][b_] = ata[b_][a_]; } }
        let mut accepted = false;
        for _try in 0..8 {
            let mut a2 = ata.clone(); for d in 0..np { a2[d][d] *= 1.0 + lambda; a2[d][d] += 1e-12; } let mut b2 = atb.clone();
            if !solve_sym(&mut a2, &mut b2) { lambda *= 10.0; continue; }
            let hd = trackmath::mul(&nim, &trackmath::mul(&par.hn(&b2[..n]), &nm)); let hdi = match trackmath::inv3(&hd) { Some(x) => x, None => { lambda *= 10.0; continue; } };
            let hn_ = match trackmath::norm_h(&trackmath::mul(&hc, &hdi)) { Some(x) => x, None => { lambda *= 10.0; continue; } }; let (gan, gbn) = (ga + b2[n], gb + b2[n + 1]);
            let (rn, okn) = eval(&hn_, gan, gbn); let cn = cost(&rn, &okn, k);
            if cn <= cur * (1.0 + 1e-12) { let mv = corner_move(&hc, &hn_); hc = hn_; ga = gan; gb = gbn; r = rn; ok = okn; lambda = (lambda * 0.3).max(1e-9); accepted = true;
                if mv < 1e-5 { let rms = rms_of(&r, &ok); let inl = r.iter().zip(&ok).filter(|(x, o)| **o && x.abs() <= k).count(); return Some((hc, rms, inl)); }
                break; }
            lambda *= 10.0; }
        if !accepted { break; }
    }
    let k = huber(&r, &ok); let rms = rms_of(&r, &ok); let inl = r.iter().zip(&ok).filter(|(x, o)| **o && x.abs() <= k).count();
    Some((hc, rms, inl))
}
fn rms_of(r: &[f64], ok: &[bool]) -> f64 { let (mut s, mut c) = (0.0, 0usize); for (x, o) in r.iter().zip(ok) { if *o { s += x * x; c += 1; } } if c == 0 { f64::INFINITY } else { (s / c as f64).sqrt() } }
/// RMS photometric residual of `hp` with the least-squares gain/bias (used to choose and to accept estimates).
fn rms_at(rs: &RefSet, img: &[f32], w: usize, h: usize, hp: &H3) -> f64 {
    let (mut st, mut sv, mut stt, mut stv) = (0.0, 0.0, 0.0, 0.0); let mut vals = vec![];
    for i in 0..rs.x.len() { if let Some(q) = trackmath::apply(hp, rs.x[i]) { if let Some((v, _, _)) = sample_cr(img, w, h, q.0, q.1) { vals.push((rs.t[i], v)); st += rs.t[i]; sv += v; stt += rs.t[i] * rs.t[i]; stv += rs.t[i] * v; } } }
    let nn = vals.len() as f64; if nn < 3.0 { return f64::INFINITY; } let a = (nn * stv - st * sv) / (nn * stt - st * st).max(1e-12); let b = (sv - a * st) / nn;
    let s: f64 = vals.iter().map(|(t, v)| (v - a * t - b).powi(2)).sum(); (s / nn).sqrt()
}

/// Track the region `poly` (pixel units, in frame `ids[0]`) through frames `ids`. `frames(i)` returns frame i as a w*h gray image
/// (0..255). Result h[k] maps reference-frame pixels to frame ids[k] pixels; h[0] is the identity.
pub fn track_region(frames: &mut dyn FnMut(usize) -> Result<Vec<f32>, String>, ids: &[usize], w: usize, h: usize, poly: &[P], model: Model, o: &Opts) -> Result<RegionTrack, String> {
    if ids.is_empty() { return Err("no frames to track".into()); } if poly.len() < 3 { return Err("mask outline has fewer than 3 points".into()); }
    let mut warnings = vec![]; let reference = frames(ids[0])?; if reference.len() != w * h { return Err("frame size mismatch".into()); }
    // region: pixel centres at signed distance <= expand - 2 (2 px clear of the outline, so the boundary's background is excluded)
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); for p in poly { x0 = x0.min(p.0); y0 = y0.min(p.1); x1 = x1.max(p.0); y1 = y1.max(p.1); }
    let grow = o.expand.max(0.0) + 1.0; let lim = o.expand - 2.0;
    let (bx0, by0, bx1, by1) = (((x0 - grow).floor().max(0.0)) as usize, ((y0 - grow).floor().max(0.0)) as usize, ((x1 + grow).ceil().min(w as f64)) as usize, ((y1 + grow).ceil().min(h as f64)) as usize);
    let rsm = smooth1(&reference, w, h); let mut pts: Vec<(P, f64)> = vec![];
    for y in by0..by1 { for x in bx0..bx1 { let (cx, cy) = (x as f64 + 0.5, y as f64 + 0.5); if x >= 1 && y >= 1 && x + 1 < w && y + 1 < h && signed_dist(poly, cx, cy) <= lim { pts.push(((cx, cy), rsm[y * w + x] as f64)); } } }
    if pts.len() < 64 { return Err(format!("tracked region has only {} pixels (mask too small or expand too negative)", pts.len())); }
    let nn = pts.len() as f64; let c = (pts.iter().map(|p| p.0 .0).sum::<f64>() / nn, pts.iter().map(|p| p.0 .1).sum::<f64>() / nn);
    let s = (pts.iter().map(|p| (p.0 .0 - c.0).powi(2) + (p.0 .1 - c.1).powi(2)).sum::<f64>() / nn).sqrt().max(1.0);
    let par = Param { m: model, c, s };
    let grad = |p: P| { let (x, y) = ((p.0 - 0.5) as usize, (p.1 - 0.5) as usize); (0.5 * (rsm[y * w + x + 1] - rsm[y * w + x - 1]) as f64, 0.5 * (rsm[(y + 1) * w + x] - rsm[(y - 1) * w + x]) as f64) };
    let rs = RefSet { x: pts.iter().map(|p| p.0).collect(), u: pts.iter().map(|p| ((p.0 .0 - c.0) / s, (p.0 .1 - c.1) / s)).collect(), t: pts.iter().map(|p| p.1).collect(), g: pts.iter().map(|p| grad(p.0)).collect(), corners: [(x0, y0), (x1, y0), (x1, y1), (x0, y1)] };
    drop(pts);
    // features
    let md = ((w.min(h)) as f32 / 80.0).max(4.0); let feat_lim = o.expand - 4.0;
    let detect_in = |img: &[f32], region: &[P], avoid: &[P]| -> Vec<P> { let av: Vec<P> = avoid.iter().map(|p| (p.0 - 0.5, p.1 - 0.5)).collect();
        crate::sfm::detect(img, w, h, o.max_features * 4, md, &av).into_iter().map(|p| (p.0 + 0.5, p.1 + 0.5)).filter(|p| signed_dist(region, p.0, p.1) <= feat_lim).take(o.max_features).collect() };
    let mut live: Vec<P> = detect_in(&reference, poly, &[]); let n0 = live.len().max(1);
    let mut prev_pyr = crate::sfm::pyramid(&reference, w, h, 4);
    let mut hs: Vec<H3> = vec![IDENTITY]; let mut stats = vec![FrameStat { frame: ids[0], features: live.len(), inliers: live.len(), rms: 0.0, refined: false }];
    let mut rng = trackmath::Xs(o.seed | 1); let mp = model.min_points();
    for k in 1..ids.len() {
        let img = frames(ids[k])?; if img.len() != w * h { return Err("frame size mismatch".into()); } let pyr = crate::sfm::pyramid(&img, w, h, 4);
        let hprev = hs[k - 1]; let pred = if k >= 2 { trackmath::inv3(&hs[k - 2]).map(|i2| trackmath::mul(&hprev, &trackmath::mul(&i2, &hprev))).unwrap_or(hprev) } else { hprev };
        let idx: Vec<P> = live.iter().map(|p| (p.0 - 0.5, p.1 - 0.5)).collect(); let tr = crate::sfm::klt(&prev_pyr, &pyr, &idx, 7);
        let (mut src, mut dst) = (vec![], vec![]); for (i, t) in tr.iter().enumerate() { if let Some(q) = t { src.push(live[i]); dst.push((q.0 + 0.5, q.1 + 0.5)); } }
        if o.inject_outlier_frac > 0.0 { let m = ((dst.len() as f64) * o.inject_outlier_frac).round() as usize; for _ in 0..m { let i = rng.below(dst.len().max(1)); if i < dst.len() { dst[i] = ((rng.next() % (w as u64 * 1000)) as f64 / 1000.0, (rng.next() % (h as u64 * 1000)) as f64 / 1000.0); } } }
        let mut init = None; let mut n_in = 0;
        if src.len() >= mp { if let Some(rr) = robust_est::ransac(model, &src, &dst, o.ransac_px, 0.995, 2000, o.seed ^ (k as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)) {
            if rr.n_inliers >= mp.max(4).min(src.len()) { init = Some(trackmath::mul(&rr.model, &hprev)); n_in = rr.n_inliers; live = (0..dst.len()).filter(|&i| rr.inliers[i]).map(|i| dst[i]).collect(); } } }
        let mut hk = match init { Some(hh) => hh, None => { warnings.push(format!("frame {}: only {} trackable features, used the motion prediction", ids[k], src.len())); live = dst.clone(); pred } };
        let mut st = FrameStat { frame: ids[k], features: src.len(), inliers: n_in, rms: 0.0, refined: false };
        if o.refine {
            // start from whichever of the RANSAC estimate and the constant-velocity prediction fits the reference better
            let ism = smooth1(&img, w, h); let (ra, rb) = (rms_at(&rs, &ism, w, h, &hk), rms_at(&rs, &ism, w, h, &pred)); let start = if rb < ra { pred } else { hk };
            match refine(&par, &rs, &ism, w, h, &start) {
                Some((hr, _rms, _inl)) if { let rr = rms_at(&rs, &ism, w, h, &hr); rr.is_finite() && rr <= ra.min(rb) * 1.0001 + 1e-9 } => { st.rms = rms_at(&rs, &ism, w, h, &hr); hk = hr; st.refined = true; }
                _ => { st.rms = ra; warnings.push(format!("frame {}: photometric refinement did not improve the fit, kept the feature estimate", ids[k])); }
            }
        } else { st.rms = rms_at(&rs, &smooth1(&img, w, h), w, h, &hk); }
        if live.len() < ((n0 as f64 * 0.4) as usize).max(4 * mp) { let region = map_poly(&hk, poly); let add = detect_in(&img, &region, &live); for p in add { if live.len() >= o.max_features { break; } live.push(p); } }
        hs.push(hk); stats.push(st); prev_pyr = pyr;
    }
    Ok(RegionTrack { h: hs, stats, warnings })
}

/// Apply a pixel-space transform to a path (normalised points with offset tangents) at canvas size w x h. Per-point feather is
/// scaled by the local area scale sqrt(|det J|).
pub fn apply_to_path(pts: &[PathPt], hp: &H3, w: usize, h: usize) -> Vec<PathPt> {
    let (wf, hf) = (w as f64, h as f64); let ap = |x: f64, y: f64| trackmath::apply(hp, (x, y)).unwrap_or((x, y));
    pts.iter().map(|p| { let v = (p.x * wf, p.y * hf); let nv = ap(v.0, v.1); let ni = ap(v.0 + p.ix * wf, v.1 + p.iy * hf); let no = ap(v.0 + p.ox * wf, v.1 + p.oy * hf);
        let d = hp[2][0] * v.0 + hp[2][1] * v.1 + hp[2][2]; let j = [[(hp[0][0] - nv.0 * hp[2][0]) / d, (hp[0][1] - nv.0 * hp[2][1]) / d], [(hp[1][0] - nv.1 * hp[2][0]) / d, (hp[1][1] - nv.1 * hp[2][1]) / d]];
        let sc = (j[0][0] * j[1][1] - j[0][1] * j[1][0]).abs().sqrt();
        PathPt { x: nv.0 / wf, y: nv.1 / hf, ix: (ni.0 - nv.0) / wf, iy: (ni.1 - nv.1) / hf, ox: (no.0 - nv.0) / wf, oy: (no.1 - nv.1) / hf, f: if p.f > 0.0 { p.f * sc } else { p.f }, fi: p.fi * sc, fo: p.fo * sc } }).collect()
}

/// Tool body: track `layer` on media clip `clip_id` from clip-local `t0` to `t1` (backwards when t1 < t0). Returns new path keys
/// replacing the keys inside [min, max] and the report JSON.
pub fn track_mask(p: &crate::model::Project, clip_id: &str, layer: &crate::masks::MaskLayer, t0: f64, t1: f64, model: Model, o: &Opts) -> Result<(Vec<crate::masks::PathKey>, serde_json::Value), String> {
    let started = std::time::Instant::now();
    let c = p.find_clip(clip_id).ok_or(format!("no clip '{clip_id}'"))?.clone(); let (w, h) = (p.width as usize, p.height as usize); let fps = p.fps;
    let nfr = ((c.duration * fps).ceil() as usize).max(1); let fi = |t: f64| ((t * fps).round().max(0.0) as usize).min(nfr - 1);
    let (f0, f1) = (fi(t0), fi(t1)); let ids: Vec<usize> = if f1 >= f0 { (f0..=f1).collect() } else { (f1..=f0).rev().collect() };
    let base = crate::masks::path_at(layer, f0 as f64 / fps); if base.len() < 3 { return Err("path mask needs at least 3 points".into()); }
    let poly: Vec<P> = crate::masks::flatten_pub(&base, layer.closed, w, h).into_iter().map(|q| (q.0, q.1)).collect();
    let mut cache = crate::render::Cache::new();
    let mut frames = |i: usize| -> Result<Vec<f32>, String> { crate::tracking::clip_gray_pub(p, &mut cache, &c, i) };
    let tr = track_region(&mut frames, &ids, w, h, &poly, model, o)?;
    let keys: Vec<crate::masks::PathKey> = ids.iter().zip(&tr.h).map(|(&f, hk)| crate::masks::PathKey { t: f as f64 / fps, points: apply_to_path(&base, hk, w, h), hold: false }).collect();
    let inl: Vec<usize> = tr.stats.iter().skip(1).map(|s| s.inliers).collect(); let rms: Vec<f64> = tr.stats.iter().skip(1).map(|s| s.rms).collect();
    let rep = serde_json::json!({"keys_written": keys.len(), "frames": [f0, f1], "motion": model.name(), "mean_inliers": if inl.is_empty() { 0.0 } else { inl.iter().sum::<usize>() as f64 / inl.len() as f64 },
        "min_inliers": inl.iter().min().copied().unwrap_or(0), "mean_rms": if rms.is_empty() { 0.0 } else { rms.iter().sum::<f64>() / rms.len() as f64 }, "refined_frames": tr.stats.iter().filter(|s| s.refined).count(),
        "warnings": tr.warnings, "secs": started.elapsed().as_secs_f64()});
    Ok((keys, rep))
}
