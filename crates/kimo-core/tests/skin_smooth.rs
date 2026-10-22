//! skin_smooth (Resolve Face Refinement-style skin smoothing): a soft skin-tone key, then frequency separation inside it.
//! The edge-aware base (self-guided filter, radius r) flattens blotches and blemishes, the finest band (pores) is put back by
//! `texture`, chroma blotches are evened by `color_even`; strong edges (eyes, lips, jaw) and non-skin colours are kept.
//! A catalog effect and a grade-node kind; put it under a tracked mask (track_mask / track_matte + set_effect_mask) for a face.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::{json, Value};

fn dup(f: &Frame) -> Frame { Frame { w: f.w, h: f.h, d: f.d.clone() } }
fn pr(kv: &[(&str, f64)]) -> Params { let mut p = Params::new(); for (k, v) in kv { p.insert(k.to_string(), Prop::new(*v)); } p }
fn run(p: &Params, f: &mut Frame) { kimo_core::color::apply3("skin_smooth", p, f, 0.0, 0).unwrap_or_else(|e| panic!("skin_smooth: {e}")); }
const N: usize = 128;
fn hash(i: usize) -> f32 { let mut x = (i as u64).wrapping_mul(0x9E3779B97F4A7C15) ^ 0x5851F42D4C957F2D; x ^= x >> 31; x = x.wrapping_mul(0xBF58476D1CE4E5B9); x ^= x >> 29; (x >> 40) as f32 / 16777216.0 - 0.5 }
/// blemish field: smooth bumps (sigma 3 px) of both signs; pores: per-pixel white texture
fn blem(x: usize, y: usize) -> f32 { let mut s = 0.0; for k in 0..40 { let (cx, cy) = ((hash(k * 3) + 0.5) * N as f32, (hash(k * 3 + 1) + 0.5) * N as f32); let a = if k % 2 == 0 { 1.0 } else { -0.8 };
    let d2 = (x as f32 - cx).powi(2) + (y as f32 - cy).powi(2); s += a * (-d2 / 18.0).exp(); } s }
fn pore(x: usize, y: usize) -> f32 { hash(1000 + y * N + x) }
const SKIN: [f32; 3] = [0.85, 0.62, 0.50]; const SHIRT: [f32; 3] = [0.15, 0.25, 0.60]; const EYE: [f32; 3] = [0.12, 0.08, 0.07];
fn eye(x: usize, y: usize) -> bool { let (dx, dy) = (x as f32 - 64.0, y as f32 - 40.0); dx * dx + dy * dy < 100.0 }
/// rows 0..96 skin (with a dark eye disk), rows 96.. a blue shirt; both carry the same blemishes (luma + a red blotch) and pores
fn face() -> Frame { let mut d = Vec::with_capacity(N * N * 4);
    for y in 0..N { for x in 0..N { let base = if y >= 96 { SHIRT } else if eye(x, y) { EYE } else { SKIN }; let (b, p) = (blem(x, y), pore(x, y));
        d.extend_from_slice(&[base[0] + 0.06 * b + 0.03 * b.max(0.0) + 0.05 * p, base[1] + 0.06 * b + 0.05 * p, base[2] + 0.06 * b + 0.05 * p, 1.0]); } }
    Frame { w: N, h: N, d } }
fn luma(f: &Frame, i: usize) -> f32 { 0.2126 * f.d[i * 4] + 0.7152 * f.d[i * 4 + 1] + 0.0722 * f.d[i * 4 + 2] }
/// regression of (out - clean) on the blemish and pore fields over clean skin pixels (away from the eye and the shirt)
fn kept(f: &Frame) -> (f32, f32) { let (mut bb, mut br, mut pp, mut prr) = (0.0, 0.0, 0.0, 0.0);
    for y in 6..88 { for x in 6..N - 6 { if (x as f32 - 64.0).powi(2) + (y as f32 - 40.0).powi(2) < 400.0 { continue; } let i = y * N + x;
        let r = luma(f, i) - (0.2126 * SKIN[0] + 0.7152 * SKIN[1] + 0.0722 * SKIN[2]); let (b, p) = (0.06 * blem(x, y), 0.05 * pore(x, y));
        bb += b * b; br += b * r; pp += p * p; prr += p * r; } }
    (br / bb, prr / pp) }

#[test]
fn skin_smooth_removes_blemishes_and_keeps_pore_texture() {
    let mut f = face(); let src = dup(&f); let (b0, p0) = kept(&src); assert!(b0 > 0.9 && p0 > 0.9, "fixture: {b0} {p0}");
    run(&pr(&[("amount", 1.0), ("radius", 45.0), ("texture", 0.6), ("color_even", 1.0)]), &mut f);
    let (b1, p1) = kept(&f);
    assert!(b1 < 0.35, "blemishes must be smoothed away: share of the blemish field left {b1}");
    assert!(p1 > 0.45, "pore texture must stay (texture 0.6): share of the pore field left {p1}");
    let mut flat = dup(&src); run(&pr(&[("amount", 1.0), ("radius", 45.0), ("texture", 0.0)]), &mut flat); let (_, p2) = kept(&flat);
    assert!(p2 < 0.6 * p1, "texture 0 must take the pores too (plastic look, by choice): {p2} vs {p1}");
    // the red blotches even out: Cr spread over clean skin shrinks
    let cr = |f: &Frame| { let v: Vec<f32> = (6..88).flat_map(|y| (6..N - 6).map(move |x| (x, y))).filter(|(x, y)| (*x as f32 - 64.0).powi(2) + (*y as f32 - 40.0).powi(2) >= 400.0)
        .map(|(x, y)| { let i = y * N + x; f.d[i * 4] - luma(f, i) }).collect(); let m = v.iter().sum::<f32>() / v.len() as f32; (v.iter().map(|a| (a - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt() };
    assert!(cr(&f) < 0.5 * cr(&src), "red blotches must even out: R-Y spread {} -> {}", cr(&src), cr(&f));
    for i in 0..N * N { assert_eq!(f.d[i * 4 + 3], 1.0, "alpha kept"); }
}

#[test]
fn skin_smooth_keeps_edges_and_leaves_non_skin_alone() {
    let mut f = face(); let src = dup(&f); run(&pr(&[("amount", 1.0), ("radius", 45.0)]), &mut f);
    // the eye's rim stays sharp: luma step across the edge (6 px out minus 6 px in, averaged around the eye)
    let step = |f: &Frame| { let mut s = 0.0; for k in 0..32 { let a = k as f32 / 32.0 * std::f32::consts::TAU; let at = |r: f32| { let (x, y) = ((64.0 + r * a.cos()).round() as usize, (40.0 + r * a.sin()).round() as usize); luma(f, y * N + x) }; s += at(13.0) - at(7.0); } s / 32.0 };
    assert!(step(&f) > 0.8 * step(&src), "edges (eye rim) must stay: step {} -> {}", step(&src), step(&f));
    // the blue shirt has the same blemishes but is not skin: untouched away from the boundary
    let moved = (110 * N..N * N).flat_map(|i| (0..3).map(move |j| (i, j))).map(|(i, j)| (f.d[i * 4 + j] - src.d[i * 4 + j]).abs()).fold(0.0, f32::max);
    assert!(moved < 0.01, "non-skin colour must not be smoothed: largest change on the shirt {moved}");
    // skin_only 0 smooths everything (for use under a face mask)
    let mut all = dup(&src); run(&pr(&[("amount", 1.0), ("radius", 45.0), ("skin_only", 0.0)]), &mut all);
    let moved_all = (110 * N..N * N).map(|i| (all.d[i * 4 + 1] - src.d[i * 4 + 1]).abs()).fold(0.0, f32::max); assert!(moved_all > 0.02, "skin_only 0 must smooth the shirt too: {moved_all}");
    let mut z = dup(&src); run(&pr(&[("amount", 0.0)]), &mut z); assert_eq!(z.d, src.d, "amount 0 is bit-exact identity");
    let mut m = dup(&src); run(&pr(&[("show_matte", 1.0)]), &mut m);
    assert!(m.d[(70 * N + 20) * 4] > 0.9 && m.d[(120 * N + 20) * 4] < 0.05, "show_matte shows the skin key: skin {} shirt {}", m.d[(70 * N + 20) * 4], m.d[(120 * N + 20) * 4]);
}

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
#[test]
fn skin_smooth_is_a_listed_effect_grade_node_and_refuses_bad_values() {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":32,"height":32,"fps":25})); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[0.85,0.62,0.5],"start":0,"duration":2}))["clip_id"].clone();
    c(&mut s, "add_effect", json!({"clip_id":id,"kind":"grain","params":{"amount":0.3}}));
    let before = kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d;
    c(&mut s, "add_effect", json!({"clip_id":id,"kind":"skin_smooth","params":{"amount":1.0,"skin_only":0}}));
    let after = kimo_core::render::render_frame(&s.project, 0.5, &s.cache).unwrap().d;
    assert!(before.iter().zip(&after).any(|(a, b)| (a - b).abs() > 1e-3), "skin_smooth must change a textured render");
    let l = c(&mut s, "list_effects", json!({})); assert!(l.as_array().unwrap().iter().any(|e| e["kind"] == "skin_smooth"), "listed");
    c(&mut s, "add_node", json!({"clip_id":id,"node_id":"skin","kind":"grade","effect_kind":"skin_smooth"}));
    let mut f = Frame { w: 1, h: 1, d: vec![0.5, 0.4, 0.3, 1.0] };
    for (k, v) in [("amount", 1.5), ("radius", 0.0), ("texture", -0.1), ("color_even", 2.0), ("skin_width", 0.0)] {
        let e = kimo_core::color::apply3("skin_smooth", &pr(&[(k, v)]), &mut f, 0.0, 0).expect_err(&format!("{k} {v} must be refused")); assert!(e.contains(k), "'{e}' lacks '{k}'");
        assert_eq!(f.d, vec![0.5, 0.4, 0.3, 1.0], "a refused effect leaves the frame untouched"); }
}
