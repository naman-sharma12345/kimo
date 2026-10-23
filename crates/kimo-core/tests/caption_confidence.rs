//! Caption confidence (KA1, hackathon ASR robustness): caption_from_words is told words by the caller, not recognized by
//! kimo - on Naman's real accented/informal clips the words it was given (an outside auto-transcript) turned out to be
//! whisper hallucinations ("bohozhada", "kettlebanyan") that a small director-mode model had to hand-fix after the fact,
//! with nothing to tell it which cues were suspect. This best-effort cross-checks each cue's given words against what
//! kimo's own already-Hinglish-tuned recognizer (auto_captions's pipeline: beam search, Hinglish prompt, Hindi pass) hears
//! on that SAME audio, and flags any cue the recognizer genuinely cannot make out - so lint_edit can hand a small model the
//! exact clip_id and an edit_caption fix, instead of unverified nonsense reaching the screen unflagged.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_caption_confidence_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }

/// A stub whisper-cli that "hears" three words at a fixed confidence `p`, whatever audio or args it is actually given
/// (same whisper.cpp `--output-json-full` shape the real auto_captions tests stub).
fn stub(p: f32, tag: &str) -> PathBuf {
    let d = dir(); let bin = d.join(format!("whisper_conf_{tag}.sh"));
    let tok = |t: &str, ms: u32, dtw: u32| format!("{{\"text\":\"{t}\",\"offsets\":{{\"from\":{ms},\"to\":{}}},\"p\":{p},\"t_dtw\":{dtw}}}", ms + 200);
    let body = format!("[{{\"text\":\"[_BEG_]\",\"offsets\":{{\"from\":0,\"to\":0}},\"p\":0.9,\"t_dtw\":-1}},{},{},{}]",
        tok(" one", 300, 80), tok(" two", 800, 130), tok(" three", 1200, 185));
    let script = format!("#!/bin/sh\nOF=''\nwhile [ $# -gt 0 ]; do case \"$1\" in -of) OF=\"$2\"; shift;; esac; shift; done\ncat > \"$OF.json\" <<EOF\n{{\"result\":{{\"language\":\"en\"}},\"transcription\":[{{\"offsets\":{{\"from\":0,\"to\":2000}},\"text\":\" one two three.\",\"tokens\":{body}}}]}}\nEOF\n");
    std::fs::write(&bin, script).unwrap(); Command::new("chmod").arg("+x").arg(&bin).status().unwrap(); bin
}
fn model() -> PathBuf { let p = dir().join("ggml-small-q5_1.bin"); if !p.exists() { std::fs::write(&p, b"stub").unwrap(); } p }

/// 2 s of 360x640 video with three tone bursts (speech stand-ins) - the same fixture shape auto_captions's own tests use,
/// so the dialogue-track speech detector has something to time cues against.
fn bursts() -> PathBuf {
    let p = dir().join("bursts.mp4"); if !p.exists() {
        let st = Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", "color=c=gray:s=360x640:r=30:d=2", "-f", "lavfi", "-i",
            "aevalsrc='0.5*sin(2*PI*1000*t)*(between(t,0.3,0.5)+between(t,0.8,1.0)+between(t,1.2,1.4))':s=48000:d=2", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
            .arg(&p).stdin(Stdio::null()).status().unwrap(); assert!(st.success()); }
    p
}
fn with_clip() -> (Session, String, String) {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 360, "height": 640, "fps": 30}));
    let p = bursts();
    let asset = sid(&call(&mut ss, "import_media", json!({"path": p.to_str().unwrap()})), "asset_id");
    let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": asset, "start": 0.0, "duration": 2.0}));
    (ss, t, asset)
}

#[test] fn a_cue_the_recognizer_cannot_make_out_is_flagged_low_confidence() {
    let bin = stub(0.15, "low"); let m = model();
    let (mut ss, t, asset) = with_clip();
    let r = call(&mut ss, "caption_from_words", json!({"track_id": t, "by_asset": {asset.clone(): "completely different words"}, "whisper_bin": bin.to_str().unwrap(), "model": m.to_str().unwrap()}));
    assert_eq!(r["confidence_checked"], true, "{r}");
    let low: Vec<String> = r["low_confidence_cues"].as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect();
    assert!(!low.is_empty(), "a cue the recognizer could only hear at 0.15 confidence must be flagged: {r}");
    assert_eq!(r["cues"][0]["low_confidence"], true, "{r}");
    let cid = low[0].clone();
    assert!(ss.project.find_clip(&cid).unwrap().tags.iter().any(|tg| tg == "low_confidence_caption"), "the clip itself must carry the tag, so a lint_edit run later (even a fresh load) still sees it");
    let lint = call(&mut ss, "lint_edit", json!({}));
    let probs = lint["problems"].as_array().unwrap();
    let hit = probs.iter().find(|p| p["code"] == "caption_low_confidence" && p["clip_id"] == cid).unwrap_or_else(|| panic!("lint_edit must surface caption_low_confidence for {cid}: {lint}"));
    assert_eq!(hit["fix"]["tool"], "edit_caption", "{hit}");
    assert_eq!(hit["fix"]["args"]["clip_id"], cid, "{hit}");
    assert!(lint["checked"].as_array().unwrap().iter().any(|c| c.as_str().unwrap().contains("confidence")), "{lint}");
}

#[test] fn a_confidently_heard_cue_is_not_flagged() {
    let bin = stub(0.95, "high"); let m = model();
    let (mut ss, t, asset) = with_clip();
    let r = call(&mut ss, "caption_from_words", json!({"track_id": t, "by_asset": {asset.clone(): "one two three"}, "whisper_bin": bin.to_str().unwrap(), "model": m.to_str().unwrap()}));
    assert_eq!(r["confidence_checked"], true, "{r}");
    assert!(r["low_confidence_cues"].as_array().unwrap().is_empty(), "{r}");
    let lint = call(&mut ss, "lint_edit", json!({}));
    assert!(!lint["problems"].as_array().unwrap().iter().any(|p| p["code"] == "caption_low_confidence"), "{lint}");
}

#[test] fn check_confidence_false_skips_the_cross_check_entirely() {
    let bin = stub(0.15, "low_off"); let m = model();
    let (mut ss, t, asset) = with_clip();
    let r = call(&mut ss, "caption_from_words", json!({"track_id": t, "by_asset": {asset.clone(): "whatever"}, "whisper_bin": bin.to_str().unwrap(), "model": m.to_str().unwrap(), "check_confidence": false}));
    assert_eq!(r["confidence_checked"], false, "{r}");
    assert!(r["low_confidence_cues"].as_array().unwrap().is_empty(), "{r}");
}

/// caption_from_words has never required a recognizer; adding the cross-check must not change that.
#[test] fn no_recognizer_installed_still_works_exactly_as_before() {
    let (mut ss, t, asset) = with_clip();
    let r = call(&mut ss, "caption_from_words", json!({"track_id": t, "by_asset": {asset.clone(): "one two three"}, "whisper_bin": "/nonexistent/whisper-cli"}));
    assert_eq!(r["confidence_checked"], false, "{r}");
    assert!(r["low_confidence_cues"].as_array().unwrap().is_empty(), "{r}");
    assert!(!r["cues"].as_array().unwrap().is_empty(), "{r}");
}
