//! hdr_zones (Resolve HDR palette zones): exposure in stops per luminance zone (dark, shadow, light, highlight, specular, plus a
//! global), zone boundaries in stops around 18 % grey, a soft falloff in stops, per-zone saturation. Applied in linear light as a
//! luminance gain (hue and RGB ratios kept); the resulting tone curve is forced monotone so a strong pull never inverts tones.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::{json, Value};

fn dup(f: &Frame) -> Frame { Frame { w: f.w, h: f.h, d: f.d.clone() } }
fn pr(kv: &[(&str, f64)]) -> Params { let mut p = Params::new(); for (k, v) in kv { p.insert(k.to_string(), Prop::new(*v)); } p }
fn run(p: &Params, f: &mut Frame) { kimo_core::color::apply3("hdr_zones", p, f, 0.0, 0).unwrap_or_else(|e| panic!("hdr_zones: {e}")); }
fn lin(v: f32) -> f64 { let v = v as f64; if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }
fn enc(l: f64) -> f32 { (if l <= 0.0031308 { l * 12.92 } else { 1.055 * l.powf(1.0 / 2.4) - 0.055 }) as f32 }
/// a display value whose linear luminance sits `s` stops from 18 % grey
fn at_stops(s: f64) -> f32 { enc(0.18 * 2f64.powf(s)) }
fn stops(f: &Frame, i: usize) -> f64 { let y = 0.2126 * lin(f.d[i * 4]) + 0.7152 * lin(f.d[i * 4 + 1]) + 0.0722 * lin(f.d[i * 4 + 2]); (y / 0.18).log2() }
fn greys(s: &[f64]) -> Frame { Frame { w: s.len(), h: 1, d: s.iter().flat_map(|v| { let g = at_stops(*v); [g, g, g, 1.0] }).collect() } }

#[test]
fn hdr_zones_lifts_shadows_by_stops_and_leaves_highlights() {
    let pts = [-3.5, -2.5, 0.0, 1.0, 2.0];
    let mut f = greys(&pts); let src = dup(&f); run(&pr(&[("shadow", 1.0)]), &mut f);
    let d: Vec<f64> = (0..pts.len()).map(|i| stops(&f, i) - stops(&src, i)).collect();
    assert!((d[0] - 1.0).abs() < 0.1 && (d[1] - 1.0).abs() < 0.1, "shadow +1 must lift deep shadows by one stop: {d:?}");
    assert!(d[2].abs() < 0.02 && d[3].abs() < 0.02 && d[4].abs() < 0.02, "mid grey and brighter must stay: {d:?}");
    // a coloured shadow keeps its hue: linear R:G:B ratios unchanged
    let mut c = Frame { w: 1, h: 1, d: vec![0.30, 0.12, 0.06, 1.0] }; run(&pr(&[("shadow", 1.0)]), &mut c);
    let ratio = |r: f32, g: f32| lin(r) / lin(g); assert!((ratio(c.d[0], c.d[1]) / ratio(0.30, 0.12) - 1.0).abs() < 1e-3 && (ratio(c.d[2], c.d[1]) / ratio(0.06, 0.12) - 1.0).abs() < 1e-3, "hue kept: {:?}", c.d);
    assert!(lin(c.d[1]) > 1.8 * lin(0.12), "and it is lifted: {:?}", c.d);
}

#[test]
fn hdr_zones_pulls_highlights_and_the_falloff_controls_the_spread() {
    let mut f = greys(&[2.4, 0.0, -2.0]); let src = dup(&f); run(&pr(&[("highlight", -1.0)]), &mut f);
    let d0 = stops(&f, 0) - stops(&src, 0); assert!((d0 + 1.0).abs() < 0.05, "highlight -1 must bring near-white down 1 stop: {d0}");
    assert!((stops(&f, 1) - stops(&src, 1)).abs() < 0.01 && (stops(&f, 2) - stops(&src, 2)).abs() < 0.01, "mids and shadows stay");
    // -1.5 stops cannot fit a 1-stop falloff without inverting tones: the falloff widens into the mids (slope kept >= 0.1), the zone still gets its full pull
    let mut f = greys(&[2.4, 0.0, -2.0]); run(&pr(&[("highlight", -1.5)]), &mut f);
    let d0 = stops(&f, 0) - stops(&src, 0); assert!((d0 + 1.5).abs() < 0.05, "highlight -1.5 must bring near-white down 1.5 stops: {d0}");
    let dm = stops(&f, 1) - stops(&src, 1); assert!(dm < 0.0 && dm > -0.15, "the widened falloff only grazes mid grey: {dm}"); assert!((stops(&f, 2) - stops(&src, 2)).abs() < 0.01, "shadows stay");
    // light zone (boundary +0.5 stops): mid grey sits 0.5 stops below; a wide falloff reaches it, a tight one does not
    let mv = |fall: f64| { let mut g = greys(&[0.0]); run(&pr(&[("light", 1.0), ("falloff", fall)]), &mut g); stops(&g, 0) };
    assert!(mv(0.5).abs() < 0.01, "tight falloff: mid grey untouched, moved {}", mv(0.5));
    assert!(mv(3.0) > 0.15, "wide falloff spreads the light zone into the mids: moved {}", mv(3.0));
    // highlight saturation: a saturated highlight desaturates, a saturated shadow does not
    let mut s = Frame { w: 2, h: 1, d: vec![1.0, 0.75, 0.55, 1.0, 0.20, 0.08, 0.04, 1.0] }; let s0 = dup(&s); run(&pr(&[("highlight_sat", 0.3)]), &mut s);
    let spread = |f: &Frame, i: usize| f.d[i * 4] - f.d[i * 4 + 2]; assert!(spread(&s, 0) < 0.8 * spread(&s0, 0) && (spread(&s, 1) - spread(&s0, 1)).abs() < 1e-3, "highlight_sat: {:?}", s.d);
}

#[test]
fn hdr_zones_never_inverts_tones_and_zero_is_identity() {
    let ramp = Frame { w: 256, h: 1, d: (0..256).flat_map(|i| { let v = i as f32 / 255.0; [v, v, v, 1.0] }).collect() };
    let mut f = dup(&ramp); run(&pr(&[("highlight", -4.0), ("falloff", 0.2)]), &mut f);
    for i in 1..256 { assert!(f.d[i * 4] >= f.d[(i - 1) * 4] - 1e-6, "a strong pull with a tight falloff must not invert tones: in {} -> {}, in {} -> {}", ramp.d[(i - 1) * 4], f.d[(i - 1) * 4], ramp.d[i * 4], f.d[i * 4]); }
    assert!(f.d[255 * 4] < 0.75, "and white still comes down: {}", f.d[255 * 4]);
    let mut z = dup(&ramp); run(&pr(&[("global", 0.0), ("dark", 0.0)]), &mut z); assert_eq!(z.d, ramp.d, "all zones at 0 is bit-exact identity");
    let mut g = greys(&[-1.0, 0.0, 1.0]); let g0 = dup(&g); run(&pr(&[("global", 0.5)]), &mut g);
    for i in 0..3 { assert!((stops(&g, i) - stops(&g0, i) - 0.5).abs() < 0.01, "global +0.5 moves every zone by half a stop"); }
}

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
#[test]
fn hdr_zones_is_a_listed_effect_grade_node_and_refuses_bad_values() {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":32,"height":32,"fps":25})); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[0.05,0.05,0.05],"color2":[0.9,0.9,0.9],"angle":0,"start":0,"duration":2}))["clip_id"].clone();
    let before = kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d;
    c(&mut s, "add_effect", json!({"clip_id":id,"kind":"hdr_zones","params":{"shadow":1.0,"highlight":-1.0}}));
    let after = kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d;
    assert!(before.iter().zip(&after).any(|(a, b)| (a - b).abs() > 1e-2), "hdr_zones must change the render");
    c(&mut s, "add_node", json!({"clip_id":id,"node_id":"zones","kind":"grade","effect_kind":"hdr_zones"}));
    let mut f = Frame { w: 1, h: 1, d: vec![0.5, 0.4, 0.3, 1.0] };
    for (k, v) in [("shadow", 9.0), ("falloff", 0.0), ("light_range", -20.0), ("highlight_sat", -1.0), ("global", f64::NAN)] {
        let e = kimo_core::color::apply3("hdr_zones", &pr(&[(k, v)]), &mut f, 0.0, 0).expect_err(&format!("{k} {v} must be refused")); assert!(e.contains(k), "'{e}' lacks '{k}'");
        assert_eq!(f.d, vec![0.5, 0.4, 0.3, 1.0], "a refused effect leaves the frame untouched"); }
}
