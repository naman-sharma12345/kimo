//! 3D glass shard burst: hundreds of thick, translucent, faceted shards flying in 3D space, seen through the project camera.
//! Per shard: random convex polygon with thickness (two faces + rim), tumbling rotation, drag/gravity motion, depth of field
//! (soft edges and lower alpha away from the focus plane), chromatic rim dispersion, specular glints, motion blur by time sub-sampling.
use crate::effects::Frame;
use crate::three_d::{rot_matrix, CamView, Camera};
use serde::{Deserialize, Serialize};
use crate::shard_look::{self, Material};

fn d_count() -> u32 { 450 } fn d_origin() -> [f64; 3] { [0.0, 40.0, 0.0] } fn d_speed() -> f64 { 520.0 } fn d_smin() -> f64 { 5.0 } fn d_smax() -> f64 { 58.0 } fn d_seed() -> u64 { 7 }
fn d_tint() -> [f64; 3] { [0.78, 0.9, 1.0] } fn d_drag() -> f64 { 1.1 } fn d_spin() -> f64 { 260.0 } fn d_dof() -> f64 { 1.0 } fn d_one() -> f64 { 1.0 } fn d_light() -> [f64; 3] { [-0.5, -0.6, -0.62] }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShardBurst {
    #[serde(default = "d_count")] pub count: u32, #[serde(default = "d_origin")] pub origin: [f64; 3], #[serde(default = "d_speed")] pub speed: f64,
    #[serde(default = "d_smin")] pub size_min: f64, #[serde(default = "d_smax")] pub size_max: f64, #[serde(default = "d_seed")] pub seed: u64,
    #[serde(default = "d_tint")] pub tint: [f64; 3], #[serde(default = "d_drag")] pub drag: f64, #[serde(default)] pub gravity: f64, #[serde(default = "d_spin")] pub spin: f64,
    #[serde(default = "d_dof")] pub dof: f64, #[serde(default)] pub focus: f64, #[serde(default = "d_one")] pub brightness: f64, #[serde(default = "d_light")] pub light: [f64; 3],
    #[serde(default)] pub toward_camera: f64, #[serde(default)] pub spread_time: f64, #[serde(default)] pub fade_out: f64, #[serde(default)] pub dispersion: f64, #[serde(default, skip_serializing_if = "refd_zero")] pub refract_dispersion: f64, #[serde(default)] pub origin_spread: f64,
    #[serde(default)] pub physics: bool, #[serde(default = "d_rest")] pub restitution: f64, #[serde(default = "d_fric")] pub friction: f64, #[serde(default)] pub floor: Option<f64>, #[serde(default)] pub ang_drag: f64, #[serde(default = "d_tr")] pub collide: bool,
    #[serde(default)] pub thickness: Option<[f64; 2]>, #[serde(default)] pub glass: f64, #[serde(default)] pub aspect: Option<[f64; 2]>, #[serde(default)] pub fresnel: f64, #[serde(default)] pub refract: f64, #[serde(default)] pub falloff: f64,
    /// Opt-in looks. Legacy preserves the existing shader. Size controls affect free-burst geometry and physics, not pane cells.
    #[serde(default)] pub material: Material,
    #[serde(default = "d_one")] pub size_bias: f64, #[serde(default = "d_one")] pub size_scale: f64,
    /// World-space analytic sky/ground reflection. Zero leaves the selected look exactly unchanged; roughness widens the softbox.
    #[serde(default)] pub env_reflection: f64, #[serde(default)] pub env_roughness: f64,
    #[serde(default)] pub plane: Option<Plane>,
    /// Contact shadows cast onto the receiving surface (the z = `shadow_plane_z` plane, i.e. the plate the pane sat in): strength 0..1 (0 = off), falls off with height over `shadow_reach` (720p px, 0 = 220), blur grows with height.
    /// Motion blur: shutter angle in degrees (180 = the previous fixed half-frame shutter, 0 = off) and temporal samples (0 = automatic: 4, or 8 with a texture). More samples remove the stepped ghost copies of fast shards (cost is linear).
    /// Debris trails: `trail` 0..1 strength (0 = off), `trail_length` seconds of history (default 0.12), `trail_samples` 2..24 (default 8). Faded earlier-time copies of every shard are composited behind the shard (same shader, same occluder).
    #[serde(default)] pub trail: f64, #[serde(default)] pub trail_length: f64, #[serde(default)] pub trail_samples: u32,
    #[serde(default = "d_shut")] pub shutter_deg: f64, #[serde(default)] pub blur_samples: u32,
    #[serde(default)] pub contact_shadow: f64, #[serde(default)] pub shadow_reach: f64, #[serde(default)] pub shadow_plane_z: f64,
}
fn d_shut() -> f64 { 180.0 }
fn d_rest() -> f64 { 0.35 } fn d_fric() -> f64 { 0.5 } fn d_tr() -> bool { true }

/// `plane_fracture` birth mode: a coplanar pane split into connected, non-overlapping Voronoi cells (small near the impact, large far away).
/// Cells are released by an expanding wave from `impact` and thrown outward and toward the camera with a pressure impulse.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Plane {
    #[serde(default)] pub impact: [f64; 2], #[serde(default = "d_region")] pub region: [f64; 2], #[serde(default = "d_cells")] pub cells: u32, #[serde(default = "d_pr")] pub radius: f64,
    #[serde(default = "d_wave")] pub wave_speed: f64, #[serde(default = "d_imp")] pub impulse: f64, #[serde(default = "d_lift")] pub lift: f64, #[serde(default)] pub under_depth: f64,
    #[serde(default = "d_grade")] pub grading: f64, #[serde(default = "d_tr")] pub hidden_until_birth: bool, #[serde(default = "d_maxd")] pub max_delay: f64,
    /// Source quad in 1280x720 canvas pixels (TL,TR,BR,BL). The plane (`region` units, use the texture UV size) is mapped onto it by a homography, so birth is confined to the quad and starts in its pose.
    #[serde(default)] pub quad: Option<[[f64; 2]; 4]>, #[serde(default)] pub impact_uv: Option<[f64; 2]>,
    /// Image carried on the facets (UV = plane coordinates). `opaque` 0..1 mixes it over the glass shading.
    /// Foreground occluder matte (white = in front of the plane, cells hidden there). A path; `%d`/`%04d` style is replaced by the frame index round(t*fps). Any ffmpeg-readable image, sized to the canvas.
    /// Preload dimple: before a cell is released the plane sinks away from the camera by `preload_depth` (720p px, strongest at the impact, falling off with distance) over `preload_time` s, so the pressure builds before the break.
    #[serde(default)] pub preload_depth: f64, #[serde(default)] pub preload_time: f64,
    #[serde(default)] pub occlusion: Option<String>, /// Id of a pinned clip that carries an adaptive occluder recipe (track_planar occluder_mode=adaptive, ideally occluder_expand > 0); its live matte replaces the static `occlusion` pattern, with the same source timing and pin transform as the pinned graphic.
    #[serde(default)] pub occlusion_target: Option<String>, #[serde(default)] pub texture: Option<String>, #[serde(default = "d_one")] pub opaque: f64,
}
fn d_region() -> [f64; 2] { [1280.0, 720.0] } fn d_cells() -> u32 { 120 } fn d_pr() -> f64 { 140.0 } fn d_wave() -> f64 { 2400.0 } fn d_imp() -> f64 { 1100.0 } fn d_lift() -> f64 { 0.9 } fn d_grade() -> f64 { 3.0 } fn d_maxd() -> f64 { 0.25 }
pub struct Cell { pub poly: Vec<(f64, f64)>, pub c: (f64, f64), pub area: f64, pub lc: (f64, f64), pub uv: Vec<(f64, f64)> }
pub fn sq2quad(q: [(f64, f64); 4]) -> [f64; 8] { let (x0, y0, x1, y1, x2, y2, x3, y3) = (q[0].0, q[0].1, q[1].0, q[1].1, q[2].0, q[2].1, q[3].0, q[3].1);
    let (dx1, dx2, dx3, dy1, dy2, dy3) = (x1 - x2, x3 - x2, x0 - x1 + x2 - x3, y1 - y2, y3 - y2, y0 - y1 + y2 - y3); let den = dx1 * dy2 - dy1 * dx2; let (g, hh) = if den.abs() < 1e-12 { (0.0, 0.0) } else { ((dx3 * dy2 - dy3 * dx2) / den, (dx1 * dy3 - dy1 * dx3) / den) };
    [x1 - x0 + g * x1, x3 - x0 + hh * x3, x0, y1 - y0 + g * y1, y3 - y0 + hh * y3, y0, g, hh] }
pub fn hmap(m: &[f64; 8], u: f64, v: f64) -> (f64, f64) { let w = m[6] * u + m[7] * v + 1.0; ((m[0] * u + m[1] * v + m[2]) / w, (m[3] * u + m[4] * v + m[5]) / w) }
/// Quad homography in canvas-centre offsets (pixels at this render scale) or None for the legacy screen-facing plane.
pub fn plane_warp(p: &Plane, k: f64) -> Option<[f64; 8]> { p.quad.map(|q| { let o = |i: usize| ((q[i][0] - 640.0) * k, (q[i][1] - 360.0) * k); sq2quad([o(0), o(1), o(2), o(3)]) }) }
/// (local impact, canvas impact) for the plane.
pub fn plane_impact(p: &Plane, k: f64) -> ((f64, f64), (f64, f64)) { let (rw, rh) = (p.region[0] * k, p.region[1] * k);
    let li = match p.impact_uv { Some(iu) => ((iu[0] - 0.5) * rw, (iu[1] - 0.5) * rh), None => (p.impact[0] * k, p.impact[1] * k) };
    let ci = match plane_warp(p, k) { Some(m) => hmap(&m, li.0 / rw + 0.5, li.1 / rh + 0.5), None => li }; (li, ci) }
fn clip(poly: &[(f64, f64)], n: (f64, f64), off: f64) -> Vec<(f64, f64)> { // keep points with n.p <= off
    let mut out = vec![]; for i in 0..poly.len() { let (a, b) = (poly[i], poly[(i + 1) % poly.len()]); let (da, db) = (n.0 * a.0 + n.1 * a.1 - off, n.0 * b.0 + n.1 * b.1 - off);
        if da <= 0.0 { out.push(a); } if (da < 0.0) != (db < 0.0) && (da - db).abs() > 1e-12 { let t = da / (da - db); out.push((a.0 + (b.0 - a.0) * t, a.1 + (b.1 - a.1) * t)); } } out }
pub fn plane_cells(p: &Plane, seed: u64, k: f64) -> Vec<Cell> {
    let (rw, rh) = (p.region[0] * k, p.region[1] * k); let ((ix, iy), _) = plane_impact(p, k); let warp = plane_warp(p, k); let n = p.cells.clamp(4, 400) as usize; let rad = (p.radius * k).max(1.0);
    // dart throwing with a spacing that grows with distance from the impact (area-weighted size distribution)
    let base = (rw * rh / n as f64).sqrt() * 0.62; let mut sites: Vec<(f64, f64)> = vec![]; let mut tries = 0u64;
    while sites.len() < n && tries < 40000 { tries += 1; let x = (h(seed, tries, 900) - 0.5) * rw; let y = (h(seed, tries, 901) - 0.5) * rh; let d = ((x - ix).powi(2) + (y - iy).powi(2)).sqrt();
        let sp = base * (0.35 + p.grading * sstep(d / (rad * 3.5)) * 0.6 + 0.9 * sstep(d / (rad * 9.0))); if sites.iter().all(|q| ((q.0 - x).powi(2) + (q.1 - y).powi(2)).sqrt() >= sp) { sites.push((x, y)); } }
    let rect = vec![(-rw / 2.0, -rh / 2.0), (rw / 2.0, -rh / 2.0), (rw / 2.0, rh / 2.0), (-rw / 2.0, rh / 2.0)]; let mut cells = vec![];
    for (i, &s) in sites.iter().enumerate() { let mut poly = rect.clone();
        for (j, &t) in sites.iter().enumerate() { if i == j { continue; } let nv = (t.0 - s.0, t.1 - s.1); let mid = ((s.0 + t.0) * 0.5, (s.1 + t.1) * 0.5); if (nv.0 * nv.0 + nv.1 * nv.1) > (4.0 * base * base * 9.0) { continue; } poly = clip(&poly, nv, nv.0 * mid.0 + nv.1 * mid.1); if poly.len() < 3 { break; } }
        if poly.len() < 3 { continue; }
        while poly.len() > 6 { let m = poly.len(); let mut best = (f64::MAX, 0); for v in 0..m { let (a, b, c) = (poly[(v + m - 1) % m], poly[v], poly[(v + 1) % m]); let ar = ((b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1)).abs() * 0.5; if ar < best.0 { best = (ar, v); } } poly.remove(best.1); }
        let mut area = 0.0; let (mut cx, mut cy) = (0.0, 0.0); for v in 0..poly.len() { let (a, b) = (poly[v], poly[(v + 1) % poly.len()]); let cr = a.0 * b.1 - b.0 * a.1; area += cr; cx += (a.0 + b.0) * cr; cy += (a.1 + b.1) * cr; }
        if area.abs() < 1e-6 { continue; } let lc = (cx / (3.0 * area), cy / (3.0 * area)); let uv: Vec<(f64, f64)> = poly.iter().map(|q| (q.0 / rw + 0.5, q.1 / rh + 0.5)).collect();
        let wp: Vec<(f64, f64)> = match &warp { Some(m) => uv.iter().map(|q| hmap(m, q.0, q.1)).collect(), None => poly.clone() };
        let (mut wa, mut wx, mut wy) = (0.0, 0.0, 0.0); for v in 0..wp.len() { let (a, b) = (wp[v], wp[(v + 1) % wp.len()]); let cr = a.0 * b.1 - b.0 * a.1; wa += cr; wx += (a.0 + b.0) * cr; wy += (a.1 + b.1) * cr; } if wa.abs() < 1e-6 { continue; }
        let c = (wx / (3.0 * wa), wy / (3.0 * wa)); let rel: Vec<(f64, f64)> = wp.iter().map(|q| (q.0 - c.0, q.1 - c.1)).collect(); cells.push(Cell { poly: rel, c, area: wa.abs() * 0.5, lc, uv }); }
    cells
}

fn h(a: u64, i: u64, k: u64) -> f64 { let mut x = a.wrapping_mul(0x9E3779B97F4A7C15) ^ i.wrapping_mul(0xBF58476D1CE4E5B9) ^ k.wrapping_mul(0x94D049BB133111EB); x ^= x >> 30; x = x.wrapping_mul(0xBF58476D1CE4E5B9); x ^= x >> 27; x = x.wrapping_mul(0x94D049BB133111EB); x ^= x >> 31; (x >> 11) as f64 / (1u64 << 53) as f64 }
fn sstep(x: f64) -> f64 { let t = x.clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

struct Prim { depth: f64, pts: [(f64, f64); 6], n: usize, nz: f64, spec: f64, rim_c: f64, blur: f64, alpha: f64, face: usize, u: f64, tx: f64, ty: f64, n3: [f64; 3], tex: Option<[f64; 6]>, id: u64, flat: f64, env: ([f64; 3], f64) }

/// Per-render state that used to be process-global. Built once per frame from the project and passed down; two renders with different ctx never interfere.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct RenderCtx { pub work_linear: bool }

pub fn render(e: &ShardBurst, f: &mut Frame, t: f64, cam: &Camera, fps: f64, total: f64) { render_ctx(&RenderCtx::default(), e, f, t, cam, fps, total) }

pub fn render_ctx(ctx: &RenderCtx, e: &ShardBurst, f: &mut Frame, t: f64, cam: &Camera, fps: f64, total: f64) {
    let (w, hh) = (f.w, f.h); let cv = CamView::new(w, hh, cam); let fl = cv.focal(); let k = w.min(hh) as f64 / 720.0;
    let nsub = if e.shutter_deg <= 0.0 { 1usize } else if e.blur_samples > 0 { (e.blur_samples as usize).clamp(1, 64) } else if e.plane.as_ref().map(|p| p.texture.is_some()).unwrap_or(false) { 8usize } else { 4usize }; let shutter = (e.shutter_deg.clamp(0.0, 720.0) / 360.0) / fps.max(1.0); let mut accc = vec![0f32; w * hh * 3]; let mut acca = vec![0f32; w * hh]; let rf = e.refract > 0.0 && e.material.transmissive(); let mut dacc = vec![0f32; if rf { w * hh * 2 } else { 0 }]; let mut dsum = vec![0f32; if rf { w * hh * 2 } else { 0 }];
    let lg = { let l = e.light; let m = (l[0] * l[0] + l[1] * l[1] + l[2] * l[2]).sqrt().max(1e-9); [l[0] / m, l[1] / m, l[2] / m] };
    let focus = if e.focus > 0.0 { e.focus } else { fl };
    let pcells: Option<(Plane, Vec<Cell>)> = e.plane.as_ref().map(|p| (p.clone(), plane_cells(p, e.seed, k)));
    let tex = e.plane.as_ref().and_then(|p| p.texture.as_ref().and_then(|t| load_tex(ctx, t, p.region)));
    let occ = if let Some(m) = EXT_OCC.with(|c| c.borrow_mut().take()) { if m.len() == w * hh { Some(m) } else { None } } else { e.plane.as_ref().and_then(|p| p.occlusion.as_ref().and_then(|o| load_occ(o, (t * fps).round() as i64, w, hh))) };
    let mut shadow = vec![0f32; if e.contact_shadow > 0.0 { w * hh } else { 0 }]; let sreach = if e.shadow_reach > 0.0 { e.shadow_reach } else { 220.0 } * k;
    let traj = if e.physics { Some((bake_cached(e, k, total, hh), 0)) } else { None };
    // sample list: trail samples first (older time, fading weight), then the shutter samples; the last sample is always a shutter sample
    let mut samples: Vec<(f64, f32, bool)> = vec![];
    if e.trail > 0.0 && e.trail.is_finite() { let nt = if e.trail_samples == 0 { 8 } else { (e.trail_samples as usize).clamp(2, 24) }; let len = if e.trail_length > 0.0 { e.trail_length.min(2.0) } else { 0.12 };
        for kk in (1..=nt).rev() { let u = kk as f64 / nt as f64; let wt = (e.trail.clamp(0.0, 1.0) * (1.0 - u * 0.85).powi(2)) as f32 / nsub as f32 * (4.0 / nt as f32).min(1.0); samples.push(((t - u * len).max(0.0), wt, true)); } }
    for sub in 0..nsub { samples.push(((t + ((sub as f64 + 0.5) / nsub as f64 - 0.5) * shutter).max(0.0), 1.0 / nsub as f32, false)); }
    for (si, &(ts, wt, is_trail)) in samples.iter().enumerate() {
        let mut prims: Vec<Prim> = Vec::with_capacity(e.count as usize * 2); let mut shp: Vec<(Vec<(f64, f64)>, f64, f64)> = vec![];
        let ncount = pcells.as_ref().map(|c| c.1.len() as u64).unwrap_or(e.count as u64);
        for i in 0..ncount {
            let r = |kk: u64| h(e.seed, i, kk); let pc = pcells.as_ref().map(|c| (&c.0, &c.1[i as usize]));
            let pdist = pc.map(|(p, c)| { let (li, _) = plane_impact(p, k); ((c.lc.0 - li.0).powi(2) + (c.lc.1 - li.1).powi(2)).sqrt() });
            let birth = match (pc, pdist) { (Some((p, _)), Some(d)) => (d / (p.wave_speed * k)).min(p.max_delay), _ => r(40) * e.spread_time };
            if ts < birth && pc.map(|(p, _)| p.hidden_until_birth).unwrap_or(true) { continue; } let age = (ts - birth).max(0.0);
            let size = if let Some((_, c)) = pc { (c.area.sqrt() / 1.8).max(1.0) } else { shard_look::size(e.size_min, e.size_max, e.size_bias, e.size_scale, r(1)) * k }; // heavy toward small, a few large
            let (th, ph) = (r(2) * std::f64::consts::TAU, (r(3) * 2.0 - 1.0).acos()); let dir = [ph.sin() * th.cos(), ph.sin() * th.sin() * 0.8, ph.cos()];
            let sp = e.speed * k * (0.35 + 1.1 * r(4).powf(1.2)) / (0.6 + size / (30.0 * k)).sqrt(); let dr = e.drag.max(1e-3); let travel = (1.0 - (-dr * age).exp()) / dr;
            let mut vz = dir[2] * sp; vz -= e.toward_camera * sp * (0.3 + r(5)); // negative z = toward camera
            let os = e.origin_spread * k; let pos = [e.origin[0] * k + (r(50) - 0.5) * 2.0 * os + dir[0] * sp * travel, e.origin[1] * k + (r(51) - 0.5) * 2.0 * os * 0.6 + dir[1] * sp * travel + 0.5 * e.gravity * k * age * age, e.origin[2] + (r(52) - 0.5) * 2.0 * os + vz * travel];
            let pos = if let (Some((p, c)), Some(d)) = (pc, pdist) { let (_, (ix, iy)) = plane_impact(p, k); let (dx, dy) = (c.c.0 - ix, c.c.1 - iy); let m = (dx * dx + dy * dy).sqrt().max(1e-6); let (ux, uy) = if d < 1e-3 { let a = r(60) * std::f64::consts::TAU; (a.cos(), a.sin()) } else { (dx / m, dy / m) };
                let v0 = p.impulse * k * (-d / (p.radius * k * 3.0)).exp().max(0.12) * (0.6 + 0.8 * r(4)) / (0.7 + size / (40.0 * k)).sqrt(); let vz = -p.lift * v0 * (0.6 + 0.8 * r(5));
                [c.c.0 + ux * v0 * travel, c.c.1 + uy * v0 * travel + 0.5 * e.gravity * k * age * age, p.under_depth * k + vz * travel + pre_dep(p, d, k, ts, birth)] } else { pos };
            let phys = traj.as_ref().map(|(tj, _): &(std::sync::Arc<crate::physics::Traj>, i32)| tj.at(i as usize, ts));
            let ang = if pc.is_some() { [(r(7) - 0.5) * 2.0 * e.spin * age, (r(9) - 0.5) * 2.0 * e.spin * age, (r(11) - 0.5) * e.spin * age] } else { [r(6) * 360.0 + (r(7) - 0.5) * 2.0 * e.spin * age, r(8) * 360.0 + (r(9) - 0.5) * 2.0 * e.spin * age, r(10) * 360.0 + (r(11) - 0.5) * 2.0 * e.spin * age] };
            let (pos, rm) = match &phys { Some((p, q)) => (*p, crate::physics::quat_matrix(*q)), None => (pos, rot_matrix(ang)) }; let pos = match (&phys, pc, pdist) { (Some(_), Some((p, _)), Some(d)) => [pos[0], pos[1], pos[2] + pre_dep(p, d, k, ts, birth)], _ => pos }; let nv = pc.map(|(_, c)| c.poly.len()).unwrap_or(4 + (r(12) * 2.99) as usize); let aspect = if pc.is_some() { 1.0 } else { let a = e.aspect.unwrap_or([0.62, 1.22]); a[0] + (a[1] - a[0]) * r(13) }; let thick = ({ let t = e.thickness.unwrap_or([0.3, 0.9]); t[0] + (t[1] - t[0]) * r(14) }) * size;
            let life_fade = if e.fade_out > 0.0 { 1.0 - sstep((age - (total - e.fade_out - birth).max(0.1)) / e.fade_out) } else { 1.0 }; let born = if ts < birth { 1.0 } else { sstep(age / 0.05) }; let fl = if pc.is_some() && e.plane.is_some() { if ts < birth { 1.0 } else { 1.0 - sstep(age / 0.1) } } else { 0.0 };
            let mut loc = [(0.0, 0.0); 6]; if let Some((_, c)) = pc { for v in 0..nv { loc[v] = c.poly[v]; } } else { for v in 0..nv { let a = v as f64 / nv as f64 * std::f64::consts::TAU + (h(e.seed, i, 20 + v as u64) - 0.5) * 1.1; let rr = size * (0.62 + 0.42 * h(e.seed, i, 30 + v as u64)); loc[v] = (a.cos() * rr, a.sin() * rr * aspect); } }
            let mut shard: Vec<([(f64, f64); 6], f64, f64, f64, f64)> = vec![]; // per face
            let nrm = [rm[0][2], rm[1][2], rm[2][2]];
            let eye = cv.eye(); let to_eye = [eye[0]-pos[0], eye[1]-pos[1], eye[2]-pos[2]];
            let env_for = |normal| if e.env_reflection.is_finite() && e.env_reflection > 0.0 { shard_look::environment(normal, to_eye, e.env_roughness, ctx.work_linear) } else { ([0.0;3],0.0) }; let mut zsum = 0.0; let mut ok = true;
            for face in 0..2 { let zoff = if face == 0 { thick * 0.5 } else { -thick * 0.5 }; let mut pts = [(0.0, 0.0); 6]; let mut dz = 0.0;
                for v in 0..nv { let l = [loc[v].0, loc[v].1, zoff]; let wp = [pos[0] + rm[0][0] * l[0] + rm[0][1] * l[1] + rm[0][2] * l[2], pos[1] + rm[1][0] * l[0] + rm[1][1] * l[1] + rm[1][2] * l[2], pos[2] + rm[2][0] * l[0] + rm[2][1] * l[1] + rm[2][2] * l[2]];
                    match cv.project(wp) { Some((sx, sy, zv)) => { pts[v] = (sx, sy); dz += zv; } None => { ok = false; } } }
                if !ok { break; } dz /= nv as f64; zsum += dz; shard.push((pts, dz, 0.0, 0.0, 0.0)); }
            if e.contact_shadow > 0.0 && !is_trail { let mut poly = Vec::with_capacity(nv); let mut hs = 0.0; let mut okp = true; let zoff = thick * 0.5;
                for v in 0..nv { let l = [loc[v].0, loc[v].1, zoff]; let wp = [pos[0] + rm[0][0] * l[0] + rm[0][1] * l[1] + rm[0][2] * l[2], pos[1] + rm[1][0] * l[0] + rm[1][1] * l[1] + rm[1][2] * l[2], pos[2] + rm[2][0] * l[0] + rm[2][1] * l[1] + rm[2][2] * l[2]];
                    let hgt = (e.shadow_plane_z - wp[2]).max(0.0); hs += hgt; let lz = lg[2].abs().max(0.25); let (sx, sy) = ((-lg[0] / lz).clamp(-1.5, 1.5), (-lg[1] / lz).clamp(-1.5, 1.5));
                    match cv.project([wp[0] + sx * hgt, wp[1] + sy * hgt, e.shadow_plane_z]) { Some((px, py, _)) => poly.push((px, py)), None => { okp = false; } } }
                let hm = hs / nv as f64; if okp && hm > 0.5 * k { shp.push((poly, e.contact_shadow.clamp(0.0, 1.0) * (-hm / sreach).exp() * life_fade * born, (1.5 + hm * 0.10).min(40.0 * k))); } }
            if !ok || shard.len() < 2 { continue; } let zv = zsum / 2.0; let coc = (e.dof * ((zv - focus).abs() / zv) * fl * 0.035 * k).min(10.0 * k); let ndl = nrm[0] * lg[0] + nrm[1] * lg[1] + nrm[2] * lg[2];
            let refl = [lg[0] - 2.0 * ndl * nrm[0], lg[1] - 2.0 * ndl * nrm[1], lg[2] - 2.0 * ndl * nrm[2]]; let spec = refl[2].max(0.0).powi(18) + 0.35 * (-refl[2]).max(0.0).powi(18);
            // Legacy shadows the focal length with flatness (`fl`), producing NaN at flatness=0.
            // Preserve its output for old projects; opt-in materials use the true focal depth.
            let focal_depth = if e.material == Material::Legacy { fl } else { cv.focal() };
            let dim = 1.0 / (1.0 + coc * 0.12) * (-e.falloff * ((zv - focal_depth).max(0.0) / focal_depth)).exp(); let fres = 0.04 + 0.96 * (1.0 - nrm[2].abs()).powi(5); let al = life_fade * born * dim;
            { let (fp, bp) = (shard[0].0, shard[1].0); for v in 0..nv { let w2 = (v + 1) % nv; let (ex, ey) = (loc[w2].0 - loc[v].0, loc[w2].1 - loc[v].1); let l = (ex * ex + ey * ey).sqrt().max(1e-9); let nl = [ey / l, -ex / l, 0.0];
                let nw = [rm[0][0] * nl[0] + rm[0][1] * nl[1], rm[1][0] * nl[0] + rm[1][1] * nl[1], rm[2][0] * nl[0] + rm[2][1] * nl[1]]; let nd = nw[0] * lg[0] + nw[1] * lg[1] + nw[2] * lg[2]; let rf = [lg[0] - 2.0 * nd * nw[0], lg[1] - 2.0 * nd * nw[1], lg[2] - 2.0 * nd * nw[2]];
                let sp2 = rf[2].max(0.0).powi(14) + 0.4 * (-rf[2]).max(0.0).powi(14); let mut q = [(0.0, 0.0); 6]; q[0] = fp[v]; q[1] = fp[w2]; q[2] = bp[w2]; q[3] = bp[v];
                prims.push(Prim { depth: (shard[0].1 + shard[1].1) / 2.0 - 0.01, pts: q, n: 4, nz: nw[2].abs(), spec: sp2 + 0.12, rim_c: coc, blur: coc, alpha: al * 0.8 * (1.0 - fl), face: 2, u: r(15), tx: nw[0] * thick * 1.6, ty: nw[1] * thick * 1.6, n3: nw, tex: None, id: i, flat: fl, env: env_for(nw) }); } }
            for (fi, (pts, dz, _, _, _)) in shard.into_iter().enumerate() { let tex_aff = match (pc, &tex) { (Some((_, c)), Some(_)) if nv >= 3 => uv_affine(&pts, &c.uv), _ => None }; prims.push(Prim { depth: dz, pts, n: nv, nz: nrm[2].abs(), spec, rim_c: fres, blur: coc, alpha: if fi == 1 { al * (1.0 - fl) } else { al }, face: fi, u: r(15), tx: nrm[0] * thick, ty: nrm[1] * thick, n3: nrm, tex: tex_aff, id: i, flat: fl, env: env_for(nrm) }); }
        }
        for (poly, sa, sb) in &shp { if sa < &0.003 { continue; } let n = poly.len(); if n < 3 { continue; }
            let mut area = 0.0; for v in 0..n { let (a, b) = (poly[v], poly[(v + 1) % n]); area += a.0 * b.1 - b.0 * a.1; } if area.abs() < 0.5 { continue; } let sg = if area >= 0.0 { 1.0 } else { -1.0 };
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); for p in poly { x0 = x0.min(p.0); x1 = x1.max(p.0); y0 = y0.min(p.1); y1 = y1.max(p.1); }
            let (xa, xb, ya, yb) = ((x0 - sb).floor().max(0.0) as usize, (x1 + sb).ceil().min(w as f64 - 1.0) as usize, (y0 - sb).floor().max(0.0) as usize, (y1 + sb).ceil().min(hh as f64 - 1.0) as usize);
            for y in ya..=yb { for x in xa..=xb { let (px, py) = (x as f64 + 0.5, y as f64 + 0.5); let mut d = f64::MAX; for v in 0..n { let (a, b) = (poly[v], poly[(v + 1) % n]); let (ex, ey) = (b.0 - a.0, b.1 - a.1); let l = (ex * ex + ey * ey).sqrt().max(1e-9); d = d.min(-sg * ((px - a.0) * ey - (py - a.1) * ex) / l); }
                let cov = sstep((d + sb) / (2.0 * sb)); if cov <= 0.002 { continue; } let o = y * w + x; let v = (cov * sa * occ.as_ref().map(|m| 1.0 - m[o] as f64).unwrap_or(1.0)) as f32 / nsub as f32; shadow[o] = shadow[o] + v - shadow[o] * v * nsub as f32 / 1.0 * 0.0; } } }
        prims.sort_by(|a, b| b.depth.partial_cmp(&a.depth).unwrap());
        PROBE.with(|pr| { let mut v = pr.borrow_mut(); v.clear(); for q in prims.iter().filter(|q| q.face == 0) { let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); let mut c = (0.0, 0.0); for j in 0..q.n { x0 = x0.min(q.pts[j].0); x1 = x1.max(q.pts[j].0); y0 = y0.min(q.pts[j].1); y1 = y1.max(q.pts[j].1); c.0 += q.pts[j].0 / q.n as f64; c.1 += q.pts[j].1 / q.n as f64; } v.push(ProbeShard { id: q.id, cx: c.0, cy: c.1, bbox: [x0, y0, x1, y1], depth: q.depth, alpha: q.alpha }); } });
        for pr in &prims {
            let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN); for v in 0..pr.n { x0 = x0.min(pr.pts[v].0); x1 = x1.max(pr.pts[v].0); y0 = y0.min(pr.pts[v].1); y1 = y1.max(pr.pts[v].1); }
            let pad = pr.blur * 1.2 + 2.0; let (xa, xb, ya, yb) = (((x0 - pad).floor().max(0.0)) as usize, ((x1 + pad).ceil().min(w as f64 - 1.0)) as usize, ((y0 - pad).floor().max(0.0)) as usize, ((y1 + pad).ceil().min(hh as f64 - 1.0)) as usize); if x0 > x1 || xa > xb || ya > yb || (x1 - x0) > w as f64 * 3.0 { continue; }
            // orientation sign for signed distance
            let mut area = 0.0; for v in 0..pr.n { let (a, b) = (pr.pts[v], pr.pts[(v + 1) % pr.n]); area += a.0 * b.1 - b.0 * a.1; } let sg = if area >= 0.0 { 1.0 } else { -1.0 }; if area.abs() < 0.5 { continue; }
            let (cxm, cym) = ((x0 + x1) * 0.5, (y0 + y1) * 0.5); let gl = e.glass.clamp(0.0, 1.0); let sw = ((pr.blur * 0.9 + 0.7).max(0.7)) * (1.0 + 1.4 * gl); let size_px = (x1 - x0).max(y1 - y0).max(1.0);
            for y in ya..=yb { for x in xa..=xb { let (px, py) = (x as f64 + 0.5, y as f64 + 0.5); let d; let mut dmin = f64::MAX; let mut ins = false; for v in 0..pr.n { let (a, b) = (pr.pts[v], pr.pts[(v + 1) % pr.n]); let (ex, ey) = (b.0 - a.0, b.1 - a.1); let l2 = (ex * ex + ey * ey).max(1e-9); let tt = (((px - a.0) * ex + (py - a.1) * ey) / l2).clamp(0.0, 1.0); let dd = ((px - a.0 - tt * ex).hypot(py - a.1 - tt * ey)); if dd < dmin { dmin = dd; } if (a.1 > py) != (b.1 > py) && px < (b.0 - a.0) * (py - a.1) / (b.1 - a.1) + a.0 { ins = !ins; } } let _ = sg; d = if ins { dmin } else { -dmin };
                let cov = sstep((d + pr.flat * 0.75 + sw) / (2.0 * sw)) * occ.as_ref().map(|m| 1.0 - m[y * w + x] as f64).unwrap_or(1.0); if cov <= 0.002 { continue; }
                let inner = d.max(0.0); let rimw = (1.1 * k + pr.blur * 0.5) * (1.0 + 1.8 * gl); let rim = (-inner / rimw).exp(); let (rr, rg, rb) = if e.dispersion > 0.0 { let o = (e.dispersion.min(3.0) * 0.4 * (1.0 + pr.blur * 0.04)).min(1.6 * k); ((-(inner + o).max(0.0) / rimw).exp(), rim, (-(inner - o).max(0.0) / rimw).exp()) } else { (rim, rim, rim) };
                let grad = 0.55 + 0.45 * (((px - cxm) / size_px + (py - cym) / size_px) * 0.9 + 0.5 + pr.u * 0.3).clamp(0.0, 1.0);
                let face_a = (e.fresnel * 1.3 * pr.rim_c * (0.6 + 0.8 * grad) + 0.17 + 0.2 * (1.0 - pr.nz) + 0.1 * grad + 1.0 * pr.spec * if pr.face == 0 { 1.0 } else { 0.6 }) * if pr.face == 1 { 0.7 } else { 1.0 };
                let face_a = if rf { face_a * 0.45 } else { face_a }; let face_a = face_a * (1.0 - 0.5 * gl); let a = ((face_a + 0.5 * rim * (1.0 - 0.8 * gl) * (0.5 + 0.5 * pr.face as f64 * 0.0 + 0.5)) * cov * pr.alpha).min(0.95); if a <= 0.002 { continue; }
                let br = e.brightness; let base = [e.tint[0], e.tint[1], e.tint[2]]; let sp = pr.spec * 0.8; let o = y * w + x;
                let envl = if gl > 0.0 { let (n0, n1, n2) = (pr.n3[0], pr.n3[1], pr.n3[2]); let sg = if n2 < 0.0 { -1.0 } else { 1.0 }; let (n0, n1, n2) = (n0 * sg, n1 * sg, n2 * sg); let (lx, ly) = ((px - cxm) / size_px, (py - cym) / size_px);
                    let rx = 2.0 * n2 * n0 + lx * 0.9; let ry = 2.0 * n2 * n1 + ly * 0.9; let ang = pr.u * 6.2831; let ax = rx * ang.cos() + ry * ang.sin(); let c0 = 0.15 + 0.5 * (pr.u * 7.0).fract();
                    let strip = (-((ax - c0) / 0.16).powi(2)).exp(); let strip2 = (-((ax + 0.55 - 0.3 * c0) / 0.07).powi(2)).exp(); let sky = 0.1 + 0.3 * (ry * 0.5 + 0.5).clamp(0.0, 1.0); (sky + 1.3 * strip + 0.9 * strip2) * (0.5 + 0.5 * (1.0 - n2.abs() * 0.6)) } else { 0.0 };
                let rw = |c: f64, t: f64| c * (1.0 - 0.6 * gl) + c * t * 0.6 * gl; let glint = gl * pr.spec * pr.spec * 1.2 * rim + gl * envl * 0.55; let cr = ((base[0] * (0.35 + 0.45 * grad) + sp) * (1.0 - rim) + rw(rr, base[0]) + glint) * br; let cg = ((base[1] * (0.35 + 0.45 * grad) + sp) * (1.0 - rim) + rw(rg, base[1]) + glint) * br; let cb = ((base[2] * (0.35 + 0.45 * grad) + sp) * (1.0 - rim) + rw(rb, base[2]) + glint) * br;
                let a = (a + gl * envl * 0.3 * cov * pr.alpha).min(0.95);
                let (rgb, a) = shard_look::shade(e.material, [cr,cg,cb], a, base, br, pr.n3, lg, pr.spec, rim, pr.u, cov*pr.alpha);
                let (cr,cg,cb) = (rgb[0],rgb[1],rgb[2]);
                let (cr, cg, cb, a) = match (&pr.tex, &tex) { (Some(m), Some(t)) => { let (u, v) = (m[0] * px + m[1] * py + m[2], m[3] * px + m[4] * py + m[5]); let s = t.sample(u, v); let mo = e.plane.as_ref().map(|p| p.opaque).unwrap_or(1.0).clamp(0.0, 1.0);
                        let sh = (0.62 + 0.38 * grad) * if pr.face == 1 { 0.7 } else { 1.0 }; let sh = sh * (1.0 - pr.flat) + pr.flat; let hl = (sp * 0.35 + rim * 0.22) * (1.0 - pr.flat); let br = br * (1.0 - pr.flat) + pr.flat;
                        let (tr, tg, tb) = ((s[0] * sh + hl) * br, (s[1] * sh + hl) * br, (s[2] * sh + hl) * br); let oa = (cov * pr.alpha * 0.985).min(0.985);
                        (cr * (1.0 - mo) + tr * mo, cg * (1.0 - mo) + tg * mo, cb * (1.0 - mo) + tb * mo, a * (1.0 - mo) + oa * mo) } (None, Some(t)) if pr.face == 2 => { let mo = e.plane.as_ref().map(|p| p.opaque).unwrap_or(1.0).clamp(0.0, 1.0); let sd = 0.16 + 0.5 * sp + 0.1 * grad; let m = t.mean; let oa = (cov * pr.alpha * 0.985).min(0.985);
                        (cr * (1.0 - mo) + (m[0] * sd) * br * mo, cg * (1.0 - mo) + (m[1] * sd) * br * mo, cb * (1.0 - mo) + (m[2] * sd) * br * mo, a * (1.0 - mo) + oa * mo) } _ => (cr, cg, cb, a) }; if a <= 0.002 { continue; }
                let rgb = shard_look::reflect([cr,cg,cb], pr.env, e.env_reflection*(1.0-pr.flat), e.material, base);
                let (cr,cg,cb) = (rgb[0],rgb[1],rgb[2]);
                let (ca, aa) = (a as f32, acca[o]); // over (premultiplied accumulation)
                accc[o * 3] = accc[o * 3] * (1.0 - ca) + cr as f32 * ca; accc[o * 3 + 1] = accc[o * 3 + 1] * (1.0 - ca) + cg as f32 * ca; accc[o * 3 + 2] = accc[o * 3 + 2] * (1.0 - ca) + cb as f32 * ca; acca[o] = ca + aa * (1.0 - ca); if rf { let k2 = e.refract as f32 * k as f32 * (1.0 + 0.5 * rim as f32); dacc[o * 2] = dacc[o * 2] * (1.0 - ca) + (pr.tx as f32 * k2) * ca; dacc[o * 2 + 1] = dacc[o * 2 + 1] * (1.0 - ca) + (pr.ty as f32 * k2) * ca; } } } }
        // fold sub-frame into frame accumulators happens after all subs; keep running average via scaling below
        if si + 1 < samples.len() { // store partial: normalise at the end using per-sub snapshots
            // snapshots are summed by re-using buffers: move current to f (additive average) then clear
                for o in 0..w * hh { let i4 = o * 4; f.d[i4] += accc[o * 3] * wt; f.d[i4 + 1] += accc[o * 3 + 1] * wt; f.d[i4 + 2] += accc[o * 3 + 2] * wt; f.d[i4 + 3] += acca[o] * wt; if rf { dsum[o * 2] += dacc[o * 2] * wt; dsum[o * 2 + 1] += dacc[o * 2 + 1] * wt; dacc[o * 2] = 0.0; dacc[o * 2 + 1] = 0.0; } accc[o * 3] = 0.0; accc[o * 3 + 1] = 0.0; accc[o * 3 + 2] = 0.0; acca[o] = 0.0; }
        }
    }
    if rf { for o in 0..w * hh { dsum[o * 2] += dacc[o * 2] / nsub as f32; dsum[o * 2 + 1] += dacc[o * 2 + 1] / nsub as f32; } DISP.with(|d| *d.borrow_mut() = Some((w, hh, dsum))); } else { DISP.with(|d| *d.borrow_mut() = None); }
    for o in 0..w * hh { let i4 = o * 4; f.d[i4] += accc[o * 3] / nsub as f32; f.d[i4 + 1] += accc[o * 3 + 1] / nsub as f32; f.d[i4 + 2] += accc[o * 3 + 2] / nsub as f32; f.d[i4 + 3] += acca[o] / nsub as f32;
        let a = f.d[i4 + 3]; if a > 1e-5 { for j in 0..3 { f.d[i4 + j] = (f.d[i4 + j] / a).min(1.5); } } }
    if !shadow.is_empty() { for o in 0..w * hh { let sa = shadow[o].min(0.95); if sa <= 0.0 { continue; } let i4 = o * 4; let a = f.d[i4 + 3]; let aa = a + sa * (1.0 - a); for j in 0..3 { f.d[i4 + j] = f.d[i4 + j] * a / aa; } f.d[i4 + 3] = aa; } }
}

fn bake_cached(e: &ShardBurst, k: f64, total: f64, hh: usize) -> std::sync::Arc<crate::physics::Traj> {
    use std::sync::{Arc, Mutex}; use std::collections::HashMap; use std::sync::OnceLock;
    static C: OnceLock<Mutex<HashMap<String, Arc<crate::physics::Traj>>>> = OnceLock::new();
    let key = format!("{:?}|{}|{}|{}", serde_json::to_string(e).unwrap_or_default(), k, total, hh); let m = C.get_or_init(|| Mutex::new(HashMap::new()));
    if let Some(t) = m.lock().unwrap().get(&key) { return t.clone(); }
    let mut bodies = vec![];
    if let Some(p) = e.plane.as_ref() { // plane cells as rigid bodies: same birth wave and pressure impulse as the analytic path, then drag, gravity, spin and the floor
        let cells = plane_cells(p, e.seed, k); let (li, (ix, iy)) = { let (l, c) = plane_impact(p, k); (l, c) };
        for (i, c) in cells.iter().enumerate() { let i = i as u64; let r = |kk: u64| h(e.seed, i, kk); let d = ((c.lc.0 - li.0).powi(2) + (c.lc.1 - li.1).powi(2)).sqrt(); let birth = (d / (p.wave_speed * k)).min(p.max_delay);
            let size = (c.area.sqrt() / 1.8).max(1.0); let (dx, dy) = (c.c.0 - ix, c.c.1 - iy); let mg = (dx * dx + dy * dy).sqrt().max(1e-6); let (ux, uy) = if d < 1e-3 { let a = r(60) * std::f64::consts::TAU; (a.cos(), a.sin()) } else { (dx / mg, dy / mg) };
            let v0 = p.impulse * k * (-d / (p.radius * k * 3.0)).exp().max(0.12) * (0.6 + 0.8 * r(4)) / (0.7 + size / (40.0 * k)).sqrt(); let vz = -p.lift * v0 * (0.6 + 0.8 * r(5));
            let sd = e.spin.to_radians(); let thick = ({ let t = e.thickness.unwrap_or([0.3, 0.9]); t[0] + (t[1] - t[0]) * r(14) }) * size;
            bodies.push(crate::physics::Body { p: [c.c.0, c.c.1, p.under_depth * k], v: [ux * v0, uy * v0, vz], q: [1.0, 0.0, 0.0, 0.0], w: [(r(7) - 0.5) * 2.0 * sd, (r(9) - 0.5) * 2.0 * sd, (r(11) - 0.5) * sd], r: size * 0.55, m: size * size * thick * 1e-3 + 1e-3, birth }); }
        let par = crate::physics::Params { gravity: [0.0, e.gravity * k, 0.0], drag: e.drag, ang_drag: e.ang_drag, restitution: e.restitution, friction: e.friction, floor_y: e.floor.map(|f| f * k), collide: false };
        let tj = Arc::new(crate::physics::bake(bodies, &par, total + 0.1)); m.lock().unwrap().insert(key, tj.clone()); return tj; }
    for i in 0..e.count as u64 { let r = |kk: u64| h(e.seed, i, kk);
        let size = shard_look::size(e.size_min, e.size_max, e.size_bias, e.size_scale, r(1)) * k; let (th, ph) = (r(2) * std::f64::consts::TAU, (r(3) * 2.0 - 1.0).acos()); let dir = [ph.sin() * th.cos(), ph.sin() * th.sin() * 0.8, ph.cos()];
        let sp = e.speed * k * (0.35 + 1.1 * r(4).powf(1.2)) / (0.6 + size / (30.0 * k)).sqrt(); let mut vz = dir[2] * sp; vz -= e.toward_camera * sp * (0.3 + r(5)); let os = e.origin_spread * k;
        let p = [e.origin[0] * k + (r(50) - 0.5) * 2.0 * os, e.origin[1] * k + (r(51) - 0.5) * 2.0 * os * 0.6, e.origin[2] + (r(52) - 0.5) * 2.0 * os];
        let sd = e.spin.to_radians(); let w = [(r(60) - 0.5) * 2.0 * sd, (r(61) - 0.5) * 2.0 * sd, (r(62) - 0.5) * 2.0 * sd];
        let thick = ({ let t = e.thickness.unwrap_or([0.45, 1.05]); t[0] + (t[1] - t[0]) * r(14) }) * size; let m = size * size * thick * 1e-3 + 1e-3;
        bodies.push(crate::physics::Body { p, v: [dir[0] * sp, dir[1] * sp, vz], q: crate::physics::quat_euler([r(6) * 360.0, r(8) * 360.0, r(10) * 360.0]), w, r: size * 0.55, m, birth: r(40) * e.spread_time }); }
    let par = crate::physics::Params { gravity: [0.0, e.gravity * k, 0.0], drag: e.drag, ang_drag: e.ang_drag, restitution: e.restitution, friction: e.friction, floor_y: e.floor.map(|f| f * k), collide: e.collide };
    let tj = Arc::new(crate::physics::bake(bodies, &par, total + 0.1)); m.lock().unwrap().insert(key, tj.clone()); tj
}

thread_local! { static DISP: std::cell::RefCell<Option<(usize, usize, Vec<f32>)>> = std::cell::RefCell::new(None); }
/// Take the premultiplied screen-space refraction field written by the last shard render (if refract > 0).
pub fn take_disp() -> Option<(usize, usize, Vec<f32>)> { DISP.with(|d| d.borrow_mut().take()) }
/// Bend the canvas behind a layer by a premultiplied displacement field; `layer_a` is the layer alpha (field is divided by it).
pub fn refract_canvas(out: &mut Frame, disp: &(usize, usize, Vec<f32>), layer: &Frame, rdisp: f64) {
    let (w, hh) = (out.w, out.h); if disp.0 != w || disp.1 != hh { return; } let src = out.d.clone();
    for y in 0..hh { for x in 0..w { let o = y * w + x; let a = layer.d[o * 4 + 3]; if a < 0.003 { continue; } let lim = 28.0 * (hh as f32 / 720.0); let (dx, dy) = ((disp.2[o * 2] / a.max(0.05)).clamp(-lim, lim), (disp.2[o * 2 + 1] / a.max(0.05)).clamp(-lim, lim));
        let wgt = a.min(1.0); for j in 0..3 { let sc = 1.0 + rdisp as f32 * (j as f32 - 1.0); let (sx, sy) = ((x as f32 + dx * sc).clamp(0.0, w as f32 - 1.001), (y as f32 + dy * sc).clamp(0.0, hh as f32 - 1.001)); let (x0, y0) = (sx as usize, sy as usize); let (fx, fy) = (sx - x0 as f32, sy - y0 as f32); let v = |xx: usize, yy: usize| src[(yy * w + xx) * 4 + j]; let s = v(x0, y0) * (1.0 - fx) * (1.0 - fy) + v(x0 + 1, y0) * fx * (1.0 - fy) + v(x0, y0 + 1) * (1.0 - fx) * fy + v(x0 + 1, y0 + 1) * fx * fy; out.d[o * 4 + j] = src[o * 4 + j] * (1.0 - wgt) + s * wgt; } } }
}

struct Tex { w: usize, h: usize, d: Vec<f32>, mean: [f64; 3] }
impl Tex { fn sample(&self, u: f64, v: f64) -> [f64; 3] { let x = (u * self.w as f64 - 0.5).clamp(0.0, self.w as f64 - 1.001); let y = (v * self.h as f64 - 0.5).clamp(0.0, self.h as f64 - 1.001); let (x0, y0) = (x as usize, y as usize); let (fx, fy) = (x - x0 as f64, y - y0 as f64);
    let g = |xx: usize, yy: usize, c: usize| self.d[(yy * self.w + xx) * 3 + c] as f64; let mut o = [0.0; 3]; for c in 0..3 { o[c] = g(x0, y0, c) * (1.0 - fx) * (1.0 - fy) + g(x0 + 1, y0, c) * fx * (1.0 - fy) + g(x0, y0 + 1, c) * (1.0 - fx) * fy + g(x0 + 1, y0 + 1, c) * fx * fy; } o } }
thread_local! { static TEX: std::cell::RefCell<Option<(String, std::sync::Arc<Tex>)>> = std::cell::RefCell::new(None); }
/// Load an image as an sRGB-decoded linear float texture at the plane's UV size (via ffmpeg, bounded to 4096x4096).
/// Set by the renderer: true when frame buffers are scene-linear (colour pipeline on), false when display-referred 0..1 (default).
/// Texture cache key: depends on the decode flag so a linear and an sRGB render never share a decoded texture.
pub fn tex_key(ctx: &RenderCtx, path: &str, region: [f64; 2]) -> String { format!("{}|{}|{}|{}", path, region[0], region[1], ctx.work_linear) }
fn load_tex(ctx: &RenderCtx, path: &str, region: [f64; 2]) -> Option<std::sync::Arc<Tex>> {
    let wl = ctx.work_linear; let key = tex_key(ctx, path, region); if let Some((k, t)) = TEX.with(|c| c.borrow().clone()) { if k == key { return Some(t); } }
    let (w, hh) = ((region[0].round() as usize).clamp(8, 4096), (region[1].round() as usize).clamp(8, 4096));
    let out = std::process::Command::new("ffmpeg").args(["-v", "error", "-i", &crate::deps::resolve_str(path), "-vf", &format!("scale={}:{}", w, hh), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).output().ok()?;
    if out.stdout.len() != w * hh * 3 { return None; }
    let lin = |b: u8| { let c = b as f32 / 255.0; if !wl { c } else if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } }; let lut: Vec<f32> = (0..256).map(|i| lin(i as u8)).collect();
    let d: Vec<f32> = out.stdout.iter().map(|&b| lut[b as usize]).collect(); let mut mean = [0.0f64; 3]; for (i, v) in d.iter().enumerate() { mean[i % 3] += *v as f64; } for m in mean.iter_mut() { *m /= (w * hh) as f64; } let t = std::sync::Arc::new(Tex { w, h: hh, d, mean }); TEX.with(|c| *c.borrow_mut() = Some((key, t.clone()))); Some(t)
}
/// Affine screen->uv from the first three vertices of a face (cells are small, so perspective error inside a facet is negligible).
fn uv_affine(pts: &[(f64, f64); 6], uv: &[(f64, f64)]) -> Option<[f64; 6]> {
    let (a, b, c) = (pts[0], pts[1], pts[2]); let det = (b.0 - a.0) * (c.1 - a.1) - (c.0 - a.0) * (b.1 - a.1); if det.abs() < 1e-6 { return None; }
    let solve = |ua: f64, ub: f64, uc: f64| { let (p, q) = (ub - ua, uc - ua); let gx = (p * (c.1 - a.1) - q * (b.1 - a.1)) / det; let gy = (q * (b.0 - a.0) - p * (c.0 - a.0)) / det; [gx, gy, ua - gx * a.0 - gy * a.1] };
    let u = solve(uv[0].0, uv[1].0, uv[2].0); let v = solve(uv[0].1, uv[1].1, uv[2].1); Some([u[0], u[1], u[2], v[0], v[1], v[2]]) }

thread_local! { static OCC: std::cell::RefCell<Option<(String, std::sync::Arc<Vec<f32>>)>> = std::cell::RefCell::new(None); }
thread_local! { pub static TARGET_MATTE: std::cell::RefCell<std::collections::HashMap<String, (f64, std::sync::Arc<Vec<f32>>)>> = std::cell::RefCell::new(Default::default()); }
thread_local! { pub static EXT_OCC: std::cell::RefCell<Option<std::sync::Arc<Vec<f32>>>> = std::cell::RefCell::new(None); }
fn load_occ(pat: &str, frame: i64, w: usize, hh: usize) -> Option<std::sync::Arc<Vec<f32>>> {
    let path = if let Some(i) = pat.find('%') { let rest = &pat[i + 1..]; let digs: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect(); let wid: usize = digs.parse().unwrap_or(0); if rest[digs.len()..].starts_with('d') { format!("{}{:0wid$}{}", &pat[..i], frame.max(0), &rest[digs.len() + 1..], wid = wid) } else { pat.to_string() } } else { pat.to_string() };
    let key = format!("{}|{}|{}", path, w, hh); if let Some((k, m)) = OCC.with(|c| c.borrow().clone()) { if k == key { return Some(m); } }
    // alpha if present, else luma; sized to the canvas
    let out = std::process::Command::new("ffmpeg").args(["-v", "error", "-i", &crate::deps::resolve_str(&path), "-vf", &format!("scale={}:{},format=rgba,alphaextract", w, hh), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).output().ok()?;
    let rgbmode = std::process::Command::new("ffmpeg").args(["-v", "error", "-i", &path, "-vf", &format!("scale={}:{}", w, hh), "-frames:v", "1", "-f", "rawvideo", "-pix_fmt", "gray", "-"]).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::null()).output().ok()?;
    let a = if out.stdout.len() == w * hh { out.stdout } else { Vec::new() }; let l = if rgbmode.stdout.len() == w * hh { rgbmode.stdout } else { Vec::new() };
    let src = if !a.is_empty() && a.iter().any(|&v| v < 250) { a } else { l }; if src.len() != w * hh { return None; }
    let m = std::sync::Arc::new(src.iter().map(|&v| v as f32 / 255.0).collect::<Vec<f32>>()); OCC.with(|c| *c.borrow_mut() = Some((key, m.clone()))); Some(m)
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct ProbeShard { pub id: u64, pub cx: f64, pub cy: f64, pub bbox: [f64; 4], pub depth: f64, pub alpha: f64 }
thread_local! { static PROBE: std::cell::RefCell<Vec<ProbeShard>> = std::cell::RefCell::new(Vec::new()); }
/// Per-shard screen-space centre, bounding box, depth and alpha of the last shard render (front face, last motion sub-sample). Pixels, canvas space.
pub fn take_probe() -> Vec<ProbeShard> { PROBE.with(|p| std::mem::take(&mut *p.borrow_mut())) }

/// Depth sink of a not-yet-released cell: ramps over `preload_time`, strongest at the impact, held until the cell's birth.
fn pre_dep(p: &Plane, d: f64, k: f64, ts: f64, birth: f64) -> f64 { if p.preload_depth <= 0.0 || ts >= birth { return 0.0; } let r = sstep(ts / p.preload_time.max(1e-3)); p.preload_depth * k * r * (-(d / (p.radius * k * 5.0)).powi(2)).exp() }
fn refd_zero(v: &f64) -> bool { *v == 0.0 }
