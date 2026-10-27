//! Persistent editorial metadata and parameterized audio processing.
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Editorial {
    pub captions: BTreeMap<String, CaptionGroup>,
    pub multicams: BTreeMap<String, Multicam>,
    /// RV-8 transcripts. Absent from saved projects until one is imported, so older files stay byte-identical.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub transcripts: BTreeMap<String, crate::transcript::Transcript>,
    /// cue clip id -> the transcript words (and line breaks) it was built from. Editing a word re-composes the cue text, never its timing.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub cue_words: BTreeMap<String, crate::transcript::CueBinding>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaptionGroup {
    pub track_id: String,
    pub clips: Vec<String>,
    pub style: String,
    pub burn_in: bool,
    /// Transcript this group was generated from (RV-8); absent for hand-made groups.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transcript_id: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Multicam {
    pub track_id: String,
    pub angles: Vec<Clip>,
    pub switches: Vec<(f64, usize)>,
    pub audio_angle: usize,
    pub start: f64,
    pub duration: f64,
}
pub fn caption_visible(p: &Project, id: &str) -> bool {
    !p.editorial
        .captions
        .values()
        .any(|g| !g.burn_in && g.clips.iter().any(|x| x == id))
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AudioEffect {
    Biquad {
        sections: Vec<[f64; 5]>,
        #[serde(default)]
        channels: Option<Vec<usize>>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        keys: BTreeMap<String, crate::keyframe::Prop>,
    },
    Fir {
        coeffs: Vec<f64>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        keys: BTreeMap<String, crate::keyframe::Prop>,
    },
    MixMatrix {
        matrix: Vec<Vec<f64>>,
        #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
        keys: BTreeMap<String, crate::keyframe::Prop>,
    },
    Eq {
        frequency: f64,
        width: f64,
        gain_db: f64,
    },
    Compressor {
        threshold_db: f64,
        ratio: f64,
        attack_ms: f64,
        release_ms: f64,
        makeup_db: f64,
    },
    Deesser {
        intensity: f64,
        max: f64,
        frequency: f64,
    },
    Reverb {
        decay: f64,
        delay_ms: f64,
    },
    Denoise {
        reduction_db: f64,
        floor_db: f64,
    },
    /// Noise gate (ffmpeg agate): mutes/attenuates signal below threshold_db, down to range_db when fully closed.
    NoiseGate {
        threshold_db: f64,
        range_db: f64,
        ratio: f64,
        attack_ms: f64,
        release_ms: f64,
    },
    /// Brickwall limiter (ffmpeg alimiter): true ceiling at ceiling_db, auto makeup to reach it.
    Limiter {
        ceiling_db: f64,
        attack_ms: f64,
        release_ms: f64,
    },
}
/// dB (relative to full scale) to a linear amplitude ratio, for ffmpeg filters (agate/alimiter) that take a linear 0..1 level.
fn db_to_lin(db: f64) -> f64 { 10f64.powf(db / 20.0) }
impl AudioEffect {
    pub fn filter(&self) -> String {
        match self {
 Self::Biquad{..}|Self::Fir{..}|Self::MixMatrix{..}=>"anull".into(),
 Self::Eq{frequency,width,gain_db}=>format!("equalizer=f={frequency}:t=q:w={width}:g={gain_db}"),
 Self::Compressor{threshold_db,ratio,attack_ms,release_ms,makeup_db}=>format!("acompressor=threshold={}dB:ratio={ratio}:attack={attack_ms}:release={release_ms}:makeup={}dB",threshold_db,makeup_db),
 Self::Deesser{intensity,max,frequency}=>format!("deesser=i={intensity}:m={max}:f={frequency}"),
 Self::Reverb{..}=>"anull".into(),
 Self::Denoise{reduction_db,floor_db}=>format!("afftdn=nr={reduction_db}:nf={floor_db}"),
 Self::NoiseGate{threshold_db,range_db,ratio,attack_ms,release_ms}=>format!("agate=threshold={:.8}:range={:.8}:ratio={ratio}:attack={attack_ms}:release={release_ms}",db_to_lin(*threshold_db),db_to_lin(*range_db)),
 Self::Limiter{ceiling_db,attack_ms,release_ms}=>format!("alimiter=limit={:.8}:attack={attack_ms}:release={release_ms}:level=0",db_to_lin(*ceiling_db).min(1.0)),
}
    }
}
/// ITU BS.1770 K weighting at 48 kHz, stereo channel weights 1, EBU R128
/// 400 ms blocks at 100 ms hops; absolute -70 LUFS and relative -10 LU gates.
/// No true-peak claim: peak below is a sample peak.
/// Per-frame K-weighted energy (BS.1770 pre-filter + RLB high-pass, 48 kHz coefficients), summed over L and R (G = 1.0).
/// Shared by `loudness` and the momentary / short-term / LRA series so every number comes from one filter.
pub fn k_energy(pcm: &[f32]) -> Option<Vec<f64>> {
    crate::guard::reserve(pcm.len().checked_mul(4)?);
    let mut energy = vec![0.; pcm.len() / 2];
    for ch in 0..2 {
        let mut s1 = [0.; 4];
        let mut s2 = [0.; 4];
        for (i, e) in energy.iter_mut().enumerate() {
            if i % 32768 == 0 {
                crate::guard::check()
            }
            let x = pcm[i * 2 + ch] as f64;
            let biquad = |x: f64, s: &mut [f64; 4], b: [f64; 3], a: [f64; 2]| {
                let y = b[0] * x + b[1] * s[0] + b[2] * s[1] - a[0] * s[2] - a[1] * s[3];
                s[1] = s[0];
                s[0] = x;
                s[3] = s[2];
                s[2] = y;
                y
            };
            let y = biquad(
                x,
                &mut s1,
                [1.53512485958697, -2.69169618940638, 1.19839281085285],
                [-1.69065929318241, 0.73248077421585],
            );
            let z = biquad(
                y,
                &mut s2,
                [1., -2., 1.],
                [-1.99004745483398, 0.99007225036621],
            );
            *e += z * z;
        }
    }
    Some(energy)
}
pub fn loudness(pcm: &[f32]) -> Option<f64> {
    if pcm.len() < 38400 {
        return None;
    }
    let energy = k_energy(pcm)?;
    let mut blocks = vec![];
    let mut sum: f64 = energy[..19200].iter().sum();
    for st in (0..=energy.len() - 19200).step_by(4800) {
        if st > 0 {
            sum -= energy[st - 4800..st].iter().sum::<f64>();
            sum += energy[st + 14400..st + 19200].iter().sum::<f64>();
        }
        let e = sum / 19200.;
        if e > 0. && -0.691 + 10. * e.log10() >= -70. {
            blocks.push(e)
        }
    }
    if blocks.is_empty() {
        return None;
    }
    let mean = blocks.iter().sum::<f64>() / blocks.len() as f64;
    let threshold = mean * 0.1;
    let gated: Vec<f64> = blocks.into_iter().filter(|v| *v >= threshold).collect();
    Some(-0.691 + 10. * (gated.iter().sum::<f64>() / gated.len() as f64).log10())
}

/// Exact 10 ms (480-frame) sums of `k_energy`; a trailing partial chunk (< 10 ms) is dropped. Window sums are
/// built from these by addition only, so a silent window sums to exactly 0 and reads null, never a drift value.
pub fn k_energy_chunks(energy: &[f64]) -> Vec<f64> {
    energy.chunks_exact(480).map(|c| c.iter().sum()).collect()
}
/// Ungated sliding loudness (EBU Tech 3341 momentary = 40 chunks / 400 ms, short-term = 300 chunks / 3 s): reading k
/// covers chunks [k*hop, k*hop+win), i.e. it ends at (win + k*hop) * 10 ms. LUFS = -0.691 + 10 log10(mean energy);
/// None where the window energy is zero. No readings when fewer than `win` chunks exist.
pub fn loudness_series(chunks: &[f64], win: usize, hop: usize) -> Vec<Option<f64>> {
    let mut out = vec![];
    if win == 0 || hop == 0 || chunks.len() < win {
        return out;
    }
    let mut st = 0;
    while st + win <= chunks.len() {
        if out.len() % 4096 == 0 {
            crate::guard::check()
        }
        let e = chunks[st..st + win].iter().sum::<f64>() / (win * 480) as f64;
        out.push(if e > 0. { Some(-0.691 + 10. * e.log10()) } else { None });
        st += hop;
    }
    out
}
/// Loudness range per EBU Tech 3342 from short-term (3 s) readings: absolute gate -70 LUFS, relative gate 20 LU below
/// the energy mean of the absolutely gated readings, then the 10th and 95th percentiles (nearest rank on the sorted
/// values, index round(p*(n-1))). Returns (lra_lu, low_lufs, high_lufs, readings_used); None when no reading passes.
pub fn loudness_range(short_term: &[Option<f64>]) -> Option<(f64, f64, f64, usize)> {
    let abs: Vec<f64> = short_term.iter().flatten().copied().filter(|v| *v >= -70.).collect();
    if abs.is_empty() {
        return None;
    }
    let mean = abs.iter().map(|v| 10f64.powf((v + 0.691) / 10.)).sum::<f64>() / abs.len() as f64;
    let rel = -0.691 + 10. * mean.log10() - 20.;
    let mut kept: Vec<f64> = abs.into_iter().filter(|v| *v >= rel).collect();
    if kept.is_empty() {
        return None;
    }
    kept.sort_by(f64::total_cmp);
    let at = |p: f64| kept[((kept.len() - 1) as f64 * p).round() as usize];
    let (lo, hi) = (at(0.10), at(0.95));
    Some((hi - lo, lo, hi, kept.len()))
}
/// Count interleaved stereo samples with |x| >= `threshold` (linear), and frames where either channel does.
pub fn count_clipped(pcm: &[f32], threshold: f64) -> (usize, usize) {
    let (mut samples, mut frames) = (0, 0);
    for f in pcm.chunks_exact(2) {
        let n = f.iter().filter(|v| v.abs() as f64 >= threshold).count();
        samples += n;
        frames += (n > 0) as usize;
    }
    (samples, frames)
}

/// Stereo Schroeder reverb: eight parallel damped combs and four serial
/// allpasses per channel, with decorrelated delay lines. Feedback < 1.
/// `decay` controls room feedback; `delay_ms` scales the room, not a single echo.
pub fn reverb(pcm: &mut [f32], decay: f64, delay_ms: f64) {
    struct Delay {
        buffer: Vec<f32>,
        index: usize,
        low: f32,
    }
    impl Delay {
        fn new(n: usize) -> Self {
            Self {
                buffer: vec![0.; n.max(1)],
                index: 0,
                low: 0.,
            }
        }
    }
    let scale = (delay_ms / 60.).clamp(0.1, 4.);
    let feedback = (0.28 + decay * 0.82) as f32;
    for ch in 0..2 {
        let mut combs: Vec<Delay> = [1116, 1188, 1277, 1356, 1422, 1491, 1557, 1617]
            .iter()
            .map(|n| Delay::new(((*n + ch * 23) as f64 * 48000. / 44100. * scale) as usize))
            .collect();
        let mut passes: Vec<Delay> = [556, 441, 341, 225]
            .iter()
            .map(|n| Delay::new(((*n + ch * 23) as f64 * 48000. / 44100. * scale) as usize))
            .collect();
        for (i, frame) in pcm.chunks_exact_mut(2).enumerate() {
            if i % 32768 == 0 {
                crate::guard::check()
            }
            let dry = frame[ch];
            let mut wet = 0.;
            for c in &mut combs {
                let out = c.buffer[c.index];
                c.low = out * 0.8 + c.low * 0.2;
                c.buffer[c.index] = dry * 0.12 + c.low * feedback;
                c.index = (c.index + 1) % c.buffer.len();
                wet += out;
            }
            for a in &mut passes {
                let delayed = a.buffer[a.index];
                let input = wet;
                wet = delayed - input;
                a.buffer[a.index] = input + delayed * 0.5;
                a.index = (a.index + 1) % a.buffer.len();
            }
            frame[ch] = dry * 0.75 + wet * 0.25;
        }
    }
}
pub fn process_pcm(pcm: &mut [f32], effects: &[AudioEffect]) {
    process_pcm_checked(pcm, effects).unwrap_or_else(|e| panic!("{e}"));
}
pub fn process_pcm_checked(pcm: &mut [f32], effects: &[AudioEffect]) -> Result<(), String> {
    for e in effects {
        use crate::perception::audio_raw::{Samples, Section};
        let raw = matches!(
            e,
            AudioEffect::Biquad { .. } | AudioEffect::Fir { .. } | AudioEffect::MixMatrix { .. }
        );
        if raw {
            if raw_keys(e).map_or(false, |k| !k.is_empty()) {
                process_keyed_raw(pcm, e)?;
                continue;
            }
            let input = Samples {
                rate: 48000,
                planes: vec![
                    pcm.chunks_exact(2).map(|p| p[0]).collect(),
                    pcm.chunks_exact(2).map(|p| p[1]).collect(),
                ],
            };
            let result = match e {
                AudioEffect::Biquad {
                    sections,
                    channels,
                    keys,
                } => {
                    let sec: Vec<_> = sections
                        .iter()
                        .map(|s| Section {
                            b: [s[0], s[1], s[2]],
                            a: [1., s[3], s[4]],
                        })
                        .collect();
                    crate::perception::audio_raw::biquad(&input, &sec).map(|mut out| {
                        if let Some(ch) = channels {
                            for c in 0..2 {
                                if !ch.contains(&c) {
                                    out.planes[c] = input.planes[c].clone();
                                }
                            }
                        }
                        out
                    })
                }
                AudioEffect::Fir { coeffs, keys } => {
                    crate::perception::audio_raw::fir(&input, coeffs)
                }
                AudioEffect::MixMatrix { matrix, keys } => {
                    crate::perception::audio_raw::mix_matrix(&input, matrix)
                }
                _ => unreachable!(),
            };
            match result {
                Ok(out) => {
                    for (i, p) in pcm.chunks_exact_mut(2).enumerate() {
                        p[0] = out.planes[0][i];
                        p[1] = out.planes[1][i];
                    }
                }
                Err(e) => return Err(e),
            };
            continue;
        }
        if let AudioEffect::Reverb { decay, delay_ms } = e {
            reverb(pcm, *decay, *delay_ms)
        }
    }
    Ok(())
}

/// Four-phase windowed-sinc reconstruction estimate of intersample peak.
/// Includes sample peaks, reconstructs at 4x; not a certified hardware meter.
/// Blackman window, 64 taps per fractional phase, normalized DC gain.
pub fn true_peak_4x(pcm: &[f32]) -> Result<Option<f64>, String> {
    if pcm.len() % 2 != 0 || pcm.iter().any(|v| !v.is_finite()) {
        return Err("finite stereo PCM required".into());
    }
    if pcm.is_empty() {
        return Ok(None);
    }
    let frames = pcm.len() / 2;
    let mut phases = vec![];
    for phase in 1..4 {
        let frac = phase as f64 / 4.;
        let mut weights = vec![];
        for tap in -31i32..=32 {
            let x = tap as f64 - frac;
            let pi = std::f64::consts::PI;
            let sinc = if x.abs() < 1e-12 {
                1.
            } else {
                (pi * x).sin() / (pi * x)
            };
            let window = 0.42 + 0.5 * (pi * x / 32.).cos() + 0.08 * (2. * pi * x / 32.).cos();
            weights.push((tap, sinc * window));
        }
        let sum = weights.iter().map(|(_, v)| v).sum::<f64>();
        for (_, v) in &mut weights {
            *v /= sum
        }
        phases.push(weights);
    }
    let mut peak = pcm.iter().map(|v| v.abs() as f64).fold(0., f64::max);
    for i in 0..frames {
        if i % 4096 == 0 {
            crate::guard::check()
        }
        for weights in &phases {
            for ch in 0..2 {
                let mut v = 0.;
                for (tap, weight) in weights {
                    let j = i as i64 + *tap as i64;
                    if j >= 0 && (j as usize) < frames {
                        v += pcm[j as usize * 2 + ch] as f64 * weight;
                    }
                }
                peak = peak.max(v.abs());
            }
        }
    }
    Ok(if peak > 0. {
        Some(20. * peak.log10())
    } else {
        None
    })
}
#[cfg(test)]
mod truepeak_tests {
    use super::*;
    #[test]
    fn intersample_over_and_silence() {
        let mut pcm = vec![];
        for i in 0..48000 {
            let x = 0.9
                * (std::f64::consts::TAU * 12000. * i as f64 / 48000.
                    + std::f64::consts::FRAC_PI_4)
                    .sin();
            let fade = (i as f64 / 1000.).min((48000 - i) as f64 / 1000.).min(1.);
            pcm.extend([x as f32 * fade as f32; 2]);
        }
        let sample = 20.
            * pcm
                .iter()
                .map(|v| v.abs() as f64)
                .fold(0., f64::max)
                .log10();
        let peak = true_peak_4x(&pcm).unwrap().unwrap();
        assert!(peak - sample > 2.9, "sample{sample}, reconstructed{peak}");
        assert!((peak - 20. * 0.9f64.log10()).abs() < 0.1);
        assert_eq!(true_peak_4x(&vec![0.; 100]).unwrap(), None);
        assert!(true_peak_4x(&[f32::NAN, 0.]).is_err());
    }
    #[test]
    fn compare_ffmpeg_ebur128() {
        let path = std::env::temp_dir().join(format!("kimo-truepeak-{}.wav", std::process::id()));
        for hz in [997., 12000., 17000.] {
            let pcm: Vec<f32> = (0..48000)
                .flat_map(|i| {
                    let fade = (i as f64 / 1000.).min((48000 - i) as f64 / 1000.).min(1.);
                    let v = (0.8
                        * (std::f64::consts::TAU * hz * i as f64 / 48000. + 0.785).sin()
                        * fade) as f32;
                    [v, v]
                })
                .collect();
            crate::audio::write_wav(path.to_str().unwrap(), &pcm).unwrap();
            let out = std::process::Command::new("ffmpeg")
                .args([
                    "-hide_banner",
                    "-nostats",
                    "-i",
                    path.to_str().unwrap(),
                    "-af",
                    "ebur128=peak=true",
                    "-f",
                    "null",
                    "-",
                ])
                .output()
                .unwrap();
            assert!(out.status.success());
            let text = String::from_utf8_lossy(&out.stderr);
            let summary = text.rsplit("True peak:").next().unwrap();
            let expected = summary
                .split("Peak:")
                .nth(1)
                .unwrap()
                .split_whitespace()
                .next()
                .unwrap()
                .parse::<f64>()
                .unwrap();
            let estimate = true_peak_4x(&pcm).unwrap().unwrap();
            assert!(
                (estimate - expected).abs() < 0.3,
                "{hz}Hz ours{estimate}, ffmpeg{expected}"
            );
        }
        std::fs::remove_file(path).unwrap();
    }
}

fn raw_keys(e: &AudioEffect) -> Option<&BTreeMap<String, crate::keyframe::Prop>> {
    match e {
        AudioEffect::Biquad { keys, .. }
        | AudioEffect::Fir { keys, .. }
        | AudioEffect::MixMatrix { keys, .. } => Some(keys),
        _ => None,
    }
}
fn process_keyed_raw(pcm: &mut [f32], e: &AudioEffect) -> Result<(), String> {
    let keys = raw_keys(e).unwrap();
    let at = |name: &str, d: f64, t: f64| keys.get(name).map(|p| p.raw(t)).unwrap_or(d);
    let input = pcm.to_vec();
    let mut states = match e {
        AudioEffect::Biquad { sections, .. } => vec![[[0f64; 2]; 2]; sections.len()],
        _ => vec![],
    };
    for i in 0..pcm.len() / 2 {
        if i % 16384 == 0 {
            crate::guard::check();
        }
        let t = i as f64 / 48000.;
        let mut pair = [input[i * 2] as f64, input[i * 2 + 1] as f64];
        match e {
            AudioEffect::Biquad {
                sections, channels, ..
            } => {
                for (k, s) in sections.iter().enumerate() {
                    let mut co = *s;
                    for (c, name) in ["b0", "b1", "b2", "a1", "a2"].iter().enumerate() {
                        co[c] = at(&format!("sections.{k}.{name}"), co[c], t);
                    }
                    for ch in 0..2 {
                        if channels.as_ref().map_or(false, |c| !c.contains(&ch)) {
                            continue;
                        }
                        let st = &mut states[k][ch];
                        let x = pair[ch];
                        let y = co[0] * x + st[0];
                        st[0] = co[1] * x - co[3] * y + st[1];
                        st[1] = co[2] * x - co[4] * y;
                        pair[ch] = y;
                    }
                }
            }
            AudioEffect::Fir { coeffs, .. } => {
                pair = [0.; 2];
                for (k, &c) in coeffs.iter().enumerate().take(i + 1) {
                    let c = at(&format!("coeffs.{k}"), c, t);
                    for ch in 0..2 {
                        pair[ch] += c * input[(i - k) * 2 + ch] as f64;
                    }
                }
            }
            AudioEffect::MixMatrix { matrix, .. } => {
                let x = pair;
                for ch in 0..2 {
                    pair[ch] = 0.;
                    for src in 0..2 {
                        pair[ch] += at(&format!("matrix.{ch}.{src}"), matrix[ch][src], t) * x[src];
                    }
                }
            }
            _ => {}
        }
        for ch in 0..2 {
            let v = pair[ch] as f32;
            if !v.is_finite() {
                return Err(format!("invalid_filter: keyed raw coefficient output nonfinite at sample {i}, no substitution"));
            }
            pcm[i * 2 + ch] = v;
        }
    }
    Ok(())
}
