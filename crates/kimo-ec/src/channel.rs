// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Channel effects: channel routing (Set/Shift Channels, Channel Combiner, Set Matte), morphology
//! (Minimax), per-channel arithmetic, solid compositing and un-matting.

use crate::color::{BlendMode, blend_pixel, hsl_to_rgb, luminance, rgb_to_hsl};
use crate::Value;
use crate::ParamUi;
use rayon::prelude::*;

use crate::util::{Plane, SRC_NAMES, Src, join, layer_or_self, morph_plane, pick, premul, src_at, unpremul};
use crate::{Buf, EffectCtx, EffectSpec, col, num, p, popup, slider};

fn spec(id: &'static str, name: &'static str, params: Vec<crate::ParamSpec>, render: crate::RenderFn) -> EffectSpec {
    EffectSpec { id, name, category: "Channel", params, render, gpu: false, float: true }
}

/// Route four sources into R, G, B, A (straight colour), premultiplying the result.
fn route(b: &mut Buf, srcs: [Src; 4]) {
    if srcs == [Src::Red, Src::Green, Src::Blue, Src::Alpha] {
        return;
    }
    b.img.data.par_iter_mut().for_each(|px| {
        let (c, a) = unpremul(*px);
        let v = srcs.map(|s| pick(s, c, a));
        *px = premul([v[0], v[1], v[2]], v[3].clamp(0.0, 1.0));
    });
}

const SET_CHANNEL_LAYERS: [&str; 4] = ["sourceLayer1", "sourceLayer2", "sourceLayer3", "sourceLayer4"];

fn set_channels(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let s = ["setRedTo", "setGreenTo", "setBlueTo", "setAlphaTo"].map(|id| src_at(ctx.params.e(id)));
    // Source layers default to the effect's own layer ("None").
    let stretch = ctx.params.b("stretchLayersToFit");
    let layers: Vec<Option<crate::Image>> =
        SET_CHANNEL_LAYERS.iter().map(|id| ctx.layer_param(id, true).map(|o| crate::util::fit_layer(ctx, &b, &o, stretch))).collect();
    if layers.iter().all(Option::is_none) {
        route(&mut b, s);
        return b;
    }
    let own = b.img.clone();
    b.img.data.par_iter_mut().enumerate().for_each(|(i, px)| {
        let v: [f32; 4] = std::array::from_fn(|k| {
            let q = layers[k].as_ref().map(|l| l.data[i]).unwrap_or(own.data[i]);
            let (c, a) = unpremul(q);
            pick(s[k], c, a)
        });
        *px = premul([v[0], v[1], v[2]], v[3].clamp(0.0, 1.0));
    });
    b
}

/// Shift Channels' Take … From menu. After Effects' ten options come first, in its order, so a
/// project or script that sets 10 ("Full Off" there) turns the channel off here too; "Half On" is
/// an EffectCraft extra and sits last.
const SHIFT_ORDER: [Src; 11] =
    [Src::Alpha, Src::Red, Src::Green, Src::Blue, Src::Luminance, Src::Hue, Src::Lightness, Src::Saturation, Src::Full, Src::Off, Src::Half];
const SHIFT_NAMES: [&str; 11] = ["Alpha", "Red", "Green", "Blue", "Luminance", "Hue", "Lightness", "Saturation", "Full On", "Full Off", "Half On"];

fn shift_channels(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let s = ["takeRedFrom", "takeGreenFrom", "takeBlueFrom", "takeAlphaFrom"].map(|id| SHIFT_ORDER[(ctx.params.e(id) as usize).min(SHIFT_ORDER.len() - 1)]);
    route(&mut b, s);
    b
}

fn rgb_to_yuv(c: [f32; 3]) -> [f32; 3] {
    let y = 0.299 * c[0] + 0.587 * c[1] + 0.114 * c[2];
    [y, 0.492 * (c[2] - y) + 0.5, 0.877 * (c[0] - y) + 0.5]
}

fn yuv_to_rgb(c: [f32; 3]) -> [f32; 3] {
    let (y, u, v) = (c[0], c[1] - 0.5, c[2] - 0.5);
    let r = y + v / 0.877;
    let b = y + u / 0.492;
    let g = (y - 0.299 * r - 0.114 * b) / 0.587;
    [r, g, b]
}

/// Channel Combiner's From options.
const COMBINER_FROM: [&str; 14] = [
    "RGB to HLS",
    "HLS to RGB",
    "RGB to YUV",
    "YUV to RGB",
    "Red",
    "Green",
    "Blue",
    "Alpha",
    "Lightness",
    "Hue",
    "Saturation",
    "Luminance",
    "Max RGB",
    "Min RGB",
];
/// Channel Combiner's To options (a single value goes into one channel).
const COMBINER_TO: [&str; 9] =
    ["Red Only", "Green Only", "Blue Only", "Alpha Only", "Lightness Only", "Hue Only", "Saturation Only", "Grayscale", "Saturation Multiplied"];

fn channel_combiner(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let from = ctx.params.e("from");
    let to = ctx.params.e("to");
    let inv = ctx.params.b("invert");
    let solid = ctx.params.b("solidAlpha");
    // Use 2nd Layer: read the input values from the source layer instead.
    let second = if ctx.params.b("useSecondLayer") { Some(layer_or_self(ctx, &b, "sourceLayer", true, true)) } else { None };
    b.img.data.par_iter_mut().enumerate().for_each(|(i, px)| {
        let (c, mut a) = unpremul(*px);
        let (sc, sa) = match &second {
            Some(img) => unpremul(img.data[i]),
            None => (c, a),
        };
        if sa <= 0.0 && from != 7 && second.is_none() {
            if solid {
                *px = premul(c, 1.0);
            }
            return;
        }
        let iv = |v: f32| if inv { 1.0 - v } else { v };
        let out = match from {
            0..=3 => {
                let o = match from {
                    0 => {
                        let (h, s, l) = rgb_to_hsl(sc[0], sc[1], sc[2]);
                        [h, l, s]
                    }
                    1 => {
                        let (r, g, bb) = hsl_to_rgb(sc[0], sc[2], sc[1]);
                        [r, g, bb]
                    }
                    2 => rgb_to_yuv(sc),
                    _ => yuv_to_rgb(sc),
                };
                o.map(iv)
            }
            _ => {
                let v = iv(match from {
                    4 => sc[0],
                    5 => sc[1],
                    6 => sc[2],
                    7 => sa,
                    8 => rgb_to_hsl(sc[0], sc[1], sc[2]).2,
                    9 => rgb_to_hsl(sc[0], sc[1], sc[2]).0,
                    10 => rgb_to_hsl(sc[0], sc[1], sc[2]).1,
                    11 => luminance(sc[0], sc[1], sc[2]),
                    12 => sc[0].max(sc[1]).max(sc[2]),
                    _ => sc[0].min(sc[1]).min(sc[2]),
                });
                // The single value goes to one channel; the others are set to fixed values
                // (zero for the other colour channels, white for Alpha Only, 50 % lightness /
                // full saturation for Hue Only, …).
                match to {
                    0 => [v, 0.0, 0.0],
                    1 => [0.0, v, 0.0],
                    2 => [0.0, 0.0, v],
                    3 => {
                        a = v.clamp(0.0, 1.0);
                        [1.0, 1.0, 1.0]
                    }
                    5 => {
                        let (r, g, bb) = hsl_to_rgb(v, 1.0, 0.5);
                        [r, g, bb]
                    }
                    6 => {
                        let (r, g, bb) = hsl_to_rgb(0.0, v.clamp(0.0, 1.0), 0.5);
                        [r, g, bb]
                    }
                    // Saturation Multiplied: the pixel keeps its hue and lightness; its saturation
                    // is scaled by the value.
                    8 => {
                        let (h, s, l) = rgb_to_hsl(c[0], c[1], c[2]);
                        let (r, g, bb) = hsl_to_rgb(h, (s * v).clamp(0.0, 1.0), l);
                        [r, g, bb]
                    }
                    _ => [v, v, v],
                }
            }
        };
        if solid {
            a = 1.0;
        }
        *px = premul(out.map(|v| v.max(0.0)), a);
    });
    b
}

/// Minimax's Direction options.
pub(crate) const MINIMAX_DIRECTIONS: [&str; 3] = ["Horizontal & Vertical", "Just Horizontal", "Just Vertical"];

/// `p` with `rx` / `ry` zero pixels added on every side.
fn pad_plane(p: &Plane, rx: usize, ry: usize) -> Plane {
    let (w, h) = (p.w + 2 * rx, p.h + 2 * ry);
    let mut out = Plane::new(w, h);
    for y in 0..p.h {
        out.data[(y + ry) * w + rx..(y + ry) * w + rx + p.w].copy_from_slice(&p.data[y * p.w..(y + 1) * p.w]);
    }
    out
}

/// The `w`×`h` interior of a plane padded by [`pad_plane`].
fn crop_plane(p: &Plane, rx: usize, ry: usize, w: usize, h: usize) -> Plane {
    let mut out = Plane::new(w, h);
    for y in 0..h {
        out.data[y * w..(y + 1) * w].copy_from_slice(&p.data[(y + ry) * p.w + rx..(y + ry) * p.w + rx + w]);
    }
    out
}

fn minimax(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let r = (ctx.params.f("radius") * b.scale).round().max(0.0) as usize;
    if r == 0 {
        return b;
    }
    let op = ctx.params.e("operation");
    let ch = ctx.params.e("channel");
    let dont_shrink = ctx.params.b("dontShrinkEdges");
    let (rx, ry) = match ctx.params.e("direction") {
        1 => (r, 0),
        2 => (0, r),
        _ => (r, r),
    };
    // Which straight channels (R, G, B, A) to process.
    let sel: [bool; 4] = match ch {
        1 => [false, false, false, true],
        2 => [true, true, true, true],
        3 => [true, false, false, false],
        4 => [false, true, false, false],
        5 => [false, false, true, false],
        _ => [true, true, true, false],
    };
    if sel[3] && op != 0 && !ctx.adjustment {
        b.pad((rx.max(ry)) as u32);
    }
    let mut planes = [0, 1, 2, 3].map(|k| Plane::from_image(&b.img, |px| if k == 3 { px[3] } else { unpremul(px).0[k] }));
    for (k, pl) in planes.iter_mut().enumerate() {
        if !sel[k] {
            continue;
        }
        // Beyond the layer edge is empty (0), so Minimum eats into the edges unless Don't
        // Shrink Edges extends the edge pixels instead.
        let padded = if dont_shrink { pl.clone() } else { pad_plane(pl, rx, ry) };
        let out = match op {
            0 => morph_plane(&padded, rx, ry, false),
            1 => morph_plane(&padded, rx, ry, true),
            2 => morph_plane(&morph_plane(&padded, rx, ry, false), rx, ry, true),
            _ => morph_plane(&morph_plane(&padded, rx, ry, true), rx, ry, false),
        };
        *pl = if dont_shrink { out } else { crop_plane(&out, rx, ry, pl.w, pl.h) };
    }
    let mut img = join(&planes);
    img.data.par_iter_mut().for_each(|px| {
        let a = px[3].clamp(0.0, 1.0);
        *px = premul([px[0], px[1], px[2]], a);
    });
    b.img = img;
    b
}

fn arithmetic(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let op = ctx.params.e("operator");
    let k = ["redValue", "greenValue", "blueValue"].map(|id| ctx.params.f(id) as f32 / 255.0);
    let clip = ctx.params.b("clip");
    b.img.map_straight(|c| {
        let mut o = [0.0f32; 3];
        for i in 0..3 {
            let (v, kv) = (c[i], k[i]);
            let iv = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
            let ik = (kv * 255.0).round() as u8;
            let r = match op {
                0 => (iv & ik) as f32 / 255.0,
                1 => (iv | ik) as f32 / 255.0,
                2 => (iv ^ ik) as f32 / 255.0,
                3 => v + kv,
                4 => v - kv,
                5 => (v - kv).abs(),
                6 => v.min(kv),
                7 => v.max(kv),
                8 => {
                    if v > kv {
                        0.0
                    } else {
                        v
                    }
                }
                9 => {
                    if v < kv {
                        0.0
                    } else {
                        v
                    }
                }
                10 => {
                    if v >= kv {
                        1.0
                    } else {
                        0.0
                    }
                }
                11 => v * kv,
                _ => 1.0 - (1.0 - v) * (1.0 - kv),
            };
            o[i] = if clip { r.clamp(0.0, 1.0) } else { r.max(0.0) };
        }
        o
    });
    b
}

const SOLID_MODES: [BlendMode; 12] = [
    BlendMode::Normal,
    BlendMode::Add,
    BlendMode::Multiply,
    BlendMode::Screen,
    BlendMode::Overlay,
    BlendMode::SoftLight,
    BlendMode::HardLight,
    BlendMode::Darken,
    BlendMode::Lighten,
    BlendMode::Difference,
    BlendMode::Color,
    BlendMode::Luminosity,
];
const SOLID_MODE_NAMES: [&str; 12] =
    ["Normal", "Add", "Multiply", "Screen", "Overlay", "Soft Light", "Hard Light", "Darken", "Lighten", "Difference", "Color", "Luminosity"];

fn solid_composite(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let so = ctx.params.f("sourceOpacity") as f32 / 100.0;
    let c = ctx.params.color("color");
    let op = (ctx.params.f("opacity") as f32 / 100.0 * c[3]).clamp(0.0, 1.0);
    let mode = SOLID_MODES[(ctx.params.e("blendingMode") as usize).min(SOLID_MODES.len() - 1)];
    let solid = premul([c[0], c[1], c[2]], op);
    b.img.data.par_iter_mut().for_each(|px| {
        let src = px.map(|v| v * so);
        let mut o = blend_pixel(mode, solid, src, 0.5);
        o[3] = o[3].clamp(0.0, 1.0);
        for v in o.iter_mut().take(3) {
            *v = v.max(0.0);
        }
        *px = o;
    });
    b
}

fn remove_color_matting(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let bg = ctx.params.color("backgroundColor");
    let clip = ctx.params.b("clipHdr");
    b.img.data.par_iter_mut().for_each(|px| {
        let a = px[3];
        if a <= 1e-6 {
            *px = [0.0; 4];
            return;
        }
        for i in 0..3 {
            let s = px[i] / a;
            let mut v = (s - bg[i] * (1.0 - a)).max(0.0);
            if clip {
                v = v.min(a);
            }
            px[i] = v;
        }
    });
    b
}

const MATTE_ORDER: [Src; 10] = [Src::Red, Src::Green, Src::Blue, Src::Alpha, Src::Luminance, Src::Hue, Src::Lightness, Src::Saturation, Src::Full, Src::Off];

fn set_matte(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let src = MATTE_ORDER[(ctx.params.e("takeMatteFrom") as usize).min(9)];
    let inv = ctx.params.b("invertMatte");
    let comp = ctx.params.b("compositeMatteWithOriginal");
    let pre = ctx.params.b("premultiplyMatteLayer");
    // Take Matte From Layer: another layer (stretched or centred), else the layer itself.
    let matte = ctx.layer_param("takeMatteFromLayer", true).map(|o| crate::util::fit_layer(ctx, &b, &o, ctx.params.b("stretchMatteToFit")));
    let own_matte = matte.is_none();
    b.img.data.par_iter_mut().enumerate().for_each(|(i, px)| {
        let (c, a) = unpremul(*px);
        let (mc, ma) = match &matte {
            Some(m) => unpremul(m.data[i]),
            None => (c, a),
        };
        let mut m = pick(src, mc, ma);
        if pre && !matches!(src, Src::Alpha | Src::Full | Src::Off) {
            m *= ma;
        }
        let m = m.clamp(0.0, 1.0);
        let m = if inv { 1.0 - m } else { m };
        // The layer's own (non-inverted) alpha already is the original matte: don't square it.
        let na = if comp && (!own_matte || src != Src::Alpha || inv) { a * m } else { m };
        *px = premul(c, na.clamp(0.0, 1.0));
    });
    b
}

pub fn specs() -> Vec<EffectSpec> {
    let srcp = |id: &'static str, name: &'static str, d: u32| p(id, name, Value::Enum(d), popup(&SRC_NAMES));
    let shp = |id: &'static str, name: &'static str, d: u32| p(id, name, Value::Enum(d), popup(&SHIFT_NAMES));
    let v255 = || slider(0.0, 255.0, 0.0, 255.0, 0);
    vec![
        spec(
            "ec.channel.setchannels",
            "Set Channels",
            vec![
                p("sourceLayer1", "Source Layer 1", Value::Layer(None), ParamUi::Layer),
                srcp("setRedTo", "Set Red To Source 1's", 0),
                p("sourceLayer2", "Source Layer 2", Value::Layer(None), ParamUi::Layer),
                srcp("setGreenTo", "Set Green To Source 2's", 1),
                p("sourceLayer3", "Source Layer 3", Value::Layer(None), ParamUi::Layer),
                srcp("setBlueTo", "Set Blue To Source 3's", 2),
                p("sourceLayer4", "Source Layer 4", Value::Layer(None), ParamUi::Layer),
                srcp("setAlphaTo", "Set Alpha To Source 4's", 3),
                p("stretchLayersToFit", "Stretch Layers to Fit", Value::Bool(true), ParamUi::Checkbox),
            ],
            set_channels,
        ),
        spec(
            "ec.channel.shiftchannels",
            "Shift Channels",
            vec![
                shp("takeAlphaFrom", "Take Alpha From", 0),
                shp("takeRedFrom", "Take Red From", 1),
                shp("takeGreenFrom", "Take Green From", 2),
                shp("takeBlueFrom", "Take Blue From", 3),
            ],
            shift_channels,
        ),
        spec(
            "ec.channel.combiner",
            "Channel Combiner",
            vec![
                p("useSecondLayer", "Use 2nd Layer", Value::Bool(false), ParamUi::Checkbox),
                p("sourceLayer", "Source Layer", Value::Layer(None), ParamUi::Layer),
                p("from", "From", Value::Enum(8), popup(&COMBINER_FROM)),
                p("to", "To", Value::Enum(4), popup(&COMBINER_TO)),
                p("invert", "Invert", Value::Bool(false), ParamUi::Checkbox),
                p("solidAlpha", "Solid Alpha", Value::Bool(false), ParamUi::Checkbox),
            ],
            channel_combiner,
        ),
        spec(
            "ec.channel.minimax",
            "Minimax",
            vec![
                p("operation", "Operation", Value::Enum(1), popup(&["Minimum", "Maximum", "Minimum Then Maximum", "Maximum Then Minimum"])),
                p("radius", "Radius", num(0.0), slider(0.0, 500.0, 0.0, 100.0, 0)),
                p("channel", "Channel", Value::Enum(0), popup(&["Color", "Alpha", "Alpha and Color", "Red", "Green", "Blue"])),
                p("direction", "Direction", Value::Enum(0), popup(&MINIMAX_DIRECTIONS)),
                p("dontShrinkEdges", "Don't Shrink Edges", Value::Bool(false), ParamUi::Checkbox),
            ],
            minimax,
        ),
        spec(
            "ec.channel.arithmetic",
            "Arithmetic",
            vec![
                p(
                    "operator",
                    "Operator",
                    Value::Enum(3),
                    popup(&["And", "Or", "Xor", "Add", "Subtract", "Difference", "Min", "Max", "Block Above", "Block Below", "Slice", "Multiply", "Screen"]),
                ),
                p("redValue", "Red Value", num(0.0), v255()),
                p("greenValue", "Green Value", num(0.0), v255()),
                p("blueValue", "Blue Value", num(0.0), v255()),
                p("clip", "Clip Result Values", Value::Bool(true), ParamUi::Checkbox),
            ],
            arithmetic,
        ),
        spec(
            "ec.channel.solidcomposite",
            "Solid Composite",
            vec![
                p("sourceOpacity", "Source Opacity", num(100.0), slider(0.0, 100.0, 0.0, 100.0, 1)),
                p("color", "Color", col(1.0, 1.0, 1.0), ParamUi::Color),
                p("opacity", "Opacity", num(100.0), slider(0.0, 100.0, 0.0, 100.0, 1)),
                p("blendingMode", "Blending Mode", Value::Enum(0), popup(&SOLID_MODE_NAMES)),
            ],
            solid_composite,
        ),
        spec(
            "ec.channel.removecolormatting",
            "Remove Color Matting",
            vec![
                p("backgroundColor", "Background Color", col(0.0, 0.0, 0.0), ParamUi::Color),
                p("clipHdr", "Clip HDR Values", Value::Bool(true), ParamUi::Checkbox),
            ],
            remove_color_matting,
        ),
        spec(
            "ec.channel.setmatte",
            "Set Matte",
            vec![
                p("takeMatteFromLayer", "Take Matte From Layer", Value::Layer(None), ParamUi::Layer),
                p(
                    "takeMatteFrom",
                    "Use For Matte",
                    Value::Enum(3),
                    popup(&["Red", "Green", "Blue", "Alpha", "Luminance", "Hue", "Lightness", "Saturation", "Full", "Off"]),
                ),
                p("invertMatte", "Invert Matte", Value::Bool(false), ParamUi::Checkbox),
                p("stretchMatteToFit", "Stretch Matte to Fit", Value::Bool(true), ParamUi::Checkbox),
                p("compositeMatteWithOriginal", "Composite Matte with Original", Value::Bool(true), ParamUi::Checkbox),
                p("premultiplyMatteLayer", "Premultiply Matte Layer", Value::Bool(true), ParamUi::Checkbox),
            ],
            set_matte,
        ),
    ]
}


