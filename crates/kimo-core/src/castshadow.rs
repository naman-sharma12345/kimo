//! cast_shadow: a layer casts a light-driven soft shadow onto what is below it. The shadow is the layer's alpha projected onto the
//! receiving plane (z = 0) from a point light (the first enabled scene light, or light_x/light_y/light_z) by similar triangles, with a
//! penumbra of a finite-size light: blur radius = light_size * H / (Lz - H), so higher casters and bigger lights give softer shadows.
//! One constant height for the whole layer, so there is no per-point contact hardening. Written under the layer (post transform), only where the layer is not opaque.
use crate::effects::{box_blur3, p, Frame, Params};

pub fn apply(pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    let g = |k: &str, d: f64| p(pr, k, d, t) as f64;
    let (w, h) = (f.w, f.h); let (wf, hf) = (w as f64, h as f64);
    let (mut lx, mut ly, mut lz) = (g("light_x", 0.2), g("light_y", 0.1), g("light_z", 0.8));
    if g("use_scene_light", 1.0) > 0.5 { if let Some(sc) = crate::lighting::scene() { if let Some(l) = sc.lights.iter().find(|l| l.1.kind != "directional" && l.1.enabled) { lx = l.1.pos[0]; ly = l.1.pos[1]; lz = l.1.pos[2]; } } }
    let height = g("height", 0.08).clamp(0.0, 1.0) * hf; let lzp = (lz * hf).max(height + 1.0);
    let opacity = g("opacity", 0.6).clamp(0.0, 1.0) as f32; let col = [g("r", 0.0) as f32, g("g", 0.0) as f32, g("b", 0.0) as f32];
    // alpha-weighted centroid of the layer = the caster point used for the (planar) shift
    let (mut sx, mut sy, mut sa) = (0f64, 0f64, 0f64); for y in 0..h { for x in 0..w { let a = f.d[(y * w + x) * 4 + 3] as f64; if a > 0.0 { sx += x as f64 * a; sy += y as f64 * a; sa += a; } } }
    if sa <= 0.0 { return Ok(()); } let (cx, cy) = (sx / sa, sy / sa);
    let k = height / (lzp - height); let (dx, dy) = ((cx - lx * wf) * k, (cy - ly * hf) * k);
    // shifted hard mask (bilinear)
    let at = |x: i64, y: i64| -> f32 { if x < 0 || y < 0 || x >= w as i64 || y >= h as i64 { 0.0 } else { f.d[(y as usize * w + x as usize) * 4 + 3] } };
    let mut m = vec![0f32; w * h]; for y in 0..h { for x in 0..w { let (fx, fy) = (x as f64 - dx, y as f64 - dy); let (x0, y0) = (fx.floor(), fy.floor()); let (ax, ay) = ((fx - x0) as f32, (fy - y0) as f32); let (x0, y0) = (x0 as i64, y0 as i64);
        m[y * w + x] = at(x0, y0) * (1.0 - ax) * (1.0 - ay) + at(x0 + 1, y0) * ax * (1.0 - ay) + at(x0, y0 + 1) * (1.0 - ax) * ay + at(x0 + 1, y0 + 1) * ax * ay; } }
    // penumbra of a light of finite size over a planar caster at constant height: radius = light_size * H / (Lz - H) (similar triangles)
    let r = (g("light_size", 0.05).clamp(0.0, 1.0) * hf * k).round().clamp(0.0, 200.0) as usize;
    let sm: Vec<f32> = if r == 0 { m.clone() } else { let mut fr = Frame { w, h, d: vec![0.0; w * h * 4] }; for i in 0..w * h { fr.d[i * 4] = m[i]; fr.d[i * 4 + 3] = 1.0; } box_blur3(&mut fr, r); (0..w * h).map(|i| fr.d[i * 4]).collect() };
    for i in 0..w * h { let a = f.d[i * 4 + 3]; if a >= 1.0 { continue; }
        let s = sm[i];
        let sa2 = (s * opacity * (1.0 - a)).clamp(0.0, 1.0); if sa2 <= 0.0 { continue; }
        let na = a + sa2; for c in 0..3 { f.d[i * 4 + c] = (f.d[i * 4 + c] * a + col[c] * sa2) / na; } f.d[i * 4 + 3] = na; }
    Ok(())
}
