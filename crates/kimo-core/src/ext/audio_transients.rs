//! RV-26: thin read-only acquisition/schema/output wrapper around audio_events.
use super::kit::*;
use crate::perception::{audio_events as ev, audio_targets};
fn number(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) {
        None => Ok(d),
        Some(v) => v
            .as_f64()
            .filter(|x| x.is_finite())
            .ok_or_else(|| crate::errors::arg_err(a, k, "number")),
    }
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let mut p = ev::Params::default();
    p.sensitivity = number(a, "sensitivity", 0.5)?;
    p.min_gap_ms = number(a, "min_gap_ms", 80.)?;
    p.max_items = match a.get("max_items") {
        None => 500,
        Some(v) => v
            .as_u64()
            .filter(|x| *x >= 1 && *x <= 5000)
            .ok_or("max_items must be 1..5000")? as usize,
    };
    p.mode = a
        .get("mode")
        .map(|v| v.as_str().ok_or("unknown mode. Valid: onset, beat"))
        .transpose()?
        .unwrap_or("onset")
        .into();
    if !["onset", "beat"].contains(&p.mode.as_str()) {
        return Err(format!("mode '{}' not allowed. Valid: onset, beat", p.mode));
    }
    let image = a
        .get("image")
        .map(|v| v.as_str().ok_or("unknown image. Valid: none, waveform"))
        .transpose()?
        .unwrap_or("none");
    if !["none", "waveform"].contains(&image) {
        return Err(format!("image '{image}' not allowed. Valid: none, waveform"));
    }
    p.band = match a.get("band") {
        None => [0., 24000.],
        Some(Value::String(s)) => match s.as_str() {
            "full" => [0., 24000.],
            "low" => [20., 250.],
            "mid" => [250., 4000.],
            "high" => [4000., 24000.],
            _ => return Err("unknown band. Valid: full, low, mid, high, [f_lo,f_hi]".into()),
        },
        Some(v) => {
            let v = v
                .as_array()
                .filter(|x| x.len() == 2)
                .ok_or("band must be full|low|mid|high or [f_lo,f_hi]")?;
            [
                v[0].as_f64().ok_or("band frequencies must be numeric")?,
                v[1].as_f64().ok_or("band frequencies must be numeric")?,
            ]
        }
    };
    if !(0.0..=1.0).contains(&p.sensitivity) {
        return Err("sensitivity must be 0..1".into());
    }
    if !(20.0..=2000.0).contains(&p.min_gap_ms) {
        return Err("min_gap_ms must be 20..2000".into());
    }
    let target = a.get("target").cloned().unwrap_or(json!("mix"));
    let decoded = audio_targets::decode(ss, &target)?;
    if decoded.pcm.is_empty() {
        return Err("no_audio: empty target".into());
    }
    let total = decoded.pcm.len() / 2;
    let duration = total as f64 / 48000.;
    let st = number(a, "start", decoded.origin)?;
    let en = number(a, "end", decoded.origin + duration)?;
    if st < decoded.origin || en <= st || st >= decoded.origin + duration {
        return Err("range_empty: start/end selects no target samples".into());
    }
    let i0 = ((st - decoded.origin) * 48000.).floor() as usize;
    let i1 = (((en - decoded.origin) * 48000.).floor() as usize).min(total);
    if i1 <= i0 {
        return Err("range_empty: no samples in range".into());
    }
    crate::guard::reserve((i1 - i0) * 4);
    let mut mono = Vec::with_capacity(i1 - i0);
    for (i, x) in decoded.pcm[i0 * 2..i1 * 2].chunks_exact(2).enumerate() {
        if i % 32768 == 0 {
            crate::guard::check();
        }
        mono.push((x[0] + x[1]) * 0.5);
    }
    let r = ev::analyze(&mono, &p)?;
    let events=r.transients.iter().map(|e|json!({"t":e.t+st,"strength":e.strength,"level_db":e.level_db,"kind":e.kind,"nearest_frame_time":((e.t+st)*ss.project.fps).round()/ss.project.fps,"frame_offset_ms":((((e.t+st)*ss.project.fps).round()/ss.project.fps)-(e.t+st))*1000.})).collect::<Vec<_>>();
    let fps = ss.project.fps;
    let times = r.transients.iter().map(|e| e.t + st).collect::<Vec<_>>();
    let nearest = times
        .iter()
        .map(|t| (t * fps).round() / fps)
        .collect::<Vec<_>>();
    let offsets = times
        .iter()
        .zip(&nearest)
        .map(|(t, n)| (n - t) * 1000.)
        .collect::<Vec<_>>();
    let mut out = json!({"params":{"target":target,"start":st,"end":st+mono.len() as f64/48000.,"sensitivity":p.sensitivity,"min_gap_ms":p.min_gap_ms,"band":p.band,"mode":p.mode,"max_items":p.max_items,"image":image,"frame_rate":fps},"method":{"sample_rate":48000,"channels":"mono arithmetic stereo mean","frame":1024,"hop":256,"window":"Hann symmetric","fft":"fixed radix-2 f64 in-tree, fixed reduction order","onset":"half-wave log(1+magnitude) spectral flux","adaptive_window_s":0.5,"threshold":"median + k*MAD + floor","k":6.-5.*p.sensitivity,"floor":0.001,"peak_pick":"local maximum; earliest refined sample anchors min_gap before threshold eligibility (monotonic sensitivity)","refinement":"maximum absolute PCM sample within +/-1024 of flux center","level":"10 ms RMS centered at envelope peak","kind":"percussive if spectral flatness > .2 else tonal","flatness_threshold":0.2,"envelope_peak_floor":0.005,"envelope_rise_ratio":1.25,"envelope_reference_ms":[-20,-10],"tempo":"onset-envelope autocorrelation 60..200 BPM, median event spacing for matching octave, keep observed 60 BPM; no downbeat inference","grid_confidence_threshold":0.5,"source":decoded.source},"duration_s":mono.len() as f64/48000.,"transients":events,"count":r.count,"truncated":r.truncated,"snap_hints":{"nearest_frame_times":nearest,"frame_offset_ms":offsets}});
    if p.mode == "beat" {
        out["bpm"] = json!(r.bpm);
        out["confidence"] = json!(r.confidence);
        out["alt_bpm"] = json!(r.alt_bpm);
        out["downbeat"] = Value::Null;
        out["beat_grid"] = json!(r
            .beat_grid
            .iter()
            .map(|b| json!({"t":b.t+st,"strength":b.strength,"phase":b.phase}))
            .collect::<Vec<_>>());
    }
    if image == "waveform" {
        // Deterministic, compact SVG visualization; no path side effect.
        let bins = 640usize.min(mono.len());
        let mut points = String::new();
        for b in 0..bins {
            let x = &mono[b * mono.len() / bins..(b + 1) * mono.len() / bins];
            let peak = x.iter().map(|v| v.abs()).fold(0f32, f32::max);
            points.push_str(&format!(
                "M{} {:.3}V{:.3}",
                b,
                50. - peak as f64 * 45.,
                50. + peak as f64 * 45.
            ));
        }
        out["waveform"] = json!({"mime_type":"image/svg+xml","width":bins,"height":100,"svg":format!("<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{bins}\" height=\"100\"><path d=\"{points}\" stroke=\"#ff5c28\"/></svg>")});
    }
    Ok(out)
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"audio_transients",description:"Native deterministic audio onset, transient and beat perception. target is clip_id, track_id, mix or a local media path; target objects {clip_id|track_id|path|mix:true} also work. start/end use absolute timeline seconds (path: source seconds). Spectral-flux onset strength, median/MAD threshold, sub-hop sample refinement, percussive/tonal flatness, relative strengths and 10ms RMS levels. Beat mode reports tempo, confidence, octave alternative, no downbeat guess and no grid on uncertain material. Snap hints use project fps. Optional waveform SVG returned without files. Read-only; legacy detect_beats unchanged.",mutates:false,schema:obj(vec![("target",json!({"description":"clip_id, track_id, mix, path or target object","oneOf":[{"type":"string"},{"type":"object"}]})),("start",n("timeline seconds; path source seconds")),("end",n("timeline seconds; path source seconds")),("sensitivity",json!({"type":"number","minimum":0,"maximum":1})),("min_gap_ms",json!({"type":"number","minimum":20,"maximum":2000})),("band",json!({"oneOf":[{"type":"string","enum":["full","low","mid","high"]},{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2}]})),("mode",json!({"type":"string","enum":["onset","beat"]})),("max_items",json!({"type":"integer","minimum":1,"maximum":5000})),("image",json!({"type":"string","enum":["none","waveform"]}))],&[]),run}]
}
