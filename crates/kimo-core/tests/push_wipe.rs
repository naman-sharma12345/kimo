//! add_push_wipe: Push and Gradient Wipe transitions, on synthetic ramps (160x90 @ 10 fps). A is a red ramp across x, B a
//! green ramp, so every column says which picture it shows and from which source column. Push must move BOTH pictures edge
//! to edge (A leaves as B arrives); a gradient wipe must reveal B in the ramp's order with A held still; a luma clip drives
//! the reveal from its own pixels and never shows itself.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 90;
fn png(name: &str, f: impl Fn(usize, usize) -> [u8; 3]) -> String {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kimo-pw-{}-{name}.png", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{W}x{H}"), "-i", "-", "-frames:v", "1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); let mut o = Vec::with_capacity(W * H * 3); for y in 0..H { for x in 0..W { o.extend_from_slice(&f(x, y)); } } si.write_all(&o).unwrap(); }
    assert!(ch.wait().unwrap().success()); path.to_str().unwrap().to_string()
}
fn ramp(x: usize) -> u8 { (x * 255 / (W - 1)) as u8 }
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
/// A (red ramp) and B (green ramp). `overlap`: A on track 1 for 0..2 and B on track 2 for 1..2; else both on one track, A 0..1, B 1..2.
fn scene(overlap: bool) -> (Session, String, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let pa = png("a", |x, _| [ramp(x), 0, 40]); let pb = png("b", |x, _| [0, ramp(x), 40]);
    let aa = id(&c(&mut s, "import_media", json!({"path": pa})), "asset_id"); let ab = id(&c(&mut s, "import_media", json!({"path": pb})), "asset_id");
    let t1 = id(&c(&mut s, "add_track", json!({})), "track_id"); let t2 = if overlap { id(&c(&mut s, "add_track", json!({})), "track_id") } else { t1.clone() };
    let ca = id(&c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": aa, "start": 0, "duration": if overlap { 2.0 } else { 1.0 }})), "clip_id");
    let cb = id(&c(&mut s, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": ab, "start": 1.0, "duration": 1.0})), "clip_id");
    (s, ca, cb)
}
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
fn px(f: &Frame, x: usize, y: usize) -> [f32; 3] { let i = (y * f.w + x) * 4; [f.d[i] * 255.0, f.d[i + 1] * 255.0, f.d[i + 2] * 255.0] }
/// Which picture a pixel shows and the source column it came from: ('A', col) for the red ramp, ('B', col) for the green one.
fn who(f: &Frame, x: usize, y: usize) -> (char, f32) { let p = px(f, x, y); if p[0] > p[1] + 8.0 { ('A', p[0] * (W - 1) as f32 / 255.0) } else if p[1] > p[0] + 8.0 { ('B', p[1] * (W - 1) as f32 / 255.0) } else { ('?', p[0].max(p[1])) } }
fn shows(f: &Frame, x: usize, which: char, col: f32, what: &str) { let (w, cc) = who(f, x, H / 2); assert!(w == which && (cc - col).abs() <= 2.5, "{what}: column {x} must show {which} source column {col}, got {w} column {cc:.1} (rgb {:?})", px(f, x, H / 2)); }
fn pj(s: &Session) -> String { serde_json::to_string(&s.project).unwrap() }

#[test]
fn push_slides_the_outgoing_clip_out_as_the_incoming_clip_slides_in() {
    let (mut s, _a, b) = scene(false);
    let after_plain = frame(&s, 1.6);
    let r = c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "push", "duration": 0.5, "ease": "linear"}));
    assert!(r["outgoing"].as_str().unwrap().contains("held"), "{r}");
    let f = frame(&s, 1.25); // k = 0.5, direction left: A moved 80 px left, B 80 px from the right edge
    shows(&f, 10, 'A', 90.0, "push left mid"); shows(&f, 70, 'A', 150.0, "push left mid");
    shows(&f, 90, 'B', 10.0, "push left mid"); shows(&f, 150, 'B', 70.0, "push left mid");
    let f = frame(&s, 1.05); // k = 0.1: the seam sits at x 144
    shows(&f, 100, 'A', 116.0, "push left early"); shows(&f, 150, 'B', 6.0, "push left early");
    let f = frame(&s, 1.6); assert_eq!(f.d, after_plain.d, "after the push B renders exactly as without it");
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "push", "duration": 0.5, "ease": "linear", "direction": "right"}));
    let f = frame(&s, 1.25); shows(&f, 10, 'B', 90.0, "push right mid"); shows(&f, 150, 'A', 70.0, "push right mid");
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "push", "duration": 0.5, "ease": "linear", "direction": "up"}));
    let f = frame(&s, 1.25); // A moved 45 px up, B rises from the bottom: same columns, different rows
    let (top, bottom) = (who(&f, 40, 10), who(&f, 40, 80)); assert!(top.0 == 'A' && bottom.0 == 'B' && (top.1 - 40.0).abs() < 2.5 && (bottom.1 - 40.0).abs() < 2.5, "push up mid: top {top:?} bottom {bottom:?}");
}

#[test]
fn push_moves_an_overlapping_lower_layer_out() {
    let (mut s, _a, b) = scene(true);
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "push", "duration": 0.5, "ease": "linear"}));
    let r = c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "push", "duration": 0.5, "ease": "linear"})); assert!(r["outgoing"].as_str().unwrap().contains("under"), "{r}");
    let f = frame(&s, 1.25); shows(&f, 10, 'A', 90.0, "overlap push mid"); shows(&f, 90, 'B', 10.0, "overlap push mid");
    let f = frame(&s, 1.5); shows(&f, 10, 'B', 10.0, "overlap push done"); shows(&f, 150, 'B', 150.0, "overlap push done");
}

#[test]
fn gradient_wipe_reveals_in_ramp_order_with_the_outgoing_clip_held_still() {
    let (mut s, _a, b) = scene(false);
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "gradient_wipe", "duration": 0.5, "ease": "linear", "softness": 0.0}));
    let f = frame(&s, 1.25); shows(&f, 40, 'B', 40.0, "linear wipe mid"); shows(&f, 79, 'B', 79.0, "linear wipe mid"); shows(&f, 81, 'A', 81.0, "linear wipe mid"); shows(&f, 120, 'A', 120.0, "linear wipe mid");
    let f = frame(&s, 1.05); shows(&f, 12, 'B', 12.0, "linear wipe early"); shows(&f, 40, 'A', 40.0, "linear wipe early");
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "gradient_wipe", "duration": 0.5, "ease": "linear", "softness": 0.0, "angle": 180}));
    let f = frame(&s, 1.25); shows(&f, 40, 'A', 40.0, "linear wipe 180"); shows(&f, 120, 'B', 120.0, "linear wipe 180");
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "gradient_wipe", "duration": 0.5, "ease": "linear", "softness": 0.0, "gradient": "radial"}));
    let f = frame(&s, 1.15); let (mid, corner) = (who(&f, 80, 45), who(&f, 157, 2)); assert!(mid.0 == 'B' && corner.0 == 'A', "radial: the centre opens first, got centre {mid:?} corner {corner:?}");
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "gradient_wipe", "duration": 0.5, "ease": "linear", "softness": 0.0, "gradient": "angular"}));
    let f = frame(&s, 1.125); let (below_right, above_right) = (who(&f, 110, 75), who(&f, 110, 15)); // k 0.25: below-right sits at 0.125 of the sweep, above-right at 0.875
    assert!(below_right.0 == 'B' && above_right.0 == 'A', "angular: the sweep runs clockwise from 3 o'clock, got below-right {below_right:?} above-right {above_right:?}");
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "gradient_wipe", "duration": 0.5, "ease": "linear", "softness": 0.0, "gradient": "angular", "invert": true}));
    let f = frame(&s, 1.125); let (below_right, above_right) = (who(&f, 110, 75), who(&f, 110, 15));
    assert!(below_right.0 == 'A' && above_right.0 == 'B', "angular invert: reversed order, got below-right {below_right:?} above-right {above_right:?}");
    // softness widens the edge: a soft linear wipe has mixed pixels around the edge, a hard one none
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "gradient_wipe", "duration": 0.5, "ease": "linear", "softness": 0.4}));
    let f = frame(&s, 1.25); let mixed = (0..W).filter(|&x| { let p = px(&f, x, 45); p[0] > 12.0 && p[1] > 12.0 }).count(); assert!(mixed > 30, "softness 0.4 must blend a wide edge, {mixed} mixed columns");
}

/// A 4 s video whose every pixel's red value counts its frame: 40 + 5 * frame (10 fps).
fn counting_video() -> String {
    use std::io::Write;
    let path = std::env::temp_dir().join(format!("kimo-pw-{}-count.mp4", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{W}x{H}"), "-r", "10", "-i", "-", "-c:v", "libx264rgb", "-qp", "0", "-pix_fmt", "rgb24", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); for fi in 0..40usize { let mut o = Vec::with_capacity(W * H * 3); for _ in 0..W * H { o.extend_from_slice(&[(40 + 5 * fi) as u8, 0, 40]); } si.write_all(&o).unwrap(); } }
    assert!(ch.wait().unwrap().success()); path.to_str().unwrap().to_string()
}

/// KP4: on a plain cut the outgoing clip keeps playing from its media past its out point (Premiere's handles) instead of
/// freezing on its last frame; with no media left it holds its last frame as before.
#[test]
fn on_a_cut_the_outgoing_clip_keeps_playing_through_the_transition() {
    for (in_pt, want, word) in [(0.0, 100.0, "keeps playing"), (3.0, 235.0, "held")] {
        let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
        let av = id(&c(&mut s, "import_media", json!({"path": counting_video()})), "asset_id"); let pb = png("b", |x, _| [0, ramp(x), 40]); let ab = id(&c(&mut s, "import_media", json!({"path": pb})), "asset_id");
        let t1 = id(&c(&mut s, "add_track", json!({})), "track_id");
        c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": av, "start": 0, "duration": 1.0, "in_point": in_pt}));
        let b = id(&c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": ab, "start": 1.0, "duration": 1.0})), "clip_id");
        let own_last = if in_pt == 0.0 { 85.0 } else { 235.0 }; let f = frame(&s, 0.95); let last = px(&f, 10, 45)[0]; assert!((last - own_last).abs() <= 3.0, "in {in_pt}: the clip's own last frame is red {own_last}, got {last}");
        for kind in ["push", "gradient_wipe"] {
            let r = if kind == "push" { c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": kind, "duration": 0.5, "ease": "linear"})) } else { c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": kind, "duration": 0.5, "ease": "linear", "softness": 0.0})) };
            assert!(r["outgoing"].as_str().unwrap().contains(word), "in {in_pt} {kind}: {r}");
            let f = frame(&s, 1.25); let (x, y) = if kind == "push" { (10, 45) } else { (120, 45) }; let p = px(&f, x, y);
            assert!(p[1] < 8.0 && (p[0] - want).abs() <= 3.0, "in {in_pt} {kind}: at 1.25 s the outgoing clip must show red {want} (its media 0.25 s past the cut), got {p:?}");
        }
    }
}

/// KP4: an angular wipe's start line is a soft edge too: crossing it, the mix of B changes gradually, never in one row.
#[test]
fn an_angular_wipe_has_no_hard_seam_on_its_start_line() {
    let (mut s, _a, b) = scene(false);
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "gradient_wipe", "duration": 0.5, "ease": "linear", "softness": 0.3, "gradient": "angular"}));
    let f = frame(&s, 1.25); let x = 150; let r = ramp(x) as f32;
    let mix: Vec<f32> = (30..61).map(|y| (px(&f, x, y)[1] / r).clamp(0.0, 1.0)).collect();
    let jump = mix.windows(2).map(|w| (w[1] - w[0]).abs()).fold(0.0f32, f32::max);
    assert!(jump < 0.3, "angular start line (3 o'clock, x {x}): B's share must change gradually across it, biggest row-to-row jump {jump:.2} in {mix:.2?}");
    assert!(mix[0] < mix[30], "above the line is later in the sweep than below it: {mix:.2?}");
    let f = frame(&s, 1.49); let late = (0..W).step_by(8).flat_map(|x| (0..H).step_by(8).map(move |y| (x, y))).map(|(x, y)| { let p = px(&f, x, y); p[0] }).fold(0.0f32, f32::max);
    assert!(late < 40.0, "at the end of the sweep the start line must have closed (no A left), max red {late}");
}

#[test]
fn a_luma_clip_drives_the_wipe_and_stays_hidden() {
    let (mut s, _a, b) = scene(false);
    let pl = png("l", |x, _| if x < W / 2 { [255, 255, 255] } else { [0, 0, 0] });
    let al = id(&c(&mut s, "import_media", json!({"path": pl})), "asset_id"); let t = id(&c(&mut s, "add_track", json!({})), "track_id");
    let cl = id(&c(&mut s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": al, "start": 1.0, "duration": 1.0})), "clip_id");
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "gradient_wipe", "duration": 0.5, "ease": "linear", "softness": 0.05, "luma_clip_id": cl}));
    let f = frame(&s, 1.15); shows(&f, 30, 'A', 30.0, "luma wipe: white waits"); shows(&f, 130, 'B', 130.0, "luma wipe: black opens first");
    let f = frame(&s, 1.7); shows(&f, 30, 'B', 30.0, "luma clip hidden after the wipe"); shows(&f, 130, 'B', 130.0, "luma clip hidden after the wipe");
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "gradient_wipe", "duration": 0.5, "ease": "linear", "softness": 0.05, "luma_clip_id": cl, "invert": true}));
    let f = frame(&s, 1.15); shows(&f, 30, 'B', 30.0, "inverted luma wipe"); shows(&f, 130, 'A', 130.0, "inverted luma wipe");
    // deleting the luma clip clears the reference (no dangling ref); the wipe falls back to its linear gradient
    assert!(s.project.dangling_refs().is_empty());
    c(&mut s, "delete_clip", json!({"clip_id": cl}));
    assert!(s.project.dangling_refs().is_empty(), "dangling: {:?}", s.project.dangling_refs());
    assert!(s.project.find_clip(&b).unwrap().transition_in.as_ref().unwrap().spec.as_ref().unwrap().luma_clip.is_none(), "the luma reference must be cleared");
    let f = frame(&s, 1.25); shows(&f, 40, 'A', 40.0, "after the luma clip is deleted: inverted linear"); shows(&f, 120, 'B', 120.0, "after the luma clip is deleted: inverted linear");
}

#[test]
fn bad_input_is_refused_without_changing_the_project_and_add_transition_is_unchanged() {
    let (mut s, a, b) = scene(false); let before = pj(&s);
    for (args, want) in [
        (json!({"clip_id": b, "kind": "push", "softness": 0.2}), "gradient_wipe setting"),
        (json!({"clip_id": b, "kind": "gradient_wipe", "direction": "left"}), "push setting"),
        (json!({"clip_id": b, "kind": "push", "direction": "sideways"}), "direction must be"),
        (json!({"clip_id": b, "kind": "push", "duration": 3.0}), "longer than clip"),
        (json!({"clip_id": b, "kind": "spin"}), "kind must be"),
        (json!({"clip_id": b, "kind": "gradient_wipe", "luma_clip_id": b}), "another clip"),
        (json!({"clip_id": b, "kind": "gradient_wipe", "luma_clip_id": "nope"}), "no such clip"),
        (json!({"clip_id": b, "kind": "gradient_wipe", "gradient": "spiral"}), "gradient must be"),
        (json!({"clip_id": b, "kind": "push", "speed": 2}), "unknown argument"),
        (json!({"clip_id": b, "kind": "push", "ease": "wobble"}), "unknown ease"),
    ] { let e = dispatch(&mut s, "add_push_wipe", &args).expect_err(&format!("{args} must be refused")); assert!(e.contains(want), "{args}: {e}"); assert_eq!(pj(&s), before, "{args} changed the project"); }
    let kinds = c(&mut s, "list_transitions", json!({})); assert!(!kinds.as_array().unwrap().iter().any(|k| k == "push" || k == "gradient_wipe"), "add_transition kinds unchanged: {kinds}");
    c(&mut s, "add_transition", json!({"clip_id": b, "kind": "crossfade", "duration": 0.5})); assert!(!pj(&s).contains("\"spec\""), "add_transition projects serialise as before");
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "push"})); let r = c(&mut s, "add_push_wipe", json!({"clip_id": b, "remove": true})); assert_eq!(r["removed"], true);
    assert!(s.project.find_clip(&b).unwrap().transition_in.is_none()); let _ = a;
}
