//! Fixed-order, fixed-FFT spectral-flux detector over mono 48 kHz PCM.
//! No decoder, Session, mutation, random state or thread-dependent reduction.
use serde::{Deserialize, Serialize};
pub const SR: usize = 48000;
pub const FRAME: usize = crate::dsp::fft::FRAME;
pub const HOP: usize = 256;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Params {
    pub sensitivity: f64,
    pub min_gap_ms: f64,
    pub band: [f64; 2],
    pub mode: String,
    pub max_items: usize,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            sensitivity: 0.5,
            min_gap_ms: 80.0,
            band: [0.0, 24000.0],
            mode: "onset".into(),
            max_items: 500,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Event {
    pub t: f64,
    pub strength: f64,
    pub level_db: f64,
    pub kind: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Beat {
    pub t: f64,
    pub strength: f64,
    pub phase: f64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Analysis {
    pub transients: Vec<Event>,
    pub count: usize,
    pub truncated: bool,
    pub bpm: Option<f64>,
    pub confidence: f64,
    pub alt_bpm: Option<f64>,
    pub beat_grid: Vec<Beat>,
}
fn median(v: &mut [f64]) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}
use crate::dsp::fft::Fft;
pub fn analyze(pcm: &[f32], p: &Params) -> Result<Analysis, String> {
    if pcm.len() < FRAME {
        return Err("insufficient_data: at least 1024 mono samples (21.334 ms) required".into());
    }
    if !p.sensitivity.is_finite() || !(0.0..=1.0).contains(&p.sensitivity) {
        return Err("sensitivity must be 0..1".into());
    }
    if !p.min_gap_ms.is_finite() || !(20.0..=2000.0).contains(&p.min_gap_ms) {
        return Err("min_gap_ms must be 20..2000".into());
    }
    if !p.band[0].is_finite()
        || !p.band[1].is_finite()
        || p.band[0] < 0.
        || p.band[1] > 24000.
        || p.band[0] >= p.band[1]
    {
        return Err("band must satisfy 0 <= f_lo < f_hi <= 24000".into());
    }
    if !["onset", "beat"].contains(&p.mode.as_str()) {
        return Err("unknown mode. Valid: onset, beat".into());
    }
    if p.max_items == 0 || p.max_items > 5000 {
        return Err("max_items must be 1..5000".into());
    }
    let n = pcm.len().div_ceil(HOP);
    crate::guard::reserve(n * 8 * 5 + FRAME * 8 * 10);
    let mut flux = vec![0.; n];
    let mut flat = vec![0.; n];
    let mut prev = [0.; FRAME / 2 + 1];
    let mut fft = Fft::new();
    if pcm.iter().any(|v| !v.is_finite()) {
        return Err("decode_failed: nonfinite PCM".into());
    }
    let lo = (p.band[0] * FRAME as f64 / SR as f64).ceil() as usize;
    let hi = ((p.band[1] * FRAME as f64 / SR as f64).floor() as usize).min(FRAME / 2);
    if lo > hi {
        return Err("band must include at least one FFT bin (46.875 Hz spacing)".into());
    }
    let bins = hi - lo + 1;
    for f in 0..n {
        crate::guard::check();
        fft.run(pcm, f * HOP);
        let (mut sum, mut logs, mut rise) = (0., 0., 0.);
        for k in lo..=hi {
            let mag = (fft.re[k] * fft.re[k] + fft.im[k] * fft.im[k]).sqrt();
            let lm = (1. + mag).ln();
            rise += (lm - prev[k]).max(0.);
            prev[k] = lm;
            sum += mag;
            logs += (mag + 1e-12).ln();
        }
        flux[f] = rise / bins as f64;
        flat[f] = (logs / bins as f64).exp() / (sum / bins as f64 + 1e-12);
    }
    let max = flux.iter().copied().fold(0., f64::max);
    let k = 6.0 - 5.0 * p.sensitivity;
    let half = (0.25 * SR as f64 / HOP as f64).round() as usize;
    let floor = 0.001;
    let mut cand = vec![];
    let mut local = Vec::with_capacity(2 * half + 1);
    let mut dev = Vec::with_capacity(2 * half + 1);
    for i in 1..n.saturating_sub(1) {
        if i % 256 == 0 {
            crate::guard::check();
        }
        if flux[i] <= floor || flux[i] < flux[i - 1] || flux[i] <= flux[i + 1] {
            continue;
        }
        local.clear();
        local.extend_from_slice(&flux[i.saturating_sub(half)..(i + half + 1).min(n)]);
        let med = median(&mut local);
        dev.clear();
        dev.extend(local.iter().map(|x| (x - med).abs()));
        let mad = median(&mut dev);
        let eligible = flux[i] > med + k * mad + floor;
        // Refine to peak absolute amplitude in +/- one frame around the flux peak.
        let center = i * HOP;
        let a = center.saturating_sub(FRAME);
        let b = (center + FRAME).min(pcm.len());
        let mut peak = a;
        let mut amp = 0f32;
        for j in a..b {
            if !pcm[j].is_finite() {
                return Err("decode_failed: nonfinite PCM".into());
            }
            if pcm[j].abs() > amp {
                amp = pcm[j].abs();
                peak = j;
            }
        }
        // Reject decaying/resonant tails: an onset must raise local envelope peak
        // above the preceding non-overlapping 10ms window, not just shift FFT bins.
        let past0 = peak.saturating_sub(960);
        let past1 = peak.saturating_sub(480);
        let past = pcm[past0..past1]
            .iter()
            .map(|v| v.abs())
            .fold(0f32, f32::max);
        if amp < 0.005 || amp < past * 1.25 {
            continue;
        }
        let t = peak as f64 / SR as f64;
        let a = peak.saturating_sub(240);
        let b = (peak + 240).min(pcm.len());
        let rms = (pcm[a..b].iter().map(|v| (*v as f64).powi(2)).sum::<f64>()
            / (b - a).max(1) as f64)
            .sqrt();
        cand.push((
            Event {
                t,
                strength: if max > 0. {
                    (flux[i] / max).clamp(0., 1.)
                } else {
                    0.
                },
                level_db: 20. * rms.max(1e-12).log10(),
                kind: if flat[i] > 0.2 { "percussive" } else { "tonal" }.into(),
            },
            eligible,
        ));
    }
    // Deterministic earliest-wins refractory suppression ensures a stable, monotonic
    // event count as threshold sensitivity increases: refractory anchors are chosen
    // BEFORE adaptive threshold eligibility, so sensitivity cannot move them.
    cand.sort_by(|a, b| a.0.t.total_cmp(&b.0.t));
    let gap = p.min_gap_ms / 1000.;
    let mut events: Vec<Event> = vec![];
    let mut anchor = None;
    for (e, eligible) in cand {
        if anchor.map_or(true, |t| e.t - t >= gap - 1e-12) {
            anchor = Some(e.t);
            if eligible {
                events.push(e);
            }
        }
    }
    let (mut bpm, mut confidence, mut alt, mut grid) = (None, 0., None, vec![]);
    if p.mode == "beat" && events.len() >= 4 {
        let fps = SR as f64 / HOP as f64;
        let mut ac = vec![0.; (fps * 60. / 60.).ceil() as usize + 2];
        let energy = flux.iter().map(|x| x * x).sum::<f64>();
        let (mut score, mut lag) = (0., 0usize);
        for l in (fps * 60. / 200.).floor() as usize..=(fps * 60. / 60.).ceil() as usize {
            crate::guard::check();
            if l >= n {
                continue;
            }
            let s = (0..n - l).map(|i| flux[i] * flux[i + l]).sum::<f64>() / energy.max(1e-20);
            ac[l] = s;
            if s > score {
                score = s;
                lag = l;
            }
        }
        if lag > 0 {
            let off = if lag > 0 && lag + 1 < ac.len() {
                let d = ac[lag - 1] - 2. * ac[lag] + ac[lag + 1];
                if d.abs() > 1e-15 {
                    (0.5 * (ac[lag - 1] - ac[lag + 1]) / d).clamp(-0.5, 0.5)
                } else {
                    0.
                }
            } else {
                0.
            };
            let ac_period = (lag as f64 + off) / fps;
            let mut intervals = events
                .windows(2)
                .map(|w| w[1].t - w[0].t)
                .collect::<Vec<_>>();
            let period = median(&mut intervals);
            let regular = intervals
                .iter()
                .filter(|d| (**d - period).abs() < 0.02)
                .count() as f64
                / intervals.len() as f64;
            confidence = (regular * score.sqrt()).clamp(0., 1.);
            // Octave rule: prefer median event spacing when it agrees with autocorrelation;
            // at 60 BPM retain 60 rather than inventing unobserved 120 BPM hits.
            let period = if (period / ac_period - 1.).abs() < 0.1
                || (period / ac_period - 2.).abs() < 0.1
                || (period / ac_period - 0.5).abs() < 0.1
            {
                period
            } else {
                ac_period
            };
            let tempo = 60. / period;
            if confidence > 0.5 && (59.5..=200.5).contains(&tempo) {
                bpm = Some(tempo);
                if tempo < 100. {
                    alt = Some(tempo * 2.);
                }
                let t0 = events[0].t;
                let mut t = t0;
                while t < pcm.len() as f64 / SR as f64 && grid.len() < 5000 {
                    let strength = events
                        .iter()
                        .filter(|e| (e.t - t).abs() < 0.025)
                        .map(|e| e.strength)
                        .fold(0., f64::max);
                    grid.push(Beat {
                        t,
                        strength,
                        phase: (t - t0) / period,
                    });
                    t = t0 + grid.len() as f64 * period;
                }
            }
        }
    }
    let count = events.len();
    let truncated = count > p.max_items;
    events.truncate(p.max_items);
    Ok(Analysis {
        transients: events,
        count,
        truncated,
        bpm,
        confidence,
        alt_bpm: alt,
        beat_grid: grid,
    })
}
