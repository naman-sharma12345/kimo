//! On-device vision ("engine eyes", layer 6): faces, people/objects and the main subject of a frame.
//!
//! Small permissively licensed ONNX models (see `models.toml`, `THIRD_PARTY.md`), run by tract-onnx (pure Rust),
//! downloaded on first use into `~/.cache/kimo_models/vision` and SHA-512 verified. The model runtime is the cargo
//! feature `vision` (on by default); this module and its API exist in every build.
//!
//! Stable API for critics and tools (boxes in the frame's own pixels, x/y top-left):
//! - [`faces`] -> `Vec<FaceBox>` (box, score, 5 landmarks)
//! - [`objects`] -> `Vec<Detection>` (box, COCO class, score); [`people`] is the "person" class only
//! - [`subject_mask`] -> [`Mask`] (0..1, where the main subject is)
//! - [`person_matte`] -> [`Mask`] (MODNet, downloaded only when first asked for)
//!
//! When a model cannot be had (offline, download refused, feature off), detections come back empty and the masks fall
//! back to the spectral residual saliency map (`ext::saliency_map`); the `detect_*` variants say so in `Seen::fallback`.
//! Results are cached per (source, frame): pass a [`FrameKey`] to the `detect_*` functions, or let the plain functions
//! key on the frame's pixels.
pub mod models;
pub mod post;
#[cfg(feature = "vision")]
pub mod infer;

use crate::effects::Frame;
use serde::Serialize;
use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct BBox { pub x: f32, pub y: f32, pub w: f32, pub h: f32 }
impl BBox {
    pub fn area(&self) -> f32 { self.w * self.h }
    pub fn center(&self) -> [f32; 2] { [self.x + self.w / 2.0, self.y + self.h / 2.0] }
    /// Share of this box inside `other` (0..1).
    pub fn frac_inside(&self, o: &BBox) -> f32 {
        let ix = ((self.x + self.w).min(o.x + o.w) - self.x.max(o.x)).max(0.0); let iy = ((self.y + self.h).min(o.y + o.h) - self.y.max(o.y)).max(0.0);
        if self.area() > 0.0 { ix * iy / self.area() } else { 0.0 }
    }
    pub fn scaled(&self, sx: f32, sy: f32) -> BBox { BBox { x: self.x * sx, y: self.y * sy, w: self.w * sx, h: self.h * sy } }
}

/// One face. Landmarks: right eye, left eye, nose tip, right mouth corner, left mouth corner (subject's left/right).
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct FaceBox { pub bbox: BBox, pub score: f32, pub landmarks: [[f32; 2]; 5] }

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Detection { pub bbox: BBox, pub class: &'static str, pub class_id: u16, pub score: f32 }

/// A soft mask on its own grid (`w` x `h`) stretched over a `frame_w` x `frame_h` frame. Values 0..1.
#[derive(Clone, Debug, PartialEq)]
pub struct Mask {
    pub w: usize, pub h: usize, pub data: Vec<f32>,
    pub frame_w: usize, pub frame_h: usize,
    /// The model that made it ("u2netp", "modnet"), or "saliency" for the fallback.
    pub model: &'static str,
    /// Set when this is the saliency fallback: why the model was not used.
    pub fallback: Option<String>,
}
impl Mask {
    /// Value at frame pixel (x, y), nearest grid cell.
    pub fn at(&self, x: f32, y: f32) -> f32 {
        if self.w == 0 || self.h == 0 { return 0.0; }
        let gx = ((x / self.frame_w as f32 * self.w as f32) as isize).clamp(0, self.w as isize - 1) as usize;
        let gy = ((y / self.frame_h as f32 * self.h as f32) as isize).clamp(0, self.h as isize - 1) as usize;
        self.data[gy * self.w + gx]
    }
    /// Bounding box (frame px) of cells >= `thr`, None when nothing reaches it.
    pub fn bbox(&self, thr: f32) -> Option<BBox> {
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0, 0);
        for y in 0..self.h { for x in 0..self.w { if self.data[y * self.w + x] >= thr { x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x + 1); y1 = y1.max(y + 1); } } }
        if x0 == usize::MAX { return None; }
        let (sx, sy) = (self.frame_w as f32 / self.w as f32, self.frame_h as f32 / self.h as f32);
        Some(BBox { x: x0 as f32 * sx, y: y0 as f32 * sy, w: (x1 - x0) as f32 * sx, h: (y1 - y0) as f32 * sy })
    }
    /// Share of the frame at or above `thr`.
    pub fn area_frac(&self, thr: f32) -> f32 { if self.data.is_empty() { 0.0 } else { self.data.iter().filter(|v| **v >= thr).count() as f32 / self.data.len() as f32 } }
    /// Share of the mask's mass that lies inside `r` (frame px); e.g. how much of the subject a crop keeps.
    pub fn mass_inside(&self, r: &BBox) -> f32 {
        let (sx, sy) = (self.frame_w as f32 / self.w.max(1) as f32, self.frame_h as f32 / self.h.max(1) as f32);
        let (mut tot, mut ins) = (0.0f32, 0.0f32);
        for y in 0..self.h { for x in 0..self.w { let v = self.data[y * self.w + x]; if v <= 0.0 { continue; } tot += v;
            let c = BBox { x: x as f32 * sx, y: y as f32 * sy, w: sx, h: sy }; ins += v * c.frac_inside(r); } }
        if tot > 0.0 { ins / tot } else { 0.0 }
    }
    /// Mass-weighted centre (frame px).
    pub fn centroid(&self) -> Option<[f32; 2]> {
        let (sx, sy) = (self.frame_w as f32 / self.w.max(1) as f32, self.frame_h as f32 / self.h.max(1) as f32);
        let (mut t, mut cx, mut cy) = (0.0f32, 0.0f32, 0.0f32);
        for y in 0..self.h { for x in 0..self.w { let v = self.data[y * self.w + x]; t += v; cx += v * (x as f32 + 0.5) * sx; cy += v * (y as f32 + 0.5) * sy; } }
        (t > 0.0).then(|| [cx / t, cy / t])
    }
}

/// A result plus where it came from.
#[derive(Clone, Debug)]
pub struct Seen<T> { pub value: Arc<T>, pub model: &'static str, pub fallback: Option<String>, pub cached: bool }

/// Cache key: the source (a media path, an asset id, or "comp:<hash>" for a composited frame) and the frame number.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FrameKey { pub source: String, pub frame: i64 }
impl FrameKey {
    pub fn new(source: impl Into<String>, frame: i64) -> Self { FrameKey { source: source.into(), frame } }
    /// Key a frame by its pixels (any edit that changes the picture changes the key).
    pub fn of_pixels(f: &Frame) -> Self { FrameKey { source: format!("px:{:016x}:{}x{}", pixel_hash(f), f.w, f.h), frame: 0 } }
}

/// FNV-1a over the frame's 8-bit display pixels.
pub fn pixel_hash(f: &Frame) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for v in &f.d { h ^= (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8 as u64; h = h.wrapping_mul(0x100000001b3); }
    h
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Kind { Faces, Objects, Subject, Matte }

#[derive(Clone)]
enum Val { Faces(Arc<Vec<FaceBox>>), Objects(Arc<Vec<Detection>>), Mask(Arc<Mask>) }

const CACHE_MAX: usize = 96;
struct Cache { map: HashMap<(Kind, FrameKey), (Val, &'static str, Option<String>)>, order: VecDeque<(Kind, FrameKey)> }
fn cache() -> &'static Mutex<Cache> { static C: OnceLock<Mutex<Cache>> = OnceLock::new(); C.get_or_init(|| Mutex::new(Cache { map: HashMap::new(), order: VecDeque::new() })) }

fn cached(kind: Kind, key: &FrameKey) -> Option<(Val, &'static str, Option<String>)> { cache().lock().unwrap_or_else(|e| e.into_inner()).map.get(&(kind, key.clone())).cloned() }
fn store(kind: Kind, key: FrameKey, v: Val, model: &'static str, fb: Option<String>) {
    // fallbacks are not cached: once the model is downloaded the next call should use it
    if fb.is_some() { return; }
    let mut c = cache().lock().unwrap_or_else(|e| e.into_inner());
    if c.map.insert((kind, key.clone()), (v, model, fb)).is_none() { c.order.push_back((kind, key)); }
    while c.order.len() > CACHE_MAX { if let Some(k) = c.order.pop_front() { c.map.remove(&k); } }
}

/// Drop every cached result (e.g. in tests).
pub fn clear_cache() { let mut c = cache().lock().unwrap_or_else(|e| e.into_inner()); c.map.clear(); c.order.clear(); }

#[cfg(feature = "vision")]
fn rgba(f: &Frame) -> Vec<u8> { crate::render::to_rgba8(f) }

#[cfg(not(feature = "vision"))]
fn unavailable() -> String { models::ModelError::Disabled.to_string() }

pub fn detect_faces(f: &Frame, key: Option<FrameKey>) -> Seen<Vec<FaceBox>> {
    let key = key.unwrap_or_else(|| FrameKey::of_pixels(f));
    if let Some((Val::Faces(v), m, fb)) = cached(Kind::Faces, &key) { return Seen { value: v, model: m, fallback: fb, cached: true }; }
    #[cfg(feature = "vision")]
    let r = { let px = rgba(f); infer::faces(&post::Rgba { w: f.w, h: f.h, px: &px }) };
    #[cfg(not(feature = "vision"))]
    let r: Result<Vec<FaceBox>, String> = Err(unavailable());
    let (v, fb) = match r { Ok(v) => (Arc::new(v), None), Err(e) => (Arc::new(vec![]), Some(format!("face model unavailable: {e}; no face boxes (the saliency map cannot find faces)"))) };
    store(Kind::Faces, key, Val::Faces(v.clone()), "yunet", fb.clone());
    Seen { value: v, model: if fb.is_some() { "none" } else { "yunet" }, fallback: fb, cached: false }
}

pub fn detect_objects(f: &Frame, key: Option<FrameKey>) -> Seen<Vec<Detection>> {
    let key = key.unwrap_or_else(|| FrameKey::of_pixels(f));
    if let Some((Val::Objects(v), m, fb)) = cached(Kind::Objects, &key) { return Seen { value: v, model: m, fallback: fb, cached: true }; }
    #[cfg(feature = "vision")]
    let r = { let px = rgba(f); infer::objects(&post::Rgba { w: f.w, h: f.h, px: &px }) };
    #[cfg(not(feature = "vision"))]
    let r: Result<Vec<Detection>, String> = Err(unavailable());
    let (v, fb) = match r { Ok(v) => (Arc::new(v), None), Err(e) => (Arc::new(vec![]), Some(format!("object model unavailable: {e}; no person/object boxes"))) };
    store(Kind::Objects, key, Val::Objects(v.clone()), "yolox_nano", fb.clone());
    Seen { value: v, model: if fb.is_some() { "none" } else { "yolox_nano" }, fallback: fb, cached: false }
}

/// The saliency map of `f` as a Mask (the fallback for subject and matte): spectral residual on luma, 64 cells on the long side.
pub fn saliency_mask(f: &Frame, why: String) -> Mask {
    use crate::ext::saliency_map as sm;
    let long = f.w.max(f.h).max(1); let (ww, wh) = (((f.w * 64) as f64 / long as f64).round().max(1.0) as usize, ((f.h * 64) as f64 / long as f64).round().max(1.0) as usize);
    let g = sm::area_down(&sm::luma_of(f), f.w, f.h, ww, wh); let s = sm::spectral_residual(&g, ww, wh, 2.5);
    Mask { w: ww, h: wh, data: s.into_iter().map(|v| v as f32).collect(), frame_w: f.w, frame_h: f.h, model: "saliency", fallback: Some(why) }
}

fn detect_mask(f: &Frame, key: Option<FrameKey>, kind: Kind) -> Seen<Mask> {
    let key = key.unwrap_or_else(|| FrameKey::of_pixels(f));
    if let Some((Val::Mask(v), m, fb)) = cached(kind, &key) { return Seen { value: v, model: m, fallback: fb, cached: true }; }
    let model = if kind == Kind::Matte { "modnet" } else { "u2netp" };
    #[cfg(feature = "vision")]
    let r = { let px = rgba(f); let im = post::Rgba { w: f.w, h: f.h, px: &px }; if kind == Kind::Matte { infer::matte(&im) } else { infer::subject(&im) } };
    #[cfg(not(feature = "vision"))]
    let r: Result<(usize, usize, Vec<f32>), String> = Err(unavailable());
    let m = match r {
        Ok((w, h, data)) => Mask { w, h, data, frame_w: f.w, frame_h: f.h, model, fallback: None },
        Err(e) => saliency_mask(f, format!("{} model unavailable: {e}; used the saliency map instead", if kind == Kind::Matte { "matte" } else { "subject" })),
    };
    let (mm, fb) = (m.model, m.fallback.clone()); let v = Arc::new(m);
    store(kind, key, Val::Mask(v.clone()), mm, fb.clone());
    Seen { value: v, model: mm, fallback: fb, cached: false }
}

pub fn detect_subject(f: &Frame, key: Option<FrameKey>) -> Seen<Mask> { detect_mask(f, key, Kind::Subject) }
pub fn detect_matte(f: &Frame, key: Option<FrameKey>) -> Seen<Mask> { detect_mask(f, key, Kind::Matte) }

/// Faces in `f` (empty when the model is unavailable).
pub fn faces(f: &Frame) -> Vec<FaceBox> { detect_faces(f, None).value.as_ref().clone() }
/// People and objects in `f`, best first (empty when the model is unavailable).
pub fn objects(f: &Frame) -> Vec<Detection> { detect_objects(f, None).value.as_ref().clone() }
/// Only the "person" detections.
pub fn people(f: &Frame) -> Vec<Detection> { objects(f).into_iter().filter(|d| d.class_id == 0).collect() }
/// Where the main subject is (U^2-Netp; the saliency map when the model is unavailable, see `Mask::fallback`).
pub fn subject_mask(f: &Frame) -> Mask { detect_subject(f, None).value.as_ref().clone() }
/// Person matte (MODNet; the saliency map when the model is unavailable).
pub fn person_matte(f: &Frame) -> Mask { detect_matte(f, None).value.as_ref().clone() }

/// Whether the model runtime is compiled in.
pub const ENABLED: bool = cfg!(feature = "vision");

#[cfg(test)]
mod tests;
