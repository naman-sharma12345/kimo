// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/time,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Tempo and meter maps: the conversion between musical time (ticks) and samples.

use super::{MAX_POSITION, SampleRate, Samples, TimeError, to_samples};

/// Ticks per quarter note (Pro Tools' resolution).
pub const TICKS_PER_QUARTER: i64 = 960;

/// A tempo change at a tick position. `bpm` is quarter notes per minute.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TempoEvent {
    pub tick: i64,
    pub bpm: f64,
}

/// A meter (time signature) change at a tick position, which must fall on a bar line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct MeterEvent {
    pub tick: i64,
    pub numerator: u32,
    pub denominator: u32,
}

impl MeterEvent {
    pub fn ticks_per_beat(&self) -> i64 {
        let d = i64::from(self.denominator.max(1));
        (TICKS_PER_QUARTER * 4 / d).max(1)
    }
    pub fn ticks_per_bar(&self) -> i64 {
        self.ticks_per_beat().saturating_mul(i64::from(self.numerator.max(1)))
    }
}

/// A musical position: 1-based bar and beat plus ticks within the beat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BarBeat {
    pub bar: i64,
    pub beat: i64,
    pub tick: i64,
}

/// Tempo and meter changes for a session. Always has at least one of each, at tick 0.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(from = "TempoMapData")]
pub struct TempoMap {
    tempos: Vec<TempoEvent>,
    meters: Vec<MeterEvent>,
    /// For each tempo event, the elapsed time at its tick in quarter-note minutes (ticks / bpm,
    /// summed over the earlier segments). Lets tick/sample conversions binary-search the segment
    /// instead of walking every earlier tempo change, so converting many positions against a large
    /// tempo map stays O(log n) each. Rebuilt whenever `tempos` changes.
    #[serde(skip)]
    elapsed: Vec<f64>,
}

/// The serialized form of [`TempoMap`], without the derived lookup table.
#[derive(serde::Deserialize)]
struct TempoMapData {
    tempos: Vec<TempoEvent>,
    meters: Vec<MeterEvent>,
}

impl From<TempoMapData> for TempoMap {
    fn from(d: TempoMapData) -> Self {
        TempoMap::from_parts(d.tempos, d.meters)
    }
}

impl Default for TempoMap {
    fn default() -> Self {
        TempoMap::from_parts(vec![TempoEvent { tick: 0, bpm: 120.0 }], vec![MeterEvent { tick: 0, numerator: 4, denominator: 4 }])
    }
}

fn valid_bpm(bpm: f64) -> Result<f64, TimeError> {
    if bpm.is_finite() && (5.0..=1000.0).contains(&bpm) { Ok(bpm) } else { Err(TimeError::Tempo(bpm)) }
}

fn valid_meter(numerator: u32, denominator: u32) -> bool {
    (1..=64).contains(&numerator) && [1, 2, 4, 8, 16, 32, 64].contains(&denominator)
}

impl TempoMap {
    fn from_parts(tempos: Vec<TempoEvent>, meters: Vec<MeterEvent>) -> Self {
        let mut m = TempoMap { tempos, meters, elapsed: Vec::new() };
        m.reindex();
        m
    }

    /// Rebuild `elapsed` from `tempos`. The first segment runs from tick 0 at the first tempo.
    fn reindex(&mut self) {
        let mut elapsed = Vec::with_capacity(self.tempos.len());
        let mut acc = 0.0;
        let mut seg_tick = 0i64;
        let mut bpm = self.tempos.first().map_or(120.0, |e| e.bpm);
        for (i, e) in self.tempos.iter().enumerate() {
            if i > 0 {
                // Saturating: a file's ticks are indexed before `sanitize` orders them.
                acc += e.tick.saturating_sub(seg_tick) as f64 / bpm;
                seg_tick = e.tick;
                bpm = e.bpm;
            }
            elapsed.push(acc);
        }
        self.elapsed = elapsed;
    }

    /// Samples per quarter-note minute (the unit of `elapsed`) at `sr`.
    fn samples_per_elapsed(sr: SampleRate) -> f64 {
        sr.as_f64() * 60.0 / TICKS_PER_QUARTER as f64
    }

    /// The tempo segment `i` (an index into `tempos`, 0 when it is empty): its start tick (0 for
    /// the first), bpm and elapsed time at its start.
    fn segment(&self, i: usize) -> (i64, f64, f64) {
        let bpm = self.tempos.get(i).or(self.tempos.first()).map_or(120.0, |e| e.bpm);
        let tick = if i == 0 { 0 } else { self.tempos.get(i).map_or(0, |e| e.tick) };
        (tick, bpm, self.elapsed.get(i).copied().unwrap_or(0.0))
    }

    pub fn new(bpm: f64, numerator: u32, denominator: u32) -> Result<Self, TimeError> {
        let mut m = TempoMap::default();
        m.set_tempo(0, bpm)?;
        m.set_meter(0, numerator, denominator)?;
        Ok(m)
    }

    pub fn tempos(&self) -> &[TempoEvent] {
        &self.tempos
    }
    pub fn meters(&self) -> &[MeterEvent] {
        &self.meters
    }

    /// Repair a map read from a file: only valid tempos and meters within `0..=MAX_POSITION` ticks, in
    /// tick order, with one of each at tick 0.
    pub fn sanitize(&mut self) {
        self.tempos.retain(|e| (0..=MAX_POSITION).contains(&e.tick) && valid_bpm(e.bpm).is_ok());
        self.tempos.sort_by_key(|e| e.tick);
        self.tempos.dedup_by_key(|e| e.tick);
        if self.tempos.first().is_none_or(|e| e.tick > 0) {
            let bpm = self.tempos.first().map_or(120.0, |e| e.bpm);
            self.tempos.insert(0, TempoEvent { tick: 0, bpm });
        }
        self.meters.retain(|m| (0..=MAX_POSITION).contains(&m.tick) && valid_meter(m.numerator, m.denominator));
        self.meters.sort_by_key(|m| m.tick);
        self.meters.dedup_by_key(|m| m.tick);
        if self.meters.first().is_none_or(|m| m.tick > 0) {
            let (numerator, denominator) = self.meters.first().map_or((4, 4), |m| (m.numerator, m.denominator));
            self.meters.insert(0, MeterEvent { tick: 0, numerator, denominator });
        }
        self.reindex();
    }

    /// Insert or replace the tempo at `tick`.
    pub fn set_tempo(&mut self, tick: i64, bpm: f64) -> Result<(), TimeError> {
        let bpm = valid_bpm(bpm)?;
        let tick = tick.max(0);
        match self.tempos.binary_search_by(|e| e.tick.cmp(&tick)) {
            Ok(i) => {
                if let Some(e) = self.tempos.get_mut(i) {
                    e.bpm = bpm;
                }
            }
            Err(i) => self.tempos.insert(i, TempoEvent { tick, bpm }),
        }
        self.reindex();
        Ok(())
    }

    /// Remove a tempo event (the one at tick 0 is kept).
    pub fn remove_tempo(&mut self, tick: i64) -> bool {
        if tick == 0 {
            return false;
        }
        let before = self.tempos.len();
        self.tempos.retain(|e| e.tick != tick);
        self.reindex();
        before != self.tempos.len()
    }

    /// Insert or replace the meter at `tick` (snapped to the bar line at or before it).
    /// Later meter changes keep their bar numbers, so they stay on bar lines.
    pub fn set_meter(&mut self, tick: i64, numerator: u32, denominator: u32) -> Result<(), TimeError> {
        if !valid_meter(numerator, denominator) {
            return Err(TimeError::Meter(numerator, denominator));
        }
        let tick = if tick <= 0 { 0 } else { self.bar_start_tick(self.bar_beat_at_tick(tick).bar) };
        let later = self.take_meters_after(tick);
        self.put_meter(MeterEvent { tick, numerator, denominator });
        self.restore_meters_at_bars(later);
        Ok(())
    }

    /// Remove a meter event (the one at tick 0 is kept). Later meter changes keep their bar numbers.
    pub fn remove_meter(&mut self, tick: i64) -> bool {
        if tick == 0 || !self.meters.iter().any(|e| e.tick == tick) {
            return false;
        }
        let later = self.take_meters_after(tick);
        self.meters.retain(|e| e.tick != tick);
        self.restore_meters_at_bars(later);
        true
    }

    fn put_meter(&mut self, ev: MeterEvent) {
        match self.meters.binary_search_by(|e| e.tick.cmp(&ev.tick)) {
            Ok(i) => {
                if let Some(e) = self.meters.get_mut(i) {
                    *e = ev;
                }
            }
            Err(i) => self.meters.insert(i, ev),
        }
    }

    /// Remove the meters after `tick`, returning each with the bar it starts: the bar
    /// [`Self::bar_beat_at_tick`] gives its tick, counted in one pass over the map rather than one
    /// walk per meter, so a session with many meter changes edits in linear time.
    fn take_meters_after(&mut self, tick: i64) -> Vec<(i64, MeterEvent)> {
        let mut later = Vec::new();
        let (mut bar, mut seg_tick) = (1i64, 0i64);
        for (i, m) in self.meters.iter().enumerate() {
            let tpb = m.ticks_per_bar();
            if m.tick > tick {
                later.push((bar.saturating_add(m.tick.saturating_sub(seg_tick) / tpb), *m));
            }
            if let Some(n) = self.meters.get(i + 1) {
                bar = bar.saturating_add(ceil_div(n.tick.saturating_sub(seg_tick), tpb));
                seg_tick = n.tick;
            }
        }
        self.meters.retain(|e| e.tick <= tick);
        later
    }

    /// The bar and tick of the last meter's segment, where [`Self::bar_start_tick`]'s walk ends.
    fn last_segment(&self) -> (i64, i64) {
        let (mut cur_bar, mut cur_tick) = (1i64, 0i64);
        for (i, m) in self.meters.iter().enumerate() {
            if let Some(n) = self.meters.get(i + 1) {
                cur_bar = cur_bar.saturating_add(ceil_div(n.tick.saturating_sub(cur_tick), m.ticks_per_bar()));
                cur_tick = n.tick;
            }
        }
        (cur_bar, cur_tick)
    }

    /// Put meters back at the start of their bars under the current earlier meters, in bar order.
    /// Each lands at or past the last meter, so the walk to its bar resumes from there; a bar
    /// before it (meters off their bar lines) takes the full walk.
    fn restore_meters_at_bars(&mut self, later: Vec<(i64, MeterEvent)>) {
        let (mut cur_bar, mut cur_tick) = self.last_segment();
        for (bar, ev) in later {
            let tick = if bar >= cur_bar {
                let tpb = self.meters.last().map_or(MeterEvent { tick: 0, numerator: 4, denominator: 4 }, |m| *m).ticks_per_bar();
                cur_tick.saturating_add(bar.saturating_sub(cur_bar).saturating_mul(tpb))
            } else {
                self.bar_start_tick(bar)
            };
            self.put_meter(MeterEvent { tick, ..ev });
            (cur_bar, cur_tick) = if bar >= cur_bar { (bar, tick) } else { self.last_segment() };
        }
    }

    /// Tempo in effect at `tick`.
    pub fn tempo_at_tick(&self, tick: i64) -> f64 {
        let mut bpm = self.tempos.first().map_or(120.0, |e| e.bpm);
        for e in &self.tempos {
            if e.tick <= tick {
                bpm = e.bpm;
            } else {
                break;
            }
        }
        bpm
    }

    /// Meter in effect at `tick`.
    pub fn meter_at_tick(&self, tick: i64) -> MeterEvent {
        let mut m = self.meters.first().copied().unwrap_or(MeterEvent { tick: 0, numerator: 4, denominator: 4 });
        for e in &self.meters {
            if e.tick <= tick {
                m = *e;
            } else {
                break;
            }
        }
        m
    }

    fn samples_per_tick(bpm: f64, sr: SampleRate) -> f64 {
        sr.as_f64() * 60.0 / (bpm * TICKS_PER_QUARTER as f64)
    }

    /// Ticks → samples (exact for constant-tempo segments).
    pub fn tick_to_samples(&self, tick: i64, sr: SampleRate) -> Samples {
        to_samples(self.tick_to_samples_f(tick, sr))
    }

    fn tick_to_samples_f(&self, tick: i64, sr: SampleRate) -> f64 {
        // Tempo events after the first that start before `tick` (the segments it has passed).
        let passed = self.tempos.get(1..).map_or(0, |rest| rest.partition_point(|e| e.tick < tick));
        let (seg_tick, bpm, elapsed) = self.segment(passed);
        elapsed * Self::samples_per_elapsed(sr) + (tick - seg_tick) as f64 * Self::samples_per_tick(bpm, sr)
    }

    /// Samples → ticks (fractional).
    pub fn samples_to_ticks_f(&self, s: Samples, sr: SampleRate) -> f64 {
        let s = s as f64;
        let scale = Self::samples_per_elapsed(sr);
        // Tempo events after the first that start at or before `s`.
        let passed = self.elapsed.get(1..).map_or(0, |rest| rest.partition_point(|e| e * scale <= s));
        let (seg_tick, bpm, elapsed) = self.segment(passed);
        seg_tick as f64 + (s - elapsed * scale) / Self::samples_per_tick(bpm, sr)
    }

    pub fn samples_to_ticks(&self, s: Samples, sr: SampleRate) -> i64 {
        to_samples(self.samples_to_ticks_f(s, sr))
    }

    /// Tick of the first beat of `bar` (1-based; bars before 1 extend the first meter backwards).
    pub fn bar_start_tick(&self, bar: i64) -> i64 {
        let mut cur_bar = 1i64;
        let mut cur_tick = 0i64;
        let first = self.meter_at_tick(0);
        if bar < 1 {
            return bar.saturating_sub(1).saturating_mul(first.ticks_per_bar());
        }
        for (i, m) in self.meters.iter().enumerate() {
            let next_tick = self.meters.get(i + 1).map(|n| n.tick);
            let tpb = m.ticks_per_bar();
            let bars_in_seg = next_tick.map(|nt| ceil_div(nt.saturating_sub(cur_tick), tpb));
            match bars_in_seg {
                Some(n) if bar >= cur_bar.saturating_add(n) => {
                    cur_bar = cur_bar.saturating_add(n);
                    cur_tick = next_tick.unwrap_or(cur_tick);
                }
                _ => return cur_tick.saturating_add((bar - cur_bar).saturating_mul(tpb)),
            }
        }
        cur_tick
    }

    /// Bar|beat|tick at `tick`.
    pub fn bar_beat_at_tick(&self, tick: i64) -> BarBeat {
        let first = self.meter_at_tick(0);
        if tick < 0 {
            let tpb = first.ticks_per_bar();
            let abs = tick.unsigned_abs();
            let tpb_u = tpb.unsigned_abs();
            let bars_back_u = abs / tpb_u + u64::from(!abs.is_multiple_of(tpb_u));
            let bars_back = i64::try_from(bars_back_u).unwrap_or(i64::MAX);
            // Offset of `tick` inside its bar, without negating `i64::MIN`.
            let within = ((tpb_u - abs % tpb_u) % tpb_u) as i64;
            let beat_t = first.ticks_per_beat();
            return BarBeat { bar: 1 - bars_back, beat: within / beat_t + 1, tick: within % beat_t };
        }
        let mut bar = 1i64;
        let mut seg_tick = 0i64;
        for (i, m) in self.meters.iter().enumerate() {
            let tpb = m.ticks_per_bar();
            if let Some(n) = self.meters.get(i + 1)
                && tick >= n.tick
            {
                bar = bar.saturating_add(ceil_div(n.tick.saturating_sub(seg_tick), tpb));
                seg_tick = n.tick;
                continue;
            }
            let rel = tick - seg_tick;
            let bars = rel / tpb;
            let within = rel % tpb;
            let beat_t = m.ticks_per_beat();
            return BarBeat { bar: bar + bars, beat: within / beat_t + 1, tick: within % beat_t };
        }
        BarBeat { bar, beat: 1, tick: 0 }
    }

    /// Bar|beat|tick → tick.
    pub fn tick_at_bar_beat(&self, bb: BarBeat) -> i64 {
        let start = self.bar_start_tick(bb.bar);
        let m = self.meter_at_tick(start);
        start.saturating_add(bb.beat.saturating_sub(1).max(0).saturating_mul(m.ticks_per_beat())).saturating_add(bb.tick)
    }

    pub fn bar_beat_at(&self, s: Samples, sr: SampleRate) -> BarBeat {
        self.bar_beat_at_tick(self.samples_to_ticks(s, sr))
    }

    pub fn samples_at_bar_beat(&self, bb: BarBeat, sr: SampleRate) -> Samples {
        self.tick_to_samples(self.tick_at_bar_beat(bb), sr)
    }

    /// Scale every tempo by `factor` (Tempo Operations › Scale).
    pub fn scale_tempos(&mut self, factor: f64) -> Result<(), TimeError> {
        let mut out = self.tempos.clone();
        for e in &mut out {
            e.bpm = valid_bpm(e.bpm * factor)?;
        }
        self.tempos = out;
        self.reindex();
        Ok(())
    }
}

/// Ceiling division for a non-negative `num` and positive `den`, without overflow.
fn ceil_div(num: i64, den: i64) -> i64 {
    num / den + i64::from(num % den != 0)
}


