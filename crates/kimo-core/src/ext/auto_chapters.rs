//! v7.3: `auto_chapters` -- mux_chapters needs at least 2 timeline markers to already exist; today an
//! agent with no markers yet has to notice that, call mark_scene_cuts itself, and only then call
//! mux_chapters. This composes the two: if the timeline already has enough markers, it chapters the
//! export from those (no detection run, no markers touched); if it does not, it runs mark_scene_cuts
//! first (same scene-cut detector, same thresholds) to create them, then chapters from the result.
//! Pure composition through dispatch_nested -- no new marker/chapter logic, same pattern mark_scene_cuts
//! and self_review already established.
use super::kit::*;
use crate::commands::{dispatch_nested, Session};

fn auto_chapters(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let path = str_arg(a, "path")?.to_string();
    let min_markers = a.get("min_markers").and_then(|v| v.as_u64()).unwrap_or(2) as usize;
    let sensitivity = a.get("sensitivity").and_then(|v| v.as_f64());
    let name_prefix = a.get("name_prefix").and_then(|v| v.as_str()).unwrap_or("chapter").to_string();
    let dry_run = a.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);

    let existing = ss.project.markers.len();
    let mut detection: Value = json!({"ran": false, "reason": format!("{existing} marker(s) already on the timeline, >= min_markers ({min_markers})")});
    let mut used_existing = true;

    if existing < min_markers {
        used_existing = false;
        if dry_run {
            let mut q = json!({"source": "timeline", "name_prefix": name_prefix, "dry_run": true});
            if let Some(s) = sensitivity { q["sensitivity"] = json!(s); }
            let det = dispatch_nested(ss, "mark_scene_cuts", &q)?;
            let would_have = existing + det.get("would_place").and_then(|v| v.as_u64()).unwrap_or(0) as usize;
            return Ok(json!({"dry_run": true, "used_existing_markers": false, "existing_markers": existing,
                "scene_cut_detection": det, "would_have_markers": would_have,
                "would_chapter": would_have >= min_markers,
                "note": if would_have >= min_markers { "would run mark_scene_cuts, then mux_chapters".to_string() }
                        else { format!("even after scene-cut detection only {would_have} marker(s) would exist; still short of min_markers ({min_markers}) -- mux_chapters would be skipped") }}));
        }
        let mut q = json!({"source": "timeline", "name_prefix": name_prefix});
        if let Some(s) = sensitivity { q["sensitivity"] = json!(s); }
        let det = dispatch_nested(ss, "mark_scene_cuts", &q)?;
        detection = det;
    } else if dry_run {
        return Ok(json!({"dry_run": true, "used_existing_markers": true, "existing_markers": existing, "scene_cut_detection": detection, "would_chapter": true}));
    }

    let total_markers = ss.project.markers.len();
    if total_markers < min_markers {
        return Ok(json!({"ok": false, "chaptered": false, "used_existing_markers": used_existing, "existing_markers": existing,
            "markers_now": total_markers, "scene_cut_detection": detection,
            "reason": format!("only {total_markers} marker(s) even after scene-cut detection -- mux_chapters needs at least {min_markers}; this is likely a single continuous shot with no detectable cuts")}));
    }

    let mux = dispatch_nested(ss, "mux_chapters", &json!({"path": path}))?;
    Ok(json!({"ok": true, "chaptered": true, "used_existing_markers": used_existing, "existing_markers": existing,
        "markers_now": total_markers, "scene_cut_detection": detection, "mux_chapters": mux}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "auto_chapters", description: "Write chapters into an already-exported file, from this timeline's markers -- detecting them first if none exist yet. mux_chapters alone needs at least `min_markers` (default 2) markers already on the timeline; today an agent with a bare timeline has to notice that, call mark_scene_cuts itself, then call mux_chapters. This does both in one call: if enough markers already exist, chapters straight from them (no detection run, nothing added); otherwise runs mark_scene_cuts (source timeline) to create them first, then chapters from the result. If even scene-cut detection does not reach min_markers (e.g. one continuous shot with no cuts), returns ok:false with chaptered:false and why, rather than failing loudly or guessing chapter boundaries. `sensitivity` (0..1) passes through to mark_scene_cuts when detection runs; `name_prefix` (default \"chapter\") names any markers it creates. `dry_run` reports whether it would use existing markers or run detection, and whether chaptering would succeed, without creating markers or touching the exported file.", mutates: true,
        schema: obj(vec![("path", s("path of an already-exported video file to add chapters to, in place")), ("min_markers", json!({"type":"integer","description":"minimum markers needed before chaptering; default 2 (mux_chapters' own minimum)"})), ("sensitivity", n("0..1, passed to mark_scene_cuts if it runs, default .5")), ("name_prefix", s("prefix for any markers created by detection, default \"chapter\"")), ("dry_run", json!({"type":"boolean","description":"report the plan, create nothing, write nothing"}))], &["path"]), run: auto_chapters },
] }
