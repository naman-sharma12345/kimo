//! Look tools: the deep look an agent asks for when the automatic per-edit `observe` is not enough.
//!
//! * `look`: one frame drawn with an inspector on top. Every visible clip is outlined with its TRUE on-screen box
//!   (each clip rendered alone, transparent, and measured by its opaque pixels, so rotation, masks, parenting and real
//!   text extents are all exact) and labelled with its id or @name; title/action safe areas; motion paths of keyed
//!   moves with ticks at the keys; optionally an alpha checkerboard and a waveform + vectorscope panel.
//! * `watch`: motion as one picture. An onion skin of a time range with each moving element's path drawn on it,
//!   shaded by speed (blue slow, red fast), a strip of the frames underneath, and per-element speed/acceleration/ease.
//! * `compare_reference`: the edit against a reference (a video file or another stretch of the timeline): SSIM/PSNR
//!   (via compare_ranges), brightness, contrast, saturation, warmth and motion energy, with what to change.
use super::kit::*;
use crate::commands::Media;
use crate::model::{Clip, Project, Source};

// ------------------------------------------------------------------------------------------------ drawing

struct Canvas { w: usize, h: usize, px: Vec<u8> }

impl Canvas {
    fn new(w: usize, h: usize, bg: [u8; 4]) -> Canvas { let mut px = vec![0u8; w * h * 4]; for p in px.chunks_mut(4) { p.copy_from_slice(&bg); } Canvas { w, h, px } }
    fn put(&mut self, x: i64, y: i64, c: [u8; 4], a: f32) {
        if x < 0 || y < 0 || x as usize >= self.w || y as usize >= self.h || a <= 0.0 { return; }
        let i = (y as usize * self.w + x as usize) * 4; let a = a.min(1.0) * c[3] as f32 / 255.0;
        for k in 0..3 { self.px[i + k] = (self.px[i + k] as f32 * (1.0 - a) + c[k] as f32 * a).round() as u8; }
        self.px[i + 3] = 255;
    }
    fn fill(&mut self, x0: i64, y0: i64, x1: i64, y1: i64, c: [u8; 4]) { for y in y0.max(0)..y1.min(self.h as i64) { for x in x0.max(0)..x1.min(self.w as i64) { self.put(x, y, c, 1.0); } } }
    fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, c: [u8; 4], th: f64) {
        let n = ((x1 - x0).abs().max((y1 - y0).abs()).ceil() as usize).max(1);
        let r = (th / 2.0).max(0.5);
        for i in 0..=n { let t = i as f64 / n as f64; let (x, y) = (x0 + (x1 - x0) * t, y0 + (y1 - y0) * t);
            for dy in (-r).floor() as i64..=r.ceil() as i64 { for dx in (-r).floor() as i64..=r.ceil() as i64 { if ((dx * dx + dy * dy) as f64) <= r * r + 0.25 { self.put(x.round() as i64 + dx, y.round() as i64 + dy, c, 1.0); } } } }
    }
    fn rect(&mut self, l: f64, t: f64, r: f64, b: f64, c: [u8; 4], th: f64, dash: bool) {
        let seg = |cv: &mut Canvas, x0: f64, y0: f64, x1: f64, y1: f64| {
            if !dash { cv.line(x0, y0, x1, y1, c, th); return; }
            let len = ((x1 - x0).powi(2) + (y1 - y0).powi(2)).sqrt(); let n = (len / 10.0).ceil() as usize;
            for i in (0..n).step_by(2) { let (a, bb) = (i as f64 / n as f64, ((i + 1) as f64 / n as f64).min(1.0)); cv.line(x0 + (x1 - x0) * a, y0 + (y1 - y0) * a, x0 + (x1 - x0) * bb, y0 + (y1 - y0) * bb, c, th); }
        };
        seg(self, l, t, r, t); seg(self, r, t, r, b); seg(self, r, b, l, b); seg(self, l, b, l, t);
    }
    fn dot(&mut self, x: f64, y: f64, r: f64, c: [u8; 4]) { for dy in -(r.ceil() as i64)..=r.ceil() as i64 { for dx in -(r.ceil() as i64)..=r.ceil() as i64 { if ((dx * dx + dy * dy) as f64) <= r * r { self.put(x.round() as i64 + dx, y.round() as i64 + dy, c, 1.0); } } } }
    /// A label: text on a solid background chip, kept inside the image.
    fn label(&mut self, x: f64, y: f64, text: &str, size: f32, fg: [u8; 4], bg: [u8; 4]) {
        let Some(f) = label_font() else { return };
        use ab_glyph::{Font, ScaleFont};
        let sf = f.as_scaled(size);
        let w: f32 = text.chars().map(|ch| sf.h_advance(f.glyph_id(ch))).sum::<f32>() + 8.0;
        let h = (sf.ascent() - sf.descent()) + 4.0;
        let x = x.clamp(0.0, (self.w as f64 - w as f64).max(0.0)) as f32; let y = y.clamp(0.0, (self.h as f64 - h as f64).max(0.0)) as f32;
        self.fill(x as i64, y as i64, (x + w) as i64, (y + h) as i64, bg);
        let mut cx = x + 4.0; let base = y + 2.0 + sf.ascent();
        for ch in text.chars() {
            let id = f.glyph_id(ch);
            let g = id.with_scale_and_position(size, ab_glyph::point(cx, base));
            if let Some(o) = f.outline_glyph(g) { let bb = o.px_bounds(); o.draw(|gx, gy, cov| self.put(bb.min.x as i64 + gx as i64, bb.min.y as i64 + gy as i64, fg, cov)); }
            cx += sf.h_advance(id);
        }
    }
    fn blit(&mut self, src: &[u8], sw: usize, sh: usize, x0: usize, y0: usize) { for y in 0..sh.min(self.h.saturating_sub(y0)) { let n = sw.min(self.w.saturating_sub(x0)) * 4; let d = ((y0 + y) * self.w + x0) * 4; self.px[d..d + n].copy_from_slice(&src[y * sw * 4..y * sw * 4 + n]); } }
}

fn label_font() -> Option<&'static ab_glyph::FontVec> {
    static F: std::sync::OnceLock<Option<ab_glyph::FontVec>> = std::sync::OnceLock::new();
    F.get_or_init(|| {
        let mut paths = vec![crate::text::resolve_font("Geist Mono"), crate::text::resolve_font("Montserrat")];
        if let Some(p) = crate::text::system_fallback_font() { paths.push(p.to_string_lossy().into_owned()); }
        paths.into_iter().find_map(|p| std::fs::read(p).ok().and_then(|d| ab_glyph::FontVec::try_from_vec(d).ok()))
    }).as_ref()
}

const PALETTE: [[u8; 4]; 10] = [[255, 90, 36, 255], [64, 160, 255, 255], [60, 210, 120, 255], [255, 200, 40, 255], [200, 90, 255, 255], [255, 80, 150, 255], [40, 220, 220, 255], [170, 220, 60, 255], [255, 150, 90, 255], [150, 150, 255, 255]];

fn save(ss: &mut Session, cv: &Canvas, path: Option<&str>, stem: &str) -> Result<String, String> {
    let path = match path { Some(p) => p.to_string(), None => {
        let dir = std::env::temp_dir().join("kimo-look"); std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        dir.join(format!("{stem}-{}-{:02}.png", std::process::id(), SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed) % 32)).to_string_lossy().replace('\\', "/") } };
    let mut fr = crate::effects::Frame::new(cv.w, cv.h);
    for (o, p) in fr.d.iter_mut().zip(cv.px.iter()) { *o = *p as f32 / 255.0; }
    crate::render::write_image(&fr, &path)?;
    ss.media.push(Media { path: path.clone(), w: cv.w as u32, h: cv.h as u32, mime: "image/png" });
    Ok(path)
}

// ------------------------------------------------------------------------------------------------ rendering

/// The composite at t, full canvas size, RGBA8 (transparent background when asked).
fn render_rgba(ss: &mut Session, p: &Project, t: f64, transparent: bool, draft: bool) -> Result<(Vec<u8>, usize, usize), String> {
    let Session { cache, .. } = &mut *ss;
    if draft { crate::render::prepare_draft_at(p, cache, t)?; } else { crate::render::prepare_at(p, cache, Some(t))?; }
    let f = crate::render::render_frame_opts(p, t, cache, transparent)?;
    Ok((crate::render::to_rgba8(&f), f.w, f.h))
}

fn down(rgba: &[u8], w: usize, h: usize, px: usize) -> (Vec<u8>, usize, usize) {
    let s = (px as f64 / w.max(h) as f64).min(1.0);
    let (tw, th) = (((w as f64 * s).round() as usize).max(1), ((h as f64 * s).round() as usize).max(1));
    (super::contact_sheet::box_downscale(rgba, w, h, tw, th), tw, th)
}

/// Opaque-pixel box of a transparent render: [left, top, right, bottom] in its pixels, and the share of pixels covered.
fn alpha_box(rgba: &[u8], w: usize, h: usize) -> Option<([usize; 4], f64)> {
    let (mut x0, mut y0, mut x1, mut y1, mut n) = (usize::MAX, usize::MAX, 0, 0, 0usize);
    for y in 0..h { for x in 0..w { if rgba[(y * w + x) * 4 + 3] > 5 { n += 1; x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y); } } }
    if n == 0 { None } else { Some(([x0, y0, x1 + 1, y1 + 1], n as f64 / (w * h) as f64)) }
}

fn kind(c: &Clip) -> &'static str {
    match &c.source { Source::Media { .. } => "media", Source::Solid { .. } => "solid", Source::Gradient { .. } => "gradient", Source::Text { .. } => "text", Source::Shape { .. } => "shape",
        Source::Particles { .. } => "particles", Source::Precomp { .. } => "precomp", Source::Code(_) => "code", Source::Scene3D(_) => "scene3d", #[allow(unreachable_patterns)] _ => "other" }
}
fn title(c: &Clip) -> String {
    let n = c.name.as_ref().map(|n| format!("@{n}")).unwrap_or_else(|| c.id.clone());
    match &c.source { Source::Text { text, .. } => format!("{n} \"{}\"", text.chars().take(18).collect::<String>().replace('\n', " ")), _ => format!("{n} {}", kind(c)) }
}

/// Clips on screen at t, bottom (z=0) to top, with their track ids.
fn visible(p: &Project, t: f64) -> Vec<(usize, String, std::sync::Arc<Clip>)> {
    let mut v = vec![];
    for (z, tr) in p.tracks.iter().enumerate() { for c in &tr.clips {
        if t >= c.start && t < c.start + c.duration && crate::render::resolve_xform(p, c, t - c.start).opacity > 0.01 { v.push((z, tr.id.clone(), c.clone())); }
    } }
    v
}

fn r1(v: f64) -> f64 { (v * 10.0).round() / 10.0 }

// ------------------------------------------------------------------------------------------------ look

fn look(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t0 = std::time::Instant::now();
    let p = ss.project.clone();
    let end = super::export_manifest::project_end(&p);
    let t = num_arg(a, "time")?.clamp(0.0, (end - 1.0 / p.fps.max(1.0)).max(0.0));
    let px = opt_num(a, "px", 1280.0).clamp(320.0, 2048.0) as usize;
    let ov: Vec<String> = a.get("overlays").and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_else(|| ["ids", "safe", "motion_paths"].map(String::from).to_vec());
    let has = |k: &str| ov.iter().any(|x| x == k);
    let alpha = has("alpha");
    let (rgba, fw, fh) = render_rgba(ss, &p, t, alpha, false)?;
    let (mut img, iw, ih) = down(&rgba, fw, fh, px);
    if alpha { for y in 0..ih { for x in 0..iw { let i = (y * iw + x) * 4; let chk = if (x / 12 + y / 12) % 2 == 0 { 200.0 } else { 140.0 }; let al = img[i + 3] as f32 / 255.0; for k in 0..3 { img[i + k] = (img[i + k] as f32 * al + chk * (1.0 - al)).round() as u8; } img[i + 3] = 255; } } }
    let scopes = has("scopes");
    let panel_w = if scopes { (ih as f64 * 0.75) as usize } else { 0 };
    let mut cv = Canvas::new(iw + panel_w, ih, [18, 18, 20, 255]);
    cv.blit(&img, iw, ih, 0, 0);
    let s = iw as f64 / p.width as f64;
    let (cw, ch) = (p.width as f64, p.height as f64);
    let fsize = ((ih as f32) / 34.0).clamp(11.0, 22.0);
    if has("safe") {
        cv.rect(cw * 0.05 * s, ch * 0.05 * s, cw * 0.95 * s, ch * 0.95 * s, [255, 255, 255, 170], 1.0, true);
        cv.rect(cw * 0.035 * s, ch * 0.035 * s, cw * 0.965 * s, ch * 0.965 * s, [160, 160, 160, 140], 1.0, true);
    }
    // each visible clip alone: its true box
    let vis = visible(&p, t);
    let mut clips = vec![]; let mut warnings: Vec<String> = vec![];
    let limit = opt_num(a, "max_clips", 16.0).clamp(1.0, 40.0) as usize;
    let skip = vis.len().saturating_sub(limit);
    for (i, (z, track, c)) in vis.iter().enumerate().skip(skip) {
        let iso = super::view::view_of(&p, Some(&[c.id.clone()]), None);
        let (ir, w2, h2) = render_rgba(ss, &iso, t, true, true)?;
        let bx = alpha_box(&ir, w2, h2);
        let xf = crate::render::resolve_xform(&p, c, t - c.start);
        let lt = t - c.start;
        let keys: serde_json::Map<String, Value> = c.props.iter().filter(|(_, pr)| !pr.keys.is_empty()).map(|(n, pr)| (n.clone(), json!(pr.keys.iter().map(|k| crate::critic::r3(c.start + k.t)).collect::<Vec<_>>()))).collect();
        let mut e = json!({"id": c.id, "name": c.name, "kind": kind(c), "track": track, "z": z, "opacity": crate::critic::r3(xf.opacity),
            "clip_time": crate::critic::r3(lt), "transform": {"x": r1(xf.x), "y": r1(xf.y), "scale": crate::critic::r3(xf.scale), "rotation": r1(xf.rotation)}});
        if !keys.is_empty() { e["keyframes"] = Value::Object(keys); }
        let col = PALETTE[i % PALETTE.len()];
        match bx {
            None => { e["box"] = Value::Null; e["note"] = json!("nothing drawn at this time (fully transparent or off-screen)"); }
            Some((b, cover)) => {
                let k = cw / w2 as f64;
                let cb = [b[0] as f64 * k, b[1] as f64 * k, b[2] as f64 * k, b[3] as f64 * k];
                e["box"] = json!(cb.map(r1)); e["area_share"] = json!(crate::critic::r3(cover));
                if cb[0] <= 0.5 && cb[1] <= 0.5 && cb[2] >= cw - 0.5 && cb[3] >= ch - 0.5 { e["fills_frame"] = json!(true); }
                if has("ids") {
                    cv.rect(cb[0] * s, cb[1] * s, cb[2] * s - 1.0, cb[3] * s - 1.0, col, 2.0, false);
                    // the chip sits just outside the box (above it, else below), never over what it labels
                    let lh = fsize as f64 + 8.0;
                    let ly = if cb[1] * s >= lh + 2.0 { cb[1] * s - lh - 1.0 } else if cb[3] * s + lh + 2.0 <= ih as f64 { cb[3] * s + 1.0 } else { cb[1] * s + 2.0 };
                    cv.label(cb[0] * s, ly, &format!("{} z{}", title(c), z), fsize, [12, 12, 14, 255], col);
                }
                if matches!(c.source, Source::Text { .. }) && (cb[0] < cw * 0.05 || cb[1] < ch * 0.05 || cb[2] > cw * 0.95 || cb[3] > ch * 0.95) {
                    warnings.push(format!("{} is outside title safe", c.id));
                }
            }
        }
        // motion path of a keyed move: where the clip's box centre travels over the clip
        if has("motion_paths") && (c.props.get("x").map(|p| !p.keys.is_empty()).unwrap_or(false) || c.props.get("y").map(|p| !p.keys.is_empty()).unwrap_or(false)) {
            if let Some(b) = e["box"].as_array().map(|v| v.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect::<Vec<_>>()) {
                let (bcx, bcy) = ((b[0] + b[2]) / 2.0, (b[1] + b[3]) / 2.0);
                let n = 80usize; let mut prev: Option<(f64, f64)> = None;
                for j in 0..=n { let lt2 = c.duration * j as f64 / n as f64; let x2 = crate::render::resolve_xform(&p, c, lt2);
                    let (x, y) = ((bcx + x2.x - xf.x) * s, (bcy + x2.y - xf.y) * s);
                    if let Some((px0, py0)) = prev { cv.line(px0, py0, x, y, col, 1.5); } prev = Some((x, y)); }
                for pr in ["x", "y"] { if let Some(pp) = c.props.get(pr) { for k in &pp.keys { let x2 = crate::render::resolve_xform(&p, c, k.t); cv.dot((bcx + x2.x - xf.x) * s, (bcy + x2.y - xf.y) * s, 3.5, [255, 255, 255, 255]); } } }
                cv.dot(bcx * s, bcy * s, 5.0, col);
            }
        }
        clips.push(e);
    }
    if skip > 0 { warnings.push(format!("{skip} lower clip(s) not outlined (max_clips)")); }
    if scopes { draw_scopes(&mut cv, &img, iw, ih, iw); }
    cv.label(6.0, ih as f64 - fsize as f64 - 10.0, &format!("t={t:.3}s  {}x{}", p.width, p.height), fsize, [240, 240, 240, 255], [0, 0, 0, 190]);
    let path = save(ss, &cv, a.get("path").and_then(|v| v.as_str()), "look")?;
    clips.reverse(); // top of the stack first
    Ok(json!({"time": crate::critic::r3(t), "image": path, "image_size": [cv.w, cv.h], "canvas": [p.width, p.height],
        "safe": {"title": [r1(cw * 0.05), r1(ch * 0.05), r1(cw * 0.95), r1(ch * 0.95)], "action": [r1(cw * 0.035), r1(ch * 0.035), r1(cw * 0.965), r1(ch * 0.965)]},
        "clips": clips, "warnings": warnings, "overlays": ov, "ms": t0.elapsed().as_millis() as u64,
        "legend": "each clip outlined in its own colour with id (or @name), kind and z (stack order, 0 = bottom); boxes are exact (each clip rendered alone); dashed white = title safe, dashed grey = action safe; lines = motion paths, white dots = keyframes"}))
}

/// Waveform (top) and vectorscope (bottom) of the frame, in a panel at x0.
fn draw_scopes(cv: &mut Canvas, img: &[u8], w: usize, h: usize, x0: usize) {
    let pw = cv.w - x0; let half = h / 2;
    cv.fill(x0 as i64, 0, cv.w as i64, h as i64, [10, 10, 12, 255]);
    // waveform: columns of the image -> panel x, luma -> panel y
    let mut acc = vec![0u32; pw * half];
    for y in 0..h { for x in 0..w { let i = (y * w + x) * 4; let l = (0.2126 * img[i] as f64 + 0.7152 * img[i + 1] as f64 + 0.0722 * img[i + 2] as f64) / 255.0;
        let (px, py) = (x * pw / w, ((1.0 - l) * (half - 1) as f64) as usize); acc[py * pw + px] += 1; } }
    let mx = (*acc.iter().max().unwrap_or(&1)).max(1) as f32;
    for (k, v) in acc.iter().enumerate() { if *v > 0 { let a = ((*v as f32 / mx).sqrt() * 1.6).min(1.0); cv.put((x0 + k % pw) as i64, (k / pw) as i64, [120, 255, 150, 255], a); } }
    for q in [0.0, 0.5, 1.0] { let y = ((1.0 - q) * (half - 1) as f64) as i64; for x in x0..cv.w { cv.put(x as i64, y, [80, 80, 90, 255], 0.6); } }
    // vectorscope: Cb/Cr scatter in a circle
    let (cx, cy, r) = ((x0 + pw / 2) as f64, (half + (h - half) / 2) as f64, ((pw.min(h - half)) as f64 / 2.0 - 6.0).max(4.0));
    for a in 0..360 { let t = a as f64 * std::f64::consts::PI / 180.0; cv.put((cx + r * t.cos()) as i64, (cy + r * t.sin()) as i64, [80, 80, 90, 255], 0.8); }
    // skin-tone line: 123 degrees from the +Cb axis on a standard vectorscope
    let sk = 123f64.to_radians(); cv.line(cx, cy, cx + r * sk.cos(), cy - r * sk.sin(), [255, 190, 120, 255], 1.0);
    for y in (0..h).step_by(2) { for x in (0..w).step_by(2) { let i = (y * w + x) * 4; let (r8, g8, b8) = (img[i] as f64 / 255.0, img[i + 1] as f64 / 255.0, img[i + 2] as f64 / 255.0);
        let yy = 0.2126 * r8 + 0.7152 * g8 + 0.0722 * b8; let (cb, cr) = ((b8 - yy) / 1.8556, (r8 - yy) / 1.5748);
        cv.put((cx + cb * 2.0 * r) as i64, (cy - cr * 2.0 * r) as i64, [(r8 * 255.0) as u8, (g8 * 255.0) as u8, (b8 * 255.0) as u8, 255], 0.5); } }
}

// ------------------------------------------------------------------------------------------------ watch

fn watch(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t0 = std::time::Instant::now();
    let p = ss.project.clone();
    let end = super::export_manifest::project_end(&p);
    if end <= 0.0 { return Err("the timeline is empty".into()); }
    let fr = 1.0 / p.fps.max(1.0);
    let start = opt_num(a, "start", 0.0).clamp(0.0, end);
    let stop = opt_num(a, "end", (start + 2.0).min(end)).clamp(start + fr, end);
    let n = opt_num(a, "frames", 8.0).clamp(2.0, 16.0) as usize;
    let px = opt_num(a, "px", 640.0).clamp(240.0, 1280.0) as usize;
    let times: Vec<f64> = (0..n).map(|i| start + (stop - start - fr) * i as f64 / (n - 1) as f64).collect();
    let mut frames = vec![]; let (mut w, mut h) = (1, 1);
    for &t in &times { let (rgba, fw, fh) = render_rgba(ss, &p, t, false, true)?; let (d, dw, dh) = down(&rgba, fw, fh, px); w = dw; h = dh; frames.push(d); }
    // onion skin: later frames weigh more, so the direction of travel reads at a glance
    let mut onion = vec![0f32; w * h * 3]; let mut wsum = 0f32;
    for (i, f) in frames.iter().enumerate() { let wt = 0.35 + 0.65 * i as f32 / (n - 1) as f32; wsum += wt; for k in 0..w * h { for c in 0..3 { onion[k * 3 + c] += f[k * 4 + c] as f32 * wt; } } }
    let per_row = n.min(4);
    let tile_w = w / per_row; let tile_h = (h as f64 * tile_w as f64 / w as f64).round() as usize;
    let rows = n.div_ceil(per_row);
    let mut cv = Canvas::new(w, h + rows * (tile_h + 4) + 4, [18, 18, 20, 255]);
    for k in 0..w * h { let i = k * 4; for c in 0..3 { cv.px[i + c] = (onion[k * 3 + c] / wsum).round() as u8; } }
    // trajectories of everything keyed in the range, shaded by speed
    let s = w as f64 / p.width as f64;
    let mut elems = vec![];
    let fsize = ((h as f32) / 30.0).clamp(11.0, 20.0);
    for (ci, c) in p.tracks.iter().flat_map(|t| t.clips.iter()).enumerate() {
        if c.start >= stop || c.start + c.duration <= start { continue; }
        if !["x", "y", "scale", "rotation"].iter().any(|k| c.props.get(*k).map(|pr| !pr.keys.is_empty()).unwrap_or(false)) { continue; }
        let (a0, a1) = (start.max(c.start), stop.min(c.start + c.duration));
        let m = (((a1 - a0) / fr).ceil() as usize).clamp(2, 600);
        let pts: Vec<(f64, f64, f64)> = (0..=m).map(|j| { let tt = a0 + (a1 - a0) * j as f64 / m as f64; let x = crate::render::resolve_xform(&p, c, tt - c.start); (p.width as f64 / 2.0 + x.x, p.height as f64 / 2.0 + x.y, x.scale) }).collect();
        let dt = (a1 - a0) / m as f64;
        let sp: Vec<f64> = pts.windows(2).map(|w2| ((w2[1].0 - w2[0].0).powi(2) + (w2[1].1 - w2[0].1).powi(2)).sqrt() / dt).collect();
        let acc: Vec<f64> = sp.windows(2).map(|w2| (w2[1] - w2[0]).abs() / dt).collect();
        let vmax = sp.iter().cloned().fold(0.0, f64::max);
        let path_len: f64 = sp.iter().map(|v| v * dt).sum();
        // direction reversals along x (axis 0) or y (axis 1) with a real step either side
        let rev = |axis: usize| { let v: Vec<f64> = pts.iter().map(|q| if axis == 0 { q.0 } else { q.1 }).collect();
            v.windows(3).filter(|w3| { let (d0, d1) = (w3[1] - w3[0], w3[2] - w3[1]); d0.abs() > 0.3 && d1.abs() > 0.3 && d0.signum() != d1.signum() }).count() };
        let q = (sp.len() / 8).max(1);
        let (v_in, v_out) = (sp[..q].iter().sum::<f64>() / q as f64, sp[sp.len() - q..].iter().sum::<f64>() / q as f64);
        let eased_in = vmax > 1.0 && v_in < vmax * 0.35; let eased_out = vmax > 1.0 && v_out < vmax * 0.35;
        for (j, w2) in pts.windows(2).enumerate() {
            let f = if vmax > 0.0 { (sp[j] / vmax) as f32 } else { 0.0 };
            let col = [(60.0 + 195.0 * f) as u8, (120.0 + 60.0 * (1.0 - (2.0 * f - 1.0).abs())) as u8, (255.0 - 215.0 * f) as u8, 255];
            cv.line(w2[0].0 * s, w2[0].1 * s, w2[1].0 * s, w2[1].1 * s, col, 2.5);
        }
        if let (Some(f0), Some(fl)) = (pts.first(), pts.last()) { cv.dot(f0.0 * s, f0.1 * s, 4.0, [255, 255, 255, 255]); cv.dot(fl.0 * s, fl.1 * s, 6.0, [255, 60, 40, 255]);
            cv.label(fl.0 * s + 8.0, fl.1 * s - 8.0, &title(c), fsize, [12, 12, 14, 255], PALETTE[ci % PALETTE.len()]); }
        elems.push(json!({"id": c.id, "name": c.name, "kind": kind(c), "from_s": crate::critic::r3(a0), "to_s": crate::critic::r3(a1),
            "path_px": r1(path_len), "max_speed_px_s": r1(vmax), "mean_speed_px_s": r1(path_len / (a1 - a0).max(fr)), "max_accel_px_s2": r1(acc.iter().cloned().fold(0.0, f64::max)),
            "reversals": {"x": rev(0), "y": rev(1)}, "eased_in": eased_in, "eased_out": eased_out,
            "scale": [crate::critic::r3(pts[0].2), crate::critic::r3(pts[pts.len() - 1].2)],
            "verdict": if rev(0) + rev(1) >= 3 { "jittery: keeps reversing" } else if vmax > 1.0 && !eased_in && !eased_out { "mechanical: starts and stops at full speed (no ease)" } else if vmax <= 1.0 { "barely moves in this range" } else { "smooth" }}));
    }
    // the frames themselves, small, under the onion skin
    for (i, f) in frames.iter().enumerate() {
        let tl = super::contact_sheet::box_downscale(f, w, h, tile_w, tile_h);
        let (x, y) = ((i % per_row) * tile_w, h + 4 + (i / per_row) * (tile_h + 4));
        cv.blit(&tl, tile_w, tile_h, x, y);
        cv.label(x as f64 + 2.0, y as f64 + 2.0, &format!("{:.2}s", times[i]), (fsize * 0.75).max(10.0), [240, 240, 240, 255], [0, 0, 0, 170]);
    }
    let path = save(ss, &cv, a.get("path").and_then(|v| v.as_str()), "watch")?;
    Ok(json!({"range": [crate::critic::r3(start), crate::critic::r3(stop)], "frames": times.iter().map(|t| crate::critic::r3(*t)).collect::<Vec<_>>(), "image": path, "image_size": [cv.w, cv.h],
        "elements": elems, "ms": t0.elapsed().as_millis() as u64,
        "legend": "top: onion skin of the range (later frames stronger) with each keyed element's path, blue = slow, red = fast, white dot = start, red dot = end; bottom: the frames in order"}))
}

// ------------------------------------------------------------------------------------------------ compare_reference

#[derive(Default, Clone, Copy)]
struct Look { luma: f64, contrast: f64, sat: f64, warmth: f64, tint: f64 }
fn look_of(rgba: &[u8]) -> Look {
    let n = (rgba.len() / 4).max(1) as f64; let (mut l, mut l2, mut s, mut r, mut g, mut b) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    for p in rgba.chunks_exact(4) { let (rr, gg, bb) = (p[0] as f64 / 255.0, p[1] as f64 / 255.0, p[2] as f64 / 255.0); let y = 0.2126 * rr + 0.7152 * gg + 0.0722 * bb;
        l += y; l2 += y * y; s += rr.max(gg).max(bb) - rr.min(gg).min(bb); r += rr; g += gg; b += bb; }
    let (l, r, g, b) = (l / n, r / n, g / n, b / n);
    Look { luma: l, contrast: (l2 / n - l * l).max(0.0).sqrt(), sat: s / n, warmth: r - b, tint: g - (r + b) / 2.0 }
}
fn file_frame(path: &str, t: f64) -> Result<Vec<u8>, String> {
    let out = capture(std::process::Command::new("ffmpeg").args(["-v", "error", "-ss", &format!("{t:.3}"), "-i", path, "-frames:v", "1", "-vf", "scale=320:180:force_original_aspect_ratio=decrease,pad=320:180:-1:-1", "-f", "rawvideo", "-pix_fmt", "rgba", "-"]).stdin(std::process::Stdio::null()), 1 << 20, std::time::Duration::from_secs(30))?;
    if out.len() != 320 * 180 * 4 { return Err(format!("could not decode {path} at {t:.2} s")); }
    Ok(out)
}

fn compare_reference(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t0 = std::time::Instant::now();
    let p = ss.project.clone();
    let end = super::export_manifest::project_end(&p);
    let rng = a.get("range").and_then(|v| v.as_array()).ok_or("range: [start, end] of your edit to compare")?;
    let (s0, s1) = (rng.first().and_then(|v| v.as_f64()).unwrap_or(0.0).max(0.0), rng.get(1).and_then(|v| v.as_f64()).unwrap_or(end).min(end));
    if s1 <= s0 { return Err("range is empty".into()); }
    let ref_path = a.get("ref_path").and_then(|v| v.as_str()).map(String::from);
    let ref_start = opt_num(a, "ref_start", 0.0).max(0.0);
    if ref_path.is_none() && a.get("ref_start").is_none() { return Err("give ref_path (a video file) or ref_start (where the reference sits on this timeline)".into()); }
    let k = opt_num(a, "samples", 6.0).clamp(2.0, 16.0) as usize;
    let fr = 1.0 / p.fps.max(1.0);
    let (mut ours, mut theirs, mut act_o, mut act_t) = (vec![], vec![], 0.0, 0.0);
    for i in 0..k {
        let off = (s1 - s0 - fr) * (i as f64 + 0.5) / k as f64;
        let (o, ow, oh) = render_rgba(ss, &p, s0 + off, false, true)?; let (o, _, _) = down(&o, ow, oh, 320);
        let (o2, _, _) = { let (x, w2, h2) = render_rgba(ss, &p, (s0 + off + fr).min(end - fr), false, true)?; down(&x, w2, h2, 320) };
        act_o += o.iter().zip(o2.iter()).step_by(4).map(|(x, y)| x.abs_diff(*y) as f64).sum::<f64>() / (o.len() / 4) as f64;
        let (r, r2) = match &ref_path {
            Some(path) => (file_frame(path, ref_start + off)?, file_frame(path, ref_start + off + fr)?),
            None => { let (x, w2, h2) = render_rgba(ss, &p, ref_start + off, false, true)?; let (x2, w3, h3) = render_rgba(ss, &p, ref_start + off + fr, false, true)?; (down(&x, w2, h2, 320).0, down(&x2, w3, h3, 320).0) }
        };
        act_t += r.iter().zip(r2.iter()).step_by(4).map(|(x, y)| x.abs_diff(*y) as f64).sum::<f64>() / (r.len() / 4) as f64;
        ours.push(look_of(&o)); theirs.push(look_of(&r));
    }
    let avg = |v: &[Look]| { let n = v.len() as f64; Look { luma: v.iter().map(|x| x.luma).sum::<f64>() / n, contrast: v.iter().map(|x| x.contrast).sum::<f64>() / n, sat: v.iter().map(|x| x.sat).sum::<f64>() / n, warmth: v.iter().map(|x| x.warmth).sum::<f64>() / n, tint: v.iter().map(|x| x.tint).sum::<f64>() / n } };
    let (o, r) = (avg(&ours), avg(&theirs));
    let (ao, at) = (act_o / k as f64, act_t / k as f64);
    let mut todo: Vec<String> = vec![];
    let d = |name: &str, x: f64, y: f64, thr: f64, more: &str, less: &str, todo: &mut Vec<String>| { if x - y > thr { todo.push(format!("{name}: yours is higher ({x:.3} vs {y:.3}): {less}")); } else if y - x > thr { todo.push(format!("{name}: yours is lower ({x:.3} vs {y:.3}): {more}")); } };
    d("brightness", o.luma, r.luma, 0.04, "raise exposure (effects.add_effect exposure, or color_grade lift/gain)", "lower exposure", &mut todo);
    d("contrast", o.contrast, r.contrast, 0.03, "add contrast (add_effect contrast or an S-curve)", "reduce contrast", &mut todo);
    d("saturation", o.sat, r.sat, 0.04, "add saturation", "reduce saturation", &mut todo);
    d("warmth", o.warmth, r.warmth, 0.03, "warm it up (add_effect temperature, positive)", "cool it down (add_effect temperature, negative)", &mut todo);
    d("tint", o.tint, r.tint, 0.025, "push toward green (add_effect tint)", "push toward magenta (add_effect tint)", &mut todo);
    if at > 0.5 || ao > 0.5 { let ratio = (ao + 0.01) / (at + 0.01); if ratio > 1.6 { todo.push(format!("motion: yours moves {ratio:.1}x as much frame to frame: calmer moves or longer shots")); } else if ratio < 0.6 { todo.push(format!("motion: yours moves {:.1}x less frame to frame: more movement (punch-ins, camera moves) or faster cuts", 1.0 / ratio)); } }
    // full-reference SSIM/PSNR through compare_ranges (same frames, same time base)
    let b_src = match &ref_path { Some(path) => format!("file:{path}"), None => "composite".into() };
    let q = crate::commands::dispatch_nested(ss, "compare_ranges", &json!({"a": {"source": "composite", "start": s0}, "b": {"source": b_src, "start": ref_start}, "duration": crate::critic::r3((s1 - s0).min(60.0)), "max_points": 12, "quality": ["ssim", "psnr"]}));
    let quality = match q {
        Ok(v) => json!({"ssim_mean": v.pointer("/ssim/mean").cloned().unwrap_or(Value::Null), "ssim_min": v.pointer("/ssim/min").cloned().unwrap_or(Value::Null), "psnr_mean_db": v.pointer("/psnr/mean").cloned().unwrap_or(Value::Null)}),
        Err(e) if e.contains("size_mismatch") => json!({"skipped": "SSIM/PSNR need the same frame size on both sides (they compare the same shot pixel by pixel); colour and motion above still compare", "detail": e}),
        Err(e) => json!({"error": e}),
    };
    let f3 = |x: Look| json!({"brightness": crate::critic::r3(x.luma), "contrast": crate::critic::r3(x.contrast), "saturation": crate::critic::r3(x.sat), "warmth": crate::critic::r3(x.warmth), "tint": crate::critic::r3(x.tint)});
    Ok(json!({"yours": f3(o), "reference": f3(r), "motion_energy": {"yours": r1(ao), "reference": r1(at)}, "quality": quality,
        "close": todo.is_empty(), "to_match": todo, "samples": k, "ms": t0.elapsed().as_millis() as u64,
        "note": "colour and motion are averages over the samples; SSIM/PSNR compare the same moments pixel by pixel (1 = identical) and only mean something for the same shot"}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "look", description: "Look closely at one frame: the picture with every visible clip outlined at its exact on-screen box and labelled with its id/@name, kind and stack order, title/action safe areas, motion paths of keyed moves (white dots = keyframes), and optionally an alpha checkerboard or a waveform + vectorscope panel. Returns the image (shown to you) and per-clip boxes, transforms, opacity and keyframe times. Use it whenever the per-edit observe strip is not enough to tell what is where.", mutates: false,
        schema: obj(vec![("time", n("timeline seconds")), ("px", n("long side of the image, default 1280")),
            ("overlays", json!({"type":"array","items":{"type":"string","enum":["ids","safe","motion_paths","alpha","scopes"]},"description":"default ids, safe, motion_paths"})),
            ("max_clips", n("outline at most this many clips, top of the stack first (default 16)")), ("path", s("optional .png path"))], &["time"]),
        run: look },
    Command { name: "watch", description: "See motion in one picture: an onion skin of a time range with every keyframed element's path drawn on it (blue slow, red fast, white start, red end) and the frames in order underneath. Per element: path length, max/mean speed, max acceleration, direction reversals, whether it eases in/out, and a verdict (smooth, mechanical, jittery). Use it to judge animation and motion graphics without video.", mutates: false,
        schema: obj(vec![("start", n("seconds, default 0")), ("end", n("seconds, default start + 2")), ("frames", n("frames in the strip, 2..16, default 8")), ("px", n("onion skin width, default 640")), ("path", s("optional .png path"))], &[]),
        run: watch },
    Command { name: "compare_reference", description: "Compare part of your edit with a reference (a video file, or another stretch of this timeline): brightness, contrast, saturation, warmth and tint, frame-to-frame motion energy, and SSIM/PSNR at the same moments, with a to_match list saying what to change (and which effect does it). Use it to make a grade, a look or a pace match a reference.", mutates: false,
        schema: obj(vec![("range", json!({"type":"array","items":{"type":"number"},"description":"[start, end] of your edit"})), ("ref_path", s("reference video file")), ("ref_start", n("where the reference starts (in the file, or on this timeline when no ref_path), default 0")), ("samples", n("moments compared, 2..16, default 6"))], &["range"]),
        run: compare_reference },
] }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::critic::testkit::*;
    #[test]
    fn look_boxes_are_exact_and_labelled() {
        let mut ss = session();
        let t1 = track(&mut ss); let t2 = track(&mut ss);
        solid(&mut ss, &t1, 0.0, 4.0);
        let tx = text(&mut ss, &t2, "Hello", 0.0, 4.0);
        call(&mut ss, "set_property", json!({"clip_id": tx, "property": "x", "value": 400.0}));
        let dir = std::env::temp_dir().join(format!("kimo-look-test-{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("look.png").to_string_lossy().into_owned();
        let r = look(&mut ss, &json!({"time": 1.0, "path": path, "px": 640})).unwrap();
        assert!(std::path::Path::new(&path).is_file());
        let clips = r["clips"].as_array().unwrap();
        assert_eq!(clips.len(), 2);
        assert_eq!(clips[0]["id"], tx.as_str(), "top of the stack first");
        let b: Vec<f64> = clips[0]["box"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        let cx = (b[0] + b[2]) / 2.0;
        assert!((cx - (960.0 + 400.0)).abs() < 40.0, "text centred 400 px right of centre: {b:?}");
        assert_eq!(clips[1]["fills_frame"], true, "the solid fills the frame");
        assert_eq!(ss.media.len(), 1, "the image goes to the agent");
    }
    #[test]
    fn watch_measures_a_keyed_move() {
        let mut ss = session();
        let t = track(&mut ss);
        let c = solid(&mut ss, &t, 0.0, 3.0);
        call(&mut ss, "set_property", json!({"clip_id": c, "property": "scale", "value": 0.3}));
        call(&mut ss, "set_keyframe", json!({"clip_id": c, "property": "x", "time": 0.0, "value": -600.0, "ease": "in_out"}));
        call(&mut ss, "set_keyframe", json!({"clip_id": c, "property": "x", "time": 2.0, "value": 600.0, "ease": "in_out"}));
        let dir = std::env::temp_dir().join(format!("kimo-watch-test-{}", std::process::id())); std::fs::create_dir_all(&dir).unwrap();
        let r = watch(&mut ss, &json!({"start": 0.0, "end": 2.0, "frames": 4, "px": 320, "path": dir.join("w.png").to_string_lossy()})).unwrap();
        let e = &r["elements"][0];
        assert!((e["path_px"].as_f64().unwrap() - 1200.0).abs() < 30.0, "{e}");
        assert_eq!(e["eased_in"], true); assert_eq!(e["eased_out"], true);
        assert_eq!(e["verdict"], "smooth");
    }
    #[test]
    fn compare_two_stretches_of_the_timeline() {
        let mut ss = session();
        let t = track(&mut ss);
        call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.2, 0.2, 0.25], "start": 0, "duration": 2}));
        call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "solid", "color": [0.9, 0.6, 0.3], "start": 2, "duration": 2}));
        let r = compare_reference(&mut ss, &json!({"range": [0.0, 2.0], "ref_start": 2.0, "samples": 2})).unwrap();
        let todo = r["to_match"].as_array().unwrap().iter().map(|x| x.as_str().unwrap().to_string()).collect::<Vec<_>>().join(" | ");
        assert!(todo.contains("brightness: yours is lower") && todo.contains("warm it up"), "{todo}");
        assert_eq!(r["close"], false);
    }
}
