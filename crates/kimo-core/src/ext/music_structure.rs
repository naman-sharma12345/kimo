//! music_structure: the bar grid, downbeats, sections and drops of a music track, so a cut can land on the 1 of a bar
//! and a drop instead of on any onset.
//!
//! The beat grid comes from motion::detect_beats (tempo by autocorrelation, phase-locked grid). On top of it:
//! - downbeats: of the `beats_per_bar` possible bar phases, the one whose beats carry the most low-frequency onset
//!   energy (the kick usually lands on the 1);
//! - per-bar features: loudness (RMS), low-band share and onset density;
//! - sections: a novelty curve over 4-bar blocks; boundaries where it peaks, on 4-bar multiples, at least 4 bars apart;
//! - labels from each section's energy relative to the track (low = intro / break / outro by position, high = drop or
//!   chorus, in between = verse, or build when the next section is high);
//! - drops: a high section that starts at least 1.6x louder than the section before it.
use super::kit::*;

const SR: f64 = 22050.0;

fn decode(path: &str, t0: f64, dur: f64) -> Result<Vec<f32>, String> {
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-v", "error", "-ss", &format!("{t0}"), "-t", &format!("{dur}"), "-i", path, "-vn", "-ac", "1", "-ar", "22050", "-f", "f32le", "-"]);
    let max = ((dur.max(1.0) * SR) as usize + 22050) * 4;
    let raw = crate::color::process::decode_capture(&mut cmd, max, std::time::Duration::from_secs(120))?;
    Ok(raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect())
}

/// Per-sample low band (one-pole low-pass at ~150 Hz): where the kick and bass live.
fn lowpass(x: &[f32]) -> Vec<f32> {
    let a = (-2.0 * std::f64::consts::PI * 150.0 / SR).exp() as f32; let mut y = 0f32;
    x.iter().map(|&v| { y = a * y + (1.0 - a) * v; y }).collect()
}
fn rms(x: &[f32]) -> f64 { if x.is_empty() { 0.0 } else { (x.iter().map(|v| (*v as f64) * (*v as f64)).sum::<f64>() / x.len() as f64).sqrt() } }
/// Onset strength around time t (seconds, relative to the decoded start): the rise in short-window energy.
fn onset_at(x: &[f32], t: f64) -> f64 {
    let (w, c) = ((0.03 * SR) as usize, (t * SR) as usize);
    if c < w || c + w >= x.len() { return 0.0; }
    (rms(&x[c..c + w]) - rms(&x[c - w..c])).max(0.0)
}

#[derive(Clone, Debug)]
pub struct Bar { pub start: f64, pub end: f64, pub energy: f64, pub low: f64, pub onsets: f64 }
#[derive(Clone, Debug)]
pub struct Section { pub first_bar: usize, pub last_bar: usize, pub label: &'static str, pub energy: f64 }

/// Section boundaries (bar indices where a section starts, after 0) from per-bar features: novelty between the 4-bar
/// blocks either side of each 4-bar-aligned bar, kept where it is a local peak above the track's mean + 0.5 sd.
pub fn boundaries(bars: &[Bar]) -> Vec<usize> {
    let n = bars.len(); if n < 8 { return vec![]; }
    let emax = bars.iter().map(|b| b.energy).fold(1e-9, f64::max); let omax = bars.iter().map(|b| b.onsets).fold(1e-9, f64::max);
    let feat = |b: &Bar| [b.energy / emax, b.low, b.onsets / omax];
    let block = |a: usize, z: usize| -> [f64; 3] { let k = (z - a).max(1) as f64; let mut m = [0.0; 3]; for b in &bars[a..z] { let f = feat(b); for j in 0..3 { m[j] += f[j] / k; } } m };
    let mut nov: Vec<(usize, f64)> = vec![];
    let mut i = 4; while i + 4 <= n { let (l, r) = (block(i - 4, i), block(i, i + 4)); nov.push((i, ((l[0] - r[0]).powi(2) * 2.0 + (l[1] - r[1]).powi(2) + (l[2] - r[2]).powi(2)).sqrt())); i += 4; }
    if nov.is_empty() { return vec![]; }
    let mean = nov.iter().map(|x| x.1).sum::<f64>() / nov.len() as f64;
    let sd = (nov.iter().map(|x| (x.1 - mean).powi(2)).sum::<f64>() / nov.len() as f64).sqrt();
    let mut out = vec![];
    for k in 0..nov.len() {
        let v = nov[k].1; let prev = if k > 0 { nov[k - 1].1 } else { 0.0 }; let next = nov.get(k + 1).map_or(0.0, |x| x.1);
        if v > mean + 0.5 * sd && v >= prev && v >= next && v > 0.08 { out.push(nov[k].0); }
    }
    out
}

/// Labels sections by energy relative to the whole track (tertiles) and position.
pub fn label(bars: &[Bar], cuts: &[usize]) -> Vec<Section> {
    let mut starts = vec![0usize]; starts.extend_from_slice(cuts);
    let mut secs: Vec<Section> = starts.iter().enumerate().map(|(k, &a)| { let z = starts.get(k + 1).copied().unwrap_or(bars.len());
        let e = bars[a..z].iter().map(|b| b.energy).sum::<f64>() / (z - a).max(1) as f64; Section { first_bar: a, last_bar: z - 1, label: "", energy: e } }).collect();
    let mut es: Vec<f64> = secs.iter().map(|s| s.energy).collect(); es.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let (lo, hi) = if es.len() >= 3 { (es[es.len() / 3], es[2 * es.len() / 3]) } else { let m = es.iter().sum::<f64>() / es.len().max(1) as f64; (m * 0.75, m * 1.15) };
    let peak = es.last().copied().unwrap_or(0.0);
    let n = secs.len();
    for k in 0..n {
        let e = secs[k].energy; let next_e = secs.get(k + 1).map(|s| s.energy);
        secs[k].label = if e >= hi.max(peak * 0.8) { "drop" }
            else if e < lo.min(peak * 0.6) { if k == 0 { "intro" } else if k == n - 1 { "outro" } else { "break" } }
            else if next_e.map_or(false, |ne| ne >= hi.max(peak * 0.8) && ne > e * 1.25) { "build" }
            else if k == 0 { "intro" } else if k == n - 1 && e < hi { "outro" } else { "verse" };
    }
    secs
}

pub fn analyze(path: &str, t0: f64, dur: f64, beats_per_bar: usize) -> Result<Value, String> {
    let (bpm, beats_abs, conf) = crate::motion::detect_beats(path, t0, dur, 70.0, 180.0)?;
    let x = decode(path, t0, dur)?; let lo = lowpass(&x);
    let beats: Vec<f64> = beats_abs.iter().map(|b| b - t0).filter(|b| *b >= 0.0 && *b * SR < x.len() as f64).collect();
    if beats.len() < beats_per_bar * 2 { return Err(format!("too few beats ({}) to build bars; is this music?", beats.len())); }
    // downbeat phase: the bar position whose beats carry the most low-band onset energy
    let phase = (0..beats_per_bar).max_by(|&p, &q| {
        let sc = |ph: usize| beats.iter().enumerate().filter(|(i, _)| i % beats_per_bar == ph).map(|(_, b)| onset_at(&lo, *b)).sum::<f64>() / beats.iter().enumerate().filter(|(i, _)| i % beats_per_bar == ph).count().max(1) as f64;
        sc(p).partial_cmp(&sc(q)).unwrap_or(std::cmp::Ordering::Equal) }).unwrap_or(0);
    let downs: Vec<f64> = beats.iter().enumerate().filter(|(i, _)| i % beats_per_bar == phase).map(|(_, b)| *b).collect();
    let period = 60.0 / bpm.max(1.0);
    let bars: Vec<Bar> = downs.iter().enumerate().map(|(k, &a)| {
        let z = downs.get(k + 1).copied().unwrap_or(a + period * beats_per_bar as f64).min(x.len() as f64 / SR);
        let (i, j) = (((a * SR) as usize).min(x.len()), ((z * SR) as usize).min(x.len()));
        let (e, el) = (rms(&x[i..j]), rms(&lo[i..j]));
        let ons = beats.iter().filter(|b| **b >= a && **b < z).map(|b| onset_at(&x, *b)).sum::<f64>();
        Bar { start: a, end: z, energy: e, low: if e > 1e-9 { (el / e).min(1.0) } else { 0.0 }, onsets: ons } }).collect();
    let cuts = boundaries(&bars); let secs = label(&bars, &cuts);
    let r3 = |v: f64| (v * 1000.0).round() / 1000.0;
    let drops: Vec<f64> = secs.iter().enumerate().filter(|(k, s)| s.label == "drop" && *k > 0 && s.energy >= secs[k - 1].energy * 1.6).map(|(_, s)| r3(t0 + bars[s.first_bar].start)).collect();
    let emax = bars.iter().map(|b| b.energy).fold(1e-9, f64::max);
    Ok(json!({
        "bpm": r3(bpm), "confidence": r3(conf), "beats_per_bar": beats_per_bar,
        "beats": beats.iter().map(|b| r3(t0 + b)).collect::<Vec<_>>(),
        "downbeats": downs.iter().map(|b| r3(t0 + b)).collect::<Vec<_>>(),
        "bars": bars.iter().enumerate().map(|(k, b)| json!({"index": k + 1, "start": r3(t0 + b.start), "end": r3(t0 + b.end), "energy": r3(b.energy / emax)})).collect::<Vec<_>>(),
        "sections": secs.iter().map(|s| json!({"label": s.label, "start": r3(t0 + bars[s.first_bar].start), "end": r3(t0 + bars[s.last_bar].end), "bars": [s.first_bar + 1, s.last_bar + 1], "energy": r3(s.energy / emax)})).collect::<Vec<_>>(),
        "drops": drops,
        "note": "cut on downbeats (the 1 of each bar); sections start on bar lines, usually every 4 or 8 bars; a drop is the loudest change in the track",
    }))
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = if let Some(id) = a.get("asset_id").and_then(|v| v.as_str()) { ss.project.assets.get(id).map(|x| x.path.clone()).ok_or_else(|| format!("no asset '{id}'"))? }
        else if let Some(p) = a.get("path").and_then(|v| v.as_str()) { p.to_string() } else { return Err("give asset_id or path".into()) };
    let t0 = a.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0).max(0.0);
    let dur = a.get("duration").and_then(|v| v.as_f64()).unwrap_or(600.0).clamp(4.0, 1800.0);
    let bpb = a.get("beats_per_bar").and_then(|v| v.as_u64()).unwrap_or(4).clamp(2, 12) as usize;
    let mut r = analyze(&path, t0, dur, bpb)?; r["no_history"] = json!(true); Ok(r)
}

pub fn commands() -> Vec<Command> {
    vec![Command { name: "music_structure", description: "Bar grid of a music track: tempo, beats, downbeats (the 1 of each bar), bars with their energy, sections (intro, verse, build, drop, break, outro) and drops. Use it to cut on downbeats and land changes on section starts and drops. Times are seconds in the source file.", mutates: false,
        schema: obj(vec![("asset_id", s("music asset (or path)")), ("path", s("audio/video file, instead of asset_id")), ("start", n("seconds into the file to start, default 0")), ("duration", n("seconds to analyse, default 600")), ("beats_per_bar", n("default 4"))], &[]),
        run }]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn bar(e: f64) -> Bar { Bar { start: 0.0, end: 1.0, energy: e, low: 0.5, onsets: e } }
    #[test] fn a_quiet_then_loud_track_splits_into_intro_and_drop() {
        let mut bars: Vec<Bar> = (0..8).map(|_| bar(0.2)).collect(); bars.extend((0..8).map(|_| bar(1.0)));
        let cuts = boundaries(&bars); assert_eq!(cuts, vec![8], "one boundary, on the bar where it gets loud");
        let secs = label(&bars, &cuts); assert_eq!(secs.iter().map(|s| s.label).collect::<Vec<_>>(), vec!["intro", "drop"]);
    }
    #[test] fn a_steady_track_has_no_sections_to_invent() {
        let bars: Vec<Bar> = (0..16).map(|_| bar(0.6)).collect(); assert!(boundaries(&bars).is_empty());
    }
}
