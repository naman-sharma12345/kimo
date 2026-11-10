//! Spectator telemetry hook, engine side.
//!
//! A host (kimo-mcp) may install one progress sink at startup; long-running tools call `progress` (or
//! `progress_frames` when they count frames) as they advance. With no sink installed either call is a single
//! `OnceLock` load and returns. Tools never learn whether anyone is listening, and nothing here can change a
//! tool's result.
use std::sync::OnceLock;

/// One progress report from the tool currently running.
#[derive(Debug, Clone, Copy)]
pub struct Progress<'a> {
    /// Fraction done, 0..=1.
    pub frac: f64,
    /// Short stage label ("frames").
    pub stage: &'a str,
    /// Frames finished so far (so the next frame's 0-based index), when the tool counts frames.
    pub frame: Option<u64>,
    /// Frames the tool will produce in all, when it counts frames.
    pub total_frames: Option<u64>,
}

/// Receives each progress report.
pub type ProgressFn = Box<dyn Fn(&Progress) + Send + Sync>;

static HOOK: OnceLock<ProgressFn> = OnceLock::new();

/// Install the process-wide progress sink. Returns false when one is already installed.
pub fn set_progress_hook(f: ProgressFn) -> bool { HOOK.set(f).is_ok() }

/// True once a sink is installed.
pub fn enabled() -> bool { HOOK.get().is_some() }

/// Report progress of the running tool. Free when no sink is installed.
#[inline]
pub fn progress(frac: f64, stage: &str) {
    if let Some(h) = HOOK.get() {
        let f = if frac.is_finite() { frac.clamp(0.0, 1.0) } else { 0.0 };
        h(&Progress { frac: f, stage, frame: None, total_frames: None })
    }
}

/// Report frame progress: `done` of `total` frames finished. Free when no sink is installed.
#[inline]
pub fn progress_frames(done: usize, total: usize, stage: &str) {
    if let Some(h) = HOOK.get() {
        let total = total.max(1);
        let done = done.min(total);
        h(&Progress { frac: done as f64 / total as f64, stage, frame: Some(done as u64), total_frames: Some(total as u64) })
    }
}
