//! `auto_edit`: one call, raw clips -> a finished, polished edit, chaining Kimo's own existing tools with
//! sensible defaults -- rough cut (dead air out) -> b-roll placement -> jump-cut punch-ins (only on the
//! segments that stay on screen, so the zoom is never hidden under b-roll) -> word-synced captions -> an
//! optional cold-open hook title -> an optional music bed -> auto color -> a final audio mix to a YouTube
//! loudness target -> lint_edit's own review, auto-applying whatever it can fix by itself. Built because an
//! agent handed a folder of raw clips had no single call for "edit this well"; it had to compose eight-plus
//! tool calls in the right order to get a result that didn't look assembled by hand. Every stage below is an
//! ordinary existing tool, called through dispatch_nested (same validation, same locks, one undo step for
//! the whole edit); nothing here duplicates another tool's logic.
use super::kit::*;
use crate::commands::dispatch_nested as call;

fn atomic(ss: &mut Session, a: &Value, f: fn(&mut Session, &Value) -> Result<Value, String>) -> Result<Value, String> {
    let before = ss.project.clone();
    match f(ss, a) { Ok(v) => Ok(v), Err(e) => { ss.project = before; Err(e) } }
}
fn str_list(a: &Value, k: &str) -> Vec<String> {
    a.get(k).and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}
fn new_track(ss: &mut Session) -> Result<String, String> {
    call(ss, "add_track", &json!({}))?["track_id"].as_str().map(String::from).ok_or_else(|| "add_track returned no id".into())
}
/// Fraction of [s, e) covered by any range in `cover` (ranges assumed non-overlapping with each other).
fn covered_fraction(s: f64, e: f64, cover: &[(f64, f64)]) -> f64 {
    let span = (e - s).max(1e-6);
    let mut covered = 0.0;
    for (cs, ce) in cover { let a = s.max(*cs); let b = e.min(*ce); if b > a { covered += b - a; } }
    (covered / span).clamp(0.0, 1.0)
}

fn auto_edit(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let th_ids = str_list(a, "talking_head_asset_ids");
    if th_ids.is_empty() { return Err("talking_head_asset_ids: give one or more imported talking-head asset ids, in story order (import_media first)".into()); }
    let broll_ids = str_list(a, "broll_asset_ids");
    let music_id = a.get("music_asset_id").and_then(|v| v.as_str()).map(String::from);
    let hook_text = a.get("hook_text").and_then(|v| v.as_str()).filter(|s| !s.is_empty()).map(String::from);
    let hook_style = a.get("hook_style").and_then(|v| v.as_str()).unwrap_or("hook_box").to_string();
    let caption_style = a.get("caption_style").and_then(|v| v.as_str()).unwrap_or("pop").to_string();
    let captions_on = a.get("captions").and_then(|v| v.as_bool()).unwrap_or(true);
    let target_lufs = opt_num(a, "target_lufs", -14.0);
    let punch_scale = opt_num(a, "punch_scale", 1.15).clamp(1.01, 3.0);
    let punch_on = a.get("punch_ins").and_then(|v| v.as_bool()).unwrap_or(true);
    let grade_on = a.get("grade").and_then(|v| v.as_bool()).unwrap_or(true);
    let platform = a.get("platform").and_then(|v| v.as_str()).unwrap_or("none").to_string();

    let mut stages = serde_json::Map::new();
    let mut warnings: Vec<String> = vec![];

    // 1. Rough cut: lay the talking-head assets back to back with the dead air cut out. This is the spine
    // everything else attaches to, so it runs first and un-skippably.
    let rc = call(ss, "auto_rough_cut", &json!({"asset_ids": th_ids}))?;
    let track_id = rc["track_id"].as_str().ok_or("auto_rough_cut returned no track_id")?.to_string();
    let dialogue_clips: Vec<(String, f64, f64)> = rc["clips"].as_array().map(|v| v.iter().filter_map(|c| Some((c["clip_id"].as_str()?.to_string(), c["start"].as_f64()?, c["end"].as_f64()?))).collect()).unwrap_or_default();
    if dialogue_clips.is_empty() { return Err("auto_rough_cut kept no speech from talking_head_asset_ids -- check the clips have audible speech".into()); }
    let edit_duration = rc["duration"].as_f64().unwrap_or(0.0);
    stages.insert("rough_cut".into(), json!({"track_id": track_id, "clip_ids": dialogue_clips.iter().map(|(id,_,_)| id).collect::<Vec<_>>(), "duration": edit_duration}));

    // 2. B-roll over the dialogue, before punch-ins: place_broll decides which stretches get covered, and
    // the punch-in step right after reads those stretches back so it never zooms a clip the viewer can't see.
    let mut broll_ranges: Vec<(f64, f64)> = vec![];
    if !broll_ids.is_empty() {
        let br = call(ss, "place_broll", &json!({"track_id": track_id, "asset_ids": broll_ids}))?;
        if let Some(arr) = br["clips"].as_array() { for c in arr { if let (Some(s), Some(e)) = (c["start"].as_f64(), c["end"].as_f64()) { broll_ranges.push((s, e)); } } }
        stages.insert("broll".into(), br);
    } else {
        warnings.push("no broll_asset_ids given: the talking head runs with no cutaways".into());
    }

    // 3. Punch-ins (jump-cut zoom variation) only on the dialogue clips that stay mostly on screen -- a
    // clip b-roll already covers for most of its length would animate a zoom nobody ever sees.
    if punch_on {
        let visible: Vec<String> = dialogue_clips.iter().filter(|(_, s, e)| covered_fraction(*s, *e, &broll_ranges) < 0.5).map(|(id, _, _)| id.clone()).collect();
        if visible.len() >= 2 {
            let pin = call(ss, "add_punch_ins", &json!({"clip_ids": visible, "scale": punch_scale, "pattern": "alternate", "ease": 0.2}))?;
            stages.insert("punch_ins".into(), pin);
        } else {
            warnings.push(format!("punch-ins skipped: only {} dialogue clip(s) stay on screen past the b-roll coverage, not enough for wide/tight variation", visible.len()));
        }
    }

    // 4. Captions: word-synced, from Kimo's own local speech recognition (no transcript needed).
    if captions_on {
        let clip_ids: Vec<&String> = dialogue_clips.iter().map(|(id, _, _)| id).collect();
        let cap = call(ss, "auto_captions", &json!({"clip_ids": clip_ids, "style": caption_style, "position": "bottom"}))?;
        if let Some(ws) = cap["warnings"].as_array() { for w in ws { if let Some(s) = w.as_str() { warnings.push(s.to_string()); } } }
        stages.insert("captions".into(), cap);
    }

    // 5. Cold-open hook title over the first few seconds -- the thing a top editor never skips and an agent
    // handed raw clips usually forgets. Only when the caller gives the line: guessing invented copy nobody
    // wrote would be putting words in the creator's mouth, so with no hook_text this stage is honestly skipped.
    if let Some(text) = hook_text {
        let tt = new_track(ss)?;
        let dur = edit_duration.min(3.0).max(1.0);
        let title = call(ss, "add_title", &json!({"track_id": tt, "style": hook_style, "text": text, "start": 0.0, "duration": dur}))?;
        if let Some(ws) = title["warnings"].as_array() { for w in ws { if let Some(s) = w.as_str() { warnings.push(s.to_string()); } } }
        stages.insert("hook_title".into(), title);
    } else {
        warnings.push("no hook_text given: the edit opens on the raw first clip with no hook title (pass hook_text for a cold-open line)".into());
    }

    // 6. Music bed, ducked under speech in the audio-finish stage below (finish_audio classifies any
    // audio-only asset as music on its own).
    if let Some(mid) = &music_id {
        let mdur = ss.project.assets.get(mid).map(|x| x.duration).unwrap_or(0.0);
        if mdur <= 0.0 { warnings.push(format!("music_asset_id '{mid}' has no measurable duration; music bed skipped")); }
        else {
            let mt = new_track(ss)?;
            let clip = call(ss, "add_clip", &json!({"track_id": mt, "source_type": "media", "asset_id": mid, "start": 0.0, "duration": edit_duration.min(mdur)}))?;
            stages.insert("music".into(), clip);
        }
    }

    // 7. Color: neutralise cast / fix exposure on every clip auto_grade can read (talking head + b-roll).
    if grade_on {
        let grade = call(ss, "auto_grade", &json!({}))?;
        stages.insert("grade".into(), grade);
    }

    // 8. Final mix: clean dialogue, b-roll sound off, music under speech, mix to the loudness target.
    let audio = call(ss, "finish_audio", &json!({"target_lufs": target_lufs}))?;
    stages.insert("audio".into(), audio.clone());

    // 9. Let the edit catch its own remaining mistakes: lint_edit's own checks, auto-applying whatever has
    // a concrete fix (no guessing at anything that needs a human call).
    let fixes = call(ss, "apply_review_fixes", &json!({"platform": platform}))?;
    let lint_ok = fixes["lint_edit"]["ok"].as_bool().unwrap_or(true);
    let remaining: Vec<Value> = fixes["lint_edit"]["problems"].as_array().cloned().unwrap_or_default().into_iter()
        .filter(|p| !fixes["applied"].as_array().map(|v| v.iter().any(|f| f["code"] == p["code"] && f["clip_id"] == p["clip_id"])).unwrap_or(false)).collect();
    stages.insert("review".into(), fixes);

    Ok(json!({
        "ok": true, "track_id": track_id, "duration": edit_duration, "mix_lufs": audio["mix_lufs"],
        "stages": stages, "warnings": warnings, "lint_clean": lint_ok, "remaining_problems": remaining,
        "next": "export when ready; self_review or review_export to look at it first",
    }))
}

pub fn commands() -> Vec<Command> {
    let ids = |d: &str| json!({"type": "array", "items": {"type": "string"}, "description": d});
    vec![
        Command { name: "auto_edit", description: "One call, raw clips to a finished edit. Chains rough cut (dead air out), b-roll, jump-cut punch-ins (only where the face stays on screen), word-synced captions, an optional hook title and music bed, auto color, a final mix to target loudness, and lint_edit's own auto-fixes -- the ordering (b-roll before punch-ins) keeps a zoom from landing on a clip nothing shows, which is the mistake this exists to prevent.", mutates: true,
            schema: obj(vec![
                ("talking_head_asset_ids", ids("required: talking-head assets, in story order (import_media first)")),
                ("broll_asset_ids", ids("optional: b-roll cutaways, in order")),
                ("music_asset_id", s("optional: an imported audio-only asset for a music bed, ducked under speech")),
                ("hook_text", s("optional: a cold-open hook line for the first few seconds; omit to skip the hook honestly rather than invent one")),
                ("hook_style", s("add_title style for the hook, default hook_box")),
                ("caption_style", s("auto_captions style: pop (default)|karaoke|clean|boxed|impact")),
                ("captions", json!({"type": "boolean", "description": "default true"})),
                ("punch_ins", json!({"type": "boolean", "description": "default true"})),
                ("punch_scale", n("punched-in zoom over wide, default 1.15")),
                ("grade", json!({"type": "boolean", "description": "default true: auto_grade every clip"})),
                ("target_lufs", n("final mix loudness target, default -14 (YouTube)")),
                ("platform", s("passed to lint_edit/apply_review_fixes: tiktok|reels|shorts|none (default none)")),
            ], &["talking_head_asset_ids"]), run: |ss, a| atomic(ss, a, auto_edit) },
    ]
}
