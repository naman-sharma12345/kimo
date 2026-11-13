//! Picture critic: what a rendered frame looks like. Black frames, blown highlights, crushed shadows, and an edit that
//! wiped the picture out. Works on 8-bit RGBA (the draft frames observe renders, or any rendered frame).
use super::{r3, Finding, Severity};
use serde::Serialize;
use serde_json::json;

#[derive(Clone, Copy, Debug, Serialize, Default, PartialEq)]
pub struct Stats {
    /// Mean Rec.709 luma, 0..1.
    pub luma: f64,
    /// Share of pixels at or under 2% luma (crushed / black).
    pub crushed: f64,
    /// Share of pixels blown to white (every channel at or over 97%).
    pub clipped: f64,
    /// Mean saturation (max-min of RGB), 0..1.
    pub saturation: f64,
}

pub fn stats(rgba: &[u8]) -> Stats {
    let n = (rgba.len() / 4).max(1) as f64;
    let (mut l, mut cr, mut cl, mut sat) = (0.0, 0.0, 0.0, 0.0);
    for px in rgba.chunks_exact(4) {
        let (r, g, b) = (px[0] as f64 / 255.0, px[1] as f64 / 255.0, px[2] as f64 / 255.0);
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        l += y;
        if y <= 0.02 { cr += 1.0; }
        if r.min(g).min(b) >= 0.97 { cl += 1.0; } // blown to white; a saturated primary at full level is not clipping
        sat += r.max(g).max(b) - r.min(g).min(b);
    }
    Stats { luma: r3(l / n), crushed: r3(cr / n), clipped: r3(cl / n), saturation: r3(sat / n) }
}

/// Findings for one rendered frame at time `t`. `before` = the same frame before the edit, when there was one.
pub fn check_frame(t: f64, after: &Stats, before: Option<&Stats>) -> Vec<Finding> {
    let mut out = vec![];
    if after.luma < 0.03 && after.crushed > 0.95 {
        match before {
            Some(b) if b.luma > 0.08 => out.push(Finding::new("edit_made_frame_black", Severity::Error, "picture", format!("this edit turned the frame at {t:.2} s black (luma {:.2} -> {:.2})", b.luma, after.luma))
                .at(t).measured(json!({"before": b.luma, "after": after.luma}))
                .fix("undo", json!({}))),
            Some(_) => {}
            None => out.push(Finding::new("black_frame", Severity::Error, "picture", format!("the frame at {t:.2} s is black")).at(t).measured(json!({"luma": after.luma}))),
        }
    }
    let blown_now = after.clipped > 0.06;
    let blown_before = before.map(|b| b.clipped > 0.06).unwrap_or(false);
    if blown_now && !blown_before {
        out.push(Finding::new("highlights_clipped", Severity::Warning, "color", format!("{:.0}% of the frame at {t:.2} s is blown to pure white/colour: detail is lost", after.clipped * 100.0))
            .at(t).measured(json!({"clipped_share": after.clipped, "before": before.map(|b| b.clipped)})).threshold(json!({"max_share": 0.06})));
    }
    let crushed_now = after.crushed > 0.3 && after.luma >= 0.03;
    let crushed_before = before.map(|b| b.crushed > 0.3).unwrap_or(false);
    if crushed_now && !crushed_before {
        out.push(Finding::new("shadows_crushed", Severity::Info, "color", format!("{:.0}% of the frame at {t:.2} s is crushed to black", after.crushed * 100.0))
            .at(t).measured(json!({"crushed_share": after.crushed, "before": before.map(|b| b.crushed)})).threshold(json!({"max_share": 0.3})));
    }
    out
}

/// Bounding box of pixels that changed by more than `thr` (0..255 on any channel) between two same-size RGBA8 images,
/// and the share of pixels that changed.
pub fn changed_box(a: &[u8], b: &[u8], w: usize, h: usize, thr: u8) -> (Option<[usize; 4]>, f64) {
    let (mut x0, mut y0, mut x1, mut y1, mut n) = (usize::MAX, usize::MAX, 0usize, 0usize, 0usize);
    if a.len() != b.len() || a.len() != w * h * 4 { return (None, 0.0); }
    for y in 0..h { for x in 0..w {
        let i = (y * w + x) * 4;
        if (0..3).any(|k| a[i + k].abs_diff(b[i + k]) > thr) { n += 1; x0 = x0.min(x); y0 = y0.min(y); x1 = x1.max(x); y1 = y1.max(y); }
    } }
    if n == 0 { (None, 0.0) } else { (Some([x0, y0, x1, y1]), n as f64 / (w * h) as f64) }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn img(w: usize, h: usize, rgb: [u8; 3]) -> Vec<u8> { (0..w * h).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect() }
    #[test]
    fn black_after_an_edit_is_an_error_with_undo() {
        let before = stats(&img(8, 8, [120, 120, 120]));
        let after = stats(&img(8, 8, [0, 0, 0]));
        let f = check_frame(1.0, &after, Some(&before));
        assert_eq!(f[0].code, "edit_made_frame_black");
        assert_eq!(f[0].fix["tool"], "undo");
        assert!(check_frame(1.0, &after, Some(&after)).is_empty(), "already black before: not this edit's doing");
    }
    #[test]
    fn clipping_is_reported_only_when_the_edit_caused_it() {
        let ok = stats(&img(8, 8, [128, 128, 128]));
        let blown = stats(&img(8, 8, [255, 255, 255]));
        assert!(check_frame(0.0, &blown, Some(&ok)).iter().any(|x| x.code == "highlights_clipped"));
        assert!(check_frame(0.0, &blown, Some(&blown)).iter().all(|x| x.code != "highlights_clipped"));
    }
    #[test]
    fn changed_box_finds_the_edited_region() {
        let a = img(10, 10, [0, 0, 0]);
        let mut b = a.clone();
        for y in 2..5 { for x in 6..9 { let i = (y * 10 + x) * 4; b[i] = 255; } }
        let (bx, share) = changed_box(&a, &b, 10, 10, 16);
        assert_eq!(bx, Some([6, 2, 8, 4]));
        assert!((share - 0.09).abs() < 1e-9);
    }
}
