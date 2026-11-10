//! RV-8 step 7: validate_captions - one fixture per code (pure) plus the tool end to end (clean, defects after edits, read-only, skipped list).
use kimo_core::caption_validate::*;
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::collections::BTreeSet;

fn cue(id: &str, a: f64, b: f64, t: &str) -> CueIn { CueIn { id: id.into(), start: a, end: b, text: t.into(), words: vec![] } }
fn codes(c: &[CueIn]) -> Vec<&'static str> { validate(c, &profile("web").unwrap(), 25.0, None).iter().map(|f| f.code).collect() }
#[test] fn one_fixture_per_code_and_a_clean_one() {
    assert!(codes(&[cue("a", 0.0, 2.0, "Hello there my friend"), cue("b", 2.1, 4.0, "Second cue here now")]).is_empty(), "clean");
    assert_eq!(codes(&[cue("a", 0.0, 2.0, "one two three"), cue("b", 1.5, 4.0, "four five six")]), ["CAPTION_OVERLAP"]);
    assert_eq!(codes(&[cue("a", 0.0, 2.0, "one two three"), cue("b", 2.02, 4.0, "four five six")]), ["CAPTION_GAP_TOO_SMALL"]);
    assert_eq!(codes(&[cue("a", 0.0, 0.5, "one two")]), ["CUE_TOO_SHORT"]);
    assert_eq!(codes(&[cue("a", 0.0, 7.0, "one two three")]), ["CUE_TOO_LONG"]);
    assert_eq!(codes(&[cue("a", 0.0, 1.0, "abcdef ghijkl mnopqr stuvwx yzabcd")]), ["READING_SPEED"]);
    assert!(codes(&[cue("a", 0.0, 6.0, &format!("{} {}", "x".repeat(30), "y".repeat(20)))]).contains(&"LINE_TOO_LONG"));
    assert!(codes(&[cue("a", 0.0, 6.0, "one two\nthree four\nfive six")]).contains(&"TOO_MANY_LINES"));
    assert_eq!(codes(&[cue("a", 0.0, 4.0, "one two three\nfour")]), ["ORPHAN_WORD"]);
    let mut c = cue("a", 0.0, 3.0, "one two three"); c.words = vec![WordIn { id: "w1".into(), start: 0.0, end: 0.5, low_confidence: true }]; assert_eq!(codes(&[c]), ["LOW_CONFIDENCE_WORD"]);
    let mut c = cue("a", 0.0, 3.0, "one two three"); c.words = vec![WordIn { id: "w1".into(), start: 1.0, end: 1.2, low_confidence: false }, WordIn { id: "w2".into(), start: 0.5, end: 0.8, low_confidence: false }]; assert_eq!(codes(&[c]), ["WORD_TIMING_NON_MONOTONIC"]);
    let sc: BTreeSet<String> = ["one", "two", "three"].iter().map(|s| s.to_string()).collect(); let f = validate(&[cue("a", 0.0, 3.0, "one two banana")], &profile("web").unwrap(), 25.0, Some(&sc)); assert_eq!(f[0].code, "SCRIPT_MISMATCH");
    assert!(f.iter().all(|x| !x.message.contains("banana") && !x.fix_hint.contains("banana")), "findings never quote cue text");
    assert_eq!(validate(&[cue("a", 0.0, 3.0, &"w ".repeat(20))], &profile("social_9x16").unwrap(), 25.0, None).iter().filter(|x| x.code == "LINE_TOO_LONG").count(), 1, "profile limits differ");
}
fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
#[test] fn tool_validates_generated_captions_clean_then_flags_edits_read_only() {
    let mut ss = Session::new(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let words = "Cold tyres read lower than warm ones because the air inside contracts when the temperature falls overnight. Check them before you drive. Never trust a gauge you have not tested.";
    let mut tm = 0.0; let mut csv = String::from("word,start,end,confidence\n"); for (i, w) in words.split_whitespace().enumerate() { csv.push_str(&format!("{},{:.3},{:.3},{}\n", w.replace(',', ""), tm, tm + 0.4, if i == 3 { 0.2 } else { 0.95 })); tm += 0.4; }
    let p = std::env::temp_dir().join(format!("kimo_cv_{}.csv", std::process::id())); std::fs::write(&p, csv).unwrap();
    c(&mut ss, "transcript_import", json!({"path": p.to_str().unwrap()}));
    let r = c(&mut ss, "transcript_to_captions", json!({"track_id": t})); let gid = r["caption_id"].as_str().unwrap().to_string(); let clips: Vec<String> = r["clip_ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().into()).collect();
    let before = serde_json::to_string(&ss.project).unwrap(); let v = c(&mut ss, "validate_captions", json!({"group_id": gid}));
    let codes: Vec<&str> = v["groups"][0]["findings"].as_array().unwrap().iter().map(|f| f["code"].as_str().unwrap()).collect(); assert_eq!(codes, ["LOW_CONFIDENCE_WORD"], "{v}");
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "read-only"); assert_eq!(v["skipped"].as_array().unwrap().len(), 3);
    c(&mut ss, "edit_caption", json!({"clip_id": clips[1], "start": 0.5})); let v = c(&mut ss, "validate_captions", json!({"all": true}));
    let codes: Vec<&str> = v["groups"][0]["findings"].as_array().unwrap().iter().map(|f| f["code"].as_str().unwrap()).collect(); assert!(codes.contains(&"CAPTION_OVERLAP"), "{codes:?}"); assert_eq!(v["clean"], false);
    assert!(dispatch(&mut ss, "validate_captions", &json!({})).unwrap_err().contains("group_id")); assert!(dispatch(&mut ss, "validate_captions", &json!({"all": true, "profile": "nope"})).is_err());
    let _ = std::fs::remove_file(p);
}

fn hwm_kb() -> u64 { std::fs::read_to_string("/proc/self/status").ok().and_then(|s| s.lines().find(|l| l.starts_with("VmHWM")).and_then(|l| l.split_whitespace().nth(1)?.parse().ok())).unwrap_or(0) }
#[test] fn timing_numbers_and_large_validation_memory() {
    use std::time::Instant;
    let mut tm = 0.0; let mut csv = String::from("word,start,end,confidence\n"); for i in 0..250 { csv.push_str(&format!("word{},{:.3},{:.3},0.9\n", i % 40, tm, tm + 0.34)); tm += 0.36; } // ~90 s, 250 words
    let p = std::env::temp_dir().join(format!("kimo_cvt_{}.csv", std::process::id())); std::fs::write(&p, csv).unwrap();
    let mut ss = Session::new(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let t0 = Instant::now(); c(&mut ss, "transcript_import", json!({"path": p.to_str().unwrap()})); let imp = t0.elapsed();
    let t0 = Instant::now(); let r = c(&mut ss, "transcript_to_captions", json!({"track_id": t})); let cap = t0.elapsed(); let n = r["cues"].as_u64().unwrap();
    let t0 = Instant::now(); c(&mut ss, "validate_captions", json!({"all": true})); let val = t0.elapsed();
    eprintln!("RV8_TIMING import(250 words, ~90 s)={:?} to_captions({n} cues)={:?} validate_captions({n} cues)={:?}", imp, cap, val);
    assert!(imp.as_millis() < 500 && cap.as_millis() < 500 && val.as_millis() < 500);
    // large file: 50k cues through the pure validator; peak RSS growth must stay small
    let before = hwm_kb(); let cues: Vec<CueIn> = (0..50_000).map(|i| cue(&format!("c{i}"), i as f64 * 3.0, i as f64 * 3.0 + 2.5, "A line of caption text\nwith two lines here")).collect();
    let t0 = Instant::now(); let f = validate(&cues, &profile("web").unwrap(), 25.0, None); let big = t0.elapsed(); let after = hwm_kb();
    eprintln!("RV8_TIMING validate(50000 cues)={:?} findings={} peak_rss_growth_kb={}", big, f.len(), after.saturating_sub(before));
    assert!(f.is_empty()); assert!(after.saturating_sub(before) < 100_000, "validation of 50k cues grew RSS by {} kB", after.saturating_sub(before)); let _ = std::fs::remove_file(p);
}
