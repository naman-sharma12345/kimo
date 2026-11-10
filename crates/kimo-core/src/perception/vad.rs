//! Deterministic energy/ZCR/spectral VAD, not a neural speech recognizer.
//! Fixed-order mono reductions and FFT; stationary pure tones are rejected at aggressiveness >=2.
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Params {
    pub mode: String,
    pub aggressiveness: u8,
    pub min_speech_ms: f64,
    pub min_silence_ms: f64,
    pub pad_ms: f64,
    pub hangover_ms: f64,
    pub threshold_db: f64,
    pub merge_gap_ms: f64,
    pub max_items: usize,
}
impl Default for Params {
    fn default() -> Self {
        Self {
            mode: "vad".into(),
            aggressiveness: 1,
            min_speech_ms: 200.,
            min_silence_ms: 300.,
            pad_ms: 120.,
            hangover_ms: 200.,
            threshold_db: -60.,
            merge_gap_ms: 0.,
            max_items: 1000,
        }
    }
}
impl Params {
    pub fn check(&self) -> Result<(), String> {
        if !["vad", "silence"].contains(&self.mode.as_str()) {
            return Err("mode must be vad|silence".into());
        }
        if self.aggressiveness > 3 {
            return Err("aggressiveness must be 0..3".into());
        }
        for v in [
            self.min_speech_ms,
            self.min_silence_ms,
            self.pad_ms,
            self.hangover_ms,
            self.merge_gap_ms,
        ] {
            if !v.is_finite() || !(0. ..=60000.).contains(&v) {
                return Err("timing parameters must be finite 0..60000 ms".into());
            }
        }
        if !self.threshold_db.is_finite() || !(-200. ..=0.).contains(&self.threshold_db) {
            return Err("threshold_db must be -200..0".into());
        }
        if !(1..=100000).contains(&self.max_items) {
            return Err("max_items must be 1..100000".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Speech {
    pub start: f64,
    pub end: f64,
    pub confidence: f64,
    pub level_db: f64,
}
#[derive(Clone, Debug, Serialize)]
pub struct Silence {
    pub start: f64,
    pub end: f64,
    pub duration: f64,
    pub level_db: Option<f64>,
}
#[derive(Clone, Debug, Serialize)]
pub struct Cut {
    pub start: f64,
    pub end: f64,
    pub remove_s: f64,
    pub reason: &'static str,
}
#[derive(Clone, Debug, Serialize)]
pub struct ResultVad {
    pub params: Params,
    pub method: &'static str,
    pub duration_s: f64,
    pub speech: Vec<Speech>,
    pub silence: Vec<Silence>,
    pub speech_ratio: f64,
    pub noise_floor_db: f64,
    pub cut_suggestions: Vec<Cut>,
    pub count: usize,
    pub truncated: bool,
}
#[derive(Clone, Copy)]
struct Feature {
    db: f64,
    zcr: f64,
    centroid: f64,
    flat: f64,
    band: f64,
    tonal: f64,
}
fn features(x: &[f32]) -> Feature {
    let energy = x.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / x.len().max(1) as f64;
    let zcr = x
        .windows(2)
        .filter(|w| (w[0] >= 0.) != (w[1] >= 0.))
        .count() as f64
        / x.len().max(1) as f64;
    let (mut re, mut im) = ([0f64; 512], [0f64; 512]);
    for (i, v) in x.iter().enumerate() {
        re[i] = *v as f64 * (0.5 - 0.5 * (2. * std::f64::consts::PI * i as f64 / 319.).cos());
    }
    let mut j = 0;
    for i in 1..512 {
        let mut bit = 256;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= 512 {
        let a = -2. * std::f64::consts::PI / len as f64;
        let (wr, wi) = (a.cos(), a.sin());
        for b in (0..512).step_by(len) {
            let (mut r, mut q) = (1., 0.);
            for k in 0..len / 2 {
                let u = b + k;
                let v = u + len / 2;
                let (tr, ti) = (r * re[v] - q * im[v], r * im[v] + q * re[v]);
                re[v] = re[u] - tr;
                im[v] = im[u] - ti;
                re[u] += tr;
                im[u] += ti;
                let nr = r * wr - q * wi;
                q = r * wi + q * wr;
                r = nr;
            }
        }
        len *= 2;
    }
    let (mut total, mut band, mut weighted, mut logs, mut peak) = (0., 0., 0., 0., 0f64);
    let mut n = 0;
    for k in 1..257 {
        let e = re[k] * re[k] + im[k] * im[k];
        total += e;
        let hz = k as f64 * 16000. / 512.;
        if (300. ..=3400.).contains(&hz) {
            band += e;
            weighted += hz * e;
            logs += e.max(1e-30).ln();
            peak = peak.max(e);
            n += 1;
        }
    }
    Feature {
        db: 10. * energy.max(1e-20).log10(),
        zcr,
        centroid: weighted / band.max(1e-30),
        flat: (logs / n as f64).exp() / (band / n as f64).max(1e-30),
        band: band / total.max(1e-30),
        tonal: peak / band.max(1e-30),
    }
}
/// Stereo 48k engine PCM -> mono 16k, fixed 6-sample box anti-alias reduction.
pub fn mono16(pcm: &[f32]) -> Result<Vec<f32>, String> {
    if pcm.len() % 2 != 0 {
        return Err("PCM must be interleaved stereo".into());
    }
    for (i, v) in pcm.iter().enumerate() {
        if i % 32768 == 0 {
            crate::guard::check()
        }
        if !v.is_finite() {
            return Err("PCM must be finite".into());
        }
    }
    crate::guard::reserve(pcm.len() / 6 * 4);
    let mut out = Vec::with_capacity(pcm.len() / 6);
    for (i, c) in pcm.chunks_exact(6).enumerate() {
        if i % 1024 == 0 {
            crate::guard::check()
        }
        if c.iter().any(|v| !v.is_finite()) {
            return Err("PCM must be finite".into());
        }
        out.push((c.iter().map(|v| *v as f64).sum::<f64>() / 6.) as f32);
    }
    Ok(out)
}
fn complement(speech: &[Speech], duration: f64, min: f64) -> Vec<Silence> {
    let mut out = vec![];
    let mut at = 0.;
    for s in speech {
        if s.start - at >= min && s.start > at {
            out.push(Silence {
                start: at,
                end: s.start,
                duration: s.start - at,
                level_db: None,
            })
        }
        at = s.end;
    }
    if duration - at >= min && duration > at {
        out.push(Silence {
            start: at,
            end: duration,
            duration: duration - at,
            level_db: None,
        })
    }
    out
}
/// Matches audio_probe's default 100ms stereo-RMS silence windows exactly.
pub fn silence_probe(pcm: &[f32], threshold: f64, min_ms: f64) -> Vec<Silence> {
    let frames = pcm.len() / 2;
    let mut out = vec![];
    let (mut run, mut level) = (None, f64::NEG_INFINITY);
    let mut flush = |a: usize, b: usize, lv: f64| {
        let (s, e) = (a as f64 / 48000., b.min(frames) as f64 / 48000.);
        if e - s >= min_ms / 1000. {
            out.push(Silence {
                start: s,
                end: e,
                duration: e - s,
                level_db: if lv.is_finite() { Some(lv) } else { None },
            })
        }
    };
    for (w, c) in pcm.chunks(9600).enumerate() {
        crate::guard::check();
        let rms = (c.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / c.len() as f64).sqrt();
        let db = if rms > 0. {
            20. * rms.log10()
        } else {
            f64::NEG_INFINITY
        };
        if db < threshold {
            run.get_or_insert(w * 4800);
            level = level.max(db)
        } else if let Some(a) = run.take() {
            flush(a, w * 4800, level);
            level = f64::NEG_INFINITY;
        }
    }
    if let Some(a) = run {
        flush(a, frames, level)
    }
    out
}
pub fn analyze(pcm: &[f32], p: &Params) -> Result<ResultVad, String> {
    p.check()?;
    if pcm.is_empty() {
        return Err("no audio in target".into());
    }
    let mono = mono16(pcm)?;
    let duration = pcm.len() as f64 / 96000.;
    let nf = mono.len().div_ceil(320);
    crate::guard::reserve(nf * 128);
    let mut fs = Vec::with_capacity(nf);
    for c in mono.chunks(320) {
        crate::guard::check();
        fs.push(features(c))
    }
    let mut levels: Vec<f64> = fs.iter().map(|f| f.db).collect();
    levels.sort_by(f64::total_cmp);
    let floor = levels.get(levels.len() / 5).copied().unwrap_or(-200.);
    let mut speech = vec![];
    let mut silence;
    if p.mode == "silence" {
        silence = silence_probe(pcm, p.threshold_db, p.min_silence_ms);
        let mut at = 0.;
        for s in &silence {
            if s.start > at {
                speech.push(Speech {
                    start: at,
                    end: s.start,
                    confidence: 1.,
                    level_db: p.threshold_db,
                })
            }
            at = s.end
        }
        if duration > at {
            speech.push(Speech {
                start: at,
                end: duration,
                confidence: 1.,
                level_db: p.threshold_db,
            })
        }
    } else {
        let mut noise_history = vec![floor; 100];
        let mut noise_cursor = 0usize;
        let mut adaptive_floor = floor;
        let enter = [5., 7., 10., 13.][p.aggressiveness as usize];
        let exit = enter - 2.;
        let mut ranges = vec![];
        let (mut active, mut last) = (None, 0usize);
        for (i, f) in fs.iter().enumerate() {
            let shape = f.zcr > 0.01
                && f.zcr < 0.48
                // Low formant vowels can have substantial energy below 300 Hz.
                // Admit only structured multi-harmonic spectra in the lower band.
                && (f.band > 0.25 || (f.band > 0.15 && f.flat < 0.1 && f.tonal < 0.38))
                && f.centroid > 300.
                && f.centroid < 3400.
                && f.flat < 0.9
                && (p.aggressiveness < 2 || f.tonal < 0.38);
            if !shape || f.db < adaptive_floor + exit {
                noise_history[noise_cursor % 100] = f.db;
                noise_cursor += 1;
                if noise_cursor % 10 == 0 {
                    let mut sorted = noise_history.clone();
                    sorted.sort_by(f64::total_cmp);
                    adaptive_floor = sorted[20];
                }
            }
            let hot = shape && f.db - adaptive_floor >= if active.is_some() { exit } else { enter };
            if hot {
                if active.is_none() {
                    active = Some(i)
                }
                last = i + 1;
            } else if let Some(a) = active {
                if (i + 1 - last) as f64 * 20. >= p.hangover_ms {
                    ranges.push((a, last));
                    active = None
                }
            }
        }
        if let Some(a) = active {
            ranges.push((a, last));
        }
        for (a, b) in ranges {
            if (b - a) as f64 * 20. < p.min_speech_ms {
                continue;
            }
            let start = (a as f64 * 0.02 - p.pad_ms / 1000.).max(0.);
            let end = (b as f64 * 0.02 + p.hangover_ms / 1000. + p.pad_ms / 1000.).min(duration);
            let level = fs[a..b].iter().map(|f| f.db).sum::<f64>() / (b - a) as f64;
            let confidence = ((level - floor - enter + 10.) / 20.).clamp(0., 1.);
            if let Some(prev) = speech.last_mut() {
                let prev: &mut Speech = prev;
                if start - prev.end <= p.min_silence_ms.max(p.merge_gap_ms) / 1000. {
                    prev.end = prev.end.max(end);
                    prev.level_db = prev.level_db.max(level);
                    prev.confidence = prev.confidence.max(confidence);
                    continue;
                }
            }
            speech.push(Speech {
                start,
                end,
                confidence,
                level_db: level,
            });
        }
        silence = complement(&speech, duration, p.min_silence_ms / 1000.);
    }
    let ratio = speech.iter().map(|s| s.end - s.start).sum::<f64>() / duration;
    let cuts: Vec<Cut> = silence
        .iter()
        .map(|s| Cut {
            start: s.start,
            end: s.end,
            remove_s: s.duration,
            reason: "pause>=min_silence",
        })
        .collect();
    let count = speech.len();
    let truncated = speech.len() > p.max_items || silence.len() > p.max_items;
    speech.truncate(p.max_items);
    silence.truncate(p.max_items);
    let mut cuts = cuts;
    cuts.truncate(p.max_items);
    Ok(ResultVad {
        params: p.clone(),
        method: if p.mode == "vad" {
            "energy_zcr_spectral_vad_v1"
        } else {
            "audio_probe_rms_silence_v1"
        },
        duration_s: duration,
        speech,
        silence,
        speech_ratio: ratio,
        noise_floor_db: floor,
        cut_suggestions: cuts,
        count,
        truncated,
    })
}
