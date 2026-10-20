//! After Effects / W3C blend modes on set_blend. kimo had normal/add/multiply/screen/overlay only; this adds darken, lighten,
//! difference, exclusion, soft_light, hard_light, color_dodge, color_burn, linear_burn, linear_dodge, vivid_light,
//! linear_light, pin_light and the non-separable hue, saturation, color, luminosity.
//!
//! Math (W3C Compositing and Blending Level 1): B(cb, cs) is computed on UNPREMULTIPLIED colour (the canvas keeps
//! premultiplied colour, so cb = canvas_rgb / canvas_alpha), clamped to 0..1, then
//!   cs' = (1 - ab) * cs + ab * B(cb, cs)
//! and cs' goes source-over with the source alpha as = layer alpha * opacity:
//!   co = as * cs' + (1 - as) * canvas_rgb,  ao = as + ab * (1 - as).
//! The five old modes keep their exact previous arithmetic (byte-identical renders).
//!
//! Every expected pixel below is a literal, computed by hand from those formulas in f64 (not by calling kimo), for two
//! colour pairs that hit both branches of the piecewise modes: pair 0 cb [0.1, 0.3, 0.6] cs [0.6, 0.4, 0.3]; pair 1
//! cb [0.8, 0.4, 0.2] cs [0.3, 0.55, 0.75]. Three cases per mode:
//!   opaque         : opaque backdrop, opaque source            -> B(cb, cs)
//!   src_alpha      : opaque backdrop, source alpha 0.4         -> 0.6 * cb + 0.4 * B
//!   backdrop_alpha : transparent canvas, backdrop alpha 0.5, source alpha 0.4 (premultiplied rgb + alpha out)
//!                    -> 0.4 * (0.5 * cs + 0.5 * B) + 0.6 * 0.5 * cb,  alpha 0.4 + 0.5 * 0.6 = 0.7
//! Tolerance 1e-6 is f32 rounding of the renderer's single-precision math only.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
const PAIRS: [([f64; 3], [f64; 3]); 2] = [([0.1, 0.3, 0.6], [0.6, 0.4, 0.3]), ([0.8, 0.4, 0.2], [0.3, 0.55, 0.75])];
const AS: f64 = 0.4;
const AB: f64 = 0.5;

struct P { opaque: [f64; 3], src_alpha: [f64; 3], backdrop_alpha: [f64; 4] }

/// Backdrop solid (alpha `ab`) under a source solid (alpha `a_s`) in `mode`; the centre pixel, rgba.
fn px(mode: &str, cb: [f64; 3], ab: f64, cs: [f64; 3], a_s: f64, transparent: bool) -> [f32; 4] {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":16,"height":16,"fps":25}));
    let t1 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_clip", json!({"track_id":t1,"source_type":"solid","color":[cb[0],cb[1],cb[2],ab],"start":0,"duration":1}));
    let top = c(&mut s, "add_clip", json!({"track_id":t2,"source_type":"solid","color":[cs[0],cs[1],cs[2],a_s],"start":0,"duration":1}))["clip_id"].clone();
    c(&mut s, "set_blend", json!({"clip_id":top,"mode":mode}));
    let f = kimo_core::render::render_frame_opts(&s.project, 0.5, &s.cache, transparent).unwrap();
    let i = (8 * f.w + 8) * 4; [f.d[i], f.d[i + 1], f.d[i + 2], f.d[i + 3]]
}

fn near(what: &str, got: [f32; 4], want: &[f64]) {
    for (j, w) in want.iter().enumerate() { assert!((got[j] as f64 - w).abs() <= 1e-6, "{what}: channel {j} got {} want {w} (got {got:?} want {want:?})", got[j]); }
}

fn opaque(mode: &str) { for (k, (cb, cs)) in PAIRS.iter().enumerate() { let g = px(mode, *cb, 1.0, *cs, 1.0, false); near(&format!("{mode} opaque pair {k}"), g, &[want(mode, k).opaque.as_slice(), &[1.0f64][..]].concat()); } }
fn src_alpha(mode: &str) { for (k, (cb, cs)) in PAIRS.iter().enumerate() { let g = px(mode, *cb, 1.0, *cs, AS, false); near(&format!("{mode} source alpha 0.4 pair {k}"), g, &[want(mode, k).src_alpha.as_slice(), &[1.0f64][..]].concat()); } }
fn backdrop_alpha(mode: &str) { for (k, (cb, cs)) in PAIRS.iter().enumerate() { let g = px(mode, *cb, AB, *cs, AS, true); near(&format!("{mode} backdrop alpha 0.5 + source alpha 0.4 pair {k}"), g, &want(mode, k).backdrop_alpha); } }

fn want(mode: &str, k: usize) -> &'static P { &WANT.iter().find(|(m, _)| *m == mode).unwrap_or_else(|| panic!("no table row {mode}")).1[k] }

macro_rules! modes { ($($m:ident),*) => { $(mod $m {
    #[test] fn opaque() { super::opaque(stringify!($m)) }
    #[test] fn source_alpha_0_4() { super::src_alpha(stringify!($m)) }
    #[test] fn backdrop_alpha_0_5_source_alpha_0_4() { super::backdrop_alpha(stringify!($m)) }
})* } }
modes!(darken, lighten, difference, exclusion, soft_light, hard_light, color_dodge, color_burn, linear_burn, linear_dodge, vivid_light, linear_light, pin_light, hue, saturation, color, luminosity);

/// The schema lists all 22 modes; an unknown one is still refused.
#[test]
fn schema_lists_every_mode_and_refuses_unknown() {
    let reg = kimo_core::commands::registry();
    let sb = reg.iter().find(|c| c.name == "set_blend").unwrap();
    let en: Vec<&str> = sb.schema.pointer("/properties/mode/enum").unwrap().as_array().unwrap().iter().map(|v| v.as_str().unwrap()).collect();
    let all = ["normal", "add", "multiply", "screen", "overlay", "darken", "lighten", "difference", "exclusion", "soft_light", "hard_light", "color_dodge", "color_burn", "linear_burn", "linear_dodge", "vivid_light", "linear_light", "pin_light", "hue", "saturation", "color", "luminosity"];
    assert_eq!(en, all);
    assert!(sb.description.len() < 150, "description {} chars", sb.description.len());
    let mut s = Session::new();
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[1,1,1],"start":0,"duration":1}))["clip_id"].clone();
    assert!(dispatch(&mut s, "set_blend", &json!({"clip_id":id,"mode":"bogus"})).is_err());
}

/// Hand-computed expectations. Per-mode B(cb, cs) by channel (r, g, b) for pair 0 / pair 1:
///   darken        min(cb,cs)  ->  [0.1, 0.3, 0.3] / [0.3, 0.4, 0.2]
///   lighten       max(cb,cs)  ->  [0.6, 0.4, 0.6] / [0.8, 0.55, 0.75]
///   difference    |cb-cs|  ->  [0.5, 0.1, 0.3] / [0.5, 0.15, 0.55]
///   exclusion     cb+cs-2*cb*cs  ->  [0.58, 0.46, 0.54] / [0.62, 0.51, 0.65]
///   soft_light    cs<=0.5: cb-(1-2cs)*cb*(1-cb); else cb+(2cs-1)*(D(cb)-cb), D = cb<=0.25 ? ((16cb-12)cb+4)cb : sqrt(cb)  ->  [0.1392, 0.258, 0.504] / [0.736, 0.423245553, 0.324]
///   hard_light    cs<=0.5: cb*2cs; else screen(cb, 2cs-1)  ->  [0.28, 0.24, 0.36] / [0.48, 0.46, 0.6]
///   color_dodge   cb==0: 0; cs>=1: 1; else min(1, cb/(1-cs))  ->  [0.25, 0.5, 0.857142857] / [1.0, 0.888888889, 0.8]
///   color_burn    cb==1: 1; cs<=0: 0; else 1-min(1,(1-cb)/cs)  ->  [0.0, 0.0, 0.0] / [0.333333333, 0.0, 0.0]
///   linear_burn   clamp(cb+cs-1, 0, 1)  ->  [0.0, 0.0, 0.0] / [0.1, 0.0, 0.0]
///   linear_dodge  clamp(cb+cs, 0, 1)  ->  [0.7, 0.7, 0.9] / [1.0, 0.95, 0.95]
///   vivid_light   cs<=0.5: color_burn(cb, 2cs); else color_dodge(cb, 2cs-1)  ->  [0.125, 0.125, 0.333333333] / [0.666666667, 0.444444444, 0.4]
///   linear_light  clamp(cb+2cs-1, 0, 1)  ->  [0.3, 0.1, 0.2] / [0.4, 0.5, 0.7]
///   pin_light     cs<=0.5: min(cb, 2cs); else max(cb, 2cs-1)  ->  [0.2, 0.3, 0.6] / [0.6, 0.4, 0.5]
///   hue           SetLum(SetSat(cs, Sat(cb)), Lum(cb))  ->  [0.524666667, 0.191333333, 0.0246666667] / [0.235333333, 0.568666667, 0.835333333]
///   saturation    SetLum(SetSat(cb, Sat(cs)), Lum(cb))  ->  [0.1692, 0.2892, 0.4692] / [0.7245, 0.4245, 0.2745]
///   color         SetLum(cs, Lum(cb))  ->  [0.424, 0.224, 0.124] / [0.301, 0.551, 0.751]
///   luminosity    SetLum(cb, Lum(cs))  ->  [0.276, 0.476, 0.776] / [0.799, 0.399, 0.199]
/// (Lum = 0.3r + 0.59g + 0.11b; SetLum/SetSat/ClipColor as in W3C Compositing §10.2.)
#[rustfmt::skip]
const WANT: &[(&str, [P; 2])] = &[
    ("darken", [P { opaque: [0.1, 0.3, 0.3], src_alpha: [0.1, 0.3, 0.48], backdrop_alpha: [0.17, 0.23, 0.3, 0.7] },
               P { opaque: [0.3, 0.4, 0.2], src_alpha: [0.6, 0.4, 0.2], backdrop_alpha: [0.36, 0.31, 0.25, 0.7] }]),
    ("lighten", [P { opaque: [0.6, 0.4, 0.6], src_alpha: [0.3, 0.34, 0.6], backdrop_alpha: [0.27, 0.25, 0.36, 0.7] },
               P { opaque: [0.8, 0.55, 0.75], src_alpha: [0.8, 0.46, 0.42], backdrop_alpha: [0.46, 0.34, 0.36, 0.7] }]),
    ("difference", [P { opaque: [0.5, 0.1, 0.3], src_alpha: [0.26, 0.22, 0.48], backdrop_alpha: [0.25, 0.19, 0.3, 0.7] },
               P { opaque: [0.5, 0.15, 0.55], src_alpha: [0.68, 0.3, 0.34], backdrop_alpha: [0.4, 0.26, 0.32, 0.7] }]),
    ("exclusion", [P { opaque: [0.58, 0.46, 0.54], src_alpha: [0.292, 0.364, 0.576], backdrop_alpha: [0.266, 0.262, 0.348, 0.7] },
               P { opaque: [0.62, 0.51, 0.65], src_alpha: [0.728, 0.444, 0.38], backdrop_alpha: [0.424, 0.332, 0.34, 0.7] }]),
    ("soft_light", [P { opaque: [0.1392, 0.258, 0.504], src_alpha: [0.11568, 0.2832, 0.5616], backdrop_alpha: [0.17784, 0.2216, 0.3408, 0.7] },
               P { opaque: [0.736, 0.423245553, 0.324], src_alpha: [0.7744, 0.409298221, 0.2496], backdrop_alpha: [0.4472, 0.314649111, 0.2748, 0.7] }]),
    ("hard_light", [P { opaque: [0.28, 0.24, 0.36], src_alpha: [0.172, 0.276, 0.504], backdrop_alpha: [0.206, 0.218, 0.312, 0.7] },
               P { opaque: [0.48, 0.46, 0.6], src_alpha: [0.672, 0.424, 0.36], backdrop_alpha: [0.396, 0.322, 0.33, 0.7] }]),
    ("color_dodge", [P { opaque: [0.25, 0.5, 0.857142857], src_alpha: [0.16, 0.38, 0.702857143], backdrop_alpha: [0.2, 0.27, 0.411428571, 0.7] },
               P { opaque: [1.0, 0.888888889, 0.8], src_alpha: [0.88, 0.595555556, 0.44], backdrop_alpha: [0.5, 0.407777778, 0.37, 0.7] }]),
    ("color_burn", [P { opaque: [0.0, 0.0, 0.0], src_alpha: [0.06, 0.18, 0.36], backdrop_alpha: [0.15, 0.17, 0.24, 0.7] },
               P { opaque: [0.333333333, 0.0, 0.0], src_alpha: [0.613333333, 0.24, 0.12], backdrop_alpha: [0.366666667, 0.23, 0.21, 0.7] }]),
    ("linear_burn", [P { opaque: [0.0, 0.0, 0.0], src_alpha: [0.06, 0.18, 0.36], backdrop_alpha: [0.15, 0.17, 0.24, 0.7] },
               P { opaque: [0.1, 0.0, 0.0], src_alpha: [0.52, 0.24, 0.12], backdrop_alpha: [0.32, 0.23, 0.21, 0.7] }]),
    ("linear_dodge", [P { opaque: [0.7, 0.7, 0.9], src_alpha: [0.34, 0.46, 0.72], backdrop_alpha: [0.29, 0.31, 0.42, 0.7] },
               P { opaque: [1.0, 0.95, 0.95], src_alpha: [0.88, 0.62, 0.5], backdrop_alpha: [0.5, 0.42, 0.4, 0.7] }]),
    ("vivid_light", [P { opaque: [0.125, 0.125, 0.333333333], src_alpha: [0.11, 0.23, 0.493333333], backdrop_alpha: [0.175, 0.195, 0.306666667, 0.7] },
               P { opaque: [0.666666667, 0.444444444, 0.4], src_alpha: [0.746666667, 0.417777778, 0.28], backdrop_alpha: [0.433333333, 0.318888889, 0.29, 0.7] }]),
    ("linear_light", [P { opaque: [0.3, 0.1, 0.2], src_alpha: [0.18, 0.22, 0.44], backdrop_alpha: [0.21, 0.19, 0.28, 0.7] },
               P { opaque: [0.4, 0.5, 0.7], src_alpha: [0.64, 0.44, 0.4], backdrop_alpha: [0.38, 0.33, 0.35, 0.7] }]),
    ("pin_light", [P { opaque: [0.2, 0.3, 0.6], src_alpha: [0.14, 0.3, 0.6], backdrop_alpha: [0.19, 0.23, 0.36, 0.7] },
               P { opaque: [0.6, 0.4, 0.5], src_alpha: [0.72, 0.4, 0.32], backdrop_alpha: [0.42, 0.31, 0.31, 0.7] }]),
    ("hue", [P { opaque: [0.524666667, 0.191333333, 0.0246666667], src_alpha: [0.269866667, 0.256533333, 0.369866667], backdrop_alpha: [0.254933333, 0.208266667, 0.244933333, 0.7] },
               P { opaque: [0.235333333, 0.568666667, 0.835333333], src_alpha: [0.574133333, 0.467466667, 0.454133333], backdrop_alpha: [0.347066667, 0.343733333, 0.377066667, 0.7] }]),
    ("saturation", [P { opaque: [0.1692, 0.2892, 0.4692], src_alpha: [0.12768, 0.29568, 0.54768], backdrop_alpha: [0.18384, 0.22784, 0.33384, 0.7] },
               P { opaque: [0.7245, 0.4245, 0.2745], src_alpha: [0.7698, 0.4098, 0.2298], backdrop_alpha: [0.4449, 0.3149, 0.2649, 0.7] }]),
    ("color", [P { opaque: [0.424, 0.224, 0.124], src_alpha: [0.2296, 0.2696, 0.4096], backdrop_alpha: [0.2348, 0.2148, 0.2648, 0.7] },
               P { opaque: [0.301, 0.551, 0.751], src_alpha: [0.6004, 0.4604, 0.4204], backdrop_alpha: [0.3602, 0.3402, 0.3602, 0.7] }]),
    ("luminosity", [P { opaque: [0.276, 0.476, 0.776], src_alpha: [0.1704, 0.3704, 0.6704], backdrop_alpha: [0.2052, 0.2652, 0.3952, 0.7] },
               P { opaque: [0.799, 0.399, 0.199], src_alpha: [0.7996, 0.3996, 0.1996], backdrop_alpha: [0.4598, 0.3098, 0.2498, 0.7] }]),
];
