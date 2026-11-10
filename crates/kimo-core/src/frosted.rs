//! frosted_glass: a glass pane whose shape is the layer's alpha. Inside the pane the layer shows
//! the composite below it (the backdrop) blurred, tinted and refracted at the pane edge.
use crate::effects::{box_blur3, p, Frame, Params};

fn sample(src: &[f32], w: usize, h: usize, x: f32, y: f32, ch: usize) -> f32 {
    let x = x.clamp(0.0, (w - 1) as f32); let y = y.clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (x as usize, y as usize); let (x1, y1) = ((x0 + 1).min(w - 1), (y0 + 1).min(h - 1)); let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |xx: usize, yy: usize| src[(yy * w + xx) * 4 + ch];
    (at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx) * (1.0 - fy) + (at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx) * fy
}

thread_local! { static POST: std::cell::Cell<bool> = Default::default(); }
/// True only while the renderer runs frosted_glass after the layer transform.
thread_local! { static TILT: std::cell::Cell<(f32, f32)> = std::cell::Cell::new((0.0, 0.0)); }
/// Screen-space xy of the layer's 3D surface normal (set by the renderer for tilted layers, else 0).
pub fn set_tilt(nx: f32, ny: f32) { TILT.with(|c| c.set((nx, ny))) }
thread_local! { static SLAB: std::cell::RefCell<Option<crate::three_d::Projector>> = std::cell::RefCell::new(None); }
pub fn set_slab(p: Option<crate::three_d::Projector>) { SLAB.with(|c| *c.borrow_mut() = p) }
pub fn in_post() -> bool { POST.with(|c| c.get()) }
pub fn set_post(v: bool) { POST.with(|c| c.set(v)) }

pub fn apply(pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    let g = |k: &str, d: f64| p(pr, k, d, t);
    let bd = crate::render::backdrop().filter(|b| b.w == f.w && b.h == f.h).ok_or("frosted_glass needs a layer below it (nothing to frost)")?;
    let (w, h) = (f.w, f.h);
    let blur = g("blur", 24.0).clamp(0.0, 200.0) as usize; let refr = g("refraction", 14.0).clamp(0.0, 80.0) as f32; let disp = g("dispersion", 0.25).clamp(0.0, 1.5) as f32;
    let bevel = g("bevel", 10.0).clamp(1.0, 80.0) as usize; let tint = [g("tint_r", 1.0) as f32, g("tint_g", 1.0) as f32, g("tint_b", 1.0) as f32]; let tamt = g("tint_amount", 0.12).clamp(0.0, 1.0) as f32;
    let sat = g("saturation", 1.15).clamp(0.0, 3.0) as f32; let rim = g("edge_light", 0.5).clamp(0.0, 2.0) as f32; let mix = g("layer_mix", 0.0).clamp(0.0, 1.0) as f32;
    let magnify = g("magnify", 1.0).clamp(0.25, 8.0) as f32;
    // centre of the pane = alpha centroid (only needed for magnify != 1)
    let (mut cx, mut cy) = (w as f32 * 0.5, h as f32 * 0.5); if magnify != 1.0 { let (mut sx, mut sy, mut sa) = (0f64, 0f64, 0f64); for y in 0..h { for x in 0..w { let a = f.d[(y * w + x) * 4 + 3] as f64; if a > 0.0 { sx += x as f64 * a; sy += y as f64 * a; sa += a; } } } if sa > 0.0 { cx = (sx / sa) as f32; cy = (sy / sa) as f32; } }
    // magnify_cx / magnify_cy: zoom about a canvas point (0..1 of width / height) instead of the pane centroid; -1 (default) keeps the centroid
    let (mcx, mcy) = (g("magnify_cx", -1.0) as f32, g("magnify_cy", -1.0) as f32); if mcx >= 0.0 { cx = mcx.min(2.0) * w as f32; } if mcy >= 0.0 { cy = mcy.min(2.0) * h as f32; }
    // tilt_refraction (px per unit normal, default 0 = off): the whole pane refracts the backdrop along its 3D normal, like a real tilted slab
    let tk = g("tilt_refraction", 0.0).clamp(0.0, 200.0) as f32; let (tnx, tny) = TILT.with(|c| c.get()); let (tox, toy) = (-tnx * tk, -tny * tk);
    // slab_thickness (world px, default 0 = off) + ior (default 1.5): per-pixel Snell refraction through a glass slab lying on the tilted layer plane
    let slab_t = g("slab_thickness", 0.0).clamp(0.0, 2000.0) as f64; let ior = g("ior", 1.5).clamp(1.0, 3.0) as f64; let slab = if slab_t > 0.0 { SLAB.with(|c| c.borrow().clone()) } else { None };
    // fresnel (0 = off): Schlick reflection of a procedural environment (sky gradient + one softbox) added over the pane by view angle
    let fres = g("fresnel", 0.0).clamp(0.0, 1.0) as f64; let f0 = g("f0", 0.04).clamp(0.0, 1.0) as f64; let env_gain = g("env_gain", 1.0).clamp(0.0, 16.0) as f64; let box_gain = g("box_gain", 3.0).clamp(0.0, 64.0) as f64;
    let (bx, by) = (g("box_x", -0.4) as f64, g("box_y", -0.6) as f64); let bz = (1.0 - bx * bx - by * by).max(0.01).sqrt(); let bdir = [bx, by, -bz]; let box_size = g("box_size", 0.02).clamp(0.001, 1.0) as f64;
    let proj = if fres > 0.0 { SLAB.with(|c| c.borrow().clone()).or_else(|| Some(crate::three_d::Projector::new(w, h, &crate::three_d::Camera::default(), &crate::three_d::Pose { pos: [0.0, 0.0, 0.0], rot: [0.0, 0.0, 0.0], scale: 1.0 }))) } else { None };
    // reflect_scene (0..1, default 0 = off): the reflection also shows the backdrop, looked up along the reflected ray in screen space
    // (x + r.x * reflect_dist, y + r.y * reflect_dist px), roughness-blurred by env_blur px. Mixed over the procedural environment.
    let rscene = g("reflect_scene", 0.0).clamp(0.0, 1.0); let rdist = g("reflect_dist", 300.0) as f32; let renv = if fres > 0.0 && rscene > 0.0 { let mut e = Frame { w, h, d: bd.d.clone() }; box_blur3(&mut e, g("env_blur", 6.0).clamp(0.0, 100.0) as usize); Some(e) } else { None };
    let mut frost = Frame { w, h, d: bd.d.clone() }; box_blur3(&mut frost, blur);
    // smooth alpha -> height field; its gradient is the bevel normal, nonzero only near the pane edge
    let mut hf = Frame { w, h, d: vec![0.0; w * h * 4] }; for i in 0..w * h { hf.d[i * 4] = f.d[i * 4 + 3]; hf.d[i * 4 + 3] = 1.0; } box_blur3(&mut hf, bevel * 2);
    let hh = |x: usize, y: usize| hf.d[(y.min(h - 1) * w + x.min(w - 1)) * 4];
    let src_lum = |i: usize| 0.2126 * f.d[i * 4] + 0.7152 * f.d[i * 4 + 1] + 0.0722 * f.d[i * 4 + 2]; let _ = src_lum;
    let mut out = vec![0f32; w * h * 4];
    for y in 0..h { for x in 0..w { let i = y * w + x; let a = f.d[i * 4 + 3]; if a <= 0.0 { continue; }
        let gx = hh(x + 1, y) - hh(x.saturating_sub(1), y); let gy = hh(x, y + 1) - hh(x, y.saturating_sub(1)); let gm = (gx * gx + gy * gy).sqrt();
        let (ox, oy) = (-gx * refr * bevel as f32 * 0.5 + tox, -gy * refr * bevel as f32 * 0.5 + toy);
        let (mx, my) = if magnify != 1.0 { (cx + (x as f32 - cx) / magnify, cy + (y as f32 - cy) / magnify) } else { (x as f32, y as f32) };
        let mut c = [0f32; 3]; for (j, cj) in c.iter_mut().enumerate() { let s = 1.0 + (j as f32 - 1.0) * disp * 0.5;
            let (sx, sy) = match &slab { Some(pj) => pj.slab_offset(x as f64 + 0.5, y as f64 + 0.5, slab_t, ior * (1.0 + (1.0 - j as f64) * disp as f64 * 0.04)).map(|(a, b)| (a as f32, b as f32)).unwrap_or((0.0, 0.0)), None => (0.0, 0.0) };
            *cj = sample(&frost.d, w, h, mx + ox * s + sx, my + oy * s + sy, j); }
        let l = 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]; for cj in c.iter_mut() { *cj = l + (*cj - l) * sat; }
        for j in 0..3 { c[j] = c[j] * (1.0 - tamt) + tint[j] * tamt; }
        // rim light: bright where the bevel normal faces up-left, darker down-right
        let lit = (-(gx * 0.7 + gy * 0.7) / gm.max(1e-6)) * (gm * bevel as f32 * 2.0).min(1.0);
        if let Some(pj) = &proj { let (d, n) = pj.view_and_normal(x as f64 + 0.5, y as f64 + 0.5); let cosv = (-(d[0] * n[0] + d[1] * n[1] + d[2] * n[2])).clamp(0.0, 1.0);
            let fr = (f0 + (1.0 - f0) * (1.0 - cosv).powi(5)) * fres; let dn = d[0] * n[0] + d[1] * n[1] + d[2] * n[2]; let r = [d[0] - 2.0 * dn * n[0], d[1] - 2.0 * dn * n[1], d[2] - 2.0 * dn * n[2]];
            let up = (-r[1]).clamp(-1.0, 1.0); let t = (up * 0.5 + 0.5).powf(1.5); let sky = [0.18 + 0.42 * t, 0.22 + 0.55 * t, 0.3 + 0.7 * t];
            let bd = r[0] * bdir[0] + r[1] * bdir[1] + r[2] * bdir[2]; let bl = box_gain * ((bd - 1.0) / box_size).exp();
            let sc = match &renv { Some(e) => { let mut v = [0f32; 3]; for (j, vj) in v.iter_mut().enumerate() { *vj = sample(&e.d, w, h, x as f32 + r[0] as f32 * rdist, y as f32 + r[1] as f32 * rdist, j); } Some(v) } None => None };
            for j in 0..3 { let env = match sc { Some(v) => (sky[j] * env_gain + bl) * (1.0 - rscene as f64) + v[j] as f64 * rscene as f64 * env_gain + bl, None => sky[j] * env_gain + bl }; c[j] = c[j] * (1.0 - fr as f32) + (env * fr) as f32; } }
        for j in 0..3 { c[j] = (c[j] + lit * rim * 0.35).max(0.0); c[j] = c[j] * (1.0 - mix) + f.d[i * 4 + j] * mix; out[i * 4 + j] = c[j]; }
        out[i * 4 + 3] = a; } }
    f.d = out; Ok(())
}
