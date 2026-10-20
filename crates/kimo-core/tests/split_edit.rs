//! J-cut / L-cut (Premiere "split edit", Resolve "J/L cut"): a media clip's audio can lead its video into the previous shot
//! (J) or run on under the next shot (L) without moving the video edit. set_split_edit takes the INCOMING clip of a cut:
//! J moves that cut's audio `seconds` earlier (incoming audio leads, outgoing audio ends early), L moves it later
//! (outgoing audio lingers, incoming audio starts late). The audio stays in sync with its own source: the sample heard at
//! timeline t is source in_point + (t - start) * speed for both clips, measured here with tones that change at known
//! source times. Video starts, durations and in-points never move; export (mix) and mix_strict both carry it.
use kimo_core::commands::{dispatch, Session};
use serde_json::{json, Value};

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
const SR: usize = 48000;

/// A 4 s source: solid `color` video + mono tone that is `f1` Hz before source second `at` and `f2` Hz after.
fn source(name: &str, color: &str, f1: f64, f2: f64, at: f64) -> String {
    let p = std::env::temp_dir().join(format!("kimo-jl-{name}-{}.mkv", std::process::id()));
    let expr = format!("'0.3*sin(2*PI*if(lt(t,{at}),{f1},{f2})*t)'");
    assert!(std::process::Command::new("ffmpeg").args(["-v","error","-y","-f","lavfi","-i",&format!("color=c={color}:s=64x36:r=25:d=4"),
        "-f","lavfi","-i",&format!("aevalsrc={expr}:s=48000:d=4"),"-c:v","ffv1","-c:a","pcm_s16le","-shortest",p.to_str().unwrap()]).status().unwrap().success());
    p.to_str().unwrap().to_string()
}

/// Goertzel amplitude of `f` Hz on the left channel over timeline seconds [t0, t1).
fn amp(pcm: &[f32], f: f64, t0: f64, t1: f64) -> f64 {
    let (a, b) = ((t0 * SR as f64) as usize, (t1 * SR as f64) as usize);
    let w = 2.0 * std::f64::consts::PI * f / SR as f64; let (mut re, mut im) = (0.0, 0.0);
    for i in a..b { let v = pcm[i * 2] as f64; re += v * (w * i as f64).cos(); im += v * (w * i as f64).sin(); }
    2.0 * (re * re + im * im).sqrt() / (b - a) as f64
}

/// A = red, 220 Hz until source 3.0 s then 330 Hz; B = blue, 660 Hz until source 1.0 s then 990 Hz.
/// Timeline: A at [0,2) from source 1.0 (hears 220), B at [2,4) from source 1.0 (hears 990). Both have 1 s of handle.
fn cut() -> (Session, String, String) {
    let (pa, pb) = (source("a", "red", 220.0, 330.0, 3.0), source("b", "blue", 660.0, 990.0, 1.0));
    let mut s = Session::new();
    c(&mut s, "project_set", json!({"width":64,"height":36,"fps":25}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    let aa = c(&mut s, "import_media", json!({"path":pa}))["asset_id"].clone();
    let ab = c(&mut s, "import_media", json!({"path":pb}))["asset_id"].clone();
    let a = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"media","asset_id":aa,"start":0,"duration":2,"in_point":1}))["clip_id"].as_str().unwrap().to_string();
    let b = c(&mut s, "add_clip", json!({"track_id":t,"source_type":"media","asset_id":ab,"start":2,"duration":2,"in_point":1}))["clip_id"].as_str().unwrap().to_string();
    (s, a, b)
}

/// (lead, tail) of a clip's audio as the project stores it (absent = 0, the straight cut).
fn lt(s: &Session, id: &str) -> (f64, f64) {
    let v = serde_json::to_value(s.project.find_clip(id).unwrap()).unwrap();
    (v["audio"]["lead"].as_f64().unwrap_or(0.0), v["audio"]["tail"].as_f64().unwrap_or(0.0))
}

fn video(s: &Session, id: &str) -> (f64, f64, f64, f64) { let k = s.project.find_clip(id).unwrap(); (k.start, k.duration, k.in_point, k.speed) }

fn red_at(s: &mut Session, t: f64) -> bool {
    kimo_core::render::prepare_at(&s.project, &mut s.cache, Some(t)).unwrap();
    let f = kimo_core::render::render_frame(&s.project, t, &s.cache).unwrap();
    let i = (18 * 64 + 32) * 4; f.d[i] > 0.5 && f.d[i + 2] < 0.2
}

#[test]
fn j_cut_incoming_audio_leads_in_sync_and_video_stays() {
    let (mut s, a, b) = cut();
    let (va, vb) = (video(&s, &a), video(&s, &b));
    let r = c(&mut s, "set_split_edit", json!({"clip_id":b,"kind":"j","seconds":0.5}));
    assert_eq!(r["outgoing"], json!(a)); assert_eq!(r["incoming"], json!(b));
    assert_eq!((video(&s, &a), video(&s, &b)), (va, vb), "video edit must not move");
    for pcm in [kimo_core::audio::mix(&s.project), kimo_core::audio::mix_strict(&s.project).unwrap()] {
        // [0,1.5): A only (220). [1.5,2.0): B's audio from source 0.5..1.0 = 660 Hz, A silent. [2,4): B source 1..3 = 990.
        assert!(amp(&pcm, 220.0, 0.1, 1.45) > 0.15, "A before the J: {}", amp(&pcm, 220.0, 0.1, 1.45));
        assert!(amp(&pcm, 660.0, 0.1, 1.45) < 0.01, "B must not start before 1.5: {}", amp(&pcm, 660.0, 0.1, 1.45));
        assert!(amp(&pcm, 660.0, 1.55, 1.95) > 0.15, "B leads with source 0.5..1.0 (660): {}", amp(&pcm, 660.0, 1.55, 1.95));
        assert!(amp(&pcm, 220.0, 1.55, 1.95) < 0.01, "A's audio ends at the J: {}", amp(&pcm, 220.0, 1.55, 1.95));
        assert!(amp(&pcm, 990.0, 1.55, 1.95) < 0.01, "sync: B's 990 must not appear before timeline 2.0: {}", amp(&pcm, 990.0, 1.55, 1.95));
        assert!(amp(&pcm, 990.0, 2.05, 3.9) > 0.15, "B in sync after the cut: {}", amp(&pcm, 990.0, 2.05, 3.9));
    }
    assert!(red_at(&mut s, 1.75), "video at 1.75 is still A (red)");
    assert!(!red_at(&mut s, 2.25), "video at 2.25 is B");
}

#[test]
fn l_cut_outgoing_audio_runs_on_in_sync() {
    let (mut s, a, b) = cut();
    let (va, vb) = (video(&s, &a), video(&s, &b));
    c(&mut s, "set_split_edit", json!({"clip_id":b,"kind":"l","seconds":0.5}));
    assert_eq!((video(&s, &a), video(&s, &b)), (va, vb), "video edit must not move");
    for pcm in [kimo_core::audio::mix(&s.project), kimo_core::audio::mix_strict(&s.project).unwrap()] {
        // A runs on 2.0..2.5 with source 3.0..3.5 = 330 Hz (in sync); B's audio starts at 2.5 with source 1.5 = 990.
        assert!(amp(&pcm, 220.0, 0.1, 1.9) > 0.15);
        assert!(amp(&pcm, 330.0, 2.05, 2.45) > 0.15, "A lingers in sync (330 after source 3.0): {}", amp(&pcm, 330.0, 2.05, 2.45));
        assert!(amp(&pcm, 220.0, 2.05, 2.45) < 0.01, "sync: A's 220 is source < 3.0, never after timeline 2.0: {}", amp(&pcm, 220.0, 2.05, 2.45));
        assert!(amp(&pcm, 990.0, 2.05, 2.45) < 0.01, "B starts late: {}", amp(&pcm, 990.0, 2.05, 2.45));
        assert!(amp(&pcm, 990.0, 2.55, 3.9) > 0.15);
        assert!(amp(&pcm, 330.0, 2.55, 3.9) < 0.01, "A stops at 2.5: {}", amp(&pcm, 330.0, 2.55, 3.9));
    }
    assert!(red_at(&mut s, 1.9)); assert!(!red_at(&mut s, 2.1), "video cut still at 2.0");
}

#[test]
fn none_restores_the_straight_cut_bit_identically_and_old_json_has_no_new_keys() {
    let (mut s, _a, b) = cut();
    let before = kimo_core::audio::mix(&s.project);
    let json0 = serde_json::to_string(&s.project).unwrap();
    assert!(!json0.contains("\"lead\"") && !json0.contains("\"tail\""), "a straight cut serialises as before");
    c(&mut s, "set_split_edit", json!({"clip_id":b,"kind":"j","seconds":0.5}));
    let j = serde_json::to_value(&s.project).unwrap();
    let mut s2 = Session::new(); s2.project = serde_json::from_value(j).unwrap();
    assert_eq!(lt(&s2, &b), (0.5, 0.0), "round-trips through save/load");
    assert_eq!(kimo_core::audio::mix(&s2.project), kimo_core::audio::mix(&s.project));
    c(&mut s, "set_split_edit", json!({"clip_id":b,"kind":"none"}));
    assert_eq!(serde_json::to_string(&s.project).unwrap(), json0);
    assert_eq!(kimo_core::audio::mix(&s.project), before);
}

#[test]
fn split_clip_keeps_the_lead_on_the_left_piece_and_the_tail_on_the_right() {
    let (mut s, a, b) = cut();
    c(&mut s, "set_split_edit", json!({"clip_id":b,"kind":"j","seconds":0.5}));
    let before = kimo_core::audio::mix(&s.project);
    let r = c(&mut s, "split_clip", json!({"clip_id":b,"at":3.0}));
    let right = r["right_clip_id"].as_str().unwrap().to_string();
    assert_eq!((lt(&s, &b), lt(&s, &right)), ((0.5, 0.0), (0.0, 0.0)));
    let r = c(&mut s, "split_clip", json!({"clip_id":a,"at":1.0}));
    let a2 = r["right_clip_id"].as_str().unwrap().to_string();
    assert_eq!((lt(&s, &a), lt(&s, &a2)), ((0.0, 0.0), (0.0, -0.5)));
    let after = kimo_core::audio::mix(&s.project);
    // what is heard: each tone's level in every stretch between edges is unchanged (sample-exact comparison is not
    // meaningful here: the decoder seeks this Matroska source with ~1 ms timestamp granularity, a few samples of phase)
    for (t0, t1) in [(0.1, 0.95), (1.05, 1.45), (1.55, 1.95), (2.05, 2.95), (3.05, 3.9)] {
        for f in [220.0, 330.0, 660.0, 990.0] {
            let (x, y) = (amp(&before, f, t0, t1), amp(&after, f, t0, t1));
            assert!((x - y).abs() < 0.01 * x.max(0.01), "{f} Hz over {t0}..{t1}: before {x:.4} after {y:.4}");
        }
    }
}

#[test]
fn refusals_name_the_reason_and_change_nothing() {
    let (mut s, a, b) = cut();
    let json0 = serde_json::to_string(&s.project).unwrap();
    for (args, word) in [
        (json!({"clip_id":b,"kind":"j","seconds":1.5}), "handle"),      // B has 1 s of source before its in-point
        (json!({"clip_id":b,"kind":"l","seconds":2.0}), "incoming"),    // B's audio would vanish
        (json!({"clip_id":b,"kind":"x","seconds":0.5}), "kind"),
        (json!({"clip_id":b,"kind":"j","seconds":-1}), "seconds"),
        (json!({"clip_id":b,"kind":"j"}), "seconds"),
        (json!({"clip_id":"nope","kind":"j","seconds":0.5}), "clip"),
    ] {
        let e = dispatch(&mut s, "set_split_edit", &args).unwrap_err();
        assert!(e.to_lowercase().contains(word), "{args}: {e}");
        assert_eq!(serde_json::to_string(&s.project).unwrap(), json0, "{args} changed the project");
    }
    // L longer than A's remaining source (A source ends at 4.0; A uses 1..3) is refused too.
    let e = dispatch(&mut s, "set_split_edit", &json!({"clip_id":b,"kind":"l","seconds":1.5})).unwrap_err();
    assert!(e.to_lowercase().contains("handle"), "{e}");
    let _ = a;
}

#[test]
fn registry_entry_is_short_and_spectator_gui_routes_it_as_an_edit() {
    let reg = kimo_core::commands::registry();
    let cmd = reg.iter().find(|c| c.name == "set_split_edit").expect("set_split_edit registered");
    assert!(cmd.description.len() < 150, "{} chars", cmd.description.len());
    assert!(cmd.mutates);
    assert_eq!(kimo_core::toolgroups::group_of("set_split_edit"), "timeline");
    let router = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../kimo-gui/ui/router.js")).unwrap();
    assert!(router.contains("set_split_edit"), "spectator router names set_split_edit");
    let flat = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../kimo-gui/tests/tools_flat.json")).unwrap();
    assert!(flat.contains("\"set_split_edit\""), "spectator coverage list carries set_split_edit");
}
