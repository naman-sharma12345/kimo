use kimo_core::commands::{dispatch, Session};
use serde_json::json;
#[test]
fn analytic_and_inclusive() {
    let mut s = Session::new();
    s.project.fps = 24.;
    let r = dispatch(
        &mut s,
        "match_events",
        &json!({"a":[3.,1.,2.],"b":[1.,2.+1./24.,3.-2./24.,5.],"tolerance_s":0.1}),
    )
    .unwrap();
    assert_eq!(r["stats"]["matched"], 3);
    assert_eq!(r["unmatched_b"], json!([3]));
    assert!(!r["notes"].as_array().unwrap().is_empty());
    let r = dispatch(
        &mut s,
        "match_events",
        &json!({"a":[0.],"b":[0.125],"tolerance_s":0.125}),
    )
    .unwrap();
    assert_eq!(r["pairs"][0]["within_tol"], true);
}
#[test]
fn sweep_not_global_greedy_ties() {
    let mut s = Session::new();
    for (tol, count) in [(1., 1), (11., 2)] {
        let r = dispatch(
            &mut s,
            "match_events",
            &json!({"a":[0.,10.],"b":[9.,11.],"tolerance_s":tol,"mode":"one_to_one"}),
        )
        .unwrap();
        assert_eq!(r["stats"]["matched"], count);
        if count == 2 {
            assert_eq!(r["pairs"][0]["b_index"], 0);
            assert_eq!(r["pairs"][0]["offset_s"], 9.);
        } else {
            assert_eq!(r["pairs"][0]["a_index"], 1);
            assert_eq!(r["pairs"][0]["b_index"], 0);
        }
    }
}
#[test]
fn property_monotone_empty_fuzz() {
    let mut s = Session::new();
    let before = serde_json::to_string(&s.project).unwrap();
    let r = dispatch(&mut s, "match_events", &json!({"a":[],"b":[]})).unwrap();
    assert_eq!(r["stats"]["within_tol_ratio"], 0.);
    for v in [json!(null), json!("NaN"), json!([]), json!(true)] {
        assert!(dispatch(&mut s, "match_events", &json!({"a":[v],"b":[]})).is_err());
    }
    for n in 0..1000 {
        let a: Vec<_> = (0..20).map(|i| i as f64 * 0.1).collect();
        let b: Vec<_> = (0..20)
            .map(|i| i as f64 * 0.1 + (n % 10) as f64 * 0.002)
            .collect();
        let r = dispatch(
            &mut s,
            "match_events",
            &json!({"a":a,"b":b,"tolerance_s":0.02,"mode":"one_to_one"}),
        )
        .unwrap();
        let p = r["pairs"].as_array().unwrap();
        for w in p.windows(2) {
            assert!(w[0]["a_index"].as_u64() < w[1]["a_index"].as_u64());
            assert!(w[0]["b_index"].as_u64() < w[1]["b_index"].as_u64());
        }
    }
    assert_eq!(before, serde_json::to_string(&s.project).unwrap());
}
#[test]
fn million_sweep_timing() {
    let a: Vec<_> = (0..1000000).map(|i| (i, i as f64 / 24.)).collect();
    let b: Vec<_> = (0..1000000).map(|i| (i, i as f64 / 24. + 0.001)).collect();
    let now = std::time::Instant::now();
    let (p, ua, ub) = kimo_core::ext::match_events::sweep(&a, &b, 0.01);
    eprintln!("million sweep {}ms", now.elapsed().as_millis());
    assert_eq!(p.len(), 1000000);
    assert!(ua.is_empty() && ub.is_empty());
}
#[test]
fn nearest_many_to_one_duplicate_groups_and_ties() {
    let mut s = Session::new();
    let r = dispatch(
        &mut s,
        "match_events",
        &json!({"a":[0.,5.,10.],"b":[0.,0.,10.],"tolerance_s":1}),
    )
    .unwrap();
    assert_eq!(r["pairs"][0]["b_index"], 0);
    assert_eq!(r["pairs"][1]["b_index"], 0);
    assert_eq!(r["pairs"][2]["b_index"], 2);
    assert_eq!(r["pairs"][1]["within_tol"], false);
}
#[test]
fn disjoint_windows_full_dropouts() {
    let mut s = Session::new();
    for a in [
        json!({"a":[0,1],"b":[100,101],"tolerance_s":0.1,"mode":"one_to_one"}),
        json!({"a":[0,1],"b":[100,101],"tolerance_s":0.1,"max_offset_s":0.1}),
    ] {
        let r = dispatch(&mut s, "match_events", &a).unwrap();
        assert_eq!(r["pairs"], json!([]));
        assert_eq!(r["unmatched_a"], json!([0, 1]));
        assert_eq!(r["unmatched_b"], json!([0, 1]));
    }
}
#[test]
fn cut_equidistant_to_two_transients_earliest_wins() {
    let mut s = Session::new();
    let r = dispatch(
        &mut s,
        "match_events",
        &json!({"a":[{"t":2.0,"kind":"cut"}],"b":[{"t":1.875},{"t":2.125}],"tolerance_s":0.125}),
    )
    .unwrap();
    assert_eq!(r["pairs"][0]["b_index"], 0);
    assert_eq!(r["pairs"][0]["offset_s"], -0.125);
}
