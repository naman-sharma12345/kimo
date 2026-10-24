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
use super::kit::*;
use crate::model::{Clip, Project, Source, TimeMode};

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

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "trim_filler_words",
        description: "Ripple-delete flagged word spans (e.g. filler words) from one media clip's own footage on the timeline, reusing lift_range; later clips on the track shift left to close each gap.",
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
                    json!({"type":"array","items":{"type":"string"},"description":"optional: filler words (e.g. [\"um\",\"uh\",\"matlab\"]) to auto-pick spans for from this clip's existing auto_captions/transcript words, case-insensitive; adds to spans when both are given"}),
                ),
                (
                    "pad",
                    n("padding seconds added on each side of every dropped span before merging, clamped so spans never run past this clip's own bounds (default 0.045)"),
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
                return Ok(json!({"clip_id": clip_id, "clip_ids": [clip_id], "removed_seconds": 0.0, "removed_spans": 0, "words_matched": words_matched, "dropped_spans": []}));
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
                return Ok(json!({"clip_id": clip_id, "clip_ids": [clip_id], "removed_seconds": 0.0, "removed_spans": 0, "words_matched": words_matched, "dropped_spans": dropped}));
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

            Ok(json!({"clip_id": clip_id, "clip_ids": ids, "removed_seconds": total_removed, "removed_spans": merged.len(), "words_matched": words_matched, "dropped_spans": dropped}))
        },
    }]
}
