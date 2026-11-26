// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/aaf,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Embedded PCM essence in and out of WAVE and AIFF wrappers (craft-audio does the containers).

/// PCM (little-endian, 8-bit unsigned), channels, sample rate and bits of a WAVE file.
pub(crate) fn parse_wav(b: &[u8]) -> Option<(Vec<u8>, u16, u32, u16)> {
    craft_audio::wav_pcm(b).map(|p| (p.data, p.channels, p.sample_rate, p.bits))
}
