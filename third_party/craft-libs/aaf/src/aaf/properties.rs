// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/aaf,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Borrowed little-endian AAF property-stream records, independent of CFB and object policy.

/// One indexed AAF property. The payload borrows the original stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Property<'a> {
    pub id: u16,
    pub form: u16,
    pub payload: &'a [u8],
}

/// Malformed property-stream framing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PropertyError {
    ShortHeader,
    UnsupportedByteOrder,
    TruncatedIndex,
    TruncatedValue,
}

/// Property-stream record iterator. It does not deduplicate IDs, interpret forms,
/// copy payloads or impose object budgets; those decisions belong to the caller.
#[derive(Clone, Debug)]
pub struct Properties<'a> {
    headers: std::slice::Iter<'a, [u8; 6]>,
    payload: &'a [u8],
}

impl<'a> Properties<'a> {
    /// Validate the header and complete index. Payload extents are checked during
    /// iteration; callers can validate all extents before copying by iterating a clone.
    pub fn new(stream: &'a [u8]) -> Result<Self, PropertyError> {
        let [order, _, lo, hi, tail @ ..] = stream else { return Err(PropertyError::ShortHeader) };
        if *order != 0x4c {
            return Err(PropertyError::UnsupportedByteOrder);
        }
        let count = usize::from(u16::from_le_bytes([*lo, *hi]));
        let (headers, payload) = tail.split_at_checked(count * 6).ok_or(PropertyError::TruncatedIndex)?;
        Ok(Self { headers: headers.as_chunks::<6>().0.iter(), payload })
    }
    pub(super) fn remaining_count(&self) -> usize {
        self.headers.len()
    }

    // The store checks duplicate IDs before validating the corresponding payload,
    // so a record that is both duplicate and truncated retains its old diagnostic.
    pub(super) fn next_id(&self) -> Option<u16> {
        let [lo, hi, ..] = self.headers.as_slice().first()?;
        Some(u16::from_le_bytes([*lo, *hi]))
    }
}

impl<'a> Iterator for Properties<'a> {
    type Item = Result<Property<'a>, PropertyError>;

    fn next(&mut self) -> Option<Self::Item> {
        let [id0, id1, form0, form1, len0, len1] = self.headers.next()?;
        let len = usize::from(u16::from_le_bytes([*len0, *len1]));
        let Some((payload, tail)) = self.payload.split_at_checked(len) else {
            self.headers = [].iter();
            return Some(Err(PropertyError::TruncatedValue));
        };
        self.payload = tail;
        Some(Ok(Property { id: u16::from_le_bytes([*id0, *id1]), form: u16::from_le_bytes([*form0, *form1]), payload }))
    }
}

