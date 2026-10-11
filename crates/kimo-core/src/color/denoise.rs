//! Opt-in temporal + spatial color denoise.
//!
//! Contract:
//! - Buffers are RGBA f32, straight (non-premultiplied) alpha, row-major,
//!   Rec709 display-referred (gamma-encoded) values, nominal range 0..=1.
//!   Denoise runs in the display domain, in Rec709 Y'CbCr, so it matches the
//!   display contract of the rest of the pipeline (no transfer-function change).
//! - `amount == 0` returns a bit-exact copy of the input.
//! - Alpha is never modified. Pixels with alpha 0 are returned untouched and
//!   never contribute to neighbours. Neighbour contribution is alpha-weighted.
//! - Deterministic: single-threaded, fixed iteration order, no RNG, no time.
//! - Hostile input never panics: params are sanitized (NaN/inf/out-of-range),
//!   bad dimensions or short buffers return the input unchanged.

/// User-facing parameters. All fields are sanitized by [`DenoiseParams::sanitized`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DenoiseParams {
    /// Overall mix, 0 = identity, 1 = full effect.
    pub amount: f32,
    /// Luma strength 0..=1 (spatial range sigma and temporal tolerance).
    pub luma: f32,
    /// Chroma strength 0..=1 (chroma is usually noisier, default higher).
    pub chroma: f32,
    /// Spatial window radius in pixels, 0..=4 (0 disables spatial pass).
    pub radius: u32,
    /// Temporal blend 0..=1 (0 disables temporal pass).
    pub temporal: f32,
}

impl Default for DenoiseParams {
    fn default() -> Self {
        Self { amount: 0.0, luma: 0.5, chroma: 0.7, radius: 2, temporal: 0.5 }
    }
}

fn clean(v: f32, lo: f32, hi: f32, fallback: f32) -> f32 {
    if v.is_nan() { fallback } else { v.clamp(lo, hi) } // clamp maps +-inf to bounds
}

impl DenoiseParams {
    pub fn sanitized(self) -> Self {
        let d = Self::default();
        Self {
            amount: clean(self.amount, 0.0, 1.0, 0.0),
            luma: clean(self.luma, 0.0, 1.0, d.luma),
            chroma: clean(self.chroma, 0.0, 1.0, d.chroma),
            radius: self.radius.min(4),
            temporal: clean(self.temporal, 0.0, 1.0, d.temporal),
        }
    }
}

#[inline]
fn to_ycc(r: f32, g: f32, b: f32) -> [f32; 3] {
    // Rec709 (BT.709) luma coefficients, full-range Y'CbCr.
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    [y, (b - y) / 1.8556, (r - y) / 1.5748]
}

#[inline]
fn from_ycc(y: f32, cb: f32, cr: f32) -> [f32; 3] {
    let r = y + 1.5748 * cr;
    let b = y + 1.8556 * cb;
    let g = (y - 0.2126 * r - 0.0722 * b) / 0.7152;
    [r, g, b]
}

#[inline]
fn finite_or(v: f32, f: f32) -> f32 {
    if v.is_finite() { v } else { f }
}

/// Denoise one frame. `neighbors` are temporally adjacent frames of identical
/// size (previous and/or next, any count; extra frames beyond 4 are ignored).
/// Mismatched neighbours are skipped. Returns a new buffer of `cur.len()`.
pub fn denoise_frame(
    cur: &[f32],
    neighbors: &[&[f32]],
    width: usize,
    height: usize,
    params: &DenoiseParams,
) -> Vec<f32> {
    let p = params.sanitized();
    let n = match width.checked_mul(height).and_then(|v| v.checked_mul(4)) {
        Some(n) => n,
        None => return cur.to_vec(),
    };
    if p.amount == 0.0 || width == 0 || height == 0 || cur.len() != n {
        return cur.to_vec();
    }
    let npx = width * height;

    // ---- temporal pass (RGB, per-pixel motion-adaptive weights) ----
    let mut stage: Vec<f32> = cur.to_vec();
    let nb: Vec<&[f32]> = neighbors.iter().copied().filter(|f| f.len() == n).take(4).collect();
    if p.temporal > 0.0 && !nb.is_empty() {
        // tolerance on squared RGB distance; stronger luma => more tolerant
        let sigma = 0.01 + 0.12 * p.luma;
        let inv = 1.0 / (2.0 * sigma * sigma);
        for i in 0..npx {
            let o = i * 4;
            let a = cur[o + 3];
            if !(a > 0.0) || !cur[o].is_finite() || !cur[o + 1].is_finite() || !cur[o + 2].is_finite() {
                continue;
            }
            let mut acc = [cur[o], cur[o + 1], cur[o + 2]];
            let mut wsum = 1.0f32;
            for f in &nb {
                let na = f[o + 3];
                if !(na > 0.0) {
                    continue;
                }
                let (r, g, b) = (f[o], f[o + 1], f[o + 2]);
                if !(r.is_finite() && g.is_finite() && b.is_finite()) {
                    continue;
                }
                let d2 = (r - cur[o]).powi(2) + (g - cur[o + 1]).powi(2) + (b - cur[o + 2]).powi(2);
                let w = p.temporal * na.min(1.0) * (-d2 * inv).exp();
                acc[0] += w * r;
                acc[1] += w * g;
                acc[2] += w * b;
                wsum += w;
            }
            stage[o] = acc[0] / wsum;
            stage[o + 1] = acc[1] / wsum;
            stage[o + 2] = acc[2] / wsum;
        }
    }

    // ---- spatial pass (bilateral in Y'CbCr, alpha-weighted) ----
    let mut out = stage.clone();
    if p.radius > 0 && (p.luma > 0.0 || p.chroma > 0.0) {
        let r = p.radius as isize;
        let sig_s = (p.radius as f32 * 0.5).max(0.5);
        let inv_s = 1.0 / (2.0 * sig_s * sig_s);
        let sig_l = 0.005 + 0.10 * p.luma;
        let sig_c = 0.005 + 0.12 * p.chroma;
        let inv_l = 1.0 / (2.0 * sig_l * sig_l);
        let inv_c = 1.0 / (2.0 * sig_c * sig_c);
        // precompute YCC of stage
        let mut ycc = vec![[0.0f32; 3]; npx];
        for i in 0..npx {
            let o = i * 4;
            ycc[i] = to_ycc(stage[o], stage[o + 1], stage[o + 2]);
        }
        let (w_i, h_i) = (width as isize, height as isize);
        let pout = crate::par::SyncPtr(out.as_mut_ptr());
        let sp_tab: Vec<f32> = (-r..=r).flat_map(|dy| (-r..=r).map(move |dx| (dx, dy))).map(|(dx, dy)| (-((dx * dx + dy * dy) as f32) * inv_s).exp()).collect();
        let (cur_r, ycc_r, stage_r, sp_r) = (cur, &ycc, &stage, &sp_tab);
        crate::par::ranges(height, 4, |y0, y1| { let pout = pout; for y in (y0 as isize)..(y1 as isize) {
            for x in 0..w_i {
                let i = (y * w_i + x) as usize;
                let o = i * 4;
                let a0 = cur[o + 3];
                let c = ycc[i];
                if !(a0 > 0.0) || !c[0].is_finite() || !c[1].is_finite() || !c[2].is_finite() {
                    continue;
                }
                let (mut sy, mut sb, mut sr) = (0.0f32, 0.0f32, 0.0f32);
                let (mut wy, mut wc) = (0.0f32, 0.0f32);
                for dy in -r..=r {
                    let yy = y + dy;
                    if yy < 0 || yy >= h_i {
                        continue;
                    }
                    for dx in -r..=r {
                        let xx = x + dx;
                        if xx < 0 || xx >= w_i {
                            continue;
                        }
                        let j = (yy * w_i + xx) as usize;
                        let na = cur[j * 4 + 3];
                        let q = ycc[j];
                        if !(na > 0.0) || !q[0].is_finite() || !q[1].is_finite() || !q[2].is_finite() {
                            continue;
                        }
                        let ws = sp_r[((dy + r) * (2 * r + 1) + (dx + r)) as usize] * na.min(1.0);
                        let dyl = q[0] - c[0];
                        let dcb = q[1] - c[1];
                        let dcr = q[2] - c[2];
                        // luma range kernel on luma difference only (keeps edges)
                        let w_y = ws * (-(dyl * dyl) * inv_l).exp();
                        // chroma kernel: chroma diff plus luma diff so colors don't bleed over edges
                        let w_c = ws * (-(dcb * dcb + dcr * dcr) * inv_c - (dyl * dyl) * inv_l).exp();
                        sy += w_y * q[0];
                        wy += w_y;
                        sb += w_c * q[1];
                        sr += w_c * q[2];
                        wc += w_c;
                    }
                }
                // centre always has weight >= its own alpha-term > 0, so wy,wc > 0
                let ny = if wy > 0.0 { sy / wy } else { c[0] };
                let ncb = if wc > 0.0 { sb / wc } else { c[1] };
                let ncr = if wc > 0.0 { sr / wc } else { c[2] };
                let rgb = from_ycc(ny, ncb, ncr);
                unsafe { *pout.0.add(o) = finite_or(rgb[0], stage_r[o]); *pout.0.add(o + 1) = finite_or(rgb[1], stage_r[o + 1]); *pout.0.add(o + 2) = finite_or(rgb[2], stage_r[o + 2]); }
            }
        } });
    }

    // ---- mix with original by amount; alpha untouched ----
    for i in 0..npx {
        let o = i * 4;
        if !(cur[o + 3] > 0.0) {
            out[o..o + 4].copy_from_slice(&cur[o..o + 4]);
            continue;
        }
        for k in 0..3 {
            out[o + k] = cur[o + k] + p.amount * (out[o + k] - cur[o + k]);
        }
        out[o + 3] = cur[o + 3];
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // deterministic LCG noise, ~N(0, s) via sum of 4 uniforms
    struct Lcg(u64);
    impl Lcg {
        fn u(&mut self) -> f32 {
            self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
            ((self.0 >> 40) as f32) / (1u64 << 24) as f32
        }
        fn g(&mut self, s: f32) -> f32 {
            ((self.u() + self.u() + self.u() + self.u()) - 2.0) * s * 1.732
        }
    }

    const W: usize = 48;
    const H: usize = 32;

    fn clean_frame() -> Vec<f32> {
        let mut v = vec![0.0; W * H * 4];
        for y in 0..H {
            for x in 0..W {
                let o = (y * W + x) * 4;
                // smooth gradient + a hard vertical edge at x = W/2
                let base = if x < W / 2 { 0.25 } else { 0.7 } + 0.1 * (y as f32 / H as f32);
                v[o] = base;
                v[o + 1] = base * 0.9;
                v[o + 2] = base * 0.8;
                v[o + 3] = 1.0;
            }
        }
        v
    }

    fn noisy(clean: &[f32], seed: u64, s: f32) -> Vec<f32> {
        let mut r = Lcg(seed);
        let mut v = clean.to_vec();
        for i in 0..W * H {
            for k in 0..3 {
                v[i * 4 + k] += r.g(s);
            }
        }
        v
    }

    fn mse(a: &[f32], b: &[f32]) -> f32 {
        let mut s = 0.0;
        let mut c = 0.0;
        for i in 0..a.len() / 4 {
            for k in 0..3 {
                s += (a[i * 4 + k] - b[i * 4 + k]).powi(2);
                c += 1.0;
            }
        }
        s / c
    }

    fn on(amount: f32) -> DenoiseParams {
        DenoiseParams { amount, ..Default::default() }
    }

    #[test]
    fn amount_zero_is_bit_identity() {
        let f = noisy(&clean_frame(), 1, 0.05);
        let o = denoise_frame(&f, &[&f], W, H, &on(0.0));
        assert!(f.iter().zip(&o).all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    #[test]
    fn deterministic() {
        let f = noisy(&clean_frame(), 2, 0.05);
        let p = noisy(&clean_frame(), 3, 0.05);
        let a = denoise_frame(&f, &[&p], W, H, &on(1.0));
        let b = denoise_frame(&f, &[&p], W, H, &on(1.0));
        assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()));
    }

    #[test]
    fn reduces_noise_spatial_only() {
        let c = clean_frame();
        let f = noisy(&c, 4, 0.05);
        let p = DenoiseParams { amount: 1.0, temporal: 0.0, ..Default::default() };
        let o = denoise_frame(&f, &[], W, H, &p);
        let (before, after) = (mse(&f, &c), mse(&o, &c));
        assert!(after < before * 0.6, "before {before} after {after}");
    }

    #[test]
    fn reduces_noise_temporal_and_spatial() {
        let c = clean_frame();
        let f = noisy(&c, 5, 0.05);
        let n1 = noisy(&c, 6, 0.05);
        let n2 = noisy(&c, 7, 0.05);
        let o = denoise_frame(&f, &[&n1, &n2], W, H, &on(1.0));
        let spatial = denoise_frame(&f, &[], W, H, &DenoiseParams { amount: 1.0, temporal: 0.0, ..Default::default() });
        assert!(mse(&o, &c) < mse(&f, &c) * 0.4);
        assert!(mse(&o, &c) < mse(&spatial, &c), "temporal should add benefit");
    }

    #[test]
    fn preserves_hard_edge() {
        let c = clean_frame();
        let f = noisy(&c, 8, 0.02);
        let o = denoise_frame(&f, &[], W, H, &on(1.0));
        let y = H / 2;
        let l = o[(y * W + W / 2 - 1) * 4];
        let r = o[(y * W + W / 2) * 4];
        assert!(r - l > 0.35, "edge collapsed: {l} {r}");
    }

    #[test]
    fn motion_not_ghosted() {
        let c = clean_frame();
        let mut moved = c.clone();
        for i in 0..W * H {
            moved[i * 4] = 1.0 - moved[i * 4]; // very different neighbour
        }
        let p = DenoiseParams { amount: 1.0, radius: 0, ..Default::default() };
        let o = denoise_frame(&c, &[&moved], W, H, &p);
        assert!(mse(&o, &c) < 1e-4);
    }

    #[test]
    fn alpha_preserved_and_transparent_untouched() {
        let mut f = noisy(&clean_frame(), 9, 0.05);
        for i in 0..W * H {
            f[i * 4 + 3] = if i % 5 == 0 { 0.0 } else { 0.5 };
        }
        // transparent pixels carry garbage colour that must not leak
        for i in (0..W * H).step_by(5) {
            f[i * 4] = 9.0;
        }
        let o = denoise_frame(&f, &[&f], W, H, &on(1.0));
        for i in 0..W * H {
            assert_eq!(o[i * 4 + 3].to_bits(), f[i * 4 + 3].to_bits());
            if f[i * 4 + 3] == 0.0 {
                assert_eq!(o[i * 4].to_bits(), f[i * 4].to_bits());
            } else {
                assert!(o[i * 4] < 2.0, "transparent colour leaked");
            }
        }
    }

    #[test]
    fn hostile_params_do_not_panic_or_nan() {
        let f = noisy(&clean_frame(), 10, 0.05);
        let bad = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 1e30];
        for &a in &bad {
            let p = DenoiseParams { amount: a, luma: a, chroma: a, radius: u32::MAX, temporal: a };
            let o = denoise_frame(&f, &[&f], W, H, &p);
            assert_eq!(o.len(), f.len());
            assert!(o.iter().all(|v| v.is_finite()));
        }
    }

    #[test]
    fn hostile_buffers() {
        let f = clean_frame();
        assert_eq!(denoise_frame(&f, &[], 0, 0, &on(1.0)), f);
        assert_eq!(denoise_frame(&f[..10], &[], W, H, &on(1.0)), f[..10].to_vec());
        assert_eq!(denoise_frame(&f, &[], usize::MAX, 2, &on(1.0)), f);
        let short = vec![0.0f32; 7];
        let o = denoise_frame(&f, &[&short], W, H, &on(1.0));
        assert_eq!(o.len(), f.len());
        let mut nanf = f.clone();
        nanf[0] = f32::NAN;
        let o = denoise_frame(&nanf, &[], W, H, &on(1.0));
        assert!(o[4..].iter().all(|v| v.is_finite()));
    }

    #[test]
    fn flat_frame_is_stable() {
        let mut f = vec![0.5f32; W * H * 4];
        for i in 0..W * H {
            f[i * 4 + 3] = 1.0;
        }
        let o = denoise_frame(&f, &[&f], W, H, &on(1.0));
        assert!(mse(&o, &f) < 1e-8);
    }

    #[test]
    fn ycc_roundtrip() {
        let p = to_ycc(0.2, 0.6, 0.9);
        let r = from_ycc(p[0], p[1], p[2]);
        assert!((r[0] - 0.2).abs() < 1e-5 && (r[1] - 0.6).abs() < 1e-5 && (r[2] - 0.9).abs() < 1e-5);
    }
}

/// Effect adapter: spatial-only (single frame; temporal neighbours are not available to effects, so `temporal` is not exposed).
pub fn apply(pr: &crate::effects::Params, f: &mut crate::effects::Frame, t: f64) -> Result<(), String> {
    super::frame::validate(f)?;
    let g = |k: &str, d: f64, lo: f32, hi: f32| -> Result<f32, String> { let v = crate::effects::p(pr, k, d, t); if !v.is_finite() || v < lo || v > hi { return Err(format!("invalid denoise {k}")); } Ok(v) };
    let (amount, luma, chroma, radius) = (g("amount", 0.0, 0., 1.)?, g("luma", 0.5, 0., 1.)?, g("chroma", 0.7, 0., 1.)?, g("radius", 2.0, 0., 4.)?);
    if amount == 0.0 { return Ok(()); }
    let o = denoise_frame(&f.d, &[], f.w, f.h, &DenoiseParams { amount, luma, chroma, radius: radius.round() as u32, temporal: 0.0 });
    if o.len() == f.d.len() { f.d = o; } Ok(())
}
pub fn catalog() -> Vec<super::Entry> { vec![("denoise", "Opt-in spatial colour denoise (edge-aware, Rec709 Y'CbCr, alpha untouched, deterministic). amount 0 = exact identity. Single-frame: no temporal pass in the effect path. Draft-tier, not benchmarked against a reference denoiser.", &[("amount", 0.0, "0..1"), ("luma", 0.5, "0..1"), ("chroma", 0.7, "0..1"), ("radius", 2.0, "0..4 px")])] }
