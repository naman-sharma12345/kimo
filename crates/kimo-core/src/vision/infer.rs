//! Model runtime: tract-onnx (pure Rust, no native libraries). Each model is loaded once per input shape and kept.
use super::models::{self, ModelError};
use super::post::{self, Rgba};
use super::{Detection, FaceBox};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use tract_onnx::prelude::*;

type Plan = Arc<TypedRunnableModel>;

fn plans() -> &'static Mutex<HashMap<(String, usize, usize), Plan>> { static P: OnceLock<Mutex<HashMap<(String, usize, usize), Plan>>> = OnceLock::new(); P.get_or_init(Default::default) }

/// A model that failed to load (offline, bad file) is not retried on every frame: wait this long before trying again.
const RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(60);
fn failures() -> &'static Mutex<HashMap<String, (std::time::Instant, String)>> { static F: OnceLock<Mutex<HashMap<String, (std::time::Instant, String)>>> = OnceLock::new(); F.get_or_init(Default::default) }

pub fn forget_failures() { failures().lock().unwrap_or_else(|e| e.into_inner()).clear(); }

fn plan(id: &str, w: usize, h: usize) -> Result<Plan, String> {
    let key = (id.to_string(), w, h);
    if let Some(p) = plans().lock().unwrap_or_else(|e| e.into_inner()).get(&key) { return Ok(p.clone()); }
    if let Some((t, m)) = failures().lock().unwrap_or_else(|e| e.into_inner()).get(id) { if t.elapsed() < RETRY_AFTER { return Err(m.clone()); } }
    let r = (|| -> Result<Plan, String> {
        let spec = models::spec(id).ok_or_else(|| format!("unknown vision model {id}"))?;
        let path = models::ensure(spec).map_err(|e: ModelError| format!("{id}: {e}"))?;
        load(&path, w, h).map_err(|e| format!("{id}: could not load the model ({e})"))
    })();
    match r {
        Ok(p) => { plans().lock().unwrap_or_else(|e| e.into_inner()).insert(key, p.clone()); failures().lock().unwrap_or_else(|e| e.into_inner()).remove(id); Ok(p) }
        Err(m) => { failures().lock().unwrap_or_else(|e| e.into_inner()).insert(id.to_string(), (std::time::Instant::now(), m.clone())); Err(m) }
    }
}

/// Load an ONNX file for a fixed 1x3xHxW f32 input. The graph's own intermediate shape annotations are dropped
/// (some exports pin a symbolic batch there that tract cannot unify); tract infers them from the input.
pub fn load(path: &std::path::Path, w: usize, h: usize) -> TractResult<Plan> {
    let onnx = tract_onnx::onnx();
    let mut proto = onnx.proto_model_for_path(path)?;
    if let Some(g) = proto.graph.as_mut() { g.value_info.clear(); }
    let mut m = onnx.model_for_proto_model(&proto)?;
    m.set_input_fact(0, f32::fact([1, 3, h, w]).into())?;
    m.into_optimized()?.into_runnable()
}

fn run(p: &Plan, data: Vec<f32>, w: usize, h: usize) -> Result<TVec<TValue>, String> {
    let t = Tensor::from_shape(&[1, 3, h, w], &data).map_err(|e| e.to_string())?;
    p.run(tvec!(t.into())).map_err(|e| format!("inference failed: {e}"))
}

fn flat(v: &TValue) -> Result<&[f32], String> { v.to_plain_array_view::<f32>().map_err(|e| e.to_string())?.to_slice().ok_or_else(|| "model output is not contiguous".to_string()) }

pub fn faces(im: &Rgba) -> Result<Vec<FaceBox>, String> {
    let s = post::YUNET_SIZE; let p = plan("yunet", s, s)?;
    let (data, scale, _) = post::to_tensor(im, s, s, Some(0.0), [2, 1, 0], &|_, v| v);
    let out = run(&p, data, s, s)?;
    if out.len() != 12 { return Err(format!("yunet: expected 12 outputs, got {}", out.len())); }
    let o: Vec<&[f32]> = out.iter().map(flat).collect::<Result<_, _>>()?;
    Ok(post::decode_yunet(&o, s, scale, im.w, im.h, 0.6))
}

pub fn objects(im: &Rgba) -> Result<Vec<Detection>, String> {
    let s = post::YOLOX_SIZE; let p = plan("yolox_nano", s, s)?;
    let (data, scale, _) = post::to_tensor(im, s, s, Some(114.0), [2, 1, 0], &|_, v| v);
    let out = run(&p, data, s, s)?;
    let o = flat(&out[0])?; if o.len() != 3549 * 85 { return Err(format!("yolox_nano: unexpected output of {} values", o.len())); }
    Ok(post::decode_yolox(o, s, scale, im.w, im.h, 0.3))
}

/// U^2-Netp saliency at 320x320 (stretched), min-max scaled; returns the 320x320 map.
pub fn subject(im: &Rgba) -> Result<(usize, usize, Vec<f32>), String> {
    let s = 320; let p = plan("u2netp", s, s)?;
    // rembg's preprocessing: divide by the picture's max value, then ImageNet mean/std
    let mx = (0..im.w * im.h).flat_map(|i| im.px[i * 4..i * 4 + 3].iter().copied()).max().unwrap_or(255).max(1) as f32;
    const MEAN: [f32; 3] = [0.485, 0.456, 0.406]; const STD: [f32; 3] = [0.229, 0.224, 0.225];
    let (data, _, _) = post::to_tensor(im, s, s, None, [0, 1, 2], &|k, v| (v / mx - MEAN[k]) / STD[k]);
    let out = run(&p, data, s, s)?;
    let mut m = flat(&out[0])?.to_vec(); if m.len() != s * s { return Err("u2netp: unexpected output size".into()); }
    post::minmax(&mut m); Ok((s, s, m))
}

/// MODNet person matte: the long side at 512, both sides multiples of 32.
pub fn matte(im: &Rgba) -> Result<(usize, usize, Vec<f32>), String> {
    let k = 512.0 / im.w.max(im.h) as f32; let r32 = |v: usize| (((v as f32 * k) / 32.0).round() as usize).max(1) * 32;
    let (w, h) = (r32(im.w), r32(im.h)); let p = plan("modnet", w, h)?;
    let (data, _, _) = post::to_tensor(im, w, h, None, [0, 1, 2], &|_, v| v / 127.5 - 1.0);
    let out = run(&p, data, w, h)?;
    let m = flat(&out[0])?.to_vec(); if m.len() != w * h { return Err("modnet: unexpected output size".into()); }
    Ok((w, h, m))
}
