//! trim_filler_words: ripple-delete flagged word spans from one media clip's own footage, reusing `lift_range`.
//! Checked first: transcript_edit_apply is a different, heavier whole-project/take-bound mechanism and does not
//! take a bare clip_id + time spans -- this is a new, narrow tool, not a duplicate.
use kimo_core::{
    commands::{dispatch, Session},
    model::{Asset, Source},
    transcript::{Take, TakeSource, TakeStatus, Transcript, Word},
};
use serde_json::{json, Value};

fn call(ss: &mut Session, tool: &str, args: Value) -> Value {
    dispatch(ss, tool, &args).unwrap_or_else(|e| panic!("{tool}: {e}"))
}
fn call_err(ss: &mut Session, tool: &str, args: Value) -> String {
    dispatch(ss, tool, &args).expect_err(&format!("{tool}: expected an error"))
}
fn clips(ss: &Session) -> Vec<Value> {
    let p = serde_json::to_value(&ss.project).unwrap();
    p["tracks"].as_array().unwrap().iter().flat_map(|t| t["clips"].as_array().unwrap().iter().cloned()).collect()
}

/// A track with one media clip ("c1") 6s long, plus a second clip ("c2") right after it, so ripple is observable.
/// clip_id is reused as the asset id's prefix purely for readability.
fn fixture(duration: f64) -> (Session, String, String, String) {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let track = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let c1 = call(&mut ss, "add_clip", json!({"track_id": track, "source_type": "solid", "color": "#ff0000", "start": 0, "duration": duration}))["clip_id"].as_str().unwrap().to_string();
    ss.project.assets.insert("asset1".into(), Asset { id: "asset1".into(), path: "fixture.mp4".into(), width: 64, height: 36, duration, origin: None, hash: None, kind: None, fps: 25.0 });
    ss.project.find_clip_mut(&c1).unwrap().source = Source::Media { asset: "asset1".into() };
    let c2 = call(&mut ss, "add_clip", json!({"track_id": track, "source_type": "solid", "color": "#00ff00", "start": duration, "duration": 2.0}))["clip_id"].as_str().unwrap().to_string();
    (ss, track, c1, c2)
}

fn clip_by_id<'a>(cs: &'a [Value], id: &str) -> &'a Value { cs.iter().find(|c| c["id"] == id).unwrap_or_else(|| panic!("no clip {id} among {cs:?}")) }

#[test]
fn empty_spans_is_a_no_op() {
    let (mut ss, _track, c1, _c2) = fixture(6.0);
    let before = serde_json::to_value(&ss.project).unwrap();
    let r = call(&mut ss, "trim_filler_words", json!({"clip_id": c1, "spans": []}));
    assert_eq!(r["removed_seconds"].as_f64().unwrap(), 0.0);
    assert_eq!(r["clip_ids"], json!([c1]));
    let after = serde_json::to_value(&ss.project).unwrap();
    assert_eq!(before, after, "an empty span list must not touch the project");
}

#[test]
fn no_args_at_all_is_also_a_no_op() {
    let (mut ss, _track, c1, _c2) = fixture(6.0);
    let before = serde_json::to_value(&ss.project).unwrap();
    let r = call(&mut ss, "trim_filler_words", json!({"clip_id": c1}));
    assert_eq!(r["removed_seconds"].as_f64().unwrap(), 0.0);
    let after = serde_json::to_value(&ss.project).unwrap();
    assert_eq!(before, after);
}

/// The failing-first test: a single known span, after trim the timeline shrinks by exactly the summed (padded)
/// span width, the remaining pieces are in order with no gaps, and the following clip ripples left to close it.
#[test]
fn trim_removes_exactly_the_padded_span_and_ripples_the_track() {
    let (mut ss, track, c1, c2) = fixture(6.0);
    let pad = 0.05;
    // word "um" spans source time 2.0..2.3 inside c1 (0 = c1's in_point)
    let r = call(&mut ss, "trim_filler_words", json!({"clip_id": c1, "spans": [{"start": 2.0, "end": 2.3}], "pad": pad}));
    let expected_removed = (2.3 - 2.0) + 2.0 * pad;
    assert!((r["removed_seconds"].as_f64().unwrap() - expected_removed).abs() < 1e-9, "removed {:?} vs expected {expected_removed}", r["removed_seconds"]);
    assert_eq!(r["removed_spans"], 1);

    let cs = clips(&ss);
    assert_eq!(cs.len(), 3, "c1 split into two pieces plus the untouched c2");
    let ids: Vec<String> = r["clip_ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(ids.len(), 2, "two surviving pieces of c1");
    let (left, right) = (clip_by_id(&cs, &ids[0]), clip_by_id(&cs, &ids[1]));
    let (ls, ld) = (left["start"].as_f64().unwrap(), left["duration"].as_f64().unwrap());
    let (rs, rd) = (right["start"].as_f64().unwrap(), right["duration"].as_f64().unwrap());
    assert!((ls - 0.0).abs() < 1e-9, "left piece keeps the original start");
    assert!((ls + ld - rs).abs() < 1e-9, "no gap between the two surviving pieces: {ls}+{ld} vs {rs}");
    assert!((ld + rd - (6.0 - expected_removed)).abs() < 1e-9, "total duration shrank by exactly the padded span");

    let c2_now = clip_by_id(&cs, &c2);
    let c2_start = c2_now["start"].as_f64().unwrap();
    assert!((c2_start - (6.0 - expected_removed)).abs() < 1e-9, "c2 rippled left by exactly the removed amount");
    assert!((rs + rd - c2_start).abs() < 1e-9, "no gap between c1's right piece and c2");

    // audio/video stay in sync: in_point of the right piece should have advanced by exactly the span end (clip speed 1)
    let orig_in_point = 0.0;
    assert!((right["in_point"].as_f64().unwrap() - (orig_in_point + 2.3 + pad)).abs() < 1e-9, "right piece's in_point advanced past the removed span (+pad)");

    let _ = track;
}

#[test]
fn two_spans_on_one_clip_merge_when_padding_makes_them_touch() {
    let (mut ss, _track, c1, c2) = fixture(6.0);
    // "um" at 1.0..1.2 and "uh" at 1.25..1.4: with pad 0.1 these touch/overlap once padded, and must merge into one cut
    let r = call(&mut ss, "trim_filler_words", json!({"clip_id": c1, "spans": [{"start": 1.0, "end": 1.2}, {"start": 1.25, "end": 1.4}], "pad": 0.1}));
    assert_eq!(r["removed_spans"], 1, "overlapping padded spans must merge into one cut");
    let expected_removed = (1.4 - 1.0) + 2.0 * 0.1; // from (1.0-0.1) to (1.4+0.1)
    assert!((r["removed_seconds"].as_f64().unwrap() - expected_removed).abs() < 1e-9);
    let cs = clips(&ss);
    let c2_now = clip_by_id(&cs, &c2);
    assert!((c2_now["start"].as_f64().unwrap() - (6.0 - expected_removed)).abs() < 1e-9);
}

#[test]
fn three_spans_right_to_left_leave_no_gaps() {
    let (mut ss, _track, c1, c2) = fixture(10.0);
    let spans = json!([{"start": 1.0, "end": 1.2}, {"start": 4.0, "end": 4.15}, {"start": 7.0, "end": 7.3}]);
    let pad = 0.02;
    let r = call(&mut ss, "trim_filler_words", json!({"clip_id": c1, "spans": spans, "pad": pad}));
    assert_eq!(r["removed_spans"], 3);
    let expected: f64 = [(1.0, 1.2), (4.0, 4.15), (7.0, 7.3)].iter().map(|(s, e)| (e - s) + 2.0 * pad).sum();
    assert!((r["removed_seconds"].as_f64().unwrap() - expected).abs() < 1e-9);

    let cs = clips(&ss);
    let ids: Vec<String> = r["clip_ids"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert_eq!(ids.len(), 4, "3 cuts split one clip into 4 surviving pieces");
    let mut cursor = 0.0;
    let mut total = 0.0;
    for id in &ids {
        let c = clip_by_id(&cs, id);
        let st = c["start"].as_f64().unwrap();
        let du = c["duration"].as_f64().unwrap();
        assert!((st - cursor).abs() < 1e-9, "piece {id} starts at {st}, expected {cursor} (no gap)");
        cursor += du;
        total += du;
    }
    assert!((total - (10.0 - expected)).abs() < 1e-9, "pieces sum to original duration minus total removed");
    let c2_now = clip_by_id(&cs, &c2);
    assert!((c2_now["start"].as_f64().unwrap() - cursor).abs() < 1e-9, "c2 follows immediately with no gap");
}

#[test]
fn overlapping_input_spans_are_handled_not_rejected() {
    let (mut ss, _track, c1, _c2) = fixture(6.0);
    let r = call(&mut ss, "trim_filler_words", json!({"clip_id": c1, "spans": [{"start": 2.0, "end": 2.5}, {"start": 2.2, "end": 2.6}], "pad": 0.0}));
    assert_eq!(r["removed_spans"], 1, "overlapping inputs merge into one cut");
    assert!((r["removed_seconds"].as_f64().unwrap() - 0.6).abs() < 1e-9, "merged union is 2.0..2.6 = 0.6s");
}

#[test]
fn out_of_range_spans_are_clamped_or_dropped_not_fatal() {
    let (mut ss, _track, c1, _c2) = fixture(6.0);
    // fully outside the clip's source range (6s clip at speed 1 -> source range is 0..6)
    let r = call(&mut ss, "trim_filler_words", json!({"clip_id": c1, "spans": [{"start": 50.0, "end": 51.0}], "pad": 0.0}));
    assert_eq!(r["removed_seconds"].as_f64().unwrap(), 0.0);
    assert_eq!(r["removed_spans"], 0);
    assert_eq!(r["dropped_spans"].as_array().unwrap().len(), 1);

    // partially outside: clamps to the clip's own bound instead of erroring
    let r2 = call(&mut ss, "trim_filler_words", json!({"clip_id": c1, "spans": [{"start": 5.5, "end": 7.0}], "pad": 0.0}));
    assert!((r2["removed_seconds"].as_f64().unwrap() - 0.5).abs() < 1e-9, "clamped to the clip's own 6s bound: 5.5..6.0");
}

#[test]
fn covering_the_whole_clip_is_rejected_not_silently_deleted() {
    let (mut ss, _track, c1, _c2) = fixture(6.0);
    let e = call_err(&mut ss, "trim_filler_words", json!({"clip_id": c1, "spans": [{"start": 0.0, "end": 6.0}], "pad": 0.0}));
    assert!(e.contains("whole clip") || e.contains("ripple_delete"), "{e}");
}

#[test]
fn rejects_a_non_media_clip() {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    let track = call(&mut ss, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    let shape = call(&mut ss, "add_shape", json!({"track_id": track, "shape": "rect", "width": 10, "height": 10, "fill": [1.0, 1.0, 1.0, 1.0], "start": 0, "duration": 2}))["clip_id"].as_str().unwrap().to_string();
    let e = call_err(&mut ss, "trim_filler_words", json!({"clip_id": shape, "spans": [{"start": 0.0, "end": 0.5}]}));
    assert!(e.contains("media clip"), "{e}");
}

#[test]
fn rejects_an_unknown_clip_id() {
    let mut ss = Session::new();
    call(&mut ss, "project_set", json!({"width": 64, "height": 36, "fps": 25}));
    call_err(&mut ss, "trim_filler_words", json!({"clip_id": "nope", "spans": [{"start": 0.0, "end": 0.5}]}));
}

/// `words` auto-picks spans from the clip's own transcript (a take bound to this clip_id), matching case-insensitively.
#[test]
fn words_option_auto_picks_spans_from_the_clips_own_transcript() {
    let (mut ss, _track, c1, c2) = fixture(6.0);
    let take = Take { id: "take1".into(), source: TakeSource { asset_id: Some("asset1".into()), clip_id: Some(c1.clone()), path_key: None }, src_start: 0.0, src_end: 6.0, timeline_start: Some(0.0), speed: 1.0, order: 0, status: TakeStatus::Chosen, note: String::new() };
    let words = vec![
        Word::new("w1", "take1", "Haan", 0.0, 0.4, Some(0.9)),
        Word::new("w2", "take1", "Um,", 2.0, 2.3, Some(0.3)),
        Word::new("w3", "take1", "matlab", 3.0, 3.25, Some(0.4)),
        Word::new("w4", "take1", "accha", 4.0, 4.3, Some(0.95)),
    ];
    ss.project.editorial.transcripts.insert("t1".into(), Transcript { id: "t1".into(), name: "t".into(), language: "en".into(), script_text: None, takes: vec![take], words, version: 1 });

    let r = call(&mut ss, "trim_filler_words", json!({"clip_id": c1, "words": ["um", "matlab"], "pad": 0.0}));
    assert_eq!(r["words_matched"], 2);
    assert_eq!(r["removed_spans"], 2);
    let expected = (2.3 - 2.0) + (3.25 - 3.0);
    assert!((r["removed_seconds"].as_f64().unwrap() - expected).abs() < 1e-9);
    let cs = clips(&ss);
    let c2_now = clip_by_id(&cs, &c2);
    assert!((c2_now["start"].as_f64().unwrap() - (6.0 - expected)).abs() < 1e-9);
}

#[test]
fn words_option_with_no_transcript_matches_nothing_and_is_not_fatal() {
    let (mut ss, _track, c1, _c2) = fixture(6.0);
    let r = call(&mut ss, "trim_filler_words", json!({"clip_id": c1, "words": ["um"]}));
    assert_eq!(r["words_matched"], 0);
    assert_eq!(r["removed_seconds"].as_f64().unwrap(), 0.0);
}

#[test]
fn locked_track_is_rejected() {
    let (mut ss, track, c1, _c2) = fixture(6.0);
    call(&mut ss, "set_track", json!({"track_id": track, "locked": true}));
    call_err(&mut ss, "trim_filler_words", json!({"clip_id": c1, "spans": [{"start": 1.0, "end": 1.2}]}));
}
