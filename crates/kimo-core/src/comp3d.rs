//! 3D compositor (After Effects "Classic 3D" class) for kimo's 2.5D layers. Opt-in: project field `comp3d` (tool set_3d_compositor);
//! absent or `enabled: false` leaves every render byte-identical, because the renderer then never builds a group.
//!
//! Geometry (three_d.rs): world x, y in px from the frame centre (y down), +z away from the camera; the camera centre is
//! c = (cam_x, cam_y, cam_z - f) with f = (H / 2) / tan(cam_fov / 2) px, so a layer at pos_z = 0 lies at camera depth z = f.
//! The eye ray of screen pixel (x, y) is d = Rc (x - W/2, y - H/2, f); it hits a layer plane at c + t d, camera depth z = t f.
//!
//! Groups: consecutive 3D layers in the stack (a clip with rot_x, rot_y or pos_z non-zero, the test render.rs uses) form one
//! group; any 2D layer, adjustment layer, transition dip, backdrop-reading effect or a 3D layer the compositor cannot take
//! (post effects such as frosted_glass, transition overlays) ends the group first, so 2D layers keep their stack order.
//! Each member is rasterised exactly as before (three_d::render_layer, 2x2 supersampled coverage in alpha) and keeps its
//! Projector. Per pixel the members' fragments are sorted back to front by z at the pixel centre (stable: equal depths keep
//! stack order) and blended with each layer's own blend mode and opacity using render.rs blend_region's per-pixel arithmetic.
//! Two crossing planes therefore swap order exactly along their intersection line (1/z is affine in screen space for a
//! plane, so the line is where 1/z_A = 1/z_B). A group of one layer without shadows/DOF is bit-identical to the 2D order.
//!
//! Shadows (analytic, no shadow map): for a receiver pixel the world point P is the Projector hit. For each light sample
//! position L_k the segment P -> L_k (point/spot, parameter u in (0, 1)) or the ray P -> light (directional, u > 0) is
//! intersected with every other casting layer's plane (Projector::ray_hit); the caster's source alpha at the hit (bilinear,
//! render_layer's sampling) times its opacity is a, and transmittance T_k = prod (1 - a). vis = mean_k T_k. Lights (project
//! lights, `light_id` or every enabled one) map from the lighting.rs convention (x, y fractions of the frame, z frame-heights
//! toward the viewer) to world px: X = (x - 0.5) W, Y = (y - 0.5) H, Z = -z H; dir (dx, dy, dz) -> normalize(dx, dy, -dz).
//! Darkening D = opacity * sum_l I_l cone_l(P) (1 - vis_l) / sum_l I_l (I = intensity, cone_l = spot cone smoothstep of
//! lighting::light_at, 1 for point/directional); receiver colour c' = c (1 - D) + shadow_color D, alpha unchanged.
//! Softness s = light radius in world px (directional: a disc of radius s at distance H along the light). s = 0: one exact ray
//! per 2x2 subsample (anti-aliased hard edge); s > 0: `samples` rays per pixel centre to a Vogel (golden-angle) disc
//! r_k = s sqrt((k + 0.5) / n), theta_k = k * 2.39996 rad, perpendicular to the light axis (PCF over the light area).
//! Per-layer props: casts_shadows (default 0, AE "Casts Shadows: Off"), accepts_shadows (default 1, AE "Accepts Shadows: On").
//!
//! Depth of field (thin lens): aperture A = lens diameter in world px, focus distance z_f (camera depth px, 0 = f, the z = 0
//! plane). A point at depth z images to a blur circle of diameter, in screen px,
//!     CoC(z) = A f |1/z_f - 1/z|      (= A |z - z_f| / z when z_f = f, AE's "Aperture in px" form),
//! linear in |1/z_f - 1/z|, capped at max_blur_px. Each layer is blurred before depth compositing by a variable-radius gather:
//! out(p) = mean_k premul(layer)(p + r(p) o_k), r(p) = CoC(z(p)) / 2 with z(p) the plane depth at p, o_k the unit Vogel disc,
//! bilinear premultiplied taps (outside the frame = transparent), weights 1/n. Pixels with r <= 1e-3 px are copied, so a
//! layer at the focus distance is bit-identical to no DOF.
use crate::effects::Frame;
use crate::lighting::LightDef;
use crate::model::Project;
use crate::three_d::Projector;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Upper bound for dof.samples and shadows.samples.
pub const MAX_SAMPLES: u32 = 256;
const GOLDEN: f64 = 2.399963229728653; // pi (3 - sqrt 5)

fn d_blur() -> f64 { 64.0 }
fn d_dof_n() -> u32 { 32 }
fn d_sh_n() -> u32 { 16 }
fn d_one() -> f64 { 1.0 }

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct Comp3d { #[serde(default)] pub enabled: bool, #[serde(default)] pub dof: Dof, #[serde(default)] pub shadows: Shadows }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Dof {
    #[serde(default)] pub enabled: bool,
    /// Camera depth of the sharp plane in px; 0 = the focal length f (the z = 0 plane under an undollied camera).
    #[serde(default)] pub focus_distance: f64,
    /// Lens diameter in world px (CoC = A f |1/z_f - 1/z| screen px).
    #[serde(default)] pub aperture: f64,
    /// Cap on the CoC diameter in px.
    #[serde(default = "d_blur")] pub max_blur_px: f64,
    #[serde(default = "d_dof_n")] pub samples: u32,
}
impl Default for Dof { fn default() -> Self { Dof { enabled: false, focus_distance: 0.0, aperture: 0.0, max_blur_px: d_blur(), samples: d_dof_n() } } }

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Shadows {
    #[serde(default)] pub enabled: bool,
    /// One project light; None = every enabled point/spot/directional light.
    #[serde(default, skip_serializing_if = "Option::is_none")] pub light_id: Option<String>,
    /// Light radius in world px (0 = hard, exact).
    #[serde(default)] pub softness: f64,
    #[serde(default = "d_sh_n")] pub samples: u32,
    #[serde(default)] pub color: [f64; 3],
    #[serde(default = "d_one")] pub opacity: f64,
}
impl Default for Shadows { fn default() -> Self { Shadows { enabled: false, light_id: None, softness: 0.0, samples: d_sh_n(), color: [0.0; 3], opacity: 1.0 } } }

impl Comp3d {
    pub fn validate(&self, lights: &BTreeMap<String, LightDef>) -> Result<(), String> {
        let d = &self.dof; let s = &self.shadows;
        let fin = |v: f64, k: &str| if v.is_finite() { Ok(()) } else { Err(format!("{k} must be finite")) };
        fin(d.focus_distance, "dof.focus_distance")?; fin(d.aperture, "dof.aperture")?; fin(d.max_blur_px, "dof.max_blur_px")?; fin(s.softness, "shadows.softness")?; fin(s.opacity, "shadows.opacity")?;
        if d.focus_distance < 0.0 { return Err("dof.focus_distance must be >= 0 (camera depth in px; 0 = focal length)".into()); }
        if !(0.0..=10000.0).contains(&d.aperture) { return Err(format!("dof.aperture must be in 0..10000 (lens diameter in world px), got {}", d.aperture)); }
        if !(0.0..=512.0).contains(&d.max_blur_px) { return Err("dof.max_blur_px must be in 0..512".into()); }
        if d.samples < 1 || d.samples > MAX_SAMPLES { return Err(format!("dof.samples must be 1..{MAX_SAMPLES}, got {}", d.samples)); }
        if s.samples < 1 || s.samples > MAX_SAMPLES { return Err(format!("shadows.samples must be 1..{MAX_SAMPLES}, got {}", s.samples)); }
        if !(0.0..=10000.0).contains(&s.softness) { return Err("shadows.softness must be in 0..10000 (light radius in world px)".into()); }
        if !(0.0..=1.0).contains(&s.opacity) { return Err("shadows.opacity must be in 0..1".into()); }
        if s.color.iter().any(|c| !c.is_finite() || *c < 0.0) { return Err("shadows.color must be finite and >= 0".into()); }
        if let Some(id) = &s.light_id { let l = lights.get(id).ok_or(format!("unknown light '{id}' (define it with set_light)"))?; if !["point", "spot", "directional"].contains(&l.kind.as_str()) { return Err(format!("light '{id}' has kind '{}'; shadows need point, spot or directional", l.kind)); } }
        Ok(())
    }
}

/// Circle-of-confusion diameter in screen px of a point at camera depth z: A f |1/z_f - 1/z|, capped at max_blur_px (0 for z <= 0).
pub fn coc_px(d: &Dof, f_px: f64, z: f64) -> f64 {
    if !z.is_finite() || z <= 0.0 { return 0.0; }
    let zf = if d.focus_distance > 0.0 { d.focus_distance } else { f_px };
    (d.aperture * f_px * (1.0 / zf - 1.0 / z).abs()).min(d.max_blur_px)
}

/// Light position in world px from the lighting.rs convention (x, y fractions of the frame, z frame-heights toward the viewer).
pub fn light_world(pos: [f64; 3], w: f64, h: f64) -> [f64; 3] { [(pos[0] - 0.5) * w, (pos[1] - 0.5) * h, -pos[2] * h] }

pub(crate) fn active(p: &Project) -> bool { p.comp3d.as_ref().map_or(false, |c| c.enabled) }

/// One 3D layer of a group, captured in stack order by the renderer.
pub(crate) struct Layer { img: Frame, src: Option<Frame>, pj: Projector, op: f32, mode: u8, mask: Option<Vec<f32>>, casts: bool, accepts: bool }
impl Layer {
    /// `img` = render_layer output (straight colour), `src` = the layer's source frame before projection (kept only for
    /// shadow casters), `mask` = the transition mask blend() would apply, evaluated once per pixel.
    pub(crate) fn new(p: &Project, img: Frame, src: Frame, pj: Projector, op: f32, blend: &str, mask: Option<&dyn Fn(usize, usize) -> f32>, casts: bool, accepts: bool) -> Layer {
        let keep = casts && p.comp3d.as_ref().map_or(false, |c| c.shadows.enabled); let (w, h) = (img.w, img.h);
        let mask = mask.map(|m| { let mut v = vec![1f32; w * h]; for y in 0..h { for x in 0..w { if img.d[(y * w + x) * 4 + 3] > 0.0 { v[y * w + x] = m(x, y); } } } v });
        let mode = match blend { "add" => 1, "multiply" => 2, "screen" => 3, "overlay" => 4, _ => 0 };
        Layer { img, src: if keep { Some(src) } else { None }, pj, op, mode, mask, casts, accepts }
    }
}

/// Composite and clear the pending group (no-op when empty).
pub(crate) fn flush(out: &mut Frame, g: &mut Vec<Layer>, p: &Project, t: f64) {
    if g.is_empty() { return; }
    let mut ls = std::mem::take(g); let cfg = p.comp3d.clone().unwrap_or_default();
    if cfg.shadows.enabled { let lights = world_lights(&cfg.shadows, &p.lights, t, out.w as f64, out.h as f64); if !lights.is_empty() { shade(&mut ls, &lights, &cfg.shadows); } }
    if cfg.dof.enabled && cfg.dof.aperture > 0.0 { for l in ls.iter_mut() { defocus(l, &cfg.dof); } }
    composite(out, &ls);
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[0] - b[0], a[1] - b[1], a[2] - b[2]] }
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] { [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]] }
fn norm(v: [f64; 3]) -> [f64; 3] { let m = dot(v, v).sqrt().max(1e-12); [v[0] / m, v[1] / m, v[2] / m] }
/// Two unit vectors spanning the plane perpendicular to unit `a`.
fn basis(a: [f64; 3]) -> ([f64; 3], [f64; 3]) { let t = if a[0].abs() < 0.9 { [1.0, 0.0, 0.0] } else { [0.0, 1.0, 0.0] }; let e1 = norm(cross(a, t)); (e1, cross(a, e1)) }
/// Deterministic Vogel disc of n points in the unit disc (n = 1: the centre).
fn disc(n: usize) -> Vec<(f64, f64)> { if n <= 1 { return vec![(0.0, 0.0)]; } (0..n).map(|k| { let r = ((k as f64 + 0.5) / n as f64).sqrt(); let a = k as f64 * GOLDEN; (r * a.cos(), r * a.sin()) }).collect() }

/// kind 0 point, 1 spot, 2 directional; dir = unit travel direction; weight = intensity.
struct WLight { kind: u8, pos: [f64; 3], dir: [f64; 3], cone: f64, soft: f64, weight: f64 }
fn world_lights(s: &Shadows, lights: &BTreeMap<String, LightDef>, t: f64, w: f64, h: f64) -> Vec<WLight> {
    lights.iter().filter(|(k, l)| l.enabled && s.light_id.as_ref().map_or(true, |id| id == *k)).filter_map(|(_, l)| { let l = crate::lighting::eval_light(l, t);
        let kind = match l.kind.as_str() { "point" => 0, "spot" => 1, "directional" => 2, _ => return None };
        Some(WLight { kind, pos: light_world(l.pos, w, h), dir: norm([l.dir[0], l.dir[1], -l.dir[2]]), cone: l.cone, soft: l.softness, weight: l.intensity.max(0.0) }) }).collect()
}
/// Spot cone factor at world point q (lighting::light_at's smoothstep between cone (1 - softness) and cone, half-angles in degrees).
fn spot_factor(l: &WLight, q: [f64; 3]) -> f64 {
    let c = dot(norm(sub(q, l.pos)), l.dir).clamp(-1.0, 1.0).acos().to_degrees(); let inner = l.cone * (1.0 - l.soft);
    if c <= inner { 1.0 } else if c >= l.cone { 0.0 } else { let u = (l.cone - c) / (l.cone - inner).max(1e-6); u * u * (3.0 - 2.0 * u) }
}
/// Straight source alpha at source coords (render_layer's bilinear taps); 0 outside the source frame.
fn alpha_at(src: &Frame, sx: f64, sy: f64) -> f32 {
    let (w, h) = (src.w, src.h); if sx < 0.0 || sy < 0.0 || sx >= w as f64 || sy >= h as f64 { return 0.0; }
    let (fx, fy) = (sx - 0.5, sy - 0.5); let (x0, y0) = (fx.floor(), fy.floor()); let (ax, ay) = ((fx - x0) as f32, (fy - y0) as f32); let (x0, y0) = (x0 as i64, y0 as i64);
    let at = |dx: i64, dy: i64| { let (px, py) = ((x0 + dx).clamp(0, w as i64 - 1) as usize, (y0 + dy).clamp(0, h as i64 - 1) as usize); src.d[(py * w + px) * 4 + 3] };
    at(0, 0) * (1.0 - ax) * (1.0 - ay) + at(1, 0) * ax * (1.0 - ay) + at(0, 1) * (1.0 - ax) * ay + at(1, 1) * ax * ay
}
/// A caster: its plane, source frame and opacity.
type Caster<'a> = (&'a Projector, &'a Frame, f32);
/// sum_l I_l cone_l (1 - vis_l) at world point q (the caller divides by sum_l I_l and scales by the shadow opacity).
fn occlusion(q: [f64; 3], lights: &[WLight], taps: &[(f64, f64)], soft: f64, occ: &[Caster], hf: f64) -> f64 {
    let mut sum = 0.0;
    for l in lights {
        if l.weight <= 0.0 { continue; } let cone = if l.kind == 1 { spot_factor(l, q) } else { 1.0 }; if cone <= 0.0 { continue; }
        let (axis, umax) = if l.kind == 2 { ([-l.dir[0] * hf, -l.dir[1] * hf, -l.dir[2] * hf], f64::INFINITY) } else { (sub(l.pos, q), 1.0) };
        let (e1, e2) = basis(norm(axis)); let mut vis = 0.0;
        for &(a, b) in taps {
            let d = [axis[0] + soft * (a * e1[0] + b * e2[0]), axis[1] + soft * (a * e1[1] + b * e2[1]), axis[2] + soft * (a * e1[2] + b * e2[2])];
            let mut tr = 1.0f64;
            for &(pj, src, op) in occ { if let Some((u, sx, sy)) = pj.ray_hit(q, d) { if u > 1e-9 && u < umax { let a = (alpha_at(src, sx, sy) * op) as f64; if a > 0.0 { tr *= 1.0 - a.min(1.0); if tr <= 0.0 { break; } } } } }
            vis += tr;
        }
        sum += l.weight * cone * (1.0 - vis / taps.len() as f64);
    }
    sum
}
fn shade(ls: &mut [Layer], lights: &[WLight], s: &Shadows) {
    let wsum: f64 = lights.iter().map(|l| l.weight).sum(); if wsum <= 0.0 { return; }
    let hard = s.softness <= 0.0; let taps = disc(if hard { 1 } else { s.samples.max(1) as usize });
    let subs: &[(f64, f64)] = if hard { &[(0.25, 0.25), (0.75, 0.25), (0.25, 0.75), (0.75, 0.75)] } else { &[(0.5, 0.5)] };
    let col = [s.color[0] as f32, s.color[1] as f32, s.color[2] as f32]; let op = s.opacity.clamp(0.0, 1.0);
    for i in 0..ls.len() {
        if !ls[i].accepts { continue; }
        let (w, h) = (ls[i].img.w, ls[i].img.h);
        let dk = { let occ: Vec<Caster> = (0..ls.len()).filter(|&j| j != i && ls[j].casts).filter_map(|j| ls[j].src.as_ref().map(|f| (&ls[j].pj, f, ls[j].op))).collect();
            if occ.is_empty() { continue; }
            let (img, pj) = (&ls[i].img, &ls[i].pj); let mut dk = vec![0f32; w * h];
            crate::par::rows_mut(&mut dk, w, |y, row| { for x in 0..w { if img.d[(y * w + x) * 4 + 3] <= 0.0 { continue; } let mut acc = 0.0; let mut n = 0.0;
                for &(ox, oy) in subs { if let Some(q) = pj.world_at(x as f64 + ox, y as f64 + oy) { acc += occlusion(q, lights, &taps, s.softness, &occ, h as f64); n += 1.0; } }
                if n > 0.0 { row[x] = (op * acc / (n * wsum)).clamp(0.0, 1.0) as f32; } } });
            dk };
        let img = &mut ls[i].img;
        for k in 0..w * h { let d = dk[k]; if d > 0.0 { for j in 0..3 { img.d[k * 4 + j] = img.d[k * 4 + j] * (1.0 - d) + col[j] * d; } } }
    }
}

/// Premultiplied bilinear sample at continuous coords (pixel centres at +0.5); taps outside the frame are transparent.
fn premul_at(f: &Frame, x: f64, y: f64) -> [f32; 4] {
    let (fx, fy) = (x - 0.5, y - 0.5); let (x0, y0) = (fx.floor(), fy.floor()); let (ax, ay) = ((fx - x0) as f32, (fy - y0) as f32); let (x0, y0) = (x0 as i64, y0 as i64);
    let mut acc = [0f32; 4];
    for (dx, dy, wt) in [(0, 0, (1.0 - ax) * (1.0 - ay)), (1, 0, ax * (1.0 - ay)), (0, 1, (1.0 - ax) * ay), (1, 1, ax * ay)] {
        let (px, py) = (x0 + dx, y0 + dy); if px < 0 || py < 0 || px >= f.w as i64 || py >= f.h as i64 { continue; }
        let i = (py as usize * f.w + px as usize) * 4; let a = f.d[i + 3] * wt; acc[0] += f.d[i] * a; acc[1] += f.d[i + 1] * a; acc[2] += f.d[i + 2] * a; acc[3] += a; }
    acc
}
fn defocus(l: &mut Layer, d: &Dof) {
    let (w, h) = (l.img.w, l.img.h); let f_px = l.pj.focal();
    // content bounds grown by the largest possible blur radius: nothing outside can receive colour
    let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for y in 0..h { for x in 0..w { if l.img.d[(y * w + x) * 4 + 3] > 0.0 { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y); } } }
    if x0 == usize::MAX { return; }
    let g = (d.max_blur_px * 0.5).ceil() as usize + 2; let (x0, y0, x1, y1) = (x0.saturating_sub(g), y0.saturating_sub(g), (x1 + g).min(w - 1), (y1 + g).min(h - 1));
    let mut rad = vec![0f32; w * h]; let mut any = false;
    for y in y0..=y1 { for x in x0..=x1 { let r = l.pj.depth_at(x as f64 + 0.5, y as f64 + 0.5).map_or(0.0, |z| coc_px(d, f_px, z) * 0.5); if r > 1e-3 { rad[y * w + x] = r as f32; any = true; } } }
    if !any { return; }
    let taps = disc(d.samples.max(1) as usize); let n = taps.len() as f32; let src = &l.img; let mut nd = src.d.clone();
    crate::par::rows_mut(&mut nd, w * 4, |y, row| { if y < y0 || y > y1 { return; } for x in x0..=x1 { let r = rad[y * w + x] as f64; if r <= 0.0 { continue; }
        let mut acc = [0f32; 4]; for &(u, v) in taps.iter() { let s = premul_at(src, x as f64 + 0.5 + r * u, y as f64 + 0.5 + r * v); for j in 0..4 { acc[j] += s[j]; } }
        let o = x * 4; if acc[3] > 0.0 { for j in 0..3 { row[o + j] = acc[j] / acc[3]; } row[o + 3] = acc[3] / n; } else { for j in 0..4 { row[o + j] = 0.0; } } } });
    l.img.d = nd;
}

fn composite(out: &mut Frame, ls: &[Layer]) {
    let (w, h) = (out.w, out.h);
    let depth: Vec<Vec<f64>> = ls.iter().map(|l| { let mut z = vec![f64::INFINITY; w * h];
        for y in 0..h { for x in 0..w { let i = y * w + x; if l.img.d[i * 4 + 3] > 0.0 { if let Some(d) = l.pj.depth_at(x as f64 + 0.5, y as f64 + 0.5) { z[i] = d; } } } } z }).collect();
    crate::par::rows_mut(&mut out.d, w * 4, |y, row| { let mut fr: Vec<(f64, usize, f32)> = Vec::with_capacity(ls.len());
        for x in 0..w { let i = y * w + x; fr.clear();
            for (k, l) in ls.iter().enumerate() { let a = l.img.d[i * 4 + 3] * l.op * l.mask.as_ref().map_or(1.0, |m| m[i]); if a > 0.0 { fr.push((depth[k][i], k, a)); } }
            if fr.is_empty() { continue; }
            // back to front; sort_by is stable, so equal depths keep stack order (later = on top)
            fr.sort_by(|p, q| q.0.partial_cmp(&p.0).unwrap_or(std::cmp::Ordering::Equal));
            let o = x * 4;
            for &(_, k, a) in fr.iter() { let s = &ls[k].img.d[i * 4..i * 4 + 4]; let m = ls[k].mode;
                // render.rs blend_region, per pixel
                for j in 0..3 { let (b, sv) = (row[o + j], s[j]); let r = match m { 1 => b + sv, 2 => b * sv, 3 => 1.0 - (1.0 - b) * (1.0 - sv), 4 => if b < 0.5 { 2.0 * b * sv } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - sv) }, _ => sv }; row[o + j] = b * (1.0 - a) + r * a; }
                row[o + 3] = row[o + 3] + a * (1.0 - row[o + 3]); } } });
}
