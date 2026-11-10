//! Session resource limits, separate from project/undo. Render workers capture the caller's values.
use serde::Serialize;
#[derive(Clone, Copy, Debug, Serialize)]
pub struct Limits {
    pub expr_max_ops: Option<u64>,
    pub expr_max_pixel_ops: Option<u64>,
    pub expr_timeout_ms: Option<u64>,
    pub memory_bytes: u64,
    pub max_pixels: u64,
    pub max_samples: u64,
}
impl Limits {
    pub fn defaults() -> Self {
        Self {
            expr_max_ops: Some(512),
            expr_max_pixel_ops: Some(4_000_000_000),
            expr_timeout_ms: Some(30000),
            memory_bytes: crate::guard::BUDGET.load(std::sync::atomic::Ordering::Relaxed) as u64,
            max_pixels: 64_000_000,
            max_samples: 48_000_000,
        }
    }
    pub fn startup() -> Self {
        let mut l = Self::defaults();
        for (k, v) in [
            ("KIMO_EXPR_MAX_OPS", &mut l.expr_max_ops),
            ("KIMO_EXPR_MAX_PIXEL_OPS", &mut l.expr_max_pixel_ops),
            ("KIMO_EXPR_TIMEOUT_MS", &mut l.expr_timeout_ms),
        ] {
            if let Ok(n) = std::env::var(k) {
                if let Ok(n) = n.parse() {
                    *v = Some(n)
                }
            }
        }
        for (k, v) in [
            ("KIMO_MAX_PIXELS", &mut l.max_pixels),
            ("KIMO_MAX_SAMPLES", &mut l.max_samples),
        ] {
            if let Ok(n) = std::env::var(k) {
                if let Ok(n) = n.parse() {
                    *v = n
                }
            }
        }
        l
    }
}
// thread-local-ok: per-call resource-limits scope; Restore guard resets it on drop (B4 session budgets), holds no render state
thread_local! {static CURRENT:std::cell::Cell<Option<Limits>>=const{std::cell::Cell::new(None)};}
pub fn current() -> Limits {
    CURRENT.with(|c| c.get()).unwrap_or_else(Limits::startup)
}
pub struct Restore(Option<Limits>);
impl Drop for Restore {
    fn drop(&mut self) {
        CURRENT.with(|c| c.set(self.0))
    }
}
pub fn install(l: Limits) -> Restore {
    Restore(CURRENT.with(|c| c.replace(Some(l))))
}
pub fn memory_ceiling() -> u64 {
    let physical = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|n| n.parse::<u64>().ok())
        })
        .unwrap_or(2 * 1024 * 1024)
        * 1024;
    physical.saturating_sub(1 << 30)
}
pub fn reserve(bytes: usize) -> Result<(), String> {
    let l = current();
    let live = crate::guard::LIVE.load(std::sync::atomic::Ordering::Relaxed);
    let process = crate::guard::BUDGET.load(std::sync::atomic::Ordering::Relaxed) as u64;
    let effective = l.memory_bytes.min(process);
    if (live as u64).saturating_add(bytes as u64) > effective {
        return Err(format!("memory_bytes={effective} exceeded; process budget {process} (raise at startup with KIMO_MEMORY_BUDGET_BYTES; session set_limits can only lower it)"));
    }
    Ok(())
}
