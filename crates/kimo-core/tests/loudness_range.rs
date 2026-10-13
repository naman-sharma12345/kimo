use kimo_core::editorial::loudness_range;
use std::{f64::consts::TAU, process::Command};
fn signal() -> Vec<f32> {
    (0..48_000*40).flat_map(|i| {
        let amp = if i < 48_000*20 { 0.03 } else { 0.3 };
        let x = (amp*(TAU*1000.0*i as f64/48000.0).sin()) as f32;
        [x,x]
    }).collect()
}
#[test]
fn lra_two_level_ffmpeg_and_thread_equality() {
    let pcm = signal();
    let path = std::env::temp_dir().join(format!("kimo-lra-{}.f32",std::process::id()));
    let bytes: Vec<u8> = pcm.iter().flat_map(|v|v.to_le_bytes()).collect();
    std::fs::write(&path,bytes).unwrap();
    let output = Command::new("ffmpeg").args(["-nostats","-f","f32le","-ar","48000","-ac","2","-i"]).arg(&path)
        .args(["-af","ebur128","-f","null","-"]).output().unwrap();
    std::fs::remove_file(path).unwrap(); assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stderr);
    let line = text.lines().rev().find(|l| l.trim().starts_with("LRA:")).unwrap();
    let reference: f64 = line.split_whitespace().nth(1).unwrap().parse().unwrap();
    let mut results = vec![];
    for n in [1,4] {
        kimo_core::par::set_threads(n);
        results.push(loudness_range(&pcm).unwrap().to_le_bytes());
    }
    assert_eq!(results[0], results[1]);
    let measured = f64::from_le_bytes(results[0]);
    eprintln!("LRA measured={measured:.6} ffmpeg={reference:.1}");
    assert!((measured-reference).abs() <= 0.5, "{measured} vs {reference}");
    assert!(measured > 19.0);
}
#[test]
fn lra_short_silent_and_constant() {
    assert_eq!(loudness_range(&vec![0.; 288_000]),None);
    assert_eq!(loudness_range(&vec![0.1; 287_998]),None);
    let pcm: Vec<f32> = (0..48_000*6).flat_map(|i| { let x = (0.1*(TAU*1000.*i as f64/48000.).sin()) as f32; [x,x]}).collect();
    assert!(loudness_range(&pcm).unwrap() < 0.01);
}

fn reference(pcm: &[f32], label: &str) -> f64 {
    let path = std::env::temp_dir().join(format!("kimo-lra-{}-{label}.f32",std::process::id()));
    std::fs::write(&path, pcm.iter().flat_map(|v| v.to_le_bytes()).collect::<Vec<_>>()).unwrap();
    let output = Command::new("ffmpeg").args(["-nostats","-f","f32le","-ar","48000","-ac","2","-i"]).arg(&path)
        .args(["-af","ebur128","-f","null","-"]).output().unwrap();
    std::fs::remove_file(path).unwrap(); assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stderr);
    text.lines().rev().find(|l|l.trim().starts_with("LRA:")).unwrap().split_whitespace().nth(1).unwrap().parse().unwrap()
}
fn levels(a: f64,b: f64) -> Vec<f32> {
    (0..48_000*60).flat_map(|i| {
        let amp = 10.0_f64.powf((if i < 48_000*30 { a } else { b })/20.0);
        let x = (amp*(TAU*1000.0*i as f64/48000.0).sin()) as f32; [x,x]
    }).collect()
}
#[test]
fn lra_absolute_minus70_gate_ffmpeg() {
    let pcm=levels(-80.0,-65.0);
    let measured=loudness_range(&pcm).unwrap(); let oracle=reference(&pcm,"absolute");
    eprintln!("absolute gate measured={measured:.6} ffmpeg={oracle:.1}");
    assert!((measured-oracle).abs() <= 0.5,"{measured} vs {oracle}");
}
#[test]
fn lra_relative_minus20_gate_ffmpeg() {
    let pcm=levels(-45.0,-10.0);
    let measured=loudness_range(&pcm).unwrap(); let oracle=reference(&pcm,"relative");
    eprintln!("relative gate measured={measured:.6} ffmpeg={oracle:.1}");
    assert!((measured-oracle).abs() <= 0.5,"{measured} vs {oracle}");
}

#[test]
fn lra_relative_retains_minus15_lu_ffmpeg() {
    let pcm=levels(-25.0,-10.0);
    let measured=loudness_range(&pcm).unwrap(); let oracle=reference(&pcm,"relative-retained");
    eprintln!("relative retained measured={measured:.6} ffmpeg={oracle:.1}");
    assert!((measured-oracle).abs() <= 0.5,"{measured} vs {oracle}");
}

fn frozen_integrated(pcm: &[f32]) -> Option<f64> {
    if pcm.len() < 38400 {
        return None;
    }
    kimo_core::guard::reserve(pcm.len().checked_mul(4)?);
    let mut energy = vec![0.; pcm.len() / 2];
    for ch in 0..2 {
        let mut s1 = [0.; 4];
        let mut s2 = [0.; 4];
        for (i, e) in energy.iter_mut().enumerate() {
            if i % 32768 == 0 {
                kimo_core::guard::check()
            }
            let x = pcm[i * 2 + ch] as f64;
            let biquad = |x: f64, s: &mut [f64; 4], b: [f64; 3], a: [f64; 2]| {
                let y = b[0] * x + b[1] * s[0] + b[2] * s[1] - a[0] * s[2] - a[1] * s[3];
                s[1] = s[0];
                s[0] = x;
                s[3] = s[2];
                s[2] = y;
                y
            };
            let y = biquad(
                x,
                &mut s1,
                [1.53512485958697, -2.69169618940638, 1.19839281085285],
                [-1.69065929318241, 0.73248077421585],
            );
            let z = biquad(
                y,
                &mut s2,
                [1., -2., 1.],
                [-1.99004745483398, 0.99007225036621],
            );
            *e += z * z;
        }
    }
    let mut blocks = vec![];
    let mut sum: f64 = energy[..19200].iter().sum();
    for st in (0..=energy.len() - 19200).step_by(4800) {
        if st > 0 {
            sum -= energy[st - 4800..st].iter().sum::<f64>();
            sum += energy[st + 14400..st + 19200].iter().sum::<f64>();
        }
        let e = sum / 19200.;
        if e > 0. && -0.691 + 10. * e.log10() >= -70. {
            blocks.push(e)
        }
    }
    if blocks.is_empty() {
        return None;
    }
    let mean = blocks.iter().sum::<f64>() / blocks.len() as f64;
    let threshold = mean * 0.1;
    let gated: Vec<f64> = blocks.into_iter().filter(|v| *v >= threshold).collect();
    Some(-0.691 + 10. * (gated.iter().sum::<f64>() / gated.len() as f64).log10())
}

#[test]
fn old_integrated_loudness_bit_identical() {
    for pcm in [signal(),levels(-80.0,-65.0),levels(-45.0,-10.0)] {
        assert_eq!(kimo_core::editorial::loudness(&pcm).map(f64::to_bits), frozen_integrated(&pcm).map(f64::to_bits));
    }
}
