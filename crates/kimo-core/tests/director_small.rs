//! Small-model director prep (2026-10-21): a fixed eval (tools/director_eval) had small models drive kimo over MCP. Each test
//! here is a failure they hit, or a tool that does in one call what they kept getting wrong in twenty.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

fn dir() -> PathBuf { let d = std::env::temp_dir().join(format!("kimo_director_small_{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ffmpeg(args: &[&str]) { let st = Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).stdin(Stdio::null()).status().expect("ffmpeg"); assert!(st.success(), "ffmpeg {args:?}"); }
fn call(ss: &mut Session, name: &str, a: Value) -> Value { dispatch(ss, name, &a).unwrap_or_else(|e| panic!("{name} {a}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }
/// A vertical talking-head stand-in (360x640, 30 fps): a voice-like tone (harmonics, syllable-rate wobble) in the given
/// windows over a quiet noise floor, so the speech detector hears speech there and dead air elsewhere.
fn talk(name: &str, dur: f64, speech: &[(f64, f64)], level: f64) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    let gate: Vec<String> = speech.iter().map(|(s, e)| format!("between(t,{s},{e})")).collect();
    let a = format!("aevalsrc='({})*{level}*(0.25*sin(2*PI*140*t)+0.15*sin(2*PI*280*t+1)+0.1*sin(2*PI*420*t+2)+0.05*sin(2*PI*1900*t))*(0.55+0.45*sin(2*PI*4.3*t))':s=48000:d={dur}", gate.join("+"));
    ffmpeg(&["-f", "lavfi", "-i", &format!("testsrc2=s=360x640:r=30:d={dur}"), "-f", "lavfi", "-i", &a, "-f", "lavfi", "-i", &format!("anoisesrc=a=0.002:d={dur}:r=48000"),
        "-filter_complex", "[1:a][2:a]amix=inputs=2:normalize=0[a]", "-map", "0:v", "-map", "[a]", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
/// b-roll stand-in: a coloured test pattern with room tone only; `grade` is an ffmpeg eq/colour filter to make it dark or cast.
fn broll(name: &str, dur: f64, grade: &str) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    let vf = if grade.is_empty() { "null".to_string() } else { grade.to_string() };
    ffmpeg(&["-f", "lavfi", "-i", &format!("testsrc2=s=360x640:r=30:d={dur}"), "-f", "lavfi", "-i", &format!("anoisesrc=a=0.004:d={dur}:r=48000"), "-vf", &vf,
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
fn portrait() -> Session { let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 360, "height": 640, "fps": 30})); ss }
/// A clip with a video stream and NO audio stream at all (phone b-roll shot muted, or a device/app that strips audio on export).
fn silent_no_audio_track(name: &str, dur: f64) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    ffmpeg(&["-f", "lavfi", "-i", &format!("testsrc2=s=360x640:r=30:d={dur}"), "-an", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}

/// Two models in the eval called contact_sheet with path = their finished export (to "look at it"). The sheet was written over
/// the mp4, and the deliverable became a 0.04 s, 1-frame video. A still writer must never write a video path.
#[test] fn a_still_never_overwrites_a_video_file() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.4, 0.6], "start": 0, "duration": 1}));
    let out = dir().join("export.mp4"); call(&mut ss, "export", json!({"path": out, "crf": 30}));
    let before = std::fs::read(&out).unwrap();
    for (tool, args) in [("contact_sheet", json!({"path": out, "times": [0.2, 0.6]})), ("render_frame", json!({"path": out, "time": 0.5}))] {
        let r = dispatch(&mut ss, tool, &args);
        assert!(std::fs::read(&out).unwrap() == before, "{tool} wrote over {}: the export is gone ({r:?})", out.display());
        let e = r.expect_err(&format!("{tool} with a .mp4 path must be refused"));
        assert!(e.contains(".png") && e.contains("review_export"), "{tool}: the refusal should say to use a .png path and how to look at an export: {e}");
    }
    // an existing video stays safe even under an image name
    let disguised = dir().join("export_copy.png"); std::fs::copy(&out, &disguised).unwrap();
    assert!(dispatch(&mut ss, "contact_sheet", &json!({"path": disguised, "times": [0.2]})).unwrap_err().contains("existing video"));
    assert!(std::fs::read(&disguised).unwrap() == before, "the disguised export was overwritten");
    // a png path still works
    call(&mut ss, "contact_sheet", json!({"path": dir().join("ok.png"), "times": [0.2, 0.6]}));
}

/// review_export looks at the exported file itself (models re-imported the export as a clip to see it, or overwrote it).
#[test] fn review_export_shows_the_exported_file() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk("rv.mp4", 3.0, &[(0.5, 2.5)], 1.0)})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 3, "fit": "cover"}));
    let out = dir().join("rv_export.mp4"); call(&mut ss, "export", json!({"path": out, "crf": 30}));
    let r = call(&mut ss, "review_export", json!({"path": out, "n": 6}));
    assert!((r["duration"].as_f64().unwrap() - 3.0).abs() < 0.1, "{r}");
    assert!(r["size_mb"].as_f64().unwrap() > 0.0 && r["width"] == json!(360) && r["height"] == json!(640), "{r}");
    assert!(r["lufs"].as_f64().is_some(), "the export's loudness: {r}");
    let sheet = r["sheet"].as_str().unwrap(); assert!(std::fs::metadata(sheet).map(|m| m.len() > 1000).unwrap_or(false), "sheet png at {sheet}");
    assert_eq!(ss.media.len(), 1, "the sheet comes back as an inline image");
    assert!(dispatch(&mut ss, "review_export", &json!({"path": dir().join("nope.mp4")})).unwrap_err().contains("export first"));
}

/// get_project after one stabilize was 57,000 characters; models need ids, sources and times. timeline_summary is that, small.
#[test] fn timeline_summary_is_small_and_has_the_ids() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": broll("ts.mp4", 3.0, "")})), "asset_id");
    let c = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 2, "in_point": 0.5, "fit": "cover"})), "clip_id");
    call(&mut ss, "stabilize", json!({"clip_id": c}));
    let t2 = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let x = sid(&call(&mut ss, "add_text", json!({"track_id": t2, "text": "Hello", "start": 0, "duration": 1})), "clip_id");
    let full = serde_json::to_string(&call(&mut ss, "get_project", json!({}))).unwrap().len();
    let r = call(&mut ss, "timeline_summary", json!({})); let s = serde_json::to_string(&r).unwrap();
    assert!(s.len() < 1200, "timeline_summary is {} chars (get_project {full}): {s}", s.len());
    assert!(s.contains(&c) && s.contains(&x) && s.contains("ts.mp4") && s.contains("Hello"), "{s}");
    assert_eq!(r["tracks_bottom_to_top"][0]["clips"][0]["in"], json!(0.5), "{s}");
}

/// remove_silence_cuts split and rippled the clip and returned only {"removed": n}: the new ids had to be looked up.
#[test] fn remove_silence_cuts_returns_the_new_clip_ids() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk("rs.mp4", 5.0, &[(0.5, 1.8), (3.0, 4.4)], 1.0)})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 5, "fit": "cover"}));
    let r = call(&mut ss, "remove_silence_cuts", json!({"track_id": t, "ranges": [[1.9, 2.9]]}));
    let clips = r["clips"].as_array().unwrap_or_else(|| panic!("no clips in {r}"));
    let now: Vec<String> = ss.project.tracks.iter().find(|x| x.id == t).unwrap().clips.iter().map(|c| c.id.clone()).collect();
    let got: Vec<String> = clips.iter().map(|c| c["clip_id"].as_str().unwrap().to_string()).collect();
    assert_eq!(got, now, "{r}"); assert!((clips.last().unwrap()["end"].as_f64().unwrap() - 4.0).abs() < 0.05, "{r}");
}

/// The grouped list hides every action behind group names, and the flat list is 395 tools (about 150k tokens): a small model
/// either cannot find add_clip or cannot afford the list. KIMO_TOOLS=director is the editing short list.
#[test] fn director_profile_is_a_short_editing_list() {
    let prof = kimo_core::toolgroups::parse_profile("director");
    let list = kimo_core::toolgroups::profile_list(&prof, &std::collections::HashSet::new());
    let names: Vec<&str> = list.iter().filter_map(|t| t["name"].as_str()).collect();
    for want in ["import_media", "add_clip", "export", "auto_rough_cut", "place_broll", "caption_from_words", "finish_audio", "auto_grade", "lint_edit", "review_export", "search_tools"] {
        assert!(names.contains(&want), "director profile lacks {want}: {names:?}");
    }
    assert!(names.len() <= 34, "{} tools: {names:?}", names.len());
    let bytes = serde_json::to_string(&list).unwrap().len(); assert!(bytes < 24_000, "director list is {bytes} bytes");
    for t in &list { let d = t["description"].as_str().unwrap(); assert!(d.len() <= 150, "{}: {} chars", t["name"], d.len()); }
    // every listed tool is a real one with its real schema
    for t in list.iter().filter(|t| !["search_tools", "kimo_describe"].contains(&t["name"].as_str().unwrap())) {
        let n = t["name"].as_str().unwrap(); let c = kimo_core::commands::registry().into_iter().find(|c| c.name == n).unwrap_or_else(|| panic!("{n} not registered"));
        assert_eq!(t["inputSchema"], c.schema, "{n}");
    }
}

/// Three of three eval runs exported captions under TikTok's caption bar or titles across its button rail, a crushed shot,
/// or b-roll over a pause, and none of them noticed. lint_edit finds each and hands back the call that fixes it.
#[test] fn lint_edit_finds_the_problems_and_its_fixes_clear_them() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 720, "height": 1280, "fps": 30}));
    let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk("lint.mp4", 6.0, &[(0.3, 2.6), (4.2, 5.8)], 1.0)})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 6, "fit": "cover"}));
    let bt = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let b = sid(&call(&mut ss, "import_media", json!({"path": broll("lint_b.mp4", 3.0, "")})), "asset_id");
    // b-roll laid across the pause (2.6..4.2), and crushed to near black
    let bc = sid(&call(&mut ss, "add_clip", json!({"track_id": bt, "source_type": "media", "asset_id": b, "start": 2.4, "duration": 2.0, "fit": "cover"})), "clip_id");
    call(&mut ss, "set_color_wheels", json!({"clip_id": bc, "gain": [0.05, 0.05, 0.05, 1.0]}));
    // captions at the preset's default place on 9:16 (under TikTok's caption bar)
    let ct = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    call(&mut ss, "add_caption_cues", json!({"track_id": ct, "style": "boxed", "cues": [{"start": 0.3, "end": 2.6, "text": "Three phone clips in"}, {"start": 4.2, "end": 5.8, "text": "one short out"}]}));
    let r = call(&mut ss, "lint_edit", json!({"platform": "tiktok"}));
    let codes: Vec<&str> = r["problems"].as_array().unwrap().iter().filter_map(|p| p["code"].as_str()).collect();
    for want in ["dark_frame", "broll_over_silence", "text_outside_safe_area", "loudness_off"] { assert!(codes.contains(&want), "lint missed {want}: {r}"); }
    // apply each fix exactly as given (skip placeholders the model fills from its brief)
    for p in r["problems"].as_array().unwrap() {
        let mut step = p["fix"].clone();
        while step.is_object() {
            let tool = step["tool"].as_str().unwrap_or(""); let args = step["args"].clone();
            if !tool.is_empty() && !serde_json::to_string(&args).unwrap().contains('<') && tool != "timeline_summary" { call(&mut ss, tool, args); }
            step = if step.get("then").map(|v| v.is_object()).unwrap_or(false) { step["then"].clone() } else if step.get("or").map(|v| v.is_object()).unwrap_or(false) && serde_json::to_string(&step["args"]).unwrap().contains('<') { step["or"].clone() } else { Value::Null };
        }
    }
    let r2 = call(&mut ss, "lint_edit", json!({"platform": "tiktok"}));
    let left: Vec<&str> = r2["problems"].as_array().unwrap().iter().filter_map(|p| p["code"].as_str()).collect();
    for gone in ["dark_frame", "broll_over_silence", "text_outside_safe_area", "loudness_off"] { assert!(!left.contains(&gone), "{gone} still there after its fix: {r2}"); }
}

/// The eval's talking head: models left 1-3 s of dead air between takes (laid whole clips) or cut words off (guessed times).
#[test] fn auto_rough_cut_drops_the_dead_air_and_keeps_every_word() {
    let mut ss = portrait();
    let a1 = sid(&call(&mut ss, "import_media", json!({"path": talk("rc1.mp4", 5.0, &[(1.0, 2.6), (3.6, 4.4)], 1.0)})), "asset_id");
    let a2 = sid(&call(&mut ss, "import_media", json!({"path": talk("rc2.mp4", 4.0, &[(1.5, 3.2)], 1.0)})), "asset_id");
    let r = call(&mut ss, "auto_rough_cut", json!({"asset_ids": [a1, a2]}));
    let t = sid(&r, "track_id");
    // speech total 1.6 + 0.8 + 1.7 = 4.1 s; the cut keeps it with short pads and no gap longer than 0.4 s
    let d = r["duration"].as_f64().unwrap(); assert!(d > 4.1 && d < 5.4, "{r}");
    let sp = call(&mut ss, "audio_speech_intervals", json!({"target": {"track_id": t}, "aggressiveness": 1, "min_silence_ms": 200}));
    let iv: Vec<(f64, f64)> = sp["speech"].as_array().unwrap().iter().map(|x| (x["start"].as_f64().unwrap(), x["end"].as_f64().unwrap())).collect();
    let spoken: f64 = iv.iter().map(|(s, e)| e - s).sum(); assert!(spoken > 3.8, "words were cut: {spoken:.2} s of speech left of 4.1 ({iv:?})");
    let gaps: Vec<f64> = iv.windows(2).map(|w| w[1].0 - w[0].1).collect(); assert!(gaps.iter().all(|g| *g < 0.45), "dead air left: {gaps:?}");
    assert!(iv[0].0 < 0.3, "lead-in of {:.2} s", iv[0].0);
    // a false start is dropped by index, and a re-run rebuilds the same track
    let r2 = call(&mut ss, "auto_rough_cut", json!({"asset_ids": [a1, a2], "track_id": t, "drop": [{"asset_id": a1, "index": 0}]}));
    assert_eq!(r2["clips"].as_array().unwrap().len(), 2, "{r2}");
    assert_eq!(ss.project.tracks.iter().find(|x| x.id == t).unwrap().clips.len(), 2);
    assert!(dispatch(&mut ss, "auto_rough_cut", &json!({"asset_ids": ["nope"]})).unwrap_err().contains("import_media"));
}

/// KF1: auto_rough_cut's "no speech found" error names the asset and offers add_clip as a fallback. An asset with
/// NO AUDIO TRACK AT ALL (muted phone b-roll, or an app that strips audio) hit a different code path and surfaced
/// just "no audio track" -- no asset id, no path, no fallback -- a dead end for a small model restricted to the
/// director tool list, which would have no way to know add_clip exists as the way forward.
#[test] fn auto_rough_cut_names_the_asset_and_a_fallback_when_it_has_no_audio_track() {
    let mut ss = portrait();
    let a = sid(&call(&mut ss, "import_media", json!({"path": silent_no_audio_track("noaudio.mp4", 3.0)})), "asset_id");
    let err = dispatch(&mut ss, "auto_rough_cut", &json!({"asset_ids": [a.clone()]})).unwrap_err();
    assert!(err.contains(&a), "names the asset id so a model restricted to director tools knows which clip failed: {err}");
    assert!(err.contains("add_clip"), "offers the same fallback as the no-speech-found case: {err}");
    assert!(err.contains("audio"), "still says what's actually wrong: {err}");
}

/// Models laid b-roll by guessed times: over pauses (dead picture) or with its own sound left on top of the voice.
#[test] fn place_broll_stays_inside_speech_and_is_muted() {
    let mut ss = portrait();
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk("pb.mp4", 9.0, &[(0.4, 3.4), (3.8, 8.6)], 1.0)})), "asset_id");
    let t = sid(&call(&mut ss, "auto_rough_cut", json!({"asset_ids": [a]})), "track_id");
    let b1 = sid(&call(&mut ss, "import_media", json!({"path": broll("pb1.mp4", 4.0, "")})), "asset_id");
    let b2 = sid(&call(&mut ss, "import_media", json!({"path": broll("pb2.mp4", 4.0, "hue=h=90")})), "asset_id");
    let r = call(&mut ss, "place_broll", json!({"track_id": t, "asset_ids": [b1, b2]}));
    let sp: Vec<(f64, f64)> = r["speech"].as_array().unwrap().iter().map(|p| (p[0].as_f64().unwrap(), p[1].as_f64().unwrap())).collect();
    let clips = r["clips"].as_array().unwrap(); assert_eq!(clips.len(), 2, "{r}");
    for c in clips { let (s, e) = (c["start"].as_f64().unwrap(), c["end"].as_f64().unwrap());
        assert!(sp.iter().any(|(a, b)| s >= a - 0.05 && e <= b + 0.05), "b-roll {s}..{e} is not inside one phrase {sp:?}");
        assert!(e - s >= 0.99 && e - s <= 2.51, "shot length {}", e - s);
        let id = c["clip_id"].as_str().unwrap(); assert!(ss.project.find_clip(id).unwrap().audio.mute, "b-roll {id} sound is on"); }
    assert!(clips[0]["start"].as_f64().unwrap() > 0.5, "b-roll should not cover the opening line: {r}");
}

/// Captions: models typed cue times by hand (off by seconds), and placed them in the platform's caption bar.
#[test] fn caption_from_words_follow_the_speech_and_clear_the_platform_ui() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 720, "height": 1280, "fps": 30}));
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk("cap.mp4", 7.0, &[(0.5, 2.9), (3.9, 6.4)], 1.0)})), "asset_id");
    let t = sid(&call(&mut ss, "auto_rough_cut", json!({"asset_ids": [a]})), "track_id");
    let text = "Three phone clips go in. One finished short comes out.";
    let rc = ss.project.tracks.iter().find(|x| x.id == t).unwrap().clips.iter().map(|c| c.start).collect::<Vec<f64>>();
    let r = call(&mut ss, "caption_from_words", json!({"track_id": t, "text": text, "check_confidence": false}));
    let cues = r["cues"].as_array().unwrap(); assert!(cues.len() >= 2, "{r}");  // two-line cues (default lines 2): one cue per sentence here
    let joined: Vec<String> = cues.iter().map(|c| c["text"].as_str().unwrap().replace('\n', " ")).collect();
    assert_eq!(joined.join(" "), text, "every word, in order");
    // the second sentence starts with the second phrase (not mid-pause or mid-first-phrase). After the rough cut the pause
    // between the phrases is gone, so the phrase starts are the rough cut's clip starts (a detector hears one long phrase).
    assert_eq!(rc.len(), 2, "two phrases, two clips: {rc:?}");
    let s2 = cues.iter().find(|c| c["text"].as_str().unwrap().starts_with("One")).unwrap()["start"].as_f64().unwrap();
    assert!((s2 - rc[1]).abs() < 0.35, "second sentence at {s2}, its phrase at {}", rc[1]);
    for c in cues { let id = c["clip_id"].as_str().unwrap(); let (s, e) = (c["start"].as_f64().unwrap(), c["end"].as_f64().unwrap());
        let b = call(&mut ss, "compute_text_bounds", json!({"clip_id": id, "t": (e - s) / 2.0})); let r4: Vec<f64> = b["canvas_rect"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        // TikTok: caption bar from y 921.6, action rail from x 604.8 (query_safe_margins tiktok)
        assert!(r4[1] + r4[3] <= 921.6 && r4[0] + r4[2] <= 604.8 && r4[0] >= 36.0 && r4[1] >= 128.0, "cue '{}' box {r4:?} is under the TikTok UI", c["text"]); }
    assert_eq!(r["inside_safe_area"], json!(true));
    assert!(dispatch(&mut ss, "caption_from_words", &json!({"track_id": t})).unwrap_err().contains("text"));
}

/// Mixes came out at -19.6 LUFS (music never levelled) or with one speaker 3 dB under the other.
#[test] fn finish_audio_levels_speakers_and_hits_the_target() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk("fa1.mp4", 3.0, &[(0.2, 2.8)], 1.0)})), "asset_id");
    let b = sid(&call(&mut ss, "import_media", json!({"path": talk("fa2.mp4", 3.0, &[(0.2, 2.8)], 0.25)})), "asset_id");
    let c1 = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 3, "fit": "cover"})), "clip_id");
    let c2 = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": b, "start": 3, "duration": 3, "fit": "cover"})), "clip_id");
    let r = call(&mut ss, "finish_audio", json!({}));
    let mix = call(&mut ss, "measure_loudness", json!({}))["integrated_lufs"].as_f64().unwrap(); assert!((mix + 14.0).abs() <= 1.0, "mix {mix} LUFS: {r}");
    // measure_loudness {clip_id} reads the clip before its gain (its documented contract), so add each clip's own gain
    let post = |ss: &mut Session, c: &str| { let l = call(ss, "measure_loudness", json!({"clip_id": c}))["integrated_lufs"].as_f64().unwrap(); l + 20.0 * ss.project.find_clip(c).unwrap().audio.volume.base.log10() };
    let (l1, l2) = (post(&mut ss, &c1), post(&mut ss, &c2));
    assert!((l1 - l2).abs() <= 1.0, "speakers {l1:.1} vs {l2:.1} LUFS (12 dB apart before): {r}");
    assert!(ss.project.find_clip(&c1).unwrap().audio.highpass >= 80.0);
}

/// Grades: the eval's dark and blue shots were left as shot, or "fixed" into grey-green. auto_grade measures and corrects.
#[test] fn auto_grade_lifts_a_dark_cast_shot_and_leaves_a_good_one() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let dark = sid(&call(&mut ss, "import_media", json!({"path": broll("dark.mp4", 2.0, "eq=brightness=-0.28:contrast=0.7,colorbalance=bs=0.25:rs=-0.15")})), "asset_id");
    let good = sid(&call(&mut ss, "import_media", json!({"path": broll("good.mp4", 2.0, "")})), "asset_id");
    let cd = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": dark, "start": 0, "duration": 2, "fit": "cover"})), "clip_id");
    let cg = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": good, "start": 2, "duration": 2, "fit": "cover"})), "clip_id");
    let before = call(&mut ss, "get_scopes", json!({"time": 1.0}));
    let r = call(&mut ss, "auto_grade", json!({}));
    let after = call(&mut ss, "get_scopes", json!({"time": 1.0}));
    let (lb, la) = (before["luma_mean"].as_f64().unwrap(), after["luma_mean"].as_f64().unwrap());
    assert!(la > lb + 0.08 && la > 0.28 && la < 0.6, "dark shot luma {lb:.3} -> {la:.3}: {r}");
    let rgb: Vec<f64> = after["avg_rgb"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
    assert!((rgb[2] - rgb[1]).abs() < 0.06 && (rgb[0] - rgb[1]).abs() < 0.06, "cast left: {rgb:?}");
    assert!(r["graded"].as_array().unwrap().iter().any(|g| g["clip_id"] == json!(cd)), "{r}");
    assert!(r["skipped"].as_array().unwrap().iter().any(|g| g["clip_id"] == json!(cg)), "the balanced shot should be left alone: {r}");
}

/// Music reels: models cut every 1.5 s by arithmetic while the track's first beat was not at 0, so every cut missed.
#[test] fn cut_to_beat_cuts_on_the_music_beats() {
    let mut ss = portrait();
    let m = dir().join("beat.wav"); if !m.exists() {
        // 120 BPM with the first beat at 0.23 s: a click (decaying 1 kHz) on every beat over a soft pad
        ffmpeg(&["-f", "lavfi", "-i", "aevalsrc='0.9*sin(2*PI*1000*t)*exp(-40*mod(t-0.23+10,0.5))*gte(t,0.23)+0.05*sin(2*PI*220*t)':s=48000:d=12", m.to_str().unwrap()]); }
    let ma = sid(&call(&mut ss, "import_media", json!({"path": m})), "asset_id");
    let shots: Vec<String> = (0..3).map(|i| sid(&call(&mut ss, "import_media", json!({"path": broll(&format!("cb{i}.mp4"), 3.0, &format!("hue=h={}", i * 100))})), "asset_id")).collect();
    let r = call(&mut ss, "cut_to_beat", json!({"music_asset_id": ma, "asset_ids": shots, "beats_per_shot": 4}));
    assert!((r["beat_period"].as_f64().unwrap() - 0.5).abs() < 0.02, "{r}");
    let mc = sid(&r, "music_clip_id"); let ip = ss.project.find_clip(&mc).unwrap().in_point;
    // each cut lands on a beat of the music as laid: timeline cut + music in_point = 0.23 + k * 0.5 (one frame)
    for c in r["cuts_at"].as_array().unwrap() { let src = c.as_f64().unwrap() + ip; let k = ((src - 0.23) / 0.5).round(); assert!((src - 0.23 - k * 0.5).abs() <= 1.0 / 30.0 + 0.012, "cut at {c} is {src:.3} s into the music, off the beat grid: {r}"); }
    assert!((r["end"].as_f64().unwrap() - 6.0).abs() < 0.05, "3 shots x 4 beats = 6 s: {r}");
}


// ---- 2026-10-21 real-footage eval: what the vision judge saw in the small models' exports ----

fn gray_pixels(png: &str, w: usize, h: usize) -> Vec<u8> {
    let out = Command::new("ffmpeg").args(["-v", "error", "-i", png, "-f", "rawvideo", "-pix_fmt", "gray", "-"]).stdin(Stdio::null()).output().expect("ffmpeg");
    assert_eq!(out.stdout.len(), w * h, "decode {png}"); out.stdout
}

/// Haiku and Muse put a dissolve on cuts where the clips only touch: the incoming shot faded in over nothing, so the export
/// dipped to black at every cut (the judge: "a fully black frame right after the opening title"). lint_edit must catch it.
#[test] fn a_dissolve_on_a_cut_that_only_touches_is_flagged() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": broll("tx1.mp4", 2.0, "")})), "asset_id");
    let b = sid(&call(&mut ss, "import_media", json!({"path": broll("tx2.mp4", 2.0, "hue=h=90")})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 2, "fit": "cover"}));
    let c2 = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": b, "start": 2, "duration": 2, "fit": "cover"})), "clip_id");
    call(&mut ss, "add_transition", json!({"clip_id": c2, "kind": "crossfade", "duration": 0.5}));
    let mid = call(&mut ss, "get_scopes", json!({"time": 2.05}))["luma_mean"].as_f64().unwrap();
    let r = call(&mut ss, "lint_edit", json!({"platform": "none"}));
    let hit = r["problems"].as_array().unwrap().iter().find(|p| p["code"] == json!("transition_from_black"));
    assert!(hit.is_some_and(|p| p["clip_id"] == json!(c2)), "the cut at 2.0 s dips to luma {mid:.3} and lint says nothing: {r}");
}

/// Muse wrote "My Favourite \u{2728}": the font has no such glyph and the export showed a box. A glyph the font lacks
/// must draw nothing (no box), and lint_edit names it.
#[test] fn a_glyph_the_font_lacks_draws_no_box_and_is_reported() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let c = sid(&call(&mut ss, "add_text", json!({"track_id": t, "text": "\u{1F525}", "start": 0, "duration": 1, "size": 120})), "clip_id");
    let png = dir().join("glyph.png"); let png = png.to_str().unwrap();
    call(&mut ss, "render_frame", json!({"time": 0.5, "path": png}));
    let lit = gray_pixels(png, 360, 640).iter().filter(|v| **v > 60).count();
    assert!(lit < 10, "a missing glyph drew {lit} lit pixels (a box)");
    let r = call(&mut ss, "lint_edit", json!({"platform": "none"}));
    assert!(r["problems"].as_array().unwrap().iter().any(|p| p["code"] == json!("missing_glyph") && p["clip_id"] == json!(c)), "{r}");
}

/// Captions and labels at size 22-28 on a 1280-high frame read as specks on a phone (judge: "captions are tiny").
#[test] fn text_too_small_for_a_phone_is_flagged() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let c = sid(&call(&mut ss, "add_text", json!({"track_id": t, "text": "tiny label", "start": 0, "duration": 1, "size": 11})), "clip_id");
    let ok = sid(&call(&mut ss, "add_text", json!({"track_id": t, "text": "big", "start": 1, "duration": 1, "size": 40})), "clip_id");
    let r = call(&mut ss, "lint_edit", json!({"platform": "none"}));
    let small: Vec<&Value> = r["problems"].as_array().unwrap().iter().filter(|p| p["code"] == json!("text_too_small")).collect();
    assert!(small.iter().any(|p| p["clip_id"] == json!(c)), "size 11 on a 640-high frame not flagged: {r}");
    assert!(!small.iter().any(|p| p["clip_id"] == json!(ok)), "size 40 flagged: {r}");
}


/// Muse added a second title over the first ("MY TWO BOTTLES" under "MY FAVORITE BOTTLES"): the judge read garbage.
#[test] fn two_titles_in_the_same_place_are_flagged() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 720, "height": 1280, "fps": 30}));
    let t1 = sid(&call(&mut ss, "add_track", json!({})), "track_id"); let t2 = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    call(&mut ss, "add_title", json!({"track_id": t1, "style": "impact", "text": "MY TWO BOTTLES", "start": 0, "duration": 2}));
    let second = call(&mut ss, "add_title", json!({"track_id": t2, "style": "impact", "text": "MY FAVORITE BOTTLES", "start": 0, "duration": 2}));
    let r = call(&mut ss, "lint_edit", json!({"platform": "none"}));
    assert!(r["problems"].as_array().unwrap().iter().any(|p| p["code"] == json!("text_overlap") && p["clip_id"] == second["clip_ids"][0]), "{r}");
    let t3 = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    call(&mut ss, "add_text", json!({"track_id": t3, "text": "later", "start": 3, "duration": 1, "size": 48}));
    let r2 = call(&mut ss, "lint_edit", json!({"platform": "none"}));
    assert_eq!(r2["problems"].as_array().unwrap().iter().filter(|p| p["code"] == json!("text_overlap")).count(), 1, "text at a different time is not an overlap: {r2}");
}


/// Muse's captions (one text for the whole track) drifted onto the wrong shots: words of clip 3 showed over clip 2.
/// Given words per source clip, every cue stays inside its own clip's shots.
#[test] fn caption_words_per_clip_stay_on_their_own_shots() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 720, "height": 1280, "fps": 30}));
    let a1 = sid(&call(&mut ss, "import_media", json!({"path": talk("ba1.mp4", 4.0, &[(0.4, 3.4)], 1.0)})), "asset_id");
    let a2 = sid(&call(&mut ss, "import_media", json!({"path": talk("ba2.mp4", 3.0, &[(0.4, 1.2)], 1.0)})), "asset_id");
    let rc = call(&mut ss, "auto_rough_cut", json!({"asset_ids": [a1, a2]})); let t = sid(&rc, "track_id");
    let span = |id: &str| { let cs: Vec<&Value> = rc["clips"].as_array().unwrap().iter().filter(|c| c["asset_id"] == json!(id)).collect();
        (cs.first().unwrap()["start"].as_f64().unwrap(), cs.last().unwrap()["end"].as_f64().unwrap()) };
    let (s1, e1) = span(&a1); let (s2, e2) = span(&a2);
    // a short clip with many words, a long one with few: proportional timing over the whole track would push words across the cut
    let r = call(&mut ss, "caption_from_words", json!({"track_id": t, "by_asset": {a1.clone(): "Short one here", a2.clone(): "and this second clip says a great many quick words"}, "check_confidence": false}));
    for c in r["cues"].as_array().unwrap() { let (s, e) = (c["start"].as_f64().unwrap(), c["end"].as_f64().unwrap()); let txt = c["text"].as_str().unwrap();
        let first_clip = "Short one here".contains(txt.split_whitespace().next().unwrap());
        let (lo, hi) = if first_clip { (s1, e1) } else { (s2, e2) };
        assert!(s >= lo - 0.05 && e <= hi + 0.05, "cue '{txt}' {s:.2}..{e:.2} leaves its clip {lo:.2}..{hi:.2}: {r}"); }
    assert!(dispatch(&mut ss, "caption_from_words", &json!({"track_id": t, "by_asset": {"asset99": "x"}})).unwrap_err().contains("asset99"));
}


/// Judge on Muse D2: "captions chopped into tiny mid-sentence pieces". Lines are balanced: no lone trailing word.
#[test] fn caption_lines_are_balanced_not_a_dangling_word() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 720, "height": 1280, "fps": 30}));
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk("bal.mp4", 6.0, &[(0.3, 5.6)], 1.0)})), "asset_id");
    let t = sid(&call(&mut ss, "auto_rough_cut", json!({"asset_ids": [a]})), "track_id");
    let r = call(&mut ss, "caption_from_words", json!({"track_id": t, "text": "My favorite is the navy one, it never leaks.", "check_confidence": false}));
    for c in r["cues"].as_array().unwrap() { let txt = c["text"].as_str().unwrap();
        assert!(txt.split_whitespace().count() >= 2, "cue '{txt}' is a lone word: {r}"); }
    // no flash cues: six one-word sentences in two seconds of speech still give cues of at least 0.8 s, every word in order
    let b = sid(&call(&mut ss, "import_media", json!({"path": talk("flash.mp4", 3.0, &[(0.3, 2.5)], 1.0)})), "asset_id");
    let t2 = sid(&call(&mut ss, "auto_rough_cut", json!({"asset_ids": [b]})), "track_id");
    let text = "Yes. No. Maybe. Sure. Okay. Fine.";
    let r = call(&mut ss, "caption_from_words", json!({"track_id": t2, "text": text, "check_confidence": false}));
    let cues = r["cues"].as_array().unwrap();
    for c in cues { assert!(c["end"].as_f64().unwrap() - c["start"].as_f64().unwrap() >= 0.79, "flash cue {c}: {r}"); }
    let joined: Vec<String> = cues.iter().map(|c| c["text"].as_str().unwrap().replace('\n', " ")).collect();
    assert_eq!(joined.join(" "), text);
}


/// talk() with a chosen picture: `src` an ffmpeg lavfi video source, `vf` a filter on it (e.g. a blur over a time range).
fn talk_v(name: &str, dur: f64, speech: &[(f64, f64)], src: &str, vf: &str) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    let gate: Vec<String> = speech.iter().map(|(s, e)| format!("between(t,{s},{e})")).collect();
    let a = format!("aevalsrc='({})*(0.25*sin(2*PI*140*t)+0.15*sin(2*PI*280*t+1)+0.1*sin(2*PI*420*t+2)+0.05*sin(2*PI*1900*t))*(0.55+0.45*sin(2*PI*4.3*t))':s=48000:d={dur}", gate.join("+"));
    ffmpeg(&["-f", "lavfi", "-i", &format!("{src}=s=360x640:r=30:d={dur}"), "-f", "lavfi", "-i", &a, "-f", "lavfi", "-i", &format!("anoisesrc=a=0.002:d={dur}:r=48000"),
        "-filter_complex", &format!("[0:v]{vf}[v];[1:a][2:a]amix=inputs=2:normalize=0[a]"), "-map", "[v]", "-map", "[a]", "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}

/// Judge on Muse D2: "motion-blurred, badly framed shots are kept in". The rough cut trims a blurry start of a segment.
#[test] fn rough_cut_trims_a_blurry_start() {
    let mut ss = portrait();
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk_v("blur_lead.mp4", 5.0, &[(0.3, 4.5)], "testsrc2", "boxblur=10:enable='between(t,0,1.0)'")})), "asset_id");
    let r = call(&mut ss, "auto_rough_cut", json!({"asset_ids": [a]}));
    let seg = &r["segments"][0]; let clip = &r["clips"][0];
    assert!(seg["soft_share"].as_f64().unwrap() > 0.1, "the blur is seen: {r}");
    let ip = clip["in_point"].as_f64().unwrap();
    assert!((0.9..=1.3).contains(&ip), "the clip starts after the blur (in {ip}): {r}");
    // flag mode reports and keeps it
    let r2 = call(&mut ss, "auto_rough_cut", json!({"asset_ids": [a], "soft": "flag"}));
    assert!(r2["clips"][0]["in_point"].as_f64().unwrap() < 0.4, "{r2}");
    assert!(dispatch(&mut ss, "auto_rough_cut", &json!({"asset_ids": [a], "soft": "maybe"})).unwrap_err().contains("trim"));
}

/// Judge on Muse D2: "repeated near-identical navy bottle shots drag the middle". Two clips of the same picture: the second
/// reports similar_to, and dedupe drops it; a different picture is not similar.
#[test] fn rough_cut_finds_repeated_shots() {
    let mut ss = portrait();
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk_v("dup_a.mp4", 3.0, &[(0.3, 2.6)], "smptebars", "null")})), "asset_id");
    let b = sid(&call(&mut ss, "import_media", json!({"path": talk_v("dup_b.mp4", 3.0, &[(0.3, 2.6)], "smptebars", "null")})), "asset_id");
    let c = sid(&call(&mut ss, "import_media", json!({"path": talk_v("dup_c.mp4", 3.0, &[(0.3, 2.6)], "testsrc2", "null")})), "asset_id");
    let r = call(&mut ss, "auto_rough_cut", json!({"asset_ids": [a, b, c]}));
    let segs = r["segments"].as_array().unwrap();
    let of = |id: &str| segs.iter().find(|s| s["asset_id"] == json!(id)).unwrap();
    assert_eq!(of(&b)["similar_to"]["asset_id"], json!(a), "{r}");
    assert!(of(&c).get("similar_to").is_none(), "a different picture is not a repeat: {r}");
    assert_eq!(r["clips"].as_array().unwrap().len(), 3, "reported, not dropped, by default");
    let r2 = call(&mut ss, "auto_rough_cut", json!({"asset_ids": [a, b, c], "dedupe": true}));
    let ids: Vec<&str> = r2["clips"].as_array().unwrap().iter().map(|x| x["asset_id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec![a.as_str(), c.as_str()], "{r2}");
}


/// auto_grade run twice stacked two primaries grades (the second compounding the first), and its "reset" of a crushed
/// grade added a neutral grade after the crushing one, which left the shot black. It updates the clip's grade in place.
#[test] fn auto_grade_updates_the_grade_in_place() {
    let mut ss = portrait();
    let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": broll("dark_b.mp4", 3.0, "eq=brightness=-0.35")})), "asset_id");
    let c = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 3, "fit": "cover"})), "clip_id");
    let g1 = call(&mut ss, "auto_grade", json!({"clip_ids": [c]})); let g2 = call(&mut ss, "auto_grade", json!({"clip_ids": [c]}));
    assert_eq!(g1["graded"][0]["gain"], g2["graded"][0]["gain"], "the same grade both times: {g1} {g2}");
    let n = ss.project.tracks.iter().flat_map(|t| t.clips.iter()).find(|x| x.id == c).unwrap().effects.iter().filter(|e| e.kind == "primaries").count();
    assert_eq!(n, 1, "one primaries grade, not a stack");
}

/// Perf (KP1): lint_edit's picture check samples a timeline every 0.5s via get_scopes, and get_scopes' own
/// single-instant prepare_at window (24 frames) is far smaller than the 0.5s gap between samples once a clip
/// runs more than a second or two - nearly every sample needs a fresh decode. kimo's media decode always
/// re-seeks to a clip's own start and discards every frame before the one it actually wants (bounded memory,
/// not bounded CPU), so each sample's decode cost grows with how far into the clip it sits - quadratic in a
/// long clip's length at this sampling cadence. lint_edit now pre-warms the same decode cache with a much
/// bigger (export-sized) window before each sample, so most of a clip lands from one or two real decodes
/// instead of one per sample - same decode() calls, same project, same time, so get_scopes' numbers (and
/// lint_edit's findings) are unchanged by construction; only how many of those decodes are real drops. A 60s
/// single clip with a real dark patch right at the end pins both: the exact same problem is still found, at
/// the exact same instant, and the scan no longer grows the way the old per-sample window did.
#[test] fn lint_edit_scans_a_long_clip_without_rescanning_from_its_start_each_sample() {
    let p = dir().join("kp1_long.mp4");
    if !p.exists() {
        ffmpeg(&["-f", "lavfi", "-i", "testsrc2=s=360x640:r=30:d=60", "-f", "lavfi", "-i", "color=c=black:s=360x640:r=30:d=60",
            "-filter_complex", "[0:v][1:v]overlay=enable='between(t,58,59.5)'[v]", "-map", "[v]",
            "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", p.to_str().unwrap()]);
    }
    let mut ss = portrait();
    let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": p.to_str().unwrap()})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 60, "fit": "cover"}));
    let t0 = std::time::Instant::now();
    let r = call(&mut ss, "lint_edit", json!({"platform": "none"}));
    let elapsed = t0.elapsed();
    let dark: Vec<&Value> = r["problems"].as_array().unwrap().iter().filter(|p| p["code"] == "dark_frame").collect();
    assert_eq!(dark.len(), 1, "exactly one dark patch, found: {r}");
    let at = dark[0]["at"].as_f64().unwrap();
    assert!((at - 58.25).abs() < 0.05, "dark_frame should land at 58.25s (the patch's first sampled instant), got {at}: {r}");
    // Equivalence pin: re-run on the same, unmutated project - the finding must be exactly reproducible.
    let r2 = call(&mut ss, "lint_edit", json!({"platform": "none"}));
    assert_eq!(r, r2, "lint_edit must be deterministic on an unchanged project");
    // Timing: measured on this exact fixture on this box, the pre-fix scan (always re-seeking to each clip's
    // own start per sample) took ~12.5s; the fix takes ~4.2s. 9s is a generous ceiling - comfortably above
    // the fix's real time, comfortably below the old quadratic cost - to catch a real regression, not CI noise.
    // That 3x margin is a --release number only. This test is dominated by real ffmpeg subprocess decodes,
    // whose cost barely depends on whether kimo-core itself was compiled with optimizations, so the debug
    // build's margin is far narrower. KW2 (2026-10-24) measured both sides directly in a plain debug build
    // on this box: this fixed code takes 12.7-13.2s (7 runs); the original quadratic rescan (the code from
    // before 98eed5d, rebuilt in a separate debug worktree at 77e14e2 with this same test) takes 19.0-20.5s
    // (5 runs). A flat "~3x optimized" guess (27s+) would sit above the real regression and silently stop
    // catching it, so the debug ceiling below is picked from the measured gap instead, not a multiplier:
    // comfortably above 13.2s, comfortably below 19.0s.
    let ceiling = if cfg!(debug_assertions) { 18.0 } else { 9.0 };
    assert!(elapsed.as_secs_f64() < ceiling, "lint_edit took {:.1}s on a 60s single clip (ceiling {:.1}s for this build) - the per-sample decode window regressed", elapsed.as_secs_f64(), ceiling);
}

/// KR2: lint_edit's picture-scan loop used to pre-warm the decode cache itself immediately before
/// every get_scopes call (KP1/KQ1), then get_scopes called the exact same function again for the
/// exact same instant - after KQ1 (fb6ddbf) made get_scopes' own call site use that same function,
/// the pre-warm became a byte-for-byte duplicate. This pins the duplicate's removal directly (not
/// just via timing, which is sensitive to box load): render::prepare_analysis_range must run exactly
/// once per sampled instant, not twice. thread-local counter: see its own doc comment for why this
/// is safe next to other tests in this binary running concurrently.
#[test] fn lint_edit_prepares_each_picture_scan_sample_exactly_once() {
    let mut ss = portrait();
    let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": broll("kr2_prep.mp4", 3.0, "")})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 3, "fit": "cover"}));
    let before = kimo_core::render::analysis_prepare_call_count();
    let r = call(&mut ss, "lint_edit", json!({"platform": "none"}));
    let after = kimo_core::render::analysis_prepare_call_count();
    // picture scan samples at t = 0.25, 0.75, ..., < 3.0 -> 6 samples
    let mut expected = 0usize; let mut t = 0.25; while t < 3.0 { expected += 1; t += 0.5; }
    assert_eq!(after - before, expected, "get_scopes' own prepare_analysis_range call is the only preparation that should run per sample now (a duplicate pre-warm call would double this): {r}");
}

/// KR2: lint_edit's dialogue-track finder (director_tools.rs `speech()`) probed each candidate track
/// at aggressiveness 2, then - for the winning track only - called it again at aggressiveness 1 for
/// the real intervals, paying a full ffmpeg-backed audio decode twice for the one track that matters
/// even though nothing about the project changed between the two calls. Session::audio_decode_cache
/// (ext/speech_intervals.rs `decode_memo`) now memoizes the decode by (target, whole-project
/// fingerprint), so the second call is a cache hit. Pinned directly via the decode call counter, not
/// timing (audio decode on a 6s clip is fast enough that a timing ceiling here would mostly measure
/// process noise, not the fix).
#[test] fn lint_edit_decodes_the_dialogue_track_audio_once_not_twice() {
    let mut ss = Session::new(); call(&mut ss, "project_set", json!({"width": 720, "height": 1280, "fps": 30}));
    let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    let a = sid(&call(&mut ss, "import_media", json!({"path": talk("kr2_speech.mp4", 6.0, &[(0.3, 2.6), (3.2, 5.6)], 1.0)})), "asset_id");
    call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 6, "fit": "cover"}));
    let before = kimo_core::ext::speech_intervals::speech_decode_call_count();
    let r = call(&mut ss, "lint_edit", json!({"platform": "none"}));
    let after = kimo_core::ext::speech_intervals::speech_decode_call_count();
    assert_eq!(after - before, 1, "the one dialogue track's audio should decode exactly once (probe + final reuse the same PCM), not once per aggressiveness: {r}");
}


/// KC1 (2026-10-24): landscape/4K phone footage that is pillarboxed (black bars either side of a centered
/// portrait-shaped subject -- common when a landscape capture is uploaded without a re-encode) measured auto_grade's
/// exposure from the SOURCE's raw 64x64 downsample with no fit/crop applied at all. For a pillarboxed source the raw
/// frame is mostly black bars, so auto_grade sees a false "dark" reading, computes the maximum exposure gain (clamped
/// 2.4x), and applies it to the clip -- but the clip's "cover" fit crops the black bars away entirely, so the gain
/// lands on the already-well-exposed visible content and blows it out. auto_grade must measure what the fit/crop
/// transform actually puts on screen, not the untouched source frame.
fn pillarboxed_landscape(name: &str, dur: f64, canvas_w: u32, canvas_h: u32, band_w: u32) -> String {
    let p = dir().join(name); if p.exists() { return p.to_string_lossy().into_owned(); }
    ffmpeg(&["-f", "lavfi", "-i", &format!("color=c=gray:s={band_w}x{canvas_h}:d={dur}"),
        "-f", "lavfi", "-i", &format!("anoisesrc=a=0.004:d={dur}:r=48000"),
        "-vf", &format!("pad={canvas_w}:{canvas_h}:({canvas_w}-{band_w})/2:0:color=black"),
        "-c:v", "libx264", "-preset", "ultrafast", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest", p.to_str().unwrap()]);
    p.to_string_lossy().into_owned()
}
#[test] fn auto_grade_measures_the_fitted_frame_not_the_unfit_source() {
    let mut ss = portrait(); let t = sid(&call(&mut ss, "add_track", json!({})), "track_id");
    // 1280x720 landscape source, "cover" fit into a 360x640 (9:16) project crops to the center ~360/(1280*640/720)=31.6%
    // of the width -- so a centered gray band a bit wider than that (40%) is ALL that's visible after fit; the rest
    // (pure black pillarbox) must never factor into the exposure reading.
    let band_w = (1280.0_f64 * 0.40).round() as u32;
    let src = sid(&call(&mut ss, "import_media", json!({"path": pillarboxed_landscape("pillarbox.mp4", 2.0, 1280, 720, band_w)})), "asset_id");
    let _c = sid(&call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": src, "start": 0, "duration": 2, "fit": "cover"})), "clip_id");
    let before = call(&mut ss, "get_scopes", json!({"time": 1.0}));
    let lb = before["luma_mean"].as_f64().unwrap();
    assert!((0.32..=0.6).contains(&lb), "fixture sanity: the visible (cropped) band should already read as normally exposed, got {lb:.3}");
    let r = call(&mut ss, "auto_grade", json!({}));
    let after = call(&mut ss, "get_scopes", json!({"time": 1.0}));
    let la = after["luma_mean"].as_f64().unwrap();
    assert!(la < 0.75, "auto_grade blew out a clip that was already well exposed on screen: luma {lb:.3} -> {la:.3} ({r}) -- it measured the unfit source (mostly black pillarbox), not what \"cover\" fit actually shows");
}
