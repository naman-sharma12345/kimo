//! Pure numeric kernels for track diagnostics (no project/session access): homography fitting with a soft-L1 loss,
//! robust seeding, temporal smoothing, Jacobian polar/SVD decomposition, jitter metrics and ECC refinement.
//! Everything here is a measurement helper. None of it produces ground truth.
pub type P = (f64, f64);
pub type H3 = [[f64; 3]; 3];
pub const IDENT: H3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

pub fn apply(h: &H3, p: P) -> Option<P> {
    let d = h[2][0] * p.0 + h[2][1] * p.1 + h[2][2];
    if !d.is_finite() || d.abs() < 1e-12 { return None; }
    let r = ((h[0][0] * p.0 + h[0][1] * p.1 + h[0][2]) / d, (h[1][0] * p.0 + h[1][1] * p.1 + h[1][2]) / d);
    if r.0.is_finite() && r.1.is_finite() { Some(r) } else { None }
}
pub fn mul(a: &H3, b: &H3) -> H3 { let mut r = [[0.0; 3]; 3]; for i in 0..3 { for j in 0..3 { for k in 0..3 { r[i][j] += a[i][k] * b[k][j]; } } } r }
pub fn inv3(m: &H3) -> Option<H3> {
    let c = |r0: usize, r1: usize, c0: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    let det = m[0][0] * c(1, 2, 1, 2) - m[0][1] * c(1, 2, 0, 2) + m[0][2] * c(1, 2, 0, 1);
    if !det.is_finite() || det.abs() < 1e-300 { return None; }
    let inv = [[c(1, 2, 1, 2), -c(0, 2, 1, 2), c(0, 1, 1, 2)], [-c(1, 2, 0, 2), c(0, 2, 0, 2), -c(0, 1, 0, 2)], [c(1, 2, 0, 1), -c(0, 2, 0, 1), c(0, 1, 0, 1)]];
    let mut r = [[0.0; 3]; 3]; for i in 0..3 { for j in 0..3 { r[i][j] = inv[i][j] / det; } } Some(r)
}
/// Scale so that h[2][2] == 1. None when that entry is ~0 (plane through the origin of the projective frame).
pub fn norm_h(h: &H3) -> Option<H3> { let s = h[2][2]; if !s.is_finite() || s.abs() < 1e-12 { return None; } let mut r = *h; for i in 0..3 { for j in 0..3 { r[i][j] /= s; } } Some(r) }
/// Conjugate by the isotropic scale S=diag(1/s,1/s,1): H' = S H S^-1 (well-conditioned parameters), or the inverse with `back`.
pub fn conj_scale(h: &H3, s: f64, back: bool) -> H3 { let k = if back { s } else { 1.0 / s }; let mut r = *h; r[0][2] *= k; r[1][2] *= k; r[2][0] /= k; r[2][1] /= k; r }
pub fn solve_linear(a: &mut Vec<Vec<f64>>, b: &mut Vec<f64>) -> bool {
    let n = b.len();
    for c in 0..n {
        let mut piv = c; for r in c + 1..n { if a[r][c].abs() > a[piv][c].abs() { piv = r; } }
        if !a[piv][c].is_finite() || a[piv][c].abs() < 1e-14 { return false; }
        a.swap(c, piv); b.swap(c, piv);
        for r in c + 1..n { let f = a[r][c] / a[c][c]; if f != 0.0 { for k in c..n { let v = a[c][k]; a[r][k] -= f * v; } let v = b[c]; b[r] -= f * v; } }
    }
    for c in (0..n).rev() { let mut s = b[c]; for k in c + 1..n { s -= a[c][k] * b[k]; } b[c] = s / a[c][c]; }
    b.iter().all(|v| v.is_finite())
}
fn hartley(pts: &[P]) -> H3 {
    let n = pts.len() as f64; let (cx, cy) = (pts.iter().map(|p| p.0).sum::<f64>() / n, pts.iter().map(|p| p.1).sum::<f64>() / n);
    let md = pts.iter().map(|p| ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt()).sum::<f64>() / n; let s = if md > 1e-12 { 2f64.sqrt() / md } else { 1.0 };
    [[s, 0.0, -s * cx], [0.0, s, -s * cy], [0.0, 0.0, 1.0]]
}
/// Least-squares DLT (h22 = 1, Hartley-normalised). Needs >= 4 non-degenerate pairs.
pub fn dlt(src: &[P], dst: &[P]) -> Option<H3> {
    if src.len() != dst.len() || src.len() < 4 { return None; }
    let (t1, t2) = (hartley(src), hartley(dst));
    let mut ata = vec![vec![0.0; 8]; 8]; let mut atb = vec![0.0; 8];
    for (s, d) in src.iter().zip(dst) {
        let (x, y) = apply(&t1, *s)?; let (u, v) = apply(&t2, *d)?;
        let rows = [([x, y, 1.0, 0.0, 0.0, 0.0, -x * u, -y * u], u), ([0.0, 0.0, 0.0, x, y, 1.0, -x * v, -y * v], v)];
        for (r, t) in rows { for i in 0..8 { atb[i] += r[i] * t; for j in 0..8 { ata[i][j] += r[i] * r[j]; } } }
    }
    if !solve_linear(&mut ata, &mut atb) { return None; }
    let hn = [[atb[0], atb[1], atb[2]], [atb[3], atb[4], atb[5]], [atb[6], atb[7], 1.0]];
    norm_h(&mul(&mul(&inv3(&t2)?, &hn), &t1))
}
pub fn h_from_4(src: &[P; 4], dst: &[P; 4]) -> Option<H3> { dlt(src, dst) }
pub struct Xs(pub u64);
impl Xs { pub fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 } pub fn below(&mut self, n: usize) -> usize { (self.next() % n as u64) as usize } }
/// RANSAC seed: returns an H and the inlier mask (reprojection error < thresh px). Deterministic for a given seed.
pub fn ransac(src: &[P], dst: &[P], thresh: f64, iters: usize, seed: u64) -> Option<(H3, Vec<bool>)> {
    let n = src.len(); if n < 4 { return None; }
    let mut rng = Xs(seed | 1); let mut best: Option<(usize, H3)> = None;
    let count = |h: &H3| src.iter().zip(dst).filter(|(s, d)| apply(h, **s).map(|p| ((p.0 - d.0).powi(2) + (p.1 - d.1).powi(2)).sqrt() < thresh).unwrap_or(false)).count();
    for _ in 0..iters {
        let mut idx = [0usize; 4]; let mut k = 0; while k < 4 { let i = rng.below(n); if !idx[..k].contains(&i) { idx[k] = i; k += 1; } }
        let (s4, d4) = ([src[idx[0]], src[idx[1]], src[idx[2]], src[idx[3]]], [dst[idx[0]], dst[idx[1]], dst[idx[2]], dst[idx[3]]]);
        if let Some(h) = h_from_4(&s4, &d4) { let c = count(&h); if best.map(|b| c > b.0).unwrap_or(true) { best = Some((c, h)); } }
    }
    let (_, h0) = best?; let mask: Vec<bool> = src.iter().zip(dst).map(|(s, d)| apply(&h0, *s).map(|p| ((p.0 - d.0).powi(2) + (p.1 - d.1).powi(2)).sqrt() < thresh).unwrap_or(false)).collect();
    let (si, di): (Vec<P>, Vec<P>) = src.iter().zip(dst).zip(&mask).filter(|(_, m)| **m).map(|(p, _)| (*p.0, *p.1)).unzip();
    let h = if si.len() >= 4 { dlt(&si, &di).unwrap_or(h0) } else { h0 };
    let mask: Vec<bool> = src.iter().zip(dst).map(|(s, d)| apply(&h, *s).map(|p| ((p.0 - d.0).powi(2) + (p.1 - d.1).powi(2)).sqrt() < thresh).unwrap_or(false)).collect();
    Some((h, mask))
}
/// Soft-L1 robust homography fit by Levenberg-Marquardt, loss rho(z)=2(sqrt(1+z)-1), z=(r/f_scale)^2, applied to every residual component
/// (same loss as scipy `least_squares(loss='soft_l1', f_scale=..)`). Residuals are in pixels. Returns (H, iterations, robust cost).
pub fn robust_fit(src: &[P], dst: &[P], init: &H3, f_scale: f64, max_nfev: usize, scale: f64) -> Option<(H3, usize, f64)> {
    if src.len() < 4 || src.len() != dst.len() { return None; }
    let s: Vec<P> = src.iter().map(|p| (p.0 / scale, p.1 / scale)).collect(); let d: Vec<P> = dst.iter().map(|p| (p.0 / scale, p.1 / scale)).collect();
    let h0 = norm_h(&conj_scale(init, scale, false))?; let mut x = [h0[0][0], h0[0][1], h0[0][2], h0[1][0], h1(&h0), h0[1][2], h0[2][0], h0[2][1]];
    fn h1(h: &H3) -> f64 { h[1][1] }
    let resid = |x: &[f64; 8]| -> Option<Vec<f64>> { let mut r = Vec::with_capacity(s.len() * 2); for (p, q) in s.iter().zip(&d) { let den = x[6] * p.0 + x[7] * p.1 + 1.0; if !den.is_finite() || den.abs() < 1e-9 { return None; } r.push(scale * ((x[0] * p.0 + x[1] * p.1 + x[2]) / den - q.0)); r.push(scale * ((x[3] * p.0 + x[4] * p.1 + x[5]) / den - q.1)); } Some(r) };
    let cost = |r: &[f64]| -> f64 { r.iter().map(|v| { let z = (v / f_scale).powi(2); f_scale * f_scale * 2.0 * ((1.0 + z).sqrt() - 1.0) }).sum::<f64>() * 0.5 };
    let mut r = resid(&x)?; let mut c = cost(&r); let mut mu = 1e-3; let mut it = 0;
    for _ in 0..max_nfev {
        crate::guard::check(); it += 1;
        let mut ata = vec![vec![0.0; 8]; 8]; let mut atb = vec![0.0; 8];
        for (k, p) in s.iter().enumerate() {
            let den = x[6] * p.0 + x[7] * p.1 + 1.0; let u = (x[0] * p.0 + x[1] * p.1 + x[2]) / den; let v = (x[3] * p.0 + x[4] * p.1 + x[5]) / den;
            let ju = [p.0 / den, p.1 / den, 1.0 / den, 0.0, 0.0, 0.0, -u * p.0 / den, -u * p.1 / den]; let jv = [0.0, 0.0, 0.0, p.0 / den, p.1 / den, 1.0 / den, -v * p.0 / den, -v * p.1 / den];
            for (jj, rr) in [(ju, r[2 * k]), (jv, r[2 * k + 1])] { let w = 1.0 / (1.0 + (rr / f_scale).powi(2)).sqrt(); for i in 0..8 { let ji = jj[i] * scale; atb[i] -= w * ji * rr; for j in 0..8 { ata[i][j] += w * ji * jj[j] * scale; } } }
        }
        let mut accepted = false; let mut step_norm = 0.0;
        for _ in 0..12 {
            let mut a = ata.clone(); for i in 0..8 { a[i][i] += mu * (ata[i][i].abs() + 1e-12); } let mut b = atb.clone();
            if !solve_linear(&mut a, &mut b) { mu *= 10.0; continue; }
            let mut xn = x; for i in 0..8 { xn[i] += b[i]; }
            if let Some(rn) = resid(&xn) { let cn = cost(&rn); if cn <= c { step_norm = b.iter().map(|v| v * v).sum::<f64>().sqrt(); x = xn; r = rn; c = cn; mu = (mu * 0.3).max(1e-12); accepted = true; break; } }
            mu *= 4.0;
        }
        if !accepted || step_norm < 1e-10 { break; }
    }
    let hn: H3 = [[x[0], x[1], x[2]], [x[3], x[4], x[5]], [x[6], x[7], 1.0]];
    Some((conj_scale(&hn, scale, true), it, c))
}
/// Smooth the 8 free parameters along time: minimise |x-y|^2 + lambda |D2 x|^2 (pentadiagonal banded Cholesky).
pub fn smooth_series(y: &[f64], lambda: f64) -> Vec<f64> {
    let n = y.len(); if n < 3 || lambda <= 0.0 { return y.to_vec(); }
    let mut a = vec![[0.0f64; 3]; n]; // a[i][k] = A(i, i-k)
    for i in 0..n { a[i][0] = 1.0; }
    for k in 0..n - 2 { let d = [1.0, -2.0, 1.0]; for p in 0..3 { for q in 0..=p { a[k + p][p - q] += lambda * d[p] * d[q]; } } }
    let mut l = vec![[0.0f64; 3]; n];
    for i in 0..n { for j in i.saturating_sub(2)..=i { let mut sum = a[i][i - j]; for k in i.saturating_sub(2).max(j.saturating_sub(2))..j { sum -= l[i][i - k] * l[j][j - k]; } if i == j { l[i][0] = sum.max(1e-300).sqrt(); } else { l[i][i - j] = sum / l[j][0]; } } }
    let mut z = y.to_vec(); for i in 0..n { let mut s = z[i]; for k in i.saturating_sub(2)..i { s -= l[i][i - k] * z[k]; } z[i] = s / l[i][0]; }
    for i in (0..n).rev() { let mut s = z[i]; for k in i + 1..(i + 3).min(n) { s -= l[k][k - i] * z[k]; } z[i] = s / l[i][0]; }
    z
}
pub fn percentile(v: &[f64], q: f64) -> f64 {
    let mut s: Vec<f64> = v.iter().cloned().filter(|x| x.is_finite()).collect(); if s.is_empty() { return f64::NAN; }
    s.sort_by(|a, b| a.partial_cmp(b).unwrap()); let pos = (s.len() - 1) as f64 * q / 100.0; let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize); s[lo] + (s[hi] - s[lo]) * (pos - lo as f64)
}
pub fn dist(a: P, b: P) -> f64 { ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() }
/// Local 2x2 Jacobian of the homography at pixel `c`.
pub fn jacobian(h: &H3, c: P) -> Option<[[f64; 2]; 2]> {
    let den = h[2][0] * c.0 + h[2][1] * c.1 + h[2][2]; if den.abs() < 1e-12 { return None; }
    let num = (h[0][0] * c.0 + h[0][1] * c.1 + h[0][2], h[1][0] * c.0 + h[1][1] * c.1 + h[1][2]);
    Some([[(h[0][0] * den - num.0 * h[2][0]) / (den * den), (h[0][1] * den - num.0 * h[2][1]) / (den * den)], [(h[1][0] * den - num.1 * h[2][0]) / (den * den), (h[1][1] * den - num.1 * h[2][1]) / (den * den)]])
}
pub struct Decomp { pub rotation_deg: f64, pub scale_major: f64, pub scale_minor: f64, pub shear: f64, pub det: f64, pub reflection: bool }
/// Polar decomposition J = R S and SVD singular values of a 2x2 Jacobian. rotation = angle of R; shear = off-diagonal of the symmetric stretch S.
pub fn decompose(j: &[[f64; 2]; 2]) -> Decomp {
    let (a, b, c, d) = (j[0][0], j[0][1], j[1][0], j[1][1]); let det = a * d - b * c;
    let e = ((a + d).powi(2) + (c - b).powi(2)).sqrt() / 2.0; let f = ((a - d).powi(2) + (b + c).powi(2)).sqrt() / 2.0;
    let (s1, s2) = (e + f, e - f); // s2 is signed: negative when det < 0
    // orientation-preserving polar factor: R = rotation by atan2(c-b, a+d); S = R^T J
    let th = (c - b).atan2(a + d); let (ct, st) = (th.cos(), th.sin());
    let sh = ct * b + st * d; // (R^T J)[0][1] with R=[[ct,-st],[st,ct]] -> ct*b + st*d
    Decomp { rotation_deg: th.to_degrees(), scale_major: s1, scale_minor: s2.abs(), shear: sh, det, reflection: det < 0.0 }
}
pub fn blur5(img: &[f32], w: usize, h: usize) -> Vec<f32> {
    let k = [1.0f32, 4.0, 6.0, 4.0, 1.0]; let mut t = vec![0f32; w * h]; let mut o = vec![0f32; w * h];
    for y in 0..h { for x in 0..w { let mut s = 0.0; for (i, kk) in k.iter().enumerate() { let xx = (x as i32 + i as i32 - 2).clamp(0, w as i32 - 1) as usize; s += kk * img[y * w + xx]; } t[y * w + x] = s / 16.0; } }
    for y in 0..h { for x in 0..w { let mut s = 0.0; for (i, kk) in k.iter().enumerate() { let yy = (y as i32 + i as i32 - 2).clamp(0, h as i32 - 1) as usize; s += kk * t[yy * w + x]; } o[y * w + x] = s / 16.0; } }
    o
}
fn bil(img: &[f32], w: usize, h: usize, x: f64, y: f64) -> Option<f64> {
    if !(x >= 0.0 && y >= 0.0 && x < (w - 1) as f64 && y < (h - 1) as f64) { return None; }
    let (x0, y0) = (x as usize, y as usize); let (ax, ay) = (x - x0 as f64, y - y0 as f64); let i = y0 * w + x0;
    Some(img[i] as f64 * (1.0 - ax) * (1.0 - ay) + img[i + 1] as f64 * ax * (1.0 - ay) + img[i + w] as f64 * (1.0 - ax) * ay + img[i + w + 1] as f64 * ax * ay)
}
pub struct EccResult { pub h: H3, pub rho_init: f64, pub rho: f64, pub iters: usize, pub converged: bool }
/// Enhanced Correlation Coefficient refinement of a homography (template -> image), Gauss-Newton on the 8 parameters in a
/// scale-normalised frame. `pts` are template pixels inside the mask. Inputs should already be blurred. Returns None when the
/// starting warp leaves too few mask pixels in the image or the system is singular.
pub fn ecc(tpl: &[f32], img: &[f32], w: usize, h: usize, pts: &[(u32, u32)], init: &H3, max_iter: usize, eps: f64) -> Option<EccResult> {
    let scale = w.max(h) as f64; let h0 = norm_h(&conj_scale(init, scale, false))?; let mut x = [h0[0][0], h0[0][1], h0[0][2], h0[1][0], h0[1][1], h0[1][2], h0[2][0], h0[2][1]];
    let mut gx = vec![0f32; w * h]; let mut gy = vec![0f32; w * h];
    for yy in 1..h - 1 { for xx in 1..w - 1 { gx[yy * w + xx] = (img[yy * w + xx + 1] - img[yy * w + xx - 1]) * 0.5; gy[yy * w + xx] = (img[(yy + 1) * w + xx] - img[(yy - 1) * w + xx]) * 0.5; } }
    let (mut rho_init, mut rho_last) = (f64::NAN, f64::NAN); let mut converged = false; let mut it = 0;
    let eval = |x: &[f64; 8]| -> Option<(Vec<[f64; 8]>, Vec<f64>, Vec<f64>)> {
        let mut gs = Vec::new(); let mut tv = Vec::new(); let mut iv = Vec::new();
        for &(px, py) in pts { let (xn, yn) = (px as f64 / scale, py as f64 / scale); let den = x[6] * xn + x[7] * yn + 1.0; if den.abs() < 1e-9 { continue; }
            let (u, v) = ((x[0] * xn + x[1] * yn + x[2]) / den * scale, (x[3] * xn + x[4] * yn + x[5]) / den * scale);
            let (Some(iw), Some(ix), Some(iy)) = (bil(img, w, h, u, v), bil(&gx, w, h, u, v), bil(&gy, w, h, u, v)) else { continue };
            let (uu, vv) = (u / scale, v / scale);
            let ju = [xn / den, yn / den, 1.0 / den, 0.0, 0.0, 0.0, -uu * xn / den, -uu * yn / den]; let jv = [0.0, 0.0, 0.0, xn / den, yn / den, 1.0 / den, -vv * xn / den, -vv * yn / den];
            let mut g = [0.0; 8]; for k in 0..8 { g[k] = scale * (ix * ju[k] + iy * jv[k]); }
            gs.push(g); tv.push(tpl[py as usize * w + px as usize] as f64); iv.push(iw); }
        if gs.len() < 50 { None } else { Some((gs, tv, iv)) }
    };
    for k in 0..=max_iter {
        crate::guard::check(); it = k;
        let Some((gs, mut tv, mut iv)) = eval(&x) else { eprintln!("ecc eval none k={k}"); return None };let n = gs.len() as f64;
        let (mt, mi) = (tv.iter().sum::<f64>() / n, iv.iter().sum::<f64>() / n); for v in tv.iter_mut() { *v -= mt; } for v in iv.iter_mut() { *v -= mi; }
        let (nt, ni) = (tv.iter().map(|v| v * v).sum::<f64>().sqrt(), iv.iter().map(|v| v * v).sum::<f64>().sqrt()); if nt < 1e-9 || ni < 1e-9 { return None; }
        let corr: f64 = tv.iter().zip(&iv).map(|(a, b)| a * b).sum::<f64>(); let rho = corr / (nt * ni); if k == 0 { rho_init = rho; } rho_last = rho;
        if k == max_iter { break; }
        let mut hess = vec![vec![0.0; 8]; 8]; let mut ip = vec![0.0; 8]; let mut tp = vec![0.0; 8];
        for (i, g) in gs.iter().enumerate() { for a in 0..8 { ip[a] += g[a] * iv[i]; tp[a] += g[a] * tv[i]; for b in 0..8 { hess[a][b] += g[a] * g[b]; } } }
        for a in 0..8 { let d = hess[a][a].abs() * 1e-9 + 1e-12; hess[a][a] += d; }
        let mut hi = ip.clone(); let mut h1 = hess.clone(); if !solve_linear(&mut h1, &mut hi) { eprintln!("ecc solve1 fail"); return None; } // hi = Hinv * ip
        let lam_n = iv.iter().map(|v| v * v).sum::<f64>() - ip.iter().zip(&hi).map(|(a, b)| a * b).sum::<f64>();
        let lam_d = corr - tp.iter().zip(&hi).map(|(a, b)| a * b).sum::<f64>(); if lam_d.abs() < 1e-12 { break; }
        let lam = lam_n / lam_d;
        let mut ep = vec![0.0; 8]; for (i, g) in gs.iter().enumerate() { let e = lam * tv[i] - iv[i]; for a in 0..8 { ep[a] += g[a] * e; } }
        let mut h2 = hess.clone(); if !solve_linear(&mut h2, &mut ep) { eprintln!("ecc solve2 fail"); return None; }
        let dn = ep.iter().map(|v| v * v).sum::<f64>().sqrt(); for a in 0..8 { x[a] += ep[a]; }
        if dn < eps { converged = true; let (gs2, mut t2, mut i2) = eval(&x)?; let n2 = gs2.len() as f64; let (m1, m2) = (t2.iter().sum::<f64>() / n2, i2.iter().sum::<f64>() / n2); for v in t2.iter_mut() { *v -= m1; } for v in i2.iter_mut() { *v -= m2; } let (a, b) = (t2.iter().map(|v| v * v).sum::<f64>().sqrt(), i2.iter().map(|v| v * v).sum::<f64>().sqrt()); rho_last = t2.iter().zip(&i2).map(|(p, q)| p * q).sum::<f64>() / (a * b); break; }
    }
    let hn: H3 = [[x[0], x[1], x[2]], [x[3], x[4], x[5]], [x[6], x[7], 1.0]];
    Some(EccResult { h: conj_scale(&hn, scale, true), rho_init, rho: rho_last, iters: it, converged })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn truth(t: f64) -> H3 { let a = 0.02 * t; [[a.cos() * (1.0 + 0.01 * t), -a.sin(), 5.0 * t], [a.sin(), a.cos(), -3.0 * t], [1e-5 * t, -2e-5 * t, 1.0]] }
    fn grid() -> Vec<P> { let mut v = vec![]; for i in 0..12 { for j in 0..9 { v.push((300.0 + 37.0 * i as f64 + (j * 7 % 5) as f64, 200.0 + 29.0 * j as f64 + (i * 3 % 4) as f64)); } } v }
    #[test] fn dlt_and_ransac_recover_with_outliers() {
        let src = grid(); let h = truth(4.0); let mut dst: Vec<P> = src.iter().map(|p| apply(&h, *p).unwrap()).collect();
        for i in (0..dst.len()).step_by(5) { dst[i].0 += 40.0; dst[i].1 -= 25.0; }
        let (hr, mask) = ransac(&src, &dst, 0.6, 300, 7).unwrap();
        let err: f64 = src.iter().map(|p| dist(apply(&hr, *p).unwrap(), apply(&h, *p).unwrap())).fold(0.0, f64::max); assert!(err < 1e-6, "ransac err {err}"); assert!(mask.iter().filter(|m| !**m).count() >= 20);
    }
    #[test] fn robust_fit_ignores_outliers_and_matches_truth() {
        let src = grid(); let h = truth(6.0); let mut dst: Vec<P> = src.iter().map(|p| { let q = apply(&h, *p).unwrap(); (q.0 + 0.05 * ((p.0 * 7.0).sin()), q.1 + 0.05 * ((p.1 * 3.0).cos())) }).collect();
        for i in (0..dst.len()).step_by(9) { dst[i].0 += 30.0; }
        let init = dlt(&src, &dst).unwrap(); let (hf, _, _) = robust_fit(&src, &dst, &init, 0.6, 150, 1280.0).unwrap();
        let med: Vec<f64> = src.iter().map(|p| dist(apply(&hf, *p).unwrap(), apply(&h, *p).unwrap())).collect(); assert!(percentile(&med, 50.0) < 0.3, "median {}", percentile(&med, 50.0));
        let plain: Vec<f64> = src.iter().map(|p| dist(apply(&init, *p).unwrap(), apply(&h, *p).unwrap())).collect(); assert!(percentile(&med, 50.0) < percentile(&plain, 50.0));
    }
    #[test] fn smoothing_preserves_lines_and_damps_noise() {
        let y: Vec<f64> = (0..50).map(|i| 2.0 * i as f64 + 1.0).collect(); let z = smooth_series(&y, 5.0); for (a, b) in y.iter().zip(&z) { assert!((a - b).abs() < 1e-8); }
        let n: Vec<f64> = (0..50).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect(); let z = smooth_series(&n, 5.0); assert!(z.iter().map(|v| v.abs()).fold(0.0, f64::max) < 0.5);
    }
    #[test] fn decompose_known() {
        let th = 0.3f64; let (c, s) = (th.cos(), th.sin()); let j = [[2.0 * c, -0.5 * s * 0.0 - s], [2.0 * s, c]]; let d = decompose(&j); assert!((d.rotation_deg - th.to_degrees()).abs() < 5.0);
        let r = [[c, -s], [s, c]]; let st = [[2.0, 0.3], [0.3, 1.0]]; let mut jj = [[0.0; 2]; 2]; for i in 0..2 { for k in 0..2 { for m in 0..2 { jj[i][k] += r[i][m] * st[m][k]; } } }
        let d = decompose(&jj); assert!((d.rotation_deg - th.to_degrees()).abs() < 1e-9); assert!((d.shear - 0.3).abs() < 1e-9); let tr = 3.0; let det = 2.0 - 0.09; let disc = ((tr * tr / 4.0) - det as f64).sqrt(); assert!((d.scale_major - (1.5 + disc)).abs() < 1e-9); assert!((d.scale_minor - (1.5 - disc)).abs() < 1e-9);
    }
    #[test] fn jacobian_of_affine_is_linear_part() { let h = [[1.1, 0.2, 5.0], [-0.1, 0.9, 3.0], [0.0, 0.0, 1.0]]; let j = jacobian(&h, (100.0, 50.0)).unwrap(); assert!((j[0][0] - 1.1).abs() < 1e-12 && (j[1][0] + 0.1).abs() < 1e-12 && (j[0][1] - 0.2).abs() < 1e-12); }
    #[test] fn ecc_recovers_shift() {
        let (w, h) = (200usize, 160usize); let f = |x: f64, y: f64| (0.1 * x).sin() * (0.13 * y).cos() + 0.5 * ((0.05 * x + 0.07 * y).sin()) + 0.3 * ((x - 100.0).powi(2) + (y - 80.0).powi(2) < 900.0) as i32 as f64;
        let tpl: Vec<f32> = (0..w * h).map(|i| f((i % w) as f64, (i / w) as f64) as f32).collect(); let tt = [[1.0, 0.0, 2.3], [0.0, 1.0, -1.7], [0.0, 0.0, 1.0]];
        let img: Vec<f32> = (0..w * h).map(|i| f((i % w) as f64 - 2.3, (i / w) as f64 + 1.7) as f32).collect();
        let pts: Vec<(u32, u32)> = (40..120u32).flat_map(|y| (40..160u32).map(move |x| (x, y))).collect();
        let r = ecc(&blur5(&tpl, w, h), &blur5(&img, w, h), w, h, &pts, &IDENT, 100, 1e-8).unwrap(); let e: f64 = (r.h[0][2] - tt[0][2]).abs().max((r.h[1][2] - tt[1][2]).abs()); assert!(e < 0.05, "ecc shift err {e} rho {}", r.rho); assert!(r.rho > r.rho_init);
    }
}
