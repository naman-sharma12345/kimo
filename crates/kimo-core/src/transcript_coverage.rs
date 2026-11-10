//! Script coverage (RV-8 step 4). Pure: reference script text + transcript words in script order -> what is covered, missing, repeated or extra.
//!
//! Algorithm (also stated in the tool description): both sides are normalised to tokens (lowercase, everything except letters and digits removed, empty
//! tokens dropped). The two token sequences are aligned with a global edit-distance dynamic program (match 0, near-match 0.5, substitution 1, gap 1;
//! a near-match is a pair of tokens of >= 4 characters with Levenshtein distance 1). The alignment is traced back deterministically (ties prefer
//! match/substitute, then script gap, then extra word). Script tokens aligned to a word (match or near-match) are covered. Script tokens with no partner
//! form missing passages; transcript words with no partner are extra. Repeats are 3-token sequences present at two places in the transcript.
//! It never invents text: every reported string is a slice of the supplied script or an existing word id.
use serde::Serialize;

pub const MAX_CELLS: usize = 16_000_000;
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Missing { /// Inclusive token indices into the script.
    pub script_range: (usize, usize), pub text: String, /// Ids of the transcript words just before and after the gap.
    pub nearest_words: Vec<String> }
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Repeated { pub words: Vec<String>, pub takes: Vec<String> }
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Coverage { pub coverage_pct: f64, pub script_tokens: usize, pub covered_tokens: usize, pub missing_passages: Vec<Missing>, pub repeated_passages: Vec<Repeated>, pub extra_words: Vec<String> }

pub fn norm(s: &str) -> String { s.chars().filter(|c| c.is_alphanumeric()).flat_map(|c| c.to_lowercase()).collect() }
fn lev1(a: &[char], b: &[char]) -> bool { // Levenshtein distance exactly <= 1
    if a == b { return true; } let (la, lb) = (a.len(), b.len()); if la.abs_diff(lb) > 1 { return false; }
    let (mut i, mut j, mut d) = (0, 0, 0); while i < la && j < lb { if a[i] == b[j] { i += 1; j += 1; continue; } d += 1; if d > 1 { return false; } if la > lb { i += 1; } else if lb > la { j += 1; } else { i += 1; j += 1; } } d + (la - i) + (lb - j) <= 1
}
fn cost(a: &str, b: &str) -> f64 { if a == b { 0.0 } else { let (x, y): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect()); if x.len() >= 4 && y.len() >= 4 && lev1(&x, &y) { 0.5 } else { 1.0 } } }

/// `script` as supplied; `words`: (word id, text, take id) in script order.
pub fn coverage(script: &str, words: &[(String, String, String)]) -> Result<Coverage, String> {
    let raw: Vec<&str> = script.split_whitespace().collect(); let st: Vec<(usize, String)> = raw.iter().enumerate().map(|(i, t)| (i, norm(t))).filter(|(_, n)| !n.is_empty()).collect();
    let wt: Vec<(usize, String)> = words.iter().enumerate().map(|(i, w)| (i, norm(&w.1))).filter(|(_, n)| !n.is_empty()).collect();
    let (n, m) = (st.len(), wt.len()); if n == 0 { return Err("script has no words".into()); } if (n + 1).saturating_mul(m + 1) > MAX_CELLS { return Err(format!("script x transcript is too large to align ({n} x {m} tokens)")); }
    let w = m + 1; let mut d = vec![0f64; (n + 1) * w]; for i in 1..=n { d[i * w] = i as f64; } for j in 1..=m { d[j] = j as f64; }
    for i in 1..=n { for j in 1..=m { let sub = d[(i - 1) * w + j - 1] + cost(&st[i - 1].1, &wt[j - 1].1); d[i * w + j] = sub.min(d[(i - 1) * w + j] + 1.0).min(d[i * w + j - 1] + 1.0); } }
    // trace back: partner[i] = Some(j) when script token i is covered
    let (mut i, mut j) = (n, m); let mut partner: Vec<Option<usize>> = vec![None; n]; let mut extra: Vec<usize> = vec![]; let eps = 1e-9;
    while i > 0 || j > 0 {
        if i > 0 && j > 0 { let c = cost(&st[i - 1].1, &wt[j - 1].1); if (d[i * w + j] - (d[(i - 1) * w + j - 1] + c)).abs() < eps { if c < 1.0 { partner[i - 1] = Some(j - 1); } else { extra.push(j - 1); } i -= 1; j -= 1; continue; } }
        if i > 0 && (d[i * w + j] - (d[(i - 1) * w + j] + 1.0)).abs() < eps { i -= 1; continue; }
        extra.push(j - 1); j -= 1;
    }
    extra.sort(); let covered = partner.iter().filter(|p| p.is_some()).count();
    let mut missing = vec![]; let mut k = 0;
    while k < n { if partner[k].is_some() { k += 1; continue; } let s = k; while k < n && partner[k].is_none() { k += 1; } let e = k - 1;
        let before = (0..s).rev().find_map(|x| partner[x]).map(|j| words[wt[j].0].0.clone()); let after = (k..n).find_map(|x| partner[x]).map(|j| words[wt[j].0].0.clone());
        missing.push(Missing { script_range: (s, e), text: (st[s].0..=st[e].0).map(|x| raw[x]).collect::<Vec<_>>().join(" "), nearest_words: before.into_iter().chain(after).collect() }); }
    // repeats: identical 3-token windows at two different places in the transcript
    let mut seen: std::collections::BTreeMap<Vec<&str>, usize> = Default::default(); let mut rep: Vec<(usize, usize)> = vec![];
    for a in 0..m.saturating_sub(2) { let key: Vec<&str> = wt[a..a + 3].iter().map(|x| x.1.as_str()).collect(); if let Some(&p) = seen.get(&key) { if p + 3 <= a { rep.push((a, p)); } } else { seen.insert(key, a); } }
    let mut repeated: Vec<Repeated> = vec![]; let mut x = 0;
    while x < rep.len() { let mut y = x; while y + 1 < rep.len() && rep[y + 1].0 == rep[y].0 + 1 { y += 1; } let (a0, a1) = (rep[x].0, rep[y].0 + 2); let ids: Vec<String> = (a0..=a1).map(|q| words[wt[q].0].0.clone()).collect();
        let mut tk: Vec<String> = (a0..=a1).map(|q| words[wt[q].0].2.clone()).chain((rep[x].1..=rep[y].1 + 2).map(|q| words[wt[q].0].2.clone())).collect(); tk.sort(); tk.dedup(); repeated.push(Repeated { words: ids, takes: tk }); x = y + 1; }
    Ok(Coverage { coverage_pct: (covered as f64 * 1000.0 / n as f64).round() / 10.0, script_tokens: n, covered_tokens: covered, missing_passages: missing, repeated_passages: repeated, extra_words: extra.iter().map(|j| words[wt[*j].0].0.clone()).collect() })
}

/// Timeline-side alignment of the chosen takes (RV-8 steering): how much of each take's timeline span is covered by words, where the silences are,
/// and whether any stored word time has drifted from the take formula. Pure and lightweight: one pass over the words, no text is returned.
pub fn timeline_alignment(tr: &crate::transcript::Transcript, min_gap: f64) -> serde_json::Value {
    use serde_json::json;
    let starts = tr.take_starts(); let mut takes = vec![]; let (mut span_tot, mut speech_tot, mut drift_max, mut prev_end, mut prev_id): (f64, f64, f64, f64, Option<String>) = (0.0, 0.0, 0.0, f64::NEG_INFINITY, None);
    let mut issues: Vec<serde_json::Value> = vec![];
    for t in tr.chosen_takes() {
        let s = starts.iter().find(|x| x.0 == t.id).map(|x| x.1).unwrap_or(0.0); let e = s + t.timeline_len();
        let mut ws: Vec<&crate::transcript::Word> = tr.words.iter().filter(|w| w.take_id == t.id).collect(); ws.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
        let (mut speech, mut cur, mut outside) = (0.0, s, 0usize); let mut sil = vec![];
        for w in &ws {
            let d = (w.start - t.to_timeline(s, w.src_start)).abs().max((w.end - t.to_timeline(s, w.src_end)).abs()); if d > drift_max { drift_max = d; }
            if w.src_start < t.src_start - 1e-6 || w.src_end > t.src_end + 1e-6 { outside += 1; }
            let (a, b) = (w.start.max(s), w.end.min(e)); if b > a { speech += b - a; }
            if w.start - cur >= min_gap { sil.push(json!([cur, w.start])); } if w.end > cur { cur = w.end; }
        }
        if e - cur >= min_gap { sil.push(json!([cur, e])); }
        if s < 0.0 { issues.push(json!({"code": "NEGATIVE_OFFSET", "take": t.id, "timeline_start": s, "note": "part of the take lies before timeline 0"})); }
        if s < prev_end - 1e-6 { issues.push(json!({"code": "TAKE_OVERLAP", "take": t.id, "with": prev_id, "overlap_s": prev_end - s})); }
        if outside > 0 { issues.push(json!({"code": "WORDS_OUTSIDE_TAKE", "take": t.id, "count": outside, "note": "words lie outside the take's source range (take shorter than the words)"})); }
        let span = (e - s).max(0.0); span_tot += span; speech_tot += speech; if e > prev_end { prev_end = e; } prev_id = Some(t.id.clone());
        takes.push(json!({"take_id": t.id, "timeline_start": s, "timeline_end": e, "speed": t.speed, "words": ws.len(), "speech_s": speech, "aligned_pct": if span > 0.0 { (speech / span * 1000.0).round() / 10.0 } else { 0.0 }, "silence_intervals": sil}));
    }
    json!({"timeline_span_s": span_tot, "speech_s": speech_tot, "aligned_pct": if span_tot > 0.0 { (speech_tot / span_tot * 1000.0).round() / 10.0 } else { 0.0 }, "max_drift_ms": (drift_max * 1e6).round() / 1e3, "min_gap_s": min_gap, "takes": takes, "issues": issues})
}
