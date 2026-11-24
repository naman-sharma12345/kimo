// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Offline analysis for Essential Sound auto-ducking: level envelope of the trigger audio
//! (dialogue, SFX…), activity detection with blip removal and gap bridging, and the volume
//! keyframes that duck a music clip under the detected regions.
//!
//! Pure functions on plain slices; times are seconds, levels dBFS / dB.

/// Level floor (dBFS) for silence.
pub const FLOOR_DB: f32 = -120.0;

/// RMS level (dBFS) per hop: value `k` is the RMS of the window of `window_s` seconds centred on
/// `k · hop_s` (clipped at the signal edges), with the power averaged over channels. The result
/// has `ceil(len / hop)` entries; silence reads [`FLOOR_DB`].
pub fn envelope_db(channels: &[&[f32]], sample_rate: f32, hop_s: f32, window_s: f32) -> Vec<f32> {
    let len = channels.iter().map(|c| c.len()).min().unwrap_or(0);
    if len == 0 || channels.is_empty() {
        return Vec::new();
    }
    let hop = ((hop_s * sample_rate).round() as usize).max(1);
    let half = ((window_s * sample_rate / 2.0).round() as usize).max(1);
    // prefix sums of the channel-averaged power
    let nch = channels.len() as f64;
    let mut pre = Vec::with_capacity(len + 1);
    pre.push(0.0f64);
    let mut acc = 0.0f64;
    for i in 0..len {
        let p: f64 = channels.iter().map(|c| (c[i] as f64) * (c[i] as f64)).sum::<f64>() / nch;
        acc += p;
        pre.push(acc);
    }
    (0..len.div_ceil(hop))
        .map(|k| {
            let c = k * hop;
            let (a, b) = (c.saturating_sub(half), (c + half).min(len));
            let ms = (pre[b] - pre[a]).max(0.0) / (b - a).max(1) as f64;
            if ms <= 1e-12 { FLOOR_DB } else { ((10.0 * ms.log10()) as f32).max(FLOOR_DB) }
        })
        .collect()
}

/// Regions `(start_s, end_s)` where the envelope exceeds `threshold_db`. Gaps shorter than
/// `bridge_s` are merged first, then regions shorter than `min_on_s` are dropped. A frame `k`
/// covers `[k·hop, (k+1)·hop)`.
pub fn activity(env_db: &[f32], hop_s: f32, threshold_db: f32, min_on_s: f32, bridge_s: f32) -> Vec<(f64, f64)> {
    // the f32 hop rounded to whole microseconds, so 0.01 means exactly 0.01 s
    let hop = (hop_s as f64 * 1e6).round() / 1e6;
    let mut raw: Vec<(f64, f64)> = Vec::new();
    let mut start: Option<usize> = None;
    for (k, &v) in env_db.iter().enumerate() {
        match (v > threshold_db, start) {
            (true, None) => start = Some(k),
            (false, Some(s)) => {
                raw.push((s as f64 * hop, k as f64 * hop));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        raw.push((s as f64 * hop, env_db.len() as f64 * hop));
    }
    let mut merged: Vec<(f64, f64)> = Vec::new();
    for r in raw {
        match merged.last_mut() {
            Some(last) if r.0 - last.1 < bridge_s as f64 - 1e-9 => last.1 = r.1,
            _ => merged.push(r),
        }
    }
    merged.retain(|(a, b)| b - a >= min_on_s as f64 - 1e-9);
    merged
}

/// Essential Sound ducking "Sensitivity" (0…10) → detection threshold in dBFS:
/// `−20 − 4·s` (5 → −40 dBFS; higher sensitivity reacts to quieter triggers).
pub fn sensitivity_threshold_db(sensitivity: f32) -> f32 {
    -20.0 - 4.0 * sensitivity.clamp(0.0, 10.0)
}

/// Volume keyframes `(time_s, level_db)` that duck by `reduce_db` under each region: ramp from
/// `base_db` at `a − fade` down to `base_db − reduce_db` at `a`, hold until `b`, ramp back up to
/// `base_db` at `b + fade`. Regions whose ramps would overlap are merged (the level stays
/// ducked between them). Everything is clamped to `[clip_start_s, clip_end_s]`; a ramp cut by a
/// clip edge gets a keyframe at the edge with the ramp's value there (fully ducked when the region
/// itself reaches the edge). Times are strictly increasing; no regions → no keyframes. `fade_s` is
/// at least 1 ms so a ramp never collapses into two keyframes at the same time.
pub fn duck_keyframes(regions: &[(f64, f64)], clip_start_s: f64, clip_end_s: f64, base_db: f64, reduce_db: f64, fade_s: f64) -> Vec<(f64, f64)> {
    let fade = fade_s.max(1e-3);
    let ducked = base_db - reduce_db.abs();
    let mut sorted: Vec<(f64, f64)> = regions.iter().copied().filter(|(a, b)| b > a && *b > clip_start_s && *a < clip_end_s).collect();
    sorted.sort_by(|x, y| x.0.total_cmp(&y.0));
    // merge regions whose ramps touch or overlap
    let mut groups: Vec<(f64, f64)> = Vec::new();
    for r in sorted {
        match groups.last_mut() {
            Some(g) if r.0 - fade <= g.1 + fade => g.1 = g.1.max(r.1),
            _ => groups.push(r),
        }
    }
    let mut out: Vec<(f64, f64)> = Vec::new();
    let mut push = |t: f64, v: f64| {
        if t < clip_start_s || t > clip_end_s {
            return;
        }
        match out.last_mut() {
            Some(last) if t <= last.0 + 1e-9 => {
                last.1 = v;
            }
            _ => out.push((t, v)),
        }
    };
    // value of the down-ramp ending at `a` / up-ramp starting at `b`, at time t
    let down = |a: f64, t: f64| base_db + (ducked - base_db) * ((t - (a - fade)) / fade).clamp(0.0, 1.0);
    let up = |b: f64, t: f64| ducked + (base_db - ducked) * ((t - b) / fade).clamp(0.0, 1.0);
    for (a, b) in groups {
        let ramp_start = a - fade;
        if ramp_start >= clip_start_s {
            push(ramp_start, base_db);
        } else {
            push(clip_start_s, down(a, clip_start_s));
        }
        push(a.max(clip_start_s), ducked);
        if b < clip_end_s {
            push(b, ducked);
            if b + fade <= clip_end_s {
                push(b + fade, base_db);
            } else {
                push(clip_end_s, up(b, clip_end_s));
            }
        } else {
            push(clip_end_s, ducked);
        }
    }
    out
}


