//! Shared engine limit surface for cross-lane audio and pixel primitives.
pub use crate::resource_limits::Limits;
pub fn effective_limits() -> Limits {
    let mut l = crate::resource_limits::current();
    l.memory_bytes = l
        .memory_bytes
        .min(super::BUDGET.load(std::sync::atomic::Ordering::Relaxed) as u64);
    l
}
