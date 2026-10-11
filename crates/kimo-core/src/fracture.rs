//! fracture_glass: procedural laminated-glass shatter as a native effect.
//! The frame itself is the glass: cracks grow outward from an impact point, facets refract the
//! live source, then cells release in a radial stagger and fall (gravity, spin, push) carrying
//! the source texture at their rest UV. Deterministic from `seed`. Single impact point.
use crate::effects::{p, Frame, Params};

struct Rng(u64);
impl Rng {
    fn n(&mut self) -> u64 { self.0 = self.0.wrapping_add(0x9E3779B97F4A7C15); let mut z = self.0; z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9); z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB); z ^ (z >> 31) }
    fn f(&mut self) -> f32 { (self.n() >> 40) as f32 / 16777216.0 }
    fn s(&mut self) -> f32 { self.f() * 2.0 - 1.0 }
}
type V = (f32, f32);
fn ss(a: f32, b: f32, x: f32) -> f32 { let t = ((x - a) / (b - a).max(1e-6)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }
fn dist(a: V, b: V) -> f32 { ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt() }


struct Cell { poly: Vec<V>, c: V, d: f32, tilt: V, shade: f32, t_rel: f32, falls: bool, vel: V, zv: f32, phi0: f32, om: f32, th0: f32, tl0: f32, z: f32, drag: f32, gs: f32, flut: f32, area: f32 }
struct Seg { a: V, b: V, d0: f32, d1: f32, g: f32, w: f32, cells: [usize; 2] }
fn spans(poly: &[V], w: usize, h: usize, mut f: impl FnMut(usize, usize, f32)) {
    let (mut y0, mut y1) = (f32::MAX, f32::MIN); for q in poly { y0 = y0.min(q.1); y1 = y1.max(q.1); }
    let ya = (y0.floor().max(0.0)) as usize; let yb = (y1.ceil().min(h as f32)) as usize; if ya >= yb { return; }
    const S: usize = 4; let mut row = vec![0f32; w]; let n = poly.len();
    for y in ya..yb {
        let (mut mn, mut mx) = (w, 0usize); let mut any = false;
        for sidx in 0..S {
            let sy = y as f32 + (sidx as f32 + 0.5) / S as f32; let mut xs: Vec<f32> = Vec::with_capacity(8);
            for i in 0..n { let (a, b) = (poly[i], poly[(i + 1) % n]); if (a.1 <= sy) != (b.1 <= sy) { xs.push(a.0 + (sy - a.1) * (b.0 - a.0) / (b.1 - a.1)); } }
            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            for pr in xs.chunks(2) { if pr.len() < 2 { break; } let (x0, x1) = (pr[0].max(0.0), pr[1].min(w as f32)); if x1 <= x0 { continue; }
                let (p0, p1) = (x0 as usize, (x1 as usize).min(w - 1)); any = true; mn = mn.min(p0); mx = mx.max(p1);
                if p0 == p1 { row[p0] += (x1 - x0) / S as f32; } else { row[p0] += ((p0 + 1) as f32 - x0) / S as f32; for x in p0 + 1..p1 { row[x] += 1.0 / S as f32; } row[p1] += (x1 - p1 as f32) / S as f32; } }
        }
        if any { for x in mn..=mx.min(w - 1) { let c = row[x]; if c > 0.0 { f(y, x, c.min(1.0)); } row[x] = 0.0; } }
    }
}
fn bil(src: &[f32], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let x = x.clamp(0.0, w as f32 - 1.001); let y = y.clamp(0.0, h as f32 - 1.001); let (x0, y0) = (x as usize, y as usize); let (fx, fy) = (x - x0 as f32, y - y0 as f32); let mut o = [0f32; 4];
    for j in 0..4 { let a = src[(y0 * w + x0) * 4 + j]; let b = src[(y0 * w + x0 + 1) * 4 + j]; let c = src[((y0 + 1) * w + x0) * 4 + j]; let d = src[((y0 + 1) * w + x0 + 1) * 4 + j]; o[j] = a + (b - a) * fx + (c - a) * fy + (a - b - c + d) * fx * fy; } o
}
fn seg_cov(f: &mut [f32], g: &mut [f32], w: usize, h: usize, a: V, b: V, wd: f32, glint: f32) {
    let pad = wd + 1.5; let (x0, x1) = ((a.0.min(b.0) - pad).floor().max(0.0) as usize, ((a.0.max(b.0) + pad).ceil().min(w as f32 - 1.0)).max(0.0) as usize);
    let (y0, y1) = ((a.1.min(b.1) - pad).floor().max(0.0) as usize, ((a.1.max(b.1) + pad).ceil().min(h as f32 - 1.0)).max(0.0) as usize);
    let (dx, dy) = (b.0 - a.0, b.1 - a.1); let l2 = (dx * dx + dy * dy).max(1e-6);
    for y in y0..=y1 { for x in x0..=x1 { let (px, py) = (x as f32 + 0.5, y as f32 + 0.5); let t = (((px - a.0) * dx + (py - a.1) * dy) / l2).clamp(0.0, 1.0); let d = ((px - a.0 - t * dx).powi(2) + (py - a.1 - t * dy).powi(2)).sqrt();
        let c = (wd * 0.5 + 0.5 - d).clamp(0.0, 1.0); if c > f[y * w + x] { f[y * w + x] = c; }
        let gg = (wd * 1.6 + 0.5 - d).clamp(0.0, 1.0) * glint; if gg > g[y * w + x] { g[y * w + x] = gg; } } }
}


/// Voronoi cells of `sites` clipped to `bb`; each cell edge carries the neighbour that made it (usize::MAX = frame bound).
fn voronoi(sites: &[V], bb: (f32, f32, f32, f32)) -> Vec<Vec<(V, usize)>> {
    // Uniform-grid neighbour search (near-linear). The cell of a site does not depend on clip order, and the early exit is the same
    // distance bound as the brute-force version, so the result matches it up to floating-point ties.
    let n = sites.len(); let mut out = Vec::with_capacity(n);
    let (bw, bh) = ((bb.2 - bb.0).max(1.0), (bb.3 - bb.1).max(1.0)); let cs = ((bw * bh / n.max(1) as f32).sqrt() * 1.0).max(1.0); let (gx, gy) = (((bw / cs).ceil() as usize).max(1), ((bh / cs).ceil() as usize).max(1));
    let cell_of = |q: V| -> (usize, usize) { (((q.0 - bb.0) / cs).floor().clamp(0.0, gx as f32 - 1.0) as usize, ((q.1 - bb.1) / cs).floor().clamp(0.0, gy as f32 - 1.0) as usize) };
    let mut grid: Vec<Vec<u32>> = vec![vec![]; gx * gy]; for (i, s) in sites.iter().enumerate() { let (cx, cy) = cell_of(*s); grid[cy * gx + cx].push(i as u32); }
    for i in 0..n { let si = sites[i];
        let mut poly: Vec<(V, usize)> = vec![((bb.0, bb.1), usize::MAX), ((bb.2, bb.1), usize::MAX), ((bb.2, bb.3), usize::MAX), ((bb.0, bb.3), usize::MAX)];
        let (cx, cy) = cell_of(si); let maxk = gx.max(gy);
        for kr in 0..=maxk {
            let mut maxr = 0f32; for (q, _) in &poly { maxr = maxr.max(dist(si, *q)); }
            if kr >= 2 && ((kr - 1) as f32 * cs) * 0.5 > maxr { break; }
            let (x0, x1, y0, y1) = (cx as isize - kr as isize, cx as isize + kr as isize, cy as isize - kr as isize, cy as isize + kr as isize);
            for yy in y0..=y1 { if yy < 0 || yy >= gy as isize { continue; } let edge_row = yy == y0 || yy == y1;
                let mut xx = x0; while xx <= x1 { if xx >= 0 && xx < gx as isize { for &j in &grid[yy as usize * gx + xx as usize] { let j = j as usize; if j == i { continue; }
                    let sj = sites[j]; let dd = dist(si, sj); if dd < 1e-4 { continue; } let mut mr = 0f32; for (q, _) in &poly { mr = mr.max(dist(si, *q)); } if dd * 0.5 > mr { continue; }
                    let nx = (sj.0 - si.0) / dd; let ny = (sj.1 - si.1) / dd; let m = ((si.0 + sj.0) * 0.5, (si.1 + sj.1) * 0.5);
                    let side = |q: V| (q.0 - m.0) * nx + (q.1 - m.1) * ny; let mut np: Vec<(V, usize)> = Vec::with_capacity(poly.len() + 2); let k = poly.len();
                    for e in 0..k { let (a, ta) = poly[e]; let b = poly[(e + 1) % k].0; let (sa, sb) = (side(a), side(b)); let (ina, inb) = (sa <= 0.0, sb <= 0.0);
                        let x = if ina != inb { let t = sa / (sa - sb); (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t) } else { a };
                        if ina && inb { np.push((a, ta)); } else if ina { np.push((a, ta)); np.push((x, j)); } else if inb { np.push((x, ta)); } }
                    if np.len() < 3 { poly = np; break; } poly = np; } }
                    if poly.len() < 3 { break; }
                    xx += if edge_row || kr == 0 { 1 } else { (x1 - x0).max(1) }; }
                if poly.len() < 3 { break; } }
            if poly.len() < 3 { break; }
        }
        out.push(poly); }
    out
}

pub fn apply(pr: &Params, f: &mut Frame, t: f64, seed: u64) -> Result<(), String> {
    let (w, h) = (f.w, f.h); if w < 8 || h < 8 { return Ok(()); }
    let g = |k: &str, d: f64| { let v = p(pr, k, d as f64, t); if v.is_finite() { v } else { d as f32 } };
    let hh = h as f32; let t0 = g("t0", 0.0); let tau = t as f32 - t0; if tau < 0.0 { return Ok(()); }
    let imp = (g("impact_x", 0.5).clamp(0.0, 1.0) * w as f32, g("impact_y", 0.5).clamp(0.0, 1.0) * hh); let (ix, iy) = imp;
    let nr = g("radials", 14.0).clamp(5.0, 40.0); let grow = g("ring_growth", 1.3).clamp(1.08, 2.0); let r0 = g("r0", 0.012).clamp(0.003, 0.2) * hh;
    let debris = g("debris", 70.0).clamp(0.0, 300.0) as usize; let max_cells = g("max_cells", 700.0).clamp(40.0, 2500.0) as usize;
    let speed = g("crack_speed", 12.0).clamp(0.2, 80.0) * hh; let front = speed * tau;
    let rel_t0 = g("release_delay", 0.1).max(0.0); let rel_spread = g("release_spread", 0.12).max(0.0); let rel_rad = g("release_radius", 0.6).clamp(0.02, 3.0) * hh;
    let grav = g("gravity", 2.5) * hh; let push = g("push", 0.35) * hh; let tumble = g("tumble", 6.0); let cam = g("toward_camera", 0.1) * hh;
    let fade_start = g("fade_start", 0.4); let clear_time = g("clear_time", 0.6).max(fade_start + 0.01);
    let refr = g("refraction", 7.0); let disp = g("dispersion", 0.35).clamp(0.0, 1.5); let cw = g("crack_width", 1.1).clamp(0.3, 8.0); let glint = g("glint", 0.7).clamp(0.0, 3.0); let bevel = g("bevel", 3.0).clamp(0.0, 20.0);
    let rz = g("realism", 0.0).clamp(0.0, 1.0); let core = g("core_damp", 0.0).clamp(0.0, 1.0); let core_r = (g("core_radius", 0.16).clamp(0.02, 0.6) * hh).max(1.0); let real = rz > 0.0;
    let slg = g("scene_lights", 0.0) > 0.5; let sgain = g("light_gain", 1.0).clamp(0.0, 16.0); let slights: Vec<crate::lighting::LightDef> = if slg { match crate::lighting::scene() { Some(sc) if !sc.lights.is_empty() => sc.lights.iter().map(|(_, l)| l.clone()).collect(), _ => return Err("fracture_glass scene_lights needs at least one enabled project light (set_light)".into()) } } else { vec![] };
    let etint = [g("edge_r", 0.62), g("edge_g", 0.68), g("edge_b", 0.66)]; let trans = g("transmission", 0.0).clamp(0.0, 1.0); let bd = if trans > 0.0 || real { crate::render::backdrop().filter(|b| b.w == w && b.h == h) } else { None }; let vnoise = g("vel_noise", 0.6).clamp(0.0, 1.5); let rjit = g("release_jitter", 0.08).max(0.0); let rface = g("face_refraction", 10.0).clamp(0.0, 60.0); let face_op = g("face_opacity", 0.85).clamp(0.05, 1.0); let fres = g("fresnel", 0.6).clamp(0.0, 2.0); let sheen = g("sheen", 0.5).clamp(0.0, 2.0); let crack_clear = g("crack_clear", 0.0); let thick = g("thickness", 5.0).clamp(0.0, 40.0); let hole_dim = g("hole_dim", 0.2).clamp(0.0, 1.0);
    let ns = g("blur_samples", 5.0).clamp(1.0, 16.0) as usize; let shutter = g("shutter_deg", 180.0).clamp(0.0, 360.0) / 360.0 / g("fps", 25.0).clamp(1.0, 240.0);
    let max_cells = if real { ((max_cells as f32) * (1.0 + 12.0 * rz)).min(2500.0) as usize } else { max_cells }; let debris = if real { debris.max((90.0 * rz) as usize) } else { debris };
    let mut rg = Rng(seed ^ 0xF4AC_7E55); for _ in 0..3 { rg.n(); }
    // --- sites: jittered rings (more sites per ring outward = branching) + small debris near the impact ---
    let dmax = { let mut m = 0f32; for c in [(0.0, 0.0), (w as f32, 0.0), (0.0, hh), (w as f32, hh)] { m = m.max(dist(imp, c)); } m } * 1.1;
    let budget = max_cells.saturating_sub(debris).max(30);
    let est = |gr: f32| -> usize { let (mut r, mut k, mut tot) = (r0, 0usize, 1usize); while r < dmax && k < 200 { tot += ((nr * (1.0 + 0.4 * (k / 3) as f32)).round() as usize).clamp(5, 90); r = r * gr + 0.01 * hh * (k as f32 / 8.0).min(1.0); k += 1; } tot };
    let mut grow = grow; let mut guard_i = 0; while est(grow) > budget && guard_i < 60 { grow *= 1.03; guard_i += 1; }
    let mut sites: Vec<V> = vec![imp]; let mut r = r0; let mut rprev = 0f32; let mut k = 0usize; let two = std::f32::consts::TAU; let ph0 = rg.f() * two;
    while r < dmax && k < 200 { let gap = (r - rprev).max(r0 * 0.5); let nk = ((nr * (1.0 + 0.4 * (k / 3) as f32)).round() as usize).clamp(5, 90); let ph = ph0 + (0.1 + 0.4 * (k as f32 / 12.0).min(1.0)) * two / nk as f32 * rg.s();
        for i in 0..nk { let a = ph + two * (i as f32 + 0.34 * rg.s()) / nk as f32; let rr = r + 0.25 * gap * rg.s(); if sites.len() < max_cells { sites.push((ix + rr * a.cos(), iy + rr * a.sin())); } }
        rprev = r; r *= grow; r += 0.02 * hh * rg.f() * (k as f32 / 8.0).min(1.0); k += 1; }
    for _ in 0..debris { let rr = 0.3 * hh * rg.f().powf(1.7) + 0.004 * hh; let a = rg.f() * two; sites.push((ix + rr * a.cos(), iy + rr * a.sin())); }
    let pad = 0.05 * w as f32; // legacy projects keep the brute-force clip order (identical crack jitter); realism uses the grid search
    let polys = if real { voronoi(&sites, (-pad, -pad, w as f32 + pad, hh + pad)) } else { voronoi_brute(&sites, (-pad, -pad, w as f32 + pad, hh + pad)) };
    let mut cells: Vec<Cell> = Vec::with_capacity(polys.len()); let mut edges: Vec<(usize, usize, V, V)> = Vec::new();
    for (i, pl) in polys.iter().enumerate() { if pl.len() < 3 { cells.push(Cell { poly: vec![sites[i]; 3], c: sites[i], d: dist(sites[i], imp), tilt: (0.0, 0.0), shade: 1.0, t_rel: 0.0, falls: false, vel: (0.0, 0.0), zv: 0.0, phi0: 0.0, om: 0.0, th0: 0.0, tl0: 0.0, z: 0.0, drag: 1.0, gs: 1.0, flut: 0.0, area: 0.0 }); continue; }
        let poly: Vec<V> = pl.iter().map(|q| q.0).collect(); let mut c = (0.0, 0.0); for q in &poly { c.0 += q.0; c.1 += q.1; } c = (c.0 / poly.len() as f32, c.1 / poly.len() as f32);
        let d = dist(c, imp); let rad = (c.0 - ix, c.1 - iy); let rl = (rad.0 * rad.0 + rad.1 * rad.1).sqrt().max(1e-3);
        let ar = { let mut a = 0.0f32; for i in 0..poly.len() { let (p, q) = (poly[i], poly[(i + 1) % poly.len()]); a += p.0 * q.1 - q.0 * p.1; } a.abs() * 0.5 }; let small = if real { (1.0 - ar.sqrt() / hh / 0.07).clamp(0.0, 1.0) } else { 0.0 };
        let falls = d <= rel_rad && rg.f() < 1.0 - ss(0.75, 1.0, d / rel_rad);
        let t_rel = if real { let u = rg.f(); let tail = if rg.f() < 0.08 { rg.f() * (2.0 * rel_t0 + 0.15) } else { 0.0 }; d / speed + 0.02 + rel_t0 * (0.25 + 1.5 * u * u) + rel_spread * (d / rel_rad) * (0.5 + rg.f()) + tail + rjit * rg.s().abs() } else { (rel_t0 + rel_spread * (d / rel_rad) + rjit * (rg.f() + vnoise * 0.5 * rg.s())).max(d / speed + 0.02) };
        let core_k = if core > 0.0 { let u = (d / core_r).clamp(0.0, 1.0); (1.0 - core * (1.0 - u * u * (3.0 - 2.0 * u)) * 0.75, core * (1.0 - u)) } else { (1.0, 0.0) }; let sp = push * (0.4 + 0.8 * rg.f()) * (1.0 + vnoise * 0.7 * rg.s()).max(0.15) * (1.0 + 1.3 * small) * core_k.0; let ang = vnoise * 0.9 * rg.s() + if core > 0.0 { core_k.1 * 0.9 * rg.s() } else { 0.0 }; let (ca, sa) = (ang.cos(), ang.sin()); let swirl = vnoise * 0.35 * rg.s() * push;
        let dir = ((rad.0 * ca - rad.1 * sa) / rl, (rad.0 * sa + rad.1 * ca) / rl); let vel = (dir.0 * sp - dir.1 * swirl + 0.15 * push * rg.s(), dir.1 * sp + dir.0 * swirl - 0.2 * push * rg.f());
        cells.push(Cell { poly, c, d, tilt: (rg.s() * refr * if real { 0.4 } else { 1.0 }, rg.s() * refr * if real { 0.4 } else { 1.0 }), shade: 1.0 + if real { 0.02 } else { 0.04 } * rg.s(), t_rel, falls, vel, zv: cam * (0.4 + rg.f()), phi0: rg.f() * two, om: tumble * 0.4 * rg.s(), th0: tumble * (0.25 + 1.5 * rg.f()) * if rg.f() < 0.5 { -1.0 } else { 1.0 } * if real { 0.5 + small } else { 1.0 }, tl0: tumble.min(8.0) * 0.12 * (0.3 + rg.f()) * if rg.f() < 0.5 { -1.0 } else { 1.0 }, z: rg.f(), drag: if real { 0.5 + 2.4 * small + 0.4 * rg.f() } else { 1.0 }, gs: if real { 0.8 + 0.4 * rg.f() } else { 1.0 }, flut: if real { 0.35 * small * (0.5 + rg.f()) } else { 0.0 }, area: ar });
        let m = pl.len(); for e in 0..m { let tg = pl[e].1; if tg != usize::MAX && tg > i { edges.push((i, tg, pl[e].0, pl[(e + 1) % m].0)); } } }
    // crack segments with zigzag + per-segment width/brightness variation
    let mut segs: Vec<Seg> = Vec::new();
    for &(ci, cj, a, b) in &edges { let len = dist(a, b); if len < 1.0 { continue; } let (nx, ny) = (-(b.1 - a.1) / len, (b.0 - a.0) / len); let np = if real { ((len / (0.011 * hh)).ceil() as usize).clamp(1, 14) } else if len > 0.04 * hh { 3 } else if len > 0.012 * hh { 2 } else { 1 };
        let gbase = rg.f(); let wbase = 0.55 + 0.9 * rg.f(); let mut pts = vec![a]; for q in 1..np { let tt = q as f32 / np as f32; let o = 0.07 * len * rg.s(); pts.push((a.0 + (b.0 - a.0) * tt + nx * o, a.1 + (b.1 - a.1) * tt + ny * o)); } pts.push(b);
        let (da, db) = (dist(a, imp), dist(b, imp)); let dl = 0.12 * rg.s() * hh * 0.05;
        for q in 0..np { let (u0, u1) = (q as f32 / np as f32, (q + 1) as f32 / np as f32); let wq = if real { let dm = 0.5 * (da + db); wbase * (0.65 + 0.7 * rg.f()) * (1.0 - 0.5 * ss(0.0, 0.7 * hh, dm)) } else { wbase }; segs.push(Seg { a: pts[q], b: pts[q + 1], d0: da + (db - da) * u0 + dl, d1: da + (db - da) * u1 + dl + 1.0, g: gbase, w: wq, cells: [ci, cj] }); } }
    // --- render ---
    let src = f.d.clone(); let n = w * h;
    let released = |c: &Cell| c.falls && tau >= c.t_rel;
    for c in cells.iter() { if released(c) { continue; } let act = ss(c.d - 0.02 * hh, c.d + 0.08 * hh, front); if act <= 0.0 { continue; }
        let (tx, ty) = (c.tilt.0 * act, c.tilt.1 * act); let sh = 1.0 + (c.shade - 1.0) * act;
        spans(&c.poly, w, h, |y, x, cov| { if cov < 0.5 { return; } let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5); let o = (y * w + x) * 4;
            let rch = bil(&src, w, h, fx + tx * (1.0 + disp), fy + ty * (1.0 + disp)); let gch = bil(&src, w, h, fx + tx, fy + ty); let bch = bil(&src, w, h, fx + tx * (1.0 - disp).max(0.0), fy + ty * (1.0 - disp).max(0.0));
            f.d[o] = rch[0] * sh; f.d[o + 1] = gch[1] * sh; f.d[o + 2] = bch[2] * sh; f.d[o + 3] = gch[3]; }); }
    let mut order: Vec<usize> = (0..cells.len()).filter(|&i| released(&cells[i])).collect();
    let mut holem = vec![0f32; n];
    for &ci in &order { spans(&cells[ci].poly, w, h, |y, x, cov| { let o = (y * w + x) * 4; if cov > holem[y * w + x] { holem[y * w + x] = cov; } for j in 0..3 { f.d[o + j] *= 1.0 - cov; } f.d[o + 3] = f.d[o + 3] * (1.0 - cov) + hole_dim * cov; }); }
    let mut cov = vec![0f32; n]; let mut gl = vec![0f32; n];
    for s in &segs { let rel = |c: usize| released(&cells[c]); if rel(s.cells[0]) && rel(s.cells[1]) { continue; }
        let amt = ((front - s.d0) / (s.d1 - s.d0).max(1.0)).clamp(0.0, 1.0); if amt <= 0.0 { continue; } let b = (s.a.0 + (s.b.0 - s.a.0) * amt, s.a.1 + (s.b.1 - s.a.1) * amt);
        let dm = 0.5 * (s.d0 + s.d1); let center = 0.3 + 0.7 * ss(0.0, 0.3 * hh, dm); let gl_on = if s.g > 0.55 { (s.g - 0.55) / 0.45 } else { 0.0 };
        seg_cov(&mut cov, &mut gl, w, h, s.a, b, cw * s.w, glint * (0.15 + 0.85 * gl_on) * center); }
    for i in 0..n { let o = i * 4; if f.d[o + 3] <= 0.0 { continue; } let keep = (1.0 - holem[i]) * if crack_clear > 0.0 { 1.0 - ss(crack_clear * 0.7, crack_clear, tau) } else { 1.0 }; let c = cov[i] * keep; let gg = gl[i] * keep;
        if c > 0.0 || gg > 0.0 { for j in 0..3 { f.d[o + j] = f.d[o + j] * (1.0 - 0.5 * c) + (gg * 0.5 + c * 0.1) * (0.6 + 0.4 * f.d[o + j].min(1.0)); } } }
    // falling shards: 3D tumble about an in-plane axis, slab thickness, sub-frame motion blur
    order.sort_by(|a, b| cells[*a].z.partial_cmp(&cells[*b].z).unwrap_or(std::cmp::Ordering::Equal));
    let mut acc = vec![0f32; n * 4]; let mut lay = vec![0f32; n * 4]; let mut shd = vec![0f32; if real { n } else { 0 }]; let fl = 2.5 * hh; let mut drew = false;
    for sidx in 0..ns { let tau_s = tau + if ns > 1 { (sidx as f32 / (ns - 1) as f32 - 0.5) * shutter } else { 0.0 };
        let fade = 1.0 - ss(fade_start, clear_time, tau_s); if fade <= 0.0 { continue; } for v in lay.iter_mut() { *v = 0.0; }
        for &ci in &order { let c = &cells[ci]; let jit = { let mut hsh = (ci as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (sidx as u64).wrapping_mul(0xBF58476D1CE4E5B9) ^ seed; hsh ^= hsh >> 29; hsh = hsh.wrapping_mul(0x94D049BB133111EB); hsh ^= hsh >> 32; ((hsh & 0xFFFFFF) as f32 / 16777216.0 - 0.5) * if ns > 1 { shutter / (ns - 1) as f32 } else { 0.0 } }; let ts = (tau_s + jit - c.t_rel).max(0.0);
            let (cxd, cyd, zc, th, phi) = if real { let k = c.drag; let dtr = (1.0 - (-k * ts).exp()) / k; let air = (0.35 * k).max(1e-3); let gy = grav * c.gs / air * (ts - (1.0 - (-air * ts).exp()) / air);
                (c.c.0 + c.vel.0 * dtr, c.c.1 + c.vel.1 * dtr + gy, c.zv * dtr, c.th0 * ts * (0.35 + 0.65 * ss(0.0, 0.25, ts)) + c.tl0 * (ts / 0.05).min(1.0) + c.flut * (ts * 13.0 + c.phi0).sin() * ss(0.0, 0.1, ts), c.phi0 + c.om * ts) } else { (c.c.0 + c.vel.0 * ts, c.c.1 + c.vel.1 * ts + 0.5 * grav * ts * ts, c.zv * ts, c.th0 * ts + c.tl0 * (ts / 0.05).min(1.0), c.phi0 + c.om * ts) }; let (cth, sth) = (th.cos(), th.sin()); let (ux, uy) = (phi.cos(), phi.sin()); let (nx, ny) = (-uy, ux);
            if cxd < -0.5 * w as f32 || cxd > 1.5 * w as f32 || cyd > 2.5 * hh { continue; }
            let fp = (fl - zc).max(0.9 * hh); let kk = fl / fp;
            let fwd = |q: V, zo: f32| -> V { let (dx, dy) = (q.0 - c.c.0, q.1 - c.c.1); let pa = dx * ux + dy * uy; let pb = dx * nx + dy * ny; let bq = pb * cth - zo * sth; let zq = pb * sth + zo * cth; let s = kk * fp / (fp - zq).max(0.05 * hh); (cxd + s * (pa * ux + bq * nx), cyd + s * (pa * uy + bq * ny)) };
            let m = c.poly.len(); let hz = 0.5 * thick; let front_up = cth >= 0.0;
            let pf: Vec<V> = c.poly.iter().map(|q| fwd(*q, hz)).collect(); let pbk: Vec<V> = c.poly.iter().map(|q| fwd(*q, -hz)).collect();
            if real { let off = (0.006 + 0.03 * ts.min(0.7)) * hh; let shp: Vec<V> = pf.iter().map(|q| (q.0 + off * 0.6, q.1 + off)).collect(); let wsh = fade * 0.55 / ns as f32; spans(&shp, w, h, |y, x, cv| { shd[y * w + x] += cv * wsh; }); }
            let (near, far) = if front_up { (&pf, &pbk) } else { (&pbk, &pf) };
            let edge_col = [etint[0] * c.shade, etint[1] * c.shade, etint[2] * c.shade]; let light = 0.5 + 0.5 * (th * 1.3 + c.z * 6.0).sin(); let ecol = [edge_col[0] * (0.4 + 0.8 * light), edge_col[1] * (0.4 + 0.8 * light), edge_col[2] * (0.4 + 0.8 * light)];
            let mut put = |y: usize, x: usize, col: [f32; 3], al: f32| { if al <= 0.0 { return; } let o = (y * w + x) * 4; let oa = lay[o + 3]; for j in 0..3 { lay[o + j] = col[j] * al + lay[o + j] * (1.0 - al); } lay[o + 3] = al + oa * (1.0 - al); };
            if thick > 0.2 { spans(far, w, h, |y, x, cv| put(y, x, ecol, cv * fade * face_op * 0.5)); let orient = { let mut a = 0.0f32; for i in 0..m { let (p, q) = (pf[i], pf[(i + 1) % m]); a += p.0 * q.1 - q.0 * p.1; } if a >= 0.0 { 1.0f32 } else { -1.0 } };
                for e in 0..m { let e2 = (e + 1) % m; let quad = [pf[e], pf[e2], pbk[e2], pbk[e]];
                    let ec = if real { let (ex, ey) = (pf[e2].0 - pf[e].0, pf[e2].1 - pf[e].1); let l = (ex * ex + ey * ey).sqrt().max(1e-4); let (nxo, nyo) = (ey / l * orient, -ex / l * orient); let lit = (nxo * -0.64 + nyo * -0.77) * 0.5 + 0.5; let k2 = 0.25 + 1.3 * lit * lit; let k2 = 1.0 - rz + rz * k2; [edge_col[0] * k2, edge_col[1] * k2, edge_col[2] * k2] } else { ecol };
                    spans(&quad, w, h, |y, x, cv| put(y, x, ec, cv * fade)); } }
            if cth.abs() < 0.03 { spans(near, w, h, |y, x, cv| put(y, x, ecol, cv * fade)); continue; }
            let back = cth < 0.0; let shade = if back { 0.8 - 0.2 * rz } else { 1.0 } * c.shade;
            spans(near, w, h, |y, x, cv| { let (dxp, dyp) = (x as f32 + 0.5 - cxd, y as f32 + 0.5 - cyd); let da = dxp * ux + dyp * uy; let db = dxp * nx + dyp * ny; let den = kk * fp * cth + db * sth; if den.abs() < 1e-3 { return; }
                let pb = db * fp / den; let s = kk * fp / (fp - pb * sth).max(0.05 * hh); let pa = da / s; let (qx, qy) = (c.c.0 + pa * ux + pb * nx, c.c.1 + pa * uy + pb * ny);
                let mut dm = f32::MAX; for e in 0..m { let (a, b) = (c.poly[e], c.poly[(e + 1) % m]); let (dx, dy) = (b.0 - a.0, b.1 - a.1); let l2 = (dx * dx + dy * dy).max(1e-6); let tt = (((qx - a.0) * dx + (qy - a.1) * dy) / l2).clamp(0.0, 1.0); dm = dm.min(((qx - a.0 - tt * dx).powi(2) + (qy - a.1 - tt * dy).powi(2)).sqrt()); }
                let lens = if rface > 0.0 { rface * (1.0 - ss(0.0, bevel.max(1.0) * 4.0, dm * s)) * 0.6 } else { 0.0 }; let (tcx, tcy) = (c.c.0 - qx, c.c.1 - qy); let tl = (tcx * tcx + tcy * tcy).sqrt().max(1e-3);
                let (ox, oy) = (rface * sth * nx + lens * tcx / tl, rface * sth * ny + lens * tcy / tl);
                let sg = bil(&src, w, h, qx + ox, qy + oy); let sr = if disp > 0.0 { bil(&src, w, h, qx + ox * (1.0 + disp), qy + oy * (1.0 + disp)) } else { sg }; let sb2 = if disp > 0.0 { bil(&src, w, h, qx + ox * (1.0 - disp).max(0.0), qy + oy * (1.0 - disp).max(0.0)) } else { sg };
                let mut sm = [sr[0], sg[1], sb2[2], sg[3]];
                let tre = if real && back { 0.62 * rz } else { trans }; if let (Some(b), true) = (&bd, tre > 0.0) { let (bxf, byf) = (x as f32 + 0.5, y as f32 + 0.5); let (mx, my) = (ox * 2.5, oy * 2.5);
                    let bgc = bil(&b.d, w, h, bxf + mx, byf + my); let br = bil(&b.d, w, h, bxf + mx * (1.0 + disp), byf + my * (1.0 + disp)); let bb = bil(&b.d, w, h, bxf + mx * (1.0 - disp).max(0.0), byf + my * (1.0 - disp).max(0.0));
                    let tcol = [br[0], bgc[1], bb[2]]; for j in 0..3 { sm[j] = sm[j] * (1.0 - tre) + tcol[j] * tre; } }
                if real { let szc = c.area.sqrt().max(4.0); let lp = (-0.5 * ux - 0.6 * uy, -0.5 * nx - 0.6 * ny); let gr = ((pa * lp.0 + pb * lp.1) / szc).clamp(-1.5, 1.5); let micro = (pa * 0.021 + pb * 0.008 + c.z * 40.0).sin() * 0.03; let mul = 1.0 + 0.16 * rz * gr + micro * rz; for j in 0..3 { sm[j] *= mul; } } let bv = if bevel > 0.0 { 1.0 - ss(0.0, bevel, dm * s) } else { 0.0 }; let lum = if slg { 0.0 } else { 0.3 * glint * bv * (0.3 + light) };
                let tint = if back { [0.92, 1.0, 1.04] } else { [1.0, 1.0, 1.0] }; let fr = (1.0 - cth.abs()).powi(3) * fres; let st = (((pa + pb) * 0.02 + th * 2.0 + c.z * 9.0).sin() * 3.0).tanh() * 0.5 + 0.5; let spl = if slg { let nz = if back { -1.0f64 } else { 1.0 }; let nn = [(-sth * nx) as f64 * nz, (-sth * ny) as f64 * nz, (cth as f64 * nz)]; let rgh = 0.04 + 0.22 * ((((ci as u64).wrapping_mul(0x9E3779B97F4A7C15) >> 40) & 0xFFFF) as f64 / 65535.0); let mat = crate::lighting::Material { albedo: None, roughness: rgh, metal: 0.0, ior: 1.52, clearcoat: 0.0, emissive: [0.0; 3] }; let pt = [(x as f64 + 0.5) / w as f64, (y as f64 + 0.5) / hh as f64, 0.0]; let mut o = [0f32; 3]; for l in &slights { let (lv, at) = crate::lighting::light_at(l, pt); if at <= 0.0 { continue; } let (_, sp) = crate::lighting::brdf(nn, lv, [0.0, 0.0, 1.0], &mat, [0.0; 3]); for j in 0..3 { o[j] += (l.intensity * at * l.color[j] * sp[j]) as f32; } } [1.0 - (-o[0] * 0.12 * sgain).exp(), 1.0 - (-o[1] * 0.12 * sgain).exp(), 1.0 - (-o[2] * 0.12 * sgain).exp()] } else { [0.0; 3] };
                let band = if real { let szc = c.area.sqrt().max(4.0); let an = th * 0.7 + c.z * 6.0; let off = (c.z * 2.0 - 1.0) * 0.5 * szc; let u = (pa * an.cos() + pb * an.sin() - off) / (0.22 * szc); rz * 0.30 * (-u * u).exp() * (0.3 + 0.7 * (1.0 - cth.abs())) } else { 0.0 };
                let sh2 = band + lum + spl[1] + if slg { 0.0 } else { sheen * 0.05 * st * (0.3 + light) } + fr * 0.35 + (1.0 - ss(0.0, 2.5 * bevel.max(1.0), dm * s)) * fres * 0.12;
                put(y, x, [sm[0] * shade * tint[0] + sh2 * 0.9 + (spl[0] - spl[1]), sm[1] * shade * tint[1] + sh2, sm[2] * shade * tint[2] + sh2 * 1.1 + (spl[2] - spl[1])], (cv * fade * sm[3] * (face_op + fr * 0.5 + 0.2 * (1.0 - ss(0.0, bevel.max(1.0) * 3.0, dm * s)))).min(cv * fade * sm[3])); }); }
        for i in 0..n * 4 { acc[i] += lay[i]; } drew = true; }
    if drew && real { box_blur_1ch(&mut shd, w, h, ((hh / 130.0) as usize).max(2)); box_blur_1ch(&mut shd, w, h, ((hh / 130.0) as usize).max(2));
        for i in 0..n { let sv = (shd[i] * 0.45).min(0.4); if sv <= 0.002 { continue; } let o = i * 4; let a = f.d[o + 3]; let na = a + sv * (1.0 - a); for j in 0..3 { f.d[o + j] = f.d[o + j] * a * (1.0 - sv) / na.max(1e-6); } f.d[o + 3] = na; } }
    if drew { let inv = 1.0 / ns as f32; for i in 0..n { let o = i * 4; let a = (acc[o + 3] * inv).clamp(0.0, 1.0); if a <= 0.0 { continue; } let oa = f.d[o + 3]; let na = a + oa * (1.0 - a);
        for j in 0..3 { f.d[o + j] = (acc[o + j] * inv + f.d[o + j] * oa * (1.0 - a)) / na.max(1e-6); } f.d[o + 3] = na; } }
    Ok(())
}

fn voronoi_brute(sites: &[V], bb: (f32, f32, f32, f32)) -> Vec<Vec<(V, usize)>> {
let n = sites.len(); let mut out = Vec::with_capacity(n);
for i in 0..n { let si = sites[i];
    let mut poly: Vec<(V, usize)> = vec![((bb.0, bb.1), usize::MAX), ((bb.2, bb.1), usize::MAX), ((bb.2, bb.3), usize::MAX), ((bb.0, bb.3), usize::MAX)];
    let mut order: Vec<(f32, usize)> = (0..n).filter(|&j| j != i).map(|j| (dist(si, sites[j]), j)).collect();
    order.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (dd, j) in order { let mut maxr = 0f32; for (q, _) in &poly { maxr = maxr.max(dist(si, *q)); } if dd * 0.5 > maxr { break; } if dd < 1e-4 { continue; }
        let sj = sites[j]; let nx = (sj.0 - si.0) / dd; let ny = (sj.1 - si.1) / dd; let m = ((si.0 + sj.0) * 0.5, (si.1 + sj.1) * 0.5);
        let side = |q: V| (q.0 - m.0) * nx + (q.1 - m.1) * ny; let mut np: Vec<(V, usize)> = Vec::with_capacity(poly.len() + 2); let k = poly.len();
        for e in 0..k { let (a, ta) = poly[e]; let b = poly[(e + 1) % k].0; let (sa, sb) = (side(a), side(b)); let (ina, inb) = (sa <= 0.0, sb <= 0.0);
            let x = if ina != inb { let t = sa / (sa - sb); (a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t) } else { a };
            if ina && inb { np.push((a, ta)); } else if ina { np.push((a, ta)); np.push((x, j)); } else if inb { np.push((x, ta)); } }
        if np.len() < 3 { poly = np; break; } poly = np; }
    out.push(poly); }
out
}



fn box_blur_1ch(b: &mut [f32], w: usize, h: usize, r: usize) {
    let mut t = vec![0f32; b.len()]; let r = r.min(64); let inv = 1.0 / (2 * r + 1) as f32;
    for y in 0..h { let mut acc = 0f32; for x in 0..=r.min(w - 1) { acc += b[y * w + x]; } for x in 0..w { t[y * w + x] = acc * inv; let add = x + r + 1; if add < w { acc += b[y * w + add]; } if x >= r { acc -= b[y * w + x - r]; } } }
    for x in 0..w { let mut acc = 0f32; for y in 0..=r.min(h - 1) { acc += t[y * w + x]; } for y in 0..h { b[y * w + x] = acc * inv; let add = y + r + 1; if add < h { acc += t[add * w + x]; } if y >= r { acc -= t[(y - r) * w + x]; } } }
}
