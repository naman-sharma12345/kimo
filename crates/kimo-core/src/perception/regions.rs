//! Deterministic 8-connected changed-pixel components. merge_px is the radius of each component's Chebyshev dilation.
use super::analysis::Image;
#[derive(Clone, Debug)]
pub struct Region {
    pub first: usize,
    pub points: Vec<usize>,
}
fn hash(im: &Image, x0: usize, y0: usize) -> u64 {
    let mut h = 0xcbf29ce484222325u64;
    for y in y0..(y0 + 64).min(im.h) {
        for x in x0..(x0 + 64).min(im.w) {
            for c in 0..3 {
                for b in im.rgb[c][y * im.w + x].to_bits().to_le_bytes() {
                    h = (h ^ b as u64).wrapping_mul(0x100000001b3);
                }
            }
        }
    }
    h
}
pub fn detect(a: &Image, b: &Image, threshold: f64, merge: usize) -> Vec<Region> {
    let n = a.w * a.h;
    crate::guard::reserve(n * 49);
    let mut mask = vec![false; n];
    for y in (0..a.h).step_by(64) {
        crate::guard::check();
        for x in (0..a.w).step_by(64) {
            // Hash equality alone is not proof: verify the exact bits before skipping a tile.
            let same = hash(a, x, y) == hash(b, x, y)
                && (y..(y + 64).min(a.h)).all(|yy| {
                    (x..(x + 64).min(a.w)).all(|xx| {
                        (0..3).all(|c| {
                            a.rgb[c][yy * a.w + xx].to_bits() == b.rgb[c][yy * a.w + xx].to_bits()
                        })
                    })
                });
            if same {
                continue;
            }
            for yy in y..(y + 64).min(a.h) {
                for xx in x..(x + 64).min(a.w) {
                    let i = yy * a.w + xx;
                    mask[i] = (0..3).any(|c| (a.rgb[c][i] - b.rgb[c][i]).abs() > threshold);
                }
            }
        }
    }
    let mut labels = vec![usize::MAX; n];
    let mut rs: Vec<Region> = vec![];
    for i in 0..n {
        if i % 16384 == 0 {
            crate::guard::check()
        };
        if !mask[i] || labels[i] != usize::MAX {
            continue;
        }
        let id = rs.len();
        let mut points = vec![i];
        labels[i] = id;
        let mut at = 0;
        while at < points.len() {
            let v = points[at];
            at += 1;
            let (x, y) = (v % a.w, v / a.w);
            for yy in y.saturating_sub(1)..=(y + 1).min(a.h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(a.w - 1) {
                    let j = yy * a.w + xx;
                    if mask[j] && labels[j] == usize::MAX {
                        labels[j] = id;
                        points.push(j)
                    }
                }
            }
        }
        points.sort_unstable();
        rs.push(Region { first: i, points });
    }
    if merge == 0 || rs.len() < 2 {
        return rs;
    }
    // Union labels whose dilated masks overlap. A raster sweep avoids component-pair quadratic work.
    let mut parents: Vec<usize> = (0..rs.len()).collect();
    fn root(p: &mut [usize], mut i: usize) -> usize {
        while p[i] != i {
            p[i] = p[p[i]];
            i = p[i]
        }
        i
    }
    let mut owner = labels.clone();
    let mut distance = vec![usize::MAX; n];
    let mut queue = vec![];
    for i in 0..n {
        if mask[i] {
            distance[i] = 0;
            queue.push(i)
        }
    }
    let mut at = 0;
    while at < queue.len() {
        let i = queue[at];
        at += 1;
        if at % 16384 == 0 {
            crate::guard::check()
        };
        if distance[i] >= merge {
            continue;
        }
        let (x, y) = (i % a.w, i / a.w);
        for yy in y.saturating_sub(1)..=(y + 1).min(a.h - 1) {
            for xx in x.saturating_sub(1)..=(x + 1).min(a.w - 1) {
                let j = yy * a.w + xx;
                if owner[j] == usize::MAX {
                    owner[j] = owner[i];
                    distance[j] = distance[i] + 1;
                    queue.push(j)
                } else if distance[i] + 1 + distance[j] <= merge.saturating_mul(2) {
                    let r1 = root(&mut parents, owner[i]);
                    let r2 = root(&mut parents, owner[j]);
                    if r1 != r2 {
                        parents[r1.max(r2)] = r1.min(r2)
                    }
                }
            }
        }
    }
    let mut merged: Vec<Region> = vec![];
    let mut map = std::collections::BTreeMap::new();
    for (id, r) in rs.into_iter().enumerate() {
        let rt = root(&mut parents, id);
        let k = *map.entry(rt).or_insert_with(|| {
            merged.push(Region {
                first: r.first,
                points: vec![],
            });
            merged.len() - 1
        });
        merged[k].points.extend(r.points)
    }
    for r in &mut merged {
        r.points.sort_unstable()
    }
    merged
}
