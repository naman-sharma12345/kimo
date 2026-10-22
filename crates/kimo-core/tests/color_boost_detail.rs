//! Resolve primaries-bar Color Boost (vibrance) and Midtone Detail (local contrast) as grade effects.
//! color_boost: chroma *= max(0, 1 + amount*(1-min(S,1))^3), S = 2*|(Cb,Cr)| (Rec.709), luma kept: weak colour moves, strong colour stays.
//! midtone_detail: Y' = Y + amount * 4Y(1-Y) * (Y - gauss(Y)), sigma = radius*h/720, added equally to R,G,B so Cb/Cr are kept.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::{json, Value};

fn ycc(c: &[f32]) -> (f64, f64, f64) { let (r, g, b) = (c[0] as f64, c[1] as f64, c[2] as f64); let y = 0.2126 * r + 0.7152 * g + 0.0722 * b; (y, (b - y) / 1.8556, (r - y) / 1.5748) }
fn pr(kv: &[(&str, f64)]) -> Params { let mut p = Params::new(); for (k, v) in kv { p.insert(k.to_string(), Prop::new(*v)); } p }
fn run(kind: &str, p: &Params, f: &mut Frame) { kimo_core::color::apply3(kind, p, f, 0.0, 0).unwrap_or_else(|e| panic!("{kind}: {e}")); }

#[test]
fn color_boost_lifts_weak_colour_only() {
    let px: Vec<[f32; 4]> = vec![[0.5, 0.45, 0.42, 1.0], [0.9, 0.1, 0.1, 1.0], [0.5, 0.5, 0.5, 1.0], [0.3, 0.35, 0.5, 0.5]];
    let mut f = Frame { w: px.len(), h: 1, d: px.iter().flatten().copied().collect() }; run("color_boost", &pr(&[("amount", 0.8)]), &mut f);
    for (i, q) in px.iter().enumerate() {
        let (y0, cb0, cr0) = ycc(q); let (y1, cb1, cr1) = ycc(&f.d[i * 4..i * 4 + 4]); let rho = (cb0 * cb0 + cr0 * cr0).sqrt(); let s = (2.0 * rho).min(1.0);
        let k = (1.0 + 0.8 * (1.0 - s).powi(3)).max(0.0);
        assert!((y1 - y0).abs() < 1e-6, "px{i}: color_boost keeps luma {y0} -> {y1}");
        assert!((cb1 - cb0 * k).abs() < 1e-6 && (cr1 - cr0 * k).abs() < 1e-6, "px{i}: chroma x{k}: ({cb0},{cr0}) -> ({cb1},{cr1})");
        assert_eq!(f.d[i * 4 + 3], q[3], "alpha px{i}"); }
    let gain = |i: usize| { let (_, cb0, cr0) = ycc(&px[i]); let (_, cb1, cr1) = ycc(&f.d[i * 4..i * 4 + 4]); (cb1 * cb1 + cr1 * cr1).sqrt() / (cb0 * cb0 + cr0 * cr0).sqrt() };
    assert!(gain(0) > 1.5 && gain(1) < 1.01, "weak colour boosted (x{}) and strong red left alone (x{})", gain(0), gain(1));
    let mut g = Frame { w: px.len(), h: 1, d: px.iter().flatten().copied().collect() }; run("color_boost", &pr(&[("amount", 0.0)]), &mut g);
    assert_eq!(g.d, px.iter().flatten().copied().collect::<Vec<f32>>(), "amount 0 is bit-exact identity");
}

/// 64x32: left half a midtone grey 0.45, right half 0.55, plus a shadow strip (0.03 / 0.06) at the bottom; a blue tint everywhere.
fn dup(f: &Frame) -> Frame { Frame { w: f.w, h: f.h, d: f.d.clone() } }
fn edge_frame() -> Frame { let (w, h) = (64usize, 32usize); let mut d = vec![0f32; w * h * 4];
    for y in 0..h { for x in 0..w { let v = if y >= 24 { if x < 32 { 0.03 } else { 0.06 } } else if x < 32 { 0.45 } else { 0.55 }; let i = (y * w + x) * 4; d[i] = v * 0.95; d[i + 1] = v; d[i + 2] = v * 1.1; d[i + 3] = 1.0; } }
    Frame { w, h, d } }

#[test]
fn midtone_detail_adds_local_contrast_in_mids_not_shadows() {
    let src = edge_frame(); let mut f = dup(&src); run("midtone_detail", &pr(&[("amount", 1.0), ("radius", 40.0)]), &mut f);
    let at = |fr: &Frame, x: usize, y: usize| ycc(&fr.d[(y * fr.w + x) * 4..(y * fr.w + x) * 4 + 4]);
    // just left of the mid edge gets darker, just right gets brighter: the step is exaggerated
    let (l0, r0) = (at(&src, 30, 10).0, at(&src, 33, 10).0); let (l1, r1) = (at(&f, 30, 10).0, at(&f, 33, 10).0);
    assert!(l1 < l0 - 0.005 && r1 > r0 + 0.005, "mid edge contrast must grow: {l0}->{l1}, {r0}->{r1}");
    // the shadow edge, same relative step, moves far less (midtone weight 4Y(1-Y))
    let (sl0, sr0) = (at(&src, 30, 29).0, at(&src, 33, 29).0); let (sl1, sr1) = (at(&f, 30, 29).0, at(&f, 33, 29).0);
    assert!(((sr1 - sl1) - (sr0 - sl0)).abs() < 0.25 * ((r1 - l1) - (r0 - l0)).abs(), "shadows protected: shadow step {}->{}, mid step {}->{}", sr0 - sl0, sr1 - sl1, r0 - l0, r1 - l1);
    // chroma is untouched everywhere
    for i in 0..src.w * src.h { let (_, cb0, cr0) = ycc(&src.d[i * 4..i * 4 + 4]); let (_, cb1, cr1) = ycc(&f.d[i * 4..i * 4 + 4]); assert!((cb1 - cb0).abs() < 1e-5 && (cr1 - cr0).abs() < 1e-5, "px{i} chroma moved"); }
    // a flat area far from any edge is (nearly) unchanged
    assert!((at(&f, 2, 2).0 - at(&src, 2, 2).0).abs() < 2e-3, "flat area unchanged");
    let mut g = dup(&src); run("midtone_detail", &pr(&[("amount", 0.0)]), &mut g); assert_eq!(g.d, src.d, "amount 0 is bit-exact identity");
    let mut soft = dup(&src); run("midtone_detail", &pr(&[("amount", -1.0), ("radius", 40.0)]), &mut soft);
    assert!(at(&soft, 33, 10).0 - at(&soft, 30, 10).0 < r0 - l0, "negative amount softens the mid edge");
}

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
#[test]
fn boost_and_detail_are_listed_effects_and_render() {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":32,"height":16,"fps":25})); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[0.5,0.45,0.4],"color2":[0.3,0.4,0.6],"angle":90,"start":0,"duration":2}))["clip_id"].clone();
    let before = kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d;
    c(&mut s, "add_effect", json!({"clip_id":id,"kind":"color_boost","params":{"amount":0.6}}));
    c(&mut s, "add_effect", json!({"clip_id":id,"kind":"midtone_detail","params":{"amount":0.5,"radius":12}}));
    let after = kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d;
    assert!(before.iter().zip(&after).any(|(a, b)| (a - b).abs() > 1e-3), "color_boost + midtone_detail must change the render");
    // both are grade-node effects too (usable under a power window on a node)
    c(&mut s, "add_node", json!({"clip_id":id,"node_id":"boost","kind":"grade","effect_kind":"color_boost"}));
    let mut f = Frame { w: 1, h: 1, d: vec![0.5, 0.4, 0.3, 1.0] };
    let e = kimo_core::color::apply3("color_boost", &pr(&[("amount", 3.0)]), &mut f, 0.0, 0).expect_err("amount 3 must be refused"); assert!(e.contains("amount"), "'{e}' lacks 'amount'");
    assert_eq!(f.d, vec![0.5, 0.4, 0.3, 1.0], "a refused effect leaves the frame untouched");
    let e = kimo_core::color::apply3("midtone_detail", &pr(&[("radius", 0.0)]), &mut f, 0.0, 0).expect_err("radius 0 must be refused"); assert!(e.contains("radius"), "'{e}' lacks 'radius'");
}
