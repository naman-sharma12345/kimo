//! Persistent editorial metadata and parameterized audio processing.
use crate::model::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Editorial {
    pub captions: BTreeMap<String, CaptionGroup>,
    pub multicams: BTreeMap<String, Multicam>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CaptionGroup {
    pub track_id: String,
    pub clips: Vec<String>,
    pub style: String,
    pub burn_in: bool,
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
}
impl AudioEffect {
    pub fn filter(&self) -> String {
        match self {
 Self::Eq{frequency,width,gain_db}=>format!("equalizer=f={frequency}:t=q:w={width}:g={gain_db}"),
 Self::Compressor{threshold_db,ratio,attack_ms,release_ms,makeup_db}=>format!("acompressor=threshold={}dB:ratio={ratio}:attack={attack_ms}:release={release_ms}:makeup={}dB",threshold_db,makeup_db),
 Self::Deesser{intensity,max,frequency}=>format!("deesser=i={intensity}:m={max}:f={frequency}"),
 Self::Reverb{..}=>"anull".into(),
 Self::Denoise{reduction_db,floor_db}=>format!("afftdn=nr={reduction_db}:nf={floor_db}"),
}
    }
}
/// ITU BS.1770 K weighting at 48 kHz, stereo channel weights 1, EBU R128
/// 400 ms blocks at 100 ms hops; absolute -70 LUFS and relative -10 LU gates.
/// No true-peak claim: peak below is a sample peak.
pub fn loudness(pcm: &[f32]) -> Option<f64> {
    if pcm.len() < 38400 {
        return None;
    }
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
    for e in effects {
        if let AudioEffect::Reverb { decay, delay_ms } = e {
            reverb(pcm, *decay, *delay_ms)
        }
    }
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
