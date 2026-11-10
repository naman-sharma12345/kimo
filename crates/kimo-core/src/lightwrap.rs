//! light_wrap: backdrop light bleeding over a layer's edges. wrap = blur(backdrop) * edge, edge = blur(1 - alpha, width) so it is
//! strongest at the silhouette and fades inward; added to the layer's own colour where the layer is opaque.
//! Exact formula (per pixel with layer alpha a > 0): edge = clamp(B_width(1 - alpha), 0, 1) * intensity, col = B_blur(backdrop rgb),
//! l = 0.2126 r + 0.7152 g + 0.0722 b of col, wv_j = l + (col_j - l) (1 - desaturate), rgb_j += wv_j * edge * a.
//! B_r = effects::box_blur3(r): q = max(r / 2 (integer division), 1); 3 times { horizontal then vertical clamped box mean over 2q+1 taps }, so on a
//! vertical step edge B_r(1 - alpha) is the 1D clamped box^3 (piecewise quadratic, support 3q each side) of the step.
//! linear = 1: the layer rgb and the backdrop are decoded sRGB -> linear (glow::srgb_to_linear) before the blur/add and the sum is re-encoded
//! (glow::linear_to_srgb); edge is the same alpha-only falloff. Pixels the wrap does not touch (a <= 0 or edge = 0) keep their exact bits.
use crate::effects::{box_blur3, p, Frame, Params};
use crate::glow::{linear_to_srgb, srgb_to_linear};

pub fn apply(pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    let g = |k: &str, d: f64| p(pr, k, d, t);
    let (w, h) = (f.w, f.h);
    let bd = crate::render::backdrop().filter(|b| b.w == w && b.h == h).ok_or("light_wrap needs a layer below it (nothing to wrap)")?;
    let width = g("width", 24.0).clamp(1.0, 300.0) as usize; let inten = g("intensity", 1.0).clamp(0.0, 4.0) as f32; let blur = g("blur", 20.0).clamp(0.0, 200.0) as usize; let desat = g("desaturate", 0.0).clamp(0.0, 1.0) as f32;
    if g("linear", 0.0) >= 0.5 { apply_linear(f, &bd, width, inten, blur, desat); return Ok(()); }
    let mut col = Frame { w, h, d: bd.d.clone() }; box_blur3(&mut col, blur);
    let mut inv = Frame { w, h, d: vec![0.0; w * h * 4] }; for i in 0..w * h { inv.d[i * 4] = 1.0 - f.d[i * 4 + 3]; inv.d[i * 4 + 3] = 1.0; } box_blur3(&mut inv, width);
    for i in 0..w * h { let a = f.d[i * 4 + 3]; if a <= 0.0 { continue; } let edge = inv.d[i * 4].clamp(0.0, 1.0) * inten;
        let (r, gc, b) = (col.d[i * 4], col.d[i * 4 + 1], col.d[i * 4 + 2]); let l = 0.2126 * r + 0.7152 * gc + 0.0722 * b;
        for (j, v) in [r, gc, b].into_iter().enumerate() { let wv = l + (v - l) * (1.0 - desat); f.d[i * 4 + j] += wv * edge * a; } }
    Ok(())
}

fn apply_linear(f: &mut Frame, bd: &Frame, width: usize, inten: f32, blur: usize, desat: f32) {
    let (w, h) = (f.w, f.h);
    let mut col = Frame { w, h, d: bd.d.clone() }; for i in 0..w * h { for j in 0..3 { col.d[i * 4 + j] = srgb_to_linear(col.d[i * 4 + j]); } } box_blur3(&mut col, blur);
    let mut inv = Frame { w, h, d: vec![0.0; w * h * 4] }; for i in 0..w * h { inv.d[i * 4] = 1.0 - f.d[i * 4 + 3]; inv.d[i * 4 + 3] = 1.0; } box_blur3(&mut inv, width);
    for i in 0..w * h { let a = f.d[i * 4 + 3]; if a <= 0.0 { continue; } let edge = inv.d[i * 4].clamp(0.0, 1.0) * inten; if edge <= 0.0 { continue; }
        let (r, gc, b) = (col.d[i * 4], col.d[i * 4 + 1], col.d[i * 4 + 2]); let l = 0.2126 * r + 0.7152 * gc + 0.0722 * b;
        for (j, v) in [r, gc, b].into_iter().enumerate() { let wv = l + (v - l) * (1.0 - desat); f.d[i * 4 + j] = linear_to_srgb(srgb_to_linear(f.d[i * 4 + j]) + wv * edge * a); } }
}
