//! Glow mode 1 (deep glow), lens_flare mode 1 (source-linked, occlusion-aware) and light_wrap linear (K2: AE Glow / Deep Glow / Optical Flares).
//! Every expected value is computed here from the documented formulas, independently of the crate's own kernel helpers.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::keyframe::Prop;
use kimo_core::render;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn tr(s: &mut Session) -> Value { c(s, "add_track", json!({}))["track_id"].clone() }
fn params(kv: &[(&str, f64)]) -> BTreeMap<String, Prop> { kv.iter().map(|(k, v)| (k.to_string(), Prop::new(*v))).collect() }

// ---------- deep glow: spec kernel, written out ----------
fn gauss1d(s: f64) -> Vec<f64> { let r = (3.0 * s).ceil() as i64 + 1; let g: Vec<f64> = (-r..=r).map(|i| (-((i * i) as f64) / (2.0 * s * s)).exp()).collect(); let z: f64 = g.iter().sum(); g.iter().map(|v| v / z).collect() }
fn comps(falloff: u32, radius: f64, n: usize) -> Vec<(f64, f64)> {
    if falloff == 1 { let d = |s: f64| s * s * s * (-s * s / 2.0).exp(); let ss: Vec<f64> = (0..5).map(|k| 2f64.powi(k - 2)).collect(); let z: f64 = ss.iter().map(|&s| d(s)).sum(); return ss.iter().map(|&s| (s * radius, d(s) / z)).collect(); }
    (0..n).map(|k| (radius * 2f64.powi(k as i32) / 2f64.powi(n as i32 - 1), 1.0 / n as f64)).collect()
}
fn kernel(cs: &[(f64, f64)], dx: i64, dy: i64) -> f64 { cs.iter().map(|&(s, w)| { let g = gauss1d(s); let r = ((g.len() - 1) / 2) as i64; if dx.abs() > r || dy.abs() > r { 0.0 } else { w * g[(dx + r) as usize] * g[(dy + r) as usize] } }).sum() }

fn impulse(n: usize) -> Frame { let mut f = Frame { w: n, h: n, d: vec![0.0; n * n * 4] }; for i in 0..n * n { f.d[i * 4 + 3] = 1.0; } let m = (n / 2) * n + n / 2; for j in 0..3 { f.d[m * 4 + j] = 1.0; } f }

fn check_impulse(falloff: u32, radius: f64, iters: usize, n: usize) {
    // linear 0, hard threshold 0.5 (knee 0): the white impulse's bright pass is 0.5 per channel; amount 1, exposure 0 -> G = 0.5 * K.
    let mut f = impulse(n); let pr = params(&[("mode", 1.0), ("falloff", falloff as f64), ("radius", radius), ("iterations", iters as f64), ("threshold", 0.5), ("knee", 0.0), ("amount", 1.0), ("exposure", 0.0), ("linear", 0.0)]);
    kimo_core::effects::apply("glow", &pr, &BTreeMap::new(), &mut f, 0.0, 0).unwrap();
    let cs = comps(falloff, radius, iters); let m = (n / 2) as i64; let kmax = kernel(&cs, 0, 0); let (mut worst, mut wat, mut energy) = (0f64, (0, 0), 0f64);
    for y in 0..n as i64 { for x in 0..n as i64 { let (dx, dy) = (x - m, y - m); let got = f.d[((y * n as i64 + x) * 4) as usize] as f64 - if dx == 0 && dy == 0 { 1.0 } else { 0.0 }; energy += got;
        let want = 0.5 * kernel(&cs, dx, dy); if want > 1e-3 * 0.5 * kmax { let rel = (got - want).abs() / want; if rel > worst { worst = rel; wat = (dx, dy); } } else { assert!((got - want).abs() <= 1e-6, "falloff {falloff}: tail ({dx},{dy}) got {got} want {want}"); } } }
    println!("glow falloff={falloff} radius={radius} iters={iters}: max rel err {worst:.3e} at {wat:?}, energy {energy:.6} (want 0.5)");
    assert!(worst <= 1e-3, "falloff {falloff}: kernel rel err {worst} at {wat:?} > 1e-3");
    assert!((energy - 0.5).abs() <= 0.5e-3, "falloff {falloff}: energy {energy} != 0.5");
    // 45 degrees vs the axis at equal radius (lattice points 5:(5,0)/(3,4), 10:(10,0)/(6,8), 13:(13,0)/(5,12))
    let at = |dx: i64, dy: i64| f.d[(((m + dy) * n as i64 + m + dx) * 4) as usize] as f64;
    for (a, b) in [((5, 0), (3, 4)), ((10, 0), (6, 8)), ((13, 0), (5, 12))] { let (va, vb) = (at(a.0, a.1), at(b.0, b.1)); if va > 1e-6 { assert!((va - vb).abs() / va <= 0.01, "falloff {falloff}: axis {a:?} {va} vs diagonal {b:?} {vb}"); } }
}

#[test]
fn deep_glow_impulse_response_sum_of_gaussians() { check_impulse(0, 6.0, 3, 72); }

#[test]
fn deep_glow_impulse_response_exponential() {
    check_impulse(1, 3.0, 5, 128);
    // reported, not gated: how close the 5-Gaussian mixture is to the ideal exp(-r/R) / (2 pi R^2) along the axis, R = 3
    let cs = comps(1, 3.0, 5); for r in [0i64, 3, 6, 12] { let ideal = (-(r as f64) / 3.0).exp() / (2.0 * std::f64::consts::PI * 9.0); println!("exp mixture r={r}: {:.4e} vs ideal {:.4e}", kernel(&cs, r, 0), ideal); }
}

#[test]
fn deep_glow_tool_path_and_mode0_unchanged() {
    let scene = |fx: Option<Value>| { let mut s = Session::new(); c(&mut s, "project_set", json!({"width":64,"height":48,"fps":25})); let t = tr(&mut s);
        c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[0.1,0.1,0.1],"start":0,"duration":2})); let t2 = tr(&mut s);
        let id = c(&mut s, "add_shape", json!({"track_id":t2,"shape":"rect","width":6,"height":6,"fill":"#ffffff","start":0,"duration":2}))["clip_id"].clone();
        if let Some(p) = fx { c(&mut s, "add_effect", json!({"clip_id":id,"kind":"glow","params":p})); } render::render_frame(&s.project, 0.5, &render::Cache::new()).unwrap().d };
    let (a, b) = (scene(Some(json!({}))), scene(Some(json!({"mode":0.0})))); assert!(a == b, "glow mode 0 must be bit-identical to no mode");
    let d = scene(Some(json!({"mode":1.0,"radius":4.0,"threshold":0.5}))); let plain = scene(None); assert!(d != plain && d != a, "deep glow changed nothing");
}

// ---------- lens flare v2 ----------
const W: usize = 256; const H: usize = 160;
fn flare_scene(src_keys: bool, occ_x: Option<f64>, use_occ: bool) -> (Session, Value) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":W,"height":H,"fps":25})); let t0 = tr(&mut s);
    c(&mut s, "add_clip", json!({"track_id":t0,"source_type":"solid","color":[0.0,0.0,0.0],"start":0,"duration":2}));
    let mut strings = serde_json::Map::new();
    if let Some(ox) = occ_x { let t = tr(&mut s); let id = c(&mut s, "add_shape", json!({"track_id":t,"shape":"rect","width":W,"height":H,"fill":"#000000","start":0,"duration":2}))["clip_id"].clone();
        c(&mut s, "set_property", json!({"clip_id":id,"property":"x","value":ox})); if use_occ { strings.insert("occluder".into(), id); } }
    if src_keys { let t = tr(&mut s); let id = c(&mut s, "add_shape", json!({"track_id":t,"shape":"rect","width":4,"height":4,"fill":"#ffffff","start":0,"duration":2}))["clip_id"].clone();
        c(&mut s, "set_property", json!({"clip_id":id,"property":"opacity","value":0.0}));
        c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"x","time":0.0,"value":0.0,"ease":"linear"})); c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"x","time":2.0,"value":50.0,"ease":"linear"}));
        c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"y","time":0.0,"value":0.0,"ease":"linear"})); c(&mut s, "set_keyframe", json!({"clip_id":id,"property":"y","time":2.0,"value":-20.0,"ease":"linear"}));
        strings.insert("source_clip".into(), id); }
    let ta = tr(&mut s); let adj = c(&mut s, "add_adjustment_layer", json!({"track_id":ta,"start":0,"duration":2}))["clip_id"].clone();
    let p = json!({"mode":1.0,"x":0.5,"y":0.5,"brightness":1.0,"halo":0.3,"halo_size":0.05,"ring":0.0,"ghosts":0.0,"streak":0.0,"starburst":0.0,"source_r":10.0});
    c(&mut s, "add_effect", json!({"clip_id":adj,"kind":"lens_flare","params":p,"strings":Value::Object(strings)})); (s, adj)
}
fn centroid(f: &Frame) -> (f64, f64, f64) { let (mut sx, mut sy, mut e) = (0f64, 0f64, 0f64); for y in 0..f.h { for x in 0..f.w { let v = f.d[(y * f.w + x) * 4] as f64; sx += v * x as f64; sy += v * y as f64; e += v; } } (sx / e, sy / e, e) }

#[test]
fn flare_follows_its_source_clip() {
    let (s, _) = flare_scene(true, None, false);
    for t in [0.0, 0.5, 1.3, 1.9] { let f = render::render_frame(&s.project, t, &render::Cache::new()).unwrap(); let (cx, cy, _) = centroid(&f);
        let (wx, wy) = (0.5 * W as f64 + 25.0 * t, 0.5 * H as f64 - 10.0 * t); let err = ((cx - wx).powi(2) + (cy - wy).powi(2)).sqrt();
        println!("t={t}: flare centre ({cx:.4}, {cy:.4}) source ({wx:.4}, {wy:.4}) err {err:.4} px"); assert!(err <= 0.1, "t={t}: flare centre off its source by {err} px"); }
}

fn disc_area_cover(alpha: &[f32], sx: f64, sy: f64, r: f64) -> f64 {
    // exact-enough area integral: every pixel (centre (i, j), square [i-.5, i+.5]) contributes alpha * area(square ∩ disc), 64x64 subsamples
    let (mut cov, n) = (0f64, 64); for j in (sy - r).floor() as i64 - 1..=(sy + r).ceil() as i64 + 1 { for i in (sx - r).floor() as i64 - 1..=(sx + r).ceil() as i64 + 1 {
        let a = if i < 0 || j < 0 || i >= W as i64 || j >= H as i64 { 0.0 } else { alpha[j as usize * W + i as usize] as f64 }; if a == 0.0 { continue; }
        let mut k = 0; for v in 0..n { for u in 0..n { let (x, y) = (i as f64 - 0.5 + (u as f64 + 0.5) / n as f64, j as f64 - 0.5 + (v as f64 + 0.5) / n as f64); if (x - sx).powi(2) + (y - sy).powi(2) <= r * r { k += 1; } } }
        cov += a * k as f64 / (n * n) as f64; } } cov / (std::f64::consts::PI * r * r)
}

#[test]
fn flare_dims_with_the_occluded_area_of_its_source() {
    let (sx, sy, r) = (0.5 * W as f64, 0.5 * H as f64, 10.0);
    for d in [-6.0, -3.0, 0.0, 2.5, 6.0] { // the occluder covers everything left of x = sx + d (full-frame rect shifted by -W/2 + d)
        let ox = -(W as f64) / 2.0 + d;
        let (on, _) = flare_scene(false, Some(ox), true); let (off, _) = flare_scene(false, Some(ox), false);
        let e_on = centroid(&render::render_frame(&on.project, 0.5, &render::Cache::new()).unwrap()).2; let e_off = centroid(&render::render_frame(&off.project, 0.5, &render::Cache::new()).unwrap()).2;
        let mut solo = Session::new(); c(&mut solo, "project_set", json!({"width":W,"height":H,"fps":25})); let t = tr(&mut solo);
        let id = c(&mut solo, "add_shape", json!({"track_id":t,"shape":"rect","width":W,"height":H,"fill":"#000000","start":0,"duration":2}))["clip_id"].clone(); c(&mut solo, "set_property", json!({"clip_id":id,"property":"x","value":ox}));
        let fr = render::render_frame_opts(&solo.project, 0.5, &render::Cache::new(), true).unwrap(); let alpha: Vec<f32> = (0..W * H).map(|i| fr.d[i * 4 + 3]).collect();
        let truth = 1.0 - disc_area_cover(&alpha, sx, sy, r); let got = e_on / e_off;
        println!("occluder edge at d={d}: visible measured {got:.4} vs area truth {truth:.4} (diff {:+.4})", got - truth);
        assert!((got - truth).abs() <= 0.02, "d={d}: flare visibility {got} vs occluded-area truth {truth}"); }
}

#[test]
fn flare_mode0_unchanged_and_refusals() {
    let r0 = |p: Value| { let mut s = Session::new(); c(&mut s, "project_set", json!({"width":96,"height":64,"fps":25})); let t = tr(&mut s);
        let id = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[0.0,0.0,0.0],"start":0,"duration":1}))["clip_id"].clone();
        c(&mut s, "add_effect", json!({"clip_id":id,"kind":"lens_flare","params":p})); render::render_frame(&s.project, 0.5, &render::Cache::new()).unwrap().d };
    assert!(r0(json!({})) == r0(json!({"mode":0.0})), "lens_flare mode 0 must be bit-identical to no mode");
    for st in [json!({"source_clip":"nope"}), json!({"occluder":"nope"}), json!({"source_light":"nope"})] {
        let (mut s, _) = flare_scene(false, None, false); let id = s.project.tracks[0].clips[0].id.clone();
        let r = dispatch(&mut s, "add_effect", &json!({"clip_id":id,"kind":"lens_flare","params":{"mode":1.0},"strings":st}));
        let rendered = r.is_ok() && render::render_frame(&s.project, 0.5, &render::Cache::new()).is_ok();
        assert!(!rendered, "{st}: an unknown source/occluder id must fail (at add or at render), not draw silently"); }
}

// ---------- light wrap, linear ----------
fn box3_step(alpha_row: &[f64], width: usize) -> Vec<f64> {
    let q = (width / 2).max(1) as i64; let n = alpha_row.len() as i64; let mut v: Vec<f64> = alpha_row.iter().map(|a| 1.0 - a).collect();
    for _ in 0..3 { v = (0..n).map(|x| (-q..=q).map(|o| v[(x + o).clamp(0, n - 1) as usize]).sum::<f64>() / (2 * q + 1) as f64).collect(); } v
}
fn dec(v: f64) -> f64 { if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) } }
fn enc(v: f64) -> f64 { if v <= 0.0031308 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 } }

#[test]
fn light_wrap_step_edge_matches_blur_of_one_minus_alpha() {
    let (w, h, wd) = (128usize, 32usize, 12.0);
    let mk = |linear: Option<f64>, with_bg: bool, with_fx: bool| { let mut s = Session::new(); c(&mut s, "project_set", json!({"width":w,"height":h,"fps":25}));
        if with_bg { let t = tr(&mut s); c(&mut s, "add_clip", json!({"track_id":t,"source_type":"solid","color":[0.2,0.5,0.8],"start":0,"duration":1})); }
        let t = tr(&mut s); let id = c(&mut s, "add_shape", json!({"track_id":t,"shape":"rect","width":w,"height":h * 2,"fill":"#808080","start":0,"duration":1}))["clip_id"].clone();
        c(&mut s, "set_property", json!({"clip_id":id,"property":"x","value":(w as f64) / 2.0}));
        if with_fx { let mut p = json!({"width":wd,"intensity":1.0,"blur":8.0,"desaturate":0.0}); if let Some(l) = linear { p["linear"] = json!(l); } c(&mut s, "add_effect", json!({"clip_id":id,"kind":"light_wrap","params":p})); }
        render::render_frame_opts(&s.project, 0.5, &render::Cache::new(), !with_bg).unwrap() };
    let lay = mk(None, false, false); let row: Vec<f64> = (0..w).map(|x| lay.d[((h / 2) * w + x) * 4 + 3] as f64).collect(); let edge = box3_step(&row, wd as usize);
    let base: Vec<f64> = (0..w).map(|x| lay.d[((h / 2) * w + x) * 4] as f64).collect(); let bd = [0.2f64, 0.5, 0.8];
    assert!(mk(None, true, true).d == mk(Some(0.0), true, true).d, "light_wrap linear 0 must be bit-identical to no linear");
    for (lin, name) in [(0.0, "display"), (1.0, "linear")] {
        let f = mk(Some(lin), true, true); let (mut worst, mut n) = (0f64, 0);
        for x in 0..w { if row[x] < 1.0 || edge[x].clamp(0.0, 1.0) <= 0.0 { continue; } n += 1;
            for j in 0..3 { let e = edge[x].clamp(0.0, 1.0); let want = if lin > 0.5 { enc(dec(base[x]) + dec(bd[j]) * e) } else { base[x] + bd[j] * e };
                let got = f.d[((h / 2) * w + x) * 4 + j] as f64; worst = worst.max((got - want).abs()); } }
        println!("light_wrap {name}: {n} wrapped columns, max |got - blur(1-a) formula| = {worst:.2e}"); assert!(n >= 6, "{name}: no wrapped columns"); assert!(worst <= 2e-5, "{name}: wrap off the step-edge formula by {worst}"); }
}
