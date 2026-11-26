// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/audio/src/write/raw.rs,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: u32_to_extended copied in from write/aiff.rs; test modules removed.
//! Minimal WAVE and AIFF files around integer PCM the caller already has as bytes (essence
//! embedded in an AAF or OMF file): the 44-byte canonical WAVE header and the plain AIFF header,
//! with nothing a reader of such a summary could trip over (no `JUNK`, no extensible `fmt `).
//! [`crate::encode`] writes full WAV/BWF/RF64 and AIFF files from `f32`.
//!
//! PCM here is interleaved little-endian samples of `bits.div_ceil(8)` bytes, 8-bit unsigned
//! (the layout of a WAVE `data` chunk). Sizes saturate at the 32-bit fields instead of wrapping.

/// From craft-audio's write/aiff.rs.
fn u32_to_extended(r: u32) -> [u8; 10] {
    let mut out = [0u8; 10];
    if r == 0 {
        return out;
    }
    let e = 31 - r.leading_zeros();
    let exp = (16383 + e) as u16;
    let mant = u64::from(r) << (63 - e);
    out[..2].copy_from_slice(&exp.to_be_bytes());
    out[2..].copy_from_slice(&mant.to_be_bytes());
    out
}

/// A complete WAVE file around little-endian PCM.
pub fn pcm_wav_file(pcm: &[u8], channels: u16, sample_rate: u32, bits: u16) -> Vec<u8> {
    let mut v = pcm_wav_header(u32::try_from(pcm.len()).unwrap_or(u32::MAX), channels, sample_rate, bits);
    v.extend_from_slice(pcm);
    if pcm.len() % 2 == 1 {
        v.push(0);
    }
    v
}

/// The 44 bytes of a canonical WAVE file up to and including the `data` chunk header, for
/// `data_len` bytes of PCM.
pub fn pcm_wav_header(data_len: u32, channels: u16, sample_rate: u32, bits: u16) -> Vec<u8> {
    let block = channels.max(1).saturating_mul(bits.div_ceil(8));
    let mut v = Vec::with_capacity(44);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&36u32.saturating_add(data_len).saturating_add(data_len & 1).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&channels.max(1).to_le_bytes());
    v.extend_from_slice(&sample_rate.to_le_bytes());
    v.extend_from_slice(&sample_rate.saturating_mul(u32::from(block)).to_le_bytes());
    v.extend_from_slice(&block.to_le_bytes());
    v.extend_from_slice(&bits.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    v
}

/// A complete AIFF file (big-endian, 8-bit signed) from little-endian PCM.
pub fn pcm_aiff_file(pcm: &[u8], channels: u16, sample_rate: u32, bits: u16) -> Vec<u8> {
    let width = usize::from(bits.div_ceil(8)).max(1);
    let frames = pcm.len() / (width * usize::from(channels.max(1)));
    let mut v = pcm_aiff_header(u32::try_from(frames).unwrap_or(u32::MAX), channels, sample_rate, bits);
    let head = v.len();
    for s in pcm.chunks_exact(width) {
        if width == 1 {
            v.extend(s.iter().map(|b| b ^ 0x80)); // WAVE 8-bit is unsigned, AIFF signed
        } else {
            v.extend(s.iter().rev());
        }
    }
    if (v.len() - head) % 2 == 1 {
        v.push(0);
    }
    v
}

/// The 54 bytes of an AIFF file up to the sound data (`FORM`, `COMM`, `SSND` with its offset and
/// block size), for `frames` sample frames.
pub fn pcm_aiff_header(frames: u32, channels: u16, sample_rate: u32, bits: u16) -> Vec<u8> {
    let data_len = u64::from(frames).saturating_mul(u64::from(channels.max(1))).saturating_mul(u64::from(bits.div_ceil(8)));
    let chunk = |extra: u64| u32::try_from(extra.saturating_add(data_len)).unwrap_or(u32::MAX);
    let mut v = Vec::with_capacity(54);
    v.extend_from_slice(b"FORM");
    v.extend_from_slice(&chunk(4 + 26 + 16 + (data_len & 1)).to_be_bytes());
    v.extend_from_slice(b"AIFFCOMM");
    v.extend_from_slice(&18u32.to_be_bytes());
    v.extend_from_slice(&channels.max(1).to_be_bytes());
    v.extend_from_slice(&frames.to_be_bytes());
    v.extend_from_slice(&bits.to_be_bytes());
    v.extend_from_slice(&u32_to_extended(sample_rate));
    v.extend_from_slice(b"SSND");
    v.extend_from_slice(&chunk(8).to_be_bytes());
    v.extend_from_slice(&[0; 8]);
    v
}

