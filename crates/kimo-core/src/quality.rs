//! Render quality: the project's `render_quality` ("legacy" or "v10"), carried through one frame's render.
//!
//! Effects, transitions and the resampler do not receive the project, so render::render_frame_opts sets this
//! context for the frame it renders (and restores the previous one when it ends, so a nested precomp render
//! cannot leak its setting). It is per thread: code that fans work out to other threads (`par::rows_mut`) reads
//! it first and passes the values in.
//!
//! "legacy" is every project saved before v10 and renders exactly as before. "v10" turns on the v10 algorithms:
//! linear-light blending, Gaussian blurs, bicubic magnification, transition motion blur, thin antialiased wipe
//! edges, per-frame dither, and sizes given for 1080p that scale with the frame (`px`).
use std::cell::Cell;

thread_local! { static CTX: Cell<(bool, f32)> = const { Cell::new((false, 1.0)) }; }

/// Restores the previous context when dropped.
pub struct Scope((bool, f32));
impl Drop for Scope { fn drop(&mut self) { CTX.with(|c| c.set(self.0)); } }

/// Render with v10 quality (or not) at this frame height until the returned scope is dropped.
pub fn enter(v10: bool, height: usize) -> Scope {
    let prev = CTX.with(|c| c.get());
    CTX.with(|c| c.set((v10, if height > 0 { height as f32 / 1080.0 } else { 1.0 })));
    Scope(prev)
}

/// True while rendering a v10 project.
pub fn v10() -> bool { CTX.with(|c| c.get().0) }

/// A size given for a 1080p frame, scaled to this frame in v10 (unchanged in legacy), so a look is the same
/// at 720p and at 4K.
pub fn px(v: f32) -> f32 { let (on, s) = CTX.with(|c| c.get()); if on { v * s } else { v } }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scopes_nest_and_restore() {
        assert!(!v10());
        { let _a = enter(true, 2160); assert!(v10()); assert_eq!(px(10.0), 20.0);
          { let _b = enter(false, 720); assert!(!v10()); assert_eq!(px(10.0), 10.0); }
          assert!(v10()); assert_eq!(px(10.0), 20.0); }
        assert!(!v10());
    }
}
