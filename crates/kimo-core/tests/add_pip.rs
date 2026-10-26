//! add_pip (new, lane KF14): composite a second video as a picture-in-picture inset -- a corner webcam
//! bubble, floating avatar, or TV-frame cutaway. All tests below must fail before the tool exists
//! (dispatch itself errors with "unknown action").
use kimo_core::commands::{dispatch, Session};
use kimo_core::render::{self, render_frame, Cache};
use serde_json::{json, Value};
use std::path::PathBuf;

const W: u32 = 640;
const H: u32 = 360;
const FPS: f64 = 30.0;

fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n} {a}: {e}")) }
fn sid(v: &Value, k: &str) -> String { v[k].as_str().unwrap_or_else(|| panic!("no {k} in {v}")).to_string() }
fn project() -> Session { let mut s = Session::new(); c(&mut s, "project_set", json!({"width": W, "height": H, "fps": FPS})); s }

/// A tiny flat-colour media file (ffv1/.mkv, lossless, so pixel checks can use a tight tolerance),
/// the same fixture shape pick_thumbnail.rs uses for its own synthetic media.
static FIXTURE_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn solid_media(tag: &str, rgb: [u8; 3], dur: f64) -> PathBuf {
    let n = FIXTURE_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let p = std::env::temp_dir().join(format!("kimo-add-pip-{tag}-{}-{n}.mkv", std::process::id()));
    let color = format!("{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]);
    let chain = format!("color=c=0x{color}:s=64x64:r={FPS}:d={dur}");
    let ok = std::process::Command::new("ffmpeg").args(["-v", "error", "-y", "-f", "lavfi", "-i", &chain, "-c:v", "ffv1"]).arg(&p).status().unwrap();
    assert!(ok.success(), "ffmpeg failed for {tag}");
    p
}

/// A full-canvas background clip of a flat colour on its own (first, bottom) track, plus a PiP source
/// file of a different flat colour -- a minimal "current video" + "second video" pair with solid-colour
/// synthetic sources, exactly as the brief asks (the real-check later uses the real footage).
fn background(s: &mut Session, rgb: [u8; 3], dur: f64) {
    let t = sid(&c(s, "add_track", json!({})), "track_id");
    let bg = solid_media("bg", rgb, dur);
    let a = sid(&c(s, "import_media", json!({"path": bg})), "asset_id");
    c(s, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": a, "start": 0.0, "duration": dur, "fit": "cover"}));
}

const WHITE: [u8; 3] = [255, 255, 255];
const RED: [u8; 3] = [220, 20, 20];

fn rendered(s: &Session, t: f64) -> kimo_core::effects::Frame {
    let mut cache = Cache::new();
    render::prepare_at(&s.project, &mut cache, Some(t)).unwrap();
    render_frame(&s.project, t, &cache).unwrap()
}
fn px(f: &kimo_core::effects::Frame, x: u32, y: u32) -> [f32; 4] {
    let o = ((y as usize) * f.w + x as usize) * 4;
    [f.d[o], f.d[o + 1], f.d[o + 2], f.d[o + 3]]
}
fn close_to(got: [f32; 4], want: [u8; 3], tol: f32) -> bool {
    (0..3).all(|i| (got[i] - want[i] as f32 / 255.0).abs() < tol)
}

/// The inset's own geometry, replicated from the tool's documented contract (size = fraction of frame
/// width; margin/position/shape are the tool's own choice, so this only asserts the externally-promised
/// part: a `size`-wide box in the right corner, well clear of the background everywhere else).
fn box_geometry(size: f64, position: &str) -> (f64, f64, f64, f64) {
    let (cw, ch) = (W as f64, H as f64);
    let (bw, bh) = (size * cw, size * ch);
    let (hw, hh) = (bw / 2.0, bh / 2.0);
    let margin = 0.03 * cw.min(ch);
    let (x, y) = match position {
        "bottom_right" => (cw / 2.0 - hw - margin, ch / 2.0 - hh - margin),
        "center" => (0.0, 0.0),
        _ => panic!("unhandled position in test helper"),
    };
    (x, y, bw, bh)
}

#[test]
fn inset_appears_at_the_right_place_and_size() {
    let dur = 2.0;
    let mut s = project();
    background(&mut s, WHITE, dur);
    c(&mut s, "add_pip", json!({"source": solid_media("fg1", RED, dur).to_string_lossy(), "at_s": 0.0, "duration_s": dur,
        "position": "bottom_right", "size": 0.25, "shape": "rect", "shadow": false, "animate_in": "none", "animate_out": "none"}));
    let f = rendered(&s, dur / 2.0);
    let (x_off, y_off, bw, bh) = box_geometry(0.25, "bottom_right");
    let (cx, cy) = (W as f64 / 2.0 + x_off, H as f64 / 2.0 + y_off); // box centre in pixel coords (canvas origin top-left)
    assert!(close_to(px(&f, cx as u32, cy as u32), RED, 0.08), "box centre should be the PiP colour: {:?}", px(&f, cx as u32, cy as u32));
    // far from the box (top-left quadrant) must still be the plain background
    assert!(close_to(px(&f, 10, 10), WHITE, 0.08), "far corner should be background: {:?}", px(&f, 10, 10));
    // just outside the box's own edge (half a box-width further out) must also be background, not PiP colour
    let outside_x = (cx + bw * 0.75).min((W - 2) as f64);
    assert!(close_to(px(&f, outside_x as u32, cy as u32), WHITE, 0.08), "just outside the box should be background: {:?}", px(&f, outside_x as u32, cy as u32));
    let _ = bh;
}

#[test]
fn circle_shape_a_corner_of_the_bounding_box_shows_background() {
    let dur = 2.0;
    let mut s = project();
    background(&mut s, WHITE, dur);
    c(&mut s, "add_pip", json!({"source": solid_media("fg2", RED, dur).to_string_lossy(), "at_s": 0.0, "duration_s": dur,
        "position": "center", "size": 0.4, "shape": "circle", "shadow": false, "animate_in": "none", "animate_out": "none"}));
    let f = rendered(&s, dur / 2.0);
    let (x_off, y_off, bw, bh) = box_geometry(0.4, "center");
    let (cx, cy) = (W as f64 / 2.0 + x_off, H as f64 / 2.0 + y_off);
    assert!(close_to(px(&f, cx as u32, cy as u32), RED, 0.08), "box centre (inside the circle) should be the PiP colour");
    // the bounding box's own corner (outside the inscribed circle) must show the background through the crop
    let (corner_x, corner_y) = (cx - bw / 2.0 + 2.0, cy - bh / 2.0 + 2.0);
    assert!(close_to(px(&f, corner_x as u32, corner_y as u32), WHITE, 0.08), "bbox corner should show background through the circle crop: {:?}", px(&f, corner_x as u32, corner_y as u32));
}

#[test]
fn no_inset_before_at_s_or_after_at_s_plus_duration() {
    let total = 4.0;
    let mut s = project();
    background(&mut s, WHITE, total);
    c(&mut s, "add_pip", json!({"source": solid_media("fg3", RED, total).to_string_lossy(), "at_s": 1.0, "duration_s": 1.0,
        "position": "bottom_right", "size": 0.3, "shape": "rect", "shadow": false, "animate_in": "none", "animate_out": "none"}));
    let (x_off, y_off, _bw, _bh) = box_geometry(0.3, "bottom_right");
    let (cx, cy) = (W as f64 / 2.0 + x_off, H as f64 / 2.0 + y_off);
    let before = rendered(&s, 0.5);
    assert!(close_to(px(&before, cx as u32, cy as u32), WHITE, 0.08), "before at_s: no inset should be visible yet");
    let during = rendered(&s, 1.5);
    assert!(close_to(px(&during, cx as u32, cy as u32), RED, 0.08), "mid-window: the inset must actually be there (sanity check)");
    let after = rendered(&s, 2.5);
    assert!(close_to(px(&after, cx as u32, cy as u32), WHITE, 0.08), "after at_s+duration_s: the inset must be gone: {:?}", px(&after, cx as u32, cy as u32));
}

#[test]
fn pop_animation_scale_overshoots_then_settles_by_a_quarter_second() {
    let dur = 2.0;
    let mut s = project();
    background(&mut s, WHITE, dur);
    let r = c(&mut s, "add_pip", json!({"source": solid_media("fg4", RED, dur).to_string_lossy(), "at_s": 0.0, "duration_s": dur,
        "position": "center", "size": 1.0, "shape": "rect", "shadow": false, "animate_in": "pop", "animate_out": "none"}));
    let clip_id = sid(&r, "clip_id");
    let clip = s.project.find_clip(&clip_id).unwrap();
    let scale_at = |t: f64| clip.props.get("scale").map(|p| p.at(t)).unwrap_or(1.0);
    assert!(scale_at(0.0) < 0.1, "scale should start near 0 (well under 1) at the very start: {}", scale_at(0.0));
    assert!((scale_at(0.25) - 1.0).abs() < 1e-6, "scale should have settled to ~1 (size=1.0) by 0.25s: {}", scale_at(0.25));
    assert!(scale_at(0.2) > 1.0, "the pop should overshoot past the target before settling: {}", scale_at(0.2));
}

#[test]
fn undo_restores_the_timeline_exactly() {
    let dur = 2.0;
    let mut s = project();
    background(&mut s, WHITE, dur);
    let before = serde_json::to_string(&s.project).unwrap();
    c(&mut s, "add_pip", json!({"source": solid_media("fg5", RED, dur).to_string_lossy(), "at_s": 0.0, "duration_s": dur}));
    assert_ne!(serde_json::to_string(&s.project).unwrap(), before, "sanity: add_pip should have changed the project");
    c(&mut s, "undo", json!({}));
    assert_eq!(serde_json::to_string(&s.project).unwrap(), before, "undo must restore the exact pre-add_pip timeline in one step");
}

#[test]
fn bad_args_error() {
    let dur = 2.0;
    let mut s = project();
    background(&mut s, WHITE, dur);
    let good_source = solid_media("fg6", RED, dur).to_string_lossy().to_string();
    let ok = |a: Value| json!({"source": good_source.clone(), "at_s": 0.0, "duration_s": dur}).as_object().unwrap().clone().into_iter().chain(a.as_object().unwrap().clone()).collect::<serde_json::Map<_, _>>();
    assert!(dispatch(&mut s, "add_pip", &json!({"source": "/no/such/file.mp4", "at_s": 0.0, "duration_s": 1.0})).is_err(), "nonexistent source path must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"at_s": -1.0})))).is_err(), "negative at_s must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"duration_s": 0.0})))).is_err(), "zero duration_s must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"duration_s": -1.0})))).is_err(), "negative duration_s must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"size": 0.0})))).is_err(), "zero size must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"size": 1.5})))).is_err(), "size over 1 must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"shape": "hexagon"})))).is_err(), "unknown shape must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"animate_in": "spin"})))).is_err(), "unknown animate_in must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"animate_out": "spin"})))).is_err(), "unknown animate_out must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"border_px": -1.0})))).is_err(), "negative border_px must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"position": "upper_middle"})))).is_err(), "unknown named position must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"position": {"x": 1.5, "y": 0.5}})))).is_err(), "out-of-range normalized position must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"mute_source": "yes"})))).is_err(), "non-boolean mute_source must error");
    assert!(dispatch(&mut s, "add_pip", &json!(ok(json!({"bogus_arg": 1})))).is_err(), "unknown argument must error");
    assert!(dispatch(&mut s, "add_pip", &json!({"at_s": 0.0, "duration_s": 1.0})).is_err(), "missing source must error");
}
