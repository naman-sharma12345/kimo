//! HUD / info-graphic one-call tools: detection boxes with label tags, number counters, typing prompt boxes.
use crate::commands::Session;
use crate::keyframe::{Ease, Prop};
use crate::model::*;
use serde_json::{json, Value};

fn clip(ss: &mut Session, source: Source, start: f64, dur: f64) -> Clip { Clip { id: ss.project.gen_id("clip"), source, start, duration: dur, in_point: 0.0, speed: 1.0, speed_curve: None, time_remap: None, speed_blend: Default::default(), props: default_props(), grade_graph: None, effects: vec![], blend: "normal".into(), mask: None, time_mode: TimeMode::Normal, corners: None, audio: Audio::default(), motion_blur: 0.0, echo: None, motion_path: None, posterize_fps: 0.0, text_fx: None, parent: None, inherit: Default::default(), name: None, role: None, tags: vec![], transition_in: None } }
fn num(a: &Value, k: &str, d: f64) -> f64 { a.get(k).and_then(|v| v.as_f64()).unwrap_or(d) }
fn rgb(a: &Value, k: &str, d: [f64; 3]) -> [f64; 3] { a.get(k).and_then(|v| v.as_array()).filter(|v| v.len() >= 3).map(|v| [v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0), v[2].as_f64().unwrap_or(0.0)]).unwrap_or(d) }
fn txt(text: &str, font: &str, size: f64, color: [f64; 3], align: &str) -> Source { Source::Text { text: text.into(), font: font.into(), size, color, letter_spacing: 0.0, stroke_width: 0.0, stroke_color: [0.0; 3], shadow: 0.0, align: align.into() } }
fn shape(kind: &str, w: f64, h: f64, fill: [f64; 3], sw: f64, sc: [f64; 3]) -> Source { Source::Shape { shape: kind.into(), width: w, height: h, fill, stroke_width: sw, stroke_color: sc, sides: 4, corner_radius: 0.0 } }
fn set(c: &mut Clip, p: &str, v: f64) { c.props.get_mut(p).unwrap().base = v; }
fn key(c: &mut Clip, p: &str, t: f64, v: f64, e: Ease) { c.props.get_mut(p).unwrap().set_key(t, v, e); }
fn push(ss: &mut Session, tid: &str, clips: Vec<Clip>) -> Result<Vec<String>, String> { let ids = clips.iter().map(|c| c.id.clone()).collect(); ss.project.tracks.iter_mut().find(|t| t.id == tid).ok_or("no such track")?.clips.extend(clips.into_iter().map(std::sync::Arc::new)); Ok(ids) }

/// Object-detection style overlay: hollow box + corner ticks + filled label tag with "label 0.92".
pub fn add_detection_box(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let tid = a.get("track_id").and_then(|v| v.as_str()).ok_or("missing track_id")?.to_string();
    let (start, dur) = (num(a, "start", 0.0), num(a, "duration", 3.0)); if dur <= 0.0 { return Err("duration must be > 0".into()); }
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64); let k = ch / 720.0;
    let (bx, by, bw, bh) = (num(a, "x", 0.3), num(a, "y", 0.3), num(a, "w", 0.3), num(a, "h", 0.4));
    if !(bw > 0.0 && bh > 0.0) { return Err("w and h must be > 0".into()); }
    let col = rgb(a, "color", [0.2, 0.9, 0.5]); let label = a.get("label").and_then(|v| v.as_str()).unwrap_or("object").to_string(); let conf = num(a, "confidence", 0.92);
    let font = a.get("font").and_then(|v| v.as_str()).unwrap_or("SpaceMono-Bold"); let fsz = 20.0 * k; let tag = format!("{} {:.2}", label, conf);
    let (pw, ph) = (bw * cw, bh * ch); let (cx, cy) = ((bx + bw / 2.0 - 0.5) * cw, (by + bh / 2.0 - 0.5) * ch);
    let tw = crate::text::measure(&tag, font, fsz, 0.0, 0.0, "left", cw).map(|v| v.0).unwrap_or(tag.chars().count() as f64 * fsz * 0.6) + 16.0 * k; let th = fsz * 1.5;
    let sw = (2.0 * k).max(1.5); let fill_alpha_box = num(a, "fill_opacity", 0.05);
    let imp0 = a.get("impact").and_then(|v| v.as_bool()).unwrap_or(false); let lag = if imp0 { 0.44 } else { 0.0 };
    let inst = a.get("instant").and_then(|v| v.as_bool()).unwrap_or(false); let e = Ease::OutExpo; let tin = if inst { 0.001 } else { 0.35 }; let mut cl = vec![];
    // soft fill
    let mut f = clip(ss, shape("rect", pw, ph, col, 0.0, col), start, dur); set(&mut f, "x", cx); set(&mut f, "y", cy); f.props.get_mut("opacity").unwrap().base = 0.0; key(&mut f, "opacity", 0.0, 0.0, e); key(&mut f, "opacity", tin, fill_alpha_box, e); key(&mut f, "opacity", dur - 0.15, fill_alpha_box, Ease::In); key(&mut f, "opacity", dur, 0.0, Ease::In); cl.push(f);
    // impact landing (measured from the SCUMBLE reference, 25 fps): box arrives oversized and shrinks in on an expo-out,
    // 4 lagging ghost outlines trail it (frame-delayed echo), one bright flash fill peaks at ~0.36 s and decays in 4 frames.
    if a.get("impact").and_then(|v| v.as_bool()).unwrap_or(false) {
        let s0 = num(a, "impact_scale", 3.0); let tl = 0.44; let flash = num(a, "flash", 0.8);
        if let Some(fc) = cl.get_mut(0) { let pr = fc.props.get_mut("scale").unwrap(); pr.base = 1.0; pr.set_key(0.0, s0, Ease::Out); pr.set_key(tl, 1.0, Ease::Out);
            let op = fc.props.get_mut("opacity").unwrap(); *op = Prop::new(0.0); op.set_key(0.0, 0.0, Ease::Linear); op.set_key(0.36, 0.2, Ease::Linear); op.set_key(0.44, flash, Ease::Linear); op.set_key(0.56, fill_alpha_box, Ease::Out); op.set_key(dur - 0.15, fill_alpha_box, Ease::In); op.set_key(dur, 0.0, Ease::In); fc.motion_blur = 0.0;
            if let Source::Shape { fill, .. } = &mut fc.source { *fill = [col[0] * 0.3 + 0.7, col[1] * 0.3 + 0.7, col[2] * 0.3 + 0.7]; } }
        for g in 1..=4 { let dl = g as f64 * 0.04; let mut o = clip(ss, shape("rect", pw, ph, [-1.0, 0.0, 0.0], sw, col), start + dl, tl + 0.1); set(&mut o, "x", cx); set(&mut o, "y", cy);
            let pr = o.props.get_mut("scale").unwrap(); pr.base = 1.0; pr.set_key(0.0, s0 * 0.97, Ease::Out); pr.set_key(tl - dl, 1.0, Ease::Out);
            key(&mut o, "opacity", 0.0, 0.75 / g as f64, Ease::Linear); key(&mut o, "opacity", tl - dl, 0.5 / g as f64, Ease::In); key(&mut o, "opacity", tl + 0.1 - dl, 0.0, Ease::In); cl.push(o); }
    }
    // outline
    let mut o = clip(ss, shape("rect", pw, ph, [-1.0, 0.0, 0.0], sw, col), start, dur); set(&mut o, "x", cx); set(&mut o, "y", cy);
    { let imp = a.get("impact").and_then(|v| v.as_bool()).unwrap_or(false); let (v0, tt) = if imp { (num(a, "impact_scale", 3.0), 0.44) } else { (1.12, tin) }; let ez = if imp { Ease::Out } else { e }; let pr = o.props.get_mut("scale").unwrap(); pr.base = 1.0; pr.set_key(0.0, v0, ez); pr.set_key(tt, 1.0, ez); }
    key(&mut o, "opacity", 0.0, 0.0, e); key(&mut o, "opacity", if inst { 0.001 } else { 0.15 }, 1.0, e); key(&mut o, "opacity", dur - 0.15, 1.0, Ease::In); key(&mut o, "opacity", dur, 0.0, Ease::In); o.motion_blur = 180.0; cl.push(o);
    // corner ticks (thick short bars at four corners)
    let tl = (pw.min(ph) * 0.14).clamp(8.0 * k, 36.0 * k); let tk = sw * 2.2;
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        for horiz in [true, false] {
            let (w, h) = if horiz { (tl, tk) } else { (tk, tl) };
            let mut t = clip(ss, shape("rect", w, h, col, 0.0, col), start + lag, dur - lag);
            set(&mut t, "x", cx + sx * (pw / 2.0 - if horiz { tl / 2.0 } else { tk / 2.0 })); set(&mut t, "y", cy + sy * (ph / 2.0 - if horiz { tk / 2.0 } else { tl / 2.0 }));
            key(&mut t, "opacity", 0.0, 0.0, e); key(&mut t, "opacity", if inst { 0.001 } else { 0.2 }, 1.0, e); key(&mut t, "opacity", dur - lag - 0.15, 1.0, Ease::In); key(&mut t, "opacity", dur - lag, 0.0, Ease::In); cl.push(t);
        }
    }
    // label tag: filled plate sitting on the top-left corner, then text; wipes open from the left
    let (tx, ty) = (cx - pw / 2.0 + tw / 2.0, cy - ph / 2.0 - th / 2.0 - 1.0); let d0 = if inst { 0.0 } else { 0.18 } + lag;
    let mut plate = clip(ss, shape("rect", tw, th, col, 0.0, col), start + d0, dur - d0); set(&mut plate, "x", tx); set(&mut plate, "y", ty);
    key(&mut plate, "scale", 0.0, 0.0, e); key(&mut plate, "scale", if inst { 0.001 } else { 0.3 }, 1.0, e); key(&mut plate, "opacity", dur - d0 - 0.15, 1.0, Ease::In); key(&mut plate, "opacity", dur - d0, 0.0, Ease::In); cl.push(plate);
    let lum = 0.2126 * col[0] + 0.7152 * col[1] + 0.0722 * col[2]; let ink = if lum > 0.45 { [0.04, 0.05, 0.05] } else { [1.0; 3] };
    let mut tt = clip(ss, txt(&tag, font, fsz, ink, "center"), start + d0, dur - d0); set(&mut tt, "x", tx); set(&mut tt, "y", ty);
    key(&mut tt, "reveal", if inst { 0.0 } else { 0.05 }, 0.0, Ease::Linear); key(&mut tt, "reveal", if inst { 0.001 } else { 0.35 }, 1.0, Ease::Out); key(&mut tt, "opacity", dur - d0 - 0.15, 1.0, Ease::In); key(&mut tt, "opacity", dur - d0, 0.0, Ease::In); cl.push(tt);
    if let Some(cap) = a.get("caption").and_then(|v| v.as_str()) { let cs = 19.0 * k; let mut ct = clip(ss, txt(cap, "Inter", cs, [1.0; 3], "left"), start + 0.3 + lag, dur - 0.3 - lag); set(&mut ct, "x", cx - pw / 2.0 + 12.0 * k + cap.chars().count() as f64 * cs * 0.28); set(&mut ct, "y", cy - ph / 2.0 + 26.0 * k);
        key(&mut ct, "reveal", 0.0, 0.0, Ease::Linear); key(&mut ct, "reveal", 0.35, 1.0, Ease::Out); key(&mut ct, "opacity", dur - 0.45, 1.0, Ease::In); key(&mut ct, "opacity", dur - 0.3, 0.0, Ease::In); cl.push(ct); }
    let ids = push(ss, &tid, cl)?; Ok(json!({"clip_ids": ids}))
}

/// Number counter: shows a rolling number from `from` to `to` with an ease, as a sequence of text clips (one per frame step).
pub fn add_counter(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let tid = a.get("track_id").and_then(|v| v.as_str()).ok_or("missing track_id")?.to_string();
    let (start, dur, hold) = (num(a, "start", 0.0), num(a, "duration", 3.0), num(a, "hold", 0.0)); let (from, to) = (num(a, "from", 0.0), num(a, "to", 100.0));
    if dur <= 0.0 || hold < 0.0 { return Err("duration must be > 0, hold >= 0".into()); }
    let fps = ss.project.fps; let roll = dur.min(2.0).max(0.1); let n = ((roll * fps).round() as usize).clamp(2, 600);
    let (size, color) = (num(a, "size", 120.0), rgb(a, "color", [1.0; 3])); let font = a.get("font").and_then(|v| v.as_str()).unwrap_or("Anton").to_string();
    let (suffix, prefix) = (a.get("suffix").and_then(|v| v.as_str()).unwrap_or("").to_string(), a.get("prefix").and_then(|v| v.as_str()).unwrap_or("").to_string());
    let (px, py) = (num(a, "x", 0.0), num(a, "y", 0.0)); let decimals = num(a, "decimals", 0.0).clamp(0.0, 4.0) as usize; let mut cl = vec![]; let step = 1.0 / fps;
    for i in 0..n {
        let u = (i as f64 / (n - 1) as f64).clamp(0.0, 1.0); let eased = 1.0 - (1.0 - u).powi(4); let v = from + (to - from) * eased;
        let s = format!("{}{:.*}{}", prefix, decimals, v, suffix); let t0 = start + i as f64 * step; let last = i == n - 1;
        let d = if last { (start + dur + hold - t0).max(step) } else { step };
        let mut c = clip(ss, txt(&s, &font, size, color, "center"), t0, d); set(&mut c, "x", px); set(&mut c, "y", py);
        if last { key(&mut c, "scale", 0.0, 1.0, Ease::Out); key(&mut c, "scale", 0.12, 1.06, Ease::Out); key(&mut c, "scale", 0.4, 1.0, Ease::Out); }
        cl.push(c);
    }
    let ids = push(ss, &tid, cl)?; Ok(json!({"clip_ids": ids, "frames": n}))
}

/// Prompt box: rounded dark plate with accent border, typed text and a blinking caret.
pub fn add_prompt_box(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let tid = a.get("track_id").and_then(|v| v.as_str()).ok_or("missing track_id")?.to_string();
    let (start, dur) = (num(a, "start", 0.0), num(a, "duration", 4.0)); let text = a.get("text").and_then(|v| v.as_str()).ok_or("missing text")?.to_string();
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64); let k = ch / 720.0; let accent = rgb(a, "accent", [0.95, 0.75, 0.3]); let type_time = num(a, "type_time", (text.chars().count() as f64 * 0.045).clamp(0.5, 3.5)).min(dur);
    let fsz = num(a, "size", 24.0) * k; let (w, h) = (cw * 0.62, fsz * 3.2); let y = num(a, "y", 0.3 * ch); let e = Ease::OutExpo; let mut cl = vec![];
    let mut bg = clip(ss, Source::Shape { shape: "rect".into(), width: w, height: h, fill: [0.07, 0.07, 0.08], stroke_width: 2.0 * k, stroke_color: accent, sides: 4, corner_radius: 10.0 * k }, start, dur); set(&mut bg, "y", y);
    key(&mut bg, "scale", 0.0, 0.9, e); key(&mut bg, "scale", 0.4, 1.0, e); key(&mut bg, "opacity", 0.0, 0.0, e); key(&mut bg, "opacity", 0.25, 0.94, e); key(&mut bg, "opacity", dur - 0.2, 0.94, Ease::In); key(&mut bg, "opacity", dur, 0.0, Ease::In); cl.push(bg);
    let mut t = clip(ss, txt(&text, a.get("font").and_then(|v| v.as_str()).unwrap_or("Inter"), fsz, [0.96, 0.95, 0.9], "left"), start + 0.3, dur - 0.3); set(&mut t, "y", y); let fam = a.get("font").and_then(|v| v.as_str()).unwrap_or("Inter"); let tw = crate::text::measure(&text, fam, fsz, 0.0, 0.0, "left", cw).map(|v| v.0).unwrap_or(text.chars().count() as f64 * fsz * 0.5); set(&mut t, "x", -w / 2.0 + fsz * 1.2 + tw / 2.0);
    key(&mut t, "reveal", 0.0, 0.0, Ease::Linear); key(&mut t, "reveal", type_time, 1.0, Ease::Linear); key(&mut t, "opacity", dur - 0.5, 1.0, Ease::In); key(&mut t, "opacity", dur - 0.3, 0.0, Ease::In); cl.push(t);
    // blinking caret: alternating on/off short clips after typing, plus a solid caret during typing is omitted (text engine shows reveal)
    let mut tcur = start + 0.3 + type_time; let end = start + dur - 0.3; let cx = -w / 2.0 + fsz * 1.2 + 0.0;
    let _ = cx; let mut on = true; while tcur < end - 0.01 { let d = 0.5f64.min(end - tcur); if on { let mut c = clip(ss, shape("rect", 2.5 * k, fsz * 1.1, [0.96, 0.95, 0.9], 0.0, [0.0; 3]), tcur, d); set(&mut c, "y", y); set(&mut c, "x", -w / 2.0 + fsz * 1.2 + tw + 4.0 * k); cl.push(c); } tcur += d; on = !on; }
    let ids = push(ss, &tid, cl)?; Ok(json!({"clip_ids": ids}))
}

/// Viewfinder / editor HUD frame: dark hatched backdrop, "+" registration grid, corner brackets, top labels, blinking REC dot,
/// running timecode and "BPM / BAR" readout. Every part is a normal clip so it can be restyled afterwards.
pub fn add_hud_frame(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let tid = a.get("track_id").and_then(|v| v.as_str()).ok_or("missing track_id")?.to_string();
    let (start, dur) = (num(a, "start", 0.0), num(a, "duration", 3.0)); if dur <= 0.0 { return Err("duration must be > 0".into()); }
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64); let fps = ss.project.fps; let k = ch.min(cw * 9.0 / 16.0) / 720.0 * (ch / ch.min(cw * 9.0 / 16.0)).min(1.0).max(1.0); let k = if cw >= ch { ch / 720.0 } else { cw / 1280.0 * 1.0 }.max(k * 0.0 + 0.1);
    let label = a.get("label").and_then(|v| v.as_str()).unwrap_or("BOXES").to_string(); let bpm = num(a, "bpm", 125.0); let tc0 = num(a, "tc_start", 0.0);
    let bg = a.get("background").and_then(|v| v.as_bool()).unwrap_or(true); let col = rgb(a, "color", [0.80, 0.82, 0.85]); let font = "SpaceMono-Bold"; let fs = 21.0 * k; let m = 40.0 * k; let br = 26.0 * k; let bt = 2.0 * k;
    let mut cl = vec![];
    if bg { cl.push(clip(ss, Source::Solid { color: rgb(a, "bg_color", [0.145, 0.15, 0.155]) }, start, dur)); }
    // registration "+" grid: one monospaced text clip, columns every 7 chars
    let cols = 15usize; let rows = 8usize; let mut lines = vec![]; for _ in 0..rows { lines.push(vec!["+"; cols].join("      ")); let mut g = String::new(); for _ in 0..2 { g.push_str(&" ".repeat((cols - 1) * 7 + 1)); g.push('\n'); } lines.push(g.trim_end().to_string()); }
    let gtxt = lines.join("\n"); let gs = (cw * 0.0178).min(26.0 * k * 1.2); let mut g = clip(ss, txt(&gtxt, font, gs, [0.5, 0.52, 0.55], "center"), start, dur); set(&mut g, "opacity", 0.55); cl.push(g);
    // corner brackets
    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] { for horiz in [true, false] { let (w, h) = if horiz { (br, bt) } else { (bt, br) };
        let mut t = clip(ss, shape("rect", w, h, col, 0.0, col), start, dur); set(&mut t, "x", sx * (cw / 2.0 - 28.0 * k - if horiz { br / 2.0 } else { bt / 2.0 })); set(&mut t, "y", sy * (ch / 2.0 - 25.0 * k - if horiz { bt / 2.0 } else { br / 2.0 })); set(&mut t, "opacity", 0.85); cl.push(t); } }
    let (lx, rx, ty, by) = (-(cw / 2.0 - m - br - 16.0 * k), cw / 2.0 - m - br - 16.0 * k, -(ch / 2.0 - m - 6.0 * k), ch / 2.0 - m - 6.0 * k); let w_of = |s: &str| s.chars().count() as f64 * fs * 0.6;
    let mut put = |ss: &mut Session, s: &str, x: f64, y: f64, c: [f64; 3], o: f64| -> Clip { let mut t = clip(ss, txt(s, font, fs, c, "center"), start, dur); set(&mut t, "x", x); set(&mut t, "y", y); set(&mut t, "opacity", o); t };
    cl.push(put(ss, &label, lx + w_of(&label) / 2.0, ty, col, 0.9)); cl.push(put(ss, "REC", rx - w_of("REC") / 2.0, ty, col, 0.9));
    // blinking red dot (1 Hz)
    let mut dot = clip(ss, shape("ellipse", 10.0 * k, 10.0 * k, [0.95, 0.2, 0.2], 0.0, [0.0; 3]), start, dur); set(&mut dot, "x", rx - w_of("REC") - 14.0 * k); set(&mut dot, "y", ty); let mut tt = 0.0; let mut on = true; while tt <= dur && tt < 600.0 { key(&mut dot, "opacity", tt, if on { 1.0 } else { 0.0 }, Ease::Hold); tt += 0.5; on = !on; } cl.push(dot);
    // running timecode and bpm/bar readout: single dynamic-text clips resolved at render time
    let tc = format!("TC {{tc:{}}}", tc0); let bar = format!("{:.0} BPM  BAR {{bar:{}:{}}}", bpm, bpm, tc0);
    cl.push(put(ss, &tc, lx + w_of("TC 00:00:00:00") / 2.0, by, col, 0.9)); cl.push(put(ss, &bar, rx - w_of("125 BPM  BAR 00.0.0") / 2.0, by, col, 0.9)); let _ = fps;
    let ids = push(ss, &tid, cl)?; Ok(json!({"clip_ids": ids}))
}

/// Box expansion to full frame, measured from the SCUMBLE reference (7.6-8.0 s): the detection box grows at CONSTANT speed
/// (no easing, stepped per frame, about 42 px/frame per side at 1280 wide) until it covers the frame, then a 2-frame white flash
/// (about 1.0 then 0.5 luma) hides the hard cut. Returns `end` (time the flash finishes) so the next scene can start there.
pub fn add_box_expand(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let tid = a.get("track_id").and_then(|v| v.as_str()).ok_or("missing track_id")?.to_string();
    let start = num(a, "start", 0.0); let fps = ss.project.fps; let (cw, ch) = (ss.project.width as f64, ss.project.height as f64); let k = ch / 720.0;
    let rect = |key: &str, d: [f64; 4]| -> [f64; 4] { a.get(key).and_then(|v| v.as_array()).filter(|v| v.len() >= 4).map(|v| [v[0].as_f64().unwrap_or(0.0), v[1].as_f64().unwrap_or(0.0), v[2].as_f64().unwrap_or(1.0), v[3].as_f64().unwrap_or(1.0)]).unwrap_or(d) };
    let from = rect("from", [0.247, 0.036, 0.49, 0.925]); let to = rect("to", [-0.03, -0.03, 1.06, 1.06]); let n = num(a, "frames", 8.0).clamp(1.0, 600.0) as usize;
    let col = rgb(a, "color", [1.0, 0.55, 0.1]); let fill_o = num(a, "fill_opacity", 0.22); let flash = a.get("flash").and_then(|v| v.as_bool()).unwrap_or(true); let frame = 1.0 / fps; let sw = (3.0 * k).max(2.0);
    let label = a.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string(); let mut cl = vec![];
    for i in 0..n { let u = i as f64 / n as f64; let r: Vec<f64> = (0..4).map(|j| from[j] + (to[j] - from[j]) * u).collect(); let (pw, ph) = (r[2] * cw, r[3] * ch); let (cx, cy) = ((r[0] + r[2] / 2.0 - 0.5) * cw, (r[1] + r[3] / 2.0 - 0.5) * ch); let s0 = start + i as f64 * frame;
        let mut f = clip(ss, shape("rect", pw, ph, col, 0.0, col), s0, frame); set(&mut f, "x", cx); set(&mut f, "y", cy); set(&mut f, "opacity", fill_o); cl.push(f);
        let mut o = clip(ss, shape("rect", pw, ph, [-1.0, 0.0, 0.0], sw, col), s0, frame); set(&mut o, "x", cx); set(&mut o, "y", cy); cl.push(o);
        if !label.is_empty() { let fs = 20.0 * k; let mut t = clip(ss, txt(&label, "SpaceMono-Bold", fs, col, "left"), s0, frame); set(&mut t, "x", cx - pw / 2.0 + 14.0 * k + label.chars().count() as f64 * fs * 0.3); set(&mut t, "y", cy - ph / 2.0 + 24.0 * k); cl.push(t); } }
    let mut end = start + n as f64 * frame;
    if flash { for (i, op) in [1.0, 0.55].iter().enumerate() { let mut w = clip(ss, Source::Solid { color: [1.0, 1.0, 1.0] }, end + i as f64 * frame, frame); set(&mut w, "opacity", *op); cl.push(w); } end += 2.0 * frame; }
    let ids = push(ss, &tid, cl)?; Ok(json!({"clip_ids": ids, "end": end}))
}
