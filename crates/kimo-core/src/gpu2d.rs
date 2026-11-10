//! v7 GPU/2D pipeline (opt-in, feature `gpu2d`, default off): WGSL compute kernels for two 2D
//! effects -- gaussian blur and a 4x5 colour matrix -- run through wgpu on whatever Vulkan/DX12/Metal
//! adapter is present (on this box: lavapipe, Mesa's software Vulkan implementation). This module is
//! independent of `gpu3d` (the 3D mesh/offscreen-raster path in `threed/offscreen_wgpu.rs`): it shares
//! only the `wgpu` dependency, not any code, so a 2D-only build pulls in none of gpu3d's mesh-import /
//! flat-sah / material-textures machinery.
//!
//! The CPU functions here (`cpu_gaussian_blur`, `cpu_color_matrix`) are the implementation every existing
//! effect, golden and the default render path uses (nothing new calls the GPU path automatically). They
//! also serve as the tolerance test's ground truth: `gpu_vs_cpu_tolerance.rs` renders both and asserts the
//! GPU result matches the CPU reference within a small per-channel epsilon (wgpu/lavapipe float rounding,
//! not bit-exact -- the CPU path is what stays byte-exact for goldens).
use crate::effects::Frame;

/// What actually ran. `backend` is wgpu's adapter name/backend string (e.g. a lavapipe llvmpipe string) when
/// the GPU path ran; `fallback_reason` is set (and backend is "cpu") when gpu2d is not compiled in, or no
/// adapter could be opened -- a caller can always fall back to `cpu_*` on `Err`, this struct is for when the
/// caller wants to *know* which path ran rather than silently accept either.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Gpu2dStatus { pub backend: String, pub fallback_reason: Option<String> }

/// Probe adapter availability without rendering anything (what `gpu2d_status` the MCP tool reports).
pub fn probe() -> Gpu2dStatus {
    #[cfg(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    { match request_adapter_info(false) { Ok(name) => Gpu2dStatus { backend: name, fallback_reason: None }, Err(e) => Gpu2dStatus { backend: "cpu".into(), fallback_reason: Some(e) } } }
    #[cfg(not(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos"))))]
    { Gpu2dStatus { backend: "cpu".into(), fallback_reason: Some("gpu2d feature not compiled for this target".into()) } }
}

// ---------------------------------------------------------------------------------------------
// CPU reference implementations -- the default path, used by everything except the two GPU
// functions below and their tolerance test.
// ---------------------------------------------------------------------------------------------

/// Separable gaussian blur, clamp-to-edge, same tap generation as the glow effect (`crate::glow::gaussian_kernel_1d`)
/// so the two blur implementations in this codebase (glow's and this one) agree on what "sigma" means.
pub fn cpu_gaussian_blur(f: &Frame, sigma: f32) -> Frame {
    let taps = crate::glow::gaussian_kernel_1d(sigma);
    let r = (taps.len() / 2) as i64;
    let (w, h) = (f.w as i64, f.h as i64);
    let clampi = |v: i64, lo: i64, hi: i64| v.max(lo).min(hi);
    let mut mid = vec![0f32; f.d.len()];
    for y in 0..h { for x in 0..w {
        let mut acc = [0f32; 4];
        for (k, wt) in taps.iter().enumerate() { let xx = clampi(x + k as i64 - r, 0, w - 1); let idx = ((y * w + xx) * 4) as usize; for c in 0..4 { acc[c] += wt * f.d[idx + c]; } }
        let o = ((y * w + x) * 4) as usize; for c in 0..4 { mid[o + c] = acc[c]; }
    } }
    let mut out = vec![0f32; f.d.len()];
    for y in 0..h { for x in 0..w {
        let mut acc = [0f32; 4];
        for (k, wt) in taps.iter().enumerate() { let yy = clampi(y + k as i64 - r, 0, h - 1); let idx = ((yy * w + x) * 4) as usize; for c in 0..4 { acc[c] += wt * mid[idx + c]; } }
        let o = ((y * w + x) * 4) as usize; for c in 0..4 { out[o + c] = acc[c]; }
    } }
    Frame { w: f.w, h: f.h, d: out }
}

/// AE-style 4x5 colour matrix: out_channel = sum(m[row*5+0..3] * [r,g,b,a]) + m[row*5+4] (a constant offset), row 0..3 = r,g,b,a.
pub fn cpu_color_matrix(f: &Frame, m: &[f32; 20]) -> Frame {
    let mut out = vec![0f32; f.d.len()];
    for p in 0..f.w * f.h {
        let i = p * 4; let (r, g, b, a) = (f.d[i], f.d[i + 1], f.d[i + 2], f.d[i + 3]);
        for row in 0..4 { out[i + row] = m[row * 5] * r + m[row * 5 + 1] * g + m[row * 5 + 2] * b + m[row * 5 + 3] * a + m[row * 5 + 4]; }
    }
    Frame { w: f.w, h: f.h, d: out }
}
/// Identity colour matrix (out == in): m[0]=1 (r->r), m[6]=1 (g->g), m[12]=1 (b->b), m[18]=1 (a->a), rest 0.
pub fn identity_matrix() -> [f32; 20] { let mut m = [0f32; 20]; m[0] = 1.0; m[6] = 1.0; m[12] = 1.0; m[18] = 1.0; m }

// ---------------------------------------------------------------------------------------------
// GPU (wgpu compute) implementations -- opt-in, feature `gpu2d`. Each function returns Err when
// gpu2d is not compiled in or no adapter can be opened; callers fall back to the cpu_* function above.
// ---------------------------------------------------------------------------------------------

#[cfg(not(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos"))))]
pub fn gpu_gaussian_blur(_f: &Frame, _sigma: f32, _software_only: bool) -> Result<(Frame, Gpu2dStatus), String> { Err("gpu2d feature not compiled for this target".into()) }
#[cfg(not(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos"))))]
pub fn gpu_color_matrix(_f: &Frame, _m: &[f32; 20], _software_only: bool) -> Result<(Frame, Gpu2dStatus), String> { Err("gpu2d feature not compiled for this target".into()) }

#[cfg(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos")))]
mod gpu_impl {
    use super::*;
    use wgpu::util::DeviceExt;

    /// Same timeout-bounded poll-without-an-executor trick as threed/offscreen_wgpu.rs::wait, duplicated
    /// (not shared) because this module does not depend on the gpu3d feature at all.
    pub(super) fn wait<F: std::future::Future>(future: F) -> Result<F::Output, String> {
        use std::task::{Context, Poll, Wake, Waker};
        struct Thread(std::thread::Thread);
        impl Wake for Thread { fn wake(self: std::sync::Arc<Self>) { self.0.unpark(); } fn wake_by_ref(self: &std::sync::Arc<Self>) { self.0.unpark(); } }
        let w = Waker::from(std::sync::Arc::new(Thread(std::thread::current())));
        let mut cx = Context::from_waker(&w);
        let mut future = Box::pin(future);
        let start = std::time::Instant::now();
        loop {
            crate::guard::check();
            if let Poll::Ready(x) = future.as_mut().poll(&mut cx) { return Ok(x); }
            if start.elapsed() > std::time::Duration::from_secs(20) { return Err("GPU adapter/device future timeout".into()); }
            std::thread::park_timeout(std::time::Duration::from_millis(10));
        }
    }

    pub(super) fn open(software_only: bool) -> Result<(wgpu::Device, wgpu::Queue, String), String> {
        if wgpu::Instance::enabled_backend_features().is_empty() { return Err("no compiled wgpu backend".into()); }
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter = wait(instance.request_adapter(&wgpu::RequestAdapterOptions { force_fallback_adapter: software_only, compatible_surface: None, ..Default::default() }))?.map_err(|e| e.to_string())?;
        let name = format!("wgpu:{:?}", adapter.get_info());
        let (device, queue) = wait(adapter.request_device(&wgpu::DeviceDescriptor::default()))?.map_err(|e| e.to_string())?;
        Ok((device, queue, name))
    }

    fn storage_init(device: &wgpu::Device, label: &str, data: &[f32], extra: wgpu::BufferUsages) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some(label), contents: bytemuck_cast(data), usage: wgpu::BufferUsages::STORAGE | extra })
    }
    // No bytemuck dependency: reinterpret the f32 slice as bytes for upload (native-endian round trip via
    // to_le_bytes on readback keeps this honest on any host, little-endian in practice on this box).
    fn bytemuck_cast(data: &[f32]) -> &[u8] { unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u8, data.len() * 4) } }

    fn readback(device: &wgpu::Device, queue: &wgpu::Queue, encoder_src: &wgpu::Buffer, len_f32: usize) -> Result<Vec<f32>, String> {
        let size = (len_f32 * 4) as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor { label: Some("gpu2d readback"), size, usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ, mapped_at_creation: false });
        let mut enc = device.create_command_encoder(&Default::default());
        enc.copy_buffer_to_buffer(encoder_src, 0, &staging, 0, size);
        queue.submit([enc.finish()]);
        let (tx, rx) = std::sync::mpsc::channel();
        staging.map_async(wgpu::MapMode::Read, .., move |r| { let _ = tx.send(r); });
        device.poll(wgpu::PollType::Wait { submission_index: None, timeout: Some(std::time::Duration::from_secs(20)) }).map_err(|e| e.to_string())?;
        rx.recv_timeout(std::time::Duration::from_secs(5)).map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;
        let mapped = staging.get_mapped_range(..).map_err(|e| e.to_string())?;
        let mut out = vec![0f32; len_f32];
        for i in 0..len_f32 { out[i] = f32::from_le_bytes(mapped[i * 4..i * 4 + 4].try_into().unwrap()); }
        drop(mapped); staging.unmap();
        Ok(out)
    }

    const BLUR_WGSL: &str = r#"
struct Params { width: u32, height: u32, radius: u32, _pad: u32 };
@group(0) @binding(0) var<storage, read> input_buf: array<f32>;
@group(0) @binding(1) var<storage, read> weights: array<f32>;
@group(0) @binding(2) var<storage, read_write> output_buf: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;
@compute @workgroup_size(8,8,1)
fn blur_h(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) { return; }
    let w = i32(params.width); let r = i32(params.radius);
    var sum = vec4<f32>(0.0,0.0,0.0,0.0);
    for (var k: i32 = -r; k <= r; k = k + 1) {
        let xx = clamp(i32(gid.x) + k, 0, w - 1);
        let idx = (i32(gid.y) * w + xx) * 4;
        let wt = weights[u32(k + r)];
        sum = sum + wt * vec4<f32>(input_buf[u32(idx)], input_buf[u32(idx+1)], input_buf[u32(idx+2)], input_buf[u32(idx+3)]);
    }
    let o = (i32(gid.y) * w + i32(gid.x)) * 4;
    output_buf[u32(o)] = sum.x; output_buf[u32(o+1)] = sum.y; output_buf[u32(o+2)] = sum.z; output_buf[u32(o+3)] = sum.w;
}
@compute @workgroup_size(8,8,1)
fn blur_v(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) { return; }
    let w = i32(params.width); let h = i32(params.height); let r = i32(params.radius);
    var sum = vec4<f32>(0.0,0.0,0.0,0.0);
    for (var k: i32 = -r; k <= r; k = k + 1) {
        let yy = clamp(i32(gid.y) + k, 0, h - 1);
        let idx = (yy * w + i32(gid.x)) * 4;
        let wt = weights[u32(k + r)];
        sum = sum + wt * vec4<f32>(input_buf[u32(idx)], input_buf[u32(idx+1)], input_buf[u32(idx+2)], input_buf[u32(idx+3)]);
    }
    let o = (i32(gid.y) * w + i32(gid.x)) * 4;
    output_buf[u32(o)] = sum.x; output_buf[u32(o+1)] = sum.y; output_buf[u32(o+2)] = sum.z; output_buf[u32(o+3)] = sum.w;
}
"#;

    const MATRIX_WGSL: &str = r#"
struct Params { width: u32, height: u32, _pad0: u32, _pad1: u32 };
@group(0) @binding(0) var<storage, read> input_buf: array<f32>;
@group(0) @binding(1) var<storage, read> matrix_buf: array<f32>;
@group(0) @binding(2) var<storage, read_write> output_buf: array<f32>;
@group(0) @binding(3) var<uniform> params: Params;
@compute @workgroup_size(8,8,1)
fn cmatrix(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= params.width || gid.y >= params.height) { return; }
    let idx = (i32(gid.y) * i32(params.width) + i32(gid.x)) * 4;
    let r = input_buf[u32(idx)]; let g = input_buf[u32(idx+1)]; let b = input_buf[u32(idx+2)]; let a = input_buf[u32(idx+3)];
    let or = matrix_buf[0]*r + matrix_buf[1]*g + matrix_buf[2]*b + matrix_buf[3]*a + matrix_buf[4];
    let og = matrix_buf[5]*r + matrix_buf[6]*g + matrix_buf[7]*b + matrix_buf[8]*a + matrix_buf[9];
    let ob = matrix_buf[10]*r + matrix_buf[11]*g + matrix_buf[12]*b + matrix_buf[13]*a + matrix_buf[14];
    let oa = matrix_buf[15]*r + matrix_buf[16]*g + matrix_buf[17]*b + matrix_buf[18]*a + matrix_buf[19];
    output_buf[u32(idx)] = or; output_buf[u32(idx+1)] = og; output_buf[u32(idx+2)] = ob; output_buf[u32(idx+3)] = oa;
}
"#;

    pub(super) fn run_blur(f: &Frame, sigma: f32, software_only: bool) -> Result<(Frame, Gpu2dStatus), String> {
        let (device, queue, name) = open(software_only)?;
        let taps = crate::glow::gaussian_kernel_1d(sigma);
        let radius = (taps.len() / 2) as u32;
        let n = f.w * f.h * 4;
        let input = storage_init(&device, "gpu2d blur input", &f.d, wgpu::BufferUsages::COPY_DST);
        let weights = storage_init(&device, "gpu2d blur weights", &taps, wgpu::BufferUsages::COPY_DST);
        let mid = device.create_buffer(&wgpu::BufferDescriptor { label: Some("gpu2d blur mid"), size: (n * 4) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let output = device.create_buffer(&wgpu::BufferDescriptor { label: Some("gpu2d blur output"), size: (n * 4) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let params = [f.w as u32, f.h as u32, radius, 0u32];
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu2d blur params"), contents: &{ let mut b = Vec::new(); for p in params { b.extend_from_slice(&p.to_le_bytes()); } b }, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("gpu2d blur"), source: wgpu::ShaderSource::Wgsl(BLUR_WGSL.into()) });
        let pipe_h = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("blur_h"), layout: None, module: &shader, entry_point: Some("blur_h"), compilation_options: Default::default(), cache: None });
        let pipe_v = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("blur_v"), layout: None, module: &shader, entry_point: Some("blur_v"), compilation_options: Default::default(), cache: None });
        let bind_h = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("blur_h bind"), layout: &pipe_h.get_bind_group_layout(0), entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: input.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: weights.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: mid.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: params_buf.as_entire_binding() }] });
        let bind_v = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("blur_v bind"), layout: &pipe_v.get_bind_group_layout(0), entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: mid.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: weights.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: output.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: params_buf.as_entire_binding() }] });
        let (gx, gy) = (((f.w as u32) + 7) / 8, ((f.h as u32) + 7) / 8);
        let mut enc = device.create_command_encoder(&Default::default());
        { let mut pass = enc.begin_compute_pass(&Default::default()); pass.set_pipeline(&pipe_h); pass.set_bind_group(0, &bind_h, &[]); pass.dispatch_workgroups(gx, gy, 1); }
        { let mut pass = enc.begin_compute_pass(&Default::default()); pass.set_pipeline(&pipe_v); pass.set_bind_group(0, &bind_v, &[]); pass.dispatch_workgroups(gx, gy, 1); }
        queue.submit([enc.finish()]);
        let data = readback(&device, &queue, &output, n)?;
        Ok((Frame { w: f.w, h: f.h, d: data }, Gpu2dStatus { backend: name, fallback_reason: None }))
    }

    pub(super) fn run_matrix(f: &Frame, m: &[f32; 20], software_only: bool) -> Result<(Frame, Gpu2dStatus), String> {
        let (device, queue, name) = open(software_only)?;
        let n = f.w * f.h * 4;
        let input = storage_init(&device, "gpu2d matrix input", &f.d, wgpu::BufferUsages::COPY_DST);
        let matrix_buf = storage_init(&device, "gpu2d matrix m", m, wgpu::BufferUsages::COPY_DST);
        let output = device.create_buffer(&wgpu::BufferDescriptor { label: Some("gpu2d matrix output"), size: (n * 4) as u64, usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC, mapped_at_creation: false });
        let params = [f.w as u32, f.h as u32, 0u32, 0u32];
        let params_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor { label: Some("gpu2d matrix params"), contents: &{ let mut b = Vec::new(); for p in params { b.extend_from_slice(&p.to_le_bytes()); } b }, usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor { label: Some("gpu2d matrix"), source: wgpu::ShaderSource::Wgsl(MATRIX_WGSL.into()) });
        let pipe = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor { label: Some("cmatrix"), layout: None, module: &shader, entry_point: Some("cmatrix"), compilation_options: Default::default(), cache: None });
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor { label: Some("cmatrix bind"), layout: &pipe.get_bind_group_layout(0), entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: input.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: matrix_buf.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: output.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 3, resource: params_buf.as_entire_binding() }] });
        let (gx, gy) = (((f.w as u32) + 7) / 8, ((f.h as u32) + 7) / 8);
        let mut enc = device.create_command_encoder(&Default::default());
        { let mut pass = enc.begin_compute_pass(&Default::default()); pass.set_pipeline(&pipe); pass.set_bind_group(0, &bind, &[]); pass.dispatch_workgroups(gx, gy, 1); }
        queue.submit([enc.finish()]);
        let data = readback(&device, &queue, &output, n)?;
        Ok((Frame { w: f.w, h: f.h, d: data }, Gpu2dStatus { backend: name, fallback_reason: None }))
    }

    pub(super) fn request_adapter_info(software_only: bool) -> Result<String, String> { open(software_only).map(|(_, _, name)| name) }
    pub(super) use request_adapter_info as probe_only;
}

#[cfg(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn gpu_gaussian_blur(f: &Frame, sigma: f32, software_only: bool) -> Result<(Frame, Gpu2dStatus), String> { gpu_impl::run_blur(f, sigma, software_only) }
#[cfg(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos")))]
pub fn gpu_color_matrix(f: &Frame, m: &[f32; 20], software_only: bool) -> Result<(Frame, Gpu2dStatus), String> { gpu_impl::run_matrix(f, m, software_only) }
#[cfg(all(feature = "gpu2d", any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn request_adapter_info(software_only: bool) -> Result<String, String> { gpu_impl::probe_only(software_only) }

#[cfg(test)]
mod tests {
    use super::*;
    fn checker(w: usize, h: usize) -> Frame {
        let mut d = vec![0f32; w * h * 4];
        for y in 0..h { for x in 0..w { let v = if (x / 4 + y / 4) % 2 == 0 { 1.0 } else { 0.0 }; let i = (y * w + x) * 4; d[i] = v; d[i + 1] = v; d[i + 2] = v; d[i + 3] = 1.0; } }
        Frame { w, h, d }
    }
    #[test]
    fn cpu_blur_smooths_a_checkerboard_and_preserves_mean() {
        let f = checker(32, 32);
        let mean_before: f32 = f.d.iter().step_by(4).sum::<f32>() / (f.w * f.h) as f32;
        let b = cpu_gaussian_blur(&f, 2.0);
        let mean_after: f32 = b.d.iter().step_by(4).sum::<f32>() / (b.w * b.h) as f32;
        assert!((mean_before - mean_after).abs() < 0.02, "blur should roughly preserve the mean: {mean_before} vs {mean_after}");
        // a blurred checkerboard has no more pure-0/pure-1 pixels in the interior (clamp-to-edge affects the border only)
        let interior_extreme = (4..28).flat_map(|y| (4..28).map(move |x| (x, y))).filter(|&(x, y)| { let v = b.d[(y * 32 + x) * 4]; v <= 0.01 || v >= 0.99 }).count();
        assert_eq!(interior_extreme, 0, "blur should soften every interior checker cell");
    }
    #[test]
    fn cpu_blur_sigma_zero_is_near_identity() {
        let f = checker(16, 16);
        let b = cpu_gaussian_blur(&f, 0.001);
        for i in 0..f.d.len() { assert!((f.d[i] - b.d[i]).abs() < 1e-4, "near-zero sigma should barely change pixel {i}"); }
    }
    #[test]
    fn cpu_color_matrix_identity_is_exact() {
        let f = checker(8, 8);
        let out = cpu_color_matrix(&f, &identity_matrix());
        assert_eq!(f.d, out.d);
    }
    #[test]
    fn cpu_color_matrix_invert_rgb() {
        // invert(r,g,b) = 1-r, keep alpha: row r: [-1,0,0,0,1], row g: [0,-1,0,0,1], row b: [0,0,-1,0,1], row a: identity
        let m: [f32; 20] = [-1.0,0.0,0.0,0.0,1.0, 0.0,-1.0,0.0,0.0,1.0, 0.0,0.0,-1.0,0.0,1.0, 0.0,0.0,0.0,1.0,0.0];
        let f = Frame { w: 1, h: 1, d: vec![0.2, 0.7, 1.0, 1.0] };
        let out = cpu_color_matrix(&f, &m);
        assert!((out.d[0] - 0.8).abs() < 1e-6 && (out.d[1] - 0.3).abs() < 1e-6 && (out.d[2] - 0.0).abs() < 1e-6 && (out.d[3] - 1.0).abs() < 1e-6, "{:?}", out.d);
    }
    #[test]
    fn probe_never_panics_and_reports_something() {
        let s = probe();
        assert!(!s.backend.is_empty() || s.fallback_reason.is_some());
    }
}
