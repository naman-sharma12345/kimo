//! grain (Film Look Creator-style): grain size in px, softness, colour (per-channel independence), per-channel strength and a
//! shadows / mids / highlights response, at the legacy grain's strength (same standard deviation). With every new param at its
//! default the legacy per-pixel monochrome grain is reproduced bit for bit (golden frames stay put).
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn dup(f: &Frame) -> Frame { Frame { w: f.w, h: f.h, d: f.d.clone() } }
fn pr(kv: &[(&str, f64)]) -> Params { let mut p = Params::new(); for (k, v) in kv { p.insert(k.to_string(), Prop::new(*v)); } p }
fn grain(p: &Params, f: &mut Frame, seed: u64) { kimo_core::effects::apply("grain", p, &BTreeMap::new(), f, 0.0, seed).unwrap_or_else(|e| panic!("grain: {e}")); }
const N: usize = 96;
fn flat(v: [f32; 3]) -> Frame { Frame { w: N, h: N, d: (0..N * N).flat_map(|_| [v[0], v[1], v[2], 1.0]).collect() } }
fn noise(f: &Frame, base: &Frame, c: usize) -> Vec<f32> { (0..N * N).map(|i| f.d[i * 4 + c] - base.d[i * 4 + c]).collect() }
fn std(v: &[f32]) -> f32 { let m = v.iter().sum::<f32>() / v.len() as f32; (v.iter().map(|a| (a - m).powi(2)).sum::<f32>() / v.len() as f32).sqrt() }
fn corr(a: &[f32], b: &[f32]) -> f32 { let (ma, mb) = (a.iter().sum::<f32>() / a.len() as f32, b.iter().sum::<f32>() / b.len() as f32);
    let (mut ab, mut aa, mut bb) = (0.0, 0.0, 0.0); for (x, y) in a.iter().zip(b) { ab += (x - ma) * (y - mb); aa += (x - ma).powi(2); bb += (y - mb).powi(2); } ab / (aa * bb).sqrt() }
/// correlation of the field with itself shifted by `lag` px horizontally
fn auto(v: &[f32], lag: usize) -> f32 { let (a, b): (Vec<f32>, Vec<f32>) = (0..N).flat_map(|y| (0..N - lag).map(move |x| (y, x))).map(|(y, x)| (v[y * N + x], v[y * N + x + lag])).unzip(); corr(&a, &b) }

#[test]
fn grain_defaults_are_the_legacy_grain_bit_for_bit() {
    for (amount, seed) in [(0.08, 7u64), (0.25, 123456789)] {
        let mut f = flat([0.4, 0.5, 0.6]); grain(&pr(&[("amount", amount), ("size", 1.0), ("softness", 0.0), ("color", 0.0)]), &mut f, seed);
        // the legacy formula, verbatim
        let mut g = flat([0.4, 0.5, 0.6]); let a = amount as f32; let mut s = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        for i in 0..N * N { s = s.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); let n = ((s >> 40) as f32 / 16777216.0 - 0.5) * a; for j in 0..3 { g.d[i * 4 + j] += n; } }
        assert_eq!(f.d, g.d, "default grain must be the legacy grain exactly (amount {amount}, seed {seed})");
    }
}

#[test]
fn grain_size_softness_and_colour_shape_the_grain_at_the_same_strength() {
    let base = flat([0.4, 0.5, 0.6]);
    let mut legacy = dup(&base); grain(&pr(&[("amount", 0.2)]), &mut legacy, 3); let l = noise(&legacy, &base, 1);
    let mut big = dup(&base); grain(&pr(&[("amount", 0.2), ("size", 4.0)]), &mut big, 3); let b = noise(&big, &base, 1);
    assert!(auto(&l, 2).abs() < 0.08, "legacy grain is per pixel: lag-2 correlation {}", auto(&l, 2));
    assert!(auto(&b, 2) > 0.4, "size 4 makes grains about 4 px wide: lag-2 correlation {}", auto(&b, 2));
    assert!((std(&b) / std(&l) - 1.0).abs() < 0.15, "size keeps the strength: std {} vs legacy {}", std(&b), std(&l));
    let mut soft = dup(&base); grain(&pr(&[("amount", 0.2), ("size", 2.0), ("softness", 1.0)]), &mut soft, 3); let s1 = noise(&soft, &base, 1);
    let mut hard = dup(&base); grain(&pr(&[("amount", 0.2), ("size", 2.0)]), &mut hard, 3); let s0 = noise(&hard, &base, 1);
    assert!(auto(&s1, 1) > auto(&s0, 1) + 0.05, "softness rounds the grain off: lag-1 correlation {} vs {}", auto(&s1, 1), auto(&s0, 1));
    let mut col = dup(&base); grain(&pr(&[("amount", 0.2), ("color", 1.0)]), &mut col, 3);
    assert!(corr(&noise(&col, &base, 0), &noise(&col, &base, 1)).abs() < 0.15, "colour 1: independent grain per channel, R/G correlation {}", corr(&noise(&col, &base, 0), &noise(&col, &base, 1)));
    assert!(corr(&noise(&legacy, &base, 0), &noise(&legacy, &base, 1)) > 0.99, "colour 0 (legacy): monochrome grain");
    let mut other = dup(&base); grain(&pr(&[("amount", 0.2), ("size", 4.0)]), &mut other, 4); assert!(corr(&b, &noise(&other, &base, 1)) < 0.3, "each frame's seed gives new grain");
}

#[test]
fn grain_response_and_channel_strength() {
    // left half dark, right half bright
    let base = Frame { w: N, h: N, d: (0..N * N).flat_map(|i| { let v = if i % N < N / 2 { 0.08 } else { 0.85 }; [v, v, v, 1.0] }).collect() };
    let mut f = dup(&base); grain(&pr(&[("amount", 0.2), ("shadows", 0.0), ("mids", 1.0), ("highlights", 1.0)]), &mut f, 9); let n = noise(&f, &base, 1);
    let (dark, bright): (Vec<f32>, Vec<f32>) = ((0..N * N).filter(|i| i % N < N / 2).map(|i| n[i]).collect(), (0..N * N).filter(|i| i % N >= N / 2).map(|i| n[i]).collect());
    assert!(std(&dark) < 0.3 * std(&bright), "shadows 0 keeps grain out of the shadows: dark std {} bright std {}", std(&dark), std(&bright));
    let mut r0 = dup(&base); grain(&pr(&[("amount", 0.2), ("red", 0.0), ("blue", 2.0)]), &mut r0, 9);
    assert!((0..N * N).all(|i| r0.d[i * 4] == base.d[i * 4]), "red 0: the red channel carries no grain");
    assert!(std(&noise(&r0, &base, 2)) > 1.7 * std(&noise(&r0, &base, 1)), "blue 2: twice the grain in blue");
    let mut z = dup(&base); grain(&pr(&[("amount", 0.0), ("size", 3.0)]), &mut z, 9); assert_eq!(z.d, base.d, "amount 0 is identity");
}

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
#[test]
fn grain_new_params_are_listed_and_refused_out_of_range() {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":32,"height":32,"fps":25})); let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[0.5,0.5,0.5],"start":0,"duration":2}))["clip_id"].clone();
    c(&mut s, "add_effect", json!({"clip_id":id,"kind":"grain","params":{"amount":0.2,"size":3,"softness":0.5,"color":0.6,"shadows":0.5,"mids":1,"highlights":0.8,"red":1,"green":1,"blue":1.2}}));
    let l = c(&mut s, "list_effects", json!({})); let g = l.as_array().unwrap().iter().find(|e| e["kind"] == "grain").unwrap().clone();
    let names: Vec<&str> = g["params"].as_array().unwrap().iter().map(|p| p["name"].as_str().unwrap()).collect();
    for k in ["amount", "size", "softness", "color", "shadows", "mids", "highlights", "red", "green", "blue"] { assert!(names.contains(&k), "grain lists {k}: {names:?}"); }
    let mut f = flat([0.5, 0.4, 0.3]);
    for (k, v) in [("size", 0.5), ("size", 40.0), ("softness", 1.5), ("color", -0.1), ("shadows", 3.0), ("red", -1.0)] {
        let before = f.d.clone(); let e = kimo_core::effects::apply("grain", &pr(&[("amount", 0.1), (k, v)]), &BTreeMap::new(), &mut f, 0.0, 1).expect_err(&format!("{k} {v} must be refused")); assert!(e.contains(k), "'{e}' lacks '{k}'");
        assert_eq!(f.d, before, "a refused grain leaves the frame untouched"); }
}
