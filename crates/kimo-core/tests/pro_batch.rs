//! Batch 12: captions in every format, AAF/OMF, ALE, loudness, remix, track mixer, audio export (5.1), three-point
//! editing, timecode, auto-save and recovery, delivery formats. Real media made with ffmpeg; skipped without it.
use kimo_core::commands::{dispatch, Session};
use kimo_core::model::Source;
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn have_ffmpeg() -> bool { std::process::Command::new("ffmpeg").arg("-version").output().is_ok() }
fn dir(tag: &str) -> std::path::PathBuf { let d = std::env::temp_dir().join(format!("kimo-b12-{tag}-{}", std::process::id())); std::fs::create_dir_all(&d).unwrap(); d }
fn ff(args: &[&str]) { assert!(std::process::Command::new("ffmpeg").args(["-v", "error", "-y"]).args(args).status().unwrap().success(), "ffmpeg {args:?}"); }
fn probe(path: &str, entry: &str) -> String { String::from_utf8_lossy(&std::process::Command::new("ffprobe").args(["-v", "error", "-show_entries", entry, "-of", "default=nw=1:nk=1", path]).output().unwrap().stdout).trim().to_string() }
fn session(w: u32, h: u32) -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": w, "height": h, "fps": 25})); s }
fn shot(d: &std::path::Path, secs: u32) -> String { let p = d.join("shot.mp4").to_string_lossy().to_string(); ff(&["-f", "lavfi", "-i", &format!("testsrc=size=320x180:rate=25:duration={secs}"), "-f", "lavfi", "-i", &format!("sine=frequency=440:sample_rate=48000:duration={secs}"), "-shortest", "-pix_fmt", "yuv420p", &p]); p }
fn media_clips(s: &Session) -> Vec<(f64, f64, f64, bool)> { let mut v: Vec<_> = s.project.tracks.iter().flat_map(|t| t.clips.iter()).filter(|c| matches!(c.source, Source::Media { .. })).map(|c| (c.start, c.duration, c.in_point, c.transition_in.is_some())).collect(); v.sort_by(|a, b| a.partial_cmp(b).unwrap()); v }

#[test]
fn captions_round_trip_through_every_format() {
    let d = dir("cap"); let srt = d.join("in.srt"); std::fs::write(&srt, "1\n00:00:01,000 --> 00:00:02,500\nHello there\n\n2\n00:00:03,000 --> 00:00:04,000\nSecond line\n").unwrap();
    let mut s = session(640, 360); let r = c(&mut s, "import_captions", json!({"path": srt.to_string_lossy()})); assert_eq!(r["cues"], 2, "{r}"); let tid = r["track_id"].as_str().unwrap().to_string();
    for f in ["vtt", "scc", "mcc", "ttml", "stl", "ass", "srt"] {
        let out = d.join(format!("out.{f}")).to_string_lossy().to_string(); let e = c(&mut s, "export_captions_as", json!({"path": out, "track_id": tid, "overwrite": true})); assert_eq!(e["cues"], 2, "{f}: {e}");
        let mut s2 = session(640, 360); let i = c(&mut s2, "import_captions", json!({"path": out})); assert_eq!(i["cues"], 2, "{f} re-import: {i}");
        let texts: Vec<(f64, String)> = s2.project.tracks.iter().flat_map(|t| t.clips.iter()).filter_map(|c| match &c.source { Source::Text { text, .. } => Some((c.start, text.clone())), _ => None }).collect();
        assert!(texts.iter().any(|(t, x)| (t - 1.0).abs() < 0.07 && x.to_lowercase().contains("hello")), "{f}: first cue back near 1 s: {texts:?}");
    }
}

#[test]
fn aaf_and_omf_round_trip_and_ale_logs_the_clips() {
    if !have_ffmpeg() { return; } let d = dir("aaf"); let m = shot(&d, 6);
    let mut s = session(320, 180); let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let a = c(&mut s, "import_media", json!({"path": m}))["asset_id"].clone();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0.0, "duration": 2.0, "in_point": 1.0}));
    let b = c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 2.0, "duration": 1.6, "in_point": 3.2}))["clip_id"].as_str().unwrap().to_string();
    c(&mut s, "add_transition", json!({"clip_id": b, "kind": "crossfade", "duration": 0.4})); c(&mut s, "add_marker", json!({"time": 1.0, "name": "Scene 1"}));
    let want = media_clips(&s);
    let aaf = d.join("edit.aaf").to_string_lossy().to_string(); let e = c(&mut s, "export_aaf", json!({"path": aaf})); assert!(e["picture_tracks"].as_u64().unwrap() >= 1 && e["sound_tracks"].as_u64().unwrap() >= 1, "{e}");
    let mut s2 = session(320, 180); let i = c(&mut s2, "import_aaf", json!({"path": aaf})); let got = media_clips(&s2);
    assert_eq!(got.len(), 2, "both picture clips (their sound plays with them): {got:?} {i}");
    for (g, w) in got.iter().zip(&want) { assert!((g.0 - w.0).abs() < 0.05 && (g.1 - w.1).abs() < 0.05 && (g.2 - w.2).abs() < 0.05, "aaf: {g:?} vs {w:?} {i}"); }
    assert!(got[1].3, "the dissolve comes back: {i}"); assert!(s2.project.markers.iter().any(|m| m.name == "Scene 1"), "marker: {i}");
    let omf = d.join("edit.omf").to_string_lossy().to_string();
    let o = c(&mut s, "export_aaf", json!({"path": omf})); assert_eq!(o["picture_tracks"], 0, "OMF is sound only: {o}");
    let mut s3 = session(320, 180); let oi = c(&mut s3, "import_aaf", json!({"path": omf})); assert!(oi["clips"].as_u64().unwrap() >= 2, "sound clips come back from OMF: {oi}");
    let ale = d.join("log.ale").to_string_lossy().to_string(); let l = c(&mut s, "export_ale", json!({"path": ale})); assert_eq!(l["clips"], 2, "{l}");
    let text = std::fs::read_to_string(&ale).unwrap(); assert!(text.contains("FIELD_DELIM") && text.contains("Column") && text.contains("Data"), "{text}");
}

#[test]
fn loudness_remix_mixer_and_audio_export() {
    if !have_ffmpeg() { return; } let d = dir("aud");
    let tone = d.join("tone.wav").to_string_lossy().to_string(); ff(&["-f", "lavfi", "-i", "sine=frequency=1000:sample_rate=48000:duration=4", "-ac", "2", "-af", "volume=0.25", &tone]);
    let mut s = session(320, 180); let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let a = c(&mut s, "import_media", json!({"path": tone}))["asset_id"].clone();
    c(&mut s, "add_audio_clip", json!({"asset_id": a, "track_id": t, "start": 0.0, "duration": 4.0}));
    let r = c(&mut s, "loudness_report", json!({"target": "podcast"})); let i0 = r["integrated_lufs"].as_f64().unwrap(); assert!(i0 < -6.0 && i0 > -45.0, "{r}"); assert!(r["true_peak_dbtp"].as_f64().unwrap() < 0.0, "{r}");
    // the mixer strip: -6 dB on the track lowers the mix by 6 LU
    c(&mut s, "set_track_mix", json!({"track_id": t, "gain_db": -6.0})); let i1 = c(&mut s, "loudness_report", json!({}))["integrated_lufs"].as_f64().unwrap(); assert!((i0 - i1 - 6.0).abs() < 0.3, "track gain: {i0} -> {i1}");
    c(&mut s, "set_track_mix", json!({"track_id": t, "add_effect": {"kind": "amplify", "params": {"gain": 6.0}}})); let i2 = c(&mut s, "loudness_report", json!({}))["integrated_lufs"].as_f64().unwrap(); assert!((i2 - i0).abs() < 0.3, "track insert effect: {i0} vs {i2}");
    assert!(dispatch(&mut s, "set_track_mix", &json!({"track_id": t, "add_effect": {"kind": "amplify", "params": {"nope": 1}}})).is_err());
    // export normalized to -16 LUFS, and a 5.1 file
    let m4a = d.join("mix.m4a").to_string_lossy().to_string(); let e = c(&mut s, "export_audio_as", json!({"path": m4a, "normalize_lufs": -16.0})); assert!((e["integrated_lufs"].as_f64().unwrap() + 16.0).abs() < 0.6, "{e}");
    let wav = d.join("mix51.wav").to_string_lossy().to_string(); c(&mut s, "export_audio_as", json!({"path": wav, "channels": 6})); assert_eq!(probe(&wav, "stream=channels"), "6");
    // remix a beat track to 7 s
    let beat = d.join("beat.wav").to_string_lossy().to_string(); ff(&["-f", "lavfi", "-i", "aevalsrc=0.6*sin(2*PI*110*t)*exp(-12*mod(t\\,0.5))+0.2*sin(2*PI*660*t)*exp(-30*mod(t+0.25\\,0.5)):s=48000:d=16", "-ac", "2", &beat]);
    let ba = c(&mut s, "import_media", json!({"path": beat}))["asset_id"].clone(); let bc = c(&mut s, "add_audio_clip", json!({"asset_id": ba, "start": 0.0, "duration": 16.0}))["clip_id"].as_str().unwrap().to_string();
    let rm = c(&mut s, "remix_music", json!({"clip_id": bc, "duration": 7.0})); assert_eq!(rm["duration"], 7.0, "{rm}");
    let p = rm["path"].as_str().unwrap(); let secs: f64 = probe(p, "format=duration").parse().unwrap(); assert!((secs - 7.0).abs() < 0.05, "the remix is 7 s long: {secs}");
}

#[test]
fn three_point_editing_and_timecode() {
    if !have_ffmpeg() { return; } let d = dir("edit"); let m = shot(&d, 6);
    let mut s = session(320, 180); let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); let a = c(&mut s, "import_media", json!({"path": m}))["asset_id"].as_str().unwrap().to_string();
    c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0.0, "duration": 3.0}));
    let r = c(&mut s, "insert_edit", json!({"asset_id": a, "source_in": 4.0, "source_out": 5.0, "track_id": t, "at": 1.0}));
    let mut v: Vec<(f64, f64, f64)> = s.project.tracks.iter().flat_map(|t| t.clips.iter()).map(|c| (c.start, c.duration, c.in_point)).collect(); v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    assert_eq!(v.len(), 3, "split, inserted, pushed: {v:?} {r}"); assert!((v[1].0 - 1.0).abs() < 1e-6 && (v[1].2 - 4.0).abs() < 1e-6 && (v[2].0 - 2.0).abs() < 1e-6, "{v:?}");
    c(&mut s, "overwrite_edit", json!({"asset_id": a, "source_in": 0.0, "duration": 0.5, "track_id": t, "at": 0.25}));
    let total: f64 = s.project.tracks.iter().flat_map(|t| t.clips.iter()).map(|c| c.duration).sum(); assert!((total - 4.0).abs() < 1e-6, "overwrite keeps the length: {total}");
    let tc = c(&mut s, "timecode", json!({"frames": 1800, "fps": 29.97})); assert_eq!(tc["timecode"], "00;01;00;02", "{tc}"); assert_eq!(tc["drop_frame"], true);
    let back = c(&mut s, "timecode", json!({"timecode": "00;01;00;02", "fps": 29.97})); assert_eq!(back["frames"], 1800);
    let nd = c(&mut s, "timecode", json!({"seconds": 3661.5, "fps": 25})); assert_eq!(nd["timecode"], "01:01:01:13"); // 3661.5 s x 25 = 91537.5 frames, rounds to frame 13
}

#[test]
fn autosave_writes_rolling_files_and_recovers() {
    let d = dir("auto"); let mut s = session(320, 180);
    c(&mut s, "set_autosave", json!({"enabled": true, "interval_s": 0, "keep": 3, "dir": d.to_string_lossy()}));
    for i in 0..5 { let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string(); c(&mut s, "add_shape", json!({"track_id": t, "shape": "rect", "start": i as f64, "duration": 1.0})); std::thread::sleep(std::time::Duration::from_millis(5)); }
    let l = c(&mut s, "list_autosaves", json!({})); assert_eq!(l["autosaves"].as_array().unwrap().len(), 3, "rolling, keep 3: {l}");
    let mut s2 = Session::new(); c(&mut s2, "set_autosave", json!({"dir": d.to_string_lossy()})); c(&mut s2, "recover_autosave", json!({}));
    assert_eq!(s2.project.tracks.iter().map(|t| t.clips.len()).sum::<usize>(), 5, "the newest autosave has all five clips");
    c(&mut s, "set_autosave", json!({"enabled": false}));
}

#[test]
fn delivery_formats_write_what_they_say() {
    if !have_ffmpeg() { return; } let d = dir("fmt");
    let mut s = session(512, 288); let t = c(&mut s, "add_track", json!({}))["track_id"].as_str().unwrap().to_string();
    c(&mut s, "add_shape", json!({"track_id": t, "shape": "ellipse", "width": 200, "height": 120, "fill": [0.9, 0.4, 0.1, 1], "start": 0.0, "duration": 0.4}));
    for (f, ext, codec) in [("prores_proxy", "mov", "prores"), ("prores_4444xq", "mov", "prores"), ("dnxhr_hqx", "mov", "dnxhd"), ("mxf_dnxhr", "mxf", "dnxhd"), ("webm_alpha", "webm", "vp9"), ("hevc_10bit", "mp4", "hevc")] {
        let out = d.join(format!("{f}.{ext}")).to_string_lossy().to_string(); c(&mut s, "export_format", json!({"path": out, "format": f}));
        assert_eq!(probe(&out, "stream=codec_name").lines().next().unwrap_or(""), codec, "{f}");
    }
    let pat = d.join("seq").join("f_%05d.exr").to_string_lossy().to_string(); let r = c(&mut s, "export_format", json!({"path": pat, "format": "exr_sequence"})); assert_eq!(r["files"], 10, "{r}");
    assert!(d.join("seq").join("f_00000.exr").exists() && d.join("seq").join("f_00009.exr").exists(), "ten EXR frames");
    let e = dispatch(&mut s, "export_format", &json!({"path": d.join("x.mov").to_string_lossy(), "format": "exr_sequence"})).unwrap_err(); assert!(e.contains(".exr"), "{e}");
}
