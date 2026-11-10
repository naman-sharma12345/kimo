//! v7 GPU/2D MCP tools (FEATURES.md L557): end-to-end through the actual dispatch path kimo-mcp uses,
//! not just the gpu2d module directly -- proves gpu2d_status / gpu_gaussian_blur_frame / gpu_color_matrix_frame
//! are real, callable MCP tools, registered and TDD-tested like every other feature in this registry.
use kimo_core::commands::{dispatch, Session};
use serde_json::json;

fn setup() -> Session {
    let mut ss = Session::new();
    dispatch(&mut ss, "project_set", &json!({"width": 64, "height": 48, "fps": 25})).unwrap();
    let tr = dispatch(&mut ss, "add_track", &json!({})).unwrap()["track_id"].as_str().unwrap().to_string();
    dispatch(&mut ss, "add_shape", &json!({"track_id": tr, "shape": "rect", "start": 0.0, "duration": 2.0, "color": [0.8,0.2,0.3]})).unwrap();
    ss
}

#[test]
fn gpu2d_status_is_a_registered_tool_and_answers() {
    let mut ss = setup();
    let r = dispatch(&mut ss, "gpu2d_status", &json!({})).unwrap();
    assert!(r.get("backend").is_some());
    assert!(r.get("gpu_available").is_some());
    println!("gpu2d_status: {r}");
}

#[test]
fn gpu_gaussian_blur_frame_writes_a_png_and_reports_tolerance() {
    let mut ss = setup();
    let dir = std::env::temp_dir().join(format!("kimo_gpu2d_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("blur.png");
    let r = dispatch(&mut ss, "gpu_gaussian_blur_frame", &json!({"time": 1.0, "sigma": 3.0, "path": path.to_str().unwrap()})).unwrap();
    assert!(path.exists(), "the tool should have written a PNG");
    assert!(std::fs::metadata(&path).unwrap().len() > 0);
    assert!(r.get("backend").is_some());
    // compare_cpu defaults true: max_abs_diff_vs_cpu should be present and small (GPU==result when GPU ran, so diff==0;
    // if this environment truly has no adapter at all the tool itself fell back to CPU, so diff is exactly 0 either way).
    let diff = r["max_abs_diff_vs_cpu"].as_f64().unwrap();
    assert!(diff < 0.01, "GPU (or CPU fallback) result should match the CPU reference closely: {diff}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn gpu_color_matrix_frame_rejects_wrong_length_and_runs_identity() {
    let mut ss = setup();
    let dir = std::env::temp_dir().join(format!("kimo_gpu2d_test2_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let bad = dispatch(&mut ss, "gpu_color_matrix_frame", &json!({"time": 1.0, "matrix": [1,0,0], "path": dir.join("x.png").to_str().unwrap()}));
    assert!(bad.is_err(), "a 3-element matrix should be rejected");
    let identity = [1.0,0.0,0.0,0.0,0.0, 0.0,1.0,0.0,0.0,0.0, 0.0,0.0,1.0,0.0,0.0, 0.0,0.0,0.0,1.0,0.0];
    let path = dir.join("identity.png");
    let r = dispatch(&mut ss, "gpu_color_matrix_frame", &json!({"time": 1.0, "matrix": identity, "path": path.to_str().unwrap()})).unwrap();
    assert!(path.exists());
    let diff = r["max_abs_diff_vs_cpu"].as_f64().unwrap();
    assert!(diff < 1e-4, "identity matrix GPU vs CPU should match almost exactly: {diff}");
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn both_tools_are_mutates_false_diagnostic_tools() {
    let reg = kimo_core::commands::registry();
    for name in ["gpu2d_status", "gpu_gaussian_blur_frame", "gpu_color_matrix_frame"] {
        let c = reg.iter().find(|c| c.name == name).unwrap_or_else(|| panic!("{name} not registered"));
        assert!(!c.mutates, "{name} should not mutate the project (it is a preview/diagnostic tool)");
    }
}
