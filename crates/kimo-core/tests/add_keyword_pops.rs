//! add_keyword_pops (new, lane KF15): one big emphasised word or short phrase that pops onto the
//! screen as it's spoken, holds, then clears -- the #1 on-screen graphic among top creators (research
//! lane KR2). All tests below must fail before the tool exists (dispatch errors "unknown tool
//! 'add_keyword_pops'").
use kimo_core::commands::{dispatch, Session};
use kimo_core::render;
use serde_json::{json, Value};

fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn err(ss: &mut Session, name: &str, a: Value) -> String { dispatch(ss, name, &a).err().unwrap_or_else(|| panic!("{name} should fail but succeeded")) }
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }
fn file(name: &str, body: &str) -> String { let p = std::env::temp_dir().join(format!("kimo_akp_{}_{name}", std::process::id())); std::fs::write(&p, body).unwrap(); p.to_str().unwrap().to_string() }
fn setup() -> Session { let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 720, "height": 1280, "fps": 30})); ss }

/// Any pixel in the frame brighter than near-black -- the same "non-background" threshold convention
/// text_bounds.rs's own `bbox` helper uses, on an otherwise-empty (black) canvas.
fn bright_any(ss: &Session, t: f64) -> bool {
    let fr = render::render_frame(&ss.project, t, &render::Cache::new()).unwrap();
    let (w, h) = (fr.w, fr.h);
    for y in 0..h { for x in 0..w { let i = (y * w + x) * 4; if fr.d[i] + fr.d[i + 1] + fr.d[i + 2] > 0.3 { return true; } } }
    false
}

#[test] fn explicit_mode_places_pops_at_the_right_times() {
    let mut ss = setup();
    let r = call(&mut ss, "add_keyword_pops", json!({"words": [{"text": "HUGE", "at_s": 1.0, "duration_s": 1.2}, {"text": "WOW", "at_s": 5.0, "duration_s": 1.0}]}));
    let pops = r["pops"].as_array().unwrap().clone();
    assert_eq!(pops.len(), 2, "{r}");
    assert_eq!(pops[0]["at_s"].as_f64().unwrap(), 1.0);
    assert_eq!(pops[0]["duration_s"].as_f64().unwrap(), 1.2);
    assert_eq!(pops[1]["at_s"].as_f64().unwrap(), 5.0);
    let track_id = r["track_id"].as_str().unwrap().to_string();
    assert!(ss.project.tracks.iter().any(|t| t.id == track_id), "pops should land on the returned track");
    assert!(!pops[0]["clip_ids"].as_array().unwrap().is_empty());
}

#[test] fn render_shows_text_present_after_pop_and_absent_before() {
    let mut ss = setup();
    call(&mut ss, "add_keyword_pops", json!({"words": [{"text": "HUGE", "at_s": 2.0, "duration_s": 1.5}]}));
    assert!(!bright_any(&ss, 0.5), "text must not be visible before at_s");
    assert!(!bright_any(&ss, 1.9), "text must not be visible just before at_s");
    assert!(bright_any(&ss, 2.5), "text should be visible at at_s+0.5");
}

#[test] fn pop_animation_is_smaller_at_the_start_and_full_size_after_0_3s() {
    let mut ss = setup();
    let r = call(&mut ss, "add_keyword_pops", json!({"words": [{"text": "BIG", "at_s": 0.0, "duration_s": 1.5}], "style": "bold_caps", "animate_in": "pop"}));
    let id = r["pops"][0]["clip_ids"][0].as_str().unwrap().to_string();
    let clip = ss.project.find_clip(&id).unwrap();
    let scale = clip.props.get("scale").unwrap();
    let s0 = scale.at(0.0);
    let s3 = scale.at(0.3);
    assert!(s0 < 0.3, "scale should be small at clip-local t=0, got {s0}");
    assert!((s3 - 1.0).abs() < 0.05, "scale should be ~1.0 at clip-local t=0.3, got {s3}");
}

#[test] fn boxed_style_also_pops_in_with_the_same_0_3s_overshoot() {
    let mut ss = setup();
    let r = call(&mut ss, "add_keyword_pops", json!({"words": [{"text": "WOW", "at_s": 0.0, "duration_s": 1.5}], "style": "boxed"}));
    let ids = r["pops"][0]["clip_ids"].as_array().unwrap().clone();
    assert_eq!(ids.len(), 2, "boxed style should place a box clip and a text clip");
    for idv in &ids {
        let id = idv.as_str().unwrap();
        let clip = ss.project.find_clip(id).unwrap();
        let scale = clip.props.get("scale").unwrap();
        assert!(scale.at(0.0) < 0.3, "{id}: scale should be small at t=0");
        assert!((scale.at(0.3) - 1.0).abs() < 0.05, "{id}: scale should be ~1 at t=0.3");
    }
}

#[test] fn auto_mode_respects_max_per_minute() {
    let mut ss = setup();
    let words = ["amazing", "wonderful", "fantastic", "incredible", "beautiful", "spectacular", "gorgeous", "marvelous"];
    let mut csv = String::from("word,start,end\n");
    let mut t = 0.0;
    for w in words { csv.push_str(&format!("{w},{t:.3},{:.3}\n", t + 0.3)); t += 2.0; } // 8 qualifying words, 2s apart, all inside minute 0
    call(&mut ss, "transcript_import", json!({"path": file("auto_cap.csv", &csv)}));
    let r = call(&mut ss, "add_keyword_pops", json!({"auto": true, "max_per_minute": 3.0, "min_gap_s": 0.5}));
    let pops = r["pops"].as_array().unwrap();
    assert_eq!(pops.len(), 3, "max_per_minute:3 should cap one busy minute at 3 pops, got {}: {r}", pops.len());
}

#[test] fn auto_mode_respects_min_gap_s() {
    let mut ss = setup();
    // two qualifying words 0.5s apart (too close) then one far away
    let csv = "word,start,end\namazing,0.0,0.3\nwonderful,0.5,0.8\nfantastic,10.0,10.3\n";
    call(&mut ss, "transcript_import", json!({"path": file("auto_gap.csv", csv)}));
    let r = call(&mut ss, "add_keyword_pops", json!({"auto": true, "max_per_minute": 10.0, "min_gap_s": 3.0}));
    let pops = r["pops"].as_array().unwrap();
    let times: Vec<f64> = pops.iter().map(|p| p["at_s"].as_f64().unwrap()).collect();
    for w in times.windows(2) { assert!(w[1] - w[0] >= 3.0 - 1e-6, "min_gap_s:3 violated by {times:?}"); }
    assert!(times.len() <= 2, "the two 0.5s-apart words must not both survive a 3s min_gap_s: {times:?}");
}

#[test] fn auto_with_no_timings_gives_a_clear_error() {
    let mut ss = setup();
    let m = err(&mut ss, "add_keyword_pops", json!({"auto": true}));
    assert!(m.contains("transcribe") || m.contains("transcript_import"), "error must name the tool to run first, got: {m}");
    assert!(!m.is_empty());
}

#[test] fn dry_run_changes_nothing() {
    let mut ss = setup();
    let before = pj(&ss);
    let r = call(&mut ss, "add_keyword_pops", json!({"words": [{"text": "HUGE", "at_s": 1.0}], "dry_run": true}));
    assert_eq!(r["dry_run"], true);
    assert_eq!(r["pops"][0]["text"], "HUGE");
    assert_eq!(pj(&ss), before, "dry_run must not change the project");
}

#[test] fn undo_restores_the_timeline() {
    let mut ss = setup();
    let before = pj(&ss);
    call(&mut ss, "add_keyword_pops", json!({"words": [{"text": "HUGE", "at_s": 1.0}, {"text": "WOW", "at_s": 3.0}]}));
    assert_ne!(pj(&ss), before, "the call should have changed the project");
    call(&mut ss, "undo", json!({}));
    assert_eq!(pj(&ss), before, "undo must restore the timeline exactly");
}

#[test] fn bad_args_error() {
    let mut ss = setup();
    let m = err(&mut ss, "add_keyword_pops", json!({}));
    assert!(m.contains("words") && m.contains("auto"), "{m}");
    assert!(err(&mut ss, "add_keyword_pops", json!({"words": []})).contains("words"));
    assert!(err(&mut ss, "add_keyword_pops", json!({"words": [{"text": "HI", "at_s": 1.0}], "auto": true})).contains("not both"));
    assert!(err(&mut ss, "add_keyword_pops", json!({"words": [{"text": "HI", "at_s": -1.0}]})).contains("at_s"));
    assert!(err(&mut ss, "add_keyword_pops", json!({"words": [{"at_s": 1.0}]})).contains("text"));
    assert!(err(&mut ss, "add_keyword_pops", json!({"auto": true, "max_per_minute": 0})).contains("max_per_minute"));
    assert!(err(&mut ss, "add_keyword_pops", json!({"words": [{"text": "HI", "at_s": 1.0}], "style": "bogus"})).contains("style"));
    assert!(err(&mut ss, "add_keyword_pops", json!({"words": [{"text": "HI", "at_s": 1.0}], "position": "bogus"})).contains("position"));
    assert!(err(&mut ss, "add_keyword_pops", json!({"words": [{"text": "HI", "at_s": 1.0}], "track_id": "no_such_track"})).contains("track"));
}
