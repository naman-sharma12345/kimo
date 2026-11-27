//! Tool groups: an access layer over the flat command registry. An AI client sees ~17 entry points (one per group plus
//! search/describe) instead of every tool. Every individual tool stays callable by its own name, so nothing is removed.
//! A tool's group comes from `group_of`: exact names first, then name rules, so most new tools land in the right group on
//! their own. The unit test fails if any registry tool falls through to "extras", which makes adding a tool without a
//! group a visible build failure rather than a silent drift.
use crate::commands::{registry_ref, Command};
fn reg() -> &'static [Command] { registry_ref() }
use serde_json::{json, Value};

pub struct Group { pub name: &'static str, pub about: &'static str }
pub const GROUPS: &[Group] = &[
    Group { name: "project", about: "Project settings, media import, save/load, undo/redo, inspect the project." },
    Group { name: "timeline", about: "Tracks and clips: add, move, trim, split, ripple/roll/slip/slide, speed, markers, multicam, silence cuts." },
    Group { name: "animate", about: "Properties, keyframes and curves, expressions, motion paths and presets, camera, blend, corner/mesh pin, physics, motion blur, beat binding." },
    Group { name: "effects", about: "Video effects, transitions, adjustment layers, effect masks and effect presets." },
    Group { name: "masks", about: "Masks, roto, track mattes, matte refinement and keying." },
    Group { name: "text", about: "Text, titles, text animators and paths, captions, subtitles, fonts, transcription." },
    Group { name: "graphics", about: "Shapes, bezier paths, SVG import, charts from CSV, the bundled icon/character/map library, shape modifiers, layer styles, HUD frames, counters." },
    Group { name: "vfx3d", about: "3D scenes, code clips (three.js / web pages), lights and materials, relight, particles, shard bursts and fracture glass." },
    Group { name: "tracking", about: "Point/planar/mesh/feature tracking, stabilize, camera solve, lens undistort, track validation and diagnostics." },
    Group { name: "color_grade", about: "Grading: primaries, curves, wheels, node graphs, LUTs, CDL, match grade, white balance, shared grades." },
    Group { name: "color_science", about: "Color pipeline: ACES, OCIO, HDR, float and high bit depth, color management, delivery checks and film response." },
    Group { name: "scopes", about: "Scopes and measurement: waveform/vectorscope, exposure analysis, false color, gamut checks, loudness." },
    Group { name: "audio", about: "Audio levels, effects, pan, ducking, loudness normalize, beat detection, audio export." },
    Group { name: "output", about: "Render a frame or export video, GIF, alpha, presets, EXR/TIFF, LUTs, camera solves." },
    Group { name: "extras", about: "Tools not yet assigned to a group (should stay empty)." },
];

pub fn group_of(n: &str) -> &'static str {
    if n.starts_with("proxy_") { return "project"; }
    const EXACT: &[(&str, &[&str])] = &[
        ("project", &["new_project", "timeline_summary", "set_meta", "find", "project_diff", "project_merge", "batch", "recipe_list", "recipe_describe", "recipe_apply", "template_define", "template_apply", "template_list", "template_describe", "template_delete", "export_template_package", "import_template_package", "project_set", "import_media", "save_project", "load_project", "undo", "redo", "history_list", "checkpoint", "restore", "checkpoint_delete", "get_project", "ext_list", "kimo_info", "check_environment", "cache_stats", "cache_clear", "cache_set_budget", "list_dependencies", "collect_project", "relink_media", "set_limits", "get_limits", "import_pixels", "read_pixels", "create_bin", "list_bins", "rename_bin", "delete_bin", "move_asset_to_bin", "find_assets", "create_subclip", "list_subclips", "delete_subclip", "add_clip_from_subclip", "set_essential_properties", "list_essential_properties", "clear_essential_properties", "register_hook", "list_hooks", "remove_hook", "run_hooks", "import_edl", "export_edl", "list_project_templates", "create_project_from_template", "import_clips", "project_stats", "organize_bins", "batch_set_meta"]),
        ("tracking", &["auto_reframe", "fit_info", "camera_project_points", "camera_clip_triangle", "set_ground_plane", "place_at_track", "export_st_map"]),
        ("output", &["review", "look", "watch", "compare_reference", "lint_edit", "validate_timeline", "self_review", "apply_review_fixes", "explain_cut", "review_export", "render_frame", "preview_range", "preview_stream", "session_stats", "lint_frame", "contact_sheet", "diff_frames", "compare_ranges", "edit_check", "saliency_map", "detect", "frozen_frames", "black_frames", "focus_series", "detect_cuts", "match_events", "describe_frame", "pick_thumbnail", "score_windows", "export_manifest", "explain_invalidation", "export", "export_job_start", "job_status", "cancel_job", "resume_job", "join_export", "export_alpha", "export_gif", "export_audio", "export_preset", "inspect_export_preset", "export_with_subtitles", "burn_in", "export_camera_solve", "export_grade_lut", "export_asc_cdl", "export_color_pipeline_lut", "set_ocio_view_delivery", "export_ocio_view_video", "export_high_bit", "export_aces2_reference_video", "render_float_tiff", "render_float_exr", "render_high_bit_still", "render_ocio_display_view", "render_aces2_reference", "compute_text_bounds", "query_safe_margins", "solve_constraints", "mux_chapters", "auto_chapters", "gpu2d_status", "gpu_gaussian_blur_frame", "gpu_color_matrix_frame"]),
        ("effects", &["remove_transition", "add_morph_cut", "add_lens_blur", "add_depth_parallax", "add_push_wipe", "add_displacement_map", "add_channel_combiner", "add_set_channels", "add_match_grain", "add_blur_fill", "sample_field", "sample_noise", "list_effects", "add_effect", "set_effect_param", "remove_effect", "list_transitions", "add_transition", "set_effect_mask", "add_adjustment_layer", "set_echo", "set_posterize_time", "add_speed_lines", "add_glitch_confetti", "add_managed_linear_effect", "set_effect_encoding", "effect_describe", "add_droste_zoom", "add_portal_mirror", "add_vertigo_zoom", "add_subject_clone", "add_melt_drip", "apply_vfx_preset", "add_vfx_graph", "vfx_graph_describe", "vfx_preview"]),
        ("masks", &["set_mask", "set_mask_keyframe", "remove_mask", "add_mask", "update_mask", "remove_mask_layer", "reorder_mask", "list_masks", "set_mask_prop_key", "set_mask_motion_blur", "set_mask_path_key", "roto_propagate", "track_mask", "set_track_matte", "set_matte", "remove_track_matte", "track_matte", "render_mask", "refine_matte"]),
        ("text", &["add_text", "set_text", "set_text_style", "add_text_animator", "set_animator_keyframe", "add_title", "add_keyword_pops", "set_text_weight", "set_text_path", "list_fonts", "import_srt", "add_captions", "add_caption_cues", "list_captions", "set_caption_style", "style_captions_batch", "edit_caption", "transcribe", "transcript_import", "transcript_get", "transcript_edit_word", "transcript_set_take", "transcript_coverage", "transcript_to_captions", "validate_captions", "transcript_edit_preview", "auto_captions", "edit_caption_words", "export_captions", "caption_from_words"]),
        ("graphics", &["add_shape", "set_shape_size", "add_pip", "add_arrow", "add_path_shape", "set_path", "path_info", "add_path_key", "clear_path_keys", "add_shape_modifier", "add_path_operator", "list_shape_modifiers", "remove_shape_modifier", "add_modifier", "list_modifiers", "remove_modifier", "list_layer_styles", "set_layer_style", "remove_layer_style", "add_box_expand", "add_detection_box", "add_hud_frame", "add_counter", "add_prompt_box", "add_watermark", "burn_in_timecode", "import_svg", "inspect_svg", "recolor_svg_layer", "draw_on_svg_layer", "add_chart", "search_assets", "add_asset", "export_asset_svg"]),
        ("vfx3d", &["add_code_clip", "set_code_clip_param", "check_code_clip_determinism", "inspect_threed_backend", "inspect_mesh_buffers", "inspect_scene_matrices", "raycast_mesh_raw", "sample_sdf_raw", "sample_pbr_raw", "sample_diffuse_sh_raw", "render_mesh_cpu_reference", "import_mesh_raw", "add_scene3d", "add_shard_burst", "add_particles", "add_emitter", "add_particles3d", "set_particles3d", "shards_project", "set_light", "light_rig", "remove_light", "list_lights", "set_material", "list_materials", "remove_material", "set_relight", "preview_material", "simulate_physics", "generate_mesh_primitive", "inspect_mesh_primitive", "render_mesh_camera_pass", "ingest_vertex_stream", "inspect_vertex_stream", "derive_vertex_tangents", "export_vertex_stream_obj", "export_vertex_stream_gltf", "compile_scene3d_descriptor", "render_scene3d_cpu", "render_scene3d_wgpu", "sample_scene3d_animation", "render_scene3d_sequence", "sample_direct_lighting", "render_vertex_stream_pbr", "query_flat_sah", "inspect_sdf_graph", "sample_sdf_graph", "contour_sdf_graph", "transform_vertex_stream", "instantiate_gltf_scene", "render_offscreen_mesh_normal", "sample_texture_material", "import_gltf_surface", "render_textured_bvh", "render_wgpu_pbr", "sample_advection_field", "render_particle_flow", "render_particle_flow_motion", "mesh_lathe", "mesh_extrude_path", "mesh_subdivide", "mesh_boolean", "set_3d_compositor", "set_3d_layer_shadows"]),
        ("color_science", &["evaluate_hlg_reference_display"]),
        ("color_grade", &["preview_subject_match", "set_color_wheels", "pick_key_color", "auto_color", "match_shots", "auto_grade", "match_shot_look", "check_color_consistency"]),
        ("scopes", &["get_scopes", "render_scope", "render_display_scope", "analyze_exposure", "analyze_exposure_sequence", "render_exposure_false_color", "render_gamut_excursions", "measure_loudness", "find_by_loudness", "analyze_color", "inspect_grade_scopes", "set_grade_scopes", "flash_series", "contrast_ratio", "text_contrast_series"]),
        ("audio", &["finish_audio", "set_audio_effect_key", "read_samples", "import_samples", "audio_filter_response", "audio_effect_describe", "audio_speech_intervals", "audio_transients", "audio_probe", "audio_spectrum", "set_audio", "set_audio_volume_key", "set_audio_pan", "set_audio_pan_key", "add_audio_effect", "remove_audio_effect", "normalize_loudness", "batch_normalize_loudness", "auto_duck", "detect_beats", "av_offset", "add_audio_clip"]),
        ("animate", &["set_property", "set_keyframe", "list_animated_properties", "set_keyframe_bezier", "audio_to_keyframes", "add_punch_ins", "add_smart_punch", "set_expression", "link_property", "get_curve", "move_key", "remove_key", "copy_curve", "simplify_curve", "smooth_curve", "set_motion_path", "remove_motion_path", "apply_motion_preset", "set_motion_blur", "set_flow_blur", "render_flow", "add_camera_shake", "motion_extract", "motion_diff", "bind_beats", "track_beats", "set_corner_pin", "set_mesh_pin", "set_blend", "set_camera", "set_camera_keyframe", "set_keyframes", "sample_property", "bake_property", "add_null", "set_parent", "get_transform", "animate_from_data"]),
        ("timeline", &["auto_edit", "auto_rough_cut", "place_broll", "cut_to_beat", "cut_to_speech", "add_track", "add_clip", "move_clip", "trim_clip", "split_clip", "delete_clip", "paste_attributes", "copy_keyframes", "set_speed", "set_speed_ramp", "set_time_remap", "beat_cut", "set_time_mode", "freeze_frame", "ripple_delete", "close_gaps", "reframe_timeline", "drop_short_segments", "ripple_trim", "roll_edit", "slip_clip", "set_split_edit", "slide_clip", "replace_clip_media", "insert_gap", "lift_range", "duplicate_clip", "loop_clip", "distribute_clips", "swap_clips", "rate_stretch", "set_track", "delete_track", "split_all_at", "scene_split", "remove_silence_cuts", "trim_filler_words", "tighten_pauses", "fit_clip_duration", "group_clips", "ungroup_clip", "add_marker", "list_markers", "remove_marker", "mark_scene_cuts", "snap_time", "list_edit_points", "add_multicam", "switch_angle", "list_multicam", "flatten_multicam", "sync_clips", "mark_source", "clear_source_marks", "get_source_marks", "edit_points", "apply_responsive_time", "make_montage_from_folder", "best_segments", "beat_sync_montage", "montage_color_match"]),
    ];
    if n.starts_with("transcript_") { return "text"; }
    // batches 5-12: tools whose names the keyword rules below would misplace or miss
    match n {
        "set_track_mix" | "remix_music" | "loudness_report" | "export_audio_as" | "music_structure" | "separate_stems" | "add_sfx" | "add_sfx_on" => return "audio",
        "copy_ease" | "key_graph" | "set_key_velocity" => return "animate",
        "set_animator_selectors" => return "text",
        "insert_edit" | "overwrite_edit" | "timecode" | "import_timeline" | "import_aaf" | "export_timeline" | "export_aaf" | "export_ale" | "export_markers" => return "timeline",
        "set_autosave" | "list_autosaves" | "recover_autosave" => return "project",
        "import_lottie" | "export_lottie" => return "graphics",
        _ => {}
    }
    for (g, names) in EXACT { if names.contains(&n) { return g; } }
    // rules for tools added later: the first match wins
    let has = |p: &[&str]| p.iter().any(|x| n.contains(x));
    if has(&["track_", "_track", "stabiliz", "camera_solve", "solve_camera", "undistort", "homography", "jitter", "residual", "pin_layer", "planar", "corner_disagreement", "error_field", "lk_", "ecc_"]) { return "tracking"; }
    if has(&["scope", "exposure", "gamut", "loudness"]) { return "scopes"; }
    if has(&["ocio", "aces", "hdr", "float", "high_bit", "color_pipeline", "color_metadata", "color_delivery", "film_response", "linear", "color_output", "color_frame", "input_color", "delivery_contract"]) { return "color_science"; }
    if has(&["grade", "color", "cdl", "lut", "node", "white_balance", "curve"]) { return "color_grade"; }
    if has(&["audio", "beat", "duck"]) { return "audio"; }
    if has(&["caption", "text", "title", "subtitle", "font", "srt"]) { return "text"; }
    if has(&["mask", "matte", "roto", "key"]) { return "masks"; }
    if has(&["scene3d", "light", "material", "particle", "emitter", "shard", "fracture", "relight", "glass"]) { return "vfx3d"; }
    if has(&["shape", "path", "hud", "layer_style", "modifier"]) { return "graphics"; }
    if has(&["effect", "transition", "adjustment"]) { return "effects"; }
    if n.starts_with("export") || n.starts_with("render") { return "output"; }
    if has(&["keyframe", "expression", "motion", "camera", "blend"]) { return "animate"; }
    if has(&["clip", "marker", "multicam", "ripple", "speed"]) { return "timeline"; }
    "extras"
}

fn first_sentence(d: &str) -> String { let e = d.find(". ").map(|i| i + 1).unwrap_or(d.len()); let s = &d[..e]; if s.len() > 150 { format!("{}...", &s[..s.char_indices().map(|c| c.0).take_while(|&i| i <= 147).last().unwrap_or(0)]) } else { s.to_string() } }

pub fn members(group: &str) -> Vec<&'static Command> { reg().iter().filter(|c| group_of(c.name) == group).collect() }

/// Short text for the compact list: cut at a word boundary so the whole description stays within `max` chars.
fn short(d: &str, max: usize) -> String { if d.len() <= max { return d.to_string(); } let cut = d[..max].rfind(' ').unwrap_or(max); format!("{}...", d[..cut].trim_end_matches(|c| c == ',' || c == ';')) }

/// Compact grouped tools/list (RV-23): one entry per non-empty group with a short description and the member count, and no action names (they cost 2x
/// 300-700 bytes per group: once in the description, once as the schema enum). The AI finds actions with search_tools or kimo_describe(group).
pub fn grouped_list() -> Vec<Value> { profile_list("grouped", &std::collections::HashSet::new()) }

/// The everyday actions of each group, named in its tools/list entry so the usual calls need no search or describe first.
pub const COMMON: &[(&str, &[&str])] = &[
    ("project", &["import_media", "project_set", "timeline_summary", "save_project", "undo"]),
    ("timeline", &["add_clip", "trim_clip", "split_clip", "move_clip", "ripple_delete", "auto_rough_cut"]),
    ("animate", &["set_property", "set_keyframe", "apply_motion_preset", "add_punch_ins"]),
    ("effects", &["add_effect", "add_transition", "list_effects", "add_adjustment_layer"]),
    ("masks", &["add_mask", "set_track_matte", "roto_propagate", "refine_matte"]),
    ("text", &["add_text", "add_title", "auto_captions", "set_caption_style"]),
    ("graphics", &["add_shape", "add_arrow", "add_asset", "add_chart"]),
    ("vfx3d", &["add_particles", "add_scene3d", "add_code_clip", "light_rig"]),
    ("tracking", &["stabilize", "track_point", "track_planar", "auto_reframe"]),
    ("color_grade", &["auto_grade", "match_shots", "set_color_wheels", "pick_white_balance"]),
    ("color_science", &["set_color_pipeline", "inspect_color_pipeline", "set_hdr_mastering"]),
    ("scopes", &["get_scopes", "render_scope", "measure_loudness", "analyze_exposure"]),
    ("audio", &["set_audio", "finish_audio", "auto_duck", "normalize_loudness"]),
    ("output", &["export", "render_frame", "review", "look", "export_preset"]),
];
fn group_entry(g: &Group) -> Value { let n = members(g.name).len();
    let common = COMMON.iter().find(|(x, _)| *x == g.name).map(|(_, a)| format!(" e.g. {}.", a.join(", "))).unwrap_or_default();
    json!({"name": g.name, "description": format!("{} [{} actions]{common}", short(g.about, 100), n), "inputSchema": {"type":"object","properties":{"action":{"type":"string","description":"the action (tool) name"},"args":{"type":"object","description":"its arguments; may also be given flat next to action"}},"required":["action"]}}) }
fn tool_entry(c: &Command) -> Value { json!({"name": c.name, "description": c.description, "inputSchema": c.schema}) }
fn meta_tools(with_enable: bool) -> Vec<Value> {
    let mut v = vec![json!({"name":"search_tools","description":"Find the tool for a need, in plain words (\"make the music quieter when people talk\", \"slow motion\"; typos are fine). Ranked tools, effects and transitions; the top tools come with their argument list, so you can call one straight away.","inputSchema":{"type":"object","properties":{"query":{"type":"string"},"limit":{"type":"integer"},"group":{"type":"string"}},"required":["query"]}}),
        json!({"name":"kimo_describe","description":"Full description and argument schema of one action (action), the argument lists of several at once (actions: [..]), or the action list of a group (group).","inputSchema":{"type":"object","properties":{"action":{"type":"string"},"actions":{"type":"array","items":{"type":"string"}},"group":{"type":"string"}}}})];
    if with_enable { v.push(json!({"name":"kimo_enable_group","description":"List every tool of a group individually from now on (the server sends notifications/tools/list_changed).","inputSchema":{"type":"object","properties":{"name":{"type":"string"}},"required":["name"]}})); }
    v }

/// The ~25 most used tools, listed with full schemas in the `core` profile.
pub const CORE: &[&str] = &["project_set", "import_media", "save_project", "load_project", "undo", "redo", "get_project", "add_track", "add_clip", "move_clip", "trim_clip", "split_clip",
    "delete_clip", "set_property", "set_keyframe", "add_effect", "set_effect_param", "add_transition", "add_text", "set_text_style", "add_shape", "add_mask", "set_blend", "set_audio", "render_frame", "export"];

/// The `director` profile (KIMO_TOOLS=director): the short list a small model needs to cut phone footage into a short, each
/// with a one-line description written for that job. Registry descriptions stay as they are; every other tool is still
/// callable by name and found with search_tools.
pub const DIRECTOR: &[(&str, &str)] = &[
    ("project_set", "Canvas and frame rate. A 9:16 short: {width: 720, height: 1280, fps: 30}. Call first."),
    ("import_media", "Import a video or audio file by absolute path. Returns asset_id, duration, width, height."),
    ("auto_rough_cut", "Talking-head clips back to back with dead air cut out. Leave out a false start with drop [{asset_id, index}]. Returns ids."),
    ("place_broll", "B-roll over the dialogue only while someone speaks, cut on pauses, b-roll muted. Returns ids."),
    ("cut_to_beat", "Music reel: lays the music and cuts the shots on its beats. Returns ids."),
    ("stabilize", "Steady a shaky media clip. Call after its in_point and duration are final."),
    ("auto_grade", "Fix exposure and colour cast per clip (dark, blown, blue or orange shots). Default: every clip."),
    ("add_title", "Title card: {track_id, style: \"impact\", text, start, duration}. Put it on its own top track."),
    ("add_text", "Text clip: {track_id, text, start, duration, size}. Move it with set_property y (pixels from centre, + is down)."),
    ("caption_from_words", "Captions from the words you give, timed to the speech, clear of the TikTok/Reels buttons. Best: by_asset {asset_id: words}."),
    ("auto_captions", "Captions from kimo's own speech recognition: {clip_ids: [...rough-cut segments...]}. Use with no trustworthy transcript. Flags low-confidence words."),
    ("edit_caption", "Change one caption cue's text: {clip_id, text}."),
    ("finish_audio", "Final mix in one call: clean dialogue, b-roll sound off, music under speech, -14 LUFS. Call before export."),
    ("remove_transition", "Take a transition off a clip: {clip_id}. lint_edit names it when a dissolve fades in from black."),
    ("look", "Look closely at one frame: every visible clip outlined at its exact box with its id, safe areas, motion paths; optional alpha and scopes."),
    ("watch", "See motion in one picture: onion skin with each element's path shaded by speed, plus speed/ease/jitter numbers and a verdict."),
    ("compare_reference", "Compare part of your edit with a reference video or another part of the timeline: colour, motion energy, SSIM, and what to change."),
    ("review", "Judge the whole edit before export: every mistake found (cuts, transitions, text, motion, frames), worst first, each with the exact call that fixes it. Every edit also reports its own findings in its `observe` result."),
    ("lint_edit", "Check the edit before export. Lists each problem with the exact call that fixes it. Fix, then run it again."),
    ("timeline_summary", "The edit at a glance: tracks bottom to top, each clip's id, source, start and end."),
    ("add_track", "New track on top (higher tracks draw over lower ones). Returns track_id."),
    ("add_clip", "One media clip: {track_id, source_type: \"media\", asset_id, start, duration, in_point, fit: \"cover\"}."),
    ("trim_clip", "Change a clip's in_point and/or duration."),
    ("move_clip", "Move a clip to a new start time."),
    ("delete_clip", "Remove a clip."),
    ("set_property", "Set x, y, scale, opacity or rotation on a clip (x/y in pixels from centre)."),
    ("set_audio", "A clip's sound: mute, volume (1 = unchanged), fade_in, fade_out."),
    ("render_frame", "Render the edit at one time to a .png and look at it."),
    ("contact_sheet", "Several times of the edit in one .png grid you can look at."),
    ("export", "Render the edit to an .mp4 at path (H.264, AAC). Returns duration and bytes (check bytes against a size cap)."),
    ("review_export", "Look at any video file, your export or a source clip: a grid of its frames, duration, size, loudness."),
    ("save_project", "Save the project JSON to path."),
    ("undo", "Undo the last change. A recipe call (auto_*, place_broll, finish_audio) is one step."),
    ("add_smart_punch", "Automatic subject-follow punch-in: a continuous zoom that centres on wherever the saliency map says the eye goes. {clip_id, zoom}."),
    ("add_glow_text", "A title clip with a pulsing neon glow pre-applied: one call for the caption/hook look. {track_id, text, start, duration}."),
    ("auto_reframe", "Crop a clip to the canvas aspect (e.g. 16:9 into 9:16), following its subject. The fix lint_edit names for aspect_mismatch."),
    ("add_punch_ins", "Jump-cut punch-ins: alternate a track's clips between wide and zoomed around a focus point (talking-head style)."),
];

/// Profile names: grouped (default), flat, core, director, or a comma list of group names to expand (the other groups stay collapsed). Unknown text falls back to grouped.
pub fn parse_profile(p: &str) -> String { let p = p.trim().to_lowercase(); if matches!(p.as_str(), "" | "grouped" | "flat" | "core" | "director") { return if p.is_empty() { "grouped".into() } else { p }; }
    let ok: Vec<&str> = p.split(',').map(|x| x.trim()).filter(|x| GROUPS.iter().any(|g| g.name == *x && !members(g.name).is_empty())).collect(); if ok.is_empty() { "grouped".into() } else { ok.join(",") } }

/// tools/list for a profile. `enabled` holds groups switched on with kimo_enable_group (their tools are listed individually, their group entry is dropped).
pub fn profile_list(profile: &str, enabled: &std::collections::HashSet<String>) -> Vec<Value> {
    let p = parse_profile(profile);
    match p.as_str() {
        "flat" => { let mut v: Vec<Value> = reg().iter().map(tool_entry).collect(); v.extend(meta_tools(false)); v }
        "director" => { let mut v: Vec<Value> = DIRECTOR.iter().filter_map(|(n, d)| reg().iter().find(|c| c.name == *n).map(|c| json!({"name": c.name, "description": d, "inputSchema": c.schema}))).collect(); v.extend(meta_tools(false)); v }
        "core" => { let mut v: Vec<Value> = CORE.iter().filter_map(|n| reg().iter().find(|c| c.name == *n)).map(tool_entry).collect(); v.extend(meta_tools(true)); for g in GROUPS { if enabled.contains(g.name) { v.extend(members(g.name).into_iter().filter(|c| !CORE.contains(&c.name)).map(tool_entry)); } } v }
        _ => { let mut expand: std::collections::HashSet<String> = enabled.clone(); if p != "grouped" { for g in p.split(',') { expand.insert(g.to_string()); } }
            let mut v = vec![]; for g in GROUPS { let m = members(g.name); if m.is_empty() { continue; } if expand.contains(g.name) { v.extend(m.into_iter().map(tool_entry)); } else { v.push(group_entry(g)); } }
            v.extend(meta_tools(true)); v }
    }
}

fn required_args(c: &Command) -> Vec<String> { c.schema.get("required").and_then(|r| r.as_array()).map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default() }

/// search_tools: one call from a plain-language need to a callable tool. Tools, effects and transitions are ranked on one
/// scale (tool_index); the first few tools carry their argument signature, so the usual next step is the call itself.
pub fn search_tools(q: &str, limit: usize, group: Option<&str>, effects: &Value, transitions: &Value) -> Value {
    use crate::tool_index as ti;
    let limit = limit.clamp(1, 25);
    let ranked = ti::rank(q); let total = ranked.len();
    let mut items: Vec<(f64, Value)> = vec![];
    for (name, sc) in ranked.iter().take(60) {
        if let Some(g) = group { if group_of(name) != g { continue; } }
        let Some(c) = reg().iter().find(|c| c.name == *name) else { continue };
        let req = required_args(c);
        items.push((*sc, json!({"type":"tool","name":name,"group":group_of(name),"summary":first_sentence(c.description),"required":req,"call_as":format!("{}({})", name, req.join(", "))})));
    }
    if group.map_or(true, |g| g == "effects") {
        // a catalog entry is one step further away (it is used through add_effect / add_transition), so it ranks a little below an equal tool
        for (ty, kind, d, sc) in ti::rank_catalog(q, effects, transitions).into_iter().take(8) {
            let (tool, extra) = if ty == "transition" { ("add_transition", ", duration") } else { ("add_effect", "") };
            items.push((sc * 0.85, json!({"type":ty,"name":kind,"group":"effects","summary":first_sentence(&d),"call_as":format!("{tool}(clip_id{extra}, kind=\"{kind}\")")})));
        }
    }
    items.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut out: Vec<Value> = items.into_iter().take(limit).map(|x| x.1).collect();
    let mut shown = 0;
    for m in out.iter_mut() { if shown < 3 && m["type"] == "tool" { if let Some(sig) = ti::signature_of(m["name"].as_str().unwrap_or("")) { m["args"] = json!(sig); } shown += 1; } }
    json!({"matches": out, "total": total, "how": "call a group tool with {action: name, args: {...}} (args may also sit flat next to action), or the name directly; effects and transitions go through add_effect / add_transition with kind"})
}

/// Resolve a grouped call to (tool name, args). Err carries a message for the client.
/// Forgiving on purpose: an agent's call means one thing even when its shape is a little off, so it runs instead of
/// costing a round trip. Action names are unique across groups, so a call sent to the wrong group still resolves; args may
/// sit under "args", flat next to "action", or both (args wins), or arrive as a JSON string. Only a call that cannot be
/// understood fails, and its error names the closest actions with the argument list of the best one.
pub fn resolve_group_call(group: &str, args: &Value) -> Result<(String, Value), String> {
    let Some(action) = args.get("action").and_then(|v| v.as_str()) else {
        let common = COMMON.iter().find(|(g, _)| *g == group).map(|(_, a)| a.join(", ")).unwrap_or_default();
        return Err(format!("group '{group}' needs 'action', the tool to run (common: {common}). kimo_describe {{group: \"{group}\"}} lists all {}, search_tools finds one by need.", members(group).len()));
    };
    if reg().iter().all(|c| c.name != action) {
        let near = crate::tool_index::near_names(action, 3);
        let sig = near.first().and_then(|n| crate::tool_index::signature_of(n)).map(|s| format!(" {}({})", near[0], s.join("; "))).unwrap_or_default();
        return Err(format!("unknown action '{action}'. Closest: {}.{sig}", near.join(", ")));
    }
    let mut a = match args.get("args") {
        Some(Value::String(t)) => serde_json::from_str::<Value>(t).ok().filter(|v| v.is_object()).unwrap_or_else(|| json!({})),
        Some(v) if v.is_object() => v.clone(),
        _ => json!({}),
    };
    if let (Some(top), Some(inner)) = (args.as_object(), a.as_object_mut()) { for (k, v) in top { if k != "action" && k != "args" && !inner.contains_key(k) { inner.insert(k.clone(), v.clone()); } } }
    Ok((action.to_string(), a))
}

pub fn is_group(n: &str) -> bool { GROUPS.iter().any(|g| g.name == n && !members(n).is_empty()) }

pub fn search(q: &str, limit: usize) -> Value {
    // ranked by tool_index (BM25F over name, argument names and description, read with the editing vocabulary)
    let hits = crate::tool_index::rank(q);
    json!({"matches": hits.iter().take(limit.clamp(1, 50)).filter_map(|(n, _)| reg().iter().find(|c| c.name == *n)).map(|c| json!({"group": group_of(c.name), "action": c.name, "summary": first_sentence(c.description)})).collect::<Vec<_>>(), "total": hits.len()})
}

pub fn describe(args: &Value) -> Result<Value, String> {
    // several at once: plan a multi-step edit with one call instead of one per tool
    if let Some(list) = args.get("actions").and_then(|v| v.as_array()) {
        let out: Vec<Value> = list.iter().filter_map(|x| x.as_str()).map(|a| match reg().iter().find(|c| c.name == a) {
            Some(c) => json!({"action": c.name, "group": group_of(c.name), "summary": first_sentence(c.description), "mutates": c.mutates, "args": crate::tool_index::signature(c)}),
            None => json!({"action": a, "error": format!("unknown; closest: {}", crate::tool_index::near_names(a, 3).join(", "))}),
        }).collect();
        return Ok(json!({"actions": out}));
    }
    if let Some(a) = args.get("action").and_then(|v| v.as_str()) {
        let c = reg().iter().find(|c| c.name == a).ok_or_else(|| format!("unknown action '{a}'; closest: {}", crate::tool_index::near_names(a, 3).join(", ")))?;
        return Ok(json!({"group": group_of(c.name), "action": c.name, "description": c.description, "mutates": c.mutates, "args_schema": enrich(c.name, c.schema.clone()), "call": {"tool": group_of(c.name), "arguments": {"action": c.name, "args": "{...per args_schema}"}}}));
    }
    if let Some(g) = args.get("group").and_then(|v| v.as_str()) {
        let gi = GROUPS.iter().find(|x| x.name == g).ok_or_else(|| format!("unknown group '{g}'; groups: {}", GROUPS.iter().map(|x| x.name).collect::<Vec<_>>().join(", ")))?;
        return Ok(json!({"group": g, "about": gi.about, "actions": members(g).iter().map(|c| json!({"action": c.name, "summary": first_sentence(c.description)})).collect::<Vec<_>>()}));
    }
    Err("pass 'action' or 'group'".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn every_tool_has_a_real_group() { let bad: Vec<_> = reg().iter().filter(|c| group_of(c.name) == "extras").map(|c| c.name).collect(); assert!(bad.is_empty(), "tools without a group, add them to toolgroups.rs: {bad:?}"); }
    #[test] fn groups_partition_the_reg() { let n: usize = GROUPS.iter().map(|g| members(g.name).len()).sum(); assert_eq!(n, reg().len()); }
    #[test] fn group_call_resolves_forgivingly() {
        let (n, a) = resolve_group_call("project", &json!({"action":"project_set","args":{"width":640}})).unwrap(); assert_eq!(n, "project_set"); assert_eq!(a["width"], 640);
        // wrong group: names are unique, so it still means project_set
        assert_eq!(resolve_group_call("audio", &json!({"action":"project_set","args":{}})).unwrap().0, "project_set");
        // flat args, and both (args wins)
        let (_, a) = resolve_group_call("project", &json!({"action":"project_set","width":640})).unwrap(); assert_eq!(a["width"], 640);
        let (_, a) = resolve_group_call("project", &json!({"action":"project_set","width":1,"args":{"width":2,"height":3}})).unwrap(); assert_eq!(a["width"], 2); assert_eq!(a["height"], 3);
        // args as a JSON string
        let (_, a) = resolve_group_call("project", &json!({"action":"project_set","args":"{\"fps\":25}"})).unwrap(); assert_eq!(a["fps"], 25);
        // what cannot be understood fails with the way forward
        assert!(resolve_group_call("audio", &json!({})).unwrap_err().contains("set_audio"));
        let e = resolve_group_call("timeline", &json!({"action":"spilt_clip"})).unwrap_err(); assert!(e.contains("split_clip"), "{e}"); }
    #[test] fn common_actions_are_in_their_group() { for (g, a) in COMMON { for n in *a { assert_eq!(group_of(n), *g, "{n}"); assert!(reg().iter().any(|c| c.name == *n), "{n} missing"); } } }
    #[test] fn describe_many() { let r = describe(&json!({"actions":["split_clip","nope_clip"]})).unwrap(); assert!(r["actions"][0]["args"].as_array().is_some_and(|a| !a.is_empty())); assert!(r["actions"][1]["error"].is_string()); }
    // ceiling 7168 bytes (was 6144): each group now names its everyday actions, which saves a search_tools round trip on most calls
    #[test] fn grouped_list_is_small() { let l = grouped_list(); assert!(l.len() <= 20, "{}", l.len()); let bytes = serde_json::to_string(&l).unwrap().len(); assert!(bytes <= 7168, "default tools/list is {bytes} bytes, ceiling 7168"); }
    #[test] fn core_tools_exist() { for n in CORE { assert!(reg().iter().any(|c| c.name == *n), "core tool {n} missing"); } assert!(CORE.len() <= 26); }
}

/// Effects and transitions are catalogs reached through add_effect / add_transition, so tool search alone cannot find them.
/// This matches a query against those catalogs (kind, description, parameter names) and says how to use each hit.
pub fn search_catalog(q: &str, effects: &Value, transitions: &Value, limit: usize) -> Vec<Value> {
    let words: Vec<String> = q.split(|c: char| !c.is_alphanumeric()).filter(|w| w.len() > 2).map(|w| { let w = w.to_lowercase(); if w.len() > 4 && w.ends_with('s') { w[..w.len() - 1].to_string() } else { w } }).collect();
    let syn = |w: &str| -> Vec<&'static str> { match w { "green" | "greenscreen" | "screen" => vec!["chroma", "key"], "shatter" | "break" => vec!["fracture", "shard"], "slow" => vec!["speed"], _ => vec![] } };
    let mut out: Vec<(usize, Value)> = vec![];
    for e in effects.as_array().into_iter().flatten() {
        let kind = e["kind"].as_str().unwrap_or("").to_lowercase(); let d = e["description"].as_str().unwrap_or("").to_lowercase();
        let params: String = e["params"].as_array().into_iter().flatten().filter_map(|p| p["name"].as_str()).collect::<Vec<_>>().join(" ").to_lowercase();
        let mut sc = 0; for w in &words { let mut ws = vec![w.as_str()]; ws.extend(syn(w)); if ws.iter().any(|x| kind.contains(x)) { sc += 3; } else if ws.iter().any(|x| d.contains(x)) { sc += 2; } else if ws.iter().any(|x| params.contains(x)) { sc += 1; } }
        if sc > 0 { out.push((sc, json!({"type": "effect", "kind": e["kind"], "summary": e["description"], "use": format!("effects/add_effect with kind=\"{}\"", kind)}))); } }
    for t in transitions.as_array().into_iter().flatten() { let n = t.as_str().unwrap_or("").to_lowercase(); if words.iter().any(|w| n.contains(w.as_str())) { out.push((3, json!({"type": "transition", "kind": n, "use": format!("effects/add_transition with kind=\"{}\"", n)}))); } }
    out.sort_by(|a, b| b.0.cmp(&a.0)); out.into_iter().take(limit).map(|x| x.1).collect()
}

/// Fills in the shape of colour arguments, which many tools take as plain arrays with no description. Only facts verified in the
/// code are added (colours are [r,g,b] or [r,g,b,a] floats in 0..1); other untyped arguments are left as they are.
fn enrich(tool: &str, mut schema: Value) -> Value {
    if let Some(props) = schema.get_mut("properties").and_then(|p| p.as_object_mut()) {
        for (k, v) in props.iter_mut() {
            let is_color = k.contains("color") || matches!(k.as_str(), "accent" | "albedo" | "emissive");
            let note = match (tool, k.as_str()) {
                ("group_clips", "clip_ids") => Some("array of clip id strings; must not be empty"),
                ("add_mask" | "update_mask", "ngon") => Some("polygon or star for kind=path: {sides, inner, r, cx, cy, rot, aspect}; all optional (defaults sides 5, inner 0, r 0.3, cx 0.5, cy 0.5, rot 0, aspect 16/9)"),
                ("track_mesh", "corners") => Some("exactly 8 numbers: x0,y0,x1,y1,x2,y2,x3,y3 (an error is returned for any other length)"),
                ("match_grade", "source_region" | "ref_region") => Some("normalized sample rectangle [x, y, w, h], each 0..1 with x+w<=1 and y+h<=1; w,h > 0; default whole frame"),
                ("export_color_pipeline_lut" | "generate_lut_node", "domain_min" | "domain_max") => Some("3 numbers [r,g,b], the .cube DOMAIN_MIN / DOMAIN_MAX written to the LUT"),
                ("export_color_pipeline_lut" | "generate_lut_node", "steps") => Some("ordered step objects {kind, params:{name:number}, strings:{name:string}}; kind is one of the color effect names (e.g. cdl, lut, color_space, tonemap, exposure, saturation); unknown keys or kinds are rejected"),
                ("evaluate_hlg_reference_display", "scene_rgb") => Some("exactly 3 numbers [r,g,b], scene-linear"),
                ("add_multicam" | "sync_clips", "angles" | "clip_ids") => Some("array of clip id strings"),
                _ => None };
            if let Some(n) = note { if v.get("description").is_none() { v["description"] = json!(n); } }
            if is_color && v.get("type").and_then(|t| t.as_str()) == Some("array") && v.get("description").is_none() { v["description"] = json!("color [r,g,b] or [r,g,b,a], floats 0..1"); }
        }
    }
    schema
}
