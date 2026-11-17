//! Tool discovery benchmark: how often does search_tools put a right tool where an AI will see it?
//! Each case is a request worded the way an editor (or an agent relaying one) actually says it, with every tool that
//! would satisfy it. An effect or transition hit counts when the case accepts add_effect / add_transition, since that
//! is how catalog entries are used. Run with `--nocapture` to see the scores and the misses.
//! The floors below are the measured scores of this architecture; a ranking change may not push them back down.
use kimo_core::commands::{dispatch, Session};
use kimo_core::toolgroups::search_tools;
#[path = "support/old_search.rs"] mod old_search;
use serde_json::{json, Value};

pub const CASES: &[(&str, &[&str])] = &[
    ("make the music quieter when people talk", &["auto_duck", "duck_timeline", "finish_audio"]),
    ("turn the volume down on this clip", &["set_audio"]),
    ("slow motion", &["set_speed", "set_speed_ramp", "rate_stretch"]),
    ("speed up this clip 2x", &["set_speed", "rate_stretch", "set_speed_ramp"]),
    ("remove the green screen background", &["pick_key_color", "add_effect", "refine_matte"]),
    ("fix shaky footage", &["stabilize"]),
    ("add subtitles", &["auto_captions", "add_captions", "caption_from_words", "import_srt", "add_caption_cues"]),
    ("fade out the music at the end", &["set_audio", "set_audio_volume_key"]),
    ("fade to black", &["add_transition", "set_keyframe"]),
    ("zoom in slowly on the speaker", &["add_smart_punch", "add_punch_ins", "set_keyframe", "apply_motion_preset"]),
    ("make it vertical for tiktok", &["reframe_timeline", "auto_reframe", "project_set", "create_project_from_template"]),
    ("cut out the silences", &["remove_silence_cuts", "tighten_pauses", "auto_rough_cut", "audio_speech_intervals"]),
    ("remove ums and uhs", &["trim_filler_words"]),
    ("add a title at the start", &["add_title", "add_text"]),
    ("lower third with the speaker's name", &["add_title", "add_text", "recipe_apply", "recipe_list"]),
    ("make the colours warmer", &["add_effect", "set_color_wheels", "auto_grade", "pick_white_balance"]),
    ("black and white", &["add_effect"]),
    ("fix the white balance", &["pick_white_balance", "auto_color"]),
    ("match the colour of these two shots", &["match_color", "match_shots", "match_shot_look", "match_grade"]),
    ("blur someone's face", &["detect", "add_effect", "set_effect_mask", "track_mask"]),
    ("make text stick to a moving object", &["track_point", "track_planar", "pin_layer"]),
    ("cut to the beat of the music", &["cut_to_beat", "beat_cut", "beat_sync_montage"]),
    ("split the clip at the playhead", &["split_clip", "split_all_at"]),
    ("delete this clip and close the gap", &["ripple_delete"]),
    ("move the clip later", &["move_clip", "slide_clip"]),
    ("shorten the clip", &["trim_clip", "ripple_trim"]),
    ("export the video as mp4", &["export", "export_preset", "export_job_start"]),
    ("export a gif", &["export_gif"]),
    ("export versions for youtube and instagram", &["export_variants", "export_preset"]),
    ("check my edit for mistakes", &["review", "lint_edit", "self_review"]),
    ("show me what the frame looks like", &["render_frame", "look", "describe_frame"]),
    ("loudness normalize to -14 lufs", &["normalize_loudness", "batch_normalize_loudness", "finish_audio"]),
    ("remove background noise from the voice", &["set_audio", "add_audio_effect", "finish_audio"]),
    ("add background music", &["add_audio_clip", "import_media"]),
    ("import a video file", &["import_media"]),
    ("picture in picture", &["add_pip"]),
    ("add my logo as a watermark", &["add_watermark"]),
    ("freeze frame", &["freeze_frame", "set_time_mode"]),
    ("play the clip backwards", &["set_time_mode"]),
    ("crossfade between clips", &["add_transition"]),
    ("animate the text letter by letter", &["add_text_animator", "add_title"]),
    ("typewriter text", &["add_text_animator", "add_prompt_box"]),
    ("make text follow a curve", &["set_text_path"]),
    ("change the font", &["set_text", "set_text_style", "list_fonts"]),
    ("point an arrow at something", &["add_arrow"]),
    ("draw a box around the object", &["add_detection_box", "add_shape", "add_box_expand"]),
    ("falling snow", &["add_particles", "add_emitter", "add_particles3d"]),
    ("camera shake on impact", &["add_camera_shake"]),
    ("pick the best thumbnail", &["pick_thumbnail"]),
    ("find the scene changes", &["detect_cuts", "mark_scene_cuts", "scene_split"]),
    ("sync audio from two cameras", &["sync_clips", "add_multicam"]),
    ("multicam edit", &["add_multicam", "switch_angle"]),
    ("undo that", &["undo"]),
    ("save the project", &["save_project"]),
    ("make a montage from a folder of clips", &["make_montage_from_folder", "best_segments"]),
    ("turn talking head footage into a rough cut", &["auto_rough_cut", "auto_edit"]),
    ("put b-roll over the interview", &["place_broll"]),
    ("edit the video by deleting words from the transcript", &["transcript_edit_apply", "transcript_edit_preview"]),
    ("fix a misheard caption word", &["edit_caption_words", "edit_caption"]),
    ("make the captions bigger", &["set_caption_style", "style_captions_batch"]),
    ("keyframe the opacity", &["set_keyframe", "set_keyframes"]),
    ("ease in and out", &["set_keyframe_bezier", "apply_motion_preset"]),
    ("wiggle the position", &["add_modifier", "set_expression"]),
    ("glowing neon text", &["add_glow_text", "add_effect"]),
    ("vignette", &["add_effect"]),
    ("cinematic letterbox bars", &["add_effect"]),
    ("fix lens distortion", &["undistort_clip"]),
    ("show the waveform scope", &["render_scope", "get_scopes", "render_display_scope"]),
    ("is the shot too bright", &["analyze_exposure", "get_scopes", "auto_grade"]),
    ("add chapters to the video", &["auto_chapters", "mux_chapters", "add_marker"]),
    ("replace the footage in this clip", &["replace_clip_media"]),
    ("duplicate the clip", &["duplicate_clip"]),
    ("loop this clip three times", &["loop_clip"]),
    ("mute the music track", &["set_track", "set_audio"]),
    ("add a new track", &["add_track"]),
    ("change the resolution to 4k", &["project_set"]),
    ("animated bar chart from a csv", &["add_chart"]),
    ("add an emoji sticker", &["search_assets", "add_asset"]),
    ("dolly zoom", &["add_vertigo_zoom", "apply_vfx_preset"]),
    ("colour correct every clip automatically", &["auto_grade", "auto_color", "match_shots"]),
    ("what clips are in my timeline", &["timeline_summary", "get_project", "project_stats"]),
    ("j cut so the audio starts before the video", &["set_split_edit"]),
    ("remove the gaps between clips", &["close_gaps"]),
    ("animated number counting up", &["add_counter"]),
    ("motion blur", &["set_motion_blur", "set_flow_blur"]),
    ("rotoscope the person out", &["roto_propagate", "track_matte", "refine_matte", "add_mask"]),
    ("how long is my video", &["timeline_summary", "project_stats", "get_project"]),
    ("make the clip exactly 10 seconds long", &["rate_stretch", "fit_clip_duration", "trim_clip"]),
    ("reduce the echo in the voice", &["add_audio_effect", "set_audio"]),
    // typos and spelling variants
    ("stabilise the video", &["stabilize"]),
    ("add subtitels", &["auto_captions", "add_captions", "caption_from_words", "import_srt", "add_caption_cues"]),
    ("trasition between shots", &["add_transition"]),
    ("colour grade", &["auto_grade", "set_color_wheels", "set_color_curve", "add_effect", "match_shots"]),
];


/// Held out: written after the ranking was tuned on CASES and never used to tune it, so it shows how search does on
/// requests it has not seen. Reported, with a lower floor than the tuned set.
pub const HELD_OUT: &[(&str, &[&str])] = &[
    ("louder dialogue", &["set_audio", "normalize_loudness", "finish_audio", "batch_normalize_loudness"]),
    ("speed ramp into slow motion", &["set_speed_ramp", "set_time_remap"]),
    ("put the text in the middle of the screen", &["set_property", "add_text"]),
    ("make the intro title bounce in", &["apply_motion_preset", "add_title", "add_text_animator"]),
    ("add a zoom transition", &["add_transition"]),
    ("sharpen the footage", &["add_effect"]),
    ("remove the first two seconds", &["trim_clip", "ripple_trim", "lift_range", "ripple_delete"]),
    ("cut every clip at 10 seconds", &["split_all_at"]),
    ("spread the clips out evenly", &["distribute_clips"]),
    ("add a red rectangle", &["add_shape"]),
    ("film grain", &["add_effect", "add_match_grain"]),
    ("old vhs look", &["add_effect", "apply_vfx_preset"]),
    ("glitch effect", &["add_effect", "apply_vfx_preset", "add_glitch_confetti"]),
    ("hide the clip for a moment", &["set_keyframe", "set_property", "set_track"]),
    ("lock the video track", &["set_track"]),
    ("get the audio levels of the final mix", &["audio_probe", "measure_loudness"]),
    ("save a backup before grading", &["checkpoint", "save_project"]),
    ("go back to my saved version", &["restore", "undo", "load_project"]),
    ("auto edit my vacation clips", &["auto_edit", "make_montage_from_folder", "best_segments"]),
    ("caption style like tiktok", &["set_caption_style", "style_captions_batch", "auto_captions"]),
    ("translate the captions into srt file", &["export_captions"]),
    ("music only between 10 and 20 seconds", &["add_audio_clip", "trim_clip", "set_audio"]),
    ("blur the background behind the speaker", &["add_effect", "add_blur_fill", "set_effect_mask", "add_lens_blur"]),
    ("speaker's face is too dark", &["auto_grade", "set_color_wheels", "analyze_exposure", "add_effect"]),
    ("make a 15 second teaser", &["fit_clip_duration", "auto_edit", "best_segments", "rate_stretch"]),
    ("snap the cut to the nearest beat", &["snap_time", "bind_beats", "beat_cut", "detect_beats"]),
    ("check the text is readable", &["text_contrast_series", "contrast_ratio", "review", "lint_edit"]),
    ("place a sticker that follows the dog", &["track_point", "add_asset", "pin_layer"]),
    ("organise my media into folders", &["organize_bins", "create_bin", "move_asset_to_bin"]),
    ("stabalize clip", &["stabilize"]),
];

pub struct Score { pub top1: usize, pub top3: usize, pub top8: usize, pub mrr: f64, pub misses: Vec<String> }

type Search = fn(&str, usize, Option<&str>, &Value, &Value) -> Value;
pub fn score(limit: usize) -> Score { score_on(CASES, limit) }
pub fn score_on(cases: &[(&str, &[&str])], limit: usize) -> Score { score_with(search_tools, cases, limit) }
fn score_with(search: Search, cases: &[(&str, &[&str])], limit: usize) -> Score {
    let mut ss = Session::new();
    let ef = dispatch(&mut ss, "list_effects", &json!({})).unwrap_or(json!([]));
    let tr = dispatch(&mut ss, "list_transitions", &json!({})).unwrap_or(json!([]));
    let mut s = Score { top1: 0, top3: 0, top8: 0, mrr: 0.0, misses: vec![] };
    for (q, want) in cases {
        let r = search(q, limit, None, &ef, &tr);
        let hits: Vec<&Value> = r["matches"].as_array().map(|a| a.iter().collect()).unwrap_or_default();
        let ok = |m: &Value| { let n = m["name"].as_str().unwrap_or(""); let ty = m["type"].as_str().unwrap_or("tool");
            want.contains(&n) || (ty == "effect" && want.contains(&"add_effect")) || (ty == "transition" && want.contains(&"add_transition")) };
        match hits.iter().position(|m| ok(m)) {
            Some(i) => { if i == 0 { s.top1 += 1; } if i < 3 { s.top3 += 1; } if i < 8 { s.top8 += 1; } s.mrr += 1.0 / (i as f64 + 1.0); if i >= 3 { s.misses.push(format!("#{} {q:?} -> {:?}", i + 1, hits.iter().take(3).map(|m| m["name"].as_str().unwrap_or("")).collect::<Vec<_>>())); } }
            None => s.misses.push(format!("MISS {q:?} -> {:?}", hits.iter().take(3).map(|m| m["name"].as_str().unwrap_or("")).collect::<Vec<_>>())),
        }
    }
    s.mrr /= cases.len() as f64;
    s
}

#[test]
fn tool_discovery_benchmark() {
    let n = CASES.len();
    let s = score(8);
    let pct = |x: usize| 100.0 * x as f64 / n as f64;
    println!("tool discovery, {n} requests: top-1 {:.0}%  top-3 {:.0}%  top-8 {:.0}%  MRR {:.3}", pct(s.top1), pct(s.top3), pct(s.top8), s.mrr);
    for m in &s.misses { println!("  {m}"); }
    // floors: the measured score of the current search, so a later change can only keep or raise it
    assert!(pct(s.top1) >= FLOOR_TOP1, "top-1 fell to {:.0}%", pct(s.top1));
    assert!(pct(s.top3) >= FLOOR_TOP3, "top-3 fell to {:.0}%", pct(s.top3));
    assert!(pct(s.top8) >= FLOOR_TOP8, "top-8 fell to {:.0}%", pct(s.top8));
    let h = score_on(HELD_OUT, 8); let hn = HELD_OUT.len(); let hp = |x: usize| 100.0 * x as f64 / hn as f64;
    println!("held out, {hn} requests: top-1 {:.0}%  top-3 {:.0}%  top-8 {:.0}%  MRR {:.3}", hp(h.top1), hp(h.top3), hp(h.top8), h.mrr);
    for m in &h.misses { println!("  {m}"); }
    for (label, set) in [("tuned set", CASES), ("held out", HELD_OUT)] { let o = score_with(old_search::search_tools, set, 8); let on = set.len(); let op = |x: usize| 100.0 * x as f64 / on as f64;
        println!("old search, {label}: top-1 {:.0}%  top-3 {:.0}%  top-8 {:.0}%  MRR {:.3}", op(o.top1), op(o.top3), op(o.top8), o.mrr); }
    assert!(hp(h.top8) >= FLOOR_HELD_TOP8, "held-out top-8 fell to {:.0}%", hp(h.top8));
}
// measured on the old keyword search (2026-11-17): top-1 42%, top-3 59%, top-8 69%, MRR 0.519
// measured on tool_index (BM25F + editing vocabulary): top-1 77%, top-3 100%, top-8 100%, MRR 0.882
const FLOOR_TOP1: f64 = 75.0;
const FLOOR_TOP3: f64 = 97.0;
const FLOOR_TOP8: f64 = 99.0;
// held out, old search: top-1 23%, top-3 47%, top-8 57%, MRR 0.362; tool_index: top-1 37%, top-3 67%, top-8 83%, MRR 0.546
const FLOOR_HELD_TOP8: f64 = 80.0;
