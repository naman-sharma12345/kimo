//! v6 eyes: edit_check. Did an edit stay where it was meant to? Two frames (diff_frames side objects, full resolution, no
//! downscale) and optional allowed rects: the changed pixels (any RGB channel differs by more than threshold), grouped into
//! 8-connected regions with bounding boxes (largest first, max_regions listed), counted inside vs OUTSIDE the allowed rects,
//! plus the largest delta outside them over every pixel (changed or not). Raw numbers, no verdict.
use super::kit::*;
use crate::perception::analysis::Image;

pub const MAX_ALLOWED: usize = 64;

fn rect4(v: &Value, i: usize) -> Result<[usize; 4], String> {
    let bad = || format!("out_of_range: allowed[{i}] must be [x,y,w,h] whole pixels (w,h >= 1)");
    let r = v.as_array().filter(|a| a.len() == 4).ok_or_else(bad)?;
    let mut q = [0usize; 4];
    for (k, x) in r.iter().enumerate() { q[k] = x.as_f64().filter(|x| x.is_finite() && x.fract() == 0.0 && *x >= 0.0 && *x <= 65536.0).ok_or_else(bad)? as usize; }
    if q[2] == 0 || q[3] == 0 { return Err(bad()); }
    Ok(q)
}
fn find(p: &mut [u32], mut x: u32) -> u32 { while p[x as usize] != x { p[x as usize] = p[p[x as usize] as usize]; x = p[x as usize]; } x }
fn r6(x: f64) -> f64 { (x * 1e6).round() / 1e6 }

/// The whole measurement on two equal-size images; `allowed` rects must lie inside the frame (checked by the caller).
pub fn check(a: &Image, b: &Image, allowed: &[[usize; 4]], threshold: f64, max_regions: usize, min_region_px: usize) -> Value {
    let (w, h) = (a.w, a.h); let n = w * h;
    let mut inside = vec![false; n];
    for r in allowed { for y in r[1]..r[1] + r[3] { for x in r[0]..r[0] + r[2] { inside[y * w + x] = true; } } }
    let allowed_area = inside.iter().filter(|v| **v).count();
    let mut d = vec![0f64; n];
    let mut changed = vec![false; n];
    let (mut n_ch, mut n_in, mut max_all, mut max_out, mut max_out_at) = (0usize, 0usize, 0f64, 0f64, None::<(usize, usize)>);
    let (mut ox0, mut oy0, mut ox1, mut oy1) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for i in 0..n {
        if i % 16384 == 0 { crate::guard::check(); }
        let di = (0..3).map(|c| (a.rgb[c][i] - b.rgb[c][i]).abs()).fold(0f64, f64::max);
        d[i] = di; if di > max_all { max_all = di; }
        if !inside[i] && di > max_out { max_out = di; max_out_at = Some((i % w, i / w)); }
        if di > threshold { changed[i] = true; n_ch += 1; if inside[i] { n_in += 1; } else { let (x, y) = (i % w, i / w); ox0 = ox0.min(x); oy0 = oy0.min(y); ox1 = ox1.max(x); oy1 = oy1.max(y); } }
    }
    let identical = (0..3).all(|c| a.rgb[c] == b.rgb[c]);
    // 8-connected regions: union-find over the changed mask, one pass with the four already-visited neighbours.
    let mut parent: Vec<u32> = vec![u32::MAX; n];
    for y in 0..h { for x in 0..w { let i = y * w + x; if !changed[i] { continue; } parent[i] = i as u32;
        let mut nb = [usize::MAX; 4];
        if x > 0 { nb[0] = i - 1; } if y > 0 { nb[1] = i - w; if x > 0 { nb[2] = i - w - 1; } if x + 1 < w { nb[3] = i - w + 1; } }
        for j in nb { if j != usize::MAX && changed[j] { let (ra, rb) = (find(&mut parent, i as u32), find(&mut parent, j as u32)); if ra != rb { let (lo, hi) = (ra.min(rb), ra.max(rb)); parent[hi as usize] = lo; } } }
    } if y % 64 == 0 { crate::guard::check(); } }
    struct R { x0: usize, y0: usize, x1: usize, y1: usize, px: usize, out: usize, max: f64 }
    let mut idx: std::collections::HashMap<u32, usize> = std::collections::HashMap::new(); let mut rs: Vec<R> = vec![];
    for i in 0..n { if !changed[i] { continue; } let root = find(&mut parent, i as u32); let (x, y) = (i % w, i / w);
        let k = *idx.entry(root).or_insert_with(|| { rs.push(R { x0: x, y0: y, x1: x, y1: y, px: 0, out: 0, max: 0.0 }); rs.len() - 1 });
        let r = &mut rs[k]; r.x0 = r.x0.min(x); r.y0 = r.y0.min(y); r.x1 = r.x1.max(x); r.y1 = r.y1.max(y); r.px += 1; if !inside[i] { r.out += 1; } if d[i] > r.max { r.max = d[i]; } }
    let region_count = rs.len();
    let small = rs.iter().filter(|r| r.px < min_region_px).count();
    let mut big: Vec<&R> = rs.iter().filter(|r| r.px >= min_region_px).collect();
    big.sort_by(|p, q| q.px.cmp(&p.px).then(p.y0.cmp(&q.y0)).then(p.x0.cmp(&q.x0)));
    let eligible = big.len();
    let regions: Vec<Value> = big.iter().take(max_regions).map(|r| json!({"bbox": [r.x0, r.y0, r.x1 - r.x0 + 1, r.y1 - r.y0 + 1], "pixels": r.px, "outside_pixels": r.out, "max_delta": r6(r.max)})).collect();
    let regions_outside = rs.iter().filter(|r| r.out > 0).count();
    let per_allowed: Vec<Value> = allowed.iter().map(|r| { let mut c = 0usize; for y in r[1]..r[1] + r[3] { for x in r[0]..r[0] + r[2] { if changed[y * w + x] { c += 1; } } } json!({"rect": r, "changed_pixels": c}) }).collect();
    let n_out = n_ch - n_in; let outside_area = n - allowed_area;
    let pct = |p: usize, q: usize| if q == 0 { Value::Null } else { json!(r6(100.0 * p as f64 / q as f64)) };
    json!({
        "width": w, "height": h, "threshold": threshold, "connectivity": 8, "bit_identical": identical,
        "changed_pixels": n_ch, "changed_percent": r6(100.0 * n_ch as f64 / n as f64), "max_delta": r6(max_all),
        "allowed_count": allowed.len(), "allowed_area_px": allowed_area, "allowed": per_allowed,
        "inside": {"changed_pixels": n_in, "percent_of_frame": r6(100.0 * n_in as f64 / n as f64), "percent_of_allowed": pct(n_in, allowed_area)},
        "outside": {"changed_pixels": n_out, "percent_of_frame": r6(100.0 * n_out as f64 / n as f64), "percent_of_outside": pct(n_out, outside_area),
            "max_delta": r6(max_out), "max_delta_at": max_out_at.map(|(x, y)| json!([x, y])).unwrap_or(Value::Null),
            "bbox": if n_out == 0 { Value::Null } else { json!([ox0, oy0, ox1 - ox0 + 1, oy1 - oy0 + 1]) }, "regions": regions_outside},
        "region_count": region_count, "regions_below_min": small, "min_region_px": min_region_px, "max_regions": max_regions,
        "regions_listed": regions.len(), "regions_truncated": eligible > max_regions, "regions": regions,
    })
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let threshold = match a.get("threshold") { None | Some(Value::Null) => 0.04, Some(v) => v.as_f64().filter(|t| t.is_finite() && *t >= 0.0 && *t < 1.0).ok_or("out_of_range: threshold must be a number in [0, 1): a pixel counts as changed when any RGB channel (0..1) differs by more than it")? };
    let max_regions = match a.get("max_regions") { None | Some(Value::Null) => 32, Some(v) => v.as_u64().filter(|m| (1..=256).contains(m)).ok_or("out_of_range: max_regions must be a whole number 1..256")? as usize };
    let min_region_px = match a.get("min_region_px") { None | Some(Value::Null) => 1, Some(v) => v.as_u64().filter(|m| (1..=1_000_000).contains(m)).ok_or("out_of_range: min_region_px must be a whole number 1..1000000")? as usize };
    let allowed: Vec<[usize; 4]> = match a.get("allowed") { None | Some(Value::Null) => vec![], Some(Value::Array(v)) => { if v.len() > MAX_ALLOWED { return Err(format!("out_of_range: allowed holds at most {MAX_ALLOWED} rects")); } v.iter().enumerate().map(|(i, r)| rect4(r, i)).collect::<Result<_, _>>()? } Some(_) => return Err("out_of_range: allowed must be an array of [x,y,w,h] rects".into()) };
    let mut sides = json!({}); sides["a"] = a.get("a").cloned().ok_or("missing object 'a'")?; sides["b"] = a.get("b").cloned().ok_or("missing object 'b'")?;
    let p = super::diff_frames::pair(ss, &sides, 1.0)?;
    for (i, r) in allowed.iter().enumerate() { if r[0] + r[2] > p.source_w || r[1] + r[3] > p.source_h { return Err(format!("out_of_range: allowed[{i}] {:?} reaches outside the {}x{} frame", r, p.source_w, p.source_h)); } }
    let mut out = check(&p.a, &p.b, &allowed, threshold, max_regions, min_region_px);
    out["a"] = sides["a"].clone(); out["b"] = sides["b"].clone();
    Ok(out)
}

pub fn commands() -> Vec<Command> { vec![Command { name: "edit_check", mutates: false,
    description: "Check an edit stayed inside its allowed rects: the pixels changed between two frames, inside vs outside them, as connected regions. a/b are diff_frames side objects ({time or frame, source: composite|clip:<id>|file:<path>|snapshot:<rev>}), compared at full resolution with no downscale; sizes must match. A pixel is changed when any RGB channel (0..1) differs by more than threshold (default 0.04). allowed = up to 64 [x,y,w,h] rects in frame pixels (absent = nothing allowed, every change is outside). Returns changed_pixels / changed_percent, inside and outside counts (percent of frame, of the allowed area, of the rest), outside.max_delta with its pixel (over every outside pixel, changed or not), outside.bbox, and 8-connected changed regions, largest first: bbox, pixels, outside_pixels, max_delta; max_regions (default 32, up to 256) are listed, region_count counts all, min_region_px leaves smaller ones out of the list. Raw numbers only, no verdict; bad arguments are errors, never clamped.",
    schema: obj(vec![
        ("a", json!({"type": "object", "description": "before: {time or frame, source} as diff_frames"})),
        ("b", json!({"type": "object", "description": "after: {time or frame, source} as diff_frames"})),
        ("allowed", json!({"type": "array", "maxItems": MAX_ALLOWED, "items": {"type": "array", "items": {"type": "integer"}, "minItems": 4, "maxItems": 4}, "description": "rects [x,y,w,h] in frame pixels where change is expected"})),
        ("threshold", json!({"type": "number", "minimum": 0, "exclusiveMaximum": 1, "description": "changed when any RGB channel (0..1) differs by more than this, default 0.04"})),
        ("max_regions", json!({"type": "integer", "minimum": 1, "maximum": 256, "description": "regions listed, largest first, default 32"})),
        ("min_region_px", json!({"type": "integer", "minimum": 1, "description": "leave regions smaller than this out of the list (still counted), default 1"})),
    ], &["a", "b"]), run }] }
