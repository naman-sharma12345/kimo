//! Deterministic rigid-body world (Blender-style basics): sphere-proxy bodies with mass, linear and angular velocity,
//! gravity, linear/angular drag, floor plane with restitution and Coulomb friction, body-body impulse collisions
//! with spatial-hash broadphase. Fixed 240 Hz step with sub-stepped semi-implicit integration. Results are baked to a
//! trajectory so rendering any frame is stateless and scrub-safe.
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct Body { pub p: [f64; 3], pub v: [f64; 3], pub q: [f64; 4], pub w: [f64; 3], pub r: f64, pub m: f64, pub birth: f64 }
#[derive(Clone, Debug)]
pub struct Params { pub gravity: [f64; 3], pub drag: f64, pub ang_drag: f64, pub restitution: f64, pub friction: f64, pub floor_y: Option<f64>, pub collide: bool }
pub struct Traj { pub dt: f64, pub n: usize, pub data: Vec<Vec<[f32; 7]>> }

fn qmul(a: [f64; 4], b: [f64; 4]) -> [f64; 4] { [a[0] * b[0] - a[1] * b[1] - a[2] * b[2] - a[3] * b[3], a[0] * b[1] + a[1] * b[0] + a[2] * b[3] - a[3] * b[2], a[0] * b[2] - a[1] * b[3] + a[2] * b[0] + a[3] * b[1], a[0] * b[3] + a[1] * b[2] - a[2] * b[1] + a[3] * b[0]] }
fn qnorm(q: [f64; 4]) -> [f64; 4] { let m = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt().max(1e-12); [q[0] / m, q[1] / m, q[2] / m, q[3] / m] }
pub fn quat_euler(d: [f64; 3]) -> [f64; 4] { let (a, b, c) = (d[0].to_radians() / 2.0, d[1].to_radians() / 2.0, d[2].to_radians() / 2.0); let qx = [a.cos(), a.sin(), 0.0, 0.0]; let qy = [b.cos(), 0.0, b.sin(), 0.0]; let qz = [c.cos(), 0.0, 0.0, c.sin()]; qnorm(qmul(qx, qmul(qy, qz))) }
pub fn quat_matrix(q: [f64; 4]) -> [[f64; 3]; 3] { let [w, x, y, z] = qnorm(q); [[1.0 - 2.0 * (y * y + z * z), 2.0 * (x * y - w * z), 2.0 * (x * z + w * y)], [2.0 * (x * y + w * z), 1.0 - 2.0 * (x * x + z * z), 2.0 * (y * z - w * x)], [2.0 * (x * z - w * y), 2.0 * (y * z + w * x), 1.0 - 2.0 * (x * x + y * y)]] }
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }

/// Simulate to `total` seconds, bake poses at the fixed step.
pub fn bake(mut bodies: Vec<Body>, par: &Params, total: f64) -> Traj {
    let dt = 1.0 / 240.0; let n = (total / dt).ceil() as usize + 2; let nb = bodies.len(); let mut data: Vec<Vec<[f32; 7]>> = vec![Vec::with_capacity(n); nb];
    let rmax = bodies.iter().map(|b| b.r).fold(1.0, f64::max); let cell = rmax * 2.0;
    for s in 0..n { let t = s as f64 * dt;
        for (i, b) in bodies.iter().enumerate() { data[i].push([b.p[0] as f32, b.p[1] as f32, b.p[2] as f32, b.q[0] as f32, b.q[1] as f32, b.q[2] as f32, b.q[3] as f32]); }
        for b in bodies.iter_mut() { if t < b.birth { continue; }
            for a in 0..3 { b.v[a] += par.gravity[a] * dt; } let dk = (-par.drag * dt).exp(); for a in 0..3 { b.v[a] *= dk; } let ak = (-par.ang_drag * dt).exp(); for a in 0..3 { b.w[a] *= ak; }
            for a in 0..3 { b.p[a] += b.v[a] * dt; } let wq = [0.0, b.w[0], b.w[1], b.w[2]]; let dq = qmul(wq, b.q); for a in 0..4 { b.q[a] += 0.5 * dq[a] * dt; } b.q = qnorm(b.q); }
        if let Some(fy) = par.floor_y { for b in bodies.iter_mut() { if t < b.birth { continue; } let pen = b.p[1] + b.r - fy; if pen > 0.0 { b.p[1] -= pen;
            let rc = [0.0, b.r, 0.0]; let vc = [b.v[0] + cross(b.w, rc)[0], b.v[1] + cross(b.w, rc)[1], b.v[2] + cross(b.w, rc)[2]]; let nrm = [0.0, -1.0, 0.0]; let vn = dot(vc, nrm);
            if vn < 0.0 { let inv_i = 1.0 / (0.4 * b.m * b.r * b.r); let e = if -vn < 30.0 { 0.0 } else { par.restitution }; let jn = -(1.0 + e) * vn / (1.0 / b.m + b.r * b.r * inv_i * 0.0 + 0.0);
                for a in 0..3 { b.v[a] += nrm[a] * jn / b.m; }
                let vt = [vc[0] - vn * nrm[0], vc[1] - vn * nrm[1], vc[2] - vn * nrm[2]]; let vtm = (dot(vt, vt)).sqrt();
                if vtm > 1e-6 { let jt = (par.friction * jn).min(vtm * b.m / 3.5); let td = [vt[0] / vtm, vt[1] / vtm, vt[2] / vtm]; for a in 0..3 { b.v[a] -= td[a] * jt / b.m; } let tq = cross(rc, [-td[0] * jt, -td[1] * jt, -td[2] * jt]); for a in 0..3 { b.w[a] += tq[a] * inv_i; } } } } } }
        if par.collide && nb > 1 { let mut grid: HashMap<(i32, i32, i32), Vec<usize>> = HashMap::new();
            for (i, b) in bodies.iter().enumerate() { if t < b.birth { continue; } grid.entry(((b.p[0] / cell).floor() as i32, (b.p[1] / cell).floor() as i32, (b.p[2] / cell).floor() as i32)).or_default().push(i); }
            let mut pairs: Vec<(usize, usize)> = vec![]; for (&(cx, cy, cz), v) in grid.iter() { for dx in -1..=1 { for dy in -1..=1 { for dz in -1..=1 { if let Some(o) = grid.get(&(cx + dx, cy + dy, cz + dz)) { for &i in v { for &j in o { if i < j { pairs.push((i, j)); } } } } } } } }
            pairs.sort_unstable(); pairs.dedup();
            for (i, j) in pairs { let (a, b) = { let (l, r) = bodies.split_at_mut(j); (&mut l[i], &mut r[0]) }; let d = [b.p[0] - a.p[0], b.p[1] - a.p[1], b.p[2] - a.p[2]]; let dist = dot(d, d).sqrt(); let rr = (a.r + b.r) * 0.8; if dist >= rr || dist < 1e-9 { continue; }
                let nrm = [d[0] / dist, d[1] / dist, d[2] / dist]; let (ia, ib) = (1.0 / a.m, 1.0 / b.m); let pen = rr - dist; for k in 0..3 { a.p[k] -= nrm[k] * pen * ia / (ia + ib); b.p[k] += nrm[k] * pen * ib / (ia + ib); }
                let rv = [b.v[0] - a.v[0], b.v[1] - a.v[1], b.v[2] - a.v[2]]; let vn = dot(rv, nrm); if vn < 0.0 { let jn = -(1.0 + par.restitution) * vn / (ia + ib); for k in 0..3 { a.v[k] -= nrm[k] * jn * ia; b.v[k] += nrm[k] * jn * ib; }
                    let vt = [rv[0] - vn * nrm[0], rv[1] - vn * nrm[1], rv[2] - vn * nrm[2]]; let vtm = dot(vt, vt).sqrt(); if vtm > 1e-6 { let jt = (par.friction * jn).min(vtm / (ia + ib) / 3.5); for k in 0..3 { let td = vt[k] / vtm; a.v[k] += td * jt * ia; b.v[k] -= td * jt * ib; } let sp = cross(nrm, [vt[0] / vtm, vt[1] / vtm, vt[2] / vtm]); for k in 0..3 { a.w[k] += sp[k] * jt * ia / a.r * 0.8; b.w[k] += sp[k] * jt * ib / b.r * 0.8; } } } } }
    }
    Traj { dt, n, data }
}
impl Traj { pub fn at(&self, i: usize, t: f64) -> ([f64; 3], [f64; 4]) { let f = (t / self.dt).max(0.0); let a = (f.floor() as usize).min(self.n - 1); let b = (a + 1).min(self.n - 1); let u = f - f.floor(); let (x, y) = (self.data[i][a], self.data[i][b]);
    let l = |k: usize| x[k] as f64 * (1.0 - u) + y[k] as f64 * u; ([l(0), l(1), l(2)], qnorm([l(3), l(4), l(5), l(6)])) } }

// ---- 2D layer physics: circle proxies bouncing inside the canvas ------------------------------------------------------------------------
#[derive(Clone, Debug)]
pub struct Body2 { pub x: f64, pub y: f64, pub vx: f64, pub vy: f64, pub r: f64, pub m: f64, pub rot: f64, pub w: f64 }
pub struct Params2 { pub gx: f64, pub gy: f64, pub drag: f64, pub restitution: f64, pub friction: f64, pub half_w: f64, pub half_h: f64, pub walls: [bool; 4], pub collide: bool }
/// Simulate at 480 Hz sub-steps, sample once per frame. Coordinates are pixel offsets from the canvas centre (y down), rot in degrees. Walls: [left, right, top, bottom].
pub fn sim2d(mut b: Vec<Body2>, p: &Params2, fps: f64, frames: usize) -> Vec<Vec<[f64; 3]>> {
    let sub = (480.0 / fps).ceil().max(1.0) as usize; let dt = 1.0 / (fps * sub as f64); let mut out = vec![b.iter().map(|q| [q.x, q.y, q.rot]).collect::<Vec<_>>()];
    for _ in 1..frames { for _ in 0..sub {
        for q in b.iter_mut() { q.vx += p.gx * dt; q.vy += p.gy * dt; let d = (1.0 - p.drag * dt).max(0.0); q.vx *= d; q.vy *= d; q.w *= (1.0 - 0.3 * p.drag * dt).max(0.0); q.x += q.vx * dt; q.y += q.vy * dt; q.rot += q.w * dt;
            let (lx, rx, ty, by) = (-p.half_w + q.r, p.half_w - q.r, -p.half_h + q.r, p.half_h - q.r);
            if p.walls[0] && q.x < lx { q.x = lx; if q.vx < 0.0 { q.vx = -q.vx * p.restitution; q.w += -q.vy * p.friction * 0.5 / q.r * 57.2958 * 0.0 + (q.vy / q.r) * 57.2958 * p.friction * 0.1; } }
            if p.walls[1] && q.x > rx { q.x = rx; if q.vx > 0.0 { q.vx = -q.vx * p.restitution; q.w += -(q.vy / q.r) * 57.2958 * p.friction * 0.1; } }
            if p.walls[2] && q.y < ty { q.y = ty; if q.vy < 0.0 { q.vy = -q.vy * p.restitution; q.w += (q.vx / q.r) * 57.2958 * p.friction * 0.1; } }
            if p.walls[3] && q.y > by { q.y = by; if q.vy > 0.0 { q.vy = -q.vy * p.restitution; if q.vy.abs() < 25.0 { q.vy = 0.0; } let roll = q.vx / q.r * 57.2958; q.w += (roll - q.w) * p.friction.min(1.0) * 0.2; q.vx *= 1.0 - p.friction * 0.02; } } }
        if p.collide { for i in 0..b.len() { for j in i + 1..b.len() { let (dx, dy) = (b[j].x - b[i].x, b[j].y - b[i].y); let d = dx.hypot(dy); let rs = b[i].r + b[j].r; if d < rs && d > 1e-9 { let (nx, ny) = (dx / d, dy / d); let (mi, mj) = (b[i].m, b[j].m); let tot = mi + mj;
            let pen = rs - d; b[i].x -= nx * pen * mj / tot; b[i].y -= ny * pen * mj / tot; b[j].x += nx * pen * mi / tot; b[j].y += ny * pen * mi / tot;
            let rv = (b[j].vx - b[i].vx) * nx + (b[j].vy - b[i].vy) * ny; if rv < 0.0 { let jn = -(1.0 + p.restitution) * rv / (1.0 / mi + 1.0 / mj); b[i].vx -= jn * nx / mi; b[i].vy -= jn * ny / mi; b[j].vx += jn * nx / mj; b[j].vy += jn * ny / mj;
                let (tx, ty) = (-ny, nx); let tv = (b[j].vx - b[i].vx) * tx + (b[j].vy - b[i].vy) * ty; let spin = tv * p.friction * 0.3 * 57.2958; b[i].w += spin / b[i].r.max(1.0); b[j].w += spin / b[j].r.max(1.0); } } } } } }
        out.push(b.iter().map(|q| [q.x, q.y, q.rot]).collect()); }
    out
}
