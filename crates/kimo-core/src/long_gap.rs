//! Conservative appearance-only long-gap tracking. No flow integration or guessed gap positions.
use crate::{
    model::{Project, Source, TimeMode},
    render::Cache,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const GRID: usize = 12;
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct BoxPx {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}
impl BoxPx {
    fn valid(self, w: usize, h: usize) -> bool {
        [self.x, self.y, self.w, self.h]
            .iter()
            .all(|v| v.is_finite())
            && self.x >= 0.
            && self.y >= 0.
            && self.w >= 8.
            && self.h >= 8.
            && self.x + self.w <= w as f64
            && self.y + self.h <= h as f64
    }
    fn iou(self, b: Self) -> f64 {
        let a = (self.x + self.w).min(b.x + b.w) - self.x.max(b.x);
        let c = (self.y + self.h).min(b.y + b.h) - self.y.max(b.y);
        let i = a.max(0.) * c.max(0.);
        i / (self.w * self.h + b.w * b.h - i).max(1e-9)
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Sample {
    pub frame: usize,
    pub time: f64,
    pub state: &'static str,
    pub bbox: Option<BoxPx>,
    pub score: f64,
    pub margin: f64,
}
#[derive(Clone, Copy)]
struct Candidate {
    b: BoxPx,
    score: f64,
}
/// Immutable seed plus optional bounded trusted appearance bank. Memory independent of gap length.
pub struct Tracker {
    w: usize,
    h: usize,
    anchor: Vec<f64>,
    seed: BoxPx,
    memory: Vec<Vec<f64>>,
    memory_enabled: bool,
    last_box: Option<BoxPx>,
    stable: usize,
    threshold: f64,
    margin: f64,
    visible: bool,
    pending: Option<BoxPx>,
    hits: usize,
    confirm: usize,
}
fn descriptor(g: &[f32], w: usize, h: usize, b: BoxPx) -> Option<Vec<f64>> {
    if !b.valid(w, h) {
        return None;
    }
    let mut v = Vec::with_capacity(GRID * GRID);
    for j in 0..GRID {
        for i in 0..GRID {
            let x = (b.x + (i as f64 + 0.5) * b.w / GRID as f64).floor() as usize;
            let y = (b.y + (j as f64 + 0.5) * b.h / GRID as f64).floor() as usize;
            let mut sum = 0.;
            for dy in -1isize..=1 {
                for dx in -1isize..=1 {
                    let xx = (x as isize + dx).clamp(0, w as isize - 1) as usize;
                    let yy = (y as isize + dy).clamp(0, h as isize - 1) as usize;
                    sum += g[yy * w + xx] as f64;
                }
            }
            v.push(sum / 9.);
        }
    }
    let mean = v.iter().sum::<f64>() / v.len() as f64;
    let norm = v.iter().map(|x| (x - mean).powi(2)).sum::<f64>().sqrt();
    // Reject low texture rather than correlating arbitrary flat surfaces.
    if norm / (v.len() as f64).sqrt() < 5. {
        return None;
    }
    for x in &mut v {
        *x = (*x - mean) / norm;
    }
    Some(v)
}
impl Tracker {
    pub fn new(
        g: &[f32],
        w: usize,
        h: usize,
        b: BoxPx,
        threshold: f64,
        margin: f64,
        confirm: usize,
    ) -> Result<Self, String> {
        if w < 16
            || h < 16
            || w.checked_mul(h) != Some(g.len())
            || w > 640
            || h > 640
            || !g.iter().all(|v| v.is_finite())
        {
            return Err("invalid/big analysis frame (max 640 per dimension)".into());
        }
        if !threshold.is_finite()
            || !(0.5..=0.99).contains(&threshold)
            || !margin.is_finite()
            || !(0.01..=0.5).contains(&margin)
            || !(2..=10).contains(&confirm)
        {
            return Err("threshold .5.. .99, margin .01.. .5, confirm 2..10 required".into());
        }
        let anchor = descriptor(g, w, h, b).ok_or("seed must be in-frame, >=8px and textured")?;
        Ok(Self {
            w,
            h,
            anchor,
            seed: b,
            memory: vec![],
            memory_enabled: false,
            last_box: Some(b),
            stable: 0,
            threshold,
            margin,
            visible: true,
            pending: None,
            hits: 0,
            confirm,
        })
    }
    /// Opt-in: store up to eight trusted appearance anchors; seed is never replaced.
    pub fn enable_memory(&mut self) {
        self.memory_enabled = true;
    }
    fn score(&self, g: &[f32], b: BoxPx) -> f64 {
        descriptor(g, self.w, self.h, b)
            .map(|v| {
                std::iter::once(&self.anchor)
                    .chain(self.memory.iter())
                    .map(|anchor| v.iter().zip(anchor).map(|(a, b)| a * b).sum::<f64>())
                    .fold(-1., f64::max)
            })
            .unwrap_or(-1.)
    }
    fn search(&self, g: &[f32]) -> (Candidate, f64) {
        let mut coarse = Vec::new();
        let scales: &[f64] = if self.memory_enabled {
            &[0.8, 1., 1.2]
        } else {
            &[0.65, 0.8, 1., 1.2, 1.5]
        };
        for &scale in scales {
            let (bw, bh) = (self.seed.w * scale, self.seed.h * scale);
            if bw < 8. || bh < 8. || bw > self.w as f64 || bh > self.h as f64 {
                continue;
            }
            let step = (bw.min(bh) / 8.).floor().max(2.) as usize;
            for y in (0..=self.h - bh.ceil() as usize).step_by(step) {
                crate::guard::check();
                for x in (0..=self.w - bw.ceil() as usize).step_by(step) {
                    let b = BoxPx {
                        x: x as f64,
                        y: y as f64,
                        w: bw,
                        h: bh,
                    };
                    coarse.push(Candidate {
                        b,
                        score: self.score(g, b),
                    });
                }
            }
        }
        coarse.sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
        // Refine spatially separate peaks, not eight neighbours of the same peak.
        let mut peaks: Vec<Candidate> = vec![];
        for c in &coarse {
            if peaks.iter().all(|q| q.b.iou(c.b) < 0.3) {
                peaks.push(*c);
                if peaks.len() == 12 {
                    break;
                }
            }
        }
        let mut refined = Vec::new();
        for c in peaks {
            let r = (c.b.w.min(c.b.h) / 8.).ceil().max(2.) as i32;
            let mut best = c;
            for dy in -r..=r {
                crate::guard::check();
                for dx in -r..=r {
                    let b = BoxPx {
                        x: c.b.x + dx as f64,
                        y: c.b.y + dy as f64,
                        ..c.b
                    };
                    let score = self.score(g, b);
                    if score > best.score {
                        best = Candidate { b, score }
                    }
                }
            }
            refined.push(best);
        }
        refined.sort_unstable_by(|a, b| b.score.total_cmp(&a.score));
        let best = refined.first().copied().unwrap_or(Candidate {
            b: self.seed,
            score: -1.,
        });
        let second = refined
            .iter()
            .find(|c| c.b.iou(best.b) < 0.3)
            .map(|c| c.score)
            .unwrap_or(-1.);
        (best, (best.score - second).max(0.))
    }
    pub fn step(&mut self, g: &[f32], frame: usize, time: f64) -> Result<Sample, String> {
        if g.len() != self.w * self.h || !g.iter().all(|v| v.is_finite()) || !time.is_finite() {
            return Err("invalid frame/time".into());
        }
        // A globally flat frame cannot contain the textured anchor. Avoid exhaustive search.
        let (lo, hi) = g
            .iter()
            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), &v| {
                (lo.min(v), hi.max(v))
            });
        let (c, margin) = if hi - lo < 10. {
            (
                Candidate {
                    b: self.seed,
                    score: -1.,
                },
                0.,
            )
        } else {
            self.search(g)
        };
        let good = c.score >= self.threshold && margin >= self.margin;
        let mut state = "lost";
        let mut bbox = None;
        if good {
            if self.visible {
                state = "visible";
                bbox = Some(c.b)
            } else {
                let consistent = self.pending.map(|b| b.iou(c.b) >= 0.3).unwrap_or(false);
                self.hits = if consistent { self.hits + 1 } else { 1 };
                self.pending = Some(c.b);
                if self.hits >= self.confirm {
                    self.visible = true;
                    state = "reacquired";
                    bbox = Some(c.b);
                    self.pending = None;
                    self.hits = 0
                } else {
                    state = "tentative"
                }
            }
        } else {
            self.visible = false;
            self.pending = None;
            self.hits = 0;
        }
        if self.memory_enabled {
            if bbox.is_some()
                && self.last_box.map(|b| b.iou(c.b) > 0.5).unwrap_or(false)
                && c.score >= 0.88
                && margin >= 0.12
            {
                self.stable += 1;
                if self.stable >= 3 {
                    if let Some(v) = descriptor(g, self.w, self.h, c.b) {
                        let novelty = std::iter::once(&self.anchor)
                            .chain(self.memory.iter())
                            .map(|a| a.iter().zip(&v).map(|(a, b)| a * b).sum::<f64>())
                            .fold(-1., f64::max);
                        if novelty < 0.98 {
                            if self.memory.len() == 8 {
                                self.memory.remove(0);
                            }
                            self.memory.push(v);
                        }
                    }
                }
            } else {
                self.stable = 0;
            }
            self.last_box = bbox;
        }
        Ok(Sample {
            frame,
            time,
            state,
            bbox,
            score: c.score,
            margin,
        })
    }
}
fn number(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) {
        None => Ok(d),
        Some(v) => v
            .as_f64()
            .filter(|n| n.is_finite())
            .ok_or(format!("{k} needs finite number")),
    }
}
fn integer(a: &Value, k: &str, d: usize) -> Result<usize, String> {
    let v = number(a, k, d as f64)?;
    if v < 0. || v.fract() != 0. || v > 100_000_000. {
        return Err(format!("{k} must be integer 0..100000000"));
    }
    Ok(v as usize)
}
/// Clip-local frame indices; box is normalized to the fitted, pre-effect canvas. Read-only.
pub fn run(p: &Project, a: &Value) -> Result<Value, String> {
    let allowed = [
        "clip_id",
        "bbox",
        "start_frame",
        "end_frame",
        "threshold",
        "margin",
        "confirm_frames",
        "appearance_memory",
    ];
    let obj = a.as_object().ok_or("expected object")?;
    if obj.keys().any(|k| !allowed.contains(&k.as_str())) {
        return Err("unknown tracking argument".into());
    }
    let id = a
        .get("clip_id")
        .and_then(Value::as_str)
        .ok_or("clip_id required")?;
    let c = p.find_clip(id).ok_or("no clip")?.clone();
    if !matches!(c.source, Source::Media { .. }) || !matches!(c.time_mode, TimeMode::Normal) {
        return Err("normal media clip required; reverse/freeze unsupported".into());
    }
    if !p.fps.is_finite()
        || p.fps <= 0.
        || !c.duration.is_finite()
        || c.duration <= 0.
        || p.width < 16
        || p.height < 16
    {
        return Err("invalid project dimensions/fps/duration".into());
    }
    let total = (c.duration * p.fps).ceil() as usize;
    let first = integer(a, "start_frame", 0)?;
    let last = integer(a, "end_frame", total.saturating_sub(1))?;
    if first > last || last >= total || last - first >= 10000 {
        return Err("ordered inclusive range within clip, <=10000 frames required".into());
    }
    let v = a
        .get("bbox")
        .and_then(Value::as_array)
        .filter(|v| v.len() == 4)
        .ok_or("bbox needs [x,y,width,height], normalized")?;
    let mut b = [0.; 4];
    for (i, x) in v.iter().enumerate() {
        b[i] = x
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or("finite bbox required")?;
    }
    let factor = (640. / p.width.max(p.height) as f64).min(1.);
    let mut q = p.clone();
    q.width = (p.width as f64 * factor).round() as u32;
    q.height = (p.height as f64 * factor).round() as u32;
    // Decode only this clip, with its fit/in-point/speed intact, and no unrelated sources.
    for tr in &mut q.tracks {
        tr.clips.retain(|x| x.id == id)
    }
    let (w, h) = (q.width as usize, q.height as usize);
    let seed = BoxPx {
        x: b[0] * w as f64,
        y: b[1] * h as f64,
        w: b[2] * w as f64,
        h: b[3] * h as f64,
    };
    if !seed.valid(w, h) {
        return Err("invalid bbox or <8 analysis pixels".into());
    }
    let mut cache = Cache::new();
    let g = crate::tracking::clip_gray_pub(&q, &mut cache, &c, first)?;
    let mut tracker = Tracker::new(
        &g,
        w,
        h,
        seed,
        number(a, "threshold", 0.8)?,
        number(a, "margin", 0.08)?,
        integer(a, "confirm_frames", 2)?,
    )?;
    match a.get("appearance_memory") {
        None | Some(Value::Bool(false)) => {}
        Some(Value::Bool(true)) => tracker.enable_memory(),
        _ => return Err("appearance_memory must be boolean".into()),
    }
    let mut samples = vec![Sample {
        frame: first,
        time: first as f64 / p.fps,
        state: "seed",
        bbox: Some(seed),
        score: 1.,
        margin: 0.,
    }];
    for i in first + 1..=last {
        crate::guard::check();
        let g = crate::tracking::clip_gray_pub(&q, &mut cache, &c, i)?;
        samples.push(tracker.step(&g, i, i as f64 / p.fps)?);
    }
    let (sx, sy) = (p.width as f64 / w as f64, p.height as f64 / h as f64);
    for s in &mut samples {
        if let Some(b) = &mut s.bbox {
            b.x *= sx;
            b.w *= sx;
            b.y *= sy;
            b.h *= sy
        }
    }
    Ok(
        json!({"samples":samples,"coordinate_space":"fitted_canvas_pixels_pre_effects","time_space":"clip_local_seconds","analysis_size":[w,h],"appearance_memory":tracker.memory_enabled,"learned_anchors":tracker.memory.len(),"mutated":false,"limits":"Grayscale appearance templates; immutable seed, optional bounded learned anchors. Translation and discrete seed-relative scales only (.65/.8/1/1.2/1.5 without memory; .8/1/1.2 with memory). Not semantic re-ID, rotation or deformation. Lost/tentative boxes are null. Identical-looking objects can remain ambiguous or swap."}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    fn box_at(x: f64) -> BoxPx {
        BoxPx {
            x,
            y: 20.,
            w: 24.,
            h: 24.,
        }
    }
    fn frame(xs: &[usize]) -> Vec<f32> {
        let mut f = vec![80.; 96 * 64];
        for &x in xs {
            for j in 0..24 {
                for i in 0..24 {
                    f[(20 + j) * 96 + x + i] = (120.
                        + 70. * (i as f32 * 0.43).sin()
                        + 40. * (j as f32 * 0.39 + i as f32 * 0.11).cos());
                }
            }
        }
        f
    }
    fn tracker() -> Tracker {
        Tracker::new(&frame(&[10]), 96, 64, box_at(10.), 0.8, 0.08, 2).unwrap()
    }
    #[test]
    fn survives_300_frames_and_requires_confirmation() {
        let mut t = tracker();
        for i in 1..=300 {
            let s = t.step(&frame(&[]), i, i as f64).unwrap();
            assert_eq!(s.state, "lost");
            assert!(s.bbox.is_none());
        }
        assert_eq!(t.step(&frame(&[60]), 301, 301.).unwrap().state, "tentative");
        let s = t.step(&frame(&[61]), 302, 302.).unwrap();
        assert_eq!(s.state, "reacquired");
        assert!((s.bbox.unwrap().x - 61.).abs() <= 1.);
    }
    #[test]
    fn duplicates_are_ambiguous() {
        let mut t = tracker();
        let s = t.step(&frame(&[10, 60]), 1, 1.).unwrap();
        assert_eq!(s.state, "lost");
        assert!(s.margin < 0.08);
        assert!(s.bbox.is_none());
    }
    #[test]
    fn isolated_match_and_interrupted_confirmation() {
        let mut t = tracker();
        t.step(&frame(&[]), 1, 1.).unwrap();
        assert_eq!(t.step(&frame(&[60]), 2, 2.).unwrap().state, "tentative");
        t.step(&frame(&[]), 3, 3.).unwrap();
        assert_eq!(t.step(&frame(&[60]), 4, 4.).unwrap().state, "tentative");
        assert_eq!(t.step(&frame(&[10]), 5, 5.).unwrap().state, "tentative");
    }
    #[test]
    fn exposure_offset_and_gain() {
        let mut t = tracker();
        let g: Vec<_> = frame(&[60]).iter().map(|v| v * 0.7 + 30.).collect();
        let s = t.step(&g, 1, 1.).unwrap();
        assert_eq!(s.state, "visible");
        assert!((s.bbox.unwrap().x - 60.).abs() <= 1.);
    }
    #[test]
    fn memory_duplicate_and_occlusion_cannot_update() {
        let mut t = tracker();
        t.enable_memory();
        // Trusted slight appearance change creates bank entries.
        let mut g = frame(&[10]);
        for y in 20..44 {
            for x in 10..34 {
                g[y * 96 + x] += (x as f32 * 0.8).sin() * 35.;
            }
        }
        for i in 1..=6 {
            let s = t.step(&g, i, i as f64).unwrap();
            assert_eq!(s.state, "visible");
        }
        assert!(!t.memory.is_empty());
        let before = t.memory.clone();
        let mut duplicate = g.clone();
        for y in 20..44 {
            for x in 0..24 {
                duplicate[y * 96 + 60 + x] = g[y * 96 + 10 + x];
            }
        }
        let s = t.step(&duplicate, 7, 7.).unwrap();
        assert_eq!(s.state, "lost");
        assert!(s.bbox.is_none());
        assert_eq!(t.memory, before);
        for i in 8..108 {
            let s = t.step(&frame(&[]), i, i as f64).unwrap();
            assert_eq!(s.state, "lost");
            assert!(s.bbox.is_none());
        }
        assert_eq!(t.memory, before);
        assert_eq!(t.step(&g, 108, 108.).unwrap().state, "tentative");
        assert_eq!(t.memory, before);
        assert_eq!(t.step(&g, 109, 109.).unwrap().state, "reacquired");
        assert!(t.memory.len() <= 8);
    }
    #[test]
    fn memory_stops_on_discontinuous_appearance() {
        let mut t = tracker();
        t.enable_memory();
        for i in 1..5 {
            t.step(&frame(&[10]), i, i as f64).unwrap();
        }
        let before = t.memory.clone();
        let mut g = frame(&[10]);
        for y in 20..44 {
            for x in 10..34 {
                g[y * 96 + x] = 255. - g[y * 96 + x];
            }
        }
        let s = t.step(&g, 5, 5.).unwrap();
        assert_eq!(s.state, "lost");
        assert!(s.bbox.is_none());
        assert_eq!(before, t.memory);
    }
    #[test]
    fn flat_and_invalid_inputs() {
        assert!(Tracker::new(&vec![80.; 96 * 64], 96, 64, box_at(10.), 0.8, 0.08, 2).is_err());
        let mut t = tracker();
        assert!(t.step(&[0.], 1, 1.).is_err());
        assert!(t.step(&vec![f32::NAN; 96 * 64], 1, 1.).is_err());
        assert!(Tracker::new(&frame(&[10]), 96, 64, box_at(-1.), 0.8, 0.08, 2).is_err());
        assert!(Tracker::new(&frame(&[10]), 96, 64, box_at(10.), f64::NAN, 0.08, 2).is_err());
        assert!(Tracker::new(&frame(&[10]), 96, 64, box_at(10.), 0.8, 0., 1).is_err());
    }
    #[test]
    fn command_read_only_and_early_validation() {
        use crate::commands::{dispatch, registry, Session};
        let mut s = Session::new();
        assert!(
            !registry()
                .into_iter()
                .find(|c| c.name == "track_long_gap")
                .unwrap()
                .mutates
        );
        let before = serde_json::to_value(&s.project).unwrap();
        for a in [
            json!({}),
            json!({"clip_id":"absent","bbox":[0.,0.,1.,1.]}),
            json!({"clip_id":"absent","unknown":1}),
        ] {
            assert!(dispatch(&mut s, "track_long_gap", &a).is_err());
        }
        assert_eq!(before, serde_json::to_value(&s.project).unwrap());
    }
}
