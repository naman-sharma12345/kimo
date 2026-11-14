//! Observe: every edit comes back with what it did. After any call that changed the project, the engine diffs the
//! project before and after, picks the moments the edit touched, renders them before and after at draft size, measures
//! them, runs the critics on what the edit touched, and returns all of it (plus one before/after strip image the agent
//! sees inline). Agents no longer edit blind or have to remember to render a frame.
//!
//! Per call: `observe: "off" | "numbers" | "fast" | "full"` (default `KIMO_OBSERVE`, else fast) and `expect: [...]`
//! (see expect.rs; a failed expectation rolls the edit back). Both are read here and never reach the tool itself.
//! Budget: `KIMO_OBSERVE_MS` (default 1500 ms fast, 6000 ms full). Over budget, further frames are skipped (the timeline
//! critics always run) and the result says so.
use crate::commands::{Media, Session};
use crate::critic::{self, picture, Finding};
use crate::model::Project;
use serde_json::{json, Value};

pub mod expect;

/// What every agent should know about observe/expect/review (sent as the MCP server's initialize instructions).
pub const AGENT_GUIDE: &str = "kimo shows you every edit: each call that changes the project returns `observe` (what it touched, before/after frames as an image, brightness numbers, and findings: mistakes with severity and the exact fix call). Read it after every edit and fix any error or warning before moving on. For anything that must hold, pass expect: [...] on the edit (visible/hidden at a time, inside title_safe, luma range, no_black_frames, readable, max_findings); if it fails the edit is rolled back and you are told why. Run review before export. Use observe: \"full\" for a sharper look, or render_frame/contact_sheet for any moment you want to see.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode { Off, Numbers, Fast, Full }

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> { match s { "off" | "none" | "false" => Some(Mode::Off), "numbers" => Some(Mode::Numbers), "fast" | "on" | "true" => Some(Mode::Fast), "full" => Some(Mode::Full), _ => None } }
    fn name(self) -> &'static str { match self { Mode::Off => "off", Mode::Numbers => "numbers", Mode::Fast => "fast", Mode::Full => "full" } }
    fn probes(self) -> usize { match self { Mode::Full => 8, Mode::Fast => 4, _ => 0 } }
    fn tile_px(self) -> usize { if self == Mode::Full { 480 } else { 320 } }
    fn budget_ms(self) -> u128 {
        let d = if self == Mode::Full { 6000 } else { 1500 };
        std::env::var("KIMO_OBSERVE_MS").ok().and_then(|v| v.parse::<u128>().ok()).map(|v| if self == Mode::Full { v.max(d) } else { v }).unwrap_or(d)
    }
}

/// The observe/expect arguments of one call, taken off the arguments the tool sees.
#[derive(Clone, Debug)]
pub struct Meta { pub mode: Mode, pub expect: Option<Value>, pub keep_on_fail: bool }

pub fn default_mode() -> Mode { std::env::var("KIMO_OBSERVE").ok().and_then(|v| Mode::parse(v.trim())).unwrap_or(Mode::Fast) }

/// Split `observe`, `expect` and `on_fail` off a call's arguments (top level, and inside a group call's `args`).
pub fn split_meta(args: &Value) -> (Value, Meta) {
    let mut a = args.clone();
    let mut meta = Meta { mode: default_mode(), expect: None, keep_on_fail: false };
    let mut take = |o: &mut serde_json::Map<String, Value>| {
        if let Some(v) = o.remove("observe") { if let Some(m) = v.as_str().and_then(Mode::parse).or_else(|| v.as_bool().map(|b| if b { Mode::Fast } else { Mode::Off })) { meta.mode = m; } }
        if let Some(v) = o.remove("expect") { if !v.is_null() { meta.expect = Some(if v.is_array() { v } else { json!([v]) }); } }
        if let Some(v) = o.remove("on_fail") { meta.keep_on_fail = v.as_str() == Some("keep"); }
    };
    if let Some(o) = a.as_object_mut() {
        take(o);
        if let Some(inner) = o.get_mut("args").and_then(|x| x.as_object_mut()) { take(inner); }
    }
    (a, meta)
}

/// Time spans the edit touched: every clip added, removed or changed (its old and new extent), and whether something
/// global changed (canvas, grade, colour pipeline, markers...).
pub fn spans(before: &Project, after: &Project) -> (Vec<[f64; 2]>, bool, Vec<String>) {
    use std::collections::BTreeMap;
    let map = |p: &Project| -> BTreeMap<String, (f64, f64, Value)> {
        p.tracks.iter().flat_map(|t| t.clips.iter().map(move |c| (c.id.clone(), (c.start, c.start + c.duration, { let mut v = serde_json::to_value(c.as_ref()).unwrap_or(Value::Null); v["_track"] = json!(t.id); v }))))
            .collect()
    };
    let (a, b) = (map(before), map(after));
    let mut sp = vec![]; let mut ids = vec![];
    for (id, (s, e, v)) in &a { match b.get(id) { None => { sp.push([*s, *e]); ids.push(id.clone()); } Some((s2, e2, v2)) => if v != v2 { sp.push([s.min(*s2), e.max(*e2)]); ids.push(id.clone()); } } }
    for (id, (s, e, _)) in &b { if !a.contains_key(id) { sp.push([*s, *e]); ids.push(id.clone()); } }
    let strip = |p: &Project| { let mut v = serde_json::to_value(p).unwrap_or(Value::Null); if let Some(o) = v.as_object_mut() { o.remove("tracks"); o.remove("assets"); } v };
    let global = strip(before) != strip(after);
    sp.sort_by(|x, y| x[0].partial_cmp(&y[0]).unwrap_or(std::cmp::Ordering::Equal));
    let mut merged: Vec<[f64; 2]> = vec![];
    for s in sp { if let Some(l) = merged.last_mut() { if s[0] <= l[1] + 0.05 { l[1] = l[1].max(s[1]); continue; } } merged.push(s); }
    (merged, global, ids)
}

/// The moments to look at: each touched span's middle, then its first and last frames, then transition middles and
/// key times inside it; evenly cut down to `max`. A global change looks at the start, middle and end of the timeline.
pub fn probes(after: &Project, spans: &[[f64; 2]], global: bool, max: usize) -> Vec<f64> {
    let fr = critic::frame(after);
    let end = crate::ext::export_manifest::project_end(after);
    if end <= 0.0 || max == 0 { return vec![]; }
    let clampt = |t: f64| t.clamp(0.0, (end - fr).max(0.0));
    let mut first: Vec<f64> = vec![]; let mut more: Vec<f64> = vec![];
    for s in spans { let (a, b) = (s[0].max(0.0), s[1].min(end)); if b < a { continue; } first.push(clampt((a + b) / 2.0)); if b - a > 4.0 * fr { more.push(clampt(a + fr)); more.push(clampt(b - fr)); } }
    for t in &after.tracks { for c in &t.clips {
        let inside = |x: f64| spans.iter().any(|s| x >= s[0] - 1e-6 && x <= s[1] + 1e-6);
        if let Some(tx) = &c.transition_in { let m = c.start + tx.duration.min(c.duration) / 2.0; if inside(m) { more.push(clampt(m)); } }
        for pr in c.props.values() { for k in &pr.keys { let x = c.start + k.t; if inside(x) { more.push(clampt(x)); } } }
    } }
    if global || first.is_empty() { first.extend([clampt(end * 0.1), clampt(end * 0.5), clampt(end * 0.9)]); }
    let mut all = first; all.extend(more);
    let mut out: Vec<f64> = vec![];
    for t in all { if !out.iter().any(|o: &f64| (o - t).abs() < fr * 2.0) { out.push(crate::critic::r3(t)); } if out.len() >= max { break; } }
    out.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    out
}

/// A draft render: decode through the analysis proxies, composite, box-downscale so the long side is `px`.
pub fn draft(p: &Project, cache: &mut crate::render::Cache, t: f64, px: usize) -> Result<(Vec<u8>, usize, usize), String> {
    let key = draft_key(p, t, px);
    if let Some(hit) = DRAFTS.lock().unwrap_or_else(|e| e.into_inner()).get(&key) { return Ok(hit.clone()); }
    let _nb = crate::analysis_proxy::NonBlocking::new();
    crate::render::prepare_draft_at(p, cache, t)?;
    let f = crate::render::render_frame(p, t, cache)?;
    let (fw, fh) = (f.w, f.h);
    let rgba = crate::render::to_rgba8(&f);
    drop(f);
    let s = (px as f64 / fw.max(fh) as f64).min(1.0);
    let (tw, th) = (((fw as f64 * s).round() as usize).max(1), ((fh as f64 * s).round() as usize).max(1));
    let out = (crate::ext::contact_sheet::box_downscale(&rgba, fw, fh, tw, th), tw, th);
    let mut d = DRAFTS.lock().unwrap_or_else(|e| e.into_inner());
    if d.len() >= 96 { if let Some(k) = d.keys().next().cloned() { d.remove(&k); } }
    d.insert(key, out.clone());
    Ok(out)
}

/// Draft frames by (project content, time, size): the "before" of an edit is usually a frame the previous edit already
/// rendered as its "after", so most observations render only half their frames.
static DRAFTS: std::sync::LazyLock<std::sync::Mutex<std::collections::BTreeMap<(u64, i64, usize), (Vec<u8>, usize, usize)>>> = std::sync::LazyLock::new(|| std::sync::Mutex::new(std::collections::BTreeMap::new()));
fn draft_key(p: &Project, t: f64, px: usize) -> (u64, i64, usize) {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(p).unwrap_or_default().hash(&mut h);
    (h.finish(), (t * 1000.0).round() as i64, px)
}

struct Probe { t: f64, before: Option<(Vec<u8>, picture::Stats)>, after: (Vec<u8>, picture::Stats), w: usize, h: usize }

/// What an edit did. Called after a call that pushed an undo step (`ss.undo.last()` is the project before it).
pub fn after_edit(ss: &mut Session, label: &str, mode: Mode) -> Value {
    let t0 = std::time::Instant::now();
    if mode == Mode::Off { return Value::Null; }
    let Some(before) = ss.undo.last().cloned() else { return Value::Null };
    let (sp, global, ids) = spans(&before, &ss.project);
    if sp.is_empty() && !global { return Value::Null; } // e.g. import_media: nothing on the timeline changed
    let mut findings: Vec<Finding> = if sp.is_empty() && !global { vec![] } else if global { critic::timeline(&ss.project) } else { critic::timeline_in(&ss.project, &sp) };
    let times = probes(&ss.project, &sp, global, mode.probes());
    let budget = mode.budget_ms();
    let mut shots: Vec<Probe> = vec![];
    let mut skipped = 0usize;
    let mut render_err: Option<String> = None;
    if mode != Mode::Numbers {
        let px = mode.tile_px();
        for &t in &times {
            if t0.elapsed().as_millis() > budget { skipped += 1; continue; }
            let after = { let Session { project, cache, .. } = &mut *ss; draft(project, cache, t, px) };
            let (a_rgba, w, h) = match after { Ok(x) => x, Err(e) => { render_err.get_or_insert(e); continue; } };
            let b = { let Session { cache, .. } = &mut *ss; draft(&before, cache, t, px).ok() };
            let a_st = picture::stats(&a_rgba);
            let b_pair = b.filter(|(_, bw, bh)| *bw == w && *bh == h).map(|(rgba, _, _)| { let st = picture::stats(&rgba); (rgba, st) });
            findings.extend(picture::check_frame(t, &a_st, b_pair.as_ref().map(|x| &x.1)));
            shots.push(Probe { t, before: b_pair, after: (a_rgba, a_st), w, h });
        }
    }
    critic::sort(&mut findings);
    let mut out = json!({
        "mode": mode.name(), "edit": label, "touched": {"spans": sp, "clips": ids, "global": global},
        "findings": findings, "counts": critic::counts(&findings),
    });
    if !shots.is_empty() {
        let mut probes_json = vec![]; let mut boxes = vec![];
        for s in &shots {
            let (bx, share) = s.before.as_ref().map(|(b, _)| picture::changed_box(b, &s.after.0, s.w, s.h, 16)).unwrap_or((None, 0.0));
            let scale = ss.project.width as f64 / s.w as f64;
            let canvas_box = bx.map(|b| [b[0] as f64 * scale, b[1] as f64 * scale, (b[2] + 1) as f64 * scale, (b[3] + 1) as f64 * scale].map(crate::critic::r3));
            probes_json.push(json!({"t": s.t, "after": s.after.1, "before": s.before.as_ref().map(|x| x.1), "changed_share": crate::critic::r3(share), "changed_box": canvas_box}));
            boxes.push(bx);
        }
        out["probes"] = json!(probes_json);
        match write_strip(ss, &shots, &boxes) {
            Ok(path) => out["image"] = json!({"path": path, "layout": "top row = before the edit (grey frame), bottom row = after (green frame); one column per probe time, labelled t=seconds; red box = pixels this edit changed"}),
            Err(e) => out["image_error"] = json!(e),
        }
    }
    if skipped > 0 { out["note"] = json!(format!("{skipped} of {} probe(s) not rendered: over the {budget} ms observe budget (KIMO_OBSERVE_MS); call look or render_frame for those times", times.len())); out["unrendered"] = json!(times[times.len() - skipped..].to_vec()); }
    if let Some(e) = render_err { out["render_error"] = json!(e); }
    out["ms"] = json!(t0.elapsed().as_millis() as u64);
    out
}

/// One image: before row over after row, one column per probe; red box on the after tile where pixels changed.
fn write_strip(ss: &mut Session, shots: &[Probe], boxes: &[Option<[usize; 4]>]) -> Result<String, String> {
    let (tw, th) = (shots.iter().map(|s| s.w).max().unwrap_or(1), shots.iter().map(|s| s.h).max().unwrap_or(1));
    let (gap, pad) = (6usize, 3usize);
    let cols = shots.len();
    let (sw, sh) = (gap + cols * (tw + 2 * pad + gap), gap + 2 * (th + 2 * pad + gap));
    let mut img = vec![0u8; sw * sh * 4];
    for px in img.chunks_mut(4) { px.copy_from_slice(&[22, 22, 24, 255]); }
    let fill = |img: &mut [u8], x0: usize, y0: usize, w: usize, h: usize, c: [u8; 4]| { for y in y0..(y0 + h).min(sh) { for x in x0..(x0 + w).min(sw) { let i = (y * sw + x) * 4; img[i..i + 4].copy_from_slice(&c); } } };
    let blit = |img: &mut [u8], src: &[u8], w: usize, h: usize, x0: usize, y0: usize| { for y in 0..h { let d = ((y0 + y) * sw + x0) * 4; img[d..d + w * 4].copy_from_slice(&src[y * w * 4..(y + 1) * w * 4]); } };
    for (i, s) in shots.iter().enumerate() {
        let x = gap + i * (tw + 2 * pad + gap);
        let (yb, ya) = (gap, gap + th + 2 * pad + gap);
        fill(&mut img, x, yb, s.w + 2 * pad, s.h + 2 * pad, [110, 110, 116, 255]);
        fill(&mut img, x, ya, s.w + 2 * pad, s.h + 2 * pad, [40, 190, 110, 255]);
        match &s.before { Some((b, _)) => blit(&mut img, b, s.w, s.h, x + pad, yb + pad), None => fill(&mut img, x + pad, yb + pad, s.w, s.h, [0, 0, 0, 255]) }
        blit(&mut img, &s.after.0, s.w, s.h, x + pad, ya + pad);
        if let Some(b) = boxes[i] {
            let (bx0, by0, bx1, by1) = (x + pad + b[0], ya + pad + b[1], x + pad + b[2], ya + pad + b[3]);
            let red = [255, 60, 40, 255];
            fill(&mut img, bx0, by0, bx1 - bx0 + 1, 2, red); fill(&mut img, bx0, by1.saturating_sub(1), bx1 - bx0 + 1, 2, red);
            fill(&mut img, bx0, by0, 2, by1 - by0 + 1, red); fill(&mut img, bx1.saturating_sub(1), by0, 2, by1 - by0 + 1, red);
        }
        crate::ext::contact_sheet::draw_label(&mut img, sw, sh, x + pad + 3, ya + pad + 3, &format!("t={:.2}s", s.t), 2);
    }
    let dir = std::env::temp_dir().join("kimo-observe");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 32;
    let path = dir.join(format!("observe-{}-{n:02}.png", std::process::id())).to_string_lossy().replace('\\', "/");
    let mut fr = crate::effects::Frame::new(sw, sh);
    for (o, px) in fr.d.iter_mut().zip(img.iter()) { *o = *px as f32 / 255.0; }
    crate::render::write_image(&fr, &path)?;
    ss.media.push(Media { path: path.clone(), w: sw as u32, h: sh as u32, mime: "image/png" });
    Ok(path)
}

/// Wraps one tool call: strips observe/expect, runs it, and when it changed the project attaches the observation and
/// checks the expectations (rolling the edit back when one fails, unless on_fail: "keep").
pub fn wrap(ss: &mut Session, args: &Value, run: impl FnOnce(&mut Session, &Value) -> Result<Value, String>) -> Result<Value, String> {
    let (clean, meta) = split_meta(args);
    // every recorded undo step bumps snap_n (commands::record_undo): the reliable "this call edited" signal
    let edits_before = ss.snap_n;
    let v = run(ss, &clean)?;
    finish(ss, edits_before, &meta, v)
}

/// The second half of `wrap`, for callers that run the tool themselves (the MCP server runs it on a guarded worker):
/// `edits_before` is `ss.snap_n` read before the call.
pub fn finish(ss: &mut Session, edits_before: u32, meta: &Meta, mut v: Value) -> Result<Value, String> {
    if ss.snap_n == edits_before || ss.undo.is_empty() { return Ok(v); }
    let label = ss.undo_labels.last().cloned().unwrap_or_default();
    let media_before = ss.media.len();
    let obs = after_edit(ss, &label, meta.mode);
    if let Some(ex) = &meta.expect {
        let fails = expect::check(ss, ex, &obs);
        if !fails.is_empty() {
            if meta.keep_on_fail {
                if let Some(o) = v.as_object_mut() { o.insert("expect_failed".into(), json!(fails)); o.insert("observe".into(), obs); }
                return Ok(v);
            }
            let _ = crate::commands::dispatch(ss, "undo", &json!({}));
            ss.media.truncate(media_before);
            return Err(format!("expectation failed, so the edit was rolled back (undo): {}. What the edit did: {}",
                serde_json::to_string(&fails).unwrap_or_default(), serde_json::to_string(&obs).unwrap_or_default()));
        }
        if let Some(o) = v.as_object_mut() { o.insert("expect_passed".into(), json!(ex.as_array().map(|a| a.len()).unwrap_or(1))); }
    }
    if !obs.is_null() {
        if let Some(o) = v.as_object_mut() { o.insert("observe".into(), obs); } else { v = json!({"result": v, "observe": obs}); }
    }
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::critic::testkit::*;
    #[test]
    fn meta_is_split_off_top_level_and_group_args() {
        let (a, m) = split_meta(&json!({"action": "add_clip", "args": {"x": 1, "observe": "full", "expect": {"visible": "c1", "at": 1}}}));
        assert_eq!(a, json!({"action": "add_clip", "args": {"x": 1}}));
        assert_eq!(m.mode, Mode::Full);
        assert!(m.expect.unwrap().is_array());
        let (_, m) = split_meta(&json!({"observe": false}));
        assert_eq!(m.mode, Mode::Off);
    }
    #[test]
    fn spans_cover_old_and_new_extent() {
        let mut ss = session();
        let t = track(&mut ss);
        let c = solid(&mut ss, &t, 1.0, 2.0);
        let before = ss.project.clone();
        call(&mut ss, "move_clip", json!({"clip_id": c, "start": 5.0}));
        let (sp, global, ids) = spans(&before, &ss.project);
        assert!(!global);
        assert_eq!(ids, vec![c]);
        assert_eq!(sp, vec![[1.0, 7.0]], "from the old start to the new end");
        let p = probes(&ss.project, &sp, false, 4);
        assert!(!p.is_empty() && p.len() <= 4);
    }
    #[test]
    fn wrap_observes_edits_and_not_reads() {
        let mut ss = session();
        let t = track(&mut ss);
        let r = wrap(&mut ss, &json!({"track_id": t, "source_type": "solid", "color": [1, 0, 0], "start": 0, "duration": 2, "observe": "numbers"}), |ss, a| crate::commands::dispatch(ss, "add_clip", a)).unwrap();
        assert_eq!(r["observe"]["mode"], "numbers");
        assert_eq!(r["observe"]["touched"]["spans"][0], json!([0.0, 2.0]));
        let r2 = wrap(&mut ss, &json!({}), |ss, a| crate::commands::dispatch(ss, "timeline_summary", a)).unwrap();
        assert!(r2.get("observe").is_none(), "a read is not observed");
    }
    #[test]
    fn a_failed_expectation_rolls_the_edit_back() {
        let mut ss = session();
        let t = track(&mut ss);
        solid(&mut ss, &t, 0.0, 2.0);
        let before = serde_json::to_string(&ss.project).unwrap();
        // a 2-frame flash clip, with an expectation of no warnings: rolled back
        let e = wrap(&mut ss, &json!({"track_id": t, "source_type": "solid", "color": [1, 1, 1], "start": 2.0, "duration": 0.066, "observe": "numbers", "expect": {"max_findings": "info"}}), |ss, a| crate::commands::dispatch(ss, "add_clip", a)).unwrap_err();
        assert!(e.contains("rolled back") && e.contains("flash_clip"), "{e}");
        assert_eq!(serde_json::to_string(&ss.project).unwrap(), before, "the project is exactly as before the edit");
        // the same edit with on_fail keep stays, and says what failed
        let ok = wrap(&mut ss, &json!({"track_id": t, "source_type": "solid", "color": [1, 1, 1], "start": 2.0, "duration": 0.066, "observe": "numbers", "expect": {"max_findings": "info"}, "on_fail": "keep"}), |ss, a| crate::commands::dispatch(ss, "add_clip", a)).unwrap();
        assert!(ok["expect_failed"].is_array());
    }
}
