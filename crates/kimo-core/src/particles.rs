//! Deterministic particle emitter: closed-form birth schedule + per-particle integration (forces, turbulence, attractor, floor bounce),
//! life curves (size, color, alpha), trails, sprite shapes. Stateless per frame, so scrubbing and parallel render are exact.
use crate::effects::Frame;
use serde::{Deserialize, Serialize};

fn d1() -> f64 { 1.0 } fn d_half() -> f64 { 0.5 } fn d_rate() -> f64 { 60.0 } fn d_life() -> f64 { 2.0 } fn d_speed() -> f64 { 120.0 } fn d_sz() -> f64 { 6.0 }
fn d_shape() -> String { "point".into() } fn d_sprite() -> String { "soft".into() } fn d_c0() -> [f64; 3] { [1.0, 0.8, 0.4] } fn d_c1() -> [f64; 3] { [1.0, 0.2, 0.05] } fn d_up() -> f64 { 270.0 } fn d_spread() -> f64 { 30.0 } fn d_seed() -> u64 { 1 }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Emitter {
    #[serde(default = "d_shape")] pub shape: String,            // point | line | circle | rect | ring
    #[serde(default = "d_half")] pub x: f64, #[serde(default = "d_half")] pub y: f64,          // emitter center, normalized 0..1
    #[serde(default)] pub width: f64, #[serde(default)] pub height: f64,                       // emitter size, normalized to frame
    #[serde(default = "d_rate")] pub rate: f64, #[serde(default = "d_life")] pub life: f64, #[serde(default)] pub life_var: f64,
    #[serde(default = "d_speed")] pub speed: f64, #[serde(default)] pub speed_var: f64,        // px/s
    #[serde(default = "d_up")] pub angle: f64, #[serde(default = "d_spread")] pub spread: f64,  // degrees, 270 = up (screen coords)
    #[serde(default)] pub gravity: f64, #[serde(default)] pub wind: f64, #[serde(default)] pub drag: f64,
    #[serde(default)] pub turbulence: f64, #[serde(default = "d1")] pub turb_scale: f64,       // px/s^2, field size in frame fractions
    #[serde(default)] pub attractor_x: f64, #[serde(default)] pub attractor_y: f64, #[serde(default)] pub attractor: f64, // strength px/s^2 (0 = off), pos normalized
    #[serde(default)] pub floor: bool, #[serde(default = "d_half")] pub bounce: f64,
    /// Solid obstacles particles bounce off (analytic, deterministic). Empty = off.
    /// shape "layer": spawn on the alpha silhouette of `emit_clip` (read at the current frame); emit_edge spawns only on its outline.
    #[serde(default)] pub emit_clip: String, #[serde(default)] pub emit_edge: bool, #[serde(default)] pub emit_follow: bool,
    #[serde(default)] pub colliders: Vec<Collider>, #[serde(default = "d_fric")] pub friction: f64,
    /// Swirl force around (vortex_x, vortex_y) in px/s^2 (0 = off); positive = clockwise on screen.
    #[serde(default)] pub vortex: f64, #[serde(default)] pub vortex_x: f64, #[serde(default)] pub vortex_y: f64,
    #[serde(default = "d_sz")] pub size0: f64, #[serde(default = "d_sz")] pub size1: f64, #[serde(default)] pub size_var: f64,
    #[serde(default = "d_c0")] pub color0: [f64; 3], #[serde(default = "d_c1")] pub color1: [f64; 3],
    #[serde(default = "d1")] pub alpha0: f64, #[serde(default)] pub alpha1: f64,
    #[serde(default)] pub fade_in: f64, #[serde(default)] pub trail: u32, #[serde(default = "d_sprite")] pub sprite: String,        // soft | hard | streak | ring | star | spark | mote | puff | card
    #[serde(default)] pub shutter_deg: f64, #[serde(default)] pub blur_samples: u32, #[serde(default)] pub prewarm: bool, #[serde(default = "d_seed")] pub seed: u64, #[serde(default)] pub additive: bool,
}
fn d_fric() -> f64 { 0.15 }
/// kind: circle (x,y centre, r = radius as fraction of frame height), rect (x,y centre, w,h as frame fractions), plane (x,y point, angle = degrees of the outward normal, 270 = faces up on screen).
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct Collider { #[serde(default)] pub kind: String, #[serde(default)] pub x: f64, #[serde(default)] pub y: f64, #[serde(default)] pub r: f64, #[serde(default)] pub w: f64, #[serde(default)] pub h: f64, #[serde(default)] pub angle: f64,
    /// kind "layer": the alpha silhouette of this clip (rendered once at the current frame and held static); alpha >= `threshold` (default 0.5) is solid.
    #[serde(default)] pub clip_id: String, #[serde(default)] pub follow: bool, #[serde(default = "d_half")] pub threshold: f64 }
thread_local! { static EMIT_PTS: std::cell::RefCell<Vec<(f32, f32)>> = Default::default(); static EMIT_BINS: std::cell::RefCell<std::collections::HashMap<i64, std::rc::Rc<Vec<(f32, f32)>>>> = Default::default(); }
/// Width in seconds of one silhouette time bin for `emit_follow`.
pub const EMIT_BIN: f64 = 0.1;
/// Spawn points for shape "layer": every pixel with alpha >= 0.5 (or only outline pixels when `edge`), subsampled to at most 60k points.
/// A silhouette alpha plane shared with the renderer's cache: (width, height, alpha).
pub type Plane = (usize, usize, std::sync::Arc<Vec<f32>>);
thread_local! { static PTS_CACHE: std::cell::RefCell<Vec<(std::sync::Arc<Vec<f32>>, bool, std::rc::Rc<Vec<(f32, f32)>>)>> = Default::default(); static U8_CACHE: std::cell::RefCell<Vec<(std::sync::Arc<Vec<f32>>, Mask)>> = Default::default(); }
/// Derived forms are cached per source plane (the Rc is held, so its address cannot be reused); at most 96 of each.
fn points_cached(pl: Plane, edge: bool) -> std::rc::Rc<Vec<(f32, f32)>> {
    if let Some(h) = PTS_CACHE.with(|c| c.borrow().iter().find(|(r, e, _)| std::sync::Arc::ptr_eq(r, &pl.2) && *e == edge).map(|(_, _, v)| v.clone())) { return h; }
    let v = std::rc::Rc::new(mask_points(Some((pl.0, pl.1, &pl.2[..])), edge)); PTS_CACHE.with(|c| { let mut c = c.borrow_mut(); if c.len() >= 96 { c.remove(0); } c.push((pl.2.clone(), edge, v.clone())); }); v }
fn u8_cached(pl: Plane) -> Mask {
    if let Some(h) = U8_CACHE.with(|c| c.borrow().iter().find(|(r, _)| std::sync::Arc::ptr_eq(r, &pl.2)).map(|(_, v)| v.clone())) { return h; }
    let v = to_u8((pl.0, pl.1, &pl.2[..])); U8_CACHE.with(|c| { let mut c = c.borrow_mut(); if c.len() >= 96 { c.remove(0); } c.push((pl.2.clone(), v.clone())); }); v }
fn mask_points(m: Option<(usize, usize, &[f32])>, edge: bool) -> Vec<(f32, f32)> {
    let mut pts = Vec::new(); if let Some((w, h, d)) = m { let a = |x: usize, y: usize| d[y * w + x] >= 0.5;
        for y in 0..h { for x in 0..w { if !a(x, y) { continue; } if edge { let b = x == 0 || y == 0 || x + 1 >= w || y + 1 >= h || !a(x - 1, y) || !a(x + 1, y) || !a(x, y - 1) || !a(x, y + 1); if !b { continue; } } pts.push((x as f32 + 0.5, y as f32 + 0.5)); } }
        if pts.len() > 60000 { let st = pts.len() / 60000 + 1; pts = pts.into_iter().step_by(st).collect(); } }
    pts }
pub fn set_emit_mask(m: Option<Plane>, edge: bool) { let pts = match m { Some(pl) => (*points_cached(pl, edge)).clone(), None => Vec::new() }; EMIT_PTS.with(|e| *e.borrow_mut() = pts); }
/// emit_follow: one silhouette per time bin (bin index = floor(birth time / EMIT_BIN)); a particle spawns on the silhouette of its own birth bin.
pub fn set_emit_bins(bins: Vec<(i64, Option<Plane>)>, edge: bool) { let map = bins.into_iter().map(|(b, m)| (b, match m { Some(pl) => points_cached(pl, edge), None => std::rc::Rc::new(Vec::new()) })).collect(); EMIT_BINS.with(|e| *e.borrow_mut() = map); }
pub fn clear_emit_bins() { EMIT_BINS.with(|e| e.borrow_mut().clear()); }
type Mask = std::rc::Rc<(usize, usize, Vec<u8>)>;
thread_local! { static LAYER_MASKS: std::cell::RefCell<Vec<Option<Mask>>> = Default::default(); static LAYER_BINS: std::cell::RefCell<std::collections::HashMap<(usize, i64), Mask>> = Default::default(); }
fn to_u8(m: (usize, usize, &[f32])) -> Mask { std::rc::Rc::new((m.0, m.1, m.2.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8).collect())) }
/// Silhouette masks for the emitter's `layer` colliders, indexed like `colliders`; set by the renderer around the emitter render.
pub fn set_layer_masks(m: Vec<Option<Plane>>) { LAYER_MASKS.with(|l| *l.borrow_mut() = m.into_iter().map(|o| o.map(u8_cached)).collect()); }
/// collide_follow: one silhouette per (collider index, time bin) so a moving obstacle is solid where it was at the particle's own time.
pub fn set_layer_bins(b: Vec<(usize, i64, Option<Plane>)>) { LAYER_BINS.with(|l| *l.borrow_mut() = b.into_iter().filter_map(|(i, k, m)| m.map(|m| ((i, k), u8_cached(m)))).collect()); }
pub fn clear_layer_bins() { LAYER_BINS.with(|l| l.borrow_mut().clear()); }
fn layer_hit(idx: usize, follow: bool, tt: f64, thr: f64, px: f64, py: f64, v: (f64, f64)) -> Option<((f64, f64), f64)> {
    let m: Mask = if follow { let bin = (tt / EMIT_BIN).floor() as i64; LAYER_BINS.with(|l| l.borrow().get(&(idx, bin)).cloned()).or_else(|| LAYER_MASKS.with(|l| l.borrow().get(idx).cloned().flatten()))? } else { LAYER_MASKS.with(|l| l.borrow().get(idx).cloned().flatten())? };
    let (mw, mh, d) = (m.0, m.1, &m.2); let thr = thr as f32;
    let a = |x: f64, y: f64| -> f32 { if x < 0.0 || y < 0.0 || x >= mw as f64 || y >= mh as f64 { 0.0 } else { d[(y as usize) * mw + x as usize] as f32 / 255.0 } };
    if a(px, py) < thr { return None; }
    let r = 3.0; let (gx, gy) = (a(px + r, py) - a(px - r, py), a(px, py + r) - a(px, py - r)); let gl = ((gx * gx + gy * gy) as f64).sqrt();
    let n = if gl > 1e-3 { (-(gx as f64) / gl, -(gy as f64) / gl) } else {
        // deep inside (flat alpha): leave by the nearest exit among 16 directions
        let mut best = (f64::MAX, (0.0, -1.0)); for k in 0..16 { let ang = k as f64 * std::f64::consts::TAU / 16.0; let (dx, dy) = (ang.cos(), ang.sin()); let mut dd = 0.0; while dd < 400.0 && a(px + dx * dd, py + dy * dd) >= thr { dd += 2.0; } if dd < best.0 { best = (dd, (dx, dy)); } }
        let _ = v; best.1 };
    let lim = 400.0;
    let mut pen = 0.0; while pen < lim && a(px + n.0 * pen, py + n.1 * pen) >= thr { pen += 1.0; } Some((n, pen + 0.5))
}
/// If (px,py) is inside the collider return (outward unit normal, penetration depth).
fn hit(c: &Collider, px: f64, py: f64, w: f64, hh: f64) -> Option<((f64, f64), f64)> {
    match c.kind.as_str() {
        "circle" => { let (cx, cy, r) = (c.x * w, c.y * hh, c.r * hh); let (dx, dy) = (px - cx, py - cy); let d = (dx * dx + dy * dy).sqrt(); if d < r && r > 0.0 { let n = if d > 1e-6 { (dx / d, dy / d) } else { (0.0, -1.0) }; Some((n, r - d)) } else { None } }
        "rect" => { let (cx, cy, hw, hhh) = (c.x * w, c.y * hh, c.w * w * 0.5, c.h * hh * 0.5); let (dx, dy) = (px - cx, py - cy); if dx.abs() < hw && dy.abs() < hhh { let (ox, oy) = (hw - dx.abs(), hhh - dy.abs()); if ox < oy { Some(((dx.signum(), 0.0), ox)) } else { Some(((0.0, dy.signum()), oy)) } } else { None } }
        "plane" => { let a = c.angle.to_radians(); let n = (a.cos(), a.sin()); let d = (px - c.x * w) * n.0 + (py - c.y * hh) * n.1; if d < 0.0 { Some((n, -d)) } else { None } }
        _ => None }
}
impl Default for Emitter { fn default() -> Self { serde_json::from_str("{}").unwrap() } }

fn h(seed: u64, i: i64, k: u64) -> f64 { let mut x = seed ^ (i as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ k.wrapping_mul(0xBF58476D1CE4E5B9); x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9); x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB); x ^= x >> 31; (x >> 11) as f64 / (1u64 << 53) as f64 }
fn lerp(a: f64, b: f64, t: f64) -> f64 { a + (b - a) * t }

pub fn render(e: &Emitter, f: &mut Frame, t: f64) { render_fps(e, f, t, 25.0) }
/// Motion blur: when `shutter_deg` > 0 the particle system is integrated at `blur_samples` instants spread over the shutter interval
/// centred on `t` (shutter_deg/360 of a frame) and the premultiplied accumulation buffers are averaged, which is the temporal integral of what the
/// camera sees (exact for every sprite, speed and force, cost = samples x one render). Auto samples = fastest-particle travel over the shutter divided by 0.6 of the smallest sprite size, clamped 4..32 (so no visible stepping). 0 degrees = off, pixel-identical to before.
pub fn render_fps(e: &Emitter, f: &mut Frame, t: f64, fps: f64) {
    let buf = if e.shutter_deg > 0.0 { let sh = e.shutter_deg.clamp(0.0, 720.0) / 360.0 / fps.max(1.0); let n = if e.blur_samples == 0 { let sc = f.h as f64 / 1080.0; let travel = (e.speed + e.speed_var.abs()) * sc * sh; ((travel / (0.6 * e.size0.min(e.size1).max(1.0) * sc).max(1.0)).ceil() as usize).clamp(4, 32) } else { e.blur_samples.clamp(2, 64) as usize };
        let mut acc = vec![0f32; f.w * f.h * 4]; for k in 0..n { let tk = t + sh * ((k as f64 + 0.5) / n as f64 - 0.5); let b = accumulate(e, f.w, f.h, tk); for i in 0..acc.len() { acc[i] += b[i]; } } let inv = 1.0 / n as f32; for v in acc.iter_mut() { *v *= inv; } acc }
        else { accumulate(e, f.w, f.h, t) };
    composite(e, f, &buf);
}
fn accumulate(e: &Emitter, fw: usize, fh: usize, t: f64) -> Vec<f32> {
    let (w, hh) = (fw as f64, fh as f64); let sc = hh / 1080.0; let rate = e.rate.max(0.01); let lmax = e.life + e.life_var.abs();
    let i_hi = (t * rate).floor() as i64; let i_lo = ((t - lmax) * rate).floor() as i64 + 1; let i_lo = if e.prewarm { i_lo } else { i_lo.max(0) };
    let mut buf = vec![0f32; fw * fh * 4]; // premultiplied accumulation
    for i in i_lo..=i_hi {
        let birth = i as f64 / rate; let life = (e.life + (h(e.seed, i, 9) - 0.5) * 2.0 * e.life_var).max(0.05); let age = t - birth; if age < 0.0 || age > life { continue; }
        // spawn
        let (rx, ry, ra, rs) = (h(e.seed, i, 1), h(e.seed, i, 2), h(e.seed, i, 3), h(e.seed, i, 4));
        let (ew, eh) = (e.width * w, e.height * hh); let (cx, cy) = (e.x * w, e.y * hh);
        let (mut px, mut py) = match e.shape.as_str() { "line" => (cx + (rx - 0.5) * ew, cy + (ry - 0.5) * eh), "rect" => (cx + (rx - 0.5) * ew, cy + (ry - 0.5) * eh),
            "circle" => { let a = rx * std::f64::consts::TAU; let r = ry.sqrt(); (cx + a.cos() * r * ew * 0.5, cy + a.sin() * r * ew * 0.5) }
            "ring" => { let a = rx * std::f64::consts::TAU; (cx + a.cos() * ew * 0.5, cy + a.sin() * ew * 0.5) }
            "layer" => EMIT_BINS.with(|bb| { let bb = bb.borrow(); let bin = (birth / EMIT_BIN).floor() as i64; let sel = if e.emit_follow { bb.get(&bin).cloned() } else { None }; match sel { Some(v) => { if v.is_empty() { (cx, cy) } else { let q = v[((rx * v.len() as f64) as usize + (ry * 977.0) as usize * 31) % v.len()]; (q.0 as f64, q.1 as f64) } } None => EMIT_PTS.with(|pp| { let pp = pp.borrow(); if pp.is_empty() { (cx, cy) } else { let q = pp[((rx * pp.len() as f64) as usize + (ry * 977.0) as usize * 31) % pp.len()]; (q.0 as f64, q.1 as f64) } }) } }), _ => (cx, cy) };
        let ang = (e.angle + (ra - 0.5) * 2.0 * e.spread).to_radians(); let spd = (e.speed + (rs - 0.5) * 2.0 * e.speed_var) * sc;
        let (mut vx, mut vy) = (ang.cos() * spd, ang.sin() * spd); if e.shape == "ring" && e.spread >= 179.0 { let d = (px - cx).atan2(py - cy); let _ = d; let a = (py - cy).atan2(px - cx); vx = a.cos() * spd; vy = a.sin() * spd; }
        // integrate
        let steps = ((age * 60.0).ceil() as usize).clamp(1, 400); let dt = age / steps as f64; let mut hist: Vec<(f64, f64)> = Vec::new(); let every = (steps / (e.trail as usize + 1).max(1)).max(1);
        for s in 0..steps { let tt = birth + s as f64 * dt;
            let mut ax = e.wind * sc; let mut ay = e.gravity * sc;
            if e.turbulence != 0.0 { let k = 6.0 / (e.turb_scale.max(0.05) * w.max(hh)); ax += e.turbulence * sc * ((py * k * 0.9 + tt * 1.3 + rx * 6.0).sin() + 0.5 * (py * k * 2.1 - tt * 0.7).cos()); ay += e.turbulence * sc * ((px * k * 1.1 - tt * 1.1 + ry * 6.0).cos() + 0.5 * (px * k * 1.9 + tt * 0.9).sin()); }
            if e.attractor != 0.0 { let (dx, dy) = (e.attractor_x * w - px, e.attractor_y * hh - py); let d = (dx * dx + dy * dy).sqrt().max(20.0 * sc); ax += dx / d * e.attractor * sc; ay += dy / d * e.attractor * sc; }
            if e.vortex != 0.0 { let (dx, dy) = (px - e.vortex_x * w, py - e.vortex_y * hh); let d = (dx * dx + dy * dy).sqrt().max(20.0 * sc); ax += -dy / d * e.vortex * sc; ay += dx / d * e.vortex * sc; }
            let dr = (-e.drag * dt).exp(); vx = (vx + ax * dt) * dr; vy = (vy + ay * dt) * dr; px += vx * dt; py += vy * dt;
            if e.floor && py > hh { py = hh; vy = -vy.abs() * e.bounce; vx *= 0.85; }
            for (ci, c) in e.colliders.iter().enumerate() { let hh_ = if c.kind == "layer" { layer_hit(ci, c.follow, tt, c.threshold, px, py, (vx, vy)) } else { hit(c, px, py, w, hh) }; if let Some(((nx, ny), pen)) = hh_ { px += nx * pen; py += ny * pen; let vn = vx * nx + vy * ny; if vn < 0.0 { let (tx, ty) = (vx - vn * nx, vy - vn * ny); let k = 1.0 - e.friction.clamp(0.0, 1.0); vx = tx * k - vn * nx * e.bounce; vy = ty * k - vn * ny * e.bounce; } } }
            if e.trail > 0 && s % every == 0 { hist.push((px, py)); } }
        let u = age / life; let size = lerp(e.size0, e.size1, u) * sc * (1.0 + (h(e.seed, i, 5) - 0.5) * 2.0 * e.size_var); let mut alpha = lerp(e.alpha0, e.alpha1, u).clamp(0.0, 1.5); if e.fade_in > 0.0 { let t = (u / e.fade_in).clamp(0.0, 1.0); alpha *= t * t * (3.0 - 2.0 * t); }
        let col = [lerp(e.color0[0], e.color1[0], u), lerp(e.color0[1], e.color1[1], u), lerp(e.color0[2], e.color1[2], u)];
        let mut pts: Vec<(f64, f64, f64)> = hist.iter().rev().enumerate().map(|(k, p)| (p.0, p.1, alpha * (1.0 - (k as f64 + 1.0) / (hist.len() as f64 + 1.0)) * 0.6)).collect(); pts.insert(0, (px, py, alpha));
        // card_phase: pure function of (t, particle index) -- same flip state no matter how many sub-steps were taken, so scrubbing stays exact.
        let card_phase = ((t * 4.5 + h(e.seed, i, 17) * std::f64::consts::TAU).cos()) as f32;
        for (k, (x, y, a)) in pts.iter().enumerate() { let s_ = size * (1.0 - 0.5 * k as f64 / pts.len() as f64).max(0.2); stamp(&mut buf, fw, fh, *x, *y, s_.max(0.4), *a as f32, col, &e.sprite, vx, vy, card_phase); }
    }
    buf
}
fn composite(e: &Emitter, f: &mut Frame, buf: &[f32]) {
    // composite premultiplied buffer over frame
    for i in 0..f.w * f.h { let (r, g, b, a) = (buf[i * 4], buf[i * 4 + 1], buf[i * 4 + 2], buf[i * 4 + 3]); if a <= 0.0 { continue; } let a = a.min(1.0);
        let o = &mut f.d[i * 4..i * 4 + 4]; if e.additive { for (j, c) in [r, g, b].iter().enumerate() { o[j] = (o[j] * o[3] + c).min(4.0); } o[3] = (o[3] + a).min(1.0); let al = o[3].max(1e-6); for j in 0..3 { o[j] /= al; } }
        else { let na = a + o[3] * (1.0 - a); for (j, c) in [r, g, b].iter().enumerate() { o[j] = (c + o[j] * o[3] * (1.0 - a)) / na.max(1e-6); } o[3] = na; } }
}

fn stamp(buf: &mut [f32], w: usize, hgt: usize, x: f64, y: f64, r: f64, a: f32, c: [f64; 3], sprite: &str, vx: f64, vy: f64, card_phase: f32) {
    let ext = match sprite { "streak" => r * 3.0 + vx.hypot(vy) * 0.04, "spark" => r * 4.0, "card" => r * 1.4, _ => r * 2.6 } + 1.0;
    let (x0, x1, y0, y1) = ((x - ext).floor().max(0.0) as usize, ((x + ext).ceil() as usize).min(w.saturating_sub(1)), (y - ext).floor().max(0.0) as usize, ((y + ext).ceil() as usize).min(hgt.saturating_sub(1)));
    let vl = vx.hypot(vy).max(1e-6); let (dx, dy) = (vx / vl, vy / vl); let slen = (vl * 0.035).clamp(r, r * 8.0);
    for yy in y0..=y1 { for xx in x0..=x1 { let (px, py) = (xx as f64 + 0.5 - x, yy as f64 + 0.5 - y); let d = px.hypot(py) / r;
        let k = match sprite { "hard" => (1.0 - (d - 1.0) * 2.0).clamp(0.0, 1.0), "ring" => { (1.0 - ((d - 1.0).abs() * r / (r * 0.18).max(0.7))).clamp(0.0, 1.0) }
            "star" => { let ang = py.atan2(px); let rr = 0.55 + 0.45 * (ang * 4.0).cos().abs().powf(3.0); ((1.0 - d / (1.6 * rr)).clamp(0.0, 1.0)).powf(1.5) }
            "streak" | "spark" => { let along = px * dx + py * dy; let perp = -px * dy + py * dx; let l = if sprite == "spark" { slen * 1.6 } else { slen }; let q = (along / l).abs().min(2.0) + 0.0; let core = (1.0 - (perp.abs() / (r * 0.6)).min(1.0)) * (1.0 - (along.abs() / (l * if along > 0.0 { 0.3 } else { 1.0 })).min(1.0)); let _ = q; core.max(0.0).powf(1.2) }
            "mote" => { let t = ((1.3 - d) / 0.7).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) * 0.9 }
            "puff" => (-(d * d) * 1.1).exp() * (1.0 - d / 2.6).clamp(0.0, 1.0),
            // "card": a flat rectangular sprite (After Effects Card Dance style) whose half-width is scaled by
            // |cos(card_phase)|, a continuous function of (time, particle seed) computed by the caller -- as the
            // phase sweeps through +-90deg the card edge-collapses to a sliver and reopens, the 2D stand-in for a
            // 3D flip about the card's own vertical axis. Deterministic and scrub-safe: a pure function of t, not frame count.
            "card" => { let hw = (r * 0.9 * card_phase.abs().max(0.05) as f64).max(0.6); let hh2 = r * 0.42; if px.abs() <= hw && py.abs() <= hh2 {
                    let fx = 1.0 - (px.abs() / hw.max(1e-6)).powi(8); let fy = 1.0 - (py.abs() / hh2.max(1e-6)).powi(8); (fx.min(fy)).clamp(0.0, 1.0)
                } else { 0.0 } }
            _ => (1.0 - d / 2.6).max(0.0).powi(3) * 1.8 } as f32;
        let kk = (k * a).min(1.0); if kk <= 0.0 { continue; } let o = (yy * w + xx) * 4; buf[o] += c[0] as f32 * kk; buf[o + 1] += c[1] as f32 * kk; buf[o + 2] += c[2] as f32 * kk; buf[o + 3] += kk; } }
}

pub fn preset(name: &str) -> Result<serde_json::Value, String> {
    use serde_json::json;
    Ok(match name {
        "fire" => json!({"shape":"line","x":0.5,"y":0.95,"width":0.25,"height":0.0,"rate":220,"life":1.4,"life_var":0.5,"speed":260,"speed_var":120,"angle":270,"spread":14,"gravity":-120,"turbulence":520,"turb_scale":0.25,"drag":0.6,"size0":38,"size1":6,"size_var":0.5,"color0":[1.0,0.72,0.25],"color1":[0.9,0.1,0.0],"alpha0":0.7,"alpha1":0.0,"sprite":"soft","additive":true,"prewarm":true,"seed":3}),
        "sparks" => json!({"shape":"point","x":0.5,"y":0.8,"rate":170,"life":1.2,"life_var":0.5,"speed":700,"speed_var":420,"angle":270,"spread":55,"gravity":900,"drag":0.4,"size0":3.5,"size1":1.0,"color0":[1.0,0.9,0.6],"color1":[1.0,0.35,0.05],"alpha0":1.0,"alpha1":0.0,"sprite":"streak","trail":3,"floor":true,"bounce":0.4,"additive":true,"prewarm":true,"seed":5}),
        "snow" => json!({"shape":"line","x":0.5,"y":-0.05,"width":1.2,"rate":90,"life":9,"life_var":3,"speed":70,"speed_var":40,"angle":90,"spread":12,"wind":14,"turbulence":38,"turb_scale":0.5,"size0":5,"size1":5,"size_var":0.6,"color0":[1,1,1],"color1":[0.85,0.92,1],"alpha0":0.8,"alpha1":0.8,"fade_in":0.1,"sprite":"soft","prewarm":true,"seed":9}),
        "rain" => json!({"shape":"line","x":0.5,"y":-0.05,"width":1.3,"rate":320,"life":1.1,"speed":1500,"speed_var":300,"angle":100,"spread":2,"size0":1.6,"size1":1.6,"color0":[0.75,0.85,1],"color1":[0.75,0.85,1],"alpha0":0.5,"alpha1":0.5,"sprite":"streak","prewarm":true,"seed":2}),
        "smoke" => json!({"shape":"line","x":0.5,"y":0.95,"width":0.07,"rate":36,"life":8,"life_var":2.5,"speed":75,"speed_var":35,"angle":270,"spread":8,"wind":16,"turbulence":140,"turb_scale":0.28,"drag":0.25,"size0":45,"size1":210,"size_var":0.6,"color0":[0.66,0.66,0.7],"color1":[0.32,0.32,0.36],"alpha0":0.06,"alpha1":0.0,"fade_in":0.15,"sprite":"puff","prewarm":true,"seed":4}),
        "dust" => json!({"shape":"rect","x":0.5,"y":0.5,"width":1.1,"height":1.1,"rate":9,"life":10,"life_var":3,"speed":8,"speed_var":6,"angle":300,"spread":90,"turbulence":10,"turb_scale":0.5,"size0":6,"size1":8,"size_var":1.2,"color0":[1.0,0.93,0.8],"color1":[0.9,0.92,1.0],"alpha0":0.16,"alpha1":0.16,"fade_in":0.25,"sprite":"mote","additive":true,"prewarm":true,"seed":41}),
        "confetti" => json!({"shape":"point","x":0.5,"y":0.9,"rate":150,"life":3.2,"life_var":0.8,"speed":1100,"speed_var":500,"angle":270,"spread":38,"gravity":1100,"drag":1.4,"turbulence":220,"turb_scale":0.3,"size0":9,"size1":9,"size_var":0.5,"color0":[1.0,0.3,0.5],"color1":[0.3,0.7,1.0],"alpha0":1,"alpha1":1,"sprite":"hard","seed":13}),
        "fireflies" => json!({"shape":"rect","x":0.5,"y":0.6,"width":1.0,"height":0.7,"rate":8,"life":6,"life_var":2,"speed":16,"speed_var":10,"angle":270,"spread":180,"turbulence":60,"turb_scale":0.3,"size0":7,"size1":7,"size_var":0.5,"color0":[1.0,0.95,0.55],"color1":[0.7,1.0,0.5],"alpha0":0.9,"alpha1":0.0,"sprite":"soft","additive":true,"prewarm":true,"seed":21}),
        "bokeh" => json!({"shape":"rect","x":0.5,"y":0.5,"width":1.1,"height":1.1,"rate":6,"life":8,"life_var":2,"speed":14,"speed_var":8,"angle":300,"spread":60,"size0":44,"size1":60,"size_var":0.6,"color0":[1.0,0.85,0.6],"color1":[0.6,0.7,1.0],"alpha0":0.0,"alpha1":0.0,"sprite":"ring","additive":true,"prewarm":true,"seed":17}),
        "magic_dust" => json!({"shape":"circle","x":0.5,"y":0.5,"width":0.15,"rate":120,"life":1.8,"life_var":0.6,"speed":120,"speed_var":90,"angle":270,"spread":180,"gravity":-30,"drag":0.9,"turbulence":160,"turb_scale":0.2,"size0":9,"size1":0.5,"color0":[1.0,0.95,0.7],"color1":[0.75,0.55,1.0],"alpha0":1,"alpha1":0,"sprite":"star","additive":true,"prewarm":true,"seed":23}),
        "explosion" => json!({"shape":"point","x":0.5,"y":0.5,"rate":700,"life":1.6,"life_var":0.6,"speed":900,"speed_var":700,"angle":0,"spread":180,"drag":2.2,"gravity":200,"turbulence":300,"size0":18,"size1":3,"size_var":0.4,"color0":[1.0,0.85,0.45],"color1":[0.8,0.1,0.0],"alpha0":0.5,"alpha1":0.0,"sprite":"soft","trail":2,"additive":true,"seed":31}),
        // Rising bubbles (CC Bubbles / Foam-adjacent): negative gravity = buoyancy, ring sprite reads as a bubble rim, gentle turbulence for wobble, fades near the top (pop).
        "bubbles" => json!({"shape":"rect","x":0.5,"y":1.02,"width":1.05,"height":0.0,"rate":40,"life":4.5,"life_var":1.5,"speed":95,"speed_var":40,"angle":270,"spread":16,"gravity":-55,"drag":0.3,"turbulence":55,"turb_scale":0.4,"size0":9,"size1":15,"size_var":0.5,"color0":[0.82,0.95,1.0],"color1":[0.92,1.0,1.0],"alpha0":0.45,"alpha1":0.0,"fade_in":0.08,"sprite":"ring","prewarm":true,"seed":51}),
        // Foam at a water line (CC Foam-adjacent, particle approximation not a metaball/fluid solver): dense short-lived white
        // flecks scattered near the surface line, high turbulence for the jitter, settles back with positive gravity.
        "foam" => json!({"shape":"line","x":0.5,"y":0.85,"width":1.1,"height":0.02,"rate":260,"life":0.6,"life_var":0.3,"speed":20,"speed_var":16,"angle":270,"spread":170,"gravity":55,"drag":1.2,"turbulence":190,"turb_scale":0.15,"size0":5,"size1":2,"size_var":0.6,"color0":[1,1,1],"color1":[0.9,0.95,1.0],"alpha0":0.55,"alpha1":0.0,"sprite":"soft","prewarm":true,"seed":52}),
        // Card Dance (AE Particle Playground style): a field of flat rectangular "card" sprites tumbling as they fall, the
        // flip driven by the new `card` sprite kind (see `stamp`), independent per-card via seed-derived phase offset.
        "card_dance" => json!({"shape":"rect","x":0.5,"y":-0.05,"width":1.05,"height":0.0,"rate":14,"life":5.0,"life_var":1.5,"speed":40,"speed_var":25,"angle":90,"spread":20,"gravity":95,"wind":12,"drag":0.5,"turbulence":45,"turb_scale":0.3,"size0":46,"size1":46,"size_var":0.35,"color0":[0.95,0.25,0.35],"color1":[0.25,0.55,0.95],"alpha0":1.0,"alpha1":1.0,"sprite":"card","prewarm":true,"seed":53}),
        o => return Err(format!("unknown preset {o}; presets: dust fire sparks snow rain smoke confetti fireflies bokeh magic_dust explosion bubbles foam card_dance")),
    })
}
