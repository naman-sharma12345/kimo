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
//!   - style "boxed" is built from `add_shape` (rect, corner_radius) + `add_text` -- KF16 polish: the
//!     obvious route, `add_title(style="hook_box")`, draws a box fixed at 0.86 * frame width regardless
//!     of word length (titles.rs's own `hook_box` arm; not a tool touched by this lane), which is exactly
//!     the "dark bar spans almost the full frame for one word" complaint. `add_shape` + `crate::text::
//!     measure` (the SAME metrics the renderer itself uses for layout/wrap, see text.rs) already give a
//!     box sized to the text's own bounds plus padding, rounded via `corner_radius` -- no new compositing
//!     or text primitive, just the existing ones aimed at the text's real width instead of a fixed bar.
//!   - styles "bold_caps"/"typewriter" compose `add_text` (the existing text-rendering tool -- stroke
//!     outline + fill + `shadow` (an offset drop shadow add_text already accepts, see tools_0.rs's
//!     schema and text.rs's `render_text`) gives the "white, dark outline, drop shadow" look KR2's
//!     reference frames show) with `set_keyframe` (the existing clip-transform keyframer) for the same
//!     0.0 -> 0.3s OutBack scale pop, or with `add_text_animator`'s own `preset: "typewriter"` for a
//!     character reveal. No font rendering, rasterising or keyframe/easing math is reimplemented here;
//!     every pixel is drawn by `add_text` / `add_shape`, every curve evaluated by the existing
//!     `Ease`/`Prop` machinery.
//!
//! KF16 polish (font weight/size/stroke/shadow, same scope: only this file + its test + the registry
//! line). What `text.rs` already supports, read before changing anything here: `font()` sets a variable
//! font's `wght` axis (default 700/bold when unspecified) via ab_glyph's `VariableFont`, but the bundled
//! `Anton` (fonts/fetch.sh) is a STATIC weight file -- already Google's heaviest "Black" display cut, so
//! a `wght` axis is a no-op on it either way; the renderer has no separate "boldness" knob beyond the
//! font file itself, confirming KR2/the brief's reference look is a font-choice + size/stroke/shadow
//! problem, not a missing weight feature, and Anton was already the right choice in KF15. The real gap
//! was scale: KF15 fixed `size = 150 * (min(h,w)/1080)`, which on a 720-wide vertical frame (k=0.667)
//! renders ~100px text -- nowhere near KR2's "50-70% of frame width" target -- with a hairline stroke
//! (size*0.045) and NO shadow (add_text's own `shadow` param, pixels of drop-shadow offset, was simply
//! never passed). Fixed here: `fit_size_to_width` measures the actual word with `crate::text::measure`
//! (the renderer's own layout metrics, probed once at a fixed size and scaled linearly -- glyph metrics
//! scale linearly with px size, so one measurement is exact, not a search) and solves for a size that
//! lands the word at TARGET_WIDTH_FRAC (0.60, the middle of KR2's 50-70% range) of the frame width,
//! clamped to a sane floor/ceiling in terms of frame height so a one-letter interjection can't blow past
//! a legible cap height. Stroke is now size*0.09 (was 0.045) and a shadow (size*0.05px offset, using
//! add_text's existing default black/60%-opacity/45-degree shadow) is always on for bold_caps/typewriter.
//! `accent_color` was schema'd but only ever wired to style "boxed"'s box fill; it is now ALSO wired,
//! default-off (precedence accent_color > color > white, so omitting it keeps the old look exactly), to
//! bold_caps/typewriter's own text fill -- the "optional accent colour for the word" KR2's reference
//! frames show.
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
use std::process::Stdio;

const STYLES: [&str; 3] = ["bold_caps", "boxed", "typewriter"];
const POSITIONS: [&str; 3] = ["center", "upper_third", "lower_third"];
const ANIMATE_INS: [&str; 3] = ["pop", "fade", "typewriter"];
/// Total on-screen time (seconds) used for an auto-picked pop; matches KR2's measured hold range (0.8-1.5s)
/// plus the 0.3s pop-in and a short fade-out.
const AUTO_DURATION_S: f64 = 1.3;
/// KF16: target fraction of frame width a short word/phrase's rendered bbox should span -- the middle of
/// KR2's measured 50-70% range (one word should span roughly 50-70% of frame width on a 720-wide vertical).
const TARGET_WIDTH_FRAC: f64 = 0.60;
/// KF16: style "boxed" targets a smaller TEXT width than bold_caps, because the box's own padding
/// adds to the total visual footprint -- without this, a short word's box (text + padding) would
/// itself approach the old "box spans most of the frame" bug, just arrived at a different way.
const TARGET_WIDTH_FRAC_BOXED: f64 = 0.38;
/// A fixed probe size for `fit_size_to_width`'s one-shot linear measurement (ab_glyph glyph metrics are
/// exactly proportional to px size, so measuring once at any size and scaling is exact).
const PROBE_SIZE: f64 = 100.0;

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

/// KF16: picks a font size (px) so `text` set in `font` spans roughly `target_w` px wide, by measuring
/// once at `PROBE_SIZE` with `crate::text::measure` (the renderer's own layout metrics -- the same call
/// `add_text`'s anchor logic and `add_title`'s `fit_width` already use) and scaling linearly, then
/// clamping to [min_size, max_size] so a one-character interjection can't blow past a legible cap height
/// and a long phrase can't shrink below a legible floor.
fn fit_size_to_width(text: &str, font: &str, target_w: f64, min_size: f64, max_size: f64) -> Result<f64, String> {
    let (w, _) = crate::text::measure(text, font, PROBE_SIZE, 0.0, 0.0, "center", 0.0)?;
    if w <= 0.0 { return Ok(min_size); }
    Ok((PROBE_SIZE * target_w / w).clamp(min_size, max_size))
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

/// Sets the standard 0.0 -> 0.3s OutBack scale pop-in plus a 0.0 -> 0.15s opacity fade-in and a fade-out
/// starting `duration_s - fade_out_lead` on `id`, and the vertical position offset if non-zero. Shared by
/// every clip of a pop (boxed's box + text, bold_caps/typewriter's single text clip) so box and text pop
/// in perfectly together.
fn pop_in_and_fade(ss: &mut Session, id: &str, duration_s: f64, yoff: f64, fade_out_lead: f64) -> Result<(), String> {
    if yoff != 0.0 { call(ss, "set_keyframe", &json!({"clip_id": id, "property": "y", "time": 0.0, "value": yoff, "ease": "hold"}))?; }
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "scale", "time": 0.0, "value": 0.0, "ease": "out"}))?;
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "scale", "time": 0.3, "value": 1.0, "ease": "out_back"}))?;
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": 0.0, "value": 0.0, "ease": "out"}))?;
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": 0.15, "value": 1.0, "ease": "out"}))?;
    let fade_out_start = (duration_s - fade_out_lead).max(0.05);
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": fade_out_start, "value": 1.0, "ease": "in"}))?;
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": duration_s, "value": 0.0, "ease": "in"}))?;
    Ok(())
}

/// Places one pop, returns the clip id(s) it created (2 for "boxed": box then text; 1 otherwise).
fn place_one(ss: &mut Session, track_id: &str, p: &Pop, style: &str, animate_in: &str, color: Option<[f64; 3]>, accent: Option<[f64; 3]>, yoff: f64) -> Result<Vec<String>, String> {
    let text_upper = p.text.to_uppercase();
    // KF16: size scaled to the frame, not a fixed 150*k -- see fit_size_to_width's doc. Bounds are in
    // terms of frame HEIGHT (not the old min(h,w)/1080 `k`) so a one-letter word can't demand an
    // absurdly tall size just to hit the target fraction, and a long phrase still gets a legible floor.
    let frame_w = ss.project.width as f64;
    let min_size = (ss.project.height as f64 * 0.035).max(8.0);
    let max_size = (ss.project.height as f64 * 0.45).max(min_size + 1.0);
    let target_frac = if style == "boxed" { TARGET_WIDTH_FRAC_BOXED } else { TARGET_WIDTH_FRAC };
    let size = fit_size_to_width(&text_upper, "Anton", frame_w * target_frac, min_size, max_size)?;

    if style == "boxed" {
        // KF16: box sized to the TEXT'S OWN bounds + padding, not a fixed frame-width bar -- see this
        // file's module doc for why `add_title(style="hook_box")` (fixed 0.86*frame-width) isn't reused
        // here. add_shape (rect, corner_radius) + add_text are the same existing primitives, just aimed
        // at the measured word width.
        let (tw, th) = crate::text::measure(&text_upper, "Anton", size, 0.0, 0.0, "center", 0.0)?;
        let pad_x = size * 0.30;
        let pad_y = size * 0.22;
        let box_w = tw + pad_x * 2.0;
        let box_h = th + pad_y * 2.0;
        let box_fill = accent.unwrap_or([0.05, 0.05, 0.07]);
        let box_id = call(ss, "add_shape", &json!({"track_id": track_id, "shape": "rect", "start": p.at_s, "duration": p.duration_s,
            "width": box_w, "height": box_h, "fill": box_fill, "corner_radius": (box_h * 0.22).min(box_w * 0.22)}))?["clip_id"].as_str().ok_or("add_shape returned no clip_id")?.to_string();
        let text_id = call(ss, "add_text", &json!({"track_id": track_id, "text": text_upper, "start": p.at_s, "duration": p.duration_s,
            "font": "Anton", "size": size, "color": color.unwrap_or([1.0, 1.0, 1.0])}))?["clip_id"].as_str().ok_or("add_text returned no clip_id")?.to_string();
        pop_in_and_fade(ss, &box_id, p.duration_s, yoff, 0.4)?;
        pop_in_and_fade(ss, &text_id, p.duration_s, yoff, 0.4)?;
        return Ok(vec![box_id, text_id]);
    }

    // "bold_caps" and "typewriter": a plain text clip -- heavy sans caps, big, thick dark outline, drop
    // shadow. KF16: accent_color, when given, now colours the word itself (default-off: precedence is
    // accent_color > color > white, so the look is byte-identical to KF15 when accent_color is omitted).
    let fill = accent.or(color).unwrap_or([1.0, 1.0, 1.0]);
    let stroke_w = (size * 0.09).max(2.0);
    let shadow_px = (size * 0.05).max(2.0);
    let add_args = json!({"track_id": track_id, "text": text_upper, "start": p.at_s, "duration": p.duration_s,
        "font": "Anton", "size": size, "color": fill, "stroke_width": stroke_w, "stroke_color": [0.03, 0.03, 0.03], "shadow": shadow_px});
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

/// KR2 marked pop sound effects unverified for lack of any bundled/approved sound asset -- nothing was ever
/// shipped in the repo to play (and nothing should be: media is never committed here). This synthesizes one
/// instead, at call time, into the OS temp dir -- a short, purely procedural "pop" blip (a quick sine chirp
/// with a fast attack/decay envelope, no sampled or downloaded sound), the same bytes every time (deterministic,
/// same as everything else in this tool), reused across every pop in one call rather than re-run per pop.
/// Graceful fail-open: if ffmpeg can't be run for some reason, returns None and sfx is skipped with a warning
/// rather than failing the whole placement (the pops themselves must never depend on this succeeding).
fn synth_pop_sfx() -> Option<String> {
    let path = std::env::temp_dir().join("kimo-keyword-pop-sfx-v1.wav");
    if path.exists() { return Some(path.to_str()?.to_string()); }
    let af = "afade=t=in:d=0.004,afade=t=out:st=0.055:d=0.065,volume=0.55";
    let ok = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "sine=frequency=1100:duration=0.12", "-af", af, path.to_str()?])
        .stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).status().map(|s| s.success()).unwrap_or(false);
    if ok { Some(path.to_str()?.to_string()) } else { None }
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
    if sfx {
        match synth_pop_sfx() {
            Some(path) => {
                let asset_id = call(ss, "import_media", &json!({"path": path}))?["asset_id"].as_str().ok_or("import_media returned no asset_id")?.to_string();
                let sfx_track = call(ss, "add_track", &json!({}))?["track_id"].as_str().ok_or("add_track returned no track_id")?.to_string();
                let mut sfx_clip_ids = vec![];
                for p in &pops {
                    let dur = 0.12_f64.min(p.duration_s.max(0.04));
                    let c = call(ss, "add_audio_clip", &json!({"asset_id": asset_id, "track_id": sfx_track, "start": p.at_s, "duration": dur, "volume": 0.8}))?;
                    if let Some(cid) = c["clip_id"].as_str() { sfx_clip_ids.push(cid.to_string()); }
                }
                r["sfx_track_id"] = json!(sfx_track); r["sfx_clip_ids"] = json!(sfx_clip_ids);
            }
            None => { r["warnings"] = json!(["sfx:true was requested but the sound could not be synthesized here (ffmpeg unavailable or failed); pops were placed silently."]); }
        }
    }
    Ok(r)
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "add_keyword_pops",
        description: "Pops one big emphasised word or phrase onto the screen the instant it's spoken, then clears. The #1 on-screen graphic among top creators (KR2: 6 of 8 sampled use it) -- heavy-weight caps (the bundled Anton black display font) sized to span about 60% of frame width, a thick dark outline and drop shadow or a rounded box hugging the text, with a quick scale-overshoot pop-in (clip-local 0.0 -> 0.3s, Ease::OutBack) and a short fade -- built on add_shape+add_text for style \"boxed\" and add_text + set_keyframe/add_text_animator for \"bold_caps\"/\"typewriter\", sized with the renderer's own crate::text::measure rather than reimplementing text metrics. words=[{text, at_s, duration_s?}] places exact pops; auto:true instead ranks the loaded transcript's words by speech_words::keywords() (the existing caption-keyword emphasis scorer; NOT add_smart_punch, which is a purely visual camera punch-in with no word signal) and keeps up to max_per_minute (default 4, per KR2's measured cadence) per 60s bucket at least min_gap_s (default 3) apart -- and if no transcript is loaded it errors by name, telling you to run transcribe or transcript_import first, rather than silently doing nothing. position defaults to upper_third so the text clears a selfie-framed face; accent_color, when given, colours style boxed's box or bold_caps/typewriter's own text fill (default off: omitting it keeps the plain white-on-dark look); color sets the fill otherwise (white default). sfx (default false) plays a short synthesized pop/blip at each placed word, on its own new audio track (sfx_track_id/sfx_clip_ids on the result) -- a purely procedural sound generated at call time, not a bundled sample; falls back to a warning (no sound placed) if it can't be synthesized on this machine. track_id defaults to a freshly created track. dry_run reports the pops that would be placed without changing anything. Deterministic (same transcript/args, same pops) and undoable with undo like any other mutating tool.",
        mutates: true,
        schema: obj(vec![
            ("words", json!({"type":"array","description":"explicit mode: [{text, at_s, duration_s?}], at least one entry","items":{"type":"object","properties":{"text":{"type":"string"},"at_s":{"type":"number"},"duration_s":{"type":"number","description":"default 1.3s"}},"required":["text","at_s"]}})),
            ("auto", json!({"type":"boolean","description":"auto mode: pick emphasis words from the loaded transcript's word timings instead of an explicit words list"})),
            ("max_per_minute", n("auto mode: at most this many pops per 60s of timeline, default 4")),
            ("min_gap_s", n("auto mode: at least this many seconds between consecutive pops, default 3")),
            ("style", json!({"type":"string","enum":STYLES,"description":"bold_caps (default): heavy caps sized to ~60% of frame width, with a thick dark outline and drop shadow. boxed: a rounded dark/accent box hugging the text (sized to the text's own measured bounds + padding, not a fixed bar). typewriter: bold_caps look with a character-by-character reveal."})),
            ("position", json!({"type":"string","enum":POSITIONS,"description":"upper_third (default, clears a selfie-framed face), center, or lower_third"})),
            ("color", json!({"type":"array","description":"text fill [r,g,b] 0..1, default white","items":{"type":"number"}})),
            ("accent_color", json!({"type":"array","description":"optional accent colour [r,g,b] 0..1: style boxed's box fill, or bold_caps/typewriter's own text fill. Default off (omitted = plain white-on-dark look, byte-identical to not passing it).","items":{"type":"number"}})),
            ("animate_in", json!({"type":"string","enum":ANIMATE_INS,"description":"pop (default): scale-overshoot pop-in. fade: opacity only. typewriter: character reveal. style boxed always uses the same pop-and-fade regardless of this."})),
            ("sfx", json!({"type":"boolean","description":"default false; true plays a short synthesized pop at each word on its own new audio track"})),
            ("track_id", s("track to place pops on; default: a freshly created track")),
            ("dry_run", json!({"type":"boolean","description":"report the pops that would be placed, change nothing. Default false."})),
        ], &[]),
        run,
    }]
}
