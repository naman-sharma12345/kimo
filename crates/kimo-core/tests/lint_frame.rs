//! U4 independent slice: failing fixtures first, no text-layout guess.
use kimo_core::commands::{dispatch, dispatch_ex, Session};
use serde_json::{json, Value};
fn c(s: &mut Session, n: &str, a: Value) -> Value {
    dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}"))
}
fn scene() -> (Session, String) {
    let mut s = Session::new();
    c(
        &mut s,
        "project_set",
        json!({"width":160,"height":90,"fps":24}),
    );
    let t = c(&mut s, "add_track", json!({}))["track_id"]
        .as_str()
        .unwrap()
        .to_string();
    let id = c(
        &mut s,
        "add_shape",
        json!({"track_id":t,"shape":"rect","width":40,"height":20,"start":0,"duration":6}),
    )["clip_id"]
        .as_str()
        .unwrap()
        .to_string();
    c(
        &mut s,
        "set_meta",
        json!({"id":id,"name":"plate","role":"foreground"}),
    );
    (s, id)
}
fn findings(r: &Value) -> Vec<&Value> {
    r["frames"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|f| f["findings"].as_array().unwrap())
        .collect()
}
#[test]
fn forty_pixels_outside_and_suppression_count() {
    let (mut s, id) = scene();
    c(
        &mut s,
        "set_property",
        json!({"clip_id":id,"property":"x","value":100}),
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["out_of_bounds"],"annotate":false}),
    );
    let f = findings(&r);
    assert_eq!(f.len(), 1);
    assert_eq!(f[0]["measured"]["outside_px"]["right"], 40.);
    assert_eq!(f[0]["layer"]["name"], "plate");
    c(
        &mut s,
        "set_meta",
        json!({"id":id,"tags":["lint:allow_bleed"]}),
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["out_of_bounds"],"annotate":false}),
    );
    assert!(findings(&r).is_empty());
    assert_eq!(r["summary"]["ignored_by_tag"], 1);
}
#[test]
fn clean_zero_and_read_only() {
    let (mut s, _) = scene();
    let before = serde_json::to_vec(&s.project).unwrap();
    let h = c(&mut s, "history_list", json!({}));
    let r = c(&mut s, "lint_frame", json!({"time":1,"annotate":false}));
    assert!(findings(&r).is_empty(), "{r}");
    assert_eq!(before, serde_json::to_vec(&s.project).unwrap());
    assert_eq!(h, c(&mut s, "history_list", json!({})));
}
#[test]
fn zero_opacity_is_info_and_ignore_listed() {
    let (mut s, id) = scene();
    c(
        &mut s,
        "set_property",
        json!({"clip_id":id,"property":"opacity","value":0}),
    );
    let r = c(&mut s, "lint_frame", json!({"time":1,"annotate":false}));
    assert!(findings(&r)
        .iter()
        .any(|f| f["code"] == "invisible_layer" && f["severity"] == "info"));
    c(
        &mut s,
        "set_meta",
        json!({"id":id,"tags":["lint:ignore=invisible_layer"]}),
    );
    let r = c(&mut s, "lint_frame", json!({"time":1,"annotate":false}));
    assert_eq!(
        r["summary"]["suppressed"][0]["tag"],
        "lint:ignore=invisible_layer"
    );
}
#[test]
fn static_five_seconds_interval_and_override() {
    let (mut s, _) = scene();
    let r = c(
        &mut s,
        "lint_frame",
        json!({"range":{"start":0,"end":5,"step":0.5},"checks":["static_frame"],"annotate":false}),
    );
    assert_eq!(r["intervals"].as_array().unwrap().len(), 1);
    assert_eq!(r["intervals"][0]["t0"], 0.);
    assert_eq!(r["intervals"][0]["t1"], 5.);
    assert_eq!(r["intervals"][0]["worst"]["audio_present"], false);
    let r = c(
        &mut s,
        "lint_frame",
        json!({"range":{"start":0,"end":5},"checks":["static_frame"],"params":{"max_static_s":6},"annotate":false}),
    );
    assert!(r["intervals"].as_array().unwrap().is_empty());
    assert_eq!(r["params_used"]["max_static_s"], 6.);
}
#[test]
fn named_validation_errors() {
    let (mut s, _) = scene();
    for (a, code) in [
        (json!({"time":-1}), "out_of_range"),
        (json!({"time":1,"checks":["bogus"]}), "invalid_check"),
        (
            json!({"times":[0,1,2],"params":{"max_frames":2}}),
            "too_many_frames",
        ),
    ] {
        assert_eq!(
            dispatch_ex(&mut s, "lint_frame", &a).unwrap_err().code,
            code
        );
    }
}
#[test]
fn stable_across_threads() {
    let (mut s, id) = scene();
    c(
        &mut s,
        "set_property",
        json!({"clip_id":id,"property":"x","value":100}),
    );
    let a = json!({"times":[0,1,2],"checks":["out_of_bounds"],"annotate":false});
    kimo_core::par::set_threads(1);
    let mut x = c(&mut s, "lint_frame", a.clone());
    kimo_core::par::set_threads(2);
    let mut y = c(&mut s, "lint_frame", a);
    kimo_core::par::set_threads(0);
    for v in [&mut x, &mut y] {
        for f in v["frames"].as_array_mut().unwrap() {
            f.as_object_mut().unwrap().remove("elapsed_ms");
        }
    }
    assert_eq!(x, y);
}
#[test]
fn parent_scale_and_rotation_geometry() {
    let (mut s, id) = scene();
    let t = s.project.tracks[0].id.clone();
    let parent = c(
        &mut s,
        "add_null",
        json!({"track_id":t,"start":0,"duration":6,"scale":2,"rotation":90}),
    )["clip_id"]
        .as_str()
        .unwrap()
        .to_string();
    c(
        &mut s,
        "set_parent",
        json!({"clip_id":id,"parent_id":parent,"mode":"keep_local"}),
    );
    c(
        &mut s,
        "set_property",
        json!({"clip_id":id,"property":"x","value":50}),
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["out_of_bounds"],"annotate":false}),
    );
    let f = findings(&r);
    let hit = f.iter().find(|f| f["layer"]["id"] == id).unwrap();
    assert_eq!(hit["measured"]["outside_px"]["bottom"], 95.);
}
#[test]
fn annotations_write_image_and_limit_reports_omission() {
    let (mut s, id) = scene();
    c(
        &mut s,
        "set_property",
        json!({"clip_id":id,"property":"x","value":100}),
    );
    c(
        &mut s,
        "set_property",
        json!({"clip_id":id,"property":"opacity","value":0}),
    );
    let r = c(&mut s, "lint_frame", json!({"time":1,"max_findings":1}));
    assert_eq!(findings(&r).len(), 1);
    assert_eq!(r["summary"]["omitted_findings"], 1);
    let path = r["frames"][0]["annotation"]["path"].as_str().unwrap();
    assert!(std::fs::metadata(path).unwrap().len() > 0);
    assert_eq!(s.media.last().unwrap().path, path);
}
#[test]
fn adjacent_hits_merge_and_worst_is_largest_overflow() {
    let (mut s, id) = scene();
    c(
        &mut s,
        "set_keyframes",
        json!({"clip_id":id,"property":"x","keys":[{"t":0,"v":90},{"t":2,"v":110}]}),
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"times":[0,1,2],"checks":["out_of_bounds"],"annotate":false}),
    );
    assert_eq!(r["intervals"].as_array().unwrap().len(), 1);
    assert_eq!(r["intervals"][0]["worst"]["outside_px"]["right"], 50.);
}
#[test]
fn malformed_params_and_random_times_never_panic() {
    let (mut s, _) = scene();
    for v in [
        json!(null),
        json!([]),
        json!("bad"),
        json!({"max_frames":1.5}),
        json!({"max_static_s":-1}),
        json!({"unknown":1}),
    ] {
        assert!(dispatch(
            &mut s,
            "lint_frame",
            &json!({"time":0,"params":v,"annotate":false})
        )
        .is_err());
    }
    let mut seed = 5u64;
    for _ in 0..200 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        let t = (seed % 1600) as f64 / 100. - 5.;
        let a = json!({"time":t,"annotate":false,"params":{"opacity_epsilon":((seed>>32)%200)as f64/100.}});
        let _ = dispatch(&mut s, "lint_frame", &a);
    }
}
#[test]
fn checks_select_only_requested_and_cap_before_render() {
    let (mut s, id) = scene();
    c(
        &mut s,
        "set_property",
        json!({"clip_id":id,"property":"x","value":100}),
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"times":[0,1],"checks":[],"annotate":false}),
    );
    assert!(findings(&r).is_empty());
    assert_eq!(r["summary"]["frames_checked"], 2);
    assert!(dispatch(
        &mut s,
        "lint_frame",
        &json!({"range":{"start":0,"end":6,"step":0.000001}})
    )
    .unwrap_err()
    .starts_with("too_many_frames:"));
    assert!(
        dispatch(&mut s, "lint_frame", &json!({"time":1,"profile":"social"}))
            .unwrap_err()
            .starts_with("invalid_profile:")
    );
}

// U4 text slice: failing rows pre-staged before C1-C3 land. These exercise the
// public tools; no guessed font-size-to-cap-height ratio is used.
fn text_scene(color: [f64; 3], size: f64, w: u32, h: u32) -> (Session, String) {
    let mut s = Session::new();
    c(
        &mut s,
        "project_set",
        json!({"width":w,"height":h,"fps":25}),
    );
    let bg = c(&mut s, "add_track", json!({}))["track_id"]
        .as_str()
        .unwrap()
        .to_string();
    c(
        &mut s,
        "add_clip",
        json!({"track_id":bg,"source_type":"solid","color":[1,1,1],"start":0,"duration":3}),
    );
    let tr = c(&mut s, "add_track", json!({}))["track_id"]
        .as_str()
        .unwrap()
        .to_string();
    let id=c(&mut s,"add_text",json!({"track_id":tr,"text":"H title","font":"DejaVuSans","size":size,"color":color,"start":0,"duration":3}))["clip_id"].as_str().unwrap().to_string();
    c(
        &mut s,
        "set_meta",
        json!({"id":id,"name":"title","role":"title"}),
    );
    (s, id)
}
#[test]
fn text_contrast_known_ratios_threshold_and_read_only() {
    let (mut s, id) = text_scene([119. / 255.; 3], 24., 640, 360);
    let before = serde_json::to_vec(&s.project).unwrap();
    let hist = c(&mut s, "history_list", json!({}));
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["text_contrast"],"params":{"large_cap_px":10000},"annotate":false}),
    );
    let f = findings(&r);
    assert_eq!(f.len(), 1, "#777 white must warn at 4.5: {r}");
    assert!(
        (f[0]["measured"]["min_ratio"].as_f64().unwrap() - 4.478089).abs() < 1e-5,
        "{r}"
    );
    assert_eq!(f[0]["layer"]["name"], "title");
    assert_eq!(before, serde_json::to_vec(&s.project).unwrap());
    assert_eq!(hist, c(&mut s, "history_list", json!({})));
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["text_contrast"],"params":{"contrast_normal":4.47,"large_cap_px":10000},"annotate":false}),
    );
    assert!(findings(&r).is_empty(), "{r}");
    assert_eq!(r["params_used"]["contrast_normal"], 4.47);
    c(&mut s, "set_text", json!({"clip_id":id,"color":[0,0,0]}));
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["text_contrast"],"params":{"contrast_normal":22,"large_cap_px":10000},"annotate":false}),
    );
    assert!((findings(&r)[0]["measured"]["min_ratio"].as_f64().unwrap() - 21.).abs() < 1e-8);
}
#[test]
fn h_ink_size_exact_boundary_and_delivery_scaling() {
    let (mut s, id) = text_scene([0.; 3], 24., 640, 360);
    let m = c(
        &mut s,
        "compute_text_bounds",
        json!({"text":"H","font":"DejaVuSans","size":24}),
    );
    let cap = m["ink"][3].as_f64().unwrap() - m["ink"][1].as_f64().unwrap();
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["min_text_size"],"delivery_height":1080,"params":{"min_cap_px":cap*3.},"annotate":false}),
    );
    assert!(
        findings(&r).is_empty(),
        "exact threshold must not warn: {r}"
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["min_text_size"],"delivery_height":540,"params":{"min_cap_px":cap*3.+0.01},"annotate":false}),
    );
    let f = findings(&r);
    assert_eq!(f.len(), 1, "{r}");
    assert!(
        (f[0]["measured"]["cap_height_delivery_px"].as_f64().unwrap() - cap * 1.5).abs() < 1e-8
    );
    c(
        &mut s,
        "set_meta",
        json!({"id":id,"tags":["lint:ignore=min_text_size"]}),
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["min_text_size"],"delivery_height":540,"params":{"min_cap_px":cap*3.+0.01},"annotate":false}),
    );
    assert!(findings(&r).is_empty());
    assert_eq!(r["summary"]["ignored_by_tag"], 1);
}
#[test]
fn text_canvas_overflow_exact_forty_and_allow_bleed() {
    let (mut s, id) = text_scene([0.; 3], 24., 640, 360);
    let b = c(&mut s, "compute_text_bounds", json!({"clip_id":id,"t":1}));
    let right = b["canvas_rect"][0].as_f64().unwrap() + b["canvas_rect"][2].as_f64().unwrap();
    c(
        &mut s,
        "set_property",
        json!({"clip_id":id,"property":"x","value":680.-right}),
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["text_overflow"],"annotate":false}),
    );
    let f = findings(&r);
    assert_eq!(f.len(), 1, "{r}");
    assert!((f[0]["measured"]["outside_px"]["right"].as_f64().unwrap() - 40.).abs() < 1e-6);
    assert_eq!(f[0]["measured"]["scope"], "canvas_only");
    c(
        &mut s,
        "set_meta",
        json!({"id":id,"tags":["lint:ignore=text_overflow"]}),
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["text_overflow"],"annotate":false}),
    );
    assert!(findings(&r).is_empty());
    assert_eq!(r["summary"]["ignored_by_tag"], 1);
}
#[test]
fn safe_zone_title_geometry_and_platform_estimate_provenance() {
    let (mut s, id) = text_scene([0.; 3], 24., 360, 640);
    let b = c(&mut s, "compute_text_bounds", json!({"clip_id":id,"t":1}));
    c(
        &mut s,
        "set_property",
        json!({"clip_id":id,"property":"y","value":-b["canvas_rect"][1].as_f64().unwrap()+4.}),
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["safe_zone"],"profile":"title_safe","annotate":false}),
    );
    let f = findings(&r);
    assert!(!f.is_empty(), "{r}");
    assert!((f[0]["measured"]["outside_px"]["top"].as_f64().unwrap() - 28.).abs() < 1e-6);
    assert_eq!(f[0]["measured"]["estimated"], false);
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["safe_zone"],"profile":"tiktok","annotate":false}),
    );
    let f = findings(&r);
    assert!(!f.is_empty(), "{r}");
    assert!(f.iter().all(|f| f["measured"]["estimated"] == true
        && f["message"].as_str().unwrap().contains("estimate")));
}
#[test]
fn gradient_contrast_distribution_and_opacity_are_reported() {
    let (mut s, id) = text_scene([0.; 3], 24., 640, 360);
    let bg = std::sync::Arc::make_mut(&mut s.project.tracks[0].clips[0]);
    bg.source = kimo_core::model::Source::Gradient {
        c1: [0.; 3],
        c2: [1.; 3],
    };
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["text_contrast"],"params":{"contrast_normal":100,"large_cap_px":10000},"annotate":false}),
    );
    let f = findings(&r);
    assert_eq!(f.len(), 1, "{r}");
    let m = &f[0]["measured"];
    let min = m["min_ratio"].as_f64().unwrap();
    let p10 = m["p10_ratio"].as_f64().unwrap();
    let mean = m["mean_ratio"].as_f64().unwrap();
    assert!(
        min <= p10 && p10 <= mean && m["spread"].as_f64().unwrap() > 0.,
        "{r}"
    );
    assert!(m["worst_point"].is_array());
    std::sync::Arc::make_mut(&mut s.project.tracks[0].clips[0]).source =
        kimo_core::model::Source::Solid { color: [1.; 3] };
    c(
        &mut s,
        "set_property",
        json!({"clip_id":id,"property":"opacity","value":0.5}),
    );
    let r = c(
        &mut s,
        "lint_frame",
        json!({"time":1,"checks":["text_contrast"],"params":{"large_cap_px":10000},"annotate":false}),
    );
    assert!(
        (findings(&r)[0]["measured"]["min_ratio"].as_f64().unwrap() - 3.976653).abs() < 1e-5,
        "{r}"
    );
}

// ---- ARCH ruling 11:10: lint_frame annotation legibility ----
fn ann_scene(w: u32, h: u32, bg: f64) -> Session {
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":w,"height":h,"fps":24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id":t,"source_type":"gradient","color":[bg,bg,bg],"color2":[bg,bg,bg],"start":0,"duration":6}));
    let t2 = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let id = c(&mut s, "add_text", json!({"track_id":t2,"text":"Too close to the edge","start":0,"duration":6,"size":(h as f64) / 8.0,"color":[0.5,0.5,0.5]}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "set_property", json!({"clip_id":id,"property":"x","value":(w as f64) * 0.42}));
    s
}
fn ann_lint(s: &mut Session, annotate: bool) -> Value { c(s, "lint_frame", json!({"time":1,"checks":["safe_zone"],"profile":"title_safe","annotate":annotate})) }
#[test]
fn annotation_stroke_follows_the_formula_at_three_sizes() {
    for (w, h, want) in [(256u32, 256u32, 2i64), (480, 270, 2), (1920, 1080, 2)] {
        let mut s = ann_scene(w, h, 0.0);
        let r = ann_lint(&mut s, true);
        let a = &r["frames"][0]["annotation"];
        assert_eq!(a["stroke_px"], want, "{w}x{h}: max(2, round(0.25u)), u = min(w,h)/135");
    }
}
#[test]
fn annotation_box_contrast_is_at_least_3_to_1_on_white_and_black() {
    for bg in [0.0, 1.0] {
        let mut s = ann_scene(256, 256, bg);
        let a = ann_lint(&mut s, true)["frames"][0]["annotation"].clone();
        assert!(a["min_contrast_vs_frame"].as_f64().unwrap() >= 3.0, "bg {bg}: {a}");
    }
}
#[test]
fn annotation_chip_is_inside_the_canvas_clear_of_the_box_and_labels_the_measure() {
    let mut s = ann_scene(256, 256, 0.0);
    let r = ann_lint(&mut s, true); let a = &r["frames"][0]["annotation"];
    let chips = a["chips"].as_array().unwrap(); assert!(!chips.is_empty());
    let c0: Vec<f64> = chips[0]["rect"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    assert!(c0[0] >= 0.0 && c0[1] >= 0.0 && c0[0] + c0[2] <= 256.0 && c0[1] + c0[3] <= 256.0, "chip inside canvas {c0:?}");
    let v: Vec<f64> = a["boxes"][0]["rect"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
    let hit = c0[0] < v[0] + v[2] && v[0] < c0[0] + c0[2] && c0[1] < v[1] + v[3] && v[1] < c0[1] + c0[3];
    assert!(!hit, "chip {c0:?} is clear of the rect {v:?}");
    let t = chips[0]["text"].as_str().unwrap(); assert!(t.starts_with("safe_zone -") && t.contains("px "), "{t}");
    assert!(a["zone_lines"].as_i64().unwrap() >= 1, "the violated safe-zone edge is drawn: {a}");
    assert!(a["edge_markers"].as_i64().unwrap() >= 1, "the canvas-clipped edge gets a marker: {a}");
}
#[test]
fn annotation_never_changes_the_findings() {
    let mut s = ann_scene(256, 256, 0.0);
    let (on, off) = (ann_lint(&mut s, true), ann_lint(&mut s, false));
    assert_eq!(on["frames"][0]["findings"], off["frames"][0]["findings"]);
    assert!(off["frames"][0]["annotation"].is_null());
}
