//! add_displacement_map and set_matte (AE Displacement Map / Set Matte) on a synthetic red ramp (160x90 @ 10 fps): every
//! column's red value names the source column it came from, so a shift is read off directly. Map and matte clips are solids
//! and split images on another track; they drive the effect and stay hidden unless asked.
use kimo_core::commands::{dispatch, Session};
use kimo_core::effects::Frame;
use kimo_core::render;
use serde_json::{json, Value};

const W: usize = 160; const H: usize = 90;
fn png(name: &str, f: impl Fn(usize, usize) -> [u8; 3]) -> String {
    use std::io::Write;
    // a per-call atomic counter: scene() calls png("a", ...) for nearly every test, and two tests both call
    // png("band", ...) with the identical closure -- a path built from name+pid alone is shared across tests.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!("kimo-lfx-{}-{name}-{n}.png", std::process::id()));
    let mut ch = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", &format!("{W}x{H}"), "-i", "-", "-frames:v", "1", path.to_str().unwrap()]).stdin(std::process::Stdio::piped()).spawn().expect("ffmpeg");
    { let mut si = ch.stdin.take().unwrap(); let mut o = Vec::with_capacity(W * H * 3); for y in 0..H { for x in 0..W { o.extend_from_slice(&f(x, y)); } } si.write_all(&o).unwrap(); }
    assert!(ch.wait().unwrap().success()); path.to_str().unwrap().to_string()
}
fn ramp(x: usize) -> u8 { (x * 255 / (W - 1)) as u8 }
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn id(v: &Value, k: &str) -> String { v[k].as_str().unwrap().to_string() }
/// The red ramp clip A on track 1 (0..2 s) and an upper track for map / matte clips.
fn scene() -> (Session, String, String) {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let pa = png("a", |x, _| [ramp(x), 0, 40]); let aa = id(&c(&mut s, "import_media", json!({"path": pa})), "asset_id");
    let t1 = id(&c(&mut s, "add_track", json!({})), "track_id"); let t2 = id(&c(&mut s, "add_track", json!({})), "track_id");
    let a = id(&c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": aa, "start": 0, "duration": 2.0})), "clip_id");
    (s, a, t2)
}
fn solid(s: &mut Session, t: &str, col: [f64; 3]) -> String { id(&c(s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": col, "start": 0, "duration": 2.0})), "clip_id") }
fn frame(s: &Session, t: f64) -> Frame { let mut cache = render::Cache::new(); render::prepare_at(&s.project, &mut cache, Some(t)).unwrap(); render::render_frame(&s.project, t, &cache).unwrap() }
fn px(f: &Frame, x: usize, y: usize) -> [f32; 3] { let i = (y * f.w + x) * 4; [f.d[i] * 255.0, f.d[i + 1] * 255.0, f.d[i + 2] * 255.0] }
/// Source column of the red ramp shown at column x (row 45).
fn col(f: &Frame, x: usize) -> f32 { px(f, x, H / 2)[0] * (W - 1) as f32 / 255.0 }
fn near(got: f32, want: f32, what: &str) { assert!((got - want).abs() <= 1.5, "{what}: want {want}, got {got:.2}"); }
fn pj(s: &Session) -> String { serde_json::to_string(&s.project).unwrap() }

#[test]
fn a_map_clip_shifts_pixels_by_its_channel_values() {
    let (mut s, a, t2) = scene();
    let plain = frame(&s, 0.5); near(col(&plain, 50), 50.0, "before");
    let m = solid(&mut s, &t2, [1.0, 0.5, 0.5]); // red 1 -> +max_h, green 0.5 -> no vertical shift
    let r = c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 20}));
    let f = frame(&s, 0.5); near(col(&f, 50), 70.0, "red 1.0 pulls from 20 px right"); assert!(px(&f, 50, 45)[1] < 2.0, "the map clip is hidden: {:?}", px(&f, 50, 45));
    c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": -20}));
    assert_eq!(s.project.find_clip(&a).unwrap().effects.len(), 1, "a second call updates the one effect");
    let f = frame(&s, 0.5); near(col(&f, 50), 30.0, "negative max reverses");
    // edges: x 150 pulls from 170 with +20
    for (edges, want) in [("clamp", 159.0), ("wrap", 10.0), ("transparent", 0.0)] { c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 20, "edges": edges}));
        let f = frame(&s, 0.5); near(col(&f, 150), want, &format!("edges {edges} at column 150")); }
    // the amount keyframes like any effect param
    let e = id(&r, "effect_id"); c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m}));
    c(&mut s, "set_keyframe", json!({"clip_id": a, "effect_id": e, "property": "max_h", "time": 0.0, "value": 0.0})); c(&mut s, "set_keyframe", json!({"clip_id": a, "effect_id": e, "property": "max_h", "time": 1.0, "value": 20.0}));
    let f = frame(&s, 0.5); near(col(&f, 50), 60.0, "max_h keyed 0 -> 20 is 10 at half time");
    c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "hide_map": false})); let f = frame(&s, 0.5); let p = px(&f, 50, 45); assert!(p[0] > 250.0 && p[1] > 120.0, "hide_map false shows the map clip: {p:?}");
    // neutral grey map (fresh scene, so no unused map clip is left showing): no shift at all
    let (mut s, a, t2) = scene(); let g = solid(&mut s, &t2, [0.5, 0.5, 0.5]); c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": g, "max_horizontal": 40, "max_vertical": 40}));
    let f = frame(&s, 0.5); for x in [10, 80, 150] { near(col(&f, x), x as f32, "a 0.5 grey map is neutral"); }
}

/// The map is read where the clip lands on screen: a map clip covering the left half of the canvas warps exactly the part of a
/// scaled, moved or rotated clip that is drawn over that half, by max px on screen (KP4: before, the map was read in the clip's
/// own frame before its transform, so it landed somewhere else on a scaled clip and scaled with it). map_space "clip" keeps that.
#[test]
fn the_map_lines_up_with_a_scaled_moved_or_rotated_clip() {
    let half = |s: &mut Session, t2: &str| { let pm = png("half", |x, _| if x < W / 2 { [255, 128, 128] } else { [128, 128, 128] }); let am = id(&c(s, "import_media", json!({"path": pm})), "asset_id");
        id(&c(s, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": am, "start": 0, "duration": 2.0})), "clip_id") };
    // scale 0.5 about the centre: canvas column X shows the clip's column 2X - 80
    let (mut s, a, t2) = scene(); let m = half(&mut s, &t2);
    c(&mut s, "set_property", json!({"clip_id": a, "property": "scale", "value": 0.5}));
    c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 20}));
    let f = frame(&s, 0.5);
    near(col(&f, 60), 80.0, "scaled 0.5: canvas x 60 is under the map's left half, 20 screen px = 40 clip px, so it shows clip column 40 + 40");
    near(col(&f, 100), 120.0, "scaled 0.5: canvas x 100 is under the neutral half: clip column 120 unshifted");
    c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 20, "map_space": "clip"}));
    let f = frame(&s, 0.5); near(col(&f, 60), 60.0, "map_space clip: the map rides in the clip's own frame (clip column 40 + 20)");
    // moved 60 px right: canvas x 100 shows clip column 40, and the map there is neutral
    let (mut s, a, t2) = scene(); let m = half(&mut s, &t2);
    c(&mut s, "set_property", json!({"clip_id": a, "property": "x", "value": 60.0}));
    c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 20}));
    let f = frame(&s, 0.5); near(col(&f, 100), 40.0, "moved +60: canvas x 100 (clip column 40) is under the neutral half");
    near(col(&f, 70), 30.0, "moved +60: canvas x 70 (clip column 10) is under the left half: +20 px");
    // rotated 180: canvas x 40 shows clip column 120; a +20 px screen shift is -20 px in the clip's own frame
    let (mut s, a, t2) = scene(); let m = half(&mut s, &t2);
    c(&mut s, "set_property", json!({"clip_id": a, "property": "rotation", "value": 180.0}));
    c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 20}));
    let f = frame(&s, 0.5); near(col(&f, 40), 100.0, "rotated 180: canvas x 40 pulls from 20 px to its right on screen = clip column 120 - 20");
    near(col(&f, 120), 40.0, "rotated 180: canvas x 120 (clip column 40) is under the neutral half");
    assert!(dispatch(&mut s, "add_displacement_map", &json!({"clip_id": a, "map_clip_id": m, "map_space": "world"})).unwrap_err().contains("must be one of"));
}

/// A clip moved by its own transition (a push sliding it in) is placed with that move too: the map stays put on screen while
/// the clip slides under it (KP5; before, the transition's offset was left out and the map rode along with the clip).
#[test]
fn the_map_stays_on_screen_while_a_push_slides_the_clip_in() {
    let (mut s, a, t2) = scene();
    let pm = png("band", |x, _| if x >= 120 { [255, 128, 128] } else { [128, 128, 128] }); let am = id(&c(&mut s, "import_media", json!({"path": pm})), "asset_id");
    let m = id(&c(&mut s, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": am, "start": 0, "duration": 2.0})), "clip_id");
    c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 20}));
    c(&mut s, "add_push_wipe", json!({"clip_id": a, "kind": "push", "direction": "left", "duration": 1.0, "ease": "linear"}));
    // half way through the push the clip is 80 px right of its place: canvas column X shows clip column X - 80
    let f = frame(&s, 0.5);
    near(col(&f, 140), 80.0, "push half way (clip 80 px right): canvas x 140 is under the map's red band, so it shows clip column 60 + 20");
    near(col(&f, 100), 20.0, "push half way: canvas x 100 (clip column 20) is under the neutral part of the map");
    let f = frame(&s, 1.5); near(col(&f, 125), 145.0, "after the push the clip is in place: canvas x 125 under the band shows column 125 + 20"); near(col(&f, 100), 100.0, "after the push: canvas x 100 unshifted");
}

/// The OUTGOING clip of a push keeps its displacement map while it is pushed out (KP6; before, the outgoing picture was rendered
/// alone without its map clip and was drawn unwarped for the whole push). The map clip spans only the outgoing shot, so it is
/// held at its last frame while the push carries the shot on, and it stays on screen like the incoming clip's map.
#[test]
fn the_outgoing_clip_of_a_push_keeps_its_map() {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": 10}));
    let pa = png("a", |x, _| [ramp(x), 0, 40]); let aa = id(&c(&mut s, "import_media", json!({"path": pa})), "asset_id");
    let pm = png("band", |x, _| if x >= 120 { [255, 128, 128] } else { [128, 128, 128] }); let am = id(&c(&mut s, "import_media", json!({"path": pm})), "asset_id");
    let t1 = id(&c(&mut s, "add_track", json!({})), "track_id"); let t2 = id(&c(&mut s, "add_track", json!({})), "track_id");
    let a = id(&c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "media", "asset_id": aa, "start": 0, "duration": 1.0})), "clip_id");
    let b = id(&c(&mut s, "add_clip", json!({"track_id": t1, "source_type": "solid", "color": [0.0, 0.0, 1.0], "start": 1.0, "duration": 1.0})), "clip_id");
    let m = id(&c(&mut s, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": am, "start": 0, "duration": 1.0})), "clip_id");
    c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 20}));
    c(&mut s, "add_push_wipe", json!({"clip_id": b, "kind": "push", "direction": "right", "duration": 1.0, "ease": "linear"}));
    let f = frame(&s, 0.5); near(col(&f, 130), 150.0, "before the push: canvas x 130 is under the band (+20)"); near(col(&f, 100), 100.0, "before the push: x 100 unshifted");
    // half way through the push the outgoing shot is 80 px right: canvas column X shows its column X - 80
    let f = frame(&s, 1.5);
    near(col(&f, 130), 70.0, "outgoing half way out (80 px right): canvas x 130 is under the map's red band, so it shows outgoing column 50 + 20");
    near(col(&f, 100), 20.0, "outgoing half way out: canvas x 100 (outgoing column 20) is under the neutral part of the map");
    assert!(px(&f, 40, 45)[2] > 250.0 && px(&f, 40, 45)[0] < 2.0, "the incoming blue clip fills the left half: {:?}", px(&f, 40, 45));
    // a map asked to show is still not painted into the outgoing picture itself
    c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 20, "hide_map": false}));
    let f = frame(&s, 1.5); near(col(&f, 100), 20.0, "hide_map false: the map is drawn on its own track, not inside the outgoing picture"); near(col(&f, 130), 70.0, "hide_map false: still warped");
}

#[test]
fn set_matte_takes_the_alpha_from_another_clips_channel() {
    let (mut s, a, t2) = scene();
    let pm = png("m", |x, _| if x < W / 2 { [255, 255, 255] } else { [0, 0, 0] }); let am = id(&c(&mut s, "import_media", json!({"path": pm})), "asset_id");
    let m = id(&c(&mut s, "add_clip", json!({"track_id": t2, "source_type": "media", "asset_id": am, "start": 0, "duration": 2.0})), "clip_id");
    c(&mut s, "set_matte", json!({"clip_id": a, "matte_clip_id": m, "channel": "luma"}));
    let f = frame(&s, 0.5); near(col(&f, 40), 40.0, "white half keeps A"); assert!(px(&f, 120, 45)[0] < 1.0 && px(&f, 120, 45)[2] < 1.0, "black half cuts A away (and the matte clip is hidden): {:?}", px(&f, 120, 45));
    c(&mut s, "set_matte", json!({"clip_id": a, "matte_clip_id": m, "channel": "luma", "invert": true}));
    let f = frame(&s, 0.5); assert!(px(&f, 40, 45)[0] < 1.0, "inverted: white half cut: {:?}", px(&f, 40, 45)); near(col(&f, 120), 120.0, "inverted: black half kept");
    c(&mut s, "set_matte", json!({"clip_id": a, "matte_clip_id": m, "channel": "alpha"})); let f = frame(&s, 0.5); near(col(&f, 120), 120.0, "an opaque matte clip's alpha keeps everything");
    let (mut s, a, t2) = scene(); let r = solid(&mut s, &t2, [1.0, 0.0, 0.0]); // fresh scene: the split matte clip is not left showing
    c(&mut s, "set_matte", json!({"clip_id": a, "matte_clip_id": r, "channel": "red"})); let f = frame(&s, 0.5); near(col(&f, 100), 100.0, "red channel of a red solid is 1");
    c(&mut s, "set_matte", json!({"clip_id": a, "matte_clip_id": r, "channel": "green"})); let f = frame(&s, 0.5); assert!(px(&f, 100, 45)[0] < 1.0, "green channel of a red solid is 0: {:?}", px(&f, 100, 45));
    c(&mut s, "set_matte", json!({"clip_id": a, "matte_clip_id": r, "channel": "saturation"})); let f = frame(&s, 0.5); near(col(&f, 100), 100.0, "a pure red is fully saturated");
}

#[test]
fn deleting_the_driving_clip_clears_the_reference() {
    let (mut s, a, t2) = scene(); let m = solid(&mut s, &t2, [1.0, 0.5, 0.5]); let k = solid(&mut s, &t2, [0.0, 0.0, 0.0]);
    c(&mut s, "add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 20})); c(&mut s, "set_matte", json!({"clip_id": a, "matte_clip_id": k, "channel": "luma", "invert": true}));
    let f = frame(&s, 0.5); near(col(&f, 50), 70.0, "displaced and matted (inverted black = keep)");
    assert!(s.project.dangling_refs().is_empty()); c(&mut s, "delete_clip", json!({"clip_id": m})); c(&mut s, "delete_clip", json!({"clip_id": k}));
    assert!(s.project.dangling_refs().is_empty(), "dangling: {:?}", s.project.dangling_refs());
    let f = frame(&s, 0.5); near(col(&f, 50), 50.0, "with its map gone the clip renders unshifted and unmatted");
}

#[test]
fn bad_input_is_refused_without_changing_the_project() {
    let (mut s, a, t2) = scene(); let m = solid(&mut s, &t2, [0.5, 0.5, 0.5]); let before = pj(&s);
    for (tool, args, want) in [
        ("add_displacement_map", json!({"clip_id": a}), "map_clip_id"),
        ("add_displacement_map", json!({"clip_id": a, "map_clip_id": a}), "another clip"),
        ("add_displacement_map", json!({"clip_id": a, "map_clip_id": "clip999"}), "no such clip"),
        ("add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "horizontal_channel": "purple"}), "must be one of"),
        ("add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "max_horizontal": 5000}), "within"),
        ("add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "edges": "mirror"}), "must be one of"),
        ("add_displacement_map", json!({"clip_id": a, "map_clip_id": m, "strength": 2}), "unknown argument"),
        ("set_matte", json!({"clip_id": a, "matte_clip_id": m, "channel": "off"}), "must be one of"),
        ("set_matte", json!({"clip_id": a, "matte_clip_id": m, "invert": "yes"}), "true or false"),
        ("set_matte", json!({"clip_id": "nope", "matte_clip_id": m}), "no such clip"),
    ] { let e = dispatch(&mut s, tool, &args).expect_err(&format!("{tool} {args} must be refused")); assert!(e.contains(want), "{tool} {args}: {e}"); assert_eq!(pj(&s), before, "{tool} {args} changed the project"); }
    c(&mut s, "set_matte", json!({"clip_id": a, "matte_clip_id": m})); assert_eq!(c(&mut s, "set_matte", json!({"clip_id": a, "remove": true}))["removed"], 1);
}
