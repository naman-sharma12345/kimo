//! P1 failing-first rows: diff_frames `regions` (8-connected changed-pixel components).
//! RED until P1 lands: the `regions` key does not exist yet.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

// x/y props are offsets from canvas center (lint bounds() convention); bbox = [128+x-10, 128+y-5, 20, 10] on 256.
fn session_with_square(x: f64, y: f64) -> (Session, String) {
    let mut ss = Session::new();
    let c = |ss: &mut Session, n: &str, a: Value| dispatch(ss, n, &a).unwrap();
    c(&mut ss, "project_set", json!({"width": 256, "height": 256, "fps": 24}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0, 0, 0], "start": 0, "duration": 1}));
    let cl = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 20, "height": 10, "fill": [1, 1, 1, 1], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
    c(&mut ss, "set_property", json!({"clip_id": cl, "property": "x", "value": x}));
    c(&mut ss, "set_property", json!({"clip_id": cl, "property": "y", "value": y}));
    (ss, cl)
}
fn diff(ss: &mut Session, extra: Value) -> Value {
    let mut a = json!({"a": {"time": 0.5, "source": "snapshot:checkpoint:a"}, "b": {"time": 0.5, "source": "composite"}, "scale": 1});
    a.as_object_mut().unwrap().extend(extra.as_object().unwrap().clone());
    dispatch(ss, "diff_frames", &a).unwrap()
}
fn moved() -> (Session, String) {
    let (mut ss, cl) = session_with_square(-78., -78.);
    dispatch(&mut ss, "checkpoint", &json!({"name": "a"})).unwrap();
    dispatch(&mut ss, "set_property", &json!({"clip_id": cl, "property": "x", "value": 22.})).unwrap();
    dispatch(&mut ss, "set_property", &json!({"clip_id": cl, "property": "y", "value": -8.})).unwrap();
    (ss, cl)
}

#[test]
fn moved_square_two_regions_merge_px0_one_region_merge_px64() {
    let (mut ss, _cl) = moved();
    let r0 = diff(&mut ss, json!({"merge_px": 0}));
    let regions = r0["regions"].as_array().expect("regions key (P1) missing");
    assert_eq!(regions.len(), 2, "merge_px 0 must give old+new boxes: {regions:?}");
    let mut boxes: Vec<[u64; 4]> = regions.iter().map(|r| {
        let b = &r["bbox"]; [b[0].as_u64().unwrap(), b[1].as_u64().unwrap(), b[2].as_u64().unwrap(), b[3].as_u64().unwrap()]
    }).collect();
    boxes.sort();
    assert_eq!(boxes, vec![[40, 45, 20, 10], [140, 115, 20, 10]], "source-pixel bboxes of the 20x10 square (center-anchored)");
    let r64 = diff(&mut ss, json!({"merge_px": 64}));
    assert_eq!(r64["regions"].as_array().unwrap().len(), 1, "merge_px 64 merges to one region");
    assert_eq!(r64["truncated"], false);
}

#[test]
fn identical_frames_empty_regions() {
    let (mut ss, _cl) = session_with_square(-78., -78.);
    dispatch(&mut ss, "checkpoint", &json!({"name": "a"})).unwrap();
    let r = diff(&mut ss, json!({}));
    assert_eq!(r["regions"].as_array().expect("regions key (P1) missing").len(), 0);
    assert_eq!(r["summary"].as_str().unwrap().contains("no changed regions"), true);
}

#[test]
fn layer_hits_name_the_moved_clip_and_threads_byte_equal() {
    let (mut ss, cl) = moved();
    let before = serde_json::to_vec(&ss.project).unwrap();
    let mut outs = vec![];
    for n in [1, 4] {
        kimo_core::par::set_threads(n);
        outs.push(diff(&mut ss, json!({"merge_px": 0})));
    }
    kimo_core::par::set_threads(0);
    assert_eq!(serde_json::to_vec(&ss.project).unwrap(), before);
    assert_eq!(serde_json::to_vec(&outs[0]).unwrap(), serde_json::to_vec(&outs[1]).unwrap());
    let hits = outs[0]["regions"][0]["layer_hits"].as_array().expect("layer_hits (P1) missing");
    assert!(hits.iter().any(|h| h["layer"]["id"] == cl), "layer_hits must name the moved clip {cl}: {hits:?}");
    assert!(hits[0]["overlap_ratio"].as_f64().unwrap() > 0.0);
}

#[test]
fn forty_dots_truncates_at_32() {
    let mut ss = Session::new();
    let c = |ss: &mut Session, n: &str, a: Value| dispatch(ss, n, &a).unwrap();
    c(&mut ss, "project_set", json!({"width": 256, "height": 256, "fps": 24}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "checkpoint", &json!({"name": "a"}));
    for i in 0..40 {
        let cl = c(&mut ss, "add_shape", json!({"track_id": t, "shape": "rect", "width": 2, "height": 2, "fill": [1, 1, 1, 1], "start": 0, "duration": 1}))["clip_id"].as_str().unwrap().to_string();
        c(&mut ss, "set_property", json!({"clip_id": cl, "property": "x", "value": (i % 8) as f64 * 30. - 118.}));
        c(&mut ss, "set_property", json!({"clip_id": cl, "property": "y", "value": (i / 8) as f64 * 45. - 118.}));
    }
    let r = diff(&mut ss, json!({"merge_px": 0}));
    let regions = r["regions"].as_array().expect("regions key (P1) missing");
    assert_eq!(regions.len(), 32, "cap is 32 largest first");
    assert_eq!(r["truncated"], true);
}

#[test]
fn expect_bbox_unexpected_change_finding() {
    let (mut ss, _cl) = moved();
    let r = diff(&mut ss, json!({"merge_px": 0, "expect_bbox": [140, 115, 20, 10]}));
    let findings = r["findings"].as_array().expect("findings (P1) missing");
    assert!(findings.iter().any(|f| f["kind"] == "unexpected_change"), "change outside expect_bbox must be flagged: {findings:?}");
}

#[test]
fn benchmark_256_prints_median_twenty_runs_no_gate() {
    let (mut ss, _cl) = moved();
    let _warm = diff(&mut ss, json!({"merge_px": 0}));
    let mut ms = vec![];
    for _ in 0..20 {
        let now = std::time::Instant::now();
        let r = diff(&mut ss, json!({"merge_px": 0}));
        ms.push(now.elapsed().as_secs_f64() * 1000.);
        assert_eq!(r["regions"].as_array().expect("regions (P1) missing").len(), 2);
    }
    ms.sort_by(f64::total_cmp);
    eprintln!("P1 256x256 whole-tool median_20_ms={:.6} (benchmark only, no pass/fail threshold)", (ms[9]+ms[10])/2.);
}
