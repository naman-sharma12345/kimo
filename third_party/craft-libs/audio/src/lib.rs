// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/audio/src/lib.rs,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: only the integer-PCM WAVE/AIFF wrappers craft-aaf uses; wav_pcm is built on essence_pcm::parse_wav.
//! craft-audio, reduced to the integer-PCM WAVE and AIFF wrappers interchange files embed (craft-aaf's needs).
pub mod essence_pcm;
mod write_raw;
pub use write_raw::{pcm_aiff_file, pcm_aiff_header, pcm_wav_file, pcm_wav_header};

/// Integer PCM taken out of a WAV file, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawPcm { pub data: Vec<u8>, pub channels: u16, pub sample_rate: u32, pub bits: u16 }

/// The integer PCM of a WAV file, bit for bit; `None` for anything else.
pub fn wav_pcm(bytes: &[u8]) -> Option<RawPcm> {
    essence_pcm::parse_wav(bytes).map(|(data, channels, sample_rate, bits)| RawPcm { data, channels, sample_rate, bits })
}
