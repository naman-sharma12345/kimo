//! Transcript ingest parsers (RV-8 step 2). Pure: bytes in, words out, structured errors with line/offset. No MCP or Session dependency (AC-1).
//!
//! Every parser enforces at ingest: valid UTF-8 (invalid bytes are an error with their byte offset, never replaced silently), sanitized text (control,
//! zero-width and bidi-override characters removed), finite times, end >= start, STRICTLY non-decreasing start times and no overlap between consecutive
//! words/cues (beyond OVERLAP_TOL seconds). Whatever the file says is kept as `recognized`; nothing is invented, except SRT/VTT cues without word timing,
//! whose words get times interpolated by character count and are flagged `interpolated`.
use serde::Serialize;

pub const OVERLAP_TOL: f64 = 0.001;
pub const MAX_WORD_CHARS: usize = 200;
#[derive(Clone, Debug, PartialEq)]
pub struct PWord { pub text: String, pub start: f64, pub end: f64, pub confidence: Option<f32>, pub interpolated: bool }
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Parsed { pub language: Option<String>, pub words: Vec<PWord>, pub format: &'static str, /// Things the parser repaired because the caller opted in (lossy UTF-8, clamped overlaps). Empty for strict parses.
    pub warnings: Vec<String> }
/// Ingest options. The defaults are strict: invalid UTF-8 and overlaps are errors.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Opts { /// Replace invalid UTF-8 bytes with U+FFFD (reported in `warnings`) instead of failing.
    pub lossy_utf8: bool, /// Trim the previous word/cue end to the next start instead of failing on an overlap (reported in `warnings`).
    pub clamp_overlaps: bool }
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ParseError { pub code: &'static str, pub message: String, #[serde(skip_serializing_if = "Option::is_none")] pub line: Option<usize>, #[serde(skip_serializing_if = "Option::is_none")] pub offset: Option<usize> }
impl ParseError {
    fn at(code: &'static str, line: usize, m: impl Into<String>) -> Self { ParseError { code, message: m.into(), line: Some(line), offset: None } }
    fn plain(code: &'static str, m: impl Into<String>) -> Self { ParseError { code, message: m.into(), line: None, offset: None } }
}
impl std::fmt::Display for ParseError { fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result { write!(f, "{}: {}", self.code, self.message)?; if let Some(l) = self.line { write!(f, " (line {l})")?; } if let Some(o) = self.offset { write!(f, " (byte {o})")?; } Ok(()) } }

/// Strict UTF-8 decode (BOM stripped); the error carries the byte offset of the first invalid sequence.
pub fn decode_utf8(b: &[u8]) -> Result<&str, ParseError> {
    let b = b.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(b);
    std::str::from_utf8(b).map_err(|e| ParseError { code: "invalid_utf8", message: "input is not valid UTF-8".into(), line: None, offset: Some(e.valid_up_to()) })
}
fn bad_char(c: char) -> bool { (c.is_control() && c != ' ') || matches!(c, '\u{200B}'..='\u{200F}' | '\u{2028}' | '\u{2029}' | '\u{202A}'..='\u{202E}' | '\u{2060}'..='\u{2064}' | '\u{2066}'..='\u{2069}' | '\u{FEFF}') }
/// Removes control / zero-width / bidi-override characters and collapses whitespace.
pub fn sanitize(s: &str) -> String { s.chars().map(|c| if c.is_whitespace() { ' ' } else { c }).filter(|c| !bad_char(*c)).collect::<String>().split_whitespace().collect::<Vec<_>>().join(" ") }

/// Shared ordering rules. `where_` is the line (text formats) or word index (JSON) used in the error.
fn check_word(prev: Option<&PWord>, w: &PWord, line: usize) -> Result<(), ParseError> {
    if !(w.start.is_finite() && w.end.is_finite()) { return Err(ParseError::at("bad_time", line, "time is not a finite number")); }
    if w.start < 0.0 { return Err(ParseError::at("bad_time", line, format!("negative start time {}", w.start))); }
    if w.end < w.start { return Err(ParseError::at("bad_time", line, format!("end {} is before start {}", w.end, w.start))); }
    if let Some(p) = prev {
        if w.start < p.start { return Err(ParseError::at("non_monotonic", line, format!("start {} is before the previous start {}", w.start, p.start))); }
        if w.start < p.end - OVERLAP_TOL { return Err(ParseError::at("overlap", line, format!("starts at {} before the previous word/cue ends at {}", w.start, p.end))); }
    }
    if w.text.is_empty() { return Err(ParseError::at("empty_text", line, "empty word")); }
    if w.text.chars().count() > MAX_WORD_CHARS { return Err(ParseError::at("text_too_long", line, format!("word longer than {MAX_WORD_CHARS} characters"))); }
    Ok(())
}
fn finish(format: &'static str, language: Option<String>, mut words: Vec<PWord>, lines: Vec<usize>, o: Opts) -> Result<Parsed, ParseError> {
    if words.is_empty() { return Err(ParseError::plain("no_words", "no words found")); }
    let mut warnings = vec![];
    for i in 0..words.len() {
        if o.clamp_overlaps && i > 0 && words[i].start.is_finite() && words[i - 1].end.is_finite() && words[i].start >= words[i - 1].start && words[i].start < words[i - 1].end - OVERLAP_TOL {
            warnings.push(format!("line {}: overlap with the previous word/cue; its end {} was trimmed to {}", lines[i], words[i - 1].end, words[i].start)); let st = words[i].start; words[i - 1].end = st; }
        check_word(if i > 0 { Some(&words[i - 1]) } else { None }, &words[i], lines[i])?;
    }
    Ok(Parsed { language, words, format, warnings })
}
fn parse_ts(s: &str) -> Option<f64> {
    let s = s.trim().replace(',', "."); let p: Vec<&str> = s.split(':').collect();
    let (h, m, sec) = match p.len() { 3 => (p[0].parse::<f64>().ok()?, p[1].parse::<f64>().ok()?, p[2].parse::<f64>().ok()?), 2 => (0.0, p[0].parse::<f64>().ok()?, p[1].parse::<f64>().ok()?), _ => return None };
    if m >= 60.0 || sec >= 60.0 || h < 0.0 || m < 0.0 || sec < 0.0 { return None; } Some(h * 3600.0 + m * 60.0 + sec)
}
/// Words of one cue with times interpolated by character count across [start, end]; all flagged interpolated.
fn cue_words(text: &str, start: f64, end: f64) -> Vec<PWord> {
    let toks: Vec<&str> = text.split_whitespace().collect(); let total: usize = toks.iter().map(|t| t.chars().count()).sum::<usize>().max(1); let mut at = start; let mut out = vec![];
    for (i, t) in toks.iter().enumerate() { let e = if i + 1 == toks.len() { end } else { at + (end - start) * t.chars().count() as f64 / total as f64 }; out.push(PWord { text: (*t).to_string(), start: at, end: e, confidence: None, interpolated: true }); at = e; } out
}
/// Removes `<...>` tags (<i>, <b>, <v Bob>, <c.x>, VTT timestamps) and `{...}` override blocks ({\\an8}); an unclosed tag drops the rest of the line, never panics.
pub fn strip_markup(s: &str) -> String { let (mut o, mut tag, mut brace) = (String::new(), false, false); for c in s.chars() { match c { '<' if !brace => tag = true, '>' if tag => tag = false, '{' if !tag => brace = true, '}' if brace => brace = false, _ if !tag && !brace => o.push(c), _ => {} } } o }
/// WebVTT's named escapes. Decode once (not recursively): &amp;lt; means literal &lt;.
fn decode_vtt_entities(s: &str) -> String {
    let mut out = String::new(); let mut rest = s;
    while !rest.is_empty() {
        let mut hit = false;
        for (escape, value) in [("&amp;", "&"), ("&lt;", "<"), ("&gt;", ">"), ("&nbsp;", "\u{00A0}"), ("&lrm;", "\u{200E}"), ("&rlm;", "\u{200F}")] {
            if let Some(tail) = rest.strip_prefix(escape) { out.push_str(value); rest = tail; hit = true; break; }
        }
        if !hit { let ch = rest.chars().next().unwrap(); out.push(ch); rest = &rest[ch.len_utf8()..]; }
    }
    out
}
/// SRT and VTT share one cue reader: blocks separated by blank lines; the line holding `-->` carries the times.
fn parse_cues(text: &str, vtt: bool, o: Opts) -> Result<Parsed, ParseError> {
    let mut words = vec![]; let mut lines = vec![]; let all: Vec<&str> = text.lines().collect(); let mut i = 0;
    if vtt { if !all.first().map_or(false, |l| l.trim_start().starts_with("WEBVTT")) { return Err(ParseError::at("bad_header", 1, "VTT file must start with WEBVTT")); } i = 1; }
    while i < all.len() {
        if all[i].trim().is_empty() { i += 1; continue; }
        if vtt && (all[i].starts_with("NOTE") || all[i].starts_with("STYLE") || all[i].starts_with("REGION")) { while i < all.len() && !all[i].trim().is_empty() { i += 1; } continue; }
        let mut j = i; if !all[j].contains("-->") { j += 1; }
        if j >= all.len() || !all[j].contains("-->") { return Err(ParseError::at("bad_cue", i + 1, "expected a timing line with -->")); }
        let tl = all[j]; let (a, rest) = tl.split_once("-->").unwrap(); let b = rest.trim().split_whitespace().next().unwrap_or("");
        let (s, e) = match (parse_ts(a), parse_ts(b)) { (Some(s), Some(e)) => (s, e), _ => return Err(ParseError::at("bad_time", j + 1, format!("cannot read timestamps in '{}'", sanitize(tl)))) };
        let mut body = vec![]; let mut k = j + 1; while k < all.len() && !all[k].trim().is_empty() { body.push(all[k]); k += 1; }
        let t = strip_markup(&body.join(" "));
        // Decode only after stripping markup, so escaped literal angle brackets stay text.
        let t = if vtt { decode_vtt_entities(&t) } else { t };
        let t = sanitize(&t);
        if t.is_empty() { i = k; continue; } // an empty cue carries no words
        if e < s { return Err(ParseError::at("bad_time", j + 1, format!("cue end {e} is before start {s}"))); }
        for w in cue_words(&t, s, e) { words.push(w); lines.push(j + 1); }
        i = k;
    }
    finish(if vtt { "vtt" } else { "srt" }, None, words, lines, o)
}
fn parse_csv(text: &str, o: Opts) -> Result<Parsed, ParseError> {
    fn split(l: &str) -> Vec<String> { let mut out = vec![]; let mut cur = String::new(); let mut q = false; let mut it = l.chars().peekable(); while let Some(c) = it.next() { match c { '"' if q && it.peek() == Some(&'"') => { cur.push('"'); it.next(); } '"' => q = !q, ',' if !q => { out.push(std::mem::take(&mut cur)); } _ => cur.push(c) } } out.push(cur); out }
    let (mut words, mut lines) = (vec![], vec![]);
    for (n, l) in text.lines().enumerate() {
        if l.trim().is_empty() { continue; } let f = split(l);
        if n == 0 && f.get(1).and_then(|x| x.trim().parse::<f64>().ok()).is_none() { if f.len() >= 3 && f[0].trim().eq_ignore_ascii_case("word") { continue; } return Err(ParseError::at("bad_header", 1, "first line must be a header 'word,start,end[,confidence]' or the first word")); }
        if f.len() < 3 { return Err(ParseError::at("bad_row", n + 1, "need word,start,end[,confidence]")); }
        let num = |s: &str, what: &str| s.trim().parse::<f64>().map_err(|_| ParseError::at("bad_time", n + 1, format!("{what} '{}' is not a number", sanitize(s))));
        let conf = match f.get(3).map(|s| s.trim()).filter(|s| !s.is_empty()) { Some(c) => Some(num(c, "confidence")? as f32), None => None };
        words.push(PWord { text: sanitize(&f[0]), start: num(&f[1], "start")?, end: num(&f[2], "end")?, confidence: conf, interpolated: false }); lines.push(n + 1);
    }
    finish("csv", None, words, lines, o)
}
/// whisper (`segments[].words[]` with word/start/end/probability) and faster-whisper (same shape, also as a bare array of segments).
fn parse_json(bytes: &str, o: Opts) -> Result<Parsed, ParseError> {
    let v: serde_json::Value = serde_json::from_str(bytes).map_err(|e| ParseError { code: "bad_json", message: e.to_string(), line: Some(e.line()), offset: None })?;
    let segs = if let Some(a) = v.as_array() { a.clone() } else { v.get("segments").and_then(|s| s.as_array()).cloned().ok_or_else(|| ParseError::plain("bad_json", "expected a 'segments' array (or a top-level array of segments)"))? };
    let language = v.get("language").and_then(|l| l.as_str()).map(sanitize).filter(|s| !s.is_empty());
    let (mut words, mut lines) = (vec![], vec![]); let mut idx = 0usize;
    for (si, s) in segs.iter().enumerate() {
        let ws = s.get("words").and_then(|w| w.as_array()).ok_or_else(|| ParseError::plain("no_word_timestamps", format!("segment {si} has no 'words' array; transcribe with word timestamps enabled")))?;
        for w in ws {
            idx += 1; let text = w.get("word").or_else(|| w.get("text")).and_then(|t| t.as_str()).ok_or_else(|| ParseError::at("bad_word", idx, "word has no text"))?;
            let f = |k: &str| w.get(k).and_then(|x| x.as_f64()).ok_or_else(|| ParseError::at("bad_time", idx, format!("word {idx}: '{k}' missing or not a number")));
            let conf = w.get("probability").or_else(|| w.get("confidence")).and_then(|x| x.as_f64()).map(|c| c as f32);
            words.push(PWord { text: sanitize(text), start: f("start")?, end: f("end")?, confidence: conf, interpolated: false }); lines.push(idx);
        }
    }
    finish("whisper_json", language, words, lines, o)
}
/// `format`: auto | whisper_json | faster_whisper_json | json | srt | vtt | csv. `auto` sniffs the content. Strict defaults.
pub fn parse(format: &str, bytes: &[u8]) -> Result<Parsed, ParseError> { parse_with(format, bytes, Opts::default()) }
pub fn parse_with(format: &str, bytes: &[u8], o: Opts) -> Result<Parsed, ParseError> {
    let lossy; let mut warn = None;
    let text: &str = match decode_utf8(bytes) { Ok(t) => t, Err(e) if o.lossy_utf8 => { let b = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF][..]).unwrap_or(bytes); lossy = String::from_utf8_lossy(b).into_owned(); warn = Some(format!("invalid UTF-8 (first bad byte {}); replaced with U+FFFD", e.offset.unwrap_or(0))); &lossy } Err(e) => return Err(e) };
    let t = text.trim_start();
    let fmt = match format { "auto" => if t.starts_with("WEBVTT") { "vtt" } else if t.starts_with('{') || t.starts_with('[') { "json" } else if t.contains("-->") { "srt" } else { "csv" }, f @ ("srt" | "vtt" | "csv" | "json") => f, "whisper_json" | "faster_whisper_json" => "json", o => return Err(ParseError::plain("unknown_format", format!("format must be auto|whisper_json|faster_whisper_json|srt|vtt|csv, got '{}'", sanitize(o)))) };
    let mut p = match fmt { "srt" => parse_cues(text, false, o), "vtt" => parse_cues(text, true, o), "csv" => parse_csv(text, o), _ => parse_json(text, o) }?;
    if let Some(w) = warn { p.warnings.insert(0, w); } Ok(p)
}
