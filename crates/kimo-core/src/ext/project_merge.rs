//! N1.3: project_merge - apply the clip-level differences between two project revisions onto the live
//! project. project_diff (N1.2) is read-only; nothing in the engine applies a diff, so a worktree-style
//! merge ("I made these changes over here, bring them into what I'm looking at now") needed its own call.
//! Tracks and clips are matched by id, exactly like project_diff. Scope is deliberately bounded to clips
//! and tracks (add/remove/change), the same surface project_diff already reports on -- project-wide
//! settings (grade graphs, hooks, lights, color pipeline, ...) are not merged by this tool.
use super::kit::*;
use crate::commands::Session;
use crate::ext::project_diff::resolve_rev;
use crate::model::{Project, Track};
use std::collections::BTreeMap;

/// clip id -> (owning track id, clip as JSON) for every clip in a project. JSON equality is used as the
/// content-identity check throughout (same technique as project_diff's own jdiff).
fn clip_map(p: &Project) -> BTreeMap<String, (String, Value)> {
    let mut m = BTreeMap::new();
    for t in &p.tracks { for c in &t.clips { m.insert(c.id.clone(), (t.id.clone(), serde_json::to_value(&**c).unwrap_or(Value::Null))); } }
    m
}

fn conflict(id: &str, kind: &str, reason: &str) -> Value { json!({"id": id, "kind": kind, "reason": reason}) }

/// Inserts theirs' current version of clip `id` into the live project (removing any existing occurrence
/// first), creating the destination track if it does not exist yet. Returns the destination track id.
fn apply_clip(ss: &mut Session, theirs: &Project, id: &str, created_tracks: &mut Vec<String>) {
    for t in ss.project.tracks.iter_mut() { t.clips.retain(|c| c.id != id); }
    let Some((their_ti, _)) = theirs.tracks.iter().enumerate().find(|(_, t)| t.clips.iter().any(|c| c.id == id)) else { return };
    let their_track = &theirs.tracks[their_ti];
    let Some(clip_arc) = their_track.clips.iter().find(|c| c.id == id).cloned() else { return };
    if !ss.project.tracks.iter().any(|t| t.id == their_track.id) {
        ss.project.tracks.push(Track { id: their_track.id.clone(), clips: vec![], locked: false, muted: their_track.muted, name: their_track.name.clone() });
        created_tracks.push(their_track.id.clone());
    }
    let ti = ss.project.tracks.iter().position(|t| t.id == their_track.id).unwrap();
    ss.project.tracks[ti].clips.push(clip_arc);
}

fn project_merge(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let base_rev = str_arg(a, "base")?; let theirs_rev = str_arg(a, "theirs")?;
    let base = resolve_rev(ss, base_rev)?; let theirs = resolve_rev(ss, theirs_rev)?;
    let on_conflict = a.get("on_conflict").and_then(|v| v.as_str()).unwrap_or("fail");
    if !["fail", "theirs", "base"].contains(&on_conflict) { return Err(format!("on_conflict '{on_conflict}' is not allowed. Valid: fail, theirs, base")); }
    let dry_run = a.get("dry_run").and_then(|v| v.as_bool()).unwrap_or(false);

    let mb = clip_map(&base); let mt = clip_map(&theirs); let mc = clip_map(&ss.project);
    let mut conflicts = vec![]; let mut apply_ids: Vec<String> = vec![]; let mut remove_ids: Vec<String> = vec![];

    // Clips theirs added or changed relative to base.
    for (id, (_, their_v)) in &mt {
        match mb.get(id) {
            None => match mc.get(id) { // not in base: a clip theirs introduced
                None => apply_ids.push(id.clone()), // clean add
                Some((_, cur_v)) => if cur_v != their_v {
                    conflicts.push(conflict(id, "add_add_conflict", "both the current project and theirs added a clip with this id, with different content"));
                    if on_conflict == "theirs" { apply_ids.push(id.clone()); }
                },
            },
            Some((_, base_v)) => if their_v != base_v { // theirs changed this clip since base
                match mc.get(id) {
                    None => { conflicts.push(conflict(id, "edit_removed_conflict", "theirs changed this clip, but the current project already removed it")); if on_conflict == "theirs" { apply_ids.push(id.clone()); } },
                    Some((_, cur_v)) => {
                        if cur_v == base_v { apply_ids.push(id.clone()); } // current untouched: clean fast-forward
                        else if cur_v != their_v {
                            conflicts.push(conflict(id, "edit_edit_conflict", "both the current project and theirs changed this clip differently since base"));
                            if on_conflict == "theirs" { apply_ids.push(id.clone()); }
                        } // else: current already matches theirs, nothing to do
                    }
                }
            },
        }
    }
    // Clips theirs removed relative to base.
    for (id, (_, base_v)) in &mb { if !mt.contains_key(id) {
        if let Some((_, cur_v)) = mc.get(id) {
            if cur_v == base_v { remove_ids.push(id.clone()); }
            else {
                conflicts.push(conflict(id, "remove_modify_conflict", "theirs removed this clip, but the current project has changed it since base"));
                if on_conflict == "theirs" { remove_ids.push(id.clone()); }
            }
        }
    } }

    if on_conflict == "fail" && !conflicts.is_empty() {
        return Ok(json!({"ok": false, "applied": false, "conflicts": conflicts, "message": format!("{} conflict(s); nothing applied (on_conflict:\"fail\", the default). Pass on_conflict:\"theirs\" or \"base\", or resolve by hand and retry.", conflicts.len()), "no_history": true}));
    }
    if dry_run {
        return Ok(json!({"ok": true, "applied": false, "dry_run": true, "would_apply": apply_ids.len(), "would_remove": remove_ids.len(), "conflicts": conflicts, "no_history": true}));
    }
    if apply_ids.is_empty() && remove_ids.is_empty() {
        return Ok(json!({"ok": true, "applied": true, "applied_clips": Value::Array(vec![]), "removed_clips": Value::Array(vec![]), "created_tracks": Value::Array(vec![]), "conflicts": conflicts, "note": "nothing to merge", "no_history": true}));
    }

    let mut created_tracks = vec![];
    for id in &apply_ids { apply_clip(ss, &theirs, id, &mut created_tracks); }
    for id in &remove_ids { for t in ss.project.tracks.iter_mut() { t.clips.retain(|c| &c.id != id); } }
    for t in ss.project.tracks.iter_mut() { t.clips.sort_by(|x, y| x.start.partial_cmp(&y.start).unwrap_or(std::cmp::Ordering::Equal)); }
    ss.project.recompute_next_id();
    Ok(json!({"ok": true, "applied": true, "applied_clips": apply_ids, "removed_clips": remove_ids, "created_tracks": created_tracks, "conflicts": conflicts, "conflicts_resolved_by": if conflicts.is_empty() { Value::Null } else { json!(on_conflict) }}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "project_merge", description: "Apply the clip-level changes between two project revisions onto the live project -- the write-side of project_diff, which only reports. base and theirs are each current | undo:<n> (undo:1 = the state before the last change) | checkpoint:<name> | file:<path.kimo>, exactly like project_diff. Tracks and clips are matched by id. A clip theirs added (not in base) is added here too, unless the current project already added a different clip under the same id (an add_add_conflict). A clip theirs changed since base is carried over here, unless the current project changed it differently since base (an edit_edit_conflict) or already removed it (edit_removed_conflict). A clip theirs removed since base is removed here too, unless the current project changed it since base (a remove_modify_conflict). A destination track is created (copying theirs' track name) if it does not already exist here. on_conflict: \"fail\" (default) applies nothing and reports every conflict; \"theirs\" resolves every conflict in theirs' favour; \"base\" keeps the current project's own version on every conflict (applies everything else). dry_run reports what would happen (would_apply, would_remove, conflicts) without changing anything. Project-wide settings outside tracks/clips (grade graphs, hooks, lights, color pipeline, ...) are not merged.", mutates: true,
        schema: obj(vec![("base", s("older/common-ancestor revision: current | undo:<n> | checkpoint:<name> | file:<path.kimo>")), ("theirs", s("the other revision to merge in, same forms")), ("on_conflict", json!({"type":"string","enum":["fail","theirs","base"],"description":"fail (default): apply nothing and report conflicts; theirs: theirs' version wins every conflict; base: keep the current project's own version on every conflict"})), ("dry_run", json!({"type":"boolean"}))], &["base", "theirs"]), run: project_merge },
] }
