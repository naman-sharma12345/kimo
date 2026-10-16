//! Tiny std-only data parallelism (no dependency). Deterministic: work is split by
//! disjoint index ranges and every output element is computed by exactly one thread
//! with the same arithmetic as the serial code, so results are bit-identical.
use std::sync::atomic::{AtomicUsize, Ordering};

static OVERRIDE: AtomicUsize = AtomicUsize::new(0);
/// Force the worker count (0 = auto). Used by benches/tests; KIMO_THREADS env also works.
pub fn set_threads(n: usize) { OVERRIDE.store(n, Ordering::Relaxed); }
/// Per-thread worker cap (0 = none), inventoried in tests/thread_hygiene.rs: only the spectator's preview thread sets it, for its own
/// lifetime; it changes how many workers run, never what they compute (results are bit-identical for any count).
thread_local! { static LOCAL: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
/// Cap the worker count for work started from the current thread only (0 = no cap), e.g. the spectator's
/// background preview renderer, so it leaves cores free for tool calls. Other threads are unaffected.
pub fn set_thread_cap(n: usize) { LOCAL.with(|l| l.set(n)); }
pub fn threads() -> usize {
    let cap = LOCAL.with(|l| l.get());
    if cap > 0 { return threads_uncapped().min(cap); }
    threads_uncapped()
}
fn threads_uncapped() -> usize {
    let o = OVERRIDE.load(Ordering::Relaxed); if o > 0 { return o.min(16); }
    if let Ok(v) = std::env::var("KIMO_THREADS") { if let Ok(n) = v.parse::<usize>() { if n > 0 { return n.min(16); } } }
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).min(8)
}
/// Raw pointer wrapper for disjoint writes from scoped threads.
pub struct SyncPtr<T>(pub *mut T);
impl<T> Clone for SyncPtr<T> { fn clone(&self) -> Self { *self } }
impl<T> Copy for SyncPtr<T> {}
unsafe impl<T> Send for SyncPtr<T> {}
unsafe impl<T> Sync for SyncPtr<T> {}

/// Run `f(start, end)` over `0..n` split into contiguous ranges across threads.
/// Serial when `n * cost_hint` is small. `f` must only write elements owned by its range.
pub fn ranges(n: usize, min_per_thread: usize, f: impl Fn(usize, usize) + Sync) {
    let t = threads().min(n / min_per_thread.max(1)).max(1);
    if t <= 1 { f(0, n); return; }
    let step = (n + t - 1) / t;
    std::thread::scope(|s| { for k in 0..t { let (a, b) = (k * step, ((k + 1) * step).min(n)); if a >= b { break; } let f = &f; crate::guard::scope_spawn(s, move || f(a, b)); } });
}
/// Fill `out` (len = rows*stride) row-wise: `f(row_index, row_slice)`.
pub fn rows_mut<T: Send>(out: &mut [T], stride: usize, f: impl Fn(usize, &mut [T]) + Sync) {
    if stride == 0 { return; }
    let rows = out.len() / stride; let p = SyncPtr(out.as_mut_ptr());
    ranges(rows, 8, |a, b| { let p = p; for y in a..b { let row = unsafe { std::slice::from_raw_parts_mut(p.0.add(y * stride), stride) }; f(y, row); } });
}
