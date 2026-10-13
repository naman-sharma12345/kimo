//! V1c-D failing-first rows for the tools that will land on the plumbing (bg_grid, bg_orbs): they are RED until the tools exist, by design.
//! Boundaries follow the ARCH spec: min/max accepted, just outside rejected with the typed code, unknown property rejected, defaults echoed.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn base() -> (Session, String) {
    let mut s = Session::new();
    dispatch(&mut s, "project_set", &json!({"width":256,"height":256,"fps":25})).unwrap();
    let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    (s, t)
}
fn call(s: &mut Session, tool: &str, t: &str, extra: Value) -> Result<Value, String> {
    let mut a = json!({"track":t,"duration":2}); for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); } dispatch(s, tool, &a)
}
fn code(e: &str) -> &str { e.split(':').next().unwrap() }
#[test]
fn bg_grid_boundaries_and_echo() {
    let (mut s, t) = base();
    for (k, lo, hi) in [("spacing_px", 4.0, 2048.0), ("line_px", 0.5, 16.0), ("opacity_pattern", 0.0, 1.0)] {
        for v in [lo, hi] { call(&mut s, "bg_grid", &t, json!({k:v})).unwrap_or_else(|e| panic!("bg_grid {k}={v}: {e}")); }
        for v in [lo - 1e-6, hi + 1e-6] { let e = call(&mut s, "bg_grid", &t, json!({k:v})).unwrap_err(); assert_eq!(code(&e), "bg_param_out_of_range", "{k}={v}: {e}"); assert!(e.contains("fix_hint")); }
    }
    for bad in [json!({"drift":[4097,0]}), json!({"drift":[0,-4097]}), json!({"fade":{"cx":1,"cy":1,"radius":0.5,"softness":0.5}}), json!({"mode":"waves"})] {
        let e = call(&mut s, "bg_grid", &t, bad.clone()).unwrap_err(); assert_eq!(code(&e), "bg_param_out_of_range", "{bad}: {e}");
    }
    let e = call(&mut s, "bg_grid", &t, json!({"bg":"transparent"})).unwrap_err(); assert!(e.contains("fix_hint") && e.contains("blend"), "transparent needs blend add/screen: {e}");
    let e = call(&mut s, "bg_grid", &t, json!({"nope":1})).unwrap_err(); assert_eq!(code(&e), "bg_unknown_property");
    let r = call(&mut s, "bg_grid", &t, json!({})).unwrap();
    let p = &r["params"];
    assert_eq!((p["mode"].as_str(), p["spacing_px"].as_f64(), p["line_px"].as_f64(), p["opacity_pattern"].as_f64(), p["bg"].as_str()), (Some("lines"), Some(64.0), Some(1.0), Some(0.08), Some("solid")), "documented defaults echoed");
    assert_eq!(p["engine"], "expr"); assert!(r["estimate"]["expr_ops"].as_f64().unwrap() > 0.0); assert_eq!(r["applied"], false);
}
#[test]
fn bg_grid_numbers_in_expressions_come_from_one_formatter() {
    let (mut s, t) = base();
    let r = call(&mut s, "bg_grid", &t, json!({"spacing_px":37.3,"line_px":1.7,"drift":[3.3,-0.7]})).unwrap();
    let txt = serde_json::to_string(&r["expansion"]).unwrap();
    for tok in txt.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == 'e')).filter(|t| t.chars().any(|c| c.is_ascii_digit()) && t.contains('.')) {
        let f: f32 = tok.parse().unwrap_or_else(|_| panic!("number token {tok} parses")); assert_eq!(f.to_string().trim_end_matches(".0"), tok.trim_end_matches(".0"), "{tok} is the shortest f32 round trip");
    }
}
#[test]
fn bg_orbs_limits_and_order_independence() {
    let (mut s, t) = base();
    let orb = |x: f64| json!({"cx":x,"cy":100,"radius":30,"color":[1,0.5,0.2]});
    let many: Vec<Value> = (0..17).map(|i| orb(10.0 + i as f64)).collect();
    let e = call(&mut s, "bg_orbs", &t, json!({"orbs":many})).unwrap_err(); assert_eq!(code(&e), "bg_too_many_orbs", "{e}");
    let sixteen: Vec<Value> = (0..16).map(|i| orb(10.0 + i as f64)).collect();
    call(&mut s, "bg_orbs", &t, json!({"orbs":sixteen})).unwrap_or_else(|e| panic!("16 orbs ok: {e}"));
    let keyed = |n: usize| -> Vec<Value> { (0..n).map(|i| { let mut o = orb(10.0 + i as f64); o["keys"] = json!([{"t":0,"cx":1,"cy":1},{"t":1,"cx":50,"cy":50}]); o }).collect() };
    call(&mut s, "bg_orbs", &t, json!({"orbs":keyed(4)})).unwrap_or_else(|e| panic!("keys on 4 orbs ok: {e}"));
    let e = call(&mut s, "bg_orbs", &t, json!({"orbs":keyed(5)})).unwrap_err(); assert_eq!(code(&e), "bg_key_cap", "{e}");
    let mut both = orb(20.0); both["keys"] = json!([{"t":0,"cx":1,"cy":1}]); both["orbit"] = json!({"rx":10,"ry":10,"rate_hz":1,"phase":0});
    let e = call(&mut s, "bg_orbs", &t, json!({"orbs":[both]})).unwrap_err(); assert_eq!(code(&e), "bg_param_out_of_range", "orbit and keys are mutually exclusive: {e}");
    for bad in [json!({"radius":0.5}), json!({"radius":8193}), json!({"intensity":4.5}), json!({"falloff":"box"})] { let mut o = orb(20.0); for (k, v) in bad.as_object().unwrap() { o[k] = v.clone(); } let e = call(&mut s, "bg_orbs", &t, json!({"orbs":[o]})).unwrap_err(); assert_eq!(code(&e), "bg_param_out_of_range", "{bad}: {e}"); }
    // orb order is irrelevant: swapping two orbs renders bit-equal
    let (a, b) = (orb(60.0), { let mut o = orb(170.0); o["color"] = json!([0.1, 0.4, 1.0]); o });
    let render = |orbs: Vec<Value>| { let (mut s, t) = base(); call(&mut s, "bg_orbs", &t, json!({"orbs":orbs,"apply":true})).unwrap(); kimo_core::render::render_frame(&s.project, 1.0, &kimo_core::render::Cache::new()).unwrap().d.iter().map(|v| v.to_bits()).collect::<Vec<_>>() };
    assert_eq!(render(vec![a.clone(), b.clone()]), render(vec![b, a]), "orb order irrelevant");
}
/// bg_wave chunking: adjacent chunk clips share one boundary sample and cover the whole span, with no gap, overlap or dropped sample.
/// Contract asked of bg_wave: estimate.chunk_detail = [{clip, t0, t1, first_sample, last_sample}] in order, chunks <= 255 keys, last_sample of chunk k == first_sample of chunk k+1.
#[test]
fn bg_wave_chunks_share_boundary_samples_and_cover_everything() {
    let mut s = Session::new();
    dispatch(&mut s, "project_set", &json!({"width":256,"height":256,"fps":100})).unwrap();
    let n = 48000 * 3; let data: Vec<f64> = (0..n).map(|i| ((i as f64) * 0.01).sin() * 0.5).collect();
    let a = dispatch(&mut s, "import_samples", &json!({"rate":48000,"channels":1,"data":[data]})).unwrap();
    let ta = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let clip = dispatch(&mut s, "add_clip", &json!({"track_id":ta,"asset_id":a["asset_id"],"start":0,"duration":3})).unwrap()["clip_id"].as_str().unwrap().to_string();
    let tv = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    let args = |extra: Value| { let mut o = json!({"track":tv,"start":0,"duration":3,"source":"audio","audio":{"target":format!("clip:{clip}"),"start":0,"end":3,"hop_s":0.01,"chunk":true}}); for (k, v) in extra.as_object().unwrap() { o[k] = v.clone(); } o };
    let dry = dispatch(&mut s, "bg_wave", &args(json!({}))).unwrap();
    let detail = dry["estimate"]["chunk_detail"].as_array().expect("estimate.chunk_detail").clone();
    assert_eq!(detail.len(), 2, "300 samples at 255 keys per chunk = ceil(300/255) chunks"); assert_eq!(dry["estimate"]["chunks"], 2);
    assert_eq!(detail[0]["first_sample"], 0); assert_eq!(detail[0]["last_sample"], detail[1]["first_sample"], "shared boundary sample");
    assert_eq!(detail[1]["last_sample"], 299, "the final sample is not dropped");
    assert!(detail.iter().all(|d| d["last_sample"].as_i64().unwrap() - d["first_sample"].as_i64().unwrap() + 1 <= 255), "no chunk exceeds 255 keys");
    let covered: i64 = detail.iter().map(|d| d["last_sample"].as_i64().unwrap() - d["first_sample"].as_i64().unwrap() + 1).sum::<i64>() - (detail.len() as i64 - 1);
    assert_eq!(covered, 300, "every sample covered exactly once apart from the shared boundary");
    let r = dispatch(&mut s, "bg_wave", &args(json!({"apply":true}))).unwrap();
    let ids: Vec<String> = r["clip_ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(ids.len(), 2);
    let times: Vec<(f64, f64)> = ids.iter().map(|id| { let c = s.project.find_clip(id).unwrap(); (c.start, c.start + c.duration) }).collect();
    assert!((times[0].0 - 0.0).abs() < 1e-9 && (times[1].1 - 3.0).abs() < 1e-9, "chunks span the whole duration: {times:?}");
    assert!((times[0].1 - times[1].0).abs() < 1e-9, "back-to-back: no gap, no overlap: {times:?}");
    let d = dispatch(&mut s, "bg_wave", &args(json!({"audio":{"target":format!("clip:{clip}"),"start":0,"end":3,"hop_s":0.01,"strict_hop":true}}))).unwrap_err();
    assert!(d.starts_with("bg_key_cap") && d.contains("chunk"), "strict_hop without chunk above the cap is a typed error pointing at chunk:true: {d}");
}
