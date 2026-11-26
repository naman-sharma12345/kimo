// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/aaf,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! WAV/PCM FileDescriptor metadata, without loading a SourceMob or touching its locators.

use super::ids::{cls, pid};

/// The two audio descriptor classes commonly attached to an external WAV source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorKind {
    Wave,
    Pcm,
}

impl DescriptorKind {
    /// Classify a compound-file storage's CLSID in AAF on-disk AUID byte order
    /// (little-endian GUID fields, not RFC 4122/network order).
    pub fn from_auid_le(class: [u8; 16]) -> Option<Self> {
        match class {
            cls::WAVE_DESCRIPTOR => Some(Self::Wave),
            cls::PCM_DESCRIPTOR => Some(Self::Pcm),
            _ => None,
        }
    }
}

/// One property from an already framed and caller-validated object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorProperty<'a> {
    Missing,
    OtherForm,
    Data(&'a [u8]),
}

/// Why a descriptor field is unavailable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DescriptorFieldError {
    Missing,
    OtherForm,
    Truncated,
}

/// A lazy descriptor view over a caller's property store. This neither opens
/// the media file nor determines whether the AAF source is external.
pub struct AudioFileDescriptor<F> {
    kind: DescriptorKind,
    property: F,
}

impl<'a, F: Fn(u16) -> DescriptorProperty<'a>> AudioFileDescriptor<F> {
    /// Supply the property view for this class; the descriptor chooses AAF property IDs.
    pub fn new(kind: DescriptorKind, property: F) -> Self {
        Self { kind, property }
    }

    /// `FileDescriptor::SampleRate` as the signed numerator and denominator on
    /// the wire. The caller's rational policy (including zero) is not applied.
    pub fn sample_rate(&self) -> Result<(i32, i32), DescriptorFieldError> {
        let [a, b, c, d, e, f, g, h] = self.bytes(pid::SAMPLE_RATE)?;
        Ok((i32::from_le_bytes([a, b, c, d]), i32::from_le_bytes([e, f, g, h])))
    }

    /// `FileDescriptor::Length` in sample-rate units, before validating clip ranges.
    pub fn length(&self) -> Result<i64, DescriptorFieldError> {
        self.bytes(pid::FILE_LENGTH).map(i64::from_le_bytes)
    }

    /// `SoundDescriptor::Channels` for a PCM descriptor. A WAVEDescriptor has
    /// no such declared count here; an absent PCM field is also left to the caller.
    pub fn pcm_channels(&self) -> Result<Option<u32>, DescriptorFieldError> {
        if self.kind == DescriptorKind::Wave {
            return Ok(None);
        }
        match self.bytes(pid::CHANNELS) {
            Ok(bytes) => Ok(Some(u32::from_le_bytes(bytes))),
            Err(DescriptorFieldError::Missing) => Ok(None),
            Err(err) => Err(err),
        }
    }

    fn bytes<const N: usize>(&self, pid: u16) -> Result<[u8; N], DescriptorFieldError> {
        match (self.property)(pid) {
            DescriptorProperty::Data(data) => data.first_chunk().copied().ok_or(DescriptorFieldError::Truncated),
            DescriptorProperty::Missing => Err(DescriptorFieldError::Missing),
            DescriptorProperty::OtherForm => Err(DescriptorFieldError::OtherForm),
        }
    }
}

