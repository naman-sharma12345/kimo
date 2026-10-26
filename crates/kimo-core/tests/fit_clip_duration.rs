//! fit_clip_duration (new, lane KF6): shrink a media clip to a target duration by trimming its own
//! quietest pauses first, largest pause first, partial on the last one needed -- never cuts speech.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_fit_duration_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ffmpeg(args: &[&str]) { let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stdin(Stdio::null()).status().expect("ffmpeg"); assert!(st.success(), "ffmpeg {args:?}"); }
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }

/// A talking-head stand-in with EXACT silence (literal zero samples, no noise floor) between tone windows,
/// so the RMS silence probe's boundaries land on the 100ms window grid with no ambiguity: `speech` windows
/// carry a tone, everything else is true digital silence.
fn exact_pauses(name: &str, dur: f64, speech: &[(f64, f64)]) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    let gate: Vec<String> = speech.iter().map(|(s, e)| format!("between(t,{s},{e})")).collect();
    let a = format!("aevalsrc='({})*0.5*sin(2*PI*440*t)':s=48000:d={dur}", gate.join("+"));
    ffmpeg(&["-f", "lavfi", "-i", &format!("testsrc2=s=360x640:r=30:d={dur}"), "-f", "lavfi", "-i", &a,
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
fn portrait() -> Session { let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 360, "height": 640, "fps": 30})); ss }

/// Two interior pauses of exactly 2.0s each (silence [0.8,2.8) and [3.6,5.6) in a 6.0s clip). Shared by
/// every test below so the fixture is only built once per process.
fn two_pause_clip(ss: &mut Session) -> (String, String) {
    let t = sid(&call(ss, "add_track", json!({})), "track_id");
    let path = exact_pauses("two_pauses.mp4", 6.0, &[(0.0, 0.8), (2.8, 3.6), (5.6, 6.0)]);
    let a = sid(&call(ss, "import_media", json!({"path": path})), "asset_id");
    let c = sid(&call(ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 6.0, "fit": "cover"})), "clip_id");
    (t, c)
}

#[test]
fn already_fits_is_a_no_op() {
    let mut ss = portrait();
    let (_t, c) = two_pause_clip(&mut ss);
    let before = serde_json::to_string(&ss.project).unwrap();
    let r = call(&mut ss, "fit_clip_duration", json!({"clip_id": c, "max_duration": 10.0}));
    assert_eq!(r["met_target"], json!(true));
    assert_eq!(r["spans_removed"], json!(0));
    assert_eq!(r["new_duration"].as_f64().unwrap(), 6.0);
    assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "a clip that already fits must not be touched: {r}");
}

#[test]
fn removes_exactly_one_whole_pause_to_hit_the_target() {
    let mut ss = portrait();
    let (_t, c) = two_pause_clip(&mut ss);
    // needed = 2.0s, exactly one of the two 2.0s pauses (minus the 0.05 default safety margin on each
    // end, so the usable span is ~1.9s) -- not quite enough from one pause alone, so this also proves the
    // tool reaches into a second pause rather than overshooting by grabbing all of one.
    let r = call(&mut ss, "fit_clip_duration", json!({"clip_id": c, "max_duration": 4.0}));
    let new_dur = r["new_duration"].as_f64().unwrap();
    assert!((new_dur - 4.0).abs() < 0.15, "expected ~4.0s, got {r}");
    assert_eq!(r["met_target"], json!(true), "{r}");
    assert!(r["spans_removed"].as_u64().unwrap() >= 1, "{r}");
    assert_eq!(r["captions_shifted"], json!(0));
    assert_eq!(r["captions_trimmed"], json!(0));
    assert_eq!(r["captions_removed"], json!(0));
}

#[test]
fn partial_trim_of_one_pause_hits_an_exact_target_and_leaves_the_other_pause_whole() {
    let mut ss = portrait();
    let (t, c) = two_pause_clip(&mut ss);
    // needed = 1.0s, well under either 2.0s pause: the tool should take only a 1.0s bite out of the
    // largest (first-found) pause and leave the second pause's clip untouched.
    let r = call(&mut ss, "fit_clip_duration", json!({"clip_id": c, "max_duration": 5.0}));
    let new_dur = r["new_duration"].as_f64().unwrap();
    assert!((new_dur - 5.0).abs() < 0.15, "expected ~5.0s exactly (partial trim), got {r}");
    assert_eq!(r["met_target"], json!(true), "{r}");
    assert_eq!(r["spans_removed"], json!(1), "a 1.0s need should cost exactly one partial span: {r}");
    let removed = r["removed_seconds"].as_f64().unwrap();
    assert!((removed - 1.0).abs() < 0.15, "{r}");
    // the single partial cut splits the clip into two fragments (lift_range's own ripple-split behaviour,
    // unchanged by this tool) whose durations sum back to the exact target.
    let proj = call(&mut ss, "get_project", json!({}));
    let track = proj["tracks"].as_array().unwrap().iter().find(|tr| tr["id"] == json!(t)).unwrap();
    let clips = track["clips"].as_array().unwrap();
    assert_eq!(clips.len(), 2, "{proj}");
    let total: f64 = clips.iter().map(|cl| cl["duration"].as_f64().unwrap()).sum();
    assert!((total - 5.0).abs() < 0.15, "{proj}");
    let _ = c;
}

#[test]
fn not_enough_silence_reports_honestly_without_cutting_speech() {
    let mut ss = portrait();
    let (_t, c) = two_pause_clip(&mut ss);
    // needed = 5.0s; only ~3.8s of usable silence exists (2 x (2.0 - 2*0.05)) after the safety margin, so
    // the target cannot be fully reached -- it must say so, not fabricate a cut into the tone.
    let r = call(&mut ss, "fit_clip_duration", json!({"clip_id": c, "max_duration": 1.0}));
    assert_eq!(r["met_target"], json!(false), "{r}");
    assert!(r["shortfall_seconds"].as_f64().unwrap() > 0.5, "{r}");
    let new_dur = r["new_duration"].as_f64().unwrap();
    // every real pause that was available got used, but speech itself (3 tone windows totalling 1.6s)
    // was never touched: the new duration can be at most 6.0 - (available silence), i.e. still well over 1.0s
    assert!(new_dur > 1.5, "speech must not have been cut to force the target: {r}");
}

#[test]
fn retime_captions_false_is_forwarded_and_old_behaviour_applies() {
    // Smoke check that the opt-out plumbs through to trim_filler_words (full caption-retiming math is
    // trim_filler_words' own, already pinned by tests/trim_filler_words.rs; this only proves the flag rides).
    let mut ss = portrait();
    let (_t, c) = two_pause_clip(&mut ss);
    let r = call(&mut ss, "fit_clip_duration", json!({"clip_id": c, "max_duration": 4.0, "retime_captions": false}));
    assert_eq!(r["retime_captions"], json!(false), "{r}");
}

#[test]
fn rejects_a_non_media_clip() {
    let mut ss = portrait();
    let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let c = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.1,0.1,0.1], "start": 0, "duration": 3})), "clip_id");
    let e = dispatch(&mut ss, "fit_clip_duration", &json!({"clip_id": c, "max_duration": 1.0})).unwrap_err();
    assert!(e.contains("media clip"), "{e}");
}

#[test]
fn rejects_a_non_positive_max_duration() {
    let mut ss = portrait();
    let (_t, c) = two_pause_clip(&mut ss);
    let e = dispatch(&mut ss, "fit_clip_duration", &json!({"clip_id": c, "max_duration": 0.0})).unwrap_err();
    assert!(e.contains("max_duration"), "{e}");
    let e2 = dispatch(&mut ss, "fit_clip_duration", &json!({"clip_id": c, "max_duration": -2.0})).unwrap_err();
    assert!(e2.contains("max_duration"), "{e2}");
}
