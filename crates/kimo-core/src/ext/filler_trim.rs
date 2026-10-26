//! trim_filler_words: ripple-delete flagged word spans (e.g. filler words) from ONE media clip's own footage on the
//! timeline. New, narrow tool -- reuses the existing `lift_range` ripple primitive by calling it (same pattern as
//! `remove_silence_cuts` in edit.rs) rather than reimplementing split/ripple. Does not touch auto_rough_cut or any
//! existing split/ripple/trim tool's own behaviour.
//!
//! Checked first: transcript_edit_apply (ext/transcript_edit.rs) is a different, heavier mechanism -- it requires a
//! whole project built around one transcript, with every target clip wrapped in a chosen Take that references it by
//! id, and it cuts by transcript WORD INDEX across every track. It does not take a bare clip_id + time spans, so it
//! is not a duplicate of this tool; this tool is additive and does not change it.
//!
//! Span coordinates ("source time"): seconds since the CLIP's own in_point (0 = in_point), the same zero point
//! auto_captions/transcript words already use for `Word::src_start` / `Word::src_end` (see ext/auto_captions.rs,
//! where recognition runs on audio extracted starting at the clip's in_point) -- not absolute source-file time and
//! not timeline time.
//!
//! Caption retiming (retime_captions, default true): auto_captions lays its caption/text pages on their OWN track,
//! bound back to the media clip only through the transcript -- a `Take` whose `source.clip_id` is this clip, and a
//! `CueBinding` (ss.project.editorial.cue_words) naming which of that take's transcript words each caption clip was
//! built from (see ext/auto_captions.rs's `build`, which inserts exactly that pair). Because captions live on a
//! different track, `lift_range` (which only ripples the ONE track_id it's given) never moves or shrinks them, so
//! before this a cut left captions drifting (a real-clip check: 17.267s -> 16.292s clip, captions still running to
//! 17.2s). We find the words bound to this clip's take(s), then every caption clip whose CueBinding references one
//! of those words, and remap each one's own `start`/`start+duration` through the same removed ranges `lift_range`
//! was given -- the same ripple math, computed directly since lift_range itself never touches the captions track.
use super::kit::*;
use crate::model::{Clip, Project, Source, TimeMode};
use std::collections::HashSet;

const EPS: f64 = 1e-6;
const DEFAULT_PAD: f64 = 0.045;

fn loc(p: &Project, id: &str) -> Result<(usize, usize), String> {
    for (ti, t) in p.tracks.iter().enumerate() {
        if let Some(ci) = t.clips.iter().position(|c| c.id == id) {
            return Ok((ti, ci));
        }
    }
    Err(format!("no such clip '{id}'"))
}

/// Clip-local source time (0 = this clip's in_point) -> timeline time. Mirrors `Clip::source_time`'s inverse for the
/// plain (no remap, no speed ramp) case this tool requires.
fn to_timeline(c: &Clip, src_t: f64) -> f64 {
    c.start + src_t / c.speed
}

fn bool_arg(a: &Value, k: &str, default: bool) -> Result<bool, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(default),
        Some(Value::Bool(b)) => Ok(*b),
        _ => Err(format!("{k} must be a boolean")),
    }
}

/// Spans for words matching `words` (case-insensitive, punctuation-trimmed) from any transcript take bound to this
/// clip_id, in the same source-time coordinate as `spans`.
fn words_from_transcript(ss: &Session, clip_id: &str, words: &[String]) -> Vec<(f64, f64)> {
    let wanted: std::collections::HashSet<String> = words
        .iter()
        .map(|w| w.trim().to_lowercase())
        .filter(|w| !w.is_empty())
        .collect();
    if wanted.is_empty() {
        return vec![];
    }
    let mut out = vec![];
    for tr in ss.project.editorial.transcripts.values() {
        let take_ids: std::collections::HashSet<&str> = tr
            .takes
            .iter()
            .filter(|t| t.source.clip_id.as_deref() == Some(clip_id))
            .map(|t| t.id.as_str())
            .collect();
        if take_ids.is_empty() {
            continue;
        }
        for w in &tr.words {
            if !take_ids.contains(w.take_id.as_str()) {
                continue;
            }
            let norm: String = w
                .recognized
                .trim()
                .trim_matches(|c: char| c.is_ascii_punctuation())
                .to_lowercase();
            if wanted.contains(&norm) {
                out.push((w.src_start, w.src_end));
            }
        }
    }
    out
}

/// Every linked transcript word's own `(src_start, src_end)` span for this clip, in the same
/// "source time since this clip's own in_point" coordinate `words_from_transcript` above uses -- same
/// Take -> word chain (a `Take` whose `source.clip_id` is this clip, then that take's words by `take_id`),
/// just without the text filter: every word is returned, not only ones matching a wanted set. Used by
/// `fit_clip_duration`'s `protect_words` (ext/fit_duration.rs) so its silence cuts never land inside a
/// transcribed word, reusing this lookup rather than re-deriving it. Read-only; does not touch cue_words
/// or anything caption-retiming related.
pub(crate) fn all_word_spans_for_clip(ss: &Session, clip_id: &str) -> Vec<(f64, f64)> {
    let mut out = vec![];
    for tr in ss.project.editorial.transcripts.values() {
        let take_ids: std::collections::HashSet<&str> = tr
            .takes
            .iter()
            .filter(|t| t.source.clip_id.as_deref() == Some(clip_id))
            .map(|t| t.id.as_str())
            .collect();
        if take_ids.is_empty() {
            continue;
        }
        for w in &tr.words {
            if take_ids.contains(w.take_id.as_str()) {
                out.push((w.src_start, w.src_end));
            }
        }
    }
    out
}

/// Maps a timeline time through a sorted, non-overlapping set of removed `[start, end)` ranges: a time inside a
/// removed range collapses onto the range's own start; a time at or after a range's end shifts left by that range's
/// width. This is exactly `lift_range`'s own ripple math, applied directly to a clip that lives on a track
/// `lift_range` was never asked to touch (captions have their own track) rather than by calling it.
fn remap_time(t: f64, removed: &[(f64, f64)]) -> f64 {
    let mut shift = 0.0;
    for &(rst, ren) in removed {
        if t <= rst + EPS {
            break;
        }
        if t >= ren - EPS {
            shift += ren - rst;
        } else {
            shift += t - rst;
            break;
        }
    }
    (t - shift).max(0.0)
}

#[derive(Default, Clone, Copy)]
struct CaptionRetime {
    shifted: usize,
    trimmed: usize,
    removed: usize,
}

/// Retimes caption/text clips that auto_captions built from THIS media clip's own speech. Linkage: any transcript
/// take whose `source.clip_id` is this clip -> that take's words (by id) -> any caption clip whose `CueBinding`
/// (editorial.cue_words) names one of those word ids. A caption built from more than one clip's words (two takes
/// merged onto one shared page) counts as linked here too -- only its position changes, by the ranges removed from
/// THIS clip; it is still checked again, independently, if the other clip is trimmed in its own call. A caption
/// clip never bound to this clip_id (a manually added title, or any clip auto_captions never touched) is never
/// found by this lookup and so is never touched -- the one, documented exception a caption-linked title could ever
/// hit is exactly that: not linked, not touched.
///
/// Per removed range (already the padded, clamped, merged, ascending timeline ranges `lift_range` was called with):
/// a caption entirely inside it is deleted (and unlinked: dropped from cue_words and from its CaptionGroup's own
/// clip list); one entirely after it shifts left by the removed width; one straddling a cut (on either edge, or
/// spanning a cut in its middle) is trimmed to its surviving span. `remap_time` applied to both the clip's start and
/// its end reproduces all three cases (and shift+trim together) without needing to special-case them.
fn retime_linked_captions(ss: &mut Session, media_clip_id: &str, removed: &[(f64, f64)]) -> CaptionRetime {
    let mut stats = CaptionRetime::default();
    if removed.is_empty() {
        return stats;
    }

    let mut linked_word_ids: HashSet<String> = HashSet::new();
    for tr in ss.project.editorial.transcripts.values() {
        let take_ids: HashSet<&str> = tr
            .takes
            .iter()
            .filter(|t| t.source.clip_id.as_deref() == Some(media_clip_id))
            .map(|t| t.id.as_str())
            .collect();
        if take_ids.is_empty() {
            continue;
        }
        for w in &tr.words {
            if take_ids.contains(w.take_id.as_str()) {
                linked_word_ids.insert(w.id.clone());
            }
        }
    }
    if linked_word_ids.is_empty() {
        return stats; // no captions were ever built from this clip's speech
    }

    let linked_clip_ids: Vec<String> = ss
        .project
        .editorial
        .cue_words
        .iter()
        .filter(|(_, b)| b.word_ids.iter().any(|w| linked_word_ids.contains(w)))
        .map(|(cid, _)| cid.clone())
        .collect();
    if linked_clip_ids.is_empty() {
        return stats;
    }

    let mut to_delete: Vec<String> = vec![];
    for cid in &linked_clip_ids {
        let Some(c) = ss.project.find_clip(cid) else { continue };
        let (old_start, old_dur) = (c.start, c.duration);
        let new_start = remap_time(old_start, removed);
        let new_end = remap_time(old_start + old_dur, removed);
        let new_dur = (new_end - new_start).max(0.0);

        if new_dur <= EPS {
            to_delete.push(cid.clone());
            stats.removed += 1;
            continue;
        }
        let start_moved = (new_start - old_start).abs() > EPS;
        let dur_changed = (new_dur - old_dur).abs() > EPS;
        if dur_changed {
            stats.trimmed += 1;
        } else if start_moved {
            stats.shifted += 1;
        }
        if start_moved || dur_changed {
            if let Some(cm) = ss.project.find_clip_mut(cid) {
                cm.start = new_start;
                cm.duration = new_dur;
            }
        }
    }

    for cid in &to_delete {
        for t in ss.project.tracks.iter_mut() {
            t.clips.retain(|c| &c.id != cid);
        }
        ss.project.editorial.cue_words.remove(cid);
        for g in ss.project.editorial.captions.values_mut() {
            g.clips.retain(|c| c != cid);
        }
    }

    stats
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "trim_filler_words",
        description: "Ripple-delete flagged word spans (e.g. filler words like \"um\", \"uh\", repeated ums/uhs, stutters and other disfluencies) from one media clip's own footage on the timeline, reusing lift_range; later clips on the track shift left to close each gap. Caption/text clips auto_captions built from this clip's own words are retimed to match (retime_captions, default true): deleted if entirely inside a removed span, trimmed if straddling a cut, shifted left if after it.",
        mutates: true,
        schema: obj(
            vec![
                ("clip_id", s("the media clip to trim words out of")),
                (
                    "spans",
                    json!({"type":"array","items":{"type":"object","properties":{"start":{"type":"number"},"end":{"type":"number"}},"required":["start","end"]},"description":"word spans to drop, in source time: seconds since this clip's own in_point, the same zero point auto_captions word timestamps use (word.src_start/src_end) -- not absolute file time and not timeline time"}),
                ),
                (
                    "words",
                    json!({"type":"array","items":{"type":"string"},"description":"optional: filler words, stutters or other disfluencies (e.g. [\"um\",\"uh\",\"matlab\"]) to auto-pick spans for from this clip's existing auto_captions/transcript words, case-insensitive; adds to spans when both are given"}),
                ),
                (
                    "pad",
                    n("padding seconds added on each side of every dropped span before merging, clamped so spans never run past this clip's own bounds (default 0.045)"),
                ),
                (
                    "retime_captions",
                    json!({"type":"boolean","description":"also retime caption/text clips auto_captions built from this clip's own words, so they stay in sync with the cut: delete any entirely inside a removed span, trim any straddling a cut, shift any after it left by the removed width. A caption never bound to this clip (a manual title) is never touched. Default true."}),
                ),
            ],
            &["clip_id"],
        ),
        run: |ss, a| {
            let clip_id = str_arg(a, "clip_id")?.to_string();
            let (ti, ci) = loc(&ss.project, &clip_id)?;
            if ss.project.tracks[ti].locked {
                return Err(format!("track {} is locked", ss.project.tracks[ti].id));
            }
            let track_id = ss.project.tracks[ti].id.clone();
            let c = ss.project.tracks[ti].clips[ci].clone();
            if !matches!(c.source, Source::Media { .. }) {
                return Err(format!("trim_filler_words needs a media clip: '{clip_id}' is not one"));
            }
            if !matches!(c.time_mode, TimeMode::Normal) {
                return Err("trim_filler_words needs time_mode normal (no freeze/reverse)".into());
            }
            if c.time_remap.is_some() || c.speed_curve.is_some() {
                return Err("trim_filler_words needs a clip with a plain speed (no time remap or speed ramp)".into());
            }
            if !(c.speed.is_finite() && c.speed > 0.0) {
                return Err("clip speed must be a finite number > 0".into());
            }

            let pad = opt_num(a, "pad", DEFAULT_PAD);
            if !pad.is_finite() || pad < 0.0 {
                return Err("pad must be a finite number >= 0".into());
            }
            let retime_captions = bool_arg(a, "retime_captions", true)?;

            let mut raw_spans: Vec<(f64, f64)> = vec![];
            if let Some(arr) = a.get("spans").and_then(|v| v.as_array()) {
                for (i, item) in arr.iter().enumerate() {
                    let st = item.get("start").and_then(|v| v.as_f64()).ok_or_else(|| format!("spans[{i}].start must be a number"))?;
                    let en = item.get("end").and_then(|v| v.as_f64()).ok_or_else(|| format!("spans[{i}].end must be a number"))?;
                    if !(st.is_finite() && en.is_finite()) {
                        return Err(format!("spans[{i}]: start/end must be finite"));
                    }
                    if en <= st {
                        return Err(format!("spans[{i}]: end must be > start"));
                    }
                    raw_spans.push((st, en));
                }
            }
            let mut words_matched = 0usize;
            if let Some(arr) = a.get("words").and_then(|v| v.as_array()) {
                let words: Vec<String> = arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect();
                let picked = words_from_transcript(ss, &clip_id, &words);
                words_matched = picked.len();
                raw_spans.extend(picked);
            }

            if raw_spans.is_empty() {
                return Ok(json!({"clip_id": clip_id, "clip_ids": [clip_id], "removed_seconds": 0.0, "removed_spans": 0, "words_matched": words_matched, "dropped_spans": [], "retime_captions": retime_captions, "captions_shifted": 0, "captions_trimmed": 0, "captions_removed": 0}));
            }

            // this clip's own source window, 0-based at in_point (matches auto_captions' coordinate convention)
            let src_len = c.duration * c.speed;
            let mut dropped = vec![];
            let mut tl_spans: Vec<(f64, f64)> = vec![];
            for (st, en) in raw_spans {
                let cst = st.max(0.0);
                let cen = en.min(src_len);
                if cen <= cst + EPS {
                    dropped.push(json!({"start": st, "end": en, "reason": "out of this clip's source range"}));
                    continue;
                }
                let mut tst = to_timeline(&c, cst) - pad;
                let mut ten = to_timeline(&c, cen) + pad;
                tst = tst.max(c.start);
                ten = ten.min(c.start + c.duration);
                if ten <= tst + EPS {
                    dropped.push(json!({"start": st, "end": en, "reason": "degenerate after padding/clamping"}));
                    continue;
                }
                tl_spans.push((tst, ten));
            }
            if tl_spans.is_empty() {
                return Ok(json!({"clip_id": clip_id, "clip_ids": [clip_id], "removed_seconds": 0.0, "removed_spans": 0, "words_matched": words_matched, "dropped_spans": dropped, "retime_captions": retime_captions, "captions_shifted": 0, "captions_trimmed": 0, "captions_removed": 0}));
            }
            tl_spans.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            let mut merged: Vec<(f64, f64)> = vec![];
            for (st, en) in tl_spans {
                if let Some(last) = merged.last_mut() {
                    if st <= last.1 + EPS {
                        last.1 = last.1.max(en);
                        continue;
                    }
                }
                merged.push((st, en));
            }
            let total_removed: f64 = merged.iter().map(|(st, en)| en - st).sum();
            if total_removed >= c.duration - EPS {
                return Err("trim_filler_words: the spans (with padding) cover the whole clip; use ripple_delete instead of trimming it to nothing".into());
            }

            let original_start = c.start;
            let expected_final_end = c.start + c.duration - total_removed;

            // Rightmost first: removing a span never moves the timeline position of an earlier (smaller-start, not
            // yet processed) span on this same clip, because `lift_range`'s ripple only shifts clips that start at
            // or after the removed range's END -- which, for a smaller-start span on the same clip, it never is.
            let mut order = merged.clone();
            order.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());
            for (st, en) in &order {
                crate::commands::dispatch(ss, "lift_range", &json!({"track_id": track_id, "start": st, "end": en, "extract": true}))?;
            }

            // Captions live on their own track, which the lift_range calls above never touched -- `merged` is the
            // exact same set of (padded, clamped, non-overlapping, ascending) ranges that were removed from THIS
            // clip's track, so remapping every linked caption through them reproduces the ripple directly.
            let cap_stats = if retime_captions { retime_linked_captions(ss, &clip_id, &merged) } else { CaptionRetime::default() };

            let mut clip_ids: Vec<(f64, String)> = ss
                .project
                .tracks
                .iter()
                .find(|t| t.id == track_id)
                .map(|t| {
                    t.clips
                        .iter()
                        .filter(|c| c.start >= original_start - 1e-4 && c.start < expected_final_end - 1e-4)
                        .map(|c| (c.start, c.id.clone()))
                        .collect()
                })
                .unwrap_or_default();
            clip_ids.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap());
            let ids: Vec<String> = clip_ids.into_iter().map(|(_, id)| id).collect();

            Ok(json!({"clip_id": clip_id, "clip_ids": ids, "removed_seconds": total_removed, "removed_spans": merged.len(), "words_matched": words_matched, "dropped_spans": dropped, "retime_captions": retime_captions, "captions_shifted": cap_stats.shifted, "captions_trimmed": cap_stats.trimmed, "captions_removed": cap_stats.removed}))
        },
    }]
}
