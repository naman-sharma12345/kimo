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
    let level_dynamics = a.get("level_dynamics").and_then(|v| v.as_bool());
    let level_strength = a.get("level_strength").and_then(|v| v.as_f64());
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

    // 1b. Transcribe the dialogue ONCE, here, in timeline seconds -- reused by both b-roll relevance (next)
    // and captions (stage 4), instead of each stage running the recognizer separately. When captions are on,
    // a transcription failure (no speech model installed) is the same honest hard stop auto_captions always
    // gave, just raised earlier so nothing gets built on top of an edit that can't get its captions; when
    // captions are off this degrades quietly and b-roll falls back to its given order (RV-33 honesty rule).
    let mut transcript_words: Vec<Value> = vec![];
    if captions_on || !broll_ids.is_empty() {
        let clip_ids: Vec<&String> = dialogue_clips.iter().map(|(id, _, _)| id).collect();
        let transcribe = || -> Result<Vec<Value>, String> {
            let mut src_warnings = vec![];
            let srcs = super::auto_captions::sources(ss, &json!({"clip_ids": clip_ids}), &mut src_warnings)?;
            let mut mode = String::new(); let mut lang_seen: Option<String> = None; let mut out = vec![];
            for src in &srcs {
                let (ws, _) = super::auto_captions::recognize(src, &json!({}), &mut mode, &mut lang_seen)?;
                for w in ws { out.push(json!({"text": w.text, "start": src.start + w.start / src.speed, "end": src.start + w.end / src.speed})); }
            }
            Ok(out)
        };
        match transcribe() {
            Ok(ws) => transcript_words = ws,
            Err(e) => { if captions_on { return Err(e); } warnings.push(format!("b-roll relevance matching skipped: {e}")); }
        }
    }

    // 2. B-roll over the dialogue, before punch-ins: place_broll decides which stretches get covered, and
    // the punch-in step right after reads those stretches back so it never zooms a clip the viewer can't see.
    // With the transcript above, each cutaway is matched to the clip whose name/tags fit what's said right
    // then (RV-33); without it (no speech recognized, or recognition skipped), it falls back to given order.
    let mut broll_ranges: Vec<(f64, f64)> = vec![];
    if !broll_ids.is_empty() {
        let mut broll_args = json!({"track_id": track_id, "asset_ids": broll_ids});
        if !transcript_words.is_empty() { broll_args["words"] = json!(transcript_words); }
        let br = call(ss, "place_broll", &broll_args)?;
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
            let pin = call(ss, "add_punch_ins", &json!({"clip_ids": visible, "scale": punch_scale, "pattern": "alternate", "ease": 0.2, "variety": true}))?;
            stages.insert("punch_ins".into(), pin);
        } else {
            warnings.push(format!("punch-ins skipped: only {} dialogue clip(s) stay on screen past the b-roll coverage, not enough for wide/tight variation", visible.len()));
        }
    }

    // 4. Captions: word-synced, from the transcript taken in step 1b (same words b-roll relevance used --
    // one recognizer pass for the whole edit, not two).
    if captions_on {
        let mut cap_args = json!({"style": caption_style, "position": "bottom"});
        if !transcript_words.is_empty() { cap_args["words"] = json!(transcript_words); }
        else { cap_args["clip_ids"] = json!(dialogue_clips.iter().map(|(id, _, _)| id).collect::<Vec<_>>()); }
        let cap = call(ss, "auto_captions", &cap_args)?;
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

    // 6. Music bed. finish_audio (next) sets its overall level against the dialogue; duck_timeline (after
    // that) layers a dynamic envelope on top through duck_gain, a SEPARATE multiplier from volume -- the two
    // never fight. Without this the bed sits at one flat level the whole edit: no swell in the gaps, no real
    // give-and-take with the voice, the "lifeless" mix a static level-match alone produces.
    let mut music_track_id: Option<String> = None;
    if let Some(mid) = &music_id {
        let mdur = ss.project.assets.get(mid).map(|x| x.duration).unwrap_or(0.0);
        if mdur <= 0.0 { warnings.push(format!("music_asset_id '{mid}' has no measurable duration; music bed skipped")); }
        else {
            let mt = new_track(ss)?;
            let clip = call(ss, "add_clip", &json!({"track_id": mt, "source_type": "media", "asset_id": mid, "start": 0.0, "duration": edit_duration.min(mdur)}))?;
            stages.insert("music".into(), clip);
            music_track_id = Some(mt);
        }
    }

    // 7. Color: neutralise cast / fix exposure on every clip auto_grade can read (talking head + b-roll).
    if grade_on {
        let grade = call(ss, "auto_grade", &json!({}))?;
        stages.insert("grade".into(), grade);
    }

    // 8. Final mix: clean dialogue, b-roll sound off, music under speech, mix to the loudness target.
    let mut finish_args = json!({"target_lufs": target_lufs});
    if let Some(v) = level_dynamics { finish_args["level_dynamics"] = json!(v); }
    if let Some(v) = level_strength { finish_args["level_strength"] = json!(v); }
    let audio = call(ss, "finish_audio", &finish_args)?;
    stages.insert("audio".into(), audio.clone());

    // 8b. Dynamic ducking on top of finish_audio's static levels: the music bed actually swells in the gaps
    // and comes back down under speech, instead of sitting at one flat level for the whole edit.
    if let Some(mt) = &music_track_id {
        match call(ss, "duck_timeline", &json!({"dialogue_tracks": [track_id.clone()], "music_tracks": [mt]})) {
            Ok(duck) => { stages.insert("duck".into(), duck); }
            Err(e) => warnings.push(format!("music ducking skipped: {e}")),
        }
    }

    // 9. Let the edit catch its own remaining mistakes: lint_edit's own checks, auto-applying whatever has
    // a concrete fix (no guessing at anything that needs a human call). apply_review_fixes re-checks with a
    // fresh lint_edit pass after applying anything, so a fix that solves one problem and creates a new one
    // (the real bug this chain exists to prevent -- a caption fix landing on the hook title) is caught here
    // rather than silently shipped.
    let fixes = call(ss, "apply_review_fixes", &json!({"platform": platform}))?;
    let after = if fixes["lint_edit_after"].is_object() { &fixes["lint_edit_after"] } else { &fixes["lint_edit"] };
    let lint_ok = after["ok"].as_bool().unwrap_or(true);
    let applied_keys: Vec<(Option<&str>, Option<&str>)> = fixes["applied"].as_array().map(|v| v.iter().map(|f| (f["code"].as_str(), f["clip_id"].as_str())).collect()).unwrap_or_default();
    let mut remaining: Vec<Value> = after["problems"].as_array().cloned().unwrap_or_default().into_iter()
        .filter(|p| !applied_keys.contains(&(p["code"].as_str(), p.get("clip_id").and_then(|v| v.as_str())))).collect();
    if let Some(introduced) = fixes["introduced_problems"].as_array() {
        for p in introduced { if !remaining.iter().any(|r| r["code"] == p["code"] && r["clip_id"] == p["clip_id"]) { remaining.push(p.clone()); } }
        if !introduced.is_empty() { warnings.push(format!("a fix introduced {} new problem(s) of its own (see remaining_problems) -- not auto-chased, worth a look", introduced.len())); }
    }
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
                ("level_dynamics", json!({"type": "boolean", "description": "passed to finish_audio: gentle speech leveling, default true"})),
                ("level_strength", n("passed to finish_audio: leveling factor 1..4, default 2.5")),
            ], &["talking_head_asset_ids"]), run: |ss, a| atomic(ss, a, auto_edit) },
    ]
}
