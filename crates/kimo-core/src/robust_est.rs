//! Robust 2-D point-set registration: RANSAC with OpenCV's adaptive termination, minimal solvers for translation (1 point),
//! similarity (2 points), affine (3 points) and homography (4 points), OpenCV's degenerate-sample checks, and least-squares refits.
//!
//! PORTED FROM OpenCV, https://github.com/opencv/opencv at commit 355813ccd6e1e74a45f0ac68b0bf0ba322e4b4a8:
//!   - `ransac_update_num_iters`      <- RANSACUpdateNumIters, modules/geometry/src/ptsetreg.cpp:54
//!   - `ransac` (loop, subset draw, inlier count, best-model bookkeeping)
//!                                    <- RANSACPointSetRegistrator::{getSubset, findInliers, run}, ptsetreg.cpp:84-254
//!   - `have_collinear_points`        <- haveCollinearPoints, modules/geometry/src/precomp.hpp:141
//!   - similarity 2-point kernel      <- AffinePartial2DEstimatorCallback::runKernel, ptsetreg.cpp:717
//!   - affine 3-point kernel + error  <- Affine2DEstimatorCallback::{runKernel, computeError, checkSubset}, ptsetreg.cpp:604-699
//!   - translation 1-point kernel     <- Translation2DEstimatorCallback::runKernel, ptsetreg.cpp
//!   - homography subset check        <- HomographyEstimatorCallback::checkSubset, modules/geometry/src/fundam.cpp:73
//!     (the 4-point homography kernel itself is kimo's existing normalised DLT, trackmath::dlt)
//! Differences from upstream, on purpose: f64 throughout (upstream computes errors in f32); OpenCV's cv::RNG is replaced by kimo's
//! seeded xorshift (trackmath::Xs), so individual draws differ but the algorithm and the termination rule are the same; the
//! least-squares refits on the final inliers are kimo's.
//! Licensed under the Apache License, Version 2.0 (see crates/kimo-core/THIRD_PARTY/OpenCV-LICENSE.txt). Upstream file notice:
//! 
//!   IMPORTANT: READ BEFORE DOWNLOADING, COPYING, INSTALLING OR USING.
//! 
//!   By downloading, copying, installing or using the software you agree to this license.
//!   If you do not agree to this license, do not download, install,
//!   copy or use the software.
//! 
//! 
//!                           License Agreement
//!                 For Open Source Computer Vision Library
//! 
//!  Copyright (C) 2000-2008, Intel Corporation, all rights reserved.
//!  Copyright (C) 2009, Willow Garage Inc., all rights reserved.
//!  Copyright (C) 2013, OpenCV Foundation, all rights reserved.
//!  Third party copyrights are property of their respective owners.
//! 
//!  Redistribution and use in source and binary forms, with or without modification,
//!  are permitted provided that the following conditions are met:
//! 
//!    * Redistribution's of source code must retain the above copyright notice,
//!      this list of conditions and the following disclaimer.
//! 
//!    * Redistribution's in binary form must reproduce the above copyright notice,
//!      this list of conditions and the following disclaimer in the documentation
//!      and/or other materials provided with the distribution.
//! 
//!    * The name of the copyright holders may not be used to endorse or promote products
//!      derived from this software without specific prior written permission.
//! 
//!  This software is provided by the copyright holders and contributors "as is" and
//!  any express or implied warranties, including, but not limited to, the implied
//!  warranties of merchantability and fitness for a particular purpose are disclaimed.
//!  In no event shall the Intel Corporation or contributors be liable for any direct,
//!  indirect, incidental, special, exemplary, or consequential damages
//!  (including, but not limited to, procurement of substitute goods or services;
//!  loss of use, data, or profits; or business interruption) however caused
//!  and on any theory of liability, whether in contract, strict liability,
//!  or tort (including negligence or otherwise) arising in any way out of
//!  the use of this software, even if advised of the possibility of such damage.
//! 

use crate::trackmath::{self, H3, P};

/// Motion model, AE "Track Mask" methods plus OpenCV's estimator families.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Model { Translation, Similarity, Affine, Perspective }
pub const MODEL_NAMES: [&str; 4] = ["translation", "similarity", "affine", "perspective"];
impl Model {
    pub fn parse(s: &str) -> Result<Model, String> {
        match s { "translation" => Ok(Model::Translation), "similarity" => Ok(Model::Similarity), "affine" => Ok(Model::Affine), "perspective" => Ok(Model::Perspective),
            _ => Err(format!("unknown motion '{s}' (valid: {})", MODEL_NAMES.join(", "))) }
    }
    /// Minimal sample size.
    pub fn min_points(self) -> usize { match self { Model::Translation => 1, Model::Similarity => 2, Model::Affine => 3, Model::Perspective => 4 } }
    pub fn name(self) -> &'static str { MODEL_NAMES[self as usize] }
}

/// OpenCV RANSACUpdateNumIters: iterations needed so that with probability `p` at least one sample is outlier-free,
/// given outlier ratio `ep` and `model_points` per sample; never more than `max_iters`.
pub fn ransac_update_num_iters(p: f64, ep: f64, model_points: usize, max_iters: usize) -> usize {
    assert!(model_points > 0, "the number of model points should be positive");
    let p = p.clamp(0.0, 1.0); let ep = ep.clamp(0.0, 1.0);
    // avoid inf's & nan's
    let num = (1.0 - p).max(f64::MIN_POSITIVE);
    let denom = 1.0 - (1.0 - ep).powi(model_points as i32);
    if denom < f64::MIN_POSITIVE { return 0; }
    let (num, denom) = (num.ln(), denom.ln());
    if denom >= 0.0 || -num >= max_iters as f64 * (-denom) { max_iters } else { (num / denom).round_ties_even() as usize }
}

/// OpenCV haveCollinearPoints: is the last of `pts` on a line through two earlier points (or too close to them)?
pub fn have_collinear_points(pts: &[P]) -> bool {
    let n = pts.len(); if n == 0 { return false; } let i = n - 1;
    for j in 0..i { let (dx1, dy1) = (pts[j].0 - pts[i].0, pts[j].1 - pts[i].1);
        for k in 0..j { let (dx2, dy2) = (pts[k].0 - pts[i].0, pts[k].1 - pts[i].1);
            if (dx2 * dy1 - dy2 * dx1).abs() <= f32::EPSILON as f64 * (dx1.abs() + dy1.abs() + dx2.abs() + dy2.abs()) { return true; } } }
    false
}

/// Subset sanity check per model (OpenCV checkSubset of the matching callback).
pub fn check_subset(m: Model, s: &[P], d: &[P]) -> bool {
    match m {
        Model::Translation => true,
        Model::Similarity | Model::Affine => !have_collinear_points(s) && !have_collinear_points(d),
        Model::Perspective => {
            if have_collinear_points(s) || have_collinear_points(d) { return false; }
            if s.len() == 4 { let tt = [[0usize, 1, 2], [1, 2, 3], [0, 2, 3], [0, 1, 3]]; let mut negative = 0;
                let det = |p: &[P], t: &[usize; 3]| { let (a, b, c) = (p[t[0]], p[t[1]], p[t[2]]); a.0 * (b.1 - c.1) - a.1 * (b.0 - c.0) + (b.0 * c.1 - c.0 * b.1) };
                for t in &tt { if det(s, t) * det(d, t) < 0.0 { negative += 1; } }
                if negative != 0 && negative != 4 { return false; } }
            true
        }
    }
}

/// 2x3 affine as a 3x3 matrix.
fn aff(m: [f64; 6]) -> H3 { [[m[0], m[1], m[2]], [m[3], m[4], m[5]], [0.0, 0.0, 1.0]] }

/// Minimal-sample kernel (OpenCV runKernel). Returns None for a degenerate sample.
pub fn run_kernel(m: Model, s: &[P], d: &[P]) -> Option<H3> {
    match m {
        Model::Translation => Some(aff([1.0, 0.0, d[0].0 - s[0].0, 0.0, 1.0, d[0].1 - s[0].1])),
        Model::Similarity => { // AffinePartial2DEstimatorCallback::runKernel
            let (x1, y1, x2, y2) = (s[0].0, s[0].1, s[1].0, s[1].1); let (cx1, cy1, cx2, cy2) = (d[0].0, d[0].1, d[1].0, d[1].1);
            let dd = (x1 - x2) * (x1 - x2) + (y1 - y2) * (y1 - y2); if dd <= 0.0 { return None; } let dd = 1.0 / dd;
            let s0 = dd * ((cx1 - cx2) * (x1 - x2) + (cy1 - cy2) * (y1 - y2));
            let s1 = dd * ((cy1 - cy2) * (x1 - x2) - (cx1 - cx2) * (y1 - y2));
            let s2 = dd * ((cy1 - cy2) * (x1 * y2 - x2 * y1) - (cx1 * y2 - cx2 * y1) * (y1 - y2) - (cx1 * x2 - cx2 * x1) * (x1 - x2));
            let s3 = dd * (-(cx1 - cx2) * (x1 * y2 - x2 * y1) - (cy1 * x2 - cy2 * x1) * (x1 - x2) - (cy1 * y2 - cy2 * y1) * (y1 - y2));
            Some(aff([s0, -s1, s2, s1, s0, s3]))
        }
        Model::Affine => { // Affine2DEstimatorCallback::runKernel
            let (x1, y1, x2, y2, x3, y3) = (s[0].0, s[0].1, s[1].0, s[1].1, s[2].0, s[2].1);
            let (cx1, cy1, cx2, cy2, cx3, cy3) = (d[0].0, d[0].1, d[1].0, d[1].1, d[2].0, d[2].1);
            let den = x1 * (y2 - y3) + x2 * (y3 - y1) + x3 * (y1 - y2); if den == 0.0 { return None; } let dd = 1.0 / den;
            Some(aff([dd * (cx1 * (y2 - y3) + cx2 * (y3 - y1) + cx3 * (y1 - y2)), dd * (cx1 * (x3 - x2) + cx2 * (x1 - x3) + cx3 * (x2 - x1)),
                dd * (cx1 * (x2 * y3 - x3 * y2) + cx2 * (x3 * y1 - x1 * y3) + cx3 * (x1 * y2 - x2 * y1)),
                dd * (cy1 * (y2 - y3) + cy2 * (y3 - y1) + cy3 * (y1 - y2)), dd * (cy1 * (x3 - x2) + cy2 * (x1 - x3) + cy3 * (x2 - x1)),
                dd * (cy1 * (x2 * y3 - x3 * y2) + cy2 * (x3 * y1 - x1 * y3) + cy3 * (x1 * y2 - x2 * y1))]))
        }
        Model::Perspective => trackmath::dlt(s, d).and_then(|h| trackmath::norm_h(&h)),
    }
}

/// Squared reprojection error (OpenCV computeError).
pub fn sq_err(h: &H3, s: P, d: P) -> f64 { match trackmath::apply(h, s) { Some(p) => (p.0 - d.0).powi(2) + (p.1 - d.1).powi(2), None => f64::INFINITY } }

/// Least-squares refit of the model on the given correspondences (kimo's, after inlier selection).
pub fn refit(m: Model, s: &[P], d: &[P]) -> Option<H3> {
    let n = s.len(); if n < m.min_points() { return None; } let nf = n as f64;
    match m {
        Model::Translation => { let (mut tx, mut ty) = (0.0, 0.0); for i in 0..n { tx += d[i].0 - s[i].0; ty += d[i].1 - s[i].1; } Some(aff([1.0, 0.0, tx / nf, 0.0, 1.0, ty / nf])) }
        Model::Similarity => { // closed-form least squares for q = [a -b; b a] p + t
            let (mut sx, mut sy, mut dx, mut dy) = (0.0, 0.0, 0.0, 0.0); for i in 0..n { sx += s[i].0; sy += s[i].1; dx += d[i].0; dy += d[i].1; } sx /= nf; sy /= nf; dx /= nf; dy /= nf;
            let (mut num_a, mut num_b, mut den) = (0.0, 0.0, 0.0);
            for i in 0..n { let (px, py, qx, qy) = (s[i].0 - sx, s[i].1 - sy, d[i].0 - dx, d[i].1 - dy); num_a += px * qx + py * qy; num_b += px * qy - py * qx; den += px * px + py * py; }
            if den <= 0.0 { return None; } let (a, b) = (num_a / den, num_b / den);
            Some(aff([a, -b, dx - (a * sx - b * sy), b, a, dy - (b * sx + a * sy)]))
        }
        Model::Affine => { // normal equations, centred for conditioning
            let (mut sx, mut sy) = (0.0, 0.0); for p in s { sx += p.0; sy += p.1; } sx /= nf; sy /= nf;
            let mut ata = vec![vec![0.0; 3]; 3]; let (mut bx, mut by) = (vec![0.0; 3], vec![0.0; 3]);
            for i in 0..n { let r = [s[i].0 - sx, s[i].1 - sy, 1.0]; for a in 0..3 { for b in 0..3 { ata[a][b] += r[a] * r[b]; } bx[a] += r[a] * d[i].0; by[a] += r[a] * d[i].1; } }
            let mut a2 = ata.clone(); if !trackmath::solve_linear(&mut a2, &mut bx) { return None; } let mut a3 = ata; if !trackmath::solve_linear(&mut a3, &mut by) { return None; }
            Some(aff([bx[0], bx[1], bx[2] - bx[0] * sx - bx[1] * sy, by[0], by[1], by[2] - by[0] * sx - by[1] * sy]))
        }
        Model::Perspective => trackmath::dlt(s, d).and_then(|h| trackmath::norm_h(&h)),
    }
}

pub struct RansacResult { pub model: H3, pub inliers: Vec<bool>, pub n_inliers: usize, pub iterations: usize }

/// OpenCV RANSACPointSetRegistrator::run with `threshold` in pixels (inlier if squared error <= threshold^2), then a least-squares
/// refit on the inliers and one re-classification (kimo). Deterministic for a given seed.
pub fn ransac(m: Model, s: &[P], d: &[P], threshold: f64, confidence: f64, max_iters: usize, seed: u64) -> Option<RansacResult> {
    let count = s.len(); let mp = m.min_points(); if count < mp || d.len() != count { return None; }
    let t2 = threshold * threshold; let inl = |h: &H3| -> (usize, Vec<bool>) { let mk: Vec<bool> = (0..count).map(|i| sq_err(h, s[i], d[i]) <= t2).collect(); (mk.iter().filter(|b| **b).count(), mk) };
    let (mut best, mut best_mask, mut max_good, mut iters_done) = (None::<H3>, vec![false; count], 0usize, 0usize);
    if count == mp { let h = run_kernel(m, s, d)?; return Some(RansacResult { model: h, inliers: vec![true; count], n_inliers: count, iterations: 0 }); }
    let mut rng = trackmath::Xs(seed | 1); let mut niters = max_iters.max(1); let mut it = 0;
    let (mut ss, mut dd) = (vec![(0.0, 0.0); mp], vec![(0.0, 0.0); mp]);
    while it < niters {
        // getSubset: up to 10000 attempts to draw distinct indices that pass checkSubset
        let mut found = false;
        for _ in 0..10000 { let mut idx = vec![0usize; mp];
            for i in 0..mp { let mut j = rng.below(count); while idx[..i].contains(&j) { j = rng.below(count); } idx[i] = j; ss[i] = s[j]; dd[i] = d[j]; }
            if check_subset(m, &ss, &dd) { found = true; break; } }
        if !found { if it == 0 { return None; } break; }
        if let Some(h) = run_kernel(m, &ss, &dd) { let (good, mk) = inl(&h);
            if good > max_good.max(mp - 1) { best = Some(h); best_mask = mk; max_good = good;
                niters = ransac_update_num_iters(confidence, (count - good) as f64 / count as f64, mp, niters); } }
        it += 1; iters_done = it;
    }
    let h0 = best?;
    let (si, di): (Vec<P>, Vec<P>) = (0..count).filter(|&i| best_mask[i]).map(|i| (s[i], d[i])).unzip();
    let h = refit(m, &si, &di).unwrap_or(h0); let (n2, m2) = inl(&h);
    let (model, inliers, n_inliers) = if n2 >= max_good { (h, m2, n2) } else { (h0, best_mask, max_good) };
    Some(RansacResult { model, inliers, n_inliers, iterations: iters_done })
}
