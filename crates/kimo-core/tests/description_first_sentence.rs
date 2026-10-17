//! Every registered tool's first sentence (up to the first '.' followed by whitespace or the end) must be under 150
//! characters, so a client that shows only the first sentence still reads what the tool does. The 77 tools below already
//! had a longer first sentence in v5.0.0 (5ad355c); they are frozen here, sorted, and the list can only shrink: the test
//! also fails when an allowlisted tool comes in under 150, so its name has to be removed. Rewriting them is a separate job.
use kimo_core::commands::registry_ref;
const LIMIT: usize = 150;
/// v5.0.0 (5ad355c) tools whose first sentence was already >= 150 characters. Sorted. Only remove names from this list.
const V5_LONG: [&str; 77] = [
    "add_camera_shake",
    "add_hud_frame",
    "add_scene3d",
    "add_shape_modifier",
    "add_shard_burst",
    "apply_camera_solve",
    "apply_motion_preset",
    "audio_probe",
    "audio_speech_intervals",
    "collect_project",
    "contact_sheet",
    "copy_curve",
    "corner_disagreement",
    "decompose_track_homography",
    "describe_frame",
    "ecc_refine_track",
    "explain_invalidation",
    "export_alpha",
    "export_camera_solve",
    "export_manifest",
    "export_ocio_view_video",
    "export_preset",
    "fit_info",
    "fit_track_homography",
    "get_curve",
    "get_transform",
    "import_gltf_surface",
    "inspect_export_preset",
    "job_status",
    "join_export",
    "list_dependencies",
    "lk_residual_audit",
    "mesh_extrude_path",
    "mesh_subdivide",
    "motion_extract",
    "pick_white_balance",
    "pin_layer",
    "preview_range",
    "recipe_apply",
    "refine_matte",
    "render_error_field",
    "render_mask",
    "render_particle_flow",
    "render_particle_flow_motion",
    "render_scene3d_cpu",
    "render_scene3d_wgpu",
    "render_scope",
    "sample_advection_field",
    "set_audio",
    "set_effect_encoding",
    "set_expression",
    "set_flow_blur",
    "set_keyframe_bezier",
    "set_layer_style",
    "set_mask_prop_key",
    "set_material",
    "set_mesh_pin",
    "set_property",
    "set_text_path",
    "set_text_style",
    "shards_project",
    "simulate_physics",
    "solve_camera",
    "stabilize",
    "sync_clips",
    "track_beats",
    "track_feature_residuals",
    "track_features",
    "track_jitter_metrics",
    "track_long_gap",
    "track_matte",
    "track_mesh",
    "track_set_info",
    "transcript_import",
    "transform_vertex_stream",
    "undistort_clip",
    "validate_planar_track",
];
fn first_sentence(d: &str) -> &str {
    let b = d.as_bytes();
    for i in 0..b.len() { if b[i] == b'.' && (i + 1 == b.len() || b[i + 1].is_ascii_whitespace()) { return &d[..=i]; } }
    d
}
#[test] fn allowlist_is_sorted_unique_77_and_has_no_get_scopes() {
    let mut s = V5_LONG.to_vec(); s.sort(); s.dedup(); assert_eq!(s, V5_LONG.to_vec(), "keep V5_LONG sorted and without duplicates");
    assert_eq!(V5_LONG.len(), 77, "the frozen list held 77 names when it was made; it may only shrink, and then this number with it");
    assert!(!V5_LONG.contains(&"get_scopes"), "get_scopes was fixed and must never be allowlisted again");
}
#[test] fn every_first_sentence_is_under_150_characters() {
    let (mut long, mut fixed, mut seen) = (vec![], vec![], std::collections::HashSet::new());
    for c in registry_ref() {
        let f = first_sentence(&c.description); let n = f.chars().count(); seen.insert(c.name);
        if n == 0 { long.push(format!("{}: empty first sentence", c.name)); continue; }
        let allowed = V5_LONG.contains(&c.name);
        if n >= LIMIT && !allowed { long.push(format!("{}: first sentence is {n} characters (limit {}): \"{f}\"", c.name, LIMIT - 1)); }
        if n < LIMIT && allowed { fixed.push(format!("{}: now {n} characters; remove it from V5_LONG", c.name)); }
    }
    let gone: Vec<&str> = V5_LONG.iter().copied().filter(|n| !seen.contains(n)).collect();
    assert!(long.is_empty() && fixed.is_empty() && gone.is_empty(), "first-sentence rule:\n{}{}{}", long.join("\n"), fixed.join("\n"), if gone.is_empty() { String::new() } else { format!("\nnot registered any more, remove from V5_LONG: {gone:?}") });
}
#[test] fn get_scopes_names_its_numbers() {
    let d = registry_ref().iter().find(|c| c.name == "get_scopes").unwrap(); let f = first_sentence(&d.description);
    for w in ["brightness", "colourfulness", "percentiles", "blown-out", "crushed", "zone"] { assert!(f.contains(w), "get_scopes first sentence lacks {w}: {f}"); }
}
