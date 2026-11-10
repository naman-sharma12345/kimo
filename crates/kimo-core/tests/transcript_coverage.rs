//! RV-8 step 4: script coverage (pure). Take-order fixture, dropped take, repeats, extras, fuzzy tokens, limits.
use kimo_core::transcript_coverage::*;

const SCRIPT: &str = "The quick brown fox jumps. Over the lazy dog today! We love foxes and dogs.";
fn ws(take: &str, text: &str, start: usize) -> Vec<(String, String, String)> { text.split_whitespace().enumerate().map(|(i, t)| (format!("w{}", start + i), t.to_string(), take.to_string())).collect() }
fn all(parts: &[(&str, &str)]) -> Vec<(String, String, String)> { let mut v = vec![]; for (t, x) in parts { let n = v.len(); v.extend(ws(t, x, n + 1)); } v }

#[test] fn full_coverage_in_script_order_even_when_takes_were_recorded_out_of_order() {
    // 3 takes; given to coverage already assembled by `order` (the caller passes script order), punctuation and case differ from the script
    let w = all(&[("t3", "the quick, brown FOX jumps"), ("t1", "over the lazy dog today"), ("t2", "we love foxes and dogs")]);
    let c = coverage(SCRIPT, &w).unwrap(); assert_eq!((c.coverage_pct, c.covered_tokens, c.script_tokens), (100.0, 15, 15)); assert!(c.missing_passages.is_empty() && c.extra_words.is_empty() && c.repeated_passages.is_empty(), "{c:?}");
}
#[test] fn a_dropped_take_names_the_missing_passage_with_neighbours() {
    let w = all(&[("t1", "the quick brown fox jumps"), ("t3", "we love foxes and dogs")]); let c = coverage(SCRIPT, &w).unwrap();
    assert_eq!(c.missing_passages.len(), 1, "{c:?}"); let m = &c.missing_passages[0]; assert_eq!((m.script_range, m.text.as_str()), ((5, 9), "Over the lazy dog today!")); assert_eq!(m.nearest_words, ["w5", "w6"]);
    assert_eq!((c.covered_tokens, c.coverage_pct), (10, 66.7));
}
#[test] fn extra_words_repeats_and_near_matches() {
    let w = all(&[("t1", "the quick brown fox jumps"), ("t2", "the quick brown fox jumps"), ("t2", "over the lazy dog todai um we love foxes and dogs")]);
    let c = coverage(SCRIPT, &w).unwrap(); assert!(c.missing_passages.is_empty(), "todai is a near match of today: {c:?}"); assert_eq!(c.covered_tokens, 15);
    assert_eq!(c.repeated_passages.len(), 1, "{c:?}"); assert_eq!(c.repeated_passages[0].takes, ["t1", "t2"]); assert_eq!(c.repeated_passages[0].words.len(), 5);
    assert!(!c.extra_words.is_empty() && c.extra_words.iter().any(|x| x == "w16"), "the 'um' and one copy of the retake are extra: {:?}", c.extra_words);
}
#[test] fn never_invents_text_and_handles_degenerate_input() {
    assert!(coverage("   ", &[]).is_err()); let c = coverage("alpha beta", &[]).unwrap(); assert_eq!((c.coverage_pct, c.missing_passages[0].text.as_str()), (0.0, "alpha beta")); assert!(c.missing_passages[0].nearest_words.is_empty());
    let big: Vec<_> = (0..5000).map(|i| (format!("w{i}"), "x".to_string(), "t".to_string())).collect(); let script = vec!["y"; 5000].join(" "); assert!(coverage(&script, &big).unwrap_err().contains("too large"), "25M cells is over the 16M cap");
}
