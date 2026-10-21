//! Planar (homography) tracker + perspective-correct corner-pin compositing.
//! Tracker: Gauss-Newton on the 4 corner positions (8 params) with a zero-mean/unit-variance photometric model (so exposure changes and
//! flicker do not matter), Tukey-robust per-pixel weights (occluders are rejected), 3-level pyramid, sub-pixel by construction.
//! Drift control: every frame is aligned to the previous frame first, then re-aligned to the ANCHOR template from frame 0; the anchor result wins
//! whenever its confidence is good, so error does not accumulate. Occlusion: when both confidences collapse the track coasts on damped velocity and
//! tries to re-acquire the anchor with a coarse translation search.
use crate::model::*;
use crate::effects::Frame;

pub type Quad = [[f64; 2]; 4];
type Img = (Vec<f32>, usize, usize);

/// Homography solving for dst = H(src), 4 point pairs. Returns h[0..8] (h8 = 1).
pub fn solve_h(s: &Quad, d: &Quad) -> Option<[f64; 8]> {
    let mut a = [[0f64; 9]; 8];
    for i in 0..4 { let (x, y) = (s[i][0], s[i][1]); let (u, v) = (d[i][0], d[i][1]);
        a[i * 2] = [x, y, 1.0, 0.0, 0.0, 0.0, -u * x, -u * y, u]; a[i * 2 + 1] = [0.0, 0.0, 0.0, x, y, 1.0, -v * x, -v * y, v]; }
    for c in 0..8 { let mut m = c; for r in c + 1..8 { if a[r][c].abs() > a[m][c].abs() { m = r; } } a.swap(c, m); let dv = a[c][c]; if dv.abs() < 1e-12 { return None; }
        for k in c..9 { a[c][k] /= dv; } for r in 0..8 { if r != c { let f = a[r][c]; for k in c..9 { a[r][k] -= f * a[c][k]; } } } }
    let mut h = [0f64; 8]; for i in 0..8 { h[i] = a[i][8]; } Some(h)
}
/// Map a quad's corners through the homography taking `from` to `to` (context propagation).
pub fn propagate_quad(from: &Quad, to: &Quad, target: &Quad) -> Option<Quad> { let h = solve_h(from, to)?; let mut o = *target; for i in 0..4 { let (x, y) = ap(&h, target[i][0], target[i][1]); o[i] = [x, y]; } Some(o) }
#[inline] fn ap(h: &[f64; 8], x: f64, y: f64) -> (f64, f64) { let w = h[6] * x + h[7] * y + 1.0; ((h[0] * x + h[1] * y + h[2]) / w, (h[3] * x + h[4] * y + h[5]) / w) }
const UNIT: Quad = [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];

fn down(s: &Img) -> Img { let (w, h) = (s.1, s.2); let (nw, nh) = ((w / 2).max(1), (h / 2).max(1)); let mut o = vec![0f32; nw * nh];
    for y in 0..nh { for x in 0..nw { let (x0, y0) = ((x * 2).min(w - 1), (y * 2).min(h - 1)); let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1)); o[y * nw + x] = 0.25 * (s.0[y0 * w + x0] + s.0[y0 * w + x1] + s.0[y1 * w + x0] + s.0[y1 * w + x1]); } } (o, nw, nh) }
fn pyramid(g: Vec<f32>, w: usize, h: usize, n: usize) -> Vec<Img> { let mut v = vec![(g, w, h)]; for _ in 1..n { let d = down(v.last().unwrap()); v.push(d); } v }
#[inline] fn bil(im: &Img, x: f64, y: f64) -> Option<f32> { let (w, h) = (im.1, im.2); if x < 0.0 || y < 0.0 || x > w as f64 - 1.001 || y > h as f64 - 1.001 { return None; }
    let (x0, y0) = (x as usize, y as usize); let (fx, fy) = ((x - x0 as f64) as f32, (y - y0 as f64) as f32); let d = &im.0;
    let (a, b, c, e) = (d[y0 * w + x0], d[y0 * w + x0 + 1], d[(y0 + 1) * w + x0], d[(y0 + 1) * w + x0 + 1]); Some(a + (b - a) * fx + (c - a) * fy + (a - b - c + e) * fx * fy) }

pub struct Tmpl { n: Vec<(usize, usize)>, v: Vec<Vec<f32>>, us: Vec<Vec<(f64, f64)>> }
fn quad_scale(q: &Quad, s: f64) -> Quad { let mut o = *q; for p in o.iter_mut() { p[0] *= s; p[1] *= s; } o }
fn make_tmpl(pyr: &[Img], q: &Quad) -> Tmpl { let mut t = Tmpl { n: vec![], v: vec![], us: vec![] };
    for (l, im) in pyr.iter().enumerate() { let s = 1.0 / (1u32 << l) as f64; let ql = quad_scale(q, s); let h = solve_h(&UNIT, &ql).unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        let wd = ((ql[1][0] - ql[0][0]).hypot(ql[1][1] - ql[0][1])).max((ql[2][0] - ql[3][0]).hypot(ql[2][1] - ql[3][1])); let ht = ((ql[3][0] - ql[0][0]).hypot(ql[3][1] - ql[0][1])).max((ql[2][0] - ql[1][0]).hypot(ql[2][1] - ql[1][1]));
        let (nx, ny) = (((wd / 1.5) as usize).clamp(10, 110), ((ht / 1.5) as usize).clamp(10, 110)); let mut vals = vec![]; let mut us = vec![];
        for j in 0..ny { for i in 0..nx { let (u, v) = ((i as f64 + 0.5) / nx as f64, (j as f64 + 0.5) / ny as f64); let (x, y) = ap(&h, u, v); vals.push(bil(im, x, y).unwrap_or(f32::NAN)); us.push((u, v)); } }
        t.n.push((nx, ny)); t.v.push(vals); t.us.push(us); } t }

pub struct Fit { pub q: Quad, pub conf: f64, pub gain: f64 }
fn tukey(r: f32, c: f32) -> f32 { let a = r.abs() / c; if a >= 1.0 { 0.0 } else { let t = 1.0 - a * a; t * t } }

/// Align template level data to image pyramid, coarse to fine, starting from quad `q0` (full-res pixel coords).
/// Floor for the similarity-lock blend (0 = free homography when visible, 1 = rigid+scale only). Set per track by the command.
pub static SHAPE_LOCK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub static SCALE_DAMP: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub fn align(pyr: &[Img], t: &Tmpl, q0: &Quad, iters: &[usize], prior: Option<&Quad>) -> Fit {
    let nl = pyr.len().min(t.n.len()); let mut q = *q0; let mut conf = 0.0; let mut gain = 1.0;
    for l in (0..nl).rev() { let s = 1.0 / (1u32 << l) as f64; let mut ql = quad_scale(&q, s); let im = &pyr[l]; let tv = &t.v[l]; let us = &t.us[l];
        let valid: Vec<usize> = (0..tv.len()).filter(|&k| tv[k].is_finite()).collect(); if valid.len() < 30 { continue; }
        let mt = valid.iter().map(|&k| tv[k]).sum::<f32>() / valid.len() as f32; let sdt = (valid.iter().map(|&k| (tv[k] - mt).powi(2)).sum::<f32>() / valid.len() as f32).sqrt().max(1e-4);
        let mut lam = 1e-3f64;
        for it in 0..iters[l.min(iters.len() - 1)] {
            let Some(h0) = solve_h(&UNIT, &ql) else { break };
            let mut hp: Vec<[f64; 8]> = vec![]; for j in 0..8 { let mut qq = ql; qq[j / 2][j % 2] += 0.5; hp.push(solve_h(&UNIT, &qq).unwrap_or(h0)); }
            let mut ii = vec![0f32; valid.len()]; let mut jac = vec![[0f32; 8]; valid.len()]; let mut ok = vec![false; valid.len()];
            for (n, &k) in valid.iter().enumerate() { let (u, v) = us[k]; let (x, y) = ap(&h0, u, v);
                let (Some(i0), Some(ixp), Some(ixm), Some(iyp), Some(iym)) = (bil(im, x, y), bil(im, x + 1.0, y), bil(im, x - 1.0, y), bil(im, x, y + 1.0), bil(im, x, y - 1.0)) else { continue };
                let (gx, gy) = ((ixp - ixm) * 0.5, (iyp - iym) * 0.5); ii[n] = i0; ok[n] = true;
                for j in 0..8 { let (xj, yj) = ap(&hp[j], u, v); jac[n][j] = (gx * ((xj - x) * 2.0) as f32 + gy * ((yj - y) * 2.0) as f32) as f32; } }
            let idx: Vec<usize> = (0..valid.len()).filter(|&n| ok[n]).collect(); if idx.len() < 30 { break; }
            let m = idx.iter().map(|&n| ii[n]).sum::<f32>() / idx.len() as f32; let sd = (idx.iter().map(|&n| (ii[n] - m).powi(2)).sum::<f32>() / idx.len() as f32).sqrt().max(1e-4);
            let res: Vec<f32> = idx.iter().map(|&n| (ii[n] - m) / sd - (tv[valid[n]] - mt) / sdt).collect();
            let mut ar: Vec<f32> = res.iter().map(|r| r.abs()).collect(); ar.sort_by(|a, b| a.partial_cmp(b).unwrap()); let mad = ar[ar.len() / 2].max(0.02); let cst = (mad * 1.4826 * 4.685).max(0.15);
            let ws: Vec<f32> = res.iter().map(|&r| tukey(r, cst)).collect(); let wsum: f32 = ws.iter().sum::<f32>().max(1e-6);
            conf = (1.0 - 0.5 * res.iter().zip(&ws).map(|(r, w)| w * r * r).sum::<f32>() / wsum) as f64; let vis = idx.len() as f64 / valid.len() as f64; let inl = ws.iter().filter(|&&w| w > 0.5).count() as f64 / idx.len() as f64; conf = conf.max(0.0) * inl.min(1.0).sqrt() * (0.5 + vis).min(1.0);
            gain = (m / mt.max(1e-3)) as f64;
            let mut a = [[0f64; 8]; 8]; let mut b = [0f64; 8];
            for (e, &n) in idx.iter().enumerate() { let w = ws[e] as f64; if w <= 0.0 { continue; } let j = &jac[n].map(|x| (x / sd) as f64); for p in 0..8 { b[p] -= w * j[p] * res[e] as f64; for qn in 0..8 { a[p][qn] += w * j[p] * j[qn]; } } }
            if let Some(pq) = prior { let tr: f64 = (0..8).map(|p| a[p][p]).sum::<f64>() / 8.0; let st = tr * 0.4 * (1.0 - vis).powi(3) + 1e-6; let pl = quad_scale(pq, s); for p in 0..8 { a[p][p] += st; b[p] -= st * (ql[p / 2][p % 2] - pl[p / 2][p % 2]); } }
            for p in 0..8 { a[p][p] += lam * (a[p][p] + 1e-3); }
            // solve 8x8
            let mut mtx = [[0f64; 9]; 8]; for p in 0..8 { for qn in 0..8 { mtx[p][qn] = a[p][qn]; } mtx[p][8] = b[p]; }
            let mut sing = false; for c in 0..8 { let mut mx = c; for r in c + 1..8 { if mtx[r][c].abs() > mtx[mx][c].abs() { mx = r; } } mtx.swap(c, mx); let d = mtx[c][c]; if d.abs() < 1e-12 { sing = true; break; } for k in c..9 { mtx[c][k] /= d; } for r in 0..8 { if r != c { let f = mtx[r][c]; for k in c..9 { mtx[r][k] -= f * mtx[c][k]; } } } }
            if sing { break; }
            let mut dl = [[0f64; 2]; 4]; for p in 0..8 { dl[p / 2][p % 2] = mtx[p][8].clamp(-3.0, 3.0); }
            // occlusion-aware shape constraint: when much of the template is outvoted (an occluder covers it), only the rigid-plus-scale part of the update is trusted,
            // so the quad cannot shrink or shear into the visible strip. beta 0 = free homography (fully visible), 1 = similarity only.
            let beta = { let x = ((0.9 - inl * vis.min(1.0).sqrt()) / 0.35).clamp(0.0, 1.0); x * x * (3.0 - 2.0 * x) };
            let beta = beta.max(f32::from_bits(SHAPE_LOCK.load(std::sync::atomic::Ordering::Relaxed)) as f64);
            if beta > 0.0 { let cen = |q: &Quad| [(q[0][0] + q[1][0] + q[2][0] + q[3][0]) / 4.0, (q[0][1] + q[1][1] + q[2][1] + q[3][1]) / 4.0]; let c0 = cen(&ql); let mut tgt = ql; for k in 0..4 { tgt[k][0] += dl[k][0]; tgt[k][1] += dl[k][1]; } let c1 = cen(&tgt);
                let (mut sa, mut sb, mut sn) = (0.0, 0.0, 0.0); for k in 0..4 { let (px, py) = (ql[k][0] - c0[0], ql[k][1] - c0[1]); let (qx, qy) = (tgt[k][0] - c1[0], tgt[k][1] - c1[1]); sa += px * qx + py * qy; sb += px * qy - py * qx; sn += px * px + py * py; }
                let (ca, cb) = (sa / sn.max(1e-9), sb / sn.max(1e-9));
                let damp = f32::from_bits(SCALE_DAMP.load(std::sync::atomic::Ordering::Relaxed)) as f64; let (ca, cb) = if damp > 0.0 { let sc = ca.hypot(cb).max(1e-9); let an = cb.atan2(ca); let sc2 = 1.0 + (sc - 1.0) * (1.0 - damp); (sc2 * an.cos(), sc2 * an.sin()) } else { (ca, cb) }; // best similarity: [ca -cb; cb ca] about the centroid
                for k in 0..4 { let (px, py) = (ql[k][0] - c0[0], ql[k][1] - c0[1]); let sim = [c1[0] + ca * px - cb * py - ql[k][0], c1[1] + cb * px + ca * py - ql[k][1]]; for a in 0..2 { dl[k][a] = beta * sim[a] + (1.0 - beta) * dl[k][a]; } } }
            let mut mx = 0f64; for k in 0..4 { for a in 0..2 { ql[k][a] += dl[k][a]; mx = mx.max(dl[k][a].abs()); } }
            if mx < 0.005 && it > 0 { break; } lam *= 0.7;
        }
        q = quad_scale(&ql, (1u32 << l) as f64);
    }
    Fit { q, conf, gain }
}

/// ZNCC of template (finest level) at quad, for the coarse re-acquisition search.
fn ncc_at(im: &Img, t: &Tmpl, l: usize, ql: &Quad) -> f64 { let Some(h) = solve_h(&UNIT, ql) else { return -1.0 }; let (tv, us) = (&t.v[l], &t.us[l]); let (mut a, mut b) = (vec![], vec![]);
    for k in (0..tv.len()).step_by(2) { if !tv[k].is_finite() { continue; } let (x, y) = ap(&h, us[k].0, us[k].1); if let Some(v) = bil(im, x, y) { a.push(v as f64); b.push(tv[k] as f64); } }
    if a.len() < 30 { return -1.0; } let n = a.len() as f64; let (ma, mb) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n); let (mut sab, mut saa, mut sbb) = (0.0, 1e-9, 1e-9);
    for i in 0..a.len() { sab += (a[i] - ma) * (b[i] - mb); saa += (a[i] - ma).powi(2); sbb += (b[i] - mb).powi(2); } sab / (saa * sbb).sqrt() * (a.len() as f64 / (tv.len() / 2) as f64).min(1.0) }

/// Sanity gate: convex, non-degenerate, and no sudden area/shape jump relative to the previous quad.
fn quad_valid(prev: &Quad, q: &Quad) -> bool {
    let cr = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| (b[0] - a[0]) * (c[1] - b[1]) - (b[1] - a[1]) * (c[0] - b[0]); let s0 = cr(q[0], q[1], q[2]).signum();
    for i in 0..4 { let v = cr(q[i], q[(i + 1) % 4], q[(i + 2) % 4]); if !v.is_finite() || v.signum() != s0 || v.abs() < 1.0 { return false; } }
    let area = |q: &Quad| 0.5 * ((q[0][0] * q[1][1] - q[1][0] * q[0][1]) + (q[1][0] * q[2][1] - q[2][0] * q[1][1]) + (q[2][0] * q[3][1] - q[3][0] * q[2][1]) + (q[3][0] * q[0][1] - q[0][0] * q[3][1])).abs();
    let r = area(q) / area(prev).max(1.0); if r < 0.6 || r > 1.7 { return false; }
    let sz = (prev[1][0] - prev[0][0]).hypot(prev[1][1] - prev[0][1]).max((prev[3][0] - prev[0][0]).hypot(prev[3][1] - prev[0][1])).max(20.0);
    (0..4).all(|k| (q[k][0] - prev[k][0]).hypot(q[k][1] - prev[k][1]) < 0.35 * sz.max(40.0))
}

// ---- plane-pose motion model (engine core for edge-on / off-frame prediction) ----------------------------------------------------------
type V3 = [f64; 3];
fn v_sub(a: V3, b: V3) -> V3 { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn v_add(a: V3, b: V3) -> V3 { [a[0] + b[0], a[1] + b[1], a[2] + b[2]] }
fn v_mul(a: V3, k: f64) -> V3 { [a[0] * k, a[1] * k, a[2] * k] }
fn v_dot(a: V3, b: V3) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn v_cross(a: V3, b: V3) -> V3 { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
fn v_norm(a: V3) -> f64 { v_dot(a, a).sqrt() }
fn v_unit(a: V3) -> V3 { let n = v_norm(a).max(1e-12); v_mul(a, 1.0 / n) }
/// Plane pose from a quad under an assumed pinhole focal `f` (principal point at the frame centre): rotation columns (e1,e2,e3), translation, plane size (a,b) in depth units.
struct Pose { r: [V3; 3], t: V3, a: f64, b: f64 }
fn pose_from_quad(q: &Quad, f: f64, cx: f64, cy: f64) -> Option<Pose> { let h = solve_h(&UNIT, q)?;
    let kin = |c: usize| -> V3 { let (x, y, z) = (h[c], h[3 + c], if c == 0 { h[6] } else { h[7] }); [(x - cx * z) / f, (y - cy * z) / f, z] };
    let (m1, m2) = (kin(0), kin(1)); let m3 = [(h[2] - cx) / f, (h[5] - cy) / f, 1.0];
    let (a, b) = (v_norm(m1), v_norm(m2)); if a < 1e-9 || b < 1e-9 { return None; }
    let (mut e1, mut e2) = (v_unit(m1), v_unit(m2)); let e3 = v_unit(v_cross(e1, e2)); e2 = v_unit(v_cross(e3, e1)); e1 = v_unit(v_cross(e2, e3)); let mut t = m3; let mut e3 = e3;
    if t[2] < 0.0 { t = v_mul(t, -1.0); e1 = v_mul(e1, -1.0); e2 = v_mul(e2, -1.0); e3 = v_unit(v_cross(e1, e2)); }
    Some(Pose { r: [e1, e2, e3], t: v_mul(t, 1.0 / a), a: 1.0, b: b / a }) }
fn mat_mul_t(a: &[V3; 3], b: &[V3; 3]) -> [V3; 3] { // returns A * B^T where matrices are stored as columns: result columns c_j = A * (row j of B)... computed as R = A * B^-1
    let bt = |i: usize, j: usize| b[i][j]; let mut o = [[0f64; 3]; 3]; for c in 0..3 { for r in 0..3 { o[c][r] = (0..3).map(|k| a[k][r] * bt(k, c)).sum(); } } o }
fn rot_axis(m: &[V3; 3]) -> V3 { let tr = m[0][0] + m[1][1] + m[2][2]; let ang = ((tr - 1.0) * 0.5).clamp(-1.0, 1.0).acos(); let ax = [m[1][2] - m[2][1], m[2][0] - m[0][2], m[0][1] - m[1][0]]; let n = v_norm(ax); if n < 1e-9 { return [0.0; 3]; } v_mul(ax, ang / n) }
fn rot_from(w: V3) -> [V3; 3] { let ang = v_norm(w); if ang < 1e-12 { return [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]; } let k = v_mul(w, 1.0 / ang); let (c, s) = (ang.cos(), ang.sin()); let mut o = [[0f64; 3]; 3];
    for j in 0..3 { let mut e = [0.0; 3]; e[j] = 1.0; let r = v_add(v_add(v_mul(e, c), v_mul(v_cross(k, e), s)), v_mul(k, v_dot(k, e) * (1.0 - c))); o[j] = r; } o }
fn rot_apply(r: &[V3; 3], v: V3) -> V3 { v_add(v_add(v_mul(r[0], v[0]), v_mul(r[1], v[1])), v_mul(r[2], v[2])) }
/// Predict the next quad from the last accepted quads by constant-velocity extrapolation in plane-pose space (rotation vector + translation), which stays physically consistent as the plane goes edge-on or leaves the frame.
pub fn pose_predict(hist: &[Quad], w: usize, h: usize) -> Option<Quad> {
    if hist.len() < 3 { return None; } let (f, cx, cy) = (std::env::var("KIMO_F").ok().and_then(|v| v.parse::<f64>().ok()).unwrap_or(0.9) * w as f64, w as f64 / 2.0, h as f64 / 2.0); let n = hist.len().min(5); let ps: Vec<Pose> = hist[hist.len() - n..].iter().filter_map(|q| pose_from_quad(q, f, cx, cy)).collect(); if ps.len() < 3 { return None; }
    let mut wv = [0f64; 3]; let mut tv = [0f64; 3]; let m = ps.len() - 1; for i in 1..ps.len() { let dr = mat_mul_t(&ps[i].r, &ps[i - 1].r); let ww = rot_axis(&dr); wv = v_add(wv, ww); tv = v_add(tv, v_sub(ps[i].t, ps[i - 1].t)); }
    let (wv, tv) = (v_mul(wv, 1.0 / m as f64), v_mul(tv, 1.0 / m as f64)); let last = &ps[ps.len() - 1]; let rn = { let d = rot_from(wv); let mut o = [[0f64; 3]; 3]; for j in 0..3 { o[j] = rot_apply(&d, last.r[j]); } o }; let tn = v_add(last.t, tv);
    let mut q = [[0f64; 2]; 4]; for (k, (u, v)) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].iter().enumerate() { let pt = v_add(v_add(v_mul(rn[0], last.a * u), v_mul(rn[1], last.b * v)), tn); if pt[2] < 0.02 { return None; } q[k] = [f * pt[0] / pt[2] + cx, f * pt[1] / pt[2] + cy]; if !q[k][0].is_finite() || !q[k][1].is_finite() || q[k][0].abs() > 1e5 || q[k][1].abs() > 1e5 { return None; } }
    Some(q) }

pub struct Track { pub quads: Vec<Quad>, pub conf: Vec<f64>, pub gain: Vec<f64>, pub lost: Vec<bool> }

/// Track the quad `q0` (pixel coords) through frames `[f0, f1]` of a media clip.
pub fn track(p: &Project, clip_id: &str, q0: Quad, f0: usize, f1: usize, levels: usize) -> Result<Track, String> { track_opts(p, clip_id, q0, f0, f1, levels, false) }
/// `reanchor`: segment mode. When the frame-to-frame fit is confident but the original anchor no longer matches (lighting or appearance change), the anchor is refreshed from the current frame so the track restarts a new drift-free segment instead of dropping.
pub fn track_opts(p: &Project, clip_id: &str, q0: Quad, f0: usize, f1: usize, levels: usize, reanchor: bool) -> Result<Track, String> { let frames: Vec<usize> = (f0..=f1).collect(); track_frames(p, clip_id, q0, &frames, levels, reanchor) }
/// Same tracker over an explicit frame order (frames[0] is the anchor frame where `q0` is given); descending order tracks backward in time.
pub fn track_frames(p: &Project, clip_id: &str, q0: Quad, frames: &[usize], levels: usize, reanchor: bool) -> Result<Track, String> {
    if frames.is_empty() { return Err("no frames".into()); } let f0 = frames[0];
    let c = p.find_clip(clip_id).ok_or("no such clip")?.clone(); let mut cache = crate::render::Cache::new(); let (w, h) = (p.width as usize, p.height as usize);
    let levels = levels.clamp(2, 5); let iters = [6usize, 8, 10, 12, 12];
    let get = |cache: &mut crate::render::Cache, i: usize| -> Result<Vec<Img>, String> { let g = crate::tracking::clip_gray_pub(p, cache, &c, i)?; Ok(pyramid(g, w, h, levels)) };
    let p0 = get(&mut cache, f0)?; let mut anchor = make_tmpl(&p0, &q0);
    let mut out = Track { quads: vec![q0], conf: vec![1.0], gain: vec![1.0], lost: vec![false] };
    let mut prev_pyr = p0; let (mut q, mut vel) = (q0, [[0f64; 2]; 4]); let mut lost = false; let mut prevq: Option<Quad> = None; let mut hist: Vec<f64> = vec![]; let mut qh: Vec<Quad> = vec![q0];
    for &i in &frames[1..] {
        let cur = get(&mut cache, i)?; let mut pred = q; for k in 0..4 { pred[k][0] += vel[k][0]; pred[k][1] += vel[k][1]; }
        if let (Some(pq), false) = (prevq, lost) { if let Some(g) = solve_h(&pq, &q) { let mut hp = q; let mut okh = true; for k in 0..4 { let (x, y) = ap(&g, q[k][0], q[k][1]); if !x.is_finite() || !y.is_finite() || (x - q[k][0]).abs() > 400.0 || (y - q[k][1]).abs() > 400.0 { okh = false; } hp[k] = [x, y]; } if okh { pred = hp; } } }
        let pose_pred = pose_predict(&qh, w, h).filter(|pp| { let szp = (q[1][0] - q[0][0]).hypot(q[1][1] - q[0][1]).max((q[3][0] - q[0][0]).hypot(q[3][1] - q[0][1])).max(40.0); (0..4).all(|k| (pp[k][0] - pred[k][0]).hypot(pp[k][1] - pred[k][1]) < 1.5 * szp) });
        let recent = make_tmpl(&prev_pyr, &q);
        let mut best = Fit { q: pred, conf: 0.0, gain: 1.0 };
        if !lost { let fr = align(&cur, &recent, &pred, &iters, Some(&pred)); best = fr; }
        // anchor refinement (drift-free); from lost state do a coarse search first
        let start = if lost || best.conf < 0.45 { let l = (levels - 1).min(2); let s = 1.0 / (1u32 << l) as f64; let r = (60.0 * s) as i32; let (mut bs, mut bd) = (-2.0, (0.0, 0.0));
            let base = quad_scale(&pred, s); for dy in (-r..=r).step_by(2.max((r / 8) as usize)) { for dx in (-r..=r).step_by(2.max((r / 8) as usize)) { let mut qq = base; for k in 0..4 { qq[k][0] += dx as f64; qq[k][1] += dy as f64; }
                let sc = ncc_at(&cur[l], &anchor, l, &qq); if sc > bs { bs = sc; bd = (dx as f64, dy as f64); } } }
            let mut qq = pred; for k in 0..4 { qq[k][0] += bd.0 / s; qq[k][1] += bd.1 / s; } qq } else { best.q };
        let fa = align(&cur, &anchor, &start, &iters, Some(&pred));
        let szq = (q[1][0] - q[0][0]).hypot(q[1][1] - q[0][1]).max((q[3][0] - q[0][0]).hypot(q[3][1] - q[0][1])).max(40.0);
        let near = |fq: &Quad| (0..4).map(|k| (fq[k][0] - pred[k][0]).hypot(fq[k][1] - pred[k][1])).fold(0.0, f64::max) < 0.06 * szq + 12.0;
        let mc = if hist.is_empty() { 0.0 } else { hist.iter().sum::<f64>() / hist.len() as f64 };
        let fa_ok = fa.conf >= 0.55 && quad_valid(&q, &fa.q) && (fa.conf >= 0.8 * mc || (near(&fa.q) && fa.conf >= 0.68 * mc)); let be_ok = !lost && best.conf >= 0.5 && quad_valid(&q, &best.q) && (best.conf >= 0.8 * mc || (near(&best.q) && best.conf >= 0.68 * mc));
        if reanchor && !lost && !fa_ok && best.conf >= 0.85 && quad_valid(&q, &best.q) { anchor = make_tmpl(&cur, &best.q); }
        let (chosen, ok) = if fa_ok { (fa, true) } else if be_ok { (best, true) } else { (Fit { q: pred, conf: fa.conf.max(best.conf), gain: 1.0 }, false) };
        if ok { let nv: Quad = { let mut v = [[0f64; 2]; 4]; for k in 0..4 { v[k] = [chosen.q[k][0] - q[k][0], chosen.q[k][1] - q[k][1]]; } v }; for k in 0..4 { vel[k] = [0.6 * nv[k][0] + 0.4 * vel[k][0], 0.6 * nv[k][1] + 0.4 * vel[k][1]]; } prevq = Some(q); q = chosen.q; qh.push(q); if qh.len() > 8 { qh.remove(0); } lost = false; hist.push(chosen.conf); if hist.len() > 6 { hist.remove(0); } }
        else { for k in 0..4 { vel[k] = [vel[k][0] * 0.85, vel[k][1] * 0.85]; } q = if lost { pose_pred.unwrap_or(pred) } else { pred }; qh.push(q); if qh.len() > 8 { qh.remove(0); } lost = true; }
        out.quads.push(q); out.conf.push(chosen.conf); out.gain.push(if ok { chosen.gain } else { *out.gain.last().unwrap() }); out.lost.push(lost); prev_pyr = cur;
    }
    Ok(out)
}

/// Destination quad of a pinned clip at clip-local time (absolute pixels). None when the clip is not pinned.
pub fn pin_quad(c: &Clip, lt: f64) -> Option<Quad> { if c.props.get("pin_on").map(|p| p.at(lt)).unwrap_or(0.0) < 0.5 { return None; } let mut q = [[0f64; 2]; 4];
    for i in 0..4 { q[i] = [c.props.get(&format!("pin{i}x"))?.at(lt), c.props.get(&format!("pin{i}y"))?.at(lt)]; } Some(q) }

/// Perspective-correct pin of the whole layer onto `to`, 3x3 supersampled: anti-aliased edges, minification-safe, premultiplied interpolation.
pub fn pin_frame(src: &Frame, to: &Quad, gain: f32) -> Frame { let mut o = Frame::new(src.w, src.h); pin_rect(src, [0.0, 0.0, src.w as f64, src.h as f64], to, gain, &mut o); o }

/// Pin the source rectangle `rect` = [x0,y0,x1,y1] (pixels) onto quad `to`, compositing premultiplied over `o`.
pub fn pin_rect(src: &Frame, rect: [f64; 4], to: &Quad, gain: f32, o: &mut Frame) {
    let (w, h) = (src.w, src.h); let from: Quad = [[rect[0], rect[1]], [rect[2], rect[1]], [rect[2], rect[3]], [rect[0], rect[3]]];
    let Some(hm) = solve_h(to, &from) else { return };
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); for p in to { x0 = x0.min(p[0]); y0 = y0.min(p[1]); x1 = x1.max(p[0]); y1 = y1.max(p[1]); }
    if x1 < 0.0 || y1 < 0.0 || x0 > w as f64 || y0 > h as f64 { return; }
    let (bx0, by0, bx1, by1) = ((x0.floor().max(0.0)) as usize, (y0.floor().max(0.0)) as usize, (x1.ceil().min(w as f64 - 1.0)) as usize, (y1.ceil().min(h as f64 - 1.0)) as usize);
    let area = |q: &Quad| 0.5 * ((q[0][0] * q[1][1] - q[1][0] * q[0][1]) + (q[1][0] * q[2][1] - q[2][0] * q[1][1]) + (q[2][0] * q[3][1] - q[3][0] * q[2][1]) + (q[3][0] * q[0][1] - q[0][0] * q[3][1])).abs();
    let ratio = (area(&from) / area(to).max(1.0)).sqrt(); let ss_edge = (ratio.ceil() as usize + 1).clamp(3, 6); let ss_in = if ratio > 1.3 { (ratio.ceil() as usize).clamp(2, 5) } else { 1 };
    let (rw, rh) = (rect[2] - rect[0], rect[3] - rect[1]);
    for y in by0..=by1.min(h - 1) { for x in bx0..=bx1.min(w - 1) { let mut acc = [0f32; 4];
        let (cu, cv) = ap(&hm, x as f64 + 0.5, y as f64 + 0.5); let mg = 2.5 * ratio.max(1.0) + 1.0; let interior = cu > rect[0] + mg && cv > rect[1] + mg && cu < rect[2] - mg && cv < rect[3] - mg; let ss = if interior { ss_in } else { ss_edge }; let inv = 1.0 / (ss * ss) as f32;
        for sy in 0..ss { for sx in 0..ss { let (xf, yf) = (x as f64 + (sx as f64 + 0.5) / ss as f64, y as f64 + (sy as f64 + 0.5) / ss as f64); let (u, v) = ap(&hm, xf, yf);
            if u < rect[0] || v < rect[1] || u > rect[2] || v > rect[3] { continue; } let (uu, vv) = ((u - 0.5).clamp(0.0, w as f64 - 1.001), (v - 0.5).clamp(0.0, h as f64 - 1.001)); let (ix, iy) = (uu as usize, vv as usize); let (fx, fy) = ((uu - ix as f64) as f32, (vv - iy as f64) as f32);
            let g = |xx: usize, yy: usize| -> [f32; 4] { let i = (yy * w + xx) * 4; let a = src.d[i + 3]; [src.d[i] * a, src.d[i + 1] * a, src.d[i + 2] * a, a] };
            let (a, b, c, d) = (g(ix, iy), g((ix + 1).min(w - 1), iy), g(ix, (iy + 1).min(h - 1)), g((ix + 1).min(w - 1), (iy + 1).min(h - 1)));
            for j in 0..4 { acc[j] += a[j] * (1.0 - fx) * (1.0 - fy) + b[j] * fx * (1.0 - fy) + c[j] * (1.0 - fx) * fy + d[j] * fx * fy; } } }
        let a = acc[3] * inv; let i = (y * w + x) * 4; if a > 1e-6 { let (nr, ng, nb) = ((acc[0] * inv * gain).min(4.0 * a), (acc[1] * inv * gain).min(4.0 * a), (acc[2] * inv * gain).min(4.0 * a));
            let (or, og, ob, oa) = (o.d[i] * o.d[i + 3], o.d[i + 1] * o.d[i + 3], o.d[i + 2] * o.d[i + 3], o.d[i + 3]); let na = a + oa * (1.0 - a); let k = 1.0 / na.max(1e-6);
            o.d[i] = (nr + or * (1.0 - a)) * k; o.d[i + 1] = (ng + og * (1.0 - a)) * k; o.d[i + 2] = (nb + ob * (1.0 - a)) * k; o.d[i + 3] = na; } } }
    let _ = (rw, rh);
}

/// Mesh pin: the layer canvas is cut into (nx-1)*(ny-1) cells, each pinned onto its own tracked quad.
pub fn pin_mesh(src: &Frame, verts: &[[f64; 2]], nx: usize, ny: usize, gain: f32) -> Frame {
    let mut o = Frame::new(src.w, src.h); let (w, h) = (src.w as f64, src.h as f64);
    for j in 0..ny - 1 { for i in 0..nx - 1 { let rect = [w * i as f64 / (nx - 1) as f64, h * j as f64 / (ny - 1) as f64, w * (i + 1) as f64 / (nx - 1) as f64, h * (j + 1) as f64 / (ny - 1) as f64];
        let mut q = [verts[j * nx + i], verts[j * nx + i + 1], verts[(j + 1) * nx + i + 1], verts[(j + 1) * nx + i]]; let c = [(q[0][0] + q[1][0] + q[2][0] + q[3][0]) / 4.0, (q[0][1] + q[1][1] + q[2][1] + q[3][1]) / 4.0];
        for p in q.iter_mut() { let (dx, dy) = (p[0] - c[0], p[1] - c[1]); let l = dx.hypot(dy).max(1e-6); p[0] += dx / l * 0.7; p[1] += dy / l * 0.7; }
        let mut rr = rect; rr[0] -= 0.0; rr[1] -= 0.0; pin_rect(src, rr, &q, gain, &mut o); } } o
}
pub fn mesh_verts(c: &Clip, lt: f64) -> Option<(usize, usize, Vec<[f64; 2]>)> { let nx = c.props.get("mesh_nx")?.base as usize; let ny = c.props.get("mesh_ny")?.base as usize; if nx < 2 || ny < 2 { return None; }
    let mut v = Vec::with_capacity(nx * ny); for k in 0..nx * ny { v.push([c.props.get(&format!("mx{k}"))?.at(lt), c.props.get(&format!("my{k}"))?.at(lt)]); } Some((nx, ny, v)) }

/// Render a pinned layer with optional motion blur (shutter in degrees) by averaging pinned samples across the shutter interval.
pub fn pin_render(c: &Clip, f: &Frame, lt: f64, fps: f64) -> Option<Frame> {
    let is_mesh = c.props.get("pin_on").map(|p| p.at(lt)).unwrap_or(0.0) >= 0.5 && c.props.contains_key("mesh_nx"); if !is_mesh { pin_quad(c, lt)?; } else if mesh_verts(c, lt).is_none() { return None; }
    let light = c.props.get("pin_light").map(|p| p.at(lt)).unwrap_or(0.0) as f32; let n = if c.motion_blur > 0.0 { ((c.motion_blur / 22.5).ceil() as usize).clamp(2, 16) } else { 1 }; let sh = c.motion_blur / 360.0 / fps;
    let gain_at = |t: f64| -> f32 { 1.0 + (c.props.get("pin_gain").map(|p| p.at(t)).unwrap_or(1.0) as f32 - 1.0) * light };
    let one = |t: f64| -> Option<Frame> { if is_mesh { let (nx, ny, v) = mesh_verts(c, t)?; Some(pin_mesh(f, &v, nx, ny, gain_at(t))) } else { Some(pin_frame(f, &pin_quad(c, t)?, gain_at(t))) } };
    // opt-in exact shutter (set_motion_blur samples / shutter_phase / shutter_curve): sample count from corner / mesh-vertex travel (px, x2 like the mask auto); None keeps the legacy times
    // cw = non-box shutter_curve weights (sum 1): rgb = sum(w*rgb*a)/sum(w*a), alpha = sum(w*a); None (legacy or box) keeps the plain sums and alpha / n
    let mut cw: Option<Vec<f32>> = None;
    let ts: Vec<f64> = match crate::shutter::layer_shutter(c, fps, lt) { Some(s) => { let m = if s.samples > 0 { s.samples.clamp(2, 256) } else {
            let pts = |t: f64| -> Vec<[f64; 2]> { if is_mesh { mesh_verts(c, t).map(|v| v.2).unwrap_or_default() } else { pin_quad(c, t).map(|q| q.to_vec()).unwrap_or_default() } };
            let w3 = [lt + s.open, lt + s.open + s.len * 0.5, lt + s.open + s.len].map(pts); let seg = |a: &Vec<[f64; 2]>, b: &Vec<[f64; 2]>| a.iter().zip(b).map(|(p, q)| (q[0] - p[0]).hypot(q[1] - p[1])).fold(0.0, f64::max);
            crate::shutter::auto_samples(2.0 * seg(&w3[0], &w3[1]).max(seg(&w3[1], &w3[2]))) };
            if m <= 1 { return one(lt + s.open + s.len * 0.5); } if s.curve != crate::shutter::Curve::Box { cw = Some(crate::shutter::weighted_midpoints(s.open, s.len, m, &s.curve).into_iter().map(|x| x.1 as f32).collect()); }
            crate::shutter::midpoints(s.open, s.len, m).into_iter().map(|o| lt + o).collect() }
        None => { if n == 1 { return one(lt); } (0..n).map(|k| lt + ((k as f64 + 0.5) / n as f64 - 0.5) * sh).collect() } }; let n = ts.len();
    let mut acc = Frame::new(f.w, f.h); for (ki, &t2) in ts.iter().enumerate() { let Some(g) = one(t2) else { continue }; let cwk = cw.as_ref().map(|v| v[ki]);
        for i in 0..f.w * f.h { let a = g.d[i * 4 + 3]; if a > 0.0 { if let Some(wk) = cwk { let wa = wk * a; for j in 0..3 { acc.d[i * 4 + j] += g.d[i * 4 + j] * wa; } acc.d[i * 4 + 3] += wa; } else { for j in 0..3 { acc.d[i * 4 + j] += g.d[i * 4 + j] * a; } acc.d[i * 4 + 3] += a; } } } }
    for i in 0..f.w * f.h { let a = acc.d[i * 4 + 3]; if a > 1e-6 { for j in 0..3 { acc.d[i * 4 + j] /= a; } if cw.is_none() { acc.d[i * 4 + 3] = a / n as f32; } } } Some(acc)
}

/// Track a nx*ny mesh inside quad `q0`: global homography track, then per-vertex local refinement against the frame-f0 patch warped by the local Jacobian, neighbour-regularised.
pub fn track_mesh(p: &Project, clip_id: &str, q0: Quad, nx: usize, ny: usize, f0: usize, f1: usize, init: Option<Vec<(f64, f64)>>) -> Result<(Vec<Vec<[f64; 2]>>, Vec<f64>), String> {
    let g = track(p, clip_id, q0, f0, f1, 3)?; let c = p.find_clip(clip_id).ok_or("no such clip")?.clone(); let mut cache = crate::render::Cache::new(); let (w, h) = (p.width as usize, p.height as usize);
    let hq0 = solve_h(&UNIT, &q0).ok_or("degenerate quad")?; let v0: Vec<(f64, f64)> = init.filter(|v| v.len() == nx * ny).unwrap_or_else(|| (0..nx * ny).map(|k| ap(&hq0, (k % nx) as f64 / (nx - 1) as f64, (k / nx) as f64 / (ny - 1) as f64)).collect());
    let ref_img: Img = { let gr = crate::tracking::clip_gray_pub(p, &mut cache, &c, f0)?; (gr, w, h) }; let r = 14i32;
    let mut out = vec![]; let mut conf = vec![]; let pyr = |im: &Img| -> Vec<Img> { let mut v = vec![im.clone()]; for _ in 0..2 { let a = v.last().unwrap(); let (w2, h2) = (a.1 / 2, a.2 / 2); let mut o = vec![0f32; w2 * h2]; for y in 0..h2 { for x in 0..w2 { o[y * w2 + x] = (a.0[2 * y * a.1 + 2 * x] + a.0[2 * y * a.1 + 2 * x + 1] + a.0[(2 * y + 1) * a.1 + 2 * x] + a.0[(2 * y + 1) * a.1 + 2 * x + 1]) * 0.25; } } v.push((o, w2, h2)); } v }; let rp = pyr(&ref_img);
    let mut prev = v0.clone(); let mut prev2 = v0.clone();
    for (fi, qt) in g.quads.iter().enumerate() { let cur_img: Img = if fi == 0 { ref_img.clone() } else { (crate::tracking::clip_gray_pub(p, &mut cache, &c, f0 + fi)?, w, h) };
        let cp = pyr(&cur_img);
        let rel = if fi == 0 || std::env::var("KIMO_REL").is_err() { [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0] } else { solve_h(&g.quads[fi - 1], qt).unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]) };
        let base: Vec<(f64, f64)> = prev.iter().zip(prev2.iter()).map(|(v, v2)| { let a = ap(&rel, v.0, v.1); (a.0 + (v.0 - v2.0) * 0.85, a.1 + (v.1 - v2.1) * 0.85) }).collect(); let mut d = vec![(0f64, 0f64); nx * ny]; let mut cf = vec![0f64; nx * ny];
        if fi > 0 { for k in 0..nx * ny { let (vx, vy) = v0[k]; let (bx, by) = base[k]; let (i, j) = (k % nx, k / nx);
            let (il, ir, jl, jr) = (i.saturating_sub(1), (i + 1).min(nx - 1), j.saturating_sub(1), (j + 1).min(ny - 1));
            let (rx, ry) = ((v0[j * nx + ir].0 - v0[j * nx + il].0, v0[j * nx + ir].1 - v0[j * nx + il].1), (v0[jr * nx + i].0 - v0[jl * nx + i].0, v0[jr * nx + i].1 - v0[jl * nx + i].1));
            let (ex, ey) = ((base[j * nx + ir].0 - base[j * nx + il].0, base[j * nx + ir].1 - base[j * nx + il].1), (base[jr * nx + i].0 - base[jl * nx + i].0, base[jr * nx + i].1 - base[jl * nx + i].1));
            let dr = rx.0 * ry.1 - ry.0 * rx.1; if dr.abs() < 1e-6 { continue; } let (i00, i01, i10, i11) = (ry.1 / dr, -ry.0 / dr, -rx.1 / dr, rx.0 / dr);
            let jac = [[ex.0 * i00 + ey.0 * i10, ex.0 * i01 + ey.0 * i11], [ex.1 * i00 + ey.1 * i10, ex.1 * i01 + ey.1 * i11]];
            let (mut tot, mut cfk) = ((0f64, 0f64), 0f64);
            for lvl in (0..3usize).rev() { let sc = 1.0 / (1u32 << lvl) as f64; let (rimg, cimg) = (&rp[lvl], &cp[lvl]); let rr = if lvl == 0 { r } else { 9 };
                let mut tp = vec![]; for dy in -rr..=rr { for dx in -rr..=rr { if let Some(v) = bil(rimg, vx * sc + dx as f64, vy * sc + dy as f64) { tp.push((dx as f64, dy as f64, v)); } } } if tp.len() < 80 { continue; }
                let mt = tp.iter().map(|t| t.2).sum::<f32>() / tp.len() as f32; let st = (tp.iter().map(|t| (t.2 - mt).powi(2)).sum::<f32>() / tp.len() as f32).sqrt().max(1e-3); let (mut ox, mut oy) = (0f64, 0f64);
                for _ in 0..14 { let (mut a11, mut a12, mut a22, mut b1, mut b2, mut sw) = (0f64, 0f64, 0f64, 0f64, 0f64, 0f64); let mut vals = vec![];
                    for t in &tp { let px = (bx + tot.0) * sc + ox + jac[0][0] * t.0 + jac[0][1] * t.1; let py = (by + tot.1) * sc + oy + jac[1][0] * t.0 + jac[1][1] * t.1; match (bil(cimg, px, py), bil(cimg, px + 1.0, py), bil(cimg, px - 1.0, py), bil(cimg, px, py + 1.0), bil(cimg, px, py - 1.0)) { (Some(i0), Some(a), Some(b), Some(c2), Some(d2)) => vals.push((i0, (a - b) * 0.5, (c2 - d2) * 0.5, t.2)), _ => {} } }
                    if vals.len() < 50 { break; } let mi = vals.iter().map(|v| v.0).sum::<f32>() / vals.len() as f32; let si = (vals.iter().map(|v| (v.0 - mi).powi(2)).sum::<f32>() / vals.len() as f32).sqrt().max(1e-3);
                    let mut ssum = 0f64; for v in &vals { let res = ((v.0 - mi) / si - (v.3 - mt) / st) as f64; let wgt = if res.abs() < 0.9 { 1.0 } else { 0.1 }; let (gx, gy) = (v.1 as f64 / si as f64, v.2 as f64 / si as f64); a11 += wgt * gx * gx; a12 += wgt * gx * gy; a22 += wgt * gy * gy; b1 -= wgt * gx * res; b2 -= wgt * gy * res; sw += wgt; ssum += wgt * res * res; }
                    let l = 0.06 * (a11 + a22) + 0.15 * sw + 1e-6; let det = (a11 + l) * (a22 + l) - a12 * a12; if det.abs() < 1e-12 { break; } let (dx, dy) = (((a22 + l) * b1 - a12 * b2) / det, ((a11 + l) * b2 - a12 * b1) / det); ox += dx.clamp(-3.0, 3.0); oy += dy.clamp(-3.0, 3.0); cfk = (1.0 - 0.5 * ssum / sw.max(1.0)).max(0.0); if dx.abs() + dy.abs() < 0.01 { break; } }
                let lim = 24.0 * sc.max(0.25); tot.0 += ox.clamp(-lim, lim) / sc; tot.1 += oy.clamp(-lim, lim) / sc; }
            cf[k] = cfk; d[k] = tot; }
            // neighbour regularisation: low-confidence vertices pull toward the neighbours' mean offset
            let dd = d.clone(); for k in 0..nx * ny { let (i, j) = (k % nx, k / nx); let mut sx = 0.0; let mut sy = 0.0; let mut sw = 0.0; for (ii, jj) in [(i.wrapping_sub(1), j), (i + 1, j), (i, j.wrapping_sub(1)), (i, j + 1)] { if ii < nx && jj < ny { let kk = jj * nx + ii; let wgt = cf[kk].max(0.05); sx += dd[kk].0 * wgt; sy += dd[kk].1 * wgt; sw += wgt; } } if sw > 0.0 { let a = cf[k].clamp(0.0, 1.0).powi(2); d[k] = (dd[k].0 * a + sx / sw * (1.0 - a), dd[k].1 * a + sy / sw * (1.0 - a)); } } }
        let cur: Vec<[f64; 2]> = (0..nx * ny).map(|k| [base[k].0 + d[k].0, base[k].1 + d[k].1]).collect(); prev2 = std::mem::replace(&mut prev, cur.iter().map(|v| (v[0], v[1])).collect()); out.push(cur); conf.push(if fi == 0 { 1.0 } else { cf.iter().sum::<f64>() / cf.len() as f64 * g.conf[fi].max(0.0).min(1.0).sqrt() }); }
    Ok((out, conf))
}

// ---- occluder auto-matte -------------------------------------------------------------------------------------------------------------
/// Session-held anchor for a pinned layer: the source frame the track started on, used at render time to see what covers the surface.
pub struct Anchor { pub src: String, pub src_start: f64, pub frame: Vec<u8>, pub q0: Quad, pub w: usize, pub h: usize, pub soft: f32, pub chroma: f32 }
static ANCHORS: std::sync::Mutex<Option<std::collections::HashMap<String, Anchor>>> = std::sync::Mutex::new(None);
pub fn set_anchor(target: &str, a: Anchor) { let mut g = ANCHORS.lock().unwrap(); g.get_or_insert_with(Default::default).insert(target.to_string(), a); }
/// Rebuild a missing anchor (project reloaded) from its persisted recipe by grabbing the reference frame from the source media. Tries once per target.
pub fn rebuild_anchor(target: &str, sp: &crate::model::OccSpec, w: usize, h: usize) {
    static TRIED: std::sync::Mutex<Option<std::collections::HashSet<String>>> = std::sync::Mutex::new(None);
    { let mut g = TRIED.lock().unwrap(); let set = g.get_or_insert_with(Default::default); if !set.insert(target.to_string()) { return; } }
    if sp.w != w || sp.h != h { return; }
    let out = std::process::Command::new("ffmpeg").args(["-v", "error", "-ss", &format!("{:.6}", sp.src_time), "-i", &crate::deps::resolve_str(&sp.path), "-frames:v", "1", "-vf", &format!("scale={}:{}", w, h), "-f", "rawvideo", "-pix_fmt", "rgba", "-"]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).output();
    if let Ok(o) = out { if o.stdout.len() == w * h * 4 { set_anchor(target, Anchor { src: sp.src.clone(), src_start: sp.src_start, frame: o.stdout, q0: sp.q0, w, h, soft: sp.soft, chroma: sp.chroma }); } }
}

/// Adaptive occluder: a running model of the visible surface, built in plane (rectified) space while stepping the clip frame by frame. Pixels that differ
/// sharply from the model are occluders and do not update it; everything else updates it (so key press, focus and light change on the surface are absorbed).
/// The whole per-frame matte sequence is computed once per target (streaming decode, plane-resolution only) and cached.
pub struct MatteSeq { pw: usize, ph: usize, mats: Vec<Vec<u8>> }
static SEQS: std::sync::Mutex<Option<std::collections::HashMap<String, std::sync::Arc<MatteSeq>>>> = std::sync::Mutex::new(None);
pub fn remove_anchor(target: &str) { if let Some(m) = ANCHORS.lock().unwrap().as_mut() { m.remove(target); } }
pub fn clear_seq(target: &str) { if let Some(m) = SEQS.lock().unwrap().as_mut() { m.remove(target); } }
fn build_seq(c: &crate::model::Clip, sp: &crate::model::OccSpec, fps: f64) -> Option<MatteSeq> {
    let (w, h) = (sp.w, sp.h); let n = ((c.duration * fps).ceil() as usize).clamp(1, 4000); let t_of = |i: usize| sp.src_start + (sp.f0 + i) as f64 / fps - c.start;
    let q0 = pin_quad(c, t_of(0).max(0.0))?; let len = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).hypot(a[1] - b[1]);
    let (lw, lh) = ((len(q0[0], q0[1]) + len(q0[3], q0[2])) / 2.0, (len(q0[0], q0[3]) + len(q0[1], q0[2])) / 2.0); let ex = sp.expand.clamp(0.0, 2.0) as f64; let pw = ((256.0 * (1.0 + 2.0 * ex)).round() as usize).min(640); let ph = ((pw as f64 * lh / lw.max(1.0)).round() as usize).clamp(48, 640);
    let mut child = std::process::Command::new("ffmpeg").args(["-v", "error", "-ss", &format!("{:.6}", sp.src_time), "-i", &crate::deps::resolve_str(&sp.path), "-vf", &if sp.ramp.is_empty() { format!("setpts=PTS/{},fps={},scale={}:{}", sp.speed.max(0.01), fps, w, h) } else { format!("fps={},scale={}:{}", fps, w, h) }, "-frames:v", &n.to_string(), "-f", "rawvideo", "-pix_fmt", "rgba", "-"]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).spawn().ok()?;
    let mut out = child.stdout.take()?; let mut buf = vec![0u8; w * h * 4]; let mut mats: Vec<Vec<u8>> = vec![]; let mut model: Vec<f32> = vec![]; use std::io::Read;
    let (lo, hi) = (0.07 / sp.soft.max(0.2), 0.16 / sp.soft.max(0.2));
    let mut cur: i64 = -1;
    'o: for i in 0..n { if !sp.ramp.is_empty() { let idx = ((sp.ramp[(sp.f0 + i).min(sp.ramp.len() - 1)] * fps + 1e-6).floor() as i64).max(0); while cur < idx { if out.read_exact(&mut buf).is_err() { break 'o; } cur += 1; } } else if out.read_exact(&mut buf).is_err() { break; } crate::guard::check();
        let q = match pin_quad(c, t_of(i).max(0.0)) { Some(q) => q, None => break }; let m8 = crate::shards::sq2quad([(q[0][0], q[0][1]), (q[1][0], q[1][1]), (q[2][0], q[2][1]), (q[3][0], q[3][1])]);
        let mut rect = vec![0f32; pw * ph * 3]; for y in 0..ph { for x in 0..pw { let (px, py) = crate::shards::hmap(&m8, (x as f64 + 0.5) / pw as f64 * (1.0 + 2.0 * ex) - ex, (y as f64 + 0.5) / ph as f64 * (1.0 + 2.0 * ex) - ex); let (px, py) = (px.clamp(0.0, w as f64 - 1.001), py.clamp(0.0, h as f64 - 1.001)); let (ix, iy) = (px as usize, py as usize); let (fx, fy) = ((px - ix as f64) as f32, (py - iy as f64) as f32);
            for j in 0..3 { let s = |xx: usize, yy: usize| buf[(yy * w + xx) * 4 + j] as f32 / 255.0; rect[(y * pw + x) * 3 + j] = s(ix, iy) * (1.0 - fx) * (1.0 - fy) + s(ix + 1, iy) * fx * (1.0 - fy) + s(ix, iy + 1) * (1.0 - fx) * fy + s(ix + 1, iy + 1) * fx * fy; } } }
        if model.is_empty() { model = rect.clone(); }
        // tolerant compare: distance of each live channel value to the [min,max] envelope of the surface model within `tol` plane px (survives press/parallax shifts)
        let rad = sp.tol.round().clamp(0.0, 8.0) as usize; let nch = if sp.chroma > 0.0 { 5 } else { 3 };
        let chans = |v: &Vec<f32>| -> Vec<Vec<f32>> { let mut o = vec![vec![0f32; pw * ph]; nch]; for k in 0..pw * ph { for j in 0..3 { o[j][k] = v[k * 3 + j]; } if nch == 5 { let yy = 0.299 * v[k * 3] + 0.587 * v[k * 3 + 1] + 0.114 * v[k * 3 + 2]; o[3][k] = (v[k * 3] - yy) * 2.5; o[4][k] = (v[k * 3 + 2] - yy) * 2.5; } } o };
        let (rc, mc_) = (chans(&rect), chans(&model)); let mut diff = vec![0f32; pw * ph];
        for j in 0..nch { let (mut mn, mut mx) = (mc_[j].clone(), mc_[j].clone());
            if rad > 0 { let mut tmn = mn.clone(); let mut tmx = mx.clone(); for y in 0..ph { for x in 0..pw { let (mut a0, mut a1) = (f32::MAX, f32::MIN); for dx in x.saturating_sub(rad)..=(x + rad).min(pw - 1) { let v = mc_[j][y * pw + dx]; a0 = a0.min(v); a1 = a1.max(v); } tmn[y * pw + x] = a0; tmx[y * pw + x] = a1; } }
                for y in 0..ph { for x in 0..pw { let (mut a0, mut a1) = (f32::MAX, f32::MIN); for dy in y.saturating_sub(rad)..=(y + rad).min(ph - 1) { a0 = a0.min(tmn[dy * pw + x]); a1 = a1.max(tmx[dy * pw + x]); } mn[y * pw + x] = a0; mx[y * pw + x] = a1; } } }
            let wgt = if j < 3 { (1.0 - sp.chroma).max(sp.lgate.min(1.0)) } else { sp.chroma };
            if wgt > 0.0 { for k in 0..pw * ph { let d = ((if j < 3 && sp.lgate > 0.0 && sp.chroma > 0.0 && sp.lgate > 1.5 { rc[j][k] - mx[k] } else { (rc[j][k] - mx[k]).max(mn[k] - rc[j][k]) }) - if j < 3 && sp.lgate > 0.0 && sp.chroma > 0.0 { 0.12 } else { 0.0 }).max(0.0).min(1.0) * wgt; if j == 0 || j == 3 { diff[k] += 0.0; } diff[k] = diff[k].max(d); } } }
        let mut t1 = vec![0f32; pw * ph]; let mut t2 = vec![0f32; pw * ph]; crate::masks::box_pass(&diff, &mut t1, pw, ph, 2, true); crate::masks::box_pass(&t1, &mut t2, pw, ph, 2, false);
        let occ: Vec<f32> = t2.iter().map(|&d| { let x = ((d - lo) / (hi - lo)).clamp(0.0, 1.0); x * x * (3.0 - 2.0 * x) }).collect();
        let mut occ = occ; if sp.min_area > 0.0 { let lim = (sp.min_area * (pw * ph) as f32).max(1.0) as usize;
            // two passes: drop small occluder islands (value -> 0), then fill small visible holes (value -> 1)
            for pass in [0usize, 2, 1] { if pass == 2 && sp.thin <= 0.0 { continue; } let mut seen = vec![false; pw * ph]; let thr0 = if pass == 2 { 0.04f32 } else { 0.3 }; let inside = |v: f32| if pass != 1 { v > thr0 } else { v < 0.7 };
                for st in 0..pw * ph { if seen[st] || !inside(occ[st]) { continue; } let mut comp = vec![st]; seen[st] = true; let mut qi = 0; let mut border = false;
                    while qi < comp.len() { let k = comp[qi]; qi += 1; let (x, y) = (k % pw, k / pw); if x == 0 || y == 0 || x == pw - 1 || y == ph - 1 { border = true; }
                        for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] { if nx < pw && ny < ph { let nk = ny * pw + nx; if !seen[nk] && inside(occ[nk]) { seen[nk] = true; comp.push(nk); } } } }
                    // a visible hole touching the plane border is the plane edge, not a hole; an occluder touching it is the real thing entering, but only large ones are kept
                    let thin_drop = pass == 2 && sp.thin > 0.0 && !border && comp.len() < lim * 40 && { let mut bd = 0usize; for &k in &comp { let (x, y) = (k % pw, k / pw); let mut e = false; for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] { if nx >= pw || ny >= ph || occ[ny * pw + nx] <= thr0 { e = true; } } if e { bd += 1; } } (comp.len() as f32) / (bd.max(1) as f32) < sp.thin };
                    if (comp.len() < lim || thin_drop) && (pass != 1 || !border) { let v = if pass != 1 { 0.0 } else { 1.0 }; for k in comp { occ[k] = v; } } } } }
        if sp.close > 0.0 { let r = sp.close.round().clamp(1.0, 8.0) as usize; // grey closing: fills small notches and bays along the occluder edge, smooths scallops
            let win = |src: &Vec<f32>, maxi: bool| -> Vec<f32> { let mut t = vec![0f32; pw * ph]; for y in 0..ph { for x in 0..pw { let mut v = if maxi { f32::MIN } else { f32::MAX }; for dx in x.saturating_sub(r)..=(x + r).min(pw - 1) { let s = src[y * pw + dx]; v = if maxi { v.max(s) } else { v.min(s) }; } t[y * pw + x] = v; } }
                let mut o = vec![0f32; pw * ph]; for y in 0..ph { for x in 0..pw { let mut v = if maxi { f32::MIN } else { f32::MAX }; for dy in y.saturating_sub(r)..=(y + r).min(ph - 1) { let s = t[dy * pw + x]; v = if maxi { v.max(s) } else { v.min(s) }; } o[y * pw + x] = v; } } o };
            let d = win(&occ, true); occ = win(&d, false); }
        // dilate the occluder by 2 plane px before deciding what may update the model, so soft finger edges never leak into it
        let mut upd = vec![0f32; pw * ph]; for y in 0..ph { for x in 0..pw { let mut mx = 0f32; for dy in -2i32..=2 { for dx in -2i32..=2 { let (xx, yy) = ((x as i32 + dx).clamp(0, pw as i32 - 1) as usize, (y as i32 + dy).clamp(0, ph as i32 - 1) as usize); mx = mx.max(occ[yy * pw + xx]); } } upd[y * pw + x] = mx; } }
        let spd = if sp.ramp.is_empty() { sp.speed as f32 } else { let k = (sp.f0 + i).min(sp.ramp.len() - 1); let k2 = (k + 1).min(sp.ramp.len() - 1); ((sp.ramp[k2] - sp.ramp[k]) * fps) as f32 };
        for k in 0..pw * ph { let a = (1.0 - 0.65f32.powf(spd.clamp(0.01, 4.0) as f32)) * (1.0 - upd[k]); for j in 0..3 { model[k * 3 + j] += (rect[k * 3 + j] - model[k * 3 + j]) * a; } }
        mats.push(occ.iter().map(|&o| ((1.0 - o) * 255.0).round() as u8).collect()); }
    if sp.temporal > 0 && mats.len() > 2 { let r = sp.temporal; let src = mats.clone(); let nn = src.len(); for i in 0..nn { let (a, b) = (i.saturating_sub(r), (i + r).min(nn - 1)); let mut win = vec![0u8; b - a + 1]; for k in 0..mats[i].len() { for (t, f) in (a..=b).enumerate() { win[t] = src[f][k]; } win.sort_unstable(); mats[i][k] = win[win.len() / 2]; } } }
    let _ = child.kill(); let _ = child.wait(); if mats.is_empty() { None } else { Some(MatteSeq { pw, ph, mats }) }
}
/// Image-space coverage matte (1 = surface visible) for the adaptive mode at timeline time `t`; None when it cannot be built.
pub fn adaptive_matte(c: &crate::model::Clip, sp: &crate::model::OccSpec, fps: f64, t: f64, w: usize, h: usize) -> Option<Vec<f32>> {
    let seq = { let mut g = SEQS.lock().unwrap(); let map = g.get_or_insert_with(Default::default); if let Some(s) = map.get(&c.id) { Some(s.clone()) } else { drop(g); let s = build_seq(c, sp, fps).map(std::sync::Arc::new); if let Some(s) = &s { SEQS.lock().unwrap().get_or_insert_with(Default::default).insert(c.id.clone(), s.clone()); } s } }?;
    let i = (((t - sp.src_start) * fps).round() as i64 - sp.f0 as i64).clamp(0, seq.mats.len() as i64 - 1) as usize; let lt = (t - c.start).max(0.0); let q = pin_quad(c, lt)?;
    let hm = solve_h(&q, &UNIT)?; let mut m = vec![1f32; w * h]; let mat = &seq.mats[i];
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); for p in &q { x0 = x0.min(p[0]); y0 = y0.min(p[1]); x1 = x1.max(p[0]); y1 = y1.max(p[1]); }
    let ex = sp.expand.clamp(0.0, 2.0) as f64; if ex > 0.0 { x0 = 0.0; y0 = 0.0; x1 = w as f64; y1 = h as f64; }
    for y in y0.max(0.0) as usize..=(y1.min(h as f64 - 1.0).max(0.0) as usize) { for x in x0.max(0.0) as usize..=(x1.min(w as f64 - 1.0).max(0.0) as usize) { let (u, v) = ap(&hm, x as f64 + 0.5, y as f64 + 0.5); let (u, v) = if ex > 0.0 { let (uu, vv) = ((u + ex) / (1.0 + 2.0 * ex), (v + ex) / (1.0 + 2.0 * ex)); if !(0.0..1.0).contains(&uu) || !(0.0..1.0).contains(&vv) { continue; } (uu, vv) } else { (u.clamp(0.0, 0.9999), v.clamp(0.0, 0.9999)) }; // edge-extend: the antialiased quad rim must be cut like the pixels just inside it
        let (fx, fy) = ((u * seq.pw as f64 - 0.5).clamp(0.0, seq.pw as f64 - 1.001), (v * seq.ph as f64 - 0.5).clamp(0.0, seq.ph as f64 - 1.001)); let (ix, iy) = (fx as usize, fy as usize); let (ax, ay) = ((fx - ix as f64) as f32, (fy - iy as f64) as f32);
        let s = |xx: usize, yy: usize| mat[yy * seq.pw + xx] as f32 / 255.0; m[y * w + x] = s(ix, iy) * (1.0 - ax) * (1.0 - ay) + s(ix + 1, iy) * ax * (1.0 - ay) + s(ix, iy + 1) * (1.0 - ax) * ay + s(ix + 1, iy + 1) * ax * ay; } }
    Some(m)
}
pub fn anchor_src(target: &str) -> Option<(String, f64)> { ANCHORS.lock().unwrap().as_ref()?.get(target).map(|a| (a.src.clone(), a.src_start)) }

/// Coverage matte (1 = surface visible, 0 = occluder) for the tracked quad `q` in the current source frame `cur` (rgba u8, same size as the anchor).
pub fn occluder_matte(target: &str, cur: &[u8], q: &Quad, gain: f32) -> Option<Vec<f32>> {
    let g = ANCHORS.lock().unwrap(); let a = g.as_ref()?.get(target)?; let (w, h) = (a.w, a.h); if cur.len() < w * h * 4 { return None; }
    let hm = solve_h(q, &a.q0)?; let mut diff = vec![0f32; w * h]; let mut inq = vec![false; w * h];
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); for p in q { x0 = x0.min(p[0]); y0 = y0.min(p[1]); x1 = x1.max(p[0]); y1 = y1.max(p[1]); }
    let (bx0, by0, bx1, by1) = (x0.max(0.0) as usize, y0.max(0.0) as usize, (x1.min(w as f64 - 1.0)).max(0.0) as usize, (y1.min(h as f64 - 1.0)).max(0.0) as usize);
    for y in by0..=by1 { for x in bx0..=bx1 { let (u, v) = ap(&hm, x as f64 + 0.5, y as f64 + 0.5); if u < 1.0 || v < 1.0 || u > w as f64 - 2.0 || v > h as f64 - 2.0 { continue; }
        // tolerant compare: the live pixel is matched against the anchor at +-1 px (warp/resample jitter on thin detail must not read as an occluder); the smallest difference wins
        let mut best = f32::MAX;
        for oy in -1i32..=1 { for ox in -1i32..=1 { let (uu, vv) = (u + ox as f64, v + oy as f64); if uu < 0.0 || vv < 0.0 || uu > w as f64 - 2.0 || vv > h as f64 - 2.0 { continue; }
            let (ix, iy) = (uu as usize, vv as usize); let (fx, fy) = ((uu - ix as f64) as f32, (vv - iy as f64) as f32); let mut m = 0f32; let mut avs = [0f32; 3];
            for j in 0..3 { let s = |xx: usize, yy: usize| a.frame[(yy * w + xx) * 4 + j] as f32; let av = s(ix, iy) * (1.0 - fx) * (1.0 - fy) + s(ix + 1, iy) * fx * (1.0 - fy) + s(ix, iy + 1) * (1.0 - fx) * fy + s(ix + 1, iy + 1) * fx * fy; avs[j] = av;
                m = m.max((cur[(y * w + x) * 4 + j] as f32 - av * gain).abs() / 255.0); }
            if a.chroma > 0.0 { // chroma compare: luminance/focus/glyph changes on a neutral surface do not count, a coloured occluder (skin, cloth) does
                let c = |j: usize| cur[(y * w + x) * 4 + j] as f32; let yc = 0.299 * c(0) + 0.587 * c(1) + 0.114 * c(2); let ya = 0.299 * avs[0] + 0.587 * avs[1] + 0.114 * avs[2];
                let dch = (((c(0) - yc) - (avs[0] - ya)).abs().max(((c(2) - yc) - (avs[2] - ya)).abs()) / 255.0 * 2.5).min(1.0); m = m * (1.0 - a.chroma) + dch * a.chroma; }
            best = best.min(m); } }
        diff[y * w + x] = if best == f32::MAX { 0.0 } else { best }; inq[y * w + x] = true; } }
    let mut t1 = vec![0f32; w * h]; let mut t2 = vec![0f32; w * h]; crate::masks::box_pass(&diff, &mut t1, w, h, 3, true); crate::masks::box_pass(&t1, &mut t2, w, h, 3, false);
    let (lo, hi) = (0.07 / a.soft.max(0.2), 0.16 / a.soft.max(0.2)); let mut m: Vec<f32> = t2.iter().map(|&d| { let x = ((d - lo) / (hi - lo)).clamp(0.0, 1.0); 1.0 - x * x * (3.0 - 2.0 * x) }).collect();
    // grow the occluder by 2px and feather
    let mut mn = m.clone(); for y in 2..h.saturating_sub(2) { for x in 2..w.saturating_sub(2) { if !inq[y * w + x] { continue; } let mut v = 1f32; for dy in -2i32..=2 { for dx in -2i32..=2 { v = v.min(m[((y as i32 + dy) as usize) * w + (x as i32 + dx) as usize]); } } mn[y * w + x] = v; } }
    crate::masks::box_pass(&mn, &mut t1, w, h, 1, true); crate::masks::box_pass(&t1, &mut m, w, h, 1, false); for i in 0..w * h { if !inq[i] { m[i] = 1.0; } } Some(m)
}

#[cfg(test)]
mod pose_tests { use super::*;
    fn proj(f: f64, cx: f64, cy: f64, r: &[V3; 3], t: V3, a: f64, b: f64) -> Quad { let mut q = [[0f64; 2]; 4]; for (k, (u, v)) in [(0.0, 0.0), (1.0, 0.0), (1.0, 1.0), (0.0, 1.0)].iter().enumerate() { let p = v_add(v_add(v_mul(r[0], a * u), v_mul(r[1], b * v)), t); q[k] = [f * p[0] / p[2] + cx, f * p[1] / p[2] + cy]; } q }
    #[test] fn pose_roundtrip_and_predict() { let (w, h) = (1280usize, 720usize); let f = 0.9 * w as f64; let (cx, cy) = (640.0, 360.0); let base = rot_from([0.1, 0.5, 0.05]);
        let mk = |i: f64| { let d = rot_from([0.0, 0.03 * i, 0.0]); let mut r = [[0f64; 3]; 3]; for j in 0..3 { r[j] = rot_apply(&d, base[j]); } proj(f, cx, cy, &r, [-0.3 + 0.02 * i, -0.1, 3.0 - 0.05 * i], 0.5, 0.3) };
        let hist: Vec<Quad> = (0..5).map(|i| mk(i as f64)).collect(); let pr = pose_predict(&hist, w, h).expect("pred"); let tr = mk(5.0);
        let e = (0..4).map(|k| (pr[k][0] - tr[k][0]).hypot(pr[k][1] - tr[k][1])).fold(0.0, f64::max); eprintln!("pose pred max err px {e}"); assert!(e < 3.0, "err {e}"); }
}

/// Global (whole-frame) motion of a media clip relative to its first frame: per frame [dx px, dy px, scale, rotation deg].
/// Robust to moving foreground and flashes: five regions (four quadrants plus the full inset frame) are tracked independently, each fitted by a similarity transform, and the per-frame result is the median across regions.
pub fn global_motion(p: &Project, clip_id: &str, f0: usize, f1: usize) -> Result<Vec<[f64; 4]>, String> {
    let (w, h) = (p.width as f64, p.height as f64); let (mx, my) = (w * 0.10, h * 0.10); let (cw, ch) = (w / 2.0, h / 2.0);
    let rect = |x0: f64, y0: f64, x1: f64, y1: f64| -> Quad { [[x0, y0], [x1, y0], [x1, y1], [x0, y1]] };
    let regions = [rect(mx, my, w - mx, h - my), rect(mx, my, cw - 4.0, ch - 4.0), rect(cw + 4.0, my, w - mx, ch - 4.0), rect(mx, ch + 4.0, cw - 4.0, h - my), rect(cw + 4.0, ch + 4.0, w - mx, h - my)];
    let fc = (w / 2.0, h / 2.0); let n = f1 + 1 - f0; let mut per: Vec<Vec<[f64; 4]>> = vec![]; // per region, per frame
    for q0 in regions.iter() { let Ok(t) = track(p, clip_id, *q0, f0, f1, 4) else { continue }; let pc = ((q0[0][0] + q0[1][0] + q0[2][0] + q0[3][0]) / 4.0, (q0[0][1] + q0[1][1] + q0[2][1] + q0[3][1]) / 4.0);
        let mut v = vec![]; for q in t.quads.iter() { let qc = ((q[0][0] + q[1][0] + q[2][0] + q[3][0]) / 4.0, (q[0][1] + q[1][1] + q[2][1] + q[3][1]) / 4.0); let (mut saa, mut sa, mut sb) = (1e-9, 0.0, 0.0);
            for k in 0..4 { let (px, py, qx, qy) = (q0[k][0] - pc.0, q0[k][1] - pc.1, q[k][0] - qc.0, q[k][1] - qc.1); saa += px * px + py * py; sa += px * qx + py * qy; sb += px * qy - py * qx; }
            let (a, b) = (sa / saa, sb / saa); let (rx, ry) = (fc.0 - pc.0, fc.1 - pc.1); let (tx, ty) = (qc.0 + a * rx - b * ry, qc.1 + b * rx + a * ry); v.push([tx - fc.0, ty - fc.1, a.hypot(b), b.atan2(a).to_degrees()]); }
        if v.len() == n { per.push(v); } }
    if per.is_empty() { return Err("global motion: no region could be tracked".into()); }
    let med = |mut x: Vec<f64>| { x.sort_by(|a, b| a.partial_cmp(b).unwrap()); let m = x.len(); if m % 2 == 1 { x[m / 2] } else { 0.5 * (x[m / 2 - 1] + x[m / 2]) } };
    // Consensus on per-frame increments: regions that disagree with the majority (burst, exposure jump, occluder) are rejected;
    // with fewer than 2 agreeing regions the last good velocity is carried (coast) so a burst cannot drag the trajectory.
    let tol = [3.0, 3.0, 0.02, 0.6]; let mut out = vec![[0f64; 4]; n]; for k in 0..4 { out[0][k] = med(per.iter().map(|r| r[0][k]).collect()); } let mut vel = [0f64; 4]; let mut damp = 1.0;
    for i in 1..n { let incs: Vec<[f64; 4]> = per.iter().map(|r| { let mut d = [0f64; 4]; for k in 0..4 { d[k] = r[i][k] - r[i - 1][k]; } d }).collect();
        let m: Vec<f64> = (0..4).map(|k| med(incs.iter().map(|d| d[k]).collect())).collect();
        let good: Vec<&[f64; 4]> = incs.iter().filter(|d| (0..4).all(|k| (d[k] - m[k]).abs() <= tol[k])).collect();
        if good.len() >= 2 || (per.len() == 1) { for k in 0..4 { vel[k] = if per.len() == 1 { incs[0][k] } else { good.iter().map(|d| d[k]).sum::<f64>() / good.len() as f64 }; } damp = 1.0; } else { damp *= 0.8; for k in 0..4 { vel[k] *= 0.8; } let _ = damp; }
        for k in 0..4 { out[i][k] = out[i - 1][k] + vel[k]; } }
    Ok(out)
}
