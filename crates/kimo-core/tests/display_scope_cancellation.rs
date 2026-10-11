use kimo_core::{
    color::scope_image::{render, Kind, Options},
    effects::Frame,
    guard,
};
use std::sync::atomic::Ordering::Relaxed;
#[test]
fn display_scope_respects_cancellation_without_mutating_input() {
    let f = Frame {
        w: 1,
        h: 1,
        d: vec![1., 0., 0., 1.],
    };
    let before = f.d.clone();
    for kind in [
        Kind::Waveform,
        Kind::Parade,
        Kind::Vectorscope,
        Kind::Histogram,
    ] {
        guard::CANCEL.store(true, Relaxed);
        let result = std::panic::catch_unwind(|| render(&f, Options::sdr(kind)));
        guard::CANCEL.store(false, Relaxed);
        assert!(result.is_err());
        assert_eq!(f.d, before);
    }
}
