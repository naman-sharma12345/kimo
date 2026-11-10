//! Unit 6a A (ffmpeg-free): the shard texture cache key takes the decode flag from the ctx, so two contexts never share a decoded texture.
use kimo_core::shards::{tex_key, RenderCtx};
#[test] fn tex_key_depends_on_ctx_not_on_globals() {
    let (m, u) = (RenderCtx { work_linear: true }, RenderCtx { work_linear: false });
    assert_ne!(tex_key(&m, "a.png", [10., 20.]), tex_key(&u, "a.png", [10., 20.]));
    assert_eq!(tex_key(&m, "a.png", [10., 20.]), tex_key(&m, "a.png", [10., 20.]));
    assert_eq!(RenderCtx::default(), u);
}
