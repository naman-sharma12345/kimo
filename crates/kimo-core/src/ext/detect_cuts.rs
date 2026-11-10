//! Read-only content cut detection on timeline/isolated clip/file sources.
use super::kit::*;
use crate::perception::{
    analysis::{self, Request},
    cuts,
};
fn numeric(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    let v = a
        .get(k)
        .map(|v| {
            v.as_f64()
                .ok_or_else(|| format!("out_of_range: {k} must be numeric"))
        })
        .transpose()?
        .unwrap_or(d);
    if !v.is_finite() {
        return Err(format!("out_of_range: {k} must be finite"));
    }
    Ok(v)
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let source = a
        .get("source")
        .map(|v| v.as_str().ok_or("invalid_source: source string required"))
        .transpose()?
        .unwrap_or("timeline");
    let parsed = if source == "timeline" {
        "composite"
    } else {
        source
    };
    if parsed != "composite" && !parsed.starts_with("clip:") && !parsed.starts_with("file:") {
        return Err("invalid_source: Valid: timeline, clip:<id>, file:<path>".into());
    }
    let method = a
        .get("method")
        .map(|v| v.as_str().ok_or("invalid_method: method string required"))
        .transpose()?
        .unwrap_or("combined");
    if !["histogram", "luma", "combined"].contains(&method) {
        return Err("invalid_method: unknown method; valid: histogram, luma, combined".into());
    }
    let mut bounds = (0., ss.project.duration());
    if let Some(id) = parsed.strip_prefix("clip:") {
        let c = ss
            .project
            .find_clip(id)
            .ok_or("invalid_source: missing clip")?;
        bounds = (c.start, c.start + c.duration);
    }
    if let Some(path) = parsed.strip_prefix("file:") {
        let (_, _, d, _) = crate::render::probe(&ss.project.resolve(path))
            .map_err(|e| format!("invalid_source: {e}"))?;
        bounds = (0., d);
    }
    let start = numeric(a, "start", bounds.0)?;
    let end = numeric(a, "end", bounds.1)?;
    let fps = numeric(a, "fps", ss.project.fps.min(12.))?;
    let scale = numeric(a, "scale", 160.)?;
    let sensitivity = numeric(a, "sensitivity", 0.5)?;
    let threshold = numeric(a, "threshold", 0.6 - 0.5 * sensitivity)?;
    let gap = numeric(a, "min_gap_s", 0.25)?;
    if start < bounds.0
        || end > bounds.1 + 1e-9
        || end <= start
        || fps <= 0.
        || fps > ss.project.fps
        || scale < 1.
        || scale > 8192.
        || !(0.0..=1.0).contains(&sensitivity)
        || !(0.0..=1.0).contains(&threshold)
        || gap < 0.
    {
        return Err("out_of_range: require source-bounded start<end, 0<fps<=timeline fps, scale 1..8192, sensitivity/threshold 0..1, min_gap_s>=0".into());
    }
    let spatial = a
        .get("spatial")
        .map(|v| -> Result<(usize, f64), String> {
            if !v.is_object() {
                return Err("out_of_range: spatial object required".into());
            }
            let grid = v.get("grid").and_then(Value::as_str).unwrap_or("4x4");
            let side = match grid {
                "4x4" => 4,
                "1x1" => 1,
                _ => return Err("out_of_range: spatial grid must be 4x4 or 1x1".into()),
            };
            let min = numeric(v, "min_cells_ratio", 0.6)?;
            if !(0.0..=1.0).contains(&min) {
                return Err("out_of_range: min_cells_ratio must be 0..1".into());
            }
            Ok((side, min))
        })
        .transpose()?;
    let n = ((end - start) * fps).ceil() as usize;
    if n > 20000 {
        return Err(format!(
            "too_many_frames: {n} exceeds cap 20000; lower fps/scale or shorten range"
        ));
    }
    let sample = |ss: &mut Session, t: f64| -> Result<analysis::Image, String> {
        let im = analysis::render(
            ss,
            &Request {
                time: t,
                source: parsed.into(),
            },
        )?;
        let ratio = (scale / im.w.max(im.h) as f64).min(1.);
        let (w, h) = (
            ((im.w as f64 * ratio).round() as usize).max(1),
            ((im.h as f64 * ratio).round() as usize).max(1),
        );
        Ok(if w == im.w && h == im.h {
            im
        } else {
            im.downscale(w, h)
        })
    };
    crate::guard::reserve(n * 512 * 8 + n * 16);
    let mut histograms = Vec::with_capacity(n);
    let mut means = Vec::with_capacity(n);
    let mut scores = Vec::with_capacity(n);
    let mut spatial_ratios = Vec::with_capacity(n);
    let mut prev = None;
    for i in 0..n {
        crate::guard::check();
        let im = sample(ss, start + i as f64 / fps)?;
        scores.push(
            prev.as_ref()
                .map(|p| cuts::score(p, &im, method))
                .unwrap_or(0.),
        );
        spatial_ratios.push(match (spatial, prev.as_ref()) {
            (Some((side, _)), Some(p)) => {
                if im.w < side || im.h < side {
                    return Err("out_of_range: spatial grid exceeds analysis dimensions".into());
                }
                analysis::spatial_change(p, &im, side, side, threshold)
            }
            _ => 0.,
        });
        means.push((0..im.w * im.h).map(|i| im.luma(i)).sum::<f64>() / (im.w * im.h) as f64);
        histograms.push(im.hist());
        prev = Some(im);
    }
    let hints = a.get("kind_hints").and_then(Value::as_bool).unwrap_or(true);
    let (found, scores) = cuts::detect_stats_spatial(
        &scores,
        &means,
        &histograms,
        threshold,
        (gap * fps).ceil() as usize,
        hints,
        spatial.map(|(_, min)| (spatial_ratios.as_slice(), min)),
    );
    let refine = a.get("refine").and_then(Value::as_bool).unwrap_or(true);
    let mut values = vec![];
    for c in found {
        let mut t = start + c.index as f64 / fps;
        let mut peak = c.score;
        if refine && c.kind == "hard_cut" {
            let lo = ((t - 1. / fps).max(start) * ss.project.fps).floor() as usize;
            let hi =
                ((t + 1. / fps).min(end - 1. / ss.project.fps) * ss.project.fps).ceil() as usize;
            let mut prev = sample(ss, lo as f64 / ss.project.fps)?;
            peak = -1.;
            for f in lo + 1..=hi {
                let next = sample(ss, f as f64 / ss.project.fps)?;
                let sc = cuts::score(&prev, &next, method);
                if sc > peak {
                    peak = sc;
                    t = f as f64 / ss.project.fps;
                }
                prev = next;
            }
        }
        let frame = (t * ss.project.fps).round() as usize;
        let mut v = json!({"time":t,"frame":frame,"score":peak,"kind":c.kind,"confidence":peak.clamp(0.,1.),"before_frame_hint":frame.saturating_sub(1),"after_frame_hint":frame});
        if c.kind == "fade" {
            v["dip"] = json!(true);
        }
        if let Some(i) = c.start {
            v["start"] = json!(start + i as f64 / fps);
        }
        if let Some(i) = c.end {
            v["end"] = json!(start + i as f64 / fps);
        }
        values.push(v);
    }
    let boundaries: Vec<f64> = if source == "timeline" {
        let mut b: Vec<_> = ss
            .project
            .tracks
            .iter()
            .flat_map(|tr| tr.clips.iter())
            .map(|c| c.start)
            .filter(|t| *t > start && *t < end)
            .collect();
        b.sort_by(f64::total_cmp);
        b.dedup();
        b
    } else {
        vec![]
    };
    let mut notes = vec![];
    if values.is_empty() {
        notes.push(
            "No content cuts above threshold and adaptive guard (static or gradual motion)."
                .to_string(),
        );
    }
    let mut out = json!({"source":source,"range":{"start":start,"end":end},"fps_analysed":fps,"params_used":{"method":method,"scale":scale,"sensitivity":sensitivity,"threshold":threshold,"threshold_explicit":a.get("threshold").is_some(),"threshold_formula":"0.6 - 0.5*sensitivity; explicit wins","adaptive_guard":"score > mean(5 prior scores)*3 + 0.02","histogram":"8x8x8 RGB normalized half-L1","luma":"mean abs Rec709 delta / 0.25, capped 1","combined":"max(histogram,luma)","min_gap_s":gap,"refine":refine,"kind_hints":hints,"spatial":a.get("spatial"),"content_guard":"histogram change>0.02 or mean luma change>0.02 before hard/flash"},"count":values.len(),"cuts":values,"timeline_clip_boundaries":boundaries,"notes":notes});
    if a.get("return_scores")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        let stride = n.div_ceil(10000).max(1);
        let points: Vec<_> = (0..n)
            .step_by(stride)
            .map(|i| json!({"time":start+i as f64/fps,"score":scores[i]}))
            .collect();
        out["scores"] = json!(points);
        if stride > 1 {
            out["notes"].as_array_mut().unwrap().push(json!(format!(
                "scores decimated every {stride} points, max 10000"
            )));
        }
    }
    Ok(out)
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"detect_cuts",mutates:false,description:"Detect content cuts in timeline composite (default), clip:<id> or file:<path>. Advisory only, never edits. Default min(timeline fps,12), analysis max side160; shared box filter, normalized 8x8x8 RGB histogram half-L1, Rec709 luma mean abs delta/0.25 capped1, combined=max. Threshold=0.6-0.5*sensitivity or explicit threshold wins; additionally score > mean(5 prior scores)*3+0.02 suppresses motion. min_gap_s keeps stronger candidate, refine resamples +/-1 analysis step at full timeline fps. Kind hints identify single-step hard_cut, monotone >=3-step dissolve, return-within-3-step flash. Dip-to-black merges opposite ramps into one fade interval with dip:true. Optional spatial grid4x4 (or1x1) requires histogram change above threshold in >=min_cells_ratio cells (default.6), off by default. Adaptive and content guards precede flash labeling; global pans can still look like cuts, verify scores with frame comparisons. Caps 20000 analysed frames, scores decimated to <=10000. Timeline clip boundaries are separate hints, not content detections.",schema:obj(vec![("source",s("timeline default, clip:<id>, file:<path>")),("start",n("range start")),("end",n("range end")),("method",json!({"type":"string","enum":["histogram","luma","combined"]})),("fps",n("0<fps<=timeline fps")),("scale",n("analysis max side default160")),("sensitivity",n("0..1 default.5")),("threshold",n("0..1 explicit wins")),("min_gap_s",n("default.25")),("refine",json!({"type":"boolean"})),("return_scores",json!({"type":"boolean"})),("kind_hints",json!({"type":"boolean"})),("spatial",json!({"type":"object","description":"off by default; grid4x4 or1x1, min_cells_ratio default0.6, per-cell histogram change>threshold"}))],&[]),run}]
}
