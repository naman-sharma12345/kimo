//! v6 eyes: lint_frame text_contrast over a media background. With decode_media: false the background is rendered with an
//! empty render cache, so a media layer draws black and white text reads 21:1 (the v5 output, kept byte for byte). With no
//! decode_media (the v6 fix) or decode_media: true the media frame is decoded first: white text over a #777777 PNG imported as media reads
//! (1 + 0.05) / (lin(119/255) + 0.05) = 4.478:1 at every pixel of its ink box. A solid layer reads the same both ways.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn f(v: &Value) -> f64 { v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")) }
fn lin(x: f64) -> f64 { if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) } }
fn strip(mut v: Value) -> String { if let Some(fr) = v["frames"].as_array_mut() { for x in fr { x.as_object_mut().unwrap().remove("elapsed_ms"); } } serde_json::to_string(&v).unwrap() }
const G: f64 = 119.0 / 255.0;

/// White 48 px text over a background layer: the png path imports it as media, otherwise a solid #777777.
fn scene(png: Option<&str>) -> Session {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 320, "height": 180, "fps": 25}));
    let tb = c(&mut s, "add_track", json!({}))["track_id"].clone();
    match png {
        Some(p) => { let a = c(&mut s, "import_media", json!({"path": p}))["asset_id"].clone(); c(&mut s, "add_clip", json!({"track_id": tb, "source_type": "media", "asset_id": a, "start": 0.0, "duration": 2.0})); }
        None => { c(&mut s, "add_clip", json!({"track_id": tb, "source_type": "solid", "color": [G, G, G], "start": 0.0, "duration": 2.0})); }
    }
    let tt = c(&mut s, "add_track", json!({}))["track_id"].clone();
    c(&mut s, "add_text", json!({"track_id": tt, "text": "HELLO", "start": 0.0, "duration": 2.0, "size": 48, "color": [1.0, 1.0, 1.0]}));
    s
}
fn grey_png() -> std::path::PathBuf {
    let mut s0 = Session::new(); c(&mut s0, "project_set", json!({"width": 320, "height": 180, "fps": 25}));
    let t0 = c(&mut s0, "add_track", json!({}))["track_id"].clone(); c(&mut s0, "add_clip", json!({"track_id": t0, "source_type": "solid", "color": [G, G, G], "start": 0.0, "duration": 2.0}));
    let png = std::env::temp_dir().join(format!("kimo_lfm_{}_grey.png", std::process::id())); c(&mut s0, "render_frame", json!({"time": 0.5, "path": png.to_str().unwrap(), "inline": "never"}));
    png
}
fn lint(s: &mut Session, extra: Value) -> Value {
    let mut a = json!({"time": 1.0, "checks": ["text_contrast"], "params": {"contrast_normal": 100, "contrast_large": 100}, "annotate": false});
    for (k, v) in extra.as_object().unwrap() { a[k] = v.clone(); }
    c(s, "lint_frame", a)
}
fn measured(r: &Value) -> Value {
    let fs: Vec<&Value> = r["frames"][0]["findings"].as_array().unwrap().iter().filter(|x| x["code"] == "text_contrast").collect();
    assert_eq!(fs.len(), 1, "{r}"); fs[0]["measured"].clone()
}

#[test] fn media_background_decoded_only_on_request() {
    let want = 1.05 / (lin(G) + 0.05);
    let png = grey_png(); let mut s = scene(Some(png.to_str().unwrap()));
    // v6-all merge with the lint_contrast_media fix: with no decode_media the media background is read for real (never the
    // black stand-in); decode_media: false is the explicit opt-out that keeps the v5 empty-cache output byte for byte.
    let dflt = lint(&mut s, json!({})); let m = measured(&dflt);
    for k in ["min_ratio", "p10_ratio", "mean_ratio"] { assert!((f(&m[k]) - want).abs() < 2e-3, "default {k} {m} vs {want}"); }
    let v5 = lint(&mut s, json!({"decode_media": false}));
    let m = measured(&v5); assert_eq!(f(&m["min_ratio"]), 21.0, "v5 background over media is black: {m}"); assert_eq!(f(&m["mean_ratio"]), 21.0);
    let on = lint(&mut s, json!({"decode_media": true}));
    let m = measured(&on);
    for k in ["min_ratio", "p10_ratio", "mean_ratio"] { assert!((f(&m[k]) - want).abs() < 2e-3, "{k} {m} vs {want}"); }
    assert!(f(&m["spread"]) < 2e-3, "{m}");
    // the same call again with the media now in the session cache: unchanged
    assert_eq!(strip(lint(&mut s, json!({"decode_media": true}))), strip(on));
    let _ = std::fs::remove_file(&png);
}

#[test] fn solid_background_same_both_ways() {
    let want = 1.05 / (lin(G) + 0.05);
    let mut s = scene(None);
    let off = lint(&mut s, json!({})); let on = lint(&mut s, json!({"decode_media": true}));
    assert_eq!(strip(off.clone()), strip(on));
    assert!((f(&measured(&off)["min_ratio"]) - want).abs() < 2e-3);
}

#[test] fn decode_media_must_be_boolean() {
    let mut s = scene(None);
    for v in [json!("yes"), json!(1), json!(null)] {
        let mut a = json!({"time": 1.0, "checks": ["text_contrast"], "annotate": false}); a["decode_media"] = v.clone();
        let r = dispatch(&mut s, "lint_frame", &a);
        if v.is_null() { assert!(r.is_err() || r.is_ok()); } else { assert!(r.unwrap_err().contains("decode_media must be boolean")); }
    }
}
