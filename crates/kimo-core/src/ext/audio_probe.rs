//! audio_probe (RV-25 slice 2, first cut): read-only audio facts for a clip or the final mix, one pass over the decoded stereo f32 buffer (48 kHz interleaved).
//! Numbers come from the same engine functions as measure_loudness (editorial::loudness, true_peak_4x), so they match it digit for digit.
//! Not in this cut (ARCH spec, later commits): speech_regions VAD, spectral, beats, sync, ducking, issues[], waveform/spectrogram images.
use super::kit::*;
use crate::model::Source;

const SR: f64 = 48000.0;
fn db(x: f64) -> Option<f64> { if x > 0.0 { Some(20.0 * x.log10()) } else { None } }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "audio_probe", mutates: false,
        description: "Audio facts for a media clip (decoded post filters, before volume/pan) or the final timeline mix (post mix/limiter), 48 kHz stereo: integrated_lufs (EBU R128, null under 0.4 s or silent, same engine call as measure_loudness), sample_peak_dbfs, true_peak_dbfs (4x sinc, not certified), rms_dbfs, crest_factor_db, dc_offset, clipping (samples at full scale, regions, risk none|near|clipped; near = peak above -1 dBFS), envelope (rms_db[] and peak_db[] per window, max_points cap with the downsample factor stated), silence regions below silence_threshold_db lasting at least min_silence_ms, leading and trailing silence. start/end restrict the range in seconds, snapped to actual PCM samples. Silence carries authoritative per-channel48k start_sample/end_sample/sample_count and ms plus inward frame_aligned cuts with exclusive ends; envelope carries integer t0_sample/dt_samples. Frame boundaries follow round(frame*48000/fps). Clip indices are target-relative, mix indices timeline-relative.",
        schema: obj(vec![("target", json!({"type":"object","description":"{clip_id} or {\"mix\":true}; default the final mix","properties":{"clip_id":{"type":"string"},"mix":{"type":"boolean"}}})), ("start", n("range start seconds, default 0")), ("end", n("range end seconds, default the end of the audio")),
            ("window_ms", n("envelope window, 10..1000, default 100")), ("max_points", n("envelope points cap, 16..2000, default 400")), ("silence_threshold_db", n("default -60 (dBFS, window RMS; user steering, ARCH spec said -50)")), ("min_silence_ms", n("default 300"))], &[]),
        run: |ss, a| {
            let pcm: Vec<f32> = match a.get("target").and_then(|t| t.get("clip_id")).and_then(|v| v.as_str()) {
                Some(id) => { let c = ss.project.find_clip(id).ok_or_else(|| format!("no clip '{id}'"))?; let Source::Media { asset } = &c.source else { return Err("no audio: media clip required".into()) };
                    let path = ss.project.resolve(&ss.project.assets.get(asset).ok_or("no asset")?.path); crate::audio::strict_decode(&path, c).map_err(|e| e.to_string())? }
                None => crate::audio::mix_strict(&ss.project).map_err(|e| e.to_string())? };
            if pcm.is_empty() { return Err("no audio in this target".into()); }
            let total_s = pcm.len() as f64 / 2.0 / SR; let st = opt_num(a, "start", 0.0); let en = opt_num(a, "end", total_s);
            if !st.is_finite() || !en.is_finite() || st < 0.0 || en <= st || st >= total_s { return Err(format!("range must satisfy 0 <= start < end and start < {:.3}", total_s)); }
            let (i0, i1) = (((st * SR) as usize) * 2, (((en.min(total_s)) * SR) as usize * 2).min(pcm.len())); let st=i0 as f64/96000.; let x = &pcm[i0..i1]; let frames = x.len() / 2; if frames == 0 { return Err("range selects no samples".into()); }
            let win_ms = opt_num(a, "window_ms", 100.0).clamp(10.0, 1000.0); let maxp = (opt_num(a, "max_points", 400.0) as usize).clamp(16, 2000); let thr = opt_num(a, "silence_threshold_db", -60.0); let min_sil = opt_num(a, "min_silence_ms", 300.0).max(0.0) / 1000.0;
            let wf = ((win_ms / 1000.0 * SR) as usize).max(1); let nwin = frames.div_ceil(wf);
            // ---- single pass: per-window sum of squares and peak, plus global peak / sum / full-scale counts
            let mut wss = vec![0f64; nwin]; let mut wpk = vec![0f32; nwin]; let (mut peak, mut sum, mut sumsq) = (0f32, 0f64, 0f64); let (mut fs_n, mut regions, mut in_fs) = (0usize, 0usize, false);
            for f in 0..frames { let (l, r) = (x[2 * f], x[2 * f + 1]); let w = f / wf; let (al, ar) = (l.abs(), r.abs()); let m = al.max(ar);
                wss[w] += (l as f64) * (l as f64) + (r as f64) * (r as f64); if m > wpk[w] { wpk[w] = m; } if m > peak { peak = m; } sum += l as f64 + r as f64; sumsq += (l as f64) * (l as f64) + (r as f64) * (r as f64);
                let clip = m >= 1.0; if clip { fs_n += 1; if !in_fs { regions += 1; } } in_fs = clip; }
            let n_s = (frames * 2) as f64; let rms = (sumsq / n_s).sqrt();
            let lufs = crate::editorial::loudness(x); let tp = crate::editorial::true_peak_4x(x)?;
            let rms_db: Vec<Option<f64>> = (0..nwin).map(|w| { let cnt = (if w == nwin - 1 { frames - w * wf } else { wf } * 2) as f64; db((wss[w] / cnt).sqrt()) }).collect();
            // silence regions on window RMS
            let mut sil: Vec<Value> = vec![]; let mut run: Option<usize> = None; let flush = |s: usize, e: usize, sil: &mut Vec<Value>| { let (t0, t1) = (s as f64 * wf as f64 / SR + st, ((e * wf).min(frames)) as f64 / SR + st); if t1 - t0 >= min_sil { let lv = (s..e).filter_map(|w| rms_db[w]).fold(f64::NEG_INFINITY, f64::max); sil.push(json!({"start": t0, "end": t1, "duration": t1 - t0, "level_db": if lv.is_finite() { json!(lv) } else { Value::Null }})); } };
            for w in 0..nwin { let quiet = rms_db[w].map(|v| v < thr).unwrap_or(true); match (quiet, run) { (true, None) => run = Some(w), (false, Some(s)) => { flush(s, w, &mut sil); run = None; } _ => {} } } if let Some(s) = run { flush(s, nwin, &mut sil); }
            for row in &mut sil{crate::perception::timebase::annotate(row,ss.project.fps,true)}
            let lead = sil.first().filter(|s| (s["start"].as_f64().unwrap() - st).abs() < 1e-9).map(|s| s["duration"].as_f64().unwrap()).unwrap_or(0.0);
            let trail = sil.last().filter(|s| (s["end"].as_f64().unwrap() - (st + frames as f64 / SR)).abs() < 1e-6).map(|s| s["duration"].as_f64().unwrap()).unwrap_or(0.0);
            // envelope downsample: max for peaks, RMS-of-RMS (energy mean) for rms
            let fac = nwin.div_ceil(maxp).max(1); let pts = nwin.div_ceil(fac);
            let env_rms: Vec<Option<f64>> = (0..pts).map(|p| { let (a0, a1) = (p * fac, ((p + 1) * fac).min(nwin)); let e: f64 = (a0..a1).map(|w| wss[w]).sum(); let c: f64 = (a0..a1).map(|w| (if w == nwin - 1 { frames - w * wf } else { wf } * 2) as f64).sum(); db((e / c).sqrt()) }).collect();
            let env_pk: Vec<Option<f64>> = (0..pts).map(|p| { let m = (p * fac..((p + 1) * fac).min(nwin)).map(|w| wpk[w]).fold(0f32, f32::max); db(m as f64) }).collect();
            let pk_db = db(peak as f64); let risk = if fs_n > 0 { "clipped" } else if pk_db.map(|v| v > -1.0).unwrap_or(false) { "near" } else { "none" };
            Ok(json!({"timebase":crate::perception::timebase::meta(ss.project.fps),"range_samples":{"start_sample":i0/2,"end_sample":i1/2,"sample_count":(i1-i0)/2},"meta": {"range": {"start": st, "end": st + frames as f64 / SR}, "sample_rate": 48000, "channels": 2, "duration_s": frames as f64 / SR, "source": if a.get("target").and_then(|t| t.get("clip_id")).is_some() { "clip post-filter pre-volume/pan" } else { "timeline post mix/limiter" }},
                "loudness": {"integrated_lufs": lufs, "lra_lu": crate::editorial::loudness_range(x), "sample_peak_dbfs": pk_db, "true_peak_dbfs": tp, "rms_dbfs": db(rms), "dc_offset": sum / n_s, "crest_factor_db": match (pk_db, db(rms)) { (Some(p), Some(r)) => Some(p - r), _ => None }},
                "envelope": {"t0_sample":i0/2,"dt_samples":fac*wf,"window_samples":wf,"window_ms": win_ms, "t0": st, "dt": fac as f64 * wf as f64 / SR, "downsample_factor": fac, "rms_db": env_rms, "peak_db": env_pk, "method": "rms: energy mean of the windows merged; peak: max of the windows merged"},
                "silence": sil, "leading_silence_s": lead, "trailing_silence_s": trail, "silence_threshold_db": thr, "min_silence_ms": min_sil * 1000.0,
                "clipping": {"samples_at_full_scale": fs_n, "regions": regions, "risk": risk}}))
        } },
] }
