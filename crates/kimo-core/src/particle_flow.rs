//! Field-driven particle flow: deterministic emitter with birth schedule and
//! lifetime recycling through a preallocated ring buffer (the pool is allocated
//! once; advancing and rendering perform no per-frame heap allocation),
//! smoothstep fade-in/out, cubic-eased size, multi-stop gradient color,
//! advection coupling to particle_advect fields, and a camera-v2 depth-splat
//! renderer with a distance-attenuated footprint, radial gaussian kernel of
//! configurable hardness, additive or premultiplied-alpha blending, a tiled
//! inner loop, and z-buffer occlusion against mesh geometry depth.
//! Original code. Correctness-first CPU path: no motion blur, trails, or fluid
//! coupling claim.
use serde::{Deserialize, Serialize};
use crate::camera_projection::Camera;
use crate::effects::Frame;
use crate::particle_advect::Advect;

pub const MAX_PARTICLES: u32 = 100_000;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Blend {
    Additive,
    PremultipliedAlpha,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorStop {
    /// Normalized lifespan offset in [0,1]; stops must be ascending, first 0, last 1.
    pub offset: f64,
    pub rgb: [f64; 3],
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Emitter {
    pub seed: u64,
    /// Ring capacity: maximum simultaneously simulated particles (total births are unbounded).
    pub count: u32,
    pub origin: [f64; 3],
    pub spread: [f64; 3],
    pub base_velocity: [f64; 3],
    pub velocity_jitter: f64,
    pub birth_rate: f64,
    pub lifetime: [f64; 2],
    pub color_stops: Vec<ColorStop>,
    pub alpha_start: f64,
    pub alpha_end: f64,
    /// Lifespan fractions for the smoothstep fade-in / fade-out envelope, each in (0, 0.5].
    pub fade_in: f64,
    pub fade_out: f64,
    /// World-space splat diameter at birth/death; screen footprint attenuates with distance.
    pub size_start: f64,
    pub size_end: f64,
    pub blend: Blend,
    /// Gaussian kernel hardness > 0; larger = tighter core.
    pub hardness: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Flow {
    pub emitter: Emitter,
    pub field: Advect,
    pub gravity: [f64; 3],
    pub drag: f64,
    /// 1/s relaxation of particle velocity toward the field velocity; 0 disables.
    pub coupling: f64,
    /// Field sampled as a direct force/acceleration (field * field_accel added to
    /// acceleration); 0 disables. Integrated into velocity every step, so the
    /// render-time aspect stretch always reads the post-field velocity. Paired with
    /// an automatic viscosity term (drag + field_accel * 0.5) so high-vorticity
    /// pockets reach a bounded terminal velocity instead of running away.
    #[serde(default)]
    pub field_accel: f64,
    pub dt: f64,
}

/// Fixed per-particle position history depth (ring, no allocation).
pub const HISTORY: usize = 6;

/// One ring-pool particle slot (public for read-only inspection; mutate via Simulation).
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub position: [f64; 3],
    pub velocity: [f64; 3],
    pub birth: f64,
    pub lifetime: f64,
    pub alive: bool,
    /// Past positions, newest at (head + HISTORY - 1) % HISTORY; len valid entries.
    pub history: [[f64; 3]; HISTORY],
    pub history_head: u8,
    pub history_len: u8,
}
impl Slot {
    fn push_history(&mut self) {
        self.history[self.history_head as usize] = self.position;
        self.history_head = (self.history_head + 1) % HISTORY as u8;
        self.history_len = (self.history_len + 1).min(HISTORY as u8);
    }
    /// Oldest-to-newest past positions.
    pub fn history_ordered(&self) -> impl Iterator<Item = [f64; 3]> + '_ {
        let n = self.history_len as usize;
        let head = self.history_head as usize;
        (0..n).map(move |k| self.history[(head + HISTORY - n + k) % HISTORY])
    }
}

fn smoothstep(e0: f64, e1: f64, x: f64) -> f64 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}
fn ease_in_out_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    if t < 0.5 { 4.0 * t * t * t } else { 1.0 - (-2.0 * t + 2.0).powi(3) / 2.0 }
}
fn jitter(seed: u64, index: u64, lane: u8) -> f64 {
    let mut h = 0xcbf29ce484222325u64;
    for b in seed.to_le_bytes().into_iter().chain(index.to_le_bytes()).chain([lane]) {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^= h.rotate_left(21);
    h ^= h >> 31;
    ((h >> 11) as f64 + 0.5) / 9007199254740992.0
}

impl Emitter {
    pub fn validate(&self) -> Result<(), String> {
        if self.count == 0 || self.count > MAX_PARTICLES {
            return Err("emitter count 1..100000".into());
        }
        if !self.origin.iter().chain(&self.spread).chain(&self.base_velocity).all(|v| v.is_finite() && v.abs() <= 1e9) {
            return Err("finite bounded origin/spread/base_velocity required".into());
        }
        if !self.velocity_jitter.is_finite() || !(0.0..=1e6).contains(&self.velocity_jitter) {
            return Err("velocity_jitter 0..1e6".into());
        }
        if !self.birth_rate.is_finite() || self.birth_rate <= 0.0 || self.birth_rate > 1e6 {
            return Err("birth_rate (0, 1e6]".into());
        }
        if !self.lifetime.iter().all(|v| v.is_finite()) || self.lifetime[0] <= 0.0 || self.lifetime[1] < self.lifetime[0] || self.lifetime[1] > 1e4 {
            return Err("lifetime 0<min<=max<=1e4".into());
        }
        if self.color_stops.len() < 2 || self.color_stops.len() > 8 {
            return Err("color_stops 2..=8".into());
        }
        let first = self.color_stops.first().unwrap();
        let last = self.color_stops.last().unwrap();
        if first.offset != 0.0 || last.offset != 1.0 || !self.color_stops.windows(2).all(|w| w[1].offset > w[0].offset) {
            return Err("color stop offsets strictly ascending, first 0, last 1".into());
        }
        if !self.color_stops.iter().all(|s| s.rgb.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v))) {
            return Err("color stop rgb 0..1".into());
        }
        if !(0.0..=1.0).contains(&self.alpha_start) || !(0.0..=1.0).contains(&self.alpha_end) {
            return Err("alpha 0..1".into());
        }
        if !(0.0..=0.5).contains(&self.fade_in) || !(0.0..=0.5).contains(&self.fade_out) || self.fade_in <= 0.0 || self.fade_out <= 0.0 {
            return Err("fade_in/fade_out (0, 0.5]".into());
        }
        if !self.size_start.is_finite() || !self.size_end.is_finite() || self.size_start <= 0.0 || self.size_end <= 0.0 || self.size_start.max(self.size_end) > 1e6 {
            return Err("sizes (0, 1e6] world units".into());
        }
        if !self.hardness.is_finite() || self.hardness <= 0.0 || self.hardness > 1e4 {
            return Err("hardness (0, 1e4]".into());
        }
        Ok(())
    }
    pub fn birth_time(&self, spawn_index: u64) -> f64 {
        spawn_index as f64 / self.birth_rate
    }
    fn lifetime_of(&self, spawn_index: u64) -> f64 {
        self.lifetime[0] + (self.lifetime[1] - self.lifetime[0]) * jitter(self.seed, spawn_index, 10)
    }
    fn spawn_state(&self, spawn_index: u64) -> ([f64; 3], [f64; 3]) {
        let p = std::array::from_fn(|j| self.origin[j] + (jitter(self.seed, spawn_index, j as u8) - 0.5) * 2.0 * self.spread[j]);
        let v = std::array::from_fn(|j| self.base_velocity[j] + (jitter(self.seed, spawn_index, 3 + j as u8) - 0.5) * 2.0 * self.velocity_jitter);
        (p, v)
    }
    /// Multi-stop gradient color at normalized age t in [0,1].
    pub fn color_at(&self, t: f64) -> [f64; 3] {
        let t = t.clamp(0.0, 1.0);
        if t <= 0.0 {
            return self.color_stops[0].rgb;
        }
        if t >= 1.0 {
            return self.color_stops[self.color_stops.len() - 1].rgb;
        }
        for w in self.color_stops.windows(2) {
            if t <= w[1].offset {
                let u = (t - w[0].offset) / (w[1].offset - w[0].offset);
                return std::array::from_fn(|j| w[0].rgb[j] + (w[1].rgb[j] - w[0].rgb[j]) * u);
            }
        }
        self.color_stops.last().unwrap().rgb
    }
    /// Alpha at normalized age: linear start/end ramp under a smoothstep fade-in/out envelope.
    pub fn alpha_at(&self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        let env = smoothstep(0.0, self.fade_in, t) * (1.0 - smoothstep(1.0 - self.fade_out, 1.0, t));
        (self.alpha_start + (self.alpha_end - self.alpha_start) * t) * env
    }
    /// World-space size at normalized age: cubic-eased bump envelope - spawns at
    /// size_start, expands to size_end by GROW, holds, then shrinks back to
    /// size_start after SHRINK so particles are small again before death.
    pub fn size_at(&self, t: f64) -> f64 {
        const GROW: f64 = 0.3;
        const SHRINK: f64 = 0.7;
        let t = t.clamp(0.0, 1.0);
        let env = if t < GROW {
            ease_in_out_cubic(t / GROW)
        } else if t <= SHRINK {
            1.0
        } else {
            1.0 - ease_in_out_cubic((t - SHRINK) / (1.0 - SHRINK))
        };
        self.size_start + (self.size_end - self.size_start) * env
    }
    /// Radial gaussian kernel weight, 1 at center and 0 at d=1; hardness tightens the core.
    pub fn kernel(&self, d: f64) -> f64 {
        if d >= 1.0 {
            return 0.0;
        }
        let e = (-self.hardness).exp();
        (((-self.hardness * d * d).exp() - e) / (1.0 - e)).max(0.0)
    }
}

/// Fixed-step simulation over a preallocated ring pool. `advance` allocates nothing.
pub struct Simulation {
    flow: Flow,
    pool: Vec<Slot>,
    cursor: usize,
    time: f64,
    spawned: u64,
    total_born: u64,
    total_died: u64,
    /// Reusable render scratch (projected splats, radii, flags, tile buckets), cleared per render.
    scratch: Vec<(f64, f64, f64, f64)>,
    radii: Vec<f64>,
    flags: Vec<u8>,
    buckets: Vec<Vec<usize>>,
    heads: Vec<Head>,
    trails: Vec<(f64, f64, f64, f64, f64, [f64; 3])>,
}

impl Simulation {
    pub fn new(flow: Flow) -> Result<Self, String> {
        flow.emitter.validate()?;
        if !flow.gravity.iter().all(|v| v.is_finite() && v.abs() <= 1e9) {
            return Err("finite bounded gravity required".into());
        }
        if !flow.drag.is_finite() || flow.drag < 0.0 || !flow.coupling.is_finite() || flow.coupling < 0.0 || flow.coupling > 1e6 {
            return Err("drag/coupling nonnegative finite (coupling<=1e6)".into());
        }
        if !flow.field_accel.is_finite() || !(0.0..=1e6).contains(&flow.field_accel) {
            return Err("field_accel 0..1e6".into());
        }
        if !flow.dt.is_finite() || flow.dt <= 0.0 || flow.dt > 1.0 {
            return Err("dt (0, 1]".into());
        }
        crate::guard::reserve(flow.emitter.count as usize * 224);
        let slot = Slot { position: [0.0; 3], velocity: [0.0; 3], birth: 0.0, lifetime: 1.0, alive: false, history: [[0.0; 3]; HISTORY], history_head: 0, history_len: 0 };
        Ok(Self { pool: vec![slot; flow.emitter.count as usize], cursor: 0, time: 0.0, spawned: 0, total_born: 0, total_died: 0, scratch: Vec::new(), radii: Vec::new(), flags: Vec::new(), buckets: Vec::new(), heads: Vec::new(), trails: Vec::new(), flow })
    }
    pub fn time(&self) -> f64 {
        self.time
    }
    pub fn pool(&self) -> &[Slot] {
        &self.pool
    }
    pub fn alive_count(&self) -> usize {
        self.pool.iter().filter(|s| s.alive).count()
    }
    /// Advance to `to_time` with exact fixed dt (last partial step). No heap allocation.
    pub fn advance(&mut self, to_time: f64) -> Result<usize, String> {
        if !to_time.is_finite() || to_time < self.time || to_time > 1e4 {
            return Err("advance requires current<=to_time<=1e4".into());
        }
        let mut steps = 0usize;
        while self.time < to_time {
            crate::guard::check();
            let h = flow_dt(self.flow.dt, to_time - self.time);
            let target = self.time + h;
            // Recycle: kill expired, then spawn all births due by target into ring slots.
            for s in self.pool.iter_mut() {
                if s.alive && target - s.birth >= s.lifetime {
                    s.alive = false;
                    self.total_died += 1;
                }
            }
            while self.flow.emitter.birth_time(self.spawned) < target {
                let idx = self.spawned;
                let birth = self.flow.emitter.birth_time(idx);
                let (p, v) = self.flow.emitter.spawn_state(idx);
                let slot = &mut self.pool[self.cursor];
                if slot.alive {
                    self.total_died += 1; // ring overwrite of the oldest live slot
                }
                *slot = Slot { position: p, velocity: v, birth, lifetime: self.flow.emitter.lifetime_of(idx), alive: true, history: [[0.0; 3]; HISTORY], history_head: 0, history_len: 0 };
                self.cursor = (self.cursor + 1) % self.pool.len();
                self.spawned += 1;
                self.total_born += 1;
            }
            for s in self.pool.iter_mut() {
                if !s.alive {
                    continue;
                }
                let field = self.flow.field.velocity(s.position)?;
                s.push_history();
                let viscosity = self.flow.drag + self.flow.field_accel * 0.5;
                for j in 0..3 {
                    let a = self.flow.gravity[j] + self.flow.field_accel * field[j] + self.flow.coupling * (field[j] - s.velocity[j]) - viscosity * s.velocity[j];
                    s.velocity[j] += a * h;
                    s.position[j] += s.velocity[j] * h;
                }
                if s.position.iter().chain(s.velocity.iter()).any(|v| !v.is_finite() || v.abs() > 1e12) {
                    return Err("particle state overflow".into());
                }
            }
            self.time = target;
            steps += 1;
        }
        Ok(steps)
    }
}

fn flow_dt(dt: f64, remaining: f64) -> f64 {
    if remaining < dt { remaining } else { dt }
}

#[derive(Debug, Serialize)]
pub struct RenderOut {
    pub frame_pixels: usize,
    pub active: usize,
    pub drawn: usize,
    pub occluded_by_mesh: usize,
    pub behind_camera: usize,
}

/// Depth-splat render at the simulation's current time. Optional mesh supplies the
/// background frame and the occluding z-buffer from the camera-v2 perspective path.
pub fn render(sim: &mut Simulation, camera: &Camera, w: usize, h: usize, mesh: Option<&crate::threed::primitives::Geometry>, depth_range: [f64; 2]) -> Result<(Frame, RenderOut), String> {
    camera.validate()?;
    if w == 0 || h == 0 || w > 1920 || h > 1080 {
        return Err("target 1..1920x1080".into());
    }
    let (mut frame, depth) = if let Some(g) = mesh {
        let o = crate::threed::perspective::render(g, camera, w, h, crate::threed::perspective::Shading::Normal, depth_range)?;
        (o.frame, o.depth)
    } else {
        (Frame::try_new(w, h)?, vec![f64::INFINITY; w * h])
    };
    let mut out = RenderOut { frame_pixels: w * h, active: 0, drawn: 0, occluded_by_mesh: 0, behind_camera: 0 };
    let e = &sim.flow.emitter;
    let focal_px = h as f64 * (camera.focal_length / camera.sensor_height);
    // Project into reusable scratch: (px, py, z, age_t, index).
    sim.scratch.clear();
    for s in &sim.pool {
        if !s.alive {
            continue;
        }
        out.active += 1;
        match camera.project(s.position, w as u32, h as u32)? {
            Some(o) => sim.scratch.push((o[0], o[1], o[2], (sim.time - s.birth) / s.lifetime)),
            None => out.behind_camera += 1,
        }
    }
    // Back-to-front for premultiplied alpha-over; additive is order-independent.
    sim.scratch.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.0.total_cmp(&b.0)).then_with(|| a.1.total_cmp(&b.1)));
    // Tiled inner loop: bucket splat indices into 64px tiles by bounding box.
    const TILE: usize = 64;
    let tw = w.div_ceil(TILE);
    let th = h.div_ceil(TILE);
    if sim.buckets.len() != tw * th {
        sim.buckets.resize_with(tw * th, Vec::new);
    }
    for b in sim.buckets.iter_mut() {
        b.clear();
    }
    sim.radii.clear();
    for (i, &(px, py, z, t)) in sim.scratch.iter().enumerate() {
        let world = e.size_at(t);
        let r = (0.5 * world * focal_px / z).clamp(0.5, 64.0);
        sim.radii.push(r);
        let (x0, x1) = (((px - r).floor().max(0.0) as usize).min(w - 1), ((px + r).ceil() as usize).min(w - 1));
        let (y0, y1) = (((py - r).floor().max(0.0) as usize).min(h - 1), ((py + r).ceil() as usize).min(h - 1));
        for ty in y0 / TILE..=y1 / TILE {
            for tx in x0 / TILE..=x1 / TILE {
                sim.buckets[ty * tw + tx].push(i);
            }
        }
    }
    sim.flags.clear();
    sim.flags.resize(sim.scratch.len() * 2, 0); // [drawn..., occluded...]
    for ty in 0..th {
        for tx in 0..tw {
            crate::guard::check();
            let ys = ty * TILE..((ty + 1) * TILE).min(h);
            let xs = tx * TILE..((tx + 1) * TILE).min(w);
            for &i in &sim.buckets[ty * tw + tx] {
                let (px, py, z, t) = sim.scratch[i];
                let r = sim.radii[i];
                let color = e.color_at(t);
                let alpha = e.alpha_at(t);
                for y in ys.clone() {
                    for x in xs.clone() {
                        let d = ((x as f64 + 0.5 - px).hypot(y as f64 + 0.5 - py)) / r;
                        if d >= 1.0 {
                            continue;
                        }
                        let id = y * w + x;
                        // Relative depth bias: near-ties resolve to the particle (deterministic, no z-fighting).
                        if z >= depth[id] * (1.0 + 1e-6) + 1e-9 {
                            sim.flags[sim.scratch.len() + i] = 1;
                            continue;
                        }
                        let k = e.kernel(d) * alpha;
                        if k <= 0.0 {
                            continue;
                        }
                        match e.blend {
                            Blend::Additive => {
                                for j in 0..3 {
                                    frame.d[id * 4 + j] = (frame.d[id * 4 + j] + (color[j] * k) as f32).min(1.0);
                                }
                            }
                            Blend::PremultipliedAlpha => {
                                for j in 0..3 {
                                    frame.d[id * 4 + j] = frame.d[id * 4 + j] * (1.0 - k as f32) + (color[j] * k) as f32;
                                }
                            }
                        }
                        frame.d[id * 4 + 3] = 1.0;
                        sim.flags[i] = 1;
                    }
                }
            }
        }
    }
    let n = sim.scratch.len();
    out.drawn = sim.flags[..n].iter().filter(|v| **v == 1).count();
    out.occluded_by_mesh = (0..n).filter(|&i| sim.flags[n + i] == 1 && sim.flags[i] == 0).count();
    Ok((frame, out))
}

/// Reflection with restitution + tangential friction against a mesh obstacle (flat SAH
/// raycast, true interpolated vertex normals - not screen-depth gradients). Normal
/// velocity reverses scaled by `restitution`; tangential velocity keeps `friction`,
/// so particles slide along the surface and lose energy (no ping-pong jitter).
/// Interpretation note: deflection is resolved in 3D against the mesh surface itself,
/// which covers the no-normal-buffer case by construction.
#[cfg(feature = "flat-sah")]
pub fn advance_deflect(sim: &mut Simulation, to_time: f64, index: &crate::threed::flat_sah::Index, stream: &crate::threed::vertex_stream::Stream, restitution: f64, friction: f64) -> Result<usize, String> {
    if !restitution.is_finite() || !(0.0..=1.0).contains(&restitution) || !friction.is_finite() || !(0.0..=1.0).contains(&friction) {
        return Err("restitution/friction 0..1".into());
    }
    // Step in dt increments like advance(), reflecting after each sub-step.
    let mut steps = 0;
    while sim.time < to_time {
        let h = flow_dt(sim.flow.dt, to_time - sim.time);
        let target = sim.time + h;
        for s in sim.pool.iter_mut() {
            if !s.alive {
                continue;
            }
            let speed = s.velocity[0].hypot(s.velocity[1]).hypot(s.velocity[2]);
            if speed <= 1e-12 {
                continue;
            }
            let dir = [s.velocity[0] / speed, s.velocity[1] / speed, s.velocity[2] / speed];
            let ray = crate::threed::spatial::Ray { origin: s.position, direction: dir, t_min: 1e-9, t_max: speed * h };
            if let Some(hit) = index.raycast(&ray, false)? {
                let tri = stream.indices[hit.triangle].map(|i| stream.vertices[i as usize].normal);
                let mut n = [0.0; 3];
                for k in 0..3 {
                    for j in 0..3 {
                        n[j] += hit.barycentric[k] * tri[k][j] as f64;
                    }
                }
                let mut n = crate::threed::vertex_stream::unit(n)?;
                let vn = s.velocity[0] * n[0] + s.velocity[1] * n[1] + s.velocity[2] * n[2];
                if vn > 0.0 {
                    n = [-n[0], -n[1], -n[2]]; // face the normal against the motion
                }
                let a = vn.abs();
                let travel = (hit.t - 1e-6).max(0.0);
                for j in 0..3 {
                    s.position[j] += dir[j] * travel;
                    // vn' = -restitution * vn (reversed normal part) + friction * vt (sliding tangent).
                    s.velocity[j] = (s.velocity[j] + (1.0 + restitution) * a * n[j]) * 1.0 - (1.0 - friction) * (s.velocity[j] + a * n[j]);
                }
                if s.velocity.iter().any(|v| !v.is_finite()) {
                    return Err("reflected velocity overflow".into());
                }
            }
        }
        sim.advance(target)?;
        steps += 1;
    }
    Ok(steps)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirLight {
    /// World-space direction TOWARD the light; normalized internally.
    pub direction: [f64; 3],
    pub color: [f64; 3],
    pub intensity: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PointLight {
    pub position: [f64; 3],
    pub color: [f64; 3],
    pub intensity: f64,
    /// Smooth cutoff radius: attenuation reaches 0 at this distance.
    pub radius: f64,
}

/// Splat lighting rig: splats shade as sphere impostors (per-pixel hemispherical
/// normal from the gaussian footprint) under half-lambert wrap for a soft
/// volumetric look; point lights use inverse-square falloff with smooth radius
/// attenuation. Computed in view space.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lights {
    pub directional: Vec<DirLight>,
    pub point: Vec<PointLight>,
    pub ambient: [f64; 3],
}

impl Lights {
    pub fn validate(&self) -> Result<(), String> {
        if self.directional.len() > 8 || self.point.len() > 8 {
            return Err("max 8 lights per kind".into());
        }
        for l in &self.directional {
            if !l.direction.iter().all(|v| v.is_finite()) || l.direction.iter().all(|v| *v == 0.0) {
                return Err("finite nonzero light direction required".into());
            }
            if !l.color.iter().all(|v| v.is_finite() && (0.0..=1e3).contains(v)) || !l.intensity.is_finite() || !(0.0..=1e3).contains(&l.intensity) {
                return Err("light color/intensity 0..1e3".into());
            }
        }
        for l in &self.point {
            if !l.position.iter().all(|v| v.is_finite() && v.abs() <= 1e9) {
                return Err("finite point light position required".into());
            }
            if !l.color.iter().all(|v| v.is_finite() && (0.0..=1e3).contains(v)) || !l.intensity.is_finite() || !(0.0..=1e3).contains(&l.intensity) {
                return Err("light color/intensity 0..1e3".into());
            }
            if !l.radius.is_finite() || l.radius <= 0.0 || l.radius > 1e6 {
                return Err("point radius (0, 1e6]".into());
            }
        }
        if !self.ambient.iter().all(|v| v.is_finite() && (0.0..=1e3).contains(v)) {
            return Err("ambient 0..1e3".into());
        }
        Ok(())
    }
    /// Half-lambert wrap: back-facing surface still gets soft fill.
    fn half_lambert(ndl: f64) -> f64 {
        let w = (ndl * 0.5 + 0.5).clamp(0.0, 1.0);
        w * w
    }
    /// Irradiance at a view-space point with impostor normal n (unit, +z toward camera).
    /// `vdirectional`/`vpoint` are already view-space transformed.
    fn shade(&self, vdir: &[[f64; 3]], vpoint: &[([f64; 3], usize)], p: [f64; 3], n: [f64; 3]) -> [f64; 3] {
        let mut e = self.ambient;
        for (k, l) in self.directional.iter().enumerate() {
            let k2 = Self::half_lambert(n[0] * vdir[k][0] + n[1] * vdir[k][1] + n[2] * vdir[k][2]);
            for j in 0..3 {
                e[j] += l.color[j] * l.intensity * k2;
            }
        }
        for (vp, k) in vpoint {
            let l = &self.point[*k];
            let d = [vp[0] - p[0], vp[1] - p[1], vp[2] - p[2]];
            let dist2 = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).max(1e-6);
            let dist = dist2.sqrt();
            // Inverse-square, clamped, with smooth (1 - (d/r)^4)^2 radius attenuation.
            let att = (1.0 - (dist / l.radius).powi(4)).clamp(0.0, 1.0).powi(2) / dist2.max(0.01);
            if att <= 0.0 {
                continue;
            }
            let inv = 1.0 / dist;
            let k2 = Self::half_lambert((d[0] * n[0] + d[1] * n[1] + d[2] * n[2]) * inv);
            for j in 0..3 {
                e[j] += l.color[j] * l.intensity * att * k2;
            }
        }
        e
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tonemap {
    /// Per-channel exponential Reinhard: 1 - exp(-x); original, smooth, mono.
    Reinhard,
    /// Clamp only (linear HDR written straight; may clip above 1).
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Style {
    /// Motion streak exposure in seconds; 0 disables oriented splats.
    pub shutter: f64,
    /// Screen-space ribbon trails from the fixed history ring for particles at or above this world speed; <0 disables.
    pub trail_speed: f64,
    /// HDR exposure multiplier before the tone map; must be > 0.
    pub exposure: f64,
    pub tonemap: Tonemap,
    /// Reuse of the existing multi-scale bloom effect: None skips, Some(amount) runs it with fixed soft-knee defaults.
    pub bloom: Option<f64>,
}

impl Default for Style {
    fn default() -> Self {
        Self { shutter: 0.0, trail_speed: -1.0, exposure: 1.0, tonemap: Tonemap::None, bloom: None }
    }
}

#[derive(Clone, Copy)]
struct Head {
    px: f64,
    py: f64,
    z: f64,
    t: f64,
    vx: f64,
    vy: f64,
    view: [f64; 3],
}

/// HDR accumulating render: streaked splats + history trails in linear f32 (no clamp),
/// optional existing-pipeline bloom, exposure, tone map. Everything else (ring pool,
/// tiled buckets, mesh z-buffer with relative bias) identical to `render`.
pub fn render_styled(sim: &mut Simulation, camera: &Camera, w: usize, h: usize, mesh: Option<&crate::threed::primitives::Geometry>, depth_range: [f64; 2], style: &Style, lights: Option<&Lights>) -> Result<(Frame, RenderOut), String> {
    camera.validate()?;
    if let Some(l) = lights {
        l.validate()?;
    }
    if w == 0 || h == 0 || w > 1920 || h > 1080 {
        return Err("target 1..1920x1080".into());
    }
    if !style.shutter.is_finite() || !(0.0..=1.0).contains(&style.shutter) {
        return Err("shutter 0..1 s".into());
    }
    if !style.exposure.is_finite() || style.exposure <= 0.0 || style.exposure > 1e4 {
        return Err("exposure (0, 1e4]".into());
    }
    if let Some(b) = style.bloom {
        if !b.is_finite() || !(0.0..=1.0).contains(&b) {
            return Err("bloom amount 0..1 (conserve)".into());
        }
    }
    let (mut frame, depth) = if let Some(g) = mesh {
        let o = crate::threed::perspective::render(g, camera, w, h, crate::threed::perspective::Shading::Normal, depth_range)?;
        (o.frame, o.depth)
    } else {
        (Frame::try_new(w, h)?, vec![f64::INFINITY; w * h])
    };
    let mut out = RenderOut { frame_pixels: w * h, active: 0, drawn: 0, occluded_by_mesh: 0, behind_camera: 0 };
    let e = &sim.flow.emitter;
    let focal_px = h as f64 * (camera.focal_length / camera.sensor_height);
    // Lights into view space: view(x) = R(x - eye), so R*d = view(eye + d).
    let vdir: Vec<[f64; 3]> = lights.map(|l| l.directional.iter().map(|d| camera.view([camera.eye[0] + d.direction[0], camera.eye[1] + d.direction[1], camera.eye[2] + d.direction[2]])).collect::<Result<Vec<_>, _>>()).transpose()?.unwrap_or_default();
    let vpoint: Vec<([f64; 3], usize)> = lights.map(|l| l.point.iter().enumerate().map(|(k, pl)| camera.view(pl.position).map(|v| (v, k))).collect::<Result<Vec<_>, _>>()).transpose()?.unwrap_or_default();
    sim.heads.clear();
    sim.trails.clear();
    for s in &sim.pool {
        if !s.alive {
            continue;
        }
        out.active += 1;
        let t = (sim.time - s.birth) / s.lifetime;
        let Some(o) = camera.project(s.position, w as u32, h as u32)? else {
            out.behind_camera += 1;
            continue;
        };
        // Screen-space velocity from the world velocity over the shutter window.
        let q = camera.project([s.position[0] + s.velocity[0] * style.shutter, s.position[1] + s.velocity[1] * style.shutter, s.position[2] + s.velocity[2] * style.shutter], w as u32, h as u32)?;
        let (vx, vy) = match q {
            Some(q) => ((q[0] - o[0]) / style.shutter.max(1e-9), (q[1] - o[1]) / style.shutter.max(1e-9)),
            None => (0.0, 0.0),
        };
        let speed = s.velocity[0].hypot(s.velocity[1]).hypot(s.velocity[2]);
        sim.heads.push(Head { px: o[0], py: o[1], z: o[2], t, vx, vy, view: camera.view(s.position)? });
        if style.trail_speed >= 0.0 && speed >= style.trail_speed && s.history_len >= 2 {
            let n = s.history_len as usize;
            let mut last: Option<[f64; 2]> = None;
            for (k, hp) in s.history_ordered().enumerate() {
                if let Some(ho) = camera.project(hp, w as u32, h as u32)? {
                    // Skip segments shorter than half a pixel (no sub-pixel ribbon stubs).
                    if last.map(|l| (ho[0] - l[0]).hypot(ho[1] - l[1]) < 0.5).unwrap_or(false) {
                        continue;
                    }
                    last = Some([ho[0], ho[1]]);
                    let frac = (k + 1) as f64 / (n + 1) as f64;
                    sim.trails.push((ho[0], ho[1], ho[2], t, frac * frac, camera.view(hp)?)); // quadratic tail decay to zero
                }
            }
        }
    }
    // Trails first (older behind), then heads; both back-to-front by view depth.
    sim.trails.sort_by(|a, b| b.2.partial_cmp(&a.2).unwrap_or(std::cmp::Ordering::Equal));
    sim.heads.sort_by(|a, b| b.z.partial_cmp(&a.z).unwrap_or(std::cmp::Ordering::Equal).then_with(|| a.px.total_cmp(&b.px)).then_with(|| a.py.total_cmp(&b.py)));
    sim.flags.clear();
    sim.flags.resize(sim.heads.len() * 2, 0);
    let splat = |frame: &mut Frame, depth: &[f64], px: f64, py: f64, z: f64, rx: f64, ry: f64, cos: f64, sin: f64, color: [f64; 3], alpha: f64, pview: [f64; 3], drawn: &mut bool, occluded: &mut bool| {
        let rmax = rx.max(ry);
        let (x0, x1) = ((px - rmax).floor().max(0.0) as usize, ((px + rmax).ceil() as usize).min(w - 1));
        let (y0, y1) = ((py - rmax).floor().max(0.0) as usize, ((py + rmax).ceil() as usize).min(h - 1));
        for y in y0..=y1 {
            for x in x0..=x1 {
                let dx = x as f64 + 0.5 - px;
                let dy = y as f64 + 0.5 - py;
                // Rotated gaussian: u along the streak axis, v across it.
                let u = dx * cos + dy * sin;
                let v = -dx * sin + dy * cos;
                let d = ((u / rx).hypot(v / ry)).sqrt();
                if d >= 1.0 {
                    continue;
                }
                let id = y * w + x;
                if z >= depth[id] * (1.0 + 1e-6) + 1e-9 {
                    *occluded = true;
                    continue;
                }
                let k = e.kernel(d) * alpha;
                if k <= 0.0 {
                    continue;
                }
                // Sphere-impostor lighting: hemispherical normal from the rotated footprint.
                let lit = if let Some(l) = lights {
                    let nx = u / rx;
                    let ny = v / ry;
                    let nz = -(1.0 - d * d).max(0.0).sqrt(); // view -z faces the camera
                    let irr = l.shade(&vdir, &vpoint, pview, [nx, ny, nz]);
                    [color[0] * irr[0], color[1] * irr[1], color[2] * irr[2]]
                } else {
                    color
                };
                let color = lit;
                match e.blend {
                    Blend::Additive => {
                        for j in 0..3 {
                            frame.d[id * 4 + j] += (color[j] * k) as f32; // HDR: no clamp
                        }
                    }
                    Blend::PremultipliedAlpha => {
                        for j in 0..3 {
                            frame.d[id * 4 + j] = frame.d[id * 4 + j] * (1.0 - k as f32) + (color[j] * k) as f32;
                        }
                    }
                }
                frame.d[id * 4 + 3] = 1.0;
                *drawn = true;
            }
        }
    };
    for &(px, py, z, t, frac, pview) in &sim.trails {
        crate::guard::check();
        let r = (0.5 * e.size_at(t) * focal_px / z).clamp(0.5, 64.0);
        let alpha = e.alpha_at(t) * frac; // alpha falloff along the trail
        let mut d = false;
        let mut o = false;
        splat(&mut frame, &depth, px, py, z, r, r, 1.0, 0.0, e.color_at(t), alpha, pview, &mut d, &mut o);
    }
    for i in 0..sim.heads.len() {
        let hd = sim.heads[i];
        crate::guard::check();
        let r = (0.5 * e.size_at(hd.t) * focal_px / hd.z).clamp(0.5, 64.0);
        // Oriented streak: major axis along screen velocity, length = |v_px|*shutter; gaussian kept on both axes.
        let streak = (hd.vx.hypot(hd.vy)) * style.shutter;
        let (rx, ry, cos, sin) = if streak > 1e-9 {
            let rl = r + streak / 2.0;
            (rl.min(r * 12.0).min(192.0), r, hd.vx / (hd.vx.hypot(hd.vy)), hd.vy / (hd.vx.hypot(hd.vy)))
        } else {
            (r, r, 1.0, 0.0)
        };
        let mut d = false;
        let mut o = false;
        splat(&mut frame, &depth, hd.px, hd.py, hd.z, rx, ry, cos, sin, e.color_at(hd.t), e.alpha_at(hd.t), hd.view, &mut d, &mut o);
        sim.flags[i] = d as u8;
        sim.flags[sim.heads.len() + i] = o as u8;
    }
    let nh = sim.heads.len();
    out.drawn = sim.flags[..nh].iter().filter(|v| **v == 1).count();
    out.occluded_by_mesh = (0..nh).filter(|&i| sim.flags[nh + i] == 1 && sim.flags[i] == 0).count();
    // NaN/inf pre-clamp on the HDR accumulation before any downstream pass.
    for v in frame.d.iter_mut() {
        if !v.is_finite() {
            *v = 0.0;
        } else {
            *v = v.clamp(-65504.0, 65504.0);
        }
    }
    // Bloom through the existing effect pipeline (multi-scale box-blur gaussian stack, HDR-aware soft knee:
    // only clusters above luma 0.8 with the quadratic knee feed the blur, so dim low-alpha particles do not bloom).
    if let Some(amount) = style.bloom {
        let mut params: crate::effects::Params = std::collections::BTreeMap::new();
        params.insert("amount".into(), crate::keyframe::Prop::new(amount));
        params.insert("threshold".into(), crate::keyframe::Prop::new(0.8));
        params.insert("knee".into(), crate::keyframe::Prop::new(0.2));
        params.insert("radius".into(), crate::keyframe::Prop::new(8.0));
        params.insert("levels".into(), crate::keyframe::Prop::new(5.0));
        params.insert("conserve".into(), crate::keyframe::Prop::new(0.0)); // additive: bloom only adds light, never darkens cores
        crate::effects::apply("bloom", &params, &std::collections::BTreeMap::new(), &mut frame, 0.0, 0)?;
    }
    // Exposure + tone map; Reinhard path finishes with the sRGB transfer and a
    // triangular dither so the 8-bit encode has no banding (Tonemap::None stays
    // clamped linear for raw comparison, matching the project's no-display-transform convention).
    let mut dith = 0x9E3779B97F4A7C15u64;
    for i in 0..w * h {
        for j in 0..3 {
            let x = frame.d[i * 4 + j] as f64 * style.exposure;
            frame.d[i * 4 + j] = match style.tonemap {
                Tonemap::Reinhard => {
                    let v = (1.0 - (-x).exp()).clamp(0.0, 1.0);
                    let srgb = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 };
                    dith ^= dith << 13;
                    dith ^= dith >> 7;
                    dith ^= dith << 17;
                    let u = (dith >> 11) as f64 / 9007199254740992.0;
                    (srgb + (u - 0.5) / 255.0) as f32
                }
                Tonemap::None => x as f32,
            }
            .clamp(0.0, 1.0);
        }
    }
    Ok((frame, out))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::threed::primitives::{generate, Primitive};

    fn flow() -> Flow {
        Flow {
            emitter: Emitter {
                seed: 11,
                count: 64,
                origin: [0.0; 3],
                spread: [0.2, 0.1, 0.2],
                base_velocity: [0.0, 1.5, 0.0],
                velocity_jitter: 0.3,
                birth_rate: 40.0,
                lifetime: [1.0, 2.0],
                color_stops: vec![
                    ColorStop { offset: 0.0, rgb: [1.0, 0.8, 0.2] },
                    ColorStop { offset: 0.5, rgb: [1.0, 0.3, 0.05] },
                    ColorStop { offset: 1.0, rgb: [0.2, 0.1, 0.6] },
                ],
                alpha_start: 0.9,
                alpha_end: 0.6,
                fade_in: 0.1,
                fade_out: 0.15,
                size_start: 0.02,
                size_end: 0.08,
                blend: Blend::Additive,
                hardness: 4.0,
            },
            field: Advect::Curl3 { noise: "perlin".into(), seed: 3, frequency: 0.8, amplitude: 1.0, octaves: 2 },
            gravity: [0.0, -0.5, 0.0],
            drag: 0.1,
            coupling: 2.0,
            field_accel: 0.0,
            dt: 0.01,
        }
    }
    #[test]
    fn ring_recycling_and_birth_schedule() {
        let mut sim = Simulation::new(flow()).unwrap();
        sim.advance(1.0).unwrap();
        assert_eq!(sim.spawned, 40); // 40/sec; birth strictly before target
        assert!(sim.alive_count() <= 64);
        // Long run: births exceed capacity, ring recycles, alive never exceeds capacity.
        sim.advance(1.0 + 64.0 / 40.0 + 3.0).unwrap();
        assert!(sim.spawned > 64);
        assert_eq!(sim.total_born, sim.spawned);
        assert!(sim.alive_count() <= 64);
        assert!(sim.total_died > 0);
        // Conservation: every spawned particle is either alive or counted dead.
        let mut sim2 = Simulation::new(Flow { emitter: Emitter { birth_rate: 10.0, ..flow().emitter }, ..flow() }).unwrap();
        sim2.advance(9.0).unwrap();
        assert_eq!(sim2.total_died + sim2.alive_count() as u64, sim2.spawned);
    }
    #[test]
    fn lifecycle_curves_endpoints() {
        let e = flow().emitter;
        // Smoothstep envelope: zero alpha at birth and death.
        assert_eq!(e.alpha_at(0.0), 0.0);
        assert_eq!(e.alpha_at(1.0), 0.0);
        let peak = e.alpha_at(0.5);
        assert!(peak > 0.0 && peak <= e.alpha_start.max(e.alpha_end));
        // Cubic bump: small at birth and death, peak mid-life.
        assert_eq!(e.size_at(0.0), e.size_start);
        assert_eq!(e.size_at(1.0), e.size_start, "shrinks back before death");
        assert!((e.size_at(0.5) - e.size_end).abs() < 1e-12, "peak at mid-life");
        assert!(e.size_at(0.9) < e.size_end && e.size_at(0.9) > e.size_start);
        // Multi-stop gradient: exact stops, midpoint blends.
        assert_eq!(e.color_at(0.0), [1.0, 0.8, 0.2]);
        assert_eq!(e.color_at(1.0), [0.2, 0.1, 0.6]);
        let c = e.color_at(0.25);
        assert!((c[0] - 1.0).abs() < 1e-12 && (c[1] - 0.55).abs() < 1e-12, "halfway between stop0/stop1");
        // Kernel: 1 at center, 0 at rim, monotonic.
        assert!((e.kernel(0.0) - 1.0).abs() < 1e-12);
        assert_eq!(e.kernel(1.0), 0.0);
        assert!(e.kernel(0.2) > e.kernel(0.6));
    }
    #[test]
    fn deterministic_repeat_and_no_alloc_advance() {
        let mut a = Simulation::new(flow()).unwrap();
        let mut b = Simulation::new(flow()).unwrap();
        a.advance(0.75).unwrap();
        b.advance(0.75).unwrap();
        for (x, y) in a.pool().iter().zip(b.pool()) {
            assert_eq!(x.position, y.position);
            assert_eq!(x.velocity, y.velocity);
            assert_eq!(x.alive, y.alive);
        }
        let cap = a.pool().len();
        let ptr = a.pool().as_ptr();
        a.advance(5.0).unwrap();
        assert_eq!(a.pool().len(), cap);
        assert_eq!(a.pool().as_ptr(), ptr, "pool never reallocates across advance");
    }
    #[test]
    fn depth_occlusion_against_mesh() {
        let mut f = flow();
        f.emitter.count = 8;
        f.emitter.origin = [0.0, 0.0, 2.0];
        f.emitter.spread = [0.1; 3];
        let camera = Camera { eye: [0.0, 0.0, -4.0], pan: 0.0, tilt: 0.0, roll: 0.0, sensor_height: 24.0, focal_length: 35.0, near: 0.1 };
        let cube = generate(&Primitive::Cube { size: [6.0, 6.0, 0.5], subdivisions: 1 }).unwrap();
        let mut sim = Simulation::new(f.clone()).unwrap();
        sim.advance(0.05).unwrap();
        let (_, out) = render(&mut sim, &camera, 64, 36, Some(&cube), [0.5, 10.0]).unwrap();
        assert!(out.active > 0);
        assert_eq!(out.drawn, 0, "all particles behind the cube");
        assert!(out.occluded_by_mesh > 0);
        let (_, out2) = render(&mut sim, &camera, 64, 36, None, [0.5, 10.0]).unwrap();
        assert!(out2.drawn > 0);
    }
    #[test]
    fn validation_rejections() {
        let mut f = flow();
        f.emitter.count = 0;
        assert!(Simulation::new(f).is_err());
        let mut f = flow();
        f.dt = 0.0;
        assert!(Simulation::new(f).is_err());
        let mut f = flow();
        f.emitter.color_stops = vec![ColorStop { offset: 0.2, rgb: [0.0; 3] }, ColorStop { offset: 1.0, rgb: [1.0; 3] }];
        assert!(Simulation::new(f).is_err());
        let mut f = flow();
        f.emitter.fade_in = 0.0;
        assert!(Simulation::new(f).is_err());
        let mut sim = Simulation::new(flow()).unwrap();
        assert!(sim.advance(-1.0).is_err());
    }

    fn single_particle_sim(vx: f64, shutter: f64) -> (Simulation, Camera) {
        let f = Flow {
            emitter: Emitter {
                seed: 3, count: 1, origin: [0.0; 3], spread: [0.0; 3], base_velocity: [vx, 0.0, 0.0], velocity_jitter: 0.0,
                birth_rate: 1.0, lifetime: [10.0, 10.0],
                color_stops: vec![ColorStop { offset: 0.0, rgb: [1.0, 1.0, 1.0] }, ColorStop { offset: 1.0, rgb: [1.0, 1.0, 1.0] }],
                alpha_start: 1.0, alpha_end: 1.0, fade_in: 0.05, fade_out: 0.05,
                size_start: 0.05, size_end: 0.05, blend: Blend::Additive, hardness: 4.0,
            },
            field: Advect::Curl3 { noise: "perlin".into(), seed: 1, frequency: 1.0, amplitude: 0.0, octaves: 1 },
            gravity: [0.0; 3], drag: 0.0, coupling: 0.0, field_accel: 0.0, dt: 0.005,
        };
        let mut sim = Simulation::new(f).unwrap();
        sim.advance(0.5).unwrap();
        let camera = Camera { eye: [0.0, 0.0, -4.0], pan: 0.0, tilt: 0.0, roll: 0.0, sensor_height: 24.0, focal_length: 35.0, near: 0.1 };
        (sim, camera)
    }
    fn extent(f: &Frame) -> (usize, usize) {
        let (mut xs, mut ys) = (vec![], vec![]);
        for y in 0..f.h {
            for x in 0..f.w {
                if f.d[(y * f.w + x) * 4] > 0.01 {
                    xs.push(x);
                    ys.push(y);
                }
            }
        }
        (xs.iter().max().unwrap() - xs.iter().min().unwrap() + 1, ys.iter().max().unwrap() - ys.iter().min().unwrap() + 1)
    }
    #[test]
    fn oriented_streak_stretches_along_screen_velocity() {
        let (mut still, cam) = single_particle_sim(0.0, 0.2);
        let (f0, _) = render_styled(&mut still, &cam, 200, 120, None, [0.5, 20.0], &Style::default(), None).unwrap();
        let (w0, h0) = extent(&f0);
        assert!((w0 as i32 - h0 as i32).abs() <= 2, "zero velocity stays round: {w0}x{h0}");
        let (mut fast, cam) = single_particle_sim(3.0, 0.2);
        let style = Style { shutter: 0.2, ..Style::default() };
        let (f1, _) = render_styled(&mut fast, &cam, 200, 120, None, [0.5, 20.0], &style, None).unwrap();
        let (w1, h1) = extent(&f1);
        assert!(w1 > w0 + 10, "streak widens: {w0} -> {w1}");
        assert!((h1 as i32 - h0 as i32).abs() <= 3, "minor axis preserved: {h0} -> {h1}");
        // Gaussian falloff across the minor axis: center row profile strictly decreasing from center.
        let (mut cy, mut cx) = (0usize, 0usize);
        let mut best = 0f32;
        for y in 0..f1.h {
            for x in 0..f1.w {
                if f1.d[(y * f1.w + x) * 4] > best {
                    best = f1.d[(y * f1.w + x) * 4];
                    cy = y;
                    cx = x;
                }
            }
        }
        let col: Vec<f32> = (0..f1.h).map(|y| f1.d[(y * f1.w + cx) * 4]).collect();
        assert!(col[cy] >= col[cy + 2] && col[cy] >= col[cy - 2], "minor-axis falloff monotone");
    }
    #[test]
    fn history_ring_fixed_and_ordered() {
        let mut sim = Simulation::new(Flow { emitter: Emitter { birth_rate: 1.0, count: 1, base_velocity: [1.0, 0.0, 0.0], velocity_jitter: 0.0, ..flow().emitter }, coupling: 0.0, gravity: [0.0; 3], drag: 0.0, ..flow() }).unwrap();
        sim.advance(0.5).unwrap(); // 125 steps at dt 0.004
        let s = &sim.pool()[0];
        assert_eq!(s.history_len as usize, HISTORY, "ring saturates at HISTORY");
        let h: Vec<_> = s.history_ordered().collect();
        assert_eq!(h.len(), HISTORY);
        for w in h.windows(2) {
            assert!(w[1][0] > w[0][0], "oldest-to-newest along +x");
        }
        assert!((h[HISTORY - 1][0] - s.position[0]).abs() < 0.01 + 1e-9, "newest history one step behind head");
    }
    #[test]
    fn trails_and_hdr_bloom_tonemap() {
        let (mut sim, cam) = single_particle_sim(4.0, 0.0);
        let style = Style { shutter: 0.0, trail_speed: 1.0, exposure: 1.0, tonemap: Tonemap::Reinhard, bloom: Some(0.6) };
        let (f, _) = render_styled(&mut sim, &cam, 200, 120, None, [0.5, 20.0], &style, None).unwrap();
        assert!(f.d.iter().all(|v| v.is_finite() && (0.0..=1.0).contains(v)), "tonemapped output in gamut");
        // Trail leaves energy behind the head (head is leading at +x).
        let (mut cx, mut cy) = (0usize, 0usize);
        let mut best = 0f32;
        for y in 0..f.h {
            for x in 0..f.w {
                let v = f.d[(y * f.w + x) * 4];
                if v > best {
                    best = v;
                    cx = x;
                    cy = y;
                }
            }
        }
        let behind: f32 = (0..cx.saturating_sub(4)).map(|x| f.d[(cy * f.w + x) * 4]).sum();
        assert!(behind > 0.0, "trail deposits energy behind the head");
    }
    #[test]
    fn reinhard_endpoints_and_exposure() {
        let (mut sim, cam) = single_particle_sim(0.0, 0.0);
        let dim = Style { exposure: 0.5, tonemap: Tonemap::Reinhard, ..Style::default() };
        let bright = Style { exposure: 4.0, tonemap: Tonemap::Reinhard, ..Style::default() };
        let (a, _) = render_styled(&mut sim, &cam, 120, 80, None, [0.5, 20.0], &dim, None).unwrap();
        let (b, _) = render_styled(&mut sim, &cam, 120, 80, None, [0.5, 20.0], &bright, None).unwrap();
        let peak = |f: &Frame| f.d.iter().skip(0).step_by(4).fold(0f32, |m, v| m.max(*v));
        assert!(peak(&a) < peak(&b), "exposure scales before the tone map");
        assert!(peak(&b) <= 1.0 && peak(&a) > 0.0);
    }
    #[cfg(feature = "flat-sah")]
    #[test]
    fn obstacle_deflection_reflects_and_damps() {
        let g = generate(&Primitive::Sphere { radius: 1.0, longitude_slices: 24, latitude_slices: 12 }).unwrap();
        let stream = crate::threed::vertex_stream::Stream { vertices: g.vertices.clone(), indices: g.indices.clone() };
        let index = crate::threed::flat_sah::Index::build(&stream).unwrap();
        // Particle at (-3,0,0) flying +x into the unit sphere.
        let mut sim = Simulation::new(Flow { emitter: Emitter { count: 1, origin: [-3.0, 0.05, 0.0], spread: [0.0; 3], base_velocity: [4.0, 0.0, 0.0], velocity_jitter: 0.0, birth_rate: 1.0, lifetime: [10.0, 10.0], ..flow().emitter }, coupling: 0.0, gravity: [0.0; 3], drag: 0.0, dt: 0.005, ..flow() }).unwrap();
        advance_deflect(&mut sim, 1.0, &index, &stream, 0.5, 0.7).unwrap();
        let s = sim.pool()[0];
        assert!(s.velocity[0] < 0.0, "reflected: v={:?}", s.velocity);
        assert!(s.velocity[0].abs() <= 4.0 * 0.5 + 0.6, "damped below restitution bound: {}", s.velocity[0]);
        assert!(s.position[0] < -0.9, "stays outside the sphere: {}", s.position[0]);
    }
    #[test]
    fn lighting_half_lambert_and_point_falloff() {
        // Half-lambert wrap: back-facing still lit (soft volumetric), front stronger.
        let l = Lights {
            directional: vec![DirLight { direction: [0.0, 0.0, -1.0], color: [1.0, 1.0, 1.0], intensity: 1.0 }],
            point: vec![],
            ambient: [0.0, 0.0, 0.0],
        };
        l.validate().unwrap();
        // Camera at -z looking +z; light at -z behind camera -> front-facing toward camera.
        let cam = Camera { eye: [0.0, 0.0, -4.0], pan: 0.0, tilt: 0.0, roll: 0.0, sensor_height: 24.0, focal_length: 35.0, near: 0.1 };
        let vd = cam.view([0.0, 0.0, -5.0]).unwrap(); // R * (0,0,-1): light behind camera
        let front = Lights::half_lambert(vd[2] * -1.0); // camera-facing impostor normal (0,0,-1)
        let back = Lights::half_lambert(vd[2] * 1.0);
        let side = Lights::half_lambert(0.0);
        assert!(front > back, "front {front} > back {back}");
        assert!(side > back && side < front, "wrap keeps grazing fill: {side}");
        // Point light: inverse-square with clamp + smooth radius cutoff.
        let lp = Lights { directional: vec![], point: vec![PointLight { position: [1.0, 0.0, 0.0], color: [1.0, 1.0, 1.0], intensity: 1.0, radius: 3.0 }], ambient: [0.0; 3] };
        let vp = [(cam.view([1.0, 0.0, 0.0]).unwrap(), 0usize)];
        let n = [0.0, 0.0, 1.0];
        let near = lp.shade(&[], &vp, cam.view([0.5, 0.0, 0.0]).unwrap(), n)[0];
        let far = lp.shade(&[], &vp, cam.view([-1.0, 0.0, 0.0]).unwrap(), n)[0];
        assert!(near > far, "nearer point brighter: {near} > {far}");
        let outside = lp.shade(&[], &vp, cam.view([4.5, 0.0, 0.0]).unwrap(), n)[0];
        assert_eq!(outside, 0.0, "zero beyond the radius");
        // End-to-end: lit render differs from unlit; ambient floor keeps shadows nonzero.
        let (mut sim, cam2) = single_particle_sim(0.0, 0.0);
        let st = Style { exposure: 1.0, tonemap: Tonemap::Reinhard, ..Style::default() };
        let rig = Lights { directional: vec![], point: vec![], ambient: [0.4, 0.4, 0.4] };
        let key = Lights { directional: vec![DirLight { direction: [0.0, 0.0, -1.0], color: [1.0, 1.0, 1.0], intensity: 1.0 }], point: vec![], ambient: [0.4, 0.4, 0.4] };
        let (fu, _) = render_styled(&mut sim, &cam2, 120, 80, None, [0.5, 20.0], &st, None).unwrap();
        let (fl, _) = render_styled(&mut sim, &cam2, 120, 80, None, [0.5, 20.0], &st, Some(&rig)).unwrap();
        let (fk, _) = render_styled(&mut sim, &cam2, 120, 80, None, [0.5, 20.0], &st, Some(&key)).unwrap();
        let sum = |f: &Frame| f.d.iter().step_by(4).sum::<f32>();
        let (pu, pl, pk) = (sum(&fu), sum(&fl), sum(&fk));
        assert!(pl < pu, "ambient-only below unity dims: {pu} -> {pl}");
        assert!(pk > pl, "camera-facing key adds over ambient: {pl} -> {pk}");
    }
    #[test]
    fn field_accel_updates_velocity_before_stretch() {
        // field_accel > 0 changes velocity vs 0 for the same seed; the streak uses the post-field velocity.
        let mk = |fa: f64| Flow { field_accel: fa, ..flow() };
        let mut a = Simulation::new(mk(0.0)).unwrap();
        let mut b = Simulation::new(mk(1.5)).unwrap();
        a.advance(0.4).unwrap();
        b.advance(0.4).unwrap();
        let va: Vec<_> = a.pool().iter().filter(|s| s.alive).map(|s| s.velocity).collect();
        let vb: Vec<_> = b.pool().iter().filter(|s| s.alive).map(|s| s.velocity).collect();
        assert_eq!(va.len(), vb.len());
        let diverged = va.iter().zip(&vb).filter(|(x, y)| (0..3).any(|j| (x[j] - y[j]).abs() > 1e-9)).count();
        assert!(diverged as f64 > va.len() as f64 * 0.8, "field_accel perturbs most velocities: {diverged}/{}", va.len());
        // Zero is a true no-op (backward compatible).
        let mut c = Simulation::new(mk(0.0)).unwrap();
        c.advance(0.4).unwrap();
        for (x, y) in a.pool().iter().zip(c.pool()) {
            assert_eq!(x.velocity, y.velocity);
        }
        assert!(Simulation::new(mk(-1.0)).is_err());
        // Viscosity pairing: strong field force with zero explicit drag stays bounded.
        let mut r = Simulation::new(Flow { drag: 0.0, coupling: 0.0, field_accel: 4.0, ..flow() }).unwrap();
        r.advance(6.0).unwrap();
        let vmax = r.pool().iter().filter(|s| s.alive).map(|s| s.velocity[0].hypot(s.velocity[1]).hypot(s.velocity[2])).fold(0.0, f64::max);
        let fmax = 4.0 * 4.0 * 1.0 * 2.0; // conservative: field_accel * worst-case bound(4*A*f*octaves-ish)
        assert!(vmax.is_finite() && vmax < fmax, "bounded terminal velocity: {vmax} < {fmax}");
    }
    #[cfg(feature = "flat-sah")]
    #[test]
    #[ignore]
    fn pv4_frames() {
        let dir = std::env::var("KIMO_PV4_FRAMES").unwrap_or_else(|_| "/tmp/pv4-frames".into());
        std::fs::create_dir_all(&format!("{dir}/seq")).unwrap();
        let camera = Camera { eye: [0.0, 0.3, -3.4], pan: 0.0, tilt: -0.06, roll: 0.0, sensor_height: 24.0, focal_length: 35.0, near: 0.1 };
        let flow = || Flow {
            emitter: Emitter {
                seed: 9,
                count: 2500,
                origin: [0.0, -1.1, 0.0],
                spread: [0.3, 0.06, 0.3],
                base_velocity: [0.0, 1.6, 0.0],
                velocity_jitter: 0.5,
                birth_rate: 1200.0,
                lifetime: [1.8, 2.6],
                color_stops: vec![
                    ColorStop { offset: 0.0, rgb: [1.0, 0.95, 0.5] },
                    ColorStop { offset: 0.4, rgb: [1.0, 0.5, 0.12] },
                    ColorStop { offset: 0.75, rgb: [0.7, 0.2, 0.6] },
                    ColorStop { offset: 1.0, rgb: [0.15, 0.25, 0.9] },
                ],
                alpha_start: 0.9,
                alpha_end: 0.55,
                fade_in: 0.1,
                fade_out: 0.15,
                size_start: 0.008,
                size_end: 0.045,
                blend: Blend::Additive,
                hardness: 5.0,
            },
            field: Advect::Curl3 { noise: "perlin".into(), seed: 4, frequency: 0.7, amplitude: 1.6, octaves: 3 },
            gravity: [0.0, -0.1, 0.0],
            drag: 0.04,
            coupling: 3.5,
            field_accel: 0.4,
            dt: 0.004,
        };
        let sphere = generate(&Primitive::Sphere { radius: 0.9, longitude_slices: 48, latitude_slices: 24 }).unwrap();
        let stream = crate::threed::vertex_stream::Stream { vertices: sphere.vertices.clone(), indices: sphere.indices.clone() };
        let index = crate::threed::flat_sah::Index::build(&stream).unwrap();
        let styled = Style { shutter: 0.06, trail_speed: 0.8, exposure: 1.2, tonemap: Tonemap::Reinhard, bloom: Some(0.55) };
        let rig = Lights {
            directional: vec![DirLight { direction: [0.4, 0.8, -0.4], color: [1.0, 0.85, 0.6], intensity: 1.1 }],
            point: vec![PointLight { position: [0.0, -0.9, 0.6], color: [0.3, 0.5, 1.0], intensity: 2.0, radius: 3.0 }],
            ambient: [0.15, 0.15, 0.18],
        };
        let styled_nobloom = Style { bloom: None, ..styled };
        // Hero sheet: 2x2 960x540 at t=1.5 (a) raw (b) streaks+trails (c) +bloom (d) deflection styled.
        let mut sim = Simulation::new(flow()).unwrap();
        advance_deflect(&mut sim, 1.5, &index, &stream, 0.5, 0.7).unwrap();
        let mut sheet = Frame::try_new(1920, 1080).unwrap();
        let mut hero_stats = Vec::new();
        for (k, st) in [Style::default(), styled_nobloom.clone(), styled.clone()].iter().enumerate() {
            let lt = if k == 0 { None } else { Some(&rig) };
            let (f, out) = render_styled(&mut sim, &camera, 960, 540, Some(&sphere), [0.5, 20.0], st, lt).unwrap();
            crate::render::write_image(&f, &format!("{dir}/hero-{k}.png")).unwrap();
            hero_stats.push((k, out.active, out.drawn, out.occluded_by_mesh));
            let (ox, oy) = ((k % 2) * 960, (k / 2) * 540);
            for y in 0..540 {
                let s0 = &f.d[y * 960 * 4..(y + 1) * 960 * 4];
                let d0 = ((oy + y) * 1920 + ox) * 4;
                sheet.d[d0..d0 + 960 * 4].copy_from_slice(s0);
            }
        }
        advance_deflect(&mut sim, 2.2, &index, &stream, 0.5, 0.7).unwrap();
        let (f, out) = render_styled(&mut sim, &camera, 960, 540, Some(&sphere), [0.5, 20.0], &styled, Some(&rig)).unwrap();
        crate::render::write_image(&f, &format!("{dir}/hero-3.png")).unwrap();
        hero_stats.push((3, out.active, out.drawn, out.occluded_by_mesh));
        for y in 0..540 {
            let s0 = &f.d[y * 960 * 4..(y + 1) * 960 * 4];
            let d0 = ((540 + y) * 1920 + 960) * 4;
            sheet.d[d0..d0 + 960 * 4].copy_from_slice(s0);
        }
        crate::render::write_image(&sheet, &format!("{dir}/hero-contact.png")).unwrap();
        eprintln!("hero stats (panel,active,drawn,occluded): {hero_stats:?}");
        // 30-frame orbit sequence, raw vs styled, 640x360.
        let mut sim = Simulation::new(flow()).unwrap();
        let mut seq_stats = Vec::new();
        for k in 0..30 {
            advance_deflect(&mut sim, (k + 1) as f64 / 15.0, &index, &stream, 0.5, 0.7).unwrap();
            let (fr, or_) = render_styled(&mut sim, &camera, 640, 360, Some(&sphere), [0.5, 20.0], &Style::default(), None).unwrap();
            crate::render::write_image(&fr, &format!("{dir}/seq/frame{k:02}-raw.png")).unwrap();
            let (fs, os) = render_styled(&mut sim, &camera, 640, 360, Some(&sphere), [0.5, 20.0], &styled, Some(&rig)).unwrap();
            crate::render::write_image(&fs, &format!("{dir}/seq/frame{k:02}-styled.png")).unwrap();
            if k % 10 == 9 {
                seq_stats.push((k + 1, or_.active, or_.drawn, os.drawn, os.occluded_by_mesh));
            }
        }
        eprintln!("seq stats (frame,active,drawn_raw,drawn_styled,occluded): {seq_stats:?}");
    }
    #[test]
    #[ignore]
    fn pv3_frames() {
        let dir = std::env::var("KIMO_PV3_FRAMES").unwrap_or_else(|_| "/tmp/pv3-frames".into());
        std::fs::create_dir_all(&dir).unwrap();
        let camera = Camera { eye: [0.0, 0.4, -3.2], pan: 0.0, tilt: -0.08, roll: 0.0, sensor_height: 24.0, focal_length: 35.0, near: 0.1 };
        let base = |blend: Blend| Flow {
            emitter: Emitter {
                seed: 7,
                count: 4000,
                origin: [0.0, -0.8, 0.0],
                spread: [0.25, 0.05, 0.25],
                base_velocity: [0.0, 1.2, 0.0],
                velocity_jitter: 0.4,
                birth_rate: 1500.0,
                lifetime: [1.6, 2.4],
                color_stops: vec![
                    ColorStop { offset: 0.0, rgb: [1.0, 0.9, 0.4] },
                    ColorStop { offset: 0.35, rgb: [1.0, 0.45, 0.1] },
                    ColorStop { offset: 0.7, rgb: [0.6, 0.15, 0.5] },
                    ColorStop { offset: 1.0, rgb: [0.1, 0.2, 0.7] },
                ],
                alpha_start: 0.85,
                alpha_end: 0.5,
                fade_in: 0.1,
                fade_out: 0.15,
                size_start: 0.008,
                size_end: 0.05,
                blend,
                hardness: 5.0,
            },
            field: Advect::Curl3 { noise: "perlin".into(), seed: 5, frequency: 0.9, amplitude: 1.4, octaves: 3 },
            gravity: [0.0, -0.15, 0.0],
            drag: 0.05,
            coupling: 3.0,
            field_accel: 0.0,
            dt: 0.004,
        };
        // Lifecycle panels: additive, no mesh, six sim times.
        let times = [0.15, 0.4, 0.8, 1.2, 1.8, 2.4];
        let (pw, ph) = (640usize, 540usize);
        let mut sheet = Frame::try_new(1920, 1080).unwrap();
        let mut stats = Vec::new();
        for (k, t) in times.iter().enumerate() {
            let mut sim = Simulation::new(base(Blend::Additive)).unwrap();
            sim.advance(*t).unwrap();
            let (f, out) = render(&mut sim, &camera, pw, ph, None, [0.5, 20.0]).unwrap();
            crate::render::write_image(&f, &format!("{dir}/life-{k}.png")).unwrap();
            stats.push((t, out.active, out.drawn));
            let (ox, oy) = ((k % 3) * pw, (k / 3) * ph);
            for y in 0..ph {
                let s = &f.d[y * pw * 4..(y + 1) * pw * 4];
                let d0 = ((oy + y) * 1920 + ox) * 4;
                sheet.d[d0..d0 + pw * 4].copy_from_slice(s);
            }
        }
        crate::render::write_image(&sheet, &format!("{dir}/contact.png")).unwrap();
        eprintln!("lifecycle stats: {stats:?}");
        // Mesh interaction: premultiplied alpha over a shaded sphere, curl advection.
        let sphere = generate(&Primitive::Sphere { radius: 0.9, longitude_slices: 48, latitude_slices: 24 }).unwrap();
        let mut sim = Simulation::new(base(Blend::PremultipliedAlpha)).unwrap();
        sim.advance(1.6).unwrap();
        let (f, out) = render(&mut sim, &camera, 1920, 1080, Some(&sphere), [0.5, 20.0]).unwrap();
        crate::render::write_image(&f, &format!("{dir}/mesh-interaction.png")).unwrap();
        eprintln!("mesh interaction: active={} drawn={} occluded={}", out.active, out.drawn, out.occluded_by_mesh);
        assert!(out.occluded_by_mesh > 0, "some particles must pass behind the sphere");
    }
}
