//! Vocal / instrumental separation with Spleeter's 2-stem U-Nets (Deezer, MIT; fp16 ONNX from sherpa-onnx), run with
//! tract on the CPU.
//!
//! Spleeter's pipeline: stereo 44.1 kHz, STFT with a 4096-point Hann window and a 1024 hop; each network takes the
//! magnitudes of the first 1024 bins in blocks of 512 frames, x = [2 channels, splits, 512, 1024], and returns that
//! stem's estimated magnitudes. The two estimates become a soft (Wiener, power 2) mask per bin, applied to the full
//! complex STFT (bins above 1024, about 11 kHz, take the mask of bin 1023), and each stem is resynthesised by an
//! overlap-added inverse STFT.
use super::models;
use rustfft::{num_complex::Complex32, FftPlanner};
use std::sync::{Arc, Mutex, OnceLock};
use tract_onnx::prelude::*;

pub const SR: usize = 44100;
const N_FFT: usize = 4096;
const HOP: usize = 1024;
const BINS: usize = 1024;
const T: usize = 512;

type Plan = Arc<TypedRunnableModel>;
fn plans() -> &'static Mutex<Vec<(String, Plan)>> { static P: OnceLock<Mutex<Vec<(String, Plan)>>> = OnceLock::new(); P.get_or_init(Default::default) }
fn plan(id: &str) -> Result<Plan, String> {
    if let Some(p) = plans().lock().unwrap_or_else(|e| e.into_inner()).iter().find(|x| x.0 == id).map(|x| x.1.clone()) { return Ok(p); }
    let spec = models::spec(id).ok_or_else(|| format!("unknown model {id}"))?;
    let path = models::ensure(spec).map_err(|e| format!("{id}: {e}"))?;
    let onnx = tract_onnx::onnx();
    let p = (|| -> TractResult<Plan> {
        let mut proto = onnx.proto_model_for_path(&path)?; if let Some(g) = proto.graph.as_mut() { g.value_info.clear(); }
        let m = onnx.model_for_proto_model(&proto)?.with_input_fact(0, f32::fact([2, 1, T, BINS]).into())?;
        m.into_optimized()?.into_runnable()
    })().map_err(|e| format!("{id}: could not load the model ({e})"))?;
    plans().lock().unwrap_or_else(|e| e.into_inner()).push((id.to_string(), p.clone()));
    Ok(p)
}

fn hann() -> Vec<f32> { (0..N_FFT).map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos()).collect() }

/// STFT of one channel: frames x (N_FFT/2 + 1) complex bins (centre-padded by N_FFT/2 so frame k is centred on k*HOP).
fn stft(x: &[f32], win: &[f32], fft: &Arc<dyn rustfft::Fft<f32>>) -> Vec<Vec<Complex32>> {
    let pad = N_FFT / 2; let n = x.len() + 2 * pad; let frames = (n.saturating_sub(N_FFT)) / HOP + 1;
    let at = |i: usize| -> f32 { if i < pad || i >= pad + x.len() { 0.0 } else { x[i - pad] } };
    (0..frames).map(|f| { let mut b: Vec<Complex32> = (0..N_FFT).map(|k| Complex32::new(at(f * HOP + k) * win[k], 0.0)).collect(); fft.process(&mut b); b.truncate(N_FFT / 2 + 1); b }).collect()
}
/// Inverse STFT by windowed overlap-add, normalised by the summed squared window; returns `len` samples.
fn istft(spec: &[Vec<Complex32>], win: &[f32], ifft: &Arc<dyn rustfft::Fft<f32>>, len: usize) -> Vec<f32> {
    let pad = N_FFT / 2; let n = (spec.len() - 1) * HOP + N_FFT; let (mut out, mut norm) = (vec![0f32; n], vec![0f32; n]);
    for (f, s) in spec.iter().enumerate() {
        let mut b = vec![Complex32::new(0.0, 0.0); N_FFT];
        for k in 0..s.len() { b[k] = s[k]; if k > 0 && k < N_FFT / 2 { b[N_FFT - k] = s[k].conj(); } }
        ifft.process(&mut b);
        for k in 0..N_FFT { out[f * HOP + k] += b[k].re / N_FFT as f32 * win[k]; norm[f * HOP + k] += win[k] * win[k]; }
    }
    (0..len).map(|i| { let j = i + pad; if j < n && norm[j] > 1e-6 { out[j] / norm[j] } else { 0.0 } }).collect()
}

/// Separates interleaved stereo at 44.1 kHz into (vocals, accompaniment), each interleaved stereo of the same length.
pub fn separate(stereo: &[f32]) -> Result<(Vec<f32>, Vec<f32>), String> {
    let len = stereo.len() / 2; if len < N_FFT { return Err("audio too short to separate".into()); }
    let (pv, pa) = (plan("spleeter_vocals")?, plan("spleeter_accompaniment")?);
    let win = hann(); let mut pl = FftPlanner::<f32>::new(); let (fft, ifft) = (pl.plan_fft_forward(N_FFT), pl.plan_fft_inverse(N_FFT));
    let ch: Vec<Vec<f32>> = (0..2).map(|c| (0..len).map(|i| stereo[i * 2 + c]).collect()).collect();
    let specs: Vec<Vec<Vec<Complex32>>> = ch.iter().map(|x| stft(x, &win, &fft)).collect();
    let frames = specs[0].len();
    // magnitudes of both stems, block by block of 512 frames
    let (mut mv, mut ma) = (vec![vec![0f32; frames * BINS]; 2], vec![vec![0f32; frames * BINS]; 2]);
    let mut f0 = 0; while f0 < frames {
        crate::guard::check();
        let mut x = vec![0f32; 2 * T * BINS];
        for c in 0..2 { for t in 0..T { let f = f0 + t; if f >= frames { break; } for b in 0..BINS { x[(c * T + t) * BINS + b] = specs[c][f][b].norm(); } } }
        let input: Tensor = tract_ndarray::Array4::from_shape_vec((2, 1, T, BINS), x).map_err(|e| e.to_string())?.into();
        for (p, dst) in [(&pv, &mut mv), (&pa, &mut ma)] {
            let out = p.run(tvec!(input.clone().into())).map_err(|e| format!("separation failed: {e}"))?;
            let ov = out[0].to_plain_array_view::<f32>().map_err(|e| e.to_string())?; let o = ov.to_slice().ok_or("separation output is not contiguous")?;
            for c in 0..2 { for t in 0..T { let f = f0 + t; if f >= frames { break; } for b in 0..BINS { dst[c][f * BINS + b] = o[(c * T + t) * BINS + b]; } } }
        }
        f0 += T;
    }
    let mut stems = (vec![0f32; len * 2], vec![0f32; len * 2]);
    for c in 0..2 {
        let (mut sv, mut sa) = (specs[c].clone(), specs[c].clone());
        for f in 0..frames { for k in 0..=N_FFT / 2 { let b = k.min(BINS - 1);
            let (v, a) = (mv[c][f * BINS + b].powi(2), ma[c][f * BINS + b].powi(2)); let m = (v + 1e-10 / 2.0) / (v + a + 1e-10);
            sv[f][k] = specs[c][f][k] * m; sa[f][k] = specs[c][f][k] * (1.0 - m); } }
        let (yv, ya) = (istft(&sv, &win, &ifft, len), istft(&sa, &win, &ifft, len));
        for i in 0..len { stems.0[i * 2 + c] = yv[i]; stems.1[i * 2 + c] = ya[i]; }
    }
    Ok(stems)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn stft_then_istft_gives_the_signal_back() {
        let x: Vec<f32> = (0..20000).map(|i| (i as f32 * 0.031).sin() * 0.5 + ((i * 7) % 13) as f32 / 40.0).collect();
        let win = hann(); let mut pl = FftPlanner::<f32>::new(); let (f, inv) = (pl.plan_fft_forward(N_FFT), pl.plan_fft_inverse(N_FFT));
        let y = istft(&stft(&x, &win, &f), &win, &inv, x.len());
        let err = x.iter().zip(&y).skip(N_FFT).take(x.len() - 2 * N_FFT).map(|(a, b)| (a - b).abs()).fold(0f32, f32::max);
        assert!(err < 1e-3, "perfect reconstruction away from the edges: max error {err}");
    }
}
