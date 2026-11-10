//! v6 eyes item #3: get_scopes numeric arguments (percentiles, waveform_columns, zones, crush_below, clip_above).
//! v5 output pinned byte for byte (bytes captured over MCP from the v5.0.0 5ad355c kimo-mcp), known references (numpy's
//! linear percentile on a hand-made ramp, BT.709 Cb/Cr of pure red / blue / white, a solid with a white rect of known size
//! and position), zone counts that sum to the v5 whole-frame clip %, thresholds as arguments, and refusals (never clamped).
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::scopes_numeric::{extend, parse, percentile_sorted};
use serde_json::{json, Value};

fn session() -> Session {
    let mut s = Session::new(); dispatch(&mut s, "project_set", &json!({"width": 320, "height": 180, "fps": 24})).unwrap();
    let t1 = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    dispatch(&mut s, "add_clip", &json!({"track_id": t1, "source_type": "solid", "color": [0.2, 0.4, 0.6], "start": 0, "duration": 1})).unwrap();
    dispatch(&mut s, "add_clip", &json!({"track_id": t1, "source_type": "gradient", "color": [1, 0, 0], "color2": [0, 0, 1], "start": 1, "duration": 1})).unwrap();
    let t2 = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    dispatch(&mut s, "add_shape", &json!({"track_id": t2, "shape": "rect", "width": 40, "height": 30, "fill": [1, 1, 1, 1], "start": 0, "duration": 1})).unwrap(); s
}
/// get_scopes output from the v5.0.0 (5ad355c) kimo-mcp over MCP (tools/call text) for the session above.
const V5: [(&str, &str); 3] = [
    (r#"{"time": 0.5}"#, r#"{"avg_rgb":[0.21666666958481073,0.41250000583628815,0.6083333566784859],"clipped_whites_pct":2.0833333333333335,"crushed_blacks_pct":0.0,"histogram_b":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,56400,0,0,0,0,0,0,0,0,0,0,0,1200],"histogram_g":[0,0,0,0,0,0,0,0,0,0,0,0,56400,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1200],"histogram_r":[0,0,0,0,0,0,56400,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1200],"luma_mean":0.38500498980283737,"luma_std":0.08970627236281896,"luma_waveform_min_max_per_column":[[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,1.0],[0.37191998958587646,1.0],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646],[0.37191998958587646,0.37191998958587646]]}"#),
    (r#"{"time": 1.5}"#, r#"{"avg_rgb":[0.5027777783960725,0.0,0.4972222228405169],"clipped_whites_pct":0.0,"crushed_blacks_pct":0.0,"histogram_b":[1920,1920,1600,1920,1920,1600,1920,1920,1600,1920,1600,1920,1920,1600,1920,1920,1600,1920,1600,1920,1920,1600,1920,1920,1600,1920,1600,1920,1920,1600,1920,1600],"histogram_g":[57600,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"histogram_r":[1600,1920,1600,1920,1920,1600,1920,1920,1600,1920,1600,1920,1920,1600,1920,1920,1600,1920,1600,1920,1920,1600,1920,1920,1600,1920,1600,1920,1920,1600,1920,1920],"luma_mean":0.14278999670512146,"luma_std":0.040529360554622175,"luma_waveform_min_max_per_column":[[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752],[0.07531999796628952,0.2125999927520752]]}"#),
    (r#"{"time": 2.5}"#, r#"{"avg_rgb":[0.0,0.0,0.0],"clipped_whites_pct":0.0,"crushed_blacks_pct":100.0,"histogram_b":[57600,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"histogram_g":[57600,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"histogram_r":[57600,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0],"luma_mean":0.0,"luma_std":0.0,"luma_waveform_min_max_per_column":[[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0],[0.0,0.0]]}"#)
];
#[test] fn get_scopes_without_new_args_is_byte_identical_to_v5() {
    let mut s = session();
    for (args, want) in V5 { let a: Value = serde_json::from_str(args).unwrap(); assert_eq!(serde_json::to_string(&dispatch(&mut s, "get_scopes", &a).unwrap()).unwrap(), want, "{args}"); }
    // explicit nulls for the new arguments are absent arguments: same v5 bytes
    let a = json!({"time": 0.5, "percentiles": null, "waveform_columns": null, "zones": null, "crush_below": null, "clip_above": null});
    assert_eq!(serde_json::to_string(&dispatch(&mut s, "get_scopes", &a).unwrap()).unwrap(), V5[0].1);
}
fn frame(px: &[[f32; 3]], w: usize, h: usize) -> Vec<f32> { assert_eq!(px.len(), w * h); px.iter().flat_map(|p| [p[0], p[1], p[2], 1.0]).collect() }
fn run(a: Value, px: &[[f32; 3]], w: usize, h: usize) -> Value { let o = parse(&a).unwrap().unwrap(); let mut out = json!({}); extend(&o, &frame(px, w, h), w, h, &mut out).unwrap(); out }
fn close(v: &Value, want: f64, tol: f64) { let g = v.as_f64().unwrap_or(f64::NAN); assert!((g - want).abs() <= tol, "{g} vs {want}"); }

#[test] fn percentiles_follow_numpy_linear() {
    // numpy.percentile([0, .25, .5, 1], [0, 25, 50, 75, 100]) = [0, .1875, .375, .625, 1]; gray pixels so Y' = v (f32 sum of the weights)
    let px: Vec<[f32; 3]> = [0.0f32, 0.25, 0.5, 1.0].iter().map(|&v| [v, v, v]).collect();
    let r = run(json!({"percentiles": [0, 25, 50, 75, 100]}), &px, 4, 1);
    for (got, want) in r["luma_percentiles"].as_array().unwrap().iter().zip([0.0, 0.1875, 0.375, 0.625, 1.0]) { close(got, want, 1e-6); }
    assert_eq!(r["percentiles"], json!([0.0, 25.0, 50.0, 75.0, 100.0]));
    // numpy.percentile([10, 20, 30, 40, 50], 5) = 12.0; numpy.percentile([1, 2], 50) = 1.5; one value is every percentile
    assert_eq!(percentile_sorted(&[10., 20., 30., 40., 50.], 5.0), 12.0); assert_eq!(percentile_sorted(&[1., 2.], 50.0), 1.5); assert_eq!(percentile_sorted(&[7.], 99.0), 7.0);
    // gray has no chroma; 4 columns each hold one pixel
    for c in r["chroma_percentiles"].as_array().unwrap() { close(c, 0.0, 1e-7); }
    // the default 16 columns fall back to the frame width on a 4 px frame (echoed); a given width is never clamped
    assert_eq!(r["waveform_columns"], json!(4)); assert_eq!(r["waveform_percentiles"].as_array().unwrap().len(), 4); close(&r["waveform_percentiles"][3][2], 1.0, 1e-6); close(&r["waveform_percentiles"][1][0], 0.25, 1e-6);
}
#[test] fn bt709_cb_cr_of_red_blue_white() {
    // BT.709: red Y' .2126 Cb -.114572 Cr .5; blue Y' .0722 Cb .5 Cr -.045847; white Y' 1 Cb 0 Cr 0
    for (rgb, cb, cr) in [([1f32, 0., 0.], -0.2126 / 1.8556, 0.5), ([0., 0., 1.], 0.5, -0.0722 / 1.5748), ([1., 1., 1.], 0.0, 0.0)] {
        let r = run(json!({"percentiles": [50]}), &[rgb; 4], 2, 2);
        close(&r["cb_mean"], cb, 1e-6); close(&r["cr_mean"], cr, 1e-6); close(&r["chroma_mean"], (cb * cb + cr * cr).sqrt(), 1e-6); close(&r["chroma_percentiles"][0], (cb * cb + cr * cr).sqrt(), 1e-6);
    }
}
#[test] fn solid_with_white_rect_known_numbers() {
    // 320x180: solid (.2, .4, .6) -> Y' .37192, Cb .1229144, Cr -.1091694; a 40x30 white rect at the centre (1200 px = 2.0833 %)
    let mut s = session(); let r = dispatch(&mut s, "get_scopes", &json!({"time": 0.5, "percentiles": [1, 50, 97, 98, 99], "zones": [2, 2], "waveform_columns": 8})).unwrap();
    let y = 0.2126 * 0.2 + 0.7152 * 0.4 + 0.0722 * 0.6; let (cb, cr) = ((0.6 - y) / 1.8556, (0.2 - y) / 1.5748);
    let lp = r["luma_percentiles"].as_array().unwrap(); close(&lp[0], y, 1e-6); close(&lp[1], y, 1e-6); close(&lp[2], y, 1e-6); close(&lp[4], 1.0, 1e-6);
    // p98: index .98 * 57599 = 56447.02 sits past the 56400 solid pixels -> white
    close(&lp[3], 1.0, 1e-6);
    let f = 56400.0 / 57600.0; close(&r["cb_mean"], cb * f, 1e-6); close(&r["cr_mean"], cr * f, 1e-6); close(&r["chroma_mean"], (cb * cb + cr * cr).sqrt() * f, 1e-6);
    // the rect straddles the centre: 20x15 white px in each 160x90 quadrant = 2.0833 %, and the 4 zones average to the v5 whole-frame %
    for z in r["zone_clipped_whites_pct"].as_array().unwrap() { close(z, 300.0 * 100.0 / 14400.0, 1e-9); }
    close(&r["zone_clipped_whites_max_pct"], 300.0 * 100.0 / 14400.0, 1e-9); close(&r["clipped_whites_pct"], 1200.0 * 100.0 / 57600.0, 1e-9);
    assert_eq!(r["zone_grid"], json!([2, 2])); assert_eq!(r["crush_below"], json!(0.02)); assert_eq!(r["clip_above"], json!(0.98));
    // waveform: 8 columns of 40 px; columns 3 and 4 hold the rect (20 of 40 px x 30 of 180 rows = 1/12 white), the rest are solid
    let wf = r["waveform_percentiles"].as_array().unwrap(); assert_eq!(wf.len(), 8); close(&wf[0][4], y, 1e-6); close(&wf[3][4], 1.0, 1e-6); close(&wf[3][1], y, 1e-6);
    // the v5 keys are untouched by the new arguments
    let v5: Value = serde_json::from_str(V5[0].1).unwrap(); for (k, v) in v5.as_object().unwrap() { assert_eq!(&r[k], v, "{k}"); }
}
#[test] fn zones_sum_to_the_whole_frame_and_thresholds_are_arguments() {
    let mut s = session();
    let r = dispatch(&mut s, "get_scopes", &json!({"time": 1.5, "zones": [5, 3]})).unwrap();
    let zc = r["zone_crushed_blacks_pct"].as_array().unwrap(); assert_eq!(zc.len(), 15); assert!(r.get("luma_percentiles").is_none());
    // 320/5 = 64 and 180/3 = 60: equal areas, so the zone mean is the whole-frame %
    let mean = zc.iter().map(|v| v.as_f64().unwrap()).sum::<f64>() / 15.0; close(&Value::from(mean), r["crushed_blacks_pct"].as_f64().unwrap(), 1e-9);
    // the red->blue gradient: Y' runs .2126 .. .0722, so everything is under crush_below .25 and nothing over clip_above .2
    let r2 = dispatch(&mut s, "get_scopes", &json!({"time": 1.5, "zones": [1, 1], "crush_below": 0.25, "clip_above": 0.25})).unwrap();
    close(&r2["zone_crushed_blacks_pct"][0], 100.0, 1e-9); close(&r2["zone_clipped_whites_pct"][0], 0.0, 1e-9);
    let r3 = dispatch(&mut s, "get_scopes", &json!({"time": 1.5, "zones": [1, 1], "crush_below": 0.0, "clip_above": 1.0})).unwrap();
    close(&r3["zone_crushed_blacks_pct"][0], 0.0, 1e-9); close(&r3["zone_clipped_whites_pct"][0], 0.0, 1e-9);
}
#[test] fn bad_arguments_are_refused_never_clamped() {
    let mut s = session();
    for (a, frag) in [
        (json!({"percentiles": 50}), "array"), (json!({"percentiles": []}), "1..32"), (json!({"percentiles": (0..33).collect::<Vec<i32>>()}), "1..32"),
        (json!({"percentiles": [101]}), "0..100"), (json!({"percentiles": [-1]}), "0..100"), (json!({"percentiles": ["50"]}), "0..100"),
        (json!({"percentiles": [50, 10]}), "ascending"), (json!({"percentiles": [10, 10]}), "ascending"),
        (json!({"waveform_columns": 8}), "needs percentiles"), (json!({"percentiles": [50], "waveform_columns": 0}), "1..256"),
        (json!({"percentiles": [50], "waveform_columns": 257}), "1..256"), (json!({"percentiles": [50], "waveform_columns": 2.5}), "1..256"),
        (json!({"zones": [0, 1]}), "1..32"), (json!({"zones": [33, 1]}), "1..32"), (json!({"zones": [2]}), "[cols, rows]"), (json!({"zones": "2x2"}), "[cols, rows]"),
        (json!({"zones": [2.5, 2]}), "1..32"), (json!({"crush_below": 0.1}), "applies to zones"), (json!({"clip_above": 0.9}), "applies to zones"),
        (json!({"zones": [2, 2], "crush_below": 1.5}), "0..1"), (json!({"zones": [2, 2], "crush_below": -0.1}), "0..1"), (json!({"zones": [2, 2], "clip_above": "x"}), "0..1"),
    ] { let mut a = a; a["time"] = json!(0.5); let e = dispatch(&mut s, "get_scopes", &a).unwrap_err(); assert!(e.contains(frag), "{a}: {e}"); }
    // frame-size checks happen after the render: a 320 px frame cannot hold 321 waveform columns... 256 is the cap, so use a tiny project
    let mut t = Session::new(); dispatch(&mut t, "project_set", &json!({"width": 16, "height": 16, "fps": 24})).unwrap();
    let e = dispatch(&mut t, "get_scopes", &json!({"time": 0, "percentiles": [50], "waveform_columns": 17})).unwrap_err(); assert!(e.contains("exceeds the frame width"), "{e}");
    let e = dispatch(&mut t, "get_scopes", &json!({"time": 0, "zones": [8, 17]})).unwrap_err(); assert!(e.contains("exceed the frame"), "{e}");
    // zones cap is 32 so 17 rows on a 16 px frame exceeds it
}
