// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Matte effects: chokers (grey-scale morphology and blur/threshold stages) and edge-aware matte
//! refinement (guided filter, He et al. 2010).

use crate::Value;
use crate::ParamUi;
use crate::raster::gaussian_blur;
use rayon::prelude::*;

use crate::util::{Plane, gauss_plane, guided_filter, morph_frac, morph_plane, premul, set_alpha, unpremul};
use crate::{Buf, EffectCtx, EffectSpec, num, p, popup, slider};

fn spec(id: &'static str, name: &'static str, params: Vec<crate::ParamSpec>, render: crate::RenderFn) -> EffectSpec {
    EffectSpec { id, name, category: "Matte", params, render, gpu: false, float: true }
}

fn matte_view(b: &mut Buf) {
    b.img.data.par_iter_mut().for_each(|px| {
        let a = px[3].clamp(0.0, 1.0);
        *px = [a, a, a, 1.0];
    });
}

fn simple_choker(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let c = ctx.params.f("chokeMatte") * b.scale;
    if c.abs() > 1e-6 {
        if c < 0.0 && !ctx.adjustment {
            b.pad((-c).ceil() as u32 + 1);
        }
        let a = Plane::alpha(&b.img);
        let na = if c > 0.0 { morph_frac(&a, c, false) } else { morph_frac(&a, -c, true) };
        set_alpha(&mut b.img, &na);
    }
    if ctx.params.e("view") == 1 {
        matte_view(&mut b);
    }
    b
}

/// One Matte Choker stage: blur by geometric softness, then a threshold ramp.
fn choke_stage(a: Plane, geo: f64, choke: f32, gray: f32) -> Plane {
    let a = if geo > 0.0 { gauss_plane(&a, geo * 0.5, geo * 0.5) } else { a };
    let t = 0.5 + choke / 255.0 * 0.5;
    let w = gray.max(1e-3);
    if (t - 0.5).abs() < 1e-6 && (w - 1.0).abs() < 1e-6 {
        return a;
    }
    a.map(|v| ((v - t) / w + 0.5).clamp(0.0, 1.0))
}

fn matte_choker(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let g1 = ctx.params.f("geometricSoftness1") * b.scale;
    let c1 = ctx.params.f("choke1") as f32;
    let s1 = ctx.params.f("grayLevelSoftness1") as f32 / 100.0;
    let g2 = ctx.params.f("geometricSoftness2") * b.scale;
    let c2 = ctx.params.f("choke2") as f32;
    let s2 = ctx.params.f("grayLevelSoftness2") as f32 / 100.0;
    let it = ctx.params.f("iterations").round().clamp(1.0, 100.0) as usize;
    if (c1 < 0.0 || c2 < 0.0 || g1 > 0.0 || g2 > 0.0) && !ctx.adjustment {
        b.pad(((g1 + g2) * 1.5 * it as f64).ceil() as u32 + 1);
    }
    let mut a = Plane::alpha(&b.img);
    for _ in 0..it {
        a = choke_stage(a, g1, c1, s1);
        a = choke_stage(a, g2, c2, s2);
    }
    set_alpha(&mut b.img, &a);
    b
}

/// Refine Soft / Hard Matte. Refine Soft Matte nests its decontamination settings in a
/// "Decontamination" twirl-down; Refine Hard Matte lists them flat.
fn refine(ctx: &EffectCtx, mut b: Buf, hard: bool) -> Buf {
    let pr = ctx.params;
    let dk = |id: &str| if hard { id.to_string() } else { format!("decontamination/{id}") };
    let r = (pr.f("radius") * b.scale).round().max(1.0) as usize;
    let smooth = pr.f("smooth") as f32 / 100.0;
    let feather = pr.f("feather") / 100.0 * r as f64 * 0.5;
    // Choke (positive shrinks; Refine Hard Matte) and Shift Edge (positive grows; Refine Soft
    // Matte; older soft instances may still carry a hidden Choke).
    let choke = (pr.f("choke") - if hard { 0.0 } else { pr.f("shiftEdge") }).clamp(-100.0, 100.0) as f32 / 100.0 * 0.5;
    let decon = pr.b("decontaminate");
    let decon_amt = pr.f(&dk("decontaminationAmount")) as f32 / 100.0;
    let extend = pr.b(&dk("extendWhereSmoothed"));
    let decon_r = r as f64 + pr.f(&dk("increaseDecontaminationRadius")).max(0.0) * b.scale;
    let view_map = pr.b(&dk("viewDecontaminationMap"));
    let invert = pr.b("invert");
    let contrast = if hard { pr.f("alphaContrast") as f32 / 100.0 } else { 1.0 + pr.f("contrast").max(0.0) as f32 / 100.0 * 3.0 };
    let edge_details = hard || pr.b("calculateEdgeDetails");
    let view_edges = !hard && pr.b("viewEdgeRegion");

    let a = Plane::alpha(&b.img);
    // Only the edge band (where the matte varies within the radius) is refined.
    let hi = morph_plane(&a, r, r, true);
    let lo = morph_plane(&a, r, r, false);
    let band = |i: usize| hi.data[i] - lo.data[i] > 1e-3;
    // The refined matte of a frame.
    let refine_alpha = |img: &crate::raster::Image| -> Plane {
        let a = Plane::alpha(img);
        let hi = morph_plane(&a, r, r, true);
        let lo = morph_plane(&a, r, r, false);
        let mut na = Plane::new(a.w, a.h);
        if edge_details {
            let guide = Plane::from_image(img, |px| crate::color::luminance(px[0], px[1], px[2]));
            let eps = 1e-4 + smooth * smooth * 0.05;
            let filtered = guided_filter(&guide, &a, r, eps);
            na.data.par_iter_mut().enumerate().for_each(|(i, v)| {
                *v = if hi.data[i] - lo.data[i] > 1e-3 { filtered.data[i].clamp(0.0, 1.0) } else { a.data[i] };
            });
        } else {
            na = a.clone();
            if smooth > 0.0 {
                let s = smooth as f64 * r as f64 * 0.5;
                na = gauss_plane(&na, s, s);
            }
        }
        if feather > 0.0 {
            na = gauss_plane(&na, feather, feather);
        }
        na.map(|v| {
            let mut v = if choke > 0.0 {
                (v - choke) / (1.0 - choke)
            } else if choke < 0.0 {
                v / (1.0 + choke)
            } else {
                v
            };
            if (contrast - 1.0).abs() > 1e-6 {
                v = (v - 0.5) * contrast + 0.5;
            }
            let v = v.clamp(0.0, 1.0);
            if invert { 1.0 - v } else { v }
        })
    };
    let mut na = refine_alpha(&b.img);
    // Neighbouring frames of this effect's input (same pixel grid), refined the same way.
    let fps = ctx.fps();
    let frame_at = |t: f64| -> Option<Plane> {
        let o = ctx.env.host?.self_at(t, ctx.env.effect_index)?;
        (o.img.width == b.img.width && o.img.height == b.img.height && (o.offset[0] - b.offset[0]).abs() < 1e-6 && (o.offset[1] - b.offset[1]).abs() < 1e-6)
            .then(|| refine_alpha(&o.img))
    };
    // Reduce Chatter: where the previous and next frames agree but this one differs, the matte
    // is chattering; average the three there. Where they disagree (motion) it is left alone.
    let chatter = (pr.f("reduceChatter") / 100.0).clamp(0.0, 1.0) as f32;
    if chatter > 0.0 {
        let prev = frame_at(ctx.time - 1.0 / fps);
        let next = frame_at(ctx.time + 1.0 / fps);
        if let (Some(p), Some(n)) = (&prev, &next) {
            na.data.par_iter_mut().enumerate().for_each(|(i, v)| {
                let (c, a, z) = (*v, p.data[i], n.data[i]);
                let steady = (1.0 - (a - z).abs() / 0.5).clamp(0.0, 1.0);
                *v = c + ((a + c + z) / 3.0 - c) * chatter * steady;
            });
        }
    }
    // Motion blur: the matte averaged over the shutter (centred on the frame).
    if pr.b("useMotionBlur") {
        let g = |id: &str| pr.get(&format!("motionBlur/{id}")).map(Value::as_f64);
        let samples = g("motionBlurSamples").unwrap_or(16.0).clamp(1.0, 64.0) as usize;
        let samples = if pr.b("motionBlur/higherQuality") { samples * 2 } else { samples }.min(64);
        let span = g("shutterAngle").unwrap_or(180.0).clamp(0.0, 720.0) / 360.0 / fps;
        if samples > 1 && span > 0.0 {
            let mut acc = na.clone();
            let mut n = 1.0f32;
            for k in 0..samples {
                let t = ctx.time - span * 0.5 + span * k as f64 / (samples - 1) as f64;
                if (t - ctx.time).abs() < 1e-9 {
                    continue;
                }
                if let Some(p) = frame_at(t) {
                    acc.data.iter_mut().zip(&p.data).for_each(|(a, q)| *a += q);
                    n += 1.0;
                }
            }
            if n > 1.0 {
                acc.data.iter_mut().for_each(|a| *a /= n);
                na = acc;
            }
        }
    }
    if view_edges {
        // The refined matte in grey with the analysed edge band tinted.
        b.img.data.par_iter_mut().enumerate().for_each(|(i, px)| {
            let v = na.data[i];
            *px = if band(i) { [0.5 + 0.5 * v, 0.5 * v, 0.5 * v, 1.0] } else { [v, v, v, 1.0] };
        });
        return b;
    }
    // Decontamination strength per pixel: partial input alpha, and (Extend Where Smoothed) where
    // the refinement made the matte partial.
    let strength = |i: usize, a0: f32| -> f32 {
        let partial = |a: f32| a > 1e-4 && a < 0.999;
        let n = na.data[i];
        let t = if partial(a0) {
            1.0 - a0
        } else if extend && partial(n) {
            1.0 - n
        } else {
            0.0
        };
        decon_amt * t
    };
    if view_map {
        b.img.data.par_iter_mut().enumerate().for_each(|(i, px)| {
            let v = if decon { strength(i, px[3]) } else { 0.0 };
            *px = [v, v, v, 1.0];
        });
        return b;
    }
    if decon && decon_amt > 0.0 {
        let est = gaussian_blur(&b.img, decon_r * 0.5, decon_r * 0.5, true);
        b.img.data.par_iter_mut().enumerate().for_each(|(i, px)| {
            let (c, a0) = unpremul(*px);
            let t = strength(i, a0);
            if t > 0.0 && a0 > 1e-4 {
                let (ec, ea) = unpremul(est.data[i]);
                if ea > 1e-4 {
                    *px = premul([0, 1, 2].map(|k| c[k] + (ec[k] - c[k]) * t), a0);
                }
            }
        });
    }
    set_alpha(&mut b.img, &na);
    b
}

fn refine_soft(ctx: &EffectCtx, b: Buf) -> Buf {
    refine(ctx, b, false)
}

fn refine_hard(ctx: &EffectCtx, b: Buf) -> Buf {
    refine(ctx, b, true)
}

pub fn specs() -> Vec<EffectSpec> {
    let pct0 = || slider(0.0, 100.0, 0.0, 100.0, 0);
    let check = |id: &'static str, name: &'static str, on: bool| p(id, name, Value::Bool(on), ParamUi::Checkbox);
    let decon = |pre: &'static [&'static str; 4]| {
        vec![
            p(pre[0], "Decontamination Amount", num(100.0), pct0()),
            check(pre[1], "Extend Where Smoothed", true),
            p(pre[2], "Increase Decontamination Radius", num(0.0), slider(0.0, 100.0, 0.0, 20.0, 1)),
            check(pre[3], "View Decontamination Map", false),
        ]
    };
    // Reduce Chatter and the motion-blur settings.
    let temporal = || {
        vec![
            p("reduceChatter", "Reduce Chatter", num(0.0), pct0()),
            check("useMotionBlur", "Use Motion Blur", false),
            p("motionBlur/motionBlurSamples", "Motion Blur Samples", num(16.0), slider(1.0, 64.0, 1.0, 64.0, 0)),
            p("motionBlur/shutterAngle", "Shutter Angle", num(180.0), slider(0.0, 720.0, 0.0, 360.0, 1)),
            check("motionBlur/higherQuality", "Higher Quality", false),
        ]
    };
    let radius = |d: f64| p("radius", "Additional Edge Radius", num(d), slider(1.0, 200.0, 1.0, 50.0, 1));
    let mut soft_params = vec![
        check("calculateEdgeDetails", "Calculate Edge Details", true),
        radius(10.0),
        check("viewEdgeRegion", "View Edge Region", false),
        p("smooth", "Smooth", num(0.0), pct0()),
        p("feather", "Feather", num(10.0), pct0()),
        p("contrast", "Contrast", num(0.0), pct0()),
        p("shiftEdge", "Shift Edge", num(0.0), slider(-100.0, 100.0, -100.0, 100.0, 0)),
    ];
    soft_params.extend(temporal());
    soft_params.push(check("decontaminate", "Decontaminate Edge Colors", true));
    soft_params.extend(decon(&[
        "decontamination/decontaminationAmount",
        "decontamination/extendWhereSmoothed",
        "decontamination/increaseDecontaminationRadius",
        "decontamination/viewDecontaminationMap",
    ]));
    soft_params.push(check("invert", "Invert", false));
    // Superseded by Shift Edge (opposite sign); kept so older projects render as before.
    soft_params.push(p("choke", "Choke", num(0.0), ParamUi::Hidden));
    let mut hard_params = vec![
        p("smooth", "Smooth", num(20.0), pct0()),
        p("feather", "Feather", num(0.0), pct0()),
        p("choke", "Choke", num(0.0), slider(-100.0, 100.0, -100.0, 100.0, 0)),
    ];
    hard_params.extend(temporal());
    hard_params.push(check("decontaminate", "Decontaminate Edge Colors", true));
    hard_params.extend(decon(&["decontaminationAmount", "extendWhereSmoothed", "increaseDecontaminationRadius", "viewDecontaminationMap"]));
    // Our extras: the analysed edge band's radius, alpha contrast and inversion.
    hard_params.push(radius(3.0));
    hard_params.push(p("alphaContrast", "Alpha Contrast", num(300.0), slider(100.0, 1000.0, 100.0, 500.0, 0)));
    hard_params.push(check("invert", "Invert", false));
    let soft = || slider(0.0, 1000.0, 0.0, 100.0, 1);
    let ch = || slider(-127.0, 127.0, -127.0, 127.0, 0);
    let gl = || slider(0.0, 100.0, 0.0, 100.0, 1);
    vec![
        spec(
            "ec.matte.simplechoker",
            "Simple Choker",
            vec![
                p("view", "View", Value::Enum(0), popup(&["Final Output", "Matte"])),
                p("chokeMatte", "Choke Matte", num(0.0), slider(-100.0, 100.0, -10.0, 10.0, 2)),
            ],
            simple_choker,
        ),
        spec(
            "ec.matte.mattechoker",
            "Matte Choker",
            vec![
                p("geometricSoftness1", "Geometric Softness 1", num(4.0), soft()),
                p("choke1", "Choke 1", num(75.0), ch()),
                p("grayLevelSoftness1", "Gray Level Softness 1", num(10.0), gl()),
                p("geometricSoftness2", "Geometric Softness 2", num(0.0), soft()),
                p("choke2", "Choke 2", num(0.0), ch()),
                p("grayLevelSoftness2", "Gray Level Softness 2", num(100.0), gl()),
                p("iterations", "Iterations", num(1.0), slider(1.0, 100.0, 1.0, 10.0, 0)),
            ],
            matte_choker,
        ),
        spec("ec.matte.refinesoft", "Refine Soft Matte", soft_params, refine_soft),
        spec("ec.matte.refinehard", "Refine Hard Matte", hard_params, refine_hard),
    ]
}


