// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Layer blend modes.

use serde::{Deserialize, Serialize};

use crate::color::luminance;

/// The blend modes of the Layer ▸ Blending Mode menu, in menu order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum BlendMode {
    #[default]
    Normal,
    Dissolve,
    DancingDissolve,
    Darken,
    Multiply,
    ColorBurn,
    ClassicColorBurn,
    LinearBurn,
    DarkerColor,
    Add,
    Lighten,
    Screen,
    ColorDodge,
    ClassicColorDodge,
    LinearDodge,
    LighterColor,
    Overlay,
    SoftLight,
    HardLight,
    LinearLight,
    VividLight,
    PinLight,
    HardMix,
    Difference,
    ClassicDifference,
    Exclusion,
    Subtract,
    Divide,
    Hue,
    Saturation,
    Color,
    Luminosity,
    StencilAlpha,
    StencilLuma,
    SilhouetteAlpha,
    SilhouetteLuma,
    AlphaAdd,
    LuminescentPremul,
}

impl BlendMode {
    pub const ALL: [BlendMode; 38] = [
        BlendMode::Normal,
        BlendMode::Dissolve,
        BlendMode::DancingDissolve,
        BlendMode::Darken,
        BlendMode::Multiply,
        BlendMode::ColorBurn,
        BlendMode::ClassicColorBurn,
        BlendMode::LinearBurn,
        BlendMode::DarkerColor,
        BlendMode::Add,
        BlendMode::Lighten,
        BlendMode::Screen,
        BlendMode::ColorDodge,
        BlendMode::ClassicColorDodge,
        BlendMode::LinearDodge,
        BlendMode::LighterColor,
        BlendMode::Overlay,
        BlendMode::SoftLight,
        BlendMode::HardLight,
        BlendMode::LinearLight,
        BlendMode::VividLight,
        BlendMode::PinLight,
        BlendMode::HardMix,
        BlendMode::Difference,
        BlendMode::ClassicDifference,
        BlendMode::Exclusion,
        BlendMode::Subtract,
        BlendMode::Divide,
        BlendMode::Hue,
        BlendMode::Saturation,
        BlendMode::Color,
        BlendMode::Luminosity,
        BlendMode::StencilAlpha,
        BlendMode::StencilLuma,
        BlendMode::SilhouetteAlpha,
        BlendMode::SilhouetteLuma,
        BlendMode::AlphaAdd,
        BlendMode::LuminescentPremul,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BlendMode::Normal => "Normal",
            BlendMode::Dissolve => "Dissolve",
            BlendMode::DancingDissolve => "Dancing Dissolve",
            BlendMode::Darken => "Darken",
            BlendMode::Multiply => "Multiply",
            BlendMode::ColorBurn => "Color Burn",
            BlendMode::ClassicColorBurn => "Classic Color Burn",
            BlendMode::LinearBurn => "Linear Burn",
            BlendMode::DarkerColor => "Darker Color",
            BlendMode::Add => "Add",
            BlendMode::Lighten => "Lighten",
            BlendMode::Screen => "Screen",
            BlendMode::ColorDodge => "Color Dodge",
            BlendMode::ClassicColorDodge => "Classic Color Dodge",
            BlendMode::LinearDodge => "Linear Dodge",
            BlendMode::LighterColor => "Lighter Color",
            BlendMode::Overlay => "Overlay",
            BlendMode::SoftLight => "Soft Light",
            BlendMode::HardLight => "Hard Light",
            BlendMode::LinearLight => "Linear Light",
            BlendMode::VividLight => "Vivid Light",
            BlendMode::PinLight => "Pin Light",
            BlendMode::HardMix => "Hard Mix",
            BlendMode::Difference => "Difference",
            BlendMode::ClassicDifference => "Classic Difference",
            BlendMode::Exclusion => "Exclusion",
            BlendMode::Subtract => "Subtract",
            BlendMode::Divide => "Divide",
            BlendMode::Hue => "Hue",
            BlendMode::Saturation => "Saturation",
            BlendMode::Color => "Color",
            BlendMode::Luminosity => "Luminosity",
            BlendMode::StencilAlpha => "Stencil Alpha",
            BlendMode::StencilLuma => "Stencil Luma",
            BlendMode::SilhouetteAlpha => "Silhouette Alpha",
            BlendMode::SilhouetteLuma => "Silhouette Luma",
            BlendMode::AlphaAdd => "Alpha Add",
            BlendMode::LuminescentPremul => "Luminescent Premul",
        }
    }

    pub fn from_name(s: &str) -> Option<BlendMode> {
        let k: String = s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_ascii_lowercase();
        BlendMode::ALL.into_iter().find(|m| m.label().chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_ascii_lowercase() == k)
    }

    /// Menu groups (a separator follows the last mode of each group).
    pub fn ends_group(self) -> bool {
        matches!(
            self,
            BlendMode::DancingDissolve
                | BlendMode::DarkerColor
                | BlendMode::LighterColor
                | BlendMode::HardMix
                | BlendMode::Divide
                | BlendMode::Luminosity
                | BlendMode::SilhouetteLuma
        )
    }

    /// Modes that act on everything below (the accumulated comp) rather than compositing on top.
    pub fn is_stencil(self) -> bool {
        matches!(self, BlendMode::StencilAlpha | BlendMode::StencilLuma | BlendMode::SilhouetteAlpha | BlendMode::SilhouetteLuma)
    }

    /// Modes whose formulas are defined for over-range (HDR, 32 bpc) values. The others clamp
    /// their inputs to 0..1 before blending, as After Effects documents for 32 bpc projects
    /// (in 8/16 bpc everything is already in range). Overlay, Exclusion, Luminosity and the
    /// Color Burns keep their plain formula above 1 in After Effects (#494).
    pub fn supports_hdr(self) -> bool {
        matches!(
            self,
            BlendMode::Normal
                | BlendMode::Dissolve
                | BlendMode::DancingDissolve
                | BlendMode::Darken
                | BlendMode::Multiply
                | BlendMode::ColorBurn
                | BlendMode::ClassicColorBurn
                | BlendMode::LinearBurn
                | BlendMode::DarkerColor
                | BlendMode::Add
                | BlendMode::Lighten
                | BlendMode::Screen
                | BlendMode::LinearDodge
                | BlendMode::LighterColor
                | BlendMode::Difference
                | BlendMode::ClassicDifference
                | BlendMode::Overlay
                | BlendMode::Exclusion
                | BlendMode::Luminosity
                | BlendMode::Subtract
                | BlendMode::Divide
                | BlendMode::StencilAlpha
                | BlendMode::StencilLuma
                | BlendMode::SilhouetteAlpha
                | BlendMode::SilhouetteLuma
                | BlendMode::AlphaAdd
                | BlendMode::LuminescentPremul
        )
    }

    /// The source colour a classic mode leaves the backdrop unchanged with. After Effects fades
    /// the source toward it by the layer's opacity before applying the classic formula at full
    /// strength, rather than mixing the full-strength result by opacity (#494).
    fn classic_neutral(self) -> Option<f32> {
        match self {
            BlendMode::ClassicColorDodge | BlendMode::ClassicDifference => Some(0.0),
            BlendMode::ClassicColorBurn => Some(1.0),
            _ => None,
        }
    }

    pub fn next(self) -> BlendMode {
        let i = BlendMode::ALL.iter().position(|m| *m == self).unwrap_or(0);
        BlendMode::ALL[(i + 1) % BlendMode::ALL.len()]
    }
    pub fn previous(self) -> BlendMode {
        let i = BlendMode::ALL.iter().position(|m| *m == self).unwrap_or(0);
        BlendMode::ALL[(i + BlendMode::ALL.len() - 1) % BlendMode::ALL.len()]
    }
}

fn burn(cb: f32, cs: f32) -> f32 {
    if cb >= 1.0 {
        1.0
    } else if cs <= 0.0 {
        0.0
    } else {
        1.0 - ((1.0 - cb) / cs).min(1.0)
    }
}
fn dodge(cb: f32, cs: f32) -> f32 {
    if cb <= 0.0 {
        0.0
    } else if cs >= 1.0 {
        1.0
    } else {
        (cb / (1.0 - cs)).min(1.0)
    }
}
fn hard_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        cb * 2.0 * cs
    } else {
        let s = 2.0 * cs - 1.0;
        cb + s - cb * s
    }
}
fn soft_light(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 {
        cb - (1.0 - 2.0 * cs) * cb * (1.0 - cb)
    } else {
        let d = if cb <= 0.25 { ((16.0 * cb - 12.0) * cb + 4.0) * cb } else { cb.max(0.0).sqrt() };
        cb + (2.0 * cs - 1.0) * (d - cb)
    }
}
fn vivid(cb: f32, cs: f32) -> f32 {
    if cs <= 0.5 { burn(cb, 2.0 * cs) } else { dodge(cb, 2.0 * (cs - 0.5)) }
}

fn separable(mode: BlendMode, cb: f32, cs: f32) -> f32 {
    match mode {
        BlendMode::Darken => cb.min(cs),
        BlendMode::Multiply => cb * cs,
        BlendMode::ColorBurn => burn(cb, cs),
        BlendMode::ClassicColorBurn => {
            // (negative results are kept, as in After Effects)
            if cs <= 0.0 { 0.0 } else { 1.0 - (1.0 - cb) / cs }
        }
        BlendMode::LinearBurn => (cb + cs - 1.0).max(0.0),
        BlendMode::Add | BlendMode::LinearDodge => cb + cs,
        BlendMode::Lighten => cb.max(cs),
        BlendMode::Screen => cb + cs - cb * cs,
        BlendMode::ColorDodge => dodge(cb, cs),
        BlendMode::ClassicColorDodge => {
            if cs >= 1.0 {
                1.0
            } else {
                (cb / (1.0 - cs)).min(1.0)
            }
        }
        BlendMode::Overlay => hard_light(cs, cb),
        BlendMode::SoftLight => soft_light(cb, cs),
        BlendMode::HardLight => hard_light(cb, cs),
        BlendMode::LinearLight => (cb + 2.0 * cs - 1.0).clamp(0.0, 1.0),
        BlendMode::VividLight => vivid(cb, cs),
        BlendMode::PinLight => {
            if cs <= 0.5 {
                cb.min(2.0 * cs)
            } else {
                cb.max(2.0 * cs - 1.0)
            }
        }
        BlendMode::HardMix => {
            if cb + cs >= 1.0 {
                1.0
            } else {
                0.0
            }
        }
        BlendMode::Difference | BlendMode::ClassicDifference => (cb - cs).abs(),
        BlendMode::Exclusion => cb + cs - 2.0 * cb * cs,
        BlendMode::Subtract => (cb - cs).max(0.0),
        BlendMode::Divide => {
            if cs <= 0.0 {
                if cb > 0.0 { 1.0 } else { 0.0 }
            } else {
                cb / cs
            }
        }
        _ => cs,
    }
}

fn lum(c: [f32; 3]) -> f32 {
    0.3 * c[0] + 0.59 * c[1] + 0.11 * c[2]
}
fn clip_color(c: [f32; 3]) -> [f32; 3] {
    let l = lum(c);
    let n = c[0].min(c[1]).min(c[2]);
    let x = c[0].max(c[1]).max(c[2]);
    let mut o = c;
    if n < 0.0 {
        for v in &mut o {
            *v = l + (*v - l) * l / (l - n).max(1e-9);
        }
    }
    if x > 1.0 {
        for v in &mut o {
            *v = l + (*v - l) * (1.0 - l) / (x - l).max(1e-9);
        }
    }
    o
}
fn set_lum(c: [f32; 3], l: f32) -> [f32; 3] {
    let d = l - lum(c);
    clip_color([c[0] + d, c[1] + d, c[2] + d])
}
fn sat(c: [f32; 3]) -> f32 {
    c[0].max(c[1]).max(c[2]) - c[0].min(c[1]).min(c[2])
}
fn set_sat(c: [f32; 3], s: f32) -> [f32; 3] {
    let mx = c[0].max(c[1]).max(c[2]);
    let mn = c[0].min(c[1]).min(c[2]);
    if mx - mn <= 1e-9 {
        return [0.0; 3];
    }
    let f = |v: f32| (v - mn) * s / (mx - mn);
    [f(c[0]), f(c[1]), f(c[2])]
}

fn non_separable(mode: BlendMode, cb: [f32; 3], cs: [f32; 3]) -> [f32; 3] {
    match mode {
        BlendMode::Hue => set_lum(set_sat(cs, sat(cb)), lum(cb)),
        BlendMode::Saturation => set_lum(set_sat(cb, sat(cs)), lum(cb)),
        BlendMode::Color => set_lum(cs, lum(cb)),
        BlendMode::Luminosity => set_lum(cb, lum(cs)),
        BlendMode::DarkerColor => {
            if luminance(cs[0], cs[1], cs[2]) < luminance(cb[0], cb[1], cb[2]) {
                cs
            } else {
                cb
            }
        }
        BlendMode::LighterColor => {
            if luminance(cs[0], cs[1], cs[2]) > luminance(cb[0], cb[1], cb[2]) {
                cs
            } else {
                cb
            }
        }
        _ => [separable(mode, cb[0], cs[0]), separable(mode, cb[1], cs[1]), separable(mode, cb[2], cs[2])],
    }
}

/// Composite one premultiplied source pixel over a premultiplied destination pixel.
///
/// `src` must already include layer opacity. `noise` (0..1) is a per-pixel random value used by
/// Dissolve modes. Stencil/Silhouette modes return the destination modified by the source.
#[inline]
pub fn blend_pixel(mode: BlendMode, dst: [f32; 4], src: [f32; 4], noise: f32) -> [f32; 4] {
    let sa = src[3];
    let da = dst[3];
    match mode {
        BlendMode::Normal => {
            let k = 1.0 - sa;
            return [src[0] + dst[0] * k, src[1] + dst[1] * k, src[2] + dst[2] * k, sa + da * k];
        }
        BlendMode::Dissolve | BlendMode::DancingDissolve => {
            if sa <= 0.0 || noise >= sa {
                return dst;
            }
            let inv = 1.0 / sa;
            return [src[0] * inv, src[1] * inv, src[2] * inv, 1.0];
        }
        BlendMode::StencilAlpha => return dst.map(|v| v * sa),
        BlendMode::SilhouetteAlpha => return dst.map(|v| v * (1.0 - sa)),
        BlendMode::StencilLuma | BlendMode::SilhouetteLuma => {
            let l = luminance(src[0], src[1], src[2]);
            let k = if mode == BlendMode::StencilLuma { l } else { 1.0 - l };
            return dst.map(|v| v * k);
        }
        BlendMode::AlphaAdd => {
            return [src[0] + dst[0] * (1.0 - sa), src[1] + dst[1] * (1.0 - sa), src[2] + dst[2] * (1.0 - sa), (sa + da).min(1.0)];
        }
        BlendMode::LuminescentPremul => {
            return [src[0] + dst[0] * (1.0 - sa), src[1] + dst[1] * (1.0 - sa), src[2] + dst[2] * (1.0 - sa), sa + da - sa * da];
        }
        _ => {}
    }
    if sa <= 0.0 {
        return dst;
    }
    if da <= 0.0 {
        return src;
    }
    let mut cs = [src[0] / sa, src[1] / sa, src[2] / sa];
    let mut cb = [dst[0] / da, dst[1] / da, dst[2] / da];
    if !mode.supports_hdr() {
        cs = cs.map(|v| v.clamp(0.0, 1.0));
        cb = cb.map(|v| v.clamp(0.0, 1.0));
    }
    let ao = sa + da - sa * da;
    let mut o = [0.0f32; 4];
    if let Some(neutral) = mode.classic_neutral() {
        // Fade the source toward the neutral colour by its coverage, then apply the formula.
        let b = non_separable(mode, cb, cs.map(|v| neutral + sa * (v - neutral)));
        for i in 0..3 {
            o[i] = (1.0 - da) * src[i] + da * b[i];
        }
    } else {
        let b = non_separable(mode, cb, cs);
        for i in 0..3 {
            o[i] = (1.0 - da) * src[i] + (1.0 - sa) * dst[i] + sa * da * b[i];
        }
    }
    o[3] = ao;
    o
}


