// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Stylize effects (batch 2): cartoon, roughened edges, scatter, strobe, relief, painterly
//! strokes, kaleidoscope, tiling, colour emboss, vignette and thresholds.

use crate::color::{luminance, rgb_to_hsl};
use crate::Value;
use crate::ParamUi;
use crate::raster::{Image, Px};
use rayon::prelude::*;

use crate::noise::fbm;
use crate::util::{Plane, gauss_plane, guided_filter, hash1, join, layer_rect, lerp3, lerp4, morph_frac, premul, smoothstep, split, unpremul};
use crate::{Buf, EffectCtx, EffectSpec, col, num, p, popup, slider};

fn spec(id: &'static str, name: &'static str, params: Vec<crate::ParamSpec>, render: crate::RenderFn) -> EffectSpec {
    EffectSpec { id, name, category: "Stylize", params, render, gpu: false, float: true }
}

fn pct() -> ParamUi {
    slider(0.0, 100.0, 0.0, 100.0, 1)
}

// ---- Cartoon ----

fn sobel(pl: &Plane) -> Plane {
    let mut out = Plane::new(pl.w, pl.h);
    out.data.par_chunks_mut(pl.w.max(1)).enumerate().for_each(|(y, row)| {
        for (x, v) in row.iter_mut().enumerate() {
            let l = |dx: i64, dy: i64| pl.get_clamped(x as i64 + dx, y as i64 + dy);
            let gx = l(1, -1) + 2.0 * l(1, 0) + l(1, 1) - l(-1, -1) - 2.0 * l(-1, 0) - l(-1, 1);
            let gy = l(-1, 1) + 2.0 * l(0, 1) + l(1, 1) - l(-1, -1) - 2.0 * l(0, -1) - l(1, -1);
            *v = (gx * gx + gy * gy).sqrt();
        }
    });
    out
}

fn cartoon(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let render = ctx.params.e("render");
    let r = (ctx.params.f("detailRadius") * b.scale).round().max(0.0) as usize;
    let thr = ctx.params.f("detailThreshold") as f32 / 100.0;
    let steps = ctx.params.f("fill/shadingSteps").max(1.0) as f32;
    let smooth = ctx.params.f("fill/shadingSmoothness") as f32 / 100.0;
    let et = ctx.params.f("edge/edgeThreshold") as f32;
    let ew = ctx.params.f("edge/edgeWidth") * b.scale;
    let es = ctx.params.f("edge/edgeSoftness") as f32 / 100.0;
    let eo = ctx.params.f("edge/edgeOpacity") as f32 / 100.0;
    // Advanced: Edge Black Level shifts the grey representation of the edges towards white
    // strokes on black; Edge Contrast (0.5 = neutral) steepens or flattens it.
    let black = (ctx.params.f("advanced/edgeBlackLevel") as f32 / 100.0).clamp(0.0, 1.0);
    let contrast = ((ctx.params.f("advanced/edgeContrast").clamp(0.0, 0.999) * std::f64::consts::FRAC_PI_2).tan() as f32).min(100.0);
    let grey = move |e: f32| (((1.0 - e) - black).abs() - 0.5) * contrast + 0.5;
    // Edge-preserving smoothing: guided filter on each premultiplied channel, guided by luma.
    let smoothed = if r > 0 {
        let guide = Plane::luma(&b.img);
        let eps = thr * thr * 0.25 + 1e-5;
        let mut ch = split(&b.img);
        for c in ch.iter_mut().take(3) {
            *c = guided_filter(&guide, c, r, eps);
        }
        join(&ch)
    } else {
        b.img.clone()
    };
    // Advanced ▸ Edge Enhancement: one shock-filter step. Each pixel samples from the side of
    // the nearest edge it belongs to (along the luma gradient, by the sign of the Laplacian):
    // positive values sharpen the transitions, negative ones soften them.
    let enh = ctx.params.f("advanced/edgeEnhancement") / 100.0;
    let smoothed = if enh < 0.0 {
        // Softening: blend towards a blur of the picture.
        let s = 1.5 * b.scale;
        let blur = crate::raster::gaussian_blur(&smoothed, s, s, true);
        let k = (-enh) as f32;
        let mut out = smoothed;
        out.data.par_iter_mut().zip(blur.data.par_iter()).for_each(|(p, q)| (0..4).for_each(|c| p[c] += (q[c] - p[c]) * k));
        out
    } else if enh > 0.0 {
        let l = Plane::luma(&smoothed);
        let (w, h) = (l.w as i64, l.h as i64);
        let s = enh * 2.0 * b.scale;
        let src = smoothed.clone();
        let mut out = smoothed;
        out.data.par_iter_mut().enumerate().for_each(|(i, px)| {
            let (x, y) = ((i as i64) % w, (i as i64) / w);
            let g = |dx: i64, dy: i64| l.get_clamped(x + dx, y + dy);
            let (gx, gy) = ((g(1, 0) - g(-1, 0)) * 0.5, (g(0, 1) - g(0, -1)) * 0.5);
            let lap = g(1, 0) + g(-1, 0) + g(0, 1) + g(0, -1) - 4.0 * g(0, 0);
            let m = (gx * gx + gy * gy).sqrt();
            if m < 1e-4 || lap.abs() < 1e-5 || y >= h {
                return;
            }
            let k = -lap.signum() as f64 * s / m as f64;
            let (sx, sy) = (x as f64 + 0.5 + gx as f64 * k, y as f64 + 0.5 + gy as f64 * k);
            *px = src.sample_bilinear_clamped(sx, sy);
        });
        out
    } else {
        smoothed
    };
    let luma = Plane::luma(&smoothed);
    let edges = if render >= 1 {
        let mag = sobel(&luma);
        let t1 = et * 0.2;
        let t0 = t1 * (1.0 - es * 0.9);
        let mut e = mag.map(|m| smoothstep(t0, t1.max(t0 + 1e-4), m));
        if ew > 1.0 {
            e = morph_frac(&e, (ew - 1.0) * 0.5, true);
        } else {
            let k = ew.max(0.0) as f32;
            e = e.map(|v| v * k);
        }
        Some(e)
    } else {
        None
    };
    b.img.data.par_iter_mut().enumerate().for_each(|(i, px)| {
        let a = px[3];
        if a <= 0.0 {
            return;
        }
        let (c, _) = unpremul(smoothed.data[i]);
        let fill = {
            let l = luminance(c[0], c[1], c[2]);
            let t = l.max(0.0) * steps;
            let f = t - t.floor();
            let f2 = if smooth <= 0.0 { if f >= 0.5 { 1.0 } else { 0.0 } } else { smoothstep(0.5 - smooth * 0.5, 0.5 + smooth * 0.5, f) };
            let lq = (t.floor() + f2) / steps;
            if l > 1e-4 { c.map(|v| v * lq / l) } else { [lq; 3] }
        };
        let e = edges.as_ref().map(|e| e.data[i] * eo).unwrap_or(0.0);
        let g = grey(e).clamp(0.0, 1.0);
        let o = match render {
            0 => fill,
            1 => [g; 3],
            _ => fill.map(|v| v * g),
        };
        *px = premul(o, a);
    });
    b
}

// ---- Roughen Edges ----

fn roughen_edges(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let kind = ctx.params.e("edgeType");
    let ec = ctx.params.color("edgeColor");
    let border = ctx.params.f("border") * b.scale;
    let sharp = ctx.params.f("edgeSharpness").max(0.01) as f32;
    let infl = ctx.params.f("fractalInfluence") as f32;
    let scale = (ctx.params.f("scale") * b.scale * 0.1).max(0.5) as f32;
    let (ox, oy) = b.to_px(ctx.params.v2("offset"));
    let oct = ctx.params.f("complexity") as f32;
    let evo = ctx.params.f("evolution") as f32 / 360.0;
    // Stretch Width or Height: positive values widen the fractal, negative ones heighten it.
    let st = ctx.params.f("stretch") as f32;
    let (sx, sy) = (1.0 + st.max(0.0), 1.0 + (-st).max(0.0));
    // Cycle Evolution: cross-fade one cycle into the next so the roughness loops seamlessly.
    let cycle = ctx.params.b("evolutionOptions/cycleEvolution").then(|| ctx.params.f("evolutionOptions/cycle").round().max(1.0) as f32);
    let (evo, evo_w) = match cycle {
        Some(c) => {
            let e = evo.rem_euclid(c);
            (e, e / c)
        }
        None => (evo, 0.0),
    };
    // (frequency, influence, sharpness, coloured) per edge type.
    let (fm, im, sm, colored) = match kind {
        1 => (1.0, 1.0, 1.0, true),
        2 => (1.0, 1.0, 3.0, false),
        3 => (3.0, 1.0, 1.5, false),
        4 => (1.5, 1.5, 1.0, false),
        5 => (1.5, 1.5, 1.0, true),
        6 => (0.6, 1.2, 2.5, false),
        7 => (0.6, 1.2, 2.5, true),
        _ => (1.0, 1.0, 1.0, false),
    };
    let alpha = Plane::alpha(&b.img);
    let d = if border > 0.05 { gauss_plane(&alpha, border * 0.5, border * 0.5) } else { alpha };
    let extra = if kind == 4 || kind == 5 { 1.0 } else { 0.0 };
    let seed = ctx.seed ^ 0x40f1 ^ (ctx.params.f("evolutionOptions/randomSeed").round() as i64 as u32).wrapping_mul(0x9e37_79b9);
    b.img.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            if px[3] <= 0.0 {
                continue;
            }
            let dv = d.get(x, y);
            let (nx, ny) = ((x as f32 + 0.5 - ox as f32) / (scale * sx) * fm, (y as f32 + 0.5 - oy as f32) / (scale * sy) * fm);
            let mut n = fbm(nx, ny, evo, seed, oct + extra);
            if let Some(c) = cycle {
                n += (fbm(nx, ny, evo - c, seed, oct + extra) - n) * evo_w;
            }
            let s = dv - 0.5 + (n - 0.5) * infl * im * 0.5;
            let m = (s * sharp * sm * 4.0 + 0.5).clamp(0.0, 1.0);
            let (mut c, a) = unpremul(*px);
            if colored {
                let band = 1.0 - smoothstep(0.5, 0.9, dv + (n - 0.5) * infl * 0.5);
                c = lerp3(c, [ec[0], ec[1], ec[2]], band);
            }
            *px = premul(c, a * m);
        }
    });
    b
}

// ---- Scatter ----

fn scatter(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let amt = ctx.params.f("amount") * b.scale;
    if amt <= 0.0 {
        return b;
    }
    let grain = ctx.params.e("grain");
    let seed = if ctx.params.b("randomizeEveryFrame") { ctx.seed ^ ((ctx.time * 1000.0) as u32).wrapping_mul(0x9e37_79b9) } else { ctx.seed };
    let src = b.img.clone();
    b.img.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let dx = if grain == 2 { 0.0 } else { (hash1(x as u32, y as u32, seed) as f64 - 0.5) * 2.0 * amt };
            let dy = if grain == 1 { 0.0 } else { (hash1(x as u32, y as u32, seed ^ 0x5bd1) as f64 - 0.5) * 2.0 * amt };
            *px = src.get((x as f64 + dx).round() as i64, (y as f64 + dy).round() as i64);
        }
    });
    b
}

// ---- Strobe Light ----

/// Whether Strobe Light flashes at `time` (also used by the GPU kernels).
pub fn strobe_on(time: f64, duration: f64, period: f64, prob: f64, seed: u32) -> bool {
    if period <= 0.0 {
        return false;
    }
    let k = (time / period).floor();
    let phase = time - k * period;
    phase < duration || (prob > 0.0 && (hash1(k as i64 as u32, 17, seed) as f64) < prob)
}

fn strobe(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let on = strobe_on(ctx.time, ctx.params.f("strobeDuration"), ctx.params.f("strobePeriod"), ctx.params.f("randomStrobeProbability") / 100.0, ctx.seed);
    if !on {
        return b;
    }
    let blend = (ctx.params.f("blendWithOriginal") as f32 / 100.0).clamp(0.0, 1.0);
    if ctx.params.e("strobe") == 1 {
        b.img.scale_alpha(blend);
        return b;
    }
    let s = ctx.params.color("strobeColor");
    let op = ctx.params.e("strobeOperator");
    b.img.map_straight(|c| {
        let o: [f32; 3] = [0, 1, 2].map(|i| {
            let (v, k) = (c[i], s[i]);
            match op {
                1 => v + k,
                2 => (v - k).max(0.0),
                3 => v * k,
                4 => (v - k).abs(),
                5 => 1.0 - (1.0 - v) * (1.0 - k),
                6 => v.max(k),
                7 => v.min(k),
                _ => k,
            }
        });
        lerp3(o, c, blend)
    });
    b
}

// ---- Texturize ----

fn texturize(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let ang = ctx.params.f("lightDirection").to_radians();
    let k = ctx.params.f("textureContrast") as f32;
    if k == 0.0 {
        return b;
    }
    // Relief from the texture layer's luminance; without a texture there is nothing to do.
    let Some(tex) = ctx.layer_param("textureLayer", true) else { return b };
    let tex = crate::transition::place_layer(ctx, &b, &tex, ctx.params.e("texturePlacement"));
    let (lx, ly) = (ang.cos() as f32, -ang.sin() as f32);
    let h = gauss_plane(&Plane::luma(&tex), 0.7 * b.scale, 0.7 * b.scale);
    b.img.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let gx = (h.get_clamped(x as i64 + 1, y as i64) - h.get_clamped(x as i64 - 1, y as i64)) * 0.5;
            let gy = (h.get_clamped(x as i64, y as i64 + 1) - h.get_clamped(x as i64, y as i64 - 1)) * 0.5;
            let shade = (1.0 - k * 4.0 * (gx * lx + gy * ly)).max(0.0);
            for c in 0..3 {
                px[c] *= shade;
            }
        }
    });
    b
}

// ---- Brush Strokes ----

fn brush_strokes(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let ang = ctx.params.f("strokeAngle");
    let size = (ctx.params.f("brushSize") * b.scale).max(0.5);
    let len = ctx.params.f("strokeLength") * b.scale;
    let density = ctx.params.f("strokeDensity") as f32;
    let rnd = ctx.params.f("strokeRandomness");
    let surface = ctx.params.e("paintSurface");
    let blend = ctx.params.f("blendWithOriginal") as f32 / 100.0;
    let seed = ctx.seed ^ 0xb705;
    let cell = (size * 2.0).max(1.0);
    let src = b.img.clone();
    b.img.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let (cx, cy) = ((x as f64 / cell).floor() as i64 as u32, (y as f64 / cell).floor() as i64 as u32);
            let h = |k: u32| hash1(cx.wrapping_add(k.wrapping_mul(7919)), cy, seed) as f64;
            let a = (ang + (h(0) - 0.5) * rnd * 60.0).to_radians();
            let l = len * (0.5 + h(1));
            let (dx, dy) = (a.sin(), -a.cos());
            let jit = (h(2) - 0.5) * size;
            let (bx, by) = (x as f64 + 0.5 - dy * jit, y as f64 + 0.5 + dx * jit);
            let n = l.ceil().clamp(1.0, 32.0) as usize;
            let mut acc = [0.0f32; 4];
            for i in 0..n {
                let t = if n == 1 { 0.0 } else { i as f64 / (n - 1) as f64 - 0.5 } * l;
                let s = src.sample_bilinear_clamped(bx + dx * t, by + dy * t);
                for c in 0..4 {
                    acc[c] += s[c];
                }
            }
            let stroke = acc.map(|v| v / n as f32);
            let m = if (h(3) as f32) < density.min(1.0) { 1.0 } else { 0.0 };
            let orig = *px;
            let base: Px = match surface {
                1 => [0.0; 4],
                2 => [1.0; 4],
                3 => [0.0, 0.0, 0.0, 1.0],
                _ => orig,
            };
            let painted = if surface == 2 || surface == 3 {
                // Stroke over an opaque surface.
                let k = 1.0 - stroke[3] * m;
                [stroke[0] * m + base[0] * k, stroke[1] * m + base[1] * k, stroke[2] * m + base[2] * k, stroke[3] * m + base[3] * k]
            } else {
                lerp4(base, stroke, m)
            };
            *px = lerp4(painted, orig, blend);
        }
    });
    b
}

// ---- Kaleidoscope ----

fn kaleida(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let c = b.to_px(ctx.params.v2("center"));
    let size = ctx.params.f("size").max(1.0) / 100.0;
    let mode = ctx.params.e("mirroring");
    let rot = ctx.params.f("rotation").to_radians();
    let n = [6.0, 8.0, 4.0, 12.0, 3.0][mode.min(4) as usize];
    let seg = std::f64::consts::TAU / n;
    let mirror = mode != 3;
    let src = b.img.clone();
    b.img.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let (dx, dy) = (x as f64 + 0.5 - c.0, y as f64 + 0.5 - c.1);
            let r = (dx * dx + dy * dy).sqrt() / size;
            let mut a = (dy.atan2(dx) - rot).rem_euclid(seg);
            if mirror && a > seg * 0.5 {
                a = seg - a;
            }
            *px = src.sample_bilinear_clamped(c.0 + r * (a + rot).cos(), c.1 + r * (a + rot).sin());
        }
    });
    b
}

// ---- Edge Tile Repeat ----

#[inline]
fn tile_coord(v: f64, size: f64) -> (i64, f64) {
    let i = (v / size).floor();
    (i as i64, v - i * size)
}

fn repetile(ctx: &EffectCtx, b: Buf) -> Buf {
    let s = b.scale;
    let (r, l, d, u) = (
        ctx.params.f("expandRight").max(0.0) * s,
        ctx.params.f("expandLeft").max(0.0) * s,
        ctx.params.f("expandDown").max(0.0) * s,
        ctx.params.f("expandUp").max(0.0) * s,
    );
    if ctx.adjustment || r + l + d + u < 0.5 {
        return b;
    }
    let mode = ctx.params.e("tiling");
    let (x0, y0, w, h) = layer_rect(ctx, &b);
    if w < 1.0 || h < 1.0 {
        return b;
    }
    let (nw, nh) = ((w + l + r).round().max(1.0) as u32, (h + u + d).round().max(1.0) as u32);
    let mut out = Image::new(nw, nh);
    let src = &b.img;
    out.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let (ix, mut fx) = tile_coord(x as f64 + 0.5 - l, w);
            let (iy, mut fy) = tile_coord(y as f64 + 0.5 - u, h);
            let odd = (ix + iy).rem_euclid(2) == 1;
            let (flip_x, flip_y) = match mode {
                1 => (ix.rem_euclid(2) == 1, iy.rem_euclid(2) == 1),
                2 => (odd, false),
                3 => (false, odd),
                4 => (odd, odd),
                _ => (false, false),
            };
            if flip_x {
                fx = w - fx;
            }
            if flip_y {
                fy = h - fy;
            }
            *px = src.sample_bilinear(x0 + fx, y0 + fy);
        }
    });
    Buf { img: out, offset: [l, u], scale: s }
}

// ---- Motion Tile ----

fn motion_tile(ctx: &EffectCtx, b: Buf) -> Buf {
    let s = b.scale;
    let (lw, lh) = (ctx.layer_size[0], ctx.layer_size[1]);
    if lw <= 0.0 || lh <= 0.0 {
        return b;
    }
    let tc = ctx.params.v2("tileCenter");
    let tw = (lw * ctx.params.f("tileWidth") / 100.0).max(0.01);
    let th = (lh * ctx.params.f("tileHeight") / 100.0).max(0.01);
    let mirror = ctx.params.b("mirrorEdges");
    let phase = ctx.params.f("phase") / 360.0;
    let hshift = ctx.params.b("horizontalPhaseShift");
    let (nw, nh, noff) = crate::motion_tile_frame(ctx, b.offset, (b.img.width, b.img.height), s);
    let mut out = Image::new(nw, nh);
    let src = &b.img;
    out.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let u = (x as f64 + 0.5 - noff[0]) / s;
            let v = (y as f64 + 0.5 - noff[1]) / s;
            let mut tx = (u - tc[0]) / tw + 0.5;
            let mut ty = (v - tc[1]) / th + 0.5;
            if hshift {
                tx += phase * ty.floor();
            } else {
                ty += phase * tx.floor();
            }
            let (i, j) = (tx.floor(), ty.floor());
            let (mut fx, mut fy) = (tx - i, ty - j);
            if mirror {
                if (i as i64).rem_euclid(2) == 1 {
                    fx = 1.0 - fx;
                }
                if (j as i64).rem_euclid(2) == 1 {
                    fy = 1.0 - fy;
                }
            }
            // Clamped: a tile edge lands exactly on a pixel centre whenever the boundary does
            // (#2820's animated Tile Center walks it across the grid), and the plain sampler
            // would mix in the transparent outside at the source's geometric edge, leaving a
            // half-faded seam pixel along the fold.
            *px = src.sample_bilinear_clamped(fx * lw * s + b.offset[0], fy * lh * s + b.offset[1]);
        }
    });
    Buf { img: out, offset: noff, scale: s }
}

// ---- Color Emboss ----

fn color_emboss(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let ang = ctx.params.f("direction").to_radians();
    let relief = ctx.params.f("relief") * b.scale;
    let contrast = ctx.params.f("contrast") as f32 / 100.0;
    let blend = ctx.params.f("blend") as f32 / 100.0;
    let (dx, dy) = (ang.cos() * relief, -ang.sin() * relief);
    let src = b.img.clone();
    b.img.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let (c, a) = unpremul(*px);
            if a <= 0.0 {
                continue;
            }
            let (pa, _) = unpremul(src.sample_bilinear_clamped(x as f64 + 0.5 + dx, y as f64 + 0.5 + dy));
            let (pb, _) = unpremul(src.sample_bilinear_clamped(x as f64 + 0.5 - dx, y as f64 + 0.5 - dy));
            let e = [0, 1, 2].map(|i| (c[i] + (pa[i] - pb[i]) * contrast).max(0.0));
            *px = premul(lerp3(e, c, blend), a);
        }
    });
    b
}

// ---- Vignette ----

fn vignette(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let amt = ctx.params.f("amount") as f32 / 100.0;
    let fov = ctx.params.f("angleOfView").clamp(0.0, 179.0).to_radians();
    if amt == 0.0 || fov <= 0.0 {
        return b;
    }
    let c = b.to_px(ctx.params.v2("center"));
    let pin = ctx.params.f("pinHighlights") as f32 / 100.0;
    let (lw, lh) = (ctx.layer_size[0] * b.scale, ctx.layer_size[1] * b.scale);
    let half_diag = (lw * lw + lh * lh).sqrt() * 0.5;
    let dist = half_diag.max(1.0) / (fov * 0.5).tan();
    b.img.rows_mut().for_each(|(y, row)| {
        for (x, px) in row.iter_mut().enumerate() {
            let (c3, a) = unpremul(*px);
            if a <= 0.0 {
                continue;
            }
            let r = ((x as f64 + 0.5 - c.0).powi(2) + (y as f64 + 0.5 - c.1).powi(2)).sqrt() / dist;
            let f = (1.0 / (1.0 + r * r)).powi(2) as f32;
            let mut k = 1.0 - amt * (1.0 - f);
            if pin > 0.0 {
                let l = luminance(c3[0], c3[1], c3[2]).clamp(0.0, 1.0);
                k += (1.0 - k) * pin * l;
            }
            *px = premul(c3.map(|v| (v * k).max(0.0)), a);
        }
    });
    b
}

// ---- Channel Threshold / RGB Threshold ----

fn cc_threshold(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let t = ctx.params.f("threshold") as f32 / 255.0;
    let ch = ctx.params.e("channel");
    let inv = ctx.params.b("invert");
    let blend = ctx.params.f("blend") as f32 / 100.0;
    let bin = |v: f32| if (v >= t) != inv { 1.0 } else { 0.0 };
    b.img.data.par_iter_mut().for_each(|px| {
        let (c, a) = unpremul(*px);
        if a <= 0.0 {
            return;
        }
        let (o, na) = match ch {
            1 => (c.map(bin), a),
            2 => ([bin(rgb_to_hsl(c[0], c[1], c[2]).1); 3], a),
            3 => (c, bin(a)),
            _ => ([bin(luminance(c[0], c[1], c[2])); 3], a),
        };
        let o = lerp3(o, c, blend);
        let na = na + (a - na) * blend;
        *px = premul(o, na);
    });
    b
}

fn cc_threshold_rgb(ctx: &EffectCtx, mut b: Buf) -> Buf {
    let t = [ctx.params.f("redThreshold"), ctx.params.f("greenThreshold"), ctx.params.f("blueThreshold")].map(|v| v as f32 / 255.0);
    let inv = [ctx.params.b("invertRed"), ctx.params.b("invertGreen"), ctx.params.b("invertBlue")];
    let blend = ctx.params.f("blend") as f32 / 100.0;
    b.img.map_straight(|c| {
        let o = [0, 1, 2].map(|i| if (c[i] >= t[i]) != inv[i] { 1.0 } else { 0.0 });
        lerp3(o, c, blend)
    });
    b
}

pub fn specs() -> Vec<EffectSpec> {
    let pt = |x, y| Value::Vec2([x, y]);
    let thr = || slider(0.0, 255.0, 0.0, 255.0, 0);
    vec![
        spec(
            "ec.stylize.cartoon",
            "Cartoon",
            vec![
                p("render", "Render", Value::Enum(2), popup(&["Fill", "Edges", "Fill & Edges"])),
                p("detailRadius", "Detail Radius", num(8.0), slider(0.0, 50.0, 0.0, 50.0, 1)),
                p("detailThreshold", "Detail Threshold", num(10.0), pct()),
                p("fill/shadingSteps", "Shading Steps", num(8.0), slider(1.0, 64.0, 1.0, 64.0, 1)),
                p("fill/shadingSmoothness", "Shading Smoothness", num(70.0), pct()),
                p("edge/edgeThreshold", "Threshold", num(1.5), slider(0.0, 5.0, 0.0, 5.0, 2)),
                p("edge/edgeWidth", "Width", num(1.5), slider(0.0, 10.0, 0.0, 5.0, 2)),
                p("edge/edgeSoftness", "Softness", num(60.0), pct()),
                p("edge/edgeOpacity", "Opacity", num(100.0), pct()),
                p("advanced/edgeEnhancement", "Edge Enhancement", num(0.0), slider(-100.0, 100.0, -100.0, 100.0, 1)),
                p("advanced/edgeBlackLevel", "Edge Black Level", num(0.0), slider(0.0, 100.0, 0.0, 100.0, 1)),
                p("advanced/edgeContrast", "Edge Contrast", num(0.5), slider(0.0, 1.0, 0.0, 1.0, 2)),
            ],
            cartoon,
        ),
        spec(
            "ec.stylize.roughenedges",
            "Roughen Edges",
            vec![
                p(
                    "edgeType",
                    "Edge Type",
                    Value::Enum(0),
                    popup(&["Roughen", "Roughen Color", "Cut", "Spiky", "Rusty", "Rusty Color", "Photocopy", "Photocopy Color"]),
                ),
                p("edgeColor", "Edge Color", col(0.55, 0.3, 0.1), ParamUi::Color),
                p("border", "Border", num(8.0), slider(0.0, 500.0, 0.0, 100.0, 1)),
                p("edgeSharpness", "Edge Sharpness", num(1.0), slider(0.0, 10.0, 0.0, 10.0, 2)),
                p("fractalInfluence", "Fractal Influence", num(1.0), slider(0.0, 1.0, 0.0, 1.0, 2)),
                p("scale", "Scale", num(100.0), slider(10.0, 1000.0, 10.0, 300.0, 1)),
                p("stretch", "Stretch Width or Height", num(0.0), slider(-10.0, 10.0, -5.0, 5.0, 2)),
                p("offset", "Offset (Turbulence)", pt(0.0, 0.0), ParamUi::Point),
                p("complexity", "Complexity", num(2.0), slider(1.0, 10.0, 1.0, 10.0, 0)),
                p("evolution", "Evolution", num(0.0), ParamUi::Angle),
                p("evolutionOptions/cycleEvolution", "Cycle Evolution", Value::Bool(false), ParamUi::Checkbox),
                p("evolutionOptions/cycle", "Cycle (in Revolutions)", num(1.0), slider(1.0, 1000.0, 1.0, 20.0, 0)),
                p("evolutionOptions/randomSeed", "Random Seed", num(0.0), slider(0.0, 10000.0, 0.0, 100.0, 0)),
            ],
            roughen_edges,
        ),
        spec(
            "ec.stylize.scatter",
            "Scatter",
            vec![
                p("amount", "Scatter Amount", num(0.0), slider(0.0, 1000.0, 0.0, 127.0, 1)),
                p("grain", "Grain", Value::Enum(0), popup(&["Both", "Horizontal", "Vertical"])),
                p("randomizeEveryFrame", "Scatter Randomness", Value::Bool(false), ParamUi::Checkbox),
            ],
            scatter,
        ),
        spec(
            "ec.stylize.strobe",
            "Strobe Light",
            vec![
                p("strobeColor", "Strobe Color", col(1.0, 1.0, 1.0), ParamUi::Color),
                p("blendWithOriginal", "Blend With Original", num(0.0), pct()),
                p("strobeDuration", "Strobe Duration (secs)", num(0.05), slider(0.0, 30.0, 0.0, 1.0, 2)),
                p("strobePeriod", "Strobe Period (secs)", num(1.0), slider(0.0, 30.0, 0.0, 5.0, 2)),
                p("randomStrobeProbability", "Random Strobe Probability", num(0.0), pct()),
                p("strobe", "Strobe", Value::Enum(0), popup(&["Operates On Color Only", "Makes Layer Transparent"])),
                p(
                    "strobeOperator",
                    "Strobe Operator",
                    Value::Enum(0),
                    popup(&["Copy", "Add", "Subtract", "Multiply", "Difference", "Screen", "Lighten", "Darken"]),
                ),
            ],
            strobe,
        ),
        spec(
            "ec.stylize.texturize",
            "Texturize",
            vec![
                p("textureLayer", "Texture Layer", Value::Layer(None), ParamUi::Layer),
                p("lightDirection", "Light Direction", num(135.0), ParamUi::Angle),
                p("textureContrast", "Texture Contrast", num(1.0), slider(0.0, 2.0, 0.0, 2.0, 2)),
                p("texturePlacement", "Texture Placement", Value::Enum(0), popup(&["Tile Texture", "Center Texture", "Stretch Texture to Fit"])),
            ],
            texturize,
        ),
        spec(
            "ec.stylize.brushstrokes",
            "Brush Strokes",
            vec![
                p("strokeAngle", "Stroke Angle", num(135.0), ParamUi::Angle),
                p("brushSize", "Brush Size", num(2.0), slider(0.0, 5.0, 0.0, 5.0, 1)),
                p("strokeLength", "Stroke Length", num(4.0), slider(0.0, 40.0, 0.0, 40.0, 0)),
                p("strokeDensity", "Stroke Density", num(1.0), slider(0.0, 2.0, 0.0, 2.0, 2)),
                p("strokeRandomness", "Stroke Randomness", num(1.0), slider(0.0, 2.0, 0.0, 2.0, 2)),
                p(
                    "paintSurface",
                    "Paint Surface",
                    Value::Enum(0),
                    popup(&["Paint On Original Image", "Paint On Transparent", "Paint On White", "Paint On Black"]),
                ),
                p("blendWithOriginal", "Blend With Original", num(0.0), pct()),
            ],
            brush_strokes,
        ),
        spec(
            "ec.stylize.cckaleida",
            "Kaleidoscope",
            vec![
                p("center", "Center", pt(0.5, 0.5), ParamUi::Point),
                p("size", "Size", num(100.0), slider(1.0, 1000.0, 1.0, 400.0, 1)),
                p("mirroring", "Mirroring", Value::Enum(0), popup(&["Flower", "Starlish", "Unfold", "Wheel", "Triangles"])),
                p("rotation", "Rotation", num(0.0), ParamUi::Angle),
            ],
            kaleida,
        ),
        spec(
            "ec.stylize.ccrepetile",
            "Edge Tile Repeat",
            vec![
                p("expandRight", "Expand Right", num(0.0), slider(0.0, 4000.0, 0.0, 500.0, 0)),
                p("expandLeft", "Expand Left", num(0.0), slider(0.0, 4000.0, 0.0, 500.0, 0)),
                p("expandDown", "Expand Down", num(0.0), slider(0.0, 4000.0, 0.0, 500.0, 0)),
                p("expandUp", "Expand Up", num(0.0), slider(0.0, 4000.0, 0.0, 500.0, 0)),
                p("tiling", "Tiling", Value::Enum(0), popup(&["Repeat", "Unfold", "Checker Flip H", "Checker Flip V", "Checker Flip"])),
            ],
            repetile,
        ),
        spec(
            "ec.stylize.motiontile",
            "Motion Tile",
            vec![
                p("tileCenter", "Tile Center", pt(0.5, 0.5), ParamUi::Point),
                p("tileWidth", "Tile Width", num(100.0), slider(1.0, 1000.0, 1.0, 200.0, 1)),
                p("tileHeight", "Tile Height", num(100.0), slider(1.0, 1000.0, 1.0, 200.0, 1)),
                p("outputWidth", "Output Width", num(100.0), slider(1.0, 1000.0, 1.0, 400.0, 1)),
                p("outputHeight", "Output Height", num(100.0), slider(1.0, 1000.0, 1.0, 400.0, 1)),
                p("mirrorEdges", "Mirror Edges", Value::Bool(false), ParamUi::Checkbox),
                p("phase", "Phase", num(0.0), ParamUi::Angle),
                p("horizontalPhaseShift", "Horizontal Phase Shift", Value::Bool(false), ParamUi::Checkbox),
            ],
            motion_tile,
        ),
        spec(
            "ec.stylize.coloremboss",
            "Color Emboss",
            vec![
                p("direction", "Direction", num(45.0), ParamUi::Angle),
                p("relief", "Relief", num(1.5), slider(0.0, 10.0, 0.0, 10.0, 2)),
                p("contrast", "Contrast", num(100.0), slider(0.0, 500.0, 0.0, 500.0, 0)),
                p("blend", "Blend With Original", num(0.0), pct()),
            ],
            color_emboss,
        ),
        spec(
            "ec.stylize.ccvignette",
            "Vignette",
            vec![
                p("amount", "Amount", num(50.0), slider(-100.0, 100.0, -100.0, 100.0, 1)),
                p("angleOfView", "Angle of View", num(60.0), slider(0.0, 179.0, 0.0, 179.0, 1)),
                p("center", "Center", pt(0.5, 0.5), ParamUi::Point),
                p("pinHighlights", "Pin Highlights", num(0.0), pct()),
            ],
            vignette,
        ),
        spec(
            "ec.stylize.ccthreshold",
            "Channel Threshold",
            vec![
                p("threshold", "Threshold", num(127.0), thr()),
                p("channel", "Channel", Value::Enum(0), popup(&["Luminance", "RGB", "Saturation", "Alpha"])),
                p("invert", "Invert", Value::Bool(false), ParamUi::Checkbox),
                p("blend", "Blend w. Original", num(0.0), pct()),
            ],
            cc_threshold,
        ),
        spec(
            "ec.stylize.ccthresholdrgb",
            "RGB Threshold",
            vec![
                p("redThreshold", "Red Threshold", num(127.0), thr()),
                p("greenThreshold", "Green Threshold", num(127.0), thr()),
                p("blueThreshold", "Blue Threshold", num(127.0), thr()),
                p("invertRed", "Invert Red Channel", Value::Bool(false), ParamUi::Checkbox),
                p("invertGreen", "Invert Green Channel", Value::Bool(false), ParamUi::Checkbox),
                p("invertBlue", "Invert Blue Channel", Value::Bool(false), ParamUi::Checkbox),
                p("blend", "Blend w. Original", num(0.0), pct()),
            ],
            cc_threshold_rgb,
        ),
    ]
}


