// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
use super::{EffectSpec, ParamUi, Value, slider};

pub(super) fn spec(id: &'static str, name: &'static str, params: Vec<crate::ParamSpec>, render: crate::RenderFn) -> EffectSpec {
    EffectSpec { id, name, category: "Generate", params, render, gpu: false, float: true }
}

pub(super) fn pt(x: f64, y: f64) -> Value {
    Value::Vec2([x, y])
}

pub(super) fn pct() -> ParamUi {
    slider(0.0, 100.0, 0.0, 100.0, 1)
}
