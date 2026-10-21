//! AE shape path operators on Kimo's signed-distance shapes: Wiggle Paths, Zig Zag, Pucker & Bloat, Twist and Round Corners.
//! Stored as effects of kind `shape_op_*` on a Shape clip (params are animatable Props), added with add_path_operator, and
//! applied in effect order inside shapemods::build together with trim/repeater/offset/merge. The outline parameter u (0..1)
//! is exact arc length on Bezier paths (Path::nearest) and the angle around the centre on primitives, as Trim Paths uses.
//! Round Corners works on the geometry (rect: corner radius; polygon: exact rounded polygon), so render.rs applies it to the base.
use crate::model::Effect;
use std::f64::consts::{PI, TAU};

pub const OPS: &[(&str, &str, &[(&str, f64)])] = &[
    ("wiggle", "Wiggle Paths: the outline wanders in and out by up to `size` px with `detail` bumps around it, `speed` wiggles/s, `seed`", &[("size", 8.0), ("detail", 12.0), ("speed", 1.0), ("seed", 1.0)]),
    ("zig_zag", "Zig Zag: `ridges` points around the outline, `size` px tall; smooth 1 = waves instead of points", &[("size", 6.0), ("ridges", 24.0), ("smooth", 0.0)]),
    ("pucker_bloat", "Pucker & Bloat: amount -1..1; + bulges the edges out toward a circle, - pulls them in and stretches the corners into a star", &[("amount", 0.5)]),
    ("twist", "Twist: rotates the outline by `angle` deg at the rim, 0 at the centre", &[("angle", 90.0)]),
    ("round_corners", "Round Corners: `radius` px fillet on every corner (rect and polygon shapes)", &[("radius", 10.0)]),
];

fn hash(i: i64, j: i64, seed: i64) -> f64 { let mut h = (i as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (j as u64).wrapping_mul(0xC2B2AE3D27D4EB4F) ^ (seed as u64).wrapping_mul(0x165667B19E3779F9);
    h ^= h >> 31; h = h.wrapping_mul(0xBF58476D1CE4E5B9); h ^= h >> 29; (h >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0 }
/// Smooth value noise, periodic in x with period n (so the outline closes), evolving in time.
fn noise_periodic(x: f64, n: i64, tt: f64, seed: i64) -> f64 {
    let (xi, ti) = (x.floor(), tt.floor()); let (fx, ft) = (x - xi, tt - ti); let s = |v: f64| v * v * (3.0 - 2.0 * v); let (sx, st) = (s(fx), s(ft));
    let n = n.max(1); let (x0, x1) = ((xi as i64).rem_euclid(n), (xi as i64 + 1).rem_euclid(n)); let t0 = ti as i64;
    let a = hash(x0, t0, seed) + (hash(x1, t0, seed) - hash(x0, t0, seed)) * sx; let b = hash(x0, t0 + 1, seed) + (hash(x1, t0 + 1, seed) - hash(x0, t0 + 1, seed)) * sx; a + (b - a) * st
}

/// Radius of the zero set along each of `n` rays from the centre (bisection on the field); 0 where the ray starts outside.
pub fn radii(f: &dyn Fn(f64, f64) -> f64, cx: f64, cy: f64, n: usize, rmax: f64) -> Vec<f64> {
    (0..n).map(|i| { let a = i as f64 / n as f64 * TAU; let (ca, sa) = (a.cos(), a.sin()); let at = |r: f64| f(cx + r * ca, cy + r * sa);
        if at(0.0) > 0.0 { return 0.0; } let (mut lo, mut hi) = (0.0, rmax); if at(hi) <= 0.0 { return rmax; }
        for _ in 0..40 { let m = 0.5 * (lo + hi); if at(m) <= 0.0 { lo = m } else { hi = m } } lo }).collect()
}

pub type Field<'a> = Box<dyn Fn(f64, f64) -> f64 + 'a>;

/// Apply one `shape_op_*` effect to the field `prev`. `u_of(px, py)` gives the outline parameter at a point relative to the centre.
pub fn apply<'a>(e: &'a Effect, prev: Field<'a>, cx: f64, cy: f64, t: f64, u_of: std::rc::Rc<dyn Fn(f64, f64) -> f64 + 'a>, rmax: f64) -> Field<'a> {
    let g = |k: &str, d: f64| e.params.get(k).map(|p| p.at(t)).unwrap_or(d);
    match e.kind.as_str() {
        "shape_op_wiggle" => { let (size, detail, speed, seed) = (g("size", 8.0), g("detail", 12.0).round().clamp(1.0, 400.0) as i64, g("speed", 1.0), g("seed", 1.0) as i64); let tt = t * speed;
            Box::new(move |x, y| { let u = u_of(x - cx, y - cy); prev(x, y) - size * noise_periodic(u * detail as f64, detail, tt, seed) }) }
        "shape_op_zig_zag" => { let (size, ridges, smooth) = (g("size", 6.0), g("ridges", 24.0).round().clamp(1.0, 1000.0), g("smooth", 0.0) > 0.5);
            Box::new(move |x, y| { let ph = u_of(x - cx, y - cy) * ridges; let w = if smooth { (ph * TAU).cos() } else { 1.0 - 4.0 * (ph - ph.floor() - 0.5).abs() }; prev(x, y) - size * w }) }
        "shape_op_pucker_bloat" => { let k = g("amount", 0.5).clamp(-1.0, 1.0); let rs = radii(&*prev, cx, cy, 128, rmax); let rm = rs.iter().sum::<f64>() / rs.len() as f64;
            Box::new(move |x, y| { let d = prev(x, y); let circ = ((x - cx).powi(2) + (y - cy).powi(2)).sqrt() - rm; d + k * (circ - d) }) }
        "shape_op_twist" => { let ang = g("angle", 90.0).to_radians(); let rs = radii(&*prev, cx, cy, 64, rmax); let r0 = rs.iter().cloned().fold(1.0, f64::max);
            Box::new(move |x, y| { let (px, py) = (x - cx, y - cy); let r = (px * px + py * py).sqrt(); let a = -ang * (r / r0).min(1.0); let (ca, sa) = (a.cos(), a.sin()); prev(cx + px * ca - py * sa, cy + px * sa + py * ca) }) }
        _ => prev, // round_corners is applied to the base geometry (render.rs)
    }
}

/// Exact signed distance to a regular polygon with circumradius r, oriented as shapemods::sd orients it, rounded by
/// `round` px when > 0: the polygon is shrunk by `round` (same angles) and its exact distance minus `round` is the filleted shape.
pub fn sd_polygon_rounded(sides: u32, r: f64, round: f64, px: f64, py: f64) -> f64 {
    let n = sides.max(3) as f64; let seg = TAU / n; let apo = r * (seg / 2.0).cos(); let round = round.clamp(0.0, apo * 0.999);
    let apo2 = apo - round; let h2 = apo2 * (seg / 2.0).tan();
    // fold into the sector whose edge normal is +y': aa = 0 on the normal (the same fold shapemods::sd uses)
    let a = py.atan2(px) + PI / 2.0; let aa = a.rem_euclid(seg) - seg / 2.0; let rr = (px * px + py * py).sqrt(); let (qx, qy) = (rr * aa.sin(), rr * aa.cos());
    let (dx, dy) = (qx - qx.clamp(-h2, h2), qy - apo2);
    (if dy > 0.0 { dx.hypot(dy) } else { dy }) - round
}
