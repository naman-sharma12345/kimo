//! v6 eyes: text_contrast_series, the worst-frame WCAG 2.x contrast of text over moving video. Known references: white text
//! over a 3 s timeline of black / grey / white solids (21:1, the grey's ratio, 1:1; worst at 2 s = frame 48, ties to the
//! earliest frame), the threshold counts worked out by hand, every 2nd frame plus every cut, the scrim suggestion (never a recolour), captions by name with the cuts they cross, detect_cuts' motion guard, the size class bars, the cut flags, the background ring geometry (text box left out, clipped at the
//! frame edge), the bg_stat choices on a synthetic frame, and refused arguments.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::contrast::{ratio, rel_lum};
use kimo_core::ext::text_contrast::{background_lums, frame_ratio, glyph_px};
use kimo_core::perception::analysis::Image;
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn timeline() -> Session {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    for (k, col) in [[0., 0., 0.], [0.5, 0.5, 0.5], [1., 1., 1.]].iter().enumerate() { c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": col, "start": k as f64, "duration": 1.0})); }
    s
}
fn close(a: f64, b: f64) -> bool { (a - b).abs() < 1e-9 }

#[test] fn white_text_over_black_grey_white() {
    let mut s = timeline();
    let r = c(&mut s, "text_contrast_series", json!({"text": "#ffffff", "text_box": [16, 10, 32, 16], "threshold": 4.5, "series": true}));
    assert_eq!(r["frames_sampled"], 72); assert_eq!(r["bg_pixels"], 48 * 32 - 32 * 16); assert_eq!(r["ring_px"], 8);
    assert!(close(r["best_ratio"].as_f64().unwrap(), 21.0) && close(r["best_time"].as_f64().unwrap(), 0.0), "{r}");
    // white over white: 1:1 from 2 s; every white frame ties, the earliest wins
    assert!(close(r["worst_ratio"].as_f64().unwrap(), 1.0), "{r}"); assert_eq!(r["worst_frame"], 48); assert!(close(r["worst_time"].as_f64().unwrap(), 2.0));
    assert!(close(r["worst_text_luminance"].as_f64().unwrap(), 1.0) && close(r["worst_bg_luminance"].as_f64().unwrap(), 1.0));
    // the grey second: the output pixels (what render_frame writes, 8-bit), so grey 0.5 reads as 128/255: L 0.21586, 3.9494:1.
    // contrast_ratio reads the pre-output float frame (0.5 exactly, 3.9767:1); the 0.03 gap is the 8-bit step, stated here.
    let g = ratio(1.0, rel_lum([128.0 / 255.0; 3], None));
    let row = r["series"].as_array().unwrap().iter().find(|x| x["frame"] == 36).unwrap().clone();
    assert!((row["ratio"].as_f64().unwrap() - g).abs() < 1e-9, "{row} vs {g}"); assert!(g > 3.0 && g < 4.5, "{g}");
    let cr = c(&mut s, "contrast_ratio", json!({"a": "#ffffff", "b": {"rect": [0, 0, 64, 36]}, "time": 1.5}))["ratio"].as_f64().unwrap(); assert!((cr - ratio(1.0, rel_lum([0.5; 3], None))).abs() < 1e-9 && (cr - g).abs() < 0.03, "{cr}");
    assert_eq!(r["cut_frames_added"], 0); assert_eq!(r["bg_region"], "ring");
    // below the bar: the fix is a scrim, never a recolour. Worst = white over white (L 1): a black scrim must bring the
    // background to (1.05 / 4.5) - 0.05 = 0.18333, so at least 81.67 % opacity in linear light
    assert_eq!(r["suggested_fix"], "scrim"); assert_eq!(r["scrim_color"], json!([0.0, 0.0, 0.0]));
    assert!(close(r["scrim_alpha_min"].as_f64().unwrap(), 1.0 - (1.05 / 4.5 - 0.05)), "{}", r["scrim_alpha_min"]); assert!(r["fix_note"].as_str().unwrap().contains("do not recolour"));
    // threshold 4.5: the 24 grey + 24 white frames are below, first at 1 s, one run of 48 frames = 2 s
    assert_eq!(r["frames_below"], 48); assert!(close(r["pct_below"].as_f64().unwrap(), 200.0 / 3.0)); assert!(close(r["first_below_time"].as_f64().unwrap(), 1.0)); assert!(close(r["longest_below_s"].as_f64().unwrap(), 2.0));
    assert_eq!(r["series"].as_array().unwrap().len(), 72);
    // no averages; the two cuts (black -> grey at 1 s, grey -> white at 2 s) are inside the range
    assert!(r.get("mean_ratio").is_none() && r.get("median_ratio").is_none());
    assert_eq!(r["cut_count"], 2); assert_eq!(r["cut_times"], json!([1.0, 2.0])); assert_eq!(r["crosses_cut"], true);
    // size class at 1080 p: the 36 px short side scales x30; 0.8 px -> 24 px large (bar 3), 0.7 px -> 21 px normal (4.5), bold 21 px large
    let big = c(&mut s, "text_contrast_series", json!({"text": "#fff", "text_box": [16, 10, 32, 16], "text_px": 0.8, "step_frames": 6}));
    assert_eq!(big["size_class"], "large"); assert_eq!(big["threshold"], 3.0); assert_eq!(big["threshold_source"], "text_px"); assert_eq!(big["frames_below"], 4);
    let small = c(&mut s, "text_contrast_series", json!({"text": "#fff", "text_box": [16, 10, 32, 16], "text_px": 0.7, "step_frames": 6}));
    assert_eq!(small["size_class"], "normal"); assert_eq!(small["threshold"], 4.5); assert_eq!(small["frames_below"], 13);
    // step 6 samples 0, 6, ..., 66; the cuts at frames 24 and 48 fall between samples 18 / 24 and 42 / 48, so 19..23 and 43..47 are measured too
    assert_eq!(small["frames_sampled"], 12 + 10); assert_eq!(small["cut_frames_added"], 10); assert_eq!(small["cut_times"], json!([1.0, 2.0])); assert!((small["text_px_1080"].as_f64().unwrap() - 21.0).abs() < 1e-9);
    let bold = c(&mut s, "text_contrast_series", json!({"text": "#fff", "text_box": [16, 10, 32, 16], "text_px": 0.7, "bold": true, "step_frames": 6})); assert_eq!(bold["size_class"], "large"); assert_eq!(bold["threshold"], 3.0);
    let ex = c(&mut s, "text_contrast_series", json!({"text": "#fff", "text_box": [16, 10, 32, 16], "text_px": 0.8, "threshold": 7, "step_frames": 6})); assert_eq!(ex["threshold"], 7.0); assert_eq!(ex["threshold_source"], "explicit");
    // a range, a step and no threshold: no threshold keys, no series
    let q = c(&mut s, "text_contrast_series", json!({"text": [1, 1, 1], "text_box": [16, 10, 32, 16], "start": 0.5, "end": 1.5, "step_frames": 4}));
    assert_eq!(q["frames_sampled"], 6 + 3); assert_eq!(q["cut_frames_added"], 3); assert!(q.get("frames_below").is_none() && q.get("series").is_none() && q.get("threshold").is_none() && q.get("suggested_fix").is_none());
    assert_eq!(q["cut_count"], 1); assert_eq!(q["crosses_cut"], true);
    let calm = c(&mut s, "text_contrast_series", json!({"text": "#fff", "text_box": [16, 10, 32, 16], "start": 0.25, "end": 0.9})); assert_eq!(calm["cut_count"], 0); assert_eq!(calm["crosses_cut"], false);
    assert!(close(q["worst_time"].as_f64().unwrap(), 1.0) && (q["worst_ratio"].as_f64().unwrap() - g).abs() < 1e-6, "{q}");
    // a text rect: mean luminance of the text pixels themselves (a rect over the same solid reads 1:1 everywhere)
    let t = c(&mut s, "text_contrast_series", json!({"text": {"rect": [20, 12, 4, 4]}, "background": {"rect": [0, 0, 8, 8]}, "step_frames": 12}));
    assert_eq!(t["bg_region"], "rect");
    assert!(close(t["worst_ratio"].as_f64().unwrap(), 1.0) && close(t["best_ratio"].as_f64().unwrap(), 1.0), "{t}");
}
#[test] fn every_2nd_frame_plus_every_cut_and_the_scrim() {
    let mut s = timeline();
    // step 5 samples 0, 5, ..., 70; the cuts at frames 24 and 48 are not samples: 21..24 and 46..49 are measured too, each cut is
    // stamped at its own frame, and the worst frame is 48 (the first white frame), which step 5 alone would miss (it would say 50)
    let r = c(&mut s, "text_contrast_series", json!({"text": "#ffffff", "text_box": [16, 10, 32, 16], "step_frames": 5, "threshold": 4.5}));
    assert_eq!(r["frames_sampled"], 15 + 8); assert_eq!(r["cut_frames_added"], 8); assert_eq!(r["cut_times"], json!([1.0, 2.0])); assert_eq!(r["crosses_cut"], true);
    assert_eq!(r["worst_frame"], 48); assert!(close(r["worst_time"].as_f64().unwrap(), 2.0) && close(r["worst_ratio"].as_f64().unwrap(), 1.0));
    // every 2nd frame plus every cut: 36 samples, and the one frame between the two samples around each cut (23, 47) is read too
    let r2 = c(&mut s, "text_contrast_series", json!({"text": "#ffffff", "text_box": [16, 10, 32, 16], "step_frames": 2}));
    assert_eq!((r2["frames_sampled"].as_u64(), r2["cut_frames_added"].as_u64()), (Some(38), Some(2))); assert_eq!(r2["cut_times"], json!([1.0, 2.0]));
    // a white scrim cannot help white text: null; no frame under the bar: no suggested_fix
    let w = c(&mut s, "text_contrast_series", json!({"text": "#ffffff", "text_box": [16, 10, 32, 16], "step_frames": 6, "threshold": 4.5, "scrim_color": "#fff"}));
    assert_eq!(w["suggested_fix"], "scrim"); assert!(w["scrim_alpha_min"].is_null(), "{w}");
    let ok = c(&mut s, "text_contrast_series", json!({"text": "#ffffff", "text_box": [16, 10, 32, 16], "end": 0.9, "threshold": 4.5})); assert_eq!(ok["frames_below"], 0); assert!(ok.get("suggested_fix").is_none());
    // dark text over the grey second: the bar needs a lighter background, a black scrim cannot reach it (null), a white one can
    let d = c(&mut s, "text_contrast_series", json!({"text": "#000000", "text_box": [16, 10, 32, 16], "start": 1.0, "end": 2.0, "step_frames": 6, "threshold": 7, "scrim_color": "#ffffff"}));
    let lb = rel_lum([128.0 / 255.0; 3], None); let target = 7.0 * 0.05 - 0.05; assert!(close(d["scrim_alpha_min"].as_f64().unwrap(), (target - lb) / (1.0 - lb)), "{d}");
    let dk = c(&mut s, "text_contrast_series", json!({"text": "#000000", "text_box": [16, 10, 32, 16], "start": 1.0, "end": 2.0, "step_frames": 6, "threshold": 7})); assert!(dk["scrim_alpha_min"].is_null(), "{dk}");
    // bg_region box: the pixels under text_box (512 of them), no ring
    let b = c(&mut s, "text_contrast_series", json!({"text": "#ffffff", "text_box": [16, 10, 32, 16], "bg_region": "box", "step_frames": 12}));
    assert_eq!(b["bg_pixels"], 32 * 16); assert_eq!(b["bg_region"], "box"); assert!(b.get("ring_px").is_none()); assert!(close(b["worst_ratio"].as_f64().unwrap(), 1.0));
}
#[test] fn motion_guard_as_detect_cuts() {
    // 24 one-frame solid clips alternating black / white: every frame pair scores 1. With the guard (default) only the first change
    // stands out from the 5 scores before it; without it every pair is a cut. A lone cut after calm frames always passes (above).
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    for k in 0..24 { let v = if k % 2 == 0 { 0.0 } else { 1.0 }; c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [v, v, v], "start": k as f64 / 24.0, "duration": 1.0 / 24.0})); }
    let g = c(&mut s, "text_contrast_series", json!({"text": "#ff0000", "text_box": [16, 10, 32, 16]}));
    assert_eq!(g["cut_guard"], true); assert_eq!(g["cut_count"], 1, "{g}"); assert_eq!(g["cut_times"], json!([1.0 / 24.0]));
    let n = c(&mut s, "text_contrast_series", json!({"text": "#ff0000", "text_box": [16, 10, 32, 16], "cut_guard": false}));
    assert_eq!(n["cut_count"], 23, "{n}"); assert_eq!(n["cut_guard"], false);
    assert!(dispatch(&mut s, "text_contrast_series", &json!({"text": "#fff", "text_box": [0, 0, 4, 4], "cut_guard": 1})).is_err());
}
#[test] fn captions_by_name_with_the_cuts_they_cross() {
    let mut s = timeline();
    // A on the black second only; B across the black -> grey cut at 1 s; C across the grey -> white cut at 2 s (white over white, 1:1)
    let caps = json!([{"name": "A", "text_box": [16, 10, 32, 16], "start": 0.0, "end": 0.9}, {"name": "B", "text_box": [16, 10, 32, 16], "start": 0.5, "end": 1.5},
        {"name": "C", "text_box": [16, 10, 32, 16], "start": 1.5, "end": 2.5}, {"name": "D", "text_box": [16, 10, 32, 16], "start": 2.0, "end": 3.0, "text": "#000000"}]);
    let r = c(&mut s, "text_contrast_series", json!({"text": "#ffffff", "threshold": 4.5, "step_frames": 2, "captions": caps}));
    assert_eq!(r["caption_count"], 4); assert_eq!(r["worst_caption"], "C"); assert!(close(r["worst_ratio"].as_f64().unwrap(), 1.0) && close(r["worst_time"].as_f64().unwrap(), 2.0)); assert_eq!(r["worst_frame"], 48);
    // by name, with the cut time; D starts on the cut (its first frame), so it does not cross one mid-line
    assert_eq!(r["captions_crossing_cut"], 2); assert_eq!(r["crossing_cut"], json!([{"name": "B", "cut_times": [1.0]}, {"name": "C", "cut_times": [2.0]}]));
    assert_eq!(r["captions_below"], 2); assert_eq!(r["below_names"], json!(["B", "C"])); assert_eq!(r["suggested_fix"], "scrim");
    let rows = r["captions"].as_array().unwrap(); assert_eq!(rows.len(), 4);
    assert!(close(rows[0]["worst_ratio"].as_f64().unwrap(), 21.0) && rows[0]["frames_below"] == 0 && rows[0].get("suggested_fix").is_none());
    // a caption row equals the single call over the same span; D's own text colour wins (black on white, 21:1)
    let b1 = c(&mut s, "text_contrast_series", json!({"text": "#ffffff", "threshold": 4.5, "step_frames": 2, "text_box": [16, 10, 32, 16], "start": 0.5, "end": 1.5}));
    for k in ["worst_ratio", "worst_time", "worst_frame", "frames_sampled", "cut_frames_added", "cut_times", "frames_below", "scrim_alpha_min"] { assert_eq!(rows[1][k], b1[k], "{k}"); }
    assert!(close(rows[3]["worst_ratio"].as_f64().unwrap(), 21.0) && rows[3]["crosses_cut"] == false, "{}", rows[3]);
    assert_eq!(r["frames_sampled"].as_u64().unwrap(), rows.iter().map(|x| x["frames_sampled"].as_u64().unwrap()).sum::<u64>());
    // refused: empty, no name, unknown key, top-level start / text_box / series, a caption error names the caption
    for a in [json!({"text": "#fff", "captions": []}), json!({"text": "#fff", "captions": [{"text_box": [0, 0, 4, 4], "start": 0, "end": 1}]}),
        json!({"text": "#fff", "captions": [{"name": "x", "text_box": [0, 0, 4, 4], "start": 0, "end": 1, "ring_px": 2}]}), json!({"text": "#fff", "start": 0, "captions": [{"name": "x", "text_box": [0, 0, 4, 4], "start": 0, "end": 1}]}),
        json!({"text": "#fff", "text_box": [0, 0, 4, 4], "captions": [{"name": "x", "text_box": [0, 0, 4, 4], "start": 0, "end": 1}]}), json!({"text": "#fff", "series": true, "captions": [{"name": "x", "text_box": [0, 0, 4, 4], "start": 0, "end": 1}]}),
        json!({"text": "#fff", "captions": [{"name": "x", "text_box": [0, 0, 4, 4], "start": 0}]}), json!({"text": "#fff", "captions": {"name": "x"}})] {
        assert!(dispatch(&mut s, "text_contrast_series", &a).is_err(), "accepted {a}");
    }
    let e = dispatch(&mut s, "text_contrast_series", &json!({"text": "#fff", "captions": [{"name": "late one", "text_box": [0, 0, 4, 4], "start": 2.5, "end": 9}]})).unwrap_err(); assert!(e.contains("late one"), "{e}");
}
fn img(w: usize, h: usize, f: impl Fn(usize, usize) -> f64) -> Image {
    let mut im = Image::new(w, h); for y in 0..h { for x in 0..w { let v = f(x, y); for ch in 0..3 { im.rgb[ch][y * w + x] = v; } } } im
}
#[test] fn ring_geometry_and_bg_stat() {
    // 20x20: text box [5,5,10,10] is white (must be left out); outside it the value is x / 19 (a left-to-right ramp)
    let im = img(20, 20, |x, y| if (5..15).contains(&x) && (5..15).contains(&y) { 1.0 } else { x as f64 / 19.0 });
    let bl = background_lums(&im, Some([5, 5, 10, 10]), None, 2);
    assert_eq!(bl.len(), 14 * 14 - 100);
    // ring columns 3..17: x = 3..16 -> the brightest ring column is x = 16, never the white box
    let max = bl.iter().cloned().fold(0. / 0., f64::max); assert!(close(max, rel_lum([16.0 / 19.0; 3], None)), "{max}");
    // clipped at the frame corner: [0,0,4,4] with ring 2 keeps the 6x6 corner minus the box
    assert_eq!(background_lums(&im, Some([0, 0, 4, 4]), None, 2).len(), 36 - 16);
    // explicit background rect: every pixel
    assert_eq!(background_lums(&im, None, Some([0, 0, 3, 2]), 8).len(), 6);
    // ring 0 = bg_region box: the pixels under the box itself (all white here)
    let under = background_lums(&im, Some([5, 5, 10, 10]), None, 0); assert_eq!(under.len(), 100); assert!(under.iter().all(|&l| close(l, 1.0)));
    // bg_stat: white text vs the ramp -> worst is p95 (the brighter side); black text -> worst is p5
    let mut v: Vec<f64> = (0..=100).map(|i| i as f64 / 100.0).collect();
    let (rw, lw, p5, p95, mean) = frame_ratio(1.0, &mut v.clone(), "worst"); assert!(close(p5, 0.05) && close(p95, 0.95) && close(mean, 0.5));
    assert!(close(lw, 0.95) && close(rw, ratio(1.0, 0.95)));
    let (rk, lk, ..) = frame_ratio(0.0, &mut v.clone(), "worst"); assert!(close(lk, 0.05) && close(rk, ratio(0.0, 0.05)));
    for (st, want) in [("mean", 0.5), ("p5", 0.05), ("p95", 0.95)] { let (r, l, ..) = frame_ratio(1.0, &mut v, st); assert!(close(l, want) && close(r, ratio(1.0, want)), "{st}"); }
}
#[test] fn refused_arguments() {
    let mut s = timeline();
    let bad = [json!({}), json!({"text": "#fff"}), json!({"text": "#ffff", "text_box": [0, 0, 4, 4]}), json!({"text": [1, 1], "text_box": [0, 0, 4, 4]}),
        json!({"text": "#fff", "text_box": [0, 0, 4, 4], "background": {"rect": [0, 0, 4, 4]}}), json!({"text": "#fff", "background": {"rect": [0, 0, 4, 4]}, "ring_px": 3}),
        json!({"text": "#fff", "text_box": [0, 0, 0, 4]}), json!({"text": "#fff", "text_box": [60, 0, 8, 4]}), json!({"text": {"rect": [0, 0, 4, 4], "x": 1}, "text_box": [0, 0, 4, 4]}),
        json!({"text": "#fff", "text_box": [0, 0, 4, 4], "ring_px": 0}), json!({"text": "#fff", "text_box": [0, 0, 4, 4], "bg_stat": "max"}), json!({"text": "#fff", "text_box": [0, 0, 4, 4], "threshold": 0.5}),
        json!({"text": "#fff", "text_box": [0, 0, 4, 4], "step_frames": 0}), json!({"text": "#fff", "text_box": [0, 0, 4, 4], "start": 2, "end": 1}), json!({"text": "#fff", "text_box": [0, 0, 4, 4], "end": 9}),
        json!({"text": "#fff", "text_box": [0, 0, 4, 4], "series": 1}), json!({"text": "#fff", "background": {"rect": [0, 0, 4.5, 4]}}), json!({"text": "#fff", "background": [0, 0, 4, 4]}), json!({"text": "#fff", "text_box": [0, 0, 4, 4], "source": "nope"}),
        json!({"text": "#fff", "text_box": [0, 0, 4, 4], "bold": true}), json!({"text": "#fff", "text_box": [0, 0, 4, 4], "text_px": 0}), json!({"text": "#fff", "text_box": [0, 0, 4, 4], "cut_threshold": 0}),
        json!({"text": "#fff", "text_box": [0, 0, 4, 4], "bg_region": "under"}), json!({"text": "#fff", "text_box": [0, 0, 4, 4], "bg_region": "box", "ring_px": 4}),
        json!({"text": "#fff", "background": {"rect": [0, 0, 4, 4]}, "bg_region": "box"}), json!({"text": "#fff", "text_box": [0, 0, 4, 4], "scrim_color": "black"})];
    for a in &bad { assert!(dispatch(&mut s, "text_contrast_series", a).is_err(), "accepted {a}"); }
}
#[test] fn text_min_px_counts_only_frames_with_text() {
    // a dark scrim (0.1) with a 3x4 white glyph block inside the box [5,5,10,10]: 12 pixels read as text for pink #ffbbce text
    let lb = rel_lum([0.1; 3], None);
    let im = img(20, 20, |x, y| if (7..10).contains(&x) && (8..12).contains(&y) { 1.0 } else { 0.1 });
    assert_eq!(glyph_px(&im, &[5, 5, 10, 10], rel_lum([1.0, 187.0 / 255.0, 206.0 / 255.0], None), lb), 12);
    // the empty scrim box before the first word: no text pixels
    assert_eq!(glyph_px(&img(20, 20, |_, _| 0.1), &[5, 5, 10, 10], 1.0, lb), 0);
    // the solid timeline has no text anywhere: every frame is left out and the call says so
    let mut s = timeline();
    let e = format!("{}", dispatch(&mut s, "text_contrast_series", &json!({"text": "#ffffff", "text_box": [16, 10, 32, 16], "text_min_px": 1, "step_frames": 12})).unwrap_err());
    assert!(e.starts_with("no_text"), "{e}");
    // without it the output carries no text_min_px keys (unchanged)
    let r = c(&mut s, "text_contrast_series", json!({"text": "#ffffff", "text_box": [16, 10, 32, 16], "step_frames": 12}));
    assert!(r.get("frames_without_text").is_none() && r.get("text_min_px").is_none(), "{r}");
    for bad in [json!({"text": "#fff", "text_box": [16, 10, 32, 16], "text_min_px": 0}), json!({"text": {"rect": [0, 0, 4, 4]}, "text_box": [16, 10, 32, 16], "text_min_px": 5}),
                json!({"text": "#fff", "background": {"rect": [0, 0, 8, 8]}, "text_min_px": 5})] { assert!(dispatch(&mut s, "text_contrast_series", &bad).is_err(), "{bad}"); }
}
