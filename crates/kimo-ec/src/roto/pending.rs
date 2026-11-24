// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Bounded assembly of the objects a tracker reports separately for each frame.

use std::collections::BTreeMap;
use std::mem::size_of;

use crate::segment::TrackedFrame;

/// Separate from the model's memory banks: probability maps are full-resolution and can otherwise
/// grow with the entire clip when objects start on different frames.
pub(super) const MAX_PENDING_BYTES: usize = 256 << 20;

pub(super) struct PendingMasks {
    frames: BTreeMap<i64, Vec<Option<TrackedFrame>>>,
    objects: usize,
    bytes: usize,
    limit: usize,
}

impl PendingMasks {
    pub(super) fn new(objects: usize, limit: usize) -> Self {
        Self { frames: BTreeMap::new(), objects, bytes: 0, limit }
    }

    /// Probability allocation plus a conservative allowance for the map node and object slots.
    fn frame_bytes(&self) -> usize {
        self.objects.saturating_mul(size_of::<Option<TrackedFrame>>()).saturating_add(256)
    }

    fn mask_bytes(mask: &TrackedFrame) -> usize {
        mask.prob.capacity().saturating_mul(size_of::<f32>())
    }

    /// Complete frames leave immediately; incomplete frames may never exceed the budget. A rejected
    /// insertion leaves the buffer unchanged, including when it replaces an object's previous result.
    pub(super) fn insert(&mut self, object: usize, frame: i64, mask: TrackedFrame) -> Result<Option<Vec<Option<TrackedFrame>>>, String> {
        if object >= self.objects {
            return Err("tracker reported an unknown object".into());
        }
        if self.objects == 1 {
            return Ok(Some(vec![Some(mask)]));
        }
        let existing = self.frames.get(&frame);
        let old_bytes = existing.and_then(|p| p.get(object)).and_then(Option::as_ref).map_or(0, Self::mask_bytes);
        let added = Self::mask_bytes(&mask).saturating_add(if existing.is_none() { self.frame_bytes() } else { 0 });
        let bytes = self.bytes.saturating_sub(old_bytes).checked_add(added);
        let Some(bytes) = bytes.filter(|n| *n <= self.limit) else {
            return Err(format!("pending object masks exceed their {} MiB memory budget", self.limit >> 20));
        };
        let parts = self.frames.entry(frame).or_insert_with(|| vec![None; self.objects]);
        if let Some(slot) = parts.get_mut(object) {
            *slot = Some(mask);
        }
        self.bytes = bytes;
        if parts.iter().all(Option::is_some) {
            let parts = self.frames.remove(&frame).unwrap_or_default();
            let used = parts.iter().flatten().fold(self.frame_bytes(), |n, m| n.saturating_add(Self::mask_bytes(m)));
            self.bytes = self.bytes.saturating_sub(used);
            return Ok(Some(parts));
        }
        Ok(None)
    }

    pub(super) fn into_incomplete(self) -> BTreeMap<i64, Vec<Option<TrackedFrame>>> {
        self.frames
    }
}


