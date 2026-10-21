//! pick_key_color (the tool the keyer's description has pointed to, which did not exist) and chroma_key's opt-in YCbCr distance.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::{apply, Frame, Params};
use kimo_core::keyframe::Prop;
use serde_json::{json, Value};
use std::collections::BTreeMap;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn session(src: Value) -> (Session, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width":64,"height":36,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone(); let mut a = src; a["track_id"] = t; a["start"] = json!(0); a["duration"] = json!(1);
    let id = c(&mut s, "add_clip", a)["clip_id"].as_str().unwrap().to_string(); (s, id)
}
fn keyers(s: &Session, id: &str) -> Vec<(String, [f64; 3])> { s.project.find_clip(id).unwrap().effects.iter().filter(|e| e.kind == "keyer").map(|e| (e.id.clone(), [e.params["r"].base, e.params["g"].base, e.params["b"].base])).collect() }

#[test]
fn pick_key_color_samples_the_screen_and_sets_the_keyer() {
    let (mut s, id) = session(json!({"source_type":"solid","color":[0.1,0.7,0.15]}));
    let r = c(&mut s, "pick_key_color", json!({"clip_id":id,"time":0.5,"x":0.0,"y":0.0,"w":0.2,"h":0.2}));
    eprintln!("pick -> {r}");
    let k = keyers(&s, &id); assert_eq!(k.len(), 1, "adds one keyer when the clip has none");
    for (j, want) in [0.1, 0.7, 0.15].iter().enumerate() { assert!((k[0].1[j] - want).abs() < 1e-3, "{:?}", k[0].1); assert!((r["sampled_rgb"][j].as_f64().unwrap() - want).abs() < 1e-3); }
    assert_eq!(r["effect_id"].as_str().unwrap(), k[0].0);
    // a second pick updates that keyer (no second keyer) and samples the clip WITHOUT the keyer (else the screen reads as keyed out)
    c(&mut s, "set_effect_param", json!({"clip_id":id,"effect_id":k[0].0,"param":"r","value":0.9}));
    let r2 = c(&mut s, "pick_key_color", json!({"clip_id":id,"time":0.5}));
    let k2 = keyers(&s, &id); assert_eq!(k2.len(), 1); assert!((k2[0].1[0] - 0.1).abs() < 1e-3 && (k2[0].1[1] - 0.7).abs() < 1e-3, "{r2}");
}

#[test]
fn pick_key_color_uses_the_median_and_can_target_chroma_key() {
    // linear gradient left->right: the per-channel median of a region is the colour at the region's centre column
    let (mut s, id) = session(json!({"source_type":"gradient","color":[0.0,0.5,0.0],"color2":[0.2,0.9,0.2],"angle":90}));
    let fx = c(&mut s, "add_effect", json!({"clip_id":id,"kind":"chroma_key"}))["effect_id"].as_str().unwrap().to_string();
    let r = c(&mut s, "pick_key_color", json!({"clip_id":id,"time":0.5,"x":0.25,"y":0.25,"w":0.5,"h":0.5,"effect_id":fx}));
    let g = r["sampled_rgb"][1].as_f64().unwrap(); eprintln!("gradient median g {g}"); assert!((g - 0.7).abs() < 0.02, "{r}");
    let e = s.project.find_clip(&id).unwrap().effects.iter().find(|e| e.id == fx).unwrap(); assert!((e.params["g"].base - g).abs() < 1e-9);
    assert!(keyers(&s, &id).is_empty(), "targeting a chroma_key must not add a keyer");
}

#[test]
fn pick_key_color_refusals() {
    let (mut s, id) = session(json!({"source_type":"solid","color":[0.1,0.7,0.15]}));
    for bad in [json!({"clip_id":id,"time":0.5,"x":0.9,"y":0.0,"w":0.2,"h":0.2}), json!({"clip_id":id,"time":5.0}), json!({"clip_id":"nope","time":0.5}), json!({"clip_id":id,"time":0.5,"effect_id":"fx_missing"})] {
        let e = dispatch(&mut s, "pick_key_color", &bad).expect_err("must refuse"); eprintln!("{bad} -> {e}"); }
    assert!(keyers(&s, &id).is_empty(), "a refused pick must not add a keyer");
}

fn ck(space: Option<f64>, rgb: [f32; 3]) -> f32 {
    let mut pr: Params = BTreeMap::new(); if let Some(v) = space { pr.insert("space".into(), Prop::new(v)); }
    let mut f = Frame { w: 1, h: 1, d: vec![rgb[0], rgb[1], rgb[2], 1.0] }; apply("chroma_key", &pr, &BTreeMap::new(), &mut f, 0.0, 0).unwrap(); f.d[3]
}

#[test]
fn chroma_key_ycbcr_ignores_screen_shading() {
    // a screen in shadow at 35 % brightness: RGB distance says "not the key colour", CbCr distance (Rec.709) says it is
    let shadow = [0.0, 0.35, 0.0];
    eprintln!("shadowed screen alpha: rgb {} ycbcr {}", ck(None, shadow), ck(Some(1.0), shadow));
    assert!(ck(None, shadow) > 0.2); assert_eq!(ck(Some(1.0), shadow), 0.0);
    assert_eq!(ck(Some(1.0), [0.0, 1.0, 0.0]), 0.0); assert_eq!(ck(Some(1.0), [0.8, 0.6, 0.5]), 1.0, "skin stays opaque");
    // the default (space absent or 0) is the old RGB key, bit for bit
    for px in [[0.3f32, 0.8, 0.2], [0.5, 0.5, 0.5], [0.1, 0.9, 0.4]] { assert_eq!(ck(None, px).to_bits(), ck(Some(0.0), px).to_bits()); }
    let e = dispatch(&mut session(json!({"source_type":"solid","color":[0,1,0]})).0, "add_effect", &json!({"clip_id":"clip_1","kind":"chroma_key","params":{"space":2}}));
    eprintln!("space 2 -> {e:?}");
}
