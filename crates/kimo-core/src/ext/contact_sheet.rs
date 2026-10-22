//! contact_sheet (RV-25 s1): render several timeline times into ONE labelled PNG grid so an agent can look at a whole edit in one image.
//! Frames come from the same cached render path as render_frame, one at a time; each is converted to 8-bit and box-downscaled (integer area filter)
//! into its tile, so a tile equals the downscaled single render_frame at that time. Hard caps: 16 tiles, tile_px 64..320, 64 keyframe-derived times.
use super::kit::*;
use crate::commands::Media;
use crate::effects::Frame;

const MAX_TILES: usize = 16; const KEYFRAME_CAP: usize = 64;

/// 5x7 glyphs for the label alphabet (rows top to bottom, bit 4 = leftmost).
fn glyph(c: char) -> [u8; 7] { match c {
    '0' => [14, 17, 19, 21, 25, 17, 14], '1' => [4, 12, 4, 4, 4, 4, 14], '2' => [14, 17, 1, 2, 4, 8, 31], '3' => [30, 1, 1, 14, 1, 1, 30], '4' => [2, 6, 10, 18, 31, 2, 2],
    '5' => [31, 16, 30, 1, 1, 17, 14], '6' => [6, 8, 16, 30, 17, 17, 14], '7' => [31, 1, 2, 4, 8, 8, 8], '8' => [14, 17, 17, 14, 17, 17, 14], '9' => [14, 17, 17, 15, 1, 2, 12],
    '.' => [0, 0, 0, 0, 0, 12, 12], '-' => [0, 0, 0, 31, 0, 0, 0], 's' => [0, 0, 15, 16, 14, 1, 30], 't' => [8, 8, 30, 8, 8, 9, 6], '=' => [0, 0, 31, 0, 31, 0, 0], ':' => [0, 12, 12, 0, 12, 12, 0],
    _ => [0; 7] } }
fn draw_label(buf: &mut [u8], bw: usize, bh: usize, x0: usize, y0: usize, text: &str, sc: usize) {
    let tw = text.chars().count() * 6 * sc + 2 * sc; let th = 7 * sc + 2 * sc;
    for y in 0..th { for x in 0..tw { let (px, py) = (x0 + x, y0 + y); if px < bw && py < bh { let i = (py * bw + px) * 4; buf[i] = 0; buf[i + 1] = 0; buf[i + 2] = 0; buf[i + 3] = 255; } } }
    for (k, ch) in text.chars().enumerate() { let g = glyph(ch); for (ry, row) in g.iter().enumerate() { for rx in 0..5 { if row >> (4 - rx) & 1 == 1 {
        for dy in 0..sc { for dx in 0..sc { let (px, py) = (x0 + sc + (k * 6 + rx) * sc + dx, y0 + sc + ry * sc + dy); if px < bw && py < bh { let i = (py * bw + px) * 4; buf[i] = 255; buf[i + 1] = 255; buf[i + 2] = 255; } } } } } } }
}
/// Integer area (box) downscale of RGBA8: each destination pixel is the rounded mean of the source pixels it covers.
pub fn box_downscale(src: &[u8], sw: usize, sh: usize, tw: usize, th: usize) -> Vec<u8> {
    let mut out = vec![255u8; tw * th * 4];
    for y in 0..th { let (y0, y1) = (y * sh / th, ((y + 1) * sh / th).max(y * sh / th + 1).min(sh)); for x in 0..tw { let (x0, x1) = (x * sw / tw, ((x + 1) * sw / tw).max(x * sw / tw + 1).min(sw));
        let (mut r, mut g, mut b) = (0u64, 0u64, 0u64); for yy in y0..y1 { for xx in x0..x1 { let i = (yy * sw + xx) * 4; r += src[i] as u64; g += src[i + 1] as u64; b += src[i + 2] as u64; } }
        let c = ((y1 - y0) * (x1 - x0)) as u64; let o = (y * tw + x) * 4; out[o] = ((r + c / 2) / c) as u8; out[o + 1] = ((g + c / 2) / c) as u8; out[o + 2] = ((b + c / 2) / c) as u8; } }
    out
}
/// Pick `k` of `v` evenly by order, always including the first and the last.
fn pick_even<T: Clone>(v: &[T], k: usize) -> Vec<T> { if v.len() <= k || k < 2 { return if k == 1 && !v.is_empty() { vec![v[0].clone()] } else { v.to_vec() }; } (0..k).map(|i| v[i * (v.len() - 1) / (k - 1)].clone()).collect() }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "contact_sheet", mutates: false,
        description: "Optional isolate (draw only these clips) / hide (draw all except these) take clip ids or @names; render-time only, the project is not changed, and a child keeps its parent-resolved pose while hiding a parent hides only that clip's own pixels. Render several timeline times into ONE labelled PNG grid (path always full-size file; MCP clients also get the image inline). Give times[], or start+end+n, or at_cuts:true (first frame of every clip), or keyframes_of (clip id or list: the keyframe times of those clips, clip-local keys converted to timeline time and clamped into the clip). Times are merged, sorted and de-duplicated (half-frame tolerance). Hard caps: 16 tiles (extra times are picked evenly, first and last always kept; all_times lists every time so you can page with times[]), tile_px 64..320, keyframe-derived times capped at 64 by even stride (keyframe_candidates reports the count before the cap). Each tile is the box-downscale of the real rendered frame.",
        schema: obj(vec![("isolate", crate::ext::view::list_schema(crate::ext::view::ISOLATE_DOC)), ("hide", crate::ext::view::list_schema(crate::ext::view::HIDE_DOC)), ("path", s("output .png path")), ("times", json!({"type":"array","items":{"type":"number"},"description":"timeline seconds"})), ("start", n("range start, with end and n")), ("end", n("range end")), ("n", n("tiles in start..end, default 6, max 16")),
            ("keyframes_of", json!({"description":"clip id or array of clip ids","oneOf":[{"type":"string"},{"type":"array","items":{"type":"string"}}]})), ("at_cuts", json!({"type":"boolean","description":"also sample every cut: the first frame of each clip (cut-boundary sampling), merged with the other times"})), ("cols", n("grid columns 1..4, default auto (3x3 / 4x4 for 9 / 16 tiles)")), ("tile_px", n("tile long edge in px, 64..320, default 320 (max 320x180 cells at 16:9)")),
            ("labels", json!({"type":"string","enum":["time","time+props","none"],"description":"default time (timestamp drawn on each tile). time+props also lists the properties keyed at that time in the result."})), ("keyframe_cap", n("max keyframe-derived times, 1..64, default 64"))], &["path"]),
        run: |ss, a| crate::ext::view::with_view(ss, a, |ss| {
            let p = str_arg(a, "path")?; crate::render::check_still_path(p, true)?; let fps = ss.project.fps.max(1.0); let tol = 0.5 / fps; let dur = ss.project.duration();
            let label_mode = a.get("labels").and_then(|v| v.as_str()).unwrap_or("time"); if !["time", "time+props", "none"].contains(&label_mode) { return Err("labels must be one of time, time+props, none".into()); }
            let mut times: Vec<(f64, Option<String>)> = vec![];
            if let Some(ts) = a.get("times").and_then(|v| v.as_array()) { for v in ts { let t = v.as_f64().filter(|t| t.is_finite()).ok_or("times must be finite numbers")?; times.push((t, None)); } }
            if a.get("start").is_some() || a.get("end").is_some() { let (st, en) = (opt_num(a, "start", 0.0), opt_num(a, "end", dur)); if !st.is_finite() || !en.is_finite() { return Err("start/end must be finite".into()); } let k = (opt_num(a, "n", 6.0) as usize).clamp(1, MAX_TILES);
                for i in 0..k { times.push((if k == 1 { st } else { st + (en - st) * i as f64 / (k - 1) as f64 }, None)); } }
            if a.get("at_cuts").and_then(|v| v.as_bool()).unwrap_or(false) { for tr in &ss.project.tracks { for c in &tr.clips { times.push((c.start, None)); } } }
            let (mut kf_candidates, mut kf_capped) = (0usize, false); let mut kf_ids: Vec<String> = vec![];
            if let Some(v) = a.get("keyframes_of") { match v { Value::String(x) => kf_ids.push(x.clone()), Value::Array(xs) => for x in xs { kf_ids.push(x.as_str().ok_or("keyframes_of must be a clip id or an array of clip ids")?.to_string()); }, _ => return Err("keyframes_of must be a clip id or an array of clip ids".into()) } }
            let mut kf: Vec<(f64, Option<String>)> = vec![];
            for id in &kf_ids { let c = ss.project.find_clip(id).ok_or_else(|| format!("no clip '{id}'"))?; let (c0, c1) = (c.start, c.start + c.duration);
                for pr in c.props.values() { for k in &pr.keys { if k.t.is_finite() { kf.push(((c0 + k.t).clamp(c0, c1), Some(id.clone()))); } } } }
            kf.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap()); kf.dedup_by(|x, y| (x.0 - y.0).abs() < tol); kf_candidates = kf.len();
            let cap = (opt_num(a, "keyframe_cap", KEYFRAME_CAP as f64) as usize).clamp(1, KEYFRAME_CAP); if kf.len() > cap { kf = pick_even(&kf, cap); kf_capped = true; }
            times.extend(kf);
            if times.is_empty() { return Err("give times, start+end+n, at_cuts or keyframes_of".into()); }
            let dmax = dur.max(0.0); for t in times.iter_mut() { t.0 = t.0.clamp(0.0, dmax); }
            times.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap()); times.dedup_by(|x, y| (x.0 - y.0).abs() < tol);
            let all_times: Vec<f64> = times.iter().map(|t| t.0).collect(); let total = times.len(); if times.len() > MAX_TILES { times = pick_even(&times, MAX_TILES); }
            let (pw, ph) = (ss.project.width as usize, ss.project.height as usize); if pw == 0 || ph == 0 { return Err("project has no canvas size".into()); }
            let tile_px = (opt_num(a, "tile_px", 320.0) as usize).clamp(64, 320); let sc = tile_px as f64 / pw.max(ph) as f64; let (tw, th) = (((pw as f64 * sc).round() as usize).max(1), ((ph as f64 * sc).round() as usize).max(1));
            let k = times.len(); let cols = (opt_num(a, "cols", (k as f64).sqrt().ceil()) as usize).clamp(1, 4).min(k); let rows = k.div_ceil(cols); let (gap, sw, sh) = (4usize, cols * tw + (cols + 1) * 4, rows * th + (rows + 1) * 4);
            crate::guard::reserve(sw * sh * 4 + sw * sh * 16 + tw * th * 4); // u8 sheet + the f32 frame handed to the PNG writer + one tile
            let mut sheet = vec![0u8; sw * sh * 4]; for px in sheet.chunks_mut(4) { px[0] = 24; px[1] = 24; px[2] = 24; px[3] = 255; }
            let mut tiles = vec![];
            for (i, (t, cid)) in times.iter().enumerate() {
                crate::render::prepare_at(&ss.project, &mut ss.cache, Some(*t))?;
                let mut f = crate::framecache::render_cached(&mut ss.fcache, &ss.project, *t, &ss.cache)?; crate::color::pipeline::output(&ss.project, &mut f)?;
                let (fw, fh) = (f.w, f.h); let u8s = crate::render::to_rgba8(&f); drop(f);
                let tile = box_downscale(&u8s, fw, fh, tw, th); drop(u8s);
                let (cx, cy) = (gap + (i % cols) * (tw + gap), gap + (i / cols) * (th + gap));
                for y in 0..th { let d = ((cy + y) * sw + cx) * 4; sheet[d..d + tw * 4].copy_from_slice(&tile[y * tw * 4..(y + 1) * tw * 4]); }
                if label_mode != "none" { draw_label(&mut sheet, sw, sh, cx + 2, cy + 2, &format!("t={:.3}s", t), (tw / 160).clamp(1, 3)); }
                let mut e = json!({"index": i, "timeline_time": t, "x": cx, "y": cy});
                // clip-local time + keyed props for the clip(s) whose keyframes asked for this tile
                let mut props = vec![]; let owner: Vec<String> = match cid { Some(c) => vec![c.clone()], None => vec![] };
                for id in owner.iter().chain(kf_ids.iter()) { if let Some(c) = ss.project.find_clip(id) { let lt = t - c.start; if lt >= -tol && lt <= c.duration + tol {
                    let keyed: Vec<Value> = c.props.iter().filter(|(_, pr)| pr.keys.iter().any(|k| (k.t - lt).abs() < tol)).map(|(nm, pr)| json!({"prop": nm, "value": pr.at(lt)})).collect();
                    if !keyed.is_empty() { e["clip_id"] = json!(id); e["clip_time"] = json!(lt); if label_mode == "time+props" { props.extend(keyed); } break; } } } }
                if !props.is_empty() { e["props"] = json!(props); } tiles.push(e);
            }
            let mut fr = Frame::new(sw, sh); for (o, px) in fr.d.iter_mut().zip(sheet.iter()) { *o = *px as f32 / 255.0; } drop(sheet);
            crate::render::write_image(&fr, p)?;
            ss.media.push(Media { path: p.to_string(), w: sw as u32, h: sh as u32, mime: "image/png" });
            let mut out = json!({"path": p, "width": sw, "height": sh, "cols": cols, "rows": rows, "tile": {"w": tw, "h": th}, "tiles": tiles, "tile_count": k});
            if total > k { out["omitted"] = json!(total - k); out["note"] = json!(format!("{total} distinct times, showing {k} picked evenly (first and last kept); all_times lists them all, page with times[]")); }
            if total > k || kf_candidates > 0 { out["all_times"] = json!(all_times); }
            if kf_candidates > 0 { out["keyframe_candidates"] = json!(kf_candidates); if kf_capped { out["keyframe_cap_applied"] = json!(cap); } }
            Ok(out) }) },
] }
