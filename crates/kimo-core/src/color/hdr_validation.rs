//! Numerical evidence only, not calibrated display or mastering certification.
//! Reference: ITU-R BT.2100-3 Tables 4/5, Note 5f (400..2000 nit monitoring).
use super::{managed, metadata::Mastering};
use crate::effects::Frame;
use serde::Serialize;

/// Zero-black reference surround target. No black lift, surround adjustment or
/// extended-range gamma model is implied. RGB must be Rec.2020 scene linear.
#[derive(Clone, Copy, Debug)]
pub struct HlgTarget {
    peak_nits: f64,
}
impl HlgTarget {
    pub fn new(peak_nits: f64) -> Result<Self, String> {
        if !peak_nits.is_finite() || !(400.0..=2000.0).contains(&peak_nits) {
            return Err("HLG reference target peak requires 400..2000 nits; extended-range/surround model unsupported".into());
        }
        Ok(Self { peak_nits })
    }
    pub fn gamma(self) -> f64 {
        1.2 + 0.42 * (self.peak_nits / 1000.0).log10()
    }
    /// Apply OOTF to nominal scene RGB. Reject excursions rather than silently
    /// clip production headroom. Results are display-linear RGB in cd/m^2.
    pub fn ootf(self, rgb: [f64; 3]) -> Result<[f64; 3], String> {
        if rgb
            .iter()
            .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
        {
            return Err("HLG target OOTF requires nominal finite Rec2020 scene RGB0..1".into());
        }
        if rgb == [0.0; 3] {
            return Ok(rgb);
        }
        let y = rec2020_luma(rgb);
        let gain = self.peak_nits * y.powf(self.gamma() - 1.0);
        Ok(rgb.map(|v| v * gain))
    }
    /// Atomic frame application: preflight every pixel before any writes.
    /// Alpha is unchanged. Callers must explicitly supply Rec.2020 scene RGB.
    pub fn apply(self, frame: &mut Frame) -> Result<(), String> {
        super::frame::validate(frame)?;
        for (i, p) in frame.d.chunks_exact(4).enumerate() {
            if i % 65536 == 0 {
                crate::guard::check();
            }
            self.ootf([p[0] as f64, p[1] as f64, p[2] as f64])?;
        }
        for (i, p) in frame.d.chunks_exact_mut(4).enumerate() {
            if i % 65536 == 0 {
                crate::guard::check();
            }
            let out = self.ootf([p[0] as f64, p[1] as f64, p[2] as f64])?;
            for j in 0..3 {
                p[j] = out[j] as f32;
            }
        }
        Ok(())
    }
}
fn rec2020_luma(rgb: [f64; 3]) -> f64 {
    0.2627 * rgb[0] + 0.6780 * rgb[1] + 0.0593 * rgb[2]
}

#[derive(Debug, Serialize)]
pub struct TransferEvidence {
    pub samples: usize,
    pub pq_reference_decode_max_nits_error: f64,
    pub pq_reference_encode_max_error: f64,
    pub pq_roundtrip_max_code_error: f64,
    pub hlg_reference_decode_max_error: f64,
    pub hlg_reference_encode_max_error: f64,
    pub hlg_roundtrip_max_code_error: f64,
    pub passed: bool,
}
// Independent reference expressions retain the standard's rational constants
// and computed HLG b/c rather than copying implementation decimal literals.
fn pq_ref_decode(code: f64) -> f64 {
    let m1 = 2610.0 / 16384.0;
    let m2 = 2523.0 / 32.0;
    let p = code.powf(1.0 / m2);
    10000.0
        * ((p - 3424.0 / 4096.0).max(0.0) / (2413.0 / 128.0 - 2392.0 / 128.0 * p)).powf(1.0 / m1)
}
fn pq_ref_encode(nits: f64) -> f64 {
    let p = (nits / 10000.0).powf(2610.0 / 16384.0);
    ((3424.0 / 4096.0 + 2413.0 / 128.0 * p) / (1.0 + 2392.0 / 128.0 * p)).powf(2523.0 / 32.0)
}
fn hlg_ref_decode(code: f64) -> f64 {
    let a: f64 = 0.17883277;
    let b = 1.0 - 4.0 * a;
    let c = 0.5 - a * (4.0 * a).ln();
    if code <= 0.5 {
        code * code / 3.0
    } else {
        (((code - c) / a).exp() + b) / 12.0
    }
}
fn hlg_ref_encode(scene: f64) -> f64 {
    let a: f64 = 0.17883277;
    if scene <= 1.0 / 12.0 {
        (3.0 * scene).sqrt()
    } else {
        a * (12.0 * scene - (1.0 - 4.0 * a)).ln() + 0.5 - a * (4.0 * a).ln()
    }
}
/// Fixed inclusive 16-bit code grid plus linear scene/luminance grid.
/// PQ code zero is deliberately included: ST2084's black encode is nonzero.
pub fn transfer_evidence() -> TransferEvidence {
    let mut e = TransferEvidence {
        samples: 65536,
        pq_reference_decode_max_nits_error: 0.0,
        pq_reference_encode_max_error: 0.0,
        pq_roundtrip_max_code_error: 0.0,
        hlg_reference_decode_max_error: 0.0,
        hlg_reference_encode_max_error: 0.0,
        hlg_roundtrip_max_code_error: 0.0,
        passed: false,
    };
    for i in 0..e.samples {
        if i % 4096 == 0 {
            crate::guard::check();
        }
        let x = i as f64 / 65535.0;
        for white in [1.0, 100.0, 203.0, 1000.0, 10000.0] {
            let d = managed::decode(6, x, white);
            e.pq_reference_decode_max_nits_error = e
                .pq_reference_decode_max_nits_error
                .max((d * white - pq_ref_decode(x)).abs());
            e.pq_reference_encode_max_error = e.pq_reference_encode_max_error.max(
                (managed::encode(6, x * 10000.0 / white, white) - pq_ref_encode(x * 10000.0)).abs(),
            );
            e.pq_roundtrip_max_code_error = e
                .pq_roundtrip_max_code_error
                .max((managed::encode(6, d, white) - x).abs());
        }
        let d = managed::decode(7, x, 100.0);
        e.hlg_reference_decode_max_error = e
            .hlg_reference_decode_max_error
            .max((d - hlg_ref_decode(x)).abs());
        e.hlg_reference_encode_max_error = e
            .hlg_reference_encode_max_error
            .max((managed::encode(7, x, 100.0) - hlg_ref_encode(x)).abs());
        e.hlg_roundtrip_max_code_error = e
            .hlg_roundtrip_max_code_error
            .max((managed::encode(7, d, 100.0) - x).abs());
    }
    e.passed = e.pq_reference_decode_max_nits_error < 1e-8
        && e.pq_reference_encode_max_error < 1e-12
        && e.pq_roundtrip_max_code_error < 8e-7
        && e.hlg_reference_decode_max_error < 3e-8
        && e.hlg_reference_encode_max_error < 1e-8
        && e.hlg_roundtrip_max_code_error < 3e-8;
    e
}

#[derive(Debug, Serialize)]
pub struct PqFrameEvidence {
    pub visible_pixels: usize,
    pub nonfinite_pixels: usize,
    pub invalid_alpha_pixels: usize,
    pub negative_rgb_pixels: usize,
    pub above_10000_nits_pixels: usize,
    pub max_rgb_nits: Option<f64>,
    pub frame_average_max_rgb_nits: Option<f64>,
    pub max_cll_exceeded: Option<bool>,
    pub max_fall_exceeded: Option<bool>,
    pub mastering_peak_exceeded: Option<bool>,
}
/// Audit already converted Rec2020 display-linear PQ RGB, in reference-white
/// units, BEFORE PQ's clamp. Never infer nits from an HLG scene-relative frame.
/// Metadata zeros mean unspecified. Exceeding mastering peak is informational:
/// mastering display capability is not the same as content MaxCLL.
/// Transparent pixels are excluded from this pre-composite diagnostic only.
/// Alpha does not weight RGB; full encoded-clip audits must include black pixels
/// and supply active-image area explicitly. MaxFALL uses mean(max(R,G,B)),
/// NOT mean(CIE Y), per CTA-861.3-A Annex A.
pub fn audit_pq_linear(
    frame: &Frame,
    reference_white: f64,
    metadata: Option<&Mastering>,
) -> Result<PqFrameEvidence, String> {
    if !reference_white.is_finite() || !(1.0..=10000.0).contains(&reference_white) {
        return Err("reference white requires1..10000 nits".into());
    }
    if frame.w == 0
        || frame.h == 0
        || frame.w.checked_mul(frame.h).and_then(|n| n.checked_mul(4)) != Some(frame.d.len())
    {
        return Err("HDR audit requires nonempty RGBA shape".into());
    }
    if let Some(m) = metadata {
        m.validate()?;
    }
    let mut e = PqFrameEvidence {
        visible_pixels: 0,
        nonfinite_pixels: 0,
        invalid_alpha_pixels: 0,
        negative_rgb_pixels: 0,
        above_10000_nits_pixels: 0,
        max_rgb_nits: None,
        frame_average_max_rgb_nits: None,
        max_cll_exceeded: None,
        max_fall_exceeded: None,
        mastering_peak_exceeded: None,
    };
    let mut sum = 0.0;
    let mut max: f64 = 0.0;
    for (i, p) in frame.d.chunks_exact(4).enumerate() {
        if i % 65536 == 0 {
            crate::guard::check();
        }
        if p.iter().any(|v| !v.is_finite()) {
            e.nonfinite_pixels += 1;
            continue;
        }
        if !(0.0..=1.0).contains(&p[3]) {
            e.invalid_alpha_pixels += 1;
            continue;
        }
        if p[3] == 0.0 {
            continue;
        }
        e.visible_pixels += 1;
        let rgb = [p[0], p[1], p[2]].map(|v| v as f64 * reference_white);
        if rgb.iter().any(|v| *v < 0.0) {
            e.negative_rgb_pixels += 1;
        }
        if rgb.iter().any(|v| *v > 10000.0) {
            e.above_10000_nits_pixels += 1;
        }
        for v in rgb {
            max = max.max(v);
        }
        sum += rgb.into_iter().fold(0.0, f64::max);
    }
    // Invalid/negative samples make physical content-light metadata comparison
    // unavailable, rather than reporting a misleading partial frame average.
    if e.visible_pixels > 0
        && e.nonfinite_pixels == 0
        && e.invalid_alpha_pixels == 0
        && e.negative_rgb_pixels == 0
    {
        let avg = sum / e.visible_pixels as f64;
        e.max_rgb_nits = Some(max);
        e.frame_average_max_rgb_nits = Some(avg);
        if let Some(m) = metadata {
            if m.max_cll > 0 {
                e.max_cll_exceeded = Some(max > m.max_cll as f64 + 0.5);
            }
            if m.max_fall > 0 {
                e.max_fall_exceeded = Some(avg > m.max_fall as f64 + 0.5);
            }
            e.mastering_peak_exceeded = Some(max > m.max_nits + 0.00005);
        }
    }
    Ok(e)
}

/// Read-only MCP diagnostics. No project/transfer semantics are changed.
pub fn commands() -> Vec<crate::commands::Command> {
    use crate::commands::Command;
    use serde_json::json;
    vec![
        Command { name: "validate_hdr_transfer_math", description: "Fixed BT2100 PQ/HLG independent-reference grid evidence, not display calibration or codec validation.", mutates: false,
            schema: json!({"type":"object","properties":{},"additionalProperties":false}),
            run: |_, _| serde_json::to_value(transfer_evidence()).map_err(|e| e.to_string()) },
        Command { name: "evaluate_hlg_reference_display", description: "Explicit BT2100 reference zero-black OOTF diagnostic for nominal Rec2020 scene RGB. Returns display-linear RGB nits. Peak400..2000 nits only. Does not change transfer7 or project.", mutates: false,
            schema: json!({"type":"object","properties":{"peak_nits":{"type":"number","minimum":400,"maximum":2000},"scene_rgb":{"type":"array","items":{"type":"number","minimum":0,"maximum":1},"minItems":3,"maxItems":3}},"required":["peak_nits","scene_rgb"],"additionalProperties":false}),
            run: |_, a| {
                let peak = a.get("peak_nits").and_then(|v| v.as_f64()).ok_or("numeric peak_nits required")?;
                let values = a.get("scene_rgb").and_then(|v| v.as_array()).ok_or("scene_rgb array required")?;
                if values.len() != 3 { return Err("scene_rgb requires3 values".into()); }
                let mut rgb = [0.0; 3];
                for j in 0..3 { rgb[j] = values[j].as_f64().ok_or("numeric scene_rgb required")?; }
                let target = HlgTarget::new(peak)?;
                Ok(json!({"display_rgb_nits":target.ootf(rgb)?,"gamma":target.gamma(),"peak_nits":peak,
                    "gamut":"Rec2020","black_nits":0,"surround":"reference","project_changed":false,
                    "note":"Separate nominal-domain numerical evaluator. Not calibrated display output or HLG encoding."}))
            } },
    ]
}
