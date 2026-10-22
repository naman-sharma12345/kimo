//! beat_cut (cut on the beat, Premiere "Automate to Sequence" at markers / CapCut beat sync): clips laid end to end onto the beats
//! of a music track, one cut per beat (or every Nth), each source picking up where it left off, with an optional zoom bump on
//! every cut. Read off flat colour sources and a frame-numbered ramp video (160x90 @ 10 fps), and a synthetic 120 bpm click track.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 90;
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
fn session(fps: f64) -> (Session, String) { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": fps})); let t = id(&c(&mut s, "add_track", json!({})), "track_id"); (s, t) }
fn solid(s: &mut Session, t: &str, col: [f64; 3], start: f64, dur: f64) -> String { id(&c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": col, "start": start, "duration": dur})), "clip_id") }
fn rgb(s: &mut Session, t: &str) -> Vec<String> { vec![solid(s, t, [1.0, 0.0, 0.0], 0.0, 2.0), solid(s, t, [0.0, 1.0, 0.0], 2.0, 2.0), solid(s, t, [0.0, 0.0, 1.0], 4.0, 2.0)] }
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
fn px(f: &Frame, x: usize, y: usize) -> [f32; 4] { let i = (y * f.w + x) * 4; [f.d[i], f.d[i + 1], f.d[i + 2], f.d[i + 3]] }
fn colour(s: &Session, t: f64) -> &'static str { let p = px(&frame(s, t), 80, 45); if p[3] < 0.5 || (p[0] < 0.1 && p[1] < 0.1 && p[2] < 0.1) { "none" } else if p[0] > 0.8 && p[1] < 0.2 { "red" } else if p[1] > 0.8 && p[0] < 0.2 { "green" } else if p[2] > 0.8 && p[0] < 0.2 { "blue" } else { "other" } }
fn starts(r: &Value) -> Vec<f64> { r["cuts"].as_array().unwrap().iter().map(|x| x["start"].as_f64().unwrap()).collect() }

#[test]
fn a_clips_are_cut_onto_the_beats_in_turn() {
    let (mut s, t) = session(10.0); let src = rgb(&mut s, &t);
    let r = dispatch(&mut s, "beat_cut", &json!({"clip_ids": src, "beats": [0.5, 1.0, 1.5, 2.0, 2.5]}));
    assert!(r.is_ok(), "beat_cut must exist: {}", r.unwrap_err()); let r = r.unwrap();
    assert_eq!(starts(&r), vec![0.5, 1.0, 1.5, 2.0], "one cut on every beat, the last beat ends the montage");
    for (tt, want) in [(0.25, "none"), (0.75, "red"), (1.25, "green"), (1.75, "blue"), (2.25, "red"), (2.75, "none"), (4.5, "none")] { assert_eq!(colour(&s, tt), want, "t {tt}: the source on this beat"); }
    // the source clips were used up (replaced by the montage on their track)
    for id in &src { assert!(s.project.find_clip(id).is_none(), "source {id} is replaced by the cut-up montage"); }
    assert_eq!(s.project.tracks.iter().find(|x| x.id == t).unwrap().clips.len(), 4, "four cuts on the source track");
}

#[test]
fn b_every_nth_beat_and_bpm_grids() {
    let (mut s, t) = session(10.0); let src = rgb(&mut s, &t);
    let r = c(&mut s, "beat_cut", json!({"clip_ids": src, "beats": [0.5, 1.0, 1.5, 2.0, 2.5], "every": 2}));
    assert_eq!(starts(&r), vec![0.5, 1.5], "every 2: a cut on every second beat");
    assert_eq!(colour(&s, 1.2), "red"); assert_eq!(colour(&s, 1.7), "green"); assert_eq!(colour(&s, 2.7), "none");
    // a bpm grid: 120 bpm from offset 0.2 until end 2.2 -> cuts at 0.2, 0.7, 1.2, 1.7 (the last runs to end)
    let (mut s2, t2) = session(10.0); let src2 = rgb(&mut s2, &t2);
    let r2 = c(&mut s2, "beat_cut", json!({"clip_ids": src2, "bpm": 120, "offset": 0.2, "end": 2.2}));
    assert_eq!(starts(&r2), vec![0.2, 0.7, 1.2, 1.7]); assert_eq!(colour(&s2, 2.1), "red", "the fourth cut comes round to the first source again"); assert_eq!(colour(&s2, 2.3), "none");
}

#[test]
fn c_each_source_picks_up_where_it_left_off() {
    use std::io::Write;
    let (mut s, t) = session(10.0);
    // a ramp video: frame k (of 20 at 10 fps) is grey k/19
    let path = std::env::temp_dir().join(format!("kimo-beat-{}-ramp.mp4", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{W}x{H}"), "-r", "10", "-i", "-", "-c:v", "libx264", "-qp", "0", "-pix_fmt", "yuv444p", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); for k in 0..20u32 { let g = (k * 255 / 19) as u8; si.write_all(&vec![g; W * H * 3]).unwrap(); } }
    assert!(ch.wait().unwrap().success());
    let a = id(&c(&mut s, "import_media", json!({"path": path.to_str().unwrap()})), "asset_id");
    let ramp = id(&c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0, "duration": 2.0, "in_point": 0})), "clip_id");
    let blue = solid(&mut s, &t, [0.0, 0.0, 1.0], 2.0, 2.0);
    let r = c(&mut s, "beat_cut", json!({"clip_ids": [ramp, blue], "bpm": 120, "offset": 0.0, "end": 2.0}));
    assert_eq!(starts(&r), vec![0.0, 0.5, 1.0, 1.5]);
    let ins: Vec<f64> = r["cuts"].as_array().unwrap().iter().map(|x| x["in_point"].as_f64().unwrap()).collect();
    assert!((ins[2] - 0.5).abs() < 1e-9, "the ramp's second cut continues from 0.5 s of its source, got in_points {ins:?}");
    let g = px(&frame(&s, 1.25), 80, 45)[0]; assert!((0.33..=0.45).contains(&g), "t 1.25 shows ramp source 0.75 s (grey ~7/19), not a restart from 0 (2/19); got {g}");
    let g0 = px(&frame(&s, 0.25), 80, 45)[0]; assert!(g0 < 0.2, "first ramp cut starts at the source's in point, got {g0}");
}

#[test]
fn d_a_zoom_bump_hits_on_every_cut_and_settles() {
    let (mut s, t) = session(10.0); let src = rgb(&mut s, &t);
    let r = c(&mut s, "beat_cut", json!({"clip_ids": src, "beats": [0.5, 1.0, 1.5, 2.0], "bump": 0.1, "bump_decay": 0.2}));
    for cut in r["cuts"].as_array().unwrap() {
        let cid = cut["clip_id"].as_str().unwrap().to_string();
        let v = c(&mut s, "sample_property", json!({"clip_id": cid, "property": "scale", "space": "clip", "times": [0.0, 0.1, 0.2, 0.4]}));
        let v: Vec<f64> = v["values"].as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
        assert!((v[0] - 1.1).abs() < 1e-6, "bump: scale 1.1 on the cut, got {v:?}"); assert!(v[1] < 1.1 && v[1] > 1.0, "bump decaying, got {v:?}");
        assert!((v[2] - 1.0).abs() < 1e-6 && (v[3] - 1.0).abs() < 1e-6, "bump settled after bump_decay, got {v:?}");
    }
    let (mut s2, t2) = session(10.0); let src2 = rgb(&mut s2, &t2);
    let r2 = c(&mut s2, "beat_cut", json!({"clip_ids": src2, "beats": [0.5, 1.0, 1.5]}));
    let cid = r2["cuts"][0]["clip_id"].as_str().unwrap().to_string(); let v = c(&mut s2, "sample_property", json!({"clip_id": cid, "property": "scale", "space": "clip", "times": [0.0]}));
    assert!((v["values"][0].as_f64().unwrap() - 1.0).abs() < 1e-12, "no bump by default");
}

#[test]
fn e_beats_come_from_a_music_clip_on_the_timeline() {
    let (mut s, t) = session(30.0); let src = rgb(&mut s, &t);
    // 6 s of a 120 bpm click track: a 12 ms 1 kHz click every 0.5 s from 0.25 s
    let path = std::env::temp_dir().join(format!("kimo-beat-{}-click.wav", std::process::id()));
    let sr = 44100usize; let n = sr * 6; let mut pcm = vec![0i16; n];
    for k in 0..12 { let t0 = ((0.25 + 0.5 * k as f64) * sr as f64) as usize; for i in 0..(sr * 12 / 1000) { if t0 + i < n { let e = (-(i as f64) / (sr as f64 * 0.003)).exp(); pcm[t0 + i] = (e * (2.0 * std::f64::consts::PI * 1000.0 * i as f64 / sr as f64).sin() * 28000.0) as i16; } } }
    let mut wav = Vec::new(); let db = (n * 2) as u32; wav.extend_from_slice(b"RIFF"); wav.extend_from_slice(&(36 + db).to_le_bytes()); wav.extend_from_slice(b"WAVEfmt "); wav.extend_from_slice(&16u32.to_le_bytes()); wav.extend_from_slice(&1u16.to_le_bytes()); wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&(sr as u32).to_le_bytes()); wav.extend_from_slice(&((sr * 2) as u32).to_le_bytes()); wav.extend_from_slice(&2u16.to_le_bytes()); wav.extend_from_slice(&16u16.to_le_bytes()); wav.extend_from_slice(b"data"); wav.extend_from_slice(&db.to_le_bytes()); for v in &pcm { wav.extend_from_slice(&v.to_le_bytes()); }
    std::fs::write(&path, wav).unwrap();
    let a = id(&c(&mut s, "import_media", json!({"path": path.to_str().unwrap()})), "asset_id");
    let music = id(&c(&mut s, "add_audio_clip", json!({"asset_id": a, "start": 0.0, "duration": 6.0})), "clip_id");
    let r = c(&mut s, "beat_cut", json!({"clip_ids": src, "audio_clip_id": music}));
    let st = starts(&r); assert!(st.len() >= 9, "a 6 s 120 bpm track gives about 11 cuts, got {st:?}");
    for x in &st { let ph = (x - 0.25).rem_euclid(0.5); assert!(ph.min(0.5 - ph) <= 0.05, "cut {x} must sit on a click (0.25 + 0.5 k) within 1.5 frames, all {st:?}"); }
    let bpm = r["bpm"].as_f64().unwrap(); assert!((bpm - 120.0).abs() < 3.0, "bpm ~120, got {bpm}");
    assert!(s.project.find_clip(&music).is_some(), "the music clip stays");
}

#[test]
fn f_bad_calls_are_refused_and_change_nothing() {
    let (mut s, t) = session(10.0); let src = rgb(&mut s, &t); let before = serde_json::to_string(&s.project).unwrap();
    for (a, want) in [
        (json!({"clip_ids": src}), "beats"),
        (json!({"clip_ids": src, "beats": [0.5, 1.0], "bpm": 120}), "only one"),
        (json!({"clip_ids": src, "beats": [0.5]}), "at least 2"),
        (json!({"clip_ids": src, "beats": [1.0, 0.5, 2.0]}), "increasing"),
        (json!({"clip_ids": [], "beats": [0.5, 1.0]}), "clip_ids"),
        (json!({"clip_ids": ["nope"], "beats": [0.5, 1.0]}), "no such clip"),
        (json!({"clip_ids": [src[0].clone(), src[0].clone()], "beats": [0.5, 1.0]}), "twice"),
        (json!({"clip_ids": src, "beats": [0.5, 1.0], "every": 0}), "every"),
        (json!({"clip_ids": src, "beats": [0.5, 1.0], "bump": 5}), "bump"),
        (json!({"clip_ids": src, "bpm": 120}), "end"),
        (json!({"clip_ids": src, "beats": [0.5, 1.0], "bumps": 0.1}), "unknown argument 'bumps'"),
        (json!({"clip_ids": src, "beats": [0.5, 1.0], "audio_clip_id": "nope"}), "only one"),
    ] { let e = dispatch(&mut s, "beat_cut", &a).expect_err(&format!("{a} must be refused")); assert!(e.contains(want), "{a}: error '{e}' must mention '{want}'"); }
    assert_eq!(serde_json::to_string(&s.project).unwrap(), before, "refused calls must not change the project");
}
