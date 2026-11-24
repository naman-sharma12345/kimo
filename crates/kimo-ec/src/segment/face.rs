// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Trained face models for face tracking, behind one swappable interface.
//!
//! - [`FaceModel`]: what the face tracker (`effectcraft-track`) asks of a model: find a face in a
//!   region of a frame, then follow it from frame to frame. A model reports its own points (a
//!   mesh, for MediaPipe) plus a [`Topology`] saying which of them are the tracker's named
//!   landmarks and which trace the face outline, so the tracker never depends on one model's
//!   layout. The classical tracker stays built in as the fallback when no model is chosen.
//! - [`Roi`] and [`crop`]: the rotated square crops face models look at.

use crate::segment::{ModelInfo, Result};

/// Which of a model's points are the tracker's landmarks and its face outline.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Topology {
    /// The points tracing the face outline, in order around the face.
    pub outline: &'static [usize],
    /// (landmark id, as the tracker names them, e.g. `leftEyeInner`; point index). "Left" is
    /// image-left. Chin and jaw come from the outline.
    pub landmarks: &'static [(&'static str, usize)],
}

/// A face in a frame: the model's points (frame pixels) and its confidence (0–1).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Face {
    pub points: Vec<[f32; 2]>,
    pub score: f32,
}

/// A trained face model (swappable: the face tracker only uses this interface). Frames are
/// straight RGB 0–1, row-major `w×h`, coordinates frame pixels.
pub trait FaceModel: Send + Sync {
    fn info(&self) -> &'static ModelInfo;
    fn topology(&self) -> &'static Topology;
    /// The most confident face whose centre lies in `region` (`[x0, y0, x1, y1]`).
    fn find(&self, rgb: &[[f32; 3]], w: usize, h: usize, region: [f32; 4]) -> Result<Option<Face>>;
    /// The face `prev` was in the previous frame, in this one; `None` when it is lost.
    fn follow(&self, rgb: &[[f32; 3]], w: usize, h: usize, prev: &Face) -> Result<Option<Face>>;
}

/// A rotated square region of a frame: centre, side and angle (radians, the direction of the
/// square's x axis in the frame, y down).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Roi {
    pub center: [f32; 2],
    pub size: f32,
    pub angle: f32,
}

impl Roi {
    /// Frame position of the normalised square position `(u, v)` (0–1).
    pub fn to_frame(&self, u: f32, v: f32) -> [f32; 2] {
        let (s, c) = self.angle.sin_cos();
        let (x, y) = ((u - 0.5) * self.size, (v - 0.5) * self.size);
        [self.center[0] + c * x - s * y, self.center[1] + s * x + c * y]
    }
}

/// What a [`crop`] reads outside the frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Border {
    /// Black (MediaPipe's face detector input).
    Zero,
    /// The nearest edge pixel (MediaPipe's face mesh input).
    Replicate,
}

/// Sample `roi` into an `n×n` RGB tensor (bilinear), each channel mapped from 0–1 to `lo`–`hi`.
///
/// The sampling follows MediaPipe's CPU `ImageToTensor`, so the landmarks match MediaPipe's to a
/// fraction of a pixel: the square's corners map to the output corners `(0,0)` to `(n,n)` (output
/// pixel `i` is at `u = i/n`, not at its centre), the source position is read as a pixel-centre
/// index with no half-pixel shift, and the result is rounded to a whole 0–255 value before the range
/// map. Outside the frame it reads `border`. A frame whose buffer is shorter than `w·h`, or whose
/// size overflows, yields `lo` everywhere.
pub fn crop(rgb: &[[f32; 3]], w: usize, h: usize, roi: &Roi, n: usize, [lo, hi]: [f32; 2], border: Border) -> Vec<f32> {
    let mut out = vec![lo; n.saturating_mul(n).saturating_mul(3)];
    if w == 0 || h == 0 || n == 0 || w.checked_mul(h).is_none_or(|len| rgb.len() < len) {
        return out;
    }
    let (wi, hi_) = (w as i64, h as i64);
    // In range after the border rule, so the index is below `w·h` (which fits in `usize`).
    let px = |x: i64, y: i64| -> [f32; 3] {
        let (x, y) = match border {
            Border::Zero if x < 0 || y < 0 || x >= wi || y >= hi_ => return [0.0; 3],
            Border::Zero => (x, y),
            Border::Replicate => (x.clamp(0, wi - 1), y.clamp(0, hi_ - 1)),
        };
        rgb.get(y as usize * w + x as usize).copied().unwrap_or([0.0; 3])
    };
    for j in 0..n {
        for i in 0..n {
            let [x, y] = roi.to_frame(i as f32 / n as f32, j as f32 / n as f32);
            let (x0, y0) = (x.floor(), y.floor());
            let (fx, fy) = (x - x0, y - y0);
            // Saturating casts: NaN becomes 0 and an absurd position reads the border.
            let (x0, y0) = (x0 as i64, y0 as i64);
            let (x1, y1) = (x0.saturating_add(1), y0.saturating_add(1));
            let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
            for k in 0..3 {
                let v = (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy;
                let v = if v.is_finite() { (v * 255.0).round().clamp(0.0, 255.0) / 255.0 } else { 0.0 };
                out[(j * n + i) * 3 + k] = lo + v * (hi - lo);
            }
        }
    }
    out
}

pub(crate) fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}


