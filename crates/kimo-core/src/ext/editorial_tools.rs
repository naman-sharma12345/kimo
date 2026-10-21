//! MCP tools for captions, audio mixing and multicam editing.
use super::kit::*;
use crate::{
    editorial::*,
    keyframe::{Ease, Prop},
    model::*,
};
use std::collections::BTreeSet;
fn existing(ss: &mut Session, name: &str, a: &Value) -> Result<Value, String> {
    let c = crate::commands::find_command(name).ok_or("missing existing tool")?;
    (c.run)(ss, a)
}
fn finite(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> {
    let v = match a.get(k) {
        Some(v) => v.as_f64().ok_or(format!("{k} must be numeric"))?,
        None => d,
    };
    if !v.is_finite() || v < lo || v > hi {
        return Err(format!("{k} must be {lo}..{hi}"));
    }
    Ok(v)
}
fn ids(a: &Value, k: &str) -> Result<Vec<String>, String> {
    let v = a[k].as_array().ok_or(format!("{k} array required"))?;
    let x: Vec<String> = v
        .iter()
        .map(|v| {
            v.as_str()
                .map(String::from)
                .ok_or("string IDs required".into())
        })
        .collect::<Result<_, String>>()?;
    if x.is_empty() || x.iter().collect::<BTreeSet<_>>().len() != x.len() {
        return Err("nonempty unique IDs required".into());
    }
    Ok(x)
}
fn get<'a>(ss: &'a mut Session, a: &Value) -> Result<&'a mut Clip, String> {
    let id = str_arg(a, "clip_id")?;
    if ss
        .project
        .tracks
        .iter()
        .any(|t| t.locked && t.clips.iter().any(|c| c.id == id))
    {
        return Err("clip is on a locked track".into());
    }
    ss.project.find_clip_mut(id).ok_or("no such clip".into())
}
fn style(c: &mut Clip, name: &str, a: &Value, h: f64) -> Result<(), String> {
    let (size, col, stroke, bg) = match name {
        "subtitle" => (h / 15., [1.; 3], 2., None),
        "boxed" => (h / 15., [1.; 3], 0., Some([0., 0., 0., 0.75])),
        "yellow" => (h / 13., [1., 0.9, 0.1], 3., None),
        _ => return Err("style must be subtitle|boxed|yellow".into()),
    };
    let size = finite(a, "size", size, 1., 1000.)?;
    let y = finite(a, "y", h * 0.35, -h, h)?;
    if let Source::Text {
        size: s,
        color,
        stroke_width,
        ..
    } = &mut c.source
    {
        *s = size;
        *color = col;
        *stroke_width = stroke;
    } else {
        return Err("not a text caption".into());
    }
    c.props.insert("y".into(), Prop::new(y));
    let fx = c.text_fx.get_or_insert_with(Default::default);
    fx.bg = bg;
    fx.box_width = h * 1.65;
    Ok(())
}
fn add_captions_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let tid = str_arg(a, "track_id")?.to_string();
    if !ss.project.tracks.iter().any(|t| t.id == tid && !t.locked) {
        return Err("missing or locked track".into());
    }
    let source = str_arg(a, "source")?;
    let mut path = str_arg(a, "path")?.to_string();
    if source == "transcribe" {
        let out = str_arg(a, "out_srt")?;
        existing(
            ss,
            "transcribe",
            &json!({"path":path,"out_srt":out,"model":a.get("model").cloned().unwrap_or(json!("base"))}),
        )?;
        path = out.into();
    } else if source != "srt" {
        return Err("source must be srt|transcribe".into());
    }
    // Validate SRT before using the already-shipped importer. No silent skipped cues.
    let txt = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let normalized = txt.trim_start_matches('\u{feff}').replace('\r', "");
    let mut count = 0;
    for block in normalized.split("\n\n").filter(|s| !s.trim().is_empty()) {
        let ls: Vec<_> = block.lines().collect();
        let i = ls
            .iter()
            .position(|l| l.contains("-->"))
            .ok_or("caption cue lacks timing")?;
        let p: Vec<_> = ls[i].split("-->").collect();
        if p.len() != 2 {
            return Err("invalid timing delimiter".into());
        }
        let st = parse_time(p[0])?;
        let en = parse_time(p[1])?;
        if en <= st || ls[i + 1..].is_empty() {
            return Err("caption cue needs positive duration and text".into());
        }
        count += 1;
    }
    if count == 0 {
        return Err("no SRT cues".into());
    }
    let result = existing(
        ss,
        "import_srt",
        &json!({"track_id":tid,"path":path,"font":a.get("font").cloned().unwrap_or(json!("/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf"))}),
    )?;
    let clips: Vec<String> = result["clip_ids"]
        .as_array()
        .ok_or("importer returned no clip_ids")?
        .iter()
        .map(|v| v.as_str().unwrap().into())
        .collect();
    let name = a["style"].as_str().unwrap_or("subtitle");
    let h = ss.project.height as f64;
    for id in &clips {
        style(ss.project.find_clip_mut(id).unwrap(), name, a, h)?;
    }
    let id = ss.project.gen_id("captions");
    ss.project.editorial.captions.insert(
        id.clone(),
        CaptionGroup {
            track_id: tid,
            clips: clips.clone(),
            style: name.into(),
            burn_in: true,
            transcript_id: None,
        },
    );
    Ok(json!({"caption_id":id,"clip_ids":clips,"count":count}))
}
fn parse_time(v: &str) -> Result<f64, String> {
    let t = v.trim().replace(',', ".");
    let p: Vec<_> = t.split(':').collect();
    if p.len() != 3 {
        return Err("SRT timestamp needs HH:MM:SS,mmm".into());
    }
    let nums: Vec<f64> = p
        .iter()
        .map(|s| s.parse::<f64>().map_err(|_| "invalid timestamp".into()))
        .collect::<Result<_, String>>()?;
    if nums.iter().any(|v| !v.is_finite() || *v < 0.) || nums[1] >= 60. || nums[2] >= 60. {
        return Err("timestamp out of range".into());
    }
    Ok(nums[0] * 3600. + nums[1] * 60. + nums[2])
}
fn set_caption_style_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = str_arg(a, "caption_id")?;
    let g = ss
        .project
        .editorial
        .captions
        .get(id)
        .ok_or("no caption group")?
        .clone();
    let name = str_arg(a, "style")?;
    let h = ss.project.height as f64;
    for cid in g.clips {
        let c = ss
            .project
            .find_clip_mut(&cid)
            .ok_or("caption clip was deleted")?;
        style(c, name, a, h)?;
    }
    ss.project.editorial.captions.get_mut(id).unwrap().style = name.into();
    Ok(json!({"ok":true}))
}
fn edit_caption_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = str_arg(a, "clip_id")?;
    if !ss
        .project
        .editorial
        .captions
        .values()
        .any(|g| g.clips.iter().any(|x| x == cid))
    {
        return Err("not a caption cue".into());
    }
    let c = get(ss, a)?;
    if let Some(t) = a.get("text") {
        if let Source::Text { text, .. } = &mut c.source {
            *text = t.as_str().ok_or("text must be string")?.into();
        }
    }
    c.start = finite(a, "start", c.start, 0., 86400.)?;
    c.duration = finite(a, "duration", c.duration, 0.001, 86400.)?;
    Ok(json!({"clip_id":cid,"start":c.start,"duration":c.duration}))
}
fn burn_in_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let g = ss
        .project
        .editorial
        .captions
        .get_mut(str_arg(a, "caption_id")?)
        .ok_or("no caption group")?;
    g.burn_in = a["enabled"].as_bool().ok_or("enabled boolean required")?;
    Ok(json!({"burn_in":g.burn_in}))
}
fn list_captions(ss: &mut Session, _: &Value) -> Result<Value, String> {
    Ok(json!(ss.project.editorial.captions))
}
fn add_audio_effect_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let p = a.get("params").cloned().unwrap_or(json!({}));
    let kind = str_arg(a, "kind")?;
    if ["biquad","fir","mix_matrix"].contains(&kind){let e=crate::ext::audio_filters::parse_effect(kind,&p)?;let c=get(ss,a)?;if !matches!(c.source,Source::Media{..}){return Err("audio effects require media clip".into());}c.audio.effects.push(e);return Ok(json!({"effect_index":c.audio.effects.len()-1}));}
    let allowed: &[&str] = match kind {
        "eq" => &["frequency", "width", "gain_db"],
        "compressor" => &[
            "threshold_db",
            "ratio",
            "attack_ms",
            "release_ms",
            "makeup_db",
        ],
        "deesser" => &["intensity", "max", "frequency"],
        "reverb" => &["decay", "delay_ms"],
        "denoise" => &["reduction_db", "floor_db"],
        _ => return Err("unknown audio effect".into()),
    };
    let map = p.as_object().ok_or("params object required")?;
    for k in map.keys() {
        if !allowed.contains(&k.as_str()) {
            return Err(format!("unknown {kind} parameter {k}"));
        }
    }
    let f = |k, d, lo, hi| finite(&p, k, d, lo, hi);
    let e = match kind {
        "eq" => AudioEffect::Eq {
            frequency: f("frequency", 1000., 20., 20000.)?,
            width: f("width", 1., 0.1, 20.)?,
            gain_db: f("gain_db", 0., -24., 24.)?,
        },
        "compressor" => AudioEffect::Compressor {
            threshold_db: f("threshold_db", -18., -60., 0.)?,
            ratio: f("ratio", 3., 1., 20.)?,
            attack_ms: f("attack_ms", 10., 0.01, 2000.)?,
            release_ms: f("release_ms", 200., 0.01, 9000.)?,
            makeup_db: f("makeup_db", 0., 0., 36.)?,
        },
        "deesser" => AudioEffect::Deesser {
            intensity: f("intensity", 0.5, 0., 1.)?,
            max: f("max", 0.5, 0., 1.)?,
            frequency: f("frequency", 0.5, 0., 1.)?,
        },
        "reverb" => AudioEffect::Reverb {
            decay: f("decay", 0.3, 0., 0.8)?,
            delay_ms: f("delay_ms", 60., 6., 240.)?,
        },
        _ => AudioEffect::Denoise {
            reduction_db: f("reduction_db", 12., 0.01, 97.)?,
            floor_db: f("floor_db", -50., -80., -20.)?,
        },
    };
    let c = get(ss, a)?;
    if !matches!(c.source, Source::Media { .. }) {
        return Err("audio effects require media clip".into());
    }
    c.audio.effects.push(e);
    Ok(json!({"effect_index":c.audio.effects.len()-1}))
}
fn remove_audio_effect_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let i = a["effect_index"]
        .as_u64()
        .ok_or("effect_index integer required")? as usize;
    let c = get(ss, a)?;
    if i >= c.audio.effects.len() {
        return Err("no audio effect index".into());
    }
    c.audio.effects.remove(i);
    Ok(json!({"ok":true}))
}
fn set_audio_pan_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let pan = finite(a, "pan", 0., -1., 1.)?;
    get(ss, a)?.audio.pan = pan;
    Ok(json!({"pan":pan,"law":"stereo balance; attenuate opposite channel"}))
}
fn clip_pcm(ss: &Session, id: &str) -> Result<Vec<f32>, String> {
    let c = ss.project.find_clip(id).ok_or("no clip")?;
    let Source::Media { asset } = &c.source else {
        return Err("media clip required".into());
    };
    let path = &ss.project.resolve(&ss.project.assets.get(asset).ok_or("no asset")?.path);
    crate::audio::strict_decode(path, c).map_err(|e| e.to_string())
}
fn measure_loudness(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let pcm = if let Some(id) = a["clip_id"].as_str() {
        clip_pcm(ss, id)?
    } else {
        crate::audio::mix_strict(&ss.project).map_err(|e| e.to_string())?
    };
    let tp = match a.get("true_peak") {
        Some(v) => v.as_bool().ok_or("true_peak boolean required")?,
        None => false,
    };
    let true_peak = if tp {
        crate::editorial::true_peak_4x(&pcm)?
    } else {
        None
    };
    let l = loudness(&pcm);
    let peak = pcm.iter().map(|v| v.abs() as f64).fold(0., f64::max);
    Ok(
        json!({"integrated_lufs":l,"sample_peak_dbfs":if peak>0.{Some(20.*peak.log10())}else{None},"method":"EBU R128 stereo 48k; clip measurement post filters, before volume/pan; timeline post mix/limiter","minimum_seconds":0.4,"true_peak_dbfs":true_peak,"true_peak_method":if tp{Some("4x 64-tap Blackman-windowed sinc estimate, not certified")}else{None}}),
    )
}
fn normalize_loudness_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = str_arg(a, "clip_id")?;
    let target = finite(a, "target_lufs", -14., -36., -5.)?;
    let pcm = clip_pcm(ss, id)?;
    let before = loudness(&pcm).ok_or("silent or shorter than 400ms; cannot normalize")?;
    let delta = target - before;
    if delta.abs() > 40. {
        return Err("normalization requires over 40dB gain; refused".into());
    }
    let gain = 10f64.powf(delta / 20.);
    let peak = pcm.iter().map(|v| v.abs() as f64).fold(0., f64::max) * gain;
    if peak > 10f64.powf(-1. / 20.) {
        return Err("target exceeds -1dBFS sample headroom; lower target or compress first".into());
    }
    let c = get(ss, a)?;
    c.audio.volume = Prop::new(gain);
    Ok(
        json!({"measured_lufs":before,"target_lufs":target,"gain_db":delta,"volume_replaced":true,"note":"final mix may differ; remeasure timeline"}),
    )
}
fn auto_duck_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let track = str_arg(a, "track")?;
    let speech = match a.get("detect") {
        None => false,
        Some(v) => match v.as_str() { Some("rms") => false, Some("speech") => true, _ => return Err("detect must be \"rms\" or \"speech\"".into()) },
    };
    let hold = finite(a, "hold", 0., 0., 5.)?;
    if speech && a.get("speech_windows").is_some() {
        return Err("detect \"speech\" cannot be combined with speech_windows".into());
    }
    let under = if a.get("speech_windows").is_some() { "" } else { str_arg(a, "under")? };
    if track == under {
        return Err("tracks must differ".into());
    }
    let amount = finite(a, "amount_db", -12., -60., 0.)?;
    let threshold = if a.get("speech_windows").is_some() || speech {-35.0}else{finite(a, "threshold_db", -35., -70., 0.)?};
    let attack = finite(a, "attack", 0.1, 0.01, 2.)?;
    let release = finite(a, "release", 0.3, 0.01, 5.)?;
    if let Some(w) = a.get("speech_windows") {
        let arr = w.as_array().filter(|v| v.len() <= 1000).ok_or("speech_windows must hold 0..1000 [start,end] pairs")?;
        let mut windows = vec![];
        for (i,v) in arr.iter().enumerate() {
            let pair = v.as_array().filter(|v| v.len()==2).ok_or_else(||format!("speech_windows[{i}] must be [start,end]"))?;
            let x = pair[0].as_f64().ok_or_else(||format!("speech_windows[{i}].start must be numeric"))?;
            let y = pair[1].as_f64().ok_or_else(||format!("speech_windows[{i}].end must be numeric"))?;
            if !x.is_finite() || !y.is_finite() || x < 0.0 || y <= x { return Err(format!("speech_windows[{i}] needs finite 0 <= start < end")); }
            windows.push((x,y+hold));
        }
        let mut r = duck_windows(ss, track, windows, amount, attack, release, a.get("under").is_some())?;
        if a.get("hold").is_some() { r["hold"] = json!(hold); }
        return Ok(r);
    }
    let mut p = ss.project.clone();
    p.tracks.retain(|t| t.id == under);
    if p.tracks.is_empty() {
        return Err("no under track".into());
    }
    let pcm = crate::audio::mix_strict(&p).map_err(|e| e.to_string())?;
    if speech {
        let vad = crate::perception::vad::analyze(&pcm, &crate::perception::vad::Params::default()).map_err(|e| format!("speech detection: {e}"))?;
        let windows = vad.speech.iter().filter(|w| w.end > w.start).map(|w| (w.start, w.end + hold)).collect();
        let mut r = duck_windows(ss, track, windows, amount, attack, release, false)?;
        r["mode"] = json!("speech"); r["detector"] = json!("vad"); r["hold"] = json!(hold);
        return Ok(r);
    }
    let hop = 2400usize;
    let mut env = vec![];
    for b in pcm.chunks(hop * 2) {
        let rms = (b.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / b.len() as f64).sqrt();
        env.push(if rms > 10f64.powf(threshold / 20.) {
            10f64.powf(amount / 20.)
        } else {
            1.
        });
    }
    // hold (a compressor's hold): the duck stays down for hold seconds after the last ducked hop, then releases
    let hold_hops = (hold / 0.05).round() as usize;
    if hold_hops > 0 {
        let mut since: Option<(usize, f64)> = None; // (hops since the last ducked hop, its target)
        for v in env.iter_mut() {
            if *v < 1. { since = Some((0, *v)); }
            else if let Some((k, g)) = since { if k < hold_hops { *v = g; since = Some((k + 1, g)); } else { since = None; } }
        }
    }
    let tr = ss
        .project
        .tracks
        .iter_mut()
        .find(|t| t.id == track && !t.locked)
        .ok_or("missing/locked target track")?;
    for ca in tr.clips.iter_mut() {
        let c = &**ca;
        let mut gain = 1.;
        let mut prop = Prop::new(1.);
        for (i, target) in env.iter().enumerate() {
            let t = i as f64 * 0.05 - c.start;
            if t < 0. {
                continue;
            }
            if t > c.duration {
                break;
            }
            let tau = if *target < gain { attack } else { release };
            gain = *target + (gain - *target) * (-0.05 / tau).exp();
            prop.set_key(t, gain, Ease::Linear);
        }
        if prop.keys.iter().all(|k| (k.v - 1.0).abs() < 1e-12) { prop = Prop::new(1.0); } // unity everywhere: keep the default (keyless) envelope
        // write only when the envelope really changes, so clips outside the dialogue stay shared with the undo snapshot
        if serde_json::to_value(&ca.audio.duck_gain).ok() != serde_json::to_value(&prop).ok() { std::sync::Arc::make_mut(ca).audio.duck_gain = prop; }
    }
    let mut r = json!({"clips":tr.clips.len(),"envelope_step":0.05,"separate_from_volume":true});
    if a.get("detect").is_some() || a.get("hold").is_some() { r["mode"] = json!("rms"); r["hold"] = json!(hold); }
    Ok(r)
}
/// Explicit or detected speech windows (ends already extended by hold) -> exact relative duck gain inside the windows,
/// linear attack pre-roll / release post-roll; windows sorted/merged, then clamped to the target clip extents.
fn duck_windows(ss: &mut Session, track: &str, mut windows: Vec<(f64, f64)>, amount: f64, attack: f64, release: f64, under_ignored: bool) -> Result<Value, String> {
        windows.sort_by(|a,b|a.0.total_cmp(&b.0));
        let supplied=windows.len();let mut merged:Vec<(f64,f64)>=vec![];
        for (x,y) in windows {match merged.last_mut(){Some(l) if x<=l.1=>l.1=l.1.max(y),_=>merged.push((x,y))}}
        let merged_count=supplied-merged.len();
        let rt=ss.project.tracks.iter().find(|t|t.id==track).ok_or("missing target track")?;
        let mut spans:Vec<(f64,f64)>=rt.clips.iter().map(|c|(c.start,c.start+c.duration)).filter(|(x,y)|x<y).collect();spans.sort_by(|a,b|a.0.total_cmp(&b.0));
        let mut extents:Vec<(f64,f64)>=vec![];for (x,y) in spans{match extents.last_mut(){Some(l)if x<=l.1=>l.1=l.1.max(y),_=>extents.push((x,y))}}
        let mut windows_clamped=0usize;let mut windows=vec![];
        for (x,y) in merged {let mut unchanged=false;for &(lo,hi) in &extents{let (a,b)=(x.max(lo),y.min(hi));if a<b {windows.push((a,b));if a==x&&b==y{unchanged=true;}}}if !unchanged{windows_clamped+=1;}}
        let mut keyframes_created=0usize;
        let tr = ss.project.tracks.iter_mut().find(|t|t.id==track && !t.locked).ok_or("missing/locked target track")?;
        let gain=10f64.powf(amount/20.0);
        for clip in tr.clips.iter_mut() {
            let c=&**clip;let mut times=vec![0.0,c.duration];
            for &(x,y) in &windows { for time in [x-attack,x,y,y+release] { let local=time-c.start;if local>0.0 && local<c.duration {times.push(local);} } }
            times.sort_by(f64::total_cmp);times.dedup();
            let mut prop=Prop::new(1.0);
            for local in times { let t=c.start+local;let v=windows.iter().map(|&(x,y)| {
                if t<x-attack || t>y+release {1.0} else if t<x {1.0+(gain-1.0)*(t-(x-attack))/attack} else if t<=y {gain} else {gain+(1.0-gain)*(t-y)/release}
            }).fold(1.0,f64::min);prop.set_key(local,v,Ease::Linear); }
            if prop.keys.iter().all(|k|(k.v-1.0).abs()<1e-12) {prop=Prop::new(1.0);}
            keyframes_created+=prop.keys.len();
            if serde_json::to_value(&c.audio.duck_gain).ok()!=serde_json::to_value(&prop).ok(){std::sync::Arc::make_mut(clip).audio.duck_gain=prop;}
        }
        return Ok(json!({"clips":tr.clips.len(),"speech_windows":windows.len(),"windows_used":windows.iter().map(|(x,y)|json!([x,y])).collect::<Vec<_>>(),"merged_count":merged_count,"windows_clamped":windows_clamped,"under_ignored":under_ignored,"threshold_ignored":true,"envelope_cleared":windows.is_empty(),"keyframes_created":keyframes_created,"amount_db":amount,"separate_from_volume":true,"mode":"explicit_windows","attack":"linear pre-roll","release":"linear post-roll"}))
}
fn sync_clips_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let names = ids(a, "clip_ids")?;
    if names.len() < 2 || names.len() > 16 {
        return Err("sync requires 2..16 clips".into());
    }
    let clips: Vec<Clip> = names
        .iter()
        .map(|id| {
            ss.project
                .find_clip(id)
                .cloned()
                .ok_or("missing sync clip".into())
        })
        .collect::<Result<_, String>>()?;
    for c in &clips {
        if !matches!(c.source, Source::Media { .. })
            || ss
                .project
                .tracks
                .iter()
                .any(|t| t.locked && t.clips.iter().any(|x| x.id == c.id))
        {
            return Err("sync needs unlocked media clips".into());
        }
        if c.speed != 1. || c.speed_curve.is_some() || c.time_remap.is_some() || !matches!(c.time_mode, TimeMode::Normal) {
            return Err("sync requires normal 1x clips".into());
        }
    }
    let by = str_arg(a, "by")?;
    let mut offsets = vec![0.];
    let mut scores = vec![1.];
    if by == "timecode" {
        let values = a["timecodes"]
            .as_array()
            .ok_or("timecodes array required: source in-point SMPTE HH:MM:SS:FF, non-drop only")?;
        if values.len() != clips.len() {
            return Err("one timecode per clip required".into());
        }
        let fps = finite(a, "fps", ss.project.fps, 1., 120.)?;
        let drop = match a.get("drop_frame") {
            Some(v) => v.as_bool().ok_or("drop_frame boolean required")?,
            None => false,
        };
        let read = |v: &Value| {
            crate::timecode::seconds(v.as_str().ok_or("timecode string required")?, fps, drop)
        };
        let reference = read(&values[0])?;
        for v in &values[1..] {
            offsets.push(read(v)? - reference);
            scores.push(1.);
        }
    } else if by == "audio" {
        let window = finite(a, "window", 30., 1., 60.)?;
        let lag = finite(a, "max_offset", 5., 0.05, 10.)?;
        let min_score = finite(a, "min_correlation", 0.75, 0.1, 1.)?;
        let env = |c: &Clip| -> Result<Vec<f64>, String> {
            let Source::Media { asset } = &c.source else {
                return Err("audio sync requires media".into());
            };
            let path = &ss.project.resolve(&ss.project.assets.get(asset).ok_or("missing sync asset")?.path);
            let mut clean = c.clone();
            clean.duration = c.duration.min(window);
            clean.audio = Audio::default();
            let pcm = crate::audio::strict_decode(path, &clean).map_err(|e| e.to_string())?;
            let e: Vec<f64> = pcm
                .chunks_exact(480)
                .map(|b| (b.iter().map(|v| (*v as f64).powi(2)).sum::<f64>() / 480.).sqrt())
                .collect();
            if e.len() < 100 {
                return Err("audio sync needs at least 0.5s".into());
            }
            Ok(e)
        };
        let base = env(&clips[0])?;
        for c in &clips[1..] {
            let other = env(c)?;
            let (offset, score) = correlate(&base, &other, (lag * 200.) as i32)?;
            if score < min_score {
                return Err(format!(
                    "audio correlation {score:.3} below {min_score}; no sync applied"
                ));
            }
            offsets.push(offset as f64 / 200.);
            scores.push(score);
        }
    } else {
        return Err("by must be audio|timecode".into());
    }
    let min = offsets.iter().copied().fold(0., f64::min);
    let origin = clips[0].start;
    for (id, off) in names.iter().zip(&offsets) {
        ss.project.find_clip_mut(id).unwrap().start = origin + off - min;
    }
    Ok(
        json!({"clip_ids":names,"offsets_seconds":offsets,"correlations":scores,"origin_shift":-min,"method":by,"audio_resolution_seconds":0.005,"timecode_mode":"explicit source in-point SMPTE, drop_frame optional; no midnight wrap"}),
    )
}
/// Offset means other's sample zero occurs at this many hops after reference zero.
fn correlate(a: &[f64], b: &[f64], lag: i32) -> Result<(i32, f64), String> {
    let mut best = (0, -1.);
    let mut candidates = vec![];
    for off in -lag..=lag {
        crate::guard::check();
        let ia = off.max(0) as usize;
        let ib = (-off).max(0) as usize;
        let n = a.len().saturating_sub(ia).min(b.len().saturating_sub(ib));
        if n < 100 || n < a.len().min(b.len()) / 2 {
            continue;
        }
        let a = &a[ia..ia + n];
        let b = &b[ib..ib + n];
        let ma = a.iter().sum::<f64>() / n as f64;
        let mb = b.iter().sum::<f64>() / n as f64;
        let (mut aa, mut bb, mut ab) = (0., 0., 0.);
        for (x, y) in a.iter().zip(b) {
            let x = x - ma;
            let y = y - mb;
            aa += x * x;
            bb += y * y;
            ab += x * y;
        }
        if aa < 1e-12 || bb < 1e-12 {
            continue;
        }
        let score = ab / (aa * bb).sqrt();
        if score > best.1 {
            best = (off, score)
        }
        candidates.push((off, score));
    }
    if best.1 < 0. {
        return Err("silent/constant audio cannot sync".into());
    }
    let second = candidates
        .iter()
        .filter(|(o, _)| (o - best.0).abs() > 10)
        .map(|(_, s)| *s)
        .fold(-1., f64::max);
    if best.1 - second < 0.03 {
        return Err("ambiguous audio sync: repeating envelope; no sync applied".into());
    }
    Ok(best)
}
fn rebuild(ss: &mut Session, id: &str) -> Result<(), String> {
    let g = ss
        .project
        .editorial
        .multicams
        .get(id)
        .ok_or("no multicam")?
        .clone();
    let mut made = vec![];
    for (i, (t, angle)) in g.switches.iter().enumerate() {
        let end = g.switches.get(i + 1).map(|v| v.0).unwrap_or(g.duration);
        let mut c = g.angles[*angle].clone();
        c.id = ss.project.gen_id("clip");
        c.start = g.start + t;
        c.duration = end - t;
        c.in_point += g.start + t - g.angles[*angle].start;
        c.audio.mute = true;
        made.push(c);
    }
    let mut audio = g.angles[g.audio_angle].clone();
    audio.id = ss.project.gen_id("clip");
    audio.in_point += g.start - audio.start;
    audio.start = g.start;
    audio.duration = g.duration;
    audio.props.insert("opacity".into(), Prop::new(0.));
    made.push(audio);
    ss.project
        .tracks
        .iter_mut()
        .find(|t| t.id == g.track_id)
        .ok_or("multicam track deleted")?
        .clips = made.into_iter().map(std::sync::Arc::new).collect();
    Ok(())
}
fn add_multicam_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let names = ids(a, "angles")?;
    if names.len() < 2 || names.len() > 16 {
        return Err("2..16 angles required".into());
    }
    let angles: Vec<Clip> = names
        .iter()
        .map(|id| {
            ss.project
                .find_clip(id)
                .cloned()
                .ok_or("missing angle".into())
        })
        .collect::<Result<_, String>>()?;
    for c in &angles {
        if !matches!(c.source, Source::Media { .. })
            || c.speed != 1.
            || c.speed_curve.is_some()
            || c.time_remap.is_some()
            || !matches!(c.time_mode, TimeMode::Normal)
        {
            return Err("angles need normal 1x media clips; synchronize first".into());
        }
        if ss
            .project
            .tracks
            .iter()
            .any(|t| t.locked && t.clips.iter().any(|x| x.id == c.id))
        {
            return Err("angle on locked track".into());
        }
    }
    let start = angles.iter().map(|c| c.start).fold(0., f64::max);
    let end = angles
        .iter()
        .map(|c| c.start + c.duration)
        .fold(f64::INFINITY, f64::min);
    if end <= start {
        return Err("angles have no common synchronized interval".into());
    }
    let audio_angle = match a.get("audio_angle") {
        Some(v) => v
            .as_u64()
            .ok_or("audio_angle must be a nonnegative integer")?,
        None => 0,
    } as usize;
    if audio_angle >= angles.len() {
        return Err("audio_angle out of range".into());
    }
    let id = ss.project.gen_id("multicam");
    let tid = ss.project.gen_id("track");
    ss.project.editorial.multicams.insert(
        id.clone(),
        Multicam {
            track_id: tid.clone(),
            angles,
            switches: vec![(0., 0)],
            audio_angle,
            start,
            duration: end - start,
        },
    );
    for t in &mut ss.project.tracks {
        t.clips.retain(|c| !names.contains(&c.id))
    }
    ss.project.tracks.push(Track {
        id: tid.clone(),
        clips: vec![],
        locked: false,
        muted: false,
        name: id.clone(),
    });
    rebuild(ss, &id)?;
    Ok(
        json!({"multicam_id":id,"track_id":tid,"start":start,"duration":end-start,"source_clips_moved":true,"audio_angle":audio_angle}),
    )
}
fn switch_angle_inner(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = str_arg(a, "multicam_id")?;
    let t = num_arg(a, "t")?;
    let angle = a["angle"]
        .as_u64()
        .ok_or("zero-based angle integer required")? as usize;
    let g = ss
        .project
        .editorial
        .multicams
        .get_mut(id)
        .ok_or("no multicam")?;
    if angle >= g.angles.len() || t < g.start || t >= g.start + g.duration || !t.is_finite() {
        return Err("angle or timeline time out of range".into());
    }
    if ss
        .project
        .tracks
        .iter()
        .any(|track| track.id == g.track_id && track.locked)
    {
        return Err("multicam track is locked".into());
    }
    let lt = t - g.start;
    g.switches.retain(|v| (v.0 - lt).abs() > 1e-8);
    g.switches.push((lt, angle));
    g.switches.sort_by(|a, b| a.0.total_cmp(&b.0));
    rebuild(ss, id)?;
    Ok(json!({"ok":true,"audio":"continuous fixed audio angle"}))
}
fn list_multicam(ss: &mut Session, _: &Value) -> Result<Value, String> {
    Ok(json!(ss.project.editorial.multicams))
}
pub fn commands() -> Vec<Command> {
    vec![
 Command{name:"add_captions",description:"Create editable timed caption group using existing SRT importer/transcriber. Presets subtitle|boxed|yellow. Burn-in on by default.",mutates:true,schema:obj(vec![("track_id",s("unlocked track")),("source",json!({"type":"string","enum":["srt","transcribe"]})),("path",s("SRT or media path")),("out_srt",s("required for transcribe")),("model",s("Whisper model")),("style",s("subtitle|boxed|yellow")),("font",s("TTF path")),("size",n("pixel size")),("y",n("vertical offset pixels"))],&["track_id","source","path"]),run:add_captions},
 Command{name:"list_captions",description:"List editable caption groups and cue clip IDs.",mutates:false,schema:obj(vec![],&[]),run:list_captions},
 Command{name:"set_caption_style",description:"Apply caption preset to every cue; optional size/y overrides.",mutates:true,schema:obj(vec![("caption_id",s("group")),("style",s("subtitle|boxed|yellow")),("size",n("px")),("y",n("px"))],&["caption_id","style"]),run:set_caption_style},
 Command{name:"edit_caption",description:"Edit caption text, timeline start, or duration.",mutates:true,schema:obj(vec![("clip_id",s("cue")),("text",s("caption text")),("start",n("timeline seconds")),("duration",n("seconds"))],&["clip_id"]),run:edit_caption},
 Command{name:"burn_in",description:"Enable/disable caption group's presence in preview and exports without deleting cues.",mutates:true,schema:obj(vec![("caption_id",s("group")),("enabled",json!({"type":"boolean"}))],&["caption_id","enabled"]),run:burn_in},
 Command{name:"add_audio_effect",description:"Append validated EQ/compressor/deesser/reverb/denoise or raw biquad (DFII transposed f64 state), fir and stereo mix_matrix. Raw defaults identity; no normalization/clamping. Render/export uses validated FFmpeg filters plus CPU reverb. Reverb is stereo damped-comb/allpass Schroeder; tail cropped to clip duration, reverb applied after FFmpeg effects.",mutates:true,schema:obj(vec![("clip_id",s("media clip")),("kind",json!({"type":"string","enum":["eq","compressor","deesser","reverb","denoise","biquad","fir","mix_matrix"]})),("params",json!({"type":"object","additionalProperties":true}))],&["clip_id","kind"]),run:add_audio_effect},
 Command{name:"remove_audio_effect",description:"Remove effect by zero-based index.",mutates:true,schema:obj(vec![("clip_id",s("clip")),("effect_index",json!({"type":"integer","minimum":0}))],&["clip_id","effect_index"]),run:remove_audio_effect},
 Command{name:"set_audio_pan",description:"Stereo balance -1 left to +1 right; attenuates opposite channel.",mutates:true,schema:obj(vec![("clip_id",s("clip")),("pan",n("-1..1"))],&["clip_id","pan"]),run:set_audio_pan},
 Command{name:"measure_loudness",description:"EBU R128 integrated stereo LUFS and sample peak. Optional clip_id measures decoded post filters/pre gain; absent measures final timeline mix. Silence or <400ms LUFS returns null. true_peak=true adds 4x sinc reconstruction peak estimate (not certified).",mutates:false,schema:obj(vec![("clip_id",s("optional clip")),("true_peak",json!({"type":"boolean","default":false}))],&[]),run:measure_loudness},
 Command{name:"normalize_loudness",description:"Replace clip volume with measured loudness gain. Rejects sample peaks over -1dBFS; target -36..-5 LUFS. Remeasure final timeline after mix.",mutates:true,schema:obj(vec![("clip_id",s("clip")),("target_lufs",n("default -14"))],&["clip_id"]),run:normalize_loudness},
 Command{name:"auto_duck",description:"Duck a music track under dialogue: detect rms or speech (VAD) on the under track, or explicit speech_windows; amount_db, attack/release/hold s.",mutates:true,schema:obj(vec![("track",s("target music track")),("under",s("dialogue track for RMS detection; omit with speech_windows")),("speech_windows",json!({"type":"array","items":{"type":"array","items":{"type":"number"},"minItems":2,"maxItems":2},"description":"Optional 0..1000 ABSOLUTE timeline-second [start,end] speech windows; sorted/merged, clamped to TARGET clip extents. under and threshold_db are ignored if supplied. Empty list removes the previous duck envelope (unity), never RMS fallback. No RMS decode; exact relative gain during windows, linear attack pre-roll/release post-roll, preserves volume"})),("amount_db",n("-60..0 default -12")),("threshold_db",n("default -35")),("attack",n("seconds default .1")),("release",n("seconds default .3")),("detect",json!({"type":"string","enum":["rms","speech"],"description":"under-track detection; speech = VAD, ignores room tone/hiss; default rms"})),("hold",n("seconds 0..5 the duck stays down after the voice stops; default 0"))],&["track"]),run:auto_duck},
 Command{name:"sync_clips",description:"Align 1x media clips by bounded RMS audio correlation (5ms resolution; rejects silence and ambiguous repeats), or explicit source in-point SMPTE timecodes, optional drop_frame at 29.97/59.94. Shifts starts, preserves source in-points.",mutates:true,schema:obj(vec![("clip_ids",json!({"type":"array","items":{"type":"string"},"minItems":2,"maxItems":16})),("by",json!({"type":"string","enum":["audio","timecode"]})),("timecodes",json!({"type":"array","items":{"type":"string"}})),("fps",n("integer non-drop or 30000/1001,60000/1001 drop")),("drop_frame",json!({"type":"boolean","default":false})),("window",n("1..60s default30")),("max_offset",n(".05..10s default5")),("min_correlation",n("default .75"))],&["clip_ids","by"]),run:sync_clips},
 Command{name:"add_multicam",description:"Move 2..16 synchronized media clips into persistent multicam group; common interval, angle zero video, fixed audio angle. Generates normal timeline clips for all exports.",mutates:true,schema:obj(vec![("angles",json!({"type":"array","items":{"type":"string"}})),("audio_angle",json!({"type":"integer","minimum":0}))],&["angles"]),run:add_multicam},
 Command{name:"switch_angle",description:"Switch multicam video at absolute timeline seconds t, zero-based angle; continuous audio remains fixed. Rebuilds group track, replacing manual edits there.",mutates:true,schema:obj(vec![("multicam_id",s("group")),("t",n("absolute timeline seconds")),("angle",json!({"type":"integer","minimum":0}))],&["multicam_id","t","angle"]),run:switch_angle},
 Command{name:"list_multicam",description:"Read multicam groups, angle recipes and switches.",mutates:false,schema:obj(vec![],&[]),run:list_multicam},
]
}

fn add_captions(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match add_captions_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn set_caption_style(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match set_caption_style_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn edit_caption(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match edit_caption_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn burn_in(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match burn_in_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn add_audio_effect(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match add_audio_effect_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn remove_audio_effect(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match remove_audio_effect_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn set_audio_pan(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match set_audio_pan_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn normalize_loudness(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match normalize_loudness_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn auto_duck(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match auto_duck_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn sync_clips(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match sync_clips_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn add_multicam(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match add_multicam_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}

fn switch_angle(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let before = ss.project.clone();
    match switch_angle_inner(ss, a) {
        Ok(v) => Ok(v),
        Err(e) => {
            ss.project = before;
            Err(e)
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::dispatch;
    fn call(s: &mut Session, n: &str, a: Value) -> Value {
        dispatch(s, n, &a).unwrap()
    }
    fn track(s: &mut Session) -> String {
        call(s, "add_track", json!({}))["track_id"]
            .as_str()
            .unwrap()
            .into()
    }
    fn media(s: &mut Session, t: &str, path: &str, start: f64, duration: f64) -> String {
        let a = call(s, "import_media", json!({"path":path}))["asset_id"].clone();
        call(s,"add_clip",json!({"track_id":t,"source_type":"media","asset_id":a,"start":start,"duration":duration}))["clip_id"].as_str().unwrap().into()
    }
    fn fixture(name: &str, src: &str) -> String {
        let p = format!("/tmp/kimo-gap4-{}-{name}.wav", std::process::id());
        assert!(std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i", src, "-ac", "2", &p])
            .status()
            .unwrap()
            .success());
        p
    }
    #[test]
    fn caption_timing_style_edit_burnin_undo_and_reload() {
        let mut s = Session::new();
        let t = track(&mut s);
        let path = format!("/tmp/gap4-caption-{}.srt", std::process::id());
        std::fs::write(&path,"1\n00:00:00,000 --> 00:00:01,500\nHello Kimo\n\n2\n00:00:01,500 --> 00:00:03,000\nSecond line\n").unwrap();
        let v = call(
            &mut s,
            "add_captions",
            json!({"track_id":t,"source":"srt","path":path,"style":"boxed"}),
        );
        let group = v["caption_id"].as_str().unwrap();
        let cue = v["clip_ids"][0].as_str().unwrap();
        assert_eq!(s.project.find_clip(cue).unwrap().duration, 1.5);
        let undo = s.undo.len();
        call(
            &mut s,
            "edit_caption",
            json!({"clip_id":cue,"text":"Edited","start":0.25,"duration":1.}),
        );
        assert_eq!(s.undo.len(), undo + 1);
        call(
            &mut s,
            "burn_in",
            json!({"caption_id":group,"enabled":false}),
        );
        assert!(!crate::editorial::caption_visible(&s.project, cue));
        let saved = serde_json::to_string(&s.project).unwrap();
        let reloaded: Project = serde_json::from_str(&saved).unwrap();
        assert!(!crate::editorial::caption_visible(&reloaded, cue));
        call(&mut s, "undo", json!({}));
        assert!(crate::editorial::caption_visible(&s.project, cue));
        let before = serde_json::to_value(&s.project).unwrap();
        assert!(dispatch(
            &mut s,
            "set_caption_style",
            &json!({"caption_id":group,"style":"nope"})
        )
        .is_err());
        assert_eq!(serde_json::to_value(&s.project).unwrap(), before);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn audio_effects_execute_pan_and_loudness_matches_ffmpeg() {
        let path = fixture("tone", "sine=frequency=1000:sample_rate=48000:duration=2");
        let mut s = Session::new();
        let t = track(&mut s);
        let c = media(&mut s, &t, &path, 0., 2.);
        let measured = call(&mut s, "measure_loudness", json!({"clip_id":c}))["integrated_lufs"]
            .as_f64()
            .unwrap();
        let out = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "info",
                "-i",
                &path,
                "-af",
                "loudnorm=I=-14:print_format=json",
                "-f",
                "null",
                "-",
            ])
            .output()
            .unwrap();
        let text = String::from_utf8_lossy(&out.stderr);
        let j: Value = serde_json::from_str(&text[text.rfind('{').unwrap()..]).unwrap();
        let expected = j["input_i"].as_str().unwrap().parse::<f64>().unwrap();
        assert!(
            (measured - expected).abs() < 0.15,
            "ours {measured}, ffmpeg {expected}"
        );
        for kind in ["eq", "compressor", "deesser", "reverb", "denoise"] {
            call(&mut s, "add_audio_effect", json!({"clip_id":c,"kind":kind}));
            let pcm = clip_pcm(&s, &c).unwrap();
            assert!(!pcm.is_empty());
            assert!(pcm.iter().all(|v| v.is_finite()));
        }
        call(&mut s, "set_audio_pan", json!({"clip_id":c,"pan":-1.}));
        let pcm = crate::audio::mix_strict(&s.project).unwrap();
        assert!(pcm.chunks_exact(2).all(|v| v[1] == 0.));
        assert!(pcm.iter().any(|v| *v != 0.));
        let before = serde_json::to_value(&s.project).unwrap();
        assert!(dispatch(
            &mut s,
            "add_audio_effect",
            &json!({"clip_id":c,"kind":"eq","params":{"frequency":0.}})
        )
        .is_err());
        assert_eq!(serde_json::to_value(&s.project).unwrap(), before);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn normalize_hits_target_and_preserves_silence_errors() {
        let path = fixture(
            "normalize",
            "sine=frequency=440:sample_rate=48000:duration=2",
        );
        let mut s = Session::new();
        let t = track(&mut s);
        let c = media(&mut s, &t, &path, 0., 2.);
        call(
            &mut s,
            "normalize_loudness",
            json!({"clip_id":c,"target_lufs":-18.}),
        );
        let l = call(&mut s, "measure_loudness", json!({}))["integrated_lufs"]
            .as_f64()
            .unwrap();
        assert!((l + 18.).abs() < 0.05);
        std::fs::remove_file(path).unwrap();
        assert!(loudness(&vec![0.; 48000]).is_none());
        assert!(loudness(&[0.; 2]).is_none());
    }
    #[test]
    fn multicam_timecode_switch_audio_and_roundtrip() {
        let path = fixture(
            "multicam",
            "sine=frequency=440:sample_rate=48000:duration=3",
        );
        let mut s = Session::new();
        let t1 = track(&mut s);
        let t2 = track(&mut s);
        let a = media(&mut s, &t1, &path, 0., 3.);
        let b = media(&mut s, &t2, &path, 0., 3.);
        call(
            &mut s,
            "sync_clips",
            json!({"clip_ids":[a,b],"by":"timecode","fps":30,"timecodes":["01:00:00:00","01:00:00:15"]}),
        );
        assert_eq!(s.project.find_clip(&b).unwrap().start, 0.5);
        let m = call(&mut s, "add_multicam", json!({"angles":[a,b]}));
        let id = m["multicam_id"].as_str().unwrap();
        call(
            &mut s,
            "switch_angle",
            json!({"multicam_id":id,"t":1.5,"angle":1}),
        );
        let g = &s.project.editorial.multicams[id];
        assert_eq!(g.switches, vec![(0., 0), (1., 1)]);
        let tr = s
            .project
            .tracks
            .iter()
            .find(|t| t.id == g.track_id)
            .unwrap();
        assert_eq!(tr.clips.len(), 3);
        assert_eq!(tr.clips[0].in_point, 0.5);
        assert_eq!(tr.clips[1].in_point, 1.);
        assert!(tr.clips[..2].iter().all(|c| c.audio.mute));
        assert!(!tr.clips[2].audio.mute);
        let pcm = crate::audio::mix_strict(&s.project).unwrap();
        assert!(pcm.iter().any(|v| *v != 0.));
        let p: Project = serde_json::from_str(&serde_json::to_string(&s.project).unwrap()).unwrap();
        assert_eq!(p.editorial.multicams[id].switches, g.switches);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn correlation_sign_and_ambiguous_rejection() {
        let a: Vec<f64> = (0..1000)
            .map(|i| ((i as f64 * 12.9898).sin() * 43758.5453).fract().abs())
            .collect();
        let b = a[67..].to_vec();
        let (offset, score) = correlate(&a, &b, 100).unwrap();
        assert_eq!(offset, 67);
        assert!(score > 0.999);
        assert!(correlate(&vec![1.; 1000], &vec![1.; 1000], 100).is_err());
        let periodic: Vec<f64> = (0..1000).map(|i| (i % 20) as f64).collect();
        assert!(correlate(&periodic, &periodic, 100).is_err());
    }
    #[test]
    fn ducking_separate_envelope_reduces_music() {
        let path = fixture("duck", "sine=frequency=400:sample_rate=48000:duration=2");
        let mut s = Session::new();
        let music = track(&mut s);
        let speech = track(&mut s);
        let c = media(&mut s, &music, &path, 0., 2.);
        media(&mut s, &speech, &path, 0.5, 1.);
        call(
            &mut s,
            "auto_duck",
            json!({"track":music,"under":speech,"threshold_db":-40.}),
        );
        let c = s.project.find_clip(&c).unwrap();
        assert_eq!(c.audio.volume.base, 1.);
        assert!(c.audio.duck_gain.at(1.) < 0.3);
        assert!(c.audio.duck_gain.at(0.1) > 0.99);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn registry_names_unique_and_old_project_compatible() {
        let reg = crate::commands::registry();
        let names: Vec<_> = reg.iter().map(|c| c.name).collect();
        assert_eq!(names.iter().collect::<BTreeSet<_>>().len(), names.len());
        let mut v = serde_json::to_value(Project::default()).unwrap();
        v.as_object_mut().unwrap().remove("editorial");
        let p: Project = serde_json::from_value(v).unwrap();
        assert!(p.editorial.captions.is_empty());
    }
}
#[cfg(test)]
mod more_tests {
    use super::*;
    use crate::commands::dispatch;
    #[test]
    fn schroeder_impulse_diffuse_and_finite() {
        let mut pcm = vec![0.; 48000 * 2];
        pcm[0] = 1.;
        pcm[1] = 1.;
        crate::editorial::reverb(&mut pcm, 0.7, 60.);
        assert!(pcm.iter().all(|v| v.is_finite()));
        let hits = pcm.iter().skip(4000).filter(|v| v.abs() > 1e-5).count();
        assert!(hits > 1000, "diffusion: {hits}");
        assert!(pcm.chunks_exact(2).any(|s| (s[0] - s[1]).abs() > 0.001));
    }
    #[test]
    fn sync_actual_modulated_audio() {
        let path = format!("/tmp/gap4-sync-{}.wav", std::process::id());
        let pcm: Vec<f32> = (0..48000 * 3)
            .flat_map(|i| {
                let hop = i / 240;
                let amplitude =
                    ((hop as f64 * 12.9898).sin() * 43758.5453).fract().abs() as f32 * 0.5;
                let x = (i as f32 * std::f32::consts::TAU * 440. / 48000.).sin() * amplitude;
                [x, x]
            })
            .collect();
        crate::audio::write_wav(&path, &pcm).unwrap();
        let mut s = Session::new();
        let t = dispatch(&mut s, "add_track", &json!({})).unwrap()["track_id"].clone();
        let asset =
            dispatch(&mut s, "import_media", &json!({"path":path})).unwrap()["asset_id"].clone();
        let mut ids = vec![];
        for ip in [0., 0.35] {
            ids.push(dispatch(&mut s,"add_clip",&json!({"track_id":t,"source_type":"media","asset_id":asset,"start":0.,"duration":2.,"in_point":ip})).unwrap()["clip_id"].clone());
        }
        let v = dispatch(
            &mut s,
            "sync_clips",
            &json!({"clip_ids":ids,"by":"audio","max_offset":0.5}),
        )
        .unwrap();
        assert!((v["offsets_seconds"][1].as_f64().unwrap() - 0.35).abs() <= 0.005);
        std::fs::remove_file(path).unwrap();
    }
}
