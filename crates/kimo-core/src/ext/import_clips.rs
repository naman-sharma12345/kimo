//! import_clips (v7.5): copy specific clips, with their full effect/keyframe/mask/grade state -- a clip
//! serializes all of that directly, so nothing extra needs copying -- from ANOTHER project revision into
//! the live project at a chosen track and time. project_merge already reconciles two revisions of the SAME
//! lineage (add/remove/change by matching clip id); nothing today does a direct "grab these specific clips
//! out of that other project and drop them into this one", independent of lineage, and with no risk of a
//! clip id collision (project_merge assumes shared ids on purpose; a straight clip copy must not).
//!
//! Reuses project_diff's own `resolve_rev` for loading the source (same forms: current | undo:<n> |
//! checkpoint:<name> | file:<path.kimo>) rather than re-implementing project loading. Every copied clip
//! gets a FRESH id in the live project (two independent projects can easily reuse the same ids, e.g. both
//! built from "clip7"), and a media clip's asset is imported into the destination's own asset pool too --
//! reused by path if the destination already has that exact file, created fresh otherwise -- so the copy
//! never carries a dangling asset reference.
//!
//! Honest gap: a clip's OWN grade_graph (embedded directly on the clip) travels with it; a grade bound
//! through the project-level shared-grade-scopes feature (bind_shared_grade_graphs) does not, since that
//! binding lives in the source project's own grade_scopes structure, not on the clip itself.
use super::kit::*;
use crate::ext::project_diff::resolve_rev;
use crate::model::Source;

fn ids_arg(a: &Value, k: &str) -> Result<Vec<String>, String> {
    a.get(k).and_then(|v| v.as_array()).ok_or_else(|| crate::errors::arg_err(a, k, "array of clip id strings"))?
        .iter().map(|v| v.as_str().map(String::from).ok_or_else(|| crate::errors::arg_err(a, k, "array of clip id strings")))
        .collect()
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let source_rev = a.get("source").and_then(|v| v.as_str()).ok_or_else(|| crate::errors::arg_err(a, "source", "a revision string: current | undo:<n> | checkpoint:<name> | file:<path.kimo>"))?;
    let clip_ids = ids_arg(a, "clip_ids")?;
    if clip_ids.is_empty() { return Err("clip_ids is empty: name at least one clip to import".into()); }
    let time_offset = a.get("time_offset").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if !time_offset.is_finite() { return Err("time_offset must be a finite number".into()); }

    let source = resolve_rev(ss, source_rev)?;

    // Destination track: the one named, or a fresh one (created only once, even for several clips).
    let (dest_ti, created_track) = match a.get("track_id") {
        None | Some(Value::Null) => {
            let id = ss.project.gen_id("track");
            ss.project.tracks.push(crate::model::Track { id: id.clone(), clips: vec![], locked: false, muted: false, solo: false, name: String::new() });
            (ss.project.tracks.len() - 1, Some(id))
        }
        Some(Value::String(tid)) => {
            let ti = ss.project.tracks.iter().position(|t| &t.id == tid).ok_or_else(|| format!("no such track '{tid}'"))?;
            if ss.project.tracks[ti].locked { return Err(format!("track '{tid}' is locked")); }
            (ti, None)
        }
        _ => return Err(crate::errors::arg_err(a, "track_id", "string")),
    };

    let mut assets_imported = 0usize;
    let mut assets_reused = 0usize;
    let mut imported: Vec<Value> = vec![];

    for cid in &clip_ids {
        let src_clip = source.find_clip(cid).ok_or_else(|| format!("no such clip '{cid}' in the source project"))?;
        let mut new_clip: crate::model::Clip = src_clip.clone();
        let new_id = ss.project.gen_id("clip");

        if let Source::Media { asset } = &new_clip.source {
            let src_asset = source.assets.get(asset).ok_or_else(|| format!("clip '{cid}' references asset '{asset}' which is missing from its own source project"))?.clone();
            // Reuse an asset already in the destination with the exact same file path; otherwise import it fresh.
            let dest_asset_id = match ss.project.assets.iter().find(|(_, a)| a.path == src_asset.path).map(|(id, _)| id.clone()) {
                Some(id) => { assets_reused += 1; id }
                None => {
                    let id = ss.project.gen_id("asset");
                    ss.project.assets.insert(id.clone(), crate::model::Asset { origin: src_asset.origin, hash: src_asset.hash, kind: src_asset.kind, id: id.clone(), path: src_asset.path, width: src_asset.width, height: src_asset.height, duration: src_asset.duration, fps: src_asset.fps, bin: None });
                    assets_imported += 1;
                    id
                }
            };
            new_clip.source = Source::Media { asset: dest_asset_id };
        }

        new_clip.id = new_id.clone();
        new_clip.start = (src_clip.start + time_offset).max(0.0);
        new_clip.transition_in = None; // a transition references the previous clip on ITS track; meaningless once moved to a new project/track

        imported.push(json!({"source_clip_id": cid, "clip_id": new_id, "start": new_clip.start}));
        ss.project.tracks[dest_ti].clips.push(std::sync::Arc::new(new_clip));
    }

    Ok(json!({
        "imported": imported,
        "track_id": ss.project.tracks[dest_ti].id,
        "created_track": created_track,
        "assets_imported": assets_imported,
        "assets_reused": assets_reused,
    }))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "import_clips",
        description: "Copy specific clips into the live project, from another project entirely if you like. Carries everything a clip itself holds -- effects, keyframes, masks, its own grade graph, speed. source is a revision string in project_diff's own forms: current | undo:<n> | checkpoint:<name> | file:<path.kimo> -- so this can pull clips out of an entirely separate project file, not just an earlier state of this one. Every copied clip gets a FRESH id in the live project (two independent project files can easily reuse the same ids); a media clip's asset is imported into the destination's own asset pool too, reused by file path if the destination already has that exact file, created fresh otherwise, so the copy never carries a dangling asset reference. track_id: the destination track, or omit it to create one new track for this call's clips. time_offset shifts every copied clip's original start by the same amount (default 0: clips land at their original timeline positions). A grade bound through the project-level shared-grade-scopes feature does not travel, since that binding lives on the source project itself, not on the clip; a clip's own embedded grade graph does.",
        mutates: true,
        schema: obj(vec![
            ("source", s("revision to copy from: current | undo:<n> | checkpoint:<name> | file:<path.kimo>")),
            ("clip_ids", json!({"type":"array","items":{"type":"string"},"description":"clip ids to copy out of the source, at least one"})),
            ("track_id", s("destination track; omit to create one new track for this call")),
            ("time_offset", n("seconds added to every copied clip's original start; default 0")),
        ], &["source", "clip_ids"]),
        run,
    }]
}
