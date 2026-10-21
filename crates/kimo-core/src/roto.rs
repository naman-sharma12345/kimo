//! Rotoscope assist: propagate keyframed path masks between user keys using dense optical flow, then snap the outline to image edges.
//! Memory: per segment, half-resolution gray frames only (<= 300 frames per segment).
use crate::masks::{PathKey, PathPt};
use crate::model::*;
use crate::render::Cache;

pub struct RotoStats { pub frames: usize, pub segments: usize, pub mean_snap_px: f64 }

fn bil(g: &[f32], w: usize, h: usize, x: f32, y: f32) -> f32 {
    let (x, y) = (x.clamp(0.0, w as f32 - 1.001), y.clamp(0.0, h as f32 - 1.001)); let (ix, iy) = (x as usize, y as usize); let (fx, fy) = (x - ix as f32, y - iy as f32);
    g[iy * w + ix] * (1.0 - fx) * (1.0 - fy) + g[iy * w + ix + 1] * fx * (1.0 - fy) + g[(iy + 1) * w + ix] * (1.0 - fx) * fy + g[(iy + 1) * w + ix + 1] * fx * fy
}
fn half(g: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) { let (nw, nh) = (w / 2, h / 2); let mut o = vec![0f32; nw * nh]; for y in 0..nh { for x in 0..nw { o[y * nw + x] = 0.25 * (g[2 * y * w + 2 * x] + g[2 * y * w + 2 * x + 1] + g[(2 * y + 1) * w + 2 * x] + g[(2 * y + 1) * w + 2 * x + 1]); } } (o, nw, nh) }
fn to_frame(g: &[f32], w: usize, h: usize) -> crate::effects::Frame { let mut f = crate::effects::Frame::new(w, h); for i in 0..w * h { let v = g[i] / 255.0; f.d[i * 4] = v; f.d[i * 4 + 1] = v; f.d[i * 4 + 2] = v; f.d[i * 4 + 3] = 1.0; } f }

/// Move `pts` one frame step. Edge points sit exactly where dense flow is blurred between object and background, so the step is a robust affine fit
/// to flow vectors sampled well inside the polygon (outlier-rejected); points fall back to their own flow when the interior is too small.
fn advect(pts: &mut [(f64, f64)], flow: &(Vec<f32>, Vec<f32>, usize, usize), gw: usize, sc: f64) {
    let (u, v, fw, fh) = flow; let k = gw as f64 / *fw as f64; let cell = k * sc; // flow grid px -> full px
    let fl = |x: f64, y: f64| -> (f64, f64) { let (fx, fy) = ((x / cell - 0.5) as f32, (y / cell - 0.5) as f32); (bil(u, *fw, *fh, fx, fy) as f64 * cell, bil(v, *fw, *fh, fx, fy) as f64 * cell) };
    let n = pts.len(); let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); for p in pts.iter() { x0 = x0.min(p.0); x1 = x1.max(p.0); y0 = y0.min(p.1); y1 = y1.max(p.1); }
    let inside = |x: f64, y: f64| -> bool { let mut c = false; let mut j = n - 1; for i in 0..n { let (a, b) = (pts[i], pts[j]); if ((a.1 > y) != (b.1 > y)) && (x < (b.0 - a.0) * (y - a.1) / (b.1 - a.1) + a.0) { c = !c; } j = i; } c };
    let edge_d = |x: f64, y: f64| -> f64 { let mut d = f64::MAX; for i in 0..n { let (a, b) = (pts[i], pts[(i + 1) % n]); let (ex, ey) = (b.0 - a.0, b.1 - a.1); let l2 = (ex * ex + ey * ey).max(1e-9); let t = (((x - a.0) * ex + (y - a.1) * ey) / l2).clamp(0.0, 1.0); d = d.min((x - a.0 - t * ex).hypot(y - a.1 - t * ey)); } d };
    let band = (2.5 * cell).max(0.12 * (x1 - x0).min(y1 - y0)); let step = ((x1 - x0).max(y1 - y0) / 24.0).max(cell * 0.75);
    let mut smp: Vec<(f64, f64, f64, f64)> = vec![]; let mut y = y0; while y <= y1 { let mut x = x0; while x <= x1 { if inside(x, y) && edge_d(x, y) > band { let (du, dv) = fl(x, y); smp.push((x, y, du, dv)); } x += step; } y += step; }
    if smp.len() >= 8 { let mut w = vec![1.0f64; smp.len()]; let mut sol = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        for _ in 0..4 { let (mut ata, mut atb) = ([[0f64; 3]; 3], [[0f64; 3]; 2]); for (i, s_) in smp.iter().enumerate() { let r = [s_.0, s_.1, 1.0]; let tg = [s_.0 + s_.2, s_.1 + s_.3]; for a in 0..3 { for b in 0..3 { ata[a][b] += w[i] * r[a] * r[b]; } for c in 0..2 { atb[c][a] += w[i] * r[a] * tg[c]; } } }
            for a in 0..3 { ata[a][a] += 1e-6; } let inv = inv3(ata); for c in 0..2 { for a in 0..3 { sol[c][a] = inv[a][0] * atb[c][0] + inv[a][1] * atb[c][1] + inv[a][2] * atb[c][2]; } }
            let mut res: Vec<f64> = smp.iter().map(|s_| { let px = sol[0][0] * s_.0 + sol[0][1] * s_.1 + sol[0][2]; let py = sol[1][0] * s_.0 + sol[1][1] * s_.1 + sol[1][2]; (px - s_.0 - s_.2).hypot(py - s_.1 - s_.3) }).collect();
            let rr = res.clone(); res.sort_by(|a, b| a.partial_cmp(b).unwrap()); let med = res[res.len() / 2].max(0.3 * cell); for (i, r_) in rr.iter().enumerate() { w[i] = if *r_ < 2.5 * med { 1.0 } else { 0.0 }; } }
        for p in pts.iter_mut() { let (x, y) = (p.0, p.1); p.0 = sol[0][0] * x + sol[0][1] * y + sol[0][2]; p.1 = sol[1][0] * x + sol[1][1] * y + sol[1][2]; }
    } else { for p in pts.iter_mut() { let (du, dv) = fl(p.0, p.1); p.0 += du; p.1 += dv; } }
}
fn inv3(m: [[f64; 3]; 3]) -> [[f64; 3]; 3] { let d = m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]); let d = if d.abs() < 1e-12 { 1e-12 } else { d }; let c = |a: usize, b: usize, c_: usize, d_: usize| m[a][b] * m[c_][d_] - m[a][d_] * m[c_][b];
    [[c(1, 1, 2, 2) / d, -c(0, 1, 2, 2) / d, c(0, 1, 1, 2) / d], [-c(1, 0, 2, 2) / d, c(0, 0, 2, 2) / d, -c(0, 0, 1, 2) / d], [c(1, 0, 2, 1) / d, -c(0, 0, 2, 1) / d, c(0, 0, 1, 1) / d]] }

/// Snap each outline point along its normal to the strongest edge within `snap` px. Returns mean absolute move.
fn snap_points(pts: &mut Vec<(f64, f64)>, g: &[f32], gw: usize, gh: usize, sc: f64, snap: f64) -> f64 {
    let n = pts.len(); if n < 3 || snap <= 0.0 { return 0.0; } let mut off = vec![0f64; n];
    for i in 0..n { let (a, b) = (pts[(i + n - 1) % n], pts[(i + 1) % n]); let (tx, ty) = (b.0 - a.0, b.1 - a.1); let l = (tx * tx + ty * ty).sqrt().max(1e-9); let (nx, ny) = (ty / l, -tx / l);
        let samp = |o: f64| -> f32 { bil(g, gw, gh, ((pts[i].0 + nx * o) / sc - 0.5) as f32, ((pts[i].1 + ny * o) / sc - 0.5) as f32) };
        let (mut best, mut bo) = (0f32, 0f64); let steps = (snap / sc).ceil().max(1.0) as i32; let st = snap / steps as f64; let base = ((samp(st) - samp(-st)).abs()) / 2.0;
        for s in -steps..=steps { let o = s as f64 * st; let gr = (samp(o + st) - samp(o - st)).abs() * (1.0 - 0.5 * (o.abs() / snap) as f32); if gr > best { best = gr; bo = o; } }
        off[i] = if best > base * 1.3 + 2.0 { bo } else { 0.0 }; }
    let sm: Vec<f64> = (0..n).map(|i| { let mut v = [off[(i + n - 1) % n], off[i], off[(i + 1) % n]]; v.sort_by(|a, b| a.partial_cmp(b).unwrap()); 0.6 * v[1] + 0.4 * off[i] }).collect();
    let mut tot = 0.0; for i in 0..n { let (a, b) = (pts[(i + n - 1) % n], pts[(i + 1) % n]); let (tx, ty) = (b.0 - a.0, b.1 - a.1); let l = (tx * tx + ty * ty).sqrt().max(1e-9); pts[i].0 += ty / l * sm[i]; pts[i].1 += -tx / l * sm[i]; tot += sm[i].abs(); }
    tot / n as f64
}

pub fn propagate(p: &Project, clip_id: &str, keys: &[PathKey], snap: f64, quality: usize, tau: f64) -> Result<(Vec<PathKey>, RotoStats), String> {
    let c = p.find_clip(clip_id).ok_or("no such clip")?.clone(); let mut cache = Cache::new(); let (w, h) = (p.width as usize, p.height as usize);
    if keys.len() < 2 { return Err("need at least two keyframes (hold-free path keys with equal point counts)".into()); }
    let n0 = keys[0].points.len(); if keys.iter().any(|k| k.points.len() != n0) { return Err("all keys must have the same point count".into()); }
    let fidx = |t: f64| (t * p.fps).round().max(0.0) as usize; let mut out: Vec<PathKey> = keys.to_vec(); let mut stats = RotoStats { frames: 0, segments: 0, mean_snap_px: 0.0 }; let mut snap_sum = 0.0; let mut snap_n = 0usize;
    for seg in keys.windows(2) { let (ka, kb) = (&seg[0], &seg[1]); let (fa, fb) = (fidx(ka.t), fidx(kb.t)); if fb <= fa + 1 { continue; } if fb - fa > 300 { return Err("segment longer than 300 frames; add intermediate keys".into()); }
        let mut grays: Vec<(Vec<f32>, usize, usize)> = vec![]; for f in fa..=fb { let g = crate::tracking::clip_gray_pub(p, &mut cache, &c, f)?; grays.push(half(&g, w, h)); crate::guard::check(); }
        let (gw, gh) = (grays[0].1, grays[0].2); let sc = w as f64 / gw as f64; let m = fb - fa;
        let px = |k: &PathKey| -> Vec<(f64, f64)> { k.points.iter().map(|q| (q.x * w as f64, q.y * h as f64)).collect() };
        let mut fwd: Vec<Vec<(f64, f64)>> = vec![px(ka)]; for i in 0..m { let fl = crate::flow::flow(&to_frame(&grays[i].0, gw, gh), &to_frame(&grays[i + 1].0, gw, gh), quality); let mut q = fwd[i].clone(); advect(&mut q, &fl, gw, sc); fwd.push(q); }
        let mut bwd: Vec<Vec<(f64, f64)>> = vec![vec![]; m + 1]; bwd[m] = px(kb); for i in (0..m).rev() { let fl = crate::flow::flow(&to_frame(&grays[i + 1].0, gw, gh), &to_frame(&grays[i].0, gw, gh), quality); let mut q = bwd[i + 1].clone(); advect(&mut q, &fl, gw, sc); bwd[i] = q; }
        for i in 1..m { let u = i as f64 / m as f64; let lin: Vec<(f64, f64)> = (0..n0).map(|j| (((1.0 - u) * ka.points[j].x + u * kb.points[j].x) * w as f64, ((1.0 - u) * ka.points[j].y + u * kb.points[j].y) * h as f64)).collect();
            let fb: Vec<(f64, f64)> = (0..n0).map(|j| ((1.0 - u) * fwd[i][j].0 + u * bwd[i][j].0, (1.0 - u) * fwd[i][j].1 + u * bwd[i][j].1)).collect();
            // forward and backward chains must agree where the flow is right; where they disagree, fall back toward plain key interpolation
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); for q in &lin { x0 = x0.min(q.0); x1 = x1.max(q.0); y0 = y0.min(q.1); y1 = y1.max(q.1); } let size = (x1 - x0).max(y1 - y0).max(8.0);
            let dis = (0..n0).map(|j| (fwd[i][j].0 - bwd[i][j].0).hypot(fwd[i][j].1 - bwd[i][j].1)).sum::<f64>() / n0 as f64; let wf = (-(dis / (tau * size)).powi(2)).exp();
            let mut pts: Vec<(f64, f64)> = (0..n0).map(|j| (wf * fb[j].0 + (1.0 - wf) * lin[j].0, wf * fb[j].1 + (1.0 - wf) * lin[j].1)).collect();
            snap_sum += snap_points(&mut pts, &grays[i].0, gw, gh, sc, snap); snap_n += 1;
            let t = (fa + i) as f64 / p.fps; let points: Vec<PathPt> = (0..n0).map(|j| { let (a, b) = (&ka.points[j], &kb.points[j]); let l = |x: f64, y: f64| x + (y - x) * u; PathPt { x: pts[j].0 / w as f64, y: pts[j].1 / h as f64, ix: l(a.ix, b.ix), iy: l(a.iy, b.iy), ox: l(a.ox, b.ox), oy: l(a.oy, b.oy), f: l(a.f, b.f), fi: l(a.fi, b.fi), fo: l(a.fo, b.fo) } }).collect();
            out.retain(|k| (k.t - t).abs() > 1e-9); out.push(PathKey { t, points, hold: false }); stats.frames += 1; }
        stats.segments += 1; }
    out.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap()); stats.mean_snap_px = if snap_n > 0 { snap_sum / snap_n as f64 } else { 0.0 }; Ok((out, stats))
}
