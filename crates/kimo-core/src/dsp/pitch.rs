//! Provenance: adapted from a clean git clone of beatbox-rs (https://github.com/naman-sharma12345/beatbox-rs)
//! commit 907fb0a1c8a30c245facba58ea27df8366080526, src/sc_dsp.rs, yin(), itself from SoundCraft
//! https://github.com/storytold/soundcraft, crates/dsp/src/pitch_detect.rs, MIT OR Apache-2.0, used here under MIT.
//! Copyright (c) 2026 ArtCraft Team and the SoundCraft contributors. See THIRD_PARTY_NOTICES.md.
//! Changes: min_hz <20 now returns None (original clamped to20); max_hz >=sr/2 now None; threshold outside(0,1) now None;
//! finite checks, cheap guard polling and (hz, confidence) return added. Pure std; no beatbox crate coupling.

/// YIN, de Cheveigne & Kawahara (2002). Frame is mono PCM; sample_rate, min_hz and max_hz are in Hz.
/// threshold is the dimensionless CMND acceptance threshold, strictly between0 and1.
/// Returns (Hz, confidence0..1); confidence is1-CMND at the integer lag,1 for perfectly periodic audio.
/// This confidence is not a calibrated voicing probability.
pub fn pitch_detect(
    frame: &[f32],
    sample_rate: f32,
    min_hz: f32,
    max_hz: f32,
    threshold: f32,
) -> Option<(f32, f32)> {
    if !(sample_rate.is_finite() && sample_rate > 0.0 && min_hz.is_finite() && max_hz.is_finite()
        && min_hz >= 20.0 && max_hz > min_hz && max_hz < sample_rate/2.0
        && threshold.is_finite() && threshold > 0.0 && threshold < 1.0)
        || frame.len() < 64 || frame.iter().any(|x| !x.is_finite()) {
        return None;
    }
    let max_tau = ((sample_rate / min_hz.max(20.0)) as usize).min(frame.len() / 2);
    let min_tau = ((sample_rate / max_hz.max(min_hz + 1.0)) as usize).max(2);
    if max_tau <= min_tau + 2 {
        return None;
    }
    let w = frame.len() - max_tau;
    crate::guard::reserve((max_tau + 1).checked_mul(8)?);
    let mut d = vec![0.0f32; max_tau + 1];
    for (tau, dv) in d.iter_mut().enumerate().skip(1) {
        if tau % 32 == 0 { crate::guard::check(); }
        let mut sum = 0.0f32;
        for j in 0..w {
            let diff = frame[j] - frame[j + tau];
            sum += diff * diff;
        }
        *dv = sum;
    }
    let mut cmnd = vec![1.0f32; max_tau + 1];
    let mut running = 0.0f32;
    for tau in 1..=max_tau {
        running += d[tau];
        cmnd[tau] = if running > 0.0 {
            d[tau] * tau as f32 / running
        } else {
            1.0
        };
    }
    let mut tau = min_tau;
    while tau < max_tau {
        if cmnd[tau] < threshold {
            while tau + 1 < max_tau && cmnd[tau + 1] < cmnd[tau] {
                tau += 1;
            }
            let (a, b, c) = (cmnd[tau - 1], cmnd[tau], cmnd[tau + 1]);
            let denom = a - 2.0 * b + c;
            let shift = if denom.abs() > 1e-9 {
                0.5 * (a - c) / denom
            } else {
                0.0
            };
            let t = tau as f32 + shift.clamp(-1.0, 1.0);
            return (t > 0.0).then_some((sample_rate / t, (1.0-cmnd[tau]).clamp(0.0,1.0)));
        }
        tau += 1;
    }
    None
}

