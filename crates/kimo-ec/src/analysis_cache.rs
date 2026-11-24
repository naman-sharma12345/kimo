// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit 53f1312448b9d87ddf9d4ec7ce62112a02da6fa9),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed.
//! Bounded parsed-JSON caches. Keep the content key with the owned source text so repeated
//! renders do not rehash long analyses. Compare contents on lookup: evaluated parameter
//! strings can be cloned, edited in place, or allocated again at an old address.

use std::sync::{Arc, Mutex, OnceLock};

struct Entry<T> {
    json: Box<str>,
    key: u64,
    value: Arc<T>,
}

pub(crate) struct AnalysisCache<T> {
    entries: OnceLock<Mutex<Vec<Entry<T>>>>,
}

pub(crate) fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

impl<T> AnalysisCache<T> {
    pub(crate) const fn new() -> Self {
        Self { entries: OnceLock::new() }
    }

    pub(crate) fn get(&self, json: &str, parse: impl FnOnce(&str) -> Option<T>) -> Option<(u64, Arc<T>)> {
        let entries = self.entries.get_or_init(Default::default);
        {
            let m = entries.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(e) = m.iter().rev().find(|e| e.json.as_ref() == json) {
                return Some((e.key, e.value.clone()));
            }
        }
        let value = Arc::new(parse(json)?);
        let key = fnv(json.as_bytes());
        let mut m = entries.lock().unwrap_or_else(|e| e.into_inner());
        // Another render may have filled the entry while this one parsed it.
        if let Some(e) = m.iter().rev().find(|e| e.json.as_ref() == json) {
            return Some((e.key, e.value.clone()));
        }
        // Preserve the existing nine-entry bound and clear-on-overflow policy.
        if m.len() > 8 {
            m.clear();
        }
        m.push(Entry { json: json.into(), key, value: value.clone() });
        Some((key, value))
    }
}


