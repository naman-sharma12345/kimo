//! RV-26 deterministic two-render frame comparison, read-only and advisory.
use super::kit::*;
use crate::perception::analysis::{self, Image};
pub const METRICS: &[&str] = &[
    "mse",
    "psnr",
    "ssim",
    "max_abs",
    "changed_ratio",
    "luma_delta",
    "hist_distance",
];
pub fn compare(a: &Image, b: &Image, threshold: f64, cols: usize, rows: usize) -> Value {
    let n = a.w * a.h;
    let (mut mse, mut max, mut changed) = (0., 0f64, 0usize);
    let mut grid = vec![0.; cols * rows];
    let mut counts = vec![0usize; cols * rows];
    for i in 0..n {
        if i % 16384 == 0 {
            crate::guard::check();
        }
        let mut abs = 0.;
        let mut mx = 0f64;
        for c in 0..3 {
            let d = (a.rgb[c][i] - b.rgb[c][i]).abs();
            mse += d * d;
            mx = mx.max(d);
            abs += d / 3.;
        }
        max = max.max(mx);
        if mx > threshold {
            changed += 1;
        }
        let x = i % a.w;
        let y = i / a.w;
        let g = (y * rows / a.h).min(rows - 1) * cols + (x * cols / a.w).min(cols - 1);
        grid[g] += abs;
        counts[g] += 1;
    }
    mse /= (n * 3) as f64;
    for (i, v) in grid.iter_mut().enumerate() {
        *v /= counts[i].max(1) as f64;
    }
    let mut ssim = 0.;
    let mut windows = 0;
    for y in (0..a.h).step_by(8) {
        for x in (0..a.w).step_by(8) {
            let mut va = vec![];
            let mut vb = vec![];
            for yy in y..(y + 8).min(a.h) {
                for xx in x..(x + 8).min(a.w) {
                    let i = yy * a.w + xx;
                    va.push(a.luma(i));
                    vb.push(b.luma(i));
                }
            }
            let nn = va.len() as f64;
            let ma = va.iter().sum::<f64>() / nn;
            let mb = vb.iter().sum::<f64>() / nn;
            let sa = va.iter().map(|v| (v - ma).powi(2)).sum::<f64>() / nn;
            let sb = vb.iter().map(|v| (v - mb).powi(2)).sum::<f64>() / nn;
            let cov = va
                .iter()
                .zip(&vb)
                .map(|(a, b)| (a - ma) * (b - mb))
                .sum::<f64>()
                / nn;
            ssim += ((2. * ma * mb + 0.0001) * (2. * cov + 0.0009))
                / ((ma * ma + mb * mb + 0.0001) * (sa + sb + 0.0009));
            windows += 1;
        }
    }
    let ratio = changed as f64 / n as f64;
    json!({"metrics":{"mse":mse,"psnr_db":if mse==0.{99.}else{-10.*mse.log10()},"ssim":ssim/windows as f64,"max_abs":max,"changed_ratio":ratio,"luma_delta":analysis::luma_delta(a,b),"hist_distance":analysis::hist_distance(a,b)},"grid":{"cols":cols,"rows":rows,"mean_abs_delta":grid},"verdict_hint":if mse==0.{"identical"}else if ratio<0.02{"minor"}else if ratio<0.2{"moderate"}else{"major"}})
}
/// N1.2: a side whose source is `snapshot:<rev>` (current | undo:<n> | checkpoint:<name> | file:<path.kimo>) or `file:<path ending .kimo/.json>` is rendered as
/// the COMPOSITE of that project revision at the requested time; the original source string is echoed. Other `file:` sources are still image files.
fn rev_side(
    ss: &Session,
    side: &Value,
) -> Result<(Value, Option<(crate::model::Project, String)>), String> {
    let Some(src) = side.get("source").and_then(|v| v.as_str()) else {
        return Ok((side.clone(), None));
    };
    let rev = if let Some(r) = src.strip_prefix("snapshot:") {
        r.to_string()
    } else if src.starts_with("file:") && (src.ends_with(".kimo") || src.ends_with(".json")) {
        src.to_string()
    } else {
        return Ok((side.clone(), None));
    };
    let proj = super::project_diff::resolve_rev(ss, &rev)?;
    let mut v = side.clone();
    v["source"] = json!("composite");
    Ok((v, Some((proj, src.to_string()))))
}
fn render_rev(
    ss: &mut Session,
    r: &analysis::Request,
    p: Option<&crate::model::Project>,
) -> Result<analysis::Image, String> {
    let Some(p) = p else {
        return analysis::render(ss, r);
    };
    let real = std::mem::replace(&mut ss.project, p.clone());
    let cache = std::mem::replace(&mut ss.cache, crate::render::Cache::new());
    let fc = std::mem::replace(
        &mut ss.fcache,
        crate::framecache::FrameCache::new(crate::framecache::default_cap()),
    );
    let out = analysis::render(ss, r);
    ss.project = real;
    ss.cache = cache;
    ss.fcache = fc;
    out
}
/// Pixel-level delta in the compared (cropped, scaled) space: bit-identical short-circuit, changed count/percent, and the bounding box [x,y,w,h] of changed pixels.
fn delta(a: &Image, b: &Image, threshold: f64) -> Value {
    if (0..3).all(|c| a.rgb[c] == b.rgb[c]) {
        return json!({"bit_identical":true,"changed_pixels":0,"changed_percent":0.0,"bbox":null});
    }
    let (mut n, mut x0, mut y0, mut x1, mut y1) = (0usize, usize::MAX, usize::MAX, 0usize, 0usize);
    for i in 0..a.w * a.h {
        if i % 16384 == 0 {
            crate::guard::check();
        }
        if (0..3).any(|c| (a.rgb[c][i] - b.rgb[c][i]).abs() > threshold) {
            let (x, y) = (i % a.w, i / a.w);
            n += 1;
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
        }
    }
    let bbox = if n == 0 {
        Value::Null
    } else {
        json!([x0, y0, x1 - x0 + 1, y1 - y0 + 1])
    };
    json!({"bit_identical":false,"changed_pixels":n,"changed_percent":100.0 * n as f64 / (a.w * a.h) as f64,"bbox":bbox})
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let aa = a.get("a").ok_or("missing object 'a'")?;
    let bb = a.get("b").ok_or("missing object 'b'")?;
    let (aa, pa) = rev_side(ss, aa)?;
    let (bb, pb) = rev_side(ss, bb)?;
    let mut ar = analysis::parse(ss, &aa)?;
    let mut br = analysis::parse(ss, &bb)?;
    let (sa, sb) = (
        pa.as_ref()
            .map(|x| x.1.clone())
            .unwrap_or_else(|| ar.source.clone()),
        pb.as_ref()
            .map(|x| x.1.clone())
            .unwrap_or_else(|| br.source.clone()),
    );
    let metrics = match a.get("metrics") {
        None => vec!["mse", "psnr", "changed_ratio", "luma_delta"],
        Some(v) => v
            .as_array()
            .ok_or("invalid_metric: metrics must be an array")?
            .iter()
            .map(|x| x.as_str().ok_or("invalid_metric: metric must be a string"))
            .collect::<Result<Vec<_>, _>>()?,
    };
    for m in &metrics {
        if !METRICS.contains(m) {
            return Err("invalid_metric: Valid: mse, psnr, ssim, max_abs, changed_ratio, luma_delta, hist_distance".into());
        }
    }
    let threshold = a
        .get("threshold")
        .map(|v| v.as_f64().ok_or("out_of_range: threshold numeric 0..1"))
        .transpose()?
        .unwrap_or(0.04);
    if !threshold.is_finite() || !(0.0..=1.0).contains(&threshold) {
        return Err("out_of_range: threshold must be 0..1".into());
    }
    let scale = a
        .get("scale")
        .map(|v| v.as_f64().ok_or("out_of_range: scale numeric"))
        .transpose()?
        .unwrap_or(480.);
    if !scale.is_finite() || scale <= 0. || scale > 8192. {
        return Err("out_of_range: scale must be >0 and <=8192 (1 = full resolution)".into());
    }
    let ga = a.get("grid");
    if ga.map_or(false, |v| !v.is_object()) {
        return Err("out_of_range: grid must be object cols/rows".into());
    }
    let gridnum = |key: &str| -> Result<usize, String> {
        ga.and_then(|v| v.get(key))
            .map(|v| {
                v.as_u64()
                    .map(|v| v as usize)
                    .ok_or_else(|| format!("out_of_range: grid {key} integer required"))
            })
            .transpose()
            .map(|v| v.unwrap_or(4))
    };
    let cols = gridnum("cols")?;
    let rows = gridnum("rows")?;
    if cols == 0 || rows == 0 || cols > 64 || rows > 64 {
        return Err("out_of_range: grid cols/rows must be 1..64".into());
    }
    let mut ia = render_rev(ss, &ar, pa.as_ref().map(|x| &x.0))?;
    let mut ib = render_rev(ss, &br, pb.as_ref().map(|x| &x.0))?;
    let (aw, ah, bw, bh) = (ia.w, ia.h, ib.w, ib.h);
    if aw != bw || ah != bh {
        return Err(format!("size_mismatch: a={aw}x{ah}, b={bw}x{bh}; choose equal source canvas sizes before scale/region"));
    }
    let region = match a.get("region") {
        None => [0, 0, aw, ah],
        Some(v) => {
            let v = v
                .as_array()
                .filter(|v| v.len() == 4)
                .ok_or("out_of_range: region must be [x,y,w,h] pixels")?;
            let r = v
                .iter()
                .map(|v| {
                    v.as_u64()
                        .map(|v| v as usize)
                        .ok_or("out_of_range: region integer pixels required")
                })
                .collect::<Result<Vec<_>, _>>()?;
            [r[0], r[1], r[2], r[3]]
        }
    };
    if region[2] == 0
        || region[3] == 0
        || region[0].checked_add(region[2]).map_or(true, |v| v > aw)
        || region[1].checked_add(region[3]).map_or(true, |v| v > ah)
    {
        return Err("out_of_range: region outside image".into());
    }
    ia = ia.crop(region);
    ib = ib.crop(region);
    let ratio = if scale == 1. {
        1.
    } else {
        (scale / ia.w.max(ia.h) as f64).min(1.)
    };
    let (w, h) = (
        ((ia.w as f64 * ratio).round() as usize).max(1),
        ((ia.h as f64 * ratio).round() as usize).max(1),
    );
    if w != ia.w || h != ia.h {
        ia = ia.downscale(w, h);
        ib = ib.downscale(w, h);
    }
    let mut out = compare(&ia, &ib, threshold, cols, rows);
    out["delta"] = delta(&ia, &ib, threshold);
    let merge = a
        .get("merge_px")
        .map(|v| {
            v.as_u64()
                .filter(|v| *v <= 8192)
                .ok_or("out_of_range: merge_px integer 0..8192")
        })
        .transpose()?
        .unwrap_or(4) as usize;
    let expect = a
        .get("expect_bbox")
        .map(|v| -> Result<[f64; 4], String> {
            let v = v
                .as_array()
                .filter(|v| v.len() == 4)
                .ok_or("out_of_range: expect_bbox must be [x,y,w,h]")?;
            let mut r = [0.; 4];
            for i in 0..4 {
                r[i] = v[i]
                    .as_f64()
                    .filter(|v| v.is_finite() && *v >= 0.)
                    .ok_or("out_of_range: expect_bbox finite nonnegative pixels")?
            }
            if r[2] == 0. || r[3] == 0. {
                return Err("out_of_range: expect_bbox width/height must be positive".into());
            }
            Ok(r)
        })
        .transpose()?;
    let mut components = crate::perception::regions::detect(&ia, &ib, threshold, merge);
    components.sort_by(|a, b| {
        b.points
            .len()
            .cmp(&a.points.len())
            .then(a.first.cmp(&b.first))
    });
    let truncated = components.len() > 32;
    let sx = region[2] as f64 / w as f64;
    let sy = region[3] as f64 / h as f64;
    let mut rows_out = vec![];
    let mut unexpected = vec![];
    for (id, c) in components.iter().take(32).enumerate() {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        let (mut sum, mut maximum, mut cx, mut cy) = (0., 0f64, 0., 0.);
        for &i in &c.points {
            let (x, y) = (i % w, i / w);
            x0 = x0.min(x);
            y0 = y0.min(y);
            x1 = x1.max(x);
            y1 = y1.max(y);
            let d = (0..3)
                .map(|cc| (ia.rgb[cc][i] - ib.rgb[cc][i]).abs())
                .fold(0f64, f64::max);
            sum += d;
            maximum = maximum.max(d);
            cx += x as f64 + 0.5;
            cy += y as f64 + 0.5;
        }
        let bx = region[0] as f64 + (x0 as f64 * sx).floor();
        let by = region[1] as f64 + (y0 as f64 * sy).floor();
        let bw = ((x1 + 1) as f64 * sx).ceil() - (x0 as f64 * sx).floor();
        let bh = ((y1 + 1) as f64 * sy).ceil() - (y0 as f64 * sy).floor();
        let bbox = [bx, by, bw, bh];
        let mut hits = std::collections::BTreeMap::<String, Value>::new();
        // Attribute both revisions: an old-position region must still name the layer that moved.
        for (p, t) in [
            (pa.as_ref().map(|x| &x.0).unwrap_or(&ss.project), ar.time),
            (pb.as_ref().map(|x| &x.0).unwrap_or(&ss.project), br.time),
        ] {
            for tr in &p.tracks {
                for cl in &tr.clips {
                    if t < cl.start || t >= cl.start + cl.duration {
                        continue;
                    }
                    let Some((r, opacity)) = super::lint_frame::bounds(p, cl, t) else {
                        continue;
                    };
                    if opacity <= 0.05 {
                        continue;
                    }
                    let ix = (bx + bw).min(r[0] + r[2]) - bx.max(r[0]);
                    let iy = (by + bh).min(r[1] + r[3]) - by.max(r[1]);
                    if ix <= 0. || iy <= 0. {
                        continue;
                    }
                    let overlap = (ix * iy / (bw * bh)).clamp(0., 1.);
                    let h = json!({"layer":{"id":cl.id,"name":cl.name,"role":cl.role},"overlap_ratio":overlap});
                    let e = hits.entry(cl.id.clone()).or_insert(h.clone());
                    if overlap > e["overlap_ratio"].as_f64().unwrap_or(0.) {
                        *e = h
                    }
                }
            }
        }
        let mut hits: Vec<Value> = hits.into_values().collect();
        hits.sort_by(|a, b| {
            b["overlap_ratio"]
                .as_f64()
                .unwrap()
                .total_cmp(&a["overlap_ratio"].as_f64().unwrap())
                .then(a["layer"]["id"].as_str().cmp(&b["layer"]["id"].as_str()))
        });
        rows_out.push(json!({"id":c.first,"bbox":[bx as usize,by as usize,bw as usize,bh as usize],"area_px":c.points.len() as f64*sx*sy,"mean_delta":sum/c.points.len() as f64,"max_delta":maximum,"centroid":[region[0] as f64+cx/c.points.len() as f64*sx,region[1] as f64+cy/c.points.len() as f64*sy],"layer_hits":hits}));
        if let Some(e) = expect {
            if bx < e[0] || by < e[1] || bx + bw > e[0] + e[2] || by + bh > e[1] + e[3] {
                let why = format!(
                    "region {id} changed {} source pixels outside the expected box",
                    c.points.len() as f64 * sx * sy
                );
                unexpected.push(json!({"code":"unexpected_change","kind":"unexpected_change","severity":"warn","region_id":c.first,"bbox":bbox,"message":why,"fix_hint":crate::perception::fix_hint::none(&why),"next_steps":[{"tool":"describe_frame","args":{"time":br.time},"why":why}]}));
            }
        }
    }
    out["summary"] = if rows_out.is_empty() {
        json!("no changed regions.")
    } else {
        let r = &rows_out[0];
        let label = r["layer_hits"]
            .as_array()
            .and_then(|h| h.first())
            .and_then(|h| h["layer"]["name"].as_str())
            .unwrap_or("unnamed");
        json!(format!(
            "{} regions changed, largest {}x{} at ({},{}), layer '{}'.",
            components.len(),
            r["bbox"][2],
            r["bbox"][3],
            r["bbox"][0],
            r["bbox"][1],
            label
        ))
    };
    out["regions"] = json!(rows_out);
    out["truncated"] = json!(truncated);
    if expect.is_some() {
        out["findings"] = json!(unexpected);
    }

    let selected = out["metrics"].as_object_mut().unwrap();
    selected.retain(|k, _| metrics.contains(&if k == "psnr_db" { "psnr" } else { k.as_str() }));
    out["a"] = json!({"time":ar.time,"source":sa,"w":aw,"h":ah});
    out["b"] = json!({"time":br.time,"source":sb,"w":bw,"h":bh});
    out["compared"] = json!({"w":w,"h":h,"scale":ratio,"region":region,"threshold":threshold,"space":"display-referred RGB u8/255","method":"fixed-order f64, box downscale, Rec709 luma, SSIM luma 8x8 population variance C1=.01^2 C2=.03^2, 8x8x8 RGB histogram half-L1"});
    if a.get("heatmap").and_then(Value::as_bool).unwrap_or(false) {
        let mut fr = crate::effects::Frame::new(w, h);
        for i in 0..w * h {
            for c in 0..3 {
                fr.d[i * 4 + c] = ((ia.rgb[c][i] - ib.rgb[c][i]).abs() * 4.).min(1.) as f32;
            }
            fr.d[i * 4 + 3] = 1.;
        }
        let mut hash = 0xcbf29ce484222325u64;
        for v in &fr.d {
            for b in v.to_bits().to_le_bytes() {
                hash = (hash ^ b as u64).wrapping_mul(0x100000001b3);
            }
        }
        let path = std::env::temp_dir().join(format!("kimo-diff-{w}x{h}-{hash:016x}.png"));
        crate::render::write_image(
            &fr,
            path.to_str()
                .ok_or("invalid_source: non-UTF8 output path")?,
        )?;
        ss.media.push(crate::commands::Media {
            path: path.to_string_lossy().into(),
            w: w as u32,
            h: h as u32,
            mime: "image/png",
        });
        out["heatmap"] = json!({"path":path,"image":{"mime_type":"image/png","gain":4.0}});
    }
    let changed = out["delta"]["changed_percent"].as_f64().unwrap_or(0.);
    let identical = out["delta"]["bit_identical"] == true;
    let why = if identical {
        "the frames are bit identical, nothing to change".to_string()
    } else {
        format!(
            "{changed:.2}% of pixels differ; a difference is not a defect, so no edit is implied"
        )
    };
    out["fix_hint"] = crate::perception::fix_hint::none(&why);
    let mut steps = vec![];
    if !identical && json!(sb) == json!("composite") {
        steps.push(json!({"tool":"lint_frame","args":{"time":br.time,"annotate":false},"why":format!("{changed:.2}% of pixels changed; lint the later frame for defects")}));
    }
    out["next_steps"] = json!(steps);
    Ok(out)
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"diff_frames",mutates:false,description:"Compare two rendered frames, advisory only. a/b={time or frame, source:composite|clip:<id>|file:<image path>|snapshot:<current|undo:<n>|checkpoint:<name>|file:<path.kimo>>|file:<path.kimo or .json>} (a project revision renders as the composite at that time, so two versions can be compared at the same time). At most two full render pipeline calls, no thumbnail comparisons. Different source sizes error before identical box-filter analysis downscale. RGB u8/255 MSE, PSNR 99 on identical, SSIM 8x8 luma C1=.01 C2=.03, max absolute delta, changed pixel ratio, Rec709 luma delta and 8x8x8 histogram half-L1. Region pixel crop, 4x4 mean-delta grid; optional abs-diff x4 PNG heatmap file and inline image. Changed regions use 8-connectivity, row-major seed-pixel IDs, largest-area ordering with row-major tie-break, cap32, source-pixel boxes and approximate bounds-based attribution from both revisions. mean_delta/max_delta are max-channel absolute RGB deltas. area_px scales changed analysis pixels; centroid uses pixel centres. No edit or undo entry.",schema:obj(vec![("a",json!({"type":"object"})),("b",json!({"type":"object"})),("region",json!({"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4})),("scale",n("max analysis side, default480;1 means full res")),("merge_px",n("Chebyshev dilation radius of each component, default4; merge when dilated masks overlap")),("expect_bbox",json!({"type":"array","items":{"type":"number"},"minItems":4,"maxItems":4})),("metrics",json!({"type":"array","items":{"type":"string"}})),("threshold",n("changed_ratio abs threshold 0..1, default.04")),("heatmap",json!({"type":"boolean"})),("grid",json!({"type":"object","properties":{"cols":{"type":"integer"},"rows":{"type":"integer"}}}))],&["a","b"]),run}]
}
