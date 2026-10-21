//! Opt-in exact shutter sampling for layer transform motion blur (set_motion_blur samples / shutter_phase / shutter_curve).
//! A clip with none of these props keeps the legacy fixed-count centred sampler, byte for byte.

/// Sample count for a shutter interval whose fastest point travels `travel_px` pixels.
/// Midpoint error on a box-filtered edge is at most S/(4n^2), so n = 16*sqrt(S) keeps it near 1e-3;
/// n >= S keeps the spacing between copies <= 1 px (no visible stepping). Clamped 2..256.
pub fn auto_samples(travel_px: f64) -> usize { if !travel_px.is_finite() { return 256; } if travel_px <= 0.0 { return 1; } ((16.0 * travel_px.sqrt()).ceil().max(travel_px.ceil()) as usize).clamp(2, 256) }

/// n midpoint sample times across [open, open + len].
pub fn midpoints(open: f64, len: f64, n: usize) -> Vec<f64> { (0..n).map(|k| open + (k as f64 + 0.5) / n as f64 * len).collect() }

/// Shutter efficiency curve (Nuke / RenderMan / Arnold): relative exposure over the normalized shutter position s (0 = open, 1 = close).
#[derive(Clone, Debug, PartialEq)]
pub enum Curve { Box, Triangle, Gaussian, Custom(Vec<(f64, f64)>) }
impl Curve {
    /// Weight at s in [0,1]: box 1, triangle peaking mid-shutter, gaussian sigma = 1/6 shutter (+-3 sigma spans it), custom piecewise-linear (clamped ends, w >= 0).
    pub fn weight(&self, s: f64) -> f64 { match self { Curve::Box => 1.0, Curve::Triangle => (1.0 - (2.0 * s - 1.0).abs()).max(0.0), Curve::Gaussian => (-0.5 * ((s - 0.5) / (1.0 / 6.0)).powi(2)).exp(),
        Curve::Custom(pts) => { let mut p = pts.clone(); p.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal)); let (Some(&f), Some(&l)) = (p.first(), p.last()) else { return 1.0 };
            let v = if s <= f.0 { f.1 } else if s >= l.0 { l.1 } else { p.windows(2).find(|w| s >= w[0].0 && s <= w[1].0).map(|w| { let d = w[1].0 - w[0].0; if d <= 0.0 { w[1].1 } else { w[0].1 + (w[1].1 - w[0].1) * (s - w[0].0) / d } }).unwrap_or(l.1) }; v.max(0.0) } } }
    /// Tool / GUI name.
    pub fn name(&self) -> &'static str { match self { Curve::Box => "box", Curve::Triangle => "triangle", Curve::Gaussian => "gaussian", Curve::Custom(_) => "custom" } }
    /// Storage as (kind code prop, custom points prop with key t = s, v = w, Ease::Linear); None for box (absent props = box).
    pub fn to_props(&self) -> Option<(crate::keyframe::Prop, Option<crate::keyframe::Prop>)> { let code = match self { Curve::Box => return None, Curve::Triangle => 1.0, Curve::Gaussian => 2.0, Curve::Custom(_) => 3.0 };
        let pts = if let Curve::Custom(v) = self { let mut p = crate::keyframe::Prop::new(0.0); for &(s, w) in v { p.set_key(s, w, crate::keyframe::Ease::Linear); } Some(p) } else { None }; Some((crate::keyframe::Prop::new(code), pts)) }
}

/// Curve from the stored kind code (0 box, 1 triangle, 2 gaussian, 3 custom) and the custom points prop (keys t = s, v = w); Box when absent or unusable.
pub fn curve_from_props(code: Option<f64>, pts: Option<&crate::keyframe::Prop>) -> Curve { match code.filter(|v| v.is_finite()).map(|v| v.round() as i64) { Some(1) => Curve::Triangle, Some(2) => Curve::Gaussian,
    Some(3) => match pts { Some(p) if p.keys.len() >= 2 => Curve::Custom(p.keys.iter().map(|k| (k.t, k.v)).collect()), _ => Curve::Box }, _ => Curve::Box } }

/// (time offset, normalized weight) per midpoint sample; Box gives exactly 1/n each, a zero-sum curve falls back to equal weights.
pub fn weighted_midpoints(open: f64, len: f64, n: usize, curve: &Curve) -> Vec<(f64, f64)> { let ts = midpoints(open, len, n); let eq = 1.0 / n as f64;
    if *curve == Curve::Box { return ts.into_iter().map(|t| (t, eq)).collect(); } let w: Vec<f64> = (0..n).map(|k| curve.weight((k as f64 + 0.5) / n as f64)).collect(); let sum: f64 = w.iter().sum();
    if !(sum > 0.0 && sum.is_finite()) { return ts.into_iter().map(|t| (t, eq)).collect(); } ts.into_iter().zip(w).map(|(t, w)| (t, w / sum)).collect() }

/// Validate the tool args shutter_curve / curve_points (set_motion_blur, set_mask_motion_blur).
pub fn parse_curve(name: &str, points: Option<&serde_json::Value>) -> Result<Curve, String> { match name { "box" => Ok(Curve::Box), "triangle" => Ok(Curve::Triangle), "gaussian" => Ok(Curve::Gaussian),
    "custom" => { let bad = || "curve_points must be [[s,w],..] with s in 0..1, w >= 0, at least 2 points and one w > 0".to_string(); let arr = points.and_then(|v| v.as_array()).ok_or_else(bad)?; let mut v = vec![];
        for p in arr { let q = p.as_array().filter(|q| q.len() == 2).ok_or_else(bad)?; let (s, w) = (q[0].as_f64().ok_or_else(bad)?, q[1].as_f64().ok_or_else(bad)?); if !(s.is_finite() && (0.0..=1.0).contains(&s) && w.is_finite() && w >= 0.0) { return Err(bad()); } v.push((s, w)); }
        if v.len() < 2 || !v.iter().any(|p| p.1 > 0.0) { return Err(bad()); } v.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap()); Ok(Curve::Custom(v)) }
    x => Err(format!("shutter_curve must be box, triangle, gaussian or custom, got {x}")) } }

/// Shutter window relative to the clip-local time: opens at lt + open, lasts len seconds; samples 0 = auto from travel; curve = shutter efficiency.
#[derive(Clone, Debug, PartialEq)]
pub struct LayerShutter { pub open: f64, pub len: f64, pub samples: usize, pub curve: Curve }

/// The opt-in exact shutter for a clip, or None (legacy sampler) unless motion blur is on and mb_samples, mb_phase or mb_curve is set.
pub fn layer_shutter(c: &crate::model::Clip, fps: f64, lt: f64) -> Option<LayerShutter> {
    if !(c.motion_blur > 0.0) || !(c.props.contains_key("mb_samples") || c.props.contains_key("mb_phase") || c.props.contains_key("mb_curve")) { return None; }
    let fps = fps.max(1.0); let len = c.motion_blur / 360.0 / fps; let open = match c.props.get("mb_phase") { Some(p) => p.at(lt) / 360.0 / fps, None => -len / 2.0 };
    let samples = c.props.get("mb_samples").map(|p| p.at(lt)).filter(|v| v.is_finite() && *v > 0.0).map(|v| v.round() as usize).unwrap_or(0);
    let curve = curve_from_props(c.props.get("mb_curve").map(|p| p.base), c.props.get("mb_curve_pts"));
    Some(LayerShutter { open, len, samples, curve })
}
