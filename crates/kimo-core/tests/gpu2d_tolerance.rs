//! v7 GPU/2D pipeline tolerance test (FEATURES.md L557): runs the gaussian blur and colour matrix
//! WGSL compute kernels on whatever wgpu adapter is present (lavapipe software Vulkan on this box) and
//! checks each against the CPU reference within a small per-channel epsilon. Skips cleanly (prints why,
//! does not fail) when gpu2d is not compiled in or no adapter can be opened -- the CPU path is what every
//! existing effect and the goldens actually use, so "no adapter" here is not a suite failure.
use kimo_core::effects::Frame;
use kimo_core::gpu2d::{cpu_color_matrix, cpu_gaussian_blur, identity_matrix, probe};

fn gradient(w: usize, h: usize) -> Frame {
    let mut d = vec![0f32; w * h * 4];
    for y in 0..h { for x in 0..w { let i = (y * w + x) * 4; d[i] = x as f32 / w as f32; d[i + 1] = y as f32 / h as f32; d[i + 2] = 0.5; d[i + 3] = 1.0; } }
    Frame { w, h, d }
}

#[test]
fn status_probe_names_a_backend_or_a_reason() {
    let s = probe();
    println!("gpu2d probe: backend={} fallback_reason={:?}", s.backend, s.fallback_reason);
    assert!(!s.backend.is_empty());
}

#[cfg(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos")))]
#[test]
fn gpu_blur_matches_cpu_reference_within_tolerance() {
    use kimo_core::gpu2d::gpu_gaussian_blur;
    let f = gradient(64, 48);
    let cpu = cpu_gaussian_blur(&f, 3.0);
    match gpu_gaussian_blur(&f, 3.0, false) {
        Ok((gpu, status)) => {
            println!("gpu2d blur ran on {} (lavapipe = software Vulkan, not hardware timings)", status.backend);
            assert_eq!((gpu.w, gpu.h), (cpu.w, cpu.h));
            let mut max_diff = 0f32;
            for (a, b) in gpu.d.iter().zip(cpu.d.iter()) { max_diff = max_diff.max((a - b).abs()); }
            println!("max per-channel diff vs CPU reference: {max_diff}");
            assert!(max_diff < 0.01, "GPU blur should match the CPU reference within float-rounding tolerance: max_diff={max_diff}");
        }
        Err(e) => { println!("SKIP (no adapter in this environment): {e}"); }
    }
}

#[cfg(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos")))]
#[test]
fn gpu_color_matrix_matches_cpu_reference_and_identity_is_unchanged() {
    use kimo_core::gpu2d::gpu_color_matrix;
    let f = gradient(32, 32);
    match gpu_color_matrix(&f, &identity_matrix(), false) {
        Ok((gpu, status)) => {
            println!("gpu2d color_matrix (identity) ran on {}", status.backend);
            let mut max_diff = 0f32;
            for (a, b) in gpu.d.iter().zip(f.d.iter()) { max_diff = max_diff.max((a - b).abs()); }
            assert!(max_diff < 1e-5, "identity matrix on GPU should reproduce the input almost exactly: max_diff={max_diff}");
            // a real (non-identity) matrix too, checked against the CPU reference
            let m: [f32; 20] = [0.0,1.0,0.0,0.0,0.0, 1.0,0.0,0.0,0.0,0.0, 0.0,0.0,0.5,0.0,0.1, 0.0,0.0,0.0,1.0,0.0]; // swap r/g, scale+offset b
            let cpu = cpu_color_matrix(&f, &m);
            let (gpu2, _) = gpu_color_matrix(&f, &m, false).unwrap();
            let mut max_diff2 = 0f32;
            for (a, b) in gpu2.d.iter().zip(cpu.d.iter()) { max_diff2 = max_diff2.max((a - b).abs()); }
            println!("max per-channel diff vs CPU reference (swap r/g + scale b): {max_diff2}");
            assert!(max_diff2 < 1e-4, "GPU colour matrix should match the CPU reference: max_diff={max_diff2}");
        }
        Err(e) => { println!("SKIP (no adapter in this environment): {e}"); }
    }
}

#[cfg(not(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos"))))]
#[test]
fn gpu_path_not_compiled_cpu_still_works() {
    // The default build (gpu2d off): the CPU path must still work standalone, which is the whole point
    // of "the CPU path stays the default".
    let f = gradient(16, 16);
    let b = cpu_gaussian_blur(&f, 1.5);
    assert_eq!(b.d.len(), f.d.len());
    println!("SKIP gpu_blur/gpu_color_matrix tests: gpu2d feature not compiled in this build (CPU path verified instead)");
}
