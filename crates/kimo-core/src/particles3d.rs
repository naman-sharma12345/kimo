//! Particles3D (lane K4, Trapcode Particular class): a 3D particle LAYER on the timeline, seen through the project camera.
//! Stateless and closed form, so scrubbing is exact and order-independent: particle i is born at start + i / rate (sub-frame, never
//! snapped), its randomness is a hash of (seed, i) only, its emitter position is sampled AT its birth time, and its position at age
//! tau is the exact solution of dv/dt = g - k v (linear drag k >= 0). Per frame cost is O(live particles), no per-particle allocation.
//! World units = project pixels on the focus plane: origin at the frame centre, +x right, +y down, +z away from the camera. The project
//! camera (three_d::Camera from `set_camera` / `set_camera_keyframe` props, the camera 3D layers and Scene3D use) has its eye at
//! cam_pos - [0, 0, f], f = (h/2) / tan(fov/2) px, so world z = 0 at the default camera is the focus plane (projection scale 1).
//! `camera_at` maps it to the explicit camera_projection::Camera (sensor_height = h, focal_length = f, near = 1 like CamView).
//! Formulas: closed-form drag solution (textbook ODE), gaussian splat kernel from particle_flow.rs; see THIRD_PARTY/particles3d.md.
use crate::effects::Frame;
use crate::model::{Clip, Project};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const EMITTER_KINDS: &[&str] = &["point", "box", "sphere", "light"];
pub const SPRITE_KINDS: &[&str] = &["disc", "layer"];
pub const TEXTURE_TIMES: &[&str] = &["current", "birth"];
pub const BLENDS: &[&str] = &["additive", "normal"];
/// Live-particle cap (rate x lifetime), shared with particle_flow's ring pool.
pub const MAX_LIVE: f64 = crate::particle_flow::MAX_PARTICLES as f64;
/// Most distinct sprite textures one frame renders for texture_time "birth" (time bins widen beyond this).
pub const MAX_TEXTURE_BINS: f64 = 16.0;

fn d_kind() -> String { "point".into() } fn d_rate() -> f64 { 100.0 } fn d_life() -> f64 { 2.0 } fn d_speed() -> f64 { 200.0 } fn d_dir() -> [f64; 3] { [0.0, -1.0, 0.0] } fn d_spread() -> f64 { 20.0 } fn d_seed() -> u64 { 1 }
fn d_size3() -> [f64; 3] { [100.0; 3] } fn d_radius() -> f64 { 50.0 } fn d_disc() -> String { "disc".into() } fn d_cur() -> String { "current".into() } fn d_true() -> bool { true } fn d_ssize() -> f64 { 10.0 } fn d_hard() -> f64 { 3.0 } fn d_white() -> [f64; 3] { [1.0; 3] }
fn d_blend() -> String { "additive".into() } fn d_samples() -> u32 { 8 } fn d_shutter() -> f64 { 180.0 }
/// Emitter position key at clip-local time `t` (linear between keys, held outside).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct PosKey { pub t: f64, pub position: [f64; 3] }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Emitter3 {
    #[serde(default = "d_kind")] pub kind: String, // point | box | sphere | light
    #[serde(default)] pub position: [f64; 3], #[serde(default)] pub position_keys: Vec<PosKey>,
    #[serde(default = "d_size3")] pub size: [f64; 3], #[serde(default = "d_radius")] pub radius: f64, // box size, sphere radius (world units)
    /// kind light: follows this project light (set_light; x,y frame fractions, z frame-heights toward the camera); `position` is an offset.
    #[serde(default)] pub light_id: String,
    #[serde(default = "d_rate")] pub rate: f64, #[serde(default = "d_life")] pub lifetime: f64,
    #[serde(default = "d_speed")] pub speed: f64, #[serde(default)] pub speed_var: f64, // speed_var: fraction 0..1
    #[serde(default = "d_dir")] pub direction: [f64; 3], #[serde(default = "d_spread")] pub spread_deg: f64, #[serde(default = "d_seed")] pub seed: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Physics { #[serde(default)] pub gravity: [f64; 3], #[serde(default)] pub drag: f64 }
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sprite {
    #[serde(default = "d_disc")] pub kind: String, // disc | layer
    #[serde(default)] pub clip_id: String, #[serde(default = "d_cur")] pub texture_time: String, // birth | current
    /// layer sprite: the source clip is not drawn on its own (AE: the sprite layer is usually hidden).
    #[serde(default = "d_true")] pub hide_source: bool,
    /// World units: disc diameter, or the width the layer sprite's frame spans (size == clip width at unit projection scale = 1:1 pixels).
    #[serde(default = "d_ssize")] pub size: f64, #[serde(default)] pub size_end: Option<f64>,
    #[serde(default = "d_hard")] pub hardness: f64, #[serde(default = "d_white")] pub color: [f64; 3], #[serde(default)] pub color_end: Option<[f64; 3]>,
    /// [[life 0..1, opacity], ...] piecewise linear over normalized age; empty = 1.
    #[serde(default)] pub opacity_curve: Vec<[f64; 2]>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MotionBlur { #[serde(default = "d_samples")] pub samples: u32, #[serde(default = "d_shutter")] pub shutter_angle: f64 }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Particles3D {
    #[serde(default)] pub emitter: Emitter3, #[serde(default)] pub physics: Physics, #[serde(default)] pub sprite: Sprite,
    #[serde(default = "d_blend")] pub blend: String, #[serde(default)] pub motion_blur: Option<MotionBlur>,
}
impl Default for Emitter3 { fn default() -> Self { serde_json::from_str("{}").unwrap() } }
impl Default for Sprite { fn default() -> Self { serde_json::from_str("{}").unwrap() } }
impl Default for Particles3D { fn default() -> Self { serde_json::from_str("{}").unwrap() } }

impl Particles3D {
    /// Canonical order for keyed lists (keys by time, opacity points by life).
    pub fn normalize(&mut self) { self.emitter.position_keys.sort_by(|a, b| a.t.total_cmp(&b.t)); self.sprite.opacity_curve.sort_by(|a, b| a[0].total_cmp(&b[0])); }
    /// Every refusal the tools give; `self_id` is the clip being edited (a layer sprite cannot be its own clip).
    pub fn validate(&self, p: &Project, self_id: Option<&str>) -> Result<(), String> {
        let fin3 = |v: &[f64; 3]| v.iter().all(|x| x.is_finite()); let em = &self.emitter;
        if !EMITTER_KINDS.contains(&em.kind.as_str()) { return Err(format!("unknown emitter kind '{}'. Valid: {}", em.kind, EMITTER_KINDS.join(", "))); }
        if !em.rate.is_finite() || em.rate <= 0.0 || em.rate > 1e6 { return Err(format!("emitter.rate must be in (0, 1e6] particles/s, got {}", em.rate)); }
        if !em.lifetime.is_finite() || em.lifetime <= 0.0 || em.lifetime > 1e4 { return Err(format!("emitter.lifetime must be in (0, 1e4] s, got {}", em.lifetime)); }
        if em.rate * em.lifetime > MAX_LIVE { return Err(format!("rate x lifetime = {} live particles exceeds the cap {MAX_LIVE}", em.rate * em.lifetime)); }
        if !em.speed.is_finite() || !(0.0..=1.0).contains(&em.speed_var) || !(0.0..=180.0).contains(&em.spread_deg) { return Err("emitter.speed must be finite, speed_var 0..1, spread_deg 0..180".into()); }
        if !fin3(&em.position) || !fin3(&em.size) || !fin3(&em.direction) || em.size.iter().any(|v| *v < 0.0) || !em.radius.is_finite() || em.radius < 0.0 { return Err("emitter position/size/direction must be finite; size and radius >= 0".into()); }
        if em.position_keys.iter().any(|k| !k.t.is_finite() || !fin3(&k.position)) { return Err("emitter.position_keys need finite t and position".into()); }
        if em.kind == "light" && !p.lights.contains_key(&em.light_id) { return Err(format!("emitter.light_id '{}' is not a project light (set_light first); lights: {}", em.light_id, p.lights.keys().cloned().collect::<Vec<_>>().join(", "))); }
        if !fin3(&self.physics.gravity) { return Err("physics.gravity must be finite".into()); }
        if !self.physics.drag.is_finite() || self.physics.drag < 0.0 { return Err(format!("physics.drag must be >= 0 (linear drag, 1/s), got {}", self.physics.drag)); }
        let sp = &self.sprite;
        if !SPRITE_KINDS.contains(&sp.kind.as_str()) { return Err(format!("unknown sprite kind '{}'. Valid: {}", sp.kind, SPRITE_KINDS.join(", "))); }
        if !TEXTURE_TIMES.contains(&sp.texture_time.as_str()) { return Err(format!("unknown sprite texture_time '{}'. Valid: {}", sp.texture_time, TEXTURE_TIMES.join(", "))); }
        let okz = |v: f64| v.is_finite() && v > 0.0 && v <= 1e6;
        if !okz(sp.size) || sp.size_end.map_or(false, |v| !okz(v)) { return Err("sprite.size and size_end must be in (0, 1e6] world units".into()); }
        if !sp.hardness.is_finite() || sp.hardness <= 0.0 || sp.hardness > 1e4 { return Err("sprite.hardness must be in (0, 1e4]".into()); }
        if !fin3(&sp.color) || sp.color_end.map_or(false, |c| !fin3(&c)) || sp.opacity_curve.iter().any(|q| !q[1].is_finite() || !(0.0..=1.0).contains(&q[0])) { return Err("sprite colors must be finite; opacity_curve points are [life 0..1, opacity]".into()); }
        if sp.kind == "layer" {
            if sp.clip_id.is_empty() { return Err("sprite.clip_id is required for a layer sprite".into()); }
            if Some(sp.clip_id.as_str()) == self_id { return Err("sprite.clip_id cannot be the particle layer itself".into()); }
            if p.find_clip(&sp.clip_id).is_none() { return Err(format!("unknown sprite clip_id '{}' (no such top-level clip)", sp.clip_id)); }
        }
        if !BLENDS.contains(&self.blend.as_str()) { return Err(format!("unknown blend '{}'. Valid: {}", self.blend, BLENDS.join(", "))); }
        if let Some(m) = &self.motion_blur { if m.samples < 1 || m.samples > 64 || !(0.0..=720.0).contains(&m.shutter_angle) { return Err("motion_blur.samples must be 1..64 and shutter_angle 0..720 degrees".into()); } }
        Ok(())
    }
}

fn hash01(seed: u64, i: u64, k: u64) -> f64 { let mut x = seed ^ i.wrapping_mul(0x9E3779B97F4A7C15) ^ k.wrapping_mul(0xBF58476D1CE4E5B9); x = (x ^ (x >> 30)).wrapping_mul(0xBF58476D1CE4E5B9); x = (x ^ (x >> 27)).wrapping_mul(0x94D049BB133111EB); x ^= x >> 31; (x >> 11) as f64 / (1u64 << 53) as f64 }
fn lerp(a: f64, b: f64, u: f64) -> f64 { a + (b - a) * u }

/// Closed-form position at age `tau` for start position x0, velocity v0, gravity g and linear drag k (1/s):
/// k > 0: x = x0 + (g/k) tau + (v0 - g/k)(1 - e^(-k tau))/k, evaluated as x0 + v0 A + g (tau - A)/k with A = (1 - e^(-k tau))/k (expm1; series when k tau < 1e-4);
/// k = 0: x = x0 + v0 tau + g tau^2 / 2.
pub fn position_at(x0: [f64; 3], v0: [f64; 3], g: [f64; 3], k: f64, tau: f64) -> [f64; 3] {
    if k <= 0.0 { return [0, 1, 2].map(|j| x0[j] + v0[j] * tau + 0.5 * g[j] * tau * tau); }
    let kt = k * tau;
    let (a, b) = if kt.abs() < 1e-4 { (tau * (1.0 - kt / 2.0 + kt * kt / 6.0 - kt * kt * kt / 24.0), tau * tau * (0.5 - kt / 6.0 + kt * kt / 24.0)) } else { let a = -(-kt).exp_m1() / k; (a, (tau - a) / k) };
    [0, 1, 2].map(|j| x0[j] + v0[j] * a + g[j] * b)
}
/// Closed-form velocity: v = g/k + (v0 - g/k) e^(-k tau) (k > 0), v0 + g tau (k = 0).
pub fn velocity_at(v0: [f64; 3], g: [f64; 3], k: f64, tau: f64) -> [f64; 3] {
    if k <= 0.0 { return [0, 1, 2].map(|j| v0[j] + g[j] * tau); }
    let e = (-k * tau).exp(); let a = -(-k * tau).exp_m1() / k; [0, 1, 2].map(|j| v0[j] * e + g[j] * a)
}

/// One live particle at time t: index i, birth = start + i/rate (timeline s), spawn position/velocity, closed-form position, age = t - birth, life = age/lifetime.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Live { pub index: u64, pub birth: f64, pub x0: [f64; 3], pub v0: [f64; 3], pub position: [f64; 3], pub age: f64, pub life: f64 }

/// Where the emitter centre is at a timeline time: keyed position (clip-local), plus the followed light. Built once per frame, so
/// sampling it per particle allocates nothing.
pub struct EmitPath<'a> { base: [f64; 3], keys: &'a [PosKey], start: f64, light: Option<(Vec<(f64, [f64; 3])>, [f64; 3])>, w: f64, h: f64 }
impl<'a> EmitPath<'a> {
    pub fn new(e: &'a Particles3D, lights: &BTreeMap<String, crate::lighting::LightDef>, w: usize, h: usize, start: f64) -> EmitPath<'a> {
        // light keys resolved like lighting::eval_light (sorted, missing pos = previous, smoothstep between keys), pre-sorted here once
        let light = if e.emitter.kind == "light" { lights.get(&e.emitter.light_id).map(|l| { let mut ks: Vec<&crate::lighting::LightKey> = l.keys.iter().collect(); ks.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap_or(std::cmp::Ordering::Equal));
            let mut pa = l.pos; let v: Vec<(f64, [f64; 3])> = ks.iter().map(|k| { let kp = k.pos.unwrap_or(pa); pa = kp; (k.t, kp) }).collect(); (v, l.pos) }) } else { None };
        EmitPath { base: e.emitter.position, keys: &e.emitter.position_keys, start, light, w: w as f64, h: h as f64 }
    }
    pub fn at(&self, tb: f64) -> [f64; 3] {
        let lt = tb - self.start; let k = self.keys;
        let mut c = if k.is_empty() { self.base } else if lt <= k[0].t { k[0].position } else if lt >= k[k.len() - 1].t { k[k.len() - 1].position } else {
            let i = k.iter().position(|q| q.t > lt).unwrap_or(k.len() - 1).max(1); let (a, b) = (&k[i - 1], &k[i]); let u = (lt - a.t) / (b.t - a.t).max(1e-12); [0, 1, 2].map(|j| lerp(a.position[j], b.position[j], u)) };
        if let Some((ks, p0)) = &self.light {
            let mut q = *p0; let mut prev: Option<(f64, [f64; 3])> = None; let mut done = false;
            for &(kt, kp) in ks { if tb <= kt { q = match prev { Some((pt, pp)) if kt > pt => { let u = ((tb - pt) / (kt - pt)).clamp(0.0, 1.0); let u = u * u * (3.0 - 2.0 * u); [0, 1, 2].map(|j| pp[j] + (kp[j] - pp[j]) * u) } _ => kp }; done = true; break; } prev = Some((kt, kp)); }
            if !done { if let Some((_, pp)) = prev { q = pp; } }
            c = [c[0] + (q[0] - 0.5) * self.w, c[1] + (q[1] - 0.5) * self.h, c[2] - q[2] * self.h];
        }
        c
    }
}

fn spawn(e: &Particles3D, i: u64, c: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let em = &e.emitter; let r = |k: u64| hash01(em.seed, i, k); let tau = std::f64::consts::TAU;
    let off = match em.kind.as_str() {
        "box" => [(r(3) - 0.5) * em.size[0], (r(4) - 0.5) * em.size[1], (r(5) - 0.5) * em.size[2]],
        "sphere" => { let (z, ph, rr) = (2.0 * r(3) - 1.0, tau * r(4), em.radius * r(5).cbrt()); let q = (1.0 - z * z).max(0.0).sqrt(); [rr * q * ph.cos(), rr * q * ph.sin(), rr * z] }
        _ => [0.0; 3] };
    let x0 = [c[0] + off[0], c[1] + off[1], c[2] + off[2]];
    let dl = (em.direction[0].powi(2) + em.direction[1].powi(2) + em.direction[2].powi(2)).sqrt(); let d = if dl > 1e-12 { em.direction.map(|v| v / dl) } else { [0.0, -1.0, 0.0] };
    // uniform direction in the spherical cap of half-angle spread_deg around d
    let ct = 1.0 - r(1) * (1.0 - em.spread_deg.clamp(0.0, 180.0).to_radians().cos()); let st = (1.0 - ct * ct).max(0.0).sqrt(); let ph = tau * r(2);
    let a = if d[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }; let b1 = { let x = [d[1] * a[2] - d[2] * a[1], d[2] * a[0] - d[0] * a[2], d[0] * a[1] - d[1] * a[0]]; let l = (x[0] * x[0] + x[1] * x[1] + x[2] * x[2]).sqrt(); x.map(|v| v / l) };
    let b2 = [d[1] * b1[2] - d[2] * b1[1], d[2] * b1[0] - d[0] * b1[2], d[0] * b1[1] - d[1] * b1[0]];
    let spd = em.speed * (1.0 + em.speed_var * (2.0 * r(6) - 1.0)); let (pc, ps) = (ph.cos() * st, ph.sin() * st);
    (x0, [0, 1, 2].map(|j| spd * (d[j] * ct + b1[j] * pc + b2[j] * ps)))
}

/// Visit every particle alive at timeline time `t` (index order). `center(birth)` is the emitter centre at a birth time (see EmitPath).
pub fn for_each_live(e: &Particles3D, start: f64, t: f64, center: &dyn Fn(f64) -> [f64; 3], mut visit: impl FnMut(&Live)) {
    let em = &e.emitter; let (rate, life) = (em.rate, em.lifetime); if !(rate > 0.0) || !(life > 0.0) || !t.is_finite() { return; }
    let lt = t - start; if lt < 0.0 { return; }
    let hi = ((lt * rate).floor() + 1.0).min(9e15) as u64; let lo = (((lt - life) * rate).floor() - 1.0).max(0.0) as u64; let lo = lo.max(hi.saturating_sub(MAX_LIVE as u64 + 4));
    for i in lo..=hi { let birth = start + i as f64 / rate; let age = t - birth; if age < 0.0 || age >= life { continue; }
        let (x0, v0) = spawn(e, i, center(birth)); let position = position_at(x0, v0, e.physics.gravity, e.physics.drag, age);
        visit(&Live { index: i, birth, x0, v0, position, age, life: age / life }); }
}
/// The live particles of a Particles3D clip (`clip_id`, top-level) at timeline time t, with the emitter path the renderer uses.
pub fn live_particles(p: &Project, clip_id: &str, t: f64) -> Result<Vec<Live>, String> {
    let c = p.find_clip(clip_id).ok_or_else(|| format!("no such clip {clip_id}"))?; let crate::model::Source::Particles3D(e) = &c.source else { return Err(format!("clip {clip_id} is not a particles3d layer")) };
    Ok(live(e, c.start, t, p))
}
pub fn live(e: &Particles3D, start: f64, t: f64, p: &Project) -> Vec<Live> {
    let path = EmitPath::new(e, &p.lights, p.width as usize, p.height as usize, start); let mut v = Vec::with_capacity((e.emitter.rate * e.emitter.lifetime).min(MAX_LIVE) as usize + 2);
    for_each_live(e, start, t, &|tb| path.at(tb), |l| v.push(*l)); v
}

/// The project camera at timeline time t as an explicit camera_projection::Camera (the camera_project_points convention):
/// eye = cam_pos - [0,0,f], f = (h/2)/tan(fov/2), sensor_height = h, focal_length = f (so pixel focal = f), near = 1 (CamView's cut).
pub fn camera_at(p: &Project, t: f64) -> crate::camera_projection::Camera {
    let c = crate::three_d::Camera::from_props(&p.camera, t); let h = p.height.max(1) as f64; let f = (h / 2.0) / (c.fov.to_radians() / 2.0).tan();
    crate::camera_projection::Camera { eye: [c.pos[0], c.pos[1], c.pos[2] - f], pan: c.pan, tilt: c.tilt, roll: c.roll, sensor_height: h, focal_length: f, near: 1.0 }
}
/// Screen position (x, y, view depth) of a world point through the project camera at time t; None behind the camera (depth < 1).
pub fn project_point(p: &Project, t: f64, world: [f64; 3]) -> Option<[f64; 3]> { camera_at(p, t).project(world, p.width.max(1), p.height.max(1)).ok().flatten() }

fn opacity_at(c: &[[f64; 2]], u: f64) -> f64 {
    if c.is_empty() { return 1.0; } if u <= c[0][0] { return c[0][1]; } if u >= c[c.len() - 1][0] { return c[c.len() - 1][1]; }
    for w in c.windows(2) { if u <= w[1][0] { return lerp(w[0][1], w[1][1], (u - w[0][0]) / (w[1][0] - w[0][0]).max(1e-12)); } } c[c.len() - 1][1]
}
/// Radial gaussian kernel, 1 at the centre and 0 at d = 1 (same shape as particle_flow::Emitter::kernel).
fn kernel(d: f64, hard: f64) -> f64 { let e = (-hard).exp(); (((-hard * d * d).exp() - e) / (1.0 - e)).max(0.0) }

/// A sprite texture: the clip frame (premultiplied, as render_clip_alone composites it) cropped to its alpha box; fw x fh = full frame.
struct Tex { fw: f64, fh: f64, x0: usize, y0: usize, cw: usize, ch: usize, d: Vec<f32> }
fn load_tex(p: &Project, id: &str, tt: f64, cache: &crate::render::Cache) -> Option<Tex> {
    let fr = crate::render::render_clip_alone(p, id, tt, cache)?; let (w, h) = (fr.w, fr.h); let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0usize, 0usize);
    for y in 0..h { for x in 0..w { if fr.d[(y * w + x) * 4 + 3] > 0.0 { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x + 1); y1 = y1.max(y + 1); } } }
    if x1 <= x0 || y1 <= y0 { return None; }
    let (cw, ch) = (x1 - x0, y1 - y0); let mut d = Vec::with_capacity(cw * ch * 4); for y in y0..y1 { d.extend_from_slice(&fr.d[(y * w + x0) * 4..(y * w + x1) * 4]); }
    Some(Tex { fw: w as f64, fh: h as f64, x0, y0, cw, ch, d })
}
#[inline] fn put(buf: &mut [f32], i: usize, s: [f64; 4], add: bool) {
    if add { for j in 0..4 { buf[i + j] += s[j] as f32; } } else { let k = 1.0 - s[3] as f32; for j in 0..4 { buf[i + j] = s[j] as f32 + buf[i + j] * k; } }
}
fn draw_disc(buf: &mut [f32], w: usize, h: usize, cx: f64, cy: f64, r: f64, hard: f64, col: [f64; 3], op: f64, add: bool) {
    let r = r.clamp(0.5, 4096.0); if cx + r < 0.0 || cy + r < 0.0 || cx - r > w as f64 || cy - r > h as f64 { return; }
    let (x0, x1) = ((cx - r).floor().max(0.0) as usize, ((cx + r).ceil().max(0.0) as usize).min(w)); let (y0, y1) = ((cy - r).floor().max(0.0) as usize, ((cy + r).ceil().max(0.0) as usize).min(h));
    for y in y0..y1 { for x in x0..x1 { let d = (x as f64 + 0.5 - cx).hypot(y as f64 + 0.5 - cy) / r; if d >= 1.0 { continue; } let k = kernel(d, hard) * op; if k <= 0.0 { continue; }
        put(buf, (y * w + x) * 4, [col[0] * k, col[1] * k, col[2] * k, k], add); } }
}
/// Layer sprite: the frame is scaled by `k` about its centre (fw/2, fh/2), which lands on (cx, cy); bilinear in premultiplied space at
/// pixel centres (pixel X covers [X, X+1), sampled at X + 0.5), zero outside the frame. At k = 1 and centre on the frame centre: identity.
fn draw_tex(buf: &mut [f32], w: usize, h: usize, t: &Tex, cx: f64, cy: f64, k: f64, col: [f64; 3], op: f64, add: bool) {
    if !(k > 1e-6) || !k.is_finite() { return; }
    let (sx0, sx1) = (cx + (t.x0 as f64 - t.fw / 2.0) * k - 1.0, cx + ((t.x0 + t.cw) as f64 - t.fw / 2.0) * k + 1.0); let (sy0, sy1) = (cy + (t.y0 as f64 - t.fh / 2.0) * k - 1.0, cy + ((t.y0 + t.ch) as f64 - t.fh / 2.0) * k + 1.0);
    if sx1 < 0.0 || sy1 < 0.0 || sx0 > w as f64 || sy0 > h as f64 { return; }
    let (x0, x1) = (sx0.floor().max(0.0) as usize, (sx1.ceil().max(0.0) as usize).min(w)); let (y0, y1) = (sy0.floor().max(0.0) as usize, (sy1.ceil().max(0.0) as usize).min(h));
    let (cw, ch) = (t.cw as i64, t.ch as i64); let tap = |xi: i64, yi: i64| -> usize { if xi < 0 || yi < 0 || xi >= cw || yi >= ch { usize::MAX } else { (yi as usize * t.cw + xi as usize) * 4 } };
    for y in y0..y1 { let fy = (y as f64 + 0.5 - cy) / k + t.fh / 2.0 - 0.5 - t.y0 as f64; let yi = fy.floor(); let ay = fy - yi; let yi = yi as i64;
        for x in x0..x1 { let fx = (x as f64 + 0.5 - cx) / k + t.fw / 2.0 - 0.5 - t.x0 as f64; let xi = fx.floor(); let ax = fx - xi; let xi = xi as i64;
            let mut s = [0.0f64; 4];
            for (dx, dy, wt) in [(0, 0, (1.0 - ax) * (1.0 - ay)), (1, 0, ax * (1.0 - ay)), (0, 1, (1.0 - ax) * ay), (1, 1, ax * ay)] { if wt == 0.0 { continue; } let o = tap(xi + dx, yi + dy); if o == usize::MAX { continue; } for j in 0..4 { s[j] += t.d[o + j] as f64 * wt; } }
            if s[3] <= 0.0 { continue; } put(buf, (y * w + x) * 4, [s[0] * col[0] * op, s[1] * col[1] * op, s[2] * col[2] * op, s[3] * op], add); } }
}

struct Splat { x: f64, y: f64, z: f64, life: f64, birth: f64, index: u64 }
/// Premultiplied accumulation of one instant `t` into `buf` (camera at t). `t_frame` is the frame time ("current" sprite texture).
fn accumulate(p: &Project, c: &Clip, e: &Particles3D, t: f64, t_frame: f64, path: &EmitPath, texs: &mut Vec<(i64, Option<Tex>)>, splats: &mut Vec<Splat>, buf: &mut [f32], w: usize, h: usize, cache: &crate::render::Cache) {
    let cam = camera_at(p, t); let (wu, hu) = (w.max(1) as u32, h.max(1) as u32); splats.clear();
    for_each_live(e, c.start, t, &|tb| path.at(tb), |l| { if let Ok(Some(q)) = cam.project(l.position, wu, hu) { splats.push(Splat { x: q[0], y: q[1], z: q[2], life: l.life, birth: l.birth, index: l.index }); } });
    let add = e.blend == "additive"; if !add { splats.sort_by(|a, b| b.z.total_cmp(&a.z).then(a.index.cmp(&b.index))); } // back to front; additive is order-free
    let f_px = h as f64 * (cam.focal_length / cam.sensor_height); let sp = &e.sprite; let layer = sp.kind == "layer" && !sp.clip_id.is_empty();
    let bw = (1.0 / p.fps.max(1e-3)).max(e.emitter.lifetime / MAX_TEXTURE_BINS); let by_birth = sp.texture_time == "birth";
    for (n, s) in splats.iter().enumerate() {
        if n & 1023 == 1023 { crate::guard::check(); }
        let op = opacity_at(&sp.opacity_curve, s.life); if !(op > 0.0) { continue; }
        let col = match sp.color_end { Some(ce) => [0, 1, 2].map(|j| lerp(sp.color[j], ce[j], s.life)), None => sp.color }; let size = sp.size_end.map_or(sp.size, |se| lerp(sp.size, se, s.life)); let scale = f_px / s.z;
        if layer {
            let (key, tt) = if by_birth { let b = (s.birth / bw).floor(); (b as i64, b * bw) } else { (i64::MIN, t_frame) };
            let ti = match texs.iter().position(|(k, _)| *k == key) { Some(i) => i, None => { texs.push((key, load_tex(p, &sp.clip_id, tt, cache))); texs.len() - 1 } };
            if let Some(tx) = &texs[ti].1 { draw_tex(buf, w, h, tx, s.x, s.y, size / tx.fw * scale, col, op, add); }
        } else { draw_disc(buf, w, h, s.x, s.y, 0.5 * size * scale, sp.hardness, col, op, add); }
    }
}

/// Render a Particles3D clip at clip-local time `lt` into `f` (straight alpha, like every source frame). Motion blur: `samples` instants
/// at the midpoints of equal sub-intervals of a shutter of shutter_angle/360 frame centred on t (camera re-posed per instant), averaged.
pub fn render_layer(p: &Project, c: &Clip, e: &Particles3D, lt: f64, f: &mut Frame, cache: &crate::render::Cache) {
    let (w, h) = (f.w, f.h); let t = c.start + lt; let path = EmitPath::new(e, &p.lights, w, h, c.start);
    let mut texs: Vec<(i64, Option<Tex>)> = Vec::new(); let mut splats: Vec<Splat> = Vec::with_capacity((e.emitter.rate * e.emitter.lifetime).clamp(0.0, MAX_LIVE) as usize + 2);
    let (n, sh) = match &e.motion_blur { Some(m) if m.samples > 1 && m.shutter_angle > 0.0 => (m.samples.min(64) as usize, m.shutter_angle.min(720.0) / 360.0 / p.fps.max(1.0)), _ => (1, 0.0) };
    let mut acc = vec![0f32; w * h * 4]; let mut tmp = if n > 1 { vec![0f32; w * h * 4] } else { Vec::new() };
    for k in 0..n {
        crate::guard::check();
        if n == 1 { accumulate(p, c, e, t, t, &path, &mut texs, &mut splats, &mut acc, w, h, cache); break; }
        tmp.iter_mut().for_each(|v| *v = 0.0); let off = sh * ((k as f64 + 0.5) / n as f64 - 0.5);
        accumulate(p, c, e, t + off, t, &path, &mut texs, &mut splats, &mut tmp, w, h, cache); for (a, b) in acc.iter_mut().zip(tmp.iter()) { *a += *b; }
    }
    if n > 1 { let inv = 1.0 / n as f32; for v in acc.iter_mut() { *v *= inv; } }
    for i in 0..w * h { let a = acc[i * 4 + 3]; if !(a > 0.0) { continue; } let al = a.min(1.0); for j in 0..3 { f.d[i * 4 + j] = acc[i * 4 + j].min(4.0) / al; } f.d[i * 4 + 3] = al; }
}
