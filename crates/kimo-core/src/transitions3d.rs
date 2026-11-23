//! 3D page transitions drawn by ray-casting textured faces: `cube_spin` (the two shots are neighbouring faces of a
//! turning cube) and `flip_over` (one card turns over; the incoming shot is printed on its back). Both pull back a
//! little at the middle so the turning edge stays in frame, shade each face by how far it is turned away, and sample
//! the pictures bilinearly so edges and texture stay smooth. Direction comes from the transition `angle`:
//! 0 = left (default), 180 = right, 90 = up, 270 = down.
use crate::effects::Frame;

/// One flat face: centre (x, z), unit tangent along its width, unit normal toward the camera side, half extents, picture.
struct Face<'a> { c: [f32; 3], t: [f32; 3], n: [f32; 3], hw: f32, hh: f32, img: &'a Frame, mirror: bool, shade: f32 }

fn sample(f: &Frame, x: f32, y: f32) -> [f32; 3] {
    let (w, h) = (f.w, f.h); if w == 0 || h == 0 { return [0.0; 3]; }
    let (x, y) = ((x - 0.5).clamp(0.0, (w - 1) as f32), (y - 0.5).clamp(0.0, (h - 1) as f32));
    let (x0, y0) = (x as usize, y as usize); let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1)); let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let mut o = [0f32; 3];
    for (qx, qy, k) in [(x0, y0, (1.0 - fx) * (1.0 - fy)), (x1, y0, fx * (1.0 - fy)), (x0, y1, (1.0 - fx) * fy), (x1, y1, fx * fy)] {
        let i = (qy * w + qx) * 4; let a = f.d[i + 3].clamp(0.0, 1.0); for j in 0..3 { o[j] += f.d[i + j] * a * k; } }
    o
}

/// Ray-cast `faces` for every pixel of a w x h canvas seen from a pinhole `dist` px in front of the screen plane
/// (black where no face is hit). `vertical` swaps the axes so the same maths turns up or down.
fn cast(w: usize, h: usize, dist: f32, faces: &[Face], vertical: bool) -> Frame {
    let mut out = Frame::new(w, h);
    let (cw, ch) = if vertical { (h as f32, w as f32) } else { (w as f32, h as f32) };
    let ss = 2usize; // 2x2 samples per pixel: clean silhouette edges
    for py in 0..h { for px in 0..w {
        let mut acc = [0f32; 3];
        for sy in 0..ss { for sx in 0..ss {
            let (fx, fy) = (px as f32 + (sx as f32 + 0.5) / ss as f32, py as f32 + (sy as f32 + 0.5) / ss as f32);
            let (a, b) = if vertical { (fy - cw / 2.0, fx - ch / 2.0) } else { (fx - cw / 2.0, fy - ch / 2.0) };
            // ray from (0, 0, -dist) through (a, b, 0)
            let d = [a, b, dist];
            let mut best: Option<(f32, [f32; 3])> = None;
            for f in faces {
                let dn = d[0] * f.n[0] + d[1] * f.n[1] + d[2] * f.n[2]; if dn.abs() < 1e-6 { continue; }
                let num = f.c[0] * f.n[0] + f.c[1] * f.n[1] + (f.c[2] + dist) * f.n[2]; let s = num / dn; if s <= 0.0 { continue; }
                if best.map_or(false, |(bs, _)| bs <= s) { continue; }
                let p = [d[0] * s, d[1] * s, d[2] * s - dist];
                let r = [p[0] - f.c[0], p[1] - f.c[1], p[2] - f.c[2]];
                let u = r[0] * f.t[0] + r[1] * f.t[1] + r[2] * f.t[2]; let v = r[1];
                if u.abs() > f.hw || v.abs() > f.hh { continue; }
                let u = if f.mirror { -u } else { u };
                let (ix, iy) = (u + f.hw, v + f.hh);
                let (tx, ty) = if vertical { (iy * f.img.w as f32 / (2.0 * f.hh), ix * f.img.h as f32 / (2.0 * f.hw)) } else { (ix * f.img.w as f32 / (2.0 * f.hw), iy * f.img.h as f32 / (2.0 * f.hh)) };
                let c = sample(f.img, tx, ty); best = Some((s, [c[0] * f.shade, c[1] * f.shade, c[2] * f.shade]));
            }
            if let Some((_, c)) = best { for j in 0..3 { acc[j] += c[j]; } }
        } }
        let o = (py * w + px) * 4; let n = (ss * ss) as f32; for j in 0..3 { out.d[o + j] = acc[j] / n; } out.d[o + 3] = 1.0;
    } }
    out
}

/// A face of width `2*hw` turned by `phi` radians about the vertical axis through (0, 0, cz), pushed `r` from that axis.
fn face<'a>(phi: f32, r: f32, cz: f32, hw: f32, hh: f32, img: &'a Frame, mirror: bool) -> Face<'a> {
    let (s, c) = phi.sin_cos();
    // normal pointing toward the camera (-z) when phi = 0; light from the camera, so shade follows how much it faces us
    Face { c: [r * s, 0.0, cz - r * c], t: [c, 0.0, s], n: [s, 0.0, -c], hw, hh, img, mirror, shade: 0.35 + 0.65 * c.abs().powf(0.8) }
}

/// cube_spin: `below` (outgoing) turns away as `inc` (incoming) turns in, `k` eased progress 0..1.
pub fn cube_spin(below: &Frame, inc: &Frame, k: f32, angle_deg: f64) -> Frame {
    let (w, h) = (inc.w, inc.h); let (dir, vertical) = dir_of(angle_deg);
    let (cw, ch) = if vertical { (h as f32, w as f32) } else { (w as f32, h as f32) };
    let (hw, hh) = (cw / 2.0, ch / 2.0); let dist = cw * 1.6;
    let th = k.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2;
    let pull = (2.0 * th).sin() * cw * 0.45; let cz = hw + pull;
    let faces = [face(-dir * th, hw, cz, hw, hh, below, false), face(dir * (std::f32::consts::FRAC_PI_2 - th), hw, cz, hw, hh, inc, false)];
    cast(w, h, dist, &faces, vertical)
}

/// flip_over: one card turns half a turn; the outgoing shot on its front, the incoming shot on its back.
pub fn flip_over(below: &Frame, inc: &Frame, k: f32, angle_deg: f64) -> Frame {
    let (w, h) = (inc.w, inc.h); let (dir, vertical) = dir_of(angle_deg);
    let (cw, ch) = if vertical { (h as f32, w as f32) } else { (w as f32, h as f32) };
    let (hw, hh) = (cw / 2.0, ch / 2.0); let dist = cw * 1.6;
    let th = k.clamp(0.0, 1.0) * std::f32::consts::PI;
    let pull = th.sin() * cw * 0.35;
    // first half shows the front (outgoing), second half the back (incoming, mirrored so it reads the right way round at the end)
    let f = if th < std::f32::consts::FRAC_PI_2 { face(-dir * th, 0.0, pull, hw, hh, below, false) } else { face(-dir * th, 0.0, pull, hw, hh, inc, true) };
    cast(w, h, dist, &[f], vertical)
}

/// (turn sign, vertical) from a transition angle: 0 left, 180 right, 90 up, 270 down.
fn dir_of(angle: f64) -> (f32, bool) {
    let a = angle.rem_euclid(360.0);
    if (45.0..135.0).contains(&a) { (1.0, true) } else if (135.0..225.0).contains(&a) { (-1.0, false) } else if (225.0..315.0).contains(&a) { (-1.0, true) } else { (1.0, false) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn solid(w: usize, h: usize, c: [f32; 3]) -> Frame { let mut f = Frame::new(w, h); for i in 0..w * h { f.d[i * 4..i * 4 + 3].copy_from_slice(&c); f.d[i * 4 + 3] = 1.0; } f }
    fn mean(f: &Frame, ch: usize) -> f32 { (0..f.w * f.h).map(|i| f.d[i * 4 + ch]).sum::<f32>() / (f.w * f.h) as f32 }
    #[test]
    fn cube_spin_starts_on_the_outgoing_shot_and_ends_on_the_incoming_one() {
        let (a, b) = (solid(64, 36, [1.0, 0.0, 0.0]), solid(64, 36, [0.0, 0.0, 1.0]));
        let s = cube_spin(&a, &b, 0.0, 0.0); assert!(mean(&s, 0) > 0.95 && mean(&s, 2) < 0.02, "start {} {}", mean(&s, 0), mean(&s, 2));
        let e = cube_spin(&a, &b, 1.0, 0.0); assert!(mean(&e, 2) > 0.95 && mean(&e, 0) < 0.02, "end {} {}", mean(&e, 0), mean(&e, 2));
        let m = cube_spin(&a, &b, 0.5, 0.0); assert!(mean(&m, 0) > 0.05 && mean(&m, 2) > 0.05, "both faces show mid-way");
        // turning left: the outgoing face is on the left half mid-way
        let left_red: f32 = (0..36).map(|y| m.d[(y * 64 + 16) * 4]).sum(); let right_red: f32 = (0..36).map(|y| m.d[(y * 64 + 48) * 4]).sum();
        assert!(left_red > right_red, "outgoing turns away to the left");
    }
    #[test]
    fn flip_over_shows_the_back_after_half_way_the_right_way_round() {
        let a = solid(64, 36, [1.0, 1.0, 1.0]); let mut b = solid(64, 36, [0.0, 0.0, 0.0]); for y in 0..36 { for x in 0..32 { b.d[(y * 64 + x) * 4] = 1.0; } }
        let e = flip_over(&a, &b, 1.0, 0.0); assert!(e.d[(18 * 64 + 8) * 4] > 0.9 && e.d[(18 * 64 + 56) * 4] < 0.1, "the incoming shot is not mirrored at the end");
        let v = flip_over(&a, &b, 1.0, 90.0); assert!(v.d[(18 * 64 + 8) * 4] > 0.9, "vertical flip ends upright");
    }
}
