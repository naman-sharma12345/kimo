//! V1c-D bg_wave: a waveform / spectrum / data line background, a PURE expansion into add_path_shape (+ add_path_key morph keys, optional glow effect and pulse keys)
//! + the common finish ops. Path keys are linear in time and capped at 256 per clip (the first key at t=0 is the base), so audio mode uses at most 255 keys per clip and
//! chunk:true splits into back-to-back clips that share one boundary sample. Coordinates in the call are canvas px from the top-left; paths are stored from the canvas centre.
use super::kit::*;
use super::bg_grid::n;
use super::bgcommon::{self as bg, bg_err, num, Common, E_AUDIO_RATE, E_KEYCAP, E_RANGE};

/// A thin stroke blurred over a 2*radius window keeps only about stroke/(2*radius) of its alpha, so a raw intensity gives an invisible halo; scale it so intensity is the halo strength next to the line.
fn gscale(rad: f64, stroke: f64) -> f64 { (2.0 * rad / stroke.max(0.5)).max(1.0) }
const OWN: &[&str] = &["source", "points", "values", "range", "audio", "box", "smooth", "tension", "closed", "stroke_px", "color", "fill_under", "glow", "pulse", "ease", "ceiling", "gradient"];
const KEY_CAP: usize = 255;
/// Chunk plan over `n` samples: inclusive (first, last) sample indices, <= 255 samples each, consecutive chunks sharing the boundary sample, every sample covered.
pub(crate) fn chunk_plan(n: usize, chunk: bool) -> Vec<(usize, usize)> {
    if n <= KEY_CAP || !chunk { return vec![(0, n.saturating_sub(1))]; }
    let step = KEY_CAP - 1; let c = (n - 1 + step - 1) / step; (0..c).map(|k| (k * step, ((k + 1) * step).min(n - 1))).collect()
}
/// Band levels 0..1 at a window centred on sample `c`: Hann window of 2048, log-spaced band centres 40 Hz..min(16 kHz, 0.45 rate), Goertzel magnitudes.
pub(crate) fn band_levels(x: &[f32], rate: f64, c: usize, bands: usize, db: bool, floor_db: f64) -> Vec<f64> {
    const W: usize = 2048; let lo = c as i64 - (W / 2) as i64; let top = (16000.0f64).min(rate * 0.45); let mut out = Vec::with_capacity(bands);
    let win: Vec<f64> = (0..W).map(|i| { let v = x.get((lo + i as i64).max(0) as usize).copied().unwrap_or(0.0) as f64; if lo + (i as i64) < 0 { 0.0 } else { v * (0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (W - 1) as f64).cos()) } }).collect();
    for b in 0..bands { let f = 40.0 * (top / 40.0).powf(b as f64 / (bands.max(2) - 1) as f64); let w = 2.0 * std::f64::consts::PI * f / rate; let co = 2.0 * w.cos(); let (mut s1, mut s2) = (0.0f64, 0.0f64);
        for v in &win { let s = v + co * s1 - s2; s2 = s1; s1 = s; }
        let mag = (s1 * s1 + s2 * s2 - co * s1 * s2).max(0.0).sqrt() / (W as f64 / 4.0);
        out.push(if db { let d = 20.0 * mag.max(1e-12).log10(); ((d - floor_db) / -floor_db).clamp(0.0, 1.0) } else { mag.clamp(0.0, 1.0) }); }
    out
}
fn rgb(v: Option<&Value>, d: [f64; 3], what: &str, call: &str) -> Result<[f64; 3], String> {
    let Some(v) = v.filter(|v| !v.is_null()) else { return Ok(d) };
    let arr = v.as_array().filter(|x| x.len() == 3).ok_or_else(|| bg_err(E_RANGE, &format!("{what} must be [r, g, b] with each 0..1 (got {v})"), call))?;
    let mut o = [0.0; 3]; for (i, c) in arr.iter().enumerate() { o[i] = c.as_f64().filter(|f| f.is_finite() && (0.0..=1.0).contains(f)).ok_or_else(|| bg_err(E_RANGE, &format!("{what}[{i}] must be a number 0..1 (got {c})"), call))?; } Ok(o)
}
fn pt_list(v: &Value, call: &str) -> Result<Vec<[f64; 2]>, String> {
    let arr = v.as_array().ok_or_else(|| bg_err(E_RANGE, "points must be [[x, y], ...]", call))?;
    if arr.len() < 2 || arr.len() > 256 { return Err(bg_err(E_RANGE, &format!("points needs 2..256 entries (got {})", arr.len()), call)); }
    arr.iter().map(|p| { let p = p.as_array().filter(|p| p.len() == 2).ok_or_else(|| bg_err(E_RANGE, "each point must be [x, y]", call))?;
        let g = |i: usize| p[i].as_f64().filter(|f| f.is_finite() && f.abs() <= 1e6).ok_or_else(|| bg_err(E_RANGE, &format!("point coordinates must be finite numbers within +-1e6 (got {})", p[i]), call)); Ok([g(0)?, g(1)?]) }).collect()
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let c = Common::parse(ss, a, "bg_wave", "wave", OWN)?; let call = |s: &str| format!("bg_wave({s})");
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);
    let source = a.get("source").and_then(|v| v.as_str()).filter(|s| ["points", "values", "audio"].contains(s)).ok_or_else(|| bg_err(E_RANGE, "source is required: points, values or audio", &call("source=\"values\", values=[0,0.5,1]")))?;
    if a.get("closed").map_or(false, |v| !v.is_null() && v != &json!(false)) { return Err(bg_err(E_RANGE, "closed paths are not supported", &call("closed=false"))); }
    let smooth = match a.get("smooth").filter(|v| !v.is_null()) { None => true, Some(v) => v.as_bool().ok_or_else(|| bg_err(E_RANGE, "smooth must be a boolean", &call("smooth=true")))? };
    let tension = num(a, "tension", 0.5, 0.0, 1.0, &call("tension=0.5"))?; let stroke = num(a, "stroke_px", 3.0, 0.5, 64.0, &call("stroke_px=3"))?;
    let color = rgb(a.get("color"), [1.0, 1.0, 1.0], "color", &call("color=[1,1,1]"))?;
    let ease = match a.get("ease").filter(|v| !v.is_null()) { None => "linear", Some(v) => v.as_str().filter(|s| ["linear", "in", "out", "in_out", "hold"].contains(s)).ok_or_else(|| bg_err(E_RANGE, &format!("ease must be linear, in, out, in_out or hold (got {v})"), &call("ease=\"linear\"")))? };
    if a.get("fill_under").map_or(false, |v| !v.is_null()) { return Err(bg_err(E_RANGE, "fill_under is not implemented in this version; leave it null", &call("fill_under=null"))); }
    let bx = match a.get("box").filter(|v| !v.is_null()) { None => [cw * 0.1, ch * 0.1, cw * 0.8, ch * 0.8], Some(v) => { let arr = v.as_array().filter(|x| x.len() == 4).ok_or_else(|| bg_err(E_RANGE, "box must be [x, y, w, h]", &call("box=[0,0,1920,1080]")))?;
        let mut o = [0.0; 4]; for (i, q) in arr.iter().enumerate() { o[i] = q.as_f64().filter(|f| f.is_finite() && f.abs() <= 1e6 && (i < 2 || *f > 0.0)).ok_or_else(|| bg_err(E_RANGE, &format!("box[{i}] must be a finite number (w and h > 0), got {q}"), &call("box=[0,0,1920,1080]")))?; } o } };
    let glow = match a.get("glow").filter(|v| !v.is_null()) { None => None, Some(g) => { let gc = |s: &str| call(&format!("glow={{{s}}}")); g.as_object().ok_or_else(|| bg_err(E_RANGE, "glow must be {radius_px, intensity}", &gc("radius_px:12,intensity:0.6")))?;
        Some((num(g, "radius_px", 12.0, 0.0, 256.0, &gc("radius_px=12"))?, num(g, "intensity", 0.6, 0.0, 4.0, &gc("intensity=0.6"))?)) } };
    // glow is three scales (tight r/3, mid r, wide 2.5r) with energy weights 0.5 / 0.3 / 0.2; glow.colors = up to 3 [r,g,b] (tight, mid, wide), default the stroke colour
    let glow_cols: [[f64; 3]; 3] = { let mut cols = [color; 3]; if let Some(cs) = a.get("glow").and_then(|g| g.get("colors")).filter(|v| !v.is_null()) { let arr = cs.as_array().filter(|x| !x.is_empty() && x.len() <= 3).ok_or_else(|| bg_err(E_RANGE, &format!("glow.colors must be 1 to 3 [r,g,b] entries (tight, mid, wide), got {cs}"), &call("glow={radius_px:24,intensity:0.6,colors:[[1,1,1],[0.3,0.6,1],[0.4,0.2,1]]}")))?; for (i, v) in arr.iter().enumerate() { cols[i] = rgb(Some(v), color, "glow.colors entry", &call("glow.colors=[[1,0.8,0.4]]"))?; } for i in arr.len()..3 { cols[i] = cols[arr.len() - 1]; } } cols };
    // gradient: {from, to, angle_deg} blends the stroke colour along the layer (two stops; 3 to 5 stops need engine item E2). With no glow.colors the glow scales take from, mid and to.
    let gradient: Option<([f64; 3], [f64; 3], f64)> = match a.get("gradient").filter(|v| !v.is_null()) { Some(Value::Bool(false)) => None, None => Some(([0.35, 0.85, 1.0], [0.75, 0.4, 1.0], 0.0)), Some(g) => { let gc = |s: &str| call(&format!("gradient={{{s}}}")); let o = g.as_object().ok_or_else(|| bg_err(E_RANGE, "gradient must be {from:[r,g,b], to:[r,g,b], angle_deg}", &gc("from:[0.35,0.8,1],to:[0.7,0.35,1],angle_deg:0")))?;
        for k in o.keys() { if !["from", "to", "angle_deg"].contains(&k.as_str()) { return Err(bg_err(super::bgcommon::E_UNKNOWN, &format!("unknown gradient property '{k}'. Valid: from, to, angle_deg"), &gc("from:[0.35,0.8,1],to:[0.7,0.35,1],angle_deg:0"))); } }
        Some((rgb(g.get("from"), color, "gradient.from", &gc("from:[0.35,0.8,1]"))?, rgb(g.get("to"), color, "gradient.to", &gc("to:[0.7,0.35,1]"))?, num(g, "angle_deg", 0.0, -360.0, 360.0, &gc("angle_deg=0"))?)) } };
    let glow_follow: f64 = if gradient.is_some() && a.get("glow").and_then(|g| g.get("colors")).filter(|v| !v.is_null()).is_none() { 1.0 } else { 0.0 }; // the halo follows the local stroke colour unless glow.colors pins it
    let acap: f64 = if glow_follow > 0.0 { 10.0 } else { 4.0 }; // a halo that follows the stroke colour is dimmer than a pinned bright one, so its amount may go to 10 (a thin line keeps only stroke/(2r) of its alpha)
    let glow_cols = if let (Some((f, t, _)), None) = (gradient, a.get("glow").and_then(|g| g.get("colors")).filter(|v| !v.is_null())) { [f, [(f[0] + t[0]) / 2.0, (f[1] + t[1]) / 2.0, (f[2] + t[2]) / 2.0], t] } else { glow_cols };
    let ceiling = num(a, "ceiling", 0.92, 0.5, 1.0, &call("ceiling=0.92"))?;
    let pulse = match a.get("pulse").filter(|v| !v.is_null()) { None => None, Some(p) => { let pc = |s: &str| call(&format!("pulse={{{s}}}")); p.as_object().ok_or_else(|| bg_err(E_RANGE, "pulse must be {rate_hz, amount}", &pc("rate_hz:1,amount:0.3")))?;
        if glow.is_none() { return Err(bg_err(E_RANGE, "pulse animates the glow (outer_glow amount), so glow must be set", &call("glow={radius_px:12,intensity:0.6}, pulse={rate_hz:1,amount:0.3}"))); }
        Some((num(p, "rate_hz", 1.0, 0.0, 20.0, &pc("rate_hz=1"))?, num(p, "amount", 0.3, 0.0, 1.0, &pc("amount=0.3"))?)) } };
    // the shape of one key: points in canvas px
    let mut frames: Vec<Vec<[f64; 2]>> = vec![]; let mut est = json!({"expr_ops": 0, "cost": "one path stroke per frame"}); let mut chunk_ranges = vec![(0usize, 0usize)]; let mut times: Vec<f64> = vec![0.0];
    let (mut hop_used, mut hop_req, mut coarsened) = (0.0, 0.0, false);
    match source {
        "points" => { frames.push(pt_list(a.get("points").ok_or_else(|| bg_err(E_RANGE, "points is required for source points", &call("source=\"points\", points=[[0,0],[100,50]]")))?, &call("points=[[0,0],[100,50]]"))?); }
        "values" => { let arr = a.get("values").and_then(|v| v.as_array()).ok_or_else(|| bg_err(E_RANGE, "values is required for source values: 2..256 numbers", &call("source=\"values\", values=[0,0.5,1]")))?;
            if arr.len() < 2 || arr.len() > 256 { return Err(bg_err(E_RANGE, &format!("values needs 2..256 entries (got {})", arr.len()), &call("values=[...256 or fewer]"))); }
            let rg = match a.get("range").filter(|v| !v.is_null()) { None => [-1.0, 1.0], Some(v) => { let r = v.as_array().filter(|x| x.len() == 2).ok_or_else(|| bg_err(E_RANGE, "range must be [lo, hi]", &call("range=[-1,1]")))?; let g = |i: usize| r[i].as_f64().filter(|f| f.is_finite()).ok_or_else(|| bg_err(E_RANGE, "range entries must be finite", &call("range=[-1,1]"))); let (lo, hi) = (g(0)?, g(1)?); if hi <= lo { return Err(bg_err(E_RANGE, "range must have lo < hi", &call("range=[-1,1]"))); } [lo, hi] } };
            let nv = arr.len(); let mut pts = vec![]; for (i, v) in arr.iter().enumerate() { let f = v.as_f64().filter(|f| f.is_finite()).ok_or_else(|| bg_err(E_RANGE, &format!("values[{i}] must be a finite number (got {v})"), &call("values=[0,0.5,1]")))?;
                let k = ((f - rg[0]) / (rg[1] - rg[0])).clamp(0.0, 1.0); pts.push([bx[0] + bx[2] * i as f64 / (nv - 1) as f64, bx[1] + bx[3] * (1.0 - k)]); } frames.push(pts); }
        _ => {
            let au = a.get("audio").and_then(|v| v.as_object()).ok_or_else(|| bg_err(E_RANGE, "audio is required for source audio: {target, start, end, bands, ...}", &call("source=\"audio\", audio={target:\"mix\"}")))?; let av = a.get("audio").unwrap(); let ac = |s: &str| call(&format!("audio={{{s}}}"));
            for k in au.keys() { if !["target", "start", "end", "bands", "scale", "floor_db", "smoothing_frames", "hop_s", "chunk", "strict_hop"].contains(&k.as_str()) { return Err(bg_err(super::bgcommon::E_UNKNOWN, &format!("unknown audio property '{k}'. Valid: target, start, end, bands, scale, floor_db, smoothing_frames, hop_s, chunk, strict_hop"), &ac("target:\"mix\""))); } }
            let target = au.get("target").and_then(|v| v.as_str()).filter(|t| *t == "mix" || t.starts_with("clip:") || t.starts_with("track:")).ok_or_else(|| bg_err(E_RANGE, "audio.target must be mix, clip:<id> or track:<id>", &ac("target:\"mix\"")))?;
            let st = num(av, "start", 0.0, 0.0, 1e7, &ac("start=0"))?; let en = num(av, "end", st + c.duration, 0.0, 1e7, &ac("end=3"))?; if en <= st { return Err(bg_err(E_RANGE, "audio.end must be greater than audio.start", &ac("start=0,end=3"))); }
            let bands = num(av, "bands", 32.0, 4.0, 128.0, &ac("bands=32"))?; if bands.fract() != 0.0 { return Err(bg_err(E_RANGE, "audio.bands must be an integer 4..128", &ac("bands=32"))); } let bands = bands as usize;
            let scale = match au.get("scale").filter(|v| !v.is_null()) { None => "db", Some(v) => v.as_str().filter(|s| ["linear", "db"].contains(s)).ok_or_else(|| bg_err(E_RANGE, &format!("audio.scale must be linear or db (got {v})"), &ac("scale=\"db\"")))? };
            let floor = num(av, "floor_db", -80.0, -120.0, -20.0, &ac("floor_db=-80"))?; let smooth_n = num(av, "smoothing_frames", 2.0, 0.0, 32.0, &ac("smoothing_frames=2"))? as usize;
            let fps = ss.project.fps as f64; hop_req = num(av, "hop_s", 1.0 / fps, 1e-4, 10.0, &ac("hop_s=0.04"))?;
            let flag = |k: &str| -> Result<bool, String> { match au.get(k).filter(|v| !v.is_null()) { None => Ok(false), Some(v) => v.as_bool().ok_or_else(|| bg_err(E_RANGE, &format!("audio.{k} must be a boolean"), &ac(&format!("{k}=false")))) } };
            let (chunk, strict) = (flag("chunk")?, flag("strict_hop")?);
            let span = c.duration.min(en - st); let floor_hop = hop_req.max(1.0 / fps); let n_at = |h: f64| ((span / h) - 1e-9).ceil().max(1.0) as usize;
            let mut nn = n_at(floor_hop); hop_used = floor_hop;
            if nn > KEY_CAP && !chunk { if strict { return Err(bg_err(E_KEYCAP, &format!("{nn} keys at hop {floor_hop} s exceed the {KEY_CAP} key cap per clip"), &call("audio={...,chunk:true}"))); }
                hop_used = floor_hop.max(span / KEY_CAP as f64); nn = n_at(hop_used); while nn > KEY_CAP { hop_used *= 1.0 + 1e-9; nn = n_at(hop_used); } coarsened = true; }
            let rd = crate::commands::dispatch_nested(ss, "read_samples", &json!({"target": target, "start": st, "end": st + span, "channels": "mono", "format": "base64_f32le"}))?;
            if let Some(r) = rd.get("source_info").and_then(|i| i.get("sample_rate").or_else(|| i.get("rate"))).and_then(|v| v.as_f64()) { if (r - 48000.0).abs() > 0.5 { return Err(bg_err(E_AUDIO_RATE, &format!("the audio source is {r} Hz, only 48000 Hz is supported"), &call("audio={target:\"mix\"}"))); } }
            let raw = crate::perception::audio_raw::unb64(rd["data"].as_str().ok_or("read_samples returned no data")?)?; let x: Vec<f32> = raw.chunks_exact(4).map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]])).collect();
            let mut lv: Vec<Vec<f64>> = vec![]; times = (0..nn).map(|k| k as f64 * hop_used).collect();
            for (k, t) in times.iter().enumerate() { let raw_l = band_levels(&x, 48000.0, (t * 48000.0).round() as usize, bands, scale == "db", floor);
                let from = k.saturating_sub(smooth_n); let mut acc = raw_l.clone(); let mut cnt = 1.0; for j in from..k { for (q, v) in lv[j].iter().enumerate() { acc[q] += v; } cnt += 1.0; } let _ = &mut acc; lv.push(raw_l.iter().enumerate().map(|(q, _)| acc[q] / cnt).collect()); }
            for l in &lv { frames.push(l.iter().enumerate().map(|(i, v)| [bx[0] + bx[2] * i as f64 / (bands - 1) as f64, bx[1] + bx[3] * (1.0 - v)]).collect()); }
            chunk_ranges = chunk_plan(nn, chunk);
        } }
    let nframes = frames.len(); let (w2, h2) = (cw / 2.0, ch / 2.0); let pts_json = |f: &Vec<[f64; 2]>| json!(f.iter().map(|p| [p[0] - w2, p[1] - h2]).collect::<Vec<_>>());
    let mut ops = vec![]; let mut ids: Vec<String> = vec![]; let mut detail = vec![]; let mut keys_total = 0usize;
    for (ci, (f0, f1)) in chunk_ranges.iter().enumerate() {
        let (t0, t1) = if source == "audio" { (times[*f0], if ci + 1 == chunk_ranges.len() { c.duration.min(times[nframes - 1] + hop_used.max(1e-9)).max(times[*f1]) } else { times[*f1] }) } else { (0.0, c.duration) };
        let id = format!("w{ci}"); ids.push(id.clone()); let first = &frames[*f0];
        ops.push(json!({"id": id, "tool": "add_path_shape", "args": {"track_id": c.track, "start": c.start + t0, "duration": t1 - t0, "points": pts_json(first), "smooth": smooth, "tension": tension, "fill": [-1, 0, 0], "stroke_width": stroke, "stroke_color": color}}));
        if let Some((f, t, ang)) = gradient { ops.push(json!({"tool": "add_shape_modifier", "args": {"clip_id": format!("${id}.clip_id"), "kind": "gradient_stroke", "params": {"r1": f[0], "g1": f[1], "b1": f[2], "r2": t[0], "g2": t[1], "b2": t[2], "angle": ang}}})); }
        if source == "audio" && f1 > f0 { for k in (*f0 + 1)..=*f1 { ops.push(json!({"tool": "add_path_key", "args": {"clip_id": format!("${id}.clip_id"), "time": times[k] - t0, "points": pts_json(&frames[k]), "smooth": smooth, "tension": tension, "ease": ease}})); keys_total += 1; } }
        if let Some((rad, inten)) = glow { let scales = [(2.5 * rad, 0.2, 2usize), (rad, 0.3, 1usize), (rad / 3.0, 0.5, 0usize)]; let mut mid_eid = String::new();
            for (si, (r_i, wgt, ci_col)) in scales.iter().enumerate() { let eid = format!("g{ci}_{si}"); let col = glow_cols[*ci_col]; if si == 1 { mid_eid = eid.clone(); }
                ops.push(json!({"id": eid, "tool": "add_effect", "args": {"clip_id": format!("${id}.clip_id"), "kind": "outer_glow", "params": {"amount": (inten * wgt * gscale(*r_i, stroke)).min(acap), "radius": r_i, "r": col[0], "g": col[1], "b": col[2], "follow": glow_follow}}})); }
            if let Some((rate, amt)) = pulse { let dur = t1 - t0; let nk = ((dur * rate * 8.0).ceil() as usize + 1).max(2); // the pulse animates the mid scale
                if nk > KEY_CAP { return Err(bg_err(E_KEYCAP, &format!("pulse needs {nk} keys for {dur} s at {rate} Hz, the cap is {KEY_CAP}"), &call("pulse={rate_hz:<=2,...} or a shorter duration"))); }
                for k in 0..nk { let tl = dur * k as f64 / (nk - 1) as f64; let v = (inten * 0.3 * gscale(rad, stroke) * (1.0 + amt * (2.0 * std::f64::consts::PI * rate * (t0 + tl)).sin())).min(acap); ops.push(json!({"tool": "set_keyframe", "args": {"clip_id": format!("${id}.clip_id"), "effect_id": format!("${mid_eid}.effect_id"), "property": "amount", "time": tl, "value": v}})); keys_total += 1; } } }
        ops.push(json!({"tool": "add_effect", "args": {"clip_id": format!("${id}.clip_id"), "kind": "shoulder", "params": {"start": (ceiling - 0.12).max(0.0), "ceiling": ceiling}}}));
        ops.extend(c.finish_ops(&id));
        detail.push(json!({"clip": id, "t0": c.start + t0, "t1": c.start + t1, "first_sample": f0, "last_sample": f1}));
    }
    est["keys"] = json!(keys_total + chunk_ranges.len()); est["chunks"] = json!(chunk_ranges.len()); est["hop_s_used"] = json!(hop_used); est["hop_s_requested"] = json!(hop_req); est["coarsened"] = json!(coarsened); est["chunk_detail"] = json!(detail);
    let params = { let mut p = c.echo(); p["source"] = json!(source); p["smooth"] = json!(smooth); p["tension"] = json!(tension); p["stroke_px"] = json!(stroke); p["color"] = json!(color); p["ease"] = json!(ease); p["box"] = json!(bx); p["glow"] = glow.map_or(Value::Null, |g| json!({"radius_px": g.0, "intensity": g.1})); p["pulse"] = pulse.map_or(Value::Null, |g| json!({"rate_hz": g.0, "amount": g.1})); p["fill_under"] = Value::Null; p["closed"] = json!(false); p };
    if ops.len() > 500 { return Err(bg_err(E_KEYCAP, &format!("the expansion needs {} calls, the batch limit is 500", ops.len()), &call("audio={...,hop_s:<larger>} or a shorter audio window"))); }
    let refs: Vec<&str> = ids.iter().map(|s| s.as_str()).collect();
    let next = if c.apply { vec![json!({"tool": "lint_frame", "args": {"time": c.start, "annotate": false}, "why": "check layers placed over the wave"})] } else { vec![] };
    bg::finish(ss, &c, params, ops, &refs, est, next)
}
pub fn commands() -> Vec<Command> {
    let mut props = bg::common_props(); props.extend([
        ("source", json!({"type":"string","enum":["points","values","audio"],"description":"points [[x,y]] 2..256, values 2..256 numbers over range, or audio bands from a clip, track or the mix (48000 Hz only)"})), ("points", json!({"type":"array","description":"[[x,y], ...] canvas px from the top-left, 2..256 points"})), ("values", json!({"type":"array","items":{"type":"number"},"description":"2..256 numbers spread across the box"})),
        ("range", json!({"type":"array","items":{"type":"number"},"description":"[lo, hi] for values, default [-1,1]"})), ("audio", json!({"type":"object","description":"{target mix|clip:<id>|track:<id>, start, end, bands 4..128 (32), scale linear|db (db), floor_db -120..-20 (-80), smoothing_frames 0..32 (2), hop_s, chunk, strict_hop}"})),
        ("box", json!({"type":"array","items":{"type":"number"},"description":"[x, y, w, h] canvas px, default the canvas inset 10%"})), ("smooth", json!({"type":"boolean","description":"default true"})), ("tension", json!({"type":"number","minimum":0,"maximum":1,"default":0.5})), ("closed", json!({"type":"boolean","description":"closed paths are rejected"})),
        ("stroke_px", json!({"type":"number","minimum":0.5,"maximum":64,"default":3})), ("color", json!({"type":"array","items":{"type":"number"},"description":"[r,g,b] 0..1, default [1,1,1]"})), ("fill_under", json!({"description":"not implemented, leave null"})),
        ("glow", json!({"type":"object","description":"{radius_px 0..256, intensity 0..4, colors?: 1-3 [r,g,b] for the tight, mid and wide scale}. Three outer_glow scales (r/3, r, 2.5r, weights 0.5/0.3/0.2)"})), ("pulse", json!({"type":"object","description":"{rate_hz 0..20, amount 0..1}, animates the mid glow scale"})), ("gradient", json!({"type":["object","boolean"],"description":"{from [r,g,b], to [r,g,b], angle_deg}: the stroke colour blends from to along the layer (angle 0 = left to right). Default {from [0.35,0.85,1], to [0.75,0.4,1], angle_deg 0}, so there is no near-white core; false = plain `color`. Glow scales default to from / mid / to"})), ("ceiling", json!({"type":"number","minimum":0.5,"maximum":1,"default":0.92,"description":"highest value any channel can reach (shoulder, 0.92 = 235 of 255); 1 lets a white core reach 255"})), ("ease", json!({"type":"string","enum":["linear","in","out","in_out","hold"],"description":"between audio keys, default linear"}))]);
    let mut m = serde_json::Map::new(); for (k, v) in props { m.insert(k.into(), v); }
    vec![Command{name:"bg_wave",mutates:true,description:"Waveform or spectrum line background: a stroked path drawn from points, values or live audio bands, with optional glow and pulse. A pure expansion into add_path_shape, add_path_key morph keys, add_effect outer_glow and set_meta; apply false (default) returns the calls and changes nothing, apply true runs them as one undo step. Audio mode keeps at most 255 keys per clip: it coarsens the hop and reports hop_s_used, or chunk true splits into back-to-back clips that share a boundary sample, or strict_hop true refuses to coarsen. Only 48000 Hz audio. Closed paths and fill_under are not implemented.",schema:json!({"type":"object","properties":m,"required":["track","duration","source"]}),run}]
}
