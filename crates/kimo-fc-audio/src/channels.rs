// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Channel layouts, format conversion (up/downmix) and the 5.1 surround panner.
//!
//! **Channel order.** 5.1 audio is always in the SMPTE / WAVE order **L, R, C, LFE, Ls, Rs**
//! ([`L`], [`R`], [`C`], [`LFE`], [`LS`], [`RS`]); stereo is L, R; mono is one channel. Codecs with a
//! different native order (AAC: C, L, R, Ls, Rs, LFE) reorder at the codec boundary.
//!
//! **Downmix** follows ITU-R BS.775 (Table 2, "3/2 source" rows), with `k = 1/√2` (−3 dB):
//!
//! | from → to | equations |
//! |---|---|
//! | 5.1 → 2.0 | `Lo = L + k·C + k·Ls`, `Ro = R + k·C + k·Rs` (LFE omitted) |
//! | 5.1 → 1.0 | `M = k·L + k·R + C + ½·Ls + ½·Rs` |
//! | 2.0 → 1.0 | `M = k·L + k·R` |
//!
//! The stereo downmix used for playback on stereo devices can be changed by the "5.1 Mixdown Type"
//! preference ([`Mixdown`]): Front Only drops the surrounds, the `+ LFE` variants add `k·LFE` to
//! both sides.
//!
//! **Upmix** places the source on the front speakers without inventing content: mono → C (5.1) or
//! both sides at unity (2.0, as mono clips are played everywhere else in FilmCraft); stereo → L, R.
//!
//! **5.1 panner** ([`Pan51`]): a puck at `(x, y)` in the listening square (x −1 left … +1 right,
//! y −1 rear … +1 front). A point source at `(x, y)` is split front/rear and left/right with the
//! sine/cosine constant-power law, and the front image is shared between the phantom L/R pair and
//! the centre speaker by **Center %** (`k_c = center · (1 − |x|)` of the front power goes to C). The
//! gains of a point source always have unit power (`Σ g² = 1`). Multichannel sources keep their
//! layout around the puck: each source channel sits at its nominal speaker position moved by the
//! puck's offset from front-centre (so the default puck (0, 1) at 100 % centre maps 5.1 to itself
//! exactly). Stereo sources sit at the puck ± 1 in x. The LFE output is the source LFE (5.1) or
//! the mean of the source channels (mono/stereo) times the **LFE** gain.

use std::f32::consts::{FRAC_1_SQRT_2, FRAC_PI_2, FRAC_PI_4};

/// −3 dB, the BS.775 downmix coefficient.
pub const K: f32 = FRAC_1_SQRT_2;

/// 5.1 channel indices (SMPTE / WAVE order).
pub const L: usize = 0;
pub const R: usize = 1;
pub const C: usize = 2;
pub const LFE: usize = 3;
pub const LS: usize = 4;
pub const RS: usize = 5;

/// Short names of the 5.1 channels in order.
pub const NAMES_51: [&str; 6] = ["L", "R", "C", "LFE", "Ls", "Rs"];

/// A channel layout.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Layout {
    Mono,
    #[default]
    Stereo,
    /// 5.1 in L, R, C, LFE, Ls, Rs order.
    Surround51,
}

impl Layout {
    pub fn channels(self) -> usize {
        match self {
            Layout::Mono => 1,
            Layout::Stereo => 2,
            Layout::Surround51 => 6,
        }
    }
    /// The layout of a plain channel count (1, 2 or 6; anything else is treated as stereo).
    pub fn from_channels(n: usize) -> Layout {
        match n {
            1 => Layout::Mono,
            6 => Layout::Surround51,
            _ => Layout::Stereo,
        }
    }
    /// Channel names for display ("L", "R" …).
    pub fn names(self) -> &'static [&'static str] {
        match self {
            Layout::Mono => &["M"],
            Layout::Stereo => &["L", "R"],
            Layout::Surround51 => &NAMES_51,
        }
    }
}

/// Premiere's "5.1 Mixdown Type" (Preferences ▸ Audio): how 5.1 is folded to stereo for playback.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Mixdown {
    /// L, R and C only.
    Front,
    /// L, R, C and the surrounds (ITU-R BS.775).
    #[default]
    FrontRear,
    /// L, R, C and LFE.
    FrontLfe,
    /// Everything.
    FrontRearLfe,
}

impl Mixdown {
    pub const ALL: [Mixdown; 4] = [Mixdown::Front, Mixdown::FrontRear, Mixdown::FrontLfe, Mixdown::FrontRearLfe];
    /// Preference value (`front`, `frontRear`, `frontLfe`, `frontRearLfe`).
    pub fn id(self) -> &'static str {
        match self {
            Mixdown::Front => "front",
            Mixdown::FrontRear => "frontRear",
            Mixdown::FrontLfe => "frontLfe",
            Mixdown::FrontRearLfe => "frontRearLfe",
        }
    }
    pub fn from_id(s: &str) -> Option<Mixdown> {
        Self::ALL.into_iter().find(|m| m.id().eq_ignore_ascii_case(s))
    }
    fn rear(self) -> bool {
        matches!(self, Mixdown::FrontRear | Mixdown::FrontRearLfe)
    }
    fn lfe(self) -> bool {
        matches!(self, Mixdown::FrontLfe | Mixdown::FrontRearLfe)
    }
}

/// Conversion matrix `m[out][in]` from `from` to `to`. Stereo downmixes of 5.1 use `mixdown`
/// ([`Mixdown::FrontRear`] is BS.775).
pub fn matrix(from: Layout, to: Layout, mixdown: Mixdown) -> Vec<Vec<f32>> {
    let (ni, no) = (from.channels(), to.channels());
    let mut m = vec![vec![0.0f32; ni]; no];
    match (from, to) {
        (Layout::Mono, Layout::Mono) | (Layout::Stereo, Layout::Stereo) | (Layout::Surround51, Layout::Surround51) => {
            for (i, row) in m.iter_mut().enumerate() {
                row[i] = 1.0;
            }
        }
        (Layout::Mono, Layout::Stereo) => {
            m[0][0] = 1.0;
            m[1][0] = 1.0;
        }
        (Layout::Mono, Layout::Surround51) => m[C][0] = 1.0,
        (Layout::Stereo, Layout::Mono) => m[0] = vec![K, K],
        (Layout::Stereo, Layout::Surround51) => {
            m[L][0] = 1.0;
            m[R][1] = 1.0;
        }
        (Layout::Surround51, Layout::Mono) => {
            m[0][L] = K;
            m[0][R] = K;
            m[0][C] = 1.0;
            m[0][LS] = 0.5;
            m[0][RS] = 0.5;
        }
        (Layout::Surround51, Layout::Stereo) => {
            m[0][L] = 1.0;
            m[1][R] = 1.0;
            m[0][C] = K;
            m[1][C] = K;
            if mixdown.rear() {
                m[0][LS] = K;
                m[1][RS] = K;
            }
            if mixdown.lfe() {
                m[0][LFE] = K;
                m[1][LFE] = K;
            }
        }
    }
    m
}

/// Apply a matrix `m[out][in]` to planar `input` (all channels the same length).
pub fn apply_matrix(m: &[Vec<f32>], input: &[&[f32]]) -> Vec<Vec<f32>> {
    let n = input.first().map_or(0, |c| c.len());
    m.iter()
        .map(|row| {
            let mut out = vec![0.0f32; n];
            for (g, x) in row.iter().zip(input) {
                if *g == 0.0 {
                    continue;
                }
                for (o, s) in out.iter_mut().zip(x.iter()) {
                    *o += g * s;
                }
            }
            out
        })
        .collect()
}

/// Convert planar audio from one layout to another ([`matrix`]).
pub fn convert(input: &[&[f32]], from: Layout, to: Layout, mixdown: Mixdown) -> Vec<Vec<f32>> {
    if from == to {
        return input.iter().map(|c| c.to_vec()).collect();
    }
    apply_matrix(&matrix(from, to, mixdown), input)
}

/// Convert owned planar audio whose layout is given by its channel count.
pub fn convert_owned(input: Vec<Vec<f32>>, to: Layout, mixdown: Mixdown) -> Vec<Vec<f32>> {
    let from = Layout::from_channels(input.len());
    if from == to && input.len() == to.channels() {
        return input;
    }
    let refs: Vec<&[f32]> = input.iter().map(Vec::as_slice).collect();
    if input.len() != from.channels() {
        // unusual channel counts: take the first channels that fit, pad with silence
        let n = input.first().map_or(0, Vec::len);
        let mut v: Vec<Vec<f32>> = input.iter().take(to.channels()).cloned().collect();
        v.resize(to.channels(), vec![0.0; n]);
        return v;
    }
    convert(&refs, from, to, mixdown)
}

// ------------------------------------------------------------------------------------- 5.1 panner

/// The 5.1 panner of a track or send feeding a 5.1 bus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pan51 {
    /// −1 (left) … +1 (right).
    pub x: f32,
    /// −1 (rear) … +1 (front).
    pub y: f32,
    /// Center %: 0 … 1, the share of the front image the centre speaker takes at x = 0.
    pub center: f32,
    /// LFE gain (linear).
    pub lfe: f32,
}

impl Default for Pan51 {
    /// Front centre, 100 % centre, LFE at unity.
    fn default() -> Self {
        Pan51 { x: 0.0, y: 1.0, center: 1.0, lfe: 1.0 }
    }
}

/// Gains of a point source at `(x, y)` to the six 5.1 outputs (LFE is 0). `Σ g² = 1`.
pub fn point_gains(x: f32, y: f32, center: f32) -> [f32; 6] {
    let x = x.clamp(-1.0, 1.0);
    let y = y.clamp(-1.0, 1.0);
    let fb = (1.0 - y) * 0.5 * FRAC_PI_2;
    let (f, b) = (fb.cos(), fb.sin());
    let lr = (x + 1.0) * FRAC_PI_4;
    let (gl, gr) = (lr.cos(), lr.sin());
    let kc = (center.clamp(0.0, 1.0) * (1.0 - x.abs())).clamp(0.0, 1.0);
    let ph = (1.0 - kc).sqrt();
    let mut g = [0.0f32; 6];
    g[L] = f * ph * gl;
    g[R] = f * ph * gr;
    g[C] = f * kc.sqrt();
    g[LS] = b * gl;
    g[RS] = b * gr;
    // f32 sin/cos at the quadrant ends are not exactly 0 / 1: snap them so speaker positions
    // route bit-exactly (a 5.1 source through the default panner is the identity)
    for v in g.iter_mut() {
        if v.abs() < 1e-6 {
            *v = 0.0;
        } else if (*v - 1.0).abs() < 1e-6 {
            *v = 1.0;
        }
    }
    g
}

/// Nominal source-channel positions relative to the puck's offset from front-centre.
fn offsets(from: Layout) -> &'static [(f32, f32)] {
    match from {
        Layout::Mono => &[(0.0, 0.0)],
        Layout::Stereo => &[(-1.0, 0.0), (1.0, 0.0)],
        // L, R, C, LFE (not positioned), Ls, Rs
        Layout::Surround51 => &[(-1.0, 0.0), (1.0, 0.0), (0.0, 0.0), (0.0, 0.0), (-1.0, -2.0), (1.0, -2.0)],
    }
}

/// Panning matrix `m[out][in]` (6 outputs) of a `from` source through the 5.1 panner.
pub fn pan51_matrix(from: Layout, p: Pan51) -> Vec<Vec<f32>> {
    let ni = from.channels();
    let mut m = vec![vec![0.0f32; ni]; 6];
    for (i, (ox, oy)) in offsets(from).iter().enumerate() {
        if from == Layout::Surround51 && i == LFE {
            m[LFE][i] = p.lfe;
            continue;
        }
        let g = point_gains(p.x + ox, p.y + oy, p.center);
        for (o, row) in m.iter_mut().enumerate() {
            row[i] = g[o];
        }
        if from != Layout::Surround51 {
            m[LFE][i] = p.lfe / ni as f32;
        }
    }
    m
}

/// Peak level (linear) per channel.
pub fn peaks(input: &[&[f32]]) -> Vec<f32> {
    input.iter().map(|c| c.iter().fold(0f32, |a, x| a.max(x.abs()))).collect()
}


