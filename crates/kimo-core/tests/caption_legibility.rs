//! v6 eyes: validate_captions legibility numbers. A 320x180 25 fps timeline: a solid background under captions made by
//! transcript_to_captions. Known references: white captions on black give WCAG 21:1 at every pixel of the ink box, on
//! #777777 the textbook 4.478:1 (worked out here from the sRGB EOTF); cps = characters (newlines left out) / on-screen
//! seconds; cap_frac = cap_px / 180; doubling one cue's font size doubles its capital-H ink height (within a pixel);
//! the group extremes name the right cues. Without the argument the output bytes equal legibility:false and carry no
//! legibility block (v5 shape); read-only; a non-boolean is refused. A media background (a #777777 PNG imported as a
//! clip) must read 4.478 too: the background frame is decoded, never an empty black cache (the first real-footage run read
//! 21:1 over jaja-ad.mp4 because the media layer was not decoded).
use kimo_core::commands::{dispatch, Session};
use kimo_core::model::Source;
use serde_json::{json, Value};

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn f(v: &Value) -> f64 { v.as_f64().unwrap_or_else(|| panic!("not a number: {v}")) }
fn lin(x: f64) -> f64 { if x <= 0.04045 { x / 12.92 } else { ((x + 0.055) / 1.055).powf(2.4) } }
/// (session, caption group id, cue clip ids)
fn fixture(bg: f64, tag: &str) -> (Session, String, Vec<String>) { fixture_on(json!({"source_type": "solid", "color": [bg, bg, bg]}), tag) }
fn fixture_on(layer: Value, tag: &str) -> (Session, String, Vec<String>) {
    let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 320, "height": 180, "fps": 25}));
    let tb = c(&mut ss, "add_track", json!({}))["track_id"].clone();
    let mut lay = layer.clone(); if let Some(path) = layer.get("png").and_then(|v| v.as_str()) { let a = c(&mut ss, "import_media", json!({"path": path}))["asset_id"].clone(); lay = json!({"source_type": "media", "asset_id": a}); }
    lay["track_id"] = tb; lay["start"] = json!(0.0); lay["duration"] = json!(12.0); c(&mut ss, "add_clip", lay);
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let words = "Cold tyres read lower than warm ones because the air inside contracts when the temperature falls overnight. Check them before you drive.";
    let mut tm = 0.0; let mut csv = String::from("word,start,end,confidence\n"); for w in words.split_whitespace() { csv.push_str(&format!("{},{:.3},{:.3},0.95\n", w.replace(',', ""), tm, tm + 0.4)); tm += 0.4; }
    let p = std::env::temp_dir().join(format!("kimo_leg_{}_{tag}.csv", std::process::id())); std::fs::write(&p, csv).unwrap();
    c(&mut ss, "transcript_import", json!({"path": p.to_str().unwrap()}));
    let r = c(&mut ss, "transcript_to_captions", json!({"track_id": t})); let _ = std::fs::remove_file(p);
    let gid = r["caption_id"].as_str().unwrap().to_string(); let clips: Vec<String> = r["clip_ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().into()).collect();
    for id in &clips { if let Some(cl) = ss.project.tracks.iter_mut().flat_map(|t| t.clips.iter_mut()).find(|x| &x.id == id) { if let Source::Text { color, .. } = &mut std::sync::Arc::make_mut(cl).source { *color = [1.0, 1.0, 1.0]; } } }
    (ss, gid, clips)
}
fn text_of(ss: &Session, id: &str) -> String { match &ss.project.find_clip(id).unwrap().source { Source::Text { text, .. } => text.clone(), _ => panic!("not text") } }

#[test] fn v5_shape_without_the_argument() {
    let (mut ss, gid, _) = fixture(0.0, "v5");
    let a = c(&mut ss, "validate_captions", json!({"group_id": gid})); let b = c(&mut ss, "validate_captions", json!({"group_id": gid, "legibility": false}));
    assert_eq!(serde_json::to_string(&a).unwrap(), serde_json::to_string(&b).unwrap());
    assert!(a["groups"][0].get("legibility").is_none());
    assert!(dispatch(&mut ss, "validate_captions", &json!({"group_id": gid, "legibility": "yes"})).is_err());
}

#[test] fn white_on_black_is_21_and_cps_cap_frac_hold() {
    let (mut ss, gid, clips) = fixture(0.0, "blk");
    let before = serde_json::to_string(&ss.project).unwrap();
    let r = c(&mut ss, "validate_captions", json!({"group_id": gid, "legibility": true}));
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "read-only");
    let l = &r["groups"][0]["legibility"]; assert_eq!(l["canvas"], json!([320.0, 180.0]));
    let rows = l["cues"].as_array().unwrap(); assert_eq!(rows.len(), clips.len()); assert!(rows.len() >= 2, "{l}");
    let mut worst_cps = (0.0, String::new());
    for row in rows {
        let id = row["cue_id"].as_str().unwrap(); let cl = ss.project.find_clip(id).unwrap().clone();
        let n = text_of(&ss, id).chars().filter(|ch| *ch != '\n' && *ch != '\r').count();
        assert_eq!(row["chars"], n); let cps = n as f64 / cl.duration; assert!((f(&row["cps"]) - cps).abs() < 1e-4, "{row}");
        if cps > worst_cps.0 { worst_cps = (cps, id.to_string()); }
        assert!((f(&row["t_measured"]) - (cl.start + cl.duration / 2.0)).abs() < 1e-4);
        assert!(f(&row["cap_px"]) > 2.0, "{row}"); assert!((f(&row["cap_frac"]) - f(&row["cap_px"]) / 180.0).abs() < 1e-4);
        let ink = row["ink_rect"].as_array().unwrap(); assert!(f(&ink[3]) >= f(&row["cap_px"]) - 1.0, "ink box holds the caps: {row}");
        assert_eq!(row["contrast"]["min_ratio"], 21.0, "{row}"); assert_eq!(row["contrast"]["mean_ratio"], 21.0);
        assert!(row["contrast"]["pixels"].as_u64().unwrap() > 0);
    }
    assert_eq!(l["max_cps"]["cue_id"], worst_cps.1.as_str()); assert!((f(&l["max_cps"]["value"]) - worst_cps.0).abs() < 1e-4);
    assert_eq!(l["min_contrast"]["value"], 21.0);
    // the top-level summary repeats the extremes as flat scalars
    let s = &r["legibility"]; assert_eq!(s["canvas_h"], 180.0); assert_eq!(s["min_contrast"], 21.0); assert_eq!(s["max_cps_cue"], worst_cps.1.as_str());
    assert_eq!(s["min_cap_frac"], l["min_cap_frac"]["value"]); assert_eq!(s["min_cap_frac_cue"], l["min_cap_frac"]["cue_id"]); assert!(f(&s["min_cap_px"]) > 2.0);
    assert!(c(&mut ss, "validate_captions", json!({"group_id": gid})).get("legibility").is_none());
}

#[test] fn grey_background_gives_the_textbook_ratio() {
    let g = 119.0 / 255.0; let (mut ss, gid, _) = fixture(g, "grey");
    let want = (1.0 + 0.05) / (lin(g) + 0.05); assert!((want - 4.478).abs() < 1e-3, "reference {want}");
    let r = c(&mut ss, "validate_captions", json!({"group_id": gid, "legibility": true}));
    for row in r["groups"][0]["legibility"]["cues"].as_array().unwrap() { assert!((f(&row["contrast"]["min_ratio"]) - want).abs() < 1e-3, "{row} vs {want}"); }
}

#[test] fn double_size_doubles_cap_height_and_min_cap_names_the_small_cue() {
    let (mut ss, gid, clips) = fixture(0.0, "size");
    let r0 = c(&mut ss, "validate_captions", json!({"group_id": gid, "legibility": true}));
    let cap0 = f(&r0["groups"][0]["legibility"]["cues"][0]["cap_px"]);
    let big = &clips[0];
    for t in ss.project.tracks.iter_mut() { for cl in t.clips.iter_mut() { if &cl.id == big { if let Source::Text { size, .. } = &mut std::sync::Arc::make_mut(cl).source { *size *= 2.0; } } } }
    let r1 = c(&mut ss, "validate_captions", json!({"group_id": gid, "legibility": true}));
    let l = &r1["groups"][0]["legibility"]; let rows = l["cues"].as_array().unwrap();
    let row0 = rows.iter().find(|x| x["cue_id"] == big.as_str()).unwrap(); let cap1 = f(&row0["cap_px"]);
    assert!((cap1 - 2.0 * cap0).abs() <= 1.0, "cap {cap0} -> {cap1}");
    assert_ne!(l["min_cap_frac"]["cue_id"], big.as_str()); assert!((f(&l["min_cap_frac"]["value"]) - cap0 / 180.0).abs() < 1e-3);
}

#[test] fn media_background_is_decoded_not_black() {
    let g = 119.0 / 255.0; let want = (1.0 + 0.05) / (lin(g) + 0.05);
    let mut s0 = Session::new(); c(&mut s0, "project_set", json!({"width": 320, "height": 180, "fps": 25}));
    let t0 = c(&mut s0, "add_track", json!({}))["track_id"].clone(); c(&mut s0, "add_clip", json!({"track_id": t0, "source_type": "solid", "color": [g, g, g], "start": 0.0, "duration": 2.0}));
    let png = std::env::temp_dir().join(format!("kimo_leg_{}_grey.png", std::process::id())); c(&mut s0, "render_frame", json!({"time": 0.5, "path": png.to_str().unwrap(), "inline": "never"}));
    let (mut ss, gid, _) = fixture_on(json!({"png": png.to_str().unwrap()}), "media");
    let r = c(&mut ss, "validate_captions", json!({"group_id": gid, "legibility": true})); let _ = std::fs::remove_file(&png);
    for row in r["groups"][0]["legibility"]["cues"].as_array().unwrap() { assert!((f(&row["contrast"]["min_ratio"]) - want).abs() < 2e-3, "{row} vs {want}"); }
}
