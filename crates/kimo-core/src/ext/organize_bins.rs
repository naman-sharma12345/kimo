//! organize_bins (v7.6): file every asset in the media pool into bins automatically, by kind (video/
//! image/audio) or by usage (used/unused), creating the bins as needed, in one call. create_bin and
//! move_asset_to_bin already do the mechanics one asset at a time; a library of any real size makes that
//! "create a bin per kind, then move_asset_to_bin once per asset" by hand. This reuses move_asset_to_bin
//! itself (dispatch_nested) for the actual move, and deps::asset_kind (the project's own file-extension
//! classifier, already used for dependency reporting) for the by:"kind" split, rather than reimplementing
//! extension matching a second time.
use super::kit::*;
use crate::commands::dispatch_nested as call;

fn kind_bin_name(path: &str) -> &'static str {
    match crate::deps::asset_kind(path) {
        crate::deps::RefKind::Image => "Image",
        crate::deps::RefKind::Audio => "Audio",
        _ => "Video",
    }
}

fn used_asset_ids(ss: &Session) -> std::collections::BTreeSet<String> {
    let mut used = std::collections::BTreeSet::new();
    for t in &ss.project.tracks {
        for c in &t.clips {
            if let crate::model::Source::Media { asset } = &c.source { used.insert(asset.clone()); }
        }
    }
    used
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let by = a.get("by").and_then(|v| v.as_str()).unwrap_or("kind");
    if by != "kind" && by != "usage" { return Err(format!("by must be \"kind\" or \"usage\", got \"{by}\"")); }
    let only_unfiled = a.get("only_unfiled").and_then(|v| v.as_bool()).unwrap_or(true);
    let dry_run = a.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);

    let used = if by == "usage" { used_asset_ids(ss) } else { Default::default() };

    // Decide the target bin for every asset against a snapshot first, so dry_run and the real run compute
    // the exact same plan and nothing is read mid-mutation.
    let mut plan: Vec<(String, String)> = vec![]; // (asset_id, target_bin)
    let mut skipped_already_filed: Vec<String> = vec![];
    for (id, asset) in &ss.project.assets {
        if only_unfiled && asset.bin.is_some() { skipped_already_filed.push(id.clone()); continue; }
        let target = match by {
            "usage" => if used.contains(id) { "Used" } else { "Unused" },
            _ => kind_bin_name(&asset.path),
        };
        if asset.bin.as_deref() == Some(target) { continue; } // already exactly there, nothing to do
        plan.push((id.clone(), target.to_string()));
    }
    plan.sort();

    let mut counts: std::collections::BTreeMap<String, usize> = std::collections::BTreeMap::new();
    for (_, bin) in &plan { *counts.entry(bin.clone()).or_insert(0) += 1; }

    if dry_run {
        return Ok(json!({
            "dry_run": true, "by": by,
            "would_move": plan.iter().map(|(id, bin)| json!({"asset_id": id, "bin": bin})).collect::<Vec<_>>(),
            "counts": counts,
            "skipped_already_filed": skipped_already_filed.len(),
            "no_history": true,
        }));
    }

    let mut moved: Vec<Value> = vec![];
    for (id, bin) in &plan {
        call(ss, "move_asset_to_bin", &json!({"asset_id": id, "bin": bin}))?;
        moved.push(json!({"asset_id": id, "bin": bin}));
    }

    Ok(json!({
        "dry_run": false, "by": by,
        "moved": moved,
        "counts": counts,
        "skipped_already_filed": skipped_already_filed.len(),
    }))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "organize_bins",
        description: "File every asset in the media pool into bins automatically, in one call. by:\"kind\" (default) sorts into Video/Image/Audio by file extension (deps::asset_kind, the same classifier dependency reporting already uses); by:\"usage\" sorts into Used/Unused by whether a clip on any track references the asset right now. Bins are created on demand (move_asset_to_bin's own behaviour). only_unfiled (default true) leaves an asset alone if it is already filed in some bin, so a sweep never undoes a person's own manual filing; set it false to re-file everything, including assets already in a bin. An asset already sitting in exactly the bin this call would choose is left untouched either way. dry_run reports would_move and counts per bin without moving anything. One atomic undo step for the whole sweep, however many assets move (dispatch_nested per move, the same shape close_gaps and style_captions_batch use for their own per-item loops).",
        mutates: true,
        schema: obj(vec![
            ("by", json!({"type":"string","enum":["kind","usage"],"description":"default \"kind\""})),
            ("only_unfiled", json!({"type":"boolean","description":"default true: skip assets already filed in a bin"})),
            ("dry_run", json!({"type":"boolean","description":"report the plan, change nothing. Default false."})),
        ], &[]),
        run,
    }]
}
