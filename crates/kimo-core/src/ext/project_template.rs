//! v7.4: project_template -- starter project templates. Every project today starts from a blank canvas:
//! project_set takes width/height/fps/name but creates no tracks, so "set up a vertical short" or "set up a
//! two-guest podcast" means the agent has to already know (or guess) the right canvas size and then call
//! add_track by hand, once per track, in the right order, every single time. These are the conventions
//! this audit actually watched kimo projects need repeatedly (see each template's own rationale below) --
//! not clip/op templates (that is template_define/template_apply, a completely different, pre-existing
//! thing: a named, parameterised op-list applied to a clip; this is a project-level starting point: canvas
//! size, fps, and an ordered, named, empty track layout).
use super::kit::*;

struct Tmpl { name: &'static str, width: u32, height: u32, fps: f64, tracks: &'static [&'static str], about: &'static str }

const TEMPLATES: &[Tmpl] = &[
    Tmpl { name: "vertical_social", width: 1080, height: 1920, fps: 30.0, tracks: &["Video", "Captions", "Music"],
        about: "9:16 for Reels/TikTok/Shorts. Captions on their own track so auto_captions' output never collides with b-roll; Music separate so ducking/normalize_loudness can target it alone." },
    Tmpl { name: "square_social", width: 1080, height: 1080, fps: 30.0, tracks: &["Video", "Captions", "Music"],
        about: "1:1 for feed posts. Same track convention as vertical_social (captions and music kept separable)." },
    Tmpl { name: "widescreen_16_9", width: 1920, height: 1080, fps: 25.0, tracks: &["A-Roll", "B-Roll", "Music", "SFX"],
        about: "Standard landscape (YouTube/web). A-Roll/B-Roll split matches place_broll's own expectation of a dedicated b-roll track; Music and SFX separate so one can be loudness-normalized without touching the other." },
    Tmpl { name: "podcast_interview", width: 1920, height: 1080, fps: 25.0, tracks: &["Host", "Guest", "Screen Share", "Music"],
        about: "Two-person talking-head interview: Host and Guest on separate tracks so auto_rough_cut's per-speaker dead-air detection and switch_angle-style cutting have something to cut between; Screen Share for slides/demo capture; Music for the intro/outro bed." },
    Tmpl { name: "multicam_panel", width: 1920, height: 1080, fps: 25.0, tracks: &["Cam A", "Cam B", "Cam C", "Screen Share", "Music"],
        about: "Panel/roundtable with multiple cameras plus a screen share -- feeds add_multicam/switch_angle a ready-made set of per-angle tracks instead of the agent inventing track names on the fly." },
];

fn list_project_templates(_ss: &mut Session, _a: &Value) -> Result<Value, String> {
    Ok(json!({"templates": TEMPLATES.iter().map(|t| json!({"name": t.name, "width": t.width, "height": t.height, "fps": t.fps, "tracks": t.tracks, "about": t.about})).collect::<Vec<_>>()}))
}

fn create_project_from_template(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let name = str_arg(a, "template")?;
    let t = TEMPLATES.iter().find(|t| t.name == name).ok_or_else(|| format!("no template '{name}'. Valid: {}", TEMPLATES.iter().map(|t| t.name).collect::<Vec<_>>().join(", ")))?;
    let existing_clips: usize = ss.project.tracks.iter().map(|tr| tr.clips.len()).sum();
    let overwrite = a.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false);
    if (existing_clips > 0 || !ss.project.tracks.is_empty()) && !overwrite {
        return Err(format!("the current project already has {} track(s) ({} clip(s) total); pass overwrite:true to replace them with the template's layout, or start a fresh session first", ss.project.tracks.len(), existing_clips));
    }
    ss.project.width = t.width; ss.project.height = t.height; ss.project.fps = t.fps;
    ss.project.tracks.clear(); ss.project.markers.clear();
    let mut track_ids = vec![];
    for tname in t.tracks {
        let tid = ss.project.gen_id("track");
        ss.project.tracks.push(crate::model::Track { id: tid.clone(), clips: vec![], locked: false, muted: false, solo: false, name: (*tname).into() });
        track_ids.push(json!({"track_id": tid, "name": tname}));
    }
    Ok(json!({"template": t.name, "width": t.width, "height": t.height, "fps": t.fps, "tracks": track_ids, "about": t.about}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "list_project_templates", description: "List the starter project templates create_project_from_template can set up: canvas size, fps, track layout and why.", mutates: false,
        schema: obj(vec![], &[]), run: list_project_templates },
    Command { name: "create_project_from_template", description: "Set up a new project from a starter template: canvas size, fps and a named, ordered, empty track layout (see list_project_templates).", mutates: true,
        schema: obj(vec![("template", s("template name, from list_project_templates")), ("overwrite", json!({"type":"boolean","description":"default false: refuses if the project already has any tracks, so a template call never silently wipes real work"}))], &["template"]),
        run: create_project_from_template },
] }
