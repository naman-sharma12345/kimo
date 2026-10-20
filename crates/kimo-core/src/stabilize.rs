//! Warp stabilizer (After Effects / Premiere Warp Stabilizer, DaVinci Resolve stabilizer): a drift-free robust camera path,
//! path smoothing (Gaussian or L1-optimal) or a locked camera, and framing (stabilize only, crop, crop + auto-scale).
//!
//! Two passes, the overall structure of OpenCV videostab (Apache-2.0; ideas only, no code ported from it):
//! 1. `estimate_path`: camera path C_t (frame-0 pixels -> frame-t pixels). Consecutive frames give a prediction (Shi-Tomasi
//!    features, pyramidal KLT with a forward-backward check, RANSAC from `robust_est` (an OpenCV port, see that file's notice),
//!    Huber IRLS refit). The prediction then seeds a direct registration of a reference frame r to frame t (KLT pre-warped by the
//!    predicted motion, bicubic sampling at the finest level), so C_t = A_rt C_r carries no frame-to-frame drift; the reference
//!    moves on (re-anchors) when fewer than half of the inliers of its first fit survive.
//! 2. `smooth_path` + `corrections` + `frame_corrections`: the virtual camera S_t, the per-frame correction B_t = S_t C_t^-1 and
//!    the crop / auto-scale that hides the borders.
//!
//! References: M. Grundmann, V. Kwatra, I. Essa, "Auto-Directed Video Stabilization with Robust L1 Optimal Camera Paths", CVPR 2011
//! (the L1 objective w1|D1 p| + w2|D2 p| + w3|D3 p| inside a crop box); A. Chambolle, T. Pock, "A first-order primal-dual algorithm
//! for convex problems with applications to imaging", JMIV 2011 (the solver used for it).
//!
//! Coordinates: every H3 here is in pixel-index coordinates (pixel (i, j) at (i, j)), the convention of the gray frames and of
//! render.rs transform_bb, whose x/y/scale/rotation props rotate and scale about (w/2, h/2). Mask paths and pin quads use pixel-edge
//! coordinates (index + 0.5); `stabilize_clip` converts.
//!
//! Deliberate departures from the textbook forms (both keep a constant-velocity pan exact):
//! - Gaussian boundary: odd reflection x[-k] = 2a - x[k] about a = the end value of a least-squares line through the first (last)
//!   3 sigma samples, not about the raw end sample, so the jitter of frame 0 is not frozen into the first sigma frames of the path.
//!   For a jitter-free path a == x[0].
//! - L1: the objective is minimised on the series minus its least-squares line (added back afterwards). With w1 = 10 the plain
//!   form shortens any monotone pan by the whole box (|D1 p|_1 only sees the end points), i.e. it slows deliberate pans;
//!   detrended, D1 penalises departures from the shot's mean velocity, so a static shot still stays static.
use crate::keyframe::{Ease, Prop};
use crate::model::{Mask, Project, Source};
use crate::robust_est::{self, Model};
use crate::trackmath::{self, H3, IDENT, P};
use serde_json::{json, Value};
use std::rc::Rc;

pub const METHODS: [&str; 3] = ["position", "position_scale_rotation", "perspective"];
pub const MODES: [&str; 2] = ["smooth_motion", "no_motion"];
pub const SMOOTHINGS: [&str; 2] = ["gaussian", "l1"];
pub const FRAMINGS: [&str; 3] = ["stabilize_only", "crop", "crop_auto_scale"];

/// AE "Method": what the correction may contain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Method { Position, Similarity, Perspective }
/// AE "Result": smooth the camera move, or no motion (lock frame 0's framing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Mode { Smooth, Locked }
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Smoothing { Gaussian, L1 }
/// AE "Framing".
#[derive(Clone, Copy, Debug, PartialEq, Eq)] pub enum Framing { StabilizeOnly, Crop, CropAutoScale }
impl Method {
    pub fn parse(s: &str) -> Result<Self, String> { match s { "position" => Ok(Method::Position), "position_scale_rotation" | "similarity" => Ok(Method::Similarity), "perspective" => Ok(Method::Perspective), _ => Err(format!("unknown method '{s}' (valid: {})", METHODS.join(", "))) } }
    pub fn name(self) -> &'static str { match self { Method::Position => METHODS[0], Method::Similarity => METHODS[1], Method::Perspective => METHODS[2] } }
    fn model(self) -> Model { if self == Method::Perspective { Model::Perspective } else { Model::Similarity } }
}
impl Mode {
    pub fn parse(s: &str) -> Result<Self, String> { match s { "smooth_motion" => Ok(Mode::Smooth), "no_motion" => Ok(Mode::Locked), _ => Err(format!("unknown mode '{s}' (valid: {})", MODES.join(", "))) } }
    pub fn name(self) -> &'static str { match self { Mode::Smooth => MODES[0], Mode::Locked => MODES[1] } }
}
impl Smoothing {
    pub fn parse(s: &str) -> Result<Self, String> { match s { "gaussian" => Ok(Smoothing::Gaussian), "l1" => Ok(Smoothing::L1), _ => Err(format!("unknown smoothing '{s}' (valid: {})", SMOOTHINGS.join(", "))) } }
    pub fn name(self) -> &'static str { match self { Smoothing::Gaussian => SMOOTHINGS[0], Smoothing::L1 => SMOOTHINGS[1] } }
}
impl Framing {
    pub fn parse(s: &str) -> Result<Self, String> { match s { "stabilize_only" => Ok(Framing::StabilizeOnly), "crop" => Ok(Framing::Crop), "crop_auto_scale" => Ok(Framing::CropAutoScale), _ => Err(format!("unknown framing '{s}' (valid: {})", FRAMINGS.join(", "))) } }
    pub fn name(self) -> &'static str { match self { Framing::StabilizeOnly => FRAMINGS[0], Framing::Crop => FRAMINGS[1], Framing::CropAutoScale => FRAMINGS[2] } }
}

/// smoothness: 0..100 %; Gaussian sigma = smoothness% of one second of frames, L1 box = smoothness% of 10% of min(w, h).
#[derive(Clone, Debug)]
pub struct Opts { pub method: Method, pub mode: Mode, pub smoothing: Smoothing, pub smoothness: f64, pub framing: Framing, pub max_features: usize, pub ransac_px: f64, pub seed: u64, pub fps: f64 }
impl Default for Opts { fn default() -> Self { Opts { method: Method::Similarity, mode: Mode::Smooth, smoothing: Smoothing::Gaussian, smoothness: 50.0, framing: Framing::CropAutoScale, max_features: 400, ransac_px: 1.0, seed: 0x5eed, fps: 30.0 } } }

/// path[t] = C_t maps frame-0 pixels to frame-t pixels (C_0 = identity). Per frame: inlier fraction and number of tracked reference
/// features of the fit that produced C_t, and the RMS residual (px) of its inliers.
pub struct PathEstimate { pub path: Vec<H3>, pub inlier_frac: Vec<f64>, pub n_features: Vec<usize>, pub reanchors: usize, pub fit_rms_px: Vec<f64> }
/// Per-frame correction (frame-t pixels -> output pixels), the auto-scale zoom, and the crop rect [x0, y0, x1, y1] in output pixels.
pub struct Framed { pub corr: Vec<H3>, pub zoom: f64, pub crop: Option<[f64; 4]> }
/// One frame's keys: clip-local time (i / fps), prop values, and the crop mask quad in normalised source coordinates.
pub struct KeyRow { pub t: f64, pub props: Vec<(&'static str, f64)>, pub crop_quad: Option<[(f64, f64); 4]> }

// ---------- image pyramid and sub-pixel sampling ----------
struct Lvl { d: Vec<f32>, w: usize, h: usize }
struct Pyr(Vec<Lvl>);
/// Level k+1 = level k blurred by the 5-tap binomial and decimated at even pixels, so level-k pixel i sits at level-0 pixel i * 2^k.
fn pyr(img: &[f32], w: usize, h: usize, n: usize) -> Pyr {
    let mut v = vec![Lvl { d: img.to_vec(), w, h }];
    while v.len() < n { let nl = { let l = v.last().unwrap(); if l.w < 32 || l.h < 32 { break; } let b = trackmath::blur5(&l.d, l.w, l.h); let (nw, nh) = ((l.w + 1) / 2, (l.h + 1) / 2);
            let mut o = vec![0f32; nw * nh]; for y in 0..nh { for x in 0..nw { o[y * nw + x] = b[2 * y * l.w + 2 * x]; } } Lvl { d: o, w: nw, h: nh } }; v.push(nl); }
    Pyr(v)
}
/// Keys cubic convolution weights (a = -0.5) for the samples at -1, 0, 1, 2 around fraction t.
#[inline] fn keys(t: f64) -> [f64; 4] { let a = -0.5; let (t2, t3) = (t * t, t * t * t); [a * (t3 - 2.0 * t2 + t), (a + 2.0) * t3 - (a + 3.0) * t2 + 1.0, -(a + 2.0) * t3 + (2.0 * a + 3.0) * t2 - a * t, -a * (t3 - t2)] }
/// Bilinear or bicubic sample; the caller keeps (x, y) inside [1, w-3] x [1, h-3].
#[inline] fn samp(l: &Lvl, x: f64, y: f64, cubic: bool) -> f64 {
    let (x0, y0) = (x.floor(), y.floor()); let (fx, fy) = (x - x0, y - y0); let (i, j) = (x0 as usize, y0 as usize);
    if cubic { let (wx, wy) = (keys(fx), keys(fy)); let mut s = 0.0; for r in 0..4 { let k = (j + r - 1) * l.w + i - 1; let d = &l.d[k..k + 4]; s += wy[r] * (wx[0] * d[0] as f64 + wx[1] * d[1] as f64 + wx[2] * d[2] as f64 + wx[3] * d[3] as f64); } s }
    else { let k = j * l.w + i; let d = &l.d; (d[k] as f64 * (1.0 - fx) + d[k + 1] as f64 * fx) * (1.0 - fy) + (d[k + l.w] as f64 * (1.0 - fx) + d[k + l.w + 1] as f64 * fx) * fy }
}
#[inline] fn inside(l: &Lvl, x: f64, y: f64, r: f64) -> bool { x - r >= 1.0 && y - r >= 1.0 && x + r <= l.w as f64 - 3.0 && y + r <= l.h as f64 - 3.0 }
const I2: [[f64; 2]; 2] = [[1.0, 0.0], [0.0, 1.0]];

/// Pyramidal Lucas-Kanade (translation) with an affine pre-warp: finds c with I(c + J w) ~ T(pt + w) over the window w, starting at
/// `guess` (level-0 pixels in I). J is the local Jacobian of the predicted motion (identity for consecutive frames), so a rotated or
/// zoomed reference still matches. The finest level samples bicubically and iterates to 1e-5 px. Levels whose window does not fit are
/// skipped (the guess carries over); returns None when the finest level does not fit, the window is flat or the target leaves the image.
fn lk(tp: &Pyr, ip: &Pyr, pt: P, jac: [[f64; 2]; 2], guess: P, levels: usize, win: i32) -> Option<P> {
    let nl = tp.0.len().min(ip.0.len()).min(levels.max(1));
    let det = jac[0][0] * jac[1][1] - jac[0][1] * jac[1][0]; if !(det.abs() > 1e-6) { return None; }
    let jit = [[jac[1][1] / det, -jac[1][0] / det], [-jac[0][1] / det, jac[0][0] / det]]; // J^-T: grad I = J^-T grad T
    let reach = win as f64 * (jac[0][0].abs() + jac[0][1].abs()).max(jac[1][0].abs() + jac[1][1].abs()) + 1.0;
    let nw = ((2 * win + 1) * (2 * win + 1)) as usize; let mut tv = vec![0f64; nw]; let mut gv = vec![(0f64, 0f64); nw]; let mut off = vec![(0f64, 0f64); nw];
    let mut c = guess;
    for l in (0..nl).rev() {
        let s = (1u64 << l) as f64; let (tl, il) = (&tp.0[l], &ip.0[l]); let cubic = l == 0;
        let (px, py) = (pt.0 / s, pt.1 / s); let (mut cx, mut cy) = (c.0 / s, c.1 / s);
        if !inside(tl, px, py, win as f64 + 1.0) || !inside(il, cx, cy, reach) { if l == 0 { return None; } continue; }
        let (mut a, mut b, mut cc) = (0.0, 0.0, 0.0); let mut k = 0;
        for dy in -win..=win { for dx in -win..=win { let (x, y) = (px + dx as f64, py + dy as f64);
            let gx = 0.5 * (samp(tl, x + 1.0, y, cubic) - samp(tl, x - 1.0, y, cubic)); let gy = 0.5 * (samp(tl, x, y + 1.0, cubic) - samp(tl, x, y - 1.0, cubic));
            let g = (jit[0][0] * gx + jit[0][1] * gy, jit[1][0] * gx + jit[1][1] * gy);
            tv[k] = samp(tl, x, y, cubic); gv[k] = g; off[k] = (jac[0][0] * dx as f64 + jac[0][1] * dy as f64, jac[1][0] * dx as f64 + jac[1][1] * dy as f64);
            a += g.0 * g.0; b += g.0 * g.1; cc += g.1 * g.1; k += 1; } }
        let hd = a * cc - b * b; let mineig = 0.5 * (a + cc - ((a - cc) * (a - cc) + 4.0 * b * b).sqrt());
        if !(hd > 0.0) || !(mineig > 0.05 * nw as f64) { if l == 0 { return None; } continue; }
        let (iters, eps) = if l == 0 { (40, 1e-5) } else { (15, 1e-3) };
        for _ in 0..iters {
            if !inside(il, cx, cy, reach) { if l == 0 { return None; } break; }
            let (mut ex, mut ey) = (0.0, 0.0);
            for k in 0..nw { let d = samp(il, cx + off[k].0, cy + off[k].1, cubic) - tv[k]; ex += d * gv[k].0; ey += d * gv[k].1; }
            let (sx, sy) = (-(cc * ex - b * ey) / hd, -(-b * ex + a * ey) / hd); cx += sx; cy += sy;
            if sx * sx + sy * sy < eps * eps { break; }
        }
        if !inside(il, cx, cy, reach) { if l == 0 { return None; } continue; }
        c = (cx * s, cy * s);
    }
    if c.0.is_finite() && c.1.is_finite() { Some(c) } else { None }
}

// ---------- robust model fitting ----------
fn hartley(pts: &[P], wt: &[f64]) -> H3 {
    let sw: f64 = wt.iter().sum::<f64>().max(1e-300); let (cx, cy) = (pts.iter().zip(wt).map(|(p, w)| p.0 * w).sum::<f64>() / sw, pts.iter().zip(wt).map(|(p, w)| p.1 * w).sum::<f64>() / sw);
    let md = pts.iter().zip(wt).map(|(p, w)| w * ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()).sum::<f64>() / sw; let s = if md > 1e-12 { 2f64.sqrt() / md } else { 1.0 };
    [[s, 0.0, -s * cx], [0.0, s, -s * cy], [0.0, 0.0, 1.0]]
}
/// Weighted similarity least squares, x' = a x - b y + tx, y' = b x + a y + ty (closed form about the weighted centroids).
fn sim_wls(s: &[P], d: &[P], wt: &[f64]) -> Option<H3> {
    let sw: f64 = wt.iter().sum(); if !(sw > 0.0) { return None; }
    let (mut sx, mut sy, mut dx, mut dy) = (0.0, 0.0, 0.0, 0.0); for i in 0..s.len() { sx += wt[i] * s[i].0; sy += wt[i] * s[i].1; dx += wt[i] * d[i].0; dy += wt[i] * d[i].1; } sx /= sw; sy /= sw; dx /= sw; dy /= sw;
    let (mut na, mut nb, mut den) = (0.0, 0.0, 0.0);
    for i in 0..s.len() { let (px, py, qx, qy) = (s[i].0 - sx, s[i].1 - sy, d[i].0 - dx, d[i].1 - dy); na += wt[i] * (px * qx + py * qy); nb += wt[i] * (px * qy - py * qx); den += wt[i] * (px * px + py * py); }
    if !(den > 1e-12) { return None; } let (a, b) = (na / den, nb / den);
    Some([[a, -b, dx - (a * sx - b * sy)], [b, a, dy - (b * sx + a * sy)], [0.0, 0.0, 1.0]])
}
/// Weighted Hartley-normalised DLT (h22 = 1).
fn dlt_w(s: &[P], d: &[P], wt: &[f64]) -> Option<H3> {
    if s.len() < 4 { return None; } let (t1, t2) = (hartley(s, wt), hartley(d, wt)); let mut ata = vec![vec![0.0; 8]; 8]; let mut atb = vec![0.0; 8];
    for i in 0..s.len() { let (x, y) = trackmath::apply(&t1, s[i])?; let (u, v) = trackmath::apply(&t2, d[i])?; let w = wt[i];
        for (r, t) in [([x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u], u), ([0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v], v)] { for a in 0..8 { atb[a] += w * r[a] * t; for b in 0..8 { ata[a][b] += w * r[a] * r[b]; } } } }
    if !trackmath::solve_linear(&mut ata, &mut atb) { return None; }
    let hn = [[atb[0], atb[1], atb[2]], [atb[3], atb[4], atb[5]], [atb[6], atb[7], 1.0]]; trackmath::norm_h(&trackmath::mul(&trackmath::mul(&trackmath::inv3(&t2)?, &hn), &t1))
}
/// Weighted Gauss-Newton on the 8 homography parameters (geometric error), in centred coordinates scaled to ~[-1, 1].
fn homog_gn(s: &[P], d: &[P], wt: &[f64], h0: &H3, w: usize, h: usize, steps: usize) -> Option<H3> {
    let sc = (w.max(h) as f64 * 0.5).max(1.0); let (cx, cy) = (w as f64 * 0.5, h as f64 * 0.5);
    let nm: H3 = [[1.0 / sc, 0.0, -cx / sc], [0.0, 1.0 / sc, -cy / sc], [0.0, 0.0, 1.0]]; let ni = trackmath::inv3(&nm)?;
    let hn = trackmath::norm_h(&trackmath::mul(&trackmath::mul(&nm, h0), &ni))?; let mut x = [hn[0][0], hn[0][1], hn[0][2], hn[1][0], hn[1][1], hn[1][2], hn[2][0], hn[2][1]];
    let sn: Vec<P> = s.iter().map(|p| ((p.0 - cx) / sc, (p.1 - cy) / sc)).collect(); let dn: Vec<P> = d.iter().map(|p| ((p.0 - cx) / sc, (p.1 - cy) / sc)).collect();
    for _ in 0..steps {
        let mut ata = vec![vec![0.0; 8]; 8]; let mut atb = vec![0.0; 8];
        for i in 0..sn.len() { let p = sn[i]; let den = x[6] * p.0 + x[7] * p.1 + 1.0; if !(den.abs() > 1e-9) { return None; }
            let (u, v) = ((x[0] * p.0 + x[1] * p.1 + x[2]) / den, (x[3] * p.0 + x[4] * p.1 + x[5]) / den);
            let ju = [p.0 / den, p.1 / den, 1.0 / den, 0.0, 0.0, 0.0, -u * p.0 / den, -u * p.1 / den]; let jv = [0.0, 0.0, 0.0, p.0 / den, p.1 / den, 1.0 / den, -v * p.0 / den, -v * p.1 / den];
            for (jj, r) in [(ju, u - dn[i].0), (jv, v - dn[i].1)] { for a in 0..8 { atb[a] -= wt[i] * jj[a] * r; for b in 0..8 { ata[a][b] += wt[i] * jj[a] * jj[b]; } } } }
        for a in 0..8 { ata[a][a] *= 1.0 + 1e-12; ata[a][a] += 1e-18; }
        if !trackmath::solve_linear(&mut ata, &mut atb) { break; }
        let mut st = 0.0; for a in 0..8 { x[a] += atb[a]; st += atb[a] * atb[a]; } if st.sqrt() < 1e-13 { break; }
    }
    let hn: H3 = [[x[0], x[1], x[2]], [x[3], x[4], x[5]], [x[6], x[7], 1.0]]; trackmath::norm_h(&trackmath::mul(&trackmath::mul(&ni, &hn), &nm))
}
/// 6 IRLS iterations with Huber weights, k = 1.345 * 1.4826 * MAD (median absolute residual, floor 0.05 px).
fn irls(m: Model, s: &[P], d: &[P], h0: H3, w: usize, h: usize) -> H3 {
    let n = s.len(); let mut hm = h0;
    for _ in 0..6 {
        let r: Vec<f64> = (0..n).map(|i| robust_est::sq_err(&hm, s[i], d[i]).sqrt()).collect(); let mut sr: Vec<f64> = r.iter().cloned().filter(|v| v.is_finite()).collect(); if sr.is_empty() { break; }
        sr.sort_by(|a, b| a.partial_cmp(b).unwrap()); let k = (1.345 * 1.4826 * sr[sr.len() / 2]).max(0.05);
        let wt: Vec<f64> = r.iter().map(|&e| if !e.is_finite() { 0.0 } else if e <= k { 1.0 } else { k / e }).collect();
        let nh = match m { Model::Perspective => dlt_w(s, d, &wt).and_then(|g| homog_gn(s, d, &wt, &g, w, h, 10)).or_else(|| homog_gn(s, d, &wt, &hm, w, h, 10)), _ => sim_wls(s, d, &wt) };
        match nh { Some(x) => hm = x, None => break }
    }
    hm
}
struct Fit { h: H3, n_inl: usize, rms: f64 }
fn min_pts(m: Model) -> usize { if m == Model::Perspective { 10 } else { 6 } }
/// RANSAC (OpenCV port) then Huber IRLS on its inliers.
fn fit(m: Model, s: &[P], d: &[P], o: &Opts, seed: u64, w: usize, h: usize) -> Option<Fit> {
    if s.len() < min_pts(m) { return None; }
    let rr = robust_est::ransac(m, s, d, o.ransac_px, 0.999, 2000, seed)?;
    let (si, di): (Vec<P>, Vec<P>) = (0..s.len()).filter(|&i| rr.inliers[i]).map(|i| (s[i], d[i])).unzip(); if si.len() < min_pts(m) { return None; }
    let hm = trackmath::norm_h(&irls(m, &si, &di, rr.model, w, h))?;
    let rms = (si.iter().zip(&di).map(|(a, b)| robust_est::sq_err(&hm, *a, *b)).sum::<f64>() / si.len() as f64).sqrt();
    if !rms.is_finite() { return None; }
    Some(Fit { h: hm, n_inl: si.len(), rms })
}
/// Shi-Tomasi features at least 16 px from the borders, 7 px apart, strongest first.
fn features(img: &[f32], w: usize, h: usize, max: usize) -> Vec<P> {
    let b = 16.0; sfm_detect(img, w, h, max * 2).into_iter().filter(|p| p.0 >= b && p.1 >= b && p.0 <= w as f64 - 1.0 - b && p.1 <= h as f64 - 1.0 - b).take(max).collect()
}
fn sfm_detect(img: &[f32], w: usize, h: usize, max: usize) -> Vec<P> { crate::sfm::detect(img, w, h, max, 7.0, &[]) }
fn nh(h: &H3) -> H3 { trackmath::norm_h(h).unwrap_or(*h) }
fn translation(t: P) -> H3 { [[1.0, 0.0, t.0], [0.0, 1.0, t.1], [0.0, 0.0, 1.0]] }

/// Camera path estimate (see the module doc). `frames(i)` returns frame i as w*h gray values 0..255; frames are requested in order 0..n.
/// Position and Similarity estimate similarities (Position then keeps only the frame centre's translation); Perspective homographies.
pub fn estimate_path(frames: &mut dyn FnMut(usize) -> Result<Vec<f32>, String>, n: usize, w: usize, h: usize, method: Method, o: &Opts) -> Result<PathEstimate, String> {
    if n == 0 { return Err("stabilize: no frames".into()); } if w < 64 || h < 64 { return Err(format!("stabilize: frame {w}x{h} is too small (min 64x64)")); }
    let (m, minp, levels, win) = (method.model(), min_pts(method.model()), 4usize, 7i32); let maxf = o.max_features.max(8);
    let f0 = frames(0)?; if f0.len() != w * h { return Err(format!("stabilize: frame 0 has {} values, expected {}", f0.len(), w * h)); }
    let mut prev = Rc::new(pyr(&f0, w, h, levels)); let mut prev_feats = features(&f0, w, h, maxf);
    let (mut ref_pyr, mut ref_feats, mut c_r) = (prev.clone(), prev_feats.clone(), IDENT); let mut ref_base: Option<usize> = None; // inliers of the first fit against this reference
    let mut est = PathEstimate { path: vec![IDENT], inlier_frac: vec![1.0], n_features: vec![ref_feats.len()], reanchors: 0, fit_rms_px: vec![0.0] };
    for t in 1..n {
        crate::guard::check();
        let img = frames(t)?; if img.len() != w * h { return Err(format!("stabilize: frame {t} has {} values, expected {}", img.len(), w * h)); }
        let cur = Rc::new(pyr(&img, w, h, levels));
        // (a) consecutive frames: KLT t-1 -> t and back, RANSAC + IRLS -> M_t, prediction C_t ~ M_t C_{t-1}
        let (mut s1, mut d1) = (vec![], vec![]);
        for &x in &prev_feats { let Some(y) = lk(&prev, &cur, x, I2, x, levels, win) else { continue }; let Some(xb) = lk(&cur, &prev, y, I2, y, levels, win) else { continue };
            if trackmath::dist(xb, x) <= 0.5 { s1.push(x); d1.push(y); } }
        let pair = fit(m, &s1, &d1, o, o.seed.wrapping_add(2 * t as u64), w, h);
        let c_prev = est.path[t - 1]; let pred = pair.as_ref().map(|f| nh(&trackmath::mul(&f.h, &c_prev))).unwrap_or(c_prev);
        // (b) drift-free: reference features tracked straight from frame r into frame t, KLT seeded and pre-warped by the prediction
        let (mut s2, mut d2) = (vec![], vec![]);
        if let Some(pr) = trackmath::inv3(&c_r).map(|ci| nh(&trackmath::mul(&pred, &ci))) { let pi = trackmath::inv3(&pr);
            for &x in &ref_feats { let (Some(yp), Some(j)) = (trackmath::apply(&pr, x), trackmath::jacobian(&pr, x)) else { continue };
                let Some(y) = lk(&ref_pyr, &cur, x, j, yp, 2, win) else { continue }; let Some(pi) = pi.as_ref() else { continue };
                let (Some(xp), Some(jb)) = (trackmath::apply(pi, y), trackmath::jacobian(pi, y)) else { continue }; let Some(xb) = lk(&cur, &ref_pyr, y, jb, xp, 2, win) else { continue };
                if trackmath::dist(xb, x) <= 0.5 { s2.push(x); d2.push(y); } } }
        let anch = fit(m, &s2, &d2, o, o.seed.wrapping_add(2 * t as u64 + 1), w, h);
        let (ct, survivors) = match (&anch, &pair) {
            (Some(f), _) => { est.inlier_frac.push(f.n_inl as f64 / s2.len() as f64); est.n_features.push(s2.len()); est.fit_rms_px.push(f.rms); (nh(&trackmath::mul(&f.h, &c_r)), f.n_inl) }
            (None, Some(f)) => { est.inlier_frac.push(f.n_inl as f64 / s1.len() as f64); est.n_features.push(s1.len()); est.fit_rms_px.push(f.rms); (pred, 0) }
            (None, None) => return Err(format!("stabilize: motion estimation failed at frame {t}: {} consecutive-frame and {} reference matches survived the checks (need {minp})", s1.len(), s2.len())),
        };
        est.path.push(ct);
        let feats = features(&img, w, h, maxf);
        if anch.is_some() && ref_base.is_none() { ref_base = Some(survivors); }
        // re-anchor when fewer than half of the reference's first-fit inliers survive (counting against all detected features would re-anchor
        // every few frames when independently moving objects own part of the reference's features)
        if anch.is_none() || (survivors as f64) < 0.5 * ref_base.unwrap_or(ref_feats.len()) as f64 { ref_pyr = cur.clone(); ref_feats = feats.clone(); c_r = ct; ref_base = None; est.reanchors += 1; }
        prev_feats = feats; prev = cur;
    }
    if method == Method::Position { let c = (w as f64 * 0.5, h as f64 * 0.5); for p in est.path.iter_mut() { let q = trackmath::apply(p, c).unwrap_or(c); *p = translation((q.0 - c.0, q.1 - c.1)); } }
    Ok(est)
}

// ---------- smoothing ----------
/// Value at the first (or last) sample of the least-squares line through `x`.
fn line_end(x: &[f64], last: bool) -> f64 {
    let m = x.len() as f64; let mi = (m - 1.0) * 0.5; let mx = x.iter().sum::<f64>() / m; let (mut sxy, mut sxx) = (0.0, 0.0);
    for (i, v) in x.iter().enumerate() { let di = i as f64 - mi; sxy += di * (v - mx); sxx += di * di; }
    let slope = if sxx > 0.0 { sxy / sxx } else { 0.0 }; mx + slope * (if last { m - 1.0 } else { 0.0 } - mi)
}
/// Sample i of x extended by repeated odd reflection about the pivots a (start) and b (end).
fn ext(x: &[f64], a: f64, b: f64, mut i: i64) -> f64 {
    let n = x.len() as i64; let (mut sign, mut off) = (1.0, 0.0);
    loop { if i < 0 { off += sign * 2.0 * a; sign = -sign; i = -i; } else if i > n - 1 { off += sign * 2.0 * b; sign = -sign; i = 2 * (n - 1) - i; } else { return off + sign * x[i as usize]; } }
}
/// Gaussian smoothing (kernel truncated at 3 sigma) with odd reflection at both ends about the end values of local least-squares
/// lines (see the module doc): any straight line, e.g. a constant-velocity pan, is reproduced exactly.
pub fn gauss_odd(x: &[f64], sigma: f64) -> Vec<f64> {
    let n = x.len(); if n < 2 || !(sigma > 1e-3) { return x.to_vec(); }
    let r = (3.0 * sigma).ceil() as i64; let l = ((3.0 * sigma).ceil() as usize).clamp(2, n); let (a, b) = (line_end(&x[..l], false), line_end(&x[n - l..], true));
    let wts: Vec<f64> = (-r..=r).map(|k| (-((k * k) as f64) / (2.0 * sigma * sigma)).exp()).collect(); let ws: f64 = wts.iter().sum();
    (0..n as i64).map(|i| (-r..=r).map(|k| wts[(k + r) as usize] * ext(x, a, b, i + k)).sum::<f64>() / ws).collect()
}
/// L1-optimal path (Grundmann 2011): minimise 10|D1 p|_1 + |D2 p|_1 + 100|D3 p|_1 subject to |p - c| <= m, on c minus its
/// least-squares line (added back; see the module doc), by Chambolle-Pock with tau = sigma = 0.99 / ||K||. A 1e-3 proximity term
/// (box-normalised units) picks the most centred of equally cheap paths. Result is always inside the box.
pub fn l1_smooth(c: &[f64], m: f64) -> Vec<f64> {
    let n = c.len(); if n < 4 || !(m > 0.0) { return c.to_vec(); }
    let mi = (n as f64 - 1.0) * 0.5; let mc = c.iter().sum::<f64>() / n as f64; let (mut sxy, mut sxx) = (0.0, 0.0); for (i, v) in c.iter().enumerate() { let d = i as f64 - mi; sxy += d * (v - mc); sxx += d * d; }
    let slope = sxy / sxx; let trend = |i: usize| mc + slope * (i as f64 - mi);
    let r: Vec<f64> = (0..n).map(|i| (c[i] - trend(i)) / m).collect();
    let (w1, w2, w3) = (10.0, 1.0, 100.0); let rows = 3 * n - 6;
    let kmul = |p: &[f64], o: &mut [f64]| { let mut k = 0; for i in 0..n - 1 { o[k] = w1 * (p[i + 1] - p[i]); k += 1; } for i in 0..n - 2 { o[k] = w2 * (p[i + 2] - 2.0 * p[i + 1] + p[i]); k += 1; } for i in 0..n - 3 { o[k] = w3 * (p[i + 3] - 3.0 * p[i + 2] + 3.0 * p[i + 1] - p[i]); k += 1; } };
    let ktmul = |y: &[f64], o: &mut [f64]| { o.iter_mut().for_each(|v| *v = 0.0); let mut k = 0;
        for i in 0..n - 1 { let v = w1 * y[k]; o[i + 1] += v; o[i] -= v; k += 1; }
        for i in 0..n - 2 { let v = w2 * y[k]; o[i + 2] += v; o[i + 1] -= 2.0 * v; o[i] += v; k += 1; }
        for i in 0..n - 3 { let v = w3 * y[k]; o[i + 3] += v; o[i + 2] -= 3.0 * v; o[i + 1] += 3.0 * v; o[i] -= v; k += 1; } };
    // ||K|| by power iteration on K^T K
    let (mut v, mut kv, mut ktkv) = ((0..n).map(|i| 1.0 + ((i * 7919) % 13) as f64 * 0.1).collect::<Vec<f64>>(), vec![0.0; rows], vec![0.0; n]); let mut lam = 0.0;
    for _ in 0..200 { kmul(&v[..], &mut kv[..]); ktmul(&kv[..], &mut ktkv[..]); let nn = ktkv.iter().map(|x| x * x).sum::<f64>().sqrt(); if !(nn > 0.0) { break; } lam = nn / v.iter().map(|x| x * x).sum::<f64>().sqrt(); for i in 0..n { v[i] = ktkv[i] / nn; } }
    let kn = lam.sqrt() * 1.02 + 1e-9; let (tau, sig, eps) = (0.99 / kn, 0.99 / kn, 1e-3);
    let mut p: Vec<f64> = gauss_odd(&r, 10.0).iter().zip(&r).map(|(g, ri)| g.clamp(ri - 1.0, ri + 1.0)).collect();
    let (mut pbar, mut y, mut kp, mut kty) = (p.clone(), vec![0.0; rows], vec![0.0; rows], vec![0.0; n]);
    for it in 0..20000 {
        kmul(&pbar[..], &mut kp[..]); for j in 0..rows { y[j] = (y[j] + sig * kp[j]).clamp(-1.0, 1.0); }
        ktmul(&y[..], &mut kty[..]); let mut ch = 0f64;
        for i in 0..n { let q = ((p[i] - tau * kty[i] + tau * eps * r[i]) / (1.0 + tau * eps)).clamp(r[i] - 1.0, r[i] + 1.0); ch = ch.max((q - p[i]).abs()); pbar[i] = 2.0 * q - p[i]; p[i] = q; }
        if it > 100 && ch < 1e-9 { break; }
    }
    (0..n).map(|i| p[i] * m + trend(i)).collect()
}
/// (log s, theta, tx, ty) of the similarity nearest to `hm`, written as p -> s R(theta) (p - c) + c + t.
fn sim_params(hm: &H3, c: P) -> [f64; 4] {
    let h = nh(hm); let (a, b) = (0.5 * (h[0][0] + h[1][1]), 0.5 * (h[1][0] - h[0][1]));
    [a.hypot(b).max(1e-12).ln(), b.atan2(a), a * c.0 - b * c.1 + h[0][2] - c.0, b * c.0 + a * c.1 + h[1][2] - c.1]
}
fn sim_from(p: [f64; 4], c: P) -> H3 { let s = p[0].exp(); let (a, b) = (s * p[1].cos(), s * p[1].sin()); [[a, -b, c.0 + p[2] - (a * c.0 - b * c.1)], [b, a, c.1 + p[3] - (b * c.0 + a * c.1)], [0.0, 0.0, 1.0]] }
fn corners(w: usize, h: usize) -> [P; 4] { [(0.0, 0.0), (w as f64, 0.0), (w as f64, h as f64), (0.0, h as f64)] }

/// S_t, the virtual camera. Locked: identity. Smooth: Position smooths the frame centre's trajectory, Similarity (log s, theta
/// unwrapped, tx, ty) about the centre, Perspective the 8 coordinates of the mapped frame corners. smoothness 0 returns the path.
pub fn smooth_path(path: &[H3], w: usize, h: usize, method: Method, o: &Opts) -> Vec<H3> {
    let n = path.len(); if o.mode == Mode::Locked { return vec![IDENT; n]; } if !(o.smoothness > 0.0) || n < 3 { return path.to_vec(); }
    let c = (w as f64 * 0.5, h as f64 * 0.5); let mpx = o.smoothness / 100.0 * 0.10 * w.min(h) as f64; let mang = mpx / (0.5 * (w as f64).hypot(h as f64));
    let sm = |x: &[f64], bm: f64| -> Vec<f64> { match o.smoothing { Smoothing::Gaussian => gauss_odd(x, o.smoothness / 100.0 * o.fps), Smoothing::L1 => l1_smooth(x, bm) } };
    match method {
        Method::Position => { let q: Vec<P> = path.iter().map(|p| trackmath::apply(p, c).unwrap_or(c)).collect();
            let (x, y) = (sm(&q.iter().map(|v| v.0 - c.0).collect::<Vec<_>>(), mpx), sm(&q.iter().map(|v| v.1 - c.1).collect::<Vec<_>>(), mpx)); (0..n).map(|i| translation((x[i], y[i]))).collect() }
        Method::Similarity => { let mut ps: Vec<[f64; 4]> = path.iter().map(|p| sim_params(p, c)).collect();
            for i in 1..n { let d = ps[i][1] - ps[i - 1][1]; ps[i][1] -= (d / std::f64::consts::TAU).round() * std::f64::consts::TAU; }
            let s: Vec<Vec<f64>> = (0..4).map(|k| sm(&ps.iter().map(|p| p[k]).collect::<Vec<_>>(), if k < 2 { mang } else { mpx })).collect();
            (0..n).map(|i| sim_from([s[0][i], s[1][i], s[2][i], s[3][i]], c)).collect() }
        Method::Perspective => { let cs = corners(w, h); let q: Vec<[P; 4]> = path.iter().map(|p| { let mut r = cs; for k in 0..4 { r[k] = trackmath::apply(p, cs[k]).unwrap_or(cs[k]); } r }).collect();
            let s: Vec<Vec<f64>> = (0..8).map(|k| sm(&q.iter().map(|v| if k % 2 == 0 { v[k / 2].0 } else { v[k / 2].1 }).collect::<Vec<_>>(), mpx)).collect();
            (0..n).map(|i| { let d: Vec<P> = (0..4).map(|k| (s[2 * k][i], s[2 * k + 1][i])).collect(); trackmath::dlt(&cs, &d).unwrap_or(path[i]) }).collect() }
    }
}
/// B_t = S_t C_t^-1: frame-t pixels -> output pixels.
pub fn corrections(path: &[H3], smooth: &[H3]) -> Vec<H3> { path.iter().zip(smooth).map(|(c, s)| trackmath::inv3(c).and_then(|ci| trackmath::norm_h(&trackmath::mul(s, &ci))).unwrap_or(IDENT)).collect() }

/// Is p inside (or on) the convex quad q (either winding)?
fn in_convex(q: &[P; 4], p: P) -> bool {
    let mut sg = 0.0; for i in 0..4 { let (a, b) = (q[i], q[(i + 1) % 4]); let cr = (b.0 - a.0) * (p.1 - a.1) - (b.1 - a.1) * (p.0 - a.0);
        if cr == 0.0 { continue; } if sg == 0.0 { sg = cr.signum(); } else if cr * sg < 0.0 { return false; } } true
}
/// Smallest crop fraction (the zoom is its inverse); the crop never goes below 5% of the frame (zoom 20).
pub const MIN_CROP: f64 = 0.05;
/// Largest centred rect with the frame's aspect inside B_t(frame) for every t, as a fraction of the frame (bisection, exact
/// containment of its 4 corners in each convex quad).
fn crop_fraction(corr: &[H3], w: usize, h: usize) -> f64 {
    let cs = corners(w, h); let c = (w as f64 * 0.5, h as f64 * 0.5);
    let quads: Vec<Option<[P; 4]>> = corr.iter().map(|b| { let mut q = cs; for k in 0..4 { q[k] = trackmath::apply(b, cs[k])?; } Some(q) }).collect();
    let ok = |f: f64| quads.iter().all(|q| match q { Some(q) => cs.iter().all(|p| in_convex(q, (c.0 + (p.0 - c.0) * f, c.1 + (p.1 - c.1) * f))), None => false });
    if ok(1.0) { return 1.0; } let (mut lo, mut hi) = (0.0, 1.0); for _ in 0..60 { let mid = 0.5 * (lo + hi); if ok(mid) { lo = mid; } else { hi = mid; } } lo.max(MIN_CROP)
}
/// Framing (AE): StabilizeOnly leaves the corrections; Crop reports the crop rect; CropAutoScale zooms about the centre by 1/fraction
/// so every frame is filled.
pub fn frame_corrections(corr: &[H3], w: usize, h: usize, framing: Framing) -> Framed {
    if framing == Framing::StabilizeOnly || corr.is_empty() { return Framed { corr: corr.to_vec(), zoom: 1.0, crop: None }; }
    let f = crop_fraction(corr, w, h); let (cx, cy) = (w as f64 * 0.5, h as f64 * 0.5); let rect = [cx - f * cx, cy - f * cy, cx + f * cx, cy + f * cy];
    if framing == Framing::Crop { return Framed { corr: corr.to_vec(), zoom: 1.0, crop: Some(rect) }; }
    let z = 1.0 / f; let zm: H3 = [[z, 0.0, cx * (1.0 - z)], [0.0, z, cy * (1.0 - z)], [0.0, 0.0, 1.0]];
    Framed { corr: corr.iter().map(|b| nh(&trackmath::mul(&zm, b))).collect(), zoom: z, crop: None }
}
/// Exact (x, y, scale, rotation_deg) for transform_bb's out = R(rot) * scale * (src - c) + c + (x, y), c = (w/2, h/2), of the
/// similarity nearest to `b`. A translation gives scale 1, rotation 0.
pub fn similarity_to_props(b: &H3, w: usize, h: usize) -> (f64, f64, f64, f64) {
    let bn = nh(b); let (a, s) = (0.5 * (bn[0][0] + bn[1][1]), 0.5 * (bn[1][0] - bn[0][1])); let c = (w as f64 * 0.5, h as f64 * 0.5);
    (a * c.0 - s * c.1 + bn[0][2] - c.0, s * c.0 + a * c.1 + bn[1][2] - c.1, a.hypot(s), s.atan2(a).to_degrees())
}

const PIN_KEYS: [&str; 8] = ["pin0x", "pin0y", "pin1x", "pin1y", "pin2x", "pin2y", "pin3x", "pin3y"];
/// Run the warp stabilizer on a media clip: per-frame keys (x/y/scale/rotation, or pin corners for Perspective, plus the crop
/// mask quad for Crop) and the report.
pub fn stabilize_clip(p: &Project, clip_id: &str, o: &Opts) -> Result<(Vec<KeyRow>, Value), String> {
    let c = p.find_clip(clip_id).ok_or("no such clip")?.clone();
    if !matches!(c.source, Source::Media { .. }) { return Err(format!("stabilize needs a media clip: '{clip_id}' is not one")); }
    let (w, h) = (p.width as usize, p.height as usize); let n = ((c.duration * p.fps).ceil() as usize).max(1);
    if n < 2 { return Err(format!("clip '{clip_id}' is shorter than 2 frames: nothing to stabilize")); }
    let mut o = o.clone(); o.fps = p.fps; let t0 = std::time::Instant::now();
    let mut cache = crate::render::Cache::new(); let mut frames = |i: usize| crate::tracking::clip_gray_pub(p, &mut cache, &c, i);
    let est = estimate_path(&mut frames, n, w, h, o.method, &o)?;
    let sm = smooth_path(&est.path, w, h, o.method, &o); let corr = corrections(&est.path, &sm); let fr = frame_corrections(&corr, w, h, o.framing);
    let cen = (w as f64 * 0.5, h as f64 * 0.5); let cs = corners(w, h); let (wf, hf) = (w as f64, h as f64);
    let mut rows = Vec::with_capacity(n);
    for i in 0..n { let b = &fr.corr[i]; let t = i as f64 / p.fps;
        let props: Vec<(&'static str, f64)> = if o.method == Method::Perspective {
            // pin quads are in pixel-edge coordinates: corner k of the source rect [0,w]x[0,h] is pixel-index point cs[k] - 0.5
            let mut v = vec![]; for k in 0..4 { let q = trackmath::apply(b, (cs[k].0 - 0.5, cs[k].1 - 0.5)).ok_or_else(|| format!("stabilize: degenerate correction at frame {i}"))?; v.push((PIN_KEYS[2 * k], q.0 + 0.5)); v.push((PIN_KEYS[2 * k + 1], q.1 + 0.5)); } v
        } else { let (x, y, s, r) = similarity_to_props(b, w, h); vec![("x", x), ("y", y), ("scale", s), ("rotation", r)] };
        let crop_quad = match fr.crop { Some(r) => { let bi = trackmath::inv3(b).ok_or_else(|| format!("stabilize: singular correction at frame {i}"))?; let mut q = [(0.0, 0.0); 4];
                for (k, pt) in [(r[0], r[1]), (r[2], r[1]), (r[2], r[3]), (r[0], r[3])].into_iter().enumerate() { let s = trackmath::apply(&bi, pt).ok_or_else(|| format!("stabilize: degenerate crop at frame {i}"))?; q[k] = ((s.0 + 0.5) / wf, (s.1 + 0.5) / hf); } Some(q) }
            None => None };
        rows.push(KeyRow { t, props, crop_quad }); }
    let (mut mpx, mut mdeg) = (0f64, 0f64); for b in &corr { for k in 0..4 { if let Some(q) = trackmath::apply(b, cs[k]) { mpx = mpx.max(trackmath::dist(q, cs[k])); } } mdeg = mdeg.max(sim_params(b, cen)[1].to_degrees().abs()); }
    let fr_in = &est.inlier_frac[1..]; let mean_in = fr_in.iter().sum::<f64>() / fr_in.len() as f64; let min_in = fr_in.iter().cloned().fold(1.0, f64::min);
    let rep = json!({"engine": "warp", "frames": n, "method": o.method.name(), "mode": o.mode.name(), "smoothing": o.smoothing.name(), "smoothness": o.smoothness, "framing": o.framing.name(),
        "zoom": fr.zoom, "crop": fr.crop, "crop_limited": fr.zoom >= 1.0 / MIN_CROP - 1e-9, "reanchors": est.reanchors, "inlier_fraction_mean": mean_in, "inlier_fraction_min": min_in,
        "fit_rms_px_median": trackmath::percentile(&est.fit_rms_px[1..], 50.0), "max_correction_px": mpx, "max_correction_deg": mdeg, "ms_per_frame": t0.elapsed().as_secs_f64() * 1000.0 / n as f64});
    Ok((rows, rep))
}

fn str_opt<'a>(a: &'a Value, k: &str) -> Result<Option<&'a str>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(Value::String(s)) => Ok(Some(s.as_str())), Some(v) => Err(format!("{k} must be a string, got {v}")) } }
fn num_opt(a: &Value, k: &str) -> Result<Option<f64>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_f64().filter(|x| x.is_finite()).map(Some).ok_or_else(|| format!("{k} must be a finite number, got {v}")) } }
/// Warp-engine options from `stabilize` tool arguments, validated (enums, smoothness 0..100, max_features 50..2000).
pub fn opts_from_args(a: &Value) -> Result<Opts, String> {
    let mut o = Opts::default();
    if let Some(s) = str_opt(a, "method")? { o.method = Method::parse(s)?; } if let Some(s) = str_opt(a, "mode")? { o.mode = Mode::parse(s)?; }
    if let Some(s) = str_opt(a, "smoothing")? { o.smoothing = Smoothing::parse(s)?; } if let Some(s) = str_opt(a, "framing")? { o.framing = Framing::parse(s)?; }
    if let Some(x) = num_opt(a, "smoothness")? { if !(0.0..=100.0).contains(&x) { return Err(format!("smoothness {x} out of range 0..100")); } o.smoothness = x; }
    if let Some(x) = num_opt(a, "max_features")? { if !(50.0..=2000.0).contains(&x) { return Err(format!("max_features {x} out of range 50..2000")); } o.max_features = x.round() as usize; }
    Ok(o)
}
/// Write the rows as linear keys on the clip (replacing those props), pin_on for Perspective, and the "stabilize_crop" path mask layer
/// (hard-edged, straight segments) for Crop; a stale "stabilize_crop" layer from an earlier run is removed.
pub fn write_keys(p: &mut Project, clip_id: &str, o: &Opts, rows: &[KeyRow]) -> Result<(), String> {
    let mask_id = if rows.iter().any(|r| r.crop_quad.is_some()) { Some(p.gen_id("mask")) } else { None };
    let c = p.find_clip_mut(clip_id).ok_or("no such clip")?;
    let mut props: Vec<(&'static str, Prop)> = vec![];
    for r in rows { for (k, v) in &r.props { match props.iter().position(|(n, _)| n == k) { Some(j) => props[j].1.set_key(r.t, *v, Ease::Linear), None => { let mut pr = Prop::new(*v); pr.set_key(r.t, *v, Ease::Linear); props.push((*k, pr)); } } } }
    for (k, pr) in props { c.props.insert(k.to_string(), pr); }
    if o.method == Method::Perspective { c.props.insert("pin_on".into(), Prop::new(1.0)); }
    if let Some(m) = c.mask.as_mut() { m.layers.retain(|l| l.name != "stabilize_crop"); }
    if let Some(id) = mask_id {
        let pts = |q: &[(f64, f64); 4]| -> Vec<crate::masks::PathPt> { q.iter().map(|v| crate::masks::PathPt { x: v.0, y: v.1, ..Default::default() }).collect() };
        let mut l = crate::masks::new_layer(id, "path"); l.name = "stabilize_crop".into(); l.closed = true; l.strings.insert("interp".into(), "linear".into());
        l.path_keys = rows.iter().filter_map(|r| r.crop_quad.as_ref().map(|q| crate::masks::PathKey { t: r.t, points: pts(q), hold: false })).collect(); l.points = l.path_keys.first().map(|k| k.points.clone()).unwrap_or_default();
        let m = c.mask.get_or_insert_with(|| Mask { shape: "stack".into(), props: Default::default(), invert: false, layers: vec![], matte: None });
        if m.layers.iter().any(|x| x.enabled && x.mode != "none") { l.mode = "intersect".into(); } m.layers.push(l);
    }
    Ok(())
}
