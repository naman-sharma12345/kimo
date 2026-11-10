//! v6 eyes: flash_series reference values. Synthetic fixtures with analytic counts, plus one engine render through dispatch.
use kimo_core::commands::{dispatch, dispatch_ex, Session};
use kimo_core::ext::flash_series::{flash_spans, measure, srgb_to_linear, transitions, window_counts, window_flash_counts, window_flash_counts_inside, FrameStats, Params, Planes};
use kimo_core::perception::analysis::Image;
use serde_json::{json, Value};

fn solid(w: usize, h: usize, rgb: [u8; 3]) -> Image {
    let mut im = Image::new(w, h);
    for c in 0..3 { for i in 0..w * h { im.rgb[c][i] = rgb[c] as f64 / 255.; } }
    im
}
/// Run a frame sequence through measure / transitions / window_counts the way the tool does.
fn series(frames: &[Image], fps: f64, window: f64, p: &Params) -> (Vec<FrameStats>, Vec<i8>, Vec<i8>, Vec<usize>, Vec<usize>) {
    let mut prev: Option<Planes> = None; let mut st = vec![];
    for f in frames { let (s, pl) = measure(f, prev.as_ref(), p); st.push(s); prev = Some(pl); }
    let lum: Vec<f64> = st.iter().map(|s| s.luminance).collect(); let rq: Vec<f64> = st.iter().map(|s| s.red_quantity).collect();
    let lt = transitions(&lum, p.lum_delta, p.dark_max); let rt = transitions(&rq, p.red_delta, f64::INFINITY);
    let times: Vec<f64> = (0..frames.len()).map(|i| i as f64 / fps).collect();
    let dur = frames.len() as f64 / fps;
    let starts: Vec<f64> = (0..frames.len()).map(|i| i as f64 / fps).filter(|s| s + window <= dur + 1e-9).collect();
    let wl = window_counts(&times, &lt, &starts, window); let wr = window_counts(&times, &rt, &starts, window);
    (st, lt, rt, wl, wr)
}
/// Square-wave flash at `hz` full cycles per second sampled at `fps`, `secs` long, toggling between a and b.
fn flash(hz: f64, fps: f64, secs: f64, a: [u8; 3], b: [u8; 3]) -> Vec<Image> {
    let half = (fps / (2. * hz)).round() as usize; assert_eq!(half as f64 * 2. * hz, fps, "fixture needs an integer half period");
    (0..(fps * secs) as usize).map(|i| solid(8, 6, if (i / half) % 2 == 0 { a } else { b })).collect()
}
const K: [u8; 3] = [0, 0, 0]; const W: [u8; 3] = [255, 255, 255]; const R: [u8; 3] = [255, 0, 0];

#[test] fn linear_luminance_reference_values() {
    let p = Params::default();
    let y = |c: [u8; 3]| measure(&solid(4, 4, c), None, &p).0.luminance;
    assert_eq!(y(K), 0.0); assert!((y(W) - 1.0).abs() < 1e-12);
    assert!((y(R) - 0.2126).abs() < 1e-12); assert!((y([0, 255, 0]) - 0.7152).abs() < 1e-12); assert!((y([0, 0, 255]) - 0.0722).abs() < 1e-12);
    // sRGB 128 decodes to 0.21586 linear (IEC 61966-2-1), not 0.502: the series is linear light
    assert!((srgb_to_linear(128. / 255.) - 0.215861).abs() < 1e-6); assert!((y([128, 128, 128]) - 0.215861).abs() < 1e-6);
    assert!((srgb_to_linear(10. / 255.) - 10. / 255. / 12.92).abs() < 1e-15, "linear segment below 0.04045");
}
#[test] fn bw_flash_3_5_10_hz_counts_match_analytically() {
    let fps = 60.; let p = Params::default();
    for hz in [3., 5., 10.] {
        let half = (fps / (2. * hz)) as usize; let frames = flash(hz, fps, 2., K, W);
        let (st, lt, rt, wl, _) = series(&frames, fps, 1.0, &p);
        // toggles at half, 2*half, ... < 120; each is one transition on the toggle frame, alternating rise / fall
        let want: Vec<usize> = (1..).map(|k| k * half).take_while(|&i| i < frames.len()).collect();
        let got: Vec<usize> = lt.iter().enumerate().filter(|(_, x)| **x != 0).map(|(i, _)| i).collect();
        assert_eq!(got, want, "{hz} Hz transition frames");
        for (j, i) in got.iter().enumerate() { assert_eq!(lt[*i], if j % 2 == 0 { 1 } else { -1 }, "{hz} Hz direction"); }
        // every 1 s window holds exactly 2*hz transitions, except the window at t=0 (frame 0 is a starting state, not a change)
        assert_eq!(wl.len(), 61); assert_eq!(wl[0], 2 * hz as usize - 1, "{hz} Hz window at 0");
        assert!(wl[1..].iter().all(|&c| c == 2 * hz as usize), "{hz} Hz windows {wl:?}");
        assert!(rt.iter().all(|&x| x == 0), "black/white has no saturated red");
        for i in 0..frames.len() { let want = if want.contains(&i) { 1.0 } else { 0.0 }; assert_eq!(st[i].changed_area_fraction, want, "{hz} Hz area at {i}"); }
    }
}
#[test] fn hz_math_holds_at_24_and_25_fps() {
    // 3 Hz at 24 fps (toggle every 4 frames) and 6 Hz (every 2); 2.5 Hz at 25 fps (every 5) and 12.5 Hz (every frame)
    let p = Params::default();
    for (fps, hz) in [(24., 3.), (24., 6.), (25., 2.5), (25., 12.5)] {
        let frames = flash(hz, fps, 2., K, W); let (_, lt, _, wl, _) = series(&frames, fps, 1.0, &p);
        let per = (2. * hz) as usize; let half = (fps / (2. * hz)) as usize;
        assert_eq!(lt.iter().filter(|x| **x != 0).count(), (frames.len() - 1) / half, "{fps} fps {hz} Hz total");
        assert_eq!(wl.len(), fps as usize + 1); assert_eq!(wl[0], per - 1); assert!(wl[1..].iter().all(|&c| c == per), "{fps} fps {hz} Hz windows {wl:?}");
    }
}
#[test] fn flash_straddling_a_window_edge_counts_in_each_window() {
    // 60 fps, 2 s, non-overlapping 1 s windows [0,1) and [1,2). One flash: white from frame 55 to 64 (rise at 55, fall at 65)
    let p = Params::default(); let fps = 60.;
    let frames: Vec<Image> = (0..120).map(|i| solid(4, 4, if (55..65).contains(&i) { W } else { K })).collect();
    let (st, lt, ..) = series(&frames, fps, 1.0, &p); let _ = st;
    let times: Vec<f64> = (0..120).map(|i| i as f64 / fps).collect();
    let spans = flash_spans(&times, &lt); assert_eq!(spans.len(), 1); assert!((spans[0].0 - 55. / 60.).abs() < 1e-12 && (spans[0].1 - 65. / 60.).abs() < 1e-12);
    let starts = [0.0, 1.0];
    assert_eq!(window_counts(&times, &lt, &starts, 1.0), vec![1, 1], "one transition on each side of the edge");
    assert_eq!(window_flash_counts(&spans, &starts, 1.0), vec![1, 1], "the straddling flash counts once in EACH window");
    assert_eq!(window_flash_counts_inside(&spans, &starts, 1.0), vec![0, 0], "fully-inside sibling: the straddling flash is inside neither window");
    // the same flash moved fully inside the first window counts only there
    let inside: Vec<Image> = (0..120).map(|i| solid(4, 4, if (20..30).contains(&i) { W } else { K })).collect();
    let (_, lt2, ..) = series(&inside, fps, 1.0, &p); let sp2 = flash_spans(&times, &lt2);
    assert_eq!(window_flash_counts(&sp2, &starts, 1.0), vec![1, 0]); assert_eq!(window_flash_counts_inside(&sp2, &starts, 1.0), vec![1, 0]);
    // fully inside means both transitions in [start, start+window): ending exactly on the window end is outside
    assert_eq!(window_flash_counts_inside(&[(0.0, 0.5), (0.5, 1.0)], &starts, 1.0), vec![1, 0]);
    // a flash ending exactly on a window's start touches it; one ending just before does not
    assert_eq!(window_flash_counts(&[(0.5, 1.0)], &starts, 1.0), vec![1, 1]); assert_eq!(window_flash_counts(&[(0.5, 59. / 60.)], &starts, 1.0), vec![1, 0]);
    // pairing: 6 alternating transitions are 3 flashes, a 7th unpaired one is no flash
    let ev = [1i8, -1, 1, -1, 1, -1, 1]; let tt: Vec<f64> = (0..7).map(|i| i as f64).collect(); assert_eq!(flash_spans(&tt, &ev).len(), 3);
}
#[test] fn real_clip_shape_touching_vs_fully_inside_and_dark_first_pair() {
    // the shape of the real render at 24 fps: 1 s grey (sRGB 128, Y .2159), 1 s black/white toggling every 3 frames from black,
    // 1 s black/red the same way, 0.5 s black; windows of 1 s every frame
    let p = Params::default(); let fps = 24.; let g = [128u8, 128, 128];
    let frames: Vec<Image> = (0..84).map(|i| solid(4, 4, match i { 0..=23 => g, 24..=47 => if ((i - 24) / 3) % 2 == 0 { K } else { W }, 48..=71 => if ((i - 48) / 3) % 2 == 0 { K } else { R }, _ => K })).collect();
    let (_, lt, rt, _, _) = series(&frames, fps, 1.0, &p);
    let times: Vec<f64> = (0..84).map(|i| i as f64 / fps).collect();
    let on: Vec<usize> = lt.iter().enumerate().filter(|(_, x)| **x != 0).map(|(i, _)| i).collect();
    assert_eq!(on, (0..17).map(|k| 24 + 3 * k).collect::<Vec<_>>(), "17 luminance transitions, f24 to f72");
    // dark-first pair: grey -> black at f24 is a fall (-1, a .2159 swing with the darker end 0 < dark_max), paired with the rise to white at f27
    assert_eq!((lt[24], lt[27]), (-1, 1)); let fl = flash_spans(&times, &lt);
    assert_eq!(fl.len(), 8, "the f72 trailing transition is unpaired"); assert!((fl[0].0 - 1.0).abs() < 1e-12 && (fl[0].1 - 27. / 24.).abs() < 1e-12, "{:?}", fl[0]);
    assert_eq!(flash_spans(&times, &rt).len(), 4, "the red segment is 4 red flashes and also 4 of the 8 luminance flashes");
    let starts: Vec<f64> = (0..=60).map(|i| i as f64 / fps).collect();
    let touch = window_flash_counts(&fl, &starts, 1.0); let inside = window_flash_counts_inside(&fl, &starts, 1.0);
    assert_eq!(touch.iter().max(), Some(&5), "touch rule: a window can touch 5 flashes"); assert_eq!(inside.iter().max(), Some(&4), "fully inside: at most 4");
    assert_eq!((touch[27], inside[27]), (5, 3), "window [27/24, 51/24): touches (24,27) .. (48,51); wholly holds only (30,33), (36,39), (42,45)");
    assert_eq!((touch[24], inside[24]), (4, 4), "window [1 s, 2 s): (24,27) .. (42,45) both ways");
    assert!(touch.iter().zip(&inside).all(|(t, i)| i <= t));
}
/// linear -> sRGB (IEC 61966-2-1), rounded to 8 bits like a rendered frame
fn enc(v: f64) -> f64 { let s = if v <= 0.0031308 { 12.92 * v } else { 1.055 * v.powf(1. / 2.4) - 0.055 }; (s * 255.).round() / 255. }
#[test] fn slow_fade_is_stamped_at_the_first_crossing() {
    // 24 fps: 1 s black, a 3 s linear fade to white (frames 24..=96, Y = (i-24)/72 in linear light), 1 s white with a tiny late
    // drift up (the extreme is only reached at frame 115), then 3 s fade back down. Stamps land on the first crossing of
    // lum_delta: rise at the first frame with Y >= .1 (frame 32: 8/72 = .111), fall at the first frame .1 below the peak
    let mut y: Vec<f64> = (0..24).map(|_| 0.).collect();
    y.extend((24..=96).map(|i| (i - 24) as f64 / 72.)); y.extend((97..120).map(|i| if i < 115 { 0.995 } else { 1.0 }));
    y.extend((1..=72).map(|k| 1.0 - k as f64 / 72.));
    let lt = transitions(&y, 0.1, 0.8); let on: Vec<(usize, i8)> = lt.iter().enumerate().filter(|(_, x)| **x != 0).map(|(i, x)| (i, *x)).collect();
    let first_rise = y.iter().position(|v| *v >= 0.1).unwrap(); let peak_i = 115usize;
    let first_fall = (peak_i..y.len()).find(|&i| y[i] <= 1.0 - 0.1).unwrap();
    assert_eq!(on.len(), 2, "{on:?}"); assert_eq!(on[0].1, 1); assert_eq!(on[1].1, -1);
    assert!((on[0].0 as i64 - first_rise as i64).abs() <= 1, "rise stamped at {} (first crossing {first_rise})", on[0].0);
    assert!((on[1].0 as i64 - first_fall as i64).abs() <= 1, "fall stamped at {} (first crossing {first_fall})", on[1].0);
    assert!(on[0].0 < 96 && on[1].0 > peak_i, "not at the fade's end (96) or the late extreme");
    // the same through frames of the engine's measure path, as images
    let p = Params::default(); let frames: Vec<Image> = y.iter().map(|v| { let mut im = Image::new(4, 4); let e = enc(*v); for c in 0..3 { for i in 0..16 { im.rgb[c][i] = e; } } im }).collect();
    let (_, lt2, ..) = series(&frames, 24., 1.0, &p); let on2: Vec<usize> = lt2.iter().enumerate().filter(|(_, x)| **x != 0).map(|(i, _)| i).collect();
    assert_eq!(on2.len(), 2); assert!((on2[0] as i64 - first_rise as i64).abs() <= 1 && (on2[1] as i64 - first_fall as i64).abs() <= 1, "{on2:?}");
}
#[test] fn hard_cut_next_to_a_slow_ramp_in_one_window() {
    // 24 fps, everything inside 1 s (frames 24..48): a 0.5 s ramp, a short hold, then a hard cut at frame 40
    let on = |y: &[f64]| -> Vec<(usize, i8)> { transitions(y, 0.1, 0.8).iter().enumerate().filter(|(_, x)| **x != 0).map(|(i, x)| (i, *x)).collect() };
    let ramp = |a: f64, b: f64| -> Vec<f64> { (1..=12).map(|k| a + (b - a) * k as f64 / 12.).collect() };
    let build = |a: f64, b: f64, hold: &[f64], cut: f64| -> Vec<f64> { let mut y = vec![a; 24]; y.extend(ramp(a, b)); y.extend_from_slice(hold); y.extend(vec![cut; 72 - 24 - 12 - hold.len()]); y };
    // 1. ramp DOWN .6 -> .45, then cut UP to .95: two transitions, the fall on the ramp's own first crossing, the rise on the cut
    let y1 = build(0.6, 0.45, &[0.45; 4], 0.95); assert_eq!(on(&y1), vec![(32, -1), (40, 1)]);
    // 2. ramp up .2 -> .28 (under lum_delta), hold, cut to .9: one rise, on the cut frame; the ramp does not take the stamp
    let y2 = build(0.2, 0.28, &[0.28; 4], 0.9); assert_eq!(on(&y2), vec![(40, 1)]);
    // 3. ramp up .2 -> .45 (over lum_delta) then a hold with one tiny dip, cut to .95: still one rise, on the cut frame,
    //    because the monotonic run that reaches the extreme starts after the dip; rule (a) alone would stamp frame 28 in the ramp
    let y3 = build(0.2, 0.45, &[0.45, 0.449, 0.45, 0.45], 0.95); assert_eq!(on(&y3), vec![(40, 1)]);
    assert_eq!(y3.iter().position(|v| v - 0.2 >= 0.1), Some(28));
    // 4. documented limit (no tolerance argument): a ramp running straight into the cut with no reversal at all is one monotonic
    //    run, so the stamp stays on the ramp's first crossing
    let y4 = build(0.2, 0.45, &[], 0.95); assert_eq!(on(&y4), vec![(28, 1)]);
}
#[test] fn static_frame_gives_zero_transitions() {
    let p = Params::default(); let frames: Vec<Image> = (0..90).map(|_| solid(8, 6, [128, 128, 128])).collect();
    let (st, lt, rt, wl, wr) = series(&frames, 30., 1.0, &p);
    assert!(lt.iter().all(|&x| x == 0) && rt.iter().all(|&x| x == 0)); assert!(wl.iter().all(|&c| c == 0) && wr.iter().all(|&c| c == 0));
    assert!(st.iter().all(|s| s.changed_area_fraction == 0. && s.red_changed_area_fraction == 0.));
}
#[test] fn pure_red_flash_exercises_the_red_series() {
    let p = Params::default(); let frames = flash(5., 60., 2., K, R);
    let (st, lt, rt, wl, wr) = series(&frames, 60., 1.0, &p);
    assert_eq!(st[0].red_quantity, 0.0); assert_eq!(st[0].red_saturation, 0.0, "black is under red_floor");
    assert!((st[6].red_quantity - 320.0).abs() < 1e-9); assert!((st[6].red_saturation - 1.0).abs() < 1e-12);
    assert!((st[6].red_changed_area_fraction - 1.0).abs() < 1e-12 && st[7].red_changed_area_fraction == 0.);
    assert_eq!(rt.iter().filter(|x| **x != 0).count(), 19); assert!(wr[1..].iter().all(|&c| c == 10), "{wr:?}");
    // red at Y .2126 vs black is also a .2126 luminance swing with the dark state 0
    assert_eq!(lt, rt); assert_eq!(wl, wr);
    // red vs white: white is R/(R+G+B)=1/3, not saturated, so the red quantity goes 320 -> 0 and still counts
    let (s2, _, rt2, _, _) = series(&flash(5., 60., 1., W, R), 60., 1.0, &p);
    assert!((s2[0].red_saturation - 1. / 3.).abs() < 1e-12 && s2[0].red_quantity == 0.); assert_eq!(rt2.iter().filter(|x| **x != 0).count(), 9);
}
#[test] fn thresholds_are_arguments_not_verdicts() {
    // light-grey flash: Y .8 <-> 1.0 is a .2 swing but the darker state is not below the default .8, so 0; raising dark_max counts it
    let frames = flash(3., 60., 1., [233, 233, 233], W);
    let (st, lt, ..) = series(&frames, 60., 1.0, &Params::default());
    assert!(st[0].luminance >= 0.8, "{}", st[0].luminance); assert!(lt.iter().all(|&x| x == 0));
    let (_, lt2, ..) = series(&frames, 60., 1.0, &Params { dark_max: 0.95, ..Params::default() });
    assert_eq!(lt2.iter().filter(|x| **x != 0).count(), 5);
    // a wiggle smaller than lum_delta never splits a swing: 0 -> .5 with a .05 dip in the middle is one rise
    // strict-run guard: the run that reaches the peak starts at the .25 dip, and moves .25 >= delta, so the stamp is its first
    // crossing (sample 3), not the swing's first crossing from 0 (sample 1)
    assert_eq!(transitions(&[0., 0.3, 0.25, 0.5, 0.5], 0.1, 0.8), vec![0, 0, 0, 1, 0]);
}
#[test] fn half_frame_flash_reports_half_the_area() {
    let p = Params::default(); let mut frames = vec![];
    for i in 0..20 { let mut im = solid(8, 6, K); if (i / 5) % 2 == 1 { for y in 0..6 { for x in 0..4 { for c in 0..3 { im.rgb[c][y * 8 + x] = 1.0; } } } } frames.push(im); }
    let (st, lt, ..) = series(&frames, 20., 1.0, &p);
    assert_eq!(st[5].changed_area_fraction, 0.5); assert_eq!(st[6].changed_area_fraction, 0.0); assert!((st[5].luminance - 0.5).abs() < 1e-12);
    assert_eq!(lt.iter().filter(|x| **x != 0).count(), 3);
}
fn c(s: &mut Session, n: &str, a: Value) -> Value { dispatch(s, n, &a).unwrap_or_else(|e| panic!("{n}: {e}")) }
#[test] fn engine_render_alternating_solid_clips() {
    // 24 fps, 2 s of black / white solids (colour arrays) of 0.125 s (3 frames) each = 4 Hz, then 1 s of black / red at the same rate,
    // rendered by the engine's own path
    let mut s = Session::new(); c(&mut s, "project_set", json!({"width": 64, "height": 36, "fps": 24}));
    let t = c(&mut s, "add_track", json!({}))["track_id"].clone();
    for k in 0..24 { let col = if k % 2 == 0 { [0., 0., 0.] } else if k < 16 { [1., 1., 1.] } else { [1., 0., 0.] }; c(&mut s, "add_clip", json!({"track_id": t, "source_type": "solid", "color": col, "start": k as f64 * 0.125, "duration": 0.125})); }
    let r = c(&mut s, "flash_series", json!({}));
    assert_eq!(r["samples"], 72); let lum = r["frames"]["luminance"].as_array().unwrap(); let rs = r["frames"]["red_saturation"].as_array().unwrap();
    assert!(lum[0].as_f64().unwrap() < 0.01 && lum[3].as_f64().unwrap() > 0.99, "{lum:?}");
    assert!((lum[51].as_f64().unwrap() - 0.2126).abs() < 0.01 && rs[51].as_f64().unwrap() > 0.99, "frame 51 is red: {} {}", lum[51], rs[51]);
    // luminance: toggles at frames 3,6,...,69 = 23; red quantity: off->red at 51, 57, 63, 69 and back at 54, 60, 66 = 7
    assert_eq!(r["totals"]["lum_transitions"], 23); assert_eq!(r["totals"]["red_transitions"], 7); assert_eq!(r["totals"]["red_flashes"], 3);
    assert_eq!(r["totals"]["max_lum_transitions_in_window"], 8); assert_eq!(r["params_used"]["max_resolvable_hz"], 12.0);
    assert_eq!(r["windows"]["lum_flashes_fully_inside"].as_array().unwrap().len(), 49); assert!(r["totals"]["max_lum_flashes_fully_inside_window"].as_u64().unwrap() <= r["totals"]["max_lum_flashes_in_window"].as_u64().unwrap());
    let w = r["windows"]["lum_transitions"].as_array().unwrap(); assert_eq!(w.len(), 49); assert_eq!(w[0], 7); assert!(w[1..].iter().all(|x| x == 8), "{w:?}");
    let txt = r.to_string(); assert!(!txt.contains("true") && !txt.contains("false"), "no boolean anywhere in the output");
    for k in ["pass", "fail", "verdict", "hazard", "ok", "exceeds"] { assert!(!txt.contains(&format!("\"{k}")), "no verdict field {k}"); }
    let half = c(&mut s, "flash_series", json!({"region": [0, 0, 32, 36], "step_frames": 2, "start": 0.5, "end": 1.5}));
    assert_eq!(half["samples"], 12); assert_eq!(half["analysed"]["w"], 32);
    assert_eq!(dispatch_ex(&mut s, "flash_series", &json!({"region": [60, 0, 10, 10]})).unwrap_err().code, "out_of_range");
    assert_eq!(dispatch_ex(&mut s, "flash_series", &json!({"source": "bad"})).unwrap_err().code, "invalid_source");
    assert_eq!(dispatch_ex(&mut s, "flash_series", &json!({"lum_delta": 0})).unwrap_err().code, "out_of_range");
}
