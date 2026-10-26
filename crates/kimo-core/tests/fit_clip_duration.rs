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


/// A clip with a constant background noise floor across its ENTIRE duration (so even the "pauses" sit
/// around the measured floor, not true digital silence) plus a much louder gated tone during "speech"
/// windows -- the shape real phone-mic room tone has, which is why the old -60 default (tuned for
/// exact_pauses' digital silence above) finds nothing on it. `noise_amp` targets an RMS floor in the
/// -45..-35 dBFS neighbourhood (uniform noise: RMS = amp/sqrt(3)); `random(0)` is ffmpeg's own
/// deterministic per-sample pseudo-random eval function, so the fixture is reproducible.
fn noisy_pauses(name: &str, dur: f64, speech: &[(f64, f64)]) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    let gate: Vec<String> = speech.iter().map(|(s, e)| format!("between(t,{s},{e})")).collect();
    let a = format!("aevalsrc='(2*random(0)-1)*0.0173 + ({})*0.5*sin(2*PI*440*t)':s=48000:d={dur}", gate.join("+"));
    ffmpeg(&["-f", "lavfi", "-i", &format!("testsrc2=s=360x640:r=30:d={dur}"), "-f", "lavfi", "-i", &a,
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}

/// Same 6.0s / two-2.0s-pause shape as two_pause_clip, but with a constant noise floor under everything
/// (see noisy_pauses). Built once per process, like two_pause_clip.
fn two_pause_noisy_clip(ss: &mut Session) -> (String, String) {
    let t = sid(&call(ss, "add_track", json!({})), "track_id");
    let path = noisy_pauses("two_pauses_noisy.mp4", 6.0, &[(0.0, 0.8), (2.8, 3.6), (5.6, 6.0)]);
    let a = sid(&call(ss, "import_media", json!({"path": path})), "asset_id");
    let c = sid(&call(ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 6.0, "fit": "cover"})), "clip_id");
    (t, c)
}

#[test]
fn auto_threshold_finds_pauses_the_old_default_cannot_on_noisy_phone_audio() {
    // the OLD numeric default (-60) must stay UNCHANGED: this clip's noise floor sits well above -60
    // (same shape as real phone-mic room tone), so it still finds NOTHING, exactly like before KF7.
    let mut ss_fixed = portrait();
    let (_t, c1) = two_pause_noisy_clip(&mut ss_fixed);
    let r_fixed = call(&mut ss_fixed, "fit_clip_duration", json!({"clip_id": c1, "max_duration": 4.0, "threshold_db": -60.0}));
    assert_eq!(r_fixed["met_target"], json!(false), "the numeric -60 path must be unchanged: still finds nothing on noisy audio: {r_fixed}");
    assert_eq!(r_fixed["spans_removed"], json!(0), "{r_fixed}");
    assert_eq!(r_fixed["threshold_db"].as_f64().unwrap(), -60.0, "{r_fixed}");
    assert_eq!(r_fixed["measured_noise_floor_db"], Value::Null, "a numeric threshold never measures a floor: {r_fixed}");

    // "auto" measures this SAME clip's own floor and finds the real pauses under it.
    let mut ss_auto = portrait();
    let (_t2, c2) = two_pause_noisy_clip(&mut ss_auto);
    let r_auto = call(&mut ss_auto, "fit_clip_duration", json!({"clip_id": c2, "max_duration": 4.0, "threshold_db": "auto"}));
    assert_eq!(r_auto["met_target"], json!(true), "auto should find the real pauses under the noise floor: {r_auto}");
    assert!(r_auto["spans_removed"].as_u64().unwrap() >= 1, "{r_auto}");
    let floor = r_auto["measured_noise_floor_db"].as_f64().expect("auto must report a measured floor");
    let chosen = r_auto["threshold_db"].as_f64().expect("auto must report the threshold it chose");
    assert!(floor > -55.0 && floor < -25.0, "measured floor should land near the engineered ~-40dB noise: {r_auto}");
    assert!(chosen > floor && chosen < -10.0, "the chosen threshold must sit above the floor and well under the ~-9dB speech tone: {r_auto}");
}

#[test]
fn auto_never_removes_speech() {
    let mut ss = portrait();
    let (_t, c) = two_pause_noisy_clip(&mut ss);
    // 2.0s of "speech" tone total exists in this 6.0s clip (0.8 + 0.8 + 0.4s); asking for something far
    // shorter than the clip minus its real silence cannot be met without cutting speech, so auto must say
    // so honestly (met_target:false) rather than ever trimming into a tone window.
    let r = call(&mut ss, "fit_clip_duration", json!({"clip_id": c, "max_duration": 1.0, "threshold_db": "auto"}));
    assert_eq!(r["met_target"], json!(false), "{r}");
    let new_dur = r["new_duration"].as_f64().unwrap();
    assert!(new_dur > 1.5, "speech must not have been cut to force the target: {r}");
    // "auto" must still report what it measured even on a shortfall path -- proves this ran the real
    // measurement, not just a silent fall-through to the old numeric default.
    assert!(r["measured_noise_floor_db"].as_f64().is_some(), "auto must report a measured floor even when it can't fully meet the target: {r}");
}

#[test]
fn rejects_a_bad_threshold_db_value() {
    let mut ss = portrait();
    let (_t, c) = two_pause_clip(&mut ss);
    for bad in [json!("loud"), json!(true), json!([1, 2]), json!({"x": 1})] {
        let e = dispatch(&mut ss, "fit_clip_duration", &json!({"clip_id": c, "max_duration": 1.0, "threshold_db": bad.clone()})).unwrap_err();
        assert!(e.contains("threshold_db"), "bad value {bad}: {e}");
    }
}

// --- protect_words (lane KF9): never let a candidate silence cut land inside a linked transcript word. ---

/// Binds a single-word transcript take to `clip_id`, mirroring the real clip3 bug this lane fixes: a
/// transcribed word whose timing lands inside what the RMS rule reported as a pause.
fn bind_one_word(ss: &mut Session, clip_id: &str, word_start: f64, word_end: f64) {
    use kimo_core::transcript::{Take, TakeSource, TakeStatus, Transcript, Word};
    let take = Take {
        id: "take1".into(),
        source: TakeSource { asset_id: None, clip_id: Some(clip_id.to_string()), path_key: None },
        src_start: 0.0,
        src_end: 6.0,
        timeline_start: Some(0.0),
        speed: 1.0,
        order: 0,
        status: TakeStatus::Chosen,
        note: String::new(),
    };
    let words = vec![Word::new("w1", "take1", "aur", word_start, word_end, Some(0.9))];
    ss.project.editorial.transcripts.insert(
        "tr1".into(),
        Transcript { id: "tr1".into(), name: "t".into(), language: "en".into(), script_text: None, takes: vec![take], words, version: 1 },
    );
}

/// This clip's own `source.asset` id, read directly off the project (before any cut), so surviving
/// fragments of the SAME media after a ripple-delete can be found back by that id.
fn clip_source_asset(ss: &Session, clip_id: &str) -> String {
    let p = serde_json::to_value(&ss.project).unwrap();
    for t in p["tracks"].as_array().unwrap() {
        for c in t["clips"].as_array().unwrap() {
            if c["id"] == json!(clip_id) {
                return c["source"]["asset"].as_str().unwrap_or_else(|| panic!("clip {clip_id} has no source.asset: {c}")).to_string();
            }
        }
    }
    panic!("clip {clip_id} not found");
}

/// Every surviving clip fragment's own source-time window `[in_point, in_point + duration*speed)` for a
/// given asset id -- lets a test check whether a particular source-time span (e.g. a transcript word's
/// own span) is still intact somewhere on the timeline after a ripple-delete, without assuming which
/// fragment id it ended up on.
fn source_windows(ss: &Session, asset_id: &str) -> Vec<(f64, f64)> {
    let p = serde_json::to_value(&ss.project).unwrap();
    p["tracks"].as_array().unwrap().iter()
        .flat_map(|t| t["clips"].as_array().unwrap().iter().cloned())
        .filter(|c| c["source"]["asset"] == json!(asset_id))
        .map(|c| {
            let ip = c["in_point"].as_f64().unwrap();
            let dur = c["duration"].as_f64().unwrap();
            let speed = c["speed"].as_f64().unwrap_or(1.0);
            (ip, ip + dur * speed)
        })
        .collect()
}

/// True when some single surviving fragment's own window fully contains `span` -- i.e. nothing cut
/// through it.
fn span_is_intact(windows: &[(f64, f64)], span: (f64, f64)) -> bool {
    windows.iter().any(|&(ws, we)| ws <= span.0 + 1e-6 && we >= span.1 - 1e-6)
}

#[test]
fn protect_words_false_cuts_straight_through_a_linked_word_unchanged_from_today() {
    // word "aur" at [1.5, 1.6] sits well inside the first pause's own usable range (~[0.85, 2.75] after
    // the default 0.05 safety margin) -- the same shape as the real clip3 bug this lane exists to fix.
    let mut ss = portrait();
    let (_t, c) = two_pause_clip(&mut ss);
    let asset = clip_source_asset(&ss, &c);
    bind_one_word(&mut ss, &c, 1.5, 1.6);
    let r = call(&mut ss, "fit_clip_duration", json!({"clip_id": c, "max_duration": 4.0}));
    assert_eq!(r["met_target"], json!(true), "{r}");
    assert_eq!(r["words_protected"], json!(0), "protect_words defaults to off: {r}");
    let windows = source_windows(&ss, &asset);
    assert!(!span_is_intact(&windows, (1.5, 1.6)), "today's (pre-protect_words) behaviour must be unchanged: the cut still swallows the word: {r} windows={windows:?}");
}

#[test]
fn protect_words_true_shrinks_the_cut_so_the_same_word_survives_and_still_meets_target() {
    let mut ss = portrait();
    let (_t, c) = two_pause_clip(&mut ss);
    let asset = clip_source_asset(&ss, &c);
    bind_one_word(&mut ss, &c, 1.5, 1.6);
    let r = call(&mut ss, "fit_clip_duration", json!({"clip_id": c, "max_duration": 4.0, "protect_words": true}));
    assert_eq!(r["protect_words"], json!(true), "{r}");
    assert!(r["words_protected"].as_u64().unwrap() >= 1, "the overlapping pause should be reported shrunk or dropped: {r}");
    assert_eq!(r["met_target"], json!(true), "the other whole pause plus the shrunk remainder is still enough: {r}");
    let new_dur = r["new_duration"].as_f64().unwrap();
    assert!((new_dur - 4.0).abs() < 0.15, "{r}");
    let windows = source_windows(&ss, &asset);
    assert!(span_is_intact(&windows, (1.5, 1.6)), "protect_words must keep the whole word intact: {r} windows={windows:?}");
}

#[test]
fn protect_words_true_with_no_linked_words_behaves_exactly_like_today_and_says_so() {
    let mut ss_plain = portrait();
    let (_t1, c1) = two_pause_clip(&mut ss_plain);
    let r_plain = call(&mut ss_plain, "fit_clip_duration", json!({"clip_id": c1, "max_duration": 4.0}));

    let mut ss_pw = portrait();
    let (_t2, c2) = two_pause_clip(&mut ss_pw);
    // no bind_one_word call: this clip has no linked transcript at all.
    let r_pw = call(&mut ss_pw, "fit_clip_duration", json!({"clip_id": c2, "max_duration": 4.0, "protect_words": true}));

    assert_eq!(r_pw["met_target"], r_plain["met_target"], "{r_pw} vs {r_plain}");
    assert_eq!(r_pw["spans_removed"], r_plain["spans_removed"], "{r_pw} vs {r_plain}");
    assert_eq!(r_pw["removed_seconds"].as_f64(), r_plain["removed_seconds"].as_f64(), "{r_pw} vs {r_plain}");
    assert_eq!(r_pw["new_duration"].as_f64(), r_plain["new_duration"].as_f64(), "{r_pw} vs {r_plain}");
    assert_eq!(r_pw["words_protected"], json!(0), "{r_pw}");
    assert!(r_pw["note"].as_str().is_some(), "a protect_words:true run with no linked words should say so: {r_pw}");
}

#[test]
fn rejects_a_bad_protect_words_value() {
    let mut ss = portrait();
    let (_t, c) = two_pause_clip(&mut ss);
    for bad in [json!("yes"), json!(1), json!([true])] {
        let e = dispatch(&mut ss, "fit_clip_duration", &json!({"clip_id": c, "max_duration": 1.0, "protect_words": bad.clone()})).unwrap_err();
        assert!(e.contains("protect_words"), "bad value {bad}: {e}");
    }
}
