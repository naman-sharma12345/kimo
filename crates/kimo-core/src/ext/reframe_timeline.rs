//! v7.4: reframe_timeline -- a real, persistent, undoable aspect-ratio conversion for the whole timeline.
//! auto_reframe (v7.2) keys one clip at a time; export_variants (v7.3) applies auto_reframe across every
//! media clip too, but only for the length of one export call -- the canvas and every reframe window it set
//! are always restored afterward, by design (its own doc comment: "net effect on the project is nothing").
//! Neither tool actually changes the project: an agent asked to "make this a vertical cut" or "convert this
//! timeline to square for Instagram" as a real edit -- not a one-off export -- had no single call for it;
//! the honest way today is a width/height edit plus calling auto_reframe by hand on every clip, one at a
//! time. This is that one call: it changes project.width/height for real and keys auto_reframe (or just
//! resizes the canvas, with `auto_reframe: false`, leaving every clip's existing fit) on every media clip in
//! scope, as one ordinary mutating tool call with its own single undo step (undo reverses the whole
//! conversion in one step, same as any other tool).
use super::kit::*;
use crate::model::Source;

/// Aspect shorthand -> (width, height), the same small preset table export_variants already uses (kept
/// independent rather than shared so each tool's own doc comment stays the complete word on its own schema).
const ASPECT_PRESETS: &[(&str, (u32, u32))] = &[("16:9", (1920, 1080)), ("9:16", (1080, 1920)), ("1:1", (1080, 1080)), ("4:5", (1080, 1350))];

fn target_size(a: &Value, cur: (u32, u32)) -> Result<(u32, u32), String> {
    if let Some(name) = a.get("aspect").and_then(|v| v.as_str()) {
        return ASPECT_PRESETS.iter().find(|(n, _)| *n == name).map(|(_, wh)| *wh).ok_or_else(|| format!("unknown aspect '{name}'. Valid: {}", ASPECT_PRESETS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")));
    }
    let (w, h) = (a.get("width").and_then(|v| v.as_u64()), a.get("height").and_then(|v| v.as_u64()));
    match (w, h) {
        (Some(w), Some(h)) => { if w == 0 || h == 0 || w > 8192 || h > 8192 { return Err("width/height must be 1..8192".into()); } Ok((w as u32, h as u32)) }
        (None, None) => Err("give aspect (one of 16:9, 9:16, 1:1, 4:5) or both width and height".into()),
        _ => Err("give both width and height, or neither (nor just one)".into()),
    }
}

fn reframe_timeline(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let orig = (ss.project.width, ss.project.height);
    let target = target_size(a, orig)?;
    let aspect_unchanged = (target.0 as f64 * orig.1 as f64 - target.1 as f64 * orig.0 as f64).abs() < 1e-6;
    let motion = a.get("motion").and_then(|v| v.as_str()).unwrap_or("default").to_string();
    if !crate::reframe::MOTIONS.contains(&motion.as_str()) { return Err(format!("motion must be one of {}, got '{motion}'", crate::reframe::MOTIONS.join(", "))); }
    let do_reframe = a.get("auto_reframe").and_then(|v| v.as_bool()).unwrap_or(true);
    let overwrite = a.get("overwrite_existing").and_then(|v| v.as_bool()).unwrap_or(false);
    let dry_run = a.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);
    let track_filter = a.get("track_id").and_then(|v| v.as_str()).map(String::from);
    if let Some(t) = &track_filter { if !ss.project.tracks.iter().any(|tr| tr.id == *t) { return Err(format!("no track '{t}'")); } }

    let candidates: Vec<String> = ss.project.tracks.iter().filter(|t| track_filter.as_deref().map_or(true, |tf| t.id == tf))
        .flat_map(|t| &t.clips).filter(|c| matches!(c.source, Source::Media { .. })).map(|c| c.id.clone()).collect();

    let mut would_reframe = vec![]; let mut skipped_already_framed = vec![]; let mut failed = vec![];
    if do_reframe && !aspect_unchanged {
        for cid in &candidates {
            let clip = ss.project.find_clip(cid).unwrap();
            if !overwrite && crate::reframe::active(clip) { skipped_already_framed.push(cid.clone()); continue; }
            would_reframe.push(cid.clone());
        }
    }

    if dry_run {
        return Ok(json!({"dry_run": true, "from": {"width": orig.0, "height": orig.1}, "to": {"width": target.0, "height": target.1},
            "aspect_unchanged": aspect_unchanged, "candidates": candidates.len(), "would_reframe": would_reframe, "skipped_already_framed": skipped_already_framed}));
    }

    ss.project.width = target.0; ss.project.height = target.1;
    let mut reframed = vec![];
    for cid in &would_reframe {
        match crate::commands::dispatch_nested(ss, "auto_reframe", &json!({"clip_id": cid, "motion": motion})) {
            Ok(_) => reframed.push(cid.clone()),
            Err(e) => failed.push(json!({"clip_id": cid, "error": e})),
        }
    }
    Ok(json!({"from": {"width": orig.0, "height": orig.1}, "to": {"width": target.0, "height": target.1}, "aspect_unchanged": aspect_unchanged,
        "reframed": reframed, "skipped_already_framed": skipped_already_framed, "failed": failed, "candidates": candidates.len()}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "reframe_timeline", description: "Convert the whole timeline to a new aspect ratio for real, as one ordinary undoable edit. Changes the project canvas (width/height) permanently and keys auto_reframe on every media clip in scope (unlike export_variants, nothing here is restored afterward).", mutates: true,
        schema: obj(vec![
            ("aspect", json!({"type":"string","enum":["16:9","9:16","1:1","4:5"],"description":"a named preset; or give width+height directly"})),
            ("width", n("explicit target width, with height")), ("height", n("explicit target height, with width")),
            ("track_id", s("restrict to one track; default every track")),
            ("motion", json!({"type":"string","enum":crate::reframe::MOTIONS,"description":"how auto_reframe tracks the subject on each clip; default \"default\""})),
            ("auto_reframe", json!({"type":"boolean","description":"default true: key auto_reframe on every media clip for the new aspect. false just resizes the canvas and leaves every clip's existing fit (contain/cover/stretch)"})),
            ("overwrite_existing", json!({"type":"boolean","description":"default false: a clip that already has an active reframe window (set by hand or by an earlier reframe_timeline/auto_reframe call) is left alone and listed under skipped_already_framed, so a manually-framed shot is never silently overwritten"})),
            ("dry_run", json!({"type":"boolean","description":"report would_reframe/skipped_already_framed without changing anything"}))], &[]),
        run: reframe_timeline },
] }
