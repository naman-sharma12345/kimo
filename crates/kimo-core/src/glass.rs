//! Glass shatter: radial + concentric fracture around an impact point, irregular (domain-warped) shards, cracks that grow outward,
//! then shards of the OUTGOING frame fly toward the camera with rotation, tilt shading, refraction offset and rim glints,
//! revealing the incoming frame. Per-frame deterministic; cost ~ O(w*h + sum of shard bboxes).
use crate::effects::Frame;

fn hash(x: i32, y: i32, s: u32) -> f32 { let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ s.wrapping_mul(2246822519); h = (h ^ (h >> 13)).wrapping_mul(1274126177); ((h ^ (h >> 16)) & 0xffffff) as f32 / 16777215.0 }
fn sstep(x: f32) -> f32 { let t = x.clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }
fn vnoise(x: f32, y: f32, s: u32) -> f32 { let (xi, yi) = (x.floor(), y.floor()); let (fx, fy) = (sstep(x - xi), sstep(y - yi)); let (xi, yi) = (xi as i32, yi as i32);
    let a = hash(xi, yi, s) * (1.0 - fx) + hash(xi + 1, yi, s) * fx; let b = hash(xi, yi + 1, s) * (1.0 - fx) + hash(xi + 1, yi + 1, s) * fx; a * (1.0 - fy) + b * fy }

struct Fracture { ix: f32, iy: f32, radii: Vec<f32>, bounds: Vec<Vec<f32>>, offs: Vec<usize>, seed: u32, maxr: f32 }
impl Fracture {
    fn new(w: f32, h: f32, seed: u32, density: f32) -> Self {
        let (ix, iy) = (w * (0.35 + 0.3 * hash(5, 6, seed)), h * (0.3 + 0.3 * hash(7, 8, seed)));
        let maxr = ((w.max(ix) ).powi(2) + (h.max(iy)).powi(2)).sqrt() * 1.05; let rings = (9.0 * density).round().max(4.0) as usize;
        let mut radii = vec![0.0f32]; let mut r = maxr * 0.025; for i in 0..rings { radii.push(r); r *= 1.55 + 0.25 * hash(i as i32, 1, seed); if r > maxr { break; } } radii.push(maxr * 3.0);
        let mut bounds = vec![]; let mut offs = vec![]; let mut tot = 0;
        for i in 0..radii.len() - 1 { let n = ((7.0 + 3.2 * i as f32) * density).round().clamp(5.0, 64.0) as usize; let mut b: Vec<f32> = (0..n).map(|j| (j as f32 + (hash(j as i32, i as i32 + 10, seed) - 0.5) * 0.75) / n as f32 * std::f32::consts::TAU - std::f32::consts::PI).collect(); b.sort_by(|a, c| a.partial_cmp(c).unwrap()); offs.push(tot); tot += n; bounds.push(b); }
        let _ = tot; Fracture { ix, iy, radii, bounds, offs, seed, maxr }
    }
    fn count(&self) -> usize { self.offs.last().unwrap() + self.bounds.last().unwrap().len() }
    /// (shard id, edge distance in px, warped radius)
    fn at(&self, x: f32, y: f32) -> (usize, f32, f32) {
        let (dx, dy) = (x - self.ix, y - self.iy); let r0 = dx.hypot(dy); let th0 = dy.atan2(dx);
        let n1 = vnoise(x * 0.035, y * 0.035, self.seed) - 0.5; let n2 = vnoise(x * 0.09 + 31.0, y * 0.09, self.seed ^ 7) - 0.5;
        let r = r0 * (1.0 + 0.16 * n1) + 6.0 * n2; let th = th0 + 0.14 * n2 / (1.0 + r0 * 0.004);
        let mut ring = 0; while ring + 2 < self.radii.len() && r >= self.radii[ring + 1] { ring += 1; }
        let b = &self.bounds[ring]; let n = b.len(); let mut thw = th; while thw < -std::f32::consts::PI { thw += std::f32::consts::TAU; } while thw >= std::f32::consts::PI { thw -= std::f32::consts::TAU; }
        let mut j = n - 1; for (k, bb) in b.iter().enumerate() { if *bb > thw { j = if k == 0 { n - 1 } else { k - 1 }; break; } }
        let lo = b[j]; let hi = b[(j + 1) % n]; let mut dlo = thw - lo; if dlo < 0.0 { dlo += std::f32::consts::TAU; } let mut dhi = hi - thw; if dhi < 0.0 { dhi += std::f32::consts::TAU; }
        let e = (r - self.radii[ring]).min(self.radii[ring + 1] - r).min(r * dlo.min(dhi).min(1.0)).max(0.0);
        (self.offs[ring] + j, e, r0)
    }
}

pub struct Shard { cx: f32, cy: f32, delay: f32, vx: f32, vy: f32, spin: f32, z: f32, tx: f32, ty: f32, bb: [i32; 4], cnt: u32, sx: f64, sy: f64 }

pub fn compose(below: &Frame, inc: &Frame, u: f32, seed: u32, density: f32, intensity: f32) -> Frame {
    let (w, h) = (below.w, below.h); let (wf, hf) = (w as f32, h as f32); let fr = Fracture::new(wf, hf, seed, density.clamp(0.4, 2.5));
    let n = fr.count(); let mut ids = vec![0u32; w * h]; let mut edge = vec![0f32; w * h];
    let mut sh: Vec<Shard> = (0..n).map(|_| Shard { cx: 0.0, cy: 0.0, delay: 0.0, vx: 0.0, vy: 0.0, spin: 0.0, z: 0.0, tx: 0.0, ty: 0.0, bb: [i32::MAX, i32::MAX, i32::MIN, i32::MIN], cnt: 0, sx: 0.0, sy: 0.0 }).collect();
    for y in 0..h { for x in 0..w { let (id, e, _) = fr.at(x as f32 + 0.5, y as f32 + 0.5); ids[y * w + x] = id as u32; edge[y * w + x] = e; let s = &mut sh[id]; s.cnt += 1; s.sx += x as f64; s.sy += y as f64;
        s.bb[0] = s.bb[0].min(x as i32); s.bb[1] = s.bb[1].min(y as i32); s.bb[2] = s.bb[2].max(x as i32); s.bb[3] = s.bb[3].max(y as i32); } }
    for (i, s) in sh.iter_mut().enumerate() { if s.cnt == 0 { continue; } s.cx = (s.sx / s.cnt as f64) as f32; s.cy = (s.sy / s.cnt as f64) as f32; let r = hash(i as i32, 77, seed); let r2 = hash(i as i32, 78, seed); let r3 = hash(i as i32, 79, seed);
        let (dx, dy) = (s.cx - fr.ix, s.cy - fr.iy); let dist = dx.hypot(dy); let nd = dist / fr.maxr; let (ux, uy) = if dist > 1.0 { (dx / dist, dy / dist) } else { (0.0, -1.0) };
        s.delay = 0.13 + 0.34 * nd.powf(0.8) + 0.06 * r; let sp = (0.18 + 0.5 * r2) * fr.maxr * (1.1 - 0.5 * nd); s.vx = ux * sp * 0.8; s.vy = uy * sp * 0.8 + hf * 0.12; s.spin = (r3 - 0.5) * 2.0 * 140.0; s.z = 0.8 + 2.2 * (1.0 - nd) * r2 + 0.6 * r; s.tx = (r - 0.5) * 2.0; s.ty = (r3 - 0.5) * 2.0; }
    // base: incoming frame, slightly pushed back
    let mut out = Frame { w, h, d: inc.d.clone() }; for i in 0..w * h { out.d[i * 4 + 3] = 1.0; for j in 0..3 { out.d[i * 4 + j] *= 0.82 + 0.18 * sstep((u - 0.1) / 0.5); } }
    let front = (u / 0.16).min(1.0) * fr.maxr * 1.1; let mut order: Vec<usize> = (0..n).filter(|&i| sh[i].cnt > 0).collect();
    let ts = |s: &Shard| ((u - s.delay) / (1.0 - s.delay).max(0.05)).clamp(0.0, 1.0);
    order.sort_by(|&a, &b| ts(&sh[a]).partial_cmp(&ts(&sh[b])).unwrap());
    let cam = crate::three_d::Camera::default(); let fl_len = (hf / 2.0) as f64 / (cam.fov.to_radians() / 2.0).tan();
    for &i in &order { let s = &sh[i]; let t = ts(s); let e = t * t; let alpha_s = 1.0 - sstep((t - 0.6) / 0.4); if alpha_s <= 0.003 { continue; }
        let ang = [(s.ty * 150.0 * t) as f64, (s.tx * 170.0 * t) as f64, (s.spin * t) as f64];
        let zc = -((s.z * 0.55 * e + 0.0) as f64) * fl_len * 0.9; // toward camera = -z
        let (ncx, ncy) = (s.cx + s.vx * t * (0.6 + t), s.cy + s.vy * t * t * 1.4 + s.vy * t * 0.3);
        let rm = crate::three_d::rot_matrix(ang); let lc = [(s.cx - wf / 2.0) as f64, (s.cy - hf / 2.0) as f64, 0.0];
        let rl = [rm[0][0] * lc[0] + rm[0][1] * lc[1], rm[1][0] * lc[0] + rm[1][1] * lc[1], rm[2][0] * lc[0] + rm[2][1] * lc[1]];
        let wc = [(ncx - wf / 2.0) as f64, (ncy - hf / 2.0) as f64, zc];
        let pose = crate::three_d::Pose { pos: [wc[0] - rl[0], wc[1] - rl[1], wc[2] - rl[2]], rot: ang, scale: 1.0 };
        let pj = crate::three_d::Projector::new(w, h, &cam, &pose);
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN); let mut ok = true;
        for (cx, cy) in [(s.bb[0], s.bb[1]), (s.bb[2] + 1, s.bb[1]), (s.bb[0], s.bb[3] + 1), (s.bb[2] + 1, s.bb[3] + 1)] { match pj.src_to_screen(cx as f64, cy as f64) { Some((px, py, _)) => { x0 = x0.min(px as f32); x1 = x1.max(px as f32); y0 = y0.min(py as f32); y1 = y1.max(py as f32); } None => { ok = false; } } }
        if !ok { continue; } let (x0, x1, y0, y1) = ((x0.floor().max(0.0)) as usize, (x1.ceil().min(wf - 1.0)) as usize, (y0.floor().max(0.0)) as usize, (y1.ceil().min(hf - 1.0)) as usize); if x0 > x1 || y0 > y1 { continue; }
        let persp = (fl_len / (fl_len + zc).max(1.0)) as f32; let rf = (s.tx * 6.0 * t, s.ty * 6.0 * t); let shade = 1.0 + 0.3 * (ang[1].to_radians().sin() as f32) * 0.6 - 0.1 * t; let lit = (0.35 + 0.9 * (s.tx * s.ty).abs()) * (0.4 + t);
        for y in y0..=y1 { for x in x0..=x1 { let Some((sx, sy, _, _)) = pj.dest_to_src(x as f64 + 0.5, y as f64 + 0.5) else { continue }; let (sx, sy) = (sx as f32, sy as f32);
            let (ix, iy) = (sx.floor() as i32, sy.floor() as i32); if ix < 0 || iy < 0 || ix >= w as i32 || iy >= h as i32 { continue; } let o = iy as usize * w + ix as usize; if ids[o] as usize != i { continue; }
            let e_px = edge[o] * persp; let cov = (e_px + 1.0).clamp(0.0, 1.0) * alpha_s * if t > 0.0 { 0.9 } else { 1.0 }; if cov <= 0.0 { continue; }
            let (rx, ry) = ((sx + rf.0).clamp(0.0, wf - 1.001), (sy + rf.1).clamp(0.0, hf - 1.001)); let (fx, fy) = (rx.fract(), ry.fract()); let (x_i, y_i) = (rx as usize, ry as usize);
            let g = |xx: usize, yy: usize, c: usize| below.d[(yy.min(h - 1) * w + xx.min(w - 1)) * 4 + c]; let mut col = [0f32; 3];
            for c in 0..3 { col[c] = (g(x_i, y_i, c) * (1.0 - fx) + g(x_i + 1, y_i, c) * fx) * (1.0 - fy) + (g(x_i, y_i + 1, c) * (1.0 - fx) + g(x_i + 1, y_i + 1, c) * fx) * fy; col[c] *= shade; }
            let r_src = ((sx - fr.ix).powi(2) + (sy - fr.iy).powi(2)).sqrt(); let vis = sstep((front - r_src) / 40.0); let line = (2.2 - e_px).clamp(0.0, 1.0); let core = (1.0 - e_px / 0.9).clamp(0.0, 1.0);
            let glint = (line * 0.55 + core) * vis * lit.max(0.55).min(1.6) * intensity; for c in 0..3 { col[c] = col[c] * (1.0 - 0.35 * line * vis) + glint * [0.95, 0.98, 1.0][c] * 0.9; }
            if t > 0.0 { let ph = (s.tx * 3.1 + s.ty * 1.7).abs(); let band = ((sx * ph.cos() + sy * ph.sin()) * 0.045 + t * 5.0 * (0.5 + s.tx.abs()) + ph * 6.0).sin(); let spec = band.max(0.0).powi(10) * 0.55 * (0.4 + t); for c in 0..3 { col[c] = col[c] * 0.96 + 0.05 * [0.55, 0.75, 1.0][c] * (0.6 + 0.4 * t) + spec * [1.0, 1.0, 1.0][c]; } }
            let oo = (y * w + x) * 4; for c in 0..3 { out.d[oo + c] = out.d[oo + c] * (1.0 - cov) + col[c] * cov; } } } }
    // impact flash + shockwave ring
    let fl = sstep((u - 0.05) / 0.04) * (1.0 - sstep((u - 0.09) / 0.1)) * 0.7; let ringr = (u - 0.1).max(0.0) / 0.5 * fr.maxr * 0.8; let ra = (1.0 - sstep((u - 0.1) / 0.3)) * 0.5 * intensity;
    if fl > 0.001 || ra > 0.002 { for y in 0..h { for x in 0..w { let d = (x as f32 - fr.ix).hypot(y as f32 - fr.iy); let g = fl * intensity * (-(d / (wf * 0.22)).powi(2)).exp() * 1.2 + if ra > 0.0 { ra * (-((d - ringr) / 6.0).powi(2)).exp() * 0.35 } else { 0.0 }; if g > 0.001 { let o = (y * w + x) * 4; out.d[o] += g; out.d[o + 1] += g * 0.97; out.d[o + 2] += g * 0.92; } } } }
    out
}
