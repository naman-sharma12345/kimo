//! Dense optical flow (coarse-to-fine Lucas-Kanade with flow smoothing) and flow-based motion blur for footage.
use crate::effects::Frame;
use crate::masks::box_pass;

fn gray(f: &Frame) -> Vec<f32> { (0..f.w * f.h).map(|i| 0.299 * f.d[i * 4] + 0.587 * f.d[i * 4 + 1] + 0.114 * f.d[i * 4 + 2]).collect() }
fn down(src: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) { let (nw, nh) = ((w / 2).max(1), (h / 2).max(1)); let mut o = vec![0f32; nw * nh];
    for y in 0..nh { for x in 0..nw { let (x0, y0) = ((x * 2).min(w - 1), (y * 2).min(h - 1)); let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1)); o[y * nw + x] = 0.25 * (src[y0 * w + x0] + src[y0 * w + x1] + src[y1 * w + x0] + src[y1 * w + x1]); } } (o, nw, nh) }
fn bil(src: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 { let x = x.clamp(0.0, w as f32 - 1.001); let y = y.clamp(0.0, h as f32 - 1.001); let (x0, y0) = (x as usize, y as usize); let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let (a, b, c, d) = (src[y0 * w + x0], src[y0 * w + x0 + 1], src[(y0 + 1) * w + x0], src[(y0 + 1) * w + x0 + 1]); a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy }
fn up_flow(u: &[f32], w: usize, h: usize, nw: usize, nh: usize) -> Vec<f32> { let mut o = vec![0f32; nw * nh]; for y in 0..nh { for x in 0..nw { o[y * nw + x] = 2.0 * bil(u, w, h, (x as f32 + 0.5) / 2.0 - 0.5, (y as f32 + 0.5) / 2.0 - 0.5); } } o }
fn boxf(src: &[f32], w: usize, h: usize, r: usize) -> Vec<f32> { let mut t = vec![0f32; w * h]; let mut o = vec![0f32; w * h]; let r = r.min(w.min(h) / 2 - 1).max(1); box_pass(src, &mut t, w, h, r, true); box_pass(&t, &mut o, w, h, r, false); o }

/// Forward flow from `a` to `b` at HALF resolution: (u, v, w, h). Pixel units of the half-res grid.
pub fn flow(a: &Frame, b: &Frame, quality: usize) -> (Vec<f32>, Vec<f32>, usize, usize) {
    let (w0, h0) = (a.w, a.h); let (g0a, w1, h1) = down(&gray(a), w0, h0); let (g0b, _, _) = down(&gray(b), w0, h0);
    let levels = quality.clamp(2, 6); let mut pa = vec![(g0a, w1, h1)]; let mut pb = vec![(g0b, w1, h1)];
    for _ in 1..levels { let (la, lw, lh) = pa.last().unwrap().clone(); if lw < 24 || lh < 24 { break; } pa.push(down(&la, lw, lh)); let (lb, _, _) = pb.last().unwrap().clone(); pb.push(down(&lb, lw, lh)); }
    let n = pa.len(); let (mut u, mut v): (Vec<f32>, Vec<f32>) = (vec![], vec![]);
    for lvl in (0..n).rev() { let (ia, w, h) = (&pa[lvl].0, pa[lvl].1, pa[lvl].2); let ib = &pb[lvl].0;
        if lvl == n - 1 { u = vec![0.0; w * h]; v = vec![0.0; w * h]; } else { let (pw, ph) = (pa[lvl + 1].1, pa[lvl + 1].2); u = up_flow(&u, pw, ph, w, h); v = up_flow(&v, pw, ph, w, h); }
        for _ in 0..4 {
            let mut bw = vec![0f32; w * h]; { let (uu, vv) = (&u, &v); crate::par::rows_mut(&mut bw, w, |y, row| for x in 0..w { let i = y * w + x; row[x] = bil(ib, w, h, x as f32 + uu[i], y as f32 + vv[i]); }); }
            let (mut ixx, mut ixy, mut iyy, mut ixt, mut iyt) = (vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h], vec![0f32; w * h]);
            { use crate::par::SyncPtr as P; let (p1, p2, p3, p4, p5) = (P(ixx.as_mut_ptr()), P(ixy.as_mut_ptr()), P(iyy.as_mut_ptr()), P(ixt.as_mut_ptr()), P(iyt.as_mut_ptr())); let bwr = &bw;
            crate::par::ranges(h, 8, |y0, y1| { let (p1, p2, p3, p4, p5) = (p1, p2, p3, p4, p5); for y in y0..y1 { for x in 0..w { let i = y * w + x; let (xl, xr, yu, yd) = (x.saturating_sub(1), (x + 1).min(w - 1), y.saturating_sub(1), (y + 1).min(h - 1));
                let gx = 0.5 * ((ia[y * w + xr] - ia[y * w + xl]) + (bwr[y * w + xr] - bwr[y * w + xl])) * 0.5; let gy = 0.5 * ((ia[yd * w + x] - ia[yu * w + x]) + (bwr[yd * w + x] - bwr[yu * w + x])) * 0.5; let it = bwr[i] - ia[i];
                unsafe { *p1.0.add(i) = gx * gx; *p2.0.add(i) = gx * gy; *p3.0.add(i) = gy * gy; *p4.0.add(i) = gx * it; *p5.0.add(i) = gy * it; } } } }); }
            let r = 5; let (sxx, sxy, syy, sxt, syt) = (boxf(&ixx, w, h, r), boxf(&ixy, w, h, r), boxf(&iyy, w, h, r), boxf(&ixt, w, h, r), boxf(&iyt, w, h, r)); let lam = 2e-5;
            { let (pu, pv) = (crate::par::SyncPtr(u.as_mut_ptr()), crate::par::SyncPtr(v.as_mut_ptr())); let (sxx, sxy, syy, sxt, syt) = (&sxx, &sxy, &syy, &sxt, &syt);
            crate::par::ranges(w * h, 4096, |i0, i1| { let (pu, pv) = (pu, pv); for i in i0..i1 { let (a_, b_, c_) = (sxx[i] + lam, sxy[i], syy[i] + lam); let det = a_ * c_ - b_ * b_; if det.abs() < 1e-12 { continue; } let (du, dv) = (-(c_ * sxt[i] - b_ * syt[i]) / det, -(-b_ * sxt[i] + a_ * syt[i]) / det);
                let lim = 4.0; unsafe { *pu.0.add(i) += du.clamp(-lim, lim); *pv.0.add(i) += dv.clamp(-lim, lim); } } }); }
            u = boxf(&u, w, h, 1); v = boxf(&v, w, h, 1); } }
    (u, v, pa[0].1, pa[0].2)
}

/// Motion blur along per-pixel flow: `amount` = shutter as a fraction of the frame interval (0.5 = 180 degrees). The flow is centred on the frame.
pub fn flow_blur(cur: &Frame, next: &Frame, amount: f32, quality: usize) -> Frame {
    let (u, v, fw, fh) = flow(cur, next, quality); let (w, h) = (cur.w, cur.h); let (sx, sy) = (fw as f32 / w as f32, fh as f32 / h as f32); let mut o = Frame::new(w, h);
    let (uu, vv, cd) = (&u, &v, &cur.d); crate::par::rows_mut(&mut o.d, w * 4, |y, orow| { for x in 0..w { let (fx, fy) = (((x as f32 + 0.5) * sx - 0.5).max(0.0), ((y as f32 + 0.5) * sy - 0.5).max(0.0)); let (mu, mv) = (bil(uu, fw, fh, fx, fy) / sx * amount, bil(vv, fw, fh, fx, fy) / sy * amount);
        let len = (mu * mu + mv * mv).sqrt(); let ns = ((len / 1.2) as usize + 1).clamp(1, 24); let mut acc = [0f32; 4];
        for k in 0..ns { let s = if ns == 1 { 0.0 } else { k as f32 / (ns - 1) as f32 - 0.5 }; let (px, py) = ((x as f32 + mu * s).clamp(0.0, w as f32 - 1.0), (y as f32 + mv * s).clamp(0.0, h as f32 - 1.0)); let (x0, y0) = (px as usize, py as usize); let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1)); let (ax, ay) = (px - x0 as f32, py - y0 as f32);
            for j in 0..4 { let a = cd[(y0 * w + x0) * 4 + j]; let b = cd[(y0 * w + x1) * 4 + j]; let c = cd[(y1 * w + x0) * 4 + j]; let d = cd[(y1 * w + x1) * 4 + j]; acc[j] += a + (b - a) * ax + (c - a) * ay + (a - b - c + d) * ax * ay; } }
        for j in 0..4 { orow[x * 4 + j] = acc[j] / ns as f32; } } });
    o
}

fn fetch4(d: &[f32], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] { let (px, py) = (x.clamp(0.0, w as f32 - 1.0), y.clamp(0.0, h as f32 - 1.0)); let (x0, y0) = (px as usize, py as usize); let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1)); let (ax, ay) = (px - x0 as f32, py - y0 as f32); let mut o = [0f32; 4];
    for j in 0..4 { let (a, b, c, e) = (d[(y0 * w + x0) * 4 + j], d[(y0 * w + x1) * 4 + j], d[(y1 * w + x0) * 4 + j], d[(y1 * w + x1) * 4 + j]); o[j] = a + (b - a) * ax + (c - a) * ay + (a - b - c + e) * ax * ay; } o }
fn rgb_diff(a: &[f32], b: &[f32]) -> f32 { (a[0] - b[0]).abs() + (a[1] - b[1]).abs() + (a[2] - b[2]).abs() }
/// Full-res velocity (px per frame) of `a` towards `b`: half-res LK flow upsampled bilinearly and rescaled.
fn full_flow(a: &Frame, b: &Frame, quality: usize) -> (Vec<f32>, Vec<f32>) { let (u, v, fw, fh) = flow(a, b, quality); let (w, h) = (a.w, a.h); let (sx, sy) = (fw as f32 / w as f32, fh as f32 / h as f32); let (mut ou, mut ov) = (vec![0f32; w * h], vec![0f32; w * h]);
    for y in 0..h { for x in 0..w { let (fx, fy) = (((x as f32 + 0.5) * sx - 0.5).max(0.0), ((y as f32 + 0.5) * sy - 0.5).max(0.0)); ou[y * w + x] = bil(&u, fw, fh, fx, fy) / sx; ov[y * w + x] = bil(&v, fw, fh, fx, fy) / sy; } } (ou, ov) }

/// Displaced RGB error of the `cur` pixel colour `c` at (x, y) moving by (tx, ty) px per frame: min over next at +t and prev at -t (+inf with no neighbour).
fn disp_err(c: &[f32], prev: Option<&Frame>, next: Option<&Frame>, w: usize, h: usize, x: f32, y: f32, tx: f32, ty: f32) -> f32 { next.map(|f| rgb_diff(c, &fetch4(&f.d, w, h, x + tx, y + ty))).unwrap_or(f32::INFINITY).min(prev.map(|f| rgb_diff(c, &fetch4(&f.d, w, h, x - tx, y - ty))).unwrap_or(f32::INFINITY)) }
/// 8-connected components of `m` dilated once (3x3): (labels, count), label 0 = background, 1..=count otherwise. Iterative flood fill with an explicit stack.
fn components(m: &[bool], w: usize, h: usize) -> (Vec<u32>, u32) { let mut d = vec![false; w * h];
    for y in 0..h { for x in 0..w { if m[y * w + x] { for yy in y.saturating_sub(1)..(y + 2).min(h) { for xx in x.saturating_sub(1)..(x + 2).min(w) { d[yy * w + xx] = true; } } } } }
    let (mut lab, mut nl, mut st) = (vec![0u32; w * h], 0u32, Vec::new());
    for s0 in 0..w * h { if !d[s0] || lab[s0] != 0 { continue; } nl += 1; lab[s0] = nl; st.push(s0);
        while let Some(i) = st.pop() { let (x, y) = (i % w, i / w); for yy in y.saturating_sub(1)..(y + 2).min(h) { for xx in x.saturating_sub(1)..(x + 2).min(w) { let j = yy * w + xx; if d[j] && lab[j] == 0 { lab[j] = nl; st.push(j); } } } } }
    (lab, nl) }
/// Square dilation of a bw x bh bool mask by `k` px: separable max via running counts (horizontal pass, then vertical), O(bw*bh) independent of k.
fn dilate_sq(m: &[bool], bw: usize, bh: usize, k: usize) -> Vec<bool> { let (mut hz, mut o) = (vec![false; bw * bh], vec![false; bw * bh]);
    for y in 0..bh { let r = &m[y * bw..(y + 1) * bw]; let mut c = r[..k.min(bw)].iter().filter(|&&b| b).count(); for x in 0..bw { if x + k < bw && r[x + k] { c += 1; } if x > k && r[x - k - 1] { c -= 1; } hz[y * bw + x] = c > 0; } }
    for x in 0..bw { let mut c = (0..k.min(bh)).filter(|&y| hz[y * bw + x]).count(); for y in 0..bh { if y + k < bh && hz[(y + k) * bw + x] { c += 1; } if y > k && hz[(y - k - 1) * bw + x] { c -= 1; } o[y * bw + x] = c > 0; } }
    o }
/// Gauss-Newton fit of one object translation (px per frame) minimising the RGB photometric error of the `core` pixels against next (at +t) and prev (at -t), seeded with `t0`.
/// Up to 10 iterations, steps clamped to +-2 px; None if the normal equations are singular or t is not finite.
fn refine_t(core: &[usize], cur: &Frame, prev: Option<&Frame>, next: Option<&Frame>, t0: (f64, f64)) -> Option<(f64, f64)> { let (w, h) = (cur.w, cur.h); let (mut tx, mut ty) = t0;
    for _ in 0..10 { let (mut a, mut b, mut c, mut rx, mut ry) = (0f64, 0f64, 0f64, 0f64, 0f64);
        for &i in core { let (x, y) = ((i % w) as f32, (i / w) as f32); let cd = &cur.d[i * 4..i * 4 + 4];
            for (f, sg) in [(next, 1.0f32), (prev, -1.0)] { let Some(f) = f else { continue }; let (px, py) = (x + sg * tx as f32, y + sg * ty as f32);
                let (v, xp, xm, yp, ym) = (fetch4(&f.d, w, h, px, py), fetch4(&f.d, w, h, px + 0.5, py), fetch4(&f.d, w, h, px - 0.5, py), fetch4(&f.d, w, h, px, py + 0.5), fetch4(&f.d, w, h, px, py - 0.5));
                for ch in 0..3 { let r = (v[ch] - cd[ch]) as f64; let (jx, jy) = ((sg * (xp[ch] - xm[ch])) as f64, (sg * (yp[ch] - ym[ch])) as f64); a += jx * jx; b += jx * jy; c += jy * jy; rx += jx * r; ry += jy * r; } } }
        let (a, c) = (a + 1e-9, c + 1e-9); let det = a * c - b * b; if !(det > 1e-12 * a * c) { return None; }
        let (dx, dy) = ((-(c * rx - b * ry) / det).clamp(-2.0, 2.0), (-(a * ry - b * rx) / det).clamp(-2.0, 2.0)); tx += dx; ty += dy; if !(tx.is_finite() && ty.is_finite()) { return None; } if dx.abs().max(dy.abs()) < 1e-3 { break; } }
    Some((tx, ty)) }
/// Per-object refinement of the static/moving labels of `vector_blur`: every component (>= 8 moving px) of the dilated moving mask gets a Gauss-Newton translation; pixels
/// within its moving pixels dilated by ceil(max |t|) + 2 are re-tested (object translation vs own LK velocity vs static). Components run in label order, a later one overrides
/// an earlier one only with a strictly lower model error; pixels outside every region keep their labels. Updates vx, vy, is_mov in place (velocity 0 when static).
fn refine_objects(prev: Option<&Frame>, cur: &Frame, next: Option<&Frame>, vx: &mut [f32], vy: &mut [f32], is_mov: &mut [bool]) { let (w, h) = (cur.w, cur.h); let n = w * h;
    let (lab, nl) = components(is_mov, w, h); let mut px: Vec<Vec<usize>> = vec![vec![]; nl as usize + 1]; for i in 0..n { if is_mov[i] { px[lab[i] as usize].push(i); } }
    let (ovx, ovy, omov) = (vx.to_vec(), vy.to_vec(), is_mov.to_vec()); let mut best = vec![f32::INFINITY; n];
    let es = |i: usize| [next, prev].iter().flatten().map(|f| rgb_diff(&cur.d[i * 4..i * 4 + 4], &f.d[i * 4..i * 4 + 4])).fold(f32::INFINITY, f32::min);
    let med = |mut v: Vec<f32>| { v.sort_by(|a, b| a.total_cmp(b)); v[v.len() / 2] as f64 };
    let full = |i: usize| { let (x, y) = (i % w, i / w); x > 0 && y > 0 && x + 1 < w && y + 1 < h && (y - 1..y + 2).all(|yy| (x - 1..x + 2).all(|xx| omov[yy * w + xx])) };
    for p in px.iter().skip(1) { if p.len() < 8 { continue; } let t0 = (med(p.iter().map(|&i| ovx[i]).collect()), med(p.iter().map(|&i| ovy[i]).collect()));
        let mut core: Vec<usize> = p.iter().copied().filter(|&i| full(i)).collect(); if core.len() < 8 { core = p.clone(); }
        let Some((tx, ty)) = refine_t(&core, cur, prev, next, t0) else { continue }; let (tx, ty) = (tx as f32, ty as f32); if !(tx.is_finite() && ty.is_finite()) { continue; }
        let k = (tx.abs().max(ty.abs()).ceil() as usize + 2).min(w.max(h)); let (mut bx0, mut by0, mut bx1, mut by1) = (w, h, 0, 0); for &i in p { let (x, y) = (i % w, i / w); bx0 = bx0.min(x); by0 = by0.min(y); bx1 = bx1.max(x); by1 = by1.max(y); }
        let (x0, y0, x1, y1) = (bx0.saturating_sub(k), by0.saturating_sub(k), (bx1 + k + 1).min(w), (by1 + k + 1).min(h)); let (bw, bh) = (x1 - x0, y1 - y0);
        let mut lm = vec![false; bw * bh]; for &i in p { lm[(i / w - y0) * bw + i % w - x0] = true; } let reg = dilate_sq(&lm, bw, bh, k);
        for ly in 0..bh { for lx in 0..bw { if !reg[ly * bw + lx] { continue; } let (x, y) = (x0 + lx, y0 + ly); let i = y * w + x; let c = &cur.d[i * 4..i * 4 + 4];
            let em = disp_err(c, prev, next, w, h, x as f32, y as f32, tx, ty); if !(em < best[i]) { continue; } best[i] = em; let e0 = es(i);
            let eo = if omov[i] { disp_err(c, prev, next, w, h, x as f32, y as f32, ovx[i], ovy[i]) } else { f32::INFINITY };
            if em + 0.01 < e0 && em <= eo { is_mov[i] = true; vx[i] = tx; vy[i] = ty; } else if omov[i] && eo + 0.01 < e0 { is_mov[i] = true; vx[i] = ovx[i]; vy[i] = ovy[i]; } else { is_mov[i] = false; } } } }
    for i in 0..n { if !is_mov[i] { vx[i] = 0.0; vy[i] = 0.0; } } }

/// Per-pixel vector motion blur with occlusion handling (ReelSmart Motion Blur / Nuke VectorBlur class). `amount` = shutter length in frames (0.5 = 180 degrees),
/// `phase` = shutter open offset in frames relative to the frame time (centred = -amount/2), `curve` = shutter efficiency over the open interval.
/// Velocity is the central estimate 0.5*(flow(cur,next) - flow(cur,prev)) (one-sided when a neighbour is missing). Each pixel is classified static or moving by a photometric
/// hypothesis test (does it match the neighbour frames better in place or displaced along its velocity?), so the static background is never smeared. The smoothed LK
/// velocities are then refined per object: each connected moving region gets one translation fitted by Gauss-Newton on the prev/next photometric error (seeded with the median
/// velocity), and every pixel near the object is re-tested against that translation; non-rigid parts keep their own LK velocity when it matches better. Per shutter sample the
/// moving pixels are forward-splatted (bilinear) over a background plate in which the area the foreground left is filled from the neighbour frame it moves towards; the
/// samples are summed with the curve weights. Returns `cur` unchanged when nothing moves. Gaps: accuracy is bounded by the coarse LK flow (fast motion, aperture, thin
/// structures); transparent, reflective and rotating/deforming objects get one translation per pixel per frame; two moving objects crossing use speed order (pixels are
/// splatted slowest first and a clearly faster splat replaces what is already there), not true depth; revealed background taken from prev/next can carry that frame's motion.
pub fn vector_blur(prev: Option<&Frame>, cur: &Frame, next: Option<&Frame>, amount: f32, phase: f32, quality: usize, curve: &crate::shutter::Curve) -> Frame {
    let (w, h) = (cur.w, cur.h); let n = w * h; let same = || Frame { w, h, d: cur.d.clone() };
    if !(amount > 0.0) || !phase.is_finite() || (prev.is_none() && next.is_none()) || n == 0 { return same(); }
    let fl = next.map(|b| full_flow(cur, b, quality)); let bl = prev.map(|b| full_flow(cur, b, quality)); let (mut vx, mut vy) = (vec![0f32; n], vec![0f32; n]);
    for i in 0..n { let (x, y) = match (&fl, &bl) { (Some(f), Some(b)) => (0.5 * (f.0[i] - b.0[i]), 0.5 * (f.1[i] - b.1[i])), (Some(f), None) => (f.0[i], f.1[i]), (None, Some(b)) => (-b.0[i], -b.1[i]), _ => (0.0, 0.0) }; vx[i] = if x.is_finite() { x } else { 0.0 }; vy[i] = if y.is_finite() { y } else { 0.0 }; }
    // Static / moving hypothesis test: a pixel moves only if the displaced match beats the in-place match.
    let mut mov: Vec<(f32, usize)> = vec![]; let mut is_mov = vec![false; n];
    for y in 0..h { for x in 0..w { let i = y * w + x; let sp = (vx[i] * vx[i] + vy[i] * vy[i]).sqrt(); if sp <= 0.25 { continue; } let c = &cur.d[i * 4..i * 4 + 4];
        let es = [next, prev].iter().flatten().map(|f| rgb_diff(c, &f.d[i * 4..i * 4 + 4])).fold(f32::INFINITY, f32::min);
        let em = next.map(|f| rgb_diff(c, &fetch4(&f.d, w, h, x as f32 + vx[i], y as f32 + vy[i]))).unwrap_or(f32::INFINITY).min(prev.map(|f| rgb_diff(c, &fetch4(&f.d, w, h, x as f32 - vx[i], y as f32 - vy[i]))).unwrap_or(f32::INFINITY));
        if em + 0.01 < es { is_mov[i] = true; mov.push((sp, i)); } } }
    // Per-object translation refinement; static footage (nothing moving) skips it and stays bit-identical.
    if !mov.is_empty() { refine_objects(prev, cur, next, &mut vx, &mut vy, &mut is_mov); mov = (0..n).filter(|&i| is_mov[i]).map(|i| ((vx[i] * vx[i] + vy[i] * vy[i]).sqrt(), i)).collect(); }
    if mov.is_empty() { return same(); }
    mov.sort_by(|a, b| a.0.total_cmp(&b.0)); let vmax = mov.last().unwrap().0;
    let ns = (((vmax * amount * 2.0).ceil() as usize).max(1).max(quality * 2)).clamp(1, 64);
    let mut out = vec![0f32; n * 4]; let (mut acc, mut s, mut zt) = (vec![0f32; n * 4], vec![0f32; n], vec![0f32; n]);
    for (tau, wk) in crate::shutter::weighted_midpoints(phase as f64, amount as f64, ns, curve) { let (tau, wk) = (tau as f32, wk as f32); acc.fill(0.0); s.fill(0.0); zt.fill(0.0);
        // Slowest first; a splat clearly faster than what already landed on a pixel replaces it (faster = in front).
        for &(sp, i) in &mov { let (px, py) = ((i % w) as f32 + vx[i] * tau, (i / w) as f32 + vy[i] * tau); if !(px > -1.0 && py > -1.0 && px < w as f32 && py < h as f32) { continue; }
            let (x0, y0) = (px.floor(), py.floor()); let (ax, ay) = (px - x0, py - y0);
            for (dx, dy, wt) in [(0.0, 0.0, (1.0 - ax) * (1.0 - ay)), (1.0, 0.0, ax * (1.0 - ay)), (0.0, 1.0, (1.0 - ax) * ay), (1.0, 1.0, ax * ay)] { let (tx, ty) = (x0 + dx, y0 + dy); if wt <= 0.0 || tx < 0.0 || ty < 0.0 || tx >= w as f32 || ty >= h as f32 { continue; } let t = ty as usize * w + tx as usize;
                if sp > zt[t] + 0.5 { if s[t] > 0.0 { acc[t * 4..t * 4 + 4].fill(0.0); s[t] = 0.0; } zt[t] = sp; } for j in 0..4 { acc[t * 4 + j] += cur.d[i * 4 + j] * wt; } s[t] += wt; } }
        let bgf = if tau >= 0.0 { next } else { prev };
        for t in 0..n { let bg = if is_mov[t] { bgf.map(|f| &f.d[t * 4..t * 4 + 4]).unwrap_or(&cur.d[t * 4..t * 4 + 4]) } else { &cur.d[t * 4..t * 4 + 4] }; let a = s[t].min(1.0);
            for j in 0..4 { let fg = if s[t] > 1e-6 { acc[t * 4 + j] / s[t] } else { 0.0 }; out[t * 4 + j] += wk * (fg * a + bg[j] * (1.0 - a)); } } }
    Frame { w, h, d: out }
}

/// Colour-wheel visualisation of a flow field (hue = direction, brightness = magnitude / max).
pub fn flow_viz(u: &[f32], v: &[f32], w: usize, h: usize, maxmag: f32) -> Frame { let mut o = Frame::new(w, h);
    for i in 0..w * h { let (a, b) = (u[i], v[i]); let m = ((a * a + b * b).sqrt() / maxmag).min(1.0); let hue = (b.atan2(a) / std::f32::consts::TAU + 1.0).fract() * 6.0; let k = hue.floor() as i32; let f = hue - k as f32;
        let (r, g, bl) = match k % 6 { 0 => (1.0, f, 0.0), 1 => (1.0 - f, 1.0, 0.0), 2 => (0.0, 1.0, f), 3 => (0.0, 1.0 - f, 1.0), 4 => (f, 0.0, 1.0), _ => (1.0, 0.0, 1.0 - f) }; o.d[i * 4] = r * m; o.d[i * 4 + 1] = g * m; o.d[i * 4 + 2] = bl * m; o.d[i * 4 + 3] = 1.0; } o }
