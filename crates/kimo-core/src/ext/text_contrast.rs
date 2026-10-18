//! v6 eyes: text_contrast_series. WCAG 2.x contrast of text against the moving picture behind it, frame by frame over a time
//! range, reporting the worst frame and its timestamp. Reuses contrast.rs (relative luminance on the sRGB EOTF, ratio
//! (L1 + 0.05) / (L2 + 0.05)). Per sampled frame: the text side is a fixed colour or the mean luminance of a text rect; the
//! background side is a ring of ring_px around text_box (text_box itself left out) or an explicit background rect, read as
//! bg_stat (worst = whichever of its p5 / p95 luminance gives the lower ratio, or mean / p5 / p95). Raw numbers, no verdict.
use super::contrast::{ratio, rel_lum};
use super::kit::*;
use super::scopes_numeric::percentile_sorted;
use crate::perception::analysis::{self, Image, Request};

pub const BG_STATS: [&str; 4] = ["worst", "mean", "p5", "p95"];
pub const BG_REGIONS: [&str; 2] = ["ring", "box"];
const CAP: usize = 20000;

fn rect4(v: &Value, k: &str) -> Result<[usize; 4], String> {
    let bad = || format!("out_of_range: {k} must be [x,y,w,h] whole pixels (w,h >= 1)");
    let r = v.as_array().filter(|a| a.len() == 4).ok_or_else(bad)?;
    let mut q = [0usize; 4];
    for (i, x) in r.iter().enumerate() { q[i] = x.as_f64().filter(|x| x.is_finite() && x.fract() == 0.0 && *x >= 0.0 && *x <= 65536.0).ok_or_else(bad)? as usize; }
    if q[2] == 0 || q[3] == 0 { return Err(bad()); }
    Ok(q)
}
fn colour(v: &Value) -> Option<[f64; 3]> {
    match v {
        Value::String(s) => { let h = s.strip_prefix('#')?; if !h.chars().all(|c| c.is_ascii_hexdigit()) { return None; }
            let b: Vec<u8> = match h.len() { 3 => h.chars().map(|c| c.to_digit(16).unwrap() as u8 * 17).collect(), 6 => (0..3).map(|i| u8::from_str_radix(&h[i * 2..i * 2 + 2], 16).unwrap()).collect(), _ => return None };
            Some([b[0] as f64 / 255.0, b[1] as f64 / 255.0, b[2] as f64 / 255.0]) }
        Value::Array(a) if a.len() == 3 => { let c: Vec<f64> = a.iter().filter_map(|x| x.as_f64().filter(|x| x.is_finite() && (0.0..=1.0).contains(x))).collect(); if c.len() == 3 { Some([c[0], c[1], c[2]]) } else { None } }
        _ => None,
    }
}
#[derive(Clone, Copy)]
enum Text { Colour([f64; 3]), Rect([usize; 4]) }
fn inside(q: &[usize; 4], w: usize, h: usize) -> bool { q[0] + q[2] <= w && q[1] + q[3] <= h }
fn px_lum(im: &Image, i: usize) -> f64 { rel_lum([im.rgb[0][i], im.rgb[1][i], im.rgb[2][i]], None) }
/// Luminance of every background pixel: the explicit rect, or the ring of `ring` px around `tb` clipped to the frame, minus `tb`
/// (ring = 0: the pixels under `tb` itself, bg_region box).
/// Glyph pixels of a text colour inside text_box: pixels whose contrast with the text (luminance lt) is lower than their
/// contrast with the frame's background (luminance lb), i.e. pixels that read as text rather than background.
pub fn glyph_px(im: &Image, q: &[usize; 4], lt: f64, lb: f64) -> usize {
    let mut n = 0;
    for y in q[1]..q[1] + q[3] { for x in q[0]..q[0] + q[2] { let l = px_lum(im, y * im.w + x); if ratio(l, lt) < ratio(l, lb) { n += 1; } } }
    n
}
pub fn background_lums(im: &Image, tb: Option<[usize; 4]>, bg: Option<[usize; 4]>, ring: usize) -> Vec<f64> {
    let mut out = vec![];
    let bg = if ring == 0 && bg.is_none() { tb } else { bg };
    if let Some(q) = bg { for y in q[1]..q[1] + q[3] { for x in q[0]..q[0] + q[2] { out.push(px_lum(im, y * im.w + x)); } } return out; }
    let t = tb.unwrap();
    let (x0, y0) = (t[0].saturating_sub(ring), t[1].saturating_sub(ring));
    let (x1, y1) = ((t[0] + t[2] + ring).min(im.w), (t[1] + t[3] + ring).min(im.h));
    for y in y0..y1 { for x in x0..x1 { if x >= t[0] && x < t[0] + t[2] && y >= t[1] && y < t[1] + t[3] { continue; } out.push(px_lum(im, y * im.w + x)); } }
    out
}
/// One frame: (ratio, text L, background L used, background p5, p95, mean).
pub fn frame_ratio(lt: f64, bgl: &mut Vec<f64>, stat: &str) -> (f64, f64, f64, f64, f64) {
    bgl.sort_by(|a, b| a.total_cmp(b));
    let (p5, p95) = (percentile_sorted(bgl, 5.0), percentile_sorted(bgl, 95.0));
    let mean = bgl.iter().sum::<f64>() / bgl.len() as f64;
    let lb = match stat { "mean" => mean, "p5" => p5, "p95" => p95, _ => if ratio(lt, p5) <= ratio(lt, p95) { p5 } else { p95 } };
    (ratio(lt, lb), lb, p5, p95, mean)
}
fn num(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or(format!("out_of_range: {k} must be a number")) }
}
fn run_one(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let source = match a.get("source") { None | Some(Value::Null) => "timeline", Some(v) => v.as_str().ok_or("invalid_source: source string required")? };
    let parsed = if source == "timeline" { "composite" } else { source };
    if parsed != "composite" && !parsed.starts_with("clip:") && !parsed.starts_with("file:") { return Err("invalid_source: Valid: timeline, clip:<id>, file:<path>".into()); }
    let mut bounds = (0., ss.project.duration());
    if let Some(id) = parsed.strip_prefix("clip:") { let c = ss.project.find_clip(id).ok_or("invalid_source: missing clip")?; bounds = (c.start, c.start + c.duration); }
    if let Some(path) = parsed.strip_prefix("file:") { let (_, _, d, _) = crate::render::probe(&ss.project.resolve(path)).map_err(|e| format!("invalid_source: {e}"))?; bounds = (0., d); }
    let text = match a.get("text") {
        None | Some(Value::Null) => return Err("out_of_range: text is required: a colour (\"#rrggbb\", \"#rgb\", [r,g,b] 0..1) or {\"rect\": [x,y,w,h]} of text pixels".into()),
        Some(Value::Object(o)) => { if o.len() != 1 || !o.contains_key("rect") { return Err("out_of_range: text object must be {\"rect\": [x,y,w,h]}".into()); } Text::Rect(rect4(&o["rect"], "text.rect")?) }
        Some(v) => Text::Colour(colour(v).ok_or("out_of_range: text must be \"#rrggbb\", \"#rgb\", [r,g,b] 0..1 or {\"rect\": [x,y,w,h]}")?),
    };
    let tb = match a.get("text_box") { None | Some(Value::Null) => None, Some(v) => Some(rect4(v, "text_box")?) };
    let bg = match a.get("background") { None | Some(Value::Null) => None, Some(Value::Object(o)) if o.len() == 1 && o.contains_key("rect") => Some(rect4(&o["rect"], "background.rect")?), Some(_) => return Err("out_of_range: background must be {\"rect\": [x,y,w,h]}".into()) };
    if tb.is_some() == bg.is_some() { return Err("out_of_range: give exactly one of text_box (background = a ring around it) or background {\"rect\"}".into()); }
    let ring = match a.get("ring_px") { None | Some(Value::Null) => 8usize, Some(v) => { if bg.is_some() { return Err("out_of_range: ring_px applies to text_box, not background".into()); } v.as_u64().filter(|x| (1..=512).contains(x)).ok_or("out_of_range: ring_px must be an integer 1..512")? as usize } };
    let region = match a.get("bg_region") { None | Some(Value::Null) => "ring", Some(v) => { let r = v.as_str().filter(|s| BG_REGIONS.contains(s)).ok_or(format!("out_of_range: bg_region {v} is not allowed. Valid: {}", BG_REGIONS.join(", ")))?; if bg.is_some() { return Err("out_of_range: bg_region applies to text_box, not background".into()); } if r == "box" && a.get("ring_px").map_or(false, |v| !v.is_null()) { return Err("out_of_range: ring_px applies to bg_region ring".into()); } r } };
    let ring = if region == "box" { 0 } else { ring };
    let scrim = match a.get("scrim_color") { None | Some(Value::Null) => [0.0, 0.0, 0.0], Some(v) => colour(v).ok_or("out_of_range: scrim_color must be \"#rrggbb\", \"#rgb\" or [r,g,b] 0..1")? };
    let stat = match a.get("bg_stat") { None | Some(Value::Null) => "worst", Some(v) => v.as_str().filter(|s| BG_STATS.contains(s)).ok_or(format!("out_of_range: bg_stat {v} is not allowed. Valid: {}", BG_STATS.join(", ")))? };
    let threshold: Option<f64> = match a.get("threshold") { None | Some(Value::Null) => None, Some(v) => Some(v.as_f64().filter(|x| x.is_finite() && *x >= 1.0 && *x <= 21.0).ok_or("out_of_range: threshold must be a ratio 1..21 (WCAG 2.x: 4.5 normal text, 3 large text)")?) };
    let text_px = match a.get("text_px") { None | Some(Value::Null) => None, Some(v) => Some(v.as_f64().filter(|x| x.is_finite() && *x > 0. && *x <= 4096.).ok_or("out_of_range: text_px must be the text height in pixels of the analysed frame, > 0")?) };
    let bold = match a.get("bold") { None | Some(Value::Null) => false, Some(v) => { if text_px.is_none() { return Err("out_of_range: bold needs text_px".into()); } v.as_bool().ok_or("out_of_range: bold must be true or false")? } };
    let cut_th = match a.get("cut_threshold") { None | Some(Value::Null) => 0.35, Some(v) => v.as_f64().filter(|x| x.is_finite() && *x > 0. && *x <= 1.).ok_or("out_of_range: cut_threshold must be a number in (0, 1]")? };
    let cut_guard = match a.get("cut_guard") { None | Some(Value::Null) => true, Some(v) => v.as_bool().ok_or("out_of_range: cut_guard must be true or false")? };
    let text_min = match a.get("text_min_px") { None | Some(Value::Null) => None, Some(v) => { let n = v.as_u64().filter(|x| (1..=10_000_000).contains(x)).ok_or("out_of_range: text_min_px must be an integer 1..10000000")? as usize; if tb.is_none() || !matches!(text, Text::Colour(_)) { return Err("out_of_range: text_min_px needs text_box and a text colour".into()); } Some(n) } };
    let series = match a.get("series") { None | Some(Value::Null) => false, Some(v) => v.as_bool().ok_or("out_of_range: series must be true or false")? };
    let fps = ss.project.fps;
    let start = num(a, "start", bounds.0)?; let end = num(a, "end", bounds.1)?;
    let step = match a.get("step_frames") { None | Some(Value::Null) => 1u64, Some(v) => v.as_u64().filter(|s| *s >= 1).ok_or("out_of_range: step_frames must be an integer >= 1")? };
    if start < bounds.0 - 1e-9 || end > bounds.1 + 1e-9 || end <= start || !(fps > 0.) { return Err(format!("out_of_range: need {} <= start < end <= {} (the source's span)", bounds.0, bounds.1)); }
    let f0 = (start * fps - 1e-9).ceil().max(0.) as u64; let f1 = (end * fps - 1e-9).ceil() as u64;
    let frames: Vec<u64> = (f0..f1).step_by(step as usize).collect(); let n = frames.len();
    if n == 0 { return Err("out_of_range: no timeline frame inside [start,end)".into()); }
    if n > CAP { return Err(format!("too_many_frames: {n} exceeds cap {CAP}; raise step_frames or shorten the range")); }
    let (mut times, mut ratios, mut lts, mut lbs, mut rows, mut fs): (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>, Vec<Value>, Vec<u64>) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), vec![], Vec::with_capacity(n));
    let mut dims = (0usize, 0usize); let mut bg_px = 0usize; let mut added = 0usize; let mut skipped = 0usize;
    let (mut prev_small, mut cuts): (Option<(u64, Image)>, Vec<f64>) = (None, vec![]);
    let mut hist: std::collections::VecDeque<f64> = std::collections::VecDeque::with_capacity(6);
    // cut score between two frames: max(8x8x8 RGB histogram half-L1, mean abs Rec.709 luma delta / 0.25 capped at 1) on a <= 160 px
    // copy, as detect_cuts scores it (without its adaptive motion guard)
    let small_of = |im: &Image| { let sc = 160.0 / im.w.max(im.h) as f64; if sc < 1.0 { im.downscale(((im.w as f64 * sc).round() as usize).max(1), ((im.h as f64 * sc).round() as usize).max(1)) } else { im.clone() } };
    let score = |a: &Image, b: &Image| analysis::hist_distance(a, b).max((analysis::luma_delta(a, b) / 0.25).min(1.0));
    let measure = |f: u64, im: &Image, times: &mut Vec<f64>, ratios: &mut Vec<f64>, lts: &mut Vec<f64>, lbs: &mut Vec<f64>, rows: &mut Vec<Value>, fs: &mut Vec<u64>, bg_px: &mut usize, skip: &mut usize| -> Result<(), String> {
        let t = f as f64 / fps;
        for (q, k) in [(tb, "text_box"), (bg, "background.rect"), (if let Text::Rect(r) = text { Some(r) } else { None }, "text.rect")] {
            if let Some(q) = q { if !inside(&q, im.w, im.h) { return Err(format!("out_of_range: {k} [{},{},{},{}] must lie inside the {}x{} frame", q[0], q[1], q[2], q[3], im.w, im.h)); } } }
        let lt = match text { Text::Colour(c) => rel_lum(c, None), Text::Rect(q) => { let mut s = 0.0; for y in q[1]..q[1] + q[3] { for x in q[0]..q[0] + q[2] { s += px_lum(im, y * im.w + x); } } s / (q[2] * q[3]) as f64 } };
        let mut bl = background_lums(im, tb, bg, ring);
        if bl.is_empty() { return Err("out_of_range: the background ring has no pixels inside the frame".into()); }
        *bg_px = bl.len();
        let (r, lb, p5, p95, mean) = frame_ratio(lt, &mut bl, stat);
        // text_min_px: a frame counts only when the text is on screen (at least text_min_px glyph pixels inside text_box)
        if let (Some(nm), Some(q)) = (text_min, tb) { if glyph_px(im, &q, lt, lb) < nm { *skip += 1; return Ok(()); } }
        if series { rows.push(json!({"time": t, "frame": f, "ratio": r, "text_luminance": lt, "bg_luminance": lb, "bg_p5": p5, "bg_p95": p95, "bg_mean": mean})); }
        times.push(t); ratios.push(r); lts.push(lt); lbs.push(lb); fs.push(f); Ok(())
    };
    for &f in &frames {
        crate::guard::check();
        let t = f as f64 / fps;
        let im = analysis::render(ss, &Request { time: t, source: parsed.into() })?;
        dims = (im.w, im.h);
        let small = small_of(&im);
        if let Some((pf, ps)) = prev_small.as_ref() {
            let sc = score(ps, &small);
            // detect_cuts' motion guard: a cut must also stand out from the 5 scores before it
            let calm = !cut_guard || hist.is_empty() || sc > hist.iter().sum::<f64>() / hist.len() as f64 * 3.0 + 0.02;
            hist.push_back(sc); if hist.len() > 5 { hist.pop_front(); }
            if sc >= cut_th && calm {
                // a cut between two samples: with step_frames > 1 the frames in between are measured too (every cut frame is
                // checked), and the cut is stamped at the first frame whose score against the frame before reaches cut_threshold
                let mut at = t; let mut before = ps.clone();
                for g in pf + 1..f {
                    if fs.len() + 1 > CAP { return Err(format!("too_many_frames: more than {CAP} with the frames at cuts; raise step_frames or shorten the range")); }
                    let ig = analysis::render(ss, &Request { time: g as f64 / fps, source: parsed.into() })?; let sg = small_of(&ig);
                    if at == t && score(&before, &sg) >= cut_th { at = g as f64 / fps; }
                    measure(g, &ig, &mut times, &mut ratios, &mut lts, &mut lbs, &mut rows, &mut fs, &mut bg_px, &mut skipped)?; added += 1; before = sg;
                }
                cuts.push(at);
            }
        }
        prev_small = Some((f, small));
        measure(f, &im, &mut times, &mut ratios, &mut lts, &mut lbs, &mut rows, &mut fs, &mut bg_px, &mut skipped)?;
    }
    let frames = fs; let n = frames.len();
    if n == 0 { return Err(format!("no_text: none of the {skipped} measured frames has text_min_px glyph pixels inside text_box")); }
    // worst = the lowest ratio; ties go to the earliest frame
    let mut wi = 0; let mut bi = 0;
    for i in 1..n { if ratios[i] < ratios[wi] { wi = i; } if ratios[i] > ratios[bi] { bi = i; } }
    let mut out = json!({
        "worst_ratio": ratios[wi], "worst_time": times[wi], "worst_frame": frames[wi], "worst_text_luminance": lts[wi], "worst_bg_luminance": lbs[wi],
        "best_ratio": ratios[bi], "best_time": times[bi],
        "frames_sampled": n, "cut_frames_added": added, "start": start, "end": end, "step_frames": step, "fps": fps, "width": dims.0, "height": dims.1,
        "bg_stat": stat, "bg_region": if bg.is_some() { "rect" } else { region }, "bg_pixels": bg_px, "source": source,
        "formula": "per frame: WCAG 2.x (L_lighter + 0.05) / (L_darker + 0.05), L = 0.2126 R + 0.7152 G + 0.0722 B on linear RGB (sRGB EOTF, 0.04045 knee); text L = the colour's L or the mean per-pixel L of text.rect; background L = bg_stat of the per-pixel L of the ring around text_box (text_box left out), of the pixels under text_box (bg_region box) or of background.rect, worst = whichever of p5 / p95 gives the lower ratio"
    });
    let m = out.as_object_mut().unwrap();
    match text { Text::Colour(c) => { m.insert("text".into(), json!({"kind": "color", "rgb": c, "luminance": rel_lum(c, None)})); } Text::Rect(q) => { m.insert("text".into(), json!({"kind": "rect", "rect": q})); } }
    if let Some(q) = tb { m.insert("text_box".into(), json!(q)); if ring > 0 { m.insert("ring_px".into(), json!(ring)); } }
    if let Some(q) = bg { m.insert("background".into(), json!({"rect": q})); }
    m.insert("cut_threshold".into(), json!(cut_th)); m.insert("cut_guard".into(), json!(cut_guard));
    if let Some(nm) = text_min { m.insert("text_min_px".into(), json!(nm)); m.insert("frames_without_text".into(), json!(skipped)); }
    m.insert("cut_count".into(), json!(cuts.len()));
    m.insert("crosses_cut".into(), json!(!cuts.is_empty()));
    m.insert("cut_times".into(), json!(cuts.iter().take(100).copied().collect::<Vec<f64>>()));
    // WCAG 2.x size class at 1080 p (the analysed frame's short side scaled to 1080): large = >= 24 px, or bold and >= 18.66 px
    let mut threshold = threshold; let explicit = threshold.is_some();
    if let Some(px) = text_px {
        let px1080 = px * 1080.0 / dims.0.min(dims.1) as f64; let large = px1080 >= 24.0 || (bold && px1080 >= 18.66);
        m.insert("text_px".into(), json!(px)); m.insert("bold".into(), json!(bold)); m.insert("text_px_1080".into(), json!(px1080));
        m.insert("size_class".into(), json!(if large { "large" } else { "normal" }));
        if !explicit { threshold = Some(if large { 3.0 } else { 4.5 }); }
    }
    if threshold.is_some() { m.insert("threshold_source".into(), json!(if explicit { "explicit" } else { "text_px" })); }
    if let Some(th) = threshold {
        let below: Vec<usize> = (0..n).filter(|&i| ratios[i] < th).collect();
        // longest run of consecutive samples below the bar, in seconds: last - first sample time + one step
        let (mut run_best, mut run0): (f64, Option<usize>) = (0.0, None);
        for i in 0..n { if ratios[i] < th { let s = *run0.get_or_insert(i); run_best = run_best.max(times[i] - times[s] + step as f64 / fps); } else { run0 = None; } }
        m.insert("threshold".into(), json!(th));
        m.insert("frames_below".into(), json!(below.len()));
        m.insert("pct_below".into(), json!(below.len() as f64 * 100.0 / n as f64));
        m.insert("first_below_time".into(), below.first().map_or(Value::Null, |&i| json!(times[i])));
        m.insert("longest_below_s".into(), json!(run_best));
        // the fix for text that fails over moving video is a scrim behind it, never a recolour (Naman, 2026-10-18 2:56 / 3:10 AM)
        if !below.is_empty() {
            let (lt, lb, ls) = (lts[wi], lbs[wi], rel_lum(scrim, None));
            // the background luminance the bar allows at the worst frame, on the scrim's side of the text
            let target = if ls <= lt { (lt + 0.05) / th - 0.05 } else { th * (lt + 0.05) - 0.05 };
            let reach = if ls <= lt { ls <= target } else { ls >= target };
            let alpha = if !reach { Value::Null } else if (ls <= lt && lb <= target) || (ls > lt && lb >= target) { json!(0.0) } else { json!(((lb - target) / (lb - ls)).clamp(0.0, 1.0)) };
            m.insert("suggested_fix".into(), json!("scrim"));
            m.insert("scrim_color".into(), json!(scrim));
            m.insert("scrim_alpha_min".into(), alpha);
            m.insert("fix_note".into(), json!("add a scrim (a semi-opaque box or gradient in scrim_color) behind the text; do not recolour the text. scrim_alpha_min = the smallest scrim opacity that brings the worst frame's background to the bar, blended in linear light (an sRGB-space blend with a black scrim darkens at least as much); null = this scrim colour cannot reach the bar"));
        }
    }
    if series { m.insert("series".into(), Value::Array(rows)); }
    Ok(out)
}
pub const CAPTION_KEYS: [&str; 8] = ["name", "text_box", "start", "end", "text", "text_px", "bold", "threshold"];
const CAPTION_CAP: usize = 500;
/// captions: each caption measured over its own [start, end) exactly as a single call (shared arguments from the top level,
/// a caption's own text / text_px / bold / threshold win); the worst caption, the captions under the bar, and every caption
/// on screen across a cut by name with the cut times.
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let caps = match a.get("captions") { None | Some(Value::Null) => return run_one(ss, a), Some(Value::Array(c)) => c.clone(), Some(_) => return Err("out_of_range: captions must be an array of {name, text_box, start, end} objects".into()) };
    if caps.is_empty() || caps.len() > CAPTION_CAP { return Err(format!("out_of_range: captions must hold 1..{CAPTION_CAP} entries")); }
    for k in ["text_box", "background", "start", "end", "series"] { if a.get(k).map_or(false, |v| !v.is_null()) { return Err(format!("out_of_range: with captions, {k} is not a top-level argument (start, end and text_box go inside each caption; series is single-call only)")); } }
    let mut base = a.as_object().cloned().unwrap_or_default(); base.remove("captions");
    let (mut rows, mut crossing, mut below, mut worst): (Vec<Value>, Vec<Value>, Vec<Value>, Option<(f64, usize)>) = (vec![], vec![], vec![], None);
    let mut any_bar = false; let mut total = 0u64;
    for (i, c) in caps.iter().enumerate() {
        let o = c.as_object().ok_or(format!("out_of_range: captions[{i}] must be an object"))?;
        if let Some(k) = o.keys().find(|k| !CAPTION_KEYS.contains(&k.as_str())) { return Err(format!("out_of_range: captions[{i}] key {k} is not allowed. Valid: {}", CAPTION_KEYS.join(", "))); }
        let name = o.get("name").and_then(|v| v.as_str()).filter(|s| !s.is_empty() && s.len() <= 200).ok_or(format!("out_of_range: captions[{i}].name must be a non-empty string (<= 200 chars)"))?.to_string();
        for k in ["text_box", "start", "end"] { if o.get(k).map_or(true, |v| v.is_null()) { return Err(format!("out_of_range: captions[{i}] ({name}) needs {k}")); } }
        let mut sub = base.clone(); for (k, v) in o { if k != "name" { sub.insert(k.clone(), v.clone()); } }
        let r = run_one(ss, &Value::Object(sub)).map_err(|e| format!("{e} (captions[{i}] {name})"))?;
        let wr = r["worst_ratio"].as_f64().unwrap_or(f64::NAN);
        if worst.map_or(true, |(w, _)| wr < w) { worst = Some((wr, i)); }
        total += r["frames_sampled"].as_u64().unwrap_or(0);
        let mut row = json!({"name": name, "text_box": r["text_box"], "start": r["start"], "end": r["end"], "worst_ratio": r["worst_ratio"], "worst_time": r["worst_time"], "worst_frame": r["worst_frame"],
            "frames_sampled": r["frames_sampled"], "cut_frames_added": r["cut_frames_added"], "crosses_cut": r["crosses_cut"], "cut_times": r["cut_times"]});
        let m = row.as_object_mut().unwrap();
        for k in ["size_class", "text_px_1080", "threshold", "frames_without_text", "frames_below", "first_below_time", "longest_below_s", "suggested_fix", "scrim_alpha_min"] { if let Some(v) = r.get(k) { m.insert(k.into(), v.clone()); } }
        if r.get("threshold").is_some() { any_bar = true; if r["frames_below"].as_u64().unwrap_or(0) > 0 { below.push(json!(name)); } }
        if r["crosses_cut"] == true { crossing.push(json!({"name": name, "cut_times": r["cut_times"]})); }
        rows.push(row);
    }
    let (_, wi) = worst.unwrap();
    let w = &rows[wi];
    let mut out = json!({"caption_count": rows.len(), "worst_caption": w["name"], "worst_ratio": w["worst_ratio"], "worst_time": w["worst_time"], "worst_frame": w["worst_frame"],
        "frames_sampled": total, "captions_crossing_cut": crossing.len(), "crossing_cut": crossing});
    let m = out.as_object_mut().unwrap();
    if any_bar { m.insert("captions_below".into(), json!(below.len())); m.insert("below_names".into(), Value::Array(below.clone())); }
    if !below.is_empty() { m.insert("suggested_fix".into(), json!("scrim")); m.insert("fix_note".into(), json!("captions under the bar get a scrim behind them (each caption's scrim_alpha_min); do not recolour the text")); }
    m.insert("captions".into(), Value::Array(rows));
    Ok(out)
}
pub fn commands() -> Vec<Command> { vec![Command { name: "text_contrast_series", mutates: false,
    description: "Worst WCAG contrast of text over moving video across a time range: the lowest ratio with its frame and timestamp, no averages. Text is a colour or a rect of text pixels; the background per frame is the ring of ring_px around text_box (the box left out, for burned-in text), the pixels under text_box (bg_region box, for footage without the text) or a background rect, read as bg_stat (worst = whichever of p5 / p95 luminance gives the lower ratio; p95 = the brightest 5 %). Static images (contrast_ratio) use the worst pixel; moving video uses p95, the brightest 5 % of the background. Optional text_px (text height in px of the analysed frame) and bold (weight 700+ only; semibold counts as normal) give size_class at 1080 p (large = 24 px+, or bold 18.66 px+) and the bar (3 large, 4.5 normal) unless threshold is given; a bar adds frames_below, pct_below, first_below_time and longest_below_s. Frames under the bar add suggested_fix scrim and scrim_alpha_min for scrim_color (default black): the fix is a scrim behind the text, never a recolour. Cuts between sampled frames (detect_cuts' histogram / luma score >= cut_threshold, default 0.35, and above 3x the mean of the 5 scores before it + 0.02 unless cut_guard is false, as detect_cuts guards motion) give cut_count, cut_times and crosses_cut (the text is on screen across a cut); when step_frames > 1 the frames between two samples around a cut are measured too (cut_frames_added). series adds every frame. captions (name, text_box, start, end each; shared arguments at the top) measures each caption over its own time on screen: per caption the worst frame, time and ratio, the worst caption, captions_below and below_names, and crossing_cut (each caption on screen across a cut, by name and its cut_times). text_min_px (needs text_box and a text colour) counts a frame only when the text is on screen: at least that many pixels inside text_box read as text (contrast with the text colour lower than with the background), so word-by-word reveals and an empty scrim box before the first word do not count; frames_without_text says how many were left out. Raw numbers.",
    schema: obj(vec![
        ("source", s("timeline default, clip:<id>, file:<path>")),
        ("start", n("range start seconds, default source start")),
        ("end", n("range end seconds, default source end")),
        ("step_frames", json!({"type": "integer", "minimum": 1, "description": "sample every Nth frame, default 1"})),
        ("text", json!({"description": "text colour \"#rrggbb\", \"#rgb\", [r,g,b] 0..1, or {\"rect\": [x,y,w,h]} of text pixels (mean luminance per frame)"})),
        ("text_box", json!({"type": "array", "items": {"type": "integer"}, "minItems": 4, "maxItems": 4, "description": "[x,y,w,h] bounding box of the text; the background is the ring of ring_px around it"})),
        ("ring_px", json!({"type": "integer", "minimum": 1, "maximum": 512, "description": "ring width around text_box, default 8"})),
        ("background", json!({"description": "{\"rect\": [x,y,w,h]} explicit background instead of text_box"})),
        ("bg_region", json!({"type": "string", "enum": BG_REGIONS, "description": "with text_box: ring (default, the ring around the box) or box (the pixels under the box)"})),
        ("bg_stat", json!({"type": "string", "enum": BG_STATS, "description": "background luminance per frame, default worst"})),
        ("scrim_color", json!({"description": "scrim colour for scrim_alpha_min, \"#rrggbb\", \"#rgb\" or [r,g,b] 0..1, default black"})),
        ("threshold", n("optional ratio 1..21 (4.5 normal text, 3 large text): adds frames_below, pct_below, first_below_time, longest_below_s")),
        ("text_px", n("optional text height in pixels of the analysed frame: sets size_class and the bar (3 or 4.5)")),
        ("bold", json!({"type": "boolean", "description": "with text_px: true only for weight 700+ (semibold counts as normal); bold text is large from 18.66 px at 1080 p"})),
        ("cut_threshold", n("cut score (0, 1], default 0.35")),
        ("cut_guard", json!({"type": "boolean", "description": "default true: a cut also needs a score above 3x the mean of the 5 scores before it + 0.02 (detect_cuts' motion guard); false = threshold only"})),
        ("text_min_px", json!({"type": "integer", "minimum": 1, "description": "optional: count a frame only when at least this many pixels inside text_box read as text (closer in contrast to the text colour than to the background); adds frames_without_text"})),
        ("series", json!({"type": "boolean", "description": "include every sampled frame, default false (single call only)"})),
        ("captions", json!({"type": "array", "maxItems": CAPTION_CAP, "items": {"type": "object"}, "description": "optional batch: [{name, text_box, start, end, text?, text_px?, bold?, threshold?}], one result row per caption; start, end, text_box and series then stay out of the top level"})),
    ], &["text"]), run }] }
