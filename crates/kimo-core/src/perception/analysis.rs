//! Shared display-referred visual analysis: fixed-order RGB histogram, luma and box filter.
use crate::{commands::Session, effects::Frame};
use serde_json::Value;
#[derive(Clone)]
pub struct Image {
    pub w: usize,
    pub h: usize,
    pub rgb: [Vec<f64>; 3],
}
impl Image {
    pub fn new(w: usize, h: usize) -> Self {
        crate::guard::reserve(w * h * 24);
        Self {
            w,
            h,
            rgb: [vec![0.; w * h], vec![0.; w * h], vec![0.; w * h]],
        }
    }
    pub fn from_frame(f: &Frame) -> Self {
        let u = crate::render::to_rgba8(f);
        let mut im = Self::new(f.w, f.h);
        for i in 0..f.w * f.h {
            for c in 0..3 {
                im.rgb[c][i] = u[i * 4 + c] as f64 / 255.;
            }
        }
        im
    }
    pub fn luma(&self, i: usize) -> f64 {
        0.2126 * self.rgb[0][i] + 0.7152 * self.rgb[1][i] + 0.0722 * self.rgb[2][i]
    }
    pub fn hist(&self) -> [f64; 512] {
        let mut h = [0.; 512];
        for i in 0..self.w * self.h {
            let r = (self.rgb[0][i] * 8.).floor() as usize;
            let g = (self.rgb[1][i] * 8.).floor() as usize;
            let b = (self.rgb[2][i] * 8.).floor() as usize;
            h[r.min(7) * 64 + g.min(7) * 8 + b.min(7)] += 1.;
        }
        for v in &mut h {
            *v /= (self.w * self.h) as f64;
        }
        h
    }
    pub fn crop(&self, r: [usize; 4]) -> Self {
        let mut out = Self::new(r[2], r[3]);
        for c in 0..3 {
            for y in 0..r[3] {
                out.rgb[c][y * r[2]..(y + 1) * r[2]].copy_from_slice(
                    &self.rgb[c][(y + r[1]) * self.w + r[0]..(y + r[1]) * self.w + r[0] + r[2]],
                );
            }
        }
        out
    }
    pub fn downscale(&self, w: usize, h: usize) -> Self {
        let mut out = Self::new(w, h);
        for y in 0..h {
            crate::guard::check();
            let (y0, y1) = (
                y * self.h / h,
                ((y + 1) * self.h / h).max(y * self.h / h + 1).min(self.h),
            );
            for x in 0..w {
                let (x0, x1) = (
                    x * self.w / w,
                    ((x + 1) * self.w / w).max(x * self.w / w + 1).min(self.w),
                );
                for c in 0..3 {
                    let mut s = 0.;
                    for yy in y0..y1 {
                        for xx in x0..x1 {
                            s += self.rgb[c][yy * self.w + xx];
                        }
                    }
                    out.rgb[c][y * w + x] = s / ((y1 - y0) * (x1 - x0)) as f64;
                }
            }
        }
        out
    }
}
pub fn hist_distance(a: &Image, b: &Image) -> f64 {
    a.hist()
        .iter()
        .zip(b.hist())
        .map(|(a, b)| (a - b).abs())
        .sum::<f64>()
        * 0.5
}
pub fn luma_delta(a: &Image, b: &Image) -> f64 {
    (0..a.w * a.h)
        .map(|i| (a.luma(i) - b.luma(i)).abs())
        .sum::<f64>()
        / (a.w * a.h) as f64
}
pub struct Request {
    pub time: f64,
    pub source: String,
}
pub fn parse(ss: &Session, a: &Value) -> Result<Request, String> {
    if !a.is_object() {
        return Err("invalid_source: frame request must be an object".into());
    }
    let time = match (a.get("time"), a.get("frame")) {
        (Some(_), Some(_)) => return Err("out_of_range: choose time or frame, not both".into()),
        (Some(v), None) => v.as_f64(),
        (None, Some(v)) => v.as_u64().map(|n| n as f64 / ss.project.fps),
        _ => Some(0.),
    }
    .filter(|t| t.is_finite() && *t >= 0.)
    .ok_or("out_of_range: time must be finite >= 0; frame a nonnegative integer")?;
    let source = a
        .get("source")
        .map(|v| {
            v.as_str()
                .ok_or("invalid_source: source must be a string".to_string())
        })
        .transpose()?
        .unwrap_or("composite")
        .to_string();
    if source != "composite" && !source.starts_with("clip:") && !source.starts_with("file:") {
        return Err("invalid_source: Valid: composite, clip:<id>, file:<path>".into());
    }
    Ok(Request { time, source })
}
pub fn render(ss: &mut Session, r: &Request) -> Result<Image, String> {
    let mut p = ss.project.clone();
    let t = r.time;
    if let Some(id) = r.source.strip_prefix("clip:") {
        let c = p
            .find_clip(id)
            .ok_or("invalid_source: clip not found")?
            .clone();
        p.tracks.retain(|tr| tr.clips.iter().any(|x| x.id == id));
        for tr in &mut p.tracks {
            tr.clips.retain(|x| x.id == id);
        }
        if r.time < c.start || r.time >= c.start + c.duration {
            return Err("out_of_range: clip time must be inside its timeline span".into());
        }
    }
    if let Some(path) = r.source.strip_prefix("file:") {
        let path = p.resolve(path);
        let (w, h, dur, fps) =
            crate::render::probe(&path).map_err(|e| format!("invalid_source: {e}"))?;
        if w == 0 || h == 0 {
            return Err("invalid_source: file has no video".into());
        }
        if dur > 0. && t >= dur {
            return Err("out_of_range: file time must be inside source duration".into());
        }
        let mut tmp = Session::new();
        crate::commands::dispatch(
            &mut tmp,
            "project_set",
            &serde_json::json!({"width":w,"height":h,"fps":fps.max(1.)}),
        )?;
        let a =
            crate::commands::dispatch(&mut tmp, "import_media", &serde_json::json!({"path":path}))?
                ["asset_id"]
                .clone();
        let tr = crate::commands::dispatch(&mut tmp, "add_track", &serde_json::json!({}))?
            ["track_id"]
            .clone();
        crate::commands::dispatch(
            &mut tmp,
            "add_clip",
            &serde_json::json!({"track_id":tr,"source_type":"media","asset_id":a,"start":0,"duration":dur.max(t+1./fps.max(1.)),"fit":"stretch"}),
        )?;
        p = tmp.project;
    }
    let cache = &mut ss.cache;
    crate::render::prepare_at(&p, cache, Some(t))?;
    let mut f = crate::render::render_frame(&p, t, cache)?;
    crate::color::pipeline::output(&p, &mut f)?;
    Ok(Image::from_frame(&f))
}
/// Fraction of fixed grid cells whose RGB histogram half-L1 strictly exceeds threshold.
pub fn spatial_change(a: &Image, b: &Image, cols: usize, rows: usize, threshold: f64) -> f64 {
    let mut changed = 0;
    for y in 0..rows {
        for x in 0..cols {
            let x0 = x * a.w / cols;
            let x1 = (x + 1) * a.w / cols;
            let y0 = y * a.h / rows;
            let y1 = (y + 1) * a.h / rows;
            let r = [x0, y0, x1 - x0, y1 - y0];
            if hist_distance(&a.crop(r), &b.crop(r)) > threshold {
                changed += 1;
            }
        }
    }
    changed as f64 / (cols * rows) as f64
}
