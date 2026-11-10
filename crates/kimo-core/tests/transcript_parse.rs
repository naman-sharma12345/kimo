//! RV-8 step 2: ingest parsers (whisper JSON, faster-whisper JSON, SRT, VTT, CSV) - table tests, strict ordering/overlap, UTF-8, structured errors.
use kimo_core::transcript_parse::*;

fn ok(f: &str, s: &str) -> Parsed { parse(f, s.as_bytes()).unwrap_or_else(|e| panic!("{f}: {e}")) }
fn err(f: &str, b: &[u8]) -> ParseError { parse(f, b).err().unwrap_or_else(|| panic!("{f}: should fail")) }
fn texts(p: &Parsed) -> Vec<&str> { p.words.iter().map(|w| w.text.as_str()).collect() }

const WHISPER: &str = r#"{"language":"en","segments":[{"text":" Hello brave","words":[{"word":" Hello","start":0.0,"end":0.4,"probability":0.98},{"word":" brave","start":0.5,"end":0.9,"probability":0.31}]},{"words":[{"word":" world.","start":1.0,"end":1.5,"probability":0.9}]}]}"#;
const FASTER: &str = r#"[{"id":1,"words":[{"word":"Hi","start":0.1,"end":0.3,"probability":0.5},{"word":"there","start":0.35,"end":0.8,"probability":0.7}]}]"#;
const SRT: &str = "1\n00:00:01,000 --> 00:00:03,000\nHello brave world\n\n2\n00:00:03,500 --> 00:00:04,500\nagain\n";
const VTT: &str = "WEBVTT\n\nNOTE a comment\n\n00:01.000 --> 00:03.000 align:start\n<v Bob>Hello <b>brave</b> world\n\n00:00:03.500 --> 00:00:04.500\nagain\n";
const CSV: &str = "word,start,end,confidence\nHello,0,0.4,0.9\n\"brave, new\",0.5,0.9,\nworld,1,1.5,0.2\n";

#[test] fn whisper_json() { let p = ok("whisper_json", WHISPER); assert_eq!(texts(&p), ["Hello", "brave", "world."]); assert_eq!(p.language.as_deref(), Some("en")); assert_eq!(p.words[1].confidence, Some(0.31)); assert!(p.words.iter().all(|w| !w.interpolated)); }
#[test] fn faster_whisper_json_top_level_array_and_auto_detect() { let p = ok("faster_whisper_json", FASTER); assert_eq!(texts(&p), ["Hi", "there"]); assert_eq!(ok("auto", FASTER), p); assert_eq!(ok("auto", WHISPER), ok("whisper_json", WHISPER)); }
#[test] fn srt_words_are_interpolated_by_character_count_and_flagged() {
    let p = ok("srt", SRT); assert_eq!(texts(&p), ["Hello", "brave", "world", "again"]); assert!(p.words.iter().all(|w| w.interpolated));
    assert_eq!((p.words[0].start, p.words[2].end), (1.0, 3.0), "cue edges are kept"); let w = &p.words; assert!((w[0].end - 1.0 - 2.0 * 5.0 / 15.0).abs() < 1e-9); assert_eq!(w[0].end, w[1].start, "words tile the cue");
    assert_eq!(ok("auto", SRT), p);
}
#[test] fn vtt_strips_tags_settings_and_notes() { let p = ok("vtt", VTT); assert_eq!(texts(&p), ["Hello", "brave", "world", "again"]); assert_eq!(p.words[0].start, 1.0); assert_eq!(ok("auto", VTT), p); }
#[test] fn csv_with_header_quotes_and_optional_confidence() { let p = ok("csv", CSV); assert_eq!(texts(&p), ["Hello", "brave, new", "world"]); assert_eq!((p.words[0].confidence, p.words[1].confidence, p.words[2].confidence), (Some(0.9), None, Some(0.2))); assert_eq!(ok("auto", CSV), p); let nh = ok("csv", "Hello,0,0.4\nworld,0.5,1\n"); assert_eq!(nh.words.len(), 2); }

#[test] fn malformed_input_gives_structured_errors_with_line_or_offset() {
    let e = err("srt", b"1\n00:00:01,000 -> 00:00:02,000\nhi\n"); assert_eq!((e.code, e.line), ("bad_cue", Some(1)));
    let e = err("srt", b"1\n00:00:xx,000 --> 00:00:02,000\nhi\n"); assert_eq!((e.code, e.line), ("bad_time", Some(2)));
    let e = err("vtt", b"00:01.000 --> 00:02.000\nhi\n"); assert_eq!((e.code, e.line), ("bad_header", Some(1)));
    let e = err("csv", b"word,start,end\nhi,0,abc\n"); assert_eq!((e.code, e.line), ("bad_time", Some(2)));
    let e = err("csv", b"word,start,end\nhi,0\n"); assert_eq!((e.code, e.line), ("bad_row", Some(2)));
    let e = err("whisper_json", b"{\"segments\": [ {\"words\": [ "); assert_eq!(e.code, "bad_json"); assert!(e.line.is_some());
    let e = err("whisper_json", br#"{"segments":[{"text":"no words here"}]}"#); assert_eq!(e.code, "no_word_timestamps");
    let e = err("whisper_json", br#"{"segments":[{"words":[{"word":"a","start":0}]}]}"#); assert_eq!((e.code, e.line), ("bad_time", Some(1)));
    assert_eq!(err("srt", b"").code, "no_words"); assert_eq!(err("bogus", b"x").code, "unknown_format");
    assert!(format!("{}", err("csv", b"word,start,end\nhi,0,abc\n")).contains("line 2"));
}
#[test] fn ordering_and_overlap_are_enforced_at_ingest() {
    let e = err("csv", b"word,start,end\na,1,2\nb,0.5,0.7\n"); assert_eq!((e.code, e.line), ("non_monotonic", Some(3)));
    let e = err("csv", b"word,start,end\na,0,2\nb,1.5,3\n"); assert_eq!((e.code, e.line), ("overlap", Some(3)));
    assert!(parse("csv", b"word,start,end\na,0,1\nb,1,2\nc,1.9995,3\n").is_ok(), "<= 1 ms of overlap is tolerated (recognizer rounding)");
    let e = err("csv", b"word,start,end\na,2,1\n"); assert_eq!(e.code, "bad_time");
    let e = err("csv", b"word,start,end\na,-1,1\n"); assert_eq!(e.code, "bad_time");
    let e = err("srt", b"1\n00:00:01,000 --> 00:00:03,000\nhi\n\n2\n00:00:02,000 --> 00:00:04,000\nthere\n"); assert_eq!((e.code, e.line), ("overlap", Some(6)));
    let e = err("srt", b"1\n00:00:05,000 --> 00:00:03,000\nhi\n"); assert_eq!(e.code, "bad_time");
    let e = err("whisper_json", br#"{"segments":[{"words":[{"word":"a","start":0,"end":1},{"word":"b","start":0.5,"end":2}]}]}"#); assert_eq!((e.code, e.line), ("overlap", Some(2)));
    let e = err("csv", b"word,start,end\na,NaN,1\n"); assert_eq!(e.code, "bad_time");
}
#[test] fn utf8_is_validated_and_text_is_sanitized() {
    let e = err("csv", b"word,start,end\nhi\xff,0,1\n"); assert_eq!((e.code, e.offset), ("invalid_utf8", Some(17)));
    let mut bom = vec![0xEF, 0xBB, 0xBF]; bom.extend_from_slice(b"word,start,end\nhi,0,1\n"); assert_eq!(parse("csv", &bom).unwrap().words.len(), 1, "BOM is stripped");
    let p = ok("csv", "word,start,end\nh\u{202E}i\u{200B}!\u{7}x,0,1\n"); assert_eq!(p.words[0].text, "hi!x", "bidi override, zero-width and control characters removed");
    assert_eq!(sanitize("  a\t b\u{0}\n c "), "a b c"); assert_eq!(sanitize("caf\u{e9} \u{1F600} \u{4e2d}\u{6587}"), "caf\u{e9} \u{1F600} \u{4e2d}\u{6587}", "ordinary unicode survives");
    let e = err("csv", format!("word,start,end\n{},0,1\n", "x".repeat(201)).as_bytes()); assert_eq!(e.code, "text_too_long");
    let e = err("csv", "word,start,end\n\u{200B},0,1\n".as_bytes()); assert_eq!(e.code, "empty_text", "a word that sanitizes to nothing is rejected, not stored empty");
}

#[test] fn lossy_utf8_and_clamped_overlaps_are_opt_in_and_reported() {
    let o = Opts { lossy_utf8: true, clamp_overlaps: true };
    let p = parse_with("csv", b"word,start,end\nhi\xff,0,1\n", o).unwrap(); assert_eq!(p.words[0].text, "hi\u{FFFD}"); assert!(p.warnings[0].contains("replaced with U+FFFD") && p.warnings[0].contains("17"));
    let p = parse_with("csv", b"word,start,end\na,0,2\nb,1.5,3\n", o).unwrap(); assert_eq!(p.words[0].end, 1.5, "previous end trimmed to the next start"); assert!(p.warnings[0].contains("trimmed"));
    // clamping never repairs out-of-order starts, and strict stays the default
    assert_eq!(parse_with("csv", b"word,start,end\na,1,2\nb,0.5,0.7\n", o).unwrap_err().code, "non_monotonic"); assert_eq!(err("csv", b"word,start,end\na,0,2\nb,1.5,3\n").code, "overlap");
    let junk: Vec<u8> = (0..=255u8).cycle().take(3000).collect(); for f in ["auto", "srt", "vtt", "csv", "json"] { let _ = parse_with(f, &junk, o); let _ = parse(f, &junk); } // never panics
}
#[test] fn fractional_seconds_irregular_timing_and_multiline_blocks() {
    let srt = "1\n00:00:01,5 --> 00:00:02,25\n- Hi there\n- how are you\n\n2\n00:00:02,25 --> 00:00:03,0\nfine\n"; let p = ok("srt", srt);
    assert_eq!(texts(&p), ["-", "Hi", "there", "-", "how", "are", "you", "fine"], "multiline dialogue is one cue"); assert_eq!(p.words[0].start, 1.5); assert_eq!(p.words[6].end, 2.25); assert_eq!(p.words[7].start, 2.25, "touching cues are not an overlap");
    let v = ok("vtt", "WEBVTT\n\n1:02.500 --> 1:03.125\nok\n"); assert_eq!((v.words[0].start, v.words[0].end), (62.5, 63.125));
    let c = ok("csv", "word,start,end\na,0.0333667,0.0667333\nb,0.0667333,0.1001\nc,0.5,2.7\n"); assert_eq!(c.words.len(), 3, "irregular 29.97 fps-style and variable gaps");
    let two = ok("srt", "1\n00:00:01,000 --> 00:00:02,000\n<i>italic</i> and {\\an8}text\n"); assert_eq!(texts(&two), ["italic", "and", "text"], "tags and override blocks stripped in SRT too"); assert_eq!(strip_markup("a <b unclosed"), "a ");
}

/// Deterministic mutation fuzz over malformed SRT/VTT/CSV/JSON: negative and overlapping timestamps, unterminated tags, invalid UTF-8, truncation, byte flips.
/// Every input must return Ok or a structured Err (strict and graceful options) - never panic.
#[test] fn mutation_fuzz_malformed_subtitles_never_panics() {
    let seeds: [&[u8]; 4] = [
        b"1\n00:00:01,000 --> 00:00:02,500\n<i>Hello</i> world\n\n2\n00:00:02,400 --> 00:00:04,000\nsecond <b>line\nmore\n\n",
        b"WEBVTT\n\n00:00:01.000 --> 00:00:02.500 align:start\n<v Bob>Hi <00:00:01.500>there</v>\n\n00:00:02.000 --> 00:00:03.000\nx &amp; y\n\n",
        b"word,start,end,confidence\nhello,0.0,0.4,0.9\nworld,0.5,0.9,0.8\n",
        br#"{"segments":[{"words":[{"word":" hi","start":0.0,"end":0.4,"probability":0.9},{"word":" there","start":0.4,"end":0.9}]}]}"#,
    ];
    let inject: [&[u8]; 12] = [b"-", b"-1", b"99:99:99,999", b"00:00:05,000 --> 00:00:01,000", b"<", b"<i", b"</", b"\xff\xfe", b"\xc3", b"\x00", b"\r\n", b"9999999999999999999"];
    let (o_strict, o_lossy) = (Opts::default(), Opts { lossy_utf8: true, clamp_overlaps: true, ..Opts::default() });
    let mut x: u64 = 0x9E3779B97F4A7C15; let mut next = move || { x ^= x << 13; x ^= x >> 7; x ^= x << 17; x };
    let mut parsed = 0usize;
    for i in 0..20_000usize {
        let mut b = seeds[i % 4].to_vec();
        for _ in 0..(1 + next() % 4) { if b.is_empty() { break; } let p = (next() as usize) % b.len();
            match next() % 5 { 0 => b[p] ^= 1 << (next() % 8), 1 => { let ins = inject[(next() as usize) % inject.len()]; b.splice(p..p, ins.iter().copied()); } 2 => b.truncate(p), 3 => { b.remove(p); } _ => { let q = (next() as usize) % b.len(); b.swap(p, q); } } }
        for f in ["auto", "srt", "vtt", "csv", "whisper_json", "faster_whisper_json"] { for o in [o_strict, o_lossy] { if parse_with(f, &b, o).is_ok() { parsed += 1; } } }
    }
    assert!(parsed > 1000, "some mutated inputs must still parse (got {parsed}) - otherwise the fuzz is not reaching the parsers");
}
