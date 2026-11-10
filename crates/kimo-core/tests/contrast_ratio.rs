//! v6 eyes item #3: contrast_ratio against known references: WCAG black/white = 21:1 and #777 on white = 4.478 (4.48:1 as
//! WCAG tools print it), colour forms (#rgb, #rrggbb, [r,g,b]), symmetry, rects of a rendered frame of known content,
//! Machado 2009 CVD ratios equal to an independent run of colorspacious (MIT, its own copy of the published matrices,
//! cspace_convert sRGB1+CVD -> sRGB1-linear, clipped to 0..1), the severity interpolation, and refusals (never clamped).
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::contrast::{machado, ratio, rel_lum, DEUTAN};
use serde_json::{json, Value};

fn cr(a: Value) -> Value { let mut s = Session::new(); dispatch(&mut s, "contrast_ratio", &a).unwrap() }
fn close(v: &Value, want: f64, tol: f64) { let g = v.as_f64().unwrap_or(f64::NAN); assert!((g - want).abs() <= tol, "{g} vs {want}"); }

#[test] fn wcag_reference_pairs() {
    let r = cr(json!({"a": "#000000", "b": "#ffffff"})); close(&r["ratio"], 21.0, 1e-12); close(&r["luminance_a"], 0.0, 0.0); close(&r["luminance_b"], 1.0, 1e-12); assert_eq!(r["lighter"], json!("b"));
    let r = cr(json!({"a": "#777", "b": "#FFFFFF"})); close(&r["ratio"], 4.478089453577214, 1e-12); assert_eq!(format!("{:.2}", r["ratio"].as_f64().unwrap()), "4.48");
    assert_eq!(r["a"]["hex"], json!("#777")); assert_eq!(r["b"]["hex"], json!("#ffffff"));
    // same pair, other forms and order
    let r2 = cr(json!({"a": [1, 1, 1], "b": "#777777"})); close(&r2["ratio"], 4.478089453577214, 1e-12); assert_eq!(r2["lighter"], json!("a")); assert_eq!(r2["a"]["hex"], json!("#ffffff"));
    // equal colours: 1:1
    close(&cr(json!({"a": "#336699", "b": [0.2, 0.4, 0.6]}))["ratio"], 1.0, 1e-12);
    assert_eq!(ratio(0.0, 1.0), 21.0); close(&Value::from(rel_lum([0.2, 0.4, 0.6], None)), 0.1250645743288924_f64, 1e-12);
}
#[test] fn machado_cvd_matches_colorspacious() {
    // red #ff0000 vs green [0, .5, 0]; no cvd 1.293072309583203 (colorspacious sRGB1 -> sRGB1-linear, same EOTF)
    let r = cr(json!({"a": "#ff0000", "b": [0, 0.5, 0], "cvd": ["protan", "deutan", "tritan"]}));
    close(&r["ratio"], 1.293072309583203, 1e-12); close(&r["severity"], 1.0, 0.0);
    close(&r["ratio_protan"], 1.3286997940742604, 1e-9); close(&r["ratio_deutan"], 1.7035862703006421, 1e-9); close(&r["ratio_tritan"], 1.2941667036754487, 1e-9);
    close(&r["cvd"]["deutan"]["ratio"], 1.7035862703006421, 1e-9);
    let r = cr(json!({"a": "#ff0000", "b": [0, 0.5, 0], "cvd": ["tritan", "deutan", "protan"], "severity": 0.531}));
    close(&r["ratio_protan"], 1.0013429825798639, 1e-9); close(&r["ratio_deutan"], 1.5111223344409925, 1e-9); close(&r["ratio_tritan"], 1.3035519132400026, 1e-9);
    // interpolation: 0.531 = 0.69 * step 5 + 0.31 * step 6 (the colorspacious test case); severity 0 is the identity
    let m = machado("deutan", 0.531).unwrap(); for i in 0..3 { for j in 0..3 { assert!((m[i][j] - (0.69 * DEUTAN[5][i][j] + 0.31 * DEUTAN[6][i][j])).abs() < 1e-12); } }
    assert_eq!(machado("protan", 1.0).unwrap()[0][0], 0.152286); assert_eq!(machado("protan", 0.0).unwrap(), [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
    close(&cr(json!({"a": "#ff0000", "b": [0, 0.5, 0], "cvd": ["deutan"], "severity": 0}))["ratio_deutan"], 1.293072309583203, 1e-12);
    assert!(cr(json!({"a": "#000", "b": "#fff"})).get("cvd").is_none());
}
fn session() -> Session {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width": 320, "height": 180, "fps": 24})).unwrap();
    let t1 = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    dispatch(&mut s, "add_clip", &json!({"track_id": t1, "source_type": "solid", "color": [0.2, 0.4, 0.6], "start": 0, "duration": 1})).unwrap();
    let t2 = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    dispatch(&mut s, "add_shape", &json!({"track_id": t2, "shape": "rect", "width": 40, "height": 30, "fill": [1, 1, 1, 1], "start": 0, "duration": 1})).unwrap(); s
}
#[test] fn rects_of_a_rendered_frame() {
    // the white 40x30 rect sits at x 140..179, y 75..104 (checked on the rendered PNG); the solid is (.2, .4, .6), L .1250646
    let mut s = session(); let ls = 0.1250645743288924;
    let r = dispatch(&mut s, "contrast_ratio", &json!({"a": {"rect": [150, 80, 20, 20]}, "b": {"rect": [0, 0, 40, 30]}, "time": 0.5})).unwrap();
    close(&r["luminance_a"], 1.0, 1e-6); close(&r["luminance_b"], ls, 1e-6); close(&r["ratio"], 1.05 / (ls + 0.05), 1e-5);
    assert_eq!(r["a"]["pixels"], json!(400)); assert_eq!(r["a"]["kind"], json!("rect")); close(&r["a"]["luminance_p5"], 1.0, 1e-6); close(&r["time"], 0.5, 0.0);
    // a rect straddling the white edge: 10 of 40 columns solid -> mean L = (300 ls + 900) / 1200, p5 = ls, max = 1
    let r = dispatch(&mut s, "contrast_ratio", &json!({"a": {"rect": [130, 75, 40, 30]}, "b": "#000000", "time": 0.5})).unwrap();
    close(&r["luminance_a"], (300.0 * ls + 900.0) / 1200.0, 1e-6); close(&r["a"]["luminance_p5"], ls, 1e-6); close(&r["a"]["luminance_max"], 1.0, 1e-6); close(&r["a"]["luminance_min"], ls, 1e-6);
    close(&r["ratio"], ((300.0 * ls + 900.0) / 1200.0 + 0.05) / 0.05, 1e-4);
    // a rect of the solid equals the solid's colour
    let r = dispatch(&mut s, "contrast_ratio", &json!({"a": {"rect": [0, 0, 100, 50]}, "b": [0.2, 0.4, 0.6], "time": 0.5, "cvd": ["deutan"]})).unwrap();
    close(&r["ratio"], 1.0, 1e-6); close(&r["ratio_deutan"], 1.0, 1e-6);
}
#[test] fn bad_arguments_are_refused_never_clamped() {
    let mut s = session();
    for (a, frag) in [
        (json!({"b": "#fff"}), "a must be"), (json!({"a": "#fff"}), "b must be"), (json!({"a": "fff", "b": "#000"}), "a must be"), (json!({"a": "#ffff", "b": "#000"}), "a must be"),
        (json!({"a": "#ggg", "b": "#000"}), "a must be"), (json!({"a": [1, 1], "b": "#000"}), "a must be"), (json!({"a": [1.2, 0, 0], "b": "#000"}), "a must be"), (json!({"a": [-0.1, 0, 0], "b": "#000"}), "a must be"),
        (json!({"a": {"rect": [0, 0, 10]}, "b": "#000", "time": 0}), "a must be"), (json!({"a": {"rect": [0, 0, 0, 10]}, "b": "#000", "time": 0}), "at least 1"), (json!({"a": {"rect": [0.5, 0, 10, 10]}, "b": "#000", "time": 0}), "a must be"),
        (json!({"a": {"rect": [0, 0, 10, 10], "x": 1}, "b": "#000", "time": 0}), "a must be"), (json!({"a": {"rect": [300, 0, 30, 10]}, "b": "#000", "time": 0}), "inside the 320x180"),
        (json!({"a": {"rect": [0, 0, 10, 10]}, "b": "#000"}), "needs time"), (json!({"a": "#fff", "b": "#000", "time": 1}), "time applies"), (json!({"a": {"rect": [0, 0, 1, 1]}, "b": "#000", "time": -1}), "time must"),
        (json!({"a": "#fff", "b": "#000", "cvd": "deutan"}), "cvd must"), (json!({"a": "#fff", "b": "#000", "cvd": []}), "cvd must"), (json!({"a": "#fff", "b": "#000", "cvd": ["achromat"]}), "Valid: protan, deutan, tritan"),
        (json!({"a": "#fff", "b": "#000", "cvd": ["deutan", "deutan"]}), "twice"), (json!({"a": "#fff", "b": "#000", "severity": 0.5}), "applies to cvd"),
        (json!({"a": "#fff", "b": "#000", "cvd": ["deutan"], "severity": 1.5}), "0..1"), (json!({"a": "#fff", "b": "#000", "cvd": ["deutan"], "severity": -0.1}), "0..1"),
    ] { let e = dispatch(&mut s, "contrast_ratio", &a).unwrap_err(); assert!(e.contains(frag), "{a}: {e}"); }
}
