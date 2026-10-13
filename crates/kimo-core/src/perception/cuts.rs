//! Fixed-order cut scoring and advisory labels over shared analysis images.
use super::analysis::{hist_distance, luma_delta, Image};
#[derive(Clone, Debug)]
pub struct Cut {
    pub index: usize,
    pub score: f64,
    pub kind: &'static str,
    pub start: Option<usize>,
    pub end: Option<usize>,
}
pub fn score(a: &Image, b: &Image, method: &str) -> f64 {
    let h = hist_distance(a, b);
    let l = (luma_delta(a, b) / 0.25).min(1.);
    match method {
        "histogram" => h,
        "luma" => l,
        _ => h.max(l),
    }
}
pub fn detect(
    images: &[Image],
    method: &str,
    threshold: f64,
    gap: usize,
    hints: bool,
) -> (Vec<Cut>, Vec<f64>) {
    let n = images.len();
    let mut scores = vec![0.; n];
    for i in 1..n {
        crate::guard::check();
        scores[i] = score(&images[i - 1], &images[i], method);
    }
    let means: Vec<f64> = images
        .iter()
        .map(|im| (0..im.w * im.h).map(|i| im.luma(i)).sum::<f64>() / (im.w * im.h) as f64)
        .collect();
    let histograms: Vec<_> = images.iter().map(Image::hist).collect();
    detect_stats(&scores, &means, &histograms, threshold, gap, hints)
}
pub fn detect_stats(
    scores: &[f64],
    means: &[f64],
    histograms: &[[f64; 512]],
    threshold: f64,
    gap: usize,
    hints: bool,
) -> (Vec<Cut>, Vec<f64>) {
    detect_stats_spatial(scores, means, histograms, threshold, gap, hints, None)
}
pub fn detect_stats_spatial(
    scores: &[f64],
    means: &[f64],
    histograms: &[[f64; 512]],
    threshold: f64,
    gap: usize,
    hints: bool,
    spatial: Option<(&[f64], f64)>,
) -> (Vec<Cut>, Vec<f64>) {
    let n = scores.len();
    let mut cuts: Vec<Cut> = vec![];
    let mut i = 1;
    while i < n {
        crate::guard::check();
        // A short excursion followed by a return is one flash, not two hard cuts.
        let lo = i.saturating_sub(5);
        let prior = if i > lo {
            scores[lo..i].iter().sum::<f64>() / (i - lo) as f64
        } else {
            0.
        };
        let spatial_ok = spatial.map_or(true, |(ratios, min)| ratios[i] >= min);
        let motion_ok = scores[i] > prior * 3. + 0.02;
        let content_change = histograms[i - 1]
            .iter()
            .zip(&histograms[i])
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            * 0.5;
        if hints
            && scores[i] > threshold
            && motion_ok
            && spatial_ok
            && (content_change > 0.02 || (means[i] - means[i - 1]).abs() > 0.02)
        {
            let return_at = (i + 1..=(i + 2).min(n - 1)).find(|&j| {
                histograms[i - 1]
                    .iter()
                    .zip(&histograms[j])
                    .map(|(a, b)| (a - b).abs())
                    .sum::<f64>()
                    * 0.5
                    < 0.02
                    && (means[i - 1] - means[j]).abs() < 0.02
            });
            if let Some(j) = return_at {
                cuts.push(Cut {
                    index: i,
                    score: scores[i],
                    kind: "flash",
                    start: Some(i),
                    end: Some(j),
                });
                i = j + 1;
                continue;
            }
        }
        // Moderate monotone luma ramps: collect the whole run before hard-cut gating.
        if hints {
            let delta = means[i] - means[i - 1];
            if delta.abs() > 0.002 && delta.abs() < 0.2 {
                let sign = delta.signum();
                let mut j = i + 1;
                while j < n {
                    let d = means[j] - means[j - 1];
                    if d.abs() <= 0.002 || d.abs() >= 0.2 || d.signum() != sign {
                        break;
                    }
                    j += 1;
                }
                if j - i >= 3
                    && (means[j - 1] - means[i - 1]).abs() > 0.2
                    && spatial.map_or(true, |(ratios, min)| ratios[i..j].iter().any(|r| *r >= min))
                {
                    let peak = scores[i..j].iter().copied().fold(0f64, f64::max);
                    cuts.push(Cut {
                        index: i,
                        score: peak,
                        kind: "dissolve",
                        start: Some(i - 1),
                        end: Some(j - 1),
                    });
                    i = j;
                    continue;
                }
            }
        }
        let lo = i.saturating_sub(5);
        let prior = if i > lo {
            scores[lo..i].iter().sum::<f64>() / (i - lo) as f64
        } else {
            0.
        };
        if scores[i] > threshold
            && spatial_ok
            && scores[i] > prior * 3. + 0.02
            && (content_change > 0.02 || (means[i] - means[i - 1]).abs() > 0.02)
        {
            cuts.push(Cut {
                index: i,
                score: scores[i],
                kind: "hard_cut",
                start: None,
                end: None,
            });
        }
        i += 1;
    }
    let mut merged: Vec<Cut> = vec![];
    for c in cuts {
        if let Some(last) = merged.last_mut() {
            if last.kind == "dissolve" && c.kind == "dissolve" && last.end == c.start {
                let st = last.start.unwrap();
                let mid = c.start.unwrap();
                let en = c.end.unwrap();
                if means[st] > means[mid] && means[en] > means[mid] {
                    last.end = c.end;
                    last.kind = "fade";
                    last.score = last.score.max(c.score);
                    continue;
                }
            }
        }
        merged.push(c);
    }
    // A merged dip fade retains its old first-ramp time. Only standalone dissolves use midpoint.
    for c in &mut merged {
        if c.kind == "dissolve" {
            c.index = (c.start.unwrap() + c.end.unwrap()) / 2;
        }
    }
    let cuts = merged;
    let mut kept: Vec<Cut> = vec![];
    for c in cuts {
        if let Some(last) = kept.last_mut() {
            if c.index - last.index < gap {
                if c.score > last.score {
                    *last = c;
                }
                continue;
            }
        }
        kept.push(c);
    }
    (kept, scores.to_vec())
}
