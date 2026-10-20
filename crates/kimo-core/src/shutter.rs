//! Opt-in exact shutter sampling for layer transform motion blur (set_motion_blur samples / shutter_phase).
//! A clip with neither prop keeps the legacy fixed-count centred sampler, byte for byte.

/// Sample count for a shutter interval whose fastest point travels `travel_px` pixels.
/// Midpoint error on a box-filtered edge is at most S/(4n^2), so n = 16*sqrt(S) keeps it near 1e-3;
/// n >= S keeps the spacing between copies <= 1 px (no visible stepping). Clamped 2..256.
pub fn auto_samples(travel_px: f64) -> usize { if !travel_px.is_finite() { return 256; } if travel_px <= 0.0 { return 1; } ((16.0 * travel_px.sqrt()).ceil().max(travel_px.ceil()) as usize).clamp(2, 256) }

/// n midpoint sample times across [open, open + len].
pub fn midpoints(open: f64, len: f64, n: usize) -> Vec<f64> { (0..n).map(|k| open + (k as f64 + 0.5) / n as f64 * len).collect() }

/// Shutter window relative to the clip-local time: opens at lt + open, lasts len seconds; samples 0 = auto from travel.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayerShutter { pub open: f64, pub len: f64, pub samples: usize }

/// The opt-in exact shutter for a clip, or None (legacy sampler) unless motion blur is on and mb_samples or mb_phase is set.
pub fn layer_shutter(c: &crate::model::Clip, fps: f64, lt: f64) -> Option<LayerShutter> {
    if !(c.motion_blur > 0.0) || !(c.props.contains_key("mb_samples") || c.props.contains_key("mb_phase")) { return None; }
    let fps = fps.max(1.0); let len = c.motion_blur / 360.0 / fps; let open = match c.props.get("mb_phase") { Some(p) => p.at(lt) / 360.0 / fps, None => -len / 2.0 };
    let samples = c.props.get("mb_samples").map(|p| p.at(lt)).filter(|v| v.is_finite() && *v > 0.0).map(|v| v.round() as usize).unwrap_or(0);
    Some(LayerShutter { open, len, samples })
}
