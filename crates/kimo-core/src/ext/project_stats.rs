//! project_stats (v7.6): one-call project health/summary for an agent, instead of piecing it together from
//! get_project (the full raw tree), find_assets (per-asset, paginated) and timeline_summary (per-clip, full
//! detail). An agent that just wants "how big is this edit, what's in it, is anything unused" today has to
//! read the whole project and count by hand, or call find_assets with a high limit and filter used_by_clips
//! == 0 itself. This is the aggregate roll-up: counts and totals only, no per-clip detail (timeline_summary
//! already owns that), computed directly off the live project so it is always exact, never a cached guess.
use super::kit::*;

fn r3(v: f64) -> f64 { (v * 1000.0).round() / 1000.0 }

fn source_kind(s: &crate::model::Source) -> &'static str {
    match s {
        crate::model::Source::Media { .. } => "media",
        crate::model::Source::Text { .. } => "text",
        _ => "generated",
    }
}

fn run(ss: &mut Session, _a: &Value) -> Result<Value, String> {
    let p = &ss.project;

    let mut duration = 0.0f64;
    let mut clip_total = 0usize;
    let mut by_kind: std::collections::BTreeMap<&'static str, usize> = std::collections::BTreeMap::new();
    let mut used_assets: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut muted_tracks = 0usize;
    let mut solo_tracks = 0usize;
    let mut locked_tracks = 0usize;
    let mut clips_with_effects = 0usize;
    let mut clips_keyframed = 0usize;

    for t in &p.tracks {
        if t.muted { muted_tracks += 1; }
        if t.solo { solo_tracks += 1; }
        if t.locked { locked_tracks += 1; }
        for c in &t.clips {
            clip_total += 1;
            duration = duration.max(c.start + c.duration);
            *by_kind.entry(source_kind(&c.source)).or_insert(0) += 1;
            if let crate::model::Source::Media { asset } = &c.source { used_assets.insert(asset.clone()); }
            if !c.effects.is_empty() { clips_with_effects += 1; }
            if c.props.values().any(|pr| !pr.keys.is_empty()) { clips_keyframed += 1; }
        }
    }

    let all_asset_ids: Vec<String> = p.assets.keys().cloned().collect();
    let unused: Vec<Value> = all_asset_ids.iter()
        .filter(|id| !used_assets.contains(*id))
        .map(|id| { let a = &p.assets[id]; json!({"asset_id": id, "path": a.path}) })
        .collect();

    let caption_group_count = p.editorial.captions.len();
    let caption_cue_count: usize = p.editorial.captions.values().map(|g| g.clips.len()).sum();

    Ok(json!({
        "width": p.width, "height": p.height, "fps": p.fps,
        "duration": r3(duration),
        "track_count": p.tracks.len(),
        "tracks_muted": muted_tracks, "tracks_solo": solo_tracks, "tracks_locked": locked_tracks,
        "clip_count": clip_total,
        "clip_count_by_kind": by_kind,
        "clips_with_effects": clips_with_effects,
        "clips_with_keyframes": clips_keyframed,
        "asset_count": all_asset_ids.len(),
        "used_asset_count": used_assets.len(),
        "unused_asset_count": unused.len(),
        "unused_assets": unused,
        "bin_count": p.bins.len(),
        "marker_count": p.markers.len(),
        "caption_group_count": caption_group_count,
        "caption_cue_count": caption_cue_count,
        "subclip_count": p.subclips.len(),
    }))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "project_stats",
        description: "Read-only one-call project summary for an agent. Canvas (width/height/fps), total timeline duration, track count with how many are muted/solo/locked, total clip count and a breakdown by source kind (media/text/generated), how many clips carry an effect or a keyframed property, total asset count against how many are actually used by a clip somewhere on the timeline (unused_assets lists the rest by id and path -- media imported but never placed, or removed from every track since), bin count, marker count, and caption group/cue counts. Equivalent to reading get_project plus calling find_assets and counting by hand, in one call; does not replace timeline_summary (per-clip detail) or find_assets (per-asset search) when that detail is what is actually needed. Nothing is changed.",
        mutates: false,
        schema: obj(vec![], &[]),
        run,
    }]
}
