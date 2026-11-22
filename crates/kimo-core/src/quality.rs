//! Render quality: the project's `render_quality` ("legacy" or "v10"), carried through one frame's render.
//!
//! Effects, transitions and the resampler do not receive the project, so render::render_frame_opts sets this
//! context for the frame it renders (and restores the previous one when it ends, so a nested precomp render
//! cannot leak its setting). It is per thread: code that fans work out to other threads (`par::rows_mut`) reads
//! it first and passes the values in.
//!
//! "legacy" is every project saved before v10 and renders exactly as before. "v10" turns on the v10 algorithms:
//! linear-light blending, Gaussian blurs, bicubic magnification, transition motion blur, thin antialiased wipe
//! edges, per-frame dither, and sizes given for 1080p that scale with the frame (`px`).
use std::cell::Cell;

thread_local! { static CTX: Cell<(bool, f32)> = const { Cell::new((false, 1.0)) }; }

/// Restores the previous context when dropped.
pub struct Scope((bool, f32));
impl Drop for Scope { fn drop(&mut self) { CTX.with(|c| c.set(self.0)); } }

/// Render with v10 quality (or not) at this frame height until the returned scope is dropped.
pub fn enter(v10: bool, height: usize) -> Scope {
    let prev = CTX.with(|c| c.get());
    CTX.with(|c| c.set((v10, if height > 0 { height as f32 / 1080.0 } else { 1.0 })));
    Scope(prev)
}

/// True while rendering a v10 project.
pub fn v10() -> bool { CTX.with(|c| c.get().0) }

/// A size given for a 1080p frame, scaled to this frame in v10 (unchanged in legacy), so a look is the same
/// at 720p and at 4K.
pub fn px(v: f32) -> f32 { let (on, s) = CTX.with(|c| c.get()); if on { v * s } else { v } }

/// Separable Gaussian blur of one plane (mask coverage), edges clamped, exact for fractional sigma.
pub fn gauss_plane(c: &mut [f32], w: usize, h: usize, sigma: f32) {
    if !(sigma > 0.05) || w == 0 || h == 0 { return; }
    let r = (sigma * 3.0).ceil().max(1.0) as isize;
    let ker: Vec<f32> = { let v: Vec<f32> = (-r..=r).map(|i| (-(i * i) as f32 / (2.0 * sigma * sigma)).exp()).collect(); let t: f32 = v.iter().sum(); v.into_iter().map(|x| x / t).collect() };
    let mut tmp = vec![0f32; c.len()];
    { let (src, ker) = (&*c, &ker); crate::par::rows_mut(&mut tmp, w, |y, out| for x in 0..w { out[x] = ker.iter().enumerate().map(|(k, kv)| src[y * w + (x as isize + k as isize - r).clamp(0, w as isize - 1) as usize] * kv).sum(); }); }
    { let (src, ker) = (&tmp, &ker); crate::par::rows_mut(c, w, |y, out| for x in 0..w { out[x] = ker.iter().enumerate().map(|(k, kv)| src[(y as isize + k as isize - r).clamp(0, h as isize - 1) as usize * w + x] * kv).sum(); }); }
}

/// sRGB encoded value to linear light (exact curve; values past 0..1 extend it).
#[inline] pub fn to_linear(v: f32) -> f32 { if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }
/// Linear light to the sRGB encoded value.
#[inline] pub fn to_srgb(v: f32) -> f32 { if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 } }

/// Linear sRGB to OkLab (Björn Ottosson): perceptual lightness and chroma, so saturation can change without
/// shifting hue or brightness.
pub fn oklab(c: [f32; 3]) -> [f32; 3] {
    let l = 0.412_221_47 * c[0] + 0.536_332_55 * c[1] + 0.051_445_995 * c[2];
    let m = 0.211_903_5 * c[0] + 0.680_699_5 * c[1] + 0.107_396_96 * c[2];
    let s = 0.088_302_46 * c[0] + 0.281_718_85 * c[1] + 0.629_978_7 * c[2];
    let (l, m, s) = (l.max(0.0).cbrt(), m.max(0.0).cbrt(), s.max(0.0).cbrt());
    [0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s, 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s, 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s]
}
/// OkLab back to linear sRGB.
pub fn oklab_inv(c: [f32; 3]) -> [f32; 3] {
    let l = c[0] + 0.396_337_78 * c[1] + 0.215_803_76 * c[2];
    let m = c[0] - 0.105_561_346 * c[1] - 0.063_854_17 * c[2];
    let s = c[0] - 0.089_484_18 * c[1] - 1.291_485_5 * c[2];
    let (l, m, s) = (l * l * l, m * m * m, s * s * s);
    [4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s, -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s, -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_nest_and_restore() {
        assert!(!v10());
        { let _a = enter(true, 2160); assert!(v10()); assert_eq!(px(10.0), 20.0);
          { let _b = enter(false, 720); assert!(!v10()); assert_eq!(px(10.0), 10.0); }
          assert!(v10()); assert_eq!(px(10.0), 20.0); }
        assert!(!v10());
    }
    #[test]
    fn transfer_and_oklab_round_trip() {
        for v in [0.0f32, 0.002, 0.04, 0.2, 0.5, 0.9, 1.0] { assert!((to_srgb(to_linear(v)) - v).abs() < 1e-5, "{v}"); }
        for c in [[0.2f32, 0.5, 0.8], [1.0, 0.0, 0.0], [0.18, 0.18, 0.18]] { let b = oklab_inv(oklab(c)); for j in 0..3 { assert!((b[j] - c[j]).abs() < 1e-4, "{c:?} -> {b:?}"); } }
        assert!(oklab([0.18; 3])[1].abs() < 1e-4 && oklab([0.18; 3])[2].abs() < 1e-4, "grey has no chroma");
    }
}
