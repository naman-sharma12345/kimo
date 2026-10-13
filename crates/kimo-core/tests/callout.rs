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
    assert_eq!(found["total"], 6, "card, text, badge, badge text, highlight, badge highlight carry role callout");
    assert_eq!(c(&mut s, "find", json!({"tag":"callout:c1"}))["total"], 6);
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
    c(&mut s2, "callout_card", json!({"text":"Hello","position":[0,0,60,40],"safe_profile":"none","style":{"size":18,"padding":4,"shadow":false},"apply":true}));
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

// ---- ARCH ruling 11:10 (Pass 2 findings): card + badge + shadow are ONE unit ----
const SAFE: [f64; 4] = [12.8, 12.8, 230.4, 230.4]; // title_safe on 256x256
fn inside(r: [f64; 4], z: [f64; 4]) -> bool { r[0] >= z[0] - 1e-6 && r[1] >= z[1] - 1e-6 && r[0] + r[2] <= z[0] + z[2] + 1e-6 && r[1] + r[3] <= z[1] + z[3] + 1e-6 }
fn hits(p: [f64; 4], q: [f64; 4]) -> bool { p[0] < q[0] + q[2] && q[0] < p[0] + p[2] && p[1] < q[1] + q[3] && q[1] < p[1] + p[3] }
fn title(s: &mut Session, y: f64) -> String {
    let t = c(s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(s, "add_text", json!({"track_id":t,"text":"Launch","start":0,"duration":5,"size":28}))["clip_id"].as_str().unwrap().to_string();
    c(s, "set_property", json!({"clip_id":id,"property":"y","value":y})); id
}
fn lint_n(s: &mut Session) -> usize { let l = c(s, "lint_frame", json!({"time":1,"checks":["safe_zone","out_of_bounds","text_overflow","text_contrast"],"profile":"title_safe","annotate":false})); l["frames"].as_array().unwrap().iter().map(|f| f["findings"].as_array().unwrap().len()).sum() }
#[test]
fn top_left_badge_and_shadow_unit_is_inside_safe_zone_and_canvas_with_pixel_proof() {
    let mut s = base();
    let r = c(&mut s, "callout_card", json!({"text":"Hello\\nworld","badge":"TIP","position":"top_left","style":{"size":18,"padding":9,"shadow":{"dy":4,"blur":6}},"apply":true}));
    let (u, card, badge, sh) = (rect(&r["params"]["unit_bounds"]), rect(&r["params"]["card_rect"]), rect(&r["params"]["badge_rect"]), rect(&r["params"]["shadow_rect"]));
    assert!(inside(u, [0.0, 0.0, 256.0, 256.0]) && inside(u, SAFE), "unit {u:?} inside canvas and title safe");
    assert!(inside(card, u) && inside(badge, u) && inside(sh, u), "unit contains card, badge and shadow extent");
    assert!(badge[1] >= SAFE[1] - 1e-6, "badge top {} is inside the 5% margin", badge[1]);
    let p = c(&mut s, "read_pixels", json!({"time":1,"points":[[(badge[0] + 3.0) as i64, (badge[1] + badge[3] / 2.0) as i64]]}));
    let d = p["data"].as_array().unwrap(); let (red, green) = (d[0].as_f64().unwrap(), d[1].as_f64().unwrap());
    assert!(red > 2.0 * green, "pixel inside the badge is the red badge fill, not the background ({red}, {green})");
    assert_eq!(lint_n(&mut s), 0);
}
#[test]
fn unit_that_cannot_fit_is_a_typed_error_with_overflow_never_a_silent_clip() {
    let mut s = base(); let before = serde_json::to_vec(&s.project).unwrap();
    let e = dispatch(&mut s, "callout_card", &json!({"text":"Hello","badge":"TIP","position":"top_left","style":{"size":18,"padding":9,"margin":4},"apply":true})).unwrap_err();
    assert!(e.starts_with("callout_unfit") && e.contains("overflow_px") && e.contains("fix_hint") && e.contains("top: "), "{e}");
    assert_eq!(before, serde_json::to_vec(&s.project).unwrap(), "nothing applied");
    // dropping the badge shrinks the unit: the same call without it needs less room (badge height is part of the unit)
    let ok = dispatch(&mut s, "callout_card", &json!({"text":"Hello","badge":"TIP","badge_pos":"none","position":"top_left","style":{"size":18,"padding":9,"margin":13},"apply":false}));
    assert!(ok.is_ok(), "{ok:?}");
    let e2 = dispatch(&mut s, "callout_card", &json!({"text":"Hello","badge":"TIP","position":"top_left","style":{"size":18,"padding":9,"margin":13},"apply":false})).unwrap();
    assert!(inside(rect(&e2["params"]["unit_bounds"]), SAFE), "badge-aware unit still placed inside safe at margin 13");
}
#[test]
fn near_title_is_ink_disjoint_for_every_fallback_side() {
    let mut s = base();
    let bx = |s: &mut Session, x: f64, w: f64, h: f64| { let t = c(s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
        let id = c(s, "add_shape", json!({"track_id":t,"shape":"rect","start":0,"duration":5,"width":w,"height":h}))["clip_id"].as_str().unwrap().to_string();
        c(s, "set_property", json!({"clip_id":id,"property":"x","value":x})); id };
    // below (title in the upper half), above (title low), right (tall plate on the left), left (tall plate on the right)
    let t_up = title(&mut s, -60.0); let t_low = title(&mut s, 80.0);
    let left_plate = bx(&mut s, -70.0, 60.0, 190.0); let right_plate = bx(&mut s, 70.0, 60.0, 190.0);
    for (id, want) in [(t_up, "below"), (t_low, "above"), (left_plate, "right"), (right_plate, "left")] {
        let r = c(&mut s, "callout_card", json!({"text":"Tip","badge":"NEW","position":format!("near:{id}"),"style":{"size":16,"padding":8}}));
        assert_eq!(r["params"]["placement"], want, "{}", r["params"]["tried"]);
        let (u, t) = (rect(&r["params"]["unit_bounds"]), if want == "below" || want == "above" { let q = c(&mut s, "compute_text_bounds", json!({"clip_id":id,"t":1})); rect(&q["canvas_rect"]) } else { shape_rect(&s, &id) });
        assert!(!hits(u, t) && inside(u, SAFE), "{want}: unit {u:?} vs target {t:?}");
        let b = rect(&r["params"]["badge_rect"]); assert!(!hits(b, t), "{want}: badge {b:?} clear of target {t:?}");
    }
}
#[test]
fn near_offset_moves_the_unit_then_revalidates_with_a_minimum_fixing_offset() {
    let mut s = base(); let id = title(&mut s, -20.0);
    // the old failing case: offset [0,20] with a badge used to put the badge ON the title; the unit is placed below the title now
    let ok = c(&mut s, "callout_card", json!({"text":"Tip","badge":"TIP","position":format!("near:{id}"),"offset":[0,20],"style":{"size":16,"padding":8},"apply":true}));
    let ink = rect(&c(&mut s, "compute_text_bounds", json!({"clip_id":id,"t":1}))["canvas_rect"]);
    assert!(!hits(rect(&ok["params"]["badge_rect"]), ink) && !hits(rect(&ok["params"]["unit_bounds"]), ink), "badge and unit clear of the title ink");
    let u = rect(&ok["params"]["unit_bounds"]);
    assert!((u[1] - (ink[1] + ink[3] + 20.0)).abs() < 1e-6, "unit top == target bottom + 20 exactly: {u:?} vs {ink:?}");
    assert_eq!(ok["params"]["applied_offset"], json!([0, 20]));
    let p = c(&mut s, "read_pixels", json!({"time":1,"points":[[(rect(&ok["params"]["badge_rect"])[0] + 3.0) as i64, (rect(&ok["params"]["badge_rect"])[1] + rect(&ok["params"]["badge_rect"])[3] / 2.0) as i64]]}));
    let d = p["data"].as_array().unwrap(); assert!(d[0].as_f64().unwrap() > 2.0 * d[1].as_f64().unwrap(), "the badge pixel is the badge, the title is not drawn over it");
    // a negative offset overlaps the target: typed error naming the smallest fixing offset, and that offset works
    let e = dispatch(&mut s, "callout_card", &json!({"text":"Tip","badge":"TIP","position":format!("near:{id}"),"offset":[0,-10],"style":{"size":16,"padding":8}})).unwrap_err();
    assert!(e.starts_with("callout_overlap") && e.contains("min_fixing_offset"), "{e}");
    let nums: Vec<f64> = e.rsplit("min_fixing_offset: [").next().unwrap().trim_end_matches(']').split(',').map(|x| x.trim().parse().unwrap()).collect();
    let fixed = c(&mut s, "callout_card", json!({"text":"Tip","badge":"TIP","position":format!("near:{id}"),"offset":nums,"style":{"size":16,"padding":8}}));
    assert!(!hits(rect(&fixed["params"]["unit_bounds"]), ink), "the suggested offset clears the title");
}
#[test]
fn badge_positions_none_inside_and_right() {
    let mut s = base();
    let n = c(&mut s, "callout_card", json!({"text":"Hello","badge":"NEW","badge_pos":"none","position":"center","style":{"size":18,"padding":10},"apply":true}));
    assert!(n["params"]["badge_rect"].is_null() && n["badge_clip_id"].is_null(), "badge_pos none draws no badge");
    let i = c(&mut s, "callout_card", json!({"text":"Hello","badge":"NEW","badge_pos":"inside_top_left","position":"center","style":{"size":18,"padding":10},"apply":true}));
    let (b, card, t) = (rect(&i["params"]["badge_rect"]), rect(&i["params"]["card_rect"]), rect(&i["params"]["text_rect"]));
    assert!(inside(b, card) && !hits(b, t), "inside badge sits in the card and clear of the text: {b:?} {card:?} {t:?}");
    let r = c(&mut s, "callout_card", json!({"text":"Hello","badge":"NEW","badge_pos":"above_right","position":"center","style":{"size":18,"padding":10},"apply":true}));
    let (b, card) = (rect(&r["params"]["badge_rect"]), rect(&r["params"]["card_rect"]));
    close([b[0] + b[2], 0.0, 0.0, 0.0], [card[0] + card[2], 0.0, 0.0, 0.0], "above_right aligns the badge right edge with the card");
    assert!(b[1] + b[3] <= card[1], "badge above the card");
    let mut s2 = base();
    c(&mut s2, "callout_card", json!({"text":"Hello","badge":"NEW","badge_pos":"above_right","position":"center","style":{"size":18,"padding":10},"apply":true}));
    assert_eq!(lint_n(&mut s2), 0, "a lone above_right callout lints clean");
    assert!(dispatch(&mut s, "callout_card", &json!({"text":"x","badge_pos":"sideways"})).unwrap_err().contains("fix_hint"));
}

#[test]
fn offset_gap_is_the_same_for_every_badge_mode_and_card_offset_is_opt_in_and_validated() {
    let mut s = base(); let id = title(&mut s, -20.0);
    let ink = rect(&c(&mut s, "compute_text_bounds", json!({"clip_id":id,"t":1}))["canvas_rect"]);
    for bp in ["none", "above_left", "inside_top_left"] {
        let r = c(&mut s, "callout_card", json!({"text":"Tip","badge":"TIP","badge_pos":bp,"position":format!("near:{id}"),"offset":[0,20],"style":{"size":16,"padding":8}}));
        let u = rect(&r["params"]["unit_bounds"]); assert!((u[1] - (ink[1] + ink[3] + 20.0)).abs() < 1e-6, "{bp}: gap is measured from the unit: {u:?}");
    }
    let both = dispatch(&mut s, "callout_card", &json!({"text":"Tip","position":format!("near:{id}"),"offset":[0,20],"card_offset":[0,20]})).unwrap_err();
    assert!(both.contains("mutually exclusive") && both.contains("fix_hint"), "{both}");
    let before = serde_json::to_vec(&s.project).unwrap();
    let e = dispatch(&mut s, "callout_card", &json!({"text":"Tip","badge":"TIP","position":format!("near:{id}"),"card_offset":[0,-40],"style":{"size":16,"padding":8},"apply":true})).unwrap_err();
    assert!(e.starts_with("callout_overlap") && e.contains("min_fixing_offset"), "card_offset [0,20] with the badge above overlaps the title: {e}");
    assert_eq!(before, serde_json::to_vec(&s.project).unwrap(), "invalid card_offset never applies");
}

#[test]
fn default_near_gap_is_the_card_padding_measured_on_visible_pixels() {
    // visible gap = first badge-red row minus last title-white row, read from the working buffer (not the unit maths)
    for pad in [8.0f64, 16.0] {
        let mut s = base(); let id = title(&mut s, -20.0);
        let r = c(&mut s, "callout_card", json!({"text":"Tip","badge":"TIP","position":format!("near:{id}"),"style":{"size":16,"padding":pad},"apply":true}));
        assert_eq!(r["params"]["placement"], "below"); assert_eq!(r["params"]["default_gap"], pad);
        let ink = rect(&c(&mut s, "compute_text_bounds", json!({"clip_id":id,"t":1}))["canvas_rect"]);
        let (y0, y1) = (ink[1] as i64, (ink[1] + ink[3] + pad + 60.0).min(255.0) as i64);
        let px = c(&mut s, "read_pixels", json!({"time":1,"region":{"x":0,"y":y0,"w":256,"h":y1 - y0 + 1}}));
        let d = px["data"].as_array().unwrap();
        let (mut last_white, mut first_red) = (None, None);
        for row in 0..=(y1 - y0) {
            let (mut white, mut red) = (false, false);
            for x in 0..256usize { let i = (row as usize * 256 + x) * 4; let (r_, g_, b_) = (d[i].as_f64().unwrap(), d[i + 1].as_f64().unwrap(), d[i + 2].as_f64().unwrap());
                if r_ > 0.9 && g_ > 0.9 && b_ > 0.9 { white = true; } if r_ > 0.5 && r_ > 3.0 * g_ { red = true; } }
            if white && first_red.is_none() { last_white = Some(y0 + row); }
            if red && first_red.is_none() { first_red = Some(y0 + row); }
        }
        let gap = first_red.expect("badge rows") - last_white.expect("title rows") - 1;
        assert!(gap as f64 >= pad - 1.0, "padding {pad}: visible title-to-badge gap {gap}px must be at least the card padding");
    }
}
