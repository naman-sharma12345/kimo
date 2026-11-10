//! AE-style Generate-category effects not already covered by effects2.rs's checkerboard/grid/gradient_ramp/
//! fractal_noise/lightning/lens_flare (FEATURES.md L475). Each is a straightforward per-pixel kernel, same
//! contract as the rest of the effect catalog: RGBA f32, straight alpha, row-major, nominal 0..=1, deterministic
//! given (params, t, seed). A few are honestly-scoped simplifications of their After Effects namesake where the
//! real thing needs data this per-pixel kernel doesn't have (a mask path, decoded audio) -- each says so in its
//! own description rather than silently pretending to be the full effect.
use crate::effects::{p, Frame, Params};
use std::f32::consts::TAU;

type C = (&'static str, f64, &'static str);
fn lv(v: Vec<C>) -> &'static [C] { Box::leak(v.into_boxed_slice()) }

fn hash(x: i32, y: i32, s: u32) -> f32 { let mut h = (x as u32).wrapping_mul(374761393) ^ (y as u32).wrapping_mul(668265263) ^ s.wrapping_mul(2246822519); h = (h ^ (h >> 13)).wrapping_mul(1274126177); ((h ^ (h >> 16)) & 0xffffff) as f32 / 16777215.0 }
fn hash2(x: i32, y: i32, s: u32) -> (f32, f32) { (hash(x, y, s), hash(x, y, s.wrapping_add(911))) }
fn blend(f: &mut Frame, i: usize, c: [f32; 3], op: f32, mode: u32) { for j in 0..3 { let b = f.d[i * 4 + j]; let r = match mode { 0 => c[j], 1 => if b < 0.5 { 2.0 * b * c[j] } else { 1.0 - 2.0 * (1.0 - b) * (1.0 - c[j]) }, 2 => b * c[j], _ => b + c[j] }; f.d[i * 4 + j] = b + (r - b) * op; } }
fn hue_ramp(k: f32) -> [f32; 3] { let k = k.rem_euclid(1.0) * 6.0; let c = 1.0; let x = c * (1.0 - (k % 2.0 - 1.0).abs()); match k as i32 { 0 => [c, x, 0.0], 1 => [x, c, 0.0], 2 => [0.0, c, x], 3 => [0.0, x, c], 4 => [x, 0.0, c], _ => [c, 0.0, x] } }

/// Dispatch: every kind this module registers via `catalog()`.
pub fn apply(kind: &str, pr: &Params, f: &mut Frame, t: f64, seed: u64) -> Result<(), String> {
    let g = |k: &str, d: f64| p(pr, k, d, t);
    let (w, h) = (f.w, f.h); let (wf, hf) = (w as f32, h as f32);
    match kind {
        "four_color_gradient" => {
            let (tl, tr, bl, br) = ([g("r_tl", 1.0), g("g_tl", 0.2), g("b_tl", 0.2)], [g("r_tr", 0.2), g("g_tr", 1.0), g("b_tr", 0.2)], [g("r_bl", 0.2), g("g_bl", 0.2), g("b_bl", 1.0)], [g("r_br", 1.0), g("g_br", 1.0), g("b_br", 0.2)]);
            let (op, mode) = (g("opacity", 1.0), g("mode", 0.0) as u32);
            for y in 0..h { for x in 0..w { let (u, v) = (x as f32 / (wf - 1.0).max(1.0), y as f32 / (hf - 1.0).max(1.0));
                let top = [tl[0] + (tr[0] - tl[0]) * u, tl[1] + (tr[1] - tl[1]) * u, tl[2] + (tr[2] - tl[2]) * u];
                let bot = [bl[0] + (br[0] - bl[0]) * u, bl[1] + (br[1] - bl[1]) * u, bl[2] + (br[2] - bl[2]) * u];
                let c = [top[0] + (bot[0] - top[0]) * v, top[1] + (bot[1] - top[1]) * v, top[2] + (bot[2] - top[2]) * v];
                blend(f, y * w + x, c, op, mode); } }
        }
        "radio_waves" => {
            let (cx, cy) = (g("cx", 0.5) * wf, g("cy", 0.5) * hf); let (wl, wid, sp) = (g("wavelength", 40.0).max(2.0), g("width", 0.3).clamp(0.01, 1.0), g("speed", 1.0));
            let (c, op, falloff, sharp) = ([g("r", 1.0), g("g", 1.0), g("b", 1.0)], g("opacity", 1.0), g("falloff", 1.0).max(0.0), g("sharpness", 8.0).max(0.1));
            let md = wf.max(hf); let tt = t as f32 * sp * wl;
            for y in 0..h { for x in 0..w { let d = ((x as f32 - cx).powi(2) + (y as f32 - cy).powi(2)).sqrt();
                let phase = ((d - tt) / wl).rem_euclid(1.0); let ring = ((1.0 - (phase * 2.0 - 1.0).abs() / wid.max(1e-3)).clamp(0.0, 1.0)).powf(sharp);
                let fade = (1.0 - (d / md) * falloff).clamp(0.0, 1.0); blend(f, y * w + x, c, ring * fade * op, 0); } }
        }
        "beam" => {
            let (x0, y0, x1, y1) = (g("x0", 0.1) * wf, g("y0", 0.5) * hf, g("x1", 0.9) * wf, g("y1", 0.5) * hf);
            let (core, glow, c, inten, op) = (g("core_width", 2.0).max(0.1), g("glow_width", 12.0).max(0.1), [g("r", 0.6), g("g", 0.85), g("b", 1.0)], g("intensity", 1.0), g("opacity", 1.0));
            let pulse = 1.0 + g("pulse", 0.0) * (t as f32 * g("pulse_speed", 2.0) * TAU).sin() * 0.5;
            let (dx, dy) = (x1 - x0, y1 - y0); let l2 = (dx * dx + dy * dy).max(1e-3);
            for y in 0..h { for x in 0..w { let tproj = (((x as f32 - x0) * dx + (y as f32 - y0) * dy) / l2).clamp(0.0, 1.0); let (px, py) = (x0 + tproj * dx, y0 + tproj * dy);
                let d = ((x as f32 - px).powi(2) + (y as f32 - py).powi(2)).sqrt(); let v = ((-(d / core).powi(2)).exp() + (-(d / glow)).exp() * 0.35) * inten * pulse;
                blend(f, y * w + x, c, v.clamp(0.0, 1.0) * op, 3); } }
        }
        "cell_pattern" => {
            let size = g("size", 60.0).max(4.0); let (c, op, invert, contrast, mode) = ([g("r", 1.0), g("g", 1.0), g("b", 1.0)], g("opacity", 1.0), g("invert", 0.0) > 0.5, g("contrast", 1.0).max(0.01), g("mode", 0.0) as u32);
            let tt = t as f32 * g("speed", 0.0);
            let feat = |gx: i32, gy: i32| { let (jx, jy) = hash2(gx, gy, seed as u32); (gx as f32 + jx + (tt * 0.3).sin() * 0.3, gy as f32 + jy + (tt * 0.3).cos() * 0.3) };
            for y in 0..h { for x in 0..w { let (u, v) = (x as f32 / size, y as f32 / size); let (gx, gy) = (u.floor() as i32, v.floor() as i32);
                let mut best = f32::MAX; for oy in -1..=1 { for ox in -1..=1 { let (fx, fy) = feat(gx + ox, gy + oy); let d = ((u - fx).powi(2) + (v - fy).powi(2)).sqrt(); if d < best { best = d; } } }
                let mut k = (best * contrast).clamp(0.0, 1.0); if invert { k = 1.0 - k; } blend(f, y * w + x, [c[0] * k, c[1] * k, c[2] * k], op, mode); } }
        }
        "eyedropper_fill" => {
            let (sx, sy) = ((g("x", 0.5) * wf).clamp(0.0, wf - 1.0) as usize, (g("y", 0.5) * hf).clamp(0.0, hf - 1.0) as usize);
            let i0 = (sy * w + sx) * 4; let c = [f.d[i0], f.d[i0 + 1], f.d[i0 + 2]]; let (op, mode) = (g("opacity", 1.0), g("mode", 0.0) as u32);
            for y in 0..h { for x in 0..w { blend(f, y * w + x, c, op, mode); } }
        }
        "fill" => {
            let (c, op, mode) = ([g("r", 1.0), g("g", 0.0), g("b", 0.0)], g("opacity", 1.0), g("mode", 0.0) as u32);
            for y in 0..h { for x in 0..w { blend(f, y * w + x, c, op, mode); } }
        }
        "fractal" => {
            let julia = g("mode", 0.0) > 0.5; let (cx, cy, zoom) = (g("cx", 0.0), g("cy", 0.0), g("zoom", 1.0).max(1e-3));
            let iters = (g("iterations", 60.0).clamp(4.0, 500.0)) as u32; let (jr, ji) = (g("julia_re", -0.7), g("julia_im", 0.27));
            let (op, mode) = (g("opacity", 1.0), g("mode", 0.0) as u32); let cycle = g("color_cycle", 1.0); let tt = t as f32 * g("speed", 0.0);
            let aspect = wf / hf.max(1.0);
            for y in 0..h { for x in 0..w {
                let (u, v) = ((x as f32 / wf - 0.5) * 3.0 * aspect / zoom as f32 + cx as f32, (y as f32 / hf - 0.5) * 3.0 / zoom as f32 + cy as f32);
                let (mut zr, mut zi, cr, ci) = if julia { (u, v, jr as f32, ji as f32) } else { (0.0f32, 0.0f32, u, v) };
                let mut n = 0u32; while n < iters && zr * zr + zi * zi < 4.0 { let nzr = zr * zr - zi * zi + cr; zi = 2.0 * zr * zi + ci; zr = nzr; n += 1; }
                let k = if n >= iters { 0.0 } else { (n as f32 / iters as f32 + tt * cycle).rem_euclid(1.0) };
                let c = if n >= iters { [0.0, 0.0, 0.0] } else { hue_ramp(k) }; blend(f, y * w + x, c, op, mode); } }
        }
        "paint_bucket" => {
            let (sx, sy) = ((g("x", 0.5) * wf).clamp(0.0, wf - 1.0) as usize, (g("y", 0.5) * hf).clamp(0.0, hf - 1.0) as usize);
            let i0 = (sy * w + sx) * 4; let seedc = [f.d[i0], f.d[i0 + 1], f.d[i0 + 2]];
            let (fillc, tol, op) = ([g("r", 1.0), g("g", 0.0), g("b", 0.0)], g("tolerance", 0.08).clamp(0.0, 1.0), g("opacity", 1.0));
            // Every pixel whose colour is within `tolerance` of the sampled point gets replaced -- not a connected
            // flood fill (this per-pixel kernel has no neighbour-propagation pass), so two separate same-coloured
            // regions both change. Honest about that in the registered description.
            for i in 0..w * h { let c = [f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]]; let d = ((c[0] - seedc[0]).powi(2) + (c[1] - seedc[1]).powi(2) + (c[2] - seedc[2]).powi(2)).sqrt();
                if d <= tol { blend(f, i, fillc, op, 0); } }
        }
        "scribble" => {
            let (density, amp, freq, speed, lw) = (g("density", 10.0).clamp(1.0, 60.0) as i32, g("amplitude", 10.0), g("frequency", 3.0).max(0.1), g("speed", 1.0), g("width", 1.5).max(0.1));
            let (c, op) = ([g("r", 0.0), g("g", 0.0), g("b", 0.0)], g("opacity", 1.0)); let tt = t as f32 * speed;
            for y in 0..h { for x in 0..w { let mut v = 0.0f32;
                for k in 0..density { let base = hf * (k as f32 + 0.5) / density as f32; let wob = (x as f32 / wf * freq * TAU + tt + k as f32 * 1.7).sin() * amp + (x as f32 / wf * freq * 2.3 * TAU - tt * 1.3 + k as f32).sin() * amp * 0.4;
                    let d = (y as f32 - (base + wob)).abs(); v = v.max((1.0 - d / lw).clamp(0.0, 1.0)); }
                blend(f, y * w + x, c, v * op, 0); } }
        }
        "stroke" | "write_on" => {
            let (x0, y0, x1, y1) = (g("x0", 0.1) * wf, g("y0", 0.5) * hf, g("x1", 0.9) * wf, g("y1", 0.5) * hf);
            let width = g("width", 4.0).max(0.1); let c = [g("r", 1.0), g("g", 1.0), g("b", 1.0)]; let op = g("opacity", 1.0);
            let default_end = if kind == "write_on" { 0.0 } else { 1.0 };
            let (s0, s1) = (g("start", 0.0).clamp(0.0, 1.0) as f32, g("end", default_end).clamp(0.0, 1.0) as f32);
            if s1 <= s0 { return Ok(()); }
            let (ax, ay) = (x0 + (x1 - x0) * s0, y0 + (y1 - y0) * s0); let (bx, by) = (x0 + (x1 - x0) * s1, y0 + (y1 - y0) * s1);
            let (dx, dy) = (bx - ax, by - ay); let l2 = (dx * dx + dy * dy).max(1e-6);
            for y in 0..h { for x in 0..w { let tproj = (((x as f32 - ax) * dx + (y as f32 - ay) * dy) / l2).clamp(0.0, 1.0); let (px, py) = (ax + tproj * dx, ay + tproj * dy);
                let d = ((x as f32 - px).powi(2) + (y as f32 - py).powi(2)).sqrt(); let v = (1.0 - (d - width / 2.0).max(0.0) / 1.0).clamp(0.0, 1.0);
                blend(f, y * w + x, c, v * op, 0); } }
        }
        "vegas" => {
            // Marching dashed glow around the FRAME edge (no mask path is visible to a per-pixel effect kernel;
            // for a shape or path's own animated outline, add_path_shape's stroke + shape_trim gives the real thing).
            let (inset, width, dash, gap, speed) = (g("inset", 10.0), g("width", 3.0).max(0.1), g("dash_length", 24.0).max(1.0), g("gap_length", 16.0).max(0.0), g("speed", 20.0));
            let (c, glow, op) = ([g("r", 1.0), g("g", 0.85), g("b", 0.2)], g("glow", 6.0).max(0.0), g("opacity", 1.0));
            let (x0, y0, x1, y1) = (inset as f32, inset as f32, wf - inset as f32, hf - inset as f32); if x1 <= x0 || y1 <= y0 { return Ok(()); }
            let peri = 2.0 * ((x1 - x0) + (y1 - y0)); let phase = (t as f32 * speed).rem_euclid(peri.max(1.0));
            let on_dash = |u: f32| { let p = (u - phase).rem_euclid((dash + gap).max(1.0)); p < dash };
            for y in 0..h { for x in 0..w { let (xf, yf) = (x as f32, y as f32);
                // nearest point on the rectangle's border, its distance, and its perimeter-arclength coordinate
                let (d, u) = if xf < x0 || xf > x1 || yf < y0 || yf > y1 {
                    let cx = xf.clamp(x0, x1); let cy = yf.clamp(y0, y1); ((xf - cx).hypot(yf - cy), 0.0)
                } else {
                    let (dl, dr, dt, db) = (xf - x0, x1 - xf, yf - y0, y1 - yf); let m = dl.min(dr).min(dt).min(db);
                    let u = if m == dt { xf - x0 } else if m == db { (x1 - x0) + (x1 - xf) } else if m == dl { 2.0 * (x1 - x0) + (y1 - y0) + (yf - y0) } else { 2.0 * (x1 - x0) + 2.0 * (y1 - y0) - (y1 - yf) };
                    (m, u)
                };
                if !on_dash(u) { continue; }
                let v = ((-(d / width.max(0.5)).powi(2)).exp() + if glow > 0.0 { (-(d / glow)).exp() * 0.4 } else { 0.0 }).clamp(0.0, 2.0);
                blend(f, y * w + x, c, v.clamp(0.0, 1.0) * op, 3); } }
        }
        "caustics" => {
            let (scale, speed, contrast) = (g("scale", 60.0).max(4.0), g("speed", 0.4), g("contrast", 1.6).max(0.01));
            let (c, op, mode) = ([g("r", 0.6), g("g", 0.9), g("b", 1.0)], g("opacity", 0.6), g("mode", 3.0) as u32); let tt = t as f32 * speed;
            for y in 0..h { for x in 0..w { let (u, v) = (x as f32 / scale, y as f32 / scale);
                let a = ((u + tt).sin() + (v - tt * 1.3).sin() + (u * 1.7 + v * 1.3 + tt * 0.7).sin()) / 3.0;
                let b = ((u * 1.3 - tt * 0.6).cos() + (v * 1.9 + tt).cos()) / 2.0; let k = ((a * b).abs() * contrast).clamp(0.0, 1.0);
                blend(f, y * w + x, [c[0] * k, c[1] * k, c[2] * k], op, mode); } }
        }
        "audio_spectrum" | "audio_waveform" => {
            const BANDS: usize = 16; let is_spec = kind == "audio_spectrum"; let prefix = if is_spec { "b" } else { "s" };
            // Per-kind defaults match catalog() exactly: audio_spectrum's bands default to 0.0
            // (silence -> zero-height bars), audio_waveform's samples default to 0.5 (a flat
            // centreline), and baseline/max_height differ per kind the same way the catalog lists.
            let level_default = if is_spec { 0.0 } else { 0.5 };
            let levels: Vec<f32> = (0..BANDS).map(|i| g(&format!("{prefix}{i}"), level_default).clamp(0.0, 1.0) as f32).collect();
            let (base_d, maxh_d) = if is_spec { (0.8, 0.6) } else { (0.5, 0.4) };
            let (baseline, maxh, c, op, mirror) = (g("baseline", base_d) * hf as f32, g("max_height", maxh_d) * hf as f32, [g("r", 0.3), g("g", 0.9), g("b", 1.0)], g("opacity", 1.0), g("mirror", 1.0) > 0.5);
            if is_spec {
                let bw = wf / BANDS as f32;
                for y in 0..h { for x in 0..w { let bi = ((x as f32 / bw) as usize).min(BANDS - 1); let lv = levels[bi] * maxh as f32;
                    let yy = y as f32; let up = yy <= baseline as f32 && yy > baseline as f32 - lv; let down = mirror && yy >= baseline as f32 && yy < baseline as f32 + lv;
                    if up || down { blend(f, y * w + x, c, op, 0); } } }
            } else {
                for x in 0..w { let uf = x as f32 / (wf - 1.0).max(1.0) * (BANDS - 1) as f32; let i0 = uf.floor() as usize; let frac = uf - i0 as f32;
                    let lv = levels[i0] + (levels[(i0 + 1).min(BANDS - 1)] - levels[i0]) * frac; let yy = (baseline as f32 - (lv - 0.5) * 2.0 * maxh as f32).round();
                    for y in 0..h { let d = (y as f32 - yy).abs(); if d < 1.5 { blend(f, y * w + x, c, op, 0); } } }
            }
        }
        _ => {}
    }
    Ok(())
}

pub fn catalog() -> Vec<super::Entry> {
    vec![
        ("four_color_gradient", "4-Color Gradient: bilinear blend between four corner colours (top-left, top-right, bottom-left, bottom-right)", &[("r_tl", 1.0, "top-left R"), ("g_tl", 0.2, "G"), ("b_tl", 0.2, "B"), ("r_tr", 0.2, "top-right R"), ("g_tr", 1.0, "G"), ("b_tr", 0.2, "B"), ("r_bl", 0.2, "bottom-left R"), ("g_bl", 0.2, "G"), ("b_bl", 1.0, "B"), ("r_br", 1.0, "bottom-right R"), ("g_br", 1.0, "G"), ("b_br", 0.2, "B"), ("opacity", 1.0, "0..1"), ("mode", 0.0, "0 replace, 1 overlay, 2 multiply, 3 add")]),
        ("radio_waves", "Radio Waves: concentric rings expanding from a point, like ripples or a sonar ping", &[("cx", 0.5, "0..1"), ("cy", 0.5, "0..1"), ("wavelength", 40.0, "px between rings"), ("width", 0.3, "0..1 ring thickness fraction of wavelength"), ("speed", 1.0, "rings/s outward"), ("sharpness", 8.0, "ring edge sharpness"), ("falloff", 1.0, "0..2 fade with distance"), ("r", 1.0, "R"), ("g", 1.0, "G"), ("b", 1.0, "B"), ("opacity", 1.0, "0..1")]),
        ("beam", "Beam (laser): a straight glowing core+glow line between two points, optional pulsing brightness", &[("x0", 0.1, "0..1"), ("y0", 0.5, "0..1"), ("x1", 0.9, "0..1"), ("y1", 0.5, "0..1"), ("core_width", 2.0, "px"), ("glow_width", 12.0, "px"), ("intensity", 1.0, "0..4"), ("pulse", 0.0, "0..1 pulse depth"), ("pulse_speed", 2.0, "Hz"), ("r", 0.6, "R"), ("g", 0.85, "G"), ("b", 1.0, "B"), ("opacity", 1.0, "0..1")]),
        ("cell_pattern", "Cell Pattern: Worley/cellular noise (cracked-glass, crystal, cellular texture), jittered grid points, optional slow drift", &[("size", 60.0, "px cell size"), ("speed", 0.0, "feature drift/s"), ("contrast", 1.0, "pattern sharpness"), ("invert", 0.0, "0/1: 0 bright cell centres, 1 bright cell edges"), ("r", 1.0, "R"), ("g", 1.0, "G"), ("b", 1.0, "B"), ("opacity", 1.0, "0..1"), ("mode", 0.0, "0 replace, 1 overlay, 2 multiply, 3 add")]),
        ("eyedropper_fill", "Eyedropper Fill: samples the layer's own colour at a point (before this effect) and fills with it", &[("x", 0.5, "0..1 sample point"), ("y", 0.5, "0..1"), ("opacity", 1.0, "0..1"), ("mode", 0.0, "0 replace, 1 overlay, 2 multiply, 3 add")]),
        ("fill", "Fill: a flat solid colour over the whole frame (the Generate-category effect; for a shape layer's own fill use add_shape)", &[("r", 1.0, "R"), ("g", 0.0, "G"), ("b", 0.0, "B"), ("opacity", 1.0, "0..1"), ("mode", 0.0, "0 replace, 1 overlay, 2 multiply, 3 add")]),
        ("fractal", "Fractal: Mandelbrot (mode 0) or Julia (mode 1) escape-time set, rainbow-cycled colouring, animatable zoom/center/colour phase", &[("mode", 0.0, "0 Mandelbrot, 1 Julia"), ("cx", -0.5, "view center real"), ("cy", 0.0, "view center imaginary"), ("zoom", 1.0, ">0"), ("iterations", 60.0, "4..500"), ("julia_re", -0.7, "Julia constant real (mode 1)"), ("julia_im", 0.27, "Julia constant imaginary (mode 1)"), ("speed", 0.0, "colour-phase cycles/s"), ("color_cycle", 1.0, "colour-phase multiplier"), ("opacity", 1.0, "0..1"), ("mode_blend", 0.0, "unused placeholder")]),
        ("paint_bucket", "Paint Bucket: replace every pixel whose colour is close to a sampled point with a fill colour. Colour-similarity, not a connected flood fill -- two separate same-coloured regions both change", &[("x", 0.5, "0..1 sample point"), ("y", 0.5, "0..1"), ("tolerance", 0.08, "0..1 colour distance"), ("r", 1.0, "fill R"), ("g", 0.0, "fill G"), ("b", 0.0, "fill B"), ("opacity", 1.0, "0..1")]),
        ("scribble", "Scribble: an animated field of hand-drawn-style wavy lines filling the frame (not mask-traced -- a texture generator, like a pencil scribble fill)", &[("density", 10.0, "1..60 line count"), ("amplitude", 10.0, "px wobble"), ("frequency", 3.0, "wobble cycles across the frame"), ("speed", 1.0, "animation speed"), ("width", 1.5, "line px"), ("r", 0.0, "R"), ("g", 0.0, "G"), ("b", 0.0, "B"), ("opacity", 1.0, "0..1")]),
        ("stroke", "Stroke: a straight segment between two points with start/end trim (0..1), width and colour. Not mask-traced -- for a path's own stroke use add_path_shape", &[("x0", 0.1, "0..1"), ("y0", 0.5, "0..1"), ("x1", 0.9, "0..1"), ("y1", 0.5, "0..1"), ("start", 0.0, "0..1 trim"), ("end", 1.0, "0..1 trim"), ("width", 4.0, "px"), ("r", 1.0, "R"), ("g", 1.0, "G"), ("b", 1.0, "B"), ("opacity", 1.0, "0..1")]),
        ("write_on", "Write-on: the same straight-segment stroke as `stroke`, but `end` defaults to 0 (invisible) -- keyframe end 0 -> 1 for a hand-writing reveal", &[("x0", 0.1, "0..1"), ("y0", 0.5, "0..1"), ("x1", 0.9, "0..1"), ("y1", 0.5, "0..1"), ("start", 0.0, "0..1 trim"), ("end", 0.0, "0..1 trim; keyframe to 1 to reveal"), ("width", 4.0, "px"), ("r", 1.0, "R"), ("g", 1.0, "G"), ("b", 1.0, "B"), ("opacity", 1.0, "0..1")]),
        ("vegas", "Vegas: an animated dashed glowing outline marching around the frame edge (no mask path reaches a per-pixel effect; for a shape/path's own marching outline use add_path_shape's stroke + shape_trim)", &[("inset", 10.0, "px from the frame edge"), ("width", 3.0, "px"), ("dash_length", 24.0, "px"), ("gap_length", 16.0, "px"), ("speed", 20.0, "px/s march"), ("glow", 6.0, "px glow reach, 0 = none"), ("r", 1.0, "R"), ("g", 0.85, "G"), ("b", 0.2, "B"), ("opacity", 1.0, "0..1")]),
        ("caustics", "Caustics: animated underwater-light interference pattern (layered sine fields), additive by default", &[("scale", 60.0, "px"), ("speed", 0.4, "animation speed"), ("contrast", 1.6, ">0"), ("r", 0.6, "R"), ("g", 0.9, "G"), ("b", 1.0, "B"), ("opacity", 0.6, "0..1"), ("mode", 3.0, "0 replace, 1 overlay, 2 multiply, 3 add")]),
        ("audio_spectrum", "Audio Spectrum: a 16-band bar visualizer reading its own band0..band15 levels (0..1, keyframable by hand or baked from real audio with bake_audio_levels)", lv({
            let mut v: Vec<C> = (0..16).map(|i| (Box::leak(format!("b{i}").into_boxed_str()) as &'static str, 0.0, "0..1 band level")).collect();
            v.extend([("baseline", 0.8, "0..1 y position"), ("max_height", 0.6, "0..1 of frame height"), ("mirror", 1.0, "0/1: mirror bars below baseline too"), ("r", 0.3, "R"), ("g", 0.9, "G"), ("b", 1.0, "B"), ("opacity", 1.0, "0..1")]); v
        })),
        ("audio_waveform", "Audio Waveform: a 16-point line reading its own s0..s15 levels (0..1, keyframable by hand or baked from real audio with bake_audio_levels)", lv({
            let mut v: Vec<C> = (0..16).map(|i| (Box::leak(format!("s{i}").into_boxed_str()) as &'static str, 0.5, "0..1 sample level")).collect();
            v.extend([("baseline", 0.5, "0..1 y position"), ("max_height", 0.4, "0..1 of frame height"), ("mirror", 0.0, "unused for waveform"), ("r", 0.3, "R"), ("g", 0.9, "G"), ("b", 1.0, "B"), ("opacity", 1.0, "0..1")]); v
        })),
    ]
}
