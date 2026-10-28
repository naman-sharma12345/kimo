//! Deep glow (glow mode 1): soft-knee bright pass (shared with bloom) -> exact sum of separable sampled Gaussians -> exposure + A/B tint.
//!
//! Kernel (all weights normalised, so the 2D kernel sums to exactly 1 away from the frame edge; clamped edges replicate the border pixel):
//! - `gaussian_kernel_1d(s)`: taps i in -r..=r, r = ceil(3 s) + 1, g_i = exp(-i^2 / (2 s^2)) / sum_j exp(-j^2 / (2 s^2)).
//!   The 2D component g_i * g_j = exp(-(i^2 + j^2) / (2 s^2)) / Z^2 is radially symmetric at every lattice point (square window of half width r).
//! - falloff 0 (sum of Gaussians): n = iterations (1..8) components, s_k = radius * 2^k / 2^(n-1) (k = 0..n-1, so the widest s = radius), w_k = 1/n.
//! - falloff 1 (exponential): fixed 5-term mixture approximating the normalised 2D kernel exp(-r/R) / (2 pi R^2), R = radius, s_k = R * 2^(k-2)
//!   (0.25R, 0.5R, R, 2R, 4R) and w_k = d(s_k/R) / sum_j d(s_j/R) with d(s) = s^3 exp(-s^2/2) (the octave-step quadrature of the subordination
//!   identity exp(-r) = int sqrt(2/pi) s^3 e^(-s^2/2) * N(r; s) dln s): w ~= 0.008248, 0.060078, 0.330330, 0.589653, 0.011693.
//! Response: K(dx, dy) = sum_k w_k g_k[dx] g_k[dy]; colour of component k = lerp(color_a, color_b, k/(n-1)) (k = 0 is the tightest).
//! Glow (premultiplied) G = amount * 2^exposure * sum_k w_k tint_k (K_k * B), B = bright pass * alpha; with white tints the glow adds exactly
//! amount * 2^exposure * (bright-pass energy). Composite: P' = c*a + G, a' = a + (1-a) min(1, max_j G_j), c' = P'/a' (a = 1: c' = c + G).
//! linear = 1: c is decoded sRGB -> linear first (and the threshold, given in display units, too), c' is re-encoded. Pixels with G = 0 are untouched.
use crate::effects::{lum, p, Frame, Params};

/// Soft-knee bright pass excess shared with bloom: quadratic ramp over [th - knee*th, th + knee*th], linear (l - th) above.
pub fn knee_excess(l: f32, th: f32, knee: f32) -> f32 { let w = knee * th; let lo = th - w; if w > 1e-6 && l > lo && l < th + w { let x = l - lo; x * x / (4.0 * w) } else if l >= th + w { l - th } else { 0.0 } }
pub fn srgb_to_linear(v: f32) -> f32 { let a = v.abs(); let o = if a <= 0.04045 { a / 12.92 } else { ((a + 0.055) / 1.055).powf(2.4) }; o.copysign(v) }
pub fn linear_to_srgb(v: f32) -> f32 { let a = v.abs(); let o = if a <= 0.0031308 { a * 12.92 } else { 1.055 * a.powf(1.0 / 2.4) - 0.055 }; o.copysign(v) }
/// Normalised sampled Gaussian, length 2r+1 with r = ceil(3 sigma) + 1 (sigma floored at 1e-3).
pub fn gaussian_kernel_1d(sigma: f32) -> Vec<f32> {
    let s = sigma.max(1e-3) as f64; let r = (3.0 * s).ceil() as i64 + 1; let g: Vec<f64> = (-r..=r).map(|i| (-((i * i) as f64) / (2.0 * s * s)).exp()).collect(); let z: f64 = g.iter().sum(); g.iter().map(|v| (v / z) as f32).collect()
}
/// (sigma, weight) of every component; weights sum to 1. falloff 0 = sum of Gaussians (iterations 1..8), 1 = exponential mixture.
pub fn deep_glow_components(falloff: u32, radius: f32, iterations: usize) -> Vec<(f32, f32)> {
    let r = radius.max(0.1) as f64;
    if falloff >= 1 { let d = |s: f64| s * s * s * (-s * s / 2.0).exp(); let ss: Vec<f64> = (0..5).map(|k| 2f64.powi(k - 2)).collect(); let z: f64 = ss.iter().map(|&s| d(s)).sum(); return ss.iter().map(|&s| ((s * r) as f32, (d(s) / z) as f32)).collect(); }
    let n = iterations.clamp(1, 8); (0..n).map(|k| ((r * 2f64.powi(k as i32) / 2f64.powi(n as i32 - 1)) as f32, 1.0 / n as f32)).collect()
}
/// Half width in px of the whole kernel (widest component): ceil(3 s_max) + 1.
pub fn deep_glow_support(falloff: u32, radius: f32, iterations: usize) -> usize { deep_glow_components(falloff, radius, iterations).iter().map(|c| (gaussian_kernel_1d(c.0).len() - 1) / 2).max().unwrap_or(0) }
/// Interior impulse response K(dx, dy) = sum_k w_k g_k[dx] g_k[dy] (untinted, gain 1). Sums to 1 over the plane.
pub fn deep_glow_response(falloff: u32, radius: f32, iterations: usize, dx: i64, dy: i64) -> f32 {
    deep_glow_components(falloff, radius, iterations).iter().map(|&(s, w)| { let g = gaussian_kernel_1d(s); let r = ((g.len() - 1) / 2) as i64; if dx.abs() > r || dy.abs() > r { 0.0 } else { w * g[(dx + r) as usize] * g[(dy + r) as usize] } }).sum()
}
/// Per-component tint: lerp(color_a, color_b, k/(n-1)), k = 0 the tightest component.
pub fn deep_glow_tints(n: usize, a: [f32; 3], b: [f32; 3]) -> Vec<[f32; 3]> { (0..n).map(|k| { let u = if n > 1 { k as f32 / (n - 1) as f32 } else { 0.0 }; [a[0] + (b[0] - a[0]) * u, a[1] + (b[1] - a[1]) * u, a[2] + (b[2] - a[2]) * u] }).collect() }
pub(crate) fn mode(pr: &Params, t: f64) -> bool { p(pr, "mode", 0.0, t) >= 0.5 }
pub(crate) fn support_of(pr: &Params, t: f64) -> usize { deep_glow_support(p(pr, "falloff", 0.0, t).round().max(0.0) as u32, p(pr, "radius", 12.0, t).clamp(0.1, 512.0), p(pr, "iterations", 5.0, t).round().max(1.0) as usize) }

/// One separable pass of 3-channel data (stride 3) with clamped edges, src -> dst.
fn pass(src: &[f32], dst: &mut [f32], w: usize, h: usize, k: &[f32], horiz: bool) {
    let r = ((k.len() - 1) / 2) as isize;
    crate::par::rows_mut(dst, w * 3, |y, out| { for x in 0..w { let mut acc = [0f32; 3];
        for (i, kv) in k.iter().enumerate() { let o = i as isize - r; let (sx, sy) = if horiz { ((x as isize + o).clamp(0, w as isize - 1) as usize, y) } else { (x, (y as isize + o).clamp(0, h as isize - 1) as usize) };
            let s = (sy * w + sx) * 3; acc[0] += src[s] * kv; acc[1] += src[s + 1] * kv; acc[2] += src[s + 2] * kv; }
        out[x * 3..x * 3 + 3].copy_from_slice(&acc); } });
}

/// glow mode 1. Params: amount, radius, threshold, knee, falloff, iterations, exposure, color_a_r/g/b, color_b_r/g/b, linear.
pub fn apply_deep(pr: &Params, f: &mut Frame, t: f64) {
    let g = |k: &str, d: f64| p(pr, k, d, t); let (w, h) = (f.w, f.h); let n = w * h; if n == 0 { return; }
    let lin = g("linear", 1.0) >= 0.5; let gain = g("amount", 0.5).max(0.0) * 2f32.powf(g("exposure", 0.0).clamp(-8.0, 8.0));
    let th0 = g("threshold", 0.7).max(0.0); let th = if lin { srgb_to_linear(th0) } else { th0 }; let knee = g("knee", 0.2).clamp(0.0, 1.0);
    let comps = deep_glow_components(g("falloff", 0.0).round().max(0.0) as u32, g("radius", 12.0).clamp(0.1, 512.0), g("iterations", 5.0).round().max(1.0) as usize);
    let tints = deep_glow_tints(comps.len(), [g("color_a_r", 1.0), g("color_a_g", 1.0), g("color_a_b", 1.0)], [g("color_b_r", 1.0), g("color_b_g", 1.0), g("color_b_b", 1.0)]);
    let dec = |v: f32| if lin { srgb_to_linear(v) } else { v };
    let mut bp = vec![0f32; n * 3]; let mut any = false;
    for i in 0..n { let a = f.d[i * 4 + 3]; if a <= 0.0 { continue; } let c = [dec(f.d[i * 4]), dec(f.d[i * 4 + 1]), dec(f.d[i * 4 + 2])]; let l = lum(c); if l <= 1e-6 { continue; }
        let e = knee_excess(l, th, knee); if e <= 0.0 { continue; } let k = e / l * a; for j in 0..3 { bp[i * 3 + j] = c[j] * k; } any = true; }
    if !any || gain <= 0.0 { return; }
    let acc = if half_res_on(pr, t) {
        // Half-res fast path: downsample once, run every component's Gaussian pass at half size with
        // half the sigma (one half-res pixel spans two full-res pixels, so this targets the same
        // real-world blur extent), accumulate at half res, upsample the SUM once at the end (cheaper
        // than upsampling every component separately, and avoids compounding upsample blur per tap).
        let (bp_half, hw, hh) = downsample2x(&bp, w, h); let hn = hw * hh;
        let (mut tmp, mut out) = (vec![0f32; hn * 3], vec![0f32; hn * 3]); let mut acc_half = vec![0f32; hn * 3];
        for (ci, &(s, wt)) in comps.iter().enumerate() { crate::guard::check(); let k = gaussian_kernel_1d((s * 0.5).max(1e-3));
            pass(&bp_half, &mut tmp, hw, hh, &k, true); pass(&tmp, &mut out, hw, hh, &k, false);
            let tn = tints[ci]; for i in 0..hn { for j in 0..3 { acc_half[i * 3 + j] += out[i * 3 + j] * wt * tn[j]; } } }
        upsample2x(&acc_half, hw, hh, w, h)
    } else {
        let (mut tmp, mut out, mut acc) = (vec![0f32; n * 3], vec![0f32; n * 3], vec![0f32; n * 3]);
        for (ci, &(s, wt)) in comps.iter().enumerate() { crate::guard::check(); let k = gaussian_kernel_1d(s); pass(&bp, &mut tmp, w, h, &k, true); pass(&tmp, &mut out, w, h, &k, false);
            let tn = tints[ci]; for i in 0..n { for j in 0..3 { acc[i * 3 + j] += out[i * 3 + j] * wt * tn[j]; } } }
        acc
    };
    for i in 0..n { let gl = [acc[i * 3] * gain, acc[i * 3 + 1] * gain, acc[i * 3 + 2] * gain]; if gl == [0.0; 3] { continue; }
        let a = f.d[i * 4 + 3]; let an = a + (1.0 - a) * gl[0].max(gl[1]).max(gl[2]).clamp(0.0, 1.0); if an <= 1e-6 { continue; }
        for j in 0..3 { let v = (dec(f.d[i * 4 + j]) * a + gl[j]) / an; f.d[i * 4 + j] = if lin { linear_to_srgb(v) } else { v }; } f.d[i * 4 + 3] = an; }
}

// ---------------------------------------------------------------------------------------------
// Half-res fast path (FEATURES.md L558 "u8/half-res fast path for glow/blur"): opt-in via the
// `half_res` param (default 0, byte-identical to the pre-existing path). The bright-pass buffer is
// box-downsampled 2x before the separable Gaussian passes (each component's sigma halved to match,
// since one half-res pixel spans two full-res pixels) and the glow buffer is bilinear-upsampled back
// before compositing. Quarter the pixels and half the kernel radius per component -- the two
// multiply, so this is roughly an 8x reduction in the separable-pass multiply-adds, at the cost of a
// softer, slightly less precise glow (a real speed/quality tradeoff, not a hidden approximation).
/// Box-downsample RGB (stride 3) by 2x, ceil-dividing odd dimensions (the last row/col duplicates its
/// only source row/col rather than reading past the edge).
fn downsample2x(src: &[f32], w: usize, h: usize) -> (Vec<f32>, usize, usize) {
    let dw = w.div_ceil(2).max(1); let dh = h.div_ceil(2).max(1);
    let mut dst = vec![0f32; dw * dh * 3];
    for dy in 0..dh { for dx in 0..dw {
        let x0 = dx * 2; let y0 = dy * 2;
        let xs = [x0, (x0 + 1).min(w - 1)]; let ys = [y0, (y0 + 1).min(h - 1)];
        let mut acc = [0f32; 3];
        for &sy in &ys { for &sx in &xs { let s = (sy * w + sx) * 3; for j in 0..3 { acc[j] += src[s + j]; } } }
        let o = (dy * dw + dx) * 3; for j in 0..3 { dst[o + j] = acc[j] * 0.25; }
    } }
    (dst, dw, dh)
}
/// Bilinear-upsample RGB (stride 3) from (sw, sh) back to (w, h) -- the approximate inverse of
/// downsample2x's 2x2 box (not exact; a box-down then bilinear-up is the standard half-res bloom
/// technique, not a lossless round trip).
fn upsample2x(src: &[f32], sw: usize, sh: usize, w: usize, h: usize) -> Vec<f32> {
    let mut dst = vec![0f32; w * h * 3];
    let (sw1, sh1) = ((sw.max(1) - 1) as f32, (sh.max(1) - 1) as f32);
    for y in 0..h { for x in 0..w {
        let fx = ((x as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, sw1); let fy = ((y as f32 + 0.5) / 2.0 - 0.5).clamp(0.0, sh1);
        let x0 = fx.floor() as usize; let y0 = fy.floor() as usize; let x1 = (x0 + 1).min(sw - 1); let y1 = (y0 + 1).min(sh - 1);
        let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
        for j in 0..3 {
            let v00 = src[(y0 * sw + x0) * 3 + j]; let v10 = src[(y0 * sw + x1) * 3 + j];
            let v01 = src[(y1 * sw + x0) * 3 + j]; let v11 = src[(y1 * sw + x1) * 3 + j];
            dst[(y * w + x) * 3 + j] = v00 * (1.0 - tx) * (1.0 - ty) + v10 * tx * (1.0 - ty) + v01 * (1.0 - tx) * ty + v11 * tx * ty;
        }
    } }
    dst
}
pub(crate) fn half_res_on(pr: &Params, t: f64) -> bool { p(pr, "half_res", 0.0, t) >= 0.5 }
