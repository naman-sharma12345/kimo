//! v6 eyes item #3: query_safe_margins `rects` overlap numbers. v5 output pinned byte for byte (bytes captured over MCP from
//! the v5.0.0 5ad355c kimo-mcp), hand-computed rectangle intersections against title_safe on 1920x1080 and the tiktok estimate
//! on 1080x1920, rects past the canvas edge, a profile with no safe zone, and refusals (never clamped). Steer 20:04Z: crossed_edges,
//! suggested_shift and worst_edge by hand on 16:9 (1920x1080) and 9:16 (1080x1920, tiktok and shorts) canvases, cards crossing
//! two edges at once (shift applied = the rect fits exactly).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn session() -> Session { let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width": 1920, "height": 1080, "fps": 24})).unwrap(); s }
fn close(v: &Value, want: f64, tol: f64) { let g = v.as_f64().unwrap_or(f64::NAN); assert!((g - want).abs() <= tol, "{g} vs {want}"); }
/// query_safe_margins output from the v5.0.0 (5ad355c) kimo-mcp over MCP (tools/call text), 1920x1080 project.
const V5: [(&str, &str); 5] = [
    (r#"{}"#, r#"{"as_of":"2026-10-13","basis":"SMPTE-style title safe: 90% of width and height, centred (5% inset each side)","canvas":{"height":1080.0,"width":1920.0},"estimated":false,"profile":"title_safe","read_only":"nothing is moved or rejected; these are numbers to compare against","source":"documented-constant","zones":[{"kind":"safe","name":"title_safe","notes":"keep titles and lower thirds inside","rect":[96.0,54.0,1728.0,972.0]}]}"#),
    (r#"{"profile":"action_safe"}"#, r#"{"as_of":"2026-10-13","basis":"SMPTE-style action safe: 93% of width and height, centred (3.5% inset each side)","canvas":{"height":1080.0,"width":1920.0},"estimated":false,"profile":"action_safe","read_only":"nothing is moved or rejected; these are numbers to compare against","source":"documented-constant","zones":[{"kind":"safe","name":"action_safe","notes":"keep important action inside","rect":[67.2,37.8,1785.6,1004.4]}]}"#),
    (r#"{"profile":"tiktok","aspect":"9:16","width":1080}"#, r#"{"as_of":"2026-10-13","basis":"estimate of the platform UI overlay, not published by the platform; check the current app before relying on it","canvas":{"height":1920.0,"width":1080.0},"estimated":true,"profile":"tiktok","read_only":"nothing is moved or rejected; these are numbers to compare against","source":"documented-constant","zones":[{"kind":"safe","name":"safe","notes":"estimated area clear of UI","rect":[64.8,192.0,820.8,1190.4]},{"kind":"avoid","name":"top_bar","notes":"search / following tabs","rect":[0.0,0.0,1080.0,192.0]},{"kind":"avoid","name":"caption_and_sound_bar","notes":"username, caption, sound ticker","rect":[0.0,1382.4,885.6,537.6]},{"kind":"avoid","name":"action_rail","notes":"like, comment, share, profile buttons","rect":[907.2,768.0,172.8,1152.0]}]}"#),
    (r#"{"profile":"youtube","width":1280,"height":720}"#, r#"{"as_of":"2026-10-13","basis":"estimate of the platform UI overlay, not published by the platform; check the current app before relying on it","canvas":{"height":720.0,"width":1280.0},"estimated":true,"profile":"youtube","read_only":"nothing is moved or rejected; these are numbers to compare against","source":"documented-constant","zones":[{"kind":"safe","name":"safe","notes":"estimated area clear of player controls","rect":[64.0,36.0,1152.0,576.0]},{"kind":"avoid","name":"player_controls","notes":"progress bar and controls when visible","rect":[0.0,633.6,1280.0,86.4]},{"kind":"avoid","name":"end_screen_corner","notes":"end-screen cards and watermark","rect":[921.6,432.0,358.4,201.6]}]}"#),
    (r#"{"profile":"custom","zones":[{"name":"logo","rect":[1700,40,180,90],"kind":"avoid"},{"name":"core","rect":[200,100,1520,880],"kind":"safe","notes":"n"}]}"#, r#"{"as_of":"2026-10-13","basis":"user-supplied zones, echoed and validated","canvas":{"height":1080.0,"width":1920.0},"estimated":false,"profile":"custom","read_only":"nothing is moved or rejected; these are numbers to compare against","source":"user","zones":[{"kind":"avoid","name":"logo","notes":"","rect":[1700.0,40.0,180.0,90.0]},{"kind":"safe","name":"core","notes":"n","rect":[200.0,100.0,1520.0,880.0]}]}"#)
];
#[test] fn query_safe_margins_without_rects_is_byte_identical_to_v5() {
    let mut s = session();
    for (args, want) in V5 { let a: Value = serde_json::from_str(args).unwrap(); assert_eq!(serde_json::to_string(&dispatch(&mut s, "query_safe_margins", &a).unwrap()).unwrap(), want, "{args}"); }
    let a = json!({"rects": null}); assert_eq!(serde_json::to_string(&dispatch(&mut s, "query_safe_margins", &a).unwrap()).unwrap(), V5[0].1);
}
#[test] fn title_safe_overlaps_by_hand() {
    // title_safe on 1920x1080 is [96, 54, 1728, 972]
    let mut s = session();
    let r = dispatch(&mut s, "query_safe_margins", &json!({"rects": [{"name": "corner", "rect": [0, 0, 192, 108]}, {"rect": [500, 500, 100, 100]}, {"name": "edge", "rect": [-100, 500, 200, 100]}]})).unwrap();
    let o = r["overlaps"].as_array().unwrap(); assert_eq!(o.len(), 3);
    // corner: the safe rect covers x 96..192, y 54..108 = 96 x 54 = 5184 of 20736 px
    close(&o[0]["zones"][0]["overlap_px"], 5184.0, 0.0); close(&o[0]["zones"][0]["overlap_pct"], 25.0, 1e-12); close(&o[0]["zones"][0]["outside_pct"], 75.0, 1e-12); close(&o[0]["outside_canvas_pct"], 0.0, 0.0);
    assert_eq!(o[1]["name"], json!("rect1")); close(&o[1]["zones"][0]["outside_pct"], 0.0, 0.0); close(&o[1]["area_px"], 10000.0, 0.0);
    // half past the left edge: on-canvas x 0..100 -> 50 % off the canvas; safe overlap x 96..100 = 4 x 100 = 400 px = 2 %
    close(&o[2]["outside_canvas_pct"], 50.0, 1e-12); close(&o[2]["zones"][0]["overlap_px"], 400.0, 1e-9); close(&o[2]["zones"][0]["outside_pct"], 98.0, 1e-12);
    close(&r["rects_outside_safe_max_pct"], 98.0, 1e-12); assert_eq!(r["rects_in_avoid_max_pct"], Value::Null);
    // the v5 keys are unchanged by rects
    let v5: Value = serde_json::from_str(V5[0].1).unwrap(); for (k, v) in v5.as_object().unwrap() { assert_eq!(&r[k], v, "{k}"); }
}
#[test] fn tiktok_estimate_overlaps_by_hand() {
    // tiktok on 1080x1920: action_rail [907.2, 768, 172.8, 1152], caption_and_sound_bar [0, 1382.4, 885.6, 537.6], safe [64.8, 192, 820.8, 1190.4]
    let mut s = session();
    let r = dispatch(&mut s, "query_safe_margins", &json!({"profile": "tiktok", "aspect": "9:16", "width": 1080, "rects": [{"name": "like", "rect": [950, 800, 100, 100]}, {"name": "title", "rect": [800, 1300, 200, 200]}]})).unwrap();
    let z = |i: usize, name: &str| r["overlaps"][i]["zones"].as_array().unwrap().iter().find(|z| z["zone"] == json!(name)).unwrap().clone();
    close(&z(0, "action_rail")["overlap_pct"], 100.0, 1e-9); close(&z(0, "safe")["outside_pct"], 100.0, 1e-9);
    close(&z(1, "caption_and_sound_bar")["overlap_px"], 85.6 * 117.6, 1e-6); close(&z(1, "caption_and_sound_bar")["overlap_pct"], 85.6 * 117.6 / 400.0, 1e-9);
    close(&z(1, "action_rail")["overlap_px"], 92.8 * 200.0, 1e-6); close(&z(1, "action_rail")["overlap_pct"], 46.4, 1e-9);
    // safe zone: x 800..885.6 = 85.6, y 1300..1382.4 = 82.4 inside
    close(&z(1, "safe")["overlap_px"], 85.6 * 82.4, 1e-6); close(&z(1, "top_bar")["overlap_px"], 0.0, 0.0);
    close(&r["rects_in_avoid_max_pct"], 100.0, 1e-9); close(&r["rects_outside_safe_max_pct"], 100.0, 1e-9);
}
#[test] fn custom_avoid_only_has_no_safe_number() {
    let mut s = session();
    let r = dispatch(&mut s, "query_safe_margins", &json!({"profile": "custom", "zones": [{"name": "logo", "rect": [1700, 40, 180, 90], "kind": "avoid"}], "rects": [{"rect": [1750, 60, 100, 50]}]})).unwrap();
    assert_eq!(r["rects_outside_safe_max_pct"], Value::Null); close(&r["rects_in_avoid_max_pct"], 100.0, 1e-12);
}
#[test] fn bad_rects_are_refused_never_clamped() {
    let mut s = session();
    for (a, frag) in [
        (json!({"rects": {"rect": [0, 0, 1, 1]}}), "array of 1..64"), (json!({"rects": []}), "array of 1..64"), (json!({"rects": vec![json!({"rect": [0, 0, 1, 1]}); 65]}), "array of 1..64"),
        (json!({"rects": [[0, 0, 1, 1]]}), "rects[0] must be"), (json!({"rects": [{"rect": [0, 0, 1]}]}), "[x,y,w,h]"), (json!({"rects": [{"rect": [0, 0, "1", 1]}]}), "[x,y,w,h]"),
        (json!({"rects": [{"rect": [0, 0, 0, 10]}]}), "> 0"), (json!({"rects": [{"rect": [0, 0, 10, -1]}]}), "> 0"), (json!({"rects": [{"rect": [70000, 0, 10, 10]}]}), "65536"),
        (json!({"rects": [{"name": "", "rect": [0, 0, 1, 1]}]}), "name must be"), (json!({"rects": [{"rect": [0, 0, 1, 1], "kind": "safe"}]}), "unknown key kind"),
        (json!({"profile": "nope", "rects": [{"rect": [0, 0, 1, 1]}]}), "invalid_profile"),
    ] { let e = dispatch(&mut s, "query_safe_margins", &a).unwrap_err(); assert!(e.contains(frag), "{a}: {e}"); }
}
#[test] fn crossed_edges_and_suggested_shift_by_hand() {
    // title_safe on 1920x1080 is [96, 54, 1728, 972]: right edge x 1824, bottom edge y 1026
    let mut s = session();
    let rects = json!([{"name": "corner", "rect": [0, 0, 192, 108]}, {"name": "inside", "rect": [500, 500, 100, 100]}, {"name": "edge", "rect": [-100, 500, 200, 100]}, {"name": "full", "rect": [0, 0, 1920, 1080]}, {"name": "br", "rect": [1800, 1000, 100, 60]}]);
    let r = dispatch(&mut s, "query_safe_margins", &json!({"rects": rects})).unwrap();
    let z = |i: usize| r["overlaps"][i]["zones"][0].clone();
    assert_eq!(z(0)["crossed_edges"], json!([{"edge": "top", "px": 54.0}, {"edge": "left", "px": 96.0}])); assert_eq!(z(0)["suggested_shift"], json!({"dx": 96.0, "dy": 54.0}));
    assert_eq!(z(1)["crossed_edges"], json!([])); assert_eq!(z(1)["suggested_shift"], json!({"dx": 0.0, "dy": 0.0}));
    assert_eq!(z(2)["crossed_edges"], json!([{"edge": "left", "px": 196.0}])); assert_eq!(z(2)["suggested_shift"], json!({"dx": 196.0, "dy": 0.0}));
    assert_eq!(z(3)["crossed_edges"], json!([{"edge": "top", "px": 54.0}, {"edge": "bottom", "px": 54.0}, {"edge": "left", "px": 96.0}, {"edge": "right", "px": 96.0}])); assert_eq!(z(3)["suggested_shift"], Value::Null);
    assert_eq!(z(4)["crossed_edges"], json!([{"edge": "bottom", "px": 34.0}, {"edge": "right", "px": 76.0}])); assert_eq!(z(4)["suggested_shift"], json!({"dx": -76.0, "dy": -34.0}));
    assert_eq!((r["worst_edge"].clone(), r["worst_edge_px"].clone(), r["worst_edge_rect"].clone(), r["worst_edge_zone"].clone()), (json!("left"), json!(196.0), json!("edge"), json!("title_safe")));
    // every suggested shift, applied, leaves the rect fully inside: a second call reports nothing crossed and outside_pct 0
    let moved: Vec<Value> = rects.as_array().unwrap().iter().enumerate().filter(|(i, _)| !z(*i)["suggested_shift"].is_null()).map(|(i, it)| { let rc = it["rect"].as_array().unwrap(); let sh = z(i)["suggested_shift"].clone();
        json!({"rect": [rc[0].as_f64().unwrap() + sh["dx"].as_f64().unwrap(), rc[1].as_f64().unwrap() + sh["dy"].as_f64().unwrap(), rc[2], rc[3]]}) }).collect();
    assert_eq!(moved.len(), 4); let m = dispatch(&mut s, "query_safe_margins", &json!({"rects": moved})).unwrap();
    for o in m["overlaps"].as_array().unwrap() { assert_eq!(o["zones"][0]["crossed_edges"], json!([])); close(&o["zones"][0]["outside_pct"], 0.0, 1e-12); }
    assert_eq!(m["worst_edge"], Value::Null); assert_eq!(m["worst_edge_px"], json!(0.0)); close(&m["rects_outside_safe_max_pct"], 0.0, 1e-12);
    // tiktok safe [64.8, 192, 820.8, 1190.4]: right edge 885.6, bottom edge 1382.4; avoid zones get no edge numbers
    let t = dispatch(&mut s, "query_safe_margins", &json!({"profile": "tiktok", "aspect": "9:16", "width": 1080, "rects": [{"name": "title", "rect": [800, 1300, 200, 200]}]})).unwrap();
    let safe = t["overlaps"][0]["zones"].as_array().unwrap().iter().find(|z| z["kind"] == json!("safe")).unwrap().clone();
    assert_eq!(safe["crossed_edges"], json!([{"edge": "bottom", "px": 117.6}, {"edge": "right", "px": 114.4}])); assert_eq!(safe["suggested_shift"], json!({"dx": -114.4, "dy": -117.6}));
    for zz in t["overlaps"][0]["zones"].as_array().unwrap() { if zz["kind"] == json!("avoid") { assert!(zz.get("crossed_edges").is_none() && zz.get("suggested_shift").is_none()); } }
    assert_eq!(t["worst_edge"], json!("bottom")); assert_eq!(t["worst_edge_px"], json!(117.6));
    // no safe zone in the profile: the edge scalars are null
    let c = dispatch(&mut s, "query_safe_margins", &json!({"profile": "custom", "zones": [{"name": "logo", "rect": [1700, 40, 180, 90], "kind": "avoid"}], "rects": [{"rect": [1750, 60, 100, 50]}]})).unwrap();
    assert_eq!((c["worst_edge"].clone(), c["worst_edge_px"].clone()), (Value::Null, Value::Null));
}
#[test] fn shorts_9_16_card_crossing_two_edges_gets_one_shift_that_clears_both() {
    // the YouTube Shorts estimate on a 1080x1920 canvas: safe [64.8, 192, 842.4, 1267.2] (right edge 907.2, bottom edge 1459.2)
    let mut s = session();
    let a = |rects: Value| json!({"profile": "shorts", "aspect": "9:16", "width": 1080, "rects": rects});
    let r = dispatch(&mut s, "query_safe_margins", &a(json!([{"name": "hook", "rect": [20, 150, 400, 100]}, {"name": "cta", "rect": [700, 1400, 300, 120]}]))).unwrap();
    let safe = |i: usize| r["overlaps"][i]["zones"].as_array().unwrap().iter().find(|z| z["kind"] == json!("safe")).unwrap().clone();
    // hook: top 192 - 150 = 42, left 64.8 - 20 = 44.8 -> one shift moves it right 44.8 and down 42
    assert_eq!(safe(0)["crossed_edges"], json!([{"edge": "top", "px": 42.0}, {"edge": "left", "px": 44.8}])); assert_eq!(safe(0)["suggested_shift"], json!({"dx": 44.8, "dy": 42.0}));
    // cta: bottom 1520 - 1459.2 = 60.8, right 1000 - 907.2 = 92.8
    assert_eq!(safe(1)["crossed_edges"], json!([{"edge": "bottom", "px": 60.8}, {"edge": "right", "px": 92.8}])); assert_eq!(safe(1)["suggested_shift"], json!({"dx": -92.8, "dy": -60.8}));
    assert_eq!((r["worst_edge"].clone(), r["worst_edge_px"].clone(), r["worst_edge_rect"].clone()), (json!("right"), json!(92.8), json!("cta")));
    // both shifts applied: inside the safe zone, nothing crossed
    let m = dispatch(&mut s, "query_safe_margins", &a(json!([{"rect": [64.8, 192, 400, 100]}, {"rect": [607.2, 1339.2, 300, 120]}]))).unwrap();
    for o in m["overlaps"].as_array().unwrap() { let z = o["zones"].as_array().unwrap().iter().find(|z| z["kind"] == json!("safe")).unwrap(); assert_eq!(z["crossed_edges"], json!([])); close(&z["outside_pct"], 0.0, 1e-9); }
    assert_eq!(m["worst_edge"], Value::Null); assert_eq!(m["worst_edge_px"], json!(0.0));
}
