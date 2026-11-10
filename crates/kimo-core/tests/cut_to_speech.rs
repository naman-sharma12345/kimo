//! "auto-cut to ... speech" (FEATURES.md "auto reframe (cover-fit decode), scene edit detection, auto-cut to
//! beats / to speech" -- auto_reframe/detect_cuts/scene_split and cut_to_beat ["to beats"] already shipped
//! pre-v7; this is the new "to speech" sibling, cut_to_speech).
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_cut_to_speech_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ffmpeg(args: &[&str]) { let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stdin(Stdio::null()).status().expect("ffmpeg"); assert!(st.success(), "ffmpeg {args:?}"); }
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn err(s: &mut Session, n: &str, a: Value) -> String { dispatch(s, n, &a).unwrap_err() }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }

/// A voice-like tone (harmonics + syllable-rate wobble) in the given windows over a quiet noise floor, same
/// recipe as director_small.rs's `talk()`, so audio_speech_intervals' VAD hears distinct speech there and
/// silence everywhere else.
fn talk(name: &str, dur: f64, windows: &[(f64, f64)]) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    let gate: Vec<String> = windows.iter().map(|(s, e)| format!("between(t,{s},{e})")).collect();
    let a = format!("aevalsrc='({})*0.5*(0.25*sin(2*PI*140*t)+0.15*sin(2*PI*280*t+1)+0.1*sin(2*PI*420*t+2)+0.05*sin(2*PI*1900*t))*(0.55+0.45*sin(2*PI*4.3*t))':s=48000:d={dur}", gate.join("+"));
    ffmpeg(&["-f", "lavfi", "-i", &a, "-f", "lavfi", "-i", &format!("anoisesrc=a=0.002:d={dur}:r=48000"),
        "-filter_complex", "[0:a][1:a]amix=inputs=2:normalize=0[a]", "-map", "[a]", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
fn broll(name: &str, dur: f64) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    ffmpeg(&["-f", "lavfi", "-i", &format!("testsrc2=s=360x640:r=30:d={dur}"), "-f", "lavfi", "-i", &format!("anoisesrc=a=0.004:d={dur}:r=48000"),
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
fn portrait() -> Session { let mut ss = Session::new(); c(&mut ss, "project_set", json!({"width": 360, "height": 640, "fps": 30})); ss }

/// 4 well-separated "sentences" (1s each, 1s of silence between): dialogue.wav is 9s long.
fn four_sentences() -> String { talk("ctts-4sent.wav", 9.0, &[(0.5, 1.5), (2.5, 3.5), (4.5, 5.5), (6.5, 7.5)]) }

#[test]
fn places_one_shot_per_speech_interval_back_to_back_with_no_gap_or_overlap() {
    let mut ss = portrait();
    let dia = sid(&c(&mut ss, "import_media", json!({"path": four_sentences()})), "asset_id");
    let shots: Vec<String> = (0..4).map(|i| sid(&c(&mut ss, "import_media", json!({"path": broll(&format!("cts-a{i}.mp4"), 3.0)})), "asset_id")).collect();
    let r = c(&mut ss, "cut_to_speech", json!({"dialogue_asset_id": dia, "asset_ids": shots}));
    let clips = r["clips"].as_array().unwrap().clone();
    assert_eq!(clips.len(), 3, "4 fenceposts -> 3 full shots at phrases_per_shot:1, {r}");
    // back to back: each clip's end equals the next clip's start (no gap, no overlap)
    for w in clips.windows(2) { assert!((w[0]["end"].as_f64().unwrap() - w[1]["start"].as_f64().unwrap()).abs() < 1e-6, "{r}"); }
    let cuts: Vec<f64> = r["cuts_at"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!(cuts.windows(2).all(|w| w[1] > w[0]), "cuts_at must be strictly increasing: {cuts:?}");
    // every shot's own audio is muted; the dialogue plays as its own clip
    let tr = ss.project.tracks.iter().find(|t| t.id == r["video_track_id"].as_str().unwrap()).unwrap();
    assert!(tr.clips.iter().all(|cl| cl.audio.mute), "shot clips must be muted so the dialogue is heard, not the shots");
    let dclip = ss.project.find_clip(r["dialogue_clip_id"].as_str().unwrap()).unwrap();
    assert!(!dclip.audio.mute, "the dialogue clip itself must not be muted");
    assert!((dclip.start - 0.0).abs() < 1e-6);
    assert!((dclip.start + dclip.duration - r["end"].as_f64().unwrap()).abs() < 1e-6, "dialogue must span exactly [start,end)");
}

#[test]
fn phrases_per_shot_groups_multiple_intervals_into_one_shot() {
    let mut ss = portrait();
    let dia = sid(&c(&mut ss, "import_media", json!({"path": four_sentences()})), "asset_id");
    let shots: Vec<String> = (0..4).map(|i| sid(&c(&mut ss, "import_media", json!({"path": broll(&format!("cts-b{i}.mp4"), 4.0)})), "asset_id")).collect();
    // 4 fenceposts, 2 phrases/shot -> (4-1)/2 = 1 full shot only
    let r = c(&mut ss, "cut_to_speech", json!({"dialogue_asset_id": dia, "asset_ids": shots, "phrases_per_shot": 2}));
    assert_eq!(r["clips"].as_array().unwrap().len(), 1, "{r}");
    assert_eq!(r["phrases_used"], 2);
}

#[test]
fn fewer_shots_than_assets_when_assets_outnumber_available_speech_groups() {
    let mut ss = portrait();
    let dia = sid(&c(&mut ss, "import_media", json!({"path": four_sentences()})), "asset_id"); // 4 intervals -> 3 shots max at n=1
    let shots: Vec<String> = (0..10).map(|i| sid(&c(&mut ss, "import_media", json!({"path": broll(&format!("cts-c{i}.mp4"), 3.0)})), "asset_id")).collect();
    let r = c(&mut ss, "cut_to_speech", json!({"dialogue_asset_id": dia, "asset_ids": shots}));
    assert_eq!(r["clips"].as_array().unwrap().len(), 3, "10 shots offered but only 3 full speech-groups exist: {r}");
}

#[test]
fn too_few_speech_intervals_is_a_clear_error_not_a_panic() {
    let mut ss = portrait();
    // one continuous 2s speech block -> exactly 1 interval, but phrases_per_shot:1 needs 2 fenceposts
    let dia = sid(&c(&mut ss, "import_media", json!({"path": talk("ctts-one.wav", 3.0, &[(0.5, 2.5)])})), "asset_id");
    let shots: Vec<String> = vec![sid(&c(&mut ss, "import_media", json!({"path": broll("cts-d0.mp4", 3.0)})), "asset_id")];
    let e = err(&mut ss, "cut_to_speech", json!({"dialogue_asset_id": dia, "asset_ids": shots}));
    assert!(e.contains("speech interval"), "{e}");
}

#[test]
fn unknown_asset_ids_are_rejected_before_any_mutation() {
    let mut ss = portrait();
    let before = serde_json::to_value(&ss.project).unwrap();
    let e = err(&mut ss, "cut_to_speech", json!({"dialogue_asset_id": "nope", "asset_ids": ["also-nope"]}));
    assert!(e.contains("unknown asset"), "{e}");
    assert_eq!(serde_json::to_value(&ss.project).unwrap(), before);
}

#[test]
fn whole_recipe_is_one_undo_step() {
    let mut ss = portrait();
    let dia = sid(&c(&mut ss, "import_media", json!({"path": four_sentences()})), "asset_id");
    let shots: Vec<String> = (0..4).map(|i| sid(&c(&mut ss, "import_media", json!({"path": broll(&format!("cts-e{i}.mp4"), 3.0)})), "asset_id")).collect();
    let before = serde_json::to_value(&ss.project).unwrap();
    let depth = ss.undo.len();
    c(&mut ss, "cut_to_speech", json!({"dialogue_asset_id": dia, "asset_ids": shots}));
    assert_eq!(ss.undo.len(), depth + 1, "cut_to_speech (several sub-calls) must be exactly one undo step");
    c(&mut ss, "undo", json!({}));
    assert_eq!(serde_json::to_value(&ss.project).unwrap(), before, "undo must restore the pre-recipe project byte-exactly");
}
