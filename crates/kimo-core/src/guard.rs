//! Sandbox layer: resource limits around every tool call so a bad call fails safely instead of hanging or taking the machine down.
//! Layers: (1) argument sanitizer (finite numbers, magnitude, array/string size, nesting), (2) project limits checked after every
//! mutation (canvas pixels, fps, clip count, duration) with automatic rollback, (3) memory budget enforced by a counting allocator
//! (`Frame::new` and the render loop check it and fail the call; a hard backstop refuses allocations far beyond the budget),
//! (4) cooperative cancellation + wall-clock time limit per call (per-call `Cancel` token, checked per clip / per exported frame),
//! (5) panics are caught and the project is rolled back. All limits can be overridden with KIMO_* environment variables.
use serde_json::Value;
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering::Relaxed};

pub static LIVE: AtomicUsize = AtomicUsize::new(0);
pub static BUDGET: AtomicUsize = AtomicUsize::new(usize::MAX);
/// Per-call cancellation token. One token is created per tool call; cancelling it stops only that call's work.
pub type Cancel = std::sync::Arc<AtomicBool>;
pub fn new_token() -> Cancel { std::sync::Arc::new(AtomicBool::new(false)) }
thread_local! { static TOKEN: std::cell::RefCell<Option<Cancel>> = const { std::cell::RefCell::new(None) }; }
/// The token active on this thread, if any.
pub fn current_token() -> Option<Cancel> { TOKEN.with(|t| t.borrow().clone()) }
pub struct Restore(Option<Cancel>);
impl Drop for Restore { fn drop(&mut self) { let p = self.0.take(); TOKEN.with(|t| *t.borrow_mut() = p); } }
/// Run `f` with `tok` as this thread's cancellation token (restored afterwards, also on panic).
pub fn with_token<R>(tok: &Cancel, f: impl FnOnce() -> R) -> R { let _g = install_token(tok); f() }
/// RAII form of `with_token`: the token is active on this thread until the guard drops.
pub fn install_token(tok: &Cancel) -> TokenGuard { TokenGuard(Restore(TOKEN.with(|t| t.borrow_mut().replace(tok.clone())))) }
pub struct TokenGuard(#[allow(dead_code)] Restore);
fn inherit<R>(tok: Option<Cancel>, f: impl FnOnce() -> R) -> R { match tok { Some(t) => with_token(&t, f), None => f() } }
/// The only sanctioned way to start threads in kimo-core: the child inherits the caller's cancellation token.
pub fn spawn_with_token<F, T>(f: F) -> std::thread::JoinHandle<T> where F: FnOnce() -> T + Send + 'static, T: Send + 'static { let tok = current_token(); std::thread::spawn(move || inherit(tok, f)) }
pub fn spawn_builder_with_token<F, T>(b: std::thread::Builder, f: F) -> std::io::Result<std::thread::JoinHandle<T>> where F: FnOnce() -> T + Send + 'static, T: Send + 'static { let tok = current_token(); b.spawn(move || inherit(tok, f)) }
/// Scoped spawn whose child inherits the caller's token.
pub fn scope_spawn<'scope, 'env, F, T>(sc: &'scope std::thread::Scope<'scope, 'env>, f: F) -> std::thread::ScopedJoinHandle<'scope, T> where F: FnOnce() -> T + Send + 'scope, T: Send + 'scope { let tok = current_token(); sc.spawn(move || inherit(tok, f)) }
/// Hard allocation ceiling (backstop). Set in `init`: never above 85% of physical RAM, so the guard refuses before the kernel OOM killer fires.
pub static HARD: AtomicUsize = AtomicUsize::new(usize::MAX);

/// Counting allocator. Install in a binary with `#[global_allocator] static A: kimo_core::guard::Counting = kimo_core::guard::Counting;`.
pub struct Counting;
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, l: Layout) -> *mut u8 { let b = BUDGET.load(Relaxed); if b != usize::MAX && LIVE.load(Relaxed).saturating_add(l.size()) > HARD.load(Relaxed) { return std::ptr::null_mut(); } let p = System.alloc(l); if !p.is_null() { LIVE.fetch_add(l.size(), Relaxed); } p }
    unsafe fn alloc_zeroed(&self, l: Layout) -> *mut u8 { let b = BUDGET.load(Relaxed); if b != usize::MAX && LIVE.load(Relaxed).saturating_add(l.size()) > HARD.load(Relaxed) { return std::ptr::null_mut(); } let p = System.alloc_zeroed(l); if !p.is_null() { LIVE.fetch_add(l.size(), Relaxed); } p }
    unsafe fn dealloc(&self, p: *mut u8, l: Layout) { System.dealloc(p, l); LIVE.fetch_sub(l.size(), Relaxed); }
    unsafe fn realloc(&self, p: *mut u8, l: Layout, n: usize) -> *mut u8 { let np = System.realloc(p, l, n); if !np.is_null() { if n >= l.size() { LIVE.fetch_add(n - l.size(), Relaxed); } else { LIVE.fetch_sub(l.size() - n, Relaxed); } } np }
}

#[derive(Clone, Debug)]
pub struct Limits { pub max_num: f64, pub max_array: usize, pub max_string: usize, pub max_depth: usize, pub max_pixels: u64, pub max_fps: f64, pub max_clips: usize, pub max_duration_s: f64, pub mem_bytes: usize, pub timeout_s: u64, pub export_timeout_s: u64 }
fn env<T: std::str::FromStr>(k: &str, d: T) -> T { std::env::var(k).ok().and_then(|v| v.parse().ok()).unwrap_or(d) }
fn total_mem() -> usize { let (mi, cg) = host_mem_sources(); total_mem_from(mi.as_deref(), &cg) }
/// The host files memory detection reads: /proc/meminfo and the cgroup v2 / v1 memory limits (None when unreadable).
fn host_mem_sources() -> (Option<String>, [Option<String>; 2]) {
    (std::fs::read_to_string("/proc/meminfo").ok(), ["/sys/fs/cgroup/memory.max", "/sys/fs/cgroup/memory/memory.limit_in_bytes"].map(|f| std::fs::read_to_string(f).ok()))
}
/// Host/cgroup memory detection, separated from the file reads so it can be tested: physical RAM is /proc/meminfo's MemTotal (8 GiB when
/// unreadable); a cgroup v2 `memory.max` or v1 `memory.limit_in_bytes` value lowers it when it is a number above 64 MiB and below physical RAM
/// ("max" and v1's huge "unlimited" value are ignored).
pub fn total_mem_from(meminfo: Option<&str>, cgroup_limits: &[Option<String>]) -> usize {
    let mut m = meminfo.and_then(|s| s.lines().find(|l| l.starts_with("MemTotal")).and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<usize>().ok()))).map(|k| k * 1024).unwrap_or(8 << 30);
    for c in cgroup_limits { if let Some(v) = c.as_deref().and_then(|s| s.trim().parse::<usize>().ok()) { if v > (64 << 20) && v < m { m = v; } } }
    m
}
/// Default heap budget for a box with `total` bytes of memory (KIMO_MEM_MB unset): 55% of it.
pub fn default_budget(total: usize) -> usize { total / 100 * 55 }
impl Limits { pub fn from_env() -> Self { Limits { max_num: env("KIMO_MAX_NUM", 1e8), max_array: env("KIMO_MAX_ARRAY", 200_000), max_string: env("KIMO_MAX_STRING", 8 << 20), max_depth: env("KIMO_MAX_DEPTH", 40), max_pixels: env("KIMO_MAX_PIXELS", 40_000_000), max_fps: env("KIMO_MAX_FPS", 480.0), max_clips: env("KIMO_MAX_CLIPS", 200_000), max_duration_s: env("KIMO_MAX_DURATION_S", 86_400.0),
    mem_bytes: env::<usize>("KIMO_MEM_MB", 0).checked_mul(1 << 20).filter(|v| *v > 0).unwrap_or(default_budget(total_mem())), timeout_s: env("KIMO_TIMEOUT_S", 600), export_timeout_s: env("KIMO_EXPORT_TIMEOUT_S", 7200) } } }
pub fn limits() -> &'static Limits { static L: std::sync::OnceLock<Limits> = std::sync::OnceLock::new(); L.get_or_init(Limits::from_env) }
/// Set the memory budget from the limits (called once at server start).
pub fn init() { let b = match std::env::var("KIMO_MEMORY_BUDGET_BYTES") { Ok(v)=>{let b=v.parse::<usize>().expect("KIMO_MEMORY_BUDGET_BYTES must be integer");let ceiling=crate::resource_limits::memory_ceiling() as usize;if b>ceiling {panic!("KIMO_MEMORY_BUDGET_BYTES exceeds physical RAM minus 1 GiB ceiling {ceiling}")}b},Err(_)=>limits().mem_bytes }; BUDGET.store(b, Relaxed); HARD.store(b.saturating_add(b / 4).saturating_add(512 << 20).min(total_mem() / 100 * 85), Relaxed); }
/// Encoder child (ffmpeg/x264) RSS measured at 1920x1080, default settings: 479 MB; with the opt-in low_memory x264 params: 257 MB (see FEATURES.md, export memory). Scaled by pixel count.
pub const ENCODER_RSS_1080P: usize = 479 << 20;
pub const ENCODER_RSS_1080P_LOW: usize = 257 << 20;
fn mem_available() -> Option<usize> { std::fs::read_to_string("/proc/meminfo").ok().and_then(|s| s.lines().find(|l| l.starts_with("MemAvailable")).and_then(|l| l.split_whitespace().nth(1).and_then(|v| v.parse::<usize>().ok()))).map(|k| k * 1024) }
/// Decide how an export can run before any frame is rendered: `(workers, low_memory)` or an explanatory error. The usable memory is the smaller of the
/// remaining heap budget and the kernel's MemAvailable, minus the encoder child (which the counting allocator cannot see). `low_memory` is turned on
/// automatically when only the lighter encoder fits. With no budget installed (library use) there is no limit.
pub fn export_plan(w: u32, h: u32, cpus: usize, want_low: bool) -> Result<(usize, bool), String> { export_plan_n(w, h, cpus, want_low, FRAME_BUFFERS_SAFE) }
/// `export_plan` with an explicit per-frame buffer count (see `render::frame_buffers`).
pub fn export_plan_n(w: u32, h: u32, cpus: usize, want_low: bool, bufs: usize) -> Result<(usize, bool), String> { export_plan_with(&MemProfile::host(), w, h, cpus, want_low, bufs) }
/// The memory facts the export planner decides from: the installed heap budget (usize::MAX = none), the live heap bytes, and the kernel's
/// MemAvailable. `host()` reads this process and machine; a test injects a fixed profile so the heuristic is checked independently of the box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MemProfile { pub budget: usize, pub live: usize, pub available: Option<usize> }
impl MemProfile { pub fn host() -> Self { MemProfile { budget: BUDGET.load(Relaxed), live: LIVE.load(Relaxed), available: mem_available() } } }
/// `export_plan_n` on an explicit memory profile (the same heuristic; `export_plan_n` passes `MemProfile::host()`).
pub fn export_plan_with(m: &MemProfile, w: u32, h: u32, cpus: usize, want_low: bool, bufs: usize) -> Result<(usize, bool), String> {
    let b = m.budget; if b == usize::MAX { return Ok((cpus.max(1), want_low)); }
    let px = (w as usize).saturating_mul(h as usize); let per = px.saturating_mul(16 * bufs.max(1)).max(1);
    let scale = |full: usize| ((full as u128 * px as u128) / (1920u128 * 1080)) as usize;
    let avail = b.saturating_sub(m.live).min(m.available.unwrap_or(usize::MAX));
    let try_with = |low: bool| { let reserve = scale(if low { ENCODER_RSS_1080P_LOW } else { ENCODER_RSS_1080P }); if avail >= reserve + per { Some(((avail - reserve) / per).clamp(1, cpus.max(1))) } else { None } };
    if let Some(n) = try_with(want_low) { return Ok((n, want_low)); }
    if !want_low { if let Some(n) = try_with(true) { return Ok((n, true)); } }
    let need = scale(ENCODER_RSS_1080P_LOW) + per;
    Err(format!("export of {w}x{h} needs about {} MB (one frame worker plus the encoder, low_memory), but only {} MB is available (budget {} MB, KIMO_MEM_MB; kernel free memory counted); lower the resolution or raise KIMO_MEM_MB", need >> 20, avail >> 20, b >> 20))
}
/// Worker threads for a frame-parallel export: each in-flight frame needs about 12 full-frame f32 buffers (16 bytes per pixel each), so the count is limited by the free memory budget (at least 1).
/// Full-frame f32 buffers one in-flight frame is assumed to hold when nothing better is known (kept for any scene that is not fully measured).
pub const FRAME_BUFFERS_SAFE: usize = 12;
pub fn export_threads(w: u32, h: u32, cpus: usize) -> usize { export_threads_n(w, h, cpus, FRAME_BUFFERS_SAFE) }
pub fn export_threads_n(w: u32, h: u32, cpus: usize, bufs: usize) -> usize { let b = BUDGET.load(Relaxed); if b == usize::MAX { return cpus.max(1); } let per = (w as usize).saturating_mul(h as usize).saturating_mul(16 * bufs.max(1)).max(1); (b.saturating_sub(LIVE.load(Relaxed)) / per).clamp(1, cpus.max(1)) }

/// Panic (caught by `safe_dispatch`) when over the memory budget or cancelled. Cheap; call from loops that allocate or run long.
#[inline] pub fn check() { if TOKEN.with(|t| t.borrow().as_ref().map_or(false, |c| c.load(Relaxed))) { panic!("cancelled: call exceeded its time limit"); } let b = BUDGET.load(Relaxed); if b != usize::MAX && LIVE.load(Relaxed) > b { panic!("memory budget exceeded: {} MB live of {} MB allowed (raise with KIMO_MEM_MB)", LIVE.load(Relaxed) >> 20, b >> 20); } }
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

/// Outcome of a guarded tool call. `Hung` means the worker did not stop after the cooperative cancel plus a 15 s grace period; the caller should report it and
/// exit so the machine is not held hostage (the MCP server exits 124, a CLI exits non-zero).
pub enum Guarded { Done(Result<Value, crate::errors::ToolError>), Hung { limit: u64 } }

/// The one place that runs a tool under the sandbox limits (shared by kimo-mcp and kimo-cli so timeouts and cancellation cannot drift): a 64 MB-stack worker
/// thread with the cancel token, the wall-clock limit (KIMO_EXPORT_TIMEOUT_S for tools whose name starts with "export", else KIMO_TIMEOUT_S), cooperative cancel
/// on timeout, then a 15 s grace period.
pub fn run_tool_guarded(ss: &std::sync::Arc<std::sync::Mutex<crate::commands::Session>>, name: &str, args: &Value) -> Guarded {
    use std::time::Duration;
    let l = limits(); let limit = if name.starts_with("export") { l.export_timeout_s } else { l.timeout_s };
    let tok = new_token(); let tok2 = tok.clone();
    let (tx, rx) = std::sync::mpsc::channel(); let (s2, n2, a2) = (ss.clone(), name.to_string(), args.clone());
    spawn_builder_with_token(std::thread::Builder::new().stack_size(64 << 20), move || { let mut g = s2.lock().unwrap_or_else(|e| e.into_inner()); let r = crate::commands::dispatch_ex_with(&mut g, &n2, &a2, &tok2); let _ = tx.send(r); }).ok();
    match rx.recv_timeout(Duration::from_secs(limit)) {
        Ok(r) => Guarded::Done(r),
        Err(_) => {
            tok.store(true, Relaxed);
            match rx.recv_timeout(Duration::from_secs(15)) {
                Ok(_) => Guarded::Done(Err(format!("tool '{name}' stopped: exceeded the {limit}s time limit (KIMO_TIMEOUT_S); project rolled back where possible").into())),
                Err(_) => Guarded::Hung { limit },
            }
        }
    }
}

pub mod limits;
pub use limits::effective_limits;

#[cfg(test)]
mod mem_detection_tests {
    use super::{default_budget, host_mem_sources, total_mem, total_mem_from};
    const MI: &str = "MemTotal:        8131228 kB\nMemFree:  100 kB\nMemAvailable:    5000000 kB\n";
    #[test]
    fn meminfo_total_is_physical_ram_and_a_cgroup_limit_lowers_it() {
        assert_eq!(total_mem_from(Some(MI), &[None, None]), 8131228 * 1024);
        assert_eq!(total_mem_from(Some(MI), &[Some("2147483648\n".into()), None]), 2 << 30, "cgroup v2 memory.max");
        assert_eq!(total_mem_from(Some(MI), &[None, Some("1073741824\n".into())]), 1 << 30, "cgroup v1 memory.limit_in_bytes");
        assert_eq!(total_mem_from(Some(MI), &[Some("2147483648".into()), Some("1073741824".into())]), 1 << 30, "the smaller limit wins");
    }
    #[test]
    fn unlimited_tiny_or_unreadable_cgroup_values_are_ignored() {
        let phys = 8131228 * 1024;
        assert_eq!(total_mem_from(Some(MI), &[Some("max\n".into()), None]), phys);
        assert_eq!(total_mem_from(Some(MI), &[None, Some("9223372036854771712".into())]), phys, "v1 unlimited");
        assert_eq!(total_mem_from(Some(MI), &[Some(format!("{}", 64 << 20)), None]), phys, "64 MiB or less is not a usable limit");
        assert_eq!(total_mem_from(None, &[None, None]), 8 << 30, "no /proc/meminfo: 8 GiB");
        assert_eq!(total_mem_from(Some("garbage"), &[Some("2147483648".into())]), 2 << 30);
    }
    #[test]
    fn host_detection_reads_this_machine_and_the_default_budget_is_55_percent() {
        let (mi, cg) = host_mem_sources();
        let t = total_mem(); assert_eq!(t, total_mem_from(mi.as_deref(), &cg)); assert!(t > 64 << 20);
        if let Some(s) = mi.as_deref() { assert!(t <= total_mem_from(Some(s), &[]), "a cgroup limit only ever lowers physical RAM"); }
        assert_eq!(default_budget(1982 << 20) >> 20, 1090);
    }
}
