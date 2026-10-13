//! V1c-C: fit_box_to_text + callout_card rows on a 256x256 canvas.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn base() -> Session {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":256,"height":256,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[0.1,0.2,0.3],"color2":[0.2,0.3,0.5],"start":0,"duration":5}));
    s
}
fn rect(r: &Value) -> [f64; 4] { let a = r.as_array().unwrap(); [a[0].as_f64().unwrap(), a[1].as_f64().unwrap(), a[2].as_f64().unwrap(), a[3].as_f64().unwrap()] }
fn close(a: [f64; 4], b: [f64; 4], what: &str) { for i in 0..4 { assert!((a[i] - b[i]).abs() < 1e-6, "{what}: {a:?} vs {b:?}"); } }
/// Card rect of a shape clip from its x/y (offset of centre) and size.
fn shape_rect(s: &Session, id: &str) -> [f64; 4] {
    let cl = s.project.find_clip(id).unwrap();
    let kimo_core::model::Source::Shape { width, height, .. } = &cl.source else { panic!("not a shape") };
    let q = kimo_core::render::resolve_xform(&s.project, cl, 0.0);
    [128.0 + q.x - width / 2.0, 128.0 + q.y - height / 2.0, *width, *height]
}
const A: fn(&str) -> Value = |t| json!({"text":t,"position":"center","style":{"size":18,"padding":10},"apply":true});
#[test]
fn fit_box_matches_ink_plus_padding_and_is_read_only() {
    let mut s = base();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let tx = c(&mut s, "add_text", json!({"track_id":t,"text":"Hello","start":0,"duration":5,"size":24}))["clip_id"].as_str().unwrap().to_string();
    let before = serde_json::to_vec(&s.project).unwrap();
    let ink = rect(&c(&mut s, "compute_text_bounds", json!({"clip_id":tx,"t":1}))["canvas_rect"]);
    let r0 = c(&mut s, "fit_box_to_text", json!({"clip_id":tx,"t":1}));
    close(rect(&r0["rect"]), ink, "padding 0");
    let r5 = c(&mut s, "fit_box_to_text", json!({"clip_id":tx,"padding":5,"t":1}));
    close(rect(&r5["rect"]), [ink[0] - 5.0, ink[1] - 5.0, ink[2] + 10.0, ink[3] + 10.0], "padding 5");
    assert_eq!(before, serde_json::to_vec(&s.project).unwrap(), "read-only");
    assert!(dispatch(&mut s, "fit_box_to_text", &json!({"clip_id":tx,"padding":-1})).unwrap_err().contains("fix_hint"));
}
#[test]
fn card_rect_equals_ink_rect_plus_padding() {
    let mut s = base();
    let r = c(&mut s, "callout_card", A("Hello"));
    let (card, text) = (r["card_clip_id"].as_str().unwrap(), r["text_clip_id"].as_str().unwrap());
    let fit = c(&mut s, "fit_box_to_text", json!({"clip_id":text,"padding":10,"t":1}));
    close(shape_rect(&s, card), rect(&fit["rect"]), "card == fit_box_to_text(text, padding)");
    close(rect(&r["params"]["card_rect"]), rect(&fit["rect"]), "echoed card_rect");
}
#[test]
fn multi_line_card_grows_with_the_text() {
    let mut s = base();
    let one = c(&mut s, "callout_card", json!({"text":"AB","position":"center","style":{"size":18,"padding":10}}));
    let two = c(&mut s, "callout_card", json!({"text":"AB\\nAB","position":"center","style":{"size":18,"padding":10},"apply":true}));
    assert!(rect(&two["params"]["card_rect"])[3] > rect(&one["params"]["card_rect"])[3] + 10.0, "two lines are taller");
    let text = two["text_clip_id"].as_str().unwrap();
    let fit = c(&mut s, "fit_box_to_text", json!({"clip_id":text,"padding":10,"t":1}));
    close(shape_rect(&s, two["card_clip_id"].as_str().unwrap()), rect(&fit["rect"]), "multi-line card");
}
#[test]
fn badge_sits_above_and_never_overlaps_text() {
    let mut s = base();
    let r = c(&mut s, "callout_card", json!({"text":"Hello world","badge":"NEW","position":"center","style":{"size":18,"padding":10},"apply":true}));
    let (t, b, card) = (rect(&r["params"]["text_rect"]), rect(&r["params"]["badge_rect"]), rect(&r["params"]["card_rect"]));
    let ov = |p: [f64; 4], q: [f64; 4]| p[0] < q[0] + q[2] && q[0] < p[0] + p[2] && p[1] < q[1] + q[3] && q[1] < p[1] + p[3];
    assert!(!ov(t, b) && !ov(card, b), "badge overlaps neither text nor card");
    let btxt = rect(&c(&mut s, "fit_box_to_text", json!({"clip_id":r["badge_text_clip_id"],"t":1}))["rect"]);
    assert!(!ov(t, btxt), "badge text does not touch the callout text");
    close(shape_rect(&s, r["badge_clip_id"].as_str().unwrap()), b, "badge shape == echoed badge_rect");
    let found = c(&mut s, "find", json!({"role":"callout"}));
    assert_eq!(found["total"], 4, "card, text, badge, badge text carry role callout");
    assert_eq!(c(&mut s, "find", json!({"tag":"callout:c1"}))["total"], 4);
}
#[test]
fn long_text_is_a_typed_error_never_a_silent_clip() {
    let mut s = base();
    let before = serde_json::to_vec(&s.project).unwrap();
    let e = dispatch(&mut s, "callout_card", &json!({"text":"this sentence is far too long to fit on one line of a tiny canvas","style":{"size":18},"apply":true})).unwrap_err();
    assert!(e.starts_with("text_too_wide") && e.contains("fix_hint"), "{e}");
    for bad in [json!({"text":"x","position":"nowhere"}), json!({"text":"x","position":[0,0,5,5]}), json!({"text":"x","position":[0,0,-1,5]}), json!({"text":"x","style":{"size":"NaN"}}), json!({"text":"x","position":"top_left","style":{"margin":-3}}), json!({"text":"  "})] {
        let e = dispatch(&mut s, "callout_card", &bad).unwrap_err(); assert!(e.contains("fix_hint"), "{bad}: {e}");
    }
    assert_eq!(before, serde_json::to_vec(&s.project).unwrap(), "errors leave the project unchanged");
}
#[test]
fn apply_equals_expansion_by_hand_and_dry_run_changes_nothing() {
    let mut a = base(); let mut b = base();
    let args = json!({"text":"Hello\\nworld","badge":"TIP","position":"top_left","style":{"size":18,"padding":9,"shadow":{"dy":4,"blur":6}}});
    let before = serde_json::to_vec(&a.project).unwrap();
    let dry = c(&mut a, "callout_card", args.clone());
    assert_eq!(dry["applied"], false);
    assert_eq!(before, serde_json::to_vec(&a.project).unwrap(), "apply:false leaves the project unchanged");
    let mut ap = args.clone(); ap["apply"] = json!(true);
    let r = c(&mut a, "callout_card", ap);
    assert_eq!(r["expansion"], dry["expansion"]);
    // by hand: run each op, resolving $id.field references from earlier results
    let mut res: std::collections::HashMap<String, Value> = Default::default();
    for op in dry["expansion"].as_array().unwrap() {
        let mut ar = op["args"].clone();
        if let Some(o) = ar.as_object_mut() { for v in o.values_mut() { if let Some(sv) = v.as_str() { if let Some(rest) = sv.strip_prefix('$') { let (id, f) = rest.split_once('.').unwrap(); *v = res[id][f].clone(); } } } }
        let out = c(&mut b, op["tool"].as_str().unwrap(), ar);
        if let Some(id) = op["id"].as_str() { res.insert(id.to_string(), out); }
    }
    assert_eq!(serde_json::to_vec(&a.project).unwrap(), serde_json::to_vec(&b.project).unwrap(), "apply:true == expansion by hand");
    c(&mut a, "undo", json!({}));
    assert_eq!(before, serde_json::to_vec(&a.project).unwrap(), "one undo removes the whole callout");
}
#[test]
fn closed_loop_card_inside_safe_zone_has_no_lint_findings() {
    let mut s = base();
    let r = c(&mut s, "callout_card", json!({"text":"Hello","badge":"NEW","position":"bottom_center","style":{"size":18,"padding":10},"apply":true}));
    assert_eq!(r["applied"], true);
    let l = c(&mut s, "lint_frame", json!({"time":1,"checks":["safe_zone","out_of_bounds","text_overflow","text_contrast"],"profile":"title_safe","annotate":false}));
    let n: usize = l["frames"].as_array().unwrap().iter().map(|f| f["findings"].as_array().unwrap().len()).sum();
    assert_eq!(n, 0, "{l}");
    // and a card pushed out of the safe zone is reported
    let mut s2 = base();
    c(&mut s2, "callout_card", json!({"text":"Hello","position":[0,0,60,40],"style":{"size":18,"padding":4},"apply":true}));
    let l2 = c(&mut s2, "lint_frame", json!({"time":1,"checks":["safe_zone"],"profile":"title_safe","annotate":false}));
    let n2: usize = l2["frames"].as_array().unwrap().iter().map(|f| f["findings"].as_array().unwrap().len()).sum();
    assert!(n2 > 0, "text near the corner is flagged: {l2}");
}
#[test]
fn style_from_copies_and_explicit_style_wins() {
    let mut s = base();
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let sh = c(&mut s, "add_shape", json!({"track_id":t,"shape":"rect","start":0,"duration":5,"width":20,"height":20,"fill":[0.5,0.25,0.75],"corner_radius":7}))["clip_id"].as_str().unwrap().to_string();
    let r = c(&mut s, "callout_card", json!({"text":"Hi","position":"center","style_from":sh,"style":{"corner_radius":3}}));
    let st = &r["params"]["style"];
    assert_eq!(st["fill"][0], 0.5); assert_eq!(st["corner_radius"], 3.0);
}
#[test]
fn threads_1_vs_4_byte_equal() {
    let mut out = vec![];
    for th in ["1", "4"] { std::env::set_var("KIMO_THREADS", th); let mut s = base(); c(&mut s, "callout_card", A("Hello")); out.push(kimo_core::render::render_frame(&s.project, 1.0, &kimo_core::render::Cache::new()).unwrap().d.iter().map(|v| v.to_bits()).collect::<Vec<_>>()); }
    assert_eq!(out[0], out[1]);
}
