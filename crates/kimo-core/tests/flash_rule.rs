//! v6 eyes: flash_series flash rule numbers (optional flash_limit / area_min_pct). Without them the output is byte-identical to
//! the flash_series of the previous eyes commit (bytes captured over MCP from that build, below). Known references: a 5 Hz
//! full-screen black/white square wave at 60 fps (worst 1 s window 6 flashes from t = 7/60, every window over 3, 24 windows
//! over 5, area 100 %), a half-frame flash (area 50 %, left out at area_min_pct 60), and an engine timeline of 0.125 s solid
//! clips (black/white then black/red) whose counts are worked out by hand in the comments.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::flash_series::{flash_rule, flash_spans, measure, transitions, FlashRule, Params, Planes};
use kimo_core::perception::analysis::Image;
use serde_json::{json, Value};

/// flash_series output (MCP tools/call text) from the build of the previous eyes commit, on the 64x36 24 fps timeline built
/// by `timeline()`: [args, bytes].
const PRE: &str = include_str!("data/flash_series_pre_rule.json");
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
fn timeline() -> Session {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    // 24 clips of 0.125 s (3 frames): black on even k, white on odd k below 16, saturated red on odd k from 17
    for k in 0..24 { let col = if k % 2 == 0 { [0., 0., 0.] } else if k < 16 { [1., 1., 1.] } else { [1., 0., 0.] }; c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": col, "start": k as f64 * 0.125, "duration": 0.125})); }
    s
}
fn img(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 3]) -> Image {
    let mut im = Image::new(w, h);
    for y in 0..h { for x in 0..w { let p = f(x, y); for ch in 0..3 { im.rgb[ch][y * w + x] = p[ch] as f64 / 255.; } } }
    im
}
/// the tool's own path: measure -> transitions -> spans -> flash_rule, windows every sample while start + 1 s fits
fn rule_of(frames: &[Image], fps: f64, limit: f64, area_min: f64, red: bool) -> FlashRule {
    let p = Params::default(); let mut prev: Option<Planes> = None; let mut st = vec![];
    for f in frames { let (s, pl) = measure(f, prev.as_ref(), &p); st.push(s); prev = Some(pl); }
    let times: Vec<f64> = (0..frames.len()).map(|i| i as f64 / fps).collect(); let dur = frames.len() as f64 / fps;
    let starts: Vec<f64> = times.iter().copied().filter(|s| s + 1.0 <= dur + 1e-9).collect();
    let (series, area): (Vec<f64>, Vec<f64>) = if red { (st.iter().map(|s| s.red_quantity).collect(), st.iter().map(|s| s.red_changed_area_fraction).collect()) } else { (st.iter().map(|s| s.luminance).collect(), st.iter().map(|s| s.changed_area_fraction).collect()) };
    let tr = if red { transitions(&series, p.red_delta, f64::INFINITY) } else { transitions(&series, p.lum_delta, p.dark_max) };
    flash_rule(&times, &flash_spans(&times, &tr), &area, &starts, 1.0, limit, area_min)
}
const K: [u8; 3] = [0, 0, 0]; const W: [u8; 3] = [255, 255, 255]; const R: [u8; 3] = [255, 0, 0];

#[test] fn without_flash_limit_the_output_is_byte_identical_to_the_previous_build() {
    let pre: Vec<(String, String)> = serde_json::from_str(PRE).unwrap(); assert_eq!(pre.len(), 3);
    let mut s = timeline();
    for (args, want) in &pre { let a: Value = serde_json::from_str(args).unwrap(); assert_eq!(&serde_json::to_string(&c(&mut s, "flash_series", a)).unwrap(), want, "{args}"); }
    let mut a: Value = serde_json::from_str(&pre[0].0).unwrap(); a["flash_limit"] = Value::Null; assert_eq!(serde_json::to_string(&c(&mut s, "flash_series", a)).unwrap(), pre[0].1);
}
#[test] fn five_hz_full_screen_square_wave_at_60_fps() {
    // toggles every 6 frames: transitions at frames 6, 12, ..., 114 (19); flashes = pairs (6,12), (18,24), ..., (102,108) = 9,
    // the last transition unpaired. A window [s, s+1) holds every flash with start < s+1 and end >= s: 5 at s = 0, and 6 from
    // s = 7/60 (the flash starting at 1.1 s enters while the one ending at 0.2 s is still inside) through s = 12/60.
    let frames: Vec<Image> = (0..120).map(|i| img(8, 6, |_, _| if (i / 6) % 2 == 0 { K } else { W })).collect();
    let g = rule_of(&frames, 60., 3., 0., false);
    assert_eq!(g.counted, 9); assert_eq!(g.worst_count, 6); assert!((g.worst_start.unwrap() - 7. / 60.).abs() < 1e-12, "{:?}", g.worst_start);
    assert_eq!(g.over, 61, "every one of the 61 windows holds more than 3"); assert_eq!(rule_of(&frames, 60., 5., 0., false).over, 24);
    assert!((g.area_max_pct - 100.).abs() < 1e-12);
    // black / white has no red: no red flash, no worst red window
    let r = rule_of(&frames, 60., 3., 0., true); assert_eq!((r.counted, r.worst_count, r.worst_start, r.over), (0, 0, None, 0));
    // the same wave in saturated red counts as red flashes too
    let red: Vec<Image> = (0..120).map(|i| img(8, 6, |_, _| if (i / 6) % 2 == 0 { K } else { R })).collect();
    let r = rule_of(&red, 60., 3., 0., true); assert_eq!((r.counted, r.worst_count, r.over), (9, 6, 61)); assert!((r.area_max_pct - 100.).abs() < 1e-12);
}
#[test] fn area_min_pct_leaves_out_small_flashes() {
    // only the left half of an 8x6 frame flashes: area 50 %
    let frames: Vec<Image> = (0..120).map(|i| img(8, 6, |x, _| if x < 4 && (i / 6) % 2 == 1 { W } else { K })).collect();
    let g = rule_of(&frames, 60., 3., 0., false); assert!((g.area_max_pct - 50.).abs() < 1e-12); assert_eq!(g.counted, 9);
    let g50 = rule_of(&frames, 60., 3., 50., false); assert_eq!(g50.counted, 9);
    let g60 = rule_of(&frames, 60., 3., 60., false); assert_eq!(g60, FlashRule { worst_start: None, worst_count: 0, over: 0, area_max_pct: 0., counted: 0 });
}
#[test] fn engine_timeline_rule_numbers_by_hand() {
    // 3 frames per clip at 24 fps. Luminance: black/white swings (1.0) then black/red (0.2126 >= lum_delta .1, darker 0 < .8),
    // a transition every 3 frames from frame 3 to 69 = 23 -> 11 flashes. Worst 1 s window: 5 flashes from s = 4/24
    // ([0.1667, 1.1667) touches the flashes starting 0.125 .. 1.125); 49 windows (starts 0 .. 2 s), every one over 3.
    // Red: red quantity 320 on the red clips (frames 51-53, 57-59, 63-65, 69-71): transitions at 51, 54, ..., 69 = 7 -> 3
    // flashes; worst 3 from s = 40/24; no window over 3.
    let mut s = timeline();
    let r = c(&mut s, "flash_series", json!({"flash_limit": 3}));
    assert_eq!(r["flash_limit"], json!(3.0)); assert_eq!(r["area_min_pct"], json!(0.0));
    assert_eq!((r["flashes_counted"].clone(), r["worst_window_flashes"].clone(), r["windows_over_limit"].clone()), (json!(11), json!(5), json!(49)));
    assert!((r["worst_window_start"].as_f64().unwrap() - 4. / 24.).abs() < 1e-6 && (r["worst_window_end"].as_f64().unwrap() - 28. / 24.).abs() < 1e-6, "{}", r["worst_window_start"]);
    assert_eq!((r["red_flashes_counted"].clone(), r["worst_red_window_flashes"].clone(), r["red_windows_over_limit"].clone()), (json!(3), json!(3), json!(0)));
    assert!((r["worst_red_window_start"].as_f64().unwrap() - 40. / 24.).abs() < 1e-6);
    assert_eq!(r["flash_area_pct_max"], json!(100.0)); assert_eq!(r["red_flash_area_pct_max"], json!(100.0));
    // the rule numbers agree with the per-window lists the tool already returns
    assert_eq!(r["worst_window_flashes"], r["totals"]["max_lum_flashes_in_window"]); assert_eq!(r["worst_red_window_flashes"], r["totals"]["max_red_flashes_in_window"]);
    // a region of the frame and a stricter limit
    let q = c(&mut s, "flash_series", json!({"flash_limit": 4.5, "area_min_pct": 100, "region": [0, 0, 32, 36]}));
    // windows hold 4 or 5 flashes (4 at s = 0..3/24, 5 at 4..6/24, repeating every 6/24, 4 in the last six): 21 hold more than 4.5
    assert_eq!(q["windows_over_limit"], json!(21)); assert_eq!(q["flashes_counted"], json!(11));
    let q = c(&mut s, "flash_series", json!({"flash_limit": 5}));
    assert_eq!(q["windows_over_limit"], json!(0));
}
#[test] fn bad_rule_arguments_are_refused_never_clamped() {
    let mut s = timeline();
    for (a, frag) in [
        (json!({"flash_limit": 0}), "flash_limit must be"), (json!({"flash_limit": -3}), "flash_limit must be"), (json!({"flash_limit": 5000}), "flash_limit must be"), (json!({"flash_limit": "3"}), "flash_limit must be"),
        (json!({"area_min_pct": 25}), "needs flash_limit"), (json!({"flash_limit": 3, "area_min_pct": 101}), "area_min_pct must be"), (json!({"flash_limit": 3, "area_min_pct": -1}), "area_min_pct must be"), (json!({"flash_limit": 3, "area_min_pct": "25"}), "area_min_pct must be"),
    ] { let e = dispatch(&mut s, "flash_series", &a).unwrap_err(); assert!(e.contains(frag), "{a}: {e}"); }
}
