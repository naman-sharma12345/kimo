//! roto_subject: one click (or a box) on a subject -> a temporally stable matte through the shot, like Roto Brush.
//!
//! 1. The clip's own frame at `time` is rendered.
//! 2. The subject under the click: an object detection containing the point (YOLOX) gives its box, else a box around the
//!    point (or the box given). A person is matted with MODNet, anything else with U^2-Net salient-object detection, on
//!    the crop (models fetched on first use, pinned in vision/models.toml).
//! 3. The connected region under the click is traced into an outline (Moore boundary tracing, simplified to ~80 points).
//! 4. That outline seeds `track_matte`, which carries it through the range as a dense, temporally stable alpha matte
//!    (attached to the clip as a matte mask layer, and usable as a track matte for "text behind the subject").
use super::kit::*;
use crate::effects::Frame;

fn crop(f: &Frame, x0: usize, y0: usize, w: usize, h: usize) -> Frame {
    let mut d = vec![0f32; w * h * 4];
    for y in 0..h { let sy = (y0 + y).min(f.h - 1); let s = (sy * f.w + x0) * 4; d[y * w * 4..(y + 1) * w * 4].copy_from_slice(&f.d[s..s + w * 4]); }
    Frame { w, h, d }
}

/// Morphological closing (dilate then erode, square of radius r, via a summed-area table): joins parts of one subject that
/// the model left apart (a head and a dark jacket on a dark background).
fn close(m: &[bool], w: usize, h: usize, r: usize) -> Vec<bool> {
    if r == 0 { return m.to_vec(); }
    let box_any = |src: &[bool], want: bool| -> Vec<bool> {
        let mut sat = vec![0u32; (w + 1) * (h + 1)];
        for y in 0..h { let mut row = 0u32; for x in 0..w { row += (src[y * w + x] == want) as u32; sat[(y + 1) * (w + 1) + x + 1] = sat[y * (w + 1) + x + 1] + row; } }
        (0..w * h).map(|i| { let (x, y) = (i % w, i / w); let (x0, y0, x1, y1) = (x.saturating_sub(r), y.saturating_sub(r), (x + r + 1).min(w), (y + r + 1).min(h));
            let n = sat[y1 * (w + 1) + x1] + sat[y0 * (w + 1) + x0] - sat[y0 * (w + 1) + x1] - sat[y1 * (w + 1) + x0]; if want { n > 0 } else { n == 0 } }).collect()
    };
    let dil = box_any(m, true); box_any(&dil, false)
}

/// The 4-connected region of `m` (w x h, true = subject) containing (sx, sy), or the largest region if that pixel is off.
fn region(m: &[bool], w: usize, h: usize, sx: usize, sy: usize) -> Vec<bool> {
    let fill = |seed: usize| -> Vec<bool> { let mut out = vec![false; w * h]; let mut st = vec![seed]; out[seed] = true;
        while let Some(i) = st.pop() { let (x, y) = (i % w, i / w);
            for (dx, dy) in [(1i64, 0i64), (-1, 0), (0, 1), (0, -1)] { let (nx, ny) = (x as i64 + dx, y as i64 + dy); if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 { continue; }
                let j = ny as usize * w + nx as usize; if m[j] && !out[j] { out[j] = true; st.push(j); } } } out };
    let s = sy * w + sx; if m[s] { return fill(s); }
    // largest region otherwise
    let mut seen = vec![false; w * h]; let mut best: Vec<bool> = vec![false; w * h]; let mut best_n = 0;
    for i in 0..w * h { if m[i] && !seen[i] { let r = fill(i); let n = r.iter().filter(|v| **v).count(); for (k, v) in r.iter().enumerate() { if *v { seen[k] = true; } } if n > best_n { best_n = n; best = r; } } }
    best
}

/// Outer boundary of a binary region by Moore-neighbour tracing (pixel centres), in order.
fn trace(m: &[bool], w: usize, h: usize) -> Vec<[f64; 2]> {
    let at = |x: i64, y: i64| x >= 0 && y >= 0 && x < w as i64 && y < h as i64 && m[y as usize * w + x as usize];
    let Some(start) = (0..w * h).find(|&i| m[i]) else { return vec![] };
    let (sx, sy) = ((start % w) as i64, (start / w) as i64);
    const D: [(i64, i64); 8] = [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
    let (mut x, mut y, mut dir) = (sx, sy, 6usize); let mut out = vec![[sx as f64 + 0.5, sy as f64 + 0.5]];
    for _ in 0..(w * h * 4).max(16) {
        let mut moved = false;
        for k in 0..8 { let d = (dir + 6 + k) % 8; let (nx, ny) = (x + D[d].0, y + D[d].1); if at(nx, ny) { x = nx; y = ny; dir = d; moved = true; break; } }
        if !moved || (x == sx && y == sy) { break; }
        out.push([x as f64 + 0.5, y as f64 + 0.5]);
    }
    out
}

/// Douglas-Peucker on a closed outline, loosened until it has at most `max` points.
fn simplify(p: &[[f64; 2]], max: usize) -> Vec<[f64; 2]> {
    fn dp(p: &[[f64; 2]], eps: f64, out: &mut Vec<[f64; 2]>) {
        if p.len() < 3 { out.extend_from_slice(&p[..p.len().saturating_sub(1)]); return; }
        let (a, b) = (p[0], p[p.len() - 1]); let (dx, dy) = (b[0] - a[0], b[1] - a[1]); let l = dx.hypot(dy).max(1e-9);
        let (mut im, mut dm) = (0, 0.0); for (i, q) in p.iter().enumerate().take(p.len() - 1).skip(1) { let d = ((q[0] - a[0]) * dy - (q[1] - a[1]) * dx).abs() / l; if d > dm { dm = d; im = i; } }
        if dm > eps { dp(&p[..=im], eps, out); dp(&p[im..], eps, out); } else { out.push(a); }
    }
    if p.len() < 4 { return p.to_vec(); }
    // a closed outline: split at the point farthest from the start, simplify each half (a segment from a point back to
    // itself has no direction to measure distances from)
    let far = (1..p.len()).max_by(|&i, &j| { let d = |k: usize| (p[k][0] - p[0][0]).powi(2) + (p[k][1] - p[0][1]).powi(2); d(i).partial_cmp(&d(j)).unwrap_or(std::cmp::Ordering::Equal) }).unwrap_or(p.len() / 2);
    let mut eps = 0.75; loop { let mut o = vec![]; dp(&p[..=far], eps, &mut o); let mut back = p[far..].to_vec(); back.push(p[0]); dp(&back, eps, &mut o); if o.len() <= max || eps > 64.0 { return o; } eps *= 1.4; }
}

/// The first camera cut in the clip between clip-local times t0 and t1 (ffmpeg scene change score over 0.3), clip-local.
fn next_cut(ss: &Session, cid: &str, t0: f64, t1: f64) -> Option<f64> {
    let c = ss.project.find_clip(cid)?; let crate::model::Source::Media { asset } = &c.source else { return None };
    let path = ss.project.assets.get(asset)?.path.clone(); let sp = c.speed.abs().max(1e-6);
    let (s0, dur) = (c.in_point + t0 * sp, (t1 - t0) * sp); if dur <= 0.05 { return None; }
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-hide_banner", "-ss", &format!("{s0}"), "-t", &format!("{dur}"), "-i", &path, "-vf", "scale=320:-2,select='gt(scene,0.3)',showinfo", "-an", "-f", "null", "-"]);
    let out = cmd.output().ok()?; let log = String::from_utf8_lossy(&out.stderr);
    let first = log.split("pts_time:").nth(1)?.split_whitespace().next()?.parse::<f64>().ok()?;
    Some(t0 + first / sp)
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let cid = str_arg(a, "clip_id")?.to_string();
    let (w, h) = (ss.project.width as usize, ss.project.height as usize);
    let (cstart, cdur) = { let c = ss.project.find_clip(&cid).ok_or("no such clip")?; (c.start, c.duration) };
    let t_local = opt_num(a, "time", 0.0).clamp(0.0, (cdur - 1e-3).max(0.0)); let t = cstart + t_local;
    crate::render::prepare_at(&ss.project, &mut ss.cache, Some(t))?;
    let frame = crate::render::render_clip_alone(&ss.project, &cid, t, &ss.cache).ok_or("could not render the clip")?;
    // the click / box, normalised 0..1 or pixels
    let norm = |v: f64, full: usize| if v.abs() <= 1.5 { v * full as f64 } else { v };
    let pt = a.get("point").and_then(|v| v.as_array()).map(|p| [norm(p.first().and_then(|v| v.as_f64()).unwrap_or(0.5), w), norm(p.get(1).and_then(|v| v.as_f64()).unwrap_or(0.5), h)]);
    let bx = a.get("box").and_then(|v| v.as_array()).filter(|b| b.len() == 4).map(|b| { let g = |i: usize, full: usize| norm(b[i].as_f64().unwrap_or(0.0), full); [g(0, w), g(1, h), g(2, w), g(3, h)] });
    if pt.is_none() && bx.is_none() { return Err("give point [x, y] (a click on the subject) or box [x, y, w, h]; normalised 0..1 or pixels".into()); }
    let click = pt.unwrap_or_else(|| { let b = bx.unwrap(); [b[0] + b[2] / 2.0, b[1] + b[3] / 2.0] });
    let kind = a.get("kind").and_then(|v| v.as_str()).unwrap_or("auto");
    // the subject's box: given, or the smallest detection containing the click, or 45% of the frame around the click
    let dets = crate::vision::objects(&frame);
    let hit = dets.iter().filter(|d| { let b = &d.bbox; (click[0] as f32) >= b.x && (click[0] as f32) <= b.x + b.w && (click[1] as f32) >= b.y && (click[1] as f32) <= b.y + b.h })
        .min_by(|x, y| x.bbox.area().partial_cmp(&y.bbox.area()).unwrap_or(std::cmp::Ordering::Equal));
    let mut b = match (bx, hit) { (Some(b), _) => b, (None, Some(d)) => [d.bbox.x as f64, d.bbox.y as f64, d.bbox.w as f64, d.bbox.h as f64], (None, None) => { let s = 0.45 * w.min(h) as f64; [click[0] - s, click[1] - s, 2.0 * s, 2.0 * s] } };
    // a margin so the outline is not cut by the box
    let (mx, my) = (b[2] * 0.12, b[3] * 0.12); b = [b[0] - mx, b[1] - my, b[2] + 2.0 * mx, b[3] + 2.0 * my];
    let (x0, y0) = (b[0].max(0.0) as usize, b[1].max(0.0) as usize); let (x1, y1) = (((b[0] + b[2]).ceil() as usize).min(w), ((b[1] + b[3]).ceil() as usize).min(h));
    if x1 <= x0 + 8 || y1 <= y0 + 8 { return Err("the subject's box is too small".into()); }
    let cr = crop(&frame, x0, y0, x1 - x0, y1 - y0);
    let person = match kind { "person" => true, "object" => false, _ => hit.map_or(false, |d| d.class_id == 0) };
    // a person: MODNet (hair, edges) together with salient-object detection (clothing MODNet loses against a dark room)
    let mask = if person { crate::vision::person_matte(&cr) } else { crate::vision::subject_mask(&cr) };
    let extra = if person { Some(crate::vision::subject_mask(&cr)) } else { None };
    let (cw, chh) = (cr.w, cr.h);
    // a person matte is soft on dark clothing: a lower cut-off, then close the gaps so head and body are one region
    let thr = if person { 0.3 } else { 0.5 };
    let bin: Vec<bool> = (0..cw * chh).map(|i| { let (x, y) = ((i % cw) as f32 + 0.5, (i / cw) as f32 + 0.5); mask.at(x, y) >= thr || extra.as_ref().map_or(false, |e| e.at(x, y) >= 0.5) }).collect();
    let bin = close(&bin, cw, chh, ((cw.min(chh) as f64) * 0.04).round() as usize);
    let (sx, sy) = (((click[0] - x0 as f64).max(0.0) as usize).min(cw - 1), ((click[1] - y0 as f64).max(0.0) as usize).min(chh - 1));
    let reg = region(&bin, cw, chh, sx, sy);
    let area = reg.iter().filter(|v| **v).count();
    if area < 64 { return Err(format!("no subject found under the click ({} model{})", mask.model, mask.fallback.as_ref().map(|f| format!(": {f}")).unwrap_or_default())); }
    let outline: Vec<[f64; 2]> = simplify(&trace(&reg, cw, chh), opt_num(a, "points", 80.0).clamp(12.0, 300.0) as usize).into_iter().map(|p| [p[0] + x0 as f64, p[1] + y0 as f64]).collect();
    // carry it through the shot with track_matte (its range limit applies: pieces of up to 240 frames), stopping before a
    // camera cut: past one the subject is gone and the matte would follow whatever replaced it
    let mut end = opt_num(a, "end", (t_local + 239.0 / ss.project.fps).min(cdur));
    let cut = next_cut(ss, &cid, t_local, end);
    if let Some(cl) = cut { end = (cl - 1.0 / ss.project.fps).max(t_local + 1.0 / ss.project.fps); }
    let mut tm = json!({"clip_id": cid, "seeds": [{"time": t_local, "points": outline}], "start": opt_num(a, "start", t_local), "end": end});
    for k in ["temporal", "band", "colour", "add_mask", "dir"] { if let Some(v) = a.get(k) { tm[k] = v.clone(); } }
    let mut r = crate::commands::dispatch_nested(ss, "track_matte", &tm)?;
    if let Some(cl) = cut { r["stopped_at_cut"] = json!((cl * 1000.0).round() / 1000.0); }
    r["subject"] = json!({"model": mask.model, "fallback": mask.fallback, "kind": if person { "person" } else { "object" }, "detection": hit.map(|d| d.class), "box": [x0, y0, x1 - x0, y1 - y0], "seed_points": outline.len(), "seed_area_px": area});
    Ok(r)
}

pub fn commands() -> Vec<Command> {
    vec![Command { name: "roto_subject", description: "AI roto in one call (like Roto Brush): click the subject (point [x,y]) or box it (box [x,y,w,h]), normalised 0..1 or pixels, at clip-local `time`. A person is matted with MODNet, anything else with salient-object detection; its outline seeds track_matte, which carries a temporally stable alpha matte through start..end (up to 240 frames per call) and attaches it to the clip. Use the result as a track matte for text behind the subject.", mutates: true,
        schema: obj(vec![("clip_id", s("media clip")), ("point", json!({"type":"array","items":{"type":"number"},"description":"a click on the subject"})), ("box", json!({"type":"array","items":{"type":"number"},"description":"[x, y, w, h] around the subject"})),
            ("time", n("clip-local seconds of the frame you clicked, default 0")), ("start", n("clip-local start of the matte, default time")), ("end", n("clip-local end (at most 240 frames on)")),
            ("kind", json!({"type":"string","enum":["auto","person","object"],"description":"auto (default): person if the click lands on a detected person"})), ("points", n("outline points, default 80")),
            ("temporal", n("temporal stability 0..1, default 0.6")), ("add_mask", json!({"type":"boolean","description":"attach the matte to the clip (default true)"}))], &["clip_id"]),
        run }]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn traces_the_outline_of_a_square_region() {
        let (w, h) = (20, 20); let mut m = vec![false; w * h]; for y in 5..15 { for x in 4..12 { m[y * w + x] = true; } }
        let r = region(&m, w, h, 6, 6); assert_eq!(r.iter().filter(|v| **v).count(), 80);
        let o = simplify(&trace(&r, w, h), 80); assert!(o.len() >= 4 && o.len() <= 12, "a rectangle simplifies to a few corners: {}", o.len());
        let (xs, ys): (Vec<f64>, Vec<f64>) = o.iter().map(|p| (p[0], p[1])).unzip();
        assert!((xs.iter().cloned().fold(f64::MAX, f64::min) - 4.5).abs() < 1.0 && (ys.iter().cloned().fold(f64::MIN, f64::max) - 14.5).abs() < 1.0);
    }
    #[test] fn a_click_off_the_mask_takes_the_largest_region() {
        let (w, h) = (10, 10); let mut m = vec![false; w * h]; m[0] = true; for i in 50..80 { m[i] = true; }
        assert_eq!(region(&m, w, h, 9, 0).iter().filter(|v| **v).count(), 30);
    }
}
