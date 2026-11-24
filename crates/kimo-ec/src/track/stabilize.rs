// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed; web_time -> std::time, craft_mime base64 -> crate::base64_encode.
//! Video stabilization (the Warp Stabilizer's analysis and stabilization steps), clean-room from
//! the published approach of 2D stabilizers:
//!
//! 1. **Analysis** ([`Analyzer`]): Shi–Tomasi features are tracked between consecutive frames
//!    with pyramidal Lucas–Kanade ([`crate::track::klt`]), and the dominant (background) motion between
//!    each pair of frames is fitted robustly with RANSAC ([`crate::track::fit`]) for three models at
//!    once: translation, similarity and homography. The result ([`WarpAnalysis`]) is small and
//!    serialisable; it is stored in the effect instance.
//! 2. **Stabilization** ([`plan`]): the camera path is smoothed and per-frame corrective
//!    transforms are derived. For *Smooth Motion* each frame is warped to the Gaussian-weighted
//!    average of its neighbours' viewpoints — the motion-smoothing scheme of Matsushita et al.,
//!    "Full-Frame Video Stabilization with Motion Inpainting" (PAMI 2006) — so the low-frequency
//!    camera move stays and the jitter goes; *No Motion* maps every frame onto one reference
//!    frame. Framing then crops to the region valid on every frame (a centred rectangle inside
//!    every warped frame outline) and optionally scales it up to fill the frame; when that would
//!    exceed Maximum Scale, the correction is relaxed towards identity on the offending frames —
//!    trading smoothness for less crop, the constraint idea of Grundmann et al., "Auto-Directed
//!    Video Stabilization with Robust L1 Optimal Camera Paths" (CVPR 2011), applied here
//!    per frame rather than through a linear program.
//!
//!
//! *Subspace Warp* additionally follows long feature trajectories through the clip
//! ([`crate::track::camtrack::TrackAnalyzer`]) and stabilises each frame with a content-preserving mesh
//! warp fitted to the subspace-smoothed trajectories ([`crate::track::subspace`]): scenes with depth
//! (parallax) that one homography per frame cannot hold still are stabilised locally. The
//! framing (crop, auto-scale, Maximum Scale relaxation) works the same on meshes.
//!
//! Coordinates are layer pixels; transforms map *source* frame pixels to *stabilized* pixels.

use serde::{Deserialize, Serialize};

use crate::track::Frame;
use crate::track::camtrack::{Track2D, TrackAnalyzer};
use crate::track::fit::{Model, least_squares, ransac};
use crate::track::klt::{CornerOpts, GrayPyramid, LkOpts, analysis_factor, good_features, track};
use crate::track::solve::Homography;
use crate::track::subspace::{Mesh, MeshOpts, smooth_trajectories, solve_mesh};

/// Inter-frame motion of one frame (from the previous frame to this one) for each model.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FrameMotion {
    /// Translation `[tx, ty]`.
    pub t: [f64; 2],
    /// Similarity `[a, b, tx, ty]` (`x' = a x − b y + tx`, `y' = b x + a y + ty`).
    pub s: [f64; 4],
    /// Homography, row-major without `h33 = 1`.
    pub h: [f64; 8],
    /// Correspondences that agreed with the homography / tracked in total.
    pub inliers: u32,
    pub features: u32,
    /// A subset of this frame's tracked features (layer pixels), for Show Track Points.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub points: Vec<[f32; 2]>,
}

impl Default for FrameMotion {
    fn default() -> Self {
        FrameMotion { t: [0.0; 2], s: [1.0, 0.0, 0.0, 0.0], h: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0], inliers: 0, features: 0, points: vec![] }
    }
}

impl FrameMotion {
    /// The motion under a stabilization method.
    pub fn motion(&self, method: Method) -> Homography {
        match method {
            Method::Position => Homography::translation(self.t),
            Method::Similarity => {
                let s = self.s;
                Homography([[s[0], -s[1], s[2]], [s[1], s[0], s[3]], [0.0, 0.0, 1.0]])
            }
            Method::Perspective | Method::SubspaceWarp => {
                let h = self.h;
                Homography([[h[0], h[1], h[2]], [h[3], h[4], h[5]], [h[6], h[7], 1.0]])
            }
        }
    }
}

/// The stored analysis of a clip.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WarpAnalysis {
    pub version: u32,
    /// Layer time (seconds) of frame 0, and the frame duration.
    pub start: f64,
    pub frame_duration: f64,
    /// Layer size (pixels).
    pub size: [f64; 2],
    /// Detailed Analysis was on.
    pub detailed: bool,
    /// Per frame; frame 0's motion is the identity.
    pub frames: Vec<FrameMotion>,
    /// Long feature trajectories (layer pixels, 0.1 px) for Subspace Warp; empty in analyses
    /// made before it (which then stabilise like Perspective).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub tracks: Vec<Track2D>,
}

impl WarpAnalysis {
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
    /// Frame index at a layer time (held at the ends).
    pub fn frame_at(&self, layer_time: f64) -> Option<usize> {
        if self.frames.is_empty() || self.frame_duration <= 0.0 {
            return None;
        }
        let i = ((layer_time - self.start) / self.frame_duration).round();
        Some(i.clamp(0.0, (self.frames.len() - 1) as f64) as usize)
    }
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
    pub fn from_json(s: &str) -> Option<WarpAnalysis> {
        if s.trim().is_empty() {
            return None;
        }
        serde_json::from_str(s).ok()
    }
}

/// Analysis options.
#[derive(Clone, Copy, Debug, Default)]
pub struct AnalyzeOpts {
    /// Detailed Analysis: twice the resolution and more features.
    pub detailed: bool,
}

/// Incremental analysis: push the clip's frames in order.
pub struct Analyzer {
    opts: AnalyzeOpts,
    prev: Option<GrayPyramid>,
    frames: Vec<FrameMotion>,
    size: [f64; 2],
    trajectories: TrackAnalyzer,
}

fn round_pts(p: &[[f64; 2]], n: usize) -> Vec<[f32; 2]> {
    let step = (p.len() / n.max(1)).max(1);
    p.iter().step_by(step).take(n).map(|q| [((q[0] * 10.0).round() / 10.0) as f32, ((q[1] * 10.0).round() / 10.0) as f32]).collect()
}

impl Analyzer {
    pub fn new(size: [f64; 2], opts: AnalyzeOpts) -> Analyzer {
        let (side, want) = if opts.detailed { (640, 260) } else { (480, 160) };
        Analyzer { opts, prev: None, frames: vec![], size, trajectories: TrackAnalyzer::with_budget(size, side, want) }
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }
    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// Analyse the next frame.
    pub fn push(&mut self, frame: &Frame) {
        self.push_impl(frame, !self.opts.detailed);
    }

    fn push_impl(&mut self, frame: &Frame, share_pyramid: bool) {
        let share_pyramid = share_pyramid && !self.opts.detailed;
        let (max_side, max_feat) = if self.opts.detailed { (960, 700) } else { (480, 350) };
        let factor = analysis_factor(frame.img.width, frame.img.height, max_side);
        let mut pyr = GrayPyramid::from_image(frame.img, frame.offset, factor, if share_pyramid { 4 } else { 3 });
        if share_pyramid {
            self.trajectories.push_pyramid(self.prev.as_ref(), &pyr);
        } else {
            // Detailed analysis uses different downsampling factors for motion and trajectories.
            self.trajectories.push(frame);
        }
        // Motion tracking historically used only three extra levels. Temporarily remove
        // the trajectory tracker's fourth level without cloning any planes or gradients.
        let extra = if share_pyramid && pyr.levels.len() > 4 { pyr.levels.pop().zip(pyr.grads.pop()) } else { None };
        if share_pyramid && let Some(prev) = &mut self.prev {
            prev.levels.truncate(4);
            prev.grads.truncate(4);
        }
        let mut fm = FrameMotion::default();
        if let Some(prev) = &self.prev
            && prev.width() == pyr.width()
            && prev.height() == pyr.height()
            && prev.offset == pyr.offset
        {
            let md = (prev.width().max(prev.height()) as f64 / 40.0).clamp(4.0, 16.0);
            let feats = good_features(prev, &CornerOpts { max_features: max_feat, min_distance: md, quality: 0.01, window: 2, border: 4 }, None);
            let q = track(prev, &pyr, &feats, None, &LkOpts { radius: 5, fb_max: 0.75, ..Default::default() });
            let (src, dst): (Vec<[f64; 2]>, Vec<[f64; 2]>) = feats.iter().zip(&q).filter_map(|(p, q)| q.map(|q| (prev.to_layer(*p), pyr.to_layer(q)))).unzip();
            fm.features = src.len() as u32;
            let thr = 1.0 * factor as f64;
            let seed = self.frames.len() as u64 + 1;
            if let Some(f) = ransac(Model::Translation, &src, &dst, thr * 1.5, 200, seed) {
                let m = f.h.0;
                fm.t = [m[0][2], m[1][2]];
            }
            if let Some(f) = ransac(Model::Similarity, &src, &dst, thr * 1.5, 300, seed) {
                let m = f.h.0;
                fm.s = [m[0][0], m[1][0], m[0][2], m[1][2]];
            }
            if let Some(f) = ransac(Model::Homography, &src, &dst, thr, 500, seed) {
                let m = f.h.normalized().0;
                // Reject wild projective fits (few features): keep the similarity then.
                let sane = m[2][0].abs() < 0.01 && m[2][1].abs() < 0.01;
                fm.h = if sane {
                    [m[0][0], m[0][1], m[0][2], m[1][0], m[1][1], m[1][2], m[2][0], m[2][1]]
                } else {
                    [fm.s[0], -fm.s[1], fm.s[2], fm.s[1], fm.s[0], fm.s[3], 0.0, 0.0]
                };
                fm.inliers = f.inlier_count() as u32;
                let inl: Vec<[f64; 2]> = dst.iter().zip(&f.inliers).filter(|(_, b)| **b).map(|(p, _)| *p).collect();
                fm.points = round_pts(&inl, 48);
            }
        }
        if let Some((level, grad)) = extra {
            pyr.levels.push(level);
            pyr.grads.push(grad);
        }
        self.frames.push(fm);
        self.prev = Some(pyr);
    }

    pub fn finish(self, start: f64, frame_duration: f64) -> WarpAnalysis {
        let mut tracks = self.trajectories.finish(start, frame_duration).tracks;
        // Short trajectories hardly constrain the subspace: keep those of at least 6 frames, at
        // 0.1 px (smaller JSON).
        tracks.retain(|t| t.pts.len() >= 6);
        for (i, t) in tracks.iter_mut().enumerate() {
            t.id = i as u32;
            for p in t.pts.iter_mut() {
                *p = [(p[0] * 10.0).round() / 10.0, (p[1] * 10.0).round() / 10.0];
            }
        }
        WarpAnalysis { version: 2, start, frame_duration, size: self.size, detailed: self.opts.detailed, frames: self.frames, tracks }
    }
}

// ---------------------------------------------------------------- stabilization

/// Stabilization ▸ Result.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum StabResult {
    #[default]
    SmoothMotion,
    NoMotion,
}

/// Stabilization ▸ Method.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Method {
    Position,
    /// Position, Scale, Rotation.
    Similarity,
    Perspective,
    /// Content-preserving mesh warps fitted to subspace-smoothed trajectories.
    #[default]
    SubspaceWarp,
}

/// Advanced ▸ Rolling Shutter Ripple: how freely Subspace Warp's mesh may bend row by row (a
/// rolling-shutter camera exposes rows at different times, so shake wobbles the frame).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Ripple {
    /// A 12 × 8 mesh.
    #[default]
    Automatic,
    /// Twice the rows and a softer shape term: follows row-wise wobble more closely.
    Enhanced,
}

impl Ripple {
    pub fn mesh(self) -> MeshOpts {
        match self {
            Ripple::Automatic => MeshOpts { cols: 12, rows: 8, alpha: 1.0, prior: 0.02 },
            Ripple::Enhanced => MeshOpts { cols: 12, rows: 16, alpha: 0.5, prior: 0.02 },
        }
    }
}

/// Borders ▸ Framing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Framing {
    StabilizeOnly,
    StabilizeCrop,
    #[default]
    StabilizeCropAutoScale,
    SynthesizeEdges,
}

/// Effect parameters that shape the stabilization.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StabSettings {
    pub result: StabResult,
    /// Smoothness, percent (50 = default).
    pub smoothness: f64,
    pub method: Method,
    pub preserve_scale: bool,
    pub framing: Framing,
    /// Maximum Scale, percent.
    pub max_scale: f64,
    /// Action Safe Margin, percent of the frame on each side.
    pub action_safe: f64,
    /// Additional Scale, percent.
    pub additional_scale: f64,
    /// Crop Less ↔ Smooth More (−100…100).
    pub crop_less_smooth_more: f64,
    /// Frames per second (for Smoothness).
    pub fps: f64,
    pub ripple: Ripple,
}

impl Default for StabSettings {
    fn default() -> Self {
        StabSettings {
            result: StabResult::SmoothMotion,
            smoothness: 50.0,
            method: Method::SubspaceWarp,
            preserve_scale: false,
            framing: Framing::StabilizeCropAutoScale,
            max_scale: 150.0,
            action_safe: 0.0,
            additional_scale: 100.0,
            crop_less_smooth_more: 0.0,
            fps: 30.0,
            ripple: Ripple::Automatic,
        }
    }
}

impl StabSettings {
    /// Gaussian smoothing sigma in frames.
    pub fn sigma(&self) -> f64 {
        let base = self.smoothness.max(0.0) / 100.0 * self.fps.max(1.0) * 0.6;
        base * 2f64.powf(self.crop_less_smooth_more.clamp(-100.0, 100.0) / 100.0)
    }
}

/// The per-frame stabilization of a clip.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    /// Final source → output transform per frame (layer pixels), framing scale included. For
    /// mesh warps, the closest homography (for overlays and edge synthesis).
    pub warps: Vec<Homography>,
    /// Subspace Warp: per frame, the mesh (stabilised pixels before framing → source pixels);
    /// empty for the other methods.
    pub meshes: Vec<Mesh>,
    /// The framing transform (auto-scale and Additional Scale about the centre) after the
    /// stabilisation.
    pub framing: Homography,
    /// The visible region in output pixels `[x0, y0, x1, y1]` (Stabilize, Crop / Auto-scale);
    /// `None` = everything the warp produces.
    pub crop: Option<[f64; 4]>,
    /// Auto-scale factor (1 = none).
    pub auto_scale: f64,
    /// Fraction of the frame (per side length) valid on every frame before scaling.
    pub valid_fraction: f64,
}

impl Plan {
    /// The source position (frame `k`'s layer pixels) shown at output point `p`.
    pub fn source_of(&self, k: usize, p: [f64; 2]) -> Option<[f64; 2]> {
        if let Some(m) = self.meshes.get(k) {
            let inv = self.framing.inverse()?;
            return Some(m.sample(inv.apply(p)));
        }
        Some(self.warps.get(k)?.inverse()?.apply(p))
    }
    /// Where source point `q` of frame `k` lands in the output.
    pub fn output_of(&self, k: usize, q: [f64; 2]) -> Option<[f64; 2]> {
        if let Some(m) = self.meshes.get(k) {
            return m.forward(q).map(|p| self.framing.apply(p));
        }
        Some(self.warps.get(k)?.apply(q))
    }
}

fn convex_contains(q: &[[f64; 2]; 4], p: [f64; 2]) -> bool {
    let mut sign = 0.0;
    for i in 0..4 {
        let (a, b) = (q[i], q[(i + 1) % 4]);
        let c = (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
        if c.abs() < 1e-12 {
            continue;
        }
        if sign == 0.0 {
            sign = c.signum();
        } else if c.signum() != sign {
            return false;
        }
    }
    true
}

/// Largest `s ∈ [0, 1]` such that the centred `s·W × s·H` rectangle lies inside the frame
/// outline mapped by `b`.
pub fn valid_fraction(b: &Homography, size: [f64; 2]) -> f64 {
    let [w, h] = size;
    let quad = [b.apply([0.0, 0.0]), b.apply([w, 0.0]), b.apply([w, h]), b.apply([0.0, h])];
    // A fold or a point at infinity: nothing is valid.
    let m = b.normalized().0;
    for p in [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]] {
        if m[2][0] * p[0] + m[2][1] * p[1] + m[2][2] <= 1e-9 {
            return 0.0;
        }
    }
    let c = [w / 2.0, h / 2.0];
    let fits = |s: f64| {
        let (hw, hh) = (s * w / 2.0, s * h / 2.0);
        [[c[0] - hw, c[1] - hh], [c[0] + hw, c[1] - hh], [c[0] + hw, c[1] + hh], [c[0] - hw, c[1] + hh]].iter().all(|p| convex_contains(&quad, *p))
    };
    if fits(1.0) {
        return 1.0;
    }
    if !fits(0.0) {
        return 0.0;
    }
    let (mut lo, mut hi) = (0.0, 1.0);
    for _ in 0..30 {
        let mid = 0.5 * (lo + hi);
        if fits(mid) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    lo
}

fn remove_scale(b: &Homography, c: [f64; 2]) -> Homography {
    // Local scale at the frame centre from the Jacobian of the map.
    let e = 1.0;
    let p0 = b.apply(c);
    let px = b.apply([c[0] + e, c[1]]);
    let py = b.apply([c[0], c[1] + e]);
    let det = ((px[0] - p0[0]) * (py[1] - p0[1]) - (px[1] - p0[1]) * (py[0] - p0[0])).abs();
    let s = det.sqrt();
    if s < 1e-9 {
        return *b;
    }
    Homography::scale_about(1.0 / s, p0).then_after(b)
}

/// Widest window: 3 sigma of the largest Smoothness (1000%) with Smooth More at its maximum, at 240 fps.
const MAX_SMOOTH_RADIUS: usize = 8640;
/// Most window taps (frames x radius) one smoothing pass does; past it the window narrows so planning
/// stays bounded (an hour at 60 fps with the widest setting is about 4.7e8).
const MAX_SMOOTH_WORK: usize = 1 << 30;

/// Corrective transforms (source → stabilized, before framing) for every frame.
pub fn corrections(a: &WarpAnalysis, s: &StabSettings) -> Vec<Homography> {
    let n = a.frames.len();
    if n == 0 {
        return vec![];
    }
    let method = s.method;
    let inter: Vec<Homography> = a.frames.iter().map(|f| f.motion(method)).collect();
    let inv: Vec<Homography> = inter.iter().map(|h| h.inverse().unwrap_or(Homography::IDENTITY)).collect();
    let c = [a.size[0] / 2.0, a.size[1] / 2.0];
    let mut out = Vec::with_capacity(n);
    match s.result {
        StabResult::NoMotion => {
            // Cumulative path C_t (frame 0 → frame t); every frame maps onto the middle frame.
            let mut cum = Vec::with_capacity(n);
            let mut cur = Homography::IDENTITY;
            for (k, h) in inter.iter().enumerate() {
                if k > 0 {
                    cur = h.then_after(&cur);
                }
                cum.push(cur);
            }
            let r = n / 2;
            for c_t in &cum {
                let ci = c_t.inverse().unwrap_or(Homography::IDENTITY);
                out.push(cum[r].then_after(&ci));
            }
        }
        StabResult::SmoothMotion => {
            let sigma = s.sigma();
            // 3 sigma covers every supported setting; the work cap only bites on extreme frame counts.
            let rad = (((3.0 * sigma).ceil() as usize).min(MAX_SMOOTH_RADIUS)).min(n.saturating_sub(1)).min(MAX_SMOOTH_WORK / n);
            for t in 0..n {
                if sigma < 0.3 || rad == 0 {
                    out.push(Homography::IDENTITY);
                    continue;
                }
                let mut acc = [[0.0; 3]; 3];
                let mut wsum = 0.0;
                let mut add = |h: &Homography, d: f64| {
                    let w = (-(d * d) / (2.0 * sigma * sigma)).exp();
                    let m = h.normalized().0;
                    for i in 0..3 {
                        for j in 0..3 {
                            acc[i][j] += w * m[i][j];
                        }
                    }
                    wsum += w;
                };
                add(&Homography::IDENTITY, 0.0);
                // Forward: T_t^{i+1} = inter[i+1] ∘ T_t^i.
                let mut tf = Homography::IDENTITY;
                for i in t + 1..=(t + rad).min(n - 1) {
                    tf = inter[i].then_after(&tf);
                    add(&tf, (i - t) as f64);
                }
                // Backward: T_t^{i-1} = inter[i]⁻¹ ∘ T_t^i.
                let mut tb = Homography::IDENTITY;
                for i in (t.saturating_sub(rad)..t).rev() {
                    tb = inv[i + 1].then_after(&tb);
                    add(&tb, (t - i) as f64);
                }
                let m = acc.map(|r| r.map(|v| v / wsum));
                out.push(Homography(m).normalized());
            }
        }
    }
    if s.preserve_scale && method != Method::Position {
        out = out.iter().map(|b| remove_scale(b, c)).collect();
    }
    out
}

/// One frame's stabilisation before framing.
#[derive(Clone, Debug)]
enum Warp {
    H(Homography),
    M(Mesh),
}

impl Warp {
    fn valid(&self, size: [f64; 2]) -> f64 {
        match self {
            Warp::H(h) => valid_fraction(h, size),
            Warp::M(m) => m.valid_fraction(),
        }
    }
    /// Blend towards the identity (`l` = 1 keeps the warp).
    fn relax(&self, l: f64) -> Warp {
        match self {
            Warp::H(h) => Warp::H(Homography::IDENTITY.lerp(h, l)),
            Warp::M(m) => Warp::M(m.lerp_identity(l)),
        }
    }
    fn homography(&self) -> Homography {
        match self {
            Warp::H(h) => *h,
            Warp::M(m) => m.homography(),
        }
    }
}

/// Remove the uniform scale of the correspondences' best similarity (Preserve Scale): the
/// wanted positions are scaled back about their centroid.
fn unscale(pairs: &mut [([f64; 2], [f64; 2])]) {
    let (src, dst): (Vec<[f64; 2]>, Vec<[f64; 2]>) = pairs.iter().copied().unzip();
    let Some(h) = least_squares(Model::Similarity, &src, &dst) else { return };
    let m = h.0;
    let k = (m[0][0] * m[1][1] - m[0][1] * m[1][0]).abs().sqrt();
    if k < 1e-6 {
        return;
    }
    let n = dst.len() as f64;
    let c = [dst.iter().map(|q| q[0]).sum::<f64>() / n, dst.iter().map(|q| q[1]).sum::<f64>() / n];
    for (_, d) in pairs.iter_mut() {
        *d = [c[0] + (d[0] - c[0]) / k, c[1] + (d[1] - c[1]) / k];
    }
}

/// Subspace Warp's meshes (stabilised → source, before framing) for every frame; `None` for the
/// other methods and for analyses without trajectories. `base`: the perspective corrections
/// ([`corrections`]), the meshes' prior.
pub fn meshes(a: &WarpAnalysis, s: &StabSettings, base: &[Homography]) -> Option<Vec<Mesh>> {
    if s.method != Method::SubspaceWarp || a.tracks.is_empty() || base.len() != a.frames.len() {
        return None;
    }
    let n = a.frames.len();
    let o = s.ripple.mesh();
    let size = a.size;
    let pairs: Vec<Vec<([f64; 2], [f64; 2])>> = match s.result {
        StabResult::NoMotion => (0..n).map(|t| a.tracks.iter().filter_map(|tr| tr.at(t as u32)).map(|p| (p, base[t].apply(p))).collect()).collect(),
        StabResult::SmoothMotion => {
            let sigma = s.sigma();
            if sigma < 0.3 {
                return Some((0..n).map(|_| Mesh::identity(o.cols, o.rows, size)).collect());
            }
            smooth_trajectories(&a.tracks, n, sigma, 9)
        }
    };
    use rayon::prelude::*;
    Some(
        pairs
            .into_par_iter()
            .enumerate()
            .map(|(t, mut pr)| {
                if s.preserve_scale && pr.len() >= 2 {
                    unscale(&mut pr);
                }
                if pr.len() < 6 { Mesh::from_homography(o.cols, o.rows, size, &base[t]) } else { solve_mesh(size, &o, &pr, &base[t]) }
            })
            .collect(),
    )
}

/// Stabilization plan: corrections plus framing.
pub fn plan(a: &WarpAnalysis, s: &StabSettings) -> Plan {
    use rayon::prelude::*;
    let base = corrections(a, s);
    let mut b: Vec<Warp> = match meshes(a, s, &base) {
        Some(m) => m.into_iter().map(Warp::M).collect(),
        None => base.into_iter().map(Warp::H).collect(),
    };
    let size = a.size;
    let c = [size[0] / 2.0, size[1] / 2.0];
    let add = (s.additional_scale / 100.0).max(0.01);
    let z_add = Homography::scale_about(add, c);
    let fracs = |b: &[Warp]| b.par_iter().map(|w| w.valid(size)).collect::<Vec<f64>>();
    let mut fr = fracs(&b);
    let mut vmin = fr.iter().copied().fold(1.0, f64::min);
    let margin = (s.action_safe / 100.0).clamp(0.0, 0.45);
    let need_cover = 1.0 - 2.0 * margin;
    let max_scale = (s.max_scale / 100.0).max(1.0);
    if s.framing == Framing::StabilizeCropAutoScale && vmin * max_scale < need_cover && !b.is_empty() {
        // Relax the correction on frames that need more than Maximum Scale.
        let target = need_cover / max_scale;
        let n = b.len();
        let lam: Vec<f64> = (0..n)
            .into_par_iter()
            .map(|t| {
                if fr[t] >= target {
                    return 1.0;
                }
                let (mut lo, mut hi) = (0.0, 1.0);
                for _ in 0..24 {
                    let mid = 0.5 * (lo + hi);
                    if b[t].relax(mid).valid(size) >= target {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                lo
            })
            .collect();
        // Spread the relaxation smoothly (min filter, then a Gaussian of the same radius) so
        // the camera doesn't jump.
        let r = ((s.sigma() * 0.5).ceil() as usize).max(1);
        let minf: Vec<f64> = (0..n).map(|t| lam[t.saturating_sub(r)..=(t + r).min(n - 1)].iter().copied().fold(1.0, f64::min)).collect();
        let sig = r as f64 / 2.0;
        let smooth: Vec<f64> = (0..n)
            .map(|t| {
                let (mut acc, mut ws) = (0.0, 0.0);
                for i in t.saturating_sub(r)..=(t + r).min(n - 1) {
                    let d = i as f64 - t as f64;
                    let w = (-(d * d) / (2.0 * sig * sig)).exp();
                    acc += w * minf[i];
                    ws += w;
                }
                (acc / ws).min(lam[t])
            })
            .collect();
        b = b.iter().zip(&smooth).map(|(w, l)| if *l >= 1.0 { w.clone() } else { w.relax(*l) }).collect();
        fr = fracs(&b);
        vmin = fr.iter().copied().fold(1.0, f64::min);
    }
    let rect = |k: f64| [c[0] - k * size[0] / 2.0, c[1] - k * size[1] / 2.0, c[0] + k * size[0] / 2.0, c[1] + k * size[1] / 2.0];
    let (auto, crop) = match s.framing {
        Framing::StabilizeOnly | Framing::SynthesizeEdges => (1.0, None),
        Framing::StabilizeCrop => (1.0, Some(rect(vmin * add))),
        Framing::StabilizeCropAutoScale => {
            let k = if vmin > 1e-6 { (need_cover / vmin).clamp(1.0, max_scale) } else { max_scale };
            let vis = vmin * k * add;
            (k, if vis >= 1.0 - 1e-9 { None } else { Some(rect(vis)) })
        }
    };
    let z = Homography::scale_about(auto, c);
    let framing = z_add.then_after(&z);
    let warps = b.par_iter().map(|w| framing.then_after(&w.homography())).collect();
    let meshes = b
        .into_iter()
        .filter_map(|w| match w {
            Warp::M(m) => Some(m),
            Warp::H(_) => None,
        })
        .collect();
    Plan { warps, meshes, framing, crop, auto_scale: auto, valid_fraction: vmin }
}


