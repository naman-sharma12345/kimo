//! Sandbox layer: resource limits around every tool call so a bad call fails safely instead of hanging or taking the machine down.
//! Layers: (1) argument sanitizer (finite numbers, magnitude, array/string size, nesting), (2) project limits checked after every
//! mutation (canvas pixels, fps, clip count, duration) with automatic rollback, (3) memory budget enforced by a counting allocator
//! (`Frame::new` and the render loop check it and fail the call; a hard backstop refuses allocations far beyond the budget),
//! (4) cooperative cancellation + wall-clock time limit per call (`CANCEL`, checked per clip / per exported frame),
//! (5) panics are caught and the project is rolled back. All limits can be overridden with KIMO_* environment variables.
use serde_json::Value;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

pub static LIVE: AtomicUsize = AtomicUsize::new(0);
pub static BUDGET: AtomicUsize = AtomicUsize::new(usize::MAX);
pub static CANCEL: AtomicBool = AtomicBool::new(false);

/// Counting allocator. Install in a binary with `#[global_allocator] static A: kimo_core::guard::Counting = kimo_core::guard::Counting;`.
pub struct Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { let b = BUDGET.load(Relaxed); if b != usize::MAX && LIVE.load(Relaxed).saturating_add(l.size()) > b.saturating_add(b / 4).saturating_add(512 << 20) { return std::ptr::null_mut(); } let p = System.alloc(l); if !p.is_null() { LIVE.fetch_add(l.size(), Relaxed); } p }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 { let b = BUDGET.load(Relaxed); if b != usize::MAX && LIVE.load(Relaxed).saturating_add(l.size()) > b.saturating_add(b / 4).saturating_add(512 << 20) { return std::ptr::null_mut(); } let p = System.alloc_zeroed(l); if !p.is_null() { LIVE.fetch_add(l.size(), Relaxed); } p }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { System.dealloc(p, l); LIVE.fetch_sub(l.size(), Relaxed); }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { let np = System.realloc(p, l, n); if !np.is_null() { if n >= l.size() { LIVE.fetch_add(n - l.size(), Relaxed); } else { LIVE.fetch_sub(l.size() - n, Relaxed); } } np }
}

#[derive(Clone, Debug)]
pub struct Limits { pub max_num: f64, pub max_array: usize, pub max_string: usize, pub max_depth: usize, pub max_pixels: u64, pub max_fps: f64, pub max_clips: usize, pub max_duration_s: f64, pub mem_bytes: usize, pub timeout_s: u64, pub export_timeout_s: u64 }
fn env<T: std::str::FromStr>(k: &str, d: T) -> T { std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d) }
fn total_mem() -> usize {
    let mut m = std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| s.lines().find(|l| l.starts_with("MemTotal")).and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<usize>().ok()))).map(|k| k * 1024).unwrap_or(8 << 30);
    for f in ["/sys/fs/cgroup/memory.max", "/sys/fs/cgroup/memory/memory.limit_in_bytes"] { if let Some(v) = std::fs::read_to_string(f).ok().and_then(|s| s.trim().parse::<usize>().ok()) { if v > (64 << 20) && v < m { m = v; } } }
    m
}
impl Limits { pub fn from_env() -> Self { Limits { max_num: env("KIMO_MAX_NUM", 1e8), max_array: env("KIMO_MAX_ARRAY", 200_000), max_string: env("KIMO_MAX_STRING", 8 << 20), max_depth: env("KIMO_MAX_DEPTH", 40), max_pixels: env("KIMO_MAX_PIXELS", 40_000_000), max_fps: env("KIMO_MAX_FPS", 480.0), max_clips: env("KIMO_MAX_CLIPS", 200_000), max_duration_s: env("KIMO_MAX_DURATION_S", 86_400.0),
    mem_bytes: env::<usize>("KIMO_MEM_MB", 0).checked_mul(1 << 20).filter(|v| *v > 0).unwrap_or(total_mem() / 10 * 7), timeout_s: env("KIMO_TIMEOUT_S", 600), export_timeout_s: env("KIMO_EXPORT_TIMEOUT_S", 7200) } } }
pub fn limits() -> &'static Limits { static L: std::sync::OnceLock<Limits> = std::sync::OnceLock::new(); L.get_or_init(Limits::from_env) }
/// Set the memory budget from the limits (called once at server start).
pub fn init() { BUDGET.store(limits().mem_bytes, Relaxed); }

/// Panic (caught by `safe_dispatch`) when over the memory budget or cancelled. Cheap; call from loops that allocate or run long.
#[inline] pub fn check() { if CANCEL.load(Relaxed) { panic!("cancelled: call exceeded its time limit"); } let b = BUDGET.load(Relaxed); if b != usize::MAX && LIVE.load(Relaxed) > b { panic!("memory budget exceeded: {} MB live of {} MB allowed (raise with KIMO_MEM_MB)", LIVE.load(Relaxed) >> 20, b >> 20); } }
/// Check before a large allocation of `bytes`.
pub fn reserve(bytes: usize) { check(); let b = BUDGET.load(Relaxed); if b != usize::MAX && LIVE.load(Relaxed).saturating_add(bytes) > b { panic!("memory budget exceeded: allocation of {} MB would pass the {} MB budget (raise with KIMO_MEM_MB)", bytes >> 20, b >> 20); } }

pub fn sanitize(v: &Value, l: &Limits, depth: usize) -> Result<(), String> {
    if depth > l.max_depth { return Err(format!("argument nesting deeper than {}", l.max_depth)); }
    match v {
        Value::Number(n) => { if let Some(f) = n.as_f64() { if !f.is_finite() { return Err("non-finite number".into()); } if f.abs() > l.max_num { return Err(format!("number {f:e} is outside the allowed range of +-{:e}", l.max_num)); } } Ok(()) }
        Value::String(s) => if s.len() > l.max_string { Err(format!("string longer than {} bytes", l.max_string)) } else { Ok(()) },
        Value::Array(a) => { if a.len() > l.max_array { return Err(format!("array longer than {} items", l.max_array)); } for x in a { sanitize(x, l, depth + 1)?; } Ok(()) }
        Value::Object(o) => { if o.len() > 10_000 { return Err("object has too many keys".into()); } for x in o.values() { sanitize(x, l, depth + 1)?; } Ok(()) }
        _ => Ok(()),
    }
}

/// Project-level limits, checked after a mutating call. Err means the call must be rolled back.
pub fn check_project(p: &crate::model::Project, l: &Limits) -> Result<(), String> {
    p.grade_scopes.validate(p)?;
    if (p.width as u64) * (p.height as u64) > l.max_pixels { return Err(format!("canvas {}x{} is over the {} pixel limit (KIMO_MAX_PIXELS)", p.width, p.height, l.max_pixels)); }
    if !(p.fps > 0.0 && p.fps <= l.max_fps) { return Err(format!("fps {} outside 0..{}", p.fps, l.max_fps)); }
    let n: usize = p.tracks.iter().map(|t| t.clips.len()).sum(); if n > l.max_clips { return Err(format!("{n} clips is over the {} clip limit", l.max_clips)); }
    if p.duration() > l.max_duration_s { return Err(format!("timeline duration {:.0}s is over the {:.0}s limit", p.duration(), l.max_duration_s)); }
    Ok(())
}
