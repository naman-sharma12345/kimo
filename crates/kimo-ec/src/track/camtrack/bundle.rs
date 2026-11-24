// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed; web_time -> std::time, craft_mime base64 -> crate::base64_encode.
//! Sparse bundle adjustment: Levenberg–Marquardt over cameras and points with the **Schur
//! complement** eliminating the points (Triggs, McLauchlan, Hartley & Fitzgibbon, "Bundle
//! Adjustment — A Modern Synthesis", 2000, §6; H&Z Appendix 6). Residuals are reprojection
//! errors in pixels with a **Huber** robust loss (applied by iteratively reweighted least
//! squares, Triggs et al. §3.3).
//!
//! Cameras are `(R, C, f)`; rotations are updated multiplicatively (`R ← exp([δ]×) R`), focal
//! lengths in log space. The focal length is held fixed, shared by all cameras, or per camera
//! (with a smoothness prior between consecutive cameras). Tripod solves hold the centres fixed.

use rayon::prelude::*;

use super::geometry::Pose;
use super::linalg::{M3, V3, add, cholesky_solve, conjugate_gradient, dot, inv3, mm, mtv, mv, rodrigues, scale, skew, sub};
use super::{Distortion, SolveError};

/// A camera in the adjustment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BaCam {
    pub pose: Pose,
    /// Focal length (pixels).
    pub f: f64,
    /// Lens distortion (shared by all cameras; adjusted when [`BaOpts::distortion`]).
    pub dist: Distortion,
}

/// One observation: camera `cam` sees point `pt` at `uv` (pixels relative to the principal
/// point).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BaObs {
    pub cam: u32,
    pub pt: u32,
    pub uv: [f64; 2],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FocalMode {
    Fixed,
    Shared,
    /// Per camera, with prior weight `k` on the log-focal difference of consecutive cameras.
    PerCamera(f64),
}

#[derive(Clone, Debug)]
pub struct BaOpts {
    pub focal: FocalMode,
    /// Motion only: the points stay fixed.
    pub fix_points: bool,
    /// The camera centres stay fixed (tripod).
    pub fix_centers: bool,
    /// Cameras that stay fixed (gauge), by index; missing entries are free.
    pub fixed: Vec<bool>,
    /// Huber threshold (pixels).
    pub huber: f64,
    /// Maximum LM iterations (at most 1000).
    pub max_iter: usize,
    /// Adjust the shared radial distortion `(k1, k2)` too.
    pub distortion: bool,
}

impl Default for BaOpts {
    fn default() -> Self {
        BaOpts { focal: FocalMode::Fixed, fix_points: false, fix_centers: false, fixed: vec![], huber: 2.0, max_iter: 30, distortion: false }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BaReport {
    pub initial_cost: f64,
    pub cost: f64,
    pub iterations: usize,
}

const BEHIND: f64 = 1e6;

fn huber(e2: f64, k: f64) -> f64 {
    if e2 <= k * k { e2 } else { 2.0 * k * e2.sqrt() - k * k }
}

// Bound the temporary systems before any input-sized allocation. Includes dense matrices
// for the main system, trial copies and both Rayon phases, plus per-point/observation data.
const MAX_BA_BYTES: usize = 256 * 1024 * 1024;
const MAX_BA_CAMERAS: usize = 4096;
const MAX_BA_POINTS: usize = 200_000;
const MAX_BA_OBSERVATIONS: usize = 1_000_000;

fn valid_camera(c: &BaCam) -> bool {
    c.f.is_finite()
        && c.f > 0.0
        && c.pose.r.iter().flatten().chain(&c.pose.c).all(|v| v.is_finite())
        && [c.dist.k1, c.dist.k2, c.dist.radius].iter().all(|v| v.is_finite())
}

fn validate(cams: &[BaCam], pts: &[V3], obs: &[BaObs], o: &BaOpts) -> Result<(), SolveError> {
    if cams.len() > MAX_BA_CAMERAS || pts.len() > MAX_BA_POINTS || obs.len() > MAX_BA_OBSERVATIONS {
        return Err(SolveError("bundle adjustment input exceeds the supported size".into()));
    }
    if !o.huber.is_finite() || o.huber <= 0.0 || o.max_iter > 1000 || matches!(o.focal, FocalMode::PerCamera(k) if !k.is_finite() || k < 0.0) {
        return Err(SolveError("invalid bundle adjustment options".into()));
    }
    if cams.iter().any(|c| !valid_camera(c)) || pts.iter().flatten().any(|v| !v.is_finite()) {
        return Err(SolveError("bundle adjustment cameras and points must be finite, with positive focal lengths".into()));
    }
    for ob in obs {
        if cams.get(ob.cam as usize).is_none() || pts.get(ob.pt as usize).is_none() {
            return Err(SolveError("bundle adjustment observation index is out of range".into()));
        }
        if ob.uv.iter().any(|v| !v.is_finite()) {
            return Err(SolveError("bundle adjustment observations must be finite".into()));
        }
    }
    Ok(())
}

fn matrix_len(n: usize, npts: usize, nobs: usize) -> Result<usize, SolveError> {
    let threads = rayon::current_num_threads();
    let build_workers = if n <= 700 && npts > 64 { threads.clamp(1, 16) } else { 0 };
    let schur_workers = if n <= 1200 && npts >= 64 && threads > 1 { threads.clamp(1, 8) } else { 0 };
    // The two worker phases do not coexist; allow the larger phase plus main/trial buffers.
    let copies = 4 + build_workers.max(schur_workers);
    let len = n.checked_mul(n);
    let bytes = len
        .and_then(|v| v.checked_add(n))
        .and_then(|v| v.checked_mul(size_of::<f64>()))
        .and_then(|v| v.checked_mul(copies))
        .and_then(|v| npts.checked_mul(320).and_then(|p| v.checked_add(p)))
        .and_then(|v| nobs.checked_mul(2 * 9 * size_of::<(usize, V3)>() + size_of::<BaObs>() + 2 * size_of::<u32>()).and_then(|p| v.checked_add(p)));
    match (len, bytes) {
        (Some(len), Some(bytes)) if bytes <= MAX_BA_BYTES => Ok(len),
        _ => Err(SolveError("bundle adjustment exceeds its 256 MiB workspace budget; use fewer cameras or observations".into())),
    }
}

/// Residual (pixels), or `None` for an invalid or behind-camera projection.
pub fn residual(c: &BaCam, x: V3, uv: [f64; 2]) -> Option<[f64; 2]> {
    if !valid_camera(c) || x.iter().chain(&uv).any(|v| !v.is_finite()) {
        return None;
    }
    project_residual(c, x, uv)
}

fn project_residual(c: &BaCam, x: V3, uv: [f64; 2]) -> Option<[f64; 2]> {
    let p = c.pose.to_cam(x);
    if p[2] <= 1e-9 || p.iter().any(|v| !v.is_finite()) {
        return None;
    }
    let q = c.dist.distort([c.f * p[0] / p[2], c.f * p[1] / p[2]]);
    let r = [q[0] - uv[0], q[1] - uv[1]];
    r.iter().all(|v| v.is_finite()).then_some(r)
}

/// Total robust cost. Rejects invalid inputs before starting parallel evaluation.
pub fn cost(cams: &[BaCam], pts: &[V3], obs: &[BaObs], o: &BaOpts) -> Result<f64, SolveError> {
    validate(cams, pts, obs, o)?;
    let c = validated_cost(cams, pts, obs, o);
    if c.is_finite() { Ok(c) } else { Err(SolveError("bundle adjustment cost is not finite".into())) }
}

fn validated_cost(cams: &[BaCam], pts: &[V3], obs: &[BaObs], o: &BaOpts) -> f64 {
    let c: f64 = obs
        .par_iter()
        .map(|ob| match cams.get(ob.cam as usize).zip(pts.get(ob.pt as usize)).and_then(|(c, x)| project_residual(c, *x, ob.uv)) {
            Some(r) => huber(r[0] * r[0] + r[1] * r[1], o.huber),
            None => BEHIND,
        })
        .sum();
    c + prior_cost(cams, o)
}

fn prior_cost(cams: &[BaCam], o: &BaOpts) -> f64 {
    match o.focal {
        FocalMode::PerCamera(k) => cams.windows(2).map(|w| (k * (w[0].f.ln() - w[1].f.ln())).powi(2)).sum(),
        _ => 0.0,
    }
}

/// Parameter layout.
struct Layout {
    /// First pose parameter of each camera (`usize::MAX` when its pose is fixed).
    start: Vec<usize>,
    /// Per-camera focal parameter (`usize::MAX` when none). Gauge-fixed cameras keep a free
    /// focal length.
    fidx: Vec<usize>,
    /// Index of the shared focal parameter.
    shared: Option<usize>,
    /// Index of `k1` (then `k2`) when the distortion is adjusted.
    dist: Option<usize>,
    n: usize,
    rot_only: bool,
}

fn layout(ncam: usize, o: &BaOpts) -> Layout {
    let per_cam_f = matches!(o.focal, FocalMode::PerCamera(_));
    let d = if o.fix_centers { 3 } else { 6 };
    let mut start = vec![usize::MAX; ncam];
    let mut fidx = vec![usize::MAX; ncam];
    let mut n = 0;
    for i in 0..ncam {
        if !o.fixed.get(i).copied().unwrap_or(false) {
            start[i] = n;
            n += d;
        }
        if per_cam_f {
            fidx[i] = n;
            n += 1;
        }
    }
    let shared = matches!(o.focal, FocalMode::Shared).then(|| {
        n += 1;
        n - 1
    });
    let dist = o.distortion.then(|| {
        n += 2;
        n - 2
    });
    Layout { start, fidx, shared, dist, n, rot_only: o.fix_centers }
}

// Six pose parameters, one shared OR per-camera focal parameter, two distortion parameters.
const MAX_CAMERA_JACOBIAN_ENTRIES: usize = 6 + 1 + 2;
type CameraJacobianEntry = (usize, [f64; 2]);

/// Keeps the current parameter set on the stack. Extra parameters spill rather than panic
/// if a future extension grows beyond the inline capacity.
struct CameraJacobian {
    entries: [CameraJacobianEntry; MAX_CAMERA_JACOBIAN_ENTRIES],
    len: usize,
    overflow: Vec<CameraJacobianEntry>,
}

impl CameraJacobian {
    fn new() -> Self {
        Self { entries: [(0, [0.0; 2]); MAX_CAMERA_JACOBIAN_ENTRIES], len: 0, overflow: Vec::new() }
    }

    fn clear(&mut self) {
        self.len = 0;
        self.overflow.clear();
    }

    fn push(&mut self, entry: CameraJacobianEntry) {
        if self.overflow.is_empty() {
            if let Some(slot) = self.entries.get_mut(self.len) {
                *slot = entry;
                self.len += 1;
                return;
            }
            self.overflow.extend_from_slice(&self.entries);
        }
        self.overflow.push(entry);
    }

    fn as_slice(&self) -> &[CameraJacobianEntry] {
        if self.overflow.is_empty() {
            // len only grows after a successful get_mut, so it cannot exceed capacity.
            self.entries.get(..self.len).unwrap_or_default()
        } else {
            &self.overflow
        }
    }
}

/// Camera-side Jacobian entries of one observation: (parameter index, d r / d param).
fn cam_jac(l: &Layout, ci: usize, c: &BaCam, xc: V3, a: &[[f64; 3]; 2], p: [f64; 2], out: &mut CameraJacobian) {
    out.clear();
    let s = l.start[ci];
    if s != usize::MAX {
        // dXc/dδ = −[Xc]×
        let sk = skew(xc);
        for k in 0..3 {
            let g = [-sk[0][k], -sk[1][k], -sk[2][k]];
            out.push((s + k, [dot(a[0], g), dot(a[1], g)]));
        }
        if !l.rot_only {
            // dXc/dC = −R
            for k in 0..3 {
                let g = [-c.pose.r[0][k], -c.pose.r[1][k], -c.pose.r[2][k]];
                out.push((s + 3 + k, [dot(a[0], g), dot(a[1], g)]));
            }
        }
    }
    if l.fidx[ci] != usize::MAX {
        out.push((l.fidx[ci], p));
    }
    if let Some(g) = l.shared {
        out.push((g, p));
    }
}

struct PtSys {
    v: M3,
    bp: V3,
    w: Vec<(usize, V3)>,
}

struct PreconditionerBlock {
    start: usize,
    dim: usize,
    inverse: Vec<f64>,
}

fn camera_preconditioner(u: &[f64], l: &Layout) -> Option<Vec<PreconditionerBlock>> {
    let mut ranges = Vec::new();
    let pose_dim = if l.rot_only { 3 } else { 6 };
    for (&pose, &focal) in l.start.iter().zip(&l.fidx) {
        if pose != usize::MAX {
            ranges.push((pose, pose_dim + usize::from(focal != usize::MAX)));
        } else if focal != usize::MAX {
            ranges.push((focal, 1));
        }
    }
    if let Some(start) = l.shared.or(l.dist) {
        ranges.push((start, l.n - start));
    }
    ranges
        .into_iter()
        .map(|(start, dim)| {
            let mut matrix = Vec::with_capacity(dim * dim);
            for i in 0..dim {
                for j in 0..dim {
                    matrix.push(*u.get((start + i) * l.n + start + j)?);
                }
            }
            let mut inverse = vec![0.0; dim * dim];
            for col in 0..dim {
                let mut a = matrix.clone();
                let mut b = vec![0.0; dim];
                *b.get_mut(col)? = 1.0;
                if !cholesky_solve(&mut a, dim, &mut b) {
                    return None;
                }
                for (row, value) in b.into_iter().enumerate() {
                    inverse[row * dim + col] = value;
                }
            }
            Some(PreconditionerBlock { start, dim, inverse })
        })
        .collect()
}

/// Apply the point-eliminated system without materialising W V^-1 W^T.
fn iterative_schur(u: &[f64], rhs: &[f64], systems: &[PtSys], vinv: &[Option<M3>], l: &Layout) -> Option<Vec<f64>> {
    if l.n == 0 || l.n.checked_mul(l.n) != Some(u.len()) || rhs.len() != l.n || systems.len() != vinv.len() {
        return None;
    }
    let blocks = camera_preconditioner(u, l)?;
    // U contains camera blocks, global parameters and the focal prior. Store its nonzeros
    // once; all subsequent products are linear in these and the point-camera connections.
    let rows: Vec<Vec<(usize, f64)>> =
        u.chunks_exact(l.n).map(|row| row.iter().enumerate().filter(|(_, v)| **v != 0.0).map(|(i, v)| (i, *v)).collect()).collect();
    let mut reduced_rhs = rhs.to_vec();
    for (ps, vi) in systems.iter().zip(vinv) {
        let Some(vi) = vi else { continue };
        let vb = mv(vi, ps.bp);
        for (i, w) in &ps.w {
            *reduced_rhs.get_mut(*i)? -= dot(*w, vb);
        }
    }
    conjugate_gradient(
        &reduced_rhs,
        |x, y| {
            for (i, row) in rows.iter().enumerate() {
                y[i] = row.iter().map(|(j, v)| v * x[*j]).sum();
            }
            for (ps, vi) in systems.iter().zip(vinv) {
                let Some(vi) = vi else { continue };
                let q = ps.w.iter().fold([0.0; 3], |q, (i, w)| add(q, scale(*w, x[*i])));
                let q = mv(vi, q);
                for (i, w) in &ps.w {
                    y[*i] -= dot(*w, q);
                }
            }
        },
        |r, z| {
            z.fill(0.0);
            for block in &blocks {
                for i in 0..block.dim {
                    z[block.start + i] = (0..block.dim).map(|j| block.inverse[i * block.dim + j] * r[block.start + j]).sum();
                }
            }
        },
    )
}

/// Smooth per-camera log focal lengths in the normal equations.
fn add_focal_prior(cams: &[BaCam], o: &BaOpts, l: &Layout, s_mat: &mut [f64], bc: &mut [f64]) {
    let n = l.n;
    // Focal prior.
    if let FocalMode::PerCamera(k) = o.focal {
        for i in 0..cams.len().saturating_sub(1) {
            let r = k * (cams[i].f.ln() - cams[i + 1].f.ln());
            let (a, b) = (l.fidx[i], l.fidx[i + 1]);
            let ia = (a != usize::MAX).then_some(a);
            let ib = (b != usize::MAX).then_some(b);
            for (idx, g) in [(ia, k), (ib, -k)] {
                let Some(x) = idx else { continue };
                bc[x] -= g * r;
                for (idy, h) in [(ia, k), (ib, -k)] {
                    if let Some(y) = idy {
                        s_mat[x * n + y] += g * h;
                    }
                }
            }
        }
    }
}

/// Eliminate the points into the camera system explicitly (`S -= W V^-1 W^T`, `rhs -= W V^-1 bp`).
fn reduce_points(systems: &[PtSys], vinv: &[Option<M3>], n: usize, s: &mut [f64], rhs: &mut [f64]) {
    let npts = systems.len();
    let reduce = |range: std::ops::Range<usize>, s: &mut [f64], rhs: &mut [f64]| {
        for j in range {
            let (Some(vi), ps) = (&vinv[j], &systems[j]) else { continue };
            let vb = mv(vi, ps.bp);
            let wv: Vec<V3> = ps.w.iter().map(|(_, w)| mv(vi, *w)).collect();
            for (a, (ia, wa)) in ps.w.iter().enumerate() {
                rhs[*ia] -= dot(*wa, vb);
                for (ib, wb) in &ps.w[a..] {
                    let d = dot(wv[a], *wb);
                    s[ia * n + ib] -= d;
                    if ia != ib {
                        s[ib * n + ia] -= d;
                    }
                }
            }
        }
    };
    let chunks = rayon::current_num_threads().clamp(1, 8);
    if n <= 1200 && npts >= 64 && chunks > 1 {
        let per = npts.div_ceil(chunks);
        let parts: Vec<(Vec<f64>, Vec<f64>)> = (0..chunks)
            .into_par_iter()
            .map(|t| {
                let (mut ds, mut dr) = (vec![0.0; n * n], vec![0.0; n]);
                reduce(t * per..((t + 1) * per).min(npts), &mut ds, &mut dr);
                (ds, dr)
            })
            .collect();
        for (ds, dr) in parts {
            s.par_iter_mut().zip(ds.par_iter()).for_each(|(x, y)| *x += y);
            for (x, y) in rhs.iter_mut().zip(dr) {
                *x += y;
            }
        }
    } else {
        reduce(0..npts, s, rhs);
    }
}

/// Run Levenberg–Marquardt. `cams` and `pts` are updated in place.
/// Invalid inputs or workspaces exceeding 256 MiB return an error before mutation.
pub fn adjust(cams: &mut [BaCam], pts: &mut [V3], obs: &[BaObs], o: &BaOpts) -> Result<BaReport, SolveError> {
    validate(cams, pts, obs, o)?;
    adjust_impl(cams, pts, obs, o, true)
}

fn adjust_impl(cams: &mut [BaCam], pts: &mut [V3], obs: &[BaObs], o: &BaOpts, allow_iterative: bool) -> Result<BaReport, SolveError> {
    let l = layout(cams.len(), o);
    let matrix_len = matrix_len(l.n, pts.len(), obs.len())?;
    let npts = pts.len();
    // Observations by point.
    let mut by_pt: Vec<Vec<u32>> = vec![vec![]; npts];
    for (i, ob) in obs.iter().enumerate() {
        by_pt[ob.pt as usize].push(i as u32);
    }
    let mut cur = validated_cost(cams, pts, obs, o);
    if !cur.is_finite() {
        return Err(SolveError("bundle adjustment cost is not finite".into()));
    }
    let initial = cur;
    let t0 = std::time::Instant::now();
    let mut tries = 0usize;
    let dbg = std::env::var_os("EC_CAMTRACK_DEBUG").is_some() && l.n > 200;
    let mut lambda = 1e-4;
    let mut iters = 0;
    if l.n == 0 && o.fix_points {
        return Ok(BaReport { initial_cost: initial, cost: cur, iterations: 0 });
    }
    let n = l.n;
    let parallel = n <= 700;
    for _ in 0..o.max_iter {
        iters += 1;
        // ---- Normal equations (Gauss–Newton with IRLS weights).
        // Per point: V (3×3), bp (3), W entries (cam-side index → 3).
        let build_point = |j: usize, s_acc: &mut Vec<f64>, bc: &mut Vec<f64>| -> PtSys {
            let mut ps = PtSys { v: [[0.0; 3]; 3], bp: [0.0; 3], w: vec![] };
            let mut jc = CameraJacobian::new();
            // Observations that repeat a camera add to the same parameters: combine them, so the
            // point's entries stay at most one per parameter (the reduction is quadratic in them).
            let mut cams_seen: Vec<u32> = by_pt[j].iter().map(|&oi| obs[oi as usize].cam).collect();
            cams_seen.sort_unstable();
            let repeated_camera = cams_seen.windows(2).any(|p| p[0] == p[1]);
            // Parameter index → entry in `ps.w`, for points that repeat a camera (a linear search
            // per entry would be quadratic in their observations).
            let mut slot: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
            for &oi in &by_pt[j] {
                let ob = &obs[oi as usize];
                let ci = ob.cam as usize;
                let c = &cams[ci];
                let xc = c.pose.to_cam(pts[j]);
                if xc[2] <= 1e-9 {
                    continue;
                }
                let iz = 1.0 / xc[2];
                let p0 = [c.f * xc[0] * iz, c.f * xc[1] * iz];
                let pd = c.dist.distort(p0);
                let r = [pd[0] - ob.uv[0], pd[1] - ob.uv[1]];
                let e = (r[0] * r[0] + r[1] * r[1]).sqrt();
                let w = if e <= o.huber { 1.0 } else { o.huber / e };
                let a0 = [[c.f * iz, 0.0, -c.f * xc[0] * iz * iz], [0.0, c.f * iz, -c.f * xc[1] * iz * iz]];
                // Through the distortion: d r = D d p.
                let (dj, dk) = c.dist.jacobian(p0);
                let a = [0, 1].map(|i| [0, 1, 2].map(|k| dj[i][0] * a0[0][k] + dj[i][1] * a0[1][k]));
                let p = [dj[0][0] * p0[0] + dj[0][1] * p0[1], dj[1][0] * p0[0] + dj[1][1] * p0[1]];
                cam_jac(&l, ci, c, xc, &a, p, &mut jc);
                if let Some(g) = l.dist {
                    jc.push((g, [dk[0][0], dk[1][0]]));
                    jc.push((g + 1, [dk[0][1], dk[1][1]]));
                }
                let jc = jc.as_slice();
                // Point Jacobian: A R.
                let jx = if o.fix_points {
                    [[0.0; 3]; 2]
                } else {
                    let r0 = mtv(&c.pose.r, a[0]);
                    let r1 = mtv(&c.pose.r, a[1]);
                    [r0, r1]
                };
                for (ia, ja) in jc.iter() {
                    bc[*ia] -= w * (ja[0] * r[0] + ja[1] * r[1]);
                    for (ib, jb) in jc.iter() {
                        s_acc[ia * n + ib] += w * (ja[0] * jb[0] + ja[1] * jb[1]);
                    }
                }
                if !o.fix_points {
                    for a1 in 0..3 {
                        ps.bp[a1] -= w * (jx[0][a1] * r[0] + jx[1][a1] * r[1]);
                        for b1 in 0..3 {
                            ps.v[a1][b1] += w * (jx[0][a1] * jx[0][b1] + jx[1][a1] * jx[1][b1]);
                        }
                    }
                    for (ia, ja) in jc.iter() {
                        let wv =
                            [w * (ja[0] * jx[0][0] + ja[1] * jx[1][0]), w * (ja[0] * jx[0][1] + ja[1] * jx[1][1]), w * (ja[0] * jx[0][2] + ja[1] * jx[1][2])];
                        // Each observation of a point is on another camera: only the shared focal
                        // length repeats.
                        let repeats = Some(*ia) == l.shared || l.dist.is_some_and(|g| *ia == g || *ia == g + 1);
                        if repeated_camera {
                            match slot.get(ia) {
                                Some(&k) => ps.w[k].1 = add(ps.w[k].1, wv),
                                None => {
                                    slot.insert(*ia, ps.w.len());
                                    ps.w.push((*ia, wv));
                                }
                            }
                            continue;
                        }
                        let found = if repeats { ps.w.iter_mut().find(|e| e.0 == *ia) } else { None };
                        match found {
                            Some(e) => e.1 = add(e.1, wv),
                            None => ps.w.push((*ia, wv)),
                        }
                    }
                }
            }
            ps
        };
        let (mut s_mat, mut bc, systems): (Vec<f64>, Vec<f64>, Vec<PtSys>) = if parallel && npts > 64 {
            let chunks = rayon::current_num_threads().clamp(1, 16);
            let per = npts.div_ceil(chunks);
            let parts: Vec<(Vec<f64>, Vec<f64>, Vec<PtSys>)> = (0..chunks)
                .into_par_iter()
                .map(|t| {
                    let (mut s, mut b) = (vec![0.0; matrix_len], vec![0.0; n]);
                    let v = (t * per..((t + 1) * per).min(npts)).map(|j| build_point(j, &mut s, &mut b)).collect();
                    (s, b, v)
                })
                .collect();
            let mut s = vec![0.0; matrix_len];
            let mut b = vec![0.0; n];
            let mut sys = Vec::with_capacity(npts);
            for (ps, pb, pv) in parts {
                for (x, y) in s.iter_mut().zip(ps) {
                    *x += y;
                }
                for (x, y) in b.iter_mut().zip(pb) {
                    *x += y;
                }
                sys.extend(pv);
            }
            (s, b, sys)
        } else {
            let mut s = vec![0.0; matrix_len];
            let mut b = vec![0.0; n];
            let sys = (0..npts).map(|j| build_point(j, &mut s, &mut b)).collect();
            (s, b, sys)
        };
        add_focal_prior(cams, o, &l, &mut s_mat, &mut bc);
        let base_s = s_mat.clone();
        let base_bc = bc.clone();
        let mean_diag = ((0..n).map(|i| base_s[i * n + i]).sum::<f64>() / n.max(1) as f64).max(1e-12);
        let schur_pairs = systems.iter().fold(0usize, |sum, ps| sum.saturating_add(ps.w.len().saturating_mul(ps.w.len().saturating_add(1)) / 2));
        // Reserve the iterative solve for expensive long-track reductions; small systems
        // retain the direct path and its rounding. Failed solves use the direct path too.
        let mut use_iterative = allow_iterative && !o.fix_points && n >= 120 && schur_pairs > 4_000_000;
        // ---- Try steps with increasing damping.
        let mut accepted = false;
        for _try in 0..12 {
            tries += 1;
            let mut s = base_s.clone();
            let mut rhs = base_bc.clone();
            for i in 0..n {
                let d = s[i * n + i];
                s[i * n + i] = d + lambda * d.max(1e-6 * mean_diag);
            }
            // Schur complement over the points: S −= W V⁻¹ Wᵀ, rhs −= W V⁻¹ bp (in parallel
            // chunks of points when the reduced system is small enough to copy per thread).
            let vinv: Vec<Option<M3>> = if o.fix_points {
                vec![None; npts]
            } else {
                systems
                    .par_iter()
                    .map(|ps| {
                        let mut v = ps.v;
                        let vd = (v[0][0] + v[1][1] + v[2][2]) / 3.0;
                        for k in 0..3 {
                            v[k][k] += lambda * v[k][k].max(1e-6 * vd.max(1e-12)) + 1e-12;
                        }
                        inv3(&v)
                    })
                    .collect()
            };
            let iterative_step = if use_iterative { iterative_schur(&s, &rhs, &systems, &vinv, &l) } else { None };
            if dbg && use_iterative {
                eprintln!("ba iterative n={n} pairs={schur_pairs} solved={}", iterative_step.is_some());
            }
            if use_iterative && iterative_step.is_none() {
                use_iterative = false;
            }
            let dc = if let Some(dc) = iterative_step {
                dc
            } else {
                if !o.fix_points {
                    reduce_points(&systems, &vinv, n, &mut s, &mut rhs);
                }
                let mut dc = rhs;
                if n > 0 && !cholesky_solve(&mut s, n, &mut dc) {
                    lambda *= 10.0;
                    continue;
                }
                dc
            };
            // Apply.
            let mut nc: Vec<BaCam> = cams.to_vec();
            for (i, c) in nc.iter_mut().enumerate() {
                let st = l.start[i];
                if st != usize::MAX {
                    let dr = [dc[st], dc[st + 1], dc[st + 2]];
                    c.pose.r = mm(&rodrigues(dr), &c.pose.r);
                    if !l.rot_only {
                        c.pose.c = add(c.pose.c, [dc[st + 3], dc[st + 4], dc[st + 5]]);
                    }
                }
                if l.fidx[i] != usize::MAX {
                    c.f *= dc[l.fidx[i]].clamp(-0.5, 0.5).exp();
                }
                if let Some(g) = l.shared {
                    c.f *= dc[g].clamp(-0.5, 0.5).exp();
                }
                if let Some(g) = l.dist {
                    c.dist.k1 += dc[g].clamp(-0.2, 0.2);
                    c.dist.k2 += dc[g + 1].clamp(-0.2, 0.2);
                }
            }
            let mut np: Vec<V3> = pts.to_vec();
            if !o.fix_points {
                np.par_iter_mut().enumerate().for_each(|(j, x)| {
                    let Some(vi) = &vinv[j] else { return };
                    let ps = &systems[j];
                    let mut b = ps.bp;
                    for (ia, w) in &ps.w {
                        b = sub(b, scale(*w, dc[*ia]));
                    }
                    *x = add(*x, mv(vi, b));
                });
            }
            if nc.iter().any(|c| !valid_camera(c)) || np.iter().flatten().any(|v| !v.is_finite()) {
                lambda *= 4.0;
                continue;
            }
            let c2 = validated_cost(&nc, &np, obs, o);
            if c2.is_finite() && c2 < cur {
                cams.copy_from_slice(&nc);
                pts.copy_from_slice(&np);
                let gain = cur - c2;
                cur = c2;
                lambda = (lambda / 3.0).max(1e-9);
                accepted = gain > 1e-6 * cur.max(1e-12);
                if !accepted {
                    // Converged (tiny improvement).
                    if dbg {
                        eprintln!(
                            "ba n={} pts={} obs={} iters={iters} tries={tries} {:.2}s cost {initial:.1} -> {cur:.1}",
                            l.n,
                            npts,
                            obs.len(),
                            t0.elapsed().as_secs_f64()
                        );
                    }
                    return Ok(BaReport { initial_cost: initial, cost: cur, iterations: iters });
                }
                break;
            }
            lambda *= 4.0;
        }
        if !accepted {
            break;
        }
    }
    if dbg {
        eprintln!("ba n={} pts={} obs={} iters={iters} tries={tries} {:.2}s cost {initial:.1} -> {cur:.1}", l.n, npts, obs.len(), t0.elapsed().as_secs_f64());
    }
    Ok(BaReport { initial_cost: initial, cost: cur, iterations: iters })
}


