//! VR / 360 equirectangular effects (FEATURES.md L477).
//!
//! A 360 video's equirectangular frame wraps horizontally: column w-1 sits right next to column 0
//! on the actual sphere (longitude 360 == 0). Every ordinary per-pixel effect in effects.rs/effects2.rs
//! (box_blur3, warp(), sample()) CLAMPS at the frame edge instead, which puts a visible seam down the
//! back of the sphere the moment you blur, sharpen, glow or rotate across it. Each effect here is the
//! honest "VR" variant: the same underlying algorithm as its ordinary counterpart, but sampling wraps
//! horizontally instead of clamping, so the effect runs across the seam the way it actually would run
//! across any other point on a sphere.
//!
//! Honest limits, stated once here rather than per-function:
//! - Vertical (the poles) is CLAMPED, not wrapped. A true pole wrap would need a reflect-and-rotate-180
//!   (crossing a pole on an equirectangular map flips you to the opposite longitude), which none of
//!   these functions does. Blur/sharpen/denoise/glow therefore soften slightly less correctly right at
//!   the top/bottom couple of rows; this is a real, documented gap, not hidden behind a wrap that isn't
//!   actually there.
//! - box_blur_wrap is a direct O(w*h*(2r+1)) separable convolution, not the integral-image / SIMD path
//!   that box_blur3 uses. It is correct but not optimized for large radii on 4K+ frames; fine for the
//!   effect's actual target (360 footage, viewed through a VR headset, at modest blur radii).
//! - vr_rotate_sphere/vr_convert_projection/vr_plane_to_sphere assume an equirectangular (2:1-ish, full
//!   360x180) frame as their domain unless stated otherwise per function.

use crate::effects::{Frame, Params};
use std::f32::consts::PI;

fn p(params: &Params, k: &str, d: f64, t: f64) -> f32 { crate::effects::p(params, k, d, t) as f32 }

/// Bilinear sample that WRAPS horizontally (longitude) and CLAMPS vertically (poles).
pub fn sample_wrap(src: &[f32], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let wf = w as f32;
    let mut xx = x % wf;
    if xx < 0.0 { xx += wf; }
    let yy = y.clamp(0.0, h as f32 - 1.001);
    let x0 = xx as usize;
    let x1 = (x0 + 1) % w.max(1);
    let y0 = yy as usize;
    let y1 = (y0 + 1).min(h.saturating_sub(1));
    let (fx, fy) = (xx - x0 as f32, yy - y0 as f32);
    let mut o = [0f32; 4];
    for j in 0..4 {
        let a = src[(y0 * w + x0) * 4 + j];
        let b = src[(y0 * w + x1) * 4 + j];
        let c = src[(y1 * w + x0) * 4 + j];
        let d = src[(y1 * w + x1) * 4 + j];
        o[j] = (a + (b - a) * fx) * (1.0 - fy) + (c + (d - c) * fx) * fy;
    }
    o
}

fn warp_wrap(f: &mut Frame, m: impl Fn(f32, f32) -> (f32, f32)) {
    let src = f.d.clone();
    let (w, h) = (f.w, f.h);
    for y in 0..h {
        for x in 0..w {
            let (sx, sy) = m(x as f32, y as f32);
            let o = sample_wrap(&src, w, h, sx, sy);
            f.d[(y * w + x) * 4..][..4].copy_from_slice(&o);
        }
    }
}

/// Box blur, separable: horizontal pass WRAPS (longitude), vertical pass CLAMPS (poles).
pub fn box_blur_wrap(f: &mut Frame, r: usize) {
    if r == 0 || f.w == 0 || f.h == 0 { return; }
    let (w, h) = (f.w, f.h);
    let ri = r as i32;
    let win = (2 * r + 1) as f32;
    let mut tmp = vec![0f32; f.d.len()];
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f32; 4];
            for k in -ri..=ri {
                let mut sx = (x as i32 + k) % w as i32;
                if sx < 0 { sx += w as i32; }
                let o = (y * w + sx as usize) * 4;
                for j in 0..4 { acc[j] += f.d[o + j]; }
            }
            let o = (y * w + x) * 4;
            for j in 0..4 { tmp[o + j] = acc[j] / win; }
        }
    }
    for y in 0..h {
        for x in 0..w {
            let mut acc = [0f32; 4];
            for k in -ri..=ri {
                let sy = (y as i32 + k).clamp(0, h as i32 - 1) as usize;
                let o = (sy * w + x) * 4;
                for j in 0..4 { acc[j] += tmp[o + j]; }
            }
            let o = (y * w + x) * 4;
            for j in 0..4 { f.d[o + j] = acc[j] / win; }
        }
    }
}

// --- direction <-> equirectangular lon/lat ---------------------------------------------------

/// Destination equirect pixel (x,y) -> unit direction vector. lon in [-pi,pi), lat in [-pi/2,pi/2].
/// +Z is "forward" (frame centre), +X is right, +Y is up -- a right-handed camera-style frame.
fn equirect_to_dir(x: f32, y: f32, w: f32, h: f32) -> (f32, f32, f32) {
    let lon = (x / w) * 2.0 * PI - PI;
    let lat = PI / 2.0 - (y / h) * PI;
    let (sl, cl) = lon.sin_cos();
    let (sa, ca) = lat.sin_cos();
    (ca * sl, sa, ca * cl)
}

fn dir_to_equirect(dx: f32, dy: f32, dz: f32, w: f32, h: f32) -> (f32, f32) {
    let lon = dx.atan2(dz);
    let lat = dy.clamp(-1.0, 1.0).asin();
    let x = (lon + PI) / (2.0 * PI) * w;
    let y = (PI / 2.0 - lat) / PI * h;
    (x, y)
}

fn rot_yaw(d: (f32, f32, f32), a: f32) -> (f32, f32, f32) {
    let (s, c) = a.sin_cos();
    (d.0 * c + d.2 * s, d.1, -d.0 * s + d.2 * c)
}
fn rot_pitch(d: (f32, f32, f32), a: f32) -> (f32, f32, f32) {
    let (s, c) = a.sin_cos();
    (d.0, d.1 * c - d.2 * s, d.1 * s + d.2 * c)
}
fn rot_roll(d: (f32, f32, f32), a: f32) -> (f32, f32, f32) {
    let (s, c) = a.sin_cos();
    (d.0 * c - d.1 * s, d.0 * s + d.1 * c, d.2)
}

/// VR Rotate Sphere: re-orient a full equirectangular frame by yaw/pitch/roll (degrees), as if the
/// 360 camera itself had been turned. Real spherical rotation (direction-vector rotate, not a 2D warp),
/// horizontal-wrap sampled so the seam moves cleanly with the rotation instead of tearing.
pub fn rotate_sphere(f: &mut Frame, yaw_deg: f32, pitch_deg: f32, roll_deg: f32) {
    let (w, h) = (f.w as f32, f.h as f32);
    let (yaw, pitch, roll) = (yaw_deg.to_radians(), pitch_deg.to_radians(), roll_deg.to_radians());
    warp_wrap(f, |x, y| {
        let d = equirect_to_dir(x, y, w, h);
        // Inverse mapping: dest pixel's ray, rotated by the INVERSE of the requested rotation,
        // tells us which source direction to sample (apply rotations in reverse order, negated).
        let d = rot_roll(d, -roll);
        let d = rot_pitch(d, -pitch);
        let d = rot_yaw(d, -yaw);
        dir_to_equirect(d.0, d.1, d.2, w, h)
    });
}

/// VR Convert Projection: equirectangular <-> fisheye.
/// mode 0: dest is fisheye (square-ish, circular image), source is equirectangular.
/// mode 1: dest is equirectangular, source is fisheye.
/// fov_deg is the fisheye's field of view (180 = full hemisphere, the common VR180/GoPro case).
pub fn convert_projection(f: &mut Frame, mode: f32, fov_deg: f32) {
    let (w, h) = (f.w as f32, f.h as f32);
    let fov = fov_deg.to_radians().max(0.01);
    if mode < 0.5 {
        // dest fisheye <- source equirect
        let (cx, cy) = (w * 0.5, h * 0.5);
        let radius = cx.min(cy);
        let src = f.d.clone();
        let (fw, fh) = (f.w, f.h);
        for y in 0..f.h {
            for x in 0..f.w {
                let (nx, ny) = ((x as f32 - cx) / radius, (y as f32 - cy) / radius);
                let r = (nx * nx + ny * ny).sqrt();
                let o = (y * fw + x) * 4;
                if r > 1.0 {
                    f.d[o] = 0.0; f.d[o + 1] = 0.0; f.d[o + 2] = 0.0; f.d[o + 3] = 0.0;
                    continue;
                }
                let theta = r * (fov * 0.5); // angle from forward
                let phi = ny.atan2(nx);
                let (st, ct) = theta.sin_cos();
                let (sp, cp) = phi.sin_cos();
                let d = (st * cp, st * sp, ct);
                let (sx, sy) = dir_to_equirect(d.0, d.1, d.2, fw as f32, fh as f32);
                let px = sample_wrap(&src, fw, fh, sx, sy);
                f.d[o..o + 4].copy_from_slice(&px);
            }
        }
    } else {
        // dest equirect <- source fisheye (clamped sample: a fisheye frame does not wrap)
        let src = f.d.clone();
        let (fw, fh) = (f.w, f.h);
        let (cx, cy) = (fw as f32 * 0.5, fh as f32 * 0.5);
        let radius = cx.min(cy);
        for y in 0..f.h {
            for x in 0..f.w {
                let d = equirect_to_dir(x as f32, y as f32, w, h);
                let theta = d.2.clamp(-1.0, 1.0).acos(); // angle from forward (+Z)
                let o = (y * fw + x) * 4;
                if theta > fov * 0.5 {
                    f.d[o] = 0.0; f.d[o + 1] = 0.0; f.d[o + 2] = 0.0; f.d[o + 3] = 0.0;
                    continue;
                }
                let phi = d.1.atan2(d.0);
                let r = theta / (fov * 0.5);
                let (sxn, syn) = (r * phi.cos(), r * phi.sin());
                let sx = cx + sxn * radius;
                let sy = cy + syn * radius;
                let px = crate::effects2::sample(&src, fw, fh, sx, sy);
                f.d[o..o + 4].copy_from_slice(&px);
            }
        }
    }
}

/// VR Plane to Sphere: treat THIS clip's own (flat, rectilinear) frame content as footage shot through
/// a camera of `fov_h_deg` horizontal field of view pointed at (yaw_deg, pitch_deg), and remap it onto
/// its proper patch of a full equirectangular canvas of the same pixel dimensions. Everywhere outside
/// that patch becomes transparent (alpha 0) -- this is a single plane being placed on the sphere, not
/// a full-sphere source.
pub fn plane_to_sphere(f: &mut Frame, yaw_deg: f32, pitch_deg: f32, fov_h_deg: f32) {
    let (w, h) = (f.w as f32, f.h as f32);
    let (yaw, pitch) = (yaw_deg.to_radians(), pitch_deg.to_radians());
    let fov_h = fov_h_deg.to_radians().clamp(1f32.to_radians(), 170f32.to_radians());
    let max_gx = (fov_h * 0.5).tan();
    let max_gy = max_gx * (h / w); // vertical fov derived from aspect, square-pixel assumption
    let src = f.d.clone();
    let (fw, fh) = (f.w, f.h);
    for y in 0..f.h {
        for x in 0..f.w {
            let d = equirect_to_dir(x as f32, y as f32, w, h);
            // rotate dest direction into the plane camera's local frame (inverse yaw/pitch)
            let d = rot_pitch(d, -pitch);
            let d = rot_yaw(d, -yaw);
            let o = (y * fw + x) * 4;
            if d.2 <= 1e-4 {
                f.d[o] = 0.0; f.d[o + 1] = 0.0; f.d[o + 2] = 0.0; f.d[o + 3] = 0.0;
                continue;
            }
            let gx = d.0 / d.2;
            let gy = d.1 / d.2;
            if gx.abs() > max_gx || gy.abs() > max_gy {
                f.d[o] = 0.0; f.d[o + 1] = 0.0; f.d[o + 2] = 0.0; f.d[o + 3] = 0.0;
                continue;
            }
            let u = (gx / max_gx) * 0.5 + 0.5;
            let v = 0.5 - (gy / max_gy) * 0.5; // image y grows downward, local y grows up
            let px = crate::effects2::sample(&src, fw, fh, u * w, v * h);
            f.d[o..o + 4].copy_from_slice(&px);
        }
    }
}

/// VR Blur: box_blur_wrap, no pole/seam clamp artifact.
pub fn vr_blur(f: &mut Frame, pr: &Params, t: f64) {
    let r = p(pr, "radius", 8.0, t).max(0.0) as usize;
    box_blur_wrap(f, r);
}

/// VR Sharpen: unsharp mask using the wrap-aware blur as the low-pass.
pub fn vr_sharpen(f: &mut Frame, pr: &Params, t: f64) {
    let a = p(pr, "amount", 0.8, t);
    let r = p(pr, "radius", 2.0, t).max(0.0) as usize;
    let mut b = Frame { w: f.w, h: f.h, d: f.d.clone() };
    box_blur_wrap(&mut b, r);
    for i in 0..f.w * f.h {
        for j in 0..3 { f.d[i * 4 + j] += (f.d[i * 4 + j] - b.d[i * 4 + j]) * a; }
    }
}

/// VR Denoise: a simple wrap-aware spatial-only smoothing mixed in by `amount` (0 = untouched,
/// 1 = full box_blur_wrap at the given radius). Honest limit: no luma/chroma split and no temporal
/// pass, unlike the main `denoise` effect (color/denoise.rs) -- this exists for the seam-free property,
/// not as a richer denoiser.
pub fn vr_denoise(f: &mut Frame, pr: &Params, t: f64) {
    let amount = p(pr, "amount", 0.5, t).clamp(0.0, 1.0);
    let r = p(pr, "radius", 2.0, t).max(0.0) as usize;
    if amount <= 0.0 || r == 0 { return; }
    let mut b = Frame { w: f.w, h: f.h, d: f.d.clone() };
    box_blur_wrap(&mut b, r);
    for i in 0..f.w * f.h {
        for j in 0..3 { f.d[i * 4 + j] += (b.d[i * 4 + j] - f.d[i * 4 + j]) * amount; }
    }
}

/// VR Glow: threshold bright-pass, wrap-aware blur, additive -- same recipe as the plain `glow`
/// effect (effects.rs) but the bloom itself wraps around the 360 seam instead of dimming near it.
pub fn vr_glow(f: &mut Frame, pr: &Params, t: f64) {
    let amount = p(pr, "amount", 0.5, t);
    let r = p(pr, "radius", 12.0, t).max(0.0) as usize;
    let th = p(pr, "threshold", 0.7, t);
    let mut b = Frame { w: f.w, h: f.h, d: f.d.clone() };
    for i in 0..f.w * f.h {
        let c = [b.d[i * 4], b.d[i * 4 + 1], b.d[i * 4 + 2]];
        let l = crate::effects::lum(c);
        let k = ((l - th) / (1.0 - th).max(0.01)).clamp(0.0, 1.0);
        for j in 0..3 { b.d[i * 4 + j] *= k; }
    }
    box_blur_wrap(&mut b, r);
    for i in 0..f.w * f.h {
        for j in 0..3 { f.d[i * 4 + j] += b.d[i * 4 + j] * amount; }
    }
}

/// VR Chromatic Aberrations: the same radial R/B channel split as the ordinary `chromatic_aberration`
/// effect, but sampled with horizontal wrap so a frame edge near the seam fringes the same as anywhere
/// else on the sphere instead of clamping (smearing) the fringe into the last column.
pub fn vr_chromatic_aberration(f: &mut Frame, pr: &Params, t: f64) {
    let amount = p(pr, "amount", 4.0, t);
    let cx = p(pr, "cx", 0.5, t) * f.w as f32;
    let cy = p(pr, "cy", 0.5, t) * f.h as f32;
    let radial_scale = p(pr, "radial_scale", 2.0, t);
    let r_scale = p(pr, "r_scale", 1.0, t);
    let b_scale = p(pr, "b_scale", -1.0, t);
    let (w, h) = (f.w, f.h);
    let src = f.d.clone();
    for y in 0..h {
        for x in 0..w {
            let (xf, yf) = (x as f32, y as f32);
            let o = (y * w + x) * 4;
            for (ch, scale) in [(0usize, r_scale), (2usize, b_scale)] {
                let shift = amount * radial_scale * scale;
                let sx = xf + (xf / w as f32 - cx / w as f32) * shift;
                let sy = yf + (yf / h as f32 - cy / h as f32) * shift;
                let px = sample_wrap(&src, w, h, sx, sy);
                f.d[o + ch] = px[ch];
            }
        }
    }
}
