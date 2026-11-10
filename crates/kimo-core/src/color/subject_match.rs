//! Opt-in matte-guided shot matching, display-sRGB / D65, straight RGBA only.
//! This module is deliberately not wired into the renderer or MCP registry.
//! The caller supplies semantic mattes; this is not automatic skin detection.
//! Skin, background and caller-designated lighting regions are analyzed and
//! matched independently. Log-luminance controls exposure/contrast; a/b Lab
//! covariance controls chroma, so color transport cannot change exposure.
use crate::{color, effects, guard};
use color::{managed, matching};
use effects::Frame;

pub const SKIN: usize = 0;
pub const BACKGROUND: usize = 1;
pub const LIGHTING: usize = 2;
const FLOOR: f64 = 1e-6;

/// Three soft semantic weights per pixel, in skin/background/lighting order.
/// Each weight must be finite in 0..1, with sum <= 1. Unassigned weight is
/// preserved. A lighting matte means a user-selected lighting sample region,
/// not estimated physical illumination. Different shots may have different sizes.
pub struct Regions<'a> {
    pub weights: &'a [[f32; 3]],
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ExposureTarget {
    /// Match reference geometric mean linear luminance.
    Reference,
    /// Preserve source geometric mean, while matching reference contrast/chroma.
    PreserveSource,
    /// Desired geometric mean linear luminance in 1e-6..1, not encoded sRGB.
    LinearLuminance(f64),
    /// Reference geometric mean, offset by -8..8 exposure stops.
    ReferenceOffsetStops(f64),
}

#[derive(Clone, Copy, Debug)]
pub struct Options {
    /// Global blend. Zero is a bit-exact no-op after preflight.
    pub amount: f64,
    /// Independent skin/background/lighting blends in 0..1.
    pub region_amount: [f64; 3],
    pub exposure_target: [ExposureTarget; 3],
    /// Chroma covariance diagonal regularizer, 1e-8..0.1.
    pub regularization: f64,
    /// Log-luminance contrast blend in 0..1. Zero matches exposure only.
    pub contrast_amount: f64,
    /// Maximum absolute change in log luminance per pixel, 0..8 stops.
    pub max_exposure_stops: f64,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            amount: 1.,
            region_amount: [1.; 3],
            exposure_target: [ExposureTarget::Reference; 3],
            regularization: 1e-4,
            contrast_amount: 1.,
            max_exposure_stops: 4.,
        }
    }
}

/// Population statistics, weighted by semantic matte * straight alpha.
/// Black uses a 1e-6 linear-luminance floor for finite log statistics.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionStats {
    pub contributing_pixels: usize,
    pub weight: f64,
    pub effective_samples: f64,
    pub lab_mean: [f64; 3],
    pub lab_covariance: [[f64; 3]; 3],
    pub log_luminance_mean: f64,
    pub log_luminance_variance: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ShotStats {
    pub regions: [Option<RegionStats>; 3],
}
#[derive(Clone, Debug)]
struct RegionTransform {
    source_chroma: [f64; 2],
    target_chroma: [f64; 2],
    chroma_matrix: [[f64; 3]; 3],
    source_log: f64,
    target_log: f64,
    log_scale: f64,
    identity: bool,
}
#[derive(Clone, Debug)]
pub struct MatchPlan {
    pub source: ShotStats,
    pub reference: ShotStats,
    options: Options,
    transforms: [Option<RegionTransform>; 3],
}

fn options_valid(o: &Options) -> Result<(), String> {
    for v in [o.amount, o.contrast_amount]
        .into_iter()
        .chain(o.region_amount)
    {
        if !v.is_finite() || !(0. ..=1.).contains(&v) {
            return Err("match amounts must be finite in 0..1".into());
        }
    }
    if !o.regularization.is_finite()
        || !(1e-8..=0.1).contains(&o.regularization)
        || !o.max_exposure_stops.is_finite()
        || !(0. ..=8.).contains(&o.max_exposure_stops)
    {
        return Err("invalid regularization or exposure safety bound".into());
    }
    for t in o.exposure_target {
        match t {
            ExposureTarget::LinearLuminance(v) if !v.is_finite() || !(FLOOR..=1.).contains(&v) => {
                return Err("creative luminance target must be finite in 1e-6..1".into())
            }
            ExposureTarget::ReferenceOffsetStops(v)
                if !v.is_finite() || !(-8. ..=8.).contains(&v) =>
            {
                return Err("creative stop offset must be finite in -8..8".into())
            }
            _ => (),
        }
    }
    Ok(())
}
fn validate(f: &Frame, regions: &Regions<'_>) -> Result<(), String> {
    guard::check();
    if f.w == 0
        || f.h == 0
        || f.w.checked_mul(f.h).and_then(|n| n.checked_mul(4)) != Some(f.d.len())
        || regions.weights.len() != f.d.len() / 4
    {
        return Err(
            "subject match requires nonempty RGBA frame and one region weight triple per pixel"
                .into(),
        );
    }
    for (i, (px, weights)) in f.d.chunks_exact(4).zip(regions.weights).enumerate() {
        if i % 65536 == 0 {
            guard::check();
        }
        if px.iter().any(|v| !v.is_finite() || !(0. ..=1.).contains(v)) {
            return Err(
                "subject match requires finite display-sRGB and straight alpha in 0..1".into(),
            );
        }
        if weights
            .iter()
            .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            || weights.iter().map(|v| *v as f64).sum::<f64>() > 1. + 1e-7
        {
            return Err("semantic weights must be finite in 0..1 and sum to at most 1".into());
        }
    }
    Ok(())
}
fn luminance(rgb: [f64; 3]) -> f64 {
    let l = rgb.map(|v| managed::decode(1, v, 100.));
    0.2126390058715104 * l[0] + 0.715168678767756 * l[1] + 0.0721923153607337 * l[2]
}
fn compress(c: [f64; 3]) -> [f64; 3] {
    let y = (0.2126390058715104 * c[0] + 0.715168678767756 * c[1] + 0.0721923153607337 * c[2])
        .clamp(0., 1.);
    let mut scale: f64 = 1.;
    for v in c {
        let d = v - y;
        if d > 0. {
            scale = scale.min((1. - y) / d);
        } else if d < 0. {
            scale = scale.min(-y / d);
        }
    }
    c.map(|v| (y + (v - y) * scale.clamp(0., 1.)).clamp(0., 1.))
}
fn lightness(y: f64) -> f64 {
    let fy = if y > 216. / 24389. {
        y.cbrt()
    } else {
        (24389. / 27. * y + 16.) / 116.
    };
    (116. * fy - 16.) / 100.
}
#[derive(Default)]
struct Accumulator {
    n: usize,
    weight: f64,
    squared_weight: f64,
    mean: [f64; 4],
    m2: [[f64; 4]; 4],
}
impl Accumulator {
    fn add(&mut self, x: [f64; 4], w: f64) {
        if w == 0. {
            return;
        }
        self.n += 1;
        let total = self.weight + w;
        let delta: [f64; 4] = std::array::from_fn(|j| x[j] - self.mean[j]);
        for j in 0..4 {
            self.mean[j] += delta[j] * w / total;
        }
        for j in 0..4 {
            for k in 0..4 {
                self.m2[j][k] += w * delta[j] * (x[k] - self.mean[k]);
            }
        }
        self.weight = total;
        self.squared_weight += w * w;
    }
    fn finish(self) -> Option<RegionStats> {
        if self.weight == 0. {
            return None;
        }
        Some(RegionStats {
            contributing_pixels: self.n,
            weight: self.weight,
            effective_samples: self.weight * self.weight / self.squared_weight,
            lab_mean: [self.mean[0], self.mean[1], self.mean[2]],
            lab_covariance: std::array::from_fn(|j| {
                std::array::from_fn(|k| (self.m2[j][k] + self.m2[k][j]) / (2. * self.weight))
            }),
            log_luminance_mean: self.mean[3],
            log_luminance_variance: (self.m2[3][3] / self.weight).max(0.),
        })
    }
}
/// No subsampling: deterministic row-major alpha-weighted Welford statistics.
pub fn statistics(f: &Frame, regions: &Regions<'_>) -> Result<ShotStats, String> {
    validate(f, regions)?;
    let mut accumulators: [Accumulator; 3] = std::array::from_fn(|_| Accumulator::default());
    for (i, (px, weights)) in f.d.chunks_exact(4).zip(regions.weights).enumerate() {
        if i % 65536 == 0 {
            guard::check();
        }
        if px[3] == 0. || weights.iter().all(|v| *v == 0.) {
            continue;
        }
        let rgb = [px[0] as f64, px[1] as f64, px[2] as f64];
        let lab = matching::to_lab(rgb);
        let x = [lab[0], lab[1], lab[2], luminance(rgb).max(FLOOR).log2()];
        for r in 0..3 {
            accumulators[r].add(x, weights[r] as f64 * px[3] as f64);
        }
    }
    Ok(ShotStats {
        regions: accumulators.map(Accumulator::finish),
    })
}
fn chroma_stats(s: &RegionStats) -> matching::Stats {
    let mut cov = [[0.; 3]; 3];
    for j in 1..3 {
        for k in 1..3 {
            cov[j][k] = s.lab_covariance[j][k];
        }
    }
    matching::Stats {
        mean: [0., s.lab_mean[1], s.lab_mean[2]],
        cov,
        count: s.contributing_pixels,
    }
}
/// Fits a reusable plan. Enabled regions need >=4 effective samples in both
/// frames; absent/weak regions fail explicitly, rather than borrowing a global
/// transform. Set their region_amount to zero to deliberately leave them alone.
pub fn fit(
    source: &Frame,
    source_regions: &Regions<'_>,
    reference: &Frame,
    reference_regions: &Regions<'_>,
    options: Options,
) -> Result<MatchPlan, String> {
    options_valid(&options)?;
    let source = statistics(source, source_regions)?;
    let reference = statistics(reference, reference_regions)?;
    let mut transforms = [None, None, None];
    for r in 0..3 {
        if options.amount == 0. || options.region_amount[r] == 0. {
            continue;
        }
        let src = source.regions[r]
            .as_ref()
            .ok_or_else(|| format!("source region {r} is empty"))?;
        let dst = reference.regions[r]
            .as_ref()
            .ok_or_else(|| format!("reference region {r} is empty"))?;
        if src.effective_samples < 4. - 1e-10 || dst.effective_samples < 4. - 1e-10 {
            return Err(format!(
                "region {r} needs at least four effective alpha-weighted samples"
            ));
        }
        let matrix = matching::transport(
            &chroma_stats(src),
            &chroma_stats(dst),
            options.regularization,
        )?;
        let target_log = match options.exposure_target[r] {
            ExposureTarget::Reference => dst.log_luminance_mean,
            ExposureTarget::PreserveSource => src.log_luminance_mean,
            ExposureTarget::LinearLuminance(v) => v.log2(),
            ExposureTarget::ReferenceOffsetStops(v) => dst.log_luminance_mean + v,
        };
        // Bounded contrast for weak/flat data, avoiding noise explosions.
        let raw_scale = ((dst.log_luminance_variance + 1e-4) / (src.log_luminance_variance + 1e-4))
            .sqrt()
            .clamp(0.25, 4.);
        transforms[r] = Some(RegionTransform {
            source_chroma: [src.lab_mean[1], src.lab_mean[2]],
            target_chroma: [dst.lab_mean[1], dst.lab_mean[2]],
            chroma_matrix: matrix,
            source_log: src.log_luminance_mean,
            target_log,
            log_scale: 1. + (raw_scale - 1.) * options.contrast_amount,
            identity: src == dst && target_log == src.log_luminance_mean,
        });
    }
    Ok(MatchPlan {
        source,
        reference,
        options,
        transforms,
    })
}
impl MatchPlan {
    /// Applies in-place after complete preflight. Transparent pixels, alpha,
    /// unassigned pixels, disabled regions and zero amount are bit-preserved.
    /// Soft masks blend *deltas* (including unchanged remainder), not transforms.
    /// Input mattes must track the shot; the plan does not infer/travel with skin.
    pub fn apply(&self, f: &mut Frame, regions: &Regions<'_>) -> Result<(), String> {
        validate(f, regions)?;
        options_valid(&self.options)?;
        if self.options.amount == 0. {
            return Ok(());
        }
        for (i, (px, weights)) in f.d.chunks_exact_mut(4).zip(regions.weights).enumerate() {
            if i % 65536 == 0 {
                guard::check();
            }
            if px[3] == 0. {
                continue;
            }
            let strength: [f64; 3] = std::array::from_fn(|r| {
                weights[r] as f64 * self.options.region_amount[r] * self.options.amount
            });
            if strength.iter().all(|s| *s == 0.) {
                continue;
            }
            let rgb = [px[0] as f64, px[1] as f64, px[2] as f64];
            let lab = matching::to_lab(rgb);
            let log_y = luminance(rgb).max(FLOOR).log2();
            let mut delta = [0.; 3];
            for r in 0..3 {
                let Some(t) = &self.transforms[r] else {
                    continue;
                };
                if strength[r] == 0. || t.identity {
                    continue;
                }
                let mapped = managed::mv(
                    t.chroma_matrix,
                    [0., lab[1] - t.source_chroma[0], lab[2] - t.source_chroma[1]],
                );
                let desired_log = t.target_log + (log_y - t.source_log) * t.log_scale;
                let change = (desired_log - log_y).clamp(
                    -self.options.max_exposure_stops,
                    self.options.max_exposure_stops,
                );
                // SDR destination: floor and ceiling are part of this API contract.
                // Preserve true zero instead of manufacturing light at the log floor.
                let y = (luminance(rgb) * change.exp2()).min(1.);
                let target = [
                    lightness(y),
                    mapped[1] + t.target_chroma[0],
                    mapped[2] + t.target_chroma[1],
                ];
                for j in 0..3 {
                    delta[j] += strength[r] * (target[j] - lab[j]);
                }
            }
            if delta.iter().all(|v| v.abs() < 1e-14) {
                continue;
            }
            let out = std::array::from_fn(|j| lab[j] + delta[j]);
            let matched = matching::from_lab(out).map(|v| managed::decode(1, v, 100.));
            // Exact D65 luminance-preserving chroma compression bounds SDR.
            let encoded = compress(matched).map(|v| managed::encode(1, v, 100.));
            for j in 0..3 {
                px[j] = encoded[j].clamp(0., 1.) as f32;
            }
        }
        Ok(())
    }
}
