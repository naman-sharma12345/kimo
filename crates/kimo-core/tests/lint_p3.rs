//! V4-1 P3: text vs foreground / overlap / caption style / budget rows (256x256, repo-owned fonts).
//! Stand-in for "video" layers: full-frame or PiP rect shapes (same bounds/matte/z logic as media in lint bounds()).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn pin_fonts() { static O: std::sync::Once = std::sync::Once::new(); O.call_once(|| { std::env::set_var("KIMO_FONTS", std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts")); }); }
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
fn new() -> Session { pin_fonts(); let mut s = Session::new(); c(&mut s, "project_set", json!({"width":256,"height":256,"fps":25})); s }
/// Adds a rect shape on a NEW top track. Draw order = call order.
fn shape(s: &mut Session, name: &str, w: f64, h: f64, x: f64, y: f64) -> String {
    let t = sid(&c(s, "add_track", json!({})), "track_id");
    let id = sid(&c(s, "add_shape", json!({"track_id":t,"shape":"rect","width":w,"height":h,"start":0,"duration":4})), "clip_id");
    c(s, "set_property", json!({"clip_id":id,"property":"x","value":x}));
    c(s, "set_property", json!({"clip_id":id,"property":"y","value":y}));
    c(s, "set_meta", json!({"id":id,"name":name}));
    id
}
/// Square media layer (a real asset, so bounds/fit/scale follow the media path): drawn size = `size` px.
fn layer(s: &mut Session, name: &str, size: f64, x: f64, y: f64) -> String {
    static N: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    let path = std::env::temp_dir().join(format!("kimo-p3-{}-{}.png", std::process::id(), N.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=gray:s=256x256", "-frames:v", "1", path.to_str().unwrap()]).status().unwrap().success());
    let a = c(s, "import_media", json!({"path":path}))["asset_id"].clone();
    let t = sid(&c(s, "add_track", json!({})), "track_id");
    let id = sid(&c(s, "add_clip", json!({"track_id":t,"source_type":"media","asset_id":a,"start":0,"duration":4})), "clip_id");
    for (k, v) in [("scale", size / 256.), ("x", x), ("y", y)] { c(s, "set_property", json!({"clip_id":id,"property":k,"value":v})); }
    c(s, "set_meta", json!({"id":id,"name":name}));
    id
}
fn text(s: &mut Session, name: &str, role: &str) -> String {
    let t = sid(&c(s, "add_track", json!({})), "track_id");
    let id = sid(&c(s, "add_text", json!({"track_id":t,"text":"Hello","font":"Montserrat","size":28,"color":[1,1,1],"start":0,"duration":4})), "clip_id");
    c(s, "set_meta", json!({"id":id,"name":name,"role":role}));
    id
}
fn lint(s: &mut Session, checks: Value) -> Value { c(s, "lint_frame", json!({"time":1,"checks":checks,"annotate":false})) }
fn findings(r: &Value) -> Vec<Value> { r["frames"].as_array().unwrap().iter().flat_map(|f| f["findings"].as_array().unwrap().clone()).collect() }
fn ink(s: &mut Session, id: &str) -> [f64; 4] { let b = c(s, "compute_text_bounds", json!({"clip_id":id,"t":1.0})); let v = b["canvas_rect"].as_array().unwrap(); [0, 1, 2, 3].map(|i| v[i].as_f64().unwrap()) }
fn overlap_ratio(a: [f64; 4], b: [f64; 4]) -> f64 { let w = ((a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])).max(0.); let h = ((a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1])).max(0.); w * h / (a[2] * a[3]) }
const TOF: &str = "text_over_foreground";
#[test]
fn row1_title_over_full_frame_base_is_background() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    text(&mut s, "title", "title");
    let r = lint(&mut s, json!([TOF]));
    assert!(findings(&r).is_empty(), "{r}");
}
#[test]
fn row2_pip_below_title_exact_ratio_rect_basis() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    layer(&mut s, "pip", 100., 0., 0.);
    let t = text(&mut s, "title", "title");
    let r = lint(&mut s, json!([TOF]));
    let f = findings(&r);
    assert_eq!(f.len(), 1, "{r}");
    let want = overlap_ratio(ink(&mut s, &t), [78., 78., 100., 100.]);
    assert!(want > 0.5, "setup: ink must sit mostly on the pip, got {want}");
    assert!((f[0]["measured"]["overlap_ratio"].as_f64().unwrap() - want).abs() < 1e-9, "{r}");
    assert_eq!(f[0]["measured"]["basis"], "rect");
    assert_eq!(f[0]["measured"]["covered_layer"]["name"], "pip");
    assert_eq!(f[0]["code"], TOF);
}
fn matte_scene(src: (f64, f64, f64, f64)) -> (Session, String) {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    let m = shape(&mut s, "matte_src", src.0, src.1, src.2, src.3);
    let p = layer(&mut s, "cutout", 120., 0., 0.);
    c(&mut s, "set_track_matte", json!({"clip_id":p,"matte_clip_id":m,"mode":"alpha"}));
    text(&mut s, "title", "title");
    (s, p)
}
#[test]
fn row3_matte_alpha_under_ink() {
    // alpha only in a far corner: nothing under the ink
    let (mut s, _) = matte_scene((20., 20., -100., -100.));
    let r = lint(&mut s, json!([TOF]));
    assert!(findings(&r).is_empty(), "{r}");
    // alpha covers the ink
    let (mut s, _) = matte_scene((200., 200., 0., 0.));
    let r = lint(&mut s, json!([TOF]));
    let f = findings(&r);
    assert_eq!(f.len(), 1, "{r}");
    assert_eq!(f[0]["measured"]["basis"], "matte");
    assert!(f[0]["measured"]["overlap_ratio"].as_f64().unwrap() > 0.9, "{r}");
    assert!(f[0]["measured"]["subject_bbox"].is_array(), "{r}");
}
#[test]
fn row4_full_frame_above_full_frame() {
    let mut s = new();
    layer(&mut s, "lower", 256., 0., 0.);
    layer(&mut s, "upper", 256., 0., 0.);
    text(&mut s, "title", "title");
    let r = lint(&mut s, json!([TOF]));
    let f = findings(&r);
    assert_eq!(f.len(), 1, "{r}");
    assert_eq!(f[0]["measured"]["covered_layer"]["name"], "upper");
}
#[test]
fn row5_backing_plates() {
    // tagged plate: excluded, listed
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    let plate = shape(&mut s, "plate", 200., 80., 0., 0.);
    let t = text(&mut s, "title", "title");
    c(&mut s, "set_meta", json!({"id":plate,"tags":[format!("lint:backing_of={t}")]}));
    let r = lint(&mut s, json!([TOF]));
    assert!(findings(&r).is_empty(), "{r}");
    assert_eq!(r["summary"]["excluded_layers"][0]["basis"], "tag", "{r}");
    // untagged plate fully containing the ink, directly below: auto_backing, listed
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    shape(&mut s, "plate", 200., 80., 0., 0.);
    text(&mut s, "title", "title");
    let r = lint(&mut s, json!([TOF]));
    assert!(findings(&r).is_empty(), "{r}");
    assert_eq!(r["summary"]["excluded_layers"][0]["basis"], "auto_backing", "{r}");
    // partial overlap plate is not a backing
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    shape(&mut s, "half", 100., 80., -50., 0.);
    text(&mut s, "title", "title");
    let r = lint(&mut s, json!([TOF]));
    assert_eq!(findings(&r).len(), 1, "{r}");
}
#[test]
fn row6_caption_only_caption_code() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    layer(&mut s, "pip", 100., 0., 80.);
    let t = sid(&c(&mut s, "add_track", json!({})), "track_id");
    let dir = std::env::temp_dir().join(format!("kimo-p3-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let srt = dir.join("c.srt");
    std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:03,000\nHello\n").unwrap();
    c(&mut s, "add_captions", json!({"track_id":t,"source":"srt","path":srt.to_str().unwrap(),"font":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts/Montserrat-Var.ttf").to_str().unwrap()}));
    let _ = std::fs::remove_dir_all(&dir);
    let r = lint(&mut s, json!([TOF, "caption_over_foreground"]));
    let f = findings(&r);
    assert!(!f.is_empty(), "{r}");
    assert!(f.iter().all(|x| x["code"] == "caption_over_foreground"), "{r}");
}
#[test]
fn row7_layer_above_text_is_not_foreground() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    text(&mut s, "title", "title");
    shape(&mut s, "cover", 100., 100., 0., 0.);
    let r = lint(&mut s, json!([TOF]));
    assert!(findings(&r).is_empty(), "{r}");
}
#[test]
fn row8_tag_suppression_counted() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    let p = layer(&mut s, "pip", 100., 0., 0.);
    let t = text(&mut s, "title", "title");
    c(&mut s, "set_meta", json!({"id":p,"tags":["lint:allow_overlap"]}));
    let r = lint(&mut s, json!([TOF]));
    assert!(findings(&r).is_empty(), "{r}");
    assert_eq!(r["summary"]["ignored_by_tag"], 1, "{r}");
    c(&mut s, "set_meta", json!({"id":p,"tags":[]}));
    c(&mut s, "set_meta", json!({"id":t,"tags":["lint:ignore=text_over_foreground"]}));
    let r = lint(&mut s, json!([TOF]));
    assert!(findings(&r).is_empty(), "{r}");
    assert_eq!(r["summary"]["ignored_by_tag"], 1, "{r}");
}
#[test]
fn row10_read_only_and_thread_independent() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    layer(&mut s, "pip", 100., 0., 0.);
    text(&mut s, "title", "title");
    let before = serde_json::to_vec(&s.project).unwrap();
    let checks = json!([TOF, "text_overlap", "text_contrast", "optical_balance", "caption_style_consistency"]);
    let a = lint(&mut s, checks.clone());
    assert_eq!(before, serde_json::to_vec(&s.project).unwrap());
    let strip = |v: &Value| { let mut v = v.clone(); for f in v["frames"].as_array_mut().unwrap() { f.as_object_mut().unwrap().remove("elapsed_ms"); } v };
    for th in ["1", "4"] {
        std::env::set_var("KIMO_THREADS", th);
        let b = lint(&mut s, checks.clone());
        assert_eq!(strip(&a).to_string(), strip(&b).to_string(), "threads={th}");
    }
}
#[test]
fn text_overlap_pairs_and_ignore_tag() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    let a = text(&mut s, "a", "title");
    let b = text(&mut s, "b", "title");
    c(&mut s, "set_property", json!({"clip_id":b,"property":"y","value":4}));
    let r = lint(&mut s, json!(["text_overlap"]));
    let f = findings(&r);
    assert_eq!(f.len(), 1, "{r}");
    assert_eq!(f[0]["code"], "text_overlap");
    c(&mut s, "set_property", json!({"clip_id":b,"property":"y","value":90}));
    assert!(findings(&lint(&mut s, json!(["text_overlap"]))).is_empty());
    c(&mut s, "set_property", json!({"clip_id":b,"property":"y","value":4}));
    c(&mut s, "set_meta", json!({"id":a,"tags":["lint:ignore=text_overlap"]}));
    let r = lint(&mut s, json!(["text_overlap"]));
    assert!(findings(&r).is_empty());
    assert_eq!(r["summary"]["ignored_by_tag"], 1);
}
#[test]
fn caption_style_consistency_and_allow_variation() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    let t = sid(&c(&mut s, "add_track", json!({})), "track_id");
    let dir = std::env::temp_dir().join(format!("kimo-p3s-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let srt = dir.join("c.srt");
    std::fs::write(&srt, "1\n00:00:00,000 --> 00:00:03,000\nOne\n\n2\n00:00:00,000 --> 00:00:03,000\nTwo\n\n3\n00:00:00,000 --> 00:00:03,000\nThree\n").unwrap();
    let r = c(&mut s, "add_captions", json!({"track_id":t,"source":"srt","path":srt.to_str().unwrap(),"font":std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fonts/Montserrat-Var.ttf").to_str().unwrap()}));
    let _ = std::fs::remove_dir_all(&dir);
    let cid = sid(&r, "caption_id");
    let ids: Vec<String> = s.project.editorial.captions[&cid].clips.clone();
    assert!(findings(&lint(&mut s, json!(["caption_style_consistency"]))).is_empty());
    c(&mut s, "set_text", json!({"clip_id":ids[1],"size":40}));
    let r = lint(&mut s, json!(["caption_style_consistency"]));
    let f = findings(&r);
    assert_eq!(f.len(), 1, "{r}");
    assert_eq!(f[0]["layer"]["id"], ids[1].as_str());
    c(&mut s, "set_meta", json!({"id":ids[1],"tags":["lint:allow_style_variation"]}));
    let r = lint(&mut s, json!(["caption_style_consistency"]));
    assert!(findings(&r).is_empty());
    assert_eq!(r["summary"]["ignored_by_tag"], 1);
}
#[test]
fn render_budget_skips_are_explicit_and_contrast_has_distribution() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    let t = text(&mut s, "title", "title");
    let _ = t;
    let r = c(&mut s, "lint_frame", json!({"time":1,"checks":["text_contrast"],"params":{"render_budget":0,"contrast_normal":100,"large_cap_px":10000},"annotate":false}));
    assert!(findings(&r).is_empty());
    let sk = r["summary"]["skipped_checks"].as_array().unwrap();
    assert_eq!(sk.len(), 1, "{r}");
    assert_eq!(sk[0]["reason"], "render_budget");
    let r = c(&mut s, "lint_frame", json!({"time":1,"checks":["text_contrast"],"params":{"contrast_normal":100,"large_cap_px":10000},"annotate":false}));
    let m = &findings(&r)[0]["measured"];
    for k in ["p5_ratio", "p50_ratio", "background_spread", "min_ratio"] { assert!(m[k].is_number(), "{k}: {r}"); }
    assert!(m["p5_ratio"].as_f64().unwrap() <= m["p50_ratio"].as_f64().unwrap());
    assert!(r["summary"]["skipped_checks"].as_array().unwrap().is_empty());
}
#[test]
fn optical_balance_is_opt_in_info_only() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    text(&mut s, "title", "title");
    let r = c(&mut s, "lint_frame", json!({"time":1,"annotate":false}));
    assert!(findings(&r).iter().all(|f| f["code"] != "optical_balance"), "default set must not include it");
    let r = lint(&mut s, json!(["optical_balance"]));
    let f = findings(&r);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0]["severity"], "info");
    assert!(f[0]["measured"]["offset_from_centre_frac"].is_array());
}
#[test]
fn own_box_text_excludes_foreground() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    layer(&mut s, "pip", 100., 0., 0.);
    let t = text(&mut s, "title", "title");
    c(&mut s, "set_text_style", json!({"clip_id":t,"bg":[0,0,0,0.8]}));
    let r = lint(&mut s, json!([TOF]));
    assert!(findings(&r).is_empty(), "{r}");
    assert_eq!(r["summary"]["excluded_layers"][0]["basis"], "own_box", "{r}");
}
#[test]
fn text_contrast_sees_media_background() {
    // white text over a mid-gray media layer: ratio is about 3.95, not the 21 a black (unrendered) background would give
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    text(&mut s, "title", "title");
    let r = c(&mut s, "lint_frame", json!({"time":1,"checks":["text_contrast"],"params":{"contrast_normal":100,"large_cap_px":10000},"annotate":false}));
    let m = findings(&r)[0]["measured"].clone();
    assert!(m["min_ratio"].as_f64().unwrap() < 6., "{r}");
}
#[test]
fn row9_closed_loop_fix_hint_clears_the_finding() {
    let mut s = new();
    layer(&mut s, "base", 256., 0., 0.);
    layer(&mut s, "pip", 100., 0., 0.);
    let t = text(&mut s, "title", "title");
    let r = lint(&mut s, json!([TOF]));
    let f = findings(&r);
    assert_eq!(f.len(), 1, "{r}");
    let h = &f[0]["fix_hint"];
    assert_eq!(h["kind"], "move", "{r}");
    assert_eq!(h["tool"], "set_property");
    assert_eq!(h["args"]["clip_id"], t.as_str());
    c(&mut s, h["tool"].as_str().unwrap(), h["args"].clone());
    let r2 = lint(&mut s, json!([TOF, "text_overflow", "safe_zone"]));
    assert!(findings(&r2).is_empty(), "{r2}");
}
