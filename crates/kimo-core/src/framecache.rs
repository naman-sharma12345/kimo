//! V3-6a: whole-frame cache for interactive use (scrub, replay, repeated render_frame). Never used by export.
//! Key = 128-bit hash of everything that decides the pixels: the canonical JSON of the whole Project (clips, params, effect stack, resolution),
//! the time, the transparent flag, and mtime+size of every existing file path named anywhere in the project (media, LUTs, fonts, images).
//! A hit returns a copy of the stored frame, so a warm frame is bit-identical to the cold render by construction. LRU by bytes, capped.
use crate::effects::Frame;
use crate::model::Project;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

pub struct FrameCache { map: HashMap<u128, (Arc<Frame>, u64)>, bytes: usize, pub cap: usize, tick: u64, pub hits: u64, pub misses: u64, pub evictions: u64 }

fn fnv128(data: &[u8], mut a: u64, mut b: u64) -> (u64, u64) { for &x in data { a ^= x as u64; a = a.wrapping_mul(0x100000001b3); b = b.wrapping_mul(0x100000001b3) ^ (x as u64).rotate_left(17); } (a, b) }

fn collect_paths(v: &Value, out: &mut Vec<String>) { match v { Value::String(s) => { if s.len() < 4096 && (s.contains('/') || s.contains('\\') || s.contains('.')) && !s.contains('\n') { out.push(s.clone()); } }
    Value::Array(a) => for x in a { collect_paths(x, out); }, Value::Object(o) => for x in o.values() { collect_paths(x, out); }, _ => {} } }

/// Cache key for rendering `p` at time `t`. Every string in the project that names an existing file contributes its size and mtime, so touching a LUT or a media file invalidates.
pub fn key(p: &Project, t: f64, transparent: bool) -> u128 {
    let v = serde_json::to_value(p).unwrap_or(Value::Null); let json = serde_json::to_vec(&v).unwrap_or_default();
    let (mut a, mut b) = fnv128(&json, 0xcbf29ce484222325, 0x84222325cbf29ce4);
    let mut paths = vec![]; collect_paths(&v, &mut paths); paths.sort(); paths.dedup();
    for s in &paths { if let Ok(m) = std::fs::metadata(p.resolve(s)) { if m.is_file() { let mt = m.modified().ok().and_then(|x| x.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_nanos()).unwrap_or(0);
        let rec = format!("{s}|{}|{mt}", m.len()); let r = fnv128(rec.as_bytes(), a, b); a = r.0; b = r.1; } } }
    let tail = [t.to_bits().to_le_bytes().as_slice(), &[transparent as u8]].concat(); let r = fnv128(&tail, a, b);
    ((r.0 as u128) << 64) | r.1 as u128
}

impl FrameCache {
    pub fn new(cap: usize) -> Self { FrameCache { map: HashMap::new(), bytes: 0, cap, tick: 0, hits: 0, misses: 0, evictions: 0 } }
    pub fn bytes(&self) -> usize { self.bytes }
    pub fn len(&self) -> usize { self.map.len() }
    pub fn clear(&mut self) { self.map.clear(); self.bytes = 0; }
    pub fn set_cap(&mut self, cap: usize) { self.cap = cap; self.evict(); }
    fn evict(&mut self) { while self.bytes > self.cap { let Some((&k, _)) = self.map.iter().min_by_key(|(_, (_, t))| *t) else { break }; if let Some((f, _)) = self.map.remove(&k) { self.bytes -= f.d.len() * 4; self.evictions += 1; } } }
    pub fn get(&mut self, k: u128) -> Option<Frame> { self.tick += 1; let tick = self.tick; match self.map.get_mut(&k) { Some((f, t)) => { *t = tick; self.hits += 1; Some(Frame { w: f.w, h: f.h, d: f.d.clone() }) } None => { self.misses += 1; None } } }
    pub fn put(&mut self, k: u128, f: &Frame) { let b = f.d.len() * 4; if b > self.cap { return; } self.tick += 1; self.bytes += b; if let Some((old, _)) = self.map.insert(k, (Arc::new(Frame { w: f.w, h: f.h, d: f.d.clone() }), self.tick)) { self.bytes -= old.d.len() * 4; } self.evict(); }
}

/// Render through the cache: hit = copy of the stored frame, miss = render + store.
pub fn render_cached(fc: &mut FrameCache, p: &Project, t: f64, cache: &crate::render::Cache) -> Result<Frame, String> {
    let k = key(p, t, false) ^ cache.proxy_variant(); if let Some(f) = fc.get(k) { return Ok(f); }
    let f = crate::render::render_frame(p, t, cache)?; fc.put(k, &f); Ok(f)
}

/// Default cap: a quarter of the memory budget when one is set, else 256 MB.
pub fn default_cap() -> usize { let b = crate::guard::BUDGET.load(std::sync::atomic::Ordering::Relaxed); if b == usize::MAX { 256 << 20 } else { b / 4 } }
