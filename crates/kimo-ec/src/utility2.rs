// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Utility effects, batch 2: HDR Compander.
//!
//! HDR Compander squeezes high-dynamic-range values into 0–1 (Compress Range) so 8/16 bpc
//! effects can process them, and restores them afterwards (Expand Range) with the same Gain and
//! Gamma; the two modes are exact inverses.

use crate::Value;
use rayon::prelude::*;

use crate::util::{premul, unpremul};
use crate::{Buf, EffectCtx, EffectSpec, num, p, popup, slider};

#[inline]
fn spow(v: f32, e: f32) -> f32 {
    v.signum() * v.abs().powf(e)
}

fn hdr_compander(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let expand = ctx.params.e("mode") == 1;
    let gain = ctx.params.f("gain").max(1e-6) as f32;
    let gamma = ctx.params.f("gamma").max(1e-3) as f32;
    b.img.data.par_iter_mut().for_each(|px| {
        let (c, a) = unpremul(*px);
        if a <= 0.0 {
            return;
        }
        let c = c.map(|v| if expand { spow(v, gamma) * gain } else { spow(v / gain, 1.0 / gamma) });
        *px = premul(c, a);
    });
    b
}

pub fn specs() -> Vec<EffectSpec> {
    vec![EffectSpec {
        id: "ec.utility.hdrcompander",
        name: "HDR Compander",
        category: "Utility",
        params: vec![
            p("mode", "Mode", Value::Enum(0), popup(&["Compress Range", "Expand Range"])),
            p("gain", "Gain", num(1.0), slider(1.0, 100.0, 1.0, 100.0, 2)),
            p("gamma", "Gamma", num(1.0), slider(0.1, 10.0, 0.1, 3.0, 2)),
        ],
        render: hdr_compander,
        gpu: false,
        float: true,
    }]
}


