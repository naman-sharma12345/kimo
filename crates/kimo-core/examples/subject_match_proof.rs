//! Proof through the exact Rust implementation: W H DIR, RGBA and matte f32 LE.
pub use kimo_core::{color, effects, guard};
#[path = "../src/color/subject_match.rs"]
mod subject_match;
use effects::Frame;
use std::{fs, path::Path, time::Instant};
use subject_match::*;
fn floats(p: &Path) -> Vec<f32> {
    let b = fs::read(p).expect("read float input");
    assert_eq!(b.len() % 4, 0);
    b.chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect()
}
fn write(p: &Path, f: &Frame) {
    let b: Vec<u8> = f.d.iter().flat_map(|v| v.to_le_bytes()).collect();
    fs::write(p, b).unwrap();
}
fn main() {
    let a: Vec<_> = std::env::args().collect();
    assert_eq!(a.len(), 4, "W H DIR");
    let w: usize = a[1].parse().unwrap();
    let h: usize = a[2].parse().unwrap();
    let dir = Path::new(&a[3]);
    let reference = Frame {
        w,
        h,
        d: floats(&dir.join("reference.f32")),
    };
    let source = Frame {
        w,
        h,
        d: floats(&dir.join("source.f32")),
    };
    let weights = floats(&dir.join("weights.f32"));
    let weights: Vec<[f32; 3]> = weights
        .chunks_exact(3)
        .map(|v| v.try_into().unwrap())
        .collect();
    let masks = Regions { weights: &weights };
    let t = Instant::now();
    let plan = fit(&source, &masks, &reference, &masks, Options::default()).unwrap();
    println!("fit_ms={:.3}", t.elapsed().as_secs_f64() * 1000.);
    for r in 0..3 {
        println!(
            "region {r} source={:?} reference={:?}",
            plan.source.regions[r], plan.reference.regions[r]
        );
    }
    let mut matched = Frame {
        w,
        h,
        d: source.d.clone(),
    };
    let t = Instant::now();
    plan.apply(&mut matched, &masks).unwrap();
    println!("apply_ms={:.3}", t.elapsed().as_secs_f64() * 1000.);
    write(&dir.join("matched.f32"), &matched);
    let mut o = Options::default();
    o.exposure_target[SKIN] = ExposureTarget::ReferenceOffsetStops(0.35);
    let plan = fit(&source, &masks, &reference, &masks, o).unwrap();
    let mut creative = Frame {
        w,
        h,
        d: source.d.clone(),
    };
    plan.apply(&mut creative, &masks).unwrap();
    write(&dir.join("creative.f32"), &creative);
    let after = statistics(&matched, &masks).unwrap();
    for r in 0..3 {
        let a = after.regions[r].as_ref().unwrap();
        let b = plan.reference.regions[r].as_ref().unwrap();
        println!(
            "matched_region {r} log_mean_error_stops={} chroma_mean_error={}",
            a.log_luminance_mean - b.log_luminance_mean,
            ((a.lab_mean[1] - b.lab_mean[1]).powi(2) + (a.lab_mean[2] - b.lab_mean[2]).powi(2))
                .sqrt()
                * 128.
        );
    }
}
