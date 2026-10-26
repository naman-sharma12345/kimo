//! add_keyword_pops (KF15): the keyword-pop graphic -- one big emphasised word or short phrase that
//! pops onto the screen the instant it is spoken, holds briefly, then clears. Research lane KR2 (see
//! /home/user/tab/files/KR2_youtube_graphics_research.pdf) found this is the single most common graphic
//! among top YouTube creators (6 of 8 sampled use it), and ranked it the #1 missing Kimo tool. This is a
//! NEW, ADDITIVE tool: it touches nothing caption-related (no captions file, no caption ripple/drift
//! code, no caption tool default) -- Naman's "no captions" rule still holds for everything else; a
//! keyword pop is a graphic, not a subtitle track, and this lane never reads or writes a transcript's
//! caption cues.
//!
//! Built entirely on EXISTING primitives through `dispatch_nested`, per KR2's own finding that
//! `add_title` (style `hook_box`) and `add_text_animator` already render close matches:
//!   - style "boxed" is a thin wrapper around `add_title(style="hook_box", ...)` -- that preset already
//!     builds exactly the pop-in box + pop-in text KR2's reference frames show (scale 0 -> 1 with
//!     Ease::OutBack at clip-local 0.3s, a short fade in/out): see `titles.rs`'s own `hook_box` arm.
//!     `box_color` is driven by `accent_color` so the caller's accent shows through.
//!   - styles "bold_caps"/"typewriter" compose `add_text` (the existing text-rendering tool -- stroke
//!     outline + fill gives the "white, dark outline" look KR2's reference frames show) with
//!     `set_keyframe` (the existing clip-transform keyframer) for the same 0.0 -> 0.3s OutBack scale pop,
//!     or with `add_text_animator`'s own `preset: "typewriter"` for a character reveal. No font rendering,
//!     rasterising or keyframe/easing math is reimplemented here; every pixel is drawn by `add_text` /
//!     `add_title`, every curve evaluated by the existing `Ease`/`Prop` machinery.
//!
//! Word timings (auto mode). Kimo gets word timings from the SAME transcript pipeline captions use:
//! `transcribe` (whisper CLI) or `transcript_import` populate a `Transcript` (word-timed, refined against
//! audio -- see `speech_words.rs`'s module doc for the DTW-bias/onset-snap refinement), read back here
//! through the existing `transcript_get(include_words: true)` tool -- never reading
//! `ss.project.editorial.transcripts` directly. IMPORTANT CORRECTION to this lane's own brief: the brief
//! guessed `add_smart_punch` might share an "emphasis" signal; it does not -- `add_smart_punch`
//! (smartpunch_tools.rs) is a purely VISUAL subject-follow camera punch-in (spectral-residual image
//! saliency, no transcript, no words, see its own module doc). The real emphasis signal already in Kimo
//! is `speech_words::keywords()`, a pure scoring function used today by `auto_captions.rs` to bold/pick
//! per-page caption keywords (not a word-timing source, a WORD-IMPORTANCE scorer: not a stopword, >= 4
//! letters or a digit, longer/rarer/capitalised words score higher). `auto: true` calls that exact
//! function on the loaded transcript's words, then buckets its ranked output by max_per_minute and
//! min_gap_s -- see `auto_pick` below.
use super::kit::*;
use crate::commands::dispatch_nested as call;
use crate::speech_words;
use std::collections::HashMap;

const STYLES: [&str; 3] = ["bold_caps", "boxed", "typewriter"];
const POSITIONS: [&str; 3] = ["center", "upper_third", "lower_third"];
const ANIMATE_INS: [&str; 3] = ["pop", "fade", "typewriter"];
/// Total on-screen time (seconds) used for an auto-picked pop; matches KR2's measured hold range (0.8-1.5s)
/// plus the 0.3s pop-in and a short fade-out.
const AUTO_DURATION_S: f64 = 1.3;

fn enum_arg(a: &Value, k: &str, d: &str, ok: &[&str; 3]) -> Result<String, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d.to_string()),
        Some(Value::String(s)) if ok.contains(&s.as_str()) => Ok(s.clone()),
        Some(v) => Err(format!("{k} must be one of {}, got {v}", ok.join(", "))) }
}
fn bool_arg_opt(a: &Value, k: &str) -> Result<Option<bool>, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_bool().map(Some).ok_or_else(|| crate::errors::arg_err(a, k, "boolean")) }
}
fn opt_num_checked(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| crate::errors::arg_err(a, k, "number")) }
}
fn rgb_arg(a: &Value, k: &str) -> Result<Option<[f64; 3]>, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(None), Some(Value::Array(v)) if v.len() == 3 => {
        let mut out = [0.0; 3]; for i in 0..3 { out[i] = v[i].as_f64().ok_or_else(|| crate::errors::arg_err(a, k, "array of 3 numbers"))?; } Ok(Some(out)) }
        _ => Err(crate::errors::arg_err(a, k, "[r,g,b]")) }
}

#[derive(Clone)]
struct Pop { text: String, at_s: f64, duration_s: f64 }

fn parse_explicit_words(a: &Value) -> Result<Vec<Pop>, String> {
    let arr = a.get("words").and_then(|v| v.as_array()).ok_or_else(|| crate::errors::arg_err(a, "words", "array"))?;
    if arr.is_empty() { return Err("words must have at least one entry".into()); }
    let mut out = vec![];
    for (i, w) in arr.iter().enumerate() {
        let text = w.get("text").and_then(|v| v.as_str()).ok_or_else(|| format!("words[{i}].text must be a string"))?.to_string();
        if text.trim().is_empty() { return Err(format!("words[{i}].text must not be empty")); }
        let at_s = w.get("at_s").and_then(|v| v.as_f64()).filter(|x| x.is_finite() && *x >= 0.0).ok_or_else(|| format!("words[{i}].at_s must be a finite number >= 0"))?;
        let duration_s = match w.get("duration_s") { None | Some(Value::Null) => AUTO_DURATION_S, Some(v) => v.as_f64().filter(|x| x.is_finite() && *x > 0.0).ok_or_else(|| format!("words[{i}].duration_s must be a finite number > 0"))? };
        out.push(Pop { text, at_s, duration_s });
    }
    Ok(out)
}

/// Ranks every transcript word by `speech_words::keywords`' emphasis score (best first), keeps at most
/// `max_per_minute` per 60s timeline bucket, then walks the survivors in TIME order enforcing `min_gap_s`
/// between consecutive kept pops (a candidate too close to the last KEPT one is dropped, not swapped --
/// deterministic, same input same output). Every pop gets `AUTO_DURATION_S`.
fn auto_pick(ss: &mut Session, max_per_minute: f64, min_gap_s: f64) -> Result<Vec<Pop>, String> {
    let got = call(ss, "transcript_get", &json!({"include_words": true})).map_err(|e| {
        format!("auto:true needs word timings and none are loaded ({e}). Run `transcribe` (speech-to-text) or `transcript_import` first, then call add_keyword_pops(auto:true) again.")
    })?;
    let words = got.get("words").and_then(|v| v.as_array()).cloned().unwrap_or_default();
    if words.is_empty() {
        return Err("auto:true needs word timings and the loaded transcript has none (0 words). Run `transcribe` (speech-to-text) or `transcript_import` first, then call add_keyword_pops(auto:true) again.".into());
    }
    let texts: Vec<String> = words.iter().map(|w| w.get("display").and_then(|v| v.as_str()).unwrap_or("").to_string()).collect();
    let starts: Vec<f64> = words.iter().map(|w| w.get("start").and_then(|v| v.as_f64()).unwrap_or(0.0)).collect();
    if texts.len() < 2 { return Ok(vec![]); } // nothing to rank an emphasis word against

    let mut freq: HashMap<String, usize> = HashMap::new();
    for t in &texts { *freq.entry(speech_words::norm_word(t)).or_insert(0) += 1; }
    let refs: Vec<&str> = texts.iter().map(|s| s.as_str()).collect();
    let ranked = speech_words::keywords(&refs, refs.len(), &freq); // every qualifying word, best-scored first

    let mut bucket_count: HashMap<i64, usize> = HashMap::new();
    let mut kept: Vec<usize> = vec![];
    for i in ranked {
        let bucket = (starts[i] / 60.0).floor() as i64;
        let c = bucket_count.entry(bucket).or_insert(0);
        if (*c as f64) < max_per_minute { kept.push(i); *c += 1; }
    }
    kept.sort_by(|a, b| starts[*a].partial_cmp(&starts[*b]).unwrap_or(std::cmp::Ordering::Equal));

    let mut out: Vec<Pop> = vec![];
    let mut last_t: Option<f64> = None;
    for i in kept {
        let t = starts[i];
        if let Some(lt) = last_t { if t - lt < min_gap_s { continue; } }
        out.push(Pop { text: texts[i].clone(), at_s: t, duration_s: AUTO_DURATION_S });
        last_t = Some(t);
    }
    Ok(out)
}

fn y_offset(ss: &Session, position: &str) -> f64 {
    let h = ss.project.height as f64;
    match position { "upper_third" => -h * 0.30, "lower_third" => h * 0.30, _ => 0.0 }
}

/// Places one pop, returns the clip id(s) it created (2 for "boxed": box then text; 1 otherwise).
fn place_one(ss: &mut Session, track_id: &str, p: &Pop, style: &str, animate_in: &str, color: Option<[f64; 3]>, accent: Option<[f64; 3]>, yoff: f64) -> Result<Vec<String>, String> {
    if style == "boxed" {
        let mut args = json!({"track_id": track_id, "style": "hook_box", "text": p.text, "start": p.at_s, "duration": p.duration_s});
        if let Some(c) = color { args["color"] = json!(c); }
        if let Some(ac) = accent { args["box_color"] = json!(ac); }
        let r = call(ss, "add_title", &args)?;
        let ids: Vec<String> = r["clip_ids"].as_array().cloned().unwrap_or_default().iter().filter_map(|v| v.as_str().map(String::from)).collect();
        if yoff != 0.0 { for id in &ids { call(ss, "set_keyframe", &json!({"clip_id": id, "property": "y", "time": 0.0, "value": yoff, "ease": "hold"}))?; } }
        return Ok(ids);
    }
    // "bold_caps" and "typewriter": a plain text clip -- bold sans caps, white fill, dark outline.
    let k = (ss.project.height.min(ss.project.width) as f64 / 1080.0).max(0.05);
    let size = 150.0 * k;
    let col = color.unwrap_or([1.0, 1.0, 1.0]);
    let add_args = json!({"track_id": track_id, "text": p.text.to_uppercase(), "start": p.at_s, "duration": p.duration_s,
        "font": "Anton", "size": size, "color": col, "stroke_width": (size * 0.045).max(1.0), "stroke_color": [0.03, 0.03, 0.03]});
    let id = call(ss, "add_text", &add_args)?["clip_id"].as_str().ok_or("add_text returned no clip_id")?.to_string();
    if yoff != 0.0 { call(ss, "set_keyframe", &json!({"clip_id": id, "property": "y", "time": 0.0, "value": yoff, "ease": "hold"}))?; }

    let eff_in = if style == "typewriter" { "typewriter" } else { animate_in };
    match eff_in {
        "pop" => {
            call(ss, "set_keyframe", &json!({"clip_id": id, "property": "scale", "time": 0.0, "value": 0.0, "ease": "out"}))?;
            call(ss, "set_keyframe", &json!({"clip_id": id, "property": "scale", "time": 0.3, "value": 1.0, "ease": "out_back"}))?;
            call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": 0.0, "value": 0.0, "ease": "out"}))?;
            call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": 0.12, "value": 1.0, "ease": "out"}))?;
        }
        "fade" => {
            call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": 0.0, "value": 0.0, "ease": "out"}))?;
            call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": 0.25, "value": 1.0, "ease": "out"}))?;
        }
        "typewriter" => {
            let dur = (p.duration_s * 0.5).clamp(0.15, 0.8);
            call(ss, "add_text_animator", &json!({"clip_id": id, "preset": "typewriter", "duration": dur}))?;
        }
        other => return Err(format!("unknown animate_in '{other}'")),
    }
    let fade_out_start = (p.duration_s - 0.35).max(0.05);
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": fade_out_start, "value": 1.0, "ease": "in"}))?;
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": p.duration_s, "value": 0.0, "ease": "in"}))?;
    Ok(vec![id])
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let style = enum_arg(a, "style", "bold_caps", &STYLES)?;
    let position = enum_arg(a, "position", "upper_third", &POSITIONS)?;
    let animate_in = enum_arg(a, "animate_in", "pop", &ANIMATE_INS)?;
    let color = rgb_arg(a, "color")?;
    let accent_color = rgb_arg(a, "accent_color")?;
    let sfx = bool_arg_opt(a, "sfx")?.unwrap_or(false);
    let dry_run = bool_arg_opt(a, "dry_run")?.unwrap_or(false);
    let auto = bool_arg_opt(a, "auto")?.unwrap_or(false);
    let has_words = a.get("words").map(|v| !v.is_null()).unwrap_or(false);
    if auto && has_words { return Err("add_keyword_pops: pass either words or auto:true, not both".into()); }
    if !auto && !has_words { return Err("add_keyword_pops: needs either words=[{text, at_s, ...}] or auto:true".into()); }
    let max_per_minute = opt_num_checked(a, "max_per_minute", 4.0)?;
    if !(max_per_minute > 0.0) { return Err("max_per_minute must be > 0".into()); }
    let min_gap_s = opt_num_checked(a, "min_gap_s", 3.0)?;
    if !(min_gap_s >= 0.0) { return Err("min_gap_s must be >= 0".into()); }

    let pops = if auto { auto_pick(ss, max_per_minute, min_gap_s)? } else { parse_explicit_words(a)? };

    if dry_run {
        return Ok(json!({"dry_run": true, "pops": pops.iter().map(|p| json!({"text": p.text, "at_s": p.at_s, "duration_s": p.duration_s})).collect::<Vec<_>>(), "no_history": true}));
    }
    if pops.is_empty() {
        return Ok(json!({"dry_run": false, "track_id": Value::Null, "pops": [], "no_history": true, "note": "no pops to place"}));
    }

    let track_id = match a.get("track_id") {
        None | Some(Value::Null) => call(ss, "add_track", &json!({}))?["track_id"].as_str().ok_or("add_track returned no track_id")?.to_string(),
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => return Err(crate::errors::arg_err(a, "track_id", "string")),
    };
    if !ss.project.tracks.iter().any(|t| t.id == track_id) { return Err(format!("no such track '{track_id}'")); }
    let yoff = y_offset(ss, &position);

    let mut placed = vec![];
    for p in &pops {
        let ids = place_one(ss, &track_id, p, &style, &animate_in, color, accent_color, yoff)?;
        placed.push(json!({"text": p.text, "at_s": p.at_s, "duration_s": p.duration_s, "clip_ids": ids}));
    }
    let mut r = json!({"dry_run": false, "track_id": track_id, "style": style, "position": position, "pops": placed});
    if sfx { r["warnings"] = json!(["sfx:true was requested, but no pop sound effect is bundled/verified yet (KR2 marked SFX unverified); pops were placed silently."]); }
    Ok(r)
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "add_keyword_pops",
        description: "Pops one big emphasised word or phrase onto the screen the instant it's spoken, then clears. The #1 on-screen graphic among top creators (KR2: 6 of 8 sampled use it) -- a bold-caps word or boxed callout with a quick scale-overshoot pop-in (clip-local 0.0 -> 0.3s, Ease::OutBack) and a short fade, built on add_title(style=hook_box) for style \"boxed\" and add_text + set_keyframe/add_text_animator for \"bold_caps\"/\"typewriter\" -- the same close-match tools KR2 found, reused rather than reimplemented. words=[{text, at_s, duration_s?}] places exact pops; auto:true instead ranks the loaded transcript's words by speech_words::keywords() (the existing caption-keyword emphasis scorer; NOT add_smart_punch, which is a purely visual camera punch-in with no word signal) and keeps up to max_per_minute (default 4, per KR2's measured cadence) per 60s bucket at least min_gap_s (default 3) apart -- and if no transcript is loaded it errors by name, telling you to run transcribe or transcript_import first, rather than silently doing nothing. position defaults to upper_third so the text clears a selfie-framed face; style boxed's accent_color sets the box, bold_caps/typewriter use color for the fill (white default) with a dark outline. sfx (default false, KR2 marked pop sound effects unverified) is accepted but currently a no-op with a warning: no verified sound asset is bundled. track_id defaults to a freshly created track. dry_run reports the pops that would be placed without changing anything. Deterministic (same transcript/args, same pops) and undoable with undo like any other mutating tool.",
        mutates: true,
        schema: obj(vec![
            ("words", json!({"type":"array","description":"explicit mode: [{text, at_s, duration_s?}], at least one entry","items":{"type":"object","properties":{"text":{"type":"string"},"at_s":{"type":"number"},"duration_s":{"type":"number","description":"default 1.3s"}},"required":["text","at_s"]}})),
            ("auto", json!({"type":"boolean","description":"auto mode: pick emphasis words from the loaded transcript's word timings instead of an explicit words list"})),
            ("max_per_minute", n("auto mode: at most this many pops per 60s of timeline, default 4")),
            ("min_gap_s", n("auto mode: at least this many seconds between consecutive pops, default 3")),
            ("style", json!({"type":"string","enum":STYLES,"description":"bold_caps (default): plain bold white caps with a dark outline. boxed: a dark/accent rounded background box behind the text (add_title hook_box). typewriter: bold_caps look with a character-by-character reveal."})),
            ("position", json!({"type":"string","enum":POSITIONS,"description":"upper_third (default, clears a selfie-framed face), center, or lower_third"})),
            ("color", json!({"type":"array","description":"text fill [r,g,b] 0..1, default white","items":{"type":"number"}})),
            ("accent_color", json!({"type":"array","description":"style boxed only: the box fill [r,g,b] 0..1","items":{"type":"number"}})),
            ("animate_in", json!({"type":"string","enum":ANIMATE_INS,"description":"pop (default): scale-overshoot pop-in. fade: opacity only. typewriter: character reveal. style boxed always uses add_title's own pop-and-fade regardless of this."})),
            ("sfx", json!({"type":"boolean","description":"default false (KR2 marked pop sound effects unverified); currently a no-op with a warning when true, no sound is actually added"})),
            ("track_id", s("track to place pops on; default: a freshly created track")),
            ("dry_run", json!({"type":"boolean","description":"report the pops that would be placed, change nothing. Default false."})),
        ], &[]),
        run,
    }]
}
