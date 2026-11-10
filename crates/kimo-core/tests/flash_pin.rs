//! v6 eyes: flash_series pinned to the WCAG 2.x general-flash and red-flash definitions (Naman, 2026-10-18 3:10 AM).
//! - Window: 1 s windows start at EVERY sample (window_hop_s defaults to one frame), so a burst that straddles a second
//!   boundary is counted together; whole-second windows would split it (pinned below).
//! - Flash: one pair of opposing transitions (rise then fall, or fall then rise), each a swing of >= 0.1 relative luminance
//!   (10 % of the max, 1.0) with the darker state < 0.80; red: the same pairing on saturated-red transitions (R/(R+G+B) >= 0.8,
//!   a (R-G-B) x 320 change >= 20). A flash counts in every 1 s window its span touches; more than flash_limit (3) in any
//!   window = windows_over_limit > 0.
//! - Area (area_basis field): the largest share of any 10 degree field whose pixels changed, the field being 341 x 256 px at
//!   1024 x 768 scaled per axis to the analysed frame; a flash counts only when that share is MORE than 25 %.
//!   Fields slide in steps of 1/8 field per axis (Naman 3:45 AM: the field can sit anywhere), so any patch up to 7/8 of a field
//!   per axis lies wholly inside one field wherever it sits, grid corners and tile boundaries included.
//! Fixtures: full-frame 4 Hz strobe fails, full-frame 2 Hz strobe passes, a 4 Hz strobe on 2.6 % of the frame (a box under
//! 25 % of the field) passes, also centred on a corner of the old half-field grid, where a field-size strobe fails; a boundary
//! pair just under / just over 25 % of a field passes / fails; a compact strobe on 9 % of the frame fails.
//! Output with flash_limit and without area_basis is byte-identical to the previous commit.
use kimo_core::commands::{dispatch, Session};
use kimo_core::ext::flash_series::{field_px, field_rules, field_stats, field_tiles, flash_rule, flash_spans, measure, transitions, Params, Planes};
use kimo_core::perception::analysis::Image;
use serde_json::{json, Value};

const PRE: &str = include_str!("data/flash_series_pre_field.json");
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
/// 64x36 24 fps timeline of 0.125 s solid clips; `col(k)` picks clip k's colour
fn strobe(n: usize, dur: f64, col: impl Fn(usize) -> [f64; 3]) -> Session {
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    for k in 0..n { c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": col(k), "start": k as f64 * dur, "duration": dur})); }
    s
}
const K: [f64; 3] = [0., 0., 0.]; const W: [f64; 3] = [1., 1., 1.]; const R: [f64; 3] = [1., 0., 0.];
fn wcag(s: &mut Session) -> Value { c(s, "flash_series", json!({"flash_limit": 3, "area_basis": "field"})) }

#[test] fn flash_limit_output_without_area_basis_is_byte_identical_to_the_previous_commit() {
    let pre: Vec<(String, String)> = serde_json::from_str(PRE).unwrap(); assert_eq!(pre.len(), 3);
    let mut s = strobe(24, 0.125, |k| if k % 2 == 0 { K } else if k < 16 { W } else { R });
    for (args, want) in &pre { let a: Value = serde_json::from_str(args).unwrap(); assert_eq!(&serde_json::to_string(&c(&mut s, "flash_series", a)).unwrap(), want, "{args}"); }
}
#[test] fn field_size_is_341_by_256_at_1024_by_768_scaled() {
    assert_eq!(field_px(1024, 768), (341, 256)); assert_eq!(field_px(1920, 1080), (639, 360)); assert_eq!(field_px(1080, 1920), (360, 640)); assert_eq!(field_px(64, 36), (21, 12));
}
#[test] fn full_frame_4_hz_strobe_fails() {
    // 0.125 s black / white: a flash every 0.25 s = 4 per second; every full window holds 4 or 5 (a flash in every window it touches)
    let mut s = strobe(24, 0.125, |k| if k % 2 == 0 { K } else { W }); let r = wcag(&mut s);
    assert!(r["worst_window_flashes"].as_u64().unwrap() >= 4, "{r}"); assert!(r["windows_over_limit"].as_u64().unwrap() > 0, "{r}");
    assert_eq!(r["flash_area_pct_max"], 100.0); assert_eq!(r["area_basis"], "field"); assert_eq!(r["field_px"], json!([21, 12])); assert_eq!(r["area_min_pct"], 25.0);
    // the same pattern in saturated red fails the red rule
    let mut s = strobe(24, 0.125, |k| if k % 2 == 0 { K } else { R }); let r = wcag(&mut s);
    assert!(r["worst_red_window_flashes"].as_u64().unwrap() >= 4 && r["red_windows_over_limit"].as_u64().unwrap() > 0, "{r}");
}
#[test] fn full_frame_2_hz_strobe_passes() {
    // 0.25 s black / white: a flash every 0.5 s; any 1 s window touches at most 3
    let mut s = strobe(12, 0.25, |k| if k % 2 == 0 { K } else { W }); let r = wcag(&mut s);
    assert!(r["flashes_counted"].as_u64().unwrap() >= 5, "{r}"); assert_eq!(r["worst_window_flashes"], 3, "{r}"); assert_eq!(r["windows_over_limit"], 0, "{r}");
    let mut s = strobe(12, 0.25, |k| if k % 2 == 0 { K } else { R }); let r = wcag(&mut s); assert_eq!(r["red_windows_over_limit"], 0, "{r}");
}
#[test] fn a_burst_across_a_second_boundary_is_counted_together() {
    // white pulses at [0.625,0.75) [0.875,1.0) [1.125,1.25) [1.375,1.5): 4 flashes in 0.875 s, 2 before and 2 after t = 1 s
    let mut s = strobe(24, 0.125, |k| if [5, 7, 9, 11].contains(&k) { W } else { K }); let r = wcag(&mut s);
    assert_eq!(r["worst_window_flashes"], 4, "{r}"); assert!(r["windows_over_limit"].as_u64().unwrap() > 0);
    let ws = r["worst_window_start"].as_f64().unwrap(); assert!(ws > 0.375 && ws <= 0.625, "{ws}");
    // whole-second windows only (hop 1 s) see 2 + 3 and never more than 3: the sliding window is what catches it
    let r1 = c(&mut s, "flash_series", json!({"flash_limit": 3, "area_basis": "field", "window_hop_s": 1.0}));
    assert!(r1["worst_window_flashes"].as_u64().unwrap() <= 3 && r1["windows_over_limit"] == 0, "{r1}");
}
fn img(w: usize, h: usize, on: bool, bx: [usize; 4]) -> Image {
    let mut im = Image::new(w, h);
    for y in 0..h { for x in 0..w { let v = if on && x >= bx[0] && x < bx[0] + bx[2] && y >= bx[1] && y < bx[1] + bx[3] { 1.0 } else { 0.0 }; for ch in 0..3 { im.rgb[ch][y * w + x] = v; } } }
    im
}
/// flash_series' own paths on synthetic frames: field basis = field_tiles -> field_stats -> field_rules (strict area);
/// frame basis = measure -> transitions -> spans -> flash_rule. Returns (worst window flashes, windows over 3, area max %, field).
fn rule(w: usize, h: usize, bx: [usize; 4], half_period: usize, n: usize, basis_field: bool, area_min: f64) -> (usize, usize, f64, [usize; 4]) {
    rule_on(w, h, bx, half_period, n, basis_field, area_min, field_tiles(w, h))
}
/// The previous commit's field positions: a half-field grid (steps fw/2, fh/2, the last flush with the edge), to show what it missed.
fn half_grid(w: usize, h: usize) -> Vec<[usize; 4]> {
    let (fw, fh) = field_px(w, h);
    let pos = |v: usize, f: usize| { let mut o: Vec<usize> = (0..=v - f).step_by((f / 2).max(1)).collect(); if *o.last().unwrap() != v - f { o.push(v - f); } o };
    let mut out = vec![]; for y in pos(h, fh) { for x in pos(w, fw) { out.push([x, y, fw, fh]); } } out
}
fn rule_on(w: usize, h: usize, bx: [usize; 4], half_period: usize, n: usize, basis_field: bool, area_min: f64, tiles: Vec<[usize; 4]>) -> (usize, usize, f64, [usize; 4]) {
    let p = Params::default(); let mut prev: Option<Planes> = None; let (mut lum, mut area, mut ser) = (vec![], vec![], vec![]);
    for i in 0..n { let im = img(w, h, (i / half_period) % 2 == 1, bx); let (st, pl) = measure(&im, prev.as_ref(), &p);
        ser.push(field_stats(prev.as_ref(), &pl, w, &tiles, &p)); lum.push(st.luminance); area.push(st.changed_area_fraction); prev = Some(pl); }
    let times: Vec<f64> = (0..n).map(|i| i as f64 / 24.).collect(); let dur = n as f64 / 24.;
    let starts: Vec<f64> = times.iter().copied().filter(|s| s + 1.0 <= dur + 1e-9).collect();
    if basis_field { let (g, gi, _, _) = field_rules(&times, &ser, &starts, 1.0, 3., area_min, &p); (g.worst_count, g.over, g.area_max_pct, tiles[gi]) }
    else { let g = flash_rule(&times, &flash_spans(&times, &transitions(&lum, p.lum_delta, p.dark_max)), &area, &starts, 1.0, 3., area_min); (g.worst_count, g.over, g.area_max_pct, [0, 0, w, h]) }
}
#[test] fn four_hz_strobe_on_under_10_pct_of_the_frame_passes() {
    // 256x192 frame, field 85x64 = 5440 px, fields every 10 x 8 px plus one flush with each edge. A 36x36 box (1296 px = 2.6 % of the frame, 23.8 % of
    // the field that holds it) strobing at 4 Hz (24 fps, toggling every 3 frames): under 25 % of any field, so nothing counts.
    assert_eq!(field_px(256, 192), (85, 64)); assert_eq!(field_tiles(256, 192).len(), 19 * 17);
    assert_eq!(rule(256, 192, [100, 70, 36, 36], 3, 72, true, 25.).0, 0);
    let r = rule(256, 192, [100, 70, 36, 36], 3, 72, true, 25.); assert_eq!((r.0, r.1, r.2), (0, 0, 0.));
    // with no area floor the same strobe counts in the field that holds it: it is the area rule that passes it
    let (w0, o0, a0, f0) = rule(256, 192, [100, 70, 36, 36], 3, 72, true, 0.); assert!(w0 >= 4 && o0 > 0 && (a0 - 1296. / 5440. * 100.).abs() < 1e-9, "{w0} {o0} {a0}"); assert_eq!(f0, [60, 48, 85, 64]);
    // stated plainly: WCAG's field is local. A compact 77x58 box is 9.1 % of the frame but 82 % of the field it sits in: it
    // FAILS on the field basis; the whole-frame reading misses it (its frame-mean swing, 0.09, is under the 0.1 luminance step)
    let (wf, of, af, _) = rule(256, 192, [90, 66, 77, 58], 3, 72, true, 25.); assert!(wf >= 4 && of > 0 && (af - 4466. / 5440. * 100.).abs() < 1e-9, "{wf} {of} {af}");
    let (wr, or, _, _) = rule(256, 192, [90, 66, 77, 58], 3, 72, false, 0.); assert_eq!((wr, or), (0, 0));
}
#[test] fn fields_slide_so_every_position_of_a_patch_up_to_7_8_field_is_inside_one() {
    let t = field_tiles(1920, 1080); assert!(t.iter().all(|q| q[2] == 639 && q[3] == 360 && q[0] + q[2] <= 1920 && q[1] + q[3] <= 1080));
    assert!(t.iter().any(|q| q[0] + q[2] == 1920) && t.iter().any(|q| q[1] + q[3] == 1080) && t.iter().any(|q| q[0] == 0 && q[1] == 0));
    assert_eq!(t.len(), 18 * 17); // x every 79 px (639/8) + the edge one, y every 45 px (360/8)
    // brute force at 256x192 (field 85x64): a 74x56 patch (7/8 of the field per axis) at EVERY position lies wholly inside some field
    let t = field_tiles(256, 192);
    for y in 0..=192 - 56 { for x in 0..=256 - 74 { assert!(t.iter().any(|q| q[0] <= x && x + 74 <= q[0] + q[2] && q[1] <= y && y + 56 <= q[1] + q[3]), "patch at {x},{y} split"); } }
    // the old half-field grid split such patches (e.g. a 72 px wide one at x = 70)
    let hg = half_grid(256, 192); assert!(!hg.iter().any(|q| q[0] <= 70 && 70 + 72 <= q[0] + q[2]));
}
#[test] fn strobe_centred_on_a_grid_corner_behaves_like_an_uncentred_one() {
    // (126, 96) is a corner of the old half-field grid (x 0/42/84/126/168, y 0/32/64/96/128). A 36x36 box (2.6 % of the frame)
    // centred on it is seen whole (23.8 % of a field, same as uncentred) and passes; with no area floor it counts, same numbers.
    let r = rule(256, 192, [108, 78, 36, 36], 3, 72, true, 25.); assert_eq!((r.0, r.1, r.2), (0, 0, 0.));
    let (w0, o0, a0, f0) = rule(256, 192, [108, 78, 36, 36], 3, 72, true, 0.); let (wu, ou, au, _) = rule(256, 192, [100, 70, 36, 36], 3, 72, true, 0.);
    assert_eq!((w0, o0), (wu, ou)); assert!((a0 - 1296. / 5440. * 100.).abs() < 1e-9 && (a0 - au).abs() < 1e-12, "{a0} {au}");
    assert!(f0[0] <= 108 && 144 <= f0[0] + 85 && f0[1] <= 78 && 114 <= f0[1] + 64, "{f0:?}");
    // a field-size strobe (85x64) centred on the same corner fails
    let (wf, of, af, _) = rule(256, 192, [84, 64, 85, 64], 3, 72, true, 25.); assert!(wf >= 4 && of > 0 && af > 25., "{wf} {of} {af}");
}
#[test] fn boundary_pair_just_under_and_just_over_25_pct_of_a_field() {
    // 72x18 = 1296 px = 23.82 % of the 85x64 field: passes; 72x19 = 1368 px = 25.15 %: fails, wherever it sits
    for x in [70usize, 73, 84, 126, 150] {
        let r = rule(256, 192, [x, 100, 72, 18], 3, 72, true, 25.); assert_eq!((r.0, r.1, r.2), (0, 0, 0.), "under at x={x}");
        let (wf, of, af, _) = rule(256, 192, [x, 100, 72, 19], 3, 72, true, 25.);
        assert!(wf >= 4 && of > 0 && (af - 1368. / 5440. * 100.).abs() < 1e-9, "over at x={x}: {wf} {of} {af}");
    }
    // the previous half-field grid at x = 70 saw at most 58 of its 72 columns in one field (20.3 %) and passed it
    let (wh, oh, _, _) = rule_on(256, 192, [70, 100, 72, 19], 3, 72, true, 25., half_grid(256, 192)); assert_eq!((wh, oh), (0, 0));
    let (_, _, ah, _) = rule_on(256, 192, [70, 100, 72, 19], 3, 72, true, 0., half_grid(256, 192)); assert!((ah - 58. * 19. / 5440. * 100.).abs() < 1e-9, "{ah}");
}
#[test] fn compact_4_hz_strobe_on_9_pct_of_the_frame_fails() {
    // 4:3 256x192: a 77x58 box (4466 px = 9.1 % of the frame, 82 % of its 85x64 field) fails on the field basis
    let (wf, of, af, _) = rule(256, 192, [90, 66, 77, 58], 3, 72, true, 25.); assert!(wf >= 4 && of > 0 && (af - 4466. / 5440. * 100.).abs() < 1e-9, "{wf} {of} {af}");
    // 16:9 256x144 (field 85x48): a 79x42 box (3318 px = 9.0 % of the frame, 81 % of the field), straddling the frame centre, fails
    assert_eq!(field_px(256, 144), (85, 48));
    let (wg, og, ag, _) = rule(256, 144, [90, 51, 79, 42], 3, 72, true, 25.); assert!(wg >= 4 && og > 0 && (ag - 3318. / 4080. * 100.).abs() < 1e-9, "{wg} {og} {ag}");
}
#[test] fn refused() {
    let mut s = strobe(8, 0.125, |k| if k % 2 == 0 { K } else { W });
    for a in [json!({"area_basis": "field"}), json!({"flash_limit": 3, "area_basis": "screen"}), json!({"flash_limit": 3, "area_basis": 1})] { assert!(dispatch(&mut s, "flash_series", &a).is_err(), "accepted {a}"); }
    // area_basis frame is accepted and reports itself
    assert_eq!(c(&mut s, "flash_series", json!({"flash_limit": 3, "area_basis": "frame"}))["area_basis"], "frame");
}
