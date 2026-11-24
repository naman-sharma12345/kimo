// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Obsolete-category effects: Basic 3D (tilt/swivel the layer in a simple perspective space
//! with an optional specular highlight) and Gaussian Blur (Legacy).

use crate::Value;
use crate::ParamUi;
use crate::raster::gaussian_blur;
use rayon::prelude::*;

use crate::util::layer_rect;
use crate::{Buf, EffectCtx, EffectSpec, num, p, popup, slider};

fn spec(id: &'static str, name: &'static str, params: Vec<crate::ParamSpec>, render: crate::RenderFn) -> EffectSpec {
    EffectSpec { id, name, category: "Obsolete", params, render, gpu: false, float: true }
}

// ---------------------------------------------------------------- Basic 3D

fn basic_3d(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let swivel = ctx.params.f("swivel").to_radians();
    let tilt = ctx.params.f("tilt").to_radians();
    let dist = ctx.params.f("distanceToImage") * b.scale;
    let spec_on = ctx.params.b("specularHighlight");
    let wire = ctx.params.b("preview/drawPreviewWireframe");
    if swivel == 0.0 && tilt == 0.0 && dist == 0.0 && !spec_on && !wire {
        return b;
    }
    let (lx, ly, lw, lh) = layer_rect(ctx, &b);
    let (cx, cy) = (lx + lw * 0.5, ly + lh * 0.5);
    // Eye on the -z axis at a focal distance comparable to a 50 mm lens.
    let f = lw.max(lh).max(1.0) * 1.4;
    // Plane basis after Y (swivel) then X (tilt) rotations.
    let (cs, ss) = (swivel.cos(), swivel.sin());
    let (ct, st) = (tilt.cos(), tilt.sin());
    let ux = [cs, 0.0, -ss];
    let uy = [ss * st, ct, cs * st];
    let n = [ss * ct, -st, cs * ct];
    let dot = |a: [f64; 3], b: [f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    let src = b.img.clone();
    let light = {
        let l = [-0.5f64, -0.6, -1.0];
        let k = 1.0 / dot(l, l).sqrt();
        l.map(|v| v * k)
    };
    b.img.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            // Ray from the eye (0, 0, -f) through the pixel on the z = 0 plane (centre-relative).
            let d = [x as f64 + 0.5 - cx, y as f64 + 0.5 - cy, f];
            let o = [0.0, 0.0, -f];
            let denom = dot(d, n);
            if denom.abs() < 1e-9 {
                *px = [0.0; 4];
                continue;
            }
            let pc = [0.0, 0.0, dist];
            let t = dot([pc[0] - o[0], pc[1] - o[1], pc[2] - o[2]], n) / denom;
            if t <= 0.0 {
                *px = [0.0; 4];
                continue;
            }
            let hit = [o[0] + d[0] * t - pc[0], o[1] + d[1] * t - pc[1], o[2] + d[2] * t - pc[2]];
            let (u, v) = (dot(hit, ux), dot(hit, uy));
            let mut c = src.sample_bilinear(cx + u, cy + v);
            if spec_on && c[3] > 0.0 {
                // Reflect the view direction about the normal and compare with the light.
                let dl = dot(d, d).sqrt();
                let dv = d.map(|v| v / dl);
                let k = 2.0 * dot(dv, n);
                let r = [dv[0] - k * n[0], dv[1] - k * n[1], dv[2] - k * n[2]];
                let s = (-dot(r, light)).max(0.0).powi(40) as f32;
                let a = c[3];
                for ch in c.iter_mut().take(3) {
                    *ch += s * a * (1.0 - *ch / a.max(1e-6)).max(0.0);
                }
            }
            if wire {
                let e = (u.abs() - lw * 0.5).abs().min((v.abs() - lh * 0.5).abs());
                if e < 0.75 && u.abs() <= lw * 0.5 + 0.75 && v.abs() <= lh * 0.5 + 0.75 {
                    c = [1.0, 1.0, 1.0, 1.0];
                }
            }
            *px = c;
        }
    });
    b
}

// ---------------------------------------------------------------- Gaussian Blur (Legacy)

fn gaussian_legacy(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let s = ctx.params.f("blurriness").max(0.0) * 0.5 * b.scale;
    if s <= 0.0 {
        return b;
    }
    let (kx, ky) = match ctx.params.e("blurDimensions") {
        1 => (1.0, 0.0),
        2 => (0.0, 1.0),
        _ => (1.0, 1.0),
    };
    if !ctx.adjustment {
        b.pad((s * 3.0).ceil() as u32);
    }
    let (sx, sy) = (s * kx, s * ky);
    // Small sigmas use the sampled kernel, like Gaussian Blur (#1939).
    b.img = if crate::raster::gaussian_is_exact(sx, sy) {
        crate::raster::gaussian_blur_exact(&b.img, sx, sy, ctx.adjustment)
    } else {
        gaussian_blur(&b.img, sx, sy, ctx.adjustment)
    };
    b
}

pub fn specs() -> Vec<EffectSpec> {
    vec![
        spec(
            "ec.obsolete.basic3d",
            "Basic 3D",
            vec![
                p("swivel", "Swivel", num(0.0), ParamUi::Angle),
                p("tilt", "Tilt", num(0.0), ParamUi::Angle),
                p("distanceToImage", "Distance to Image", num(0.0), slider(-1000.0, 30000.0, -100.0, 100.0, 1)),
                p("specularHighlight", "Specular Highlight", Value::Bool(false), ParamUi::Checkbox),
                p("preview/drawPreviewWireframe", "Draw Preview Wireframe", Value::Bool(false), ParamUi::Checkbox),
            ],
            basic_3d,
        ),
        spec(
            "ec.obsolete.gaussianlegacy",
            "Gaussian Blur (Legacy)",
            vec![
                p("blurriness", "Blurriness", num(0.0), slider(0.0, 1000.0, 0.0, 50.0, 1)),
                p("blurDimensions", "Blur Dimensions", Value::Enum(0), popup(&["Horizontal and Vertical", "Horizontal", "Vertical"])),
            ],
            gaussian_legacy,
        ),
    ]
}


