//! v7 additions: export_variants -- batch-export several size/aspect-ratio deliverables from one project
//! in a single call (e.g. 1080p landscape, a 9:16 vertical cut, a 1:1 square) -- the brief's own example.
//! `batch` explicitly forbids export*, and no existing tool renders more than one output size per call, so
//! the only way to do this today is several separate export calls with the project's width/height hand-
//! edited (and hand-restored) in between. This tool does exactly that, safely: each variant temporarily
//! sets the project canvas to its target size, optionally re-runs auto_reframe on every media clip for the
//! new aspect (removed again before moving on, so nothing is left behind), exports, and the project's own
//! width/height/clip state is restored to exactly what it was before this call returned -- net effect on
//! the project is nothing; the only lasting result is the rendered files.
use super::kit::*;
use crate::model::Source;

/// name -> (width, height) for the social-style aspect shorthands the brief names.
const ASPECT_PRESETS: &[(&str, (u32, u32))] = &[("16:9", (1920, 1080)), ("9:16", (1080, 1920)), ("1:1", (1080, 1080)), ("4:5", (1080, 1350))];

fn variant_size(v: &Value, cur: (u32, u32)) -> Result<(u32, u32), String> {
    if let Some(a) = v.get("aspect").and_then(|x| x.as_str()) {
        return ASPECT_PRESETS.iter().find(|(n, _)| *n == a).map(|(_, wh)| *wh).ok_or_else(|| format!("unknown aspect '{a}'. Valid: {}", ASPECT_PRESETS.iter().map(|(n, _)| *n).collect::<Vec<_>>().join(", ")));
    }
    let w = v.get("width").and_then(|x| x.as_u64()); let h = v.get("height").and_then(|x| x.as_u64());
    match (w, h) {
        (Some(w), Some(h)) => { if w == 0 || h == 0 || w > 8192 || h > 8192 { return Err("width/height must be 1..8192".into()); } Ok((w as u32, h as u32)) }
        (None, None) => Ok(cur), // no size given: export at the project's current canvas size (still useful to batch named deliverables)
        _ => Err("give both width and height, or neither (nor just one)".into()),
    }
}

fn export_variants(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let variants = a.get("variants").and_then(|v| v.as_array()).filter(|v| !v.is_empty()).ok_or("variants must be a non-empty array")?;
    if variants.len() > 20 { return Err("at most 20 variants per call".into()); }
    let path_template = a.get("path_template").and_then(|v| v.as_str()).unwrap_or("{name}.mp4").to_string();
    if !path_template.contains("{name}") && variants.iter().any(|v| v.get("path").is_none()) { return Err("path_template must contain {name}, or give every variant its own path".into()); }
    let auto_reframe = a.get("auto_reframe").and_then(|v| v.as_bool()).unwrap_or(false);
    let crf = opt_num(a, "crf", 18.0) as u32; let low_memory = a.get("low_memory").and_then(|v| v.as_bool()).unwrap_or(false);
    let has_range = a.get("start").is_some() || a.get("end").is_some();
    let (start, end) = (opt_num(a, "start", 0.0), opt_num(a, "end", ss.project.duration()));
    let with_audio = a.get("audio").and_then(|v| v.as_bool()).unwrap_or(true);

    let mut names = std::collections::HashSet::new();
    for v in variants { let n = v.get("name").and_then(|x| x.as_str()).ok_or("every variant needs a name")?; if !names.insert(n.to_string()) { return Err(format!("duplicate variant name '{n}'")); } }

    let orig = (ss.project.width, ss.project.height);
    let media_clips: Vec<String> = ss.project.tracks.iter().flat_map(|t| &t.clips).filter(|c| matches!(c.source, Source::Media { .. })).map(|c| c.id.clone()).collect();
    let mut results = vec![];

    for v in variants {
        let name = v.get("name").and_then(|x| x.as_str()).unwrap();
        let path = v.get("path").and_then(|x| x.as_str()).map(String::from).unwrap_or_else(|| path_template.replace("{name}", name));
        let size = match variant_size(v, orig) { Ok(s) => s, Err(e) => { results.push(json!({"name": name, "ok": false, "error": e})); continue; } };
        ss.project.width = size.0; ss.project.height = size.1;
        let aspect_changed = (size.0 as f64 * orig.1 as f64 - size.1 as f64 * orig.0 as f64).abs() > 1e-6;
        let mut reframed = vec![];
        if auto_reframe && aspect_changed { for cid in &media_clips { if crate::commands::dispatch_nested(ss, "auto_reframe", &json!({"clip_id": cid, "motion": "default"})).is_ok() { reframed.push(cid.clone()); } } }

        let exp = if has_range { crate::render::export_range_opts(&ss.project, &path, crf, start, end, low_memory, with_audio) } else { crate::render::export_opts(&ss.project, &path, crf, low_memory) };
        let r = match exp {
            Ok(dur) => std::fs::metadata(&path).map(|m| json!({"name": name, "ok": true, "path": path, "width": size.0, "height": size.1, "duration": dur, "bytes": m.len()}))
                .unwrap_or_else(|e| json!({"name": name, "ok": false, "error": format!("export wrote no readable file at {path}: {e}")})),
            Err(e) => json!({"name": name, "ok": false, "error": e}),
        };
        for cid in &reframed { let _ = crate::commands::dispatch_nested(ss, "auto_reframe", &json!({"clip_id": cid, "remove": true})); }
        results.push(r);
    }
    ss.project.width = orig.0; ss.project.height = orig.1; // always restored, success or failure
    let ok = results.iter().all(|r| r["ok"] == true);
    Ok(json!({"ok": ok, "variants": results, "no_history": true}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "export_variants", description: "Export several size/aspect-ratio deliverables from the current project in one call (e.g. a 1080p landscape master, a 9:16 vertical cut and a 1:1 square for socials) -- `batch` explicitly forbids export* tools, and no other tool renders more than one output size per call. Each variant is {name (required, used for the default filename and in results), width+height, or aspect: one of \"16:9\"/\"9:16\"/\"1:1\"/\"4:5\" (a small preset table), or neither to export at the project's current size, optional path (overrides path_template)}. path_template (default \"{name}.mp4\") fills in {name} for any variant without its own path. crf/start/end/audio/low_memory apply uniformly, same meaning as `export`. auto_reframe (default false): when a variant's aspect differs from the project's own, run auto_reframe on every media clip for that export so the subject stays framed instead of being letterboxed/cropped blindly -- the reframe window is removed again right after that variant's export, so nothing is left on the clips. The project's width/height (and any auto_reframe windows) are always restored to exactly what they were before this call, whether every variant succeeds or not; variants are independent, so one failing (bad size, a write error) does not stop the rest -- each result carries ok and either path/width/height/duration/bytes or error. Nothing is exported until this call runs: it is not a batch op, it is its own tool.", mutates: true,
        schema: obj(vec![("variants", json!({"type":"array","minItems":1,"maxItems":20,"items":{"type":"object","properties":{"name":{"type":"string"},"width":{"type":"integer"},"height":{"type":"integer"},"aspect":{"type":"string","enum":["16:9","9:16","1:1","4:5"]},"path":{"type":"string"}},"required":["name"]}})), ("path_template", s("default \"{name}.mp4\"; {name} is replaced per variant")), ("crf", n("quality 0..51, default 18")), ("start", n("optional range start seconds")), ("end", n("optional range end seconds")), ("audio", json!({"type":"boolean"})), ("low_memory", json!({"type":"boolean"})), ("auto_reframe", json!({"type":"boolean","description":"default false: track the subject on every media clip when a variant's aspect differs from the project's own, then remove the window again"}))], &["variants"]), run: export_variants },
] }
