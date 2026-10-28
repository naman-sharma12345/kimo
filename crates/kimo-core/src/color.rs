//! Pro color: log/ACES transforms, primaries, log wheels, RGB + hue curves, qualifier, power windows,
//! film print emulation, halation, white balance, color transfer, tone mapping. Scopes live in `scope_png`.
use crate::effects::{ hsl2rgb, lum, p, rgb2hsl, Frame, Params};
use std::sync::OnceLock;
pub mod managed;
pub(crate) mod frame;
pub mod curves;
pub mod lut;
pub mod bake;
pub mod cct_grade;
pub mod scopes;
pub mod scope_image;
pub mod scope_command;
pub mod matching;
pub mod idt;
pub mod cdl;
pub mod wheels;
pub mod cdl_xml;
pub mod cdl_file;
pub mod blur;
pub mod linear;
pub mod pipeline;
pub mod view_video;
pub mod encode_contract;
pub mod wrapper;
pub mod metadata;
pub mod hdr_validation;
pub mod delivery;
pub(crate) mod process;
pub(crate) mod mux;
pub mod interchange;
pub mod exposure;
pub mod output_audit;
pub mod shoulder;
pub mod finish;
pub mod film_response;
pub mod node_graph;
pub mod grade_scopes;
pub mod printstock;
pub mod denoise;
pub mod bilateral_blur;
pub mod subject_match;
pub mod neutral;
pub mod adaptation;
pub mod gamut_ray;
pub mod auto_balance;
pub mod boost_detail;
pub mod dehaze;
pub mod skin_smooth;
pub mod looks;
pub mod warper;
pub mod hdr_zones;
pub mod grain_look;
pub mod ocio;

type C = (&'static str, f64, &'static str);
pub type Entry = (&'static str, &'static str, &'static [C]);

fn leak(s: String) -> &'static str { Box::leak(s.into_boxed_str()) }
fn knots(prefix: &str, n: usize, f: impl Fn(usize) -> f64, desc: &str) -> Vec<C> { (0..n).map(|i| (leak(format!("{prefix}{i}")), f(i), leak(desc.to_string()) as &'static str)).collect() }
fn lv(v: Vec<C>) -> &'static [C] { Box::leak(v.into_boxed_slice()) }

const ADJ: &[C] = &[("exposure", 0.0, "stops"), ("contrast", 1.0, "around 0.435 pivot"), ("saturation", 1.0, "0..3"), ("temperature", 0.0, "-1..1"), ("tint", 0.0, "-1..1"), ("hue_shift", 0.0, "degrees"), ("gain_r", 1.0, "R gain"), ("gain_g", 1.0, "G gain"), ("gain_b", 1.0, "B gain")];

pub fn catalog3() -> &'static [Entry] {
    static CAT: OnceLock<Vec<Entry>> = OnceLock::new();
    CAT.get_or_init(|| {
        let mut v: Vec<Entry> = vec![];
        v.push(("color_space", "Convert between spaces on the pixel values. 0 linear, 1 sRGB, 2 Rec.709 (gamma 2.4), 3 ACEScct, 4 ARRI LogC3 (EI800), 5 Sony S-Log3. Use log->display before grading log footage, or display->log to grade in log. Gamut is NOT converted (transfer curves only).", lv(vec![("from", 1.0, "space id"), ("to", 0.0, "space id")])));
        v.push(("primaries", "Resolve-style primary grade: lift/gamma/gain/offset per channel + master, contrast with pivot, saturation, shadows/highlights, temperature/tint", lv(vec![
            ("lift_r", 0.0, "shadows"), ("lift_g", 0.0, ""), ("lift_b", 0.0, ""), ("lift_m", 0.0, "master"),
            ("gamma_r", 1.0, "mids"), ("gamma_g", 1.0, ""), ("gamma_b", 1.0, ""), ("gamma_m", 1.0, "master"),
            ("gain_r", 1.0, "highlights"), ("gain_g", 1.0, ""), ("gain_b", 1.0, ""), ("gain_m", 1.0, "master"),
            ("offset_r", 0.0, "overall"), ("offset_g", 0.0, ""), ("offset_b", 0.0, ""), ("offset_m", 0.0, "master"),
            ("contrast", 1.0, "0..3"), ("pivot", 0.435, "0..1"), ("saturation", 1.0, "0..3"), ("shadows", 0.0, "-1..1"), ("highlights", 0.0, "-1..1"), ("temperature", 0.0, "-1..1"), ("tint", 0.0, "-1..1"), ("model", 0.0, "0 legacy, 1 exact LGG/CDL (neutral = bit-identical)"), ("space", 0.0, "model 1 grading encoding: 0 as stored, 1 sRGB, 2 Rec709, 3 ACEScct, 4 LogC3, 5 S-Log3")])));
        v.push(("log_wheels", "Resolve 'Log' wheels: independent shadow / midtone / highlight color+luma offsets with adjustable range, plus global offset. Values are signed offsets (-1..1 scaled).", lv(vec![
            ("shadow_r", 0.0, "R"), ("shadow_g", 0.0, "G"), ("shadow_b", 0.0, "B"), ("shadow_m", 0.0, "luma"),
            ("mid_r", 0.0, "R"), ("mid_g", 0.0, "G"), ("mid_b", 0.0, "B"), ("mid_m", 0.0, "luma"),
            ("high_r", 0.0, "R"), ("high_g", 0.0, "G"), ("high_b", 0.0, "B"), ("high_m", 0.0, "luma"),
            ("offset_r", 0.0, "R"), ("offset_g", 0.0, "G"), ("offset_b", 0.0, "B"), ("shadow_range", 0.33, "luma where shadows end"), ("high_range", 0.66, "luma where highlights begin")])));
        v.push(("rgb_curves", "Custom RGB curves: 9 evenly spaced knots (x=0,1/8..1) per curve m (master), r, g, b; smooth monotone interpolation. Defaults = identity (k/8). Set knot y values.", {
            let mut c = vec![]; for ch in ["m", "r", "g", "b"] { c.extend(knots(ch, 9, |i| i as f64 / 8.0, "knot y")); } lv(c) }));
        v.push(("hue_curves", "Hue-vs-Hue (degrees shift), Hue-vs-Sat (multiplier), Hue-vs-Lum (offset) at 12 hue knots (0,30..330 deg), and Lum-vs-Sat (multiplier) at 9 luma knots.", {
            let mut c = vec![]; c.extend(knots("hvh", 12, |_| 0.0, "hue shift deg at hue k*30")); c.extend(knots("hvs", 12, |_| 1.0, "sat mult at hue k*30")); c.extend(knots("hvl", 12, |_| 0.0, "luma offset at hue k*30")); c.extend(knots("lvs", 9, |_| 1.0, "sat mult at luma k/8")); lv(c) }));
        v.push(("qualifier", "HSL qualifier (secondary): key by hue/sat/lum ranges with softness, blur the matte, then grade inside (or outside with invert). show_matte=1 views the matte.", lv({
            let mut c = vec![("hue_center", 30.0, "deg"), ("hue_width", 40.0, "deg"), ("hue_soft", 20.0, "deg"), ("sat_min", 0.15, "0..1"), ("sat_max", 1.0, "0..1"), ("sat_soft", 0.1, "0..1"), ("lum_min", 0.0, "0..1"), ("lum_max", 1.0, "0..1"), ("lum_soft", 0.1, "0..1"), ("matte_blur", 3.0, "px"), ("matte_refine", 0.0, "edge-aware guided radius px"), ("refine_eps", 0.001, "guided regularization"), ("clean_black", 0.0, "matte cutoff 0..1"), ("clean_white", 1.0, "matte white level 0..1"), ("matte_gamma", 1.0, "positive matte gamma"), ("invert", 0.0, "0/1"), ("show_matte", 0.0, "0/1"), ("amount", 1.0, "0..1")]; c.extend_from_slice(ADJ); c })));
        v.push(("power_window", "Power window (secondary): ellipse/rect/gradient shape with feather and rotation; grade inside (or outside with invert). Keyframe cx/cy (or feed track_point output) to track. shape 0 ellipse, 1 rect, 2 linear gradient.", lv({
            let mut c = vec![("shape", 0.0, "0 ellipse, 1 rect, 2 gradient"), ("cx", 0.5, "0..1"), ("cy", 0.5, "0..1"), ("w", 0.4, "fraction of width"), ("h", 0.4, "fraction of height"), ("rotation", 0.0, "deg"), ("feather", 0.2, "0..1"), ("invert", 0.0, "0/1"), ("show_matte", 0.0, "0/1"), ("amount", 1.0, "0..1")]; c.extend_from_slice(ADJ); c })));
        v.push(("film_print", "Print-stock emulation: filmic toe/shoulder S-curve, dye crosstalk, density-based saturation (rich shadows, rolled-off highlights), print-light warmth", lv(vec![("toe", 0.35, "0..1 shadow compression"), ("shoulder", 0.35, "0..1 highlight roll-off"), ("pivot", 0.435, "0..1"), ("crosstalk", 0.06, "0..0.3 dye cross-contamination"), ("density", 0.25, "0..1 saturation in shadows"), ("warmth", 0.03, "-0.2..0.2 print light"), ("amount", 1.0, "0..1")])));
        v.push(("halation", "Film halation: red-orange glow bleeding from bright highlights (light scattering through the film base)", lv(vec![("threshold", 0.75, "0..1"), ("radius", 24.0, "px"), ("amount", 0.6, "0..2"), ("r", 1.0, "R"), ("g", 0.3, "G"), ("b", 0.1, "B")])));
        v.push(("bleach_bypass", "Bleach bypass: desaturated, high-contrast silver retention look", lv(vec![("amount", 0.7, "0..1")])));
        v.push(("white_balance", "Neutral-point white balance via per-channel gains (set by pick_white_balance, or by hand)", lv(vec![("r_gain", 1.0, "R"), ("g_gain", 1.0, "G"), ("b_gain", 1.0, "B")])));
        v.push(("color_transfer", "Reinhard-style statistical color match (per-channel mean/std). Normally set by the match_color tool.", lv(vec![("sr_mean", 0.5, "src R mean"), ("sg_mean", 0.5, ""), ("sb_mean", 0.5, ""), ("sr_std", 0.2, ""), ("sg_std", 0.2, ""), ("sb_std", 0.2, ""), ("tr_mean", 0.5, "ref R mean"), ("tg_mean", 0.5, ""), ("tb_mean", 0.5, ""), ("tr_std", 0.2, ""), ("tg_std", 0.2, ""), ("tb_std", 0.2, ""), ("strength", 1.0, "0..1")])));
        v.push(("tonemap", "Tone mapping for HDR-ish / linear input. mode 0 ACES filmic fit, 1 Reinhard, 2 Hable filmic. Optionally applies sRGB encoding.", lv(vec![("mode", 0.0, "0..2"), ("exposure", 0.0, "stops"), ("encode_srgb", 1.0, "0/1")])));
        v.push(("highlight_rolloff", "Soft-clip highlights above a knee (avoid harsh digital clipping)", lv(vec![("knee", 0.8, "0..1"), ("softness", 0.5, "0..1")])));
        v.push(("split_toning", "Tint shadows and highlights with separate hues and a balance control", lv(vec![("shadow_hue", 210.0, "deg"), ("shadow_sat", 0.3, "0..1"), ("high_hue", 35.0, "deg"), ("high_sat", 0.3, "0..1"), ("balance", 0.0, "-1..1")])));
        v.extend(managed::catalog());
        v.extend(curves::catalog());
        v.extend(matching::catalog());
        v.extend(idt::catalog());
        v.extend(cdl::catalog());
        v.extend(linear::catalog());
        v.extend(shoulder::catalog());
        v.extend(finish::catalog());
        v.extend(film_response::catalog());
        v.extend(cct_grade::catalog());
        v.extend(printstock::catalog());
        v.extend(denoise::catalog());
        v.extend(bilateral_blur::catalog());
        v.extend(adaptation::catalog());
        v.extend(gamut_ray::catalog());
        v.extend(boost_detail::catalog());
        v.extend(dehaze::catalog());
        v.extend(skin_smooth::catalog());
        v.extend(looks::catalog());
        v.extend(warper::catalog());
        v.extend(hdr_zones::catalog());
        v.extend(generators::catalog());
        v.extend(controls_v7::catalog());
        v.extend(crate::keying::catalog4());
        v
    })
}

// ---- transfer curves ----
pub fn to_lin(s: u32, v: f32) -> f32 { match s {
    1 => if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) },
    2 => v.max(0.0).powf(2.4),
    3 => if v <= 0.155251141552511 { (v - 0.0729055341958355) / 10.5402377416545 } else { 2f32.powf(v * 17.52 - 9.72) },
    4 => if v > 0.1496582 { (10f32.powf((v - 0.385537) / 0.24719) - 0.052272) / 5.555556 } else { (v - 0.092809) / 5.367655 },
    5 => { let t = v * 1023.0; if t >= 171.2102946929 { 10f32.powf((t - 420.0) / 261.5) * 0.19 - 0.01 } else { (t - 95.0) * 0.01125 / (171.2102946929 - 95.0) } }
    _ => v } }
pub fn from_lin(s: u32, x: f32) -> f32 { match s {
    1 => if x <= 0.0031308 { x * 12.92 } else { 1.055 * x.max(0.0).powf(1.0 / 2.4) - 0.055 },
    2 => x.max(0.0).powf(1.0 / 2.4),
    3 => if x <= 0.0078125 { 10.5402377416545 * x + 0.0729055341958355 } else { (x.max(1e-6).log2() + 9.72) / 17.52 },
    4 => if x > 0.010591 { 0.24719 * (5.555556 * x + 0.052272).max(1e-6).log10() + 0.385537 } else { 5.367655 * x + 0.092809 },
    5 => if x >= 0.01125 { (420.0 + ((x + 0.01) / 0.19).max(1e-6).log10() * 261.5) / 1023.0 } else { (x * (171.2102946929 - 95.0) / 0.01125 + 95.0) / 1023.0 },
    _ => x } }

fn sstep(a: f32, b: f32, x: f32) -> f32 { let t = ((x - a) / (b - a).max(1e-6)).clamp(0.0, 1.0); t * t * (3.0 - 2.0 * t) }

/// Monotone cubic (Fritsch-Carlson) LUT over uniformly spaced knots on 0..1.
fn curve_lut(y: &[f32], n: usize) -> Vec<f32> {
    let k = y.len(); let h = 1.0 / (k - 1) as f32; let d: Vec<f32> = (0..k - 1).map(|i| (y[i + 1] - y[i]) / h).collect(); let mut m = vec![0f32; k];
    m[0] = d[0]; m[k - 1] = d[k - 2]; for i in 1..k - 1 { m[i] = if d[i - 1] * d[i] <= 0.0 { 0.0 } else { (d[i - 1] + d[i]) * 0.5 }; }
    for i in 0..k - 1 { if d[i].abs() < 1e-9 { m[i] = 0.0; m[i + 1] = 0.0; } else { let (a, b) = (m[i] / d[i], m[i + 1] / d[i]); let s = a * a + b * b; if s > 9.0 { let t = 3.0 / s.sqrt(); m[i] = t * a * d[i]; m[i + 1] = t * b * d[i]; } } }
    (0..n).map(|j| { let x = j as f32 / (n - 1) as f32; let i = ((x / h) as usize).min(k - 2); let t = (x - i as f32 * h) / h; let (t2, t3) = (t * t, t * t * t);
        (2.0 * t3 - 3.0 * t2 + 1.0) * y[i] + (t3 - 2.0 * t2 + t) * h * m[i] + (-2.0 * t3 + 3.0 * t2) * y[i + 1] + (t3 - t2) * h * m[i + 1] }).collect() }
fn lut_at(l: &[f32], v: f32) -> f32 { let x = v.clamp(0.0, 1.0) * (l.len() - 1) as f32; let i = (x as usize).min(l.len() - 2); let t = x - i as f32; l[i] + (l[i + 1] - l[i]) * t }
fn cyc(vals: &[f32], hue: f32) -> f32 { let n = vals.len(); let x = hue.rem_euclid(360.0) / 360.0 * n as f32; let i = x as usize % n; let t = x - x.floor(); let t = (1.0 - (t * std::f32::consts::PI).cos()) * 0.5; vals[i] + (vals[(i + 1) % n] - vals[i]) * t }

/// Shared "grade inside the matte" adjustments (ADJ params).
fn adjust(c: [f32; 3], g: &dyn Fn(&str, f64) -> f32) -> [f32; 3] {
    let ex = 2f32.powf(g("exposure", 0.0)); let ct = g("contrast", 1.0); let sat = g("saturation", 1.0); let tp = g("temperature", 0.0) * 0.2; let tn = g("tint", 0.0) * 0.2; let hs = g("hue_shift", 0.0);
    let mut o = [c[0] * ex * g("gain_r", 1.0) + tp, c[1] * ex * g("gain_g", 1.0) - tn, c[2] * ex * g("gain_b", 1.0) - tp];
    for j in 0..3 { o[j] = (o[j] - 0.435) * ct + 0.435; }
    if sat != 1.0 || hs != 0.0 { let (h, s, l) = rgb2hsl(o); o = hsl2rgb((h + hs).rem_euclid(360.0), (s * sat).clamp(0.0, 1.0), l.clamp(0.0, 1.0)); let ll = lum(c) * ex; let _ = ll; }
    o }

fn matte_apply(f: &mut Frame, m: &[f32], pr: &Params, t: f64) {
    let g = |k: &str, d: f64| p(pr, k, d, t); let amt = g("amount", 1.0); let show = g("show_matte", 0.0) > 0.5; let inv = g("invert", 0.0) > 0.5;
    for i in 0..f.w * f.h { let mut w = m[i]; if inv { w = 1.0 - w; } w *= amt; if show { for j in 0..3 { f.d[i * 4 + j] = w; } continue; } if w <= 0.0 { continue; }
        let c = [f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]]; let o = adjust(c, &g); for j in 0..3 { f.d[i * 4 + j] = c[j] + (o[j] - c[j]) * w; } } }

pub fn apply3(kind: &str, pr: &Params, f: &mut Frame, t: f64, _seed: u64) -> Result<(), String> {
    let g = |k: &str, d: f64| p(pr, k, d, t); let (w, h) = (f.w, f.h); let n = w * h;
    match kind {
        "film_finish" => return finish::apply(pr,f,t,_seed),
        "film_soft_shoulder"|"film_luma_grain" => return film_response::apply(kind,pr,f,t,_seed),
        "linear_grade" => return linear::apply(pr, f, t),
        "luminance_shoulder" => return shoulder::apply(pr,f,t),
        "print_stock" => return printstock::apply(pr,f,t),
        "denoise" => return denoise::apply(pr,f,t),
        "four_color_gradient"|"radio_waves"|"beam"|"cell_pattern"|"eyedropper_fill"|"fill"|"fractal"|"paint_bucket"|"scribble"|"stroke"|"write_on"|"vegas"|"caustics"|"audio_spectrum"|"audio_waveform" => return generators::apply(kind, pr, f, t, _seed),
        "slider_control"|"checkbox_control"|"angle_control"|"point_control"|"color_control"|"dropdown_control" => return controls_v7::apply(kind, pr, f, t, _seed),
        "bilateral_blur" => return bilateral_blur::apply(pr,f,t),
        "white_adaptation" => return adaptation::apply(pr,f,t),
        "soft_gamut_ray" => return gamut_ray::apply(pr,f,t),
        "color_boost" | "midtone_detail" => return boost_detail::apply(kind, pr, f, t),
        "dehaze" => return dehaze::apply(pr, f, t),
        "skin_smooth" => return skin_smooth::apply(pr, f, t),
        "look" => return looks::apply(pr, f, t),
        "color_warper" => return warper::apply(pr, f, t),
        "hdr_zones" => return hdr_zones::apply(pr, f, t),
        "cdl" => return cdl::apply(pr, f, t),
        "lab_match" => return matching::apply(pr, f, t),
        "camera_idt" => return idt::apply(pr, f, t),
        "point_curves" => return curves::apply(pr, f, t),
        "color_managed" => return managed::apply(pr, f, t),
        "color_space" => { let (a, b) = (g("from", 1.0) as u32, g("to", 0.0) as u32); for i in 0..n { for j in 0..3 { f.d[i * 4 + j] = from_lin(b, to_lin(a, f.d[i * 4 + j])); } } }
        "primaries" => {
            if g("model", 0.0) >= 0.5 { return wheels::primaries_exact(pr, f, t); }
            let lift = [g("lift_r", 0.0) + g("lift_m", 0.0), g("lift_g", 0.0) + g("lift_m", 0.0), g("lift_b", 0.0) + g("lift_m", 0.0)]; let gam = [g("gamma_r", 1.0) * g("gamma_m", 1.0), g("gamma_g", 1.0) * g("gamma_m", 1.0), g("gamma_b", 1.0) * g("gamma_m", 1.0)];
            let gain = [g("gain_r", 1.0) * g("gain_m", 1.0), g("gain_g", 1.0) * g("gain_m", 1.0), g("gain_b", 1.0) * g("gain_m", 1.0)]; let off = [g("offset_r", 0.0) + g("offset_m", 0.0), g("offset_g", 0.0) + g("offset_m", 0.0), g("offset_b", 0.0) + g("offset_m", 0.0)];
            let (ct, pv, sat, sh, hi, tp, tn) = (g("contrast", 1.0), g("pivot", 0.435), g("saturation", 1.0), g("shadows", 0.0), g("highlights", 0.0), g("temperature", 0.0) * 0.15, g("tint", 0.0) * 0.15);
            for i in 0..n { let mut c = [f.d[i * 4] + tp, f.d[i * 4 + 1] - tn, f.d[i * 4 + 2] - tp];
                for j in 0..3 { let mut v = c[j] * gain[j] + off[j]; v = v + lift[j] * (1.0 - v.clamp(0.0, 1.0)); v = v.max(0.0).powf(1.0 / gam[j].max(0.01)); v = (v - pv) * ct + pv; c[j] = v; }
                let l = lum(c).clamp(0.0, 1.0); let add = sh * (1.0 - l).powi(2) * 0.4 + hi * l.powi(2) * 0.4; for j in 0..3 { c[j] += add; }
                let l2 = lum(c); for j in 0..3 { c[j] = l2 + (c[j] - l2) * sat; } f.d[i * 4..i * 4 + 3].copy_from_slice(&c); } }
        "log_wheels" => { let (sr, hr) = (g("shadow_range", 0.33), g("high_range", 0.66)); let ks = |a: &str| [g(&format!("{a}_r"), 0.0) + g(&format!("{a}_m"), 0.0), g(&format!("{a}_g"), 0.0) + g(&format!("{a}_m"), 0.0), g(&format!("{a}_b"), 0.0) + g(&format!("{a}_m"), 0.0)];
            let (s, m, hh) = (ks("shadow"), ks("mid"), ks("high")); let off = [g("offset_r", 0.0), g("offset_g", 0.0), g("offset_b", 0.0)];
            for i in 0..n { let c = [f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]]; let l = lum(c); let ws = 1.0 - sstep(0.0, sr.max(0.01), l); let wh = sstep(hr.min(0.99), 1.0, l); let wm = (1.0 - ws - wh).max(0.0);
                for j in 0..3 { f.d[i * 4 + j] = c[j] + (s[j] * ws + m[j] * wm + hh[j] * wh) * 0.25 + off[j] * 0.25; } } }
        "rgb_curves" => { let mk = |p_: &str| -> Vec<f32> { let y: Vec<f32> = (0..9).map(|i| g(&format!("{p_}{i}"), i as f64 / 8.0)).collect(); curve_lut(&y, 1024) }; let (lm, lr, lg, lb) = (mk("m"), mk("r"), mk("g"), mk("b"));
            for i in 0..n { let o = i * 4; f.d[o] = lut_at(&lr, lut_at(&lm, f.d[o])); f.d[o + 1] = lut_at(&lg, lut_at(&lm, f.d[o + 1])); f.d[o + 2] = lut_at(&lb, lut_at(&lm, f.d[o + 2])); } }
        "hue_curves" => { let a = |p_: &str, c: usize, d: f64| -> Vec<f32> { (0..c).map(|i| g(&format!("{p_}{i}"), d)).collect() }; let (hvh, hvs, hvl, lvs) = (a("hvh", 12, 0.0), a("hvs", 12, 1.0), a("hvl", 12, 0.0), a("lvs", 9, 1.0));
            for i in 0..n { let c = [f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]]; let (hu, s, l) = rgb2hsl(c); if s < 1e-4 { let sm = lut_at(&lvs, l); let _ = sm; continue; } let sm = cyc(&hvs, hu) * { let x = l.clamp(0.0, 1.0) * 8.0; let k = (x as usize).min(7); lvs[k] + (lvs[k + 1] - lvs[k]) * (x - k as f32) };
                let o = hsl2rgb((hu + cyc(&hvh, hu)).rem_euclid(360.0), (s * sm).clamp(0.0, 1.0), (l + cyc(&hvl, hu)).clamp(0.0, 1.0)); f.d[i * 4..i * 4 + 3].copy_from_slice(&o); } }
        "qualifier" => { let (hc, hw, hso) = (g("hue_center", 30.0), g("hue_width", 40.0).max(1.0), g("hue_soft", 20.0).max(0.1)); let (s0, s1, ss) = (g("sat_min", 0.15), g("sat_max", 1.0), g("sat_soft", 0.1).max(0.001)); let (l0, l1, ls) = (g("lum_min", 0.0), g("lum_max", 1.0), g("lum_soft", 0.1).max(0.001));
            let mut m = Frame { w, h, d: vec![0.0; n * 4] }; for i in 0..n { let c = [f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]]; let (hu, s, l) = rgb2hsl(c); let mut dh = (hu - hc).abs() % 360.0; if dh > 180.0 { dh = 360.0 - dh; }
                let kh = 1.0 - sstep(hw * 0.5, hw * 0.5 + hso, dh); let ks = sstep(s0 - ss, s0, s) * (1.0 - sstep(s1, s1 + ss, s)); let kl = sstep(l0 - ls, l0, l) * (1.0 - sstep(l1, l1 + ls, l)); let v = kh * ks * kl; m.d[i * 4] = v; m.d[i * 4 + 1] = v; m.d[i * 4 + 2] = v; m.d[i * 4 + 3] = 1.0; }
            let bl = g("matte_blur", 3.0).clamp(0.0, w.min(h) as f32 / 4.0) as usize; if bl > 0 { blur::blur(&mut m, bl * 2); }
            let mut mm: Vec<f32> = (0..n).map(|i| m.d[i * 4].clamp(0.0, 1.0)).collect();
            let refine = g("matte_refine", 0.0).clamp(0.0, 128.0) as usize;
            if refine > 0 && w.min(h) >= 4 { crate::masks::guided(&mut mm, f, refine, g("refine_eps", 0.001).clamp(1e-6, 1.0)); }
            let black=g("clean_black",0.0).clamp(0.0,0.999);let white=g("clean_white",1.0).clamp(black+0.001,1.0);let gamma=g("matte_gamma",1.0).clamp(0.01,100.0);
            for v in &mut mm { *v=((*v-black)/(white-black)).clamp(0.0,1.0).powf(gamma); }
            matte_apply(f, &mm, pr, t); }
        "power_window" => { let (sh, cx, cy, ww, hh, rot, fe) = (g("shape", 0.0) as u32, g("cx", 0.5) * w as f32, g("cy", 0.5) * h as f32, g("w", 0.4) * w as f32 * 0.5, g("h", 0.4) * h as f32 * 0.5, g("rotation", 0.0).to_radians(), g("feather", 0.2).max(0.001));
            let (sn, cs) = rot.sin_cos(); let mut mm = vec![0f32; n]; for y in 0..h { for x in 0..w { let (dx, dy) = (x as f32 - cx, y as f32 - cy); let (rx, ry) = (dx * cs + dy * sn, -dx * sn + dy * cs);
                let d = match sh { 0 => ((rx / ww.max(1.0)).powi(2) + (ry / hh.max(1.0)).powi(2)).sqrt(), 1 => (rx.abs() / ww.max(1.0)).max(ry.abs() / hh.max(1.0)), _ => (ry / hh.max(1.0)) * 0.5 + 0.5 };
                mm[y * w + x] = if sh == 2 { 1.0 - d.clamp(0.0, 1.0) } else { 1.0 - sstep(1.0 - fe, 1.0 + fe * 0.0001 + 0.0, d / (1.0 + fe)) }; } }
            matte_apply(f, &mm, pr, t); }
        "film_print" => { let (toe, sho, pv, xt, dn, wm, a) = (g("toe", 0.35), g("shoulder", 0.35), g("pivot", 0.435), g("crosstalk", 0.06), g("density", 0.25), g("warmth", 0.03), g("amount", 1.0));
            for i in 0..n { let c = [f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]]; let mut o = [c[0] * (1.0 - 2.0 * xt) + (c[1] + c[2]) * xt, c[1] * (1.0 - 2.0 * xt) + (c[0] + c[2]) * xt, c[2] * (1.0 - 2.0 * xt) + (c[0] + c[1]) * xt];
                for j in 0..3 { let v = o[j].max(0.0); o[j] = if v < pv { pv * (v / pv).powf(1.0 + toe) } else { 1.0 - (1.0 - pv) * ((1.0 - v).max(0.0) / (1.0 - pv)).powf(1.0 + sho) }; }
                o[0] += wm; o[2] -= wm; let l = lum(o); let sat = 1.0 + dn * (1.0 - l) - dn * 0.5 * l * l; for j in 0..3 { o[j] = l + (o[j] - l) * sat; o[j] = c[j] + (o[j] - c[j]) * a; } f.d[i * 4..i * 4 + 3].copy_from_slice(&o); } }
        "halation" => { let (th, r, a, col) = (g("threshold", 0.75), g("radius", 24.0) as usize, g("amount", 0.6), [g("r", 1.0), g("g", 0.3), g("b", 0.1)]); let mut m = Frame { w, h, d: f.d.clone() };
            for i in 0..n { let l = lum([m.d[i * 4], m.d[i * 4 + 1], m.d[i * 4 + 2]]); let v = ((l - th) / (1.0 - th).max(0.01)).clamp(0.0, 1.0); for j in 0..3 { m.d[i * 4 + j] = v * col[j]; } } blur::blur(&mut m, r.clamp(2, 1024)); for i in 0..n { for j in 0..3 { f.d[i * 4 + j] += m.d[i * 4 + j] * a * 2.0; } } }
        "bleach_bypass" => { let a = g("amount", 0.7); for i in 0..n { let c = [f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]]; let l = lum(c); let mix = |v: f32| if l < 0.5 { 2.0 * v * l } else { 1.0 - 2.0 * (1.0 - v) * (1.0 - l) }; for j in 0..3 { f.d[i * 4 + j] = c[j] + (mix(c[j]) - c[j]) * a; } } }
        "white_balance" => { let k = [g("r_gain", 1.0), g("g_gain", 1.0), g("b_gain", 1.0)]; for i in 0..n { for j in 0..3 { f.d[i * 4 + j] *= k[j]; } } }
        "color_transfer" => { let s = g("strength", 1.0); let get = |a: &str| [g(&format!("{a}r_mean"), 0.5), g(&format!("{a}g_mean"), 0.5), g(&format!("{a}b_mean"), 0.5)]; let gs = |a: &str| [g(&format!("{a}r_std"), 0.2), g(&format!("{a}g_std"), 0.2), g(&format!("{a}b_std"), 0.2)]; let (sm, ss, tm, ts) = (get("s"), gs("s"), get("t"), gs("t"));
            for i in 0..n { for j in 0..3 { let v = f.d[i * 4 + j]; let o = (v - sm[j]) * (ts[j] / ss[j].max(0.01)).clamp(0.2, 5.0) + tm[j]; f.d[i * 4 + j] = v + (o - v) * s; } } }
        "tonemap" => { let (mode, ex, enc) = (g("mode", 0.0) as u32, 2f32.powf(g("exposure", 0.0)), g("encode_srgb", 1.0) > 0.5); let hable = |x: f32| ((x * (0.15 * x + 0.05) + 0.004) / (x * (0.15 * x + 0.5) + 0.06)) - 0.02 / 0.3;
            for i in 0..n { for j in 0..3 { let x = f.d[i * 4 + j].max(0.0) * ex; let y = match mode { 0 => (x * (2.51 * x + 0.03)) / (x * (2.43 * x + 0.59) + 0.14), 1 => x / (1.0 + x), _ => hable(x * 2.0) / hable(11.2) }; f.d[i * 4 + j] = if enc { from_lin(1, y.clamp(0.0, 1.0)) } else { y }; } } }
        "highlight_rolloff" => { let (k, so) = (g("knee", 0.8), g("softness", 0.5).max(0.01)); for i in 0..n { for j in 0..3 { let v = f.d[i * 4 + j]; if v > k { let r = 1.0 - k; f.d[i * 4 + j] = k + r * (1.0 - (-(v - k) / (r * so)).exp()) * so.min(1.0).max(0.01) / so.min(1.0).max(0.01) * 1.0; } } } }
        "split_toning" => { let (sh, ss, hh, hs, bal) = (g("shadow_hue", 210.0), g("shadow_sat", 0.3), g("high_hue", 35.0), g("high_sat", 0.3), g("balance", 0.0)); let sc = hsl2rgb(sh, 1.0, 0.5); let hc = hsl2rgb(hh, 1.0, 0.5);
            for i in 0..n { let c = [f.d[i * 4], f.d[i * 4 + 1], f.d[i * 4 + 2]]; let l = lum(c); let t_ = sstep(0.0, 1.0, (l + bal * 0.5).clamp(0.0, 1.0)); for j in 0..3 { let tint = sc[j] * (1.0 - t_) * ss + hc[j] * t_ * hs; f.d[i * 4 + j] = c[j] + (tint - 0.5 * (ss * (1.0 - t_) + hs * t_)) * 0.5 ; } } }
        other => return crate::keying::apply4(other, pr, f, t, _seed),
    }
    if f.d.iter().any(|v| !v.is_finite()) { return Err(format!("color effect '{kind}' produced nonfinite pixels; reduce extreme parameters")); }
    Ok(())
}

#[cfg(feature = "native-ocio")]
pub mod ocio_config;
pub mod effect_domain;

pub mod fast_transfer;
pub mod generators;
pub mod controls_v7;
#[cfg(test)] mod finish_reference;
#[cfg(test)] mod printstock_reference;
#[cfg(test)] mod film_fast_proof;
#[cfg(test)] mod finish_v2_reference;
#[cfg(test)] mod blur_v2_reference;
#[cfg(test)] mod adjust_v2_proof;
