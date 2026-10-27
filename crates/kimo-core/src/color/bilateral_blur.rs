//! Standalone bilateral (edge-preserving) blur effect -- Resolve "Surface Blur" / Premiere "Bilateral Blur" style
//! general-purpose smoothing: flattens flat regions while keeping hard edges, unlike a Gaussian blur which smears
//! across them. Distinct from `denoise`'s internal bilateral pass, which is tuned narrow (radius 0..4) for noise
//! suppression; this is tuned for a visible, dial-able smoothing *look* (radius 0..10, a `smoothness` range-sigma
//! control), as its own registered effect kind rather than only living inside the denoise tool (FEATURES.md L131).
//!
//! Contract (same as denoise.rs): RGBA f32, straight alpha, row-major, Rec709 display-referred (gamma-encoded)
//! values, nominal 0..=1. `amount == 0` or `radius == 0` returns a bit-exact copy. Alpha is never modified; alpha-0
//! pixels are untouched and never contribute to neighbours (alpha-weighted contribution). Deterministic:
//! single-threaded, fixed iteration order, no RNG, no time. Hostile params are sanitized; bad dimensions or a short
//! buffer return the input unchanged.
//!
//! Window is a direct O(r^2) bilateral kernel (not separable -- the range term depends on both pixels, so it can't
//! be split into two 1D passes the way a plain Gaussian can). Radius is capped at 10px (21x21 = 441 taps/pixel) to
//! keep that honest on real footage; for a larger effective radius, chain two passes or use `skin_smooth`'s
//! guided-filter approach, which scales to a much bigger area cheaply by design.

use crate::effects::{Frame, Params};

#[inline]
fn to_ycc(r: f32, g: f32, b: f32) -> [f32; 3] {
    // Rec709 (BT.709) luma coefficients, full-range Y'CbCr. Duplicated from denoise.rs (both tiny, both private
    // there) rather than shared, so this module carries no dependency on denoise's internals.
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
fn finite_or(v: f32, f: f32) -> f32 { if v.is_finite() { v } else { f } }
#[inline]
fn san(v: f32, lo: f32, hi: f32, d: f32) -> f32 { if v.is_finite() { v.clamp(lo, hi) } else { d } }

/// User-facing parameters, sanitized (NaN/inf/out-of-range clamped to sane values, never panics).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BilateralBlurParams {
    /// Overall mix, 0 = identity, 1 = full effect.
    pub amount: f32,
    /// Window radius in pixels, 0..=10 (0 disables the effect).
    pub radius: u32,
    /// Range sigma (how much a neighbour's colour may differ from the centre and still blend in), 0..=1.
    /// Higher flattens more aggressively inside similar-colour regions while still respecting hard edges.
    pub smoothness: f32,
}
impl Default for BilateralBlurParams {
    fn default() -> Self { BilateralBlurParams { amount: 0.0, radius: 4, smoothness: 0.5 } }
}
impl BilateralBlurParams {
    pub fn sanitized(self) -> Self {
        BilateralBlurParams {
            amount: san(self.amount, 0.0, 1.0, 0.0),
            radius: if self.radius == u32::MAX { 10 } else { self.radius.min(10) },
            smoothness: san(self.smoothness, 0.0, 1.0, 0.5),
        }
    }
}

/// Bilateral-blur one frame. Returns a new buffer of `cur.len()`; hostile dimensions/buffers return `cur` unchanged.
pub fn bilateral_blur_frame(cur: &[f32], width: usize, height: usize, params: &BilateralBlurParams) -> Vec<f32> {
    let npx = width.checked_mul(height).unwrap_or(0);
    if npx == 0 || cur.len() < npx.saturating_mul(4) {
        return cur.to_vec();
    }
    let p = params.sanitized();
    if p.amount <= 0.0 || p.radius == 0 {
        return cur.to_vec();
    }
    let r = p.radius as isize;
    let sig_s = (p.radius as f32 * 0.5).max(0.5);
    let inv_s = 1.0 / (2.0 * sig_s * sig_s);
    // Range sigma: a much wider span than denoise's noise-oriented 0.005..0.115 band, since this is meant to
    // visibly flatten texture/detail within a tone, not just shave off sensor noise.
    let sig_r = 0.02 + 0.5 * p.smoothness;
    let inv_r = 1.0 / (2.0 * sig_r * sig_r);

    let mut ycc = vec![[0.0f32; 3]; npx];
    for i in 0..npx {
        let o = i * 4;
        ycc[i] = to_ycc(finite_or(cur[o], 0.0), finite_or(cur[o + 1], 0.0), finite_or(cur[o + 2], 0.0));
    }

    let (w_i, h_i) = (width as isize, height as isize);
    let sp_tab: Vec<f32> = (-r..=r).flat_map(|dy| (-r..=r).map(move |dx| (dx, dy))).map(|(dx, dy)| (-((dx * dx + dy * dy) as f32) * inv_s).exp()).collect();

    let mut out = cur.to_vec();
    for y in 0..h_i {
        for x in 0..w_i {
            let i = (y * w_i + x) as usize;
            let o = i * 4;
            let a0 = cur[o + 3];
            if !(a0 > 0.0) {
                continue; // alpha untouched, colour left as-is on fully transparent pixels
            }
            let c = ycc[i];
            if !(c[0].is_finite() && c[1].is_finite() && c[2].is_finite()) {
                continue;
            }
            let (mut sy, mut scb, mut scr, mut wsum) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for dy in -r..=r {
                let yy = y + dy;
                if yy < 0 || yy >= h_i { continue; }
                for dx in -r..=r {
                    let xx = x + dx;
                    if xx < 0 || xx >= w_i { continue; }
                    let j = (yy * w_i + xx) as usize;
                    let na = cur[j * 4 + 3];
                    if !(na > 0.0) { continue; }
                    let q = ycc[j];
                    if !(q[0].is_finite() && q[1].is_finite() && q[2].is_finite()) { continue; }
                    let ws = sp_tab[((dy + r) * (2 * r + 1) + (dx + r)) as usize] * na.min(1.0);
                    let d2 = (q[0] - c[0]).powi(2) + (q[1] - c[1]).powi(2) + (q[2] - c[2]).powi(2);
                    let w = ws * (-d2 * inv_r).exp();
                    sy += w * q[0];
                    scb += w * q[1];
                    scr += w * q[2];
                    wsum += w;
                }
            }
            let (ny, ncb, ncr) = if wsum > 0.0 { (sy / wsum, scb / wsum, scr / wsum) } else { (c[0], c[1], c[2]) };
            let rgb = from_ycc(ny, ncb, ncr);
            for k in 0..3 {
                let blended = finite_or(rgb[k], cur[o + k]);
                out[o + k] = cur[o + k] + p.amount * (blended - cur[o + k]);
            }
            out[o + 3] = cur[o + 3];
        }
    }
    out
}

/// Effect adapter.
pub fn apply(pr: &Params, f: &mut Frame, t: f64) -> Result<(), String> {
    super::frame::validate(f)?;
    let g = |k: &str, d: f64, lo: f32, hi: f32| -> Result<f32, String> {
        let v = crate::effects::p(pr, k, d, t);
        if !v.is_finite() || v < lo || v > hi { return Err(format!("invalid bilateral_blur {k}")); }
        Ok(v)
    };
    let (amount, radius, smoothness) = (g("amount", 0.0, 0., 1.)?, g("radius", 4.0, 0., 10.)?, g("smoothness", 0.5, 0., 1.)?);
    if amount == 0.0 || radius == 0.0 { return Ok(()); }
    let o = bilateral_blur_frame(&f.d, f.w, f.h, &BilateralBlurParams { amount, radius: radius.round() as u32, smoothness });
    if o.len() == f.d.len() { f.d = o; }
    Ok(())
}

pub fn catalog() -> Vec<super::Entry> {
    vec![("bilateral_blur", "Edge-preserving smoothing blur (Resolve Surface Blur / Premiere Bilateral Blur style): flattens flat regions, keeps hard edges, unlike a Gaussian blur. Direct bilateral window, radius capped 0..10px for CPU cost. amount 0 = exact identity.", &[("amount", 0.0, "0..1"), ("radius", 4.0, "0..10 px"), ("smoothness", 0.5, "0..1 range tolerance -- higher flattens more inside similar tones")])]
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: usize = 40;
    const H: usize = 28;

    fn edge_frame() -> Vec<f32> {
        let mut v = vec![0.0f32; W * H * 4];
        for y in 0..H {
            for x in 0..W {
                let o = (y * W + x) * 4;
                let base = if x < W / 2 { 0.2 } else { 0.8 };
                v[o] = base; v[o + 1] = base * 0.9; v[o + 2] = base * 0.8; v[o + 3] = 1.0;
            }
        }
        v
    }

    // deterministic LCG, matches the style used across color/denoise.rs's own tests.
    struct Lcg(u64);
    impl Lcg {
        fn u(&mut self) -> f32 { self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); ((self.0 >> 40) as f32) / (1u64 << 24) as f32 }
        fn g(&mut self, s: f32) -> f32 { ((self.u() + self.u() + self.u() + self.u()) - 2.0) * s * 1.732 }
    }
    fn speckled(clean: &[f32], seed: u64, s: f32) -> Vec<f32> {
        let mut r = Lcg(seed);
        let mut v = clean.to_vec();
        for i in 0..W * H {
            for k in 0..3 { v[i * 4 + k] = (v[i * 4 + k] + r.g(s)).clamp(0.0, 1.0); }
        }
        v
    }
    fn mse(a: &[f32], b: &[f32]) -> f32 {
        let (mut s, mut c) = (0.0, 0.0);
        for i in 0..a.len() / 4 { for k in 0..3 { s += (a[i * 4 + k] - b[i * 4 + k]).powi(2); c += 1.0; } }
        s / c
    }

    #[test]
    fn amount_zero_is_bit_identity() {
        let f = speckled(&edge_frame(), 1, 0.05);
        let o = bilateral_blur_frame(&f, W, H, &BilateralBlurParams { amount: 0.0, radius: 4, smoothness: 0.5 });
        assert!(f.iter().zip(&o).all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    #[test]
    fn radius_zero_is_bit_identity() {
        let f = speckled(&edge_frame(), 1, 0.05);
        let o = bilateral_blur_frame(&f, W, H, &BilateralBlurParams { amount: 1.0, radius: 0, smoothness: 0.5 });
        assert!(f.iter().zip(&o).all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    #[test]
    fn deterministic() {
        let f = speckled(&edge_frame(), 2, 0.05);
        let p = BilateralBlurParams { amount: 1.0, radius: 5, smoothness: 0.6 };
        let a = bilateral_blur_frame(&f, W, H, &p);
        let b = bilateral_blur_frame(&f, W, H, &p);
        assert!(a.iter().zip(&b).all(|(x, y)| x.to_bits() == y.to_bits()));
    }

    #[test]
    fn flattens_speckle_inside_a_flat_region_more_than_gaussian_would_smear_the_edge() {
        let c = edge_frame();
        let f = speckled(&c, 3, 0.05);
        let p = BilateralBlurParams { amount: 1.0, radius: 5, smoothness: 0.6 };
        let o = bilateral_blur_frame(&f, W, H, &p);
        assert!(mse(&o, &c) < mse(&f, &c) * 0.5, "should meaningfully reduce speckle");
    }

    #[test]
    fn preserves_a_hard_edge() {
        let c = edge_frame();
        let f = speckled(&c, 4, 0.02);
        let p = BilateralBlurParams { amount: 1.0, radius: 6, smoothness: 0.5 };
        let o = bilateral_blur_frame(&f, W, H, &p);
        let y = H / 2;
        let l = o[(y * W + W / 2 - 1) * 4];
        let r = o[(y * W + W / 2) * 4];
        assert!(r - l > 0.4, "edge collapsed: left={l} right={r}");
    }

    #[test]
    fn alpha_untouched_and_transparent_pixels_left_alone() {
        let mut f = speckled(&edge_frame(), 5, 0.05);
        for i in 0..W * H { f[i * 4 + 3] = if i % 4 == 0 { 0.0 } else { 0.5 }; }
        for i in (0..W * H).step_by(4) { f[i * 4] = 7.0; } // garbage colour behind alpha 0, must not leak
        let o = bilateral_blur_frame(&f, W, H, &BilateralBlurParams { amount: 1.0, radius: 4, smoothness: 0.5 });
        for i in 0..W * H {
            assert_eq!(o[i * 4 + 3].to_bits(), f[i * 4 + 3].to_bits());
            if f[i * 4 + 3] == 0.0 { assert_eq!(o[i * 4].to_bits(), f[i * 4].to_bits()); }
            else { assert!(o[i * 4] < 2.0, "transparent colour leaked into a real pixel"); }
        }
    }

    #[test]
    fn hostile_params_do_not_panic_or_nan() {
        let f = speckled(&edge_frame(), 6, 0.05);
        let bad = [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, -5.0, 1e30];
        for &v in &bad {
            let p = BilateralBlurParams { amount: v, radius: u32::MAX, smoothness: v };
            let o = bilateral_blur_frame(&f, W, H, &p);
            assert_eq!(o.len(), f.len());
            assert!(o.iter().all(|x| x.is_finite()));
        }
    }

    #[test]
    fn hostile_buffers() {
        let f = edge_frame();
        let p = BilateralBlurParams { amount: 1.0, radius: 4, smoothness: 0.5 };
        assert_eq!(bilateral_blur_frame(&f, 0, 0, &p), f);
        assert_eq!(bilateral_blur_frame(&f[..10], W, H, &p), f[..10].to_vec());
        assert_eq!(bilateral_blur_frame(&f, usize::MAX, 2, &p), f);
    }

    #[test]
    fn radius_is_capped_at_ten() {
        let f = speckled(&edge_frame(), 7, 0.05);
        let a = bilateral_blur_frame(&f, W, H, &BilateralBlurParams { amount: 1.0, radius: 10, smoothness: 0.5 });
        let b = bilateral_blur_frame(&f, W, H, &BilateralBlurParams { amount: 1.0, radius: 500, smoothness: 0.5 });
        assert_eq!(a, b, "radius above the cap should behave exactly like the cap");
    }
}
