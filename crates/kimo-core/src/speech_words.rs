//! Speech words for automatic captions: whisper.cpp `-ojf` JSON to words, word timing refined against the audio, creator-style
//! paging, keyword pick, Devanagari to casual Roman, and a caption band that keeps clear of a face. Pure functions: no Session,
//! no subprocess, no rasterizer.
//!
//! Timing. whisper.cpp's DTW token times (`t_dtw`) land late and spread: on a synthesized clip with exact word times they were
//! 248 ms late on average (max 481 ms). A word's start is estimated as the mean of (its first token's DTW time minus that bias)
//! and (the previous word's last token DTW time), then snapped to the strongest spectral-flux onset within +-80 ms, weighted
//! towards the estimate. Measured on the same clip: mean |error| 41 ms, median 31 ms, max 138 ms.
use serde_json::Value;
use std::collections::HashMap;

/// One recognized word before refinement. `t0`/`t1`: the decoder's own token timestamps (seconds); `dtw0`/`dtw_last`: DTW times
/// of the word's first and last token when the recognizer ran with `-dtw`; `p`: mean token probability.
#[derive(Clone, Debug, PartialEq)]
pub struct AsrWord { pub text: String, pub t0: f64, pub t1: f64, pub dtw0: Option<f64>, pub dtw_last: Option<f64>, pub p: f32 }
/// A word with refined times (seconds, relative to the audio that was recognized).
#[derive(Clone, Debug, PartialEq)]
pub struct TimedWord { pub text: String, pub start: f64, pub end: f64, pub conf: f32 }

/// Mean lateness of whisper.cpp DTW token times against true word starts, measured (see module docs).
pub const DTW_BIAS: f64 = 0.248;

fn special(t: &str) -> bool { let t = t.trim(); t.starts_with("[_") || (t.starts_with("<|") && t.ends_with("|>")) }
fn wordy(t: &str) -> bool { t.chars().any(|c| c.is_alphanumeric()) }

/// Parses whisper.cpp `--output-json-full` output into words (tokens joined; a token starting with a space starts a word;
/// punctuation-only tokens attach to the word before). Returns the words and the language the recognizer reported.
pub fn parse_whisper_cpp(bytes: &[u8]) -> Result<(Vec<AsrWord>, Option<String>), String> {
    let s = String::from_utf8_lossy(bytes);
    let v: Value = serde_json::from_str(&s).map_err(|e| format!("recognizer output is not JSON: {e}"))?;
    let lang = v.pointer("/result/language").and_then(|x| x.as_str()).map(String::from);
    let segs = v.get("transcription").and_then(|x| x.as_array()).ok_or("recognizer output has no transcription array")?;
    let mut out: Vec<AsrWord> = vec![]; let mut n: Vec<f32> = vec![];
    for seg in segs {
        let Some(toks) = seg.get("tokens").and_then(|x| x.as_array()) else { continue };
        let mut first = true;
        for tk in toks {
            let text = tk.get("text").and_then(|x| x.as_str()).unwrap_or("");
            if text.is_empty() || special(text) { continue; }
            let off = |k: &str| tk.get("offsets").and_then(|o| o.get(k)).and_then(|x| x.as_f64()).map(|ms| ms / 1000.0).unwrap_or(0.0);
            let dtw = tk.get("t_dtw").and_then(|x| x.as_f64()).filter(|d| *d >= 0.0).map(|cs| cs / 100.0);
            let p = tk.get("p").and_then(|x| x.as_f64()).unwrap_or(1.0) as f32;
            if (text.starts_with(' ') || first || out.is_empty()) && wordy(text) {
                out.push(AsrWord { text: text.trim().to_string(), t0: off("from"), t1: off("to"), dtw0: dtw, dtw_last: dtw, p }); n.push(1.0);
            } else if let (Some(w), Some(k)) = (out.last_mut(), n.last_mut()) {
                w.text.push_str(text.trim()); w.t1 = off("to").max(w.t1); if dtw.is_some() { w.dtw_last = dtw; } if w.dtw0.is_none() { w.dtw0 = dtw; }
                w.p = (w.p * *k + p) / (*k + 1.0); *k += 1.0;
            }
            first = false;
        }
    }
    out.retain(|w| wordy(&w.text));
    Ok((out, lang))
}

/// In-place iterative radix-2 FFT (re, im); `re.len()` must be a power of two.
fn fft(re: &mut [f32], im: &mut [f32]) {
    let n = re.len(); let mut j = 0usize;
    for i in 1..n { let mut bit = n >> 1; while j & bit != 0 { j ^= bit; bit >>= 1; } j |= bit; if i < j { re.swap(i, j); im.swap(i, j); } }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f32::consts::PI / len as f32; let (wr, wi) = (ang.cos(), ang.sin());
        let mut i = 0;
        while i < n { let (mut cr, mut ci) = (1.0f32, 0.0f32);
            for k in 0..len / 2 { let (a, b) = (i + k, i + k + len / 2); let (xr, xi) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - xr; im[b] = im[a] - xi; re[a] += xr; im[a] += xi; let t = cr * wr - ci * wi; ci = cr * wi + ci * wr; cr = t; }
            i += len; }
        len <<= 1;
    }
}

/// Onset strength per 5 ms hop: positive spectral flux of log magnitude over 100..6000 Hz (25 ms Hann window), and the frame
/// level in dBFS. `x`: mono samples in -1..1 at `sr` Hz.
pub struct Onsets { pub flux: Vec<f32>, pub db: Vec<f32>, pub hop: f64 }
pub fn onsets(x: &[f32], sr: u32) -> Onsets {
    let hop = (0.005 * sr as f64).round().max(1.0) as usize; let win = (0.025 * sr as f64).round().max(8.0) as usize; let nfft = win.next_power_of_two();
    let w: Vec<f32> = (0..win).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / (win - 1) as f32).cos()).collect();
    let (b0, b1) = (((100.0 * nfft as f64) / sr as f64).ceil() as usize, (((6000.0f64).min(sr as f64 / 2.0) * nfft as f64) / sr as f64).floor() as usize);
    let (mut flux, mut db) = (vec![], vec![]); let mut prev: Vec<f32> = vec![]; let mut i = 0;
    while i + win <= x.len() {
        let mut re = vec![0f32; nfft]; let mut im = vec![0f32; nfft]; let mut e = 0f32;
        for k in 0..win { re[k] = x[i + k] * w[k]; e += x[i + k] * x[i + k]; }
        fft(&mut re, &mut im);
        let l: Vec<f32> = (b0..=b1.min(nfft / 2)).map(|k| (1.0 + 1000.0 * (re[k] * re[k] + im[k] * im[k]).sqrt()).ln()).collect();
        let f = if prev.is_empty() { 0.0 } else { l.iter().zip(&prev).map(|(a, b)| (a - b).max(0.0)).sum::<f32>() };
        flux.push(f); db.push(10.0 * (e / win as f32 + 1e-12).log10()); prev = l; i += hop;
    }
    Onsets { flux, db, hop: hop as f64 / sr as f64 }
}

fn median(v: &[f32]) -> f32 { if v.is_empty() { return 0.0; } let mut s = v.to_vec(); s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); s[s.len() / 2] }

/// Refines word times against the audio (see module docs). Starts are strictly increasing (>= 60 ms apart); a word ends at the
/// next word's start, or where the level falls back to the floor before a pause (at least 80 ms after its start).
pub fn refine(words: &[AsrWord], o: &Onsets) -> Vec<TimedWord> {
    let n = words.len(); if n == 0 { return vec![]; }
    let est: Vec<f64> = (0..n).map(|i| {
        let w = &words[i];
        match w.dtw0 { Some(d) => { let prev = if i == 0 { d - 0.2 } else { words[i - 1].dtw_last.unwrap_or(d - 0.2) }; 0.5 * (prev + d - DTW_BIAS) + 0.02 } None => w.t0 }
    }).collect();
    let med = median(&o.flux); let mut st: Vec<f64> = Vec::with_capacity(n);
    for i in 0..n {
        let e = est[i]; let mut lo = (e - 0.08).max(0.0); let mut hi = e + 0.08;
        if let Some(p) = st.last() { lo = lo.max(p + 0.06); } if i + 1 < n { hi = hi.min(est[i + 1] - 0.03); }
        let mut s = e.max(lo);
        if hi > lo && !o.flux.is_empty() {
            let (a, b) = ((lo / o.hop).floor() as usize, ((hi / o.hop).ceil() as usize + 1).min(o.flux.len()));
            let mut best: Option<(f64, usize)> = None;
            for k in a..b { let t = k as f64 * o.hop; let sc = o.flux[k] as f64 * (-0.5 * ((t - e) / 0.1).powi(2)).exp(); if best.map_or(true, |(bs, _)| sc > bs) { best = Some((sc, k)); } }
            if let Some((_, k)) = best { if o.flux[k] > 1.5 * med { s = k as f64 * o.hop; } }
        }
        if let Some(p) = st.last() { s = s.max(p + 0.06); }
        st.push(s.max(0.0));
    }
    let floor = { let mut d = o.db.clone(); d.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); let p10 = d.get(d.len() / 10).copied().unwrap_or(-90.0); let mx = d.last().copied().unwrap_or(0.0); (p10 + 10.0).max(mx - 40.0) };
    (0..n).map(|i| {
        let s = st[i]; let limit = if i + 1 < n { st[i + 1] } else { s + 1.5f64.max(words[i].t1 - words[i].t0 + 0.3) };
        let (a, b) = ((s / o.hop) as usize, ((limit / o.hop) as usize).min(o.db.len()));
        let last = (a..b).rev().find(|&k| o.db[k] > floor).map(|k| (k + 1) as f64 * o.hop);
        let mut e = match last { Some(t) if i + 1 < n && limit - t < 0.12 => limit, Some(t) => t, None => limit.min(s + 0.3) };
        e = e.max(s + 0.08); if i + 1 < n { e = e.min(st[i + 1]); }
        TimedWord { text: words[i].text.clone(), start: s, end: e, conf: words[i].p }
    }).collect()
}

/// A caption page: words `first..=last`, a line break after each index (relative to `first`) in `breaks_after`, shown
/// `start..end` (seconds, same clock as the words).
#[derive(Clone, Debug, PartialEq)]
pub struct Page { pub first: usize, pub last: usize, pub breaks_after: Vec<usize>, pub start: f64, pub end: f64 }

fn core(t: &str) -> &str { t.trim_end_matches(|c: char| matches!(c, '"' | '\'' | ')' | ']' | '”' | '’')) }
fn sentence_end(t: &str) -> bool { core(t).chars().last().map_or(false, |c| matches!(c, '.' | '?' | '!' | '…' | '।')) }
fn clause_end(t: &str) -> bool { core(t).chars().last().map_or(false, |c| matches!(c, ',' | ';' | ':' | '—' | '–')) }

/// Creator-style pages: at most `per_line` words per line and `lines` lines, at most `max_chars` characters per line (one short
/// sentence-final word may ride along rather than stand alone). A page ends at a sentence end, at a clause end once it holds two words, before a pause over 0.45 s, or when full. A page shows from
/// its first word's start until the next page starts (when that is within 0.6 s) or 0.35 s after its last word.
pub fn paginate(w: &[TimedWord], per_line: usize, lines: usize, max_chars: usize) -> Vec<Page> {
    let (per_line, lines) = (per_line.max(1), lines.clamp(1, 3)); let cap = per_line * lines; let clen = |s: &str| s.chars().count();
    let mut cuts: Vec<(usize, usize)> = vec![]; let mut a = 0;
    for i in 0..w.len() {
        let len = i + 1 - a; let chars: usize = w[a..=i].iter().map(|x| clen(&x.text) + 1).sum();
        let next_gap = if i + 1 < w.len() { w[i + 1].start - w[i].end } else { 0.0 };
        let next_chars = if i + 1 < w.len() { chars + clen(&w[i + 1].text) + 1 } else { 0 };
        // a short word that ends the sentence may ride on a full page instead of standing alone on the next one
        let tail = i + 1 < w.len() && len == cap && sentence_end(&w[i + 1].text) && clen(&w[i + 1].text) <= 5 && next_gap <= 0.3 && next_chars <= lines * (max_chars + 1) + 6;
        let end = i + 1 == w.len() || (len >= cap && !tail) || sentence_end(&w[i].text) || (clause_end(&w[i].text) && len >= 2) || next_gap > 0.45 || (next_chars > lines * (max_chars + 1) && !tail);
        if end { cuts.push((a, i)); a = i + 1; }
    }
    // a lone trailing word joins the page before it when that page has room and no sentence ends between them
    if cuts.len() >= 2 { let (pa, pb) = cuts[cuts.len() - 2]; let (la, lb) = cuts[cuts.len() - 1];
        if la == lb && pb + 1 - pa < cap && !sentence_end(&w[pb].text) && w[la].start - w[pb].end <= 0.45 { cuts.pop(); let l = cuts.len(); cuts[l - 1] = (pa, lb); } }
    let mut pages: Vec<Page> = cuts.iter().map(|&(a, b)| {
        let words: Vec<&str> = w[a..=b].iter().map(|x| x.text.as_str()).collect();
        let breaks = if lines > 1 && words.len() > per_line { balanced(&words, lines, per_line) } else { vec![] };
        Page { first: a, last: b, breaks_after: breaks, start: w[a].start, end: w[b].end }
    }).collect();
    for k in 0..pages.len() {
        let last_end = w[pages[k].last].end; let next = pages.get(k + 1).map(|p| p.start);
        pages[k].end = match next { Some(ns) if ns - last_end <= 0.6 => ns, Some(ns) => (last_end + 0.35).min(ns), None => last_end + 0.35 };
        if pages[k].end - pages[k].start < 0.2 { pages[k].end = match next { Some(ns) => (pages[k].start + 0.2).min(ns), None => pages[k].start + 0.2 }; }
    }
    pages
}
/// Line breaks for `words` over `lines` lines of at most `per_line` words, balancing character counts.
fn balanced(words: &[&str], lines: usize, per_line: usize) -> Vec<usize> {
    let n = words.len(); let l: Vec<usize> = words.iter().map(|w| w.chars().count()).collect();
    if lines == 2 || n <= 2 * per_line {
        let mut best = (usize::MAX, 0usize);
        for b in 0..n.saturating_sub(1) { let (x, y) = (b + 1, n - b - 1); if x > per_line || y > per_line { continue; } let c1: usize = l[..=b].iter().sum::<usize>() + b; let c2: usize = l[b + 1..].iter().sum::<usize>() + (y - 1); let d = c1.abs_diff(c2); if d < best.0 { best = (d, b); } }
        return if best.0 == usize::MAX { vec![(n / 2).saturating_sub(1)] } else { vec![best.1] };
    }
    (1..lines).map(|k| (k * n / lines).saturating_sub(1)).collect()
}

const STOP: &[&str] = &["the","a","an","and","or","but","so","to","of","in","on","at","for","with","is","are","was","were","be","been","it","its","it's","this","that","these","those","i","i'm","you","your","you're","we","they","he","she","my","me","our","their","his","her","them","as","by","from","if","then","than","just","do","does","did","have","has","had","not","no","yes","can","will","would","could","should","get","got","what","when","how","who","all","very","really","there","here","about","into","out","up","down","over","like","okay","ok","yeah","um","uh","also","because","which","one","more","some","any","been","being","am","let's","don't","can't","even","only","every","before","after",
    "hai","hain","ho","hoon","hun","main","mai","mein","me","ye","yeh","woh","vo","wo","ki","ka","ke","ko","se","par","pe","aur","toh","to","bhi","na","nahi","nahin","kya","kyun","kyunki","jo","jab","tab","ab","ek","is","us","iska","uska","isko","usko","ismein","isme","mera","meri","mere","tera","teri","apna","apni","aap","tum","hum","haan","han","bas","jaise","waise","raha","rahi","rahe","tha","thi","the","hota","hoti","hote","kar","karo","karna","diya","liya","wala","wali","yaar","bhai","matlab","lekin","phir","fir","abhi","sab","kuch"];

/// Picks up to `max` emphasis words on a page: not a stopword, at least 4 letters (or any number), scored by length (capped at
/// 10), +4 for digits, +2 for a word that appears once in the whole transcript, +1 for a capital not at the page start. Returns
/// indices into `words`, best first.
pub fn keywords(words: &[&str], max: usize, freq: &HashMap<String, usize>) -> Vec<usize> {
    let mut sc: Vec<(i32, usize)> = vec![];
    for (i, w) in words.iter().enumerate() {
        let k = norm_word(w); let letters = k.chars().filter(|c| c.is_alphabetic()).count(); let digits = k.chars().any(|c| c.is_ascii_digit());
        if k.is_empty() || STOP.contains(&k.as_str()) || (letters < 4 && !digits) { continue; }
        let mut s = letters.min(10) as i32; if digits { s += 4; } if freq.get(&k).copied().unwrap_or(0) <= 1 { s += 2; }
        if i > 0 && w.chars().next().map_or(false, |c| c.is_uppercase()) { s += 1; }
        if s >= 5 { sc.push((s, i)); }
    }
    sc.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1))); sc.truncate(max); sc.into_iter().map(|x| x.1).collect()
}
/// Lowercase word without surrounding punctuation (apostrophes kept).
pub fn norm_word(w: &str) -> String { w.trim_matches(|c: char| !c.is_alphanumeric() && c != '\'').to_lowercase() }

/// True when `s` holds Devanagari letters.
pub fn has_devanagari(s: &str) -> bool { s.chars().any(|c| ('\u{0900}'..='\u{097F}').contains(&c)) }

fn dev_word(w: &str) -> Option<&'static str> {
    Some(match w { "है" => "hai", "हैं" => "hain", "में" => "mein", "मैं" => "main", "नहीं" => "nahi", "यह" => "yeh", "ये" => "ye", "वह" => "woh", "वो" => "vo",
        "क्या" => "kya", "क्यों" => "kyun", "क्योंकि" => "kyunki", "हूँ" | "हूं" => "hoon", "हो" => "ho", "तो" => "toh", "भी" => "bhi", "और" => "aur", "पे" => "pe", "पर" => "par",
        "के" => "ke", "की" => "ki", "का" => "ka", "को" => "ko", "से" => "se", "एक" => "ek", "बहुत" => "bahut", "लेकिन" => "lekin", "अच्छा" => "accha", "अच्छी" => "acchi",
        "थोड़ा" => "thoda", "थोड़ी" => "thodi", "आप" => "aap", "हम" => "hum", "हाँ" | "हां" => "haan", "यहाँ" | "यहां" => "yahan", "वहाँ" | "वहां" => "wahan", "ठीक" => "theek", _ => return None })
}
/// Devanagari to casual Roman (the spelling Hinglish captions use): common words from a table, otherwise letter by letter with
/// Hindi schwa deletion (no inherent "a" at a word end or in a vowel-consonant-[a]-consonant-vowel position). Non-Devanagari
/// text passes through unchanged.
pub fn romanize(s: &str) -> String {
    let mut out = String::new(); let mut word = String::new();
    let flush = |w: &mut String, out: &mut String| { if !w.is_empty() { out.push_str(&rom_word(w)); w.clear(); } };
    for c in s.chars() { if ('\u{0900}'..='\u{097F}').contains(&c) && c != '।' && c != '॥' { word.push(c); } else { flush(&mut word, &mut out); out.push(if c == '।' { '.' } else { c }); } }
    flush(&mut word, &mut out); out
}
fn rom_word(w: &str) -> String {
    if let Some(r) = dev_word(w) { return r.into(); }
    let cons = |c: char| -> Option<&'static str> { Some(match c { 'क' => "k", 'ख' => "kh", 'ग' => "g", 'घ' => "gh", 'ङ' => "n", 'च' => "ch", 'छ' => "chh", 'ज' => "j", 'झ' => "jh", 'ञ' => "n",
        'ट' => "t", 'ठ' => "th", 'ड' => "d", 'ढ' => "dh", 'ण' => "n", 'त' => "t", 'थ' => "th", 'द' => "d", 'ध' => "dh", 'न' => "n", 'प' => "p", 'फ' => "ph", 'ब' => "b", 'भ' => "bh", 'म' => "m",
        'य' => "y", 'र' => "r", 'ल' => "l", 'व' => "v", 'श' => "sh", 'ष' => "sh", 'स' => "s", 'ह' => "h", '\u{0958}' => "q", '\u{0959}' => "kh", '\u{095A}' => "g", '\u{095B}' => "z", '\u{095C}' => "r", '\u{095D}' => "rh", '\u{095E}' => "f", '\u{095F}' => "y", _ => return None }) };
    let vowel = |c: char| -> Option<&'static str> { Some(match c { 'अ' => "a", 'आ' => "aa", 'इ' => "i", 'ई' => "ee", 'उ' => "u", 'ऊ' => "oo", 'ए' => "e", 'ऐ' => "ai", 'ओ' => "o", 'औ' => "au", 'ऋ' => "ri", 'ऑ' => "o", 'ऍ' => "e", _ => return None }) };
    let matra = |c: char| -> Option<&'static str> { Some(match c { 'ा' => "a", 'ि' => "i", 'ी' => "i", 'ु' => "u", 'ू' => "u", 'े' => "e", 'ै' => "ai", 'ो' => "o", 'ौ' => "au", 'ृ' => "ri", 'ॉ' => "o", 'ॅ' => "e", _ => return None }) };
    // syllables: (consonant roman, vowel roman or None = inherent a, explicit virama)
    let cs: Vec<char> = w.chars().collect(); let mut syl: Vec<(String, Option<String>, bool)> = vec![]; let mut i = 0;
    while i < cs.len() {
        let mut c = cs[i];
        if i + 1 < cs.len() && cs[i + 1] == '\u{093C}' { c = match c { 'क' => '\u{0958}', 'ख' => '\u{0959}', 'ग' => '\u{095A}', 'ज' => '\u{095B}', 'ड' => '\u{095C}', 'ढ' => '\u{095D}', 'फ' => '\u{095E}', 'य' => '\u{095F}', o => o }; i += 1; }
        if let Some(r) = cons(c) {
            let (mut v, mut vir) = (None, false);
            if i + 1 < cs.len() { if let Some(m) = matra(cs[i + 1]) { v = Some(m.to_string()); i += 1; } else if cs[i + 1] == '\u{094D}' { vir = true; i += 1; } }
            syl.push((r.into(), v, vir));
        } else if let Some(r) = vowel(c) { syl.push((String::new(), Some(r.into()), false)); }
        else if c == 'ं' || c == 'ँ' { if let Some(l) = syl.last_mut() { if l.1.is_none() && !l.2 { l.1 = Some("a".into()); } l.0.push('\u{0}'); } }
        else if c == 'ः' { if let Some(l) = syl.last_mut() { l.0.push('\u{1}'); } }
        i += 1;
    }
    let n = syl.len(); let mut out = String::new();
    for k in 0..n {
        let (c, v, vir) = &syl[k]; let nas = c.contains('\u{0}'); let vis = c.contains('\u{1}'); let c: String = c.chars().filter(|ch| *ch != '\u{0}' && *ch != '\u{1}').collect();
        out.push_str(&c);
        let vowel_here = match v { Some(v) => Some(v.clone()), None if *vir || c.is_empty() => None, None => {
            // inherent a: dropped at the word end, and between a vowel and a consonant+vowel (schwa deletion); kept in the first syllable
            let last = k + 1 == n; let prev_vowel = k > 0 && !syl[k - 1].2; let next_cv = k + 1 < n && !syl[k + 1].0.is_empty() && !syl[k + 1].2 && (syl[k + 1].1.is_some() || k + 2 < n);
            if last || (k > 0 && prev_vowel && next_cv) { None } else { Some("a".into()) } } };
        if let Some(v) = vowel_here { out.push_str(&v); }
        if nas { let nx = syl.get(k + 1).map(|s| s.0.chars().next()).flatten(); out.push(if matches!(nx, Some('p') | Some('b') | Some('m')) { 'm' } else { 'n' }); }
        if vis { out.push('h'); }
    }
    out
}

/// Hinglish has no fixed spelling and the recognizer writes one word several ways (bahut / bohat / bohot). One spelling per word,
/// the one the Roman reference for Naman's clips uses, whole words only. Measured on those six clips (153 words, small model, full
/// tool path): 47.1% -> 39.2% WER strict, 38.6% -> 35.3% ignoring spelling. The table was written after reading those clips'
/// output and scored on the same words, so expect less on new speech. Left out on purpose: pita (also "father", पिता) and nai
/// (also "new", नई).
const HINGLISH_SPELLING: &[(&str, &str)] = &[
    ("acha", "accha"), ("achcha", "accha"), ("achchi", "acchi"), ("achha", "accha"), ("achhi", "acchi"), ("achi", "acchi"), ("aisey", "aise"), ("badeya", "badhiya"),
    ("badhia", "badhiya"), ("badiya", "badhiya"), ("bahot", "bohot"), ("bahut", "bohot"), ("barhiya", "badhiya"), ("bohat", "bohot"), ("bohut", "bohot"),
    ("cheej", "cheez"), ("cheezain", "cheezein"), ("cheeze", "cheezein"), ("cheezen", "cheezein"), ("chij", "cheez"), ("chiz", "cheez"), ("chize", "cheezein"),
    ("ekdam", "ekdum"), ("ese", "aise"), ("haa", "haan"), ("hamesa", "hamesha"), ("hameshe", "hamesha"), ("han", "haan"), ("hoo", "hoon"), ("hu", "hoon"),
    ("humesha", "hamesha"), ("hun", "hoon"), ("ismein", "isme"), ("ismen", "isme"), ("jyaada", "zyada"), ("jyada", "zyada"), ("kaisey", "kaise"), ("kese", "kaise"),
    ("kuchch", "kuch"), ("kuchh", "kuch"), ("kyoki", "kyunki"), ("kyonke", "kyunki"), ("kyonki", "kyunki"), ("kyuke", "kyunki"), ("kyuki", "kyunki"),
    ("kyunke", "kyunki"), ("lekhin", "lekin"), ("mese", "isme"), ("mise", "isme"), ("nahee", "nahi"), ("nahin", "nahi"), ("pani", "paani"), ("peetay", "peete"),
    ("pite", "peete"), ("rahta", "rehta"), ("theekh", "theek"), ("thik", "theek"), ("usmein", "usme"), ("usmen", "usme"), ("voh", "woh"), ("waha", "wahan"),
    ("wahaan", "wahan"), ("wo", "woh"), ("yah", "ye"), ("yaha", "yahan"), ("yahaan", "yahan"), ("yeh", "ye"), ("ziada", "zyada"), ("zyaada", "zyada"),
];
/// Two recognized words written as one in Hinglish: (first, second, joined).
const HINGLISH_JOIN: &[(&str, &str, &str)] = &[("ek", "dum", "ekdum"), ("ek", "dam", "ekdum")];

/// `w` split into leading punctuation, the word itself (letters, digits, apostrophes) and trailing punctuation.
fn split_word(w: &str) -> (&str, &str, &str) {
    let inner = |c: char| c.is_alphanumeric() || c == '\'' || c == '\u{2019}';
    let a = w.find(|c: char| inner(c)).unwrap_or(w.len());
    let b = w.rfind(|c: char| inner(c)).map_or(a, |i| i + w[i..].chars().next().map_or(0, |c| c.len_utf8()));
    (&w[..a], &w[a..b.max(a)], &w[b.max(a)..])
}
/// `rep` cased like `src`: ALL CAPS stays all caps, a capital first letter stays capital, otherwise lower case.
fn case_like(src: &str, rep: &str) -> String {
    let letters: Vec<char> = src.chars().filter(|c| c.is_alphabetic()).collect();
    if letters.len() > 1 && letters.iter().all(|c| c.is_uppercase()) { return rep.to_uppercase(); }
    if src.chars().next().map_or(false, |c| c.is_uppercase()) { let mut c = rep.chars(); return c.next().map_or(String::new(), |f| f.to_uppercase().chain(c).collect()); }
    rep.to_string()
}
/// Respells common Hinglish variants to one spelling (whole words, case and punctuation kept) and joins split pairs (ek dum ->
/// ekdum, spanning both words' times). For Roman Hinglish output only; English text must not go through it. Returns the words
/// and how many were changed.
pub fn normalize_hinglish(words: &[TimedWord]) -> (Vec<TimedWord>, usize) {
    let mut out: Vec<TimedWord> = Vec::with_capacity(words.len()); let mut n = 0;
    for w in words {
        let (pre, core, post) = split_word(&w.text); let lc = core.to_lowercase();
        if pre.is_empty() && !core.is_empty() {
            if let Some(prev) = out.last_mut() {
                let (pp, pc, ps) = split_word(&prev.text); let plc = pc.to_lowercase();
                if ps.is_empty() { if let Some(j) = HINGLISH_JOIN.iter().find(|j| j.0 == plc && j.1 == lc) {
                    prev.text = format!("{pp}{}{post}", case_like(pc, j.2)); prev.end = prev.end.max(w.end); prev.conf = prev.conf.min(w.conf); n += 1; continue; } }
            }
        }
        let mut t = w.clone();
        if let Some(&(_, to)) = HINGLISH_SPELLING.iter().find(|p| p.0 == lc) { t.text = format!("{pre}{}{post}", case_like(core, to)); n += 1; }
        out.push(t);
    }
    (out, n)
}

/// Vertical extent (fractions of the height, top to bottom) of the topmost skin-toned band in the centre 60% of an RGB frame,
/// or None. Skin is the YCbCr box Cb 77..127, Cr 133..173 with Y > 40; a band is at least 4% of the height of rows that are at
/// least 12% skin. A cheap face proxy for placing captions; it does not recognize faces.
pub fn skin_band(rgb: &[u8], w: usize, h: usize) -> Option<(f64, f64)> {
    if w == 0 || h == 0 || rgb.len() < w * h * 3 { return None; }
    let (x0, x1) = (w / 5, w - w / 5);
    let rows: Vec<bool> = (0..h).map(|y| { let mut k = 0; for x in x0..x1 { let p = &rgb[(y * w + x) * 3..(y * w + x) * 3 + 3]; let (r, g, b) = (p[0] as f32, p[1] as f32, p[2] as f32);
        let yy = 0.299 * r + 0.587 * g + 0.114 * b; let cb = 128.0 - 0.168736 * r - 0.331264 * g + 0.5 * b; let cr = 128.0 + 0.5 * r - 0.418688 * g - 0.081312 * b;
        if yy > 40.0 && (77.0..=127.0).contains(&cb) && (133.0..=173.0).contains(&cr) { k += 1; } } k as f64 >= 0.12 * (x1 - x0) as f64 }).collect();
    let min = ((h as f64) * 0.04).ceil() as usize; let mut y = 0;
    while y < h { if rows[y] { let mut e = y; let mut gap = 0; let mut k = y; while k < h && gap <= 2 { if rows[k] { e = k; gap = 0; } else { gap += 1; } k += 1; }
        if e + 1 - y >= min { return Some((y as f64 / h as f64, (e + 1) as f64 / h as f64)); } y = k; } else { y += 1; } }
    None
}
/// Caption centre as a fraction of the frame height. `position`: bottom | middle | top | auto. auto: the lower band (0.70 of the
/// height on a tall frame, 0.80 on a wide one, clear of the strip apps cover with their own buttons), moved to the upper band
/// (0.20) when a face band reaches into it and the top is clear. Returns (centre fraction, chosen band).
pub fn caption_band(tall: bool, faces: &[(f64, f64)], position: &str) -> (f64, &'static str) {
    let low = if tall { 0.70 } else { 0.80 };
    match position { "bottom" => (low, "bottom"), "middle" => (0.5, "middle"), "top" => (0.2, "top"), _ => {
        let reach = faces.iter().any(|&(_, b)| b > low - 0.08); let top_busy = faces.iter().any(|&(t, _)| t < 0.30);
        if reach && !top_busy { (0.2, "top") } else { (low, "bottom") } } }
}
