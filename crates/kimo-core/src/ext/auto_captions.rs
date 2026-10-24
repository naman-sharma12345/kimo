//! Automatic animated captions: auto_captions (speech to word-timed, styled, animated caption pages in one call),
//! edit_caption_words (fix a misheard word without re-timing) and export_captions (SRT / WebVTT sidecar).
//!
//! Speech recognition runs locally through whisper.cpp's `whisper-cli` (MIT) as a sidecar process with a ggml Whisper model
//! (MIT weights), both fetched by tools/whisper/fetch.sh into ~/.cache/kimo_models (or $KIMO_MODELS); nothing is linked into
//! kimo-core. Word times are refined against the audio (speech_words::refine). Captions are ordinary text clips on their own
//! track, one per page, bound to transcript words (so transcript_edit_word, validate_captions and export_with_subtitles keep
//! working); the per-word motion is the text engine's word animators keyed on the word times.
use super::kit::*;
use crate::keyframe::{Ease, Prop};
use crate::model::*;
use crate::speech_words as sw;
use crate::transcript::{compose_cue_text, next_id, CueBinding, Take, TakeSource, TakeStatus, Transcript, Word, WordFlag};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

const WHITE: [f64; 3] = [1.0, 1.0, 1.0];
const BLACK: [f64; 3] = [0.0, 0.0, 0.0];
const YELLOW: [f64; 3] = [1.0, 0.86, 0.0];
const GREEN: [f64; 3] = [0.24, 1.0, 0.36];
/// Roman-script Hinglish prompt: steers the recognizer to write Hindi in Roman letters and English words in English.
const HINGLISH_PROMPT: &str = "Haan toh dosto, aaj main aapko kuch dikhata hoon. Yeh kaafi accha hai, aur main isko daily use karta hoon. It's really good, trust me.";
const NBSP: char = '\u{a0}';

struct Look { font: &'static str, weight: f64, size: f64, color: [f64; 3], stroke: f64, shadow: f64, upper: bool, bg: Option<[f64; 4]>,
    active: [f64; 3], keyword: Option<[f64; 3]>, per_line: usize, lines: usize, reveal: bool, pop: f64, karaoke: bool, page_pop: bool, highlight: bool }
/// Style presets. `size` is the type size as a fraction of the frame width; `stroke` a fraction of the type size.
fn look(style: &str) -> Result<Look, String> {
    Ok(match style {
        "pop" => Look { font: "Montserrat", weight: 900.0, size: 0.12, color: WHITE, stroke: 0.11, shadow: 4.0, upper: true, bg: None, active: YELLOW, keyword: Some(GREEN), per_line: 3, lines: 1, reveal: true, pop: 1.22, karaoke: false, page_pop: false, highlight: true },
        "karaoke" => Look { font: "Montserrat", weight: 800.0, size: 0.08, color: WHITE, stroke: 0.09, shadow: 4.0, upper: false, bg: None, active: YELLOW, keyword: Some(GREEN), per_line: 4, lines: 2, reveal: false, pop: 1.08, karaoke: true, page_pop: false, highlight: true },
        "clean" => Look { font: "Inter", weight: 700.0, size: 0.064, color: WHITE, stroke: 0.05, shadow: 8.0, upper: false, bg: None, active: [1.0, 0.9, 0.45], keyword: None, per_line: 6, lines: 2, reveal: false, pop: 1.0, karaoke: false, page_pop: false, highlight: false },
        "boxed" => Look { font: "Montserrat", weight: 900.0, size: 0.09, color: WHITE, stroke: 0.0, shadow: 0.0, upper: true, bg: Some([0.0, 0.0, 0.0, 0.82]), active: GREEN, keyword: Some(YELLOW), per_line: 3, lines: 1, reveal: false, pop: 1.12, karaoke: false, page_pop: false, highlight: true },
        "impact" => Look { font: "Anton", weight: 400.0, size: 0.15, color: WHITE, stroke: 0.07, shadow: 6.0, upper: true, bg: None, active: YELLOW, keyword: Some(YELLOW), per_line: 2, lines: 1, reveal: false, pop: 1.0, karaoke: false, page_pop: true, highlight: false },
        o => return Err(format!("style must be pop|karaoke|clean|boxed|impact, got '{}'", crate::transcript_parse::sanitize(o))),
    })
}

fn bool_arg(a: &Value, k: &str) -> Result<Option<bool>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_bool().map(Some).ok_or_else(|| crate::errors::arg_err(a, k, "boolean")) } }
fn int_arg(a: &Value, k: &str, lo: u64, hi: u64) -> Result<Option<usize>, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => { let n = v.as_u64().filter(|n| (lo..=hi).contains(n)).ok_or_else(|| format!("{k} must be an integer {lo}..{hi}"))?; Ok(Some(n as usize)) } }
}
fn color_arg(a: &Value, k: &str, d: [f64; 3]) -> Result<[f64; 3], String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(d),
        Some(Value::String(s)) => { let h = s.trim_start_matches('#'); if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) { return Err(format!("{k}: a hex colour like \"#FFD400\" or [r,g,b] 0..1")); }
            let p = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0) as f64 / 255.0; Ok([p(0), p(2), p(4)]) }
        Some(Value::Array(v)) if v.len() == 3 => { let mut c = [0.0; 3]; for i in 0..3 { c[i] = v[i].as_f64().filter(|x| (0.0..=1.0).contains(x)).ok_or(format!("{k}: [r,g,b] components 0..1"))?; } Ok(c) }
        _ => Err(format!("{k}: a hex colour like \"#FFD400\" or [r,g,b] 0..1")),
    }
}

fn models_dir() -> PathBuf { std::env::var_os("KIMO_MODELS").map(PathBuf::from).unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".cache/kimo_models")) }
const FETCH_HINT: &str = "run tools/whisper/fetch.sh once (builds whisper.cpp's whisper-cli and downloads the multilingual small model, about 200 MB, into ~/.cache/kimo_models), or set KIMO_WHISPER_BIN and KIMO_WHISPER_MODEL";
fn whisper_bin(a: &Value) -> Result<PathBuf, String> {
    if let Some(p) = a.get("whisper_bin").and_then(|v| v.as_str()) { return Ok(p.into()); }
    if let Some(p) = std::env::var_os("KIMO_WHISPER_BIN") { return Ok(p.into()); }
    let local = models_dir().join("bin/whisper-cli"); if local.is_file() { return Ok(local); }
    if let Some(path) = std::env::var_os("PATH") { for d in std::env::split_paths(&path) { let p = d.join("whisper-cli"); if p.is_file() { return Ok(p); } } }
    // Director mode's own small-model caller has two real next steps right now, not a shell script
    // or an env var it cannot set itself: ship the edit without captions, or call caption_from_words
    // with words it already trusts (it best-effort flags any cue the audio makes hard to verify, the
    // same promise auto_captions makes). Both are named first; adding a real recognizer is secondary.
    Err(format!(
        "no speech model is installed, so auto_captions can't transcribe this audio: skip captions (ship the edit without them), or call caption_from_words with words you already trust, by_asset per clip (it flags, rather than guesses, any cue the audio is hard to verify). auto_captions also takes words:[{{text,start,end}}] directly if you'd rather call it again that way. To add a recognizer later: {FETCH_HINT}"
    ))
}
fn whisper_model(a: &Value) -> Result<PathBuf, String> {
    if let Some(p) = a.get("model").and_then(|v| v.as_str()) { let p = PathBuf::from(p); return if p.is_file() { Ok(p) } else { Err(format!("model file '{}' not found", p.display())) }; }
    if let Some(p) = std::env::var_os("KIMO_WHISPER_MODEL") { return Ok(p.into()); }
    let d = models_dir();
    for f in ["ggml-large-v3-turbo-q5_0.bin", "ggml-small-q5_1.bin", "ggml-small.bin", "ggml-base-q5_1.bin"] { let p = d.join(f); if p.is_file() { return Ok(p); } }
    Err(format!("no Whisper model found in {}: {FETCH_HINT}", d.display()))
}
/// whisper.cpp alignment-head preset for `-dtw`, from the model file name.
fn dtw_preset(model: &std::path::Path) -> Option<&'static str> {
    let n = model.file_name()?.to_string_lossy().to_lowercase(); let en = n.contains(".en");
    Some(if n.contains("large-v3-turbo") { "large.v3.turbo" } else if n.contains("large-v3") { "large.v3" } else if n.contains("large-v2") { "large.v2" } else if n.contains("medium") { if en { "medium.en" } else { "medium" } }
        else if n.contains("small") { if en { "small.en" } else { "small" } } else if n.contains("base") { if en { "base.en" } else { "base" } } else if n.contains("tiny") { if en { "tiny.en" } else { "tiny" } } else { return None })
}

static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn scratch(tag: &str) -> PathBuf { std::env::temp_dir().join(format!("kimo_asr_{}_{}_{tag}", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed))) }

/// 16 kHz mono PCM of `path` from `from` for `dur` seconds, as f32 samples.
fn audio16k(path: &str, from: f64, dur: f64) -> Result<Vec<f32>, String> {
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-v", "error", "-nostdin", "-ss", &format!("{from:.4}"), "-t", &format!("{dur:.4}"), "-i", path, "-vn", "-ac", "1", "-ar", "16000", "-f", "s16le", "-"]);
    let max = ((dur.max(0.1) * 16000.0 * 2.0) as usize + 65536).min(16 * 1024 * 1024);
    let b = capture(&mut cmd, max, Duration::from_secs(60 + dur as u64)).map_err(|e| format!("could not read the audio of '{}': {e}", crate::transcript_parse::sanitize(path)))?;
    Ok(b.chunks_exact(2).map(|x| i16::from_le_bytes([x[0], x[1]]) as f32 / 32768.0).collect())
}
fn write_wav(path: &std::path::Path, x: &[f32]) -> Result<(), String> {
    let n = x.len() as u32 * 2; let mut b = Vec::with_capacity(44 + n as usize);
    b.extend_from_slice(b"RIFF"); b.extend_from_slice(&(36 + n).to_le_bytes()); b.extend_from_slice(b"WAVEfmt "); b.extend_from_slice(&16u32.to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes()); b.extend_from_slice(&1u16.to_le_bytes());
    b.extend_from_slice(&16000u32.to_le_bytes()); b.extend_from_slice(&32000u32.to_le_bytes()); b.extend_from_slice(&2u16.to_le_bytes()); b.extend_from_slice(&16u16.to_le_bytes()); b.extend_from_slice(b"data"); b.extend_from_slice(&n.to_le_bytes());
    for s in x { b.extend_from_slice(&((s.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes()); }
    std::fs::write(path, b).map_err(|e| format!("could not stage audio for the recognizer: {e}"))
}

struct Asr { words: Vec<sw::AsrWord>, language: Option<String> }
fn run_whisper(bin: &std::path::Path, model: &std::path::Path, wav: &std::path::Path, lang: &str, prompt: Option<&str>, secs: f64, extra: &[&str]) -> Result<Asr, String> {
    let out = scratch("out"); let mut cmd = std::process::Command::new(bin);
    let threads = std::thread::available_parallelism().map(|n| n.get().min(4)).unwrap_or(2).to_string();
    cmd.arg("-m").arg(model).arg("-f").arg(wav).args(["-l", lang, "-ojf", "-np", "-nfa", "-t", threads.as_str()]).arg("-of").arg(&out);
    if let Some(p) = dtw_preset(model) { cmd.args(["-dtw", p]); }
    cmd.args(extra);
    if let Some(p) = prompt.filter(|p| !p.trim().is_empty()) { cmd.args(["--prompt", p]); }
    let r = capture(&mut cmd, 8 * 1024 * 1024, Duration::from_secs(120 + (secs * 20.0) as u64));
    let json = out.with_extension("json"); let bytes = std::fs::read(&json); let _ = std::fs::remove_file(&json);
    if let Err(e) = r { return Err(format!("speech recognizer failed ({e}); check that '{}' runs and the model file is complete", bin.display())); }
    let (words, language) = sw::parse_whisper_cpp(&bytes.map_err(|e| format!("speech recognizer wrote no output: {e}"))?)?;
    Ok(Asr { words, language })
}

/// One stretch of speech to recognize: a media clip's audio, mapped to the timeline by `start + t / speed`.
pub(crate) struct Src { clip_id: String, asset_id: String, path: String, in_point: f64, dur_src: f64, start: f64, speed: f64 }

pub(crate) fn sources(ss: &Session, a: &Value, warnings: &mut Vec<String>) -> Result<Vec<Src>, String> {
    let want: Option<Vec<String>> = match a.get("clip_ids") { None | Some(Value::Null) => None, Some(v) => Some(v.as_array().ok_or("clip_ids must be an array of clip ids")?.iter().map(|x| x.as_str().map(String::from).ok_or("clip_ids must be strings".to_string())).collect::<Result<_, _>>()?) };
    let p = &ss.project; let mut out = vec![];
    if let Some(w) = &want { for id in w { if p.find_clip(id).is_none() { return Err(format!("no clip '{}' (get_project lists clip ids)", crate::transcript_parse::sanitize(id))); } } }
    for tr in &p.tracks { for c in &tr.clips {
        if let Some(w) = &want { if !w.contains(&c.id) { continue; } } else if tr.muted { continue; }
        let Source::Media { asset } = &c.source else { if want.is_some() { return Err(format!("clip '{}' is not a media clip with sound", c.id)); } continue };
        let Some(ast) = p.assets.get(asset) else { continue };
        if ast.kind.as_deref().map_or(false, |k| k == "image") { if want.is_some() { return Err(format!("clip '{}' is a still image, it has no speech", c.id)); } continue; }
        if c.time_remap.is_some() || c.speed_curve.is_some() || !(c.speed.is_finite() && c.speed > 0.0) { warnings.push(format!("clip {} skipped: time remap or speed ramp (caption it before retiming)", c.id)); continue; }
        out.push(Src { clip_id: c.id.clone(), asset_id: asset.clone(), path: p.resolve(&ast.path), in_point: c.in_point, dur_src: c.duration * c.speed, start: c.start, speed: c.speed });
    } }
    if out.is_empty() { return Err("no clip with speech to caption: add the talking clips first, or pass clip_ids, or words:[{text,start,end}]".into()); }
    out.sort_by(|x, y| x.start.partial_cmp(&y.start).unwrap_or(std::cmp::Ordering::Equal)); Ok(out)
}

/// Recognized + refined words of one source (times in source seconds from its in point) and the language used.
pub(crate) fn recognize(src: &Src, a: &Value, mode: &mut String, lang_seen: &mut Option<String>) -> Result<(Vec<sw::TimedWord>, Vec<f32>), String> {
    let x = audio16k(&src.path, src.in_point, src.dur_src)?;
    if x.len() < 1600 { return Ok((vec![], x)); }
    let bin = whisper_bin(a)?; let model = whisper_model(a)?; let wav = scratch("in.wav"); write_wav(&wav, &x)?;
    let hint = a.get("hint").and_then(|v| v.as_str()).map(crate::transcript_parse::sanitize).unwrap_or_default();
    let secs = x.len() as f64 / 16000.0; let lang = a.get("language").and_then(|v| v.as_str()).unwrap_or("auto").to_lowercase();
    let roman = a.get("script").and_then(|v| v.as_str()).unwrap_or("roman") != "native";
    let hp = format!("{HINGLISH_PROMPT} {hint}");
    // beam search by default (Hinglish WER 45.8% vs 52.3% greedy on Naman's clips); fast = greedy, about 3x quicker
    let fast: &[&str] = if bool_arg(a, "fast")?.unwrap_or(false) { &["-bs", "1", "-bo", "1"] } else { &[] };
    let respell = bool_arg(a, "normalize_spelling")?.unwrap_or(true);
    // large-v3-turbo asked for -l en (with or without the Roman prompt) translates Hindi speech into English (62.7% WER on Naman's
    // clips); asked for -l hi it writes what was said, in Devanagari, romanized below (49.7% strict / 37.9% ignoring spelling).
    let turbo = model.file_name().map_or(false, |n| n.to_string_lossy().to_lowercase().contains("turbo"));
    let hinglish_pass = || if turbo { run_whisper(&bin, &model, &wav, "hi", Some(hint.as_str()), secs, fast) } else { run_whisper(&bin, &model, &wav, "en", Some(hp.as_str()), secs, fast) };
    let r = (|| -> Result<Asr, String> {
        match lang.as_str() {
            "hinglish" => { *mode = "hinglish".into(); hinglish_pass() }
            "hi" | "ur" if roman => { *mode = "hinglish".into(); hinglish_pass() }
            "auto" => {
                // language id only (one encoder pass, seconds), then one full pass in the right mode
                let det = run_whisper(&bin, &model, &wav, "auto", None, secs, &["-dl"])?.language;
                if roman && matches!(det.as_deref(), Some("hi") | Some("ur") | Some("pa") | Some("mr")) { *mode = "hinglish".into(); let mut r = hinglish_pass()?; r.language = det; Ok(r) }
                else { let l = det.clone().unwrap_or_else(|| "auto".into()); *mode = l.clone(); let mut r = run_whisper(&bin, &model, &wav, &l, Some(hint.as_str()), secs, fast)?; if r.language.is_none() { r.language = det; } Ok(r) }
            }
            l => { *mode = l.to_string(); run_whisper(&bin, &model, &wav, l, Some(hint.as_str()), secs, fast) }
        }
    })();
    let _ = std::fs::remove_file(&wav); let r = r?;
    if lang_seen.is_none() { *lang_seen = r.language.clone(); }
    let o = sw::onsets(&x, 16000); let mut words = sw::refine(&r.words, &o);
    if roman { for w in &mut words { if sw::has_devanagari(&w.text) { w.text = sw::romanize(&w.text); } } }
    // one spelling per Hinglish word (bahut / bohat -> bohot): 47.1% -> 39.2% WER strict on Naman's clips; English output is never respelled
    if roman && respell && mode.as_str() == "hinglish" { words = sw::normalize_hinglish(&words).0; }
    Ok((words, x))
}

fn anim(id: &str, start: f64, end: f64) -> TextAnimator {
    TextAnimator { id: id.into(), unit: "word".into(), shape: "square".into(), start: Prop::new(start), end: Prop::new(end), offset: Prop::new(0.0), smoothness: Prop::new(0.0),
        x: Prop::new(0.0), y: Prop::new(0.0), scale: Prop::new(1.0), rotation: Prop::new(0.0), opacity: Prop::new(1.0), tracking: Prop::new(0.0), blur: Prop::new(0.0), random: false, seed: 0, fill: None, fill_amount: None, ease: None, wiggly: None }
}
/// Per-word motion for one page. `s`/`e`: word start/end in page-local seconds; `kw`: keyword indices; `dur`: page length.
fn page_animators(l: &Look, active: [f64; 3], kwc: Option<[f64; 3]>, s: &[f64], e: &[f64], kw: &[usize], highlight: bool, dur: f64) -> Vec<TextAnimator> {
    let n = s.len().max(1) as f64; let lead = 0.04; let at = |t: f64| t.clamp(0.0, dur);
    let mut v = vec![];
    if let Some(c) = kwc { for (i, &k) in kw.iter().enumerate() { let mut k_an = anim(&format!("kw{i}"), k as f64 / n, (k + 1) as f64 / n); k_an.fill = Some(c); v.push(k_an); } }
    if l.reveal {
        // words not yet spoken are hidden: the window [offset, offset+3] covers every word after the current one
        let mut r = anim("reveal", 0.0, 3.0); r.opacity = Prop::new(0.0); r.offset.set_key(0.0, 0.0, Ease::Hold);
        for j in 0..s.len() { r.offset.set_key(at(s[j] - lead), (j + 1) as f64 / n, Ease::Hold); } v.push(r);
    }
    if l.karaoke && highlight {
        // spoken words keep the accent colour: the window [offset-2, offset] covers every word up to the current one
        let mut k = anim("spoken", -2.0, 0.0); k.fill = Some(active); k.offset.set_key(0.0, 0.0, Ease::Hold);
        for j in 0..s.len() { k.offset.set_key(at(s[j] - lead), (j + 1) as f64 / n, Ease::Hold); } v.push(k);
    }
    if highlight || l.reveal {
        // the active word: coloured (unless karaoke already colours it) and popped in on its first frame
        let mut act = anim("active", 0.0, 1.0 / n); act.offset.set_key(0.0, -2.0, Ease::Hold); if highlight && !l.karaoke { act.fill = Some(active); }
        for j in 0..s.len() {
            let t0 = at(s[j] - lead); act.offset.set_key(t0, j as f64 / n, Ease::Hold);
            if j + 1 < s.len() && s[j + 1] - e[j] > 0.6 { act.offset.set_key(at(e[j] + 0.15), -2.0, Ease::Hold); }
            if l.pop > 1.0 { let t1 = at(t0 + 0.12).min(if j + 1 < s.len() { at(s[j + 1] - lead - 0.001) } else { dur }); act.scale.set_key(t0, l.pop, if l.reveal { Ease::OutBack } else { Ease::OutQuart }); if t1 > t0 { act.scale.set_key(t1, 1.0, Ease::Hold); } }
        }
        v.push(act);
    }
    v
}

fn caption_band_for(ss: &Session, srcs: &[Src], position: &str) -> (f64, &'static str) {
    let (w, h) = (ss.project.width as f64, ss.project.height as f64); let tall = h > w * 1.05;
    if position != "auto" { return sw::caption_band(tall, &[], position); }
    let mut faces = vec![];
    for s in srcs.iter().take(4) {
        let Some(ast) = ss.project.find_clip(&s.clip_id).and_then(|c| if let Source::Media { asset } = &c.source { ss.project.assets.get(asset) } else { None }) else { continue };
        if ast.width == 0 || ast.height == 0 || ((ast.width as f64 / ast.height as f64) / (w / h) - 1.0).abs() > 0.12 { continue; }
        for k in 1..=3 { let t = s.in_point + s.dur_src * k as f64 / 4.0; let mut cmd = std::process::Command::new("ffmpeg");
            cmd.args(["-v", "error", "-nostdin", "-ss", &format!("{t:.3}"), "-i", &s.path, "-frames:v", "1", "-vf", "scale=72:128", "-f", "rawvideo", "-pix_fmt", "rgb24", "-"]);
            if let Ok(b) = capture(&mut cmd, 72 * 128 * 3 + 16, Duration::from_secs(20)) { if let Some(f) = sw::skin_band(&b, 72, 128) { faces.push(f); } } }
    }
    sw::caption_band(tall, &faces, "auto")
}

struct Built { caption_id: String, track_id: String, transcript_id: String, pages: usize, words: usize, band: &'static str, font_fallback: bool }

/// Builds transcript + caption pages from timeline-timed words grouped by take. `takes`: (take, words in source seconds).
fn build(ss: &mut Session, a: &Value, l: &Look, takes: Vec<(Take, Vec<sw::TimedWord>)>, language: &str, band: (f64, &'static str)) -> Result<Built, String> {
    let style = a.get("style").and_then(|v| v.as_str()).unwrap_or("pop");
    let per_line = int_arg(a, "max_words_per_line", 1, 8)?.unwrap_or(l.per_line); let lines = int_arg(a, "lines", 1, 2)?.unwrap_or(l.lines);
    let highlight = bool_arg(a, "highlight")?.unwrap_or(l.highlight); let use_kw = bool_arg(a, "keywords")?.unwrap_or(l.keyword.is_some());
    let color = color_arg(a, "color", l.color)?; let active = color_arg(a, "active_color", l.active)?; let kwc = color_arg(a, "keyword_color", l.keyword.unwrap_or(YELLOW))?;
    let font = a.get("font").and_then(|v| v.as_str()).unwrap_or(l.font).to_string(); let font_fallback = crate::text::resolve_font_info(&font).1;
    let (w, h) = (ss.project.width as f64, ss.project.height as f64);
    // transcript
    let tr_id = next_id("tr", ss.project.editorial.transcripts.keys().map(|s| s.as_str()));
    let mut tr = Transcript { id: tr_id.clone(), name: "auto captions".into(), language: language.to_string(), ..Default::default() };
    for (k, (mut take, ws)) in takes.into_iter().enumerate() {
        take.id = format!("take{}", k + 1); take.order = k as u32;
        for wd in &ws { let wid = format!("w{}", tr.words.len() + 1); let mut nw = Word::new(&wid, &take.id, &wd.text, wd.start, wd.end, Some(wd.conf)); nw.display = nw.display.replace(' ', &NBSP.to_string()); if wd.conf < 0.5 { nw.add_flag(WordFlag::LowConfidence); } tr.words.push(nw); }
        tr.takes.push(take);
    }
    tr.version += 1; tr.derive_times(); tr.validate()?;
    let mut order: Vec<usize> = (0..tr.words.len()).collect(); order.sort_by(|&x, &y| tr.words[x].start.partial_cmp(&tr.words[y].start).unwrap_or(std::cmp::Ordering::Equal));
    // drop words that overlap an earlier, more confident word from another take (two clips playing at once)
    let mut keep: Vec<usize> = vec![];
    for &i in &order { if let Some(&j) = keep.last() { if tr.words[i].start < tr.words[j].end - 0.05 && tr.words[i].take_id != tr.words[j].take_id { if tr.words[i].confidence > tr.words[j].confidence { keep.pop(); } else { continue; } } } keep.push(i); }
    if keep.is_empty() { return Err("no speech recognized in the chosen clips".into()); }
    let tw: Vec<sw::TimedWord> = keep.iter().map(|&i| sw::TimedWord { text: tr.words[i].display.clone(), start: tr.words[i].start, end: tr.words[i].end, conf: tr.words[i].confidence.unwrap_or(1.0) }).collect();
    let max_chars = (if l.upper { 16 } else { 20 }).max(per_line * 5);
    let pages = sw::paginate(&tw, per_line, lines, max_chars);
    let mut freq: HashMap<String, usize> = HashMap::new(); for x in &tw { *freq.entry(sw::norm_word(&x.text)).or_insert(0) += 1; }
    // track
    if let Some(old) = a.get("replace").and_then(|v| v.as_str()) { remove_group(ss, old)?; }
    let track_id = ss.project.gen_id("track"); ss.project.tracks.push(Track { id: track_id.clone(), clips: vec![], locked: false, muted: false, name: "Captions".into() });
    let y = (band.0 - 0.5) * h; let mut clip_ids = vec![]; let mut prev_end = f64::NEG_INFINITY;
    for (pi, pg) in pages.iter().enumerate() {
        let idx: Vec<usize> = (pg.first..=pg.last).map(|k| keep[k]).collect();
        let disp: Vec<String> = idx.iter().map(|&i| tr.words[i].display.clone()).collect(); let dref: Vec<&str> = disp.iter().map(|s| s.as_str()).collect();
        let text = compose_cue_text(&dref, &pg.breaks_after);
        let mut fx = TextFx { weight: l.weight, uppercase: l.upper, bg: l.bg, bg_pad: (l.size * w * 0.32).round(), bg_radius: (l.size * w * 0.22).round(), shadow_blur: l.shadow, shadow_opacity: if l.shadow > 0.0 { 0.55 } else { 0.0 }, ..Default::default() };
        // type size: the preset, shrunk until the longest line fits 86% of the width
        let mut size = opt_num(a, "size", (l.size * w).round()).clamp(8.0, 600.0);
        if let Ok(m) = crate::text::measure_text(&text, &font, size, 0.0, l.weight, "center", w, h, Some(&fx)) { if m.width > 0.86 * w { size = (size * 0.86 * w / m.width).floor().max(8.0); } }
        fx.box_width = 0.92 * w;
        let s: Vec<f64> = idx.iter().map(|&i| tr.words[i].start - pg.start).collect(); let e: Vec<f64> = idx.iter().map(|&i| tr.words[i].end - pg.start).collect();
        let kw: Vec<usize> = if use_kw && dref.len() >= 2 { sw::keywords(&dref, if dref.len() >= 6 { 2 } else { 1 }, &freq) } else { vec![] };
        let dur = (pg.end - pg.start).max(0.05);
        fx.animators = page_animators(l, active, if use_kw { Some(kwc) } else { None }, &s, &e, &kw, highlight, dur);
        let mut props = default_props(); props.insert("y".into(), Prop::new(y.round()));
        // page in: a quick fade and settle (a bigger pop for impact); page out: a fade only when a gap follows
        let mut op = Prop::new(1.0); let mut sc = Prop::new(1.0);
        if pg.start - prev_end > 0.05 { op.set_key(0.0, 0.0, Ease::OutQuart); op.set_key(0.07f64.min(dur / 2.0), 1.0, Ease::Hold); }
        if l.page_pop { sc.set_key(0.0, 0.62, Ease::OutBack); sc.set_key(0.16f64.min(dur), 1.0, Ease::Hold); } else if !l.reveal { sc.set_key(0.0, 0.94, Ease::OutQuart); sc.set_key(0.1f64.min(dur), 1.0, Ease::Hold); }
        let gap_after = pages.get(pi + 1).map_or(1.0, |n| n.start - pg.end); if gap_after > 0.05 && dur > 0.2 { if op.keys.is_empty() { op.set_key(0.0, 1.0, Ease::Hold); } op.set_key(dur - 0.08, 1.0, Ease::InOut); op.set_key(dur, 0.0, Ease::Hold); }
        if !op.keys.is_empty() { props.insert("opacity".into(), op); } if !sc.keys.is_empty() { props.insert("scale".into(), sc); }
        let id = ss.project.gen_id("clip");
        let src = Source::Text { text, font: font.clone(), size, color, letter_spacing: 0.0, stroke_width: (l.stroke * size).round(), stroke_color: BLACK, shadow: if l.shadow > 0.0 { (size * 0.05).round() } else { 0.0 }, align: "center".into() };
        let c = Clip { id: id.clone(), source: src, start: pg.start, duration: dur, in_point: 0.0, speed: 1.0, speed_curve: None, speed_blend: Default::default(), time_remap: None, props, grade_graph: None, effects: vec![], blend: "normal".into(), mask: None, time_mode: TimeMode::Normal, corners: None, audio: Audio::default(), motion_blur: 0.0, echo: None, motion_path: None, posterize_fps: 0.0, text_fx: Some(fx), parent: None, inherit: Default::default(), name: Some(format!("caption {}", pi + 1)), role: None, tags: vec!["auto_captions".into()], transition_in: None };
        ss.project.tracks.last_mut().unwrap().clips.push(std::sync::Arc::new(c));
        ss.project.editorial.cue_words.insert(id.clone(), CueBinding { word_ids: idx.iter().map(|&i| tr.words[i].id.clone()).collect(), breaks_after: pg.breaks_after.clone() });
        clip_ids.push(id); prev_end = pg.end;
    }
    let nwords = keep.len();
    ss.project.editorial.transcripts.insert(tr_id.clone(), tr);
    let caption_id = ss.project.gen_id("captions");
    ss.project.editorial.captions.insert(caption_id.clone(), crate::editorial::CaptionGroup { track_id: track_id.clone(), clips: clip_ids, style: format!("auto_{style}"), burn_in: true, transcript_id: Some(tr_id.clone()) });
    Ok(Built { caption_id, track_id, transcript_id: tr_id, pages: pages.len(), words: nwords, band: band.1, font_fallback })
}

fn remove_group(ss: &mut Session, id: &str) -> Result<(), String> {
    let g = ss.project.editorial.captions.remove(id).ok_or_else(|| format!("replace: no caption group '{}'", crate::transcript_parse::sanitize(id)))?;
    for c in &g.clips { ss.project.editorial.cue_words.remove(c); }
    if let Some(t) = ss.project.tracks.iter_mut().find(|t| t.id == g.track_id) { t.clips.retain(|c| !g.clips.contains(&c.id)); }
    if ss.project.tracks.iter().any(|t| t.id == g.track_id && t.clips.is_empty() && t.name == "Captions") { ss.project.tracks.retain(|t| t.id != g.track_id); }
    Ok(())
}

fn cues_of(ss: &Session, id: &str) -> Vec<(f64, f64, String)> {
    let Some(g) = ss.project.editorial.captions.get(id) else { return vec![] };
    let mut v: Vec<(f64, f64, String)> = g.clips.iter().filter_map(|cid| ss.project.find_clip(cid)).filter_map(|c| if let Source::Text { text, .. } = &c.source { Some((c.start, c.start + c.duration, text.replace(NBSP, " "))) } else { None }).collect();
    v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); v
}
fn write_sidecars(ss: &Session, a: &Value, id: &str) -> Result<Vec<String>, String> {
    let ow = bool_arg(a, "overwrite")?.unwrap_or(false); let cues = cues_of(ss, id); let mut out = vec![];
    if let Some(p) = a.get("srt_path").and_then(|v| v.as_str()) { crate::transcript_captions::write_atomic(std::path::Path::new(p), &crate::transcript_captions::to_srt(&cues), ow)?; out.push(p.to_string()); }
    if let Some(p) = a.get("vtt_path").and_then(|v| v.as_str()) { crate::transcript_captions::write_atomic(std::path::Path::new(p), &crate::transcript_captions::to_vtt(&cues), ow)?; out.push(p.to_string()); }
    Ok(out)
}

fn auto_captions(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let style = a.get("style").and_then(|v| v.as_str()).unwrap_or("pop"); let l = look(style)?;
    let position = a.get("position").and_then(|v| v.as_str()).unwrap_or("auto");
    if !["auto", "bottom", "middle", "top"].contains(&position) { return Err("position must be auto|bottom|middle|top".into()); }
    if let Some(s) = a.get("script").and_then(|v| v.as_str()) { if s != "roman" && s != "native" { return Err("script must be roman|native".into()); } }
    let mut warnings: Vec<String> = vec![]; let mut mode = String::from("words"); let mut lang_seen: Option<String> = None;
    let (takes, band) = if let Some(ws) = a.get("words") {
        // caption known words (timeline seconds), no recognizer
        let arr = ws.as_array().ok_or("words must be an array of {text, start, end}")?; if arr.is_empty() { return Err("words is empty".into()); }
        let mut v = vec![]; let mut prev = f64::NEG_INFINITY;
        for (i, x) in arr.iter().enumerate() {
            let num = |k: &str| x.get(k).and_then(|v| v.as_f64()).filter(|v| v.is_finite()).ok_or(format!("word {i}: '{k}' must be a number of seconds"));
            let (st, en) = (num("start")?, num("end")?); let t = x.get("text").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).unwrap_or_default();
            if t.is_empty() { return Err(format!("word {i}: text is empty")); } if st < 0.0 || en <= st { return Err(format!("word {i}: need 0 <= start < end")); } if st < prev { return Err(format!("word {i}: words must be in time order")); }
            prev = st; v.push(sw::TimedWord { text: t, start: st, end: en, conf: 1.0 });
        }
        let lo = v.iter().map(|w| w.start).fold(f64::MAX, f64::min); let hi = v.iter().map(|w| w.end).fold(0.0, f64::max);
        let take = Take { id: String::new(), source: TakeSource { asset_id: None, clip_id: None, path_key: Some("words".into()) }, src_start: 0.0f64.min(lo), src_end: hi, timeline_start: Some(0.0), speed: 1.0, order: 0, status: TakeStatus::Chosen, note: String::new() };
        let band = if position == "auto" { let srcs = sources(ss, &json!({}), &mut vec![]).unwrap_or_default(); caption_band_for(ss, &srcs, "auto") } else { caption_band_for(ss, &[], position) };
        (vec![(take, v)], band)
    } else {
        let srcs = sources(ss, a, &mut warnings)?; let mut takes = vec![];
        let mut modes: Vec<String> = vec![];
        for s in &srcs {
            let (ws, _) = recognize(s, a, &mut mode, &mut lang_seen)?;
            if !modes.contains(&mode) { modes.push(mode.clone()); }
            let ws: Vec<sw::TimedWord> = ws.into_iter().filter(|w| w.start < s.dur_src).map(|mut w| { w.end = w.end.min(s.dur_src); w }).collect();
            if ws.is_empty() { continue; }
            let take = Take { id: String::new(), source: TakeSource { asset_id: Some(s.asset_id.clone()), clip_id: Some(s.clip_id.clone()), path_key: None }, src_start: 0.0, src_end: s.dur_src, timeline_start: Some(s.start), speed: s.speed, order: 0, status: TakeStatus::Chosen, note: String::new() };
            takes.push((take, ws));
        }
        if takes.is_empty() { return Err("no speech recognized in the chosen clips".into()); }
        // one mode per clip (a Hindi clip and an English clip in one edit): report each, Hinglish first
        modes.sort_by_key(|m| m != "hinglish"); mode = modes.join("+");
        let band = caption_band_for(ss, &srcs, position); (takes, band)
    };
    let all: Vec<&sw::TimedWord> = takes.iter().flat_map(|t| t.1.iter()).collect();
    let conf = all.iter().map(|w| w.conf as f64).sum::<f64>() / all.len().max(1) as f64; let low = all.iter().filter(|w| w.conf < 0.5).count();
    let preview: String = all.iter().take(14).map(|w| w.text.as_str()).collect::<Vec<_>>().join(" ");
    let language = lang_seen.clone().unwrap_or_else(|| a.get("language").and_then(|v| v.as_str()).unwrap_or("").to_string());
    let b = build(ss, a, &l, takes, &language, band)?;
    let files = write_sidecars(ss, a, &b.caption_id)?;
    if b.font_fallback { warnings.push(format!("font for style {style} is not installed: run fonts/fetch.sh (a fallback font is used)")); }
    if mode.starts_with("hinglish") { warnings.push("Hindi speech written in Roman letters (Hinglish); script:\"native\" keeps Devanagari, which Kimo's text engine draws without conjunct shaping".into()); }
    if conf < 0.6 { warnings.push("low recognition confidence: check the words (edit_caption_words fixes one without re-timing)".into()); }
    Ok(json!({"caption_id": b.caption_id, "track_id": b.track_id, "transcript_id": b.transcript_id, "words": b.words, "pages": b.pages, "style": style, "position": b.band,
        "language": language, "mode": mode, "confidence": (conf * 100.0).round() / 100.0, "low_confidence_words": low, "preview": preview, "files": files, "warnings": warnings,
        "next": "edit_caption_words fixes a misheard word; export_captions writes .srt/.vtt; set_caption_style is not needed (pick style here)"}))
}

fn group_id(ss: &Session, a: &Value) -> Result<String, String> {
    match a.get("caption_id").and_then(|v| v.as_str()) {
        Some(id) => if ss.project.editorial.captions.contains_key(id) { Ok(id.into()) } else { Err(format!("no caption group '{}' (list_captions lists them)", crate::transcript_parse::sanitize(id))) },
        None => { let v: Vec<&String> = ss.project.editorial.captions.iter().filter(|(_, g)| g.style.starts_with("auto_")).map(|(k, _)| k).collect(); match v.len() { 1 => Ok(v[0].clone()), 0 => Err("no auto caption group: run auto_captions first".into()), _ => Err("several caption groups: pass caption_id".into()) } }
    }
}
/// Word ids of a caption group in reading order.
fn group_words(ss: &Session, id: &str) -> Vec<(String, String)> {
    let g = &ss.project.editorial.captions[id]; let mut v = vec![];
    let mut clips: Vec<(f64, &String)> = g.clips.iter().filter_map(|c| ss.project.find_clip(c).map(|x| (x.start, c))).collect(); clips.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    for (_, c) in clips { if let Some(b) = ss.project.editorial.cue_words.get(c) { for w in &b.word_ids { v.push((c.clone(), w.clone())); } } } v
}

fn edit_caption_words(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = group_id(ss, a)?; let tid = ss.project.editorial.captions[&id].transcript_id.clone().ok_or("this caption group has no transcript (use edit_caption on its clips)")?;
    let edits = a.get("edits").and_then(|v| v.as_array()).ok_or("edits: an array of {index | word_id | find, text}")?; if edits.is_empty() { return Err("edits is empty".into()); }
    let words = group_words(ss, &id); let mut changed: Vec<Value> = vec![]; let mut touched: Vec<String> = vec![];
    let mut tr = ss.project.editorial.transcripts.get(&tid).cloned().ok_or("the caption group's transcript is gone")?;
    for (k, e) in edits.iter().enumerate() {
        let text = e.get("text").and_then(|v| v.as_str()).map(|s| s.trim().to_string()).ok_or(format!("edit {k}: text is required"))?;
        if text.is_empty() { return Err(format!("edit {k}: text is empty; a caption word can be replaced but not removed")); }
        let targets: Vec<usize> = if let Some(i) = e.get("index").and_then(|v| v.as_u64()) { if (i as usize) < words.len() { vec![i as usize] } else { return Err(format!("edit {k}: index {i} is past the last word ({})", words.len().saturating_sub(1))); } }
            else if let Some(w) = e.get("word_id").and_then(|v| v.as_str()) { match words.iter().position(|x| x.1 == w) { Some(i) => vec![i], None => return Err(format!("edit {k}: no word '{}' in this caption group", crate::transcript_parse::sanitize(w))) } }
            else if let Some(f) = e.get("find").and_then(|v| v.as_str()) { let f = sw::norm_word(f); let all = e.get("all").and_then(|v| v.as_bool()).unwrap_or(false);
                let hits: Vec<usize> = words.iter().enumerate().filter(|(_, x)| tr.words.iter().find(|w| w.id == x.1).map_or(false, |w| sw::norm_word(&w.display.replace(NBSP, " ")) == f)).map(|(i, _)| i).collect();
                if hits.is_empty() { return Err(format!("edit {k}: no caption word matches '{}'", crate::transcript_parse::sanitize(&f))); } if all { hits } else { vec![hits[0]] } }
            else { return Err(format!("edit {k}: give index, word_id or find")); };
        for i in targets { let wid = words[i].1.clone(); let old = tr.set_display(&wid, &text.replace(' ', &NBSP.to_string()))?; changed.push(json!({"index": i, "from": old.replace(NBSP, " "), "to": text})); if !touched.contains(&words[i].0) { touched.push(words[i].0.clone()); } }
    }
    for c in &touched {
        let Some(b) = ss.project.editorial.cue_words.get(c).cloned() else { continue };
        let disp: Vec<String> = b.word_ids.iter().filter_map(|w| tr.words.iter().find(|x| &x.id == w).map(|x| x.display.clone())).collect(); let dref: Vec<&str> = disp.iter().map(|s| s.as_str()).collect();
        let text = compose_cue_text(&dref, &b.breaks_after);
        if let Some(cl) = ss.project.find_clip_mut(c) { if let Source::Text { text: t, .. } = &mut cl.source { *t = text; } }
    }
    ss.project.editorial.transcripts.insert(tid, tr);
    Ok(json!({"caption_id": id, "changed": changed, "cue_ids": touched, "timing": "unchanged"}))
}

fn export_captions(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let id = group_id(ss, a)?; let path = str_arg(a, "path")?;
    let fmt = match a.get("format").and_then(|v| v.as_str()) { Some(f) => f.to_string(), None => std::path::Path::new(path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default() };
    let cues = cues_of(ss, &id); if cues.is_empty() { return Err("the caption group has no cues".into()); }
    let body = match fmt.as_str() { "srt" => crate::transcript_captions::to_srt(&cues), "vtt" => crate::transcript_captions::to_vtt(&cues), _ => return Err("format must be srt or vtt (or a path ending in .srt / .vtt)".into()) };
    crate::transcript_captions::write_atomic(std::path::Path::new(path), &body, bool_arg(a, "overwrite")?.unwrap_or(false))?;
    Ok(json!({"path": path, "format": fmt, "cues": cues.len()}))
}

pub fn commands() -> Vec<Command> {
    let colr = |d: &str| json!({"description": d, "anyOf": [{"type":"string"}, {"type":"array","items":{"type":"number"},"minItems":3,"maxItems":3}]});
    vec![
        Command { name: "auto_captions", description: "Transcribe the talking clips locally and add word-synced animated captions in one call. Styles: pop, karaoke, clean, boxed, impact. Hinglish aware.", mutates: true,
            schema: obj(vec![("clip_ids", json!({"type":"array","items":{"type":"string"},"description":"clips to transcribe; default every audible media clip"})),
                ("style", json!({"type":"string","enum":["pop","karaoke","clean","boxed","impact"],"description":"default pop"})),
                ("position", json!({"type":"string","enum":["auto","bottom","middle","top"],"description":"default auto: lower band, or top when a face is low"})),
                ("max_words_per_line", n("1..8, default by style (pop 3)")), ("lines", n("1 or 2, default by style")),
                ("language", s("auto (default), hinglish, or a code like en, hi, es")), ("script", json!({"type":"string","enum":["roman","native"],"description":"Hindi output script, default roman"})),
                ("hint", s("names or terms the speaker says, to help recognition")), ("fast", json!({"type":"boolean","description":"greedy decoding: about 3x faster, less accurate (default false)"})), ("normalize_spelling", json!({"type":"boolean","description":"Hinglish only: one spelling for common variants (bahut/bohat -> bohot, pani -> paani); default true"})), ("highlight", json!({"type":"boolean","description":"colour + pop the word being spoken (default by style)"})),
                ("keywords", json!({"type":"boolean","description":"colour 1-2 stress words per page (default by style)"})),
                ("font", s("font name or .ttf path (default by style)")), ("size", n("type size px (default by style, shrunk to fit)")), ("color", colr("text colour")), ("active_color", colr("spoken-word colour")), ("keyword_color", colr("keyword colour")),
                ("words", json!({"type":"array","items":{"type":"object"},"description":"skip recognition: [{text,start,end}] timeline seconds"})),
                ("replace", s("caption_id of an earlier auto group to replace")), ("srt_path", s("also write an .srt")), ("vtt_path", s("also write a .vtt")), ("overwrite", json!({"type":"boolean"}))], &[]),
            run: auto_captions },
        Command { name: "edit_caption_words", description: "Fix misheard caption words without re-timing: edits [{index | word_id | find, text}]. Timing and animation stay.", mutates: true,
            schema: obj(vec![("caption_id", s("auto caption group (default: the only one)")), ("edits", json!({"type":"array","items":{"type":"object"},"description":"[{index:0-based | word_id | find:\"word\", all?:true, text:\"fixed\"}]"}))], &["edits"]),
            run: edit_caption_words },
        Command { name: "export_captions", description: "Write a caption group as an SRT or WebVTT sidecar file. Burned-in captions come with the normal export.", mutates: false,
            schema: obj(vec![("caption_id", s("caption group (default: the only auto group)")), ("path", s(".srt or .vtt path")), ("format", json!({"type":"string","enum":["srt","vtt"]})), ("overwrite", json!({"type":"boolean"}))], &["path"]),
            run: export_captions },
    ]
}
