//! Video matte tracker: propagates a reviewed seed outline through a clip into a dense, temporally stable alpha matte.
//! Per frame: dense optical flow warps the previous alpha, a band around the warped edge is re-decided from colour models
//! (anchored to the seed frame) and snapped to image edges with a guided filter, so partial alpha on blurred edges survives.
//! Seeds in the middle of a clip are tracked both ways and blended by distance. A final flow-compensated forward/backward
//! pass removes frame-to-frame flicker without hard thresholds. Output is a raw u8 sequence readable by the `matte` mask kind.
use crate::effects::Frame;
use crate::flow::flow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub struct Store { pub w: usize, pub h: usize, pub n: usize, pub d: Vec<u8> }
static STORE: Mutex<Option<HashMap<String, Arc<Store>>>> = Mutex::new(None);
pub fn save(dir: &str, s: &Store) -> Result<(), String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?; let mut b = Vec::with_capacity(16 + s.d.len()); b.extend_from_slice(b"KMT1"); b.extend_from_slice(&(s.w as u32).to_le_bytes()); b.extend_from_slice(&(s.h as u32).to_le_bytes()); b.extend_from_slice(&(s.n as u32).to_le_bytes()); b.extend_from_slice(&s.d);
    std::fs::write(format!("{dir}/matte.bin"), b).map_err(|e| e.to_string())?; STORE.lock().unwrap().get_or_insert_with(Default::default).insert(dir.to_string(), Arc::new(Store { w: s.w, h: s.h, n: s.n, d: s.d.clone() })); Ok(())
}
pub fn load(dir: &str) -> Option<Arc<Store>> {
    if let Some(s) = STORE.lock().unwrap().as_ref().and_then(|m| m.get(dir)) { return Some(s.clone()); }
    let b = std::fs::read(format!("{dir}/matte.bin")).ok()?; if b.len() < 16 || &b[0..4] != b"KMT1" { return None; }
    let rd = |o: usize| u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]]) as usize; let (w, h, n) = (rd(4), rd(8), rd(12)); if b.len() != 16 + w * h * n { return None; }
    let s = Arc::new(Store { w, h, n, d: b[16..].to_vec() }); STORE.lock().unwrap().get_or_insert_with(Default::default).insert(dir.to_string(), s.clone()); Some(s)
}
/// Alpha of frame `i` (clamped) resampled to w x h.
pub fn frame_alpha(s: &Store, i: f64, w: usize, h: usize) -> Vec<f32> {
    let a = i.floor().clamp(0.0, (s.n - 1) as f64) as usize; let b = (a + 1).min(s.n - 1); let t = (i - a as f64).clamp(0.0, 1.0) as f32; let fa = &s.d[a * s.w * s.h..(a + 1) * s.w * s.h]; let fb = &s.d[b * s.w * s.h..(b + 1) * s.w * s.h];
    let mut o = vec![0f32; w * h]; for y in 0..h { let sy = ((y * s.h) / h).min(s.h - 1); for x in 0..w { let sx = ((x * s.w) / w).min(s.w - 1); let k = sy * s.w + sx; o[y * w + x] = (fa[k] as f32 * (1.0 - t) + fb[k] as f32 * t) / 255.0; } } o
}

/// Anti-aliased polygon coverage (4 vertical sub-samples, exact horizontal coverage), even-odd.
pub fn raster_poly(pts: &[[f64; 2]], w: usize, h: usize) -> Vec<f32> {
    let mut c = vec![0f32; w * h]; if pts.len() < 3 { return c; } const SS: usize = 4;
    for y in 0..h { for s in 0..SS { let yy = y as f64 + (s as f64 + 0.5) / SS as f64; let mut xs: Vec<f64> = vec![];
        for i in 0..pts.len() { let (a, b) = (pts[i], pts[(i + 1) % pts.len()]); if (a[1] > yy) != (b[1] > yy) { xs.push(a[0] + (yy - a[1]) / (b[1] - a[1]) * (b[0] - a[0])); } }
        xs.sort_by(|a, b| a.partial_cmp(b).unwrap());
        for pr in xs.chunks(2) { if pr.len() < 2 { continue; } let (x0, x1) = (pr[0].max(0.0), pr[1].min(w as f64)); if x1 <= x0 { continue; } let (i0, i1) = (x0.floor() as usize, (x1.ceil() as usize).min(w));
            for x in i0..i1 { let cov = (x1.min(x as f64 + 1.0) - x0.max(x as f64)).max(0.0); c[y * w + x] += (cov / SS as f64) as f32; } } } }
    for v in c.iter_mut() { *v = v.clamp(0.0, 1.0); } c
}

fn bil(s: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 { let x = x.clamp(0.0, w as f32 - 1.001); let y = y.clamp(0.0, h as f32 - 1.001); let (x0, y0) = (x as usize, y as usize); let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let (a, b, c, d) = (s[y0 * w + x0], s[y0 * w + x0 + 1], s[(y0 + 1) * w + x0], s[(y0 + 1) * w + x0 + 1]); a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy }
/// Flow field at half resolution (u,v in half-res pixels): a pair for one frame step.
pub struct Fl { pub u: Vec<f32>, pub v: Vec<f32>, pub w2: usize, pub h2: usize }
impl Fl { fn at(&self, x: usize, y: usize) -> (f32, f32) { let fx = (x as f32 + 0.5) / 2.0 - 0.5; let fy = (y as f32 + 0.5) / 2.0 - 0.5; (2.0 * bil(&self.u, self.w2, self.h2, fx, fy), 2.0 * bil(&self.v, self.w2, self.h2, fx, fy)) } }
/// Backward warp: out(x) = src(x + flow(x)), where `fl` maps this frame to the frame `src` belongs to.
pub fn warp(src: &[f32], w: usize, h: usize, fl: &Fl) -> Vec<f32> { let mut o = vec![0f32; w * h]; for y in 0..h { for x in 0..w { let (u, v) = fl.at(x, y); o[y * w + x] = bil(src, w, h, x as f32 + u, y as f32 + v); } } o }

fn to_frame(rgb: &[u8], w: usize, h: usize) -> Frame { let mut f = Frame::new(w, h); for i in 0..w * h { for j in 0..3 { f.d[i * 4 + j] = rgb[i * 3 + j] as f32 / 255.0; } f.d[i * 4 + 3] = 1.0; } f }
fn gray(rgb: &[u8], i: usize) -> f32 { (0.299 * rgb[i * 3] as f32 + 0.587 * rgb[i * 3 + 1] as f32 + 0.114 * rgb[i * 3 + 2] as f32) / 255.0 }
fn bin(rgb: &[u8], i: usize) -> usize { ((rgb[i * 3] as usize >> 4) << 8) | ((rgb[i * 3 + 1] as usize >> 4) << 4) | (rgb[i * 3 + 2] as usize >> 4) }
fn hist(rgb: &[u8], sel: &dyn Fn(usize) -> bool, n: usize) -> Vec<f32> { let mut h = vec![0f32; 4096]; let mut tot = 0.0; for i in 0..n { if sel(i) { h[bin(rgb, i)] += 1.0; tot += 1.0; } }
    // smooth over neighbouring bins so near colours share evidence
    let mut o = vec![0f32; 4096]; for b in 0..4096usize { if h[b] == 0.0 { continue; } let (r, g, bl) = (b >> 8, (b >> 4) & 15, b & 15); for dr in -1i32..=1 { for dg in -1i32..=1 { for db in -1i32..=1 { let (rr, gg, bb) = (r as i32 + dr, g as i32 + dg, bl as i32 + db); if !(0..16).contains(&rr) || !(0..16).contains(&gg) || !(0..16).contains(&bb) { continue; } let wgt = if dr == 0 && dg == 0 && db == 0 { 1.0 } else { 0.25 }; o[((rr << 8) | (gg << 4) | bb) as usize] += h[b] * wgt; } } } }
    let s: f32 = o.iter().sum::<f32>().max(1e-6); let _ = tot; for v in o.iter_mut() { *v = (*v + 0.02) / (s + 0.02 * 4096.0); } o }

// ---- parametric (affine) object motion: direct Gauss-Newton alignment of the object's pixels, multi-scale, robust ----
fn box2(s: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) { let (nw, nh) = ((w / 2).max(1), (h / 2).max(1)); let mut o = vec![0f32; nw * nh];
    for y in 0..nh { for x in 0..nw { let (x0, y0, x1, y1) = (2 * x, 2 * y, (2 * x + 1).min(w - 1), (2 * y + 1).min(h - 1)); o[y * nw + x] = 0.25 * (s[y0 * w + x0] + s[y0 * w + x1] + s[y1 * w + x0] + s[y1 * w + x1]); } } (o, nw, nh) }
fn solve6(mut a: [[f64; 7]; 6]) -> Option<[f64; 6]> { for i in 0..6 { let mut p = i; for r in i + 1..6 { if a[r][i].abs() > a[p][i].abs() { p = r; } } if a[p][i].abs() < 1e-12 { return None; } a.swap(i, p);
    for r in 0..6 { if r == i { continue; } let f = a[r][i] / a[i][i]; for c in i..7 { a[r][c] -= f * a[i][c]; } } } let mut x = [0f64; 6]; for i in 0..6 { x[i] = a[i][6] / a[i][i]; } Some(x) }
/// Affine map prev -> cur (x' = A x + b) for the object `alpha` (in prev), as [a,b,c,d,tx,ty] with x' = (a x + b y + tx, c x + d y + ty).
pub fn aff_step(gp: &[f32], gc: &[f32], alpha: &[f32], w: usize, h: usize, init: Option<[f64; 6]>) -> [f64; 6] { aff_step_l(gp, gc, alpha, w, h, init, false) }
pub fn aff_step_l(gp: &[f32], gc: &[f32], alpha: &[f32], w: usize, h: usize, init: Option<[f64; 6]>, lock: bool) -> [f64; 6] {
    let mut pg = vec![(gp.to_vec(), w, h)]; let mut cg = vec![(gc.to_vec(), w, h)]; let mut pa = vec![(alpha.to_vec(), w, h)];
    for _ in 0..4 { let (a, aw, ah) = pg.last().unwrap().clone(); if aw < 48 || ah < 48 { break; } pg.push(box2(&a, aw, ah)); let (b, bw, bh) = cg.last().unwrap().clone(); cg.push(box2(&b, bw, bh)); let (c, cw, ch) = pa.last().unwrap().clone(); pa.push(box2(&c, cw, ch)); }
    let (mut m, mut cx, mut cy, mut tot) = ([1.0f64, 0.0, 0.0, 1.0, 0.0, 0.0], 0f64, 0f64, 0f64);
    for y in 0..h { for x in 0..w { let a = alpha[y * w + x] as f64; if a > 0.5 { cx += x as f64; cy += y as f64; tot += 1.0; } } } if tot < 30.0 { return m; } cx /= tot; cy /= tot;
    if let Some(i) = init { m = [i[0], i[1], i[2], i[3], i[0] * cx + i[1] * cy + i[4] - cx, i[2] * cx + i[3] * cy + i[5] - cy]; }
    for lvl in (0..pg.len()).rev() { let s = (1u32 << lvl) as f64; let (ip, lw, lh) = (&pg[lvl].0, pg[lvl].1, pg[lvl].2); let ic = &cg[lvl].0; let ia = &pa[lvl].0; let (lcx, lcy) = (cx / s, cy / s);
        // translation of the centroid at this level
        let mut p = [m[0], m[1], m[2], m[3], m[4] / s, m[5] / s]; let mut lam: f64 = 1e-3;
        // collect object pixels once
        let mut px: Vec<(usize, usize)> = vec![]; for y in 1..lh - 1 { for x in 1..lw - 1 { if ia[y * lw + x] > 0.5 { px.push((x, y)); } } } if px.len() < 20 { continue; }
        let mut prev_err = f64::MAX;
        for _it in 0..30 { let mut ata = [[0f64; 6]; 6]; let mut atb = [0f64; 6]; let mut err = 0f64; let mut cnt = 0f64;
            for &(x, y) in &px { let (xc, yc) = (x as f64 - lcx, y as f64 - lcy);
                let xs = lcx + p[4] + p[0] * xc + p[1] * yc; let ys = lcy + p[5] + p[2] * xc + p[3] * yc; if xs < 1.0 || ys < 1.0 || xs > lw as f64 - 2.0 || ys > lh as f64 - 2.0 { continue; }
                let (xf, yf) = (xs as f32, ys as f32); let gx = 0.5 * (bil(ic, lw, lh, xf + 1.0, yf) - bil(ic, lw, lh, xf - 1.0, yf)) as f64; let gy = 0.5 * (bil(ic, lw, lh, xf, yf + 1.0) - bil(ic, lw, lh, xf, yf - 1.0)) as f64;
                let r = (bil(ic, lw, lh, xf, yf) - ip[y * lw + x]) as f64; let wt = { let k = 0.08; let a = r.abs(); if a < k { 1.0 } else { k / a } };
                let j = [gx * xc, gx * yc, gy * xc, gy * yc, gx, gy]; for a in 0..6 { for b in 0..6 { ata[a][b] += wt * j[a] * j[b]; } atb[a] -= wt * j[a] * r; } err += wt * r * r; cnt += wt; }
            if cnt < 10.0 { break; } err /= cnt; if err > prev_err { lam *= 10.0; } else { lam = (lam * 0.5).max(1e-6); } prev_err = prev_err.min(err);
            let mut sys = [[0f64; 7]; 6]; for a in 0..6 { for b in 0..6 { sys[a][b] = ata[a][b] + if a == b { lam * (ata[a][a] + 1e-3) } else { 0.0 }; } sys[a][6] = atb[a]; }
            if let Some(mut d) = solve6(sys) { if lock { for k in 0..4 { d[k] = 0.0; } } let mut np = p; for k in 0..6 { np[k] += d[k]; } if (np[0] - 1.0).abs() < 0.6 && (np[3] - 1.0).abs() < 0.6 && np[1].abs() < 0.6 && np[2].abs() < 0.6 { p = np; } let mag: f64 = d.iter().map(|v| v * v).sum(); if mag < 1e-10 { break; } } else { break; } }
        m = [p[0], p[1], p[2], p[3], p[4] * s, p[5] * s]; }
    // convert centred form (x' = c + t + A (x - c)) to absolute: b = c + t - A c
    [m[0], m[1], m[2], m[3], cx + m[4] - m[0] * cx - m[1] * cy, cy + m[5] - m[2] * cx - m[3] * cy]
}
/// out(y) = src(M^-1 y)
pub fn aff_warp(src: &[f32], w: usize, h: usize, m: &[f64; 6]) -> Vec<f32> { let det = m[0] * m[3] - m[1] * m[2]; if det.abs() < 1e-6 { return src.to_vec(); }
    let (ia, ib, ic, id) = (m[3] / det, -m[1] / det, -m[2] / det, m[0] / det); let mut o = vec![0f32; w * h];
    for y in 0..h { for x in 0..w { let (dx, dy) = (x as f64 - m[4], y as f64 - m[5]); let (sx, sy) = (ia * dx + ib * dy, ic * dx + id * dy); o[y * w + x] = if sx < -1.0 || sy < -1.0 || sx > w as f64 || sy > h as f64 { 0.0 } else { bil(src, w, h, sx as f32, sy as f32) }; } } o }

/// Split the object into `k` compact regions (k-means on pixel coordinates); each region mask is a hard Voronoi cell of the object alpha.
pub fn region_masks(alpha: &[f32], w: usize, h: usize, k: usize) -> Vec<Vec<f32>> {
    let pts: Vec<(f32, f32, usize)> = (0..w * h).filter(|&i| alpha[i] > 0.5).map(|i| ((i % w) as f32, (i / w) as f32, i)).collect(); if k < 2 || pts.len() < 400 * k { return vec![alpha.to_vec()]; }
    let mut c: Vec<(f32, f32)> = vec![(pts[0].0, pts[0].1)]; while c.len() < k { let mut best = (0f32, 0usize); for (j, p) in pts.iter().enumerate().step_by(7) { let d = c.iter().map(|q| (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).fold(f32::MAX, f32::min); if d > best.0 { best = (d, j); } } c.push((pts[best.1].0, pts[best.1].1)); }
    let mut lab = vec![0usize; pts.len()];
    for _ in 0..10 { let mut acc = vec![(0f32, 0f32, 0f32); k]; for (j, p) in pts.iter().enumerate() { let mut b = (f32::MAX, 0); for (ci, q) in c.iter().enumerate() { let d = (p.0 - q.0).powi(2) + (p.1 - q.1).powi(2); if d < b.0 { b = (d, ci); } } lab[j] = b.1; acc[b.1].0 += p.0; acc[b.1].1 += p.1; acc[b.1].2 += 1.0; }
        for ci in 0..k { if acc[ci].2 > 0.0 { c[ci] = (acc[ci].0 / acc[ci].2, acc[ci].1 / acc[ci].2); } } }
    let mut out = vec![vec![0f32; w * h]; k]; for (j, p) in pts.iter().enumerate() { out[lab[j]][p.2] = alpha[p.2]; }
    // soft pixels (edge) go to the nearest region too
    for i in 0..w * h { if alpha[i] > 0.0 && alpha[i] <= 0.5 { let (x, y) = ((i % w) as f32, (i / w) as f32); let mut b = (f32::MAX, 0); for (ci, q) in c.iter().enumerate() { let d = (x - q.0).powi(2) + (y - q.1).powi(2); if d < b.0 { b = (d, ci); } } out[b.1][i] = alpha[i]; } }
    out.into_iter().filter(|m| m.iter().filter(|&&v| v > 0.5).count() >= 150).collect()
}
/// One piecewise step: global affine from the seed template, then per-region affines initialised from it; regions that disagree wildly with the global motion fall back to it.
pub fn pw_step(sg: &[f32], g1: &[f32], seed: &[f32], regs: &[Vec<f32>], mt: &mut [f64; 6], mr: &mut Vec<[f64; 6]>, w: usize, h: usize) -> Vec<f32> {
    *mt = aff_step(sg, g1, seed, w, h, Some(*mt)); if regs.len() < 2 { return aff_warp(seed, w, h, mt); }
    let mut out = vec![0f32; w * h];
    for (r, reg) in regs.iter().enumerate() {
        let init = { let mut i = *mt; if mr[r] != [1.0, 0.0, 0.0, 1.0, 0.0, 0.0] { let _ = &mut i; } i }; let mut m = aff_step_l(sg, g1, reg, w, h, Some(init), true);
        let (mut cx, mut cy, mut n) = (0f64, 0f64, 0f64); for i in (0..w * h).step_by(5) { if reg[i] > 0.5 { cx += (i % w) as f64; cy += (i / w) as f64; n += 1.0; } } if n > 0.0 { cx /= n; cy /= n; }
        let (px, py) = (m[0] * cx + m[1] * cy + m[4], m[2] * cx + m[3] * cy + m[5]); let (gx, gy) = (mt[0] * cx + mt[1] * cy + mt[4], mt[2] * cx + mt[3] * cy + mt[5]);
        if ((px - gx).powi(2) + (py - gy).powi(2)).sqrt() > 0.08 * w as f64 { m = *mt; } mr[r] = m;
        let wr = aff_warp(reg, w, h, &m); for i in 0..w * h { if wr[i] > out[i] { out[i] = wr[i]; } } }
    out
}

#[derive(Clone)]
pub struct Opts { pub band: f32, pub temporal: f32, pub colour: f32, pub guide_r: usize }
impl Default for Opts { fn default() -> Self { Opts { band: 14.0, temporal: 0.6, colour: 0.9, guide_r: 3 } } }
pub struct Out { pub alpha: Vec<Vec<f32>>, pub raw_flicker: f64, pub flicker: f64, pub raw_jitter: f64, pub jitter: f64, pub conf: Vec<f64>, pub warnings: Vec<String> }

/// Re-decide the band around a warped alpha from colour evidence anchored on the seed frame.
fn refine(a: &[f32], rgb: &[u8], anchor: &(Vec<f32>, Vec<f32>), w: usize, h: usize, o: &Opts) -> Vec<f32> {
    let n = w * h; let inside: Vec<bool> = a.iter().map(|&v| v > 0.5).collect(); let outside: Vec<bool> = inside.iter().map(|b| !b).collect();
    let (d_out, d_in) = (crate::masks::edt(&inside, w, h), crate::masks::edt(&outside, w, h)); let sd: Vec<f32> = (0..n).map(|i| if inside[i] { d_in[i] - 0.5 } else { -(d_out[i] - 0.5) }).collect();
    let fgc = hist(rgb, &|i| sd[i] > 4.0, n); let bgc = hist(rgb, &|i| sd[i] < -3.0 && sd[i] > -o.band * 2.5, n);
    let mut out = vec![0f32; n]; let s0 = (o.band * 0.35).max(1.0);
    for i in 0..n { if sd[i] > o.band { out[i] = 1.0; continue; } if sd[i] < -o.band { out[i] = 0.0; continue; }
        let b = bin(rgb, i); let pf = 0.5 * fgc[b] + 0.5 * anchor.0[b]; let pb = 0.5 * bgc[b] + 0.5 * anchor.1[b]; let lr = (pf.ln() - pb.ln()).clamp(-4.0, 4.0);
        // keep the warped soft value as a prior too so motion-blurred edges keep their partial alpha
        let prior = (a[i].clamp(0.02, 0.98) / (1.0 - a[i].clamp(0.02, 0.98))).ln() * 0.5; let z = sd[i] / s0 + o.colour * lr + prior; out[i] = 1.0 / (1.0 + (-z).exp()); }
    // snap the soft edge to image structure, then restore the deep interior / exterior exactly
    let f = to_frame(rgb, w, h); let mut g = out.clone(); crate::masks::guided(&mut g, &f, o.guide_r, 2e-3);
    for i in 0..n { if sd[i].abs() <= o.band { out[i] = 0.35 * out[i] + 0.65 * g[i]; } } out
}
fn anchor_models(rgb: &[u8], alpha: &[f32], w: usize, h: usize, band: f32) -> (Vec<f32>, Vec<f32>) {
    let n = w * h; let inside: Vec<bool> = alpha.iter().map(|&v| v > 0.5).collect(); let outside: Vec<bool> = inside.iter().map(|b| !b).collect(); let (d_out, d_in) = (crate::masks::edt(&inside, w, h), crate::masks::edt(&outside, w, h));
    let sd: Vec<f32> = (0..n).map(|i| if inside[i] { d_in[i] - 0.5 } else { -(d_out[i] - 0.5) }).collect(); (hist(rgb, &|i| sd[i] > 4.0, n), hist(rgb, &|i| sd[i] < -3.0 && sd[i] > -band * 3.0, n)) }

fn edge_flicker(seq: &[Vec<f32>], bf: &[Fl], w: usize, h: usize) -> f64 { let (mut tot, mut cnt) = (0f64, 0f64); for t in 1..seq.len() { let wp = warp(&seq[t - 1], w, h, &bf[t]); for i in 0..w * h { if seq[t][i].max(wp[i]) > 0.02 && seq[t][i].min(wp[i]) < 0.98 { tot += (seq[t][i] - wp[i]).abs() as f64; cnt += 1.0; } } } if cnt > 0.0 { tot / cnt } else { 0.0 } }
fn area_jitter(seq: &[Vec<f32>]) -> f64 { let a: Vec<f64> = seq.iter().map(|f| f.iter().map(|&v| v as f64).sum::<f64>()).collect(); if a.len() < 3 { return 0.0; } let m = a.iter().sum::<f64>() / a.len() as f64; if m < 1.0 { return 0.0; } let mut s = 0.0; for t in 1..a.len() - 1 { s += (a[t] - 0.5 * (a[t - 1] + a[t + 1])).powi(2); } (s / (a.len() - 2) as f64).sqrt() / m }

/// `frames`: RGB u8 per frame (all frames of the range); `seeds`: (frame index within `frames`, outline in pixels).
pub fn track(frames: &[Vec<u8>], w: usize, h: usize, seeds: &[(usize, Vec<[f64; 2]>)], o: &Opts) -> Result<Out, String> {
    let nf = frames.len(); if nf < 2 || seeds.is_empty() { return Err("need at least 2 frames and 1 seed".into()); } let mut seeds = seeds.to_vec(); seeds.sort_by_key(|s| s.0); if seeds.iter().any(|s| s.0 >= nf) { return Err("seed outside the frame range".into()); }
    let fr: Vec<Frame> = vec![]; let _ = fr;
    // flows: bf[t] maps frame t -> t-1 (for forward warping of alpha), ff[t] maps t -> t+1
    let mk = |a: usize, b: usize| -> Fl { let (fa, fb) = (to_frame(&frames[a], w, h), to_frame(&frames[b], w, h)); let (u, v, w2, h2) = flow(&fa, &fb, 5); Fl { u, v, w2, h2 } };
    let mut bf: Vec<Fl> = (0..nf).map(|_| Fl { u: vec![], v: vec![], w2: 0, h2: 0 }).collect(); let mut ff: Vec<Fl> = (0..nf).map(|_| Fl { u: vec![], v: vec![], w2: 0, h2: 0 }).collect();
    for t in 1..nf { bf[t] = mk(t, t - 1); } for t in 0..nf - 1 { ff[t] = mk(t, t + 1); }
    let seed_a: Vec<Vec<f32>> = seeds.iter().map(|s| raster_poly(&s.1, w, h)).collect(); let anchors: Vec<(Vec<f32>, Vec<f32>)> = seeds.iter().enumerate().map(|(k, s)| anchor_models(&frames[s.0], &seed_a[k], w, h, o.band)).collect();
    let aff = std::env::var("KIMO_MATTE_MOTION").map(|v| v != "flow").unwrap_or(true);
    let none: Vec<Option<Vec<f32>>> = vec![None; nf]; let mut fwd: Vec<Vec<Option<Vec<f32>>>> = vec![]; let mut bwd: Vec<Vec<Option<Vec<f32>>>> = vec![];
    for (k, s) in seeds.iter().enumerate() {
        let end_f = if k + 1 < seeds.len() { seeds[k + 1].0 } else { nf - 1 }; let mut chain = none.clone(); chain[s.0] = Some(seed_a[k].clone());
        let sg: Vec<f32> = (0..w*h).map(|i| gray(&frames[s.0], i)).collect(); let mut mt = [1.0f64, 0.0, 0.0, 1.0, 0.0, 0.0]; let regs = region_masks(&seed_a[k], w, h, std::env::var("KIMO_MATTE_REGIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(6)); let mut mr = vec![[1.0f64, 0.0, 0.0, 1.0, 0.0, 0.0]; regs.len()];
        for t in s.0 + 1..=end_f { let prev = chain[t - 1].as_ref().unwrap(); let wp = if aff { let g1: Vec<f32> = (0..w*h).map(|i| gray(&frames[t], i)).collect(); pw_step(&sg, &g1, &seed_a[k], &regs, &mut mt, &mut mr, w, h) } else { warp(prev, w, h, &bf[t]) }; chain[t] = Some(refine(&wp, &frames[t], &anchors[k], w, h, o)); } fwd.push(chain);
        let end_b = if k > 0 { seeds[k - 1].0 } else { 0 }; let mut chain = none.clone(); chain[s.0] = Some(seed_a[k].clone());
        let mut mt = [1.0f64, 0.0, 0.0, 1.0, 0.0, 0.0]; let regs = region_masks(&seed_a[k], w, h, std::env::var("KIMO_MATTE_REGIONS").ok().and_then(|v| v.parse().ok()).unwrap_or(6)); let mut mr = vec![[1.0f64, 0.0, 0.0, 1.0, 0.0, 0.0]; regs.len()];
        for t in (end_b..s.0).rev() { let prev = chain[t + 1].as_ref().unwrap(); let wp = if aff { let g1: Vec<f32> = (0..w*h).map(|i| gray(&frames[t], i)).collect(); pw_step(&sg, &g1, &seed_a[k], &regs, &mut mt, &mut mr, w, h) } else { warp(prev, w, h, &ff[t]) }; chain[t] = Some(refine(&wp, &frames[t], &anchors[k], w, h, o)); } bwd.push(chain); }
    // blend: between seeds k and k+1 use forward chain of k and backward chain of k+1 by distance
    let mut blended: Vec<Vec<f32>> = Vec::with_capacity(nf); let mut conf = vec![1.0f64; nf]; let mut warnings = vec![];
    for t in 0..nf { let prev = seeds.iter().rposition(|s| s.0 <= t); let next = seeds.iter().position(|s| s.0 >= t);
        let a = match (prev, next) { (Some(p), Some(nx)) if p == nx => seed_a[p].clone(),
            (Some(p), Some(nx)) => { let (t0, t1) = (seeds[p].0 as f32, seeds[nx].0 as f32); let wgt = (t as f32 - t0) / (t1 - t0); let (fa, ba) = (fwd[p][t].as_ref().unwrap(), bwd[nx][t].as_ref().unwrap());
                let (mut inter, mut uni) = (0f64, 0f64); for i in 0..w * h { inter += fa[i].min(ba[i]) as f64; uni += fa[i].max(ba[i]) as f64; } conf[t] = if uni > 1.0 { inter / uni } else { 1.0 };
                (0..w * h).map(|i| (1.0 - wgt) * fa[i] + wgt * ba[i]).collect() }
            (Some(p), None) => fwd[p][t].clone().unwrap(), (None, Some(nx)) => bwd[nx][t].clone().unwrap(), _ => unreachable!() }; blended.push(a); }
    // decisiveness confidence outside two-seed segments
    for t in 0..nf { if seeds.iter().any(|s| s.0 == t) { continue; } let inb = seeds.iter().any(|s| s.0 < t) && seeds.iter().any(|s| s.0 > t); if !inb { let (mut e, mut c) = (0f64, 0f64); for &v in blended[t].iter() { if v > 0.02 && v < 0.98 { e += (v - 0.5).abs() as f64 * 2.0; c += 1.0; } } conf[t] = if c > 0.0 { e / c } else { 1.0 }; } }
    let low: Vec<usize> = (0..nf).filter(|&t| conf[t] < 0.8).collect(); if !low.is_empty() { warnings.push(format!("low confidence (forward/backward disagreement or undecided edge) on {} frames, first at frame {}", low.len(), low[0])); }
    let raw_flicker = edge_flicker(&blended, &bf, w, h); let raw_jitter = area_jitter(&blended);
    // temporal stabilisation: flow-compensated, photo-consistency-gated, zero-lag (forward then backward)
    let cons = |t: usize, wp_idx: usize, fl: &Fl| -> Vec<f32> { let g1: Vec<f32> = (0..w * h).map(|i| gray(&frames[wp_idx], i)).collect(); let wg = warp(&g1, w, h, fl); (0..w * h).map(|i| { let d = (gray(&frames[t], i) - wg[i]) / 0.05; (-d * d).exp() }).collect() };
    let k = o.temporal.clamp(0.0, 0.95); let mut sf = blended.clone(); let mut sb = blended.clone();
    if k > 0.0 { for t in 1..nf { if seeds.iter().any(|s| s.0 == t) { continue; } let wp = warp(&sf[t - 1], w, h, &bf[t]); let c = cons(t, t - 1, &bf[t]); for i in 0..w * h { let kk = k * c[i]; sf[t][i] = (1.0 - kk) * blended[t][i] + kk * wp[i]; } }
        for t in (0..nf - 1).rev() { if seeds.iter().any(|s| s.0 == t) { continue; } let wp = warp(&sb[t + 1], w, h, &ff[t]); let c = cons(t, t + 1, &ff[t]); for i in 0..w * h { let kk = k * c[i]; sb[t][i] = (1.0 - kk) * blended[t][i] + kk * wp[i]; } } }
    let alpha: Vec<Vec<f32>> = (0..nf).map(|t| if seeds.iter().any(|s| s.0 == t) { blended[t].clone() } else { (0..w * h).map(|i| 0.5 * (sf[t][i] + sb[t][i])).collect() }).collect();
    let (flicker, jitter) = (edge_flicker(&alpha, &bf, w, h), area_jitter(&alpha));
    Ok(Out { alpha, raw_flicker, flicker, raw_jitter, jitter, conf, warnings })
}
