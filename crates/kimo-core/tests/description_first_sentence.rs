//! Every registered tool's first sentence (up to the first '.' followed by whitespace or the end) must be under 150
//! characters, so a client that shows only the first sentence still reads what the tool does. There is no allowlist: the
//! 77 tools that had a longer first sentence in v5.0.0 (5ad355c) got a new plain first sentence in front of their old text
//! (REWRITTEN below, kept as the record of that batch), so the rule now holds for every tool.
use kimo_core::commands::registry_ref;
const LIMIT: usize = 150;
/// The 77 v5.0.0 (5ad355c) tools whose first sentence was >= 150 characters, rewritten in one batch. Sorted.
const REWRITTEN: [&str; 77] = [
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
#[test] fn the_77_rewritten_are_registered_and_short() {
    let mut s = REWRITTEN.to_vec(); s.sort(); s.dedup(); assert_eq!(s, REWRITTEN.to_vec(), "keep REWRITTEN sorted and without duplicates"); assert_eq!(REWRITTEN.len(), 77);
    for n in REWRITTEN { let c = registry_ref().iter().find(|c| c.name == n).unwrap_or_else(|| panic!("{n} is not registered"));
        let f = first_sentence(&c.description); assert!(f.chars().count() < LIMIT && f.len() < c.description.len(), "{n}: the new first sentence must be short and followed by the old text: {f}"); }
}
#[test] fn every_first_sentence_is_under_150_characters() {
    let mut long = vec![];
    for c in registry_ref() {
        let f = first_sentence(&c.description); let n = f.chars().count();
        if n == 0 { long.push(format!("{}: empty first sentence", c.name)); continue; }
        if n >= LIMIT { long.push(format!("{}: first sentence is {n} characters (limit {}): \"{f}\"", c.name, LIMIT - 1)); }
    }
    assert!(long.is_empty(), "first-sentence rule:\n{}", long.join("\n"));
}
#[test] fn get_scopes_names_its_numbers() {
    let d = registry_ref().iter().find(|c| c.name == "get_scopes").unwrap(); let f = first_sentence(&d.description);
    for w in ["brightness", "colourfulness", "percentiles", "blown-out", "crushed", "zone"] { assert!(f.contains(w), "get_scopes first sentence lacks {w}: {f}"); }
}
