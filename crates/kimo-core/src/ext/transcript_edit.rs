//! Revision-checked transcript deletion. Stage a complete project before committing;
//! the dispatcher records the entire operation as one undo step.
use super::kit::*;
use crate::model::{Clip, Project, Source, TimeMode};
use crate::transcript::{compose_cue_text, CueBinding, TakeStatus, Transcript};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

const EPS: f64 = 1e-9;
type Range = (f64, f64);

#[derive(Clone)]
struct Piece {
    id: String,
    old_start: f64,
    old_end: f64,
}

fn retained(start: f64, end: f64, cuts: &[Range]) -> Vec<Range> {
    let mut cursor = start;
    let mut out = vec![];
    for &(a, b) in cuts {
        if b <= cursor || a >= end {
            continue;
        }
        if a > cursor {
            out.push((cursor, a.min(end)));
        }
        cursor = cursor.max(b);
        if cursor >= end {
            break;
        }
    }
    if cursor < end {
        out.push((cursor, end));
    }
    out
}

fn map_time(t: f64, cuts: &[Range]) -> f64 {
    t - cuts
        .iter()
        .map(|&(a, b)| (t.min(b) - a).clamp(0.0, b - a))
        .sum::<f64>()
}

fn animated(v: &Value) -> bool {
    match v {
        Value::Object(o) => {
            (o.contains_key("base")
                && ["keys", "expr"].iter().any(|k| {
                    o.get(*k)
                        .and_then(Value::as_array)
                        .is_some_and(|a| !a.is_empty())
                }))
                || o.values().any(animated)
        }
        Value::Array(a) => a.iter().any(animated),
        _ => false,
    }
}

fn refers_to(v: &Value, ids: &BTreeSet<String>) -> bool {
    match v {
        Value::String(s) => ids.contains(s),
        Value::Array(a) => a.iter().any(|v| refers_to(v, ids)),
        Value::Object(o) => o.values().any(|v| refers_to(v, ids)),
        _ => false,
    }
}

fn validate_clip(c: &Clip) -> Result<(), String> {
    if !matches!(
        c.source,
        Source::Media { .. }
            | Source::Solid { .. }
            | Source::Gradient { .. }
            | Source::Shape { .. }
            | Source::Text { .. }
    ) || !matches!(c.time_mode, TimeMode::Normal)
        || c.speed != 1.0
        || c.speed_curve.is_some()
        || c.time_remap.is_some()
        || c.audio.reverse
        || c.audio.lead != 0.0
        || c.audio.tail != 0.0
        || c.parent.is_some()
        || c.transition_in.is_some()
        || c.motion_path.is_some()
        || c.echo.is_some()
        || c.posterize_fps != 0.0
        || matches!(&c.source, Source::Shape { shape, .. } if !matches!(shape.as_str(), "rect" | "ellipse" | "polygon" | "line"))
    {
        return Err(format!("unsupported_edit: clip {} needs flat normal-speed media/text/basic shapes without parenting, transitions, split audio, motion paths, echo, posterized time or path shapes", c.id));
    }
    Ok(())
}

fn validate_takes(p: &Project, tr: &Transcript) -> Result<(), String> {
    tr.validate()?;
    let mut used = BTreeSet::new();
    let mut previous_end = 0.0;
    for (id, start) in tr.take_starts() {
        let take = tr.takes.iter().find(|t| t.id == id).ok_or("missing take")?;
        let cid = take.source.clip_id.as_deref().ok_or_else(||
            format!("unbound_take: {id} requires an explicit take.clip_id; asset/path-only transcripts cannot be applied"))?;
        let c = p
            .tracks
            .iter()
            .flat_map(|t| &t.clips)
            .find(|c| c.id == cid)
            .ok_or_else(|| format!("unbound_take: clip {cid} is not a top-level clip"))?;
        if !used.insert(cid) {
            return Err("unsupported_edit: multiple takes reference the same clip".into());
        }
        if take.speed != 1.0 || !matches!(c.source, Source::Media { .. }) {
            return Err("unsupported_edit: takes must reference normal-speed media clips".into());
        }
        validate_clip(c)?;
        if take
            .source
            .asset_id
            .as_ref()
            .is_some_and(|id| !matches!(&c.source, Source::Media { asset } if asset == id))
        {
            return Err(format!(
                "unbound_take: {id} asset does not match clip {cid}"
            ));
        }
        if start < previous_end - EPS
            || start < 0.0
            || take.src_start < c.in_point - EPS
            || take.src_end > c.in_point + c.duration + EPS
            || (c.start + take.src_start - c.in_point - start).abs() > EPS
        {
            return Err(format!("unbound_take: {id} source/timeline mapping differs from clip {cid}, or takes overlap/out of order"));
        }
        for word in tr.words.iter().filter(|w| w.take_id == id) {
            if word.src_start < take.src_start - EPS
                || word.src_end > take.src_end + EPS
                || (word.start - take.to_timeline(start, word.src_start)).abs() > EPS
                || (word.end - take.to_timeline(start, word.src_end)).abs() > EPS
            {
                return Err(format!(
                    "unbound_take: word {} has stale or out-of-take timing",
                    word.id
                ));
            }
        }
        previous_end = start + take.timeline_len();
    }
    if tr.takes.iter().any(|t| t.status != TakeStatus::Chosen) {
        return Err("unsupported_edit: alternate/rejected takes must be resolved before applying transcript cuts".into());
    }
    Ok(())
}

fn rebuild_transcript(
    tr: &Transcript,
    cuts: &[Range],
    pieces: &BTreeMap<String, Vec<Piece>>,
) -> Result<Transcript, String> {
    let mut out = tr.clone();
    out.takes.clear();
    out.words.clear();
    let mut used: BTreeSet<String> = tr.takes.iter().map(|t| t.id.clone()).collect();
    for (tid, start) in tr.take_starts() {
        crate::guard::check();
        let take = tr
            .takes
            .iter()
            .find(|t| t.id == tid)
            .ok_or("missing take")?;
        let cid = take.source.clip_id.as_ref().ok_or("unbound take")?;
        for (i, (a, b)) in retained(start, start + take.timeline_len(), cuts)
            .into_iter()
            .enumerate()
        {
            let mut next = take.clone();
            if i > 0 {
                next.id = crate::transcript::next_id("take", used.iter().map(String::as_str));
                used.insert(next.id.clone());
            }
            next.src_start = take.src_start + a - start;
            next.src_end = take.src_start + b - start;
            next.timeline_start = Some(map_time(a, cuts));
            next.order = u32::try_from(out.takes.len()).map_err(|_| "too many takes")?;
            next.source.clip_id = Some(
                pieces
                    .get(cid)
                    .and_then(|ps| {
                        ps.iter()
                            .find(|p| p.old_start <= a + EPS && p.old_end >= b - EPS)
                    })
                    .ok_or("cut plan cannot map take to a surviving clip")?
                    .id
                    .clone(),
            );
            for word in tr.words.iter().filter(|w| w.take_id == tid) {
                // A word straddling an edit boundary would require a partial-word
                // decision. Never silently truncate or duplicate it.
                if word.end <= a || word.start >= b {
                    continue;
                }
                if word.start < a - EPS || word.end > b + EPS {
                    return Err(format!(
                        "unsupported_edit: cut boundary crosses unselected word {}",
                        word.id
                    ));
                }
                let mut w = word.clone();
                w.take_id = next.id.clone();
                out.words.push(w);
            }
            out.takes.push(next);
        }
    }
    out.version = out
        .version
        .checked_add(1)
        .ok_or("transcript version exhausted")?;
    out.derive_times();
    out.validate()?;
    Ok(out)
}

fn rebuild_captions(
    p: &mut Project,
    original: &Project,
    tid: &str,
    pieces: &BTreeMap<String, Vec<Piece>>,
) -> Result<Vec<String>, String> {
    let tr = &p.editorial.transcripts[tid];
    let words: BTreeMap<_, _> = tr.words.iter().map(|w| (w.id.as_str(), w)).collect();
    let mut bindings = BTreeMap::new();
    let mut empty = BTreeSet::new();
    let mut updated = vec![];
    for (cid, binding) in &original.editorial.cue_words {
        crate::guard::check();
        if binding.word_ids.iter().any(|id| {
            !original.editorial.transcripts[tid]
                .words
                .iter()
                .any(|w| &w.id == id)
        }) {
            return Err(
                "unsupported_edit: caption binding contains an unknown transcript word".into(),
            );
        }
        let group = original
            .editorial
            .captions
            .values()
            .find(|g| g.clips.contains(cid))
            .ok_or("unsupported_edit: caption binding has no group")?;
        if group.transcript_id.as_deref() != Some(tid) {
            return Err(
                "unsupported_edit: caption binding belongs to another or unspecified transcript"
                    .into(),
            );
        }
        for piece in pieces.get(cid).into_iter().flatten() {
            let cue = p.find_clip(&piece.id).ok_or("missing caption clip")?;
            let kept: Vec<_> = binding
                .word_ids
                .iter()
                .enumerate()
                .filter_map(|(i, id)| {
                    words
                        .get(id.as_str())
                        .filter(|w| w.end > cue.start && w.start < cue.start + cue.duration)
                        .map(|w| (i, *w))
                })
                .collect();
            if kept.is_empty() {
                empty.insert(piece.id.clone());
                continue;
            }
            let breaks: Vec<_> = kept
                .windows(2)
                .enumerate()
                .filter_map(|(i, w)| {
                    binding
                        .breaks_after
                        .iter()
                        .any(|b| *b >= w[0].0 && *b < w[1].0)
                        .then_some(i)
                })
                .collect();
            let display: Vec<_> = kept.iter().map(|(_, w)| w.display.as_str()).collect();
            let text = if kept.len() == binding.word_ids.len() {
                let Source::Text { text, .. } = &cue.source else {
                    return Err("bound caption is not text".into());
                };
                text.clone()
            } else {
                compose_cue_text(&display, &breaks)
            };
            bindings.insert(
                piece.id.clone(),
                CueBinding {
                    word_ids: kept.iter().map(|(_, w)| w.id.clone()).collect(),
                    breaks_after: breaks,
                },
            );
            updated.push((piece.id.clone(), text));
        }
    }
    for (cid, text) in &updated {
        let c = p.find_clip_mut(cid).ok_or("missing caption clip")?;
        let Source::Text { text: current, .. } = &mut c.source else {
            return Err("bound caption is not text".into());
        };
        *current = text.clone();
    }
    for track in &mut p.tracks {
        track.clips.retain(|c| !empty.contains(&c.id));
    }
    for (gid, group) in &mut p.editorial.captions {
        group.clips = original.editorial.captions[gid]
            .clips
            .iter()
            .flat_map(|cid| {
                pieces
                    .get(cid)
                    .into_iter()
                    .flatten()
                    .filter(|p| !empty.contains(&p.id))
                    .map(|p| p.id.clone())
            })
            .collect();
    }
    p.editorial.cue_words = bindings;
    p.grade_scopes
        .remove_clips(&empty.into_iter().collect::<Vec<_>>());
    Ok(updated.into_iter().map(|(id, _)| id).collect())
}

fn stage(original: &Project, tid: &str, cuts: &[Range]) -> Result<(Project, Value), String> {
    if original.editorial.transcripts.len() != 1
        || !original.editorial.multicams.is_empty()
        || !original.occluders.is_empty()
    {
        return Err("unsupported_edit: first version requires one transcript, flattened multicam and no tracked occluders".into());
    }
    if original
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .any(|c| c.parent.is_some() || c.audio.lead != 0.0 || c.audio.tail != 0.0)
    {
        return Err("unsupported_edit: flatten parented clips and resolve split-audio offsets before ripple editing".into());
    }
    if animated(&serde_json::to_value(&original.camera).map_err(|e| e.to_string())?)
        || animated(&serde_json::to_value(&original.grade_scopes).map_err(|e| e.to_string())?)
    {
        return Err(
            "unsupported_edit: animated project camera/shared grades need explicit retiming".into(),
        );
    }
    let tr = &original.editorial.transcripts[tid];
    validate_takes(original, tr)?;
    let mut p = original.clone();
    let mut pieces = BTreeMap::new();
    let mut changed = vec![];
    let mut created = vec![];
    let mut altered_ids = BTreeSet::new();
    let mut planned_clips = 0usize;
    for ti in 0..original.tracks.len() {
        let track = &original.tracks[ti];
        let mut output = vec![];
        for c in &track.clips {
            crate::guard::check();
            let ranges = retained(c.start, c.start + c.duration, cuts);
            planned_clips = planned_clips
                .checked_add(ranges.len())
                .ok_or("clip count overflow")?;
            if planned_clips > crate::guard::limits().max_clips {
                return Err("cut plan exceeds the project clip limit".into());
            }
            let trimmed = ranges != vec![(c.start, c.start + c.duration)];
            let moved = map_time(c.start, cuts) != c.start;
            if (trimmed || moved) && track.locked {
                return Err(format!("track {} is locked", track.id));
            }
            if trimmed || moved {
                altered_ids.insert(c.id.clone());
            }
            if trimmed {
                validate_clip(c)?;
                if original.editorial.cue_words.contains_key(&c.id)
                    && c.text_fx
                        .as_ref()
                        .is_some_and(|fx| !fx.animators.is_empty())
                {
                    return Err("unsupported_edit: word-animated captions must be rebuilt after transcript cuts; use unanimated transcript_to_captions cues".into());
                }
            }
            let mut cps = vec![];
            for (i, (a, b)) in ranges.into_iter().enumerate() {
                let mut next = (**c).clone();
                if i > 0 {
                    next.id = p.gen_id("clip");
                    next.name = None;
                    created.push(next.id.clone());
                    p.grade_scopes.copy_binding(&c.id, &next.id);
                }
                let offset = a - c.start;
                if trimmed {
                    crate::keyframe::shift_clip_clock(&mut next, offset);
                    next.in_point += offset;
                    next.duration = b - a;
                    if offset > 0.0 {
                        next.audio.fade_in = 0.0;
                    }
                    if b < c.start + c.duration {
                        next.audio.fade_out = 0.0;
                    }
                }
                next.start = map_time(a, cuts);
                cps.push(Piece {
                    id: next.id.clone(),
                    old_start: a,
                    old_end: b,
                });
                output.push(if trimmed || moved {
                    Arc::new(next)
                } else {
                    c.clone()
                });
            }
            pieces.insert(c.id.clone(), cps);
            if trimmed || moved {
                changed.push(c.id.clone());
            }
        }
        p.tracks[ti].clips = output;
    }
    // References to an identity that may now denote only one of several pieces
    // cannot be retargeted by guessing. Editorial/grade bindings are rebuilt below.
    if refers_to(
        &serde_json::to_value(&original.grade_scopes.graphs).map_err(|e| e.to_string())?,
        &altered_ids,
    ) {
        return Err("unsupported_edit: shared grade graph references a clipped/split clip".into());
    }
    for c in original.tracks.iter().flat_map(|t| &t.clips) {
        let mut value = serde_json::to_value(c).map_err(|e| e.to_string())?;
        value.as_object_mut().ok_or("invalid clip")?.remove("id");
        if refers_to(&value, &altered_ids) {
            return Err(format!(
                "unsupported_edit: clip {} references a clipped/split clip",
                c.id
            ));
        }
    }
    p.editorial
        .transcripts
        .insert(tid.into(), rebuild_transcript(tr, cuts, &pieces)?);
    let updated_cues = rebuild_captions(&mut p, original, tid, &pieces)?;
    for marker in &mut p.markers {
        let end = map_time(marker.time + marker.duration, cuts);
        marker.time = map_time(marker.time, cuts);
        marker.duration = (end - marker.time).max(0.0);
    }
    let surviving: BTreeSet<_> = p
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .map(|c| c.id.clone())
        .collect();
    let removed: Vec<_> = original
        .tracks
        .iter()
        .flat_map(|t| &t.clips)
        .filter(|c| !surviving.contains(&c.id))
        .map(|c| c.id.clone())
        .collect();
    p.grade_scopes.remove_clips(&removed);
    for (before, after) in original.tracks.iter().zip(&p.tracks) {
        if before.locked
            && serde_json::to_value(before).map_err(|e| e.to_string())?
                != serde_json::to_value(after).map_err(|e| e.to_string())?
        {
            return Err(format!("track {} is locked", before.id));
        }
    }
    if !p.dangling_refs().is_empty() {
        return Err("unsupported_edit: cut would leave dangling clip references".into());
    }
    created.retain(|id| surviving.contains(id));
    let removed_words = tr.words.len() - p.editorial.transcripts[tid].words.len();
    Ok((
        p,
        json!({"changed_clip_ids": changed, "created_clip_ids": created,
        "removed_clip_ids": removed, "updated_cue_ids": updated_cues, "removed_words": removed_words}),
    ))
}

fn apply(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let revision = super::batch::revision(ss);
    if str_arg(a, "expect_revision")? != revision {
        return Err(
            "project_drift: preview the current project before applying transcript edits".into(),
        );
    }
    let dry_run = match a.get("dry_run") {
        None => false,
        Some(v) => v.as_bool().ok_or("dry_run must be boolean")?,
    };
    let plan = super::transcript_tools::edit_preview(ss, a)?;
    if plan["plan"]
        .as_array()
        .ok_or("missing plan")?
        .iter()
        .any(|e| e["op"] != "delete")
    {
        return Err(
            "unsupported_edit: only delete is supported; replacement remains preview-only".into(),
        );
    }
    let cuts: Vec<Range> =
        serde_json::from_value(plan["audio_cut_ranges"].clone()).map_err(|e| e.to_string())?;
    if cuts.is_empty()
        || cuts
            .iter()
            .any(|&(a, b)| !a.is_finite() || !b.is_finite() || a < 0.0 || b <= a)
    {
        return Err(
            "unsupported_edit: cuts must have positive duration and non-negative finite times"
                .into(),
        );
    }
    let tid = super::transcript_tools::tid(ss, a)?;
    let (project, mut result) = stage(&ss.project, &tid, &cuts)?;
    crate::guard::check_project(&project, crate::guard::limits())?;
    result["transcript_id"] = json!(tid);
    result["cut_ranges"] = plan["audio_cut_ranges"].clone();
    result["removed_s"] = plan["removed_s"].clone();
    result["timing"] = json!("timeline_seconds");
    result["dry_run"] = json!(dry_run);
    result["previous_revision"] = json!(revision);
    if dry_run {
        result["no_history"] = json!(true);
    } else {
        ss.project = project;
    }
    result["revision"] = json!(super::batch::revision(ss));
    Ok(result)
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "transcript_edit_apply",
        description: "Delete transcript word ranges and ripple every track, picture/audio, bound captions and markers in one undoable transaction. Pass edits from transcript_edit_preview and its revision as expect_revision. dry_run validates without changes. First version: one transcript, explicit normal-speed clip bindings, chosen chronological takes, flat media/text/shapes. Refuses ambiguous references, locked affected tracks, time remapping, multicam and word-animated captions. Exact timeline seconds, no frame snapping. Replacement/reordering not supported.",
        mutates: true,
        schema: obj(vec![
            ("transcript_id", s("transcript; optional when only one exists")),
            ("expect_revision", s("revision returned by transcript_edit_preview")),
            ("dry_run", json!({"type":"boolean","description":"validate/stage without changing project or history"})),
            ("edits", json!({"type":"array","minItems":1,"maxItems":200,"items":{"type":"object","properties":{
                "op":{"type":"string","enum":["delete"]},"from_index":{"type":"integer","minimum":0},"to_index":{"type":"integer","minimum":0}
            },"required":["op","from_index"],"additionalProperties":false}})),
        ], &["edits", "expect_revision"]),
        run: apply,
    }]
}
