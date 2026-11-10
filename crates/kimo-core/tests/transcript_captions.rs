//! RV-8 step 6: deterministic caption grouping (pure) - overlaps, lines, orphans, reading speed, determinism, SRT/VTT.
use kimo_core::transcript_captions::*;

const TEXT: &str = "So the first thing you want to check is the tyre pressure, because it changes everything. Cold tyres read lower than warm ones! Never trust a gauge you have not tested. If the car pulls to one side, stop, look at the tread, and compare both fronts; wear tells you the story. Then rotate them every ten thousand kilometres. Simple. Fit the winter set before the first frost arrives, not after the first slide into a ditch. That is the whole routine, and it takes about twenty minutes once you have done it twice. Keep a log in the glovebox so the next service knows what you did and when you did it. Thanks for watching and see you next week.";
fn words(gap: f64) -> Vec<W> { let mut t = 0.0; TEXT.split_whitespace().map(|x| { let d = 0.12 + 0.045 * x.trim_matches(|c: char| !c.is_alphanumeric()).chars().count() as f64; let w = W { text: x.into(), start: t, end: t + d }; t += d + if sentence_end(x) { 0.45 } else { gap }; w }).collect() }
fn p() -> Params { Params::new(2.0, 25.0) }
fn overlaps(c: &[Cue]) -> usize { c.windows(2).filter(|w| w[1].start < w[0].end - 1e-9).count() }
fn naive(ws: &[W]) -> Vec<(f64, f64)> { ws.chunks(9).map(|c| (c[0].start, c[c.len() - 1].end + 0.6)).collect() } // fixed chunks, padded for reading time: the old behaviour

#[test] fn naive_grouping_overlaps_new_grouping_does_not() {
    let ws = words(0.0); let nv = naive(&ws); let bad = nv.windows(2).filter(|w| w[1].0 < w[0].1).count(); assert!(bad >= 3, "harness must reproduce the overlaps, got {bad}");
    let c = group(&ws, &p()).unwrap(); assert_eq!(overlaps(&c), 0); let gap = 2.0 / 25.0; assert!(c.windows(2).all(|w| w[1].start - w[0].end >= gap - 1e-9 || w[1].start - w[0].end >= -1e-9 && w[1].start - w[0].end >= gap - 1e-9), "min gap of 2 frames");
}
#[test] fn lines_orphans_speed_and_durations_hold() {
    let ws = words(0.05); let c = group(&ws, &p()).unwrap(); assert!(c.len() >= 12);
    let covered: usize = c.iter().map(|x| x.last - x.first + 1).sum(); assert_eq!(covered, ws.len(), "every word in exactly one cue"); for w in c.windows(2) { assert_eq!(w[1].first, w[0].last + 1); }
    for (i, q) in c.iter().enumerate() {
        let lines: Vec<&str> = q.text.split('\n').collect(); assert!(lines.len() <= 2, "{}", q.text); assert!(lines.iter().all(|l| l.chars().count() <= 42), "{}", q.text);
        if q.last > q.first { assert!(lines.iter().all(|l| l.split(' ').count() >= 2), "orphan line in {:?}", q.text); }
        let n = q.last - q.first + 1; assert!(n >= 3 || sentence_end(&ws[q.last].text) || i == c.len() - 1, "short cue {:?}", q.text);
        let dur = q.end - q.start; assert!(dur <= 6.0 + 1e-9 && dur >= 1.0 - 1e-9 || i + 1 < c.len() && c[i + 1].start - q.start < 1.0 + 0.08, "dur {dur} {:?}", q.text);
        let ch = q.text.replace('\n', " ").chars().count() as f64; assert!(ch / dur <= 17.0 + 1e-6, "cps {} {:?}", ch / dur, q.text); assert!(q.warnings.is_empty(), "{:?}", q.warnings);
    }
}
#[test] fn prefers_sentence_ends_and_is_deterministic() {
    let ws = words(0.05); let a = group(&ws, &p()).unwrap(); let b = group(&ws, &p()).unwrap(); assert_eq!(a, b);
    let ends = a.iter().filter(|q| sentence_end(&ws[q.last].text)).count(); let sentences = TEXT.matches(|c| c == '.' || c == '!').count(); assert!(ends + 3 >= sentences, "{ends} of {sentences} sentence ends kept as breaks");
    assert!(a.iter().all(|q| !(q.last < ws.len() - 1) || !(q.text.ends_with(" the") || q.text.ends_with(" a"))), "no cue ends on a bare article");
}
#[test] fn wrap_is_balanced_and_never_orphans() {
    let w = ["alpha", "beta", "gamma", "delta", "epsilon", "zeta", "eta", "theta"]; let br = wrap(&w, 2, 24).unwrap(); assert_eq!(br.len(), 1);
    let t = cue_text(&w, &br); let ls: Vec<&str> = t.lines().collect(); assert!(ls.iter().all(|l| l.len() <= 24) && (ls[0].len() as i32 - ls[1].len() as i32).abs() <= 8, "{t}");
    assert!(wrap(&["supercalifragilistic"], 2, 10).is_none()); assert_eq!(wrap(&["a", "b"], 2, 42), Some(vec![]), "fits on one line");
    let t3 = wrap(&["aaaaaaaaaa", "bbbbbbbbbb", "c"], 2, 21).unwrap(); assert_eq!(cue_text(&["aaaaaaaaaa", "bbbbbbbbbb", "c"], &t3), "aaaaaaaaaa\nbbbbbbbbbb c", "no one-word last line when avoidable");
}
#[test] fn bad_params_and_empty_input_are_errors_and_long_word_is_reported() {
    let mut q = p(); q.max_lines = 0; assert!(group(&words(0.0), &q).is_err()); assert!(group(&[], &p()).is_err());
    let c = group(&[W { text: "x".repeat(60), start: 0.0, end: 1.0 }], &p()).unwrap(); assert_eq!(c[0].warnings, vec!["LINE_TOO_LONG"]);
}
#[test] fn srt_and_vtt_match_the_cues() {
    let ws = words(0.05); let c = group(&ws, &p()).unwrap(); let v: Vec<(f64, f64, String)> = c.iter().map(|q| (q.start, q.end, q.text.clone())).collect();
    let srt = to_srt(&v); let blocks: Vec<&str> = srt.trim_end().split("\n\n").collect(); assert_eq!(blocks.len(), c.len());
    for (b, q) in blocks.iter().zip(&c) { let l: Vec<&str> = b.lines().collect(); let (a, e) = l[1].split_once(" --> ").unwrap(); let pt = |s: &str| { let p: Vec<f64> = s.replace(',', ".").split(':').map(|x| x.parse().unwrap()).collect(); p[0] * 3600.0 + p[1] * 60.0 + p[2] };
        assert!((pt(a) - q.start).abs() <= 0.0005 + 1e-9 && (pt(e) - q.end).abs() <= 0.0005 + 1e-9); assert_eq!(l[2..].join("\n"), q.text); }
    let vtt = to_vtt(&v); assert!(vtt.starts_with("WEBVTT\n\n") && vtt.contains("00:00:00.") && !vtt.contains(" --> 00:00:00,"));
}
#[test] fn files_are_safe_atomic_and_never_overwrite_by_default() {
    let v = vec![(0.0, 1.5, "a --> b & <c>\n\nd".to_string())];
    let srt = to_srt(&v); assert!(!srt.contains("a -->") && srt.contains("a -> b & <c>"), "{srt}"); assert!(!srt.contains("\n\n\n") && !srt.contains('\r'), "no blank line inside a cue, LF only");
    let vtt = to_vtt(&v); assert!(vtt.contains("&amp;") && vtt.contains("&lt;c>"), "{vtt}"); assert_eq!(to_srt(&[(360_000.0, 360_001.0, "x".into())]).lines().nth(1).unwrap(), "100:00:00,000 --> 100:00:01,000", "hours >= 100");
    let p = std::env::temp_dir().join(format!("kimo_atomic_{}.srt", std::process::id())); let _ = std::fs::remove_file(&p);
    write_atomic(&p, "one", false).unwrap(); let e = write_atomic(&p, "two", false).unwrap_err(); assert!(e.starts_with("dest_exists"), "{e}"); assert_eq!(std::fs::read_to_string(&p).unwrap(), "one");
    write_atomic(&p, "two", true).unwrap(); assert_eq!(std::fs::read_to_string(&p).unwrap(), "two"); assert!(std::fs::read_dir(std::env::temp_dir()).unwrap().all(|e| !e.unwrap().file_name().to_string_lossy().contains(&format!(".tmp{}", std::process::id()))), "no temp file left"); let _ = std::fs::remove_file(&p);
}
