//! RV-8: deterministic, phrase-aware caption grouping from timed words, plus SRT/VTT rendering of the resulting cues.
//! Pure functions over (text, start, end) triples: no Session, no rasterizer, no font metrics (AC-1). Layout is character-count based, so it is instantaneous.
//! Rules (ARCH spec): <= max_lines lines of <= max_chars each, balanced lines, no single-word line in a multi-word cue, >= min_words words per cue unless the
//! sentence ends, reading speed <= max_cps where the timing allows, durations in [min_dur, max_dur], and `gap_s` between consecutive cues (no overlaps).
//! Breaks prefer sentence ends, then clause punctuation, then pauses > 0.3 s. Identical input always gives identical output.

#[derive(Clone, Debug)]
pub struct Params { pub max_lines: usize, pub max_chars: usize, pub max_cps: f64, pub min_dur: f64, pub max_dur: f64, pub gap_s: f64, pub min_words: usize, pub pause_s: f64 }
impl Params {
    pub fn new(gap_frames: f64, fps: f64) -> Params { Params { max_lines: 2, max_chars: 42, max_cps: 17.0, min_dur: 1.0, max_dur: 6.0, gap_s: gap_frames / fps.max(1.0), min_words: 3, pause_s: 0.3 } }
    pub fn check(&self) -> Result<(), String> {
        if !(1..=4).contains(&self.max_lines) { return Err("max_lines must be 1..4".into()); } if !(8..=120).contains(&self.max_chars) { return Err("max_chars_per_line must be 8..120".into()); }
        if !(self.max_cps.is_finite() && self.max_cps >= 1.0) { return Err("max_cps must be >= 1".into()); }
        if !(self.min_dur.is_finite() && self.max_dur.is_finite() && self.min_dur > 0.0 && self.max_dur >= self.min_dur) { return Err("need 0 < min_dur <= max_dur".into()); }
        if !(self.gap_s.is_finite() && self.gap_s >= 0.0) { return Err("min_gap_frames must be >= 0".into()); } if self.min_words == 0 { return Err("min_words must be >= 1".into()); } Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct W { pub text: String, pub start: f64, pub end: f64 }
#[derive(Clone, Debug, PartialEq)]
pub struct Cue { /// Inclusive indices into the input words.
    pub first: usize, pub last: usize, /// Indices relative to `first` after which the text breaks to a new line.
    pub breaks_after: Vec<usize>, pub start: f64, pub end: f64, pub text: String, pub warnings: Vec<String> }

fn core(t: &str) -> &str { t.trim_end_matches(|c: char| matches!(c, '"' | '\'' | ')' | ']' | '”' | '’' | '»')) }
pub fn sentence_end(t: &str) -> bool { core(t).chars().last().map_or(false, |c| matches!(c, '.' | '?' | '!' | '…')) }
pub fn clause_end(t: &str) -> bool { core(t).chars().last().map_or(false, |c| matches!(c, ',' | ';' | ':' | '—' | '–')) }
fn clen(s: &str) -> usize { s.chars().count() }

/// Balanced wrap of words into the fewest lines (<= max_lines) that fit; returns break indices (after word i), or None when impossible.
pub fn wrap(words: &[&str], max_lines: usize, max_chars: usize) -> Option<Vec<usize>> {
    let n = words.len(); if n == 0 { return Some(vec![]); } let l: Vec<usize> = words.iter().map(|w| clen(w)).collect();
    let mut pre = vec![0usize; n + 1]; for i in 0..n { pre[i + 1] = pre[i] + l[i]; }
    let linelen = |a: usize, b: usize| pre[b] - pre[a] + (b - a - 1); // words a..b
    for k in 1..=max_lines.min(n) {
        let inf = f64::INFINITY; let mut dp = vec![vec![inf; n + 1]; k + 1]; let mut from = vec![vec![0usize; n + 1]; k + 1]; dp[0][0] = 0.0;
        for q in 1..=k { for i in q..=n { for j in (q - 1)..i { if dp[q - 1][j] == inf { continue; } let len = linelen(j, i); if len > max_chars { continue; }
            let slack = (max_chars - len) as f64; let pen = if n > 1 && i - j == 1 { 10_000.0 } else { 0.0 }; let c = dp[q - 1][j] + slack * slack + pen; if c < dp[q][i] { dp[q][i] = c; from[q][i] = j; } } } }
        if dp[k][n] < inf { let mut br = vec![]; let (mut i, mut q) = (n, k); while q > 0 { let j = from[q][i]; if j > 0 { br.push(j - 1); } i = j; q -= 1; } br.sort(); return Some(br); }
    }
    None
}
pub fn cue_text(words: &[&str], breaks: &[usize]) -> String { crate::transcript::compose_cue_text(words, breaks) }

pub fn group(words: &[W], p: &Params) -> Result<Vec<Cue>, String> {
    p.check()?; let n = words.len(); if n == 0 { return Err("transcript has no words on chosen takes".into()); }
    let txt = |a: usize, b: usize| -> Vec<&str> { words[a..=b].iter().map(|w| w.text.as_str()).collect() };
    let chars = |a: usize, b: usize| -> usize { txt(a, b).iter().map(|t| clen(t)).sum::<usize>() + (b - a) };
    let mut cuts: Vec<(usize, usize)> = vec![]; let mut i = 0;
    while i < n {
        let timeok = |a: usize, c: usize| -> bool { let d = if c + 1 < n { (words[c + 1].start - p.gap_s - words[a].start).min(p.max_dur) } else { p.max_dur }; d >= p.min_dur - 1e-9 && chars(a, c) as f64 / d.max(1e-9) <= p.max_cps };
        let mut m = i; let mut k = i; let mut found = false;
        while k < n { let dur = words[k].end - words[i].start; if dur > p.max_dur || wrap(&txt(i, k), p.max_lines, p.max_chars).is_none() { break; } if timeok(i, k) { m = k; found = true; } k += 1; }
        if !found { m = i; } // nothing fits the timing: a single word is kept alone (reported below)
        let valid = |c: usize| -> bool { let rem = n - 1 - c; rem == 0 || rem >= p.min_words || sentence_end(&words[n - 1].text) };
        let ok_len = |c: usize| (c - i + 1) >= p.min_words || sentence_end(&words[c].text) || c == n - 1;
        let eager = (i..=m).find(|&c| (timeok(i, c) || !found) && sentence_end(&words[c].text) && ok_len(c) && valid(c) && (words[c].end - words[i].start >= p.min_dur || c == m));
        let cut = if m == n - 1 && eager.is_none() { m } else if let Some(c) = eager { c } else {
            let full = chars(i, m) as f64; let mut best: Option<(u8, usize)> = None;
            for c in i..=m { if !ok_len(c) || !valid(c) || !timeok(i, c) || (chars(i, c) as f64) < 0.5 * full { continue; }
                let gap = if c + 1 < n { words[c + 1].start - words[c].end } else { 0.0 }; let rank = if sentence_end(&words[c].text) { 3 } else if clause_end(&words[c].text) { 2 } else if gap > p.pause_s { 1 } else { 0 };
                if best.map_or(true, |b| (rank, c) >= b) { best = Some((rank, c)); } }
            best.map_or(m, |b| b.1) };
        cuts.push((i, cut)); i = cut + 1;
    }
    let mut cues: Vec<Cue> = vec![];
    for &(a, b) in &cuts { let t = txt(a, b); let mut warnings = vec![]; let br = match wrap(&t, p.max_lines, p.max_chars) { Some(x) => x, None => { warnings.push("LINE_TOO_LONG".into()); vec![] } };
        cues.push(Cue { first: a, last: b, text: cue_text(&t, &br), breaks_after: br, start: words[a].start, end: words[b].end, warnings }); }
    for i in 0..cues.len() {
        let limit = if i + 1 < cues.len() { cues[i + 1].start - p.gap_s } else { f64::INFINITY }; let nat = cues[i].end; let ch = chars(cues[i].first, cues[i].last) as f64;
        let need = p.min_dur.max(ch / p.max_cps).min(p.max_dur); let mut end = nat.max((cues[i].start + need).min(limit)); end = end.min(limit);
        if end < cues[i].start + 0.02 { cues[i].warnings.push("CUE_TOO_SHORT".into()); end = cues[i].start + 0.02; }
        let dur = end - cues[i].start; if dur < p.min_dur - 1e-9 && !cues[i].warnings.contains(&"CUE_TOO_SHORT".to_string()) { cues[i].warnings.push("CUE_TOO_SHORT".into()); }
        if dur > p.max_dur + 1e-9 { cues[i].warnings.push("CUE_TOO_LONG".into()); } if ch / dur > p.max_cps + 1e-9 { cues[i].warnings.push("READING_SPEED".into()); }
        cues[i].end = end; }
    Ok(cues)
}
fn ts(t: f64, sep: char) -> String { let ms = (t.max(0.0) * 1000.0).round() as u64; format!("{:02}:{:02}:{:02}{}{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, sep, ms % 1000) }
/// Cue text made safe for a subtitle file: no "-->" and no blank line inside a cue (CR dropped, blank lines become a space).
fn safe(t: &str) -> String { let t = t.replace('\r', "").replace("-->", "->"); t.split('\n').map(|l| if l.trim().is_empty() { " ".to_string() } else { l.to_string() }).collect::<Vec<_>>().join("\n") }
fn vtt_escape(t: &str) -> String { t.replace('&', "&amp;").replace('<', "&lt;") }
/// (start, end, text) cues as SRT: UTF-8, LF line ends, no BOM, comma milliseconds. Times round to the millisecond.
pub fn to_srt(cues: &[(f64, f64, String)]) -> String { let mut o = String::new(); for (i, (a, b, t)) in cues.iter().enumerate() { o.push_str(&format!("{}\n{} --> {}\n{}\n\n", i + 1, ts(*a, ','), ts(*b, ','), safe(t))); } o }
/// WebVTT: as SRT with dot milliseconds, a WEBVTT header, and `&` / `<` escaped.
pub fn to_vtt(cues: &[(f64, f64, String)]) -> String { let mut o = String::from("WEBVTT\n\n"); for (a, b, t) in cues { o.push_str(&format!("{} --> {}\n{}\n\n", ts(*a, '.'), ts(*b, '.'), vtt_escape(&safe(t)))); } o }
/// Writes `body` to `path` through a temp file in the same directory and a rename. An existing file is an error `dest_exists` unless `overwrite`.
pub fn write_atomic(path: &std::path::Path, body: &str, overwrite: bool) -> Result<(), String> {
    if path.exists() && !overwrite { return Err(format!("dest_exists: {} already exists (pass overwrite:true to replace it)", path.display())); }
    let tmp = path.with_extension(format!("tmp{}", std::process::id())); std::fs::write(&tmp, body).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| { let _ = std::fs::remove_file(&tmp); format!("cannot write {}: {e}", path.display()) })
}
