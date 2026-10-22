//! v6 "eyes" audio unit B: audio_spectrum, a read-only magnitude spectrum and band energies of the final timeline mix or one
//! clip (the same 48 kHz stereo decode as measure_loudness). Raw numbers only: no verdict, every parameter is an argument.
//! Method (textbook, own code; FFT by rustfft): periodic Hann window, frames every fft_size/2 samples (Welch average of
//! |X|^2 over frames and over the L and R channels). Two scales, both dBFS:
//! - amplitude (spectrum, peak): 10*log10(mean|X_k|^2) + 20*log10(2/sum(w)); a full-scale sine centred on a bin reads 0 dBFS.
//! - power (bands, total): one-sided P_k = c_k*mean|X_k|^2/(N*sum(w^2)), c_k = 2 except DC and Nyquist; sum of P_k is the
//!   window-weighted mean square (Parseval), so a band reads its RMS level the way ffmpeg astats does (full-scale sine -3.01).
use super::kit::*;
use crate::model::Source;
use rustfft::{num_complex::Complex, FftPlanner};

pub const SR: f64 = 48000.0;
/// Most values band_series may hold (bands x readings) before the call is refused.
pub const SERIES_VALUE_CAP: usize = 200_000;

/// Band edges in Hz: base-ten octave (IEC 61260-1: centre 1000*10^(3n/10), edges x10^(-+0.15)) for 31.5 Hz..16 kHz,
/// third-octave (centre 1000*10^(n/10), edges x10^(-+0.05)) for 25 Hz..20 kHz. Returns (lo, centre, hi) per band.
pub fn standard_bands(kind: &str) -> Option<Vec<(f64, f64, f64)>> {
    let (r, half, step): (std::ops::RangeInclusive<i32>, f64, f64) = match kind { "octave" => (-5..=4, 0.15, 0.3), "third_octave" => (-16..=13, 0.05, 0.1), _ => return None };
    Some(r.map(|n| { let c = 1000. * 10f64.powf(step * n as f64); (c * 10f64.powf(-half), c, c * 10f64.powf(half)) }).collect())
}
/// Custom edges: 2..129 finite, strictly ascending, within 0..=24000 Hz; band i is [e_i, e_i+1), centre the geometric mean.
pub fn custom_bands(edges: &[f64]) -> Result<Vec<(f64, f64, f64)>, String> {
    if !(2..=129).contains(&edges.len()) { return Err("bands as an array needs 2..129 edges in Hz".into()); }
    for w in edges.windows(2) { if !(w[1] > w[0]) { return Err("band edges must be strictly ascending".into()); } }
    if edges.iter().any(|e| !e.is_finite() || *e < 0. || *e > SR / 2.) { return Err("band edges must be numbers 0..24000 Hz".into()); }
    Ok(edges.windows(2).map(|w| (w[0], if w[0] > 0. { (w[0] * w[1]).sqrt() } else { w[1] / 2. }, w[1])).collect())
}
/// What one analysis needs, already validated.
pub struct Cfg { pub fft_size: usize, pub bands: Vec<(f64, f64, f64)>, pub bands_kind: String, pub spectrum_points: usize, pub series_hop_cs: Option<usize>, pub max_points: Option<usize>, pub t0: f64 }
impl Cfg { pub fn new(fft_size: usize, bands: Vec<(f64, f64, f64)>, bands_kind: &str) -> Self { Cfg { fft_size, bands, bands_kind: bands_kind.into(), spectrum_points: 256, series_hop_cs: None, max_points: None, t0: 0. } } }

fn db10(p: f64) -> Option<f64> { if p > 0. && p.is_finite() { Some(10. * p.log10()) } else { None } }
fn r3(x: Option<f64>) -> Option<f64> { x.map(|v| (v * 1000.).round() / 1000.) }
/// Which band (index) a bin frequency lands in; the top edge counts when it is the Nyquist frequency.
fn band_of(bands: &[(f64, f64, f64)], f: f64) -> Option<usize> { bands.iter().position(|&(lo, _, hi)| f >= lo && (f < hi || (hi >= SR / 2. && f <= hi))) }

/// Spectrum of an interleaved stereo 48 kHz buffer (already cut to the range). Errors when it holds fewer than fft_size frames.
pub fn analyze(pcm: &[f32], cfg: &Cfg) -> Result<Value, String> {
    let n = cfg.fft_size; let frames = pcm.len() / 2;
    if !(n.is_power_of_two() && (256..=65536).contains(&n)) { return Err("fft_size must be a power of 2, 256..65536".into()); }
    if frames < n { return Err(format!("the range holds {frames} samples per channel, fewer than fft_size {n}; pass a smaller fft_size or a longer range")); }
    let hop = n / 2; let nf = (frames - n) / hop + 1; let unread = frames - ((nf - 1) * hop + n);
    let half = n / 2; let bin_hz = SR / n as f64;
    let w: Vec<f64> = (0..n).map(|i| 0.5 - 0.5 * (std::f64::consts::TAU * i as f64 / n as f64).cos()).collect();
    let s1: f64 = w.iter().sum(); let s2: f64 = w.iter().map(|x| x * x).sum();
    let pk = |k: usize, m2: f64| if k == 0 || k == half { m2 / (n as f64 * s2) } else { 2. * m2 / (n as f64 * s2) };
    let bin_band: Vec<Option<usize>> = (0..=half).map(|k| band_of(&cfg.bands, k as f64 * bin_hz)).collect();
    let nb = cfg.bands.len();
    // band_series: frames assigned to readings of hop_s by their centre; max_points raises the hop (as measure_loudness's curve)
    let last_c = ((nf - 1) * hop + half) as f64 / SR;
    let series = match cfg.series_hop_cs {
        None => None,
        Some(h0) => {
            if (h0 as f64 / 100.) < hop as f64 / SR - 1e-12 { return Err(format!("hop_s {} is shorter than one frame hop ({:.4} s at fft_size {n}); pass hop_s >= {:.2}", h0 as f64 / 100., hop as f64 / SR, ((hop as f64 / SR) * 100.).ceil() / 100.)); }
            let count = |h: usize| (last_c / (h as f64 / 100.)).floor() as usize + 1;
            let mut h = h0;
            if let Some(m) = cfg.max_points { if count(h) > m { h = h.max((last_c * 100. / (m - 1) as f64).ceil() as usize); while count(h) > m { h += 1; } } }
            let r = count(h);
            if r * nb > SERIES_VALUE_CAP { return Err(format!("band_series would hold {} values ({} readings x {} bands, cap {SERIES_VALUE_CAP}); raise hop_s, pass max_points or use fewer bands", r * nb, r, nb)); }
            Some((h0, h, r))
        }
    };
    let mut seg_p = series.map(|(_, _, r)| vec![0f64; r * nb]); let mut seg_n = series.map(|(_, _, r)| vec![0usize; r]);
    let fft = FftPlanner::<f64>::new().plan_fft_forward(n);
    let mut acc = vec![0f64; half + 1]; let mut buf = vec![Complex::new(0., 0.); n]; let mut scratch = vec![Complex::new(0., 0.); fft.get_inplace_scratch_len()];
    let mut fb = vec![0f64; nb];
    for f in 0..nf {
        crate::guard::check();
        let st = f * hop; for v in fb.iter_mut() { *v = 0.; }
        for ch in 0..2 {
            for i in 0..n { buf[i] = Complex::new(pcm[2 * (st + i) + ch] as f64 * w[i], 0.); }
            fft.process_with_scratch(&mut buf, &mut scratch);
            for k in 0..=half { let m2 = buf[k].norm_sqr() / 2.; acc[k] += m2; if let Some(b) = bin_band[k] { fb[b] += pk(k, m2); } }
        }
        if let (Some((_, h, _)), Some(sp), Some(sn)) = (series, seg_p.as_mut(), seg_n.as_mut()) {
            let r = (((st + half) as f64 / SR) / (h as f64 / 100.)).floor() as usize; sn[r] += 1; for b in 0..nb { sp[r * nb + b] += fb[b]; }
        }
    }
    for v in acc.iter_mut() { *v /= nf as f64; }
    let amp = |k: usize| { let a = if k == 0 || k == half { 1. } else { 2. } / s1; db10(acc[k] * a * a) };
    let total = db10((0..=half).map(|k| pk(k, acc[k])).sum());
    let peak_k = (1..=half).max_by(|&a, &b| acc[a].partial_cmp(&acc[b]).unwrap_or(std::cmp::Ordering::Equal)).filter(|&k| acc[k] > 0.);
    let mut bp = vec![0f64; nb]; let mut bc = vec![0usize; nb];
    for k in 0..=half { if let Some(b) = bin_band[k] { bp[b] += pk(k, acc[k]); bc[b] += 1; } }
    let bdb: Vec<Option<f64>> = (0..nb).map(|b| if bc[b] == 0 { None } else { db10(bp[b]) }).collect();
    let loud = (0..nb).filter(|&b| bdb[b].is_some()).max_by(|&a, &b| bdb[a].partial_cmp(&bdb[b]).unwrap());
    let bands: Vec<Value> = cfg.bands.iter().enumerate().map(|(b, &(lo, c, hi))| json!({"lo_hz": r3(Some(lo)), "centre_hz": r3(Some(c)), "hi_hz": r3(Some(hi)), "dbfs": bdb[b], "bins": bc[b]})).collect();
    let mut out = json!({
        "sample_rate": SR as u32, "channels": "mean of the L and R power spectra", "range": {"start": cfg.t0, "end": cfg.t0 + frames as f64 / SR},
        "fft_size": n, "hop_size": hop, "window": "Hann (periodic)", "bin_hz": bin_hz, "enbw_hz": 1.5 * bin_hz, "frames_averaged": nf, "samples_unread": unread,
        "peak_hz": peak_k.map(|k| k as f64 * bin_hz), "peak_dbfs": peak_k.and_then(amp), "total_dbfs": total,
        "loudest_band": loud.map(|b| json!({"lo_hz": r3(Some(cfg.bands[b].0)), "hi_hz": r3(Some(cfg.bands[b].2)), "dbfs": bdb[b]})),
        "bands_kind": cfg.bands_kind, "bands": bands,
        "method": "Hann-windowed FFT frames every fft_size/2 samples, |X|^2 averaged over frames and the L and R channels. peak_dbfs and spectrum are amplitude dBFS (a full-scale sine centred on a bin reads 0; off-centre up to 1.42 dB lower, Hann scalloping). total_dbfs and band dbfs are power dBFS (one-sided, Parseval-scaled: RMS level, a full-scale sine reads -3.01, as ffmpeg astats); a band holds the bins whose centre frequency is in [lo_hz, hi_hz). null: zero energy or a band with no bins. samples_unread: trailing samples per channel past the last whole frame."
    });
    if cfg.spectrum_points > 0 {
        let (mut hz, mut dbv) = (vec![], vec![]);
        if cfg.spectrum_points >= half { for k in 1..=half { hz.push(k as f64 * bin_hz); dbv.push(r3(amp(k))); } }
        else {
            let (f1, f2) = (bin_hz, SR / 2.); let m = cfg.spectrum_points; let edge = |i: usize| f1 * (f2 / f1).powf(i as f64 / m as f64);
            let mut k = 1usize;
            for i in 0..m { let hi = edge(i + 1); let mut best: Option<usize> = None;
                while k <= half && (k as f64 * bin_hz < hi || i == m - 1) { if best.map_or(true, |b| acc[k] > acc[b]) { best = Some(k); } k += 1; }
                if let Some(b) = best { hz.push(b as f64 * bin_hz); dbv.push(r3(amp(b))); } }
        }
        out["spectrum"] = json!({"points": hz.len(), "requested_points": cfg.spectrum_points, "reduction": if cfg.spectrum_points >= half {"none: every bin 1..fft_size/2"} else {"log-spaced buckets from bin_hz to 24000 Hz, each the loudest bin in it (its own frequency); empty buckets skipped"}, "hz": hz, "dbfs": dbv});
    }
    if let (Some((h0, h, r)), Some(sp), Some(sn)) = (series, seg_p, seg_n) {
        let hs = h as f64 / 100.;
        let per_band: Vec<Vec<Option<f64>>> = (0..nb).map(|b| (0..r).map(|i| if sn[i] == 0 || bc[b] == 0 { None } else { r3(db10(sp[i * nb + b] / sn[i] as f64)) }).collect()).collect();
        out["band_series"] = json!({"requested_hop_s": h0 as f64 / 100., "hop_s": hs, "max_points": cfg.max_points, "readings": r,
            "t_start": (0..r).map(|i| ((cfg.t0 + i as f64 * hs) * 1e6).round() / 1e6).collect::<Vec<_>>(), "frames_per_reading": sn, "dbfs": per_band,
            "t": "reading i averages the frames whose centre lies in [t_start[i], t_start[i] + hop_s); dbfs[band][reading] in the bands order; null: no frame or no energy"});
    }
    Ok(out)
}

fn int_arg(a: &Value, k: &str, d: usize, lo: usize, hi: usize) -> Result<usize, String> {
    match a.get(k) { None => Ok(d), Some(v) => v.as_u64().map(|x| x as usize).filter(|x| (lo..=hi).contains(x)).ok_or_else(|| format!("{k} must be an integer {lo}..{hi}")) }
}
/// Parse and validate the call's arguments into a Cfg (t0 set by the caller). Nothing is clamped.
pub fn cfg_from_args(a: &Value) -> Result<Cfg, String> {
    let n = int_arg(a, "fft_size", 4096, 256, 65536)?; if !n.is_power_of_two() { return Err("fft_size must be a power of 2, 256..65536".into()); }
    let (bands, kind) = match a.get("bands") {
        None => (standard_bands("octave").unwrap(), "octave".to_string()),
        Some(Value::String(s)) => (standard_bands(s).ok_or("bands must be \"octave\", \"third_octave\" or an array of edges in Hz")?, s.clone()),
        Some(Value::Array(v)) => { let e: Option<Vec<f64>> = v.iter().map(|x| x.as_f64()).collect(); (custom_bands(&e.ok_or("band edges must be numbers 0..24000 Hz")?)?, "custom".to_string()) }
        Some(_) => return Err("bands must be \"octave\", \"third_octave\" or an array of edges in Hz".into()),
    };
    let sp = int_arg(a, "spectrum_points", 256, 0, 65537)?;
    let hop = match a.get("hop_s") { None => None, Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or("hop_s must be a number of seconds")?; let c = (x * 100.).round();
        if !(1.0..=6000.0).contains(&c) || (c / 100. - x).abs() > 1e-9 { return Err("hop_s must be 0.01..60 s in 0.01 s steps".into()); } Some(c as usize) } };
    if a.get("max_points").is_some() && hop.is_none() { return Err("max_points needs hop_s".into()); }
    let mp = match a.get("max_points") { None => None, Some(_) => Some(int_arg(a, "max_points", 0, 10, 100_000)?) };
    let mut c = Cfg::new(n, bands, &kind); c.spectrum_points = sp; c.series_hop_cs = hop; c.max_points = mp; Ok(c)
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let mut cfg = cfg_from_args(a)?;
    let pcm: Vec<f32> = match a.get("clip_id") {
        None => crate::audio::mix_strict(&ss.project).map_err(|e| e.to_string())?,
        Some(v) => { let id = v.as_str().ok_or("clip_id must be a string")?; let c = ss.project.find_clip(id).ok_or_else(|| format!("no clip '{id}'"))?;
            let Source::Media { asset } = &c.source else { return Err("no audio: media clip required".into()) };
            let path = ss.project.resolve(&ss.project.assets.get(asset).ok_or("no asset")?.path); crate::audio::strict_decode(&path, c).map_err(|e| e.to_string())? }
    };
    if pcm.len() < 2 { return Err("no audio in this target".into()); }
    let total = pcm.len() as f64 / 2. / SR;
    let num = |k: &str, d: f64| -> Result<f64, String> { match a.get(k) { None => Ok(d), Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a number of seconds")) } };
    let (st, en) = (num("start", 0.)?, num("end", total)?);
    if st < 0. || en <= st || en > total + 1e-9 { return Err(format!("range must satisfy 0 <= start < end <= {total:.6} s (the audio's length)")); }
    let (i0, i1) = ((st * SR).round() as usize, ((en * SR).round() as usize).min(pcm.len() / 2));
    cfg.t0 = i0 as f64 / SR;
    analyze(&pcm[2 * i0..2 * i1], &cfg)
}

pub fn commands() -> Vec<Command> { vec![Command { name: "audio_spectrum", mutates: false,
    description: "Magnitude spectrum and band energies in dBFS of the final timeline mix or one clip: averaged Hann-windowed FFT, octave/third-octave/custom bands. Optional clip_id measures that clip decoded post filters, before volume/pan (as measure_loudness); absent measures the final timeline mix, 48 kHz, L and R power averaged. start/end (seconds) pick a range. fft_size: power of 2, 256..65536, default 4096 (bin_hz = 48000/fft_size); frames every fft_size/2. Returns peak_hz/peak_dbfs (amplitude: a full-scale sine centred on a bin reads 0 dBFS, up to 1.42 dB lower between bins), total_dbfs and per-band dbfs (power, RMS level: full-scale sine -3.01 dBFS, as ffmpeg astats), loudest_band, and spectrum: at most spectrum_points (default 256, 0 omits) log-spaced points, each the loudest bin in its bucket. bands: octave (default, 31.5 Hz-16 kHz, IEC 61260 base-ten edges), third_octave (25 Hz-20 kHz) or an array of 2..129 ascending edges in Hz. hop_s (0.01..60, at least one frame hop) adds band_series: band levels per hop_s; max_points (10..100000) raises hop_s so it holds at most that many readings; over 200000 values is refused. Out-of-range arguments are errors, never clamped. Zero energy or a band with no FFT bins reads null. Raw numbers only, no verdict.",
    schema: obj(vec![("clip_id", s("optional media clip; absent = final timeline mix")), ("start", n("range start seconds, default 0")), ("end", n("range end seconds, default the audio's end")),
        ("fft_size", json!({"type":"integer","minimum":256,"maximum":65536,"description":"power of 2, default 4096"})),
        ("bands", json!({"description":"\"octave\" (default), \"third_octave\" or an array of 2..129 ascending band edges in Hz (0..24000)","oneOf":[{"type":"string","enum":["octave","third_octave"]},{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":129}]})),
        ("spectrum_points", json!({"type":"integer","minimum":0,"maximum":65537,"description":"spectrum point cap, default 256, 0 omits"})),
        ("hop_s", json!({"type":"number","minimum":0.01,"maximum":60,"description":"adds band_series: one reading per hop_s, 0.01 s steps"})),
        ("max_points", json!({"type":"integer","minimum":10,"maximum":100000,"description":"cap band_series readings; raises hop_s to fit"}))], &[]),
    run }] }
