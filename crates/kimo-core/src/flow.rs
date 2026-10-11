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

/// Colour-wheel visualisation of a flow field (hue = direction, brightness = magnitude / max).
pub fn flow_viz(u: &[f32], v: &[f32], w: usize, h: usize, maxmag: f32) -> Frame { let mut o = Frame::new(w, h);
    for i in 0..w * h { let (a, b) = (u[i], v[i]); let m = ((a * a + b * b).sqrt() / maxmag).min(1.0); let hue = (b.atan2(a) / std::f32::consts::TAU + 1.0).fract() * 6.0; let k = hue.floor() as i32; let f = hue - k as f32;
        let (r, g, bl) = match k % 6 { 0 => (1.0, f, 0.0), 1 => (1.0 - f, 1.0, 0.0), 2 => (0.0, 1.0, f), 3 => (0.0, 1.0 - f, 1.0), 4 => (f, 0.0, 1.0), _ => (1.0, 0.0, 1.0 - f) }; o.d[i * 4] = r * m; o.d[i * 4 + 1] = g * m; o.d[i * 4 + 2] = bl * m; o.d[i * 4 + 3] = 1.0; } o }
