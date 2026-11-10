//! Shared light and material model. One set of project-level lights and named PBR materials that any effect can read, so
//! layers, text, shards and 3D scenes are lit by the same lights instead of each effect inventing its own fake highlight.
//! `relight` is the first consumer: it derives a normal field from a 2D layer (alpha and/or luma height) and shades it with
//! GGX specular, Smith geometry, Schlick fresnel, a clearcoat lobe and soft height-field self shadows.
//! Coordinates: light x,y are fractions of the frame (0..1, y down); z is height above the layer in frame-heights.
use crate::effects::{p, Frame, Params};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

fn d1() -> f64 { 1.0 } fn dw() -> [f64; 3] { [1.0; 3] } fn dpos() -> [f64; 3] { [0.5, 0.2, 0.6] } fn dkind() -> String { "point".into() } fn dcone() -> f64 { 45.0 } fn dsoft() -> f64 { 0.3 } fn dr() -> f64 { 0.5 } fn dior() -> f64 { 1.5 } fn dtrue() -> bool { true }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightKey { pub t: f64, #[serde(default)] pub pos: Option<[f64; 3]>, #[serde(default)] pub intensity: Option<f64> }
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LightDef {
    #[serde(default = "dkind")] pub kind: String, // directional | point | spot
    #[serde(default = "dpos")] pub pos: [f64; 3],
    /// Direction the light travels (directional and spot). For a point light this is ignored.
    #[serde(default)] pub dir: [f64; 3],
    #[serde(default = "dw")] pub color: [f64; 3], #[serde(default = "d1")] pub intensity: f64,
    /// Distance falloff: 0 = none, 1 = inverse-square from 1 frame-height away.
    #[serde(default)] pub falloff: f64,
    #[serde(default = "dcone")] pub cone: f64, #[serde(default = "dsoft")] pub softness: f64,
    #[serde(default = "dtrue")] pub enabled: bool, #[serde(default)] pub keys: Vec<LightKey>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Material {
    /// None = use the layer's own colour as albedo.
    #[serde(default)] pub albedo: Option<[f64; 3]>, #[serde(default = "dr")] pub roughness: f64, #[serde(default)] pub metal: f64,
    #[serde(default = "dior")] pub ior: f64, #[serde(default)] pub clearcoat: f64, #[serde(default)] pub emissive: [f64; 3],
}
impl Default for Material { fn default() -> Self { Material { albedo: None, roughness: 0.5, metal: 0.0, ior: 1.5, clearcoat: 0.0, emissive: [0.0; 3] } } }
pub fn presets() -> Vec<(&'static str, Material)> {
    let m = |a: Option<[f64; 3]>, r, me, ior, cc| Material { albedo: a, roughness: r, metal: me, ior, clearcoat: cc, emissive: [0.0; 3] };
    vec![("plastic", m(None, 0.35, 0.0, 1.5, 0.0)), ("rubber", m(None, 0.9, 0.0, 1.4, 0.0)), ("brushed_metal", m(None, 0.38, 1.0, 1.5, 0.0)), ("chrome", m(Some([0.95; 3]), 0.06, 1.0, 1.5, 0.0)),
        ("gold", m(Some([1.0, 0.77, 0.34]), 0.25, 1.0, 1.5, 0.0)), ("glass", m(Some([0.9, 0.95, 1.0]), 0.04, 0.0, 1.52, 0.0)), ("concrete", m(Some([0.55; 3]), 0.85, 0.0, 1.5, 0.0)),
        ("wet_stone", m(Some([0.3, 0.3, 0.32]), 0.7, 0.0, 1.5, 1.0)), ("car_paint", m(None, 0.5, 0.0, 1.5, 1.0)), ("skin", m(None, 0.55, 0.0, 1.4, 0.0))]
}
pub fn lookup(mats: &BTreeMap<String, Material>, name: &str) -> Option<Material> { mats.get(name).cloned().or_else(|| presets().into_iter().find(|(n, _)| *n == name).map(|(_, m)| m)) }

/// Light state evaluated at one time, shared with effects through a thread-local for the duration of a layer's effects.
#[derive(Clone, Debug, Default)]
pub struct Scene { pub lights: Vec<(String, LightDef)>, pub materials: BTreeMap<String, Material> }
thread_local! { static SCENE: RefCell<Option<Rc<Scene>>> = Default::default(); }
pub fn scene() -> Option<Rc<Scene>> { SCENE.with(|s| s.borrow().clone()) }
pub fn set_scene(s: Option<Rc<Scene>>) { SCENE.with(|c| *c.borrow_mut() = s); }
pub fn eval_light(l: &LightDef, t: f64) -> LightDef {
    let mut o = l.clone(); if l.keys.is_empty() { return o; }
    let mut ks: Vec<&LightKey> = l.keys.iter().collect(); ks.sort_by(|a, b| a.t.partial_cmp(&b.t).unwrap());
    let (mut pa, mut ia) = (l.pos, l.intensity); let mut prev: Option<(f64, [f64; 3], f64)> = None;
    for k in &ks { let kp = k.pos.unwrap_or(pa); let ki = k.intensity.unwrap_or(ia);
        if t <= k.t { match prev { Some((pt, pp, pi)) if k.t > pt => { let u = ((t - pt) / (k.t - pt)).clamp(0.0, 1.0); let u = u * u * (3.0 - 2.0 * u); for j in 0..3 { o.pos[j] = pp[j] + (kp[j] - pp[j]) * u; } o.intensity = pi + (ki - pi) * u; } _ => { o.pos = kp; o.intensity = ki; } } return o; }
        prev = Some((k.t, kp, ki)); pa = kp; ia = ki; }
    if let Some((_, pp, pi)) = prev { o.pos = pp; o.intensity = pi; } o
}
pub fn build_scene(lights: &BTreeMap<String, LightDef>, mats: &BTreeMap<String, Material>, t: f64) -> Scene {
    Scene { lights: lights.iter().filter(|(_, l)| l.enabled).map(|(k, l)| (k.clone(), eval_light(l, t))).collect(), materials: mats.clone() }
}
pub fn validate_light(l: &LightDef) -> Result<(), String> {
    if !["directional", "point", "spot"].contains(&l.kind.as_str()) { return Err(format!("kind must be directional|point|spot, got '{}'", l.kind)); }
    let fin = |v: &[f64]| v.iter().all(|x| x.is_finite());
    if !fin(&l.pos) || !fin(&l.dir) || !fin(&l.color) || !l.intensity.is_finite() || !l.falloff.is_finite() || !l.cone.is_finite() || !l.softness.is_finite() { return Err("light values must be finite".into()); }
    if l.intensity < 0.0 || l.intensity > 1000.0 { return Err("intensity must be in 0..1000".into()); }
    if !(0.0..=100.0).contains(&l.falloff) { return Err("falloff must be in 0..100".into()); }
    if !(1.0..=179.0).contains(&l.cone) { return Err("cone must be in 1..179 degrees".into()); }
    if !(0.0..=1.0).contains(&l.softness) { return Err("softness must be in 0..1".into()); }
    if l.kind != "point" && l.dir.iter().all(|x| x.abs() < 1e-9) { return Err("directional and spot lights need a non-zero dir".into()); }
    if l.color.iter().any(|c| *c < 0.0) { return Err("color must be >= 0".into()); }
    Ok(())
}
pub fn validate_material(m: &Material) -> Result<(), String> {
    if !m.roughness.is_finite() || !(0.0..=1.0).contains(&m.roughness) { return Err("roughness must be in 0..1".into()); }
    if !m.metal.is_finite() || !(0.0..=1.0).contains(&m.metal) { return Err("metal must be in 0..1".into()); }
    if !m.ior.is_finite() || !(1.0..=4.0).contains(&m.ior) { return Err("ior must be in 1..4".into()); }
    if !m.clearcoat.is_finite() || !(0.0..=1.0).contains(&m.clearcoat) { return Err("clearcoat must be in 0..1".into()); }
    if m.albedo.map_or(false, |a| a.iter().any(|x| !x.is_finite() || *x < 0.0 || *x > 4.0)) || m.emissive.iter().any(|x| !x.is_finite() || *x < 0.0) { return Err("albedo/emissive must be finite and >= 0".into()); }
    Ok(())
}

fn norm(v: [f64; 3]) -> [f64; 3] { let m = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-12); [v[0] / m, v[1] / m, v[2] / m] }
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 { a[0] * b[0] + a[1] * b[1] + a[2] * b[2] }

/// Cook-Torrance BRDF value times n.l for one light. Returns (diffuse weight, specular weight); the caller multiplies diffuse by albedo.
pub fn brdf(n: [f64; 3], l: [f64; 3], v: [f64; 3], m: &Material, albedo: [f64; 3]) -> ([f64; 3], [f64; 3]) {
    let ndl = dot(n, l).max(0.0); if ndl <= 0.0 { return ([0.0; 3], [0.0; 3]); }
    let h = norm([l[0] + v[0], l[1] + v[1], l[2] + v[2]]); let ndh = dot(n, h).max(0.0); let ndv = dot(n, v).max(1e-3); let vdh = dot(v, h).max(0.0);
    let lobe = |rough: f64| { let a = (rough * rough).max(1e-4); let a2 = a * a; let d = a2 / (std::f64::consts::PI * (ndh * ndh * (a2 - 1.0) + 1.0).powi(2)); let k = (rough + 1.0).powi(2) / 8.0; let g = (ndl / (ndl * (1.0 - k) + k)) * (ndv / (ndv * (1.0 - k) + k)); (d, g) };
    let f0d = ((m.ior - 1.0) / (m.ior + 1.0)).powi(2); let fw = (1.0 - vdh).powi(5);
    let (d, g) = lobe(m.roughness); let mut spec = [0.0; 3]; let mut diff = [0.0; 3];
    for j in 0..3 { let f0 = f0d * (1.0 - m.metal) + albedo[j] * m.metal; let f = f0 + (1.0 - f0) * fw; spec[j] = d * g * f / (4.0 * ndv);
        diff[j] = albedo[j] * (1.0 - m.metal) * (1.0 - f) * ndl; }
    if m.clearcoat > 0.0 { let (d2, g2) = lobe(0.06); let f = 0.04 + 0.96 * fw; let c = m.clearcoat * d2 * g2 * f / (4.0 * ndv); for j in 0..3 { spec[j] += c; diff[j] *= 1.0 - m.clearcoat * f; } }
    (diff, spec)
}

fn blur_plane(a: &mut Vec<f32>, w: usize, h: usize, r: usize) {
    if r == 0 { return; } let r = r.min(64); let mut t = vec![0f32; w * h];
    for _ in 0..3 { for y in 0..h { let row = &a[y * w..(y + 1) * w]; let mut acc = 0f32; let mut cnt = 0f32; for x in 0..r.min(w) { acc += row[x]; cnt += 1.0; }
            for x in 0..w { if x + r < w { acc += row[x + r]; cnt += 1.0; } if x > r { acc -= row[x - r - 1]; cnt -= 1.0; } t[y * w + x] = acc / cnt; } }
        for x in 0..w { let mut acc = 0f32; let mut cnt = 0f32; for y in 0..r.min(h) { acc += t[y * w + x]; cnt += 1.0; }
            for y in 0..h { if y + r < h { acc += t[(y + r) * w + x]; cnt += 1.0; } if y > r { acc -= t[(y - r - 1) * w + x]; cnt -= 1.0; } a[y * w + x] = acc / cnt; } } }
}

/// Lighting vector, attenuation and cone factor for pixel position `pt` (frame-height units, z = surface height).
pub fn light_at(l: &LightDef, pt: [f64; 3]) -> ([f64; 3], f64) {
    match l.kind.as_str() {
        "directional" => { let d = norm(l.dir); ([-d[0], -d[1], -d[2]], 1.0) }
        _ => { let v = [l.pos[0] - pt[0], l.pos[1] - pt[1], l.pos[2] - pt[2]]; let dist = dot(v, v).sqrt().max(1e-6); let ln = [v[0] / dist, v[1] / dist, v[2] / dist];
            let mut at = if l.falloff > 0.0 { 1.0 / (1.0 + l.falloff * dist * dist) } else { 1.0 };
            if l.kind == "spot" { let d = norm(l.dir); let c = dot([-ln[0], -ln[1], -ln[2]], d).clamp(-1.0, 1.0).acos().to_degrees(); let inner = l.cone * (1.0 - l.softness); at *= if c <= inner { 1.0 } else if c >= l.cone { 0.0 } else { let u = (l.cone - c) / (l.cone - inner).max(1e-6); u * u * (3.0 - 2.0 * u) }; }
            (ln, at) }
    }
}

pub fn apply_relight(params: &Params, strings: &BTreeMap<String, String>, f: &mut Frame, t: f64) -> Result<(), String> {
    let sc = f.h as f32 / 720.0; let (w, h) = (f.w, f.h);
    let scene = scene().ok_or("relight needs the project lights; none are available in this render context")?;
    let amount = p(params, "amount", 1.0, t).clamp(0.0, 1.0); if amount == 0.0 { return Ok(()); }
    let bump = p(params, "bump", 1.0, t).clamp(0.0, 8.0); let blur = ((p(params, "blur", 6.0, t).clamp(0.0, 64.0)) * sc).round() as usize;
    let ambient = p(params, "ambient", 0.15, t).clamp(0.0, 4.0) as f64; let shadow = p(params, "shadow", 0.6, t).clamp(0.0, 1.0); let gain = p(params, "spec_gain", 1.0, t).clamp(0.0, 16.0) as f64; let height_px = p(params, "height", 1.0, t).clamp(0.0, 16.0);
    let src = strings.get("height_source").map(|s| s.as_str()).unwrap_or("alpha"); if !["alpha", "luma", "both"].contains(&src) { return Err(format!("height_source must be alpha|luma|both, got '{src}'")); }
    let mname = strings.get("material").map(|s| s.as_str()).unwrap_or("plastic"); let mat = lookup(&scene.materials, mname).ok_or(format!("unknown material '{mname}'; define it with set_material or use a preset"))?;
    let sel: Option<Vec<&str>> = strings.get("lights").map(|s| s.split(',').map(|x| x.trim()).filter(|x| !x.is_empty()).collect());
    let lights: Vec<&LightDef> = scene.lights.iter().filter(|(k, _)| sel.as_ref().map_or(true, |v| v.contains(&k.as_str()))).map(|(_, l)| l).collect();
    if let Some(v) = &sel { for n in v { if !scene.lights.iter().any(|(k, _)| k == n) { return Err(format!("unknown or disabled light '{n}'")); } } }
    let n = w * h; let mut hm = vec![0f32; n];
    for i in 0..n { let a = f.d[i * 4 + 3]; let l = 0.2126 * f.d[i * 4] + 0.7152 * f.d[i * 4 + 1] + 0.0722 * f.d[i * 4 + 2]; hm[i] = match src { "alpha" => a, "luma" => l * a, _ => 0.5 * a + 0.5 * l * a }; }
    blur_plane(&mut hm, w, h, blur);
    let br = (blur.max(1)) as f32; let slope = bump * br; let hz = |i: usize| hm[i] * slope * height_px; // surface height in pixels
    let fh = h as f64; let steps = 10usize; let step_px = (br * 1.5).max(2.0);
    let src_px = f.d.clone();
    for y in 0..h { for x in 0..w { let i = y * w + x; let a = src_px[i * 4 + 3]; if a <= 0.0 { continue; }
        let xm = x.saturating_sub(1); let xp = (x + 1).min(w - 1); let ym = y.saturating_sub(1); let yp = (y + 1).min(h - 1);
        let gx = (hm[y * w + xp] - hm[y * w + xm]) * 0.5 * slope; let gy = (hm[yp * w + x] - hm[ym * w + x]) * 0.5 * slope;
        let nrm = norm([-gx as f64, -gy as f64, 1.0]); let z0 = hz(i) as f64;
        let alb = mat.albedo.unwrap_or([src_px[i * 4] as f64, src_px[i * 4 + 1] as f64, src_px[i * 4 + 2] as f64]);
        let pt = [x as f64 / w as f64, y as f64 / fh, z0 / fh];
        let mut out = [ambient * alb[0] + mat.emissive[0], ambient * alb[1] + mat.emissive[1], ambient * alb[2] + mat.emissive[2]];
        for l in &lights { let (lv, at) = light_at(l, pt); if at <= 0.0 { continue; }
            let mut vis = 1.0f64;
            if shadow > 0.0 && lv[2] > 0.02 { let hl = (lv[0] * lv[0] + lv[1] * lv[1]).sqrt(); if hl > 1e-4 { let (dx, dy) = (lv[0] / hl, lv[1] / hl); let rise = lv[2] / hl; let mut occ = 0f64;
                for k in 1..=steps { let d = k as f64 * step_px as f64; let sx = x as f64 + dx * d; let sy = y as f64 + dy * d; if sx < 0.0 || sy < 0.0 || sx >= w as f64 || sy >= h as f64 { break; }
                    let hh = hm[sy as usize * w + sx as usize] as f64 * slope as f64 * height_px as f64; let ray = z0 + d * rise; occ = occ.max(((hh - ray) / (step_px as f64 * 2.0)).clamp(0.0, 1.0)); }
                vis = 1.0 - shadow as f64 * occ; } }
            let (df, sp) = brdf(nrm, lv, [0.0, 0.0, 1.0], &mat, alb); let rad = l.intensity * at * vis;
            for j in 0..3 { out[j] += rad * l.color[j] * (df[j] + sp[j] * gain); } }
        for j in 0..3 { f.d[i * 4 + j] = (src_px[i * 4 + j] as f64 * (1.0 - amount as f64) + out[j] * amount as f64) as f32; } } }
    Ok(())
}

/// Lit unit sphere / plane preview for checking a material and the lights together (analytic normals, no layer needed).
pub fn preview(scene: &Scene, mat_name: &str, size: usize, base: [f64; 3]) -> Result<Frame, String> {
    let mat = lookup(&scene.materials, mat_name).ok_or(format!("unknown material '{mat_name}'"))?; let mut f = Frame::new(size, size); let r = size as f64 * 0.42; let c = size as f64 / 2.0;
    let alb = mat.albedo.unwrap_or(base);
    for y in 0..size { for x in 0..size { let (px, py) = (x as f64 - c, y as f64 - c); let q = (px * px + py * py) / (r * r); let o = (y * size + x) * 4;
        if q > 1.0 { f.d[o + 3] = 0.0; continue; } let z = (1.0 - q).sqrt(); let nrm = [px / r, py / r, z];
        let pt = [x as f64 / size as f64, y as f64 / size as f64, z * 0.42];
        let mut out = [0.12 * alb[0] + mat.emissive[0], 0.12 * alb[1] + mat.emissive[1], 0.12 * alb[2] + mat.emissive[2]];
        for (_, l) in &scene.lights { let (lv, at) = light_at(l, pt); let (df, sp) = brdf(nrm, lv, [0.0, 0.0, 1.0], &mat, alb); for j in 0..3 { out[j] += l.intensity * at * l.color[j] * (df[j] + sp[j]); } }
        for j in 0..3 { f.d[o + j] = out[j].clamp(0.0, 1.0) as f32; } f.d[o + 3] = 1.0; } }
    Ok(f)
}
