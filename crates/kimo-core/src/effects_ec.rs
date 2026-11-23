//! After Effects-class effects that kimo lacked, batch 6 of docs/CRAFT_PARITY_BACKLOG.md: distort (wave warp, spherize,
//! magnify, optics compensation, mesh warp, transform), perspective (cylinder and sphere wrap, bevel alpha), keying
//! (linear colour key, colour range, extract, unmult, key cleaner), colour (colour balance, HLS balance, selective colour,
//! photo filter, colorama, change colour, change to colour, leave colour, equalize, auto colour, broadcast colours,
//! black & white, tritone, vibrance, gamma/pedestal/gain), channel (arithmetic, minimax, channel combiner) and
//! noise/generate (noise HLS, noise alpha, circle, ellipse).
//!
//! The list and parameter sets follow EffectCraft's catalogue (Apache-2.0, github.com/storytold/effectcraft), which
//! follows After Effects. Each effect is an independent implementation on kimo's frames (RGBA f32, straight alpha,
//! display-referred). Sizes in px are for 1080p and scale with the frame in current render quality.
use crate::effects::{Frame, Params};

type C = (&'static str, f64, &'static str);
pub const CATALOG4: &[(&str, &str, &[C])] = &[
    // ---- distort
    ("wave_warp", "Wave Warp: waves travel across the layer and push it sideways (flags, water, heat, wobbly text)", &[("wave_type", 0.0, "0 sine, 1 square, 2 triangle, 3 sawtooth, 4 smooth noise"), ("height", 10.0, "px of displacement"), ("width", 40.0, "px wavelength"), ("direction", 90.0, "degrees the waves travel (90 = to the right)"), ("speed", 1.0, "waves per second"), ("phase", 0.0, "degrees"), ("pinning", 0.0, "0 none, 1 all edges, 2 left/right, 3 top/bottom: those edges stay still")]),
    ("spherize", "Spherize: the layer seen through a glass ball (positive) or pinched in (negative)", &[("amount", 1.0, "-1..1"), ("radius", 0.4, "fraction of the frame's short side"), ("cx", 0.5, "0..1"), ("cy", 0.5, "0..1")]),
    ("magnify", "Magnify: a magnifying glass over one spot", &[("cx", 0.5, "0..1"), ("cy", 0.5, "0..1"), ("radius", 150.0, "px"), ("magnification", 2.0, "1..20"), ("feather", 4.0, "px soft edge"), ("shape", 0.0, "0 circle, 1 square"), ("border", 0.0, "px ring"), ("border_r", 1.0, ""), ("border_g", 1.0, ""), ("border_b", 1.0, "")]),
    ("optics_compensation", "Optics Compensation: add (or with reverse=1 remove) wide-angle lens barrel distortion", &[("fov", 60.0, "field of view degrees 0..170"), ("reverse", 0.0, "0 add barrel, 1 remove it"), ("cx", 0.5, "0..1"), ("cy", 0.5, "0..1")]),
    ("mesh_warp", "Mesh Warp: drag the points of a grid and the picture follows. strings.mesh = JSON [[dx,dy], ...] px offsets, (rows+1)*(cols+1) points row by row", &[("rows", 3.0, "1..32"), ("cols", 3.0, "1..32")]),
    ("transform", "Transform effect: anchor, position, scale, skew, rotation and opacity inside the effect stack (so later effects see the moved layer)", &[("anchor_x", 0.5, "0..1 of the layer"), ("anchor_y", 0.5, "0..1"), ("x", 0.5, "0..1 position of the anchor"), ("y", 0.5, "0..1"), ("scale", 1.0, "uniform"), ("scale_w", 1.0, "extra horizontal"), ("scale_h", 1.0, "extra vertical"), ("skew", 0.0, "degrees"), ("skew_axis", 0.0, "degrees"), ("rotation", 0.0, "degrees"), ("opacity", 1.0, "0..1")]),
    // ---- perspective
    ("cylinder_wrap", "Cylinder wrap: the layer printed on a turning cylinder (label, banner, spinning logo)", &[("radius", 0.35, "fraction of the frame width"), ("rotation", 0.0, "degrees around the vertical axis"), ("tilt", 0.0, "-1..1 look from above/below"), ("light", 0.6, "0..1 shading"), ("cx", 0.5, "0..1"), ("cy", 0.5, "0..1")]),
    ("sphere_wrap", "Sphere wrap: the layer wrapped round a turning ball (planet, globe). The whole layer covers the sphere like a map", &[("radius", 0.3, "fraction of the frame's short side"), ("rotation", 0.0, "degrees around the vertical axis"), ("tilt", 15.0, "degrees toward the viewer"), ("light", 0.7, "0..1 shading"), ("light_angle", 135.0, "degrees"), ("cx", 0.5, "0..1"), ("cy", 0.5, "0..1")]),
    ("bevel_alpha", "Bevel Alpha: a lit bevelled edge on the layer's alpha outline (text, logos)", &[("thickness", 4.0, "px"), ("light_angle", 135.0, "degrees (light from top-left)"), ("intensity", 0.4, "0..1"), ("light_r", 1.0, ""), ("light_g", 1.0, ""), ("light_b", 1.0, "")]),
    // ---- keying
    ("linear_color_key", "Linear Color Key: transparent where the picture is near a colour (RGB, hue or chroma match)", &[("r", 0.0, "key colour"), ("g", 1.0, ""), ("b", 0.0, ""), ("tolerance", 0.15, "0..1 fully keyed inside this"), ("softness", 0.1, "0..1 ramp beyond it"), ("match", 0.0, "0 RGB, 1 hue, 2 chroma"), ("invert", 0.0, "0/1 keep only the colour")]),
    ("color_range", "Color Range key: transparent inside a box of colour (luma and both chroma ranges, YUV)", &[("y_min", 0.0, "0..1"), ("y_max", 1.0, ""), ("u_min", -0.5, "-0.5..0.5 blue-yellow"), ("u_max", -0.1, ""), ("v_min", -0.5, "-0.5..0.5 red-green"), ("v_max", -0.1, ""), ("softness", 0.03, "ramp width"), ("invert", 0.0, "0/1")]),
    ("extract", "Extract: key by brightness; keeps what is between the black and white points", &[("channel", 0.0, "0 luma, 1 red, 2 green, 3 blue, 4 alpha"), ("black_point", 0.0, "0..1"), ("white_point", 1.0, "0..1"), ("black_softness", 0.0, "0..1"), ("white_softness", 0.0, "0..1"), ("invert", 0.0, "0/1")]),
    ("unmult", "Unmult: black becomes transparent (fire, sparks, light leaks or flares shot on black)", &[("amount", 1.0, "0..1")]),
    ("key_cleaner", "Key Cleaner: closes pinholes, removes specks and smooths a keyed matte's edge", &[("radius", 2.0, "px"), ("fill_holes", 1.0, "0/1"), ("remove_specks", 1.0, "0/1"), ("edge_smooth", 0.5, "0..1")]),
    // ---- colour
    ("color_balance", "Color Balance: push shadows, midtones and highlights toward red/green/blue separately", &[("shadow_r", 0.0, "-1..1"), ("shadow_g", 0.0, ""), ("shadow_b", 0.0, ""), ("mid_r", 0.0, ""), ("mid_g", 0.0, ""), ("mid_b", 0.0, ""), ("high_r", 0.0, ""), ("high_g", 0.0, ""), ("high_b", 0.0, ""), ("preserve_luminosity", 1.0, "0/1")]),
    ("color_balance_hls", "Color Balance (HLS): rotate hue, shift lightness and saturation", &[("hue", 0.0, "degrees"), ("lightness", 0.0, "-1..1"), ("saturation", 0.0, "-1..1")]),
    ("selective_color", "Selective Color: adjust cyan/magenta/yellow/black inside one colour family (reds, yellows, greens, cyans, blues, magentas, whites, neutrals, blacks)", &[
        ("reds_c", 0.0, "-1..1"), ("reds_m", 0.0, ""), ("reds_y", 0.0, ""), ("reds_k", 0.0, ""), ("yellows_c", 0.0, ""), ("yellows_m", 0.0, ""), ("yellows_y", 0.0, ""), ("yellows_k", 0.0, ""),
        ("greens_c", 0.0, ""), ("greens_m", 0.0, ""), ("greens_y", 0.0, ""), ("greens_k", 0.0, ""), ("cyans_c", 0.0, ""), ("cyans_m", 0.0, ""), ("cyans_y", 0.0, ""), ("cyans_k", 0.0, ""),
        ("blues_c", 0.0, ""), ("blues_m", 0.0, ""), ("blues_y", 0.0, ""), ("blues_k", 0.0, ""), ("magentas_c", 0.0, ""), ("magentas_m", 0.0, ""), ("magentas_y", 0.0, ""), ("magentas_k", 0.0, ""),
        ("whites_c", 0.0, ""), ("whites_m", 0.0, ""), ("whites_y", 0.0, ""), ("whites_k", 0.0, ""), ("neutrals_c", 0.0, ""), ("neutrals_m", 0.0, ""), ("neutrals_y", 0.0, ""), ("neutrals_k", 0.0, ""),
        ("blacks_c", 0.0, ""), ("blacks_m", 0.0, ""), ("blacks_y", 0.0, ""), ("blacks_k", 0.0, ""), ("absolute", 0.0, "0 relative, 1 absolute")]),
    ("photo_filter", "Photo Filter: a coloured lens filter (warming 85 by default; set r,g,b for cooling 80, sepia, ...)", &[("r", 0.925, ""), ("g", 0.541, ""), ("b", 0.0, ""), ("density", 0.25, "0..1"), ("preserve_luminosity", 1.0, "0/1")]),
    ("colorama", "Colorama: map brightness (or hue) through a cycling colour palette; animate phase for colour cycling", &[("input", 0.0, "0 luminance, 1 hue, 2 saturation, 3 red, 4 green, 5 blue, 6 alpha"), ("palette", 0.0, "0 hue cycle, 1 fire, 2 ice, 3 golden, 4 neon, 5 grayscale"), ("cycles", 1.0, "palette repeats"), ("phase", 0.0, "0..1 shift (animate for cycling)"), ("speed", 0.0, "phase per second"), ("blend", 0.0, "0 full effect .. 1 original")]),
    ("change_color", "Change Color: rotate hue / shift lightness and saturation only where the picture is near one colour", &[("r", 1.0, "colour to change"), ("g", 0.0, ""), ("b", 0.0, ""), ("hue", 120.0, "degrees"), ("lightness", 0.0, "-1..1"), ("saturation", 0.0, "-1..1"), ("tolerance", 0.08, "0..1 hue distance"), ("softness", 0.05, "0..1"), ("match", 1.0, "0 RGB, 1 hue, 2 chroma")]),
    ("change_to_color", "Change to Color: recolour everything near one colour to another, keeping shading", &[("from_r", 1.0, ""), ("from_g", 0.0, ""), ("from_b", 0.0, ""), ("to_r", 0.0, ""), ("to_g", 0.4, ""), ("to_b", 1.0, ""), ("tolerance", 0.08, "0..1 hue distance"), ("softness", 0.05, "0..1"), ("change", 0.0, "0 hue, 1 hue+lightness, 2 hue+saturation, 3 hue+lightness+saturation")]),
    ("leave_color", "Leave Color: everything grey except one colour (red coat, colour splash)", &[("r", 1.0, "colour to keep"), ("g", 0.0, ""), ("b", 0.0, ""), ("amount", 1.0, "0..1 how grey the rest goes"), ("tolerance", 0.08, "0..1 hue distance"), ("softness", 0.06, "0..1")]),
    ("equalize", "Equalize: spread brightness evenly (histogram equalization) for flat, foggy footage", &[("amount", 1.0, "0..1"), ("per_channel", 0.0, "0 by luminance (keeps colour), 1 each channel")]),
    ("auto_color", "Auto Color: per-channel black and white points from the frame, removing casts and stretching contrast", &[("clip", 0.001, "fraction clipped at each end"), ("amount", 1.0, "0..1"), ("snap_neutral", 1.0, "0/1 also neutralise midtones")]),
    ("broadcast_colors", "Broadcast Colors / Video Limiter: keep colours inside legal broadcast range (16-235 luma, composite amplitude)", &[("max_ire", 110.0, "composite limit, 90..120 IRE"), ("method", 0.0, "0 reduce saturation, 1 reduce luminance, 2 clip to 16-235"), ("min_luma", 0.0627, "legal floor"), ("max_luma", 0.9216, "legal ceiling")]),
    ("black_and_white", "Black & White: monochrome with a slider per colour family (how bright reds, skies, foliage turn), optional tint", &[("reds", 0.4, "-2..3"), ("yellows", 0.6, ""), ("greens", 0.4, ""), ("cyans", 0.6, ""), ("blues", 0.2, ""), ("magentas", 0.8, ""), ("tint", 0.0, "0..1"), ("tint_r", 0.86, ""), ("tint_g", 0.74, ""), ("tint_b", 0.55, "")]),
    ("tritone", "Tritone: map shadows, midtones and highlights to three colours", &[("high_r", 1.0, ""), ("high_g", 1.0, ""), ("high_b", 1.0, ""), ("mid_r", 0.5, ""), ("mid_g", 0.4, ""), ("mid_b", 0.3, ""), ("shadow_r", 0.0, ""), ("shadow_g", 0.0, ""), ("shadow_b", 0.0, ""), ("blend", 0.0, "0 full .. 1 original")]),
    ("vibrance", "Vibrance: saturate dull colours more than strong ones, protecting skin", &[("vibrance", 0.3, "-1..1"), ("saturation", 0.0, "-1..1")]),
    ("gamma_pedestal_gain", "Gamma/Pedestal/Gain per channel (black stretch, lift, gamma, gain)", &[("black_stretch", 0.0, "0..1"), ("r_gamma", 1.0, ""), ("r_pedestal", 0.0, ""), ("r_gain", 1.0, ""), ("g_gamma", 1.0, ""), ("g_pedestal", 0.0, ""), ("g_gain", 1.0, ""), ("b_gamma", 1.0, ""), ("b_pedestal", 0.0, ""), ("b_gain", 1.0, "")]),
    // ---- channel
    ("arithmetic", "Arithmetic: per-channel maths with constant values (and, or, xor, add, subtract, difference, min, max, block above/below, slice, multiply, screen)", &[("operator", 3.0, "0 and,1 or,2 xor,3 add,4 subtract,5 difference,6 min,7 max,8 block above,9 block below,10 slice,11 multiply,12 screen"), ("r", 0.0, "0..1"), ("g", 0.0, ""), ("b", 0.0, ""), ("clip", 1.0, "0/1 clip to 0..1")]),
    ("minimax", "Minimax: grow (maximum) or shrink (minimum) bright areas or the alpha; chokes mattes, thickens lines", &[("operation", 0.0, "0 maximum, 1 minimum, 2 min then max, 3 max then min"), ("radius", 2.0, "px"), ("channel", 0.0, "0 colour, 1 alpha, 2 colour+alpha"), ("direction", 0.0, "0 both, 1 horizontal, 2 vertical")]),
    ("channel_combiner", "Channel Combiner: convert between RGB, HLS and YUV, or show one channel as grey", &[("mode", 0.0, "0 RGB->HLS, 1 HLS->RGB, 2 RGB->YUV, 3 YUV->RGB, 4 lightness, 5 hue, 6 saturation, 7 min, 8 max, 9 luminance")]),
    // ---- noise and generate
    ("noise_hls", "Noise HLS: grain in hue, lightness and saturation separately; size in px, animated", &[("hue", 0.0, "0..1"), ("lightness", 0.1, "0..1"), ("saturation", 0.0, "0..1"), ("grain_size", 1.0, "px"), ("type", 0.0, "0 uniform, 1 squared (sparser), 2 grain (soft)"), ("animate", 1.0, "0 still, 1 new each frame"), ("seed", 0.0, "")]),
    ("noise_alpha", "Noise Alpha: noise in the alpha channel (dissolving, breaking up a layer)", &[("amount", 0.5, "0..1"), ("size", 4.0, "px"), ("type", 0.0, "0 uniform, 1 squared, 2 smooth"), ("animate", 1.0, "0/1"), ("seed", 0.0, ""), ("overflow", 0.0, "0 multiply alpha, 1 set alpha")]),
    ("circle", "Circle: draw a filled circle or ring (with feather) over or into the layer", &[("cx", 0.5, "0..1"), ("cy", 0.5, "0..1"), ("radius", 150.0, "px"), ("thickness", 0.0, "px ring width (0 = filled)"), ("feather", 2.0, "px"), ("r", 1.0, ""), ("g", 1.0, ""), ("b", 1.0, ""), ("opacity", 1.0, "0..1"), ("invert", 0.0, "0/1"), ("blend", 0.0, "0 over, 1 stencil (layer shows inside), 2 silhouette (layer shows outside)")]),
    ("ellipse", "Ellipse: a soft ring with an inside and outside colour (as After Effects' Ellipse generator)", &[("cx", 0.5, "0..1"), ("cy", 0.5, "0..1"), ("width", 400.0, "px"), ("height", 250.0, "px"), ("thickness", 20.0, "px"), ("softness", 0.3, "0..1"), ("in_r", 1.0, "inside colour"), ("in_g", 1.0, ""), ("in_b", 1.0, ""), ("out_r", 0.2, "outside colour"), ("out_g", 0.6, ""), ("out_b", 1.0, ""), ("on_original", 1.0, "0 replace the layer, 1 draw over it")]),
];

fn p(m: &Params, k: &str, d: f64, t: f64) -> f32 { m.get(k).map(|x| x.at(t)).unwrap_or(d) as f32 }
fn pxs(v: f32) -> f32 { crate::quality::px(v) }
fn sstep(e0: f32, e1: f32, x: f32) -> f32 { if e1 <= e0 { return if x >= e1 { 1.0 } else { 0.0 }; } let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }
fn luma(c: &[f32]) -> f32 { 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2] }
fn hash(x: i64, y: i64, s: u64) -> f32 { let mut h = (x as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ (y as u64).wrapping_mul(0xC2B2AE3D27D4EB4F) ^ s.wrapping_mul(0x165667B19E3779F9); h ^= h >> 29; h = h.wrapping_mul(0xBF58476D1CE4E5B9); h ^= h >> 32; (h & 0xffffff) as f32 / 16777215.0 }

/// Premultiplied bilinear sample at continuous pixel coords (centres at +0.5); transparent outside. Returns straight RGBA.
fn bil(src: &[f32], w: usize, h: usize, x: f32, y: f32) -> [f32; 4] {
    let (fx, fy) = (x - 0.5, y - 0.5); let (x0, y0) = (fx.floor(), fy.floor()); let (ax, ay) = (fx - x0, fy - y0); let (x0, y0) = (x0 as i64, y0 as i64);
    let mut acc = [0f32; 4];
    for (dx, dy, k) in [(0, 0, (1.0 - ax) * (1.0 - ay)), (1, 0, ax * (1.0 - ay)), (0, 1, (1.0 - ax) * ay), (1, 1, ax * ay)] {
        let (xx, yy) = (x0 + dx, y0 + dy); if xx < 0 || yy < 0 || xx >= w as i64 || yy >= h as i64 || k == 0.0 { continue; }
        let i = (yy as usize * w + xx as usize) * 4; let a = src[i + 3] * k; for j in 0..3 { acc[j] += src[i + j] * a; } acc[3] += a; }
    if acc[3] > 1e-6 { [acc[0] / acc[3], acc[1] / acc[3], acc[2] / acc[3], acc[3]] } else { [0.0; 4] }
}
/// Rebuild the frame by sampling the original at `map(x, y)` (pixel-centre coords) for every output pixel; None = transparent.
fn remap(f: &mut Frame, map: impl Fn(f32, f32) -> Option<(f32, f32)>) {
    let src = f.d.clone(); let (w, h) = (f.w, f.h);
    for y in 0..h { for x in 0..w { let o = (y * w + x) * 4; let c = match map(x as f32 + 0.5, y as f32 + 0.5) { Some((sx, sy)) => bil(&src, w, h, sx, sy), None => [0.0; 4] }; f.d[o..o + 4].copy_from_slice(&c); } }
}
fn each(f: &mut Frame, g: impl Fn([f32; 4], usize, usize) -> [f32; 4]) { let w = f.w; for (i, q) in f.d.chunks_exact_mut(4).enumerate() { let r = g([q[0], q[1], q[2], q[3]], i % w, i / w); q.copy_from_slice(&r); } }

fn rgb2hsl(c: [f32; 3]) -> [f32; 3] {
    let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2])); let l = (mx + mn) / 2.0; let d = mx - mn;
    if d < 1e-6 { return [0.0, 0.0, l]; }
    let s = if l > 0.5 { d / (2.0 - mx - mn).max(1e-6) } else { d / (mx + mn).max(1e-6) };
    let h = if mx == c[0] { ((c[1] - c[2]) / d).rem_euclid(6.0) } else if mx == c[1] { (c[2] - c[0]) / d + 2.0 } else { (c[0] - c[1]) / d + 4.0 };
    [h / 6.0, s.clamp(0.0, 1.0), l]
}
fn hsl2rgb(h: f32, s: f32, l: f32) -> [f32; 3] {
    if s <= 0.0 { return [l; 3]; } let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s }; let pp = 2.0 * l - q;
    let f = |t: f32| { let t = t.rem_euclid(1.0); if t < 1.0 / 6.0 { pp + (q - pp) * 6.0 * t } else if t < 0.5 { q } else if t < 2.0 / 3.0 { pp + (q - pp) * (2.0 / 3.0 - t) * 6.0 } else { pp } };
    [f(h + 1.0 / 3.0), f(h), f(h - 1.0 / 3.0)]
}
fn hue_dist(a: f32, b: f32) -> f32 { let d = (a - b).rem_euclid(1.0); d.min(1.0 - d) }
/// 0..1 how strongly `c` matches `key` (1 inside tolerance, ramping to 0 over softness). match: 0 RGB, 1 hue, 2 chroma.
fn color_match(c: [f32; 3], key: [f32; 3], tol: f32, soft: f32, mode: f32) -> f32 {
    let d = match mode as i32 { 1 => { let (a, b) = (rgb2hsl(c), rgb2hsl(key)); let sat = (a[1] * (1.0 - (2.0 * a[2] - 1.0).abs())).min(1.0); hue_dist(a[0], b[0]) + (1.0 - sat.min(0.25) * 4.0) * 0.5 }
        2 => { let uv = |c: [f32; 3]| { let y = luma(&c); [c[2] - y, c[0] - y] }; let (a, b) = (uv(c), uv(key)); ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt() }
        _ => ((c[0] - key[0]).powi(2) + (c[1] - key[1]).powi(2) + (c[2] - key[2]).powi(2)).sqrt() / 3f32.sqrt() };
    1.0 - sstep(tol, tol + soft.max(1e-4), d)
}
fn yuv(c: [f32; 3]) -> [f32; 3] { let y = luma(&c); [y, (c[2] - y) / 1.8556, (c[0] - y) / 1.5748] }
fn unyuv(v: [f32; 3]) -> [f32; 3] { let (y, u, w) = (v[0], v[1] * 1.8556, v[2] * 1.5748); let r = w + y; let b = u + y; [r, (y - 0.2126 * r - 0.0722 * b) / 0.7152, b] }

/// Separable running max/min over `r` px of channel `ch` (0..4).
fn minmax(d: &mut [f32], w: usize, h: usize, ch: &[usize], r: usize, max: bool, hor: bool, ver: bool) {
    let pick = |a: f32, b: f32| if max { a.max(b) } else { a.min(b) };
    for &c in ch {
        if hor { let src = d.to_vec(); for y in 0..h { for x in 0..w { let mut v = src[(y * w + x) * 4 + c]; for k in x.saturating_sub(r)..=(x + r).min(w - 1) { v = pick(v, src[(y * w + k) * 4 + c]); } d[(y * w + x) * 4 + c] = v; } } }
        if ver { let src = d.to_vec(); for y in 0..h { for x in 0..w { let mut v = src[(y * w + x) * 4 + c]; for k in y.saturating_sub(r)..=(y + r).min(h - 1) { v = pick(v, src[(k * w + x) * 4 + c]); } d[(y * w + x) * 4 + c] = v; } } }
    }
}

fn palette(n: i32, u: f32) -> [f32; 3] {
    let u = u.rem_euclid(1.0); let ramp = |stops: &[[f32; 3]]| { let x = u * (stops.len() - 1) as f32; let i = (x.floor() as usize).min(stops.len() - 2); let t = x - i as f32; [0, 1, 2].map(|j| stops[i][j] + (stops[i + 1][j] - stops[i][j]) * t) };
    match n { 1 => ramp(&[[0.0, 0.0, 0.0], [0.6, 0.05, 0.0], [1.0, 0.45, 0.0], [1.0, 0.9, 0.3], [1.0, 1.0, 1.0]]), 2 => ramp(&[[0.0, 0.02, 0.1], [0.0, 0.3, 0.6], [0.4, 0.8, 1.0], [1.0, 1.0, 1.0]]),
        3 => ramp(&[[0.1, 0.05, 0.0], [0.55, 0.35, 0.05], [1.0, 0.82, 0.35], [1.0, 0.97, 0.85]]), 4 => ramp(&[[1.0, 0.0, 0.6], [0.3, 0.0, 1.0], [0.0, 0.9, 1.0], [0.2, 1.0, 0.3], [1.0, 0.0, 0.6]]),
        5 => [u; 3], _ => hsl2rgb(u, 1.0, 0.5) }
}

/// Some(result) when `kind` is one of these effects.
pub fn apply(kind: &str, m: &Params, strings: &std::collections::BTreeMap<String, String>, f: &mut Frame, t: f64, seed: u64) -> Option<Result<(), String>> {
    if !CATALOG4.iter().any(|c| c.0 == kind) { return None; }
    let (w, h) = (f.w, f.h); if w == 0 || h == 0 { return Some(Ok(())); }
    let (wf, hf) = (w as f32, h as f32); let q = |k: &str, d: f64| p(m, k, d, t); let short = wf.min(hf);
    match kind {
        "wave_warp" => { let (ht, wl) = (pxs(q("height", 10.0)), pxs(q("width", 40.0)).max(1.0)); let dir = q("direction", 90.0).to_radians(); let (dx, dy) = (dir.sin(), -dir.cos()); let (nx, ny) = (dir.cos(), dir.sin());
            let ph = q("phase", 0.0) / 360.0 - q("speed", 1.0) * t as f32; let ty = q("wave_type", 0.0) as i32; let pin = q("pinning", 0.0) as i32;
            let wave = move |u: f32| { let u = u.rem_euclid(1.0); match ty { 1 => if u < 0.5 { 1.0 } else { -1.0 }, 2 => 1.0 - 4.0 * (u - 0.5).abs(), 3 => 2.0 * u - 1.0, 4 => { let a = (u * 6.2831).sin(); a * a.abs().sqrt() } _ => (u * std::f32::consts::TAU).sin() } };
            remap(f, |x, y| { let mut a = ht * wave((x * dx + y * dy) / wl + ph);
                let e = |d: f32| (d / (short * 0.15)).clamp(0.0, 1.0); a *= match pin { 1 => e(x.min(wf - x)).min(e(y.min(hf - y))), 2 => e(x.min(wf - x)), 3 => e(y.min(hf - y)), _ => 1.0 };
                Some((x - nx * a, y - ny * a)) }); }
        "spherize" => { let (a, r) = (q("amount", 1.0).clamp(-1.0, 1.0), q("radius", 0.4) * short); let (cx, cy) = (q("cx", 0.5) * wf, q("cy", 0.5) * hf);
            remap(f, |x, y| { let (vx, vy) = (x - cx, y - cy); let d = (vx * vx + vy * vy).sqrt(); if d >= r || d < 1e-6 || r <= 0.0 { return Some((x, y)); } let rho = d / r;
                let rs = if a >= 0.0 { rho + (rho.asin() * 2.0 / std::f32::consts::PI - rho) * a } else { rho + ((rho * std::f32::consts::FRAC_PI_2).sin() - rho) * -a }; let k = rs / rho; Some((cx + vx * k, cy + vy * k)) }); }
        "magnify" => { let (cx, cy, r) = (q("cx", 0.5) * wf, q("cy", 0.5) * hf, pxs(q("radius", 150.0)).max(1.0)); let mg = q("magnification", 2.0).clamp(1.0, 20.0); let fe = pxs(q("feather", 4.0)).max(0.5); let sq = q("shape", 0.0) >= 0.5;
            let bw = pxs(q("border", 0.0)); let bc = [q("border_r", 1.0), q("border_g", 1.0), q("border_b", 1.0)]; let src = f.d.clone();
            each(f, |c, x, y| { let (vx, vy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy); let d = if sq { vx.abs().max(vy.abs()) } else { (vx * vx + vy * vy).sqrt() };
                let k = 1.0 - sstep(r - fe, r, d); let mut o = c;
                if k > 0.0 { let s = bil(&src, w, h, cx + vx / mg, cy + vy / mg); o = [0, 1, 2, 3].map(|j| c[j] + (s[j] - c[j]) * k); }
                if bw > 0.0 { let b = sstep(r - 0.5, r + 0.5, d + bw) * (1.0 - sstep(r, r + 1.0, d)); if b > 0.0 { for j in 0..3 { o[j] += (bc[j] - o[j]) * b; } o[3] = o[3].max(b); } } o }); }
        "optics_compensation" => { let fov = q("fov", 60.0).clamp(0.0, 170.0).to_radians(); if fov < 1e-3 { return Some(Ok(())); } let rev = q("reverse", 0.0) >= 0.5; let (cx, cy) = (q("cx", 0.5) * wf, q("cy", 0.5) * hf); let rn = (wf * wf + hf * hf).sqrt() / 2.0; let tf = (fov / 2.0).tan();
            remap(f, |x, y| { let (vx, vy) = (x - cx, y - cy); let d = (vx * vx + vy * vy).sqrt() / rn; if d < 1e-6 { return Some((x, y)); }
                let s = if rev { (d * tf).atan() / (fov / 2.0) } else { (d * fov / 2.0).tan() / tf }; let k = s / d; Some((cx + vx * k, cy + vy * k)) }); }
        "mesh_warp" => { let (rows, cols) = (q("rows", 3.0).round().clamp(1.0, 32.0) as usize, q("cols", 3.0).round().clamp(1.0, 32.0) as usize); let n = (rows + 1) * (cols + 1);
            let off: Vec<[f32; 2]> = match strings.get("mesh") { None => return Some(Ok(())), Some(s) => match serde_json::from_str::<Vec<[f32; 2]>>(s) { Ok(v) if v.len() == n => v.iter().map(|o| [pxs(o[0]), pxs(o[1])]).collect(), Ok(v) => return Some(Err(format!("mesh_warp: strings.mesh has {} points, rows {rows} x cols {cols} needs {n}", v.len()))), Err(e) => return Some(Err(format!("mesh_warp: strings.mesh must be JSON [[dx,dy],...]: {e}"))) } };
            let src = f.d.clone(); for v in f.d.iter_mut() { *v = 0.0; }
            let gp = |i: usize, j: usize| [j as f32 / cols as f32 * wf, i as f32 / rows as f32 * hf];
            for i in 0..rows { for j in 0..cols { for tri in [[(i, j), (i, j + 1), (i + 1, j)], [(i, j + 1), (i + 1, j + 1), (i + 1, j)]] {
                let s: Vec<[f32; 2]> = tri.iter().map(|&(a, b)| gp(a, b)).collect(); let dpt: Vec<[f32; 2]> = tri.iter().map(|&(a, b)| { let g = gp(a, b); let o = off[a * (cols + 1) + b]; [g[0] + o[0], g[1] + o[1]] }).collect();
                let den = (dpt[1][1] - dpt[2][1]) * (dpt[0][0] - dpt[2][0]) + (dpt[2][0] - dpt[1][0]) * (dpt[0][1] - dpt[2][1]); if den.abs() < 1e-6 { continue; }
                let (x0, x1) = (dpt.iter().map(|p| p[0]).fold(f32::MAX, f32::min).floor().max(0.0) as usize, (dpt.iter().map(|p| p[0]).fold(f32::MIN, f32::max).ceil() as usize).min(w));
                let (y0, y1) = (dpt.iter().map(|p| p[1]).fold(f32::MAX, f32::min).floor().max(0.0) as usize, (dpt.iter().map(|p| p[1]).fold(f32::MIN, f32::max).ceil() as usize).min(h));
                for y in y0..y1 { for x in x0..x1 { let (px_, py_) = (x as f32 + 0.5, y as f32 + 0.5);
                    let l0 = ((dpt[1][1] - dpt[2][1]) * (px_ - dpt[2][0]) + (dpt[2][0] - dpt[1][0]) * (py_ - dpt[2][1])) / den; let l1 = ((dpt[2][1] - dpt[0][1]) * (px_ - dpt[2][0]) + (dpt[0][0] - dpt[2][0]) * (py_ - dpt[2][1])) / den; let l2 = 1.0 - l0 - l1;
                    if l0 < -1e-4 || l1 < -1e-4 || l2 < -1e-4 { continue; }
                    let c = bil(&src, w, h, l0 * s[0][0] + l1 * s[1][0] + l2 * s[2][0], l0 * s[0][1] + l1 * s[1][1] + l2 * s[2][1]); let o = (y * w + x) * 4; f.d[o..o + 4].copy_from_slice(&c); } }
            } } } }
        "transform" => { let (ax, ay, px_, py_) = (q("anchor_x", 0.5) * wf, q("anchor_y", 0.5) * hf, q("x", 0.5) * wf, q("y", 0.5) * hf); let (s, sw, sh) = (q("scale", 1.0), q("scale_w", 1.0), q("scale_h", 1.0));
            let (sx, sy) = (s * sw, s * sh); if sx.abs() < 1e-6 || sy.abs() < 1e-6 { for v in f.d.iter_mut() { *v = 0.0; } return Some(Ok(())); }
            let (rot, sk, ska) = (q("rotation", 0.0).to_radians(), q("skew", 0.0).to_radians().tan(), q("skew_axis", 0.0).to_radians()); let op = q("opacity", 1.0).clamp(0.0, 1.0);
            // forward: p' = P + R * Ska * Sk * Ska^-1 * S * (p - A); invert per pixel
            let (cr, sr) = (rot.cos(), rot.sin()); let (ca, sa) = (ska.cos(), ska.sin());
            let fwd = |v: [f32; 2]| { let v = [v[0] * sx, v[1] * sy]; let v = [ca * v[0] + sa * v[1], -sa * v[0] + ca * v[1]]; let v = [v[0] + sk * v[1], v[1]]; let v = [ca * v[0] - sa * v[1], sa * v[0] + ca * v[1]]; [cr * v[0] - sr * v[1], sr * v[0] + cr * v[1]] };
            let (e0, e1) = (fwd([1.0, 0.0]), fwd([0.0, 1.0])); let det = e0[0] * e1[1] - e1[0] * e0[1]; if det.abs() < 1e-9 { return Some(Ok(())); }
            remap(f, |x, y| { let (vx, vy) = (x - px_, y - py_); Some((ax + (e1[1] * vx - e1[0] * vy) / det, ay + (-e0[1] * vx + e0[0] * vy) / det)) });
            if op < 1.0 { for q4 in f.d.chunks_exact_mut(4) { q4[3] *= op; } } }
        "cylinder_wrap" | "sphere_wrap" => { let sphere = kind == "sphere_wrap"; let r = if sphere { q("radius", 0.3) * short } else { q("radius", 0.35) * wf }.max(1.0); let (cx, cy) = (q("cx", 0.5) * wf, q("cy", 0.5) * hf);
            let rot = q("rotation", 0.0).to_radians(); let tilt = if sphere { q("tilt", 15.0).to_radians() } else { q("tilt", 0.0).clamp(-1.0, 1.0) }; let li = q("light", 0.6).clamp(0.0, 1.0); let la = q("light_angle", 135.0).to_radians(); let src = f.d.clone();
            let ld = { let v = [-la.cos() * 0.6, -la.sin() * 0.6, -0.8f32]; let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt(); [v[0] / n, v[1] / n, v[2] / n] };
            each(f, |_, x, y| { let (vx, vy) = ((x as f32 + 0.5 - cx) / r, (y as f32 + 0.5 - cy) / r);
                if sphere { let d2 = vx * vx + vy * vy; if d2 >= 1.0 { return [0.0; 4]; } let z = -(1.0 - d2).sqrt(); // point on the front of the unit sphere (camera at -z)
                    let (ct, st) = (tilt.cos(), tilt.sin()); let (y2, z2) = (vy * ct - z * st, vy * st + z * ct); let lon = vx.atan2(-z2) + rot; let lat = y2.clamp(-1.0, 1.0).asin();
                    let u = (lon / std::f32::consts::TAU + 0.5).rem_euclid(1.0); let v = lat / std::f32::consts::PI + 0.5; let mut c = bil(&src, w, h, u * wf, v * hf);
                    let sh = (-(vx * ld[0] + vy * ld[1] + z * ld[2])).max(0.0); let k = 1.0 - li + li * (0.25 + 0.75 * sh); for j in 0..3 { c[j] *= k; } let edge = 1.0 - sstep(1.0 - 2.0 / r, 1.0, d2.sqrt()); c[3] *= edge; c }
                else { if vx.abs() >= 1.0 { return [0.0; 4]; } let phi = vx.asin(); let ang = phi + rot; let wrap = ang.rem_euclid(std::f32::consts::TAU); if wrap > std::f32::consts::PI { return [0.0; 4]; } // the back half shows nothing
                    let u = wrap / std::f32::consts::PI; let v = (y as f32 + 0.5 - cy + tilt * phi.cos() * r * 0.25) / hf + 0.5; let mut c = bil(&src, w, h, (1.0 - u) * wf, v * hf); let k = 1.0 - li + li * phi.cos().powf(0.7); for j in 0..3 { c[j] *= k; } c } }); }
        "bevel_alpha" => { let th = pxs(q("thickness", 4.0)).max(0.5); let la = q("light_angle", 135.0).to_radians(); let it = q("intensity", 0.4).clamp(0.0, 1.0); let lc = [q("light_r", 1.0), q("light_g", 1.0), q("light_b", 1.0)];
            let mut a = Frame { w, h, d: f.d.iter().enumerate().map(|(i, v)| if i % 4 == 3 { *v } else { 0.0 }).collect() }; for i in 0..w * h { a.d[i * 4] = a.d[i * 4 + 3]; a.d[i * 4 + 3] = 1.0; } crate::effects::gauss_blur(&mut a, th * 0.6, false);
            let (lx, ly) = (-la.cos(), -la.sin()); let al = |x: usize, y: usize| a.d[(y.min(h - 1) * w + x.min(w - 1)) * 4];
            each(f, |c, x, y| { if c[3] <= 0.0 { return c; } let gx = al(x + 1, y) - al(x.saturating_sub(1), y); let gy = al(x, y + 1) - al(x, y.saturating_sub(1)); let edge = 1.0 - al(x, y); let s = (gx * lx + gy * ly) * th * 1.5 * it;
                let s = s.clamp(-1.0, 1.0) * (edge * 2.0).min(1.0); let mut o = c; if s > 0.0 { for j in 0..3 { o[j] += (lc[j] - o[j]) * s; } } else { for j in 0..3 { o[j] *= 1.0 + s; } } o }); }
        // ---- keying
        "linear_color_key" => { let key = [q("r", 0.0), q("g", 1.0), q("b", 0.0)]; let (tol, soft, mode, inv) = (q("tolerance", 0.15), q("softness", 0.1), q("match", 0.0), q("invert", 0.0) >= 0.5);
            each(f, |c, _, _| { let mt = color_match([c[0], c[1], c[2]], key, tol, soft, mode); let k = if inv { mt } else { 1.0 - mt }; [c[0], c[1], c[2], c[3] * k] }); }
        "color_range" => { let lo = [q("y_min", 0.0), q("u_min", -0.5), q("v_min", -0.5)]; let hi = [q("y_max", 1.0), q("u_max", -0.1), q("v_max", -0.1)]; let s = q("softness", 0.03).max(1e-4); let inv = q("invert", 0.0) >= 0.5;
            each(f, |c, _, _| { let v = yuv([c[0], c[1], c[2]]); let inside = (0..3).map(|j| sstep(lo[j] - s, lo[j], v[j]) * (1.0 - sstep(hi[j], hi[j] + s, v[j]))).product::<f32>(); let k = if inv { inside } else { 1.0 - inside }; [c[0], c[1], c[2], c[3] * k] }); }
        "extract" => { let ch = q("channel", 0.0) as i32; let (bp, wp, bs, ws) = (q("black_point", 0.0), q("white_point", 1.0), q("black_softness", 0.0).max(1e-4), q("white_softness", 0.0).max(1e-4)); let inv = q("invert", 0.0) >= 0.5;
            each(f, |c, _, _| { let v = match ch { 1 => c[0], 2 => c[1], 3 => c[2], 4 => c[3], _ => luma(&c) }; let k = sstep(bp - bs, bp, v) * (1.0 - sstep(wp, wp + ws, v)); let k = if bp <= 0.0 && v <= 0.0 { 1.0 * (1.0 - sstep(wp, wp + ws, v)) } else { k }; let k = if inv { 1.0 - k } else { k }; [c[0], c[1], c[2], c[3] * k] }); }
        "unmult" => { let am = q("amount", 1.0).clamp(0.0, 1.0); each(f, |c, _, _| { let mx = c[0].max(c[1]).max(c[2]).clamp(0.0, 1.0); if mx <= 1e-6 { return [0.0, 0.0, 0.0, c[3] * (1.0 - am)]; } let a = 1.0 - am + am * mx; [c[0] / a, c[1] / a, c[2] / a, c[3] * a] }); }
        "key_cleaner" => { let r = pxs(q("radius", 2.0)).round().max(1.0) as usize; let all = [3usize];
            if q("fill_holes", 1.0) >= 0.5 { minmax(&mut f.d, w, h, &all, r, true, true, true); minmax(&mut f.d, w, h, &all, r, false, true, true); }
            if q("remove_specks", 1.0) >= 0.5 { minmax(&mut f.d, w, h, &all, r, false, true, true); minmax(&mut f.d, w, h, &all, r, true, true, true); }
            let es = q("edge_smooth", 0.5).clamp(0.0, 1.0); if es > 0.0 { let mut a = Frame { w, h, d: vec![0.0; w * h * 4] }; for i in 0..w * h { a.d[i * 4] = f.d[i * 4 + 3]; a.d[i * 4 + 3] = 1.0; } crate::effects::gauss_blur(&mut a, r as f32 * es, false);
                for i in 0..w * h { let b = a.d[i * 4]; f.d[i * 4 + 3] = sstep(0.5 - 0.5 * (1.0 - es * 0.5), 0.5 + 0.5 * (1.0 - es * 0.5), b); } } }
        // ---- colour
        "color_balance" => { let sh = [q("shadow_r", 0.0), q("shadow_g", 0.0), q("shadow_b", 0.0)]; let md = [q("mid_r", 0.0), q("mid_g", 0.0), q("mid_b", 0.0)]; let hi = [q("high_r", 0.0), q("high_g", 0.0), q("high_b", 0.0)]; let keep = q("preserve_luminosity", 1.0) >= 0.5;
            each(f, |c, _, _| { let l = luma(&c).clamp(0.0, 1.0); let (ws, wh) = ((1.0 - l * 2.0).max(0.0).powi(2), (l * 2.0 - 1.0).max(0.0).powi(2)); let wm = 1.0 - ws - wh;
                let mut o = [0, 1, 2].map(|j| c[j] + 0.25 * (sh[j] * ws + md[j] * wm + hi[j] * wh)); if keep { let d = l - luma(&o); for v in o.iter_mut() { *v += d; } } [o[0], o[1], o[2], c[3]] }); }
        "color_balance_hls" => { let (dh, dl, ds) = (q("hue", 0.0) / 360.0, q("lightness", 0.0), q("saturation", 0.0)); each(f, |c, _, _| { let x = rgb2hsl([c[0], c[1], c[2]]); let l = if dl >= 0.0 { x[2] + (1.0 - x[2]) * dl } else { x[2] * (1.0 + dl) }; let s = if ds >= 0.0 { x[1] + (1.0 - x[1]) * ds } else { x[1] * (1.0 + ds) }; let o = hsl2rgb(x[0] + dh, s.clamp(0.0, 1.0), l); [o[0], o[1], o[2], c[3]] }); }
        "selective_color" => { const FAM: [&str; 9] = ["reds", "yellows", "greens", "cyans", "blues", "magentas", "whites", "neutrals", "blacks"]; let adj: Vec<[f32; 4]> = FAM.iter().map(|n| ["c", "m", "y", "k"].map(|k| q(&format!("{n}_{k}"), 0.0))).collect(); let abs = q("absolute", 0.0) >= 0.5;
            each(f, |c, _, _| { let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2])); let mut sorted = [c[0], c[1], c[2]]; sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); let mid = sorted[1];
                // weight of each family: primaries by (max - mid), secondaries by (mid - min), whites/neutrals/blacks by lightness
                let mut wts = [0f32; 9]; let im = if c[0] == mx { 0 } else if c[1] == mx { 2 } else { 4 }; let inn = if c[0] == mn { 3 } else if c[1] == mn { 5 } else { 1 }; wts[im] = mx - mid; wts[inn] = mid - mn;
                let l = (mx + mn) / 2.0; wts[6] = ((mn - 0.5) * 2.0).max(0.0); wts[8] = ((0.5 - mx) * 2.0).max(0.0); wts[7] = (1.0 - (mx - mn)) * (1.0 - (l - 0.5).abs() * 2.0).max(0.0);
                let mut o = [c[0], c[1], c[2]];
                for (fi, a) in adj.iter().enumerate() { if wts[fi] <= 0.0 || a.iter().all(|v| *v == 0.0) { continue; } for (j, cv) in [0usize, 1, 2].iter().zip([a[0], a[1], a[2]]) { let ink = 1.0 - o[*j]; let delta = (cv + a[3]) * if abs { 1.0 } else { ink.max(0.0) + 0.0 } ; o[*j] -= delta * wts[fi] * if abs { 1.0 } else { o[*j].max(0.0).max(ink) }; } }
                [o[0].clamp(-0.5, 1.5), o[1].clamp(-0.5, 1.5), o[2].clamp(-0.5, 1.5), c[3]] }); }
        "photo_filter" => { let fc = [q("r", 0.925), q("g", 0.541), q("b", 0.0)]; let den = q("density", 0.25).clamp(0.0, 1.0); let keep = q("preserve_luminosity", 1.0) >= 0.5;
            each(f, |c, _, _| { let l = luma(&c); let mut o = [0, 1, 2].map(|j| c[j] + (c[j] * fc[j] - c[j]) * den); if keep { let lo = luma(&o); if lo > 1e-6 { let k = l / lo; o = o.map(|v| v * k); } } [o[0], o[1], o[2], c[3]] }); }
        "colorama" => { let (inp, pal, cyc, bl) = (q("input", 0.0) as i32, q("palette", 0.0) as i32, q("cycles", 1.0), q("blend", 0.0).clamp(0.0, 1.0)); let ph = q("phase", 0.0) + q("speed", 0.0) * t as f32;
            each(f, |c, _, _| { let x = rgb2hsl([c[0], c[1], c[2]]); let v = match inp { 1 => x[0], 2 => x[1], 3 => c[0], 4 => c[1], 5 => c[2], 6 => c[3], _ => luma(&c) }; let o = palette(pal, v.clamp(0.0, 1.0) * cyc + ph); [0, 1, 2].map(|j| o[j] + (c[j] - o[j]) * bl).into_iter().chain([c[3]]).collect::<Vec<_>>().try_into().unwrap() }); }
        "change_color" => { let key = [q("r", 1.0), q("g", 0.0), q("b", 0.0)]; let (dh, dl, ds, tol, soft, mode) = (q("hue", 120.0) / 360.0, q("lightness", 0.0), q("saturation", 0.0), q("tolerance", 0.08), q("softness", 0.05), q("match", 1.0));
            each(f, |c, _, _| { let k = color_match([c[0], c[1], c[2]], key, tol, soft, mode); if k <= 0.0 { return c; } let x = rgb2hsl([c[0], c[1], c[2]]); let o = hsl2rgb(x[0] + dh * k, (x[1] * (1.0 + ds * k)).clamp(0.0, 1.0), (x[2] + dl * k * 0.5).clamp(0.0, 1.0)); [o[0], o[1], o[2], c[3]] }); }
        "change_to_color" => { let (from, to) = ([q("from_r", 1.0), q("from_g", 0.0), q("from_b", 0.0)], [q("to_r", 0.0), q("to_g", 0.4), q("to_b", 1.0)]); let (tol, soft, ch) = (q("tolerance", 0.08), q("softness", 0.05), q("change", 0.0) as i32);
            let (hf_, ht) = (rgb2hsl(from), rgb2hsl(to));
            each(f, |c, _, _| { let k = color_match([c[0], c[1], c[2]], from, tol, soft, 1.0); if k <= 0.0 { return c; } let x = rgb2hsl([c[0], c[1], c[2]]); let dh = (ht[0] - hf_[0] + 0.5).rem_euclid(1.0) - 0.5;
                let s = if ch == 2 || ch == 3 { (x[1] + (ht[1] - hf_[1])).clamp(0.0, 1.0) } else { x[1] }; let l = if ch == 1 || ch == 3 { (x[2] + (ht[2] - hf_[2])).clamp(0.0, 1.0) } else { x[2] };
                let o = hsl2rgb(x[0] + dh, s, l); [0, 1, 2].map(|j| c[j] + (o[j] - c[j]) * k).into_iter().chain([c[3]]).collect::<Vec<_>>().try_into().unwrap() }); }
        "leave_color" => { let key = [q("r", 1.0), q("g", 0.0), q("b", 0.0)]; let (am, tol, soft) = (q("amount", 1.0).clamp(0.0, 1.0), q("tolerance", 0.08), q("softness", 0.06));
            each(f, |c, _, _| { let k = color_match([c[0], c[1], c[2]], key, tol, soft, 1.0); let g = am * (1.0 - k); let l = luma(&c); [c[0] + (l - c[0]) * g, c[1] + (l - c[1]) * g, c[2] + (l - c[2]) * g, c[3]] }); }
        "equalize" => { let am = q("amount", 1.0).clamp(0.0, 1.0); let per = q("per_channel", 0.0) >= 0.5; const N: usize = 1024;
            let cdf = |get: &dyn Fn(&[f32]) -> f32| { let mut hist = vec![0f64; N]; let mut tot = 0.0; for c in f.d.chunks_exact(4) { if c[3] <= 0.0 { continue; } hist[(get(c).clamp(0.0, 1.0) * (N - 1) as f32) as usize] += c[3] as f64; tot += c[3] as f64; } let mut acc = 0.0; hist.iter().map(|v| { acc += v; if tot > 0.0 { (acc / tot) as f32 } else { 0.0 } }).collect::<Vec<f32>>() };
            if per { let cs: Vec<Vec<f32>> = (0..3).map(|j| cdf(&move |c: &[f32]| c[j])).collect(); each(f, |c, _, _| { let mut o = c; for j in 0..3 { let e = cs[j][(c[j].clamp(0.0, 1.0) * (N - 1) as f32) as usize]; o[j] = c[j] + (e - c[j]) * am; } o }); }
            else { let cl = cdf(&|c: &[f32]| luma(c)); each(f, |c, _, _| { let l = luma(&c); let e = cl[(l.clamp(0.0, 1.0) * (N - 1) as f32) as usize]; let k = if l > 1e-4 { 1.0 + (e / l - 1.0) * am } else { 1.0 }; [c[0] * k, c[1] * k, c[2] * k, c[3]] }); } }
        "auto_color" => { let clip = q("clip", 0.001).clamp(0.0, 0.2) as f64; let am = q("amount", 1.0).clamp(0.0, 1.0); let neutral = q("snap_neutral", 1.0) >= 0.5;
            let mut lo = [0f32; 3]; let mut hi = [1f32; 3];
            for j in 0..3 { let mut v: Vec<f32> = f.d.chunks_exact(4).filter(|c| c[3] > 0.5).map(|c| c[j]).collect(); if v.len() < 16 { return Some(Ok(())); } v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); let n = v.len(); lo[j] = v[((n as f64 * clip) as usize).min(n - 1)]; hi[j] = v[((n as f64 * (1.0 - clip)) as usize).min(n - 1)]; }
            let gain = [0, 1, 2].map(|j| 1.0 / (hi[j] - lo[j]).max(0.05));
            let mid = if neutral { let m: Vec<f32> = (0..3).map(|j| { let s: f64 = f.d.chunks_exact(4).filter(|c| c[3] > 0.5).map(|c| ((c[j] - lo[j]) * gain[j]).clamp(0.0, 1.0) as f64).sum(); (s / f.d.len() as f64 * 4.0) as f32 }).collect(); let g = (m[0] + m[1] + m[2]) / 3.0; [0, 1, 2].map(|j| if m[j] > 0.02 { (g.max(1e-3).ln() / m[j].max(1e-3).ln()).clamp(0.5, 2.0) } else { 1.0 }) } else { [1.0; 3] };
            each(f, |c, _, _| { let mut o = c; for j in 0..3 { let v = ((c[j] - lo[j]) * gain[j]).max(0.0).powf(mid[j]); o[j] = c[j] + (v - c[j]) * am; } o }); }
        "broadcast_colors" => { let lim = (q("max_ire", 110.0).clamp(90.0, 120.0) - 7.5) / 92.5; let meth = q("method", 0.0) as i32; let (ymin, ymax) = (q("min_luma", 0.0627), q("max_luma", 0.9216));
            each(f, |c, _, _| { let mut v = yuv([c[0], c[1], c[2]]);
                if meth == 2 { v[0] = v[0].clamp(ymin, ymax); let ch = (v[1] * v[1] + v[2] * v[2]).sqrt(); let cmax = 0.5; if ch > cmax { v[1] *= cmax / ch; v[2] *= cmax / ch; } }
                else { let ch = (v[1] * v[1] + v[2] * v[2]).sqrt() * 1.6; let amp = v[0] + ch; if amp > lim { if meth == 1 { v[0] = (lim - ch).max(0.0); } else { let k = ((lim - v[0]) / ch.max(1e-6)).clamp(0.0, 1.0); v[1] *= k; v[2] *= k; } } }
                let o = unyuv(v); [o[0], o[1], o[2], c[3]] }); }
        "black_and_white" => { let wv = [q("reds", 0.4), q("yellows", 0.6), q("greens", 0.4), q("cyans", 0.6), q("blues", 0.2), q("magentas", 0.8)]; let ti = q("tint", 0.0).clamp(0.0, 1.0); let tc = [q("tint_r", 0.86), q("tint_g", 0.74), q("tint_b", 0.55)];
            each(f, |c, _, _| { let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2])); let mut s = [c[0], c[1], c[2]]; s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); let mid = s[1];
                let prim = if c[0] == mx { 0 } else if c[1] == mx { 2 } else { 4 }; let sec = if c[2] == mn { 1 } else if c[0] == mn { 3 } else { 5 };
                let g = mn + (mid - mn) * wv[sec] + (mx - mid) * wv[prim]; let g = g.max(0.0); let o = if ti > 0.0 { let lum_t = luma(&tc).max(1e-3); [0, 1, 2].map(|j| g + (g * tc[j] / lum_t - g) * ti) } else { [g; 3] }; [o[0], o[1], o[2], c[3]] }); }
        "tritone" => { let hi = [q("high_r", 1.0), q("high_g", 1.0), q("high_b", 1.0)]; let md = [q("mid_r", 0.5), q("mid_g", 0.4), q("mid_b", 0.3)]; let sh = [q("shadow_r", 0.0), q("shadow_g", 0.0), q("shadow_b", 0.0)]; let bl = q("blend", 0.0).clamp(0.0, 1.0);
            each(f, |c, _, _| { let l = luma(&c).clamp(0.0, 1.0); let o = if l < 0.5 { let u = l * 2.0; [0, 1, 2].map(|j| sh[j] + (md[j] - sh[j]) * u) } else { let u = l * 2.0 - 1.0; [0, 1, 2].map(|j| md[j] + (hi[j] - md[j]) * u) }; [0, 1, 2].map(|j| o[j] + (c[j] - o[j]) * bl).into_iter().chain([c[3]]).collect::<Vec<_>>().try_into().unwrap() }); }
        "vibrance" => { let (vb, sat) = (q("vibrance", 0.3), q("saturation", 0.0)); each(f, |c, _, _| { let l = luma(&c); let (mx, mn) = (c[0].max(c[1]).max(c[2]), c[0].min(c[1]).min(c[2])); let s = (mx - mn).clamp(0.0, 1.0);
            let x = rgb2hsl([c[0], c[1], c[2]]); let skin = 1.0 - sstep(0.02, 0.09, hue_dist(x[0], 0.07)); let k = 1.0 + sat + vb * (1.0 - s) * (1.0 - 0.6 * skin) * if vb > 0.0 { 1.0 } else { 1.0 }; [l + (c[0] - l) * k, l + (c[1] - l) * k, l + (c[2] - l) * k, c[3]] }); }
        "gamma_pedestal_gain" => { let bs = q("black_stretch", 0.0).clamp(0.0, 1.0); let ch = ["r", "g", "b"].map(|n| (q(&format!("{n}_gamma"), 1.0).max(0.01), q(&format!("{n}_pedestal"), 0.0), q(&format!("{n}_gain"), 1.0)));
            each(f, |c, _, _| { let mut o = c; for j in 0..3 { let v = c[j].max(0.0); let v = v + bs * v * (1.0 - v) * (1.0 - v); o[j] = ch[j].1 + ch[j].2 * v.powf(1.0 / ch[j].0) * (1.0 - ch[j].1); } o }); }
        // ---- channel
        "arithmetic" => { let op = q("operator", 3.0) as i32; let k = [q("r", 0.0), q("g", 0.0), q("b", 0.0)]; let clip = q("clip", 1.0) >= 0.5;
            each(f, |c, _, _| { let mut o = c; for j in 0..3 { let (a, b) = (c[j], k[j]); let bits = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8; let fb = |v: u8| v as f32 / 255.0;
                o[j] = match op { 0 => fb(bits(a) & bits(b)), 1 => fb(bits(a) | bits(b)), 2 => fb(bits(a) ^ bits(b)), 3 => a + b, 4 => a - b, 5 => (a - b).abs(), 6 => a.min(b), 7 => a.max(b), 8 => if a > b { 0.0 } else { a }, 9 => if a < b { 0.0 } else { a }, 10 => if a >= b { 1.0 } else { 0.0 }, 11 => a * b, _ => 1.0 - (1.0 - a) * (1.0 - b) };
                if clip { o[j] = o[j].clamp(0.0, 1.0); } } o }); }
        "minimax" => { let op = q("operation", 0.0) as i32; let r = pxs(q("radius", 2.0)).round().max(0.0) as usize; if r == 0 { return Some(Ok(())); } let chs: &[usize] = match q("channel", 0.0) as i32 { 1 => &[3], 2 => &[0, 1, 2, 3], _ => &[0, 1, 2] };
            let dir = q("direction", 0.0) as i32; let (hz, vt) = (dir != 2, dir != 1);
            match op { 1 => minmax(&mut f.d, w, h, chs, r, false, hz, vt), 2 => { minmax(&mut f.d, w, h, chs, r, false, hz, vt); minmax(&mut f.d, w, h, chs, r, true, hz, vt); } 3 => { minmax(&mut f.d, w, h, chs, r, true, hz, vt); minmax(&mut f.d, w, h, chs, r, false, hz, vt); } _ => minmax(&mut f.d, w, h, chs, r, true, hz, vt) } }
        "channel_combiner" => { let md = q("mode", 0.0) as i32; each(f, |c, _, _| { let rgb = [c[0], c[1], c[2]]; let o = match md {
                0 => { let x = rgb2hsl(rgb); [x[0], x[2], x[1]] } 1 => hsl2rgb(c[0], c[2].clamp(0.0, 1.0), c[1]), 2 => { let v = yuv(rgb); [v[0], v[1] + 0.5, v[2] + 0.5] } 3 => unyuv([c[0], c[1] - 0.5, c[2] - 0.5]),
                4 => [rgb2hsl(rgb)[2]; 3], 5 => [rgb2hsl(rgb)[0]; 3], 6 => [rgb2hsl(rgb)[1]; 3], 7 => [c[0].min(c[1]).min(c[2]); 3], 8 => [c[0].max(c[1]).max(c[2]); 3], _ => [luma(&c); 3] }; [o[0], o[1], o[2], c[3]] }); }
        // ---- noise and generate
        "noise_hls" => { let (nh, nl, ns) = (q("hue", 0.0), q("lightness", 0.1), q("saturation", 0.0)); let gs = pxs(q("grain_size", 1.0)).max(0.5); let ty = q("type", 0.0) as i32; let fr = if q("animate", 1.0) >= 0.5 { (t * 60.0).floor() as u64 } else { 0 }; let sd = seed ^ (q("seed", 0.0) as u64).wrapping_mul(7919) ^ fr.wrapping_mul(104729);
            let nz = move |x: usize, y: usize, k: u64| { let (gx, gy) = ((x as f32 / gs) as i64, (y as f32 / gs) as i64); let v = hash(gx, gy, sd ^ k) * 2.0 - 1.0; match ty { 1 => v * v.abs(), 2 => (v + hash(gx + 1, gy, sd ^ k) * 2.0 - 1.0 + hash(gx, gy + 1, sd ^ k) * 2.0 - 1.0) / 3.0, _ => v } };
            each(f, |c, x, y| { let mut v = rgb2hsl([c[0], c[1], c[2]]); v[0] += nz(x, y, 1) * nh * 0.5; v[1] = (v[1] + nz(x, y, 2) * ns).clamp(0.0, 1.0); v[2] = v[2] + nz(x, y, 3) * nl * 0.5; let o = hsl2rgb(v[0], v[1], v[2].clamp(0.0, 1.0)); [o[0], o[1], o[2], c[3]] }); }
        "noise_alpha" => { let am = q("amount", 0.5).clamp(0.0, 1.0); let sz = pxs(q("size", 4.0)).max(0.5); let ty = q("type", 0.0) as i32; let fr = if q("animate", 1.0) >= 0.5 { (t * 60.0).floor() as u64 } else { 0 }; let sd = seed ^ (q("seed", 0.0) as u64).wrapping_mul(7919) ^ fr.wrapping_mul(104729); let set = q("overflow", 0.0) >= 0.5;
            each(f, |c, x, y| { let (fx, fy) = (x as f32 / sz, y as f32 / sz); let v = match ty { 2 => { let (x0, y0) = (fx.floor(), fy.floor()); let (tx, ty_) = (fx - x0, fy - y0); let (tx, ty_) = (tx * tx * (3.0 - 2.0 * tx), ty_ * ty_ * (3.0 - 2.0 * ty_)); let (xi, yi) = (x0 as i64, y0 as i64);
                    let a = hash(xi, yi, sd) + (hash(xi + 1, yi, sd) - hash(xi, yi, sd)) * tx; let b = hash(xi, yi + 1, sd) + (hash(xi + 1, yi + 1, sd) - hash(xi, yi + 1, sd)) * tx; a + (b - a) * ty_ } 1 => hash(fx as i64, fy as i64, sd).powi(2), _ => hash(fx as i64, fy as i64, sd) };
                let n = if set { v } else { 1.0 - am + am * v }; [c[0], c[1], c[2], if set { n * am + c[3] * (1.0 - am) } else { c[3] * n }] }); }
        "circle" => { let (cx, cy, r) = (q("cx", 0.5) * wf, q("cy", 0.5) * hf, pxs(q("radius", 150.0)).max(0.0)); let (th, fe) = (pxs(q("thickness", 0.0)), pxs(q("feather", 2.0)).max(0.5)); let col = [q("r", 1.0), q("g", 1.0), q("b", 1.0)]; let op = q("opacity", 1.0).clamp(0.0, 1.0); let inv = q("invert", 0.0) >= 0.5; let bl = q("blend", 0.0) as i32;
            each(f, |c, x, y| { let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt(); let mut k = 1.0 - sstep(r - fe * 0.5, r + fe * 0.5, d); if th > 0.0 { k *= sstep(r - th - fe * 0.5, r - th + fe * 0.5, d); } if inv { k = 1.0 - k; } let k = k * op;
                match bl { 1 => [c[0], c[1], c[2], c[3] * k], 2 => [c[0], c[1], c[2], c[3] * (1.0 - k)], _ => { let a = k + c[3] * (1.0 - k); if a <= 1e-6 { return [0.0; 4]; } let o = [0, 1, 2].map(|j| (col[j] * k + c[j] * c[3] * (1.0 - k)) / a); [o[0], o[1], o[2], a] } } }); }
        "ellipse" => { let (cx, cy, ew, eh) = (q("cx", 0.5) * wf, q("cy", 0.5) * hf, pxs(q("width", 400.0)).max(1.0) / 2.0, pxs(q("height", 250.0)).max(1.0) / 2.0); let th = pxs(q("thickness", 20.0)).max(0.5); let so = q("softness", 0.3).clamp(0.0, 1.0);
            let (ci, co) = ([q("in_r", 1.0), q("in_g", 1.0), q("in_b", 1.0)], [q("out_r", 0.2), q("out_g", 0.6), q("out_b", 1.0)]); let over = q("on_original", 1.0) >= 0.5;
            each(f, |c, x, y| { let (vx, vy) = ((x as f32 + 0.5 - cx) / ew, (y as f32 + 0.5 - cy) / eh); let rr = (vx * vx + vy * vy).sqrt(); let d = (rr - 1.0) * ew.min(eh); // signed px from the ellipse line
                let core = 1.0 - sstep(th * 0.5 * (1.0 - so), th * 0.5, d.abs()); let glow = (1.0 - sstep(0.0, th * (0.5 + so * 2.0), d.abs())) * so; let k = core.max(glow);
                let col = [0, 1, 2].map(|j| co[j] + (ci[j] - co[j]) * core); if over { let a = k + c[3] * (1.0 - k); if a <= 1e-6 { return [0.0; 4]; } let o = [0, 1, 2].map(|j| (col[j] * k + c[j] * c[3] * (1.0 - k)) / a); [o[0], o[1], o[2], a] } else { [col[0], col[1], col[2], k] } }); }
        _ => return None,
    }
    Some(Ok(()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keyframe::Prop;
    fn ps(v: &[(&str, f64)]) -> Params { v.iter().map(|(k, x)| (k.to_string(), Prop::new(*x))).collect() }
    fn frame(w: usize, h: usize, g: impl Fn(usize, usize) -> [f32; 4]) -> Frame { let mut f = Frame::new(w, h); for y in 0..h { for x in 0..w { f.d[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&g(x, y)); } } f }
    fn run(k: &str, prm: &[(&str, f64)], f: &mut Frame) { apply(k, &ps(prm), &Default::default(), f, 0.0, 1).unwrap().unwrap(); }
    #[test]
    fn neutral_settings_leave_the_picture_alone() {
        let src = frame(32, 24, |x, y| [x as f32 / 32.0, y as f32 / 24.0, 0.3, 1.0]);
        for (k, prm) in [("wave_warp", vec![("height", 0.0)]), ("spherize", vec![("amount", 0.0)]), ("transform", vec![]), ("color_balance", vec![]), ("color_balance_hls", vec![]), ("selective_color", vec![]), ("gamma_pedestal_gain", vec![]), ("arithmetic", vec![("operator", 3.0)]), ("magnify", vec![("magnification", 1.0)])] {
            let mut f = Frame { w: src.w, h: src.h, d: src.d.clone() }; run(k, &prm, &mut f); let e = f.d.iter().zip(&src.d).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max); assert!(e < 2e-3, "{k} changed the picture by {e}"); }
    }
    #[test]
    fn keys_cut_what_they_should() {
        let mut f = frame(4, 1, |x, _| [[0.0, 1.0, 0.0, 1.0], [1.0, 0.0, 0.0, 1.0], [0.05, 0.9, 0.1, 1.0], [0.5, 0.5, 0.5, 1.0]][x]);
        run("linear_color_key", &[], &mut f); assert!(f.d[3] < 0.01 && f.d[7] > 0.99 && f.d[11] < 0.5 && f.d[15] > 0.99, "{:?}", f.d);
        let mut u = frame(2, 1, |x, _| [[0.0, 0.0, 0.0, 1.0], [0.5, 0.25, 0.0, 1.0]][x]); run("unmult", &[], &mut u); assert!(u.d[3] < 0.01 && (u.d[7] - 0.5).abs() < 1e-4 && (u.d[4] - 1.0).abs() < 1e-4, "{:?}", u.d);
        let mut e = frame(3, 1, |x, _| { let v = [0.1, 0.5, 0.9][x]; [v, v, v, 1.0] }); run("extract", &[("black_point", 0.3), ("white_point", 0.7)], &mut e); assert!(e.d[3] < 0.01 && e.d[7] > 0.99 && e.d[11] < 0.01, "{:?}", e.d);
    }
    #[test]
    fn colour_effects_do_what_they_say() {
        let mut l = frame(2, 1, |x, _| [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]][x]); run("leave_color", &[], &mut l); assert!(l.d[0] > 0.9 && l.d[1] < 0.1, "red is kept"); assert!((l.d[4] - l.d[6]).abs() < 1e-4, "blue goes grey");
        let mut b = frame(2, 1, |x, _| [[1.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 1.0]][x]); run("black_and_white", &[], &mut b); assert!((b.d[0] - 0.4).abs() < 1e-4 && (b.d[4] - 0.2).abs() < 1e-4, "reds 40%, blues 20%: {:?}", b.d);
        let mut c = frame(1, 1, |_, _| [1.0, 0.0, 0.0, 1.0]); run("change_to_color", &[], &mut c); assert!(c.d[2] > c.d[0], "red became blue-ish: {:?}", c.d);
        let mut eq = frame(64, 1, |x, _| { let v = 0.4 + x as f32 / 640.0; [v, v, v, 1.0] }); run("equalize", &[], &mut eq); assert!(eq.d[0] < 0.1 && eq.d[63 * 4] > 0.9, "a narrow range spreads to the full range");
        let mut bc = frame(1, 1, |_, _| [1.0, 1.0, 0.0, 1.0]); run("broadcast_colors", &[], &mut bc); let v = yuv([bc.d[0], bc.d[1], bc.d[2]]); assert!(v[0] + (v[1] * v[1] + v[2] * v[2]).sqrt() * 1.6 <= (110.0 - 7.5) / 92.5 + 1e-3, "composite amplitude limited");
    }
    #[test]
    fn distortions_move_pixels_where_expected() {
        let src = frame(64, 64, |x, _| if x < 32 { [1.0, 0.0, 0.0, 1.0] } else { [0.0, 0.0, 1.0, 1.0] });
        let mut t = Frame { w: 64, h: 64, d: src.d.clone() }; run("transform", &[("x", 0.75)], &mut t); assert!(t.d[(32 * 64 + 10) * 4 + 3] < 0.01 && t.d[(32 * 64 + 40) * 4] > 0.99, "moved right by a quarter");
        let mut m = Frame { w: 64, h: 64, d: src.d.clone() }; let mut s = std::collections::BTreeMap::new(); let mut pts = vec![[0.0f32, 0.0]; 4]; pts[1] = [0.0, 0.0]; s.insert("mesh".to_string(), serde_json::to_string(&vec![[10.0, 0.0]; 4]).unwrap());
        apply("mesh_warp", &ps(&[("rows", 1.0), ("cols", 1.0)]), &s, &mut m, 0.0, 1).unwrap().unwrap(); let _ = pts; assert!(m.d[(32 * 64 + 5) * 4 + 3] < 0.01 && m.d[(32 * 64 + 40) * 4] > 0.99, "the whole grid shifted right");
        let mut mn = frame(9, 9, |x, y| if x == 4 && y == 4 { [1.0, 1.0, 1.0, 1.0] } else { [0.0, 0.0, 0.0, 1.0] }); run("minimax", &[("radius", 1.0)], &mut mn); assert!(mn.d[(3 * 9 + 3) * 4] > 0.99 && mn.d[(2 * 9 + 2) * 4] < 0.01, "max grows one pixel each way");
    }
}
