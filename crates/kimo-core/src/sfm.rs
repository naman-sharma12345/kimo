//! Structure-from-motion camera solve: feature tracks -> 2-view init -> incremental PnP -> sparse bundle adjustment.
//! Standard CV convention: x right, y down, z forward; pose is world->camera (X_c = R X_w + t). Pure Rust, no dependencies.
use std::collections::HashMap;

pub type V3 = [f64; 3];
pub type M3 = [[f64; 3]; 3];
pub fn dot(a: &V3, b: &V3) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
pub fn cross(a: &V3, b: &V3) -> V3 { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
pub fn norm(a: &V3) -> f64 { dot(a, a).sqrt() }
pub fn mm(a: &M3, b: &M3) -> M3 { let mut r = [[0.0; 3]; 3]; for i in 0..3 { for j in 0..3 { for k in 0..3 { r[i][j] += a[i][k] * b[k][j]; } } } r }
pub fn mv(a: &M3, v: &V3) -> V3 { [dot(&a[0], v), dot(&a[1], v), dot(&a[2], v)] }
pub fn tp(a: &M3) -> M3 { let mut r = [[0.0; 3]; 3]; for i in 0..3 { for j in 0..3 { r[i][j] = a[j][i]; } } r }
pub fn det3(a: &M3) -> f64 { dot(&a[0], &cross(&a[1], &a[2])) }
pub const ID3: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

/// Rodrigues: axis-angle -> rotation.
pub fn exp_so3(w: &V3) -> M3 {
    let th = norm(w); if th < 1e-12 { return [[1.0, -w[2], w[1]], [w[2], 1.0, -w[0]], [-w[1], w[0], 1.0]]; }
    let k = [w[0] / th, w[1] / th, w[2] / th]; let (s, c) = th.sin_cos(); let v = 1.0 - c;
    [[c + k[0] * k[0] * v, k[0] * k[1] * v - k[2] * s, k[0] * k[2] * v + k[1] * s], [k[1] * k[0] * v + k[2] * s, c + k[1] * k[1] * v, k[1] * k[2] * v - k[0] * s], [k[2] * k[0] * v - k[1] * s, k[2] * k[1] * v + k[0] * s, c + k[2] * k[2] * v]]
}
pub fn log_so3(r: &M3) -> V3 {
    let c = ((r[0][0] + r[1][1] + r[2][2] - 1.0) / 2.0).clamp(-1.0, 1.0); let th = c.acos();
    if th < 1e-9 { return [(r[2][1] - r[1][2]) / 2.0, (r[0][2] - r[2][0]) / 2.0, (r[1][0] - r[0][1]) / 2.0]; }
    if std::f64::consts::PI - th < 1e-5 { // near pi: axis from diagonal
        let mut k = [((r[0][0] + 1.0) / 2.0).max(0.0).sqrt(), ((r[1][1] + 1.0) / 2.0).max(0.0).sqrt(), ((r[2][2] + 1.0) / 2.0).max(0.0).sqrt()];
        if r[0][1] + r[1][0] < 0.0 { k[1] = -k[1]; } if r[0][2] + r[2][0] < 0.0 { k[2] = -k[2]; }
        let n = norm(&k).max(1e-12); return [k[0] / n * th, k[1] / n * th, k[2] / n * th];
    }
    let s = th / (2.0 * th.sin()); [(r[2][1] - r[1][2]) * s, (r[0][2] - r[2][0]) * s, (r[1][0] - r[0][1]) * s]
}

/// Symmetric eigen-decomposition (cyclic Jacobi). Returns (eigenvalues ascending, eigenvectors as columns v[:,k] = vecs[k]).
pub fn eig_sym(a: &[Vec<f64>]) -> (Vec<f64>, Vec<Vec<f64>>) {
    let n = a.len(); let mut m: Vec<Vec<f64>> = a.to_vec(); let mut v = vec![vec![0.0; n]; n]; for i in 0..n { v[i][i] = 1.0; }
    for _ in 0..60 {
        let mut off = 0.0; for i in 0..n { for j in i + 1..n { off += m[i][j] * m[i][j]; } } if off < 1e-24 { break; }
        for p in 0..n { for q in p + 1..n { if m[p][q].abs() < 1e-300 { continue; }
            let th = (m[q][q] - m[p][p]) / (2.0 * m[p][q]); let t = th.signum() / (th.abs() + (th * th + 1.0).sqrt()); let t = if th == 0.0 { 1.0 } else { t };
            let c = 1.0 / (t * t + 1.0).sqrt(); let s = t * c;
            for k in 0..n { let (kp, kq) = (m[k][p], m[k][q]); m[k][p] = c * kp - s * kq; m[k][q] = s * kp + c * kq; }
            for k in 0..n { let (pk, qk) = (m[p][k], m[q][k]); m[p][k] = c * pk - s * qk; m[q][k] = s * pk + c * qk; }
            for k in 0..n { let (kp, kq) = (v[k][p], v[k][q]); v[k][p] = c * kp - s * kq; v[k][q] = s * kp + c * kq; } } }
    }
    let mut idx: Vec<usize> = (0..n).collect(); idx.sort_by(|&i, &j| m[i][i].partial_cmp(&m[j][j]).unwrap());
    let vals = idx.iter().map(|&i| m[i][i]).collect(); let vecs = idx.iter().map(|&i| (0..n).map(|k| v[k][i]).collect()).collect(); (vals, vecs)
}
/// Smallest right singular vector of A (rows x n).
pub fn null_vec(rows: &[Vec<f64>], n: usize) -> Vec<f64> {
    let mut ata = vec![vec![0.0; n]; n]; for r in rows { for i in 0..n { if r[i] == 0.0 { continue; } for j in 0..n { ata[i][j] += r[i] * r[j]; } } }
    eig_sym(&ata).1.swap_remove(0)
}
/// Solve SPD system (Cholesky with jitter fallback, then Gauss). a is n x n row-major flat.
pub fn solve_spd(a: &mut [f64], b: &mut [f64], n: usize) -> bool {
    let a0 = a.to_vec(); let mut jit = 0.0;
    for _ in 0..8 {
        let mut l = a0.clone(); for i in 0..n { l[i * n + i] += jit; }
        let mut ok = true;
        'c: for i in 0..n { for j in 0..=i { let mut s = l[i * n + j]; for k in 0..j { s -= l[i * n + k] * l[j * n + k]; }
            if i == j { if s <= 1e-14 { ok = false; break 'c; } l[i * n + i] = s.sqrt(); } else { l[i * n + j] = s / l[j * n + j]; } } }
        if ok { let mut y = vec![0.0; n]; for i in 0..n { let mut s = b[i]; for k in 0..i { s -= l[i * n + k] * y[k]; } y[i] = s / l[i * n + i]; }
            for i in (0..n).rev() { let mut s = y[i]; for k in i + 1..n { s -= l[k * n + i] * b[k]; } b[i] = s / l[i * n + i]; } return true; }
        jit = if jit == 0.0 { 1e-9 } else { jit * 100.0 };
    }
    false
}
fn inv3(m: &M3) -> Option<M3> {
    let d = det3(m); if d.abs() < 1e-18 { return None; } let c0 = cross(&m[1], &m[2]); let c1 = cross(&m[2], &m[0]); let c2 = cross(&m[0], &m[1]);
    Some([[c0[0] / d, c1[0] / d, c2[0] / d], [c0[1] / d, c1[1] / d, c2[1] / d], [c0[2] / d, c1[2] / d, c2[2] / d]])
}

#[derive(Clone, Copy, Debug)]
pub struct Pose { pub r: M3, pub t: V3 }
impl Pose {
    pub fn apply(&self, x: &V3) -> V3 { let v = mv(&self.r, x); [v[0] + self.t[0], v[1] + self.t[1], v[2] + self.t[2]] }
    pub fn center(&self) -> V3 { let v = mv(&tp(&self.r), &self.t); [-v[0], -v[1], -v[2]] }
}
#[derive(Clone, Copy, Debug)]
pub struct Intr { pub f: f64, pub cx: f64, pub cy: f64, pub k1: f64, pub k2: f64 }
#[derive(Clone, Copy, Debug, Default)]
pub struct Flags { pub focal: bool, pub dist: bool, pub pp: bool, pub rot_only: bool }
fn proj(p: &Pose, x: &V3, k: &Intr) -> Option<(f64, f64)> { let c = p.apply(x); if c[2] < 1e-9 { return None; } let (a, b) = (c[0] / c[2], c[1] / c[2]); let r2 = a * a + b * b; let sc = 1.0 + k.k1 * r2 + k.k2 * r2 * r2; Some((k.f * a * sc + k.cx, k.f * b * sc + k.cy)) }
fn reproj_err(p: &Pose, x: &V3, k: &Intr, o: (f64, f64)) -> f64 { match proj(p, x, k) { Some((u, v)) => ((u - o.0).powi(2) + (v - o.1).powi(2)).sqrt(), None => 1e9 } }

/// Linear triangulation from calibrated-normalised observations (x,y) = ((u-cx)/f, (v-cy)/f).
pub fn triangulate(views: &[(&Pose, (f64, f64))]) -> Option<V3> {
    let mut a = [[0.0f64; 3]; 3]; let mut b = [0.0f64; 3];
    for (p, (x, y)) in views { for (u, row) in [(*x, 0usize), (*y, 1usize)] {
        let r = [u * p.r[2][0] - p.r[row][0], u * p.r[2][1] - p.r[row][1], u * p.r[2][2] - p.r[row][2]]; let d = u * p.t[2] - p.t[row];
        for i in 0..3 { for j in 0..3 { a[i][j] += r[i] * r[j]; } b[i] -= r[i] * d; } } }
    let ai = inv3(&a)?; Some(mv(&ai, &b))
}
/// Pixel -> ideal (undistorted) normalised coordinates, inverting the radial model by fixed-point iteration.
pub fn norm_obs(o: (f64, f64), k: &Intr) -> (f64, f64) { let xd = ((o.0 - k.cx) / k.f, (o.1 - k.cy) / k.f); let mut xn = xd; for _ in 0..12 { let r2 = xn.0 * xn.0 + xn.1 * xn.1; let sc = 1.0 + k.k1 * r2 + k.k2 * r2 * r2; xn = (xd.0 / sc, xd.1 / sc); } xn }
/// Pixel position with lens distortion removed (as if shot with an ideal pinhole of the same focal and principal point).
pub fn undistort_px(o: (f64, f64), k: &Intr) -> (f64, f64) { let n = norm_obs(o, k); (n.0 * k.f + k.cx, n.1 * k.f + k.cy) }

// ---------- two view ----------
fn essential_8pt(pairs: &[((f64, f64), (f64, f64))]) -> Option<M3> {
    if pairs.len() < 8 { return None; }
    let rows: Vec<Vec<f64>> = pairs.iter().map(|((x1, y1), (x2, y2))| vec![x2 * x1, x2 * y1, *x2, y2 * x1, y2 * y1, *y2, *x1, *y1, 1.0]).collect();
    let e = null_vec(&rows, 9); let m: M3 = [[e[0], e[1], e[2]], [e[3], e[4], e[5]], [e[6], e[7], e[8]]];
    let (u, _s, v) = svd3(&m)?; let d: M3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 0.0]]; Some(mm(&mm(&u, &d), &tp(&v)))
}
/// 3x3 SVD via Jacobi on MᵀM. Returns (U, sigma, V) with M = U diag(sigma) Vᵀ, sigma descending.
pub fn svd3(m: &M3) -> Option<(M3, V3, M3)> {
    let mt = tp(m); let mtm = mm(&mt, m); let a: Vec<Vec<f64>> = mtm.iter().map(|r| r.to_vec()).collect(); let (vals, vecs) = eig_sym(&a);
    let order = [2usize, 1, 0]; let mut v = [[0.0; 3]; 3]; let mut s = [0.0; 3]; for (c, &o) in order.iter().enumerate() { s[c] = vals[o].max(0.0).sqrt(); for r in 0..3 { v[r][c] = vecs[o][r]; } }
    let mut u = [[0.0; 3]; 3];
    for c in 0..2 { if s[c] < 1e-14 { return None; } let col = mv(m, &[v[0][c], v[1][c], v[2][c]]); for r in 0..3 { u[r][c] = col[r] / s[c]; } }
    let c0 = [u[0][0], u[1][0], u[2][0]]; let c1 = [u[0][1], u[1][1], u[2][1]]; let c2 = cross(&c0, &c1); for r in 0..3 { u[r][2] = c2[r]; }
    Some((u, s, v))
}
fn sampson(e: &M3, a: (f64, f64), b: (f64, f64)) -> f64 {
    let x1 = [a.0, a.1, 1.0]; let x2 = [b.0, b.1, 1.0]; let ex1 = mv(e, &x1); let etx2 = mv(&tp(e), &x2); let num = dot(&x2, &ex1).powi(2);
    let den = ex1[0] * ex1[0] + ex1[1] * ex1[1] + etx2[0] * etx2[0] + etx2[1] * etx2[1]; if den < 1e-18 { 1e9 } else { num / den }
}
/// Small deterministic PRNG.
pub struct Rng(pub u64);
impl Rng { pub fn next(&mut self) -> u64 { self.0 ^= self.0 << 13; self.0 ^= self.0 >> 7; self.0 ^= self.0 << 17; self.0 }
    pub fn below(&mut self, n: usize) -> usize { (self.next() % n as u64) as usize }
    pub fn pick(&mut self, n: usize, k: usize) -> Vec<usize> { let mut v = vec![]; while v.len() < k.min(n) { let i = self.below(n); if !v.contains(&i) { v.push(i); } } v } }

pub struct TwoView { pub pose: Pose, pub inliers: Vec<usize>, pub pts: Vec<V3> }
/// Relative pose of camera 2 w.r.t. camera 1 (camera 1 = identity). `pairs` are pixel correspondences.
pub fn two_view(pairs: &[((f64, f64), (f64, f64))], k: &Intr, thresh_px: f64, iters: usize, rng: &mut Rng) -> Option<TwoView> {
    let n = pairs.len(); if n < 12 { return None; }
    let np: Vec<_> = pairs.iter().map(|(a, b)| (norm_obs(*a, k), norm_obs(*b, k))).collect(); let th = (thresh_px / k.f).powi(2);
    let mut best: Option<(usize, M3)> = None;
    for _ in 0..iters { let s = rng.pick(n, 8); let sp: Vec<_> = s.iter().map(|&i| np[i]).collect();
        if let Some(e) = essential_8pt(&sp) { let c = np.iter().filter(|(a, b)| sampson(&e, *a, *b) < th).count(); if best.as_ref().map(|b| c > b.0).unwrap_or(true) { best = Some((c, e)); } } }
    let (_, e0) = best?; let inl: Vec<usize> = (0..n).filter(|&i| sampson(&e0, np[i].0, np[i].1) < th).collect(); if inl.len() < 10 { return None; }
    let sp: Vec<_> = inl.iter().map(|&i| np[i]).collect(); let e = essential_8pt(&sp).unwrap_or(e0);
    let (mut u, _s, mut v) = svd3(&e)?; if det3(&u) < 0.0 { for r in 0..3 { u[r][2] = -u[r][2]; } } if det3(&v) < 0.0 { for r in 0..3 { v[r][2] = -v[r][2]; } }
    let w: M3 = [[0.0, -1.0, 0.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0]]; let r1 = mm(&mm(&u, &w), &tp(&v)); let r2 = mm(&mm(&u, &tp(&w)), &tp(&v)); let t = [u[0][2], u[1][2], u[2][2]];
    let mut bestc: Option<(usize, Pose, Vec<usize>, Vec<V3>)> = None;
    for r in [r1, r2] { for sg in [1.0, -1.0] {
        let p2 = Pose { r, t: [t[0] * sg, t[1] * sg, t[2] * sg] }; let p1 = Pose { r: ID3, t: [0.0; 3] }; let mut ok = vec![]; let mut pts = vec![];
        for &i in &inl { let (a, b) = np[i]; if let Some(x) = triangulate(&[(&p1, a), (&p2, b)]) { if x[2] > 0.0 && p2.apply(&x)[2] > 0.0 && reproj_err(&p2, &x, k, pairs[i].1) < thresh_px * 3.0 { ok.push(i); pts.push(x); } } }
        if bestc.as_ref().map(|b| ok.len() > b.0).unwrap_or(true) { bestc = Some((ok.len(), p2, ok, pts)); } } }
    let (c, pose, inliers, pts) = bestc?; if c < 10 { return None; } Some(TwoView { pose, inliers, pts })
}

/// Robust epipolar cost (mean of capped squared Sampson distance in pixels) of a relative pose over all correspondences.
fn epi_cost(pairs: &[((f64, f64), (f64, f64))], k: &Intr, pose: &Pose, cap: f64) -> f64 {
    let t = pose.t; let tx: M3 = [[0.0, -t[2], t[1]], [t[2], 0.0, -t[0]], [-t[1], t[0], 0.0]]; let e = mm(&tx, &pose.r);
    pairs.iter().map(|(a, b)| { let d = sampson(&e, norm_obs(*a, k), norm_obs(*b, k)).sqrt() * k.f; d.min(cap).powi(2) }).sum::<f64>() / pairs.len().max(1) as f64
}
// ---------- PnP ----------
fn pnp_dlt(xs: &[V3], obs: &[(f64, f64)]) -> Option<Pose> {
    if xs.len() < 6 { return None; }
    let rows: Vec<Vec<f64>> = xs.iter().zip(obs).flat_map(|(x, (u, v))| vec![vec![x[0], x[1], x[2], 1.0, 0.0, 0.0, 0.0, 0.0, -u * x[0], -u * x[1], -u * x[2], -u], vec![0.0, 0.0, 0.0, 0.0, x[0], x[1], x[2], 1.0, -v * x[0], -v * x[1], -v * x[2], -v]]).collect();
    let p = null_vec(&rows, 12); let m: M3 = [[p[0], p[1], p[2]], [p[4], p[5], p[6]], [p[8], p[9], p[10]]]; let t = [p[3], p[7], p[11]];
    let (u, s, v) = svd3(&m)?; let mut r = mm(&u, &tp(&v)); let sc = (s[0] + s[1] + s[2]) / 3.0; if sc < 1e-12 { return None; }
    let mut t = [t[0] / sc, t[1] / sc, t[2] / sc]; if det3(&r) < 0.0 { r = [[-r[0][0], -r[0][1], -r[0][2]], [-r[1][0], -r[1][1], -r[1][2]], [-r[2][0], -r[2][1], -r[2][2]]]; t = [-t[0], -t[1], -t[2]]; }
    if p[11] * 0.0 != 0.0 { return None; } Some(Pose { r, t })
}
/// Gauss-Newton refinement of one pose with fixed points (Huber).
pub fn refine_pose(mut pose: Pose, xs: &[V3], obs: &[(f64, f64)], k: &Intr) -> Pose {
    for _ in 0..15 {
        let mut h = [0.0f64; 36]; let mut g = [0.0f64; 6];
        for (x, o) in xs.iter().zip(obs) {
            let c = pose.apply(x); if c[2] < 1e-9 { continue; } let Some((u, v)) = proj(&pose, x, k) else { continue }; let r = [u - o.0, v - o.1]; let rn = (r[0] * r[0] + r[1] * r[1]).sqrt(); let w = if rn < 2.0 { 1.0 } else { 2.0 / rn };
            let iz = 1.0 / c[2]; let (xa, xb) = (c[0] * iz, c[1] * iz); let r2_ = xa * xa + xb * xb; let sc_ = 1.0 + k.k1 * r2_ + k.k2 * r2_ * r2_; let dsc = 2.0 * k.k1 + 4.0 * k.k2 * r2_; let jd = [[sc_ + xa * dsc * xa, xa * dsc * xb], [xb * dsc * xa, sc_ + xb * dsc * xb]];
            let dxn = [[iz, 0.0, -c[0] * iz * iz], [0.0, iz, -c[1] * iz * iz]]; let mut du = [0.0; 3]; let mut dv = [0.0; 3]; for j in 0..3 { du[j] = k.f * (jd[0][0] * dxn[0][j] + jd[0][1] * dxn[1][j]); dv[j] = k.f * (jd[1][0] * dxn[0][j] + jd[1][1] * dxn[1][j]); }
            // d c / d (omega, t): [-[c]x, I]
            let cx = [[0.0, -c[2], c[1]], [c[2], 0.0, -c[0]], [-c[1], c[0], 0.0]]; // [c]x ; dc/dw = -[c]x applied to omega as left-perturbation of R: c' = c + w x c = c - [c]x w
            let mut ju = [0.0; 6]; let mut jv = [0.0; 6];
            for j in 0..3 { let dcj = [-cx[0][j], -cx[1][j], -cx[2][j]]; ju[j] = dot(&du, &dcj); jv[j] = dot(&dv, &dcj); ju[3 + j] = du[j]; jv[3 + j] = dv[j]; }
            for a in 0..6 { g[a] -= w * (ju[a] * r[0] + jv[a] * r[1]); for b in 0..6 { h[a * 6 + b] += w * (ju[a] * ju[b] + jv[a] * jv[b]); } }
        }
        for a in 0..6 { h[a * 6 + a] += 1e-6 * (1.0 + h[a * 6 + a]); }
        if !solve_spd(&mut h, &mut g, 6) { break; }
        let dr = exp_so3(&[g[0], g[1], g[2]]); pose.r = mm(&dr, &pose.r); pose.t = [pose.t[0] + g[3], pose.t[1] + g[4], pose.t[2] + g[5]];
        if g.iter().map(|x| x * x).sum::<f64>().sqrt() < 1e-10 { break; }
    }
    pose
}
pub fn pnp_ransac(xs: &[V3], obs: &[(f64, f64)], k: &Intr, thresh: f64, iters: usize, rng: &mut Rng) -> Option<(Pose, Vec<usize>)> {
    let n = xs.len(); if n < 8 { return None; } let mut best: Option<(usize, Pose)> = None;
    for _ in 0..iters { let s = rng.pick(n, 6); let sx: Vec<V3> = s.iter().map(|&i| xs[i]).collect(); let so: Vec<_> = s.iter().map(|&i| norm_obs(obs[i], k)).collect();
        if let Some(p) = pnp_dlt(&sx, &so) { let c = (0..n).filter(|&i| reproj_err(&p, &xs[i], k, obs[i]) < thresh).count(); if best.as_ref().map(|b| c > b.0).unwrap_or(true) { best = Some((c, p)); } } }
    let (c, mut p) = best?; if c < 8 { return None; }
    for _ in 0..3 { let inl: Vec<usize> = (0..n).filter(|&i| reproj_err(&p, &xs[i], k, obs[i]) < thresh).collect(); if inl.len() < 6 { break; }
        p = refine_pose(p, &inl.iter().map(|&i| xs[i]).collect::<Vec<_>>(), &inl.iter().map(|&i| obs[i]).collect::<Vec<_>>(), k); }
    let inl: Vec<usize> = (0..n).filter(|&i| reproj_err(&p, &xs[i], k, obs[i]) < thresh).collect(); if inl.len() < 8 { return None; } Some((p, inl))
}

// ---------- bundle adjustment ----------
pub struct BaObs { pub cam: usize, pub pt: usize, pub u: f64, pub v: f64 }
fn res_fn(r: &M3, t: &V3, x: &V3, k: &Intr, o: &BaObs) -> [f64; 2] {
    let c = mv(r, x); let (a, b, z) = (c[0] + t[0], c[1] + t[1], (c[2] + t[2]).max(1e-9)); let (xa, xb) = (a / z, b / z); let r2 = xa * xa + xb * xb; let sc = 1.0 + k.k1 * r2 + k.k2 * r2 * r2;
    [k.f * xa * sc + k.cx - o.u, k.f * xb * sc + k.cy - o.v]
}
/// Sparse BA with Schur complement. Camera 0 is held fixed (gauge). Global parameters per Flags: focal, radial k1/k2, principal point.
/// rot_only: translations stay zero (pure-rotation cameras; points act as directions). Returns (initial rms, final rms).
pub fn bundle_adjust(poses: &mut [Pose], pts: &mut [V3], obs: &[BaObs], k: &mut Intr, fl: Flags, iters: usize) -> (f64, f64) {
    let nc = poses.len(); let np = pts.len(); let nfree = nc.saturating_sub(1);
    let mut gl: Vec<u8> = vec![]; if fl.focal { gl.push(0); } if fl.dist { gl.push(1); gl.push(2); } if fl.pp { gl.push(3); gl.push(4); }
    let d = 6 * nfree + gl.len(); let fo = 6 * nfree;
    let rms = |poses: &[Pose], pts: &[V3], k: &Intr| -> f64 { let mut s = 0.0; for o in obs { let r = res_fn(&poses[o.cam].r, &poses[o.cam].t, &pts[o.pt], k, o); s += r[0] * r[0] + r[1] * r[1]; } (s / obs.len().max(1) as f64).sqrt() };
    let cost = |poses: &[Pose], pts: &[V3], k: &Intr| -> f64 { let mut s = 0.0; for o in obs { let r = res_fn(&poses[o.cam].r, &poses[o.cam].t, &pts[o.pt], k, o); let n = (r[0] * r[0] + r[1] * r[1]).sqrt(); s += if n < 2.0 { n * n } else { 4.0 * n - 4.0 }; } s };
    let r0 = rms(poses, pts, k); let mut lambda = 1e-3; let mut cur = cost(poses, pts, k);
    for _ in 0..iters {
        let mut hcc = vec![0.0f64; d * d]; let mut bc = vec![0.0f64; d]; let mut hpp = vec![[[0.0f64; 3]; 3]; np]; let mut bp = vec![[0.0f64; 3]; np]; let mut hcp: Vec<HashMap<usize, [f64; 3]>> = vec![HashMap::new(); np];
        for o in obs {
            let p = &poses[o.cam]; let x = pts[o.pt]; let r = res_fn(&p.r, &p.t, &x, k, o); let rn = (r[0] * r[0] + r[1] * r[1]).sqrt(); let w = if rn < 2.0 { 1.0 } else { 2.0 / rn };
            let c = p.apply(&x); let z = c[2].max(1e-9); let iz = 1.0 / z; let (xa, xb) = (c[0] * iz, c[1] * iz); let r2 = xa * xa + xb * xb; let sc = 1.0 + k.k1 * r2 + k.k2 * r2 * r2; let dsc = 2.0 * k.k1 + 4.0 * k.k2 * r2;
            let jd = [[sc + xa * dsc * xa, xa * dsc * xb], [xb * dsc * xa, sc + xb * dsc * xb]]; let dxn = [[iz, 0.0, -c[0] * iz * iz], [0.0, iz, -c[1] * iz * iz]];
            let mut du = [0.0; 3]; let mut dv = [0.0; 3]; for j in 0..3 { du[j] = k.f * (jd[0][0] * dxn[0][j] + jd[0][1] * dxn[1][j]); dv[j] = k.f * (jd[1][0] * dxn[0][j] + jd[1][1] * dxn[1][j]); }
            let mut jp = [[0.0; 3]; 2]; for j in 0..3 { let dc = [p.r[0][j], p.r[1][j], p.r[2][j]]; jp[0][j] = dot(&du, &dc); jp[1][j] = dot(&dv, &dc); }
            let mut cols: Vec<(usize, [f64; 2])> = vec![];
            if o.cam > 0 { let base = 6 * (o.cam - 1); let cx_ = [[0.0, -c[2], c[1]], [c[2], 0.0, -c[0]], [-c[1], c[0], 0.0]];
                for j in 0..3 { let dcj = [-cx_[0][j], -cx_[1][j], -cx_[2][j]]; cols.push((base + j, [dot(&du, &dcj), dot(&dv, &dcj)])); }
                if !fl.rot_only { for j in 0..3 { cols.push((base + 3 + j, [du[j], dv[j]])); } } }
            for (gi, g) in gl.iter().enumerate() { let col = match g { 0 => [xa * sc, xb * sc], 1 => [k.f * xa * r2, k.f * xb * r2], 2 => [k.f * xa * r2 * r2, k.f * xb * r2 * r2], 3 => [1.0, 0.0], _ => [0.0, 1.0] }; cols.push((fo + gi, col)); }
            for (ra, ja) in &cols { bc[*ra] -= w * (ja[0] * r[0] + ja[1] * r[1]); for (rb, jb) in &cols { hcc[ra * d + rb] += w * (ja[0] * jb[0] + ja[1] * jb[1]); }
                let e = hcp[o.pt].entry(*ra).or_insert([0.0; 3]); for j in 0..3 { e[j] += w * (ja[0] * jp[0][j] + ja[1] * jp[1][j]); } }
            for a in 0..3 { bp[o.pt][a] -= w * (jp[0][a] * r[0] + jp[1][a] * r[1]); for b in 0..3 { hpp[o.pt][a][b] += w * (jp[0][a] * jp[0][b] + jp[1][a] * jp[1][b]); } }
        }
        let mut improved = false;
        for _try in 0..8 {
            let mut s = hcc.clone(); let mut g = bc.clone(); for i in 0..d { s[i * d + i] += lambda * (hcc[i * d + i] + 1e-6); }
            // translation columns in rot_only mode: pin to zero via a stiff prior
            if fl.rot_only { for c in 1..nc { for j in 3..6 { let i = 6 * (c - 1) + j; s[i * d + i] += 1e12; } } }
            let mut pinv: Vec<Option<M3>> = Vec::with_capacity(np);
            for j in 0..np { let mut h = hpp[j]; for a in 0..3 { h[a][a] += lambda * (hpp[j][a][a] + 1e-9) + 1e-12; } let hi = inv3(&h); pinv.push(hi);
                if let Some(hi) = &hi { let rows: Vec<(&usize, &[f64; 3])> = hcp[j].iter().collect();
                    for (ra, ea) in &rows { let t = mv(hi, ea); g[**ra] -= dot(&t, &bp[j]); for (rb, eb) in &rows { s[**ra * d + **rb] -= dot(&t, eb); } } } }
            if d > 0 && !solve_spd(&mut s, &mut g, d) { lambda *= 10.0; continue; }
            let mut np_ = poses.to_vec(); let mut pp = pts.to_vec();
            for c in 1..nc { let b = 6 * (c - 1); let dr = exp_so3(&[g[b], g[b + 1], g[b + 2]]); np_[c].r = mm(&dr, &poses[c].r); if fl.rot_only { np_[c].t = [0.0; 3]; } else { np_[c].t = [poses[c].t[0] + g[b + 3], poses[c].t[1] + g[b + 4], poses[c].t[2] + g[b + 5]]; } }
            let mut k2 = *k; for (gi, gg) in gl.iter().enumerate() { let dv = g[fo + gi]; match gg { 0 => k2.f = (k.f + dv).clamp(k.f * 0.5, k.f * 2.0), 1 => k2.k1 = (k.k1 + dv).clamp(-1.0, 1.0), 2 => k2.k2 = (k.k2 + dv).clamp(-1.0, 1.0), 3 => k2.cx = (k.cx + dv).clamp(k.cx * 0.8, k.cx * 1.2), _ => k2.cy = (k.cy + dv).clamp(k.cy * 0.8, k.cy * 1.2) } }
            for j in 0..np { if let Some(hi) = &pinv[j] { let mut rhs = bp[j]; for (ra, ea) in &hcp[j] { for a in 0..3 { rhs[a] -= ea[a] * g[*ra]; } } let dx = mv(hi, &rhs); pp[j] = [pts[j][0] + dx[0], pts[j][1] + dx[1], pts[j][2] + dx[2]]; } }
            let nc2 = cost(&np_, &pp, &k2);
            if nc2 < cur { let rel = (cur - nc2) / cur.max(1e-12); poses.copy_from_slice(&np_); pts.copy_from_slice(&pp); *k = k2; cur = nc2; lambda = (lambda / 3.0).max(1e-9); improved = true; if rel < 1e-7 { return (r0, rms(poses, pts, k)); } break; }
            lambda *= 4.0;
        }
        if !improved { break; }
    }
    (r0, rms(poses, pts, k))
}

// ---------- solve pipeline ----------
#[derive(Clone, Debug)]
pub struct Track { pub obs: Vec<(usize, f64, f64)> } // (frame index, x, y) pixels
pub struct SolveOpts { pub focal_px: Option<f64>, pub refine_focal: bool, pub refine_dist: bool, pub refine_pp: bool, pub max_keyframes: usize, pub reproj_thresh: f64, pub seed: u64 }
impl Default for SolveOpts { fn default() -> Self { SolveOpts { focal_px: None, refine_focal: true, refine_dist: false, refine_pp: false, max_keyframes: 30, reproj_thresh: 2.0, seed: 7 } } }
pub struct Solution { pub mode: &'static str, pub k1k2: [f64; 2], pub poses: Vec<Option<Pose>>, pub intr: Intr, pub points: Vec<V3>, pub point_track: Vec<usize>, pub rms: f64, pub frame_rms: Vec<f64>, pub registered: usize, pub notes: Vec<String> }

fn obs_at(tr: &Track, frame: usize) -> Option<(f64, f64)> { let i = tr.obs.partition_point(|o| o.0 < frame); tr.obs.get(i).filter(|o| o.0 == frame).map(|o| (o.1, o.2)) }
fn parallax_angle(c1: &V3, c2: &V3, x: &V3) -> f64 { let a = [x[0] - c1[0], x[1] - c1[1], x[2] - c1[2]]; let b = [x[0] - c2[0], x[1] - c2[1], x[2] - c2[2]]; (dot(&a, &b) / (norm(&a) * norm(&b)).max(1e-18)).clamp(-1.0, 1.0).acos().to_degrees() }

pub fn solve(tracks: &[Track], nframes: usize, w: f64, h: f64, o: &SolveOpts) -> Result<Solution, String> {
    if nframes < 3 { return Err("need at least 3 frames".into()); }
    let mut notes = vec![]; let mut rng = Rng(o.seed | 1);
    let mut k = Intr { f: o.focal_px.unwrap_or(w.max(h) * 0.9), cx: w / 2.0, cy: h / 2.0, k1: 0.0, k2: 0.0 };
    let nk = o.max_keyframes.clamp(3, nframes); let kf: Vec<usize> = (0..nk).map(|i| (i * (nframes - 1)) / (nk - 1)).collect::<Vec<_>>(); let mut kf = kf; kf.dedup();
    // ---- init pair: first keyframe vs the candidate with best inliers x parallax
    let pair_data = |a: usize, b: usize| -> Vec<(usize, (f64, f64), (f64, f64))> { tracks.iter().enumerate().filter_map(|(i, t)| Some((i, obs_at(t, a)?, obs_at(t, b)?))).collect() };
    let try_focals: Vec<f64> = match o.focal_px { Some(f) => vec![f], None => (0..14).map(|i| w.max(h) * (0.5 + 0.15 * i as f64)).collect() };
    let mut init: Option<(f64, usize, usize, Intr, TwoView, Vec<usize>)> = None;
    for &b in kf.iter().skip(1) { let a = kf[0]; let pd = pair_data(a, b); if pd.len() < 20 { continue; }
        let par = { let mut v: Vec<f64> = pd.iter().map(|(_, p, q)| ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt()).collect(); v.sort_by(|x, y| x.partial_cmp(y).unwrap()); v[v.len() / 2] };
        if par < 0.01 * w { continue; }
        for &f in &try_focals { let ki = Intr { f, ..k }; let pairs: Vec<_> = pd.iter().map(|(_, p, q)| (*p, *q)).collect();
            if let Some(tv) = two_view(&pairs, &ki, o.reproj_thresh, 200, &mut rng) {
                let p1 = Pose { r: ID3, t: [0.0; 3] }; let mut e: Vec<f64> = tv.inliers.iter().zip(&tv.pts).map(|(&i, x)| reproj_err(&p1, x, &ki, pairs[i].0).max(reproj_err(&tv.pose, x, &ki, pairs[i].1))).collect(); e.sort_by(|x, y| x.partial_cmp(y).unwrap());
                let med = e[e.len() / 2];  let mut ang: Vec<f64> = tv.pts.iter().map(|x| parallax_angle(&[0.0; 3], &tv.pose.center(), x)).collect(); ang.sort_by(|a, b| a.partial_cmp(b).unwrap()); if ang[ang.len() / 2] < 1.5 { continue; } let score = tv.inliers.len() as f64 / (0.2 + med) * (par / (0.05 * w)).min(1.0);
                if init.as_ref().map(|i| score > i.0).unwrap_or(true) { let ids: Vec<usize> = tv.inliers.iter().map(|&i| pd[i].0).collect(); init = Some((score, a, b, ki, tv, ids)); } } } }
    let score = |pairs: &[((f64, f64), (f64, f64))], ki: &Intr, tv: &TwoView| -> f64 { if o.refine_dist { epi_cost(pairs, ki, &tv.pose, o.reproj_thresh * 4.0) } else { let p1 = Pose { r: ID3, t: [0.0; 3] }; let mut e: Vec<f64> = tv.inliers.iter().zip(&tv.pts).map(|(&i, x)| reproj_err(&p1, x, ki, pairs[i].0).max(reproj_err(&tv.pose, x, ki, pairs[i].1))).collect(); e.sort_by(|x, y| x.partial_cmp(y).unwrap()); let m = (e.len() * 8 / 10).max(1); e.iter().take(m).map(|v| v * v).sum::<f64>() / m as f64 * (pairs.len() as f64 / tv.inliers.len().max(1) as f64) } };
    if o.focal_px.is_none() || o.refine_dist { if let Some((_, a, b, ki0, _, _)) = &init { let (a, b, f0) = (*a, *b, ki0.f); let pd = pair_data(a, b); let pairs: Vec<_> = pd.iter().map(|(_, p, q)| (*p, *q)).collect(); let mut best: Option<(f64, f64, f64)> = None;
        let fs: Vec<f64> = if o.focal_px.is_some() { vec![f0] } else { if o.refine_dist { (0..21).map(|i| f0 * (0.8 + 0.02 * i as f64)).collect() } else { (0..21).map(|i| f0 * (0.9 + 0.01 * i as f64)).collect() } }; let ks: Vec<f64> = if o.refine_dist { (0..13).map(|i| -0.3 + 0.05 * i as f64).collect() } else { vec![0.0] };
        for &f in &fs { for &k1 in &ks { let ki = Intr { f, k1, k2: 0.0, ..k }; let mut r2 = Rng(o.seed | 3); if let Some(tv) = two_view(&pairs, &ki, o.reproj_thresh, if o.refine_dist { 120 } else { 150 }, &mut r2) { let sc = score(&pairs, &ki, &tv); if best.map(|b| sc < b.0).unwrap_or(true) { best = Some((sc, f, k1)); } } } }
        if let Some((_, f, k1)) = best { let (mut bf, mut bk) = (f, if o.refine_dist { k1 } else { 0.0 }); let mut bs = f64::MAX; for i in 0..(if o.refine_dist { 11 } else { 0 }) { for j in 0..9 { let ff = if o.focal_px.is_some() { f } else { f * (0.96 + 0.008 * i as f64) }; let kk = if o.refine_dist { k1 - 0.04 + 0.01 * j as f64 } else { 0.0 }; let ki = Intr { f: ff, k1: kk, k2: 0.0, ..k }; let mut r2 = Rng(o.seed | 3); if let Some(tv) = two_view(&pairs, &ki, o.reproj_thresh, 120, &mut r2) { let sc = score(&pairs, &ki, &tv); if sc < bs { bs = sc; bf = ff; bk = kk; } } } }
            let ki = Intr { f: bf, k1: bk, k2: 0.0, ..k }; let mut r2 = Rng(o.seed | 3); if let Some(tv) = two_view(&pairs, &ki, o.reproj_thresh, 300, &mut r2) { let ids: Vec<usize> = tv.inliers.iter().map(|&i| pd[i].0).collect(); init = Some((0.0, a, b, ki, tv, ids)); } } } }
    if let Some((_, a, b, ki0, tv0, _)) = &init { let pd = pair_data(*a, *b); let pairs: Vec<_> = pd.iter().map(|(_, p, q)| (*p, *q)).collect(); let mut rr = Rng(o.seed | 9);
        if let Some((_, ri)) = rot_ransac(&pairs, ki0, o.reproj_thresh * 4.0, &mut rr) { if ri.len() as f64 >= 0.9 * tv0.inliers.len() as f64 { init = None; } } }
    let (_, ia, ib, ki, tv, ids) = init.ok_or("no usable initial pair: not enough parallax or tracks (camera must translate, pure rotation cannot be solved in 3D)")?;
    k = ki; notes.push(format!("init pair frames {ia}-{ib}, {} inliers, focal {:.1}px", ids.len(), k.f));
    // ---- state
    let mut kposes: HashMap<usize, Pose> = HashMap::new(); kposes.insert(ia, Pose { r: ID3, t: [0.0; 3] }); kposes.insert(ib, tv.pose);
    let mut p3: HashMap<usize, V3> = HashMap::new(); for (i, x) in ids.iter().zip(&tv.pts) { p3.insert(*i, *x); }
    // normalise scale: baseline = 1
    let bl = norm(&tv.pose.center()).max(1e-12); for x in p3.values_mut() { *x = [x[0] / bl, x[1] / bl, x[2] / bl]; } { let p = kposes.get_mut(&ib).unwrap(); p.t = [p.t[0] / bl, p.t[1] / bl, p.t[2] / bl]; }
    let ia_pos = kf.iter().position(|&f| f == ia).unwrap(); let ib_pos = kf.iter().position(|&f| f == ib).unwrap();
    let mut order: Vec<usize> = vec![]; { let (mut lo, mut hi) = (ib_pos.min(ia_pos) as isize - 1, ib_pos.max(ia_pos) + 1); let mut up = true; loop { let mut pushed = false; if up && hi < kf.len() { order.push(kf[hi]); hi += 1; pushed = true; } else if !up && lo >= 0 { order.push(kf[lo as usize]); lo -= 1; pushed = true; } up = !up; if !pushed && hi >= kf.len() && lo < 0 { break; } } }
    for &b in kf.iter() { if !kposes.contains_key(&b) && !order.contains(&b) { order.push(b); } }
    let mut since_ba = 0;
    let run_ba = |kposes: &mut HashMap<usize, Pose>, p3: &mut HashMap<usize, V3>, k: &mut Intr, rf: Flags, it: usize, thr: f64, cut: f64| -> (f64, f64) {
        let mut frames: Vec<usize> = kposes.keys().cloned().collect(); frames.sort(); let fidx: HashMap<usize, usize> = frames.iter().enumerate().map(|(i, f)| (*f, i)).collect();
        let mut tids: Vec<usize> = p3.keys().cloned().collect(); tids.sort(); let tidx: HashMap<usize, usize> = tids.iter().enumerate().map(|(i, t)| (*t, i)).collect();
        let mut bo = vec![]; for &t in &tids { for &(f, x, y) in &tracks[t].obs { if let Some(&c) = fidx.get(&f) { if cut > 0.0 && reproj_err(&kposes[&f], &p3[&t], k, (x, y)) > cut { continue; } bo.push(BaObs { cam: c, pt: tidx[&t], u: x, v: y }); } } }
        let mut ps: Vec<Pose> = frames.iter().map(|f| kposes[f]).collect(); let mut xs: Vec<V3> = tids.iter().map(|t| p3[t]).collect();
        let res = bundle_adjust(&mut ps, &mut xs, &bo, k, rf, it);
        for (i, f) in frames.iter().enumerate() { kposes.insert(*f, ps[i]); } for (i, t) in tids.iter().enumerate() { p3.insert(*t, xs[i]); }
        // prune outliers
        let bad: Vec<usize> = tids.iter().cloned().filter(|t| { let x = &p3[t]; let (mut s, mut n) = (0.0, 0); for &(f, u, v) in &tracks[*t].obs { if let Some(p) = kposes.get(&f) { s += reproj_err(p, x, k, (u, v)); n += 1; } } n < 2 || s / n as f64 > thr * 1.5 || kposes.values().any(|p| p.apply(x)[2] <= 0.0) && false }).collect();
        for t in bad { p3.remove(&t); } res };
    run_ba(&mut kposes, &mut p3, &mut k, Flags::default(), 10, o.reproj_thresh, 0.0);
    for &f in &order {
        let mut xs = vec![]; let mut ob = vec![]; for (&t, x) in &p3 { if let Some(p) = obs_at(&tracks[t], f) { xs.push(*x); ob.push(p); } }
        if xs.len() < 12 { notes.push(format!("keyframe {f}: only {} 2D-3D matches, skipped", xs.len())); continue; }
        let Some((pose, _inl)) = pnp_ransac(&xs, &ob, &k, o.reproj_thresh * 2.0, 300, &mut rng) else { notes.push(format!("keyframe {f}: PnP failed")); continue; };
        kposes.insert(f, pose); since_ba += 1;
        // triangulate new tracks seen in f and >=1 other registered keyframe
        let regs: Vec<usize> = kposes.keys().cloned().collect(); let c_f = pose.center();
        for (ti, t) in tracks.iter().enumerate() { if p3.contains_key(&ti) { continue; } if obs_at(t, f).is_none() { continue; }
            let views: Vec<(usize, (f64, f64))> = regs.iter().filter_map(|&r| Some((r, obs_at(t, r)?))).collect(); if views.len() < 2 { continue; }
            let vs: Vec<(&Pose, (f64, f64))> = views.iter().map(|(r, p)| (&kposes[r], norm_obs(*p, &k))).collect();
            if let Some(x) = triangulate(&vs) { let ok = views.iter().all(|(r, p)| { let ps = &kposes[r]; ps.apply(&x)[2] > 0.0 && reproj_err(ps, &x, &k, *p) < o.reproj_thresh * 1.5 });
                let maxang = views.iter().map(|(r, _)| parallax_angle(&c_f, &kposes[r].center(), &x)).fold(0.0, f64::max); if ok && maxang > 1.0 { p3.insert(ti, x); } } }
        if since_ba >= 4 { run_ba(&mut kposes, &mut p3, &mut k, Flags::default(), 8, o.reproj_thresh, 0.0); since_ba = 0; }
    }
    let fl_final = Flags { focal: o.refine_focal, dist: o.refine_dist, pp: o.refine_pp, rot_only: false };
    run_ba(&mut kposes, &mut p3, &mut k, Flags { focal: o.refine_focal, ..Default::default() }, 15, o.reproj_thresh, o.reproj_thresh * 4.0);
    if o.refine_dist { let base = (kposes.clone(), p3.clone(), k); let mut best: Option<(f64, HashMap<usize, Pose>, HashMap<usize, V3>, Intr)> = None;
        for k1 in [0.0, -0.15, 0.15, -0.3, 0.3] { let (mut kp, mut pp, mut kk) = (base.0.clone(), base.1.clone(), base.2); kk.k1 = k1; kk.k2 = 0.0; let (_, r1) = run_ba(&mut kp, &mut pp, &mut kk, fl_final, 50, o.reproj_thresh, 0.0);
            if r1.is_finite() && best.as_ref().map(|b| r1 < b.0).unwrap_or(true) { best = Some((r1, kp, pp, kk)); } }
        if let Some((r1, kp, pp, kk)) = best { notes.push(format!("distortion multi-start picked k1={:.3} (rms {r1:.3}px incl. outliers)", kk.k1)); kposes = kp; p3 = pp; k = kk; } }
    let (r0, r1) = run_ba(&mut kposes, &mut p3, &mut k, fl_final, 80, o.reproj_thresh, o.reproj_thresh * 3.0); notes.push(format!("final BA rms {r0:.3}px -> {r1:.3}px over {} keyframes, {} points, focal {:.1}px", kposes.len(), p3.len(), k.f));
    run_ba(&mut kposes, &mut p3, &mut k, fl_final, 25, o.reproj_thresh, o.reproj_thresh * 3.0);
    // re-scale: baseline between first two registered keyframes to 1
    { let mut fr: Vec<usize> = kposes.keys().cloned().collect(); fr.sort(); if fr.len() >= 2 { let bl = norm(&{ let a = kposes[&fr[0]].center(); let b = kposes[&fr[1]].center(); [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }).max(1e-12); for x in p3.values_mut() { *x = [x[0] / bl, x[1] / bl, x[2] / bl]; } for p in kposes.values_mut() { p.t = [p.t[0] / bl, p.t[1] / bl, p.t[2] / bl]; } } }
    // ---- all frames: PnP against solved points
    let mut poses: Vec<Option<Pose>> = vec![None; nframes];
    for (&f, &p) in &kposes { poses[f] = Some(p); }
    for f in 0..nframes { if poses[f].is_some() { continue; }
        let mut xs = vec![]; let mut ob = vec![]; for (&t, x) in &p3 { if let Some(p) = obs_at(&tracks[t], f) { xs.push(*x); ob.push(p); } }
        if xs.len() < 10 { continue; }
        let guess = { let near = kposes.keys().min_by_key(|&&kf_| (kf_ as isize - f as isize).abs()).cloned().unwrap(); kposes[&near] };
        let g = refine_pose(guess, &xs, &ob, &k); let inl = (0..xs.len()).filter(|&i| reproj_err(&g, &xs[i], &k, ob[i]) < o.reproj_thresh * 2.0).count();
        if inl as f64 >= 0.6 * xs.len() as f64 { poses[f] = Some(g); } else if let Some((p, inl)) = pnp_ransac(&xs, &ob, &k, o.reproj_thresh * 2.0, 200, &mut rng) { let (xi, oi): (Vec<_>, Vec<_>) = inl.iter().map(|&i| (xs[i], ob[i])).unzip(); poses[f] = Some(refine_pose(p, &xi, &oi, &k)); } }
    let ids_sorted: Vec<usize> = { let mut v: Vec<usize> = p3.keys().cloned().collect(); v.sort(); v };
    let points: Vec<V3> = ids_sorted.iter().map(|t| p3[t]).collect();
    let mut frame_rms = vec![f64::NAN; nframes]; let (mut tot, mut cnt) = (0.0, 0usize); let mut out_obs = 0usize;
    for f in 0..nframes { if let Some(p) = &poses[f] { let (mut s, mut n) = (0.0, 0); for (t, x) in ids_sorted.iter().zip(&points) { if let Some(ob) = obs_at(&tracks[*t], f) { let e = reproj_err(p, x, &k, ob); if e < o.reproj_thresh * 3.0 { s += e * e; n += 1; } else { out_obs += 1; } } } if n > 0 { frame_rms[f] = (s / n as f64).sqrt(); tot += s; cnt += n; } } }
    let registered = poses.iter().filter(|p| p.is_some()).count(); notes.push(format!("{registered}/{nframes} frames registered; rms is over inlier observations (<{:.1}px), {} observations rejected as outliers of {}", o.reproj_thresh * 3.0, out_obs, cnt + out_obs));
    Ok(Solution { mode: "full", k1k2: [k.k1, k.k2], poses, intr: k, points, point_track: ids_sorted, rms: if cnt > 0 { (tot / cnt as f64).sqrt() } else { f64::NAN }, frame_rms, registered, notes })
}

/// Convert a solved pose to Kimo camera props (cam_x,cam_y,cam_z px, pan,tilt,roll deg). Kimo: C = pos - (0,0,f), so with world_kimo = solver*scale + (0,0,-f) the solved camera centre maps to pos = C*scale and frame 0 sits at the default camera; R_c2w = Ry(pan) Rx(tilt) Rz(roll).
pub fn to_kimo(p: &Pose, f: f64, scale_px: f64, h: f64) -> ([f64; 3], [f64; 3], f64) {
    let rc = tp(&p.r); let c = p.center(); let tilt = -rc[1][2].clamp(-1.0, 1.0).asin(); let roll = rc[1][0].atan2(rc[1][1]); let pan = rc[0][2].atan2(rc[2][2]);
    ([c[0] * scale_px, c[1] * scale_px, c[2] * scale_px], [pan.to_degrees(), tilt.to_degrees(), roll.to_degrees()], 2.0 * (h / 2.0 / f).atan().to_degrees())
}

// ---------- feature tracking (KLT) ----------
fn bil(img: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let x = x.clamp(0.0, (w - 1) as f32 - 1e-3); let y = y.clamp(0.0, (h - 1) as f32 - 1e-3); let (x0, y0) = (x as usize, y as usize); let (ax, ay) = (x - x0 as f32, y - y0 as f32);
    let i = y0 * w + x0; img[i] * (1.0 - ax) * (1.0 - ay) + img[i + 1] * ax * (1.0 - ay) + img[i + w] * (1.0 - ax) * ay + img[i + w + 1] * ax * ay
}
fn down(img: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) { let (nw, nh) = (w / 2, h / 2); let mut o = vec![0.0; nw * nh]; for y in 0..nh { for x in 0..nw { o[y * nw + x] = (img[2 * y * w + 2 * x] + img[2 * y * w + 2 * x + 1] + img[(2 * y + 1) * w + 2 * x] + img[(2 * y + 1) * w + 2 * x + 1]) * 0.25; } } (o, nw, nh) }
pub struct Pyr { pub lv: Vec<(Vec<f32>, usize, usize)> }
pub fn pyramid(img: &[f32], w: usize, h: usize, n: usize) -> Pyr { let mut lv = vec![(img.to_vec(), w, h)]; for _ in 1..n { let l = lv.last().unwrap(); if l.1 < 32 || l.2 < 32 { break; } let d = down(&l.0, l.1, l.2); lv.push(d); } Pyr { lv } }
/// Shi-Tomasi corners with min-eigenvalue score, grid-limited non-max suppression. `avoid` are existing points.
pub fn detect(img: &[f32], w: usize, h: usize, max: usize, min_dist: f32, avoid: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let r = 3i32; let mut score = vec![0f32; w * h];
    let mut ix = vec![0f32; w * h]; let mut iy = vec![0f32; w * h];
    for y in 1..h - 1 { for x in 1..w - 1 { ix[y * w + x] = (img[y * w + x + 1] - img[y * w + x - 1]) * 0.5; iy[y * w + x] = (img[(y + 1) * w + x] - img[(y - 1) * w + x]) * 0.5; } }
    let m = (r + 1) as usize; if w <= 2 * m + 2 || h <= 2 * m + 2 { return vec![]; }
    for y in m..h - m { for x in m..w - m { let (mut a, mut b, mut c) = (0f32, 0f32, 0f32);
        for dy in -r..=r { for dx in -r..=r { let i = ((y as i32 + dy) as usize) * w + (x as i32 + dx) as usize; a += ix[i] * ix[i]; b += ix[i] * iy[i]; c += iy[i] * iy[i]; } }
        let tr_ = a + c; let det = a * c - b * b; score[y * w + x] = tr_ / 2.0 - ((tr_ * tr_ / 4.0 - det).max(0.0)).sqrt(); } }
    let mut cand: Vec<(f32, usize, usize)> = vec![]; let mx = score.iter().cloned().fold(0f32, f32::max); let th = mx * 0.01;
    for y in m..h - m { for x in m..w - m { let s = score[y * w + x]; if s > th && s >= score[y * w + x - 1] && s >= score[y * w + x + 1] && s >= score[(y - 1) * w + x] && s >= score[(y + 1) * w + x] { cand.push((s, x, y)); } } }
    cand.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let cell = min_dist.max(2.0); let gw = (w as f32 / cell) as usize + 1; let gh = (h as f32 / cell) as usize + 1; let mut grid: Vec<Vec<(f32, f32)>> = vec![vec![]; gw * gh];
    for &(px, py) in avoid { let (gx, gy) = ((px as f32 / cell) as usize, (py as f32 / cell) as usize); if gx < gw && gy < gh { grid[gy * gw + gx].push((px as f32, py as f32)); } }
    let mut out = vec![];
    for (_, x, y) in cand { if out.len() >= max { break; } let (gx, gy) = ((x as f32 / cell) as usize, (y as f32 / cell) as usize); let mut ok = true;
        'g: for yy in gy.saturating_sub(1)..(gy + 2).min(gh) { for xx in gx.saturating_sub(1)..(gx + 2).min(gw) { for &(px, py) in &grid[yy * gw + xx] { if (px - x as f32).powi(2) + (py - y as f32).powi(2) < min_dist * min_dist { ok = false; break 'g; } } } }
        if ok { grid[gy * gw + gx].push((x as f32, y as f32)); out.push((x as f64, y as f64)); } }
    out
}
pub fn lk_level(p: &(Vec<f32>, usize, usize), c: &(Vec<f32>, usize, usize), pt: (f32, f32), mut g: (f32, f32), win: i32) -> Option<(f32, f32, f32)> {
    let (w, h) = (p.1, p.2); let mut tx = vec![]; let mut gxv = vec![]; let mut gyv = vec![]; let (mut a, mut b, mut cc) = (0f32, 0f32, 0f32);
    for dy in -win..=win { for dx in -win..=win { let (x, y) = (pt.0 + dx as f32, pt.1 + dy as f32); let t = bil(&p.0, w, h, x, y); let gx = (bil(&p.0, w, h, x + 1.0, y) - bil(&p.0, w, h, x - 1.0, y)) * 0.5; let gy = (bil(&p.0, w, h, x, y + 1.0) - bil(&p.0, w, h, x, y - 1.0)) * 0.5; tx.push(t); gxv.push(gx); gyv.push(gy); a += gx * gx; b += gx * gy; cc += gy * gy; } }
    let det = a * cc - b * b; if det < 1e-3 * (tx.len() as f32) { return None; } let mut err = 0f32;
    for _ in 0..12 { let (mut ex, mut ey) = (0f32, 0f32); err = 0.0; let mut i = 0;
        for dy in -win..=win { for dx in -win..=win { let d = bil(&c.0, c.1, c.2, pt.0 + dx as f32 + g.0, pt.1 + dy as f32 + g.1) - tx[i]; ex += d * gxv[i]; ey += d * gyv[i]; err += d.abs(); i += 1; } }
        let (sx, sy) = (-(cc * ex - b * ey) / det, -(-b * ex + a * ey) / det); g.0 += sx; g.1 += sy; if sx * sx + sy * sy < 1e-4 { break; } if g.0.abs() > 60.0 || g.1.abs() > 60.0 { return None; } }
    Some((g.0, g.1, err / tx.len() as f32))
}
/// Track points prev->cur with pyramidal LK; returns new position or None. Forward-backward check rejects drift.
pub fn klt(pp: &Pyr, cp: &Pyr, pts: &[(f64, f64)], win: i32) -> Vec<Option<(f64, f64)>> {
    let nl = pp.lv.len().min(cp.lv.len());
    let run = |a: &Pyr, b: &Pyr, p: (f64, f64)| -> Option<(f64, f64, f32)> { let mut g = (0f32, 0f32); let mut e = 0.0; for l in (0..nl).rev() { let s = (1u32 << l) as f32; let pt = (p.0 as f32 / s, p.1 as f32 / s); let r = lk_level(&a.lv[l], &b.lv[l], pt, g, win)?; g = if l > 0 { (r.0 * 2.0, r.1 * 2.0) } else { (r.0, r.1) }; e = r.2; } Some((p.0 + g.0 as f64, p.1 + g.1 as f64, e)) };
    pts.iter().map(|&p| { let (w, h) = (pp.lv[0].1 as f64, pp.lv[0].2 as f64); let (x, y, e) = run(pp, cp, p)?; if x < 4.0 || y < 4.0 || x > w - 5.0 || y > h - 5.0 || e > 18.0 { return None; } let (bx, by, _) = run(cp, pp, (x, y))?; if (bx - p.0).powi(2) + (by - p.1).powi(2) > 1.0 { return None; } Some((x, y)) }).collect()
}
/// Build feature tracks over a sequence of gray frames (each w*h, 0..255).
pub fn build_tracks(frames: &mut dyn FnMut(usize) -> Result<Vec<f32>, String>, nframes: usize, w: usize, h: usize, max_feat: usize) -> Result<Vec<Track>, String> {
    let mut done: Vec<Track> = vec![]; let mut active: Vec<(Track, (f64, f64))> = vec![];
    let mut prev = frames(0)?; let mut prev_p = pyramid(&prev, w, h, 4);
    let md = (w.min(h) as f32 / 40.0).max(8.0);
    for (x, y) in detect(&prev, w, h, max_feat, md, &[]) { active.push((Track { obs: vec![(0, x, y)] }, (x, y))); }
    for f in 1..nframes {
        let cur = frames(f)?; let cur_p = pyramid(&cur, w, h, 4); let pts: Vec<_> = active.iter().map(|a| a.1).collect(); let res = klt(&prev_p, &cur_p, &pts, 7);
        let mut next = vec![]; for ((mut t, _), r) in active.into_iter().zip(res) { match r { Some((x, y)) => { t.obs.push((f, x, y)); next.push((t, (x, y))); } None => done.push(t) } }
        active = next;
        if active.len() < max_feat * 7 / 10 { let av: Vec<_> = active.iter().map(|a| a.1).collect(); for (x, y) in detect(&cur, w, h, max_feat - active.len(), md, &av) { active.push((Track { obs: vec![(f, x, y)] }, (x, y))); } }
        prev = cur; prev_p = cur_p;
    }
    done.extend(active.into_iter().map(|a| a.0)); done.retain(|t| t.obs.len() >= 4); Ok(done)
}

// ---------- rotation-only (pure pan/tilt/roll, tripod shots) ----------
fn bearing(o: (f64, f64), k: &Intr) -> V3 { let n = norm_obs(o, k); let v = [n.0, n.1, 1.0]; let l = norm(&v); [v[0] / l, v[1] / l, v[2] / l] }
/// Rotation R minimising sum |b2 - R b1|^2 (Kabsch / Wahba), optional weights.
fn kabsch(b1: &[V3], b2: &[V3]) -> Option<M3> {
    let mut m = [[0.0; 3]; 3]; for (a, b) in b1.iter().zip(b2) { for i in 0..3 { for j in 0..3 { m[i][j] += b[i] * a[j]; } } }
    let (u, _s, v) = svd3(&m)?; let mut d = ID3; let mut r = mm(&mm(&u, &d), &tp(&v)); if det3(&r) < 0.0 { d[2][2] = -1.0; let mut u2 = u; for i in 0..3 { u2[i][2] = -u2[i][2]; } r = mm(&u2, &tp(&v)); } Some(r)
}
fn rot_ransac(pairs: &[((f64, f64), (f64, f64))], k: &Intr, thr: f64, rng: &mut Rng) -> Option<(M3, Vec<usize>)> {
    let n = pairs.len(); if n < 8 { return None; } let b1: Vec<V3> = pairs.iter().map(|p| bearing(p.0, k)).collect(); let b2: Vec<V3> = pairs.iter().map(|p| bearing(p.1, k)).collect();
    let pose_of = |r: &M3| Pose { r: *r, t: [0.0; 3] }; let err = |r: &M3, i: usize| -> f64 { let x = mv(&tp(r), &b2[i]); let _ = x; reproj_err(&pose_of(r), &b1[i], k, pairs[i].1) };
    let mut best: Option<(usize, M3)> = None;
    for _ in 0..150 { let sidx = rng.pick(n, 3); let s1: Vec<V3> = sidx.iter().map(|&i| b1[i]).collect(); let s2: Vec<V3> = sidx.iter().map(|&i| b2[i]).collect();
        if let Some(r) = kabsch(&s1, &s2) { let c = (0..n).filter(|&i| err(&r, i) < thr).count(); if best.as_ref().map(|b| c > b.0).unwrap_or(true) { best = Some((c, r)); } } }
    let (_, mut r) = best?; for _ in 0..3 { let inl: Vec<usize> = (0..n).filter(|&i| err(&r, i) < thr).collect(); if inl.len() < 6 { break; } if let Some(r2) = kabsch(&inl.iter().map(|&i| b1[i]).collect::<Vec<_>>(), &inl.iter().map(|&i| b2[i]).collect::<Vec<_>>()) { r = r2; } }
    let inl: Vec<usize> = (0..n).filter(|&i| err(&r, i) < thr).collect(); if inl.len() < 8 { None } else { Some((r, inl)) }
}
/// Rotation-only camera solve. Recovers per-frame orientation, focal (and optionally radial distortion / principal point) from tracks;
/// translation and scene depth are NOT recoverable from a nodal pan, so `points` are unit view directions and every camera centre is the origin.
pub fn solve_rotation(tracks: &[Track], nframes: usize, w: f64, h: f64, o: &SolveOpts) -> Result<Solution, String> {
    if nframes < 2 { return Err("need at least 2 frames".into()); } let mut rng = Rng(o.seed | 5); let mut notes = vec!["rotation-only solve: orientation per frame and focal length; no translation or depth exists in a nodal pan".to_string()];
    let nk = o.max_keyframes.clamp(3, nframes); let mut kf: Vec<usize> = (0..nk).map(|i| (i * (nframes - 1)) / (nk - 1)).collect(); kf.dedup();
    let mut k = Intr { f: o.focal_px.unwrap_or(w.max(h) * 0.9), cx: w / 2.0, cy: h / 2.0, k1: 0.0, k2: 0.0 };
    let chain = |k: &Intr, rng: &mut Rng| -> Option<(Vec<M3>, f64)> { let mut rs = vec![ID3]; let mut tot = 0.0; let mut cnt = 0;
        for wnd in kf.windows(2) { let pairs: Vec<_> = tracks.iter().filter_map(|t| Some((obs_at(t, wnd[0])?, obs_at(t, wnd[1])?))).collect(); let (r, inl) = rot_ransac(&pairs, k, o.reproj_thresh, rng)?; rs.push(mm(&r, rs.last().unwrap()));
            let pose = Pose { r, t: [0.0; 3] }; for &i in &inl { let b = bearing(pairs[i].0, k); tot += reproj_err(&pose, &b, k, pairs[i].1); cnt += 1; } }
        Some((rs, tot / cnt.max(1) as f64)) };
    if o.focal_px.is_none() { let mut best: Option<(f64, f64)> = None; for i in 0..24 { let f = w.max(h) * (0.4 + 0.1 * i as f64); let ki = Intr { f, ..k }; let mut r2 = Rng(11); if let Some((_, e)) = chain(&ki, &mut r2) { if best.map(|b| e < b.0).unwrap_or(true) { best = Some((e, f)); } } }
        if let Some((_, f)) = best { let mut b2 = (f64::MAX, f); for i in 0..21 { let ff = f * (0.92 + 0.008 * i as f64); let ki = Intr { f: ff, ..k }; let mut r2 = Rng(11); if let Some((_, e)) = chain(&ki, &mut r2) { if e < b2.0 { b2 = (e, ff); } } } k.f = b2.1; } }
    let (rs, e0) = chain(&k, &mut rng).ok_or("rotation chain failed: too few matching tracks between keyframes")?; notes.push(format!("initial chain mean error {e0:.2}px at focal {:.1}px", k.f));
    // directions for tracks seen in >=2 keyframes
    let mut dirs: HashMap<usize, V3> = HashMap::new(); for (ti, t) in tracks.iter().enumerate() { let mut acc = [0.0; 3]; let mut n = 0; for (j, &f) in kf.iter().enumerate() { if let Some(p) = obs_at(t, f) { let b = mv(&tp(&rs[j]), &bearing(p, &k)); for q in 0..3 { acc[q] += b[q]; } n += 1; } } if n >= 2 { let l = norm(&acc); if l > 1e-9 { dirs.insert(ti, [acc[0] / l * 10.0, acc[1] / l * 10.0, acc[2] / l * 10.0]); } } }
    let mut tids: Vec<usize> = dirs.keys().cloned().collect(); tids.sort(); let tidx: HashMap<usize, usize> = tids.iter().enumerate().map(|(i, t)| (*t, i)).collect();
    let mut ps: Vec<Pose> = rs.iter().map(|r| Pose { r: *r, t: [0.0; 3] }).collect(); let mut xs: Vec<V3> = tids.iter().map(|t| dirs[t]).collect();
    let fidx: HashMap<usize, usize> = kf.iter().enumerate().map(|(i, f)| (*f, i)).collect(); let mut bo = vec![]; for &t in &tids { for &(f, x, y) in &tracks[t].obs { if let Some(&c) = fidx.get(&f) { bo.push(BaObs { cam: c, pt: tidx[&t], u: x, v: y }); } } }
    let fl = Flags { focal: o.refine_focal, dist: o.refine_dist, pp: o.refine_pp, rot_only: true };
    bundle_adjust(&mut ps, &mut xs, &bo, &mut k, Flags { dist: false, pp: false, ..fl }, 15);
    let cut = o.reproj_thresh * 3.0; let bo2: Vec<BaObs> = bo.into_iter().filter(|b| reproj_err(&ps[b.cam], &xs[b.pt], &k, (b.u, b.v)) < cut).collect();
    let (r0, r1) = bundle_adjust(&mut ps, &mut xs, &bo2, &mut k, fl, 60); notes.push(format!("rotation BA rms {r0:.3}px -> {r1:.3}px over {} keyframes, {} track directions, focal {:.1}px, k1 {:.4} k2 {:.4}", ps.len(), xs.len(), k.f, k.k1, k.k2));
    // all frames: orientation from the solved directions (Kabsch, iteratively dropping outliers)
    let dirv: HashMap<usize, V3> = tids.iter().zip(&xs).map(|(t, x)| { let l = norm(x); (*t, [x[0] / l, x[1] / l, x[2] / l]) }).collect();
    let mut poses: Vec<Option<Pose>> = vec![None; nframes]; for (j, &f) in kf.iter().enumerate() { poses[f] = Some(ps[j]); }
    for f in 0..nframes { if poses[f].is_some() { continue; } let mut d1: Vec<V3> = vec![]; let mut b2: Vec<V3> = vec![]; let mut ob = vec![]; for (&t, d) in &dirv { if let Some(p) = obs_at(&tracks[t], f) { d1.push(*d); b2.push(bearing(p, &k)); ob.push(p); } }
        if d1.len() < 8 { continue; } let mut keep: Vec<usize> = (0..d1.len()).collect(); let mut r = ID3;
        for _ in 0..4 { let Some(rr) = kabsch(&keep.iter().map(|&i| d1[i]).collect::<Vec<_>>(), &keep.iter().map(|&i| b2[i]).collect::<Vec<_>>()) else { break }; r = rr; let pose = Pose { r, t: [0.0; 3] }; let nk2: Vec<usize> = (0..d1.len()).filter(|&i| reproj_err(&pose, &d1[i], &k, ob[i]) < cut).collect(); if nk2.len() < 8 { break; } keep = nk2; }
        poses[f] = Some(Pose { r, t: [0.0; 3] }); }
    let ids_sorted = tids.clone(); let points: Vec<V3> = ids_sorted.iter().map(|t| dirv[t]).collect();
    let mut frame_rms = vec![f64::NAN; nframes]; let (mut tot, mut cnt, mut out_obs) = (0.0, 0usize, 0usize);
    for f in 0..nframes { if let Some(p) = &poses[f] { let (mut s2, mut n) = (0.0, 0); for (t, x) in ids_sorted.iter().zip(&points) { if let Some(ob) = obs_at(&tracks[*t], f) { let e = reproj_err(p, x, &k, ob); if e < cut { s2 += e * e; n += 1; } else { out_obs += 1; } } } if n > 0 { frame_rms[f] = (s2 / n as f64).sqrt(); tot += s2; cnt += n; } } }
    let registered = poses.iter().filter(|p| p.is_some()).count(); notes.push(format!("{registered}/{nframes} frames registered; rms over inlier observations (<{cut:.1}px), {out_obs} outliers rejected"));
    Ok(Solution { mode: "rotation_only", k1k2: [k.k1, k.k2], poses, intr: k, points, point_track: ids_sorted, rms: if cnt > 0 { (tot / cnt as f64).sqrt() } else { f64::NAN }, frame_rms, registered, notes })
}
/// Solve with automatic fallback: full 3D when the camera translates, rotation-only otherwise.
pub fn solve_auto(tracks: &[Track], nframes: usize, w: f64, h: f64, o: &SolveOpts, mode: &str) -> Result<Solution, String> {
    match mode { "rotation" => solve_rotation(tracks, nframes, w, h, o), "full" => solve(tracks, nframes, w, h, o),
        _ => match solve(tracks, nframes, w, h, o) { Ok(s) => Ok(s), Err(e) if e.contains("no usable initial pair") => { let mut s = solve_rotation(tracks, nframes, w, h, o).map_err(|e2| format!("{e}; rotation-only fallback also failed: {e2}"))?; s.notes.insert(0, "no usable parallax for a 3D solve; fell back to rotation-only".into()); Ok(s) } Err(e) => Err(e) } }
}
