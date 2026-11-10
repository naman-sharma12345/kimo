//! Usage: long_gap_proof gray.raw width height x y width height [threshold margin]
//! Raw uint8 grayscale frames from ffmpeg. Outputs JSON for independent scoring.
use kimo_core::long_gap::{BoxPx, Sample, Tracker};
use std::io::{BufReader, Read};
fn main() {
    let a: Vec<String> = std::env::args().collect();
    let n = |i: usize| a[i].parse::<f64>().unwrap();
    let (w, h) = (n(2) as usize, n(3) as usize);
    let b = BoxPx {
        x: n(4),
        y: n(5),
        w: n(6),
        h: n(7),
    };
    let mut reader = BufReader::new(std::fs::File::open(&a[1]).unwrap());
    let mut raw = vec![0u8; w * h];
    reader.read_exact(&mut raw).unwrap();
    let g: Vec<f32> = raw.iter().map(|x| *x as f32).collect();
    let mut t = Tracker::new(
        &g,
        w,
        h,
        b,
        a.get(8).map(|v| v.parse().unwrap()).unwrap_or(0.8),
        a.get(9).map(|v| v.parse().unwrap()).unwrap_or(0.08),
        2,
    )
    .unwrap();
    if a.get(10).map(|v| v == "memory").unwrap_or(false) {
        t.enable_memory();
    }
    let mut out = vec![Sample {
        frame: 0,
        time: 0.,
        state: "seed",
        bbox: Some(b),
        score: 1.,
        margin: 0.,
    }];
    let start = std::time::Instant::now();
    while reader.read_exact(&mut raw).is_ok() {
        let g: Vec<f32> = raw.iter().map(|x| *x as f32).collect();
        let i = out.len();
        out.push(t.step(&g, i, i as f64 / 25.).unwrap());
    }
    println!(
        "{}",
        serde_json::json!({"samples":out,"seconds":start.elapsed().as_secs_f64()})
    );
}
