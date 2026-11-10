//! RV-8 step 3: transcript_import / transcript_get / transcript_edit_word / transcript_set_take through dispatch.
use kimo_core::commands::{dispatch, Session};
use kimo_core::transcript::CueBinding;
use serde_json::{json, Value};

fn c(ss: &mut Session, n: &str, v: Value) -> Value { dispatch(ss, n, &v).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn e(ss: &mut Session, n: &str, v: Value) -> String { dispatch(ss, n, &v).err().unwrap_or_else(|| panic!("{n} should fail")) }
fn file(name: &str, body: &str) -> String { let p = std::env::temp_dir().join(format!("kimo_tt_{}_{name}", std::process::id())); std::fs::write(&p, body).unwrap(); p.to_str().unwrap().to_string() }
const CSV: &str = "word,start,end,confidence\nHello,0.0,0.4,0.99\nbrave,0.5,0.9,0.31\nbrave,1.0,1.4,0.9\nworld,3.0,3.5,0.8\n";
fn pj(ss: &Session) -> String { serde_json::to_string(&ss.project).unwrap() }

#[test] fn import_flags_low_confidence_repeats_and_gaps_and_get_is_untrusted() {
    let mut ss = Session::new(); let r = c(&mut ss, "transcript_import", json!({"path": file("a.csv", CSV), "take": {"asset_id": "asset1"}, "language": "en"}));
    assert_eq!((r["words_added"].as_u64(), r["low_confidence"].as_u64(), r["transcript_id"].as_str()), (Some(4), Some(1), Some("tr1")), "{r}");
    let g = c(&mut ss, "transcript_get", json!({"include_words": true})); assert_eq!(g["untrusted"], true); assert!(g["untrusted_fields"].as_array().unwrap().len() >= 3);
    let w = g["words"].as_array().unwrap(); assert_eq!(w[1]["flags"], json!(["low_confidence"]), "listed, never hidden"); assert_eq!(w[2]["flags"], json!(["repeated"])); assert_eq!(w[3]["flags"], json!(["gap"]));
    assert_eq!(g["takes"][0]["source"]["asset_id"], "asset1"); assert_eq!(c(&mut ss, "transcript_get", json!({"range": [0.0, 0.45]}))["words_in_range"], 1);
    let r2 = c(&mut ss, "transcript_import", json!({"path": file("a.csv", CSV), "confidence_threshold": 0.95})); assert_eq!(r2["low_confidence"], 3, "threshold parameter");
}
#[test] fn import_errors_are_structured_and_leave_the_project_unchanged() {
    let mut ss = Session::new(); let before = pj(&ss);
    let m = e(&mut ss, "transcript_import", json!({"path": file("b.csv", "word,start,end\na,2,3\nb,1,1.5\n")})); assert!(m.contains("non_monotonic") && m.contains("line 3"), "{m}");
    assert!(e(&mut ss, "transcript_import", json!({"path": "/nonexistent/x.srt"})).contains("cannot read")); assert_eq!(pj(&ss), before);
    assert!(e(&mut ss, "transcript_import", json!({"path": file("c.csv", CSV), "merge": true})).starts_with("no such transcript"));
}
#[test] fn merge_adds_a_take_and_script_order_wins_over_import_order() {
    let mut ss = Session::new(); c(&mut ss, "transcript_import", json!({"path": file("d1.csv", "word,start,end\nsecond,0,1\n"), "take": {"order": 2}}));
    let r = c(&mut ss, "transcript_import", json!({"path": file("d2.csv", "word,start,end\nfirst,0,1\n"), "merge": true, "take": {"order": 1}})); assert_eq!(r["take_id"], "take2");
    let g = c(&mut ss, "transcript_get", json!({"include_words": true})); let w: Vec<&str> = g["words"].as_array().unwrap().iter().map(|x| x["display"].as_str().unwrap()).collect(); assert_eq!(w, ["first", "second"]);
    assert_eq!(g["words"][1]["start"], 1.0, "take1 (order 2) is placed after take2 (order 1, 1 s long)");
}
#[test] fn edit_word_keeps_timing_and_recognized_updates_bound_cue_and_undoes() {
    let mut ss = Session::new(); c(&mut ss, "transcript_import", json!({"path": file("e.csv", CSV)}));
    let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let clip = c(&mut ss, "add_text", json!({"track_id": t, "text": "Hello brave", "start": 0.0, "duration": 1.0}))["clip_id"].as_str().unwrap().to_string();
    ss.project.editorial.cue_words.insert(clip.clone(), CueBinding { word_ids: vec!["w1".into(), "w2".into()], breaks_after: vec![] });
    let before = pj(&ss); let r = c(&mut ss, "transcript_edit_word", json!({"word_id": "w2", "display": "Brave!"})); assert_eq!(r["cues_updated"], json!([clip.clone()]));
    let text = |ss: &Session| match &ss.project.find_clip(&clip).unwrap().source { kimo_core::model::Source::Text { text, .. } => text.clone(), _ => panic!() };
    assert_eq!(text(&ss), "Hello Brave!"); let g = c(&mut ss, "transcript_get", json!({"include_words": true})); let w = &g["words"][1];
    assert_eq!((w["recognized"].as_str(), w["display"].as_str(), w["corrected"].as_bool(), w["start"].as_f64()), (Some("brave"), Some("Brave!"), Some(true), Some(0.5))); assert_eq!(w["flags"], json!(["low_confidence", "manual"]));
    assert_eq!(ss.project.find_clip(&clip).unwrap().duration, 1.0, "cue timing unchanged");
    c(&mut ss, "undo", json!({})); assert_eq!(pj(&ss), before, "undo restores the project byte-for-byte"); assert_eq!(text(&ss), "Hello brave");
    c(&mut ss, "transcript_edit_word", json!({"word_id": "w2", "display": "Brave!"})); c(&mut ss, "transcript_edit_word", json!({"word_id": "w2", "display": "brave"}));
    let g = c(&mut ss, "transcript_get", json!({"include_words": true})); assert_eq!((g["words"][1]["corrected"].as_bool(), g["words"][1]["flags"].clone()), (Some(false), json!(["low_confidence"])), "reverting clears corrected and manual");
}
#[test] fn edit_word_range_and_errors() {
    let mut ss = Session::new(); c(&mut ss, "transcript_import", json!({"path": file("f.csv", CSV)}));
    c(&mut ss, "transcript_edit_word", json!({"from_index": 0, "to_index": 1, "display": "Hi there"})); let g = c(&mut ss, "transcript_get", json!({"include_words": true})); assert_eq!((g["words"][0]["display"].as_str(), g["words"][1]["display"].as_str()), (Some("Hi"), Some("there")));
    assert!(e(&mut ss, "transcript_edit_word", json!({"from_index": 0, "to_index": 2, "display": "only two"})).contains("exactly 3 words")); assert!(e(&mut ss, "transcript_edit_word", json!({"from_index": 0, "to_index": 9, "display": "x"})).contains("outside"));
    assert!(e(&mut ss, "transcript_edit_word", json!({"word_id": "nope", "display": "x"})).starts_with("no such word")); assert!(e(&mut ss, "transcript_edit_word", json!({"word_id": "w1", "display": " \u{200B} "})).contains("must not be empty"));
    assert!(e(&mut ss, "transcript_edit_word", json!({"display": "x"})).contains("word_id"));
}
#[test] fn set_take_recomputes_times_and_reports_affected_cues_atomically() {
    let mut ss = Session::new(); c(&mut ss, "transcript_import", json!({"path": file("g1.csv", "word,start,end\none,0,1\ntwo,1,2\n")})); c(&mut ss, "transcript_import", json!({"path": file("g2.csv", "word,start,end\nthree,0,1\n"), "merge": true}));
    ss.project.editorial.cue_words.insert("clipX".into(), CueBinding { word_ids: vec!["w3".into()], breaks_after: vec![] });
    let w3 = |ss: &mut Session| c(ss, "transcript_get", json!({"include_words": true}))["words"].as_array().unwrap().iter().find(|w| w["id"] == "w3").unwrap()["start"].as_f64().unwrap(); assert_eq!(w3(&mut ss), 2.0);
    let r = c(&mut ss, "transcript_set_take", json!({"take_id": "take1", "speed": 2.0})); assert_eq!(w3(&mut ss), 1.0, "take1 is now 1 s long"); assert_eq!(r["affected_cues"], json!(["clipX"]));
    c(&mut ss, "transcript_set_take", json!({"take_id": "take1", "order": 5})); assert_eq!(w3(&mut ss), 0.0, "take2 now first (take1 moved after it)");
    c(&mut ss, "transcript_set_take", json!({"take_id": "take2", "status": "rejected"})); let g = c(&mut ss, "transcript_get", json!({"include_words": true})); assert_eq!(g["words_in_range"], 2, "rejected take is off the script");
    let before = pj(&ss); assert!(e(&mut ss, "transcript_set_take", json!({"take_id": "take1", "speed": 0.0})).contains("speed")); assert!(e(&mut ss, "transcript_set_take", json!({"take_id": "zz"})).starts_with("no such take")); assert!(e(&mut ss, "transcript_set_take", json!({"take_id": "take1", "status": "x"})).contains("unknown status"));
    assert_eq!(pj(&ss), before, "failed calls change nothing");
}
#[test] fn coverage_tool_names_the_missing_passage_and_is_read_only() {
    let mut ss = Session::new(); c(&mut ss, "transcript_import", json!({"path": file("cov.csv", "word,start,end\nthe,0,0.2\nquick,0.3,0.5\nfox,0.6,0.8\njumps,0.9,1.1\n")}));
    let before = pj(&ss);
    let r = c(&mut ss, "transcript_coverage", json!({"script_text": "The quick brown fox jumps over the lazy dog"}));
    assert_eq!(r["untrusted"], true); assert_eq!(r["covered_tokens"], 4, "{r}"); assert_eq!(r["script_tokens"], 9);
    assert_eq!(r["missing_passages"][0]["script_range"], json!([2, 2])); assert_eq!(r["missing_passages"][0]["text_untrusted"], "brown");
    assert!(r["missing_passages"].as_array().unwrap().iter().any(|m| m["text_untrusted"] == "over the lazy dog"), "{r}");
    assert_eq!(pj(&ss), before, "read-only");
    assert!(e(&mut ss, "transcript_coverage", json!({})).contains("script_text"));
}
#[test] fn coverage_timeline_reports_alignment_silences_speed_and_zero_drift() {
    let mut ss = Session::new(); c(&mut ss, "transcript_import", json!({"path": file("tl.csv", "word,start,end\na,0,1\nb,2,3\n"), "take": {"timeline_start": 10.0}}));
    let r = c(&mut ss, "transcript_coverage", json!({"script_text": "a b"})); let t = &r["timeline"];
    assert_eq!(t["takes"][0]["timeline_start"], 10.0); assert_eq!(t["takes"][0]["aligned_pct"], 66.7, "{t}"); assert_eq!(t["takes"][0]["silence_intervals"], json!([[11.0, 12.0]])); assert_eq!(t["max_drift_ms"], 0.0);
    c(&mut ss, "transcript_set_take", json!({"take_id": "take1", "speed": 2.0}));
    let t = c(&mut ss, "transcript_coverage", json!({"script_text": "a b"}))["timeline"].clone(); assert_eq!(t["takes"][0]["timeline_end"], 11.5, "{t}"); assert_eq!(t["takes"][0]["silence_intervals"], json!([[10.5, 11.0]])); assert_eq!(t["max_drift_ms"], 0.0, "speed change must not desync");
    // second take overlapping the first, and a negative offset cannot be set
    c(&mut ss, "transcript_import", json!({"path": file("tl2.csv", "word,start,end\nc,0,1\n"), "merge": true, "take": {"timeline_start": 11.0, "order": 5}}));
    let t = c(&mut ss, "transcript_coverage", json!({"script_text": "a b c"}))["timeline"].clone(); assert_eq!(t["issues"][0]["code"], "TAKE_OVERLAP", "{t}");
    assert!(e(&mut ss, "transcript_set_take", json!({"take_id": "take2", "timeline_start": -1.0})).contains(">= 0"));
}
const SPEECH: &str = "So the first thing you want to check is the tyre pressure; because it changes everything. Cold tyres read lower than warm ones! Never trust a gauge you have not tested. Simple.";
fn speech_csv() -> String { let mut t = 0.0; let mut s = String::from("word,start,end\n"); for w in SPEECH.split_whitespace() { let d = 0.12 + 0.045 * w.trim_matches(|c: char| !c.is_alphanumeric()).len() as f64; s.push_str(&format!("{},{:.3},{:.3}\n", w, t, t + d)); t += d + if w.ends_with('.') || w.ends_with('!') { 0.45 } else { 0.0 }; } s }
#[test] fn to_captions_builds_bound_cues_with_exact_times_edits_follow_and_replace_works() {
    let mut ss = Session::new(); let tk = c(&mut ss, "add_track", json!({"kind": "text"}));
    let track = tk["track_id"].as_str().or(tk["id"].as_str()).unwrap().to_string();
    c(&mut ss, "transcript_import", json!({"path": file("sp.csv", &speech_csv())}));
    let dir = std::env::temp_dir(); let (srt, vtt) = (dir.join(format!("kimo_sp_{}.srt", std::process::id())), dir.join(format!("kimo_sp_{}.vtt", std::process::id())));
    let r = c(&mut ss, "transcript_to_captions", json!({"track_id": track, "srt_path": srt.to_str().unwrap(), "vtt_path": vtt.to_str().unwrap()}));
    assert_eq!(r["overlaps"], 0, "{r}"); assert!(r["max_cps"].as_f64().unwrap() <= 17.0 && r["max_lines"].as_u64().unwrap() <= 2 && r["warnings"].as_array().unwrap().is_empty(), "{r}");
    let gid = r["caption_id"].as_str().unwrap().to_string(); let clips: Vec<String> = r["clip_ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().into()).collect();
    assert_eq!(ss.project.editorial.captions[&gid].transcript_id.as_deref(), Some("tr1")); assert_eq!(ss.project.editorial.cue_words.len(), clips.len());
    let tr = ss.project.editorial.transcripts["tr1"].clone(); let b0 = ss.project.editorial.cue_words[&clips[0]].clone(); let w0 = tr.words.iter().find(|w| w.id == b0.word_ids[0]).unwrap().clone();
    assert_eq!(ss.project.find_clip_mut(&clips[0]).unwrap().start, w0.start, "cue starts exactly at its first word: no ms rounding drift");
    let s = std::fs::read_to_string(&srt).unwrap(); assert_eq!(s.matches(" --> ").count(), clips.len()); assert!(std::fs::read_to_string(&vtt).unwrap().starts_with("WEBVTT"));
    // exported SRT round-trips through the existing importer: same cue count, text and times within 1 ms
    let orig: Vec<(f64, f64, String)> = clips.iter().map(|id| { let cl = ss.project.find_clip_mut(id).unwrap(); (cl.start, cl.duration, match &cl.source { kimo_core::model::Source::Text { text, .. } => text.clone(), _ => panic!() }) }).collect();
    let t2 = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let ri = c(&mut ss, "import_srt", json!({"track_id": t2, "path": srt.to_str().unwrap()}));
    let back: Vec<String> = ri["clip_ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().into()).collect(); assert_eq!(back.len(), orig.len());
    for (id, o) in back.iter().zip(&orig) { let cl = ss.project.find_clip_mut(id).unwrap(); let txt = match &cl.source { kimo_core::model::Source::Text { text, .. } => text.clone(), _ => panic!() }; assert_eq!(txt, o.2); assert!((cl.start - o.0).abs() <= 0.001 && (cl.duration - o.1).abs() <= 0.001, "{} vs {:?}", cl.start, o); }
    c(&mut ss, "transcript_edit_word", json!({"word_id": b0.word_ids[0].clone(), "display": "Well"})); let t0 = match &ss.project.find_clip_mut(&clips[0]).unwrap().source { kimo_core::model::Source::Text { text, .. } => text.clone(), _ => panic!() }; assert!(t0.starts_with("Well"), "{t0}");
    let r2 = c(&mut ss, "transcript_to_captions", json!({"track_id": track, "replace_group": gid, "max_chars_per_line": 20})); assert!(!ss.project.editorial.captions.contains_key(&gid)); assert!(ss.project.find_clip_mut(&clips[0]).is_none(), "old cue clips removed"); assert!(r2["cues"].as_u64().unwrap() > clips.len() as u64);
    assert!(e(&mut ss, "transcript_to_captions", json!({"track_id": track, "replace_group": "nope"})).contains("no such caption group")); assert!(e(&mut ss, "transcript_to_captions", json!({"track_id": "nope"})).contains("track"));
    let again = e(&mut ss, "transcript_to_captions", json!({"track_id": track, "srt_path": srt.to_str().unwrap()})); assert!(again.starts_with("dest_exists"), "{again}");
    let _ = std::fs::remove_file(srt); let _ = std::fs::remove_file(vtt);
}
#[test] fn edit_preview_is_read_only_and_reports_cuts_and_cues() {
    let mut ss = Session::new(); let t = c(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut ss, "transcript_import", json!({"path": file("ep.csv", &speech_csv())})); c(&mut ss, "transcript_to_captions", json!({"track_id": t}));
    let before = pj(&ss); let r = c(&mut ss, "transcript_edit_preview", json!({"edits": [{"op": "delete", "from_index": 2, "to_index": 4}, {"op": "delete", "from_index": 5, "to_index": 6}, {"op": "replace", "from_index": 0}]}));
    assert_eq!(pj(&ss), before, "project hash unchanged"); assert_eq!(r["read_only"], true); assert_eq!(r["audio_cut_ranges"].as_array().unwrap().len(), 1, "adjacent deletes merge: {r}"); assert!(r["removed_s"].as_f64().unwrap() > 0.5);
    assert!(!r["affected_cues"].as_array().unwrap().is_empty()); assert_eq!(r["markers"][0]["label"], "cut");
    assert!(e(&mut ss, "transcript_edit_preview", json!({"edits": [{"op": "delete", "from_index": 9999}]})).contains("outside")); assert!(e(&mut ss, "transcript_edit_preview", json!({"edits": [{"op": "wipe", "from_index": 1}]})).contains("op"));
}
