//! Data-driven chart templates fed from CSV, with built-in animation: add_chart (line | bar_race | donut | treemap | waffle).
//!
//! Every chart is ONE group clip (precomp) on a new track, so it moves, scales and fades as a unit; inside are ordinary path
//! shapes, rect shapes and text clips, so recolor_svg_layer / draw_on_svg_layer / set_keyframe keep working on the parts.
//! Animation is baked as keyframes when the chart is made:
//!   line     each series draws itself on (Trim Paths) with a dot riding the pen tip; optional area fill, value labels
//!   bar_race bars rank, grow and swap places over time (one CSV row per time step); values and period roll as live numbers
//!   donut    segments sweep round in order (trimmed thick strokes with square ends), labels pop at each segment's end
//!   treemap  squarified layout; cells pop in, largest first
//!   waffle   10x10 (or `cells`) grid filled category by category
//! Numbers use crate::numfmt (grouping none|western|indian|compact|compact_indian), so 1,00,000 style works everywhere,
//! and running values are `{num:...}` text tokens (one clip each, resolved per frame).
use super::kit::*;
use super::svg_tools::{bool_opt, effect, key, new_clip, new_track, num_opt, path_clip, precomp, rgb_opt, set_prop, timing};
use crate::keyframe::Ease;
use crate::model::*;
use crate::numfmt::{format as fmt, Grouping};

pub const PALETTE: [[f64; 3]; 10] = [[0.31, 0.55, 1.0], [1.0, 0.54, 0.24], [0.18, 0.77, 0.65], [0.95, 0.76, 0.31], [0.90, 0.34, 0.43], [0.61, 0.48, 1.0], [0.36, 0.78, 1.0], [0.64, 0.84, 0.36], [1.0, 0.48, 0.75], [0.69, 0.72, 0.76]];

/// Minimal RFC-4180 CSV: quotes, escaped quotes, CRLF. Returns rows of cells (blank lines dropped).
pub fn parse_csv(text: &str) -> Result<Vec<Vec<String>>, String> {
    let mut rows = vec![]; let mut row = vec![]; let mut cell = String::new(); let mut q = false; let mut it = text.chars().peekable();
    while let Some(c) = it.next() {
        if q { if c == '"' { if it.peek() == Some(&'"') { cell.push('"'); it.next(); } else { q = false; } } else { cell.push(c); } continue; }
        match c { '"' => q = true, ',' | ';' | '\t' if c == ',' || !text.contains(',') => { row.push(cell.trim().to_string()); cell.clear(); }
            '\n' => { row.push(cell.trim().to_string()); cell.clear(); if row.iter().any(|x| !x.is_empty()) { rows.push(std::mem::take(&mut row)); } else { row.clear(); } }
            '\r' => {}, _ => cell.push(c) }
    }
    if q { return Err("CSV has an unclosed quote".into()); }
    row.push(cell.trim().to_string()); if row.iter().any(|x| !x.is_empty()) { rows.push(row); }
    if rows.len() < 2 { return Err("CSV needs a header row and at least one data row".into()); } Ok(rows)
}
/// A number from a CSV cell: tolerates thousands separators (both systems), currency signs, %, spaces.
pub fn cell_num(s: &str) -> Option<f64> { let t: String = s.chars().filter(|c| !matches!(c, ',' | ' ' | '₹' | '$' | '€' | '£' | '%' | '\u{a0}')).collect(); if t.is_empty() { return None; } t.parse::<f64>().ok().filter(|v| v.is_finite()) }

fn csv_arg(a: &Value) -> Result<Vec<Vec<String>>, String> {
    match (a.get("csv").and_then(|v| v.as_str()), a.get("csv_path").and_then(|v| v.as_str())) {
        (Some(_), Some(_)) => Err("give either 'csv' (inline text) or 'csv_path', not both".into()),
        (Some(t), None) => parse_csv(t), (None, Some(p)) => parse_csv(&std::fs::read_to_string(p).map_err(|e| format!("cannot read '{p}': {e}"))?),
        (None, None) => Err("give 'csv' (inline CSV text, header row first) or 'csv_path'".into()),
    }
}

struct Style { text: [f64; 3], grid: [f64; 3], font: String, bold: String, k: f64, grouping: Grouping, gname: String, decimals: usize, prefix: String, suffix: String, palette: Vec<[f64; 3]>, deva: bool }
impl Style {
    fn num(&self, v: f64) -> String { let s = fmt(v, self.decimals, self.grouping); let s = if self.deva { crate::numfmt::devanagari_digits(&s) } else { s }; format!("{}{}{}", self.prefix, s, self.suffix) }
    fn live(&self, keys: &[(f64, f64)], ease: &str) -> String { format!("{}{}{}", self.prefix, crate::numfmt::token(self.decimals, &self.gname, ease, keys, self.deva), self.suffix) }
    fn col(&self, i: usize) -> [f64; 3] { self.palette[i % self.palette.len()] }
}
fn style(ss: &Session, a: &Value) -> Result<Style, String> {
    let light = match a.get("theme").and_then(|v| v.as_str()).unwrap_or("dark") { "dark" => false, "light" => true, o => return Err(format!("theme must be dark|light, got '{o}'")) };
    let gname = a.get("grouping").and_then(|v| v.as_str()).unwrap_or("western").to_string(); let grouping = Grouping::parse(&gname)?;
    let decimals = num_opt(a, "decimals")?.unwrap_or(0.0); if !(0.0..=4.0).contains(&decimals) { return Err("decimals must be 0..4".into()); }
    let palette = match a.get("palette") { None | Some(Value::Null) => PALETTE.to_vec(), Some(Value::Array(v)) if !v.is_empty() => v.iter().enumerate().map(|(i, c)| rgb_opt(&json!({"c": c}), "c").and_then(|x| x.ok_or(format!("palette[{i}] is not a colour")))).collect::<Result<_, _>>()?, _ => return Err("palette must be a non-empty list of colours".into()) };
    let font = a.get("font").and_then(|v| v.as_str()).unwrap_or("Inter").to_string();
    Ok(Style { text: rgb_opt(a, "text_color")?.unwrap_or(if light { [0.1, 0.11, 0.13] } else { [0.95, 0.96, 0.97] }), grid: if light { [0.1, 0.1, 0.12] } else { [1.0; 3] },
        bold: a.get("title_font").and_then(|v| v.as_str()).unwrap_or(&font).to_string(), font, k: ss.project.height as f64 / 720.0, grouping, gname, decimals: decimals as usize,
        prefix: a.get("prefix").and_then(|v| v.as_str()).unwrap_or("").into(), suffix: a.get("suffix").and_then(|v| v.as_str()).unwrap_or("").into(), palette, deva: bool_opt(a, "devanagari_digits")?.unwrap_or(false) })
}
fn text(ss: &mut Session, st: &Style, s: &str, size: f64, color: [f64; 3], x: f64, y: f64, dur: f64, font: Option<&str>) -> Clip {
    let mut c = new_clip(ss, Source::Text { text: s.into(), font: font.unwrap_or(&st.font).into(), size, color, letter_spacing: 0.0, stroke_width: 0.0, stroke_color: [0.0; 3], shadow: 0.0, align: "center".into() }, 0.0, dur);
    set_prop(&mut c, "x", x); set_prop(&mut c, "y", y); c
}
fn text_w(st: &Style, s: &str, size: f64) -> f64 { crate::text::measure(s, &st.font, size, 0.0, 0.0, "left", 4096.0).map(|v| v.0).unwrap_or(s.chars().count() as f64 * size * 0.55) }
fn rect(ss: &mut Session, w: f64, h: f64, fill: [f64; 3], r: f64, x: f64, y: f64, dur: f64) -> Clip { let mut c = new_clip(ss, Source::Shape { shape: "rect".into(), width: w.max(0.5), height: h.max(0.5), fill, stroke_width: 0.0, stroke_color: [0.0; 3], sides: 4, corner_radius: r.min(w / 2.0).min(h / 2.0).max(0.0) }, 0.0, dur); set_prop(&mut c, "x", x); set_prop(&mut c, "y", y); c }
fn line_d(pts: &[[f64; 2]]) -> String { let mut s = format!("M{:.2} {:.2}", pts[0][0], pts[0][1]); for p in &pts[1..] { s += &format!(" L{:.2} {:.2}", p[0], p[1]); } s }
fn fade_in(c: &mut Clip, t: f64, d: f64) { fade_to(c, t, d, 1.0) }
fn fade_to(c: &mut Clip, t: f64, d: f64, v: f64) { key(c, "opacity", t, 0.0, Ease::Out); key(c, "opacity", t + d, v, Ease::Hold); }
fn pop_in(c: &mut Clip, t: f64, d: f64) { key(c, "scale", t, 0.0, Ease::OutBack); key(c, "scale", t + d, 1.0, Ease::Hold); key(c, "opacity", t, 0.0, Ease::Out); key(c, "opacity", t + d * 0.4, 1.0, Ease::Hold); }

/// "Nice" axis ticks covering lo..hi (about `n` intervals).
pub fn nice_ticks(lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let (lo, hi) = if (hi - lo).abs() < 1e-12 { (lo - 1.0, hi + 1.0) } else { (lo, hi) };
    let raw = (hi - lo) / n.max(1) as f64; let mag = 10f64.powf(raw.log10().floor()); let f = raw / mag;
    let step = mag * if f <= 1.0 { 1.0 } else if f <= 2.0 { 2.0 } else if f <= 2.5 { 2.5 } else if f <= 5.0 { 5.0 } else { 10.0 };
    let (a, b) = ((lo / step).floor() as i64, (hi / step).ceil() as i64); (a..=b).map(|i| i as f64 * step).collect()
}

/// Squarified treemap (Bruls et al.): areas proportional to values in rect [x,y,w,h]. Returns rects in input order.
pub fn squarify(values: &[f64], r: [f64; 4]) -> Vec<[f64; 4]> {
    let total: f64 = values.iter().sum(); let mut out = vec![[0.0; 4]; values.len()]; if total <= 0.0 { return out; }
    let mut idx: Vec<usize> = (0..values.len()).collect(); idx.sort_by(|a, b| values[*b].partial_cmp(&values[*a]).unwrap_or(std::cmp::Ordering::Equal));
    let scale = r[2] * r[3] / total; let areas: Vec<f64> = idx.iter().map(|i| values[*i] * scale).collect();
    let (mut x, mut y, mut w, mut h) = (r[0], r[1], r[2], r[3]); let mut i = 0;
    let worst = |row: &[f64], side: f64| -> f64 { let s: f64 = row.iter().sum(); let (mx, mn) = row.iter().fold((0.0f64, f64::MAX), |(a, b), v| (a.max(*v), b.min(*v))); if s <= 0.0 || mn <= 0.0 { return f64::MAX; } ((side * side * mx) / (s * s)).max((s * s) / (side * side * mn)) };
    while i < areas.len() {
        let side = w.min(h); let mut j = i + 1; while j < areas.len() && worst(&areas[i..=j], side) <= worst(&areas[i..j], side) { j += 1; }
        let row = &areas[i..j]; let s: f64 = row.iter().sum();
        if w >= h { let cw = if h > 0.0 { s / h } else { 0.0 }; let mut yy = y; for (k, a) in row.iter().enumerate() { let ch = if cw > 0.0 { a / cw } else { 0.0 }; out[idx[i + k]] = [x, yy, cw, ch]; yy += ch; } x += cw; w -= cw; }
        else { let rh = if w > 0.0 { s / w } else { 0.0 }; let mut xx = x; for (k, a) in row.iter().enumerate() { let cw = if rh > 0.0 { a / rh } else { 0.0 }; out[idx[i + k]] = [xx, y, cw, rh]; xx += cw; } y += rh; h -= rh; }
        i = j;
    }
    out
}

/// Largest-remainder split of `cells` between `values`.
pub fn waffle_counts(values: &[f64], cells: usize) -> Vec<usize> {
    let total: f64 = values.iter().sum(); if total <= 0.0 { return vec![0; values.len()]; }
    let exact: Vec<f64> = values.iter().map(|v| v / total * cells as f64).collect(); let mut n: Vec<usize> = exact.iter().map(|e| e.floor() as usize).collect();
    let mut left = cells - n.iter().sum::<usize>(); let mut order: Vec<usize> = (0..values.len()).collect(); order.sort_by(|a, b| (exact[*b] - exact[*b].floor()).partial_cmp(&(exact[*a] - exact[*a].floor())).unwrap_or(std::cmp::Ordering::Equal));
    for i in order { if left == 0 { break; } n[i] += 1; left -= 1; } n
}

struct Frame { cx: f64, cy: f64, w: f64, h: f64, dur: f64, anim: f64, t0: f64 }
fn chart_frame(ss: &Session, a: &Value, dur: f64) -> Result<Frame, String> {
    let (cw, ch) = (ss.project.width as f64, ss.project.height as f64);
    let w = num_opt(a, "width")?.unwrap_or(cw * 0.78); let h = num_opt(a, "height")?.unwrap_or(ch * 0.72); if w <= 0.0 || h <= 0.0 { return Err("width/height must be > 0".into()); }
    let t0 = num_opt(a, "delay")?.unwrap_or(0.2).max(0.0); let anim = num_opt(a, "animate")?.unwrap_or((dur * 0.6).min(3.0)); if anim <= 0.0 { return Err("animate must be > 0 seconds".into()); }
    Ok(Frame { cx: num_opt(a, "x")?.unwrap_or(0.0), cy: num_opt(a, "y")?.unwrap_or(0.0), w, h, dur, anim: anim.min((dur - t0).max(0.1)), t0 })
}
/// Title at the top-left of the chart box; returns how much vertical room it took.
fn title(ss: &mut Session, st: &Style, a: &Value, f: &Frame, out: &mut Vec<Clip>) -> f64 {
    let Some(t) = a.get("title").and_then(|v| v.as_str()).filter(|t| !t.is_empty()) else { return 0.0 };
    let sz = 30.0 * st.k; let w = text_w(st, t, sz); let mut c = text(ss, st, t, sz, st.text, f.cx - f.w / 2.0 + w / 2.0, f.cy - f.h / 2.0 + sz * 0.6, f.dur, Some(&st.bold.clone())); fade_in(&mut c, 0.0, 0.4); out.push(c); sz * 1.8
}

fn line_chart(ss: &mut Session, a: &Value, rows: &[Vec<String>], st: &Style, f: &Frame) -> Result<(Vec<Clip>, Value), String> {
    let head = &rows[0]; if head.len() < 2 { return Err("line chart CSV: first column = x labels, then one numeric column per series".into()); }
    let data = &rows[1..]; if data.len() < 2 { return Err("line chart needs at least 2 data rows".into()); }
    let series: Vec<(String, Vec<f64>)> = (1..head.len()).map(|j| -> Result<_, String> { Ok((head[j].clone(), data.iter().enumerate().map(|(i, r)| r.get(j).and_then(|c| cell_num(c)).ok_or(format!("row {} column '{}' is not a number: {:?}", i + 2, head[j], r.get(j)))).collect::<Result<Vec<_>, _>>()?)) }).collect::<Result<_, _>>()?;
    let mut cl = vec![]; let top_pad = title(ss, st, a, f, &mut cl);
    let all: Vec<f64> = series.iter().flat_map(|s| s.1.iter().copied()).collect(); let (mn, mx) = all.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
    let zero = bool_opt(a, "y_from_zero")?.unwrap_or(mn >= 0.0); let ticks = nice_ticks(if zero { mn.min(0.0) } else { mn }, mx, 5); let (y0v, y1v) = (ticks[0], *ticks.last().unwrap());
    let fs = 17.0 * st.k; let ylw = ticks.iter().map(|t| text_w(st, &st.num(*t), fs)).fold(0.0, f64::max);
    let legend = series.len() > 1; let leg_h = if legend { fs * 2.0 } else { 0.0 };
    let (l, r) = (f.cx - f.w / 2.0 + ylw + 14.0 * st.k, f.cx + f.w / 2.0 - 40.0 * st.k); let (t, b) = (f.cy - f.h / 2.0 + top_pad + leg_h + 10.0 * st.k, f.cy + f.h / 2.0 - fs * 2.2);
    let px = |i: usize| l + (r - l) * i as f64 / (data.len() - 1) as f64; let py = |v: f64| b - (b - t) * (v - y0v) / (y1v - y0v);
    // grid + y labels
    for tv in &ticks { let y = py(*tv); let mut g = path_clip(ss, &format!("M{l:.2} {y:.2} L{r:.2} {y:.2}"), false, [r - l, 1.0], None, 1.0 * st.k.max(1.0), st.grid, 0.0, f.dur); set_prop(&mut g, "stroke_alpha", if tv.abs() < 1e-12 { 0.45 } else { 0.16 }); fade_in(&mut g, 0.0, 0.4); cl.push(g);
        let s = st.num(*tv); let mut tl = text(ss, st, &s, fs, st.text, l - 10.0 * st.k - text_w(st, &s, fs) / 2.0, y, f.dur, None); fade_to(&mut tl, 0.0, 0.4, 0.75); cl.push(tl); }
    // x labels (at most ~8, evenly sampled, always first and last)
    // every k-th label (evenly spaced, at most ~8), plus the last one when it does not crowd its neighbour
    let n = data.len(); let kk = n.div_ceil(8).max(1); let mut shown: std::collections::BTreeSet<usize> = (0..n).step_by(kk).collect(); if (n - 1) % kk >= kk.div_ceil(2) || kk == 1 { shown.insert(n - 1); }
    for i in shown { let mut x = text(ss, st, &data[i][0], fs, st.text, px(i), b + fs * 1.2, f.dur, None); fade_to(&mut x, 0.0, 0.4, 0.75); cl.push(x); }
    let smooth = bool_opt(a, "smooth")?.unwrap_or(false); let area = bool_opt(a, "area")?.unwrap_or(series.len() == 1); let lw = num_opt(a, "line_width")?.unwrap_or(4.0) * st.k; let fps = ss.project.fps.max(1.0);
    let mut lx = l; let mut out_series = vec![];
    for (si, (name, vals)) in series.iter().enumerate() {
        let col = st.col(si); let pts: Vec<[f64; 2]> = vals.iter().enumerate().map(|(i, v)| [px(i), py(*v)]).collect();
        let d = if smooth { crate::paths::smooth_d(&pts, false, 0.5) } else { line_d(&pts) }; let path = crate::paths::Path::parse(&d, false)?;
        let (s0, s1) = (f.t0 + si as f64 * 0.15, f.t0 + si as f64 * 0.15 + f.anim);
        if area { let mut ad = d.clone(); ad += &format!(" L{:.2} {b:.2} L{:.2} {b:.2} Z", pts[pts.len() - 1][0], pts[0][0]); let mut ar = path_clip(ss, &ad, false, [r - l, b - t], Some(col), 0.0, col, 0.0, f.dur); set_prop(&mut ar, "fill_alpha", 0.18);
            key(&mut ar, "opacity", s1 - 0.2, 0.0, Ease::Out); key(&mut ar, "opacity", s1 + 0.5, 1.0, Ease::Hold); ar.name = Some(format!("{name} area")); cl.push(ar); }
        let mut c = path_clip(ss, &d, false, [r - l, b - t], None, lw, col, 0.0, f.dur); let mut tr = effect("shape_trim", &[("start", 0.0), ("end", 1.0), ("offset", 0.0)]);
        { let e = tr.params.get_mut("end").unwrap(); e.set_key(s0, 0.0, Ease::InOut); e.set_key(s1, 1.0, Ease::Hold); } c.effects.push(tr); c.name = Some(name.clone()); let lid = c.id.clone(); cl.push(c);
        // pen-tip dot: one key per frame at the arc-length point the trim has reached
        let dr = lw * 1.6; let mut dot = path_clip(ss, &format!("M{dr} 0 A{dr} {dr} 0 1 1 {} 0 A{dr} {dr} 0 1 1 {dr} 0 Z", -dr), false, [2.0 * dr, 2.0 * dr], Some(col), 2.0 * st.k, [1.0; 3], 0.0, f.dur);
        let nfr = ((s1 - s0) * fps).ceil().max(1.0) as usize;
        for i in 0..=nfr { let tt = s0 + (s1 - s0) * i as f64 / nfr as f64; let u = Ease::InOut.apply(i as f64 / nfr as f64); let p = point_at(&path, u); key(&mut dot, "x", tt, p[0], Ease::Linear); key(&mut dot, "y", tt, p[1], Ease::Linear); }
        key(&mut dot, "opacity", s0, 0.0, Ease::Linear); key(&mut dot, "opacity", s0 + 0.1, 1.0, Ease::Hold); dot.name = Some(format!("{name} dot")); cl.push(dot);
        // final value label next to the end point
        let last = *vals.last().unwrap(); let s = st.num(last); let ep = pts[pts.len() - 1]; let mut vl = text(ss, st, &s, fs * 1.1, col, ep[0] + 12.0 * st.k + text_w(st, &s, fs * 1.1) / 2.0, ep[1] - fs * 0.9, f.dur, Some(&st.bold.clone())); pop_in(&mut vl, s1, 0.35); cl.push(vl);
        if legend { let lh = fs; let mut sw = rect(ss, lh * 1.4, lh * 0.35, col, lh * 0.17, lx + lh * 0.7, t - leg_h * 0.75, f.dur); fade_in(&mut sw, 0.1, 0.4); cl.push(sw);
            let nw = text_w(st, name, fs); let mut nt = text(ss, st, name, fs, st.text, lx + lh * 1.7 + nw / 2.0, t - leg_h * 0.75, f.dur, None); fade_in(&mut nt, 0.1, 0.4); cl.push(nt); lx += lh * 2.6 + nw; }
        out_series.push(json!({"name": name, "clip_id": lid, "draw": [s0, s1]}));
    }
    Ok((cl, json!({"series": out_series, "y_range": [y0v, y1v]})))
}
fn point_at(p: &crate::paths::Path, u: f64) -> [f64; 2] {
    let target = u.clamp(0.0, 1.0) * p.total; let i = p.cum.partition_point(|c| *c <= target).saturating_sub(1).min(p.segs.len() - 1); let s = p.segs[i];
    let l = (s[2] - s[0]).hypot(s[3] - s[1]).max(1e-12); let f = ((target - p.cum[i]) / l).clamp(0.0, 1.0); [s[0] + (s[2] - s[0]) * f, s[1] + (s[3] - s[1]) * f]
}

fn bar_race(ss: &mut Session, a: &Value, rows: &[Vec<String>], st: &Style, f: &Frame) -> Result<(Vec<Clip>, Value), String> {
    let head = &rows[0]; if head.len() < 3 { return Err("bar_race CSV: first column = time label (year), then one numeric column per competitor; one row per time step".into()); }
    let data = &rows[1..]; let names: Vec<String> = head[1..].to_vec(); let ne = names.len();
    let vals: Vec<Vec<f64>> = data.iter().enumerate().map(|(i, r)| (1..head.len()).map(|j| r.get(j).and_then(|c| cell_num(c)).ok_or(format!("row {} column '{}' is not a number", i + 2, head[j]))).collect()).collect::<Result<_, _>>()?;
    let top = num_opt(a, "top_n")?.unwrap_or(8.0).clamp(1.0, 20.0) as usize; let top = top.min(ne);
    let hold = num_opt(a, "hold")?.unwrap_or(1.0).max(0.0); let span = (f.dur - f.t0 - hold).max(0.5); let steps = data.len(); let step = if steps > 1 { span / (steps - 1) as f64 } else { span };
    let mut cl = vec![]; let top_pad = title(ss, st, a, f, &mut cl);
    let fs = 18.0 * st.k; let namew = names.iter().map(|n| text_w(st, n, fs)).fold(0.0, f64::max);
    let (l, r) = (f.cx - f.w / 2.0 + namew + 18.0 * st.k, f.cx + f.w / 2.0 - 120.0 * st.k); let (t, b) = (f.cy - f.h / 2.0 + top_pad, f.cy + f.h / 2.0);
    let slot = (b - t) / top as f64; let bh = slot * 0.78;
    let tk = |i: usize| f.t0 + i as f64 * step;
    // rank and scale per step
    let mut rank = vec![vec![0usize; ne]; steps]; let mut vmax = vec![0.0f64; steps];
    for (i, v) in vals.iter().enumerate() { let mut o: Vec<usize> = (0..ne).collect(); o.sort_by(|x, y| v[*y].partial_cmp(&v[*x]).unwrap_or(std::cmp::Ordering::Equal).then(x.cmp(y))); for (rk, e) in o.iter().enumerate() { rank[i][*e] = rk; } vmax[i] = v.iter().cloned().fold(0.0, f64::max).max(1e-9); }
    let slot_y = |rk: usize| if rk < top { t + slot * (rk as f64 + 0.5) } else { b + slot * 0.8 };
    let mut ent = vec![];
    for e in 0..ne {
        let col = st.col(e); let wl = |i: usize| ((vals[i][e] / vmax[i]).max(0.0) * (r - l)).max(1.0);
        // bar: a path whose right edge is keyed (path keys), sitting at the left axis
        // centred on its own middle (a shape is drawn about the canvas centre before its x/y offset, so an off-centre path would be cut at the canvas edge); x keeps the left edge on the axis
        let bar_d = |w: f64| format!("M{:.2} {:.2} L{:.2} {:.2} L{:.2} {:.2} L{:.2} {:.2} Z", -w / 2.0, -bh / 2.0, w / 2.0, -bh / 2.0, w / 2.0, bh / 2.0, -w / 2.0, bh / 2.0);
        let mut bar = path_clip(ss, &bar_d(wl(0)), false, [r - l, bh], Some(col), 0.0, col, 0.0, f.dur);
        let keys: Vec<(f64, String, String)> = (0..steps).map(|i| (tk(i), bar_d(wl(i)), "in_out".to_string())).collect();
        bar.effects[0].strings.insert("keys".into(), serde_json::to_string(&keys).unwrap()); bar.name = Some(names[e].clone());
        let mut nm = text(ss, st, &names[e], fs, st.text, l - 10.0 * st.k - text_w(st, &names[e], fs) / 2.0, 0.0, f.dur, None);
        let vkeys: Vec<(f64, f64)> = (0..steps).map(|i| (tk(i), vals[i][e])).collect(); let mut vt = text(ss, st, &st.live(&vkeys, "in_out"), fs, st.text, 0.0, 0.0, f.dur, Some(&st.bold.clone()));
        for i in 0..steps { let y = slot_y(rank[i][e]); let vis = if rank[i][e] < top { 1.0 } else { 0.0 };
            for c in [&mut bar, &mut nm, &mut vt] { key(c, "y", tk(i), y, Ease::InOut); key(c, "opacity", tk(i), vis, Ease::InOut); } key(&mut bar, "x", tk(i), l + wl(i) / 2.0, Ease::InOut);
            let vw = text_w(st, &st.num(vals[i][e]), fs); key(&mut vt, "x", tk(i), l + wl(i) + 10.0 * st.k + vw / 2.0, Ease::InOut); }
        ent.push(json!({"name": names[e], "clip_id": bar.id})); cl.push(bar); cl.push(nm); cl.push(vt);
    }
    let mut axis = path_clip(ss, &format!("M{l:.2} {t:.2} L{l:.2} {b:.2}"), false, [1.0, b - t], None, 2.0 * st.k, st.grid, 0.0, f.dur); set_prop(&mut axis, "stroke_alpha", 0.5); cl.insert(0, axis);
    // period: a rolling number when the labels are numeric (years), else one text per step
    let ps = 64.0 * st.k; let (px, py) = (f.cx + f.w / 2.0 - 10.0 * st.k, b - ps * 0.6);
    let nums: Option<Vec<f64>> = data.iter().map(|r| cell_num(&r[0])).collect();
    match nums { Some(n) => { let keys: Vec<(f64, f64)> = n.iter().enumerate().map(|(i, v)| (tk(i), *v)).collect(); let s = crate::numfmt::token(0, "none", "linear", &keys, st.deva); let w = text_w(st, &data[steps - 1][0], ps); let mut c = text(ss, st, &s, ps, st.text, px - w / 2.0, py, f.dur, Some(&st.bold.clone())); set_prop(&mut c, "opacity", 0.55); cl.push(c); }
        None => for i in 0..steps { let w = text_w(st, &data[i][0], ps); let mut c = text(ss, st, &data[i][0], ps, st.text, px - w / 2.0, py, f.dur, Some(&st.bold.clone())); c.start = if i == 0 { 0.0 } else { tk(i) - step / 2.0 }; c.duration = if i + 1 == steps { f.dur - c.start } else { tk(i) + step / 2.0 - c.start }; set_prop(&mut c, "opacity", 0.55); cl.push(c); } }
    Ok((cl, json!({"entities": ent, "steps": steps, "step_seconds": step})))
}

fn arc_d(cx: f64, cy: f64, r: f64, a0: f64, a1: f64) -> String {
    // angles in degrees, 0 = 12 o'clock, clockwise; cubic per <= 90 degrees
    let p = |a: f64| { let t = (a - 90.0).to_radians(); [cx + r * t.cos(), cy + r * t.sin()] };
    let n = ((a1 - a0).abs() / 90.0).ceil().max(1.0) as usize; let st = (a1 - a0) / n as f64; let k = 4.0 / 3.0 * (st.to_radians() / 4.0).tan();
    let mut d = { let q = p(a0); format!("M{:.2} {:.2}", q[0], q[1]) };
    for i in 0..n { let (b0, b1) = (a0 + st * i as f64, a0 + st * (i + 1) as f64); let (q0, q1) = (p(b0), p(b1)); let (t0, t1) = ((b0 - 90.0).to_radians(), (b1 - 90.0).to_radians());
        let c1 = [q0[0] - k * r * t0.sin(), q0[1] + k * r * t0.cos()]; let c2 = [q1[0] + k * r * t1.sin(), q1[1] - k * r * t1.cos()];
        d += &format!(" C{:.2} {:.2} {:.2} {:.2} {:.2} {:.2}", c1[0], c1[1], c2[0], c2[1], q1[0], q1[1]); }
    d
}

fn label_value(rows: &[Vec<String>], what: &str) -> Result<Vec<(String, f64)>, String> {
    let v: Vec<(String, f64)> = rows[1..].iter().enumerate().map(|(i, r)| Ok((r.first().cloned().unwrap_or_default(), r.get(1).and_then(|c| cell_num(c)).ok_or(format!("{what} CSV: row {} needs label,value with a number, got {r:?}", i + 2))?))).collect::<Result<_, String>>()?;
    if v.iter().any(|x| x.1 < 0.0) { return Err(format!("{what} values must be >= 0")); } if v.iter().map(|x| x.1).sum::<f64>() <= 0.0 { return Err(format!("{what} values sum to 0")); } Ok(v)
}

fn donut(ss: &mut Session, a: &Value, rows: &[Vec<String>], st: &Style, f: &Frame) -> Result<(Vec<Clip>, Value), String> {
    let v = label_value(rows, "donut")?; let total: f64 = v.iter().map(|x| x.1).sum();
    let mut cl = vec![]; let top_pad = title(ss, st, a, f, &mut cl);
    let (cx, cy) = (f.cx, f.cy + top_pad / 2.0); let ro = ((f.h - top_pad) / 2.0 * 0.78).min(f.w / 2.0 * 0.5); let hole = num_opt(a, "hole")?.unwrap_or(0.58).clamp(0.0, 0.95); let th = ro * (1.0 - hole); let rm = ro - th / 2.0;
    let gap = num_opt(a, "gap_degrees")?.unwrap_or(1.2).clamp(0.0, 10.0); let fs = 18.0 * st.k; let pad = 3.0;
    let mut ang = 0.0; let mut segs = vec![];
    for (i, (name, val)) in v.iter().enumerate() {
        let sweep = val / total * 360.0; let (a0, a1) = (ang + gap / 2.0, ang + sweep - gap / 2.0); let (s0, s1) = (f.t0 + f.anim * ang / 360.0, f.t0 + f.anim * (ang + sweep) / 360.0); ang += sweep;
        if a1 <= a0 { continue; }
        // the arc runs `pad` degrees past each end and is trimmed back, so both ends are square (trim cuts are flat; only true path ends are round)
        let (e0, e1) = (a0 - pad, a1 + pad); let span = e1 - e0; let (u0, u1) = (pad / span, 1.0 - pad / span);
        let col = st.col(i); let mut c = path_clip(ss, &arc_d(cx, cy, rm, e0, e1), false, [2.0 * rm, 2.0 * rm], None, th, col, 0.0, f.dur);
        let mut tr = effect("shape_trim", &[("start", u0), ("end", u1), ("offset", 0.0)]); { let e = tr.params.get_mut("end").unwrap(); e.set_key(s0, u0, Ease::Linear); e.set_key(s1, u1, Ease::Hold); }
        c.effects.push(tr); c.name = Some(name.clone()); segs.push(json!({"name": name, "clip_id": c.id, "share": val / total})); cl.push(c);
        // label outside the ring at the middle angle
        let mid = ((a0 + a1) / 2.0 - 90.0).to_radians(); let lr = ro + 26.0 * st.k; let pct = format!("{:.0}%", val / total * 100.0); let lab = format!("{name}  {pct}");
        let lw = text_w(st, &lab, fs); let (lx, ly) = (cx + lr * mid.cos(), cy + lr * mid.sin()); let lx = lx + mid.cos().signum() * lw / 2.0 * (mid.cos().abs() > 0.2) as i32 as f64;
        let mut lt = text(ss, st, &lab, fs, st.text, lx, ly, f.dur, None); pop_in(&mut lt, s1, 0.3); cl.push(lt);
    }
    if hole > 0.3 { let ts = (ro * hole * 0.5).min(56.0 * st.k); let ts = ts.min(ts * (1.5 * ro * hole) / text_w(st, &st.num(total), ts).max(1.0)); let s = st.live(&[(f.t0, 0.0), (f.t0 + f.anim, total)], "out"); let mut c = text(ss, st, &s, ts, st.text, cx, cy - ts * 0.15, f.dur, Some(&st.bold.clone())); fade_in(&mut c, f.t0, 0.3); cl.push(c);
        if let Some(cap) = a.get("center_label").and_then(|v| v.as_str()) { let mut c = text(ss, st, cap, fs, st.text, cx, cy + ts * 0.75, f.dur, None); fade_in(&mut c, f.t0, 0.3); cl.push(c); } }
    Ok((cl, json!({"segments": segs, "total": total})))
}

fn treemap(ss: &mut Session, a: &Value, rows: &[Vec<String>], st: &Style, f: &Frame) -> Result<(Vec<Clip>, Value), String> {
    let v = label_value(rows, "treemap")?; let mut cl = vec![]; let top_pad = title(ss, st, a, f, &mut cl);
    let r0 = [f.cx - f.w / 2.0, f.cy - f.h / 2.0 + top_pad, f.w, f.h - top_pad]; let rects = squarify(&v.iter().map(|x| x.1).collect::<Vec<_>>(), r0); let total: f64 = v.iter().map(|x| x.1).sum();
    let gap = 3.0 * st.k; let mut order: Vec<usize> = (0..v.len()).collect(); order.sort_by(|x, y| v[*y].1.partial_cmp(&v[*x].1).unwrap_or(std::cmp::Ordering::Equal));
    let stagger = f.anim / v.len().max(1) as f64; let mut cells = vec![];
    for (rank, &i) in order.iter().enumerate() { let rc = rects[i]; if rc[2] <= 2.0 * gap || rc[3] <= 2.0 * gap { continue; }
        let (x, y) = (rc[0] + rc[2] / 2.0, rc[1] + rc[3] / 2.0); let col = st.col(i); let t = f.t0 + rank as f64 * stagger;
        let mut c = rect(ss, rc[2] - 2.0 * gap, rc[3] - 2.0 * gap, col, 6.0 * st.k, x, y, f.dur); pop_in(&mut c, t, 0.45); c.name = Some(v[i].0.clone()); cells.push(json!({"name": v[i].0, "clip_id": c.id, "rect": rc})); cl.push(c);
        // label sized to the cell; skipped when it would not fit
        let fsz = (rc[2].min(rc[3]) * 0.16).clamp(12.0 * st.k, 34.0 * st.k); let lum = 0.2126 * col[0] + 0.7152 * col[1] + 0.0722 * col[2]; let ink = if lum > 0.55 { [0.07, 0.08, 0.1] } else { [1.0; 3] };
        let s2 = format!("{}  {:.0}%", st.num(v[i].1), v[i].1 / total * 100.0);
        // the name wraps onto two lines when that fits better, then shrinks to the cell (down to 11 px at 720p); too-small cells stay unlabelled
        let room = rc[2] - 4.0 * gap; let name = &v[i].0; let words: Vec<&str> = name.split(' ').collect();
        let (label, lines) = if words.len() > 1 && text_w(st, name, fsz) > room { let h = words.len().div_ceil(2); (format!("{}\n{}", words[..h].join(" "), words[h..].join(" ")), 2.0) } else { (name.clone(), 1.0) };
        let widest = label.split('\n').map(|l| text_w(st, l, fsz)).fold(0.0, f64::max); let fsz = if widest > room { fsz * room / widest } else { fsz };
        if fsz >= 11.0 * st.k && fsz * (1.3 * lines + 1.3) < rc[3] { let off = fsz * 0.6 * lines; let mut n1 = text(ss, st, &label, fsz, ink, x, y - off * 0.5 - fsz * 0.1, f.dur, Some(&st.bold.clone())); fade_in(&mut n1, t + 0.25, 0.3); cl.push(n1);
            if text_w(st, &s2, fsz * 0.75) < room { let mut n2 = text(ss, st, &s2, fsz * 0.75, ink, x, y + off * 0.5 + fsz * 0.55, f.dur, None); fade_in(&mut n2, t + 0.3, 0.3); cl.push(n2); } } }
    Ok((cl, json!({"cells": cells, "total": total})))
}

fn waffle(ss: &mut Session, a: &Value, rows: &[Vec<String>], st: &Style, f: &Frame) -> Result<(Vec<Clip>, Value), String> {
    let v = label_value(rows, "waffle")?; let cols = num_opt(a, "columns")?.unwrap_or(10.0).clamp(2.0, 40.0) as usize; let cells = num_opt(a, "cells")?.unwrap_or(100.0).clamp(4.0, 400.0) as usize;
    let rows_n = cells.div_ceil(cols); let counts = waffle_counts(&v.iter().map(|x| x.1).collect::<Vec<_>>(), cells); let total: f64 = v.iter().map(|x| x.1).sum();
    let mut cl = vec![]; let top_pad = title(ss, st, a, f, &mut cl);
    let fs = 19.0 * st.k; let legend_w = v.iter().map(|x| text_w(st, &format!("{}  {:.0}%", x.0, x.1 / total * 100.0), fs)).fold(0.0, f64::max) + fs * 2.0;
    let gw = (f.w - legend_w - 30.0 * st.k).max(f.w * 0.4); let gh = f.h - top_pad; let cs = (gw / cols as f64).min(gh / rows_n as f64); let gap = cs * 0.12;
    let (gx0, gy0) = (f.cx - f.w / 2.0, f.cy - f.h / 2.0 + top_pad + (gh - cs * rows_n as f64) / 2.0);
    let per = f.anim / cells as f64; let mut idx = 0; let mut out = vec![];
    for (ci, n) in counts.iter().enumerate() { let col = st.col(ci); let first = idx;
        for _ in 0..*n { let (r, c) = (idx / cols, idx % cols); let mut q = rect(ss, cs - gap, cs - gap, col, cs * 0.18, gx0 + cs * (c as f64 + 0.5), gy0 + cs * (r as f64 + 0.5), f.dur); pop_in(&mut q, f.t0 + idx as f64 * per, 0.3); cl.push(q); idx += 1; }
        // legend row
        let ly = gy0 + cs * 0.5 + ci as f64 * fs * 1.9; let lx = gx0 + cs * cols as f64 + 30.0 * st.k; let lab = format!("{}  {:.0}%", v[ci].0, v[ci].1 / total * 100.0);
        let mut sw = rect(ss, fs * 0.9, fs * 0.9, col, fs * 0.2, lx + fs * 0.45, ly, f.dur); let tt = f.t0 + first as f64 * per; fade_in(&mut sw, tt, 0.3); cl.push(sw);
        let mut lt = text(ss, st, &lab, fs, st.text, lx + fs * 1.4 + text_w(st, &lab, fs) / 2.0, ly, f.dur, None); fade_in(&mut lt, tt, 0.3); cl.push(lt);
        out.push(json!({"name": v[ci].0, "cells": n})); }
    for _ in idx..cells { let (r, c) = (idx / cols, idx % cols); let mut q = rect(ss, cs - gap, cs - gap, st.grid, cs * 0.18, gx0 + cs * (c as f64 + 0.5), gy0 + cs * (r as f64 + 0.5), f.dur); set_prop(&mut q, "fill_alpha", 0.12); cl.push(q); idx += 1; }
    Ok((cl, json!({"categories": out, "cells": cells})))
}

fn add_chart(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let kind = str_arg(a, "type")?.to_string(); let rows = csv_arg(a)?; let st = style(ss, a)?; let (start, dur) = timing(ss, a)?; let f = chart_frame(ss, a, dur)?;
    let (clips, info) = match kind.as_str() { "line" => line_chart(ss, a, &rows, &st, &f)?, "bar_race" => bar_race(ss, a, &rows, &st, &f)?, "donut" => donut(ss, a, &rows, &st, &f)?, "treemap" => treemap(ss, a, &rows, &st, &f)?, "waffle" => waffle(ss, a, &rows, &st, &f)?,
        o => return Err(format!("type must be line|bar_race|donut|treemap|waffle, got '{o}'")) };
    let n = clips.len(); let inner = new_track(ss, &format!("{kind} chart"), clips); let mut g = precomp(ss, vec![inner], start, dur, &format!("{kind} chart")); g.tags = vec!["chart".into()];
    let gid = g.id.clone(); let t = new_track(ss, &format!("{kind} chart"), vec![g]); let tid = t.id.clone(); ss.project.tracks.push(t);
    Ok(json!({"clip_id": gid, "track_id": tid, "parts": n, "type": kind, "start": start, "duration": dur, "data": info}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_chart", description: "Animated data chart from CSV, as one group clip on a new track. type: line (first column x labels, one numeric column per series; draws itself on with a pen-tip dot, area fill for one series, legend, end values) | bar_race (one row per time step: first column the period e.g. year, one column per competitor; bars re-rank and grow, values and period roll) | donut (label,value rows; segments sweep in, labels pop, total counts up in the hole) | treemap (label,value; squarified, cells pop largest first) | waffle (label,value; 10x10 grid fills by category, legend). Numbers: grouping western (default) | indian (1,00,000) | compact | compact_indian (12 L, 3.4 Cr) | none, decimals, prefix (e.g. ₹), suffix, devanagari_digits. Timing: delay then animate seconds inside the clip. Recolour parts with recolor_svg_layer; restyle via the returned clip ids.", mutates: true,
        schema: obj(vec![("type", json!({"type":"string","enum":["line","bar_race","donut","treemap","waffle"]})), ("csv", s("inline CSV text, header row first")), ("csv_path", s("path to a .csv file")),
            ("start", n("timeline seconds, default 0")), ("duration", n("clip seconds, default to end of project (min 5)")), ("delay", n("seconds before the animation starts, default 0.2")), ("animate", n("seconds the build animation takes (line/donut/treemap/waffle), default min(3, 60% of duration)")),
            ("hold", n("bar_race: seconds the last step holds, default 1")), ("top_n", n("bar_race: bars shown, default 8")),
            ("x", n("px offset of the chart centre")), ("y", n("px offset (down +)")), ("width", n("px, default 78% of frame")), ("height", n("px, default 72% of frame")),
            ("title", s("optional title")), ("theme", json!({"type":"string","enum":["dark","light"]})), ("palette", json!({"type":"array","description":"series colours: [r,g,b] 0..1 or CSS strings"})), ("text_color", json!({"description":"[r,g,b] or CSS"})),
            ("font", s("font family, default Inter (use e.g. 'Nirmala UI' for Devanagari labels)")), ("title_font", s("font for the title and highlighted numbers")),
            ("grouping", json!({"type":"string","enum":["western","indian","compact","compact_indian","none"]})), ("decimals", n("0..4")), ("prefix", s("e.g. ₹ or $")), ("suffix", s("e.g. % or  cr")), ("devanagari_digits", json!({"type":"boolean"})),
            ("smooth", json!({"type":"boolean","description":"line: smooth curve"})), ("area", json!({"type":"boolean","description":"line: shaded area (default for one series)"})), ("line_width", n("line: px at 720p, default 4")), ("y_from_zero", json!({"type":"boolean"})),
            ("hole", n("donut: inner radius fraction 0..0.95, default 0.58")), ("gap_degrees", n("donut: gap between segments")), ("center_label", s("donut: caption under the total")),
            ("cells", n("waffle: number of cells, default 100")), ("columns", n("waffle: columns, default 10"))], &["type"]),
        run: add_chart },
] }

#[cfg(test)]
mod tests {
    use super::*;
    fn ss() -> Session { let mut s = Session::new(); s.project.width = 640; s.project.height = 360; s }
    fn run(ss: &mut Session, a: Value) -> Result<Value, String> { crate::commands::dispatch(ss, "add_chart", &a) }

    #[test]
    fn csv_parsing() {
        let r = parse_csv("a,b\r\n\"x, y\",\"1,00,000\"\n\n\"q\"\"t\",2\n").unwrap(); assert_eq!(r, vec![vec!["a", "b"], vec!["x, y", "1,00,000"], vec!["q\"t", "2"]]);
        assert_eq!(cell_num("1,00,000"), Some(100000.0)); assert_eq!(cell_num("₹ 2,500"), Some(2500.0)); assert_eq!(cell_num("12%"), Some(12.0)); assert_eq!(cell_num("n/a"), None);
        assert!(parse_csv("only,header").is_err()); assert!(parse_csv("a\n\"oops").is_err());
    }
    #[test]
    fn layout_helpers() {
        assert_eq!(nice_ticks(0.0, 97.0, 5), vec![0.0, 20.0, 40.0, 60.0, 80.0, 100.0]);
        let r = squarify(&[6.0, 6.0, 4.0, 3.0, 2.0, 2.0, 1.0], [0.0, 0.0, 6.0, 4.0]);
        let area: f64 = r.iter().map(|x| x[2] * x[3]).sum(); assert!((area - 24.0).abs() < 1e-9); assert!((r[0][2] * r[0][3] - 6.0).abs() < 1e-9);
        for x in &r { assert!(x[0] >= -1e-9 && x[1] >= -1e-9 && x[0] + x[2] <= 6.0 + 1e-9 && x[1] + x[3] <= 4.0 + 1e-9); }
        assert_eq!(waffle_counts(&[1.0, 1.0, 1.0], 100), vec![34, 33, 33]); assert_eq!(waffle_counts(&[50.0, 30.0, 20.0], 100), vec![50, 30, 20]);
    }
    #[test]
    fn every_type_builds_one_group_clip() {
        let lv = "label,value\nRent,45000\nFood,22000\nTravel,8000\nSavings,25000\n";
        for (t, csv) in [("line", "month,Sales,Costs\nJan,10,8\nFeb,14,9\nMar,12,11\nApr,20,12\n"), ("bar_race", "year,A,B,C\n2020,10,20,5\n2021,30,25,10\n2022,35,60,40\n"), ("donut", lv), ("treemap", lv), ("waffle", lv)] {
            let mut s = ss(); let r = run(&mut s, json!({"type": t, "csv": csv, "duration": 6.0, "title": "Test", "grouping": "indian"})).unwrap_or_else(|e| panic!("{t}: {e}"));
            assert_eq!(s.project.tracks.len(), 1, "{t}"); let c = s.project.find_clip(r["clip_id"].as_str().unwrap()).unwrap(); assert!(matches!(c.source, Source::Precomp { .. }), "{t}"); assert!(r["parts"].as_u64().unwrap() > 3, "{t}");
            crate::render::render_frame(&s.project, 3.0, &crate::render::Cache::new()).unwrap_or_else(|e| panic!("{t} render: {e}"));
        }
    }
    #[test]
    fn line_draws_on_and_bar_race_ranks() {
        let mut s = ss(); let r = run(&mut s, json!({"type": "line", "csv": "x,y\n1,0\n2,10\n", "duration": 4.0, "delay": 0.0, "animate": 2.0})).unwrap();
        let id = r["data"]["series"][0]["clip_id"].as_str().unwrap(); let g = s.project.find_clip(r["clip_id"].as_str().unwrap()).unwrap(); let Source::Precomp { tracks } = &g.source else { panic!() };
        let line = tracks[0].clips.iter().find(|c| c.id == id).unwrap(); let tr = line.effects.iter().find(|e| e.kind == "shape_trim").unwrap(); assert_eq!(tr.params["end"].at(0.0), 0.0); assert_eq!(tr.params["end"].at(2.0), 1.0);
        let mut s = ss(); let r = run(&mut s, json!({"type": "bar_race", "csv": "year,A,B\n2000,10,5\n2001,10,50\n", "duration": 4.0, "delay": 0.0, "hold": 0.0, "top_n": 2})).unwrap();
        let g = s.project.find_clip(r["clip_id"].as_str().unwrap()).unwrap(); let Source::Precomp { tracks } = &g.source else { panic!() };
        let a = tracks[0].clips.iter().find(|c| c.name.as_deref() == Some("A")).unwrap(); let b = tracks[0].clips.iter().find(|c| c.name.as_deref() == Some("B")).unwrap();
        assert!(a.props["y"].at(0.0) < b.props["y"].at(0.0), "A leads at the start"); assert!(a.props["y"].at(4.0) > b.props["y"].at(4.0), "B overtakes"); // xform-lint: ok (test reads stored keyframes, not a render)
        assert!(tracks[0].clips.iter().any(|c| matches!(&c.source, Source::Text { text, .. } if text.starts_with("{num:"))), "values are live numbers");
    }
    #[test]
    fn errors() {
        let mut s = ss(); assert!(run(&mut s, json!({"type": "pie", "csv": "a,b\nx,1"})).unwrap_err().contains("type must be"));
        assert!(run(&mut s, json!({"type": "donut", "csv": "a,b\nx,abc"})).unwrap_err().contains("number"));
        assert!(run(&mut s, json!({"type": "line", "csv": "a,b\nx,1"})).unwrap_err().contains("2 data rows"));
        assert!(run(&mut s, json!({"type": "donut", "csv": "a,b\nx,1", "grouping": "roman"})).is_err());
        assert!(s.project.tracks.is_empty());
    }
}
