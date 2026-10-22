//! add_glow_text: a one-call creator preset for the short-form caption / hook look: one bold text clip with a pulsing neon glow.
//! It is a thin wrapper over the existing glow effect (mode 1, deep glow; glow.rs), tinted from glow_color, whose amount breathes
//! on a sine expression op (amount = 0.55 + 0.35 sin(2 pi pulse_hz t), clip time), plus a short opacity fade in and out.
//! The effect is an ordinary clip effect (id "glow1"), so set_effect_param, set_keyframe and remove_effect work on it afterwards.
use super::kit::*;
use crate::keyframe::{Ease, Op, Prop};
use crate::model::*;
use std::collections::BTreeMap;

const ARGS: &[&str] = &["track_id", "text", "start", "duration", "color", "glow_color", "size", "pulse_hz", "font"];
fn num(a: &Value, k: &str, d: f64, lo: f64, hi: f64) -> Result<f64, String> { match a.get(k) { None => Ok(d), Some(v) => { let x = v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a number"))?; if x < lo || x > hi { Err(format!("{k} must be within {lo}..{hi}, got {x}")) } else { Ok(x) } } } }
fn rgb(a: &Value, k: &str, d: [f64; 3]) -> Result<[f64; 3], String> { match a.get(k) { None => Ok(d), Some(v) => { let arr = v.as_array().filter(|x| x.len() == 3).ok_or_else(|| format!("{k} must be [r, g, b] in 0..1"))?;
    let mut o = [0.0; 3]; for (i, x) in arr.iter().enumerate() { let x = x.as_f64().filter(|x| (0.0..=1.0).contains(x)).ok_or_else(|| format!("{k} must be [r, g, b] with each in 0..1"))?; o[i] = x; } Ok(o) } } }
fn clip(ss: &mut Session, source: Source, start: f64, dur: f64) -> Clip { Clip { id: ss.project.gen_id("clip"), source, start, duration: dur, in_point: 0.0, speed: 1.0, speed_curve: None, time_remap: None, speed_blend: Default::default(), props: default_props(), grade_graph: None, effects: vec![], blend: "normal".into(), mask: None, time_mode: TimeMode::Normal, corners: None, audio: Audio::default(), motion_blur: 0.0, echo: None, motion_path: None, posterize_fps: 0.0, text_fx: None, parent: None, inherit: Default::default(), name: None, role: None, tags: vec![], transition_in: None } }
/// The pulse: amount swings BASE +/- SWING (0.2..0.9 of the glow's 0..2 range).
const BASE: f64 = 0.55;
const SWING: f64 = 0.35;

pub fn commands() -> Vec<Command> { vec![
    Command { name: "add_glow_text", description: "A bold text clip with a pulsing neon glow pre-applied (the glow effect, its amount breathing on a sine wave). One call for the caption / hook look; the glow stays an ordinary effect (id glow1) you can edit or remove.", mutates: true,
        schema: obj(vec![("track_id", s("the track to put the text clip on")),
            ("text", s("the caption / hook text")),
            ("start", n("seconds, default 0")),
            ("duration", n("seconds, default 3")),
            ("color", json!({"type":"array","items":{"type":"number"},"description":"text colour [r, g, b] 0..1, default white"})),
            ("glow_color", json!({"type":"array","items":{"type":"number"},"description":"glow tint [r, g, b] 0..1, default warm pink [1, 0.55, 0.85]"})),
            ("size", n("font size in px, default 110 at 1080p (scaled with the frame's short side)")),
            ("pulse_hz", n("how fast the glow breathes, 0.1..8 pulses per second, default 1.6")),
            ("font", s("font name, default Anton"))], &["track_id", "text"]),
        run: |ss, a| { let o = a.as_object().ok_or("add_glow_text: arguments must be an object")?;
            for k in o.keys() { if !ARGS.contains(&k.as_str()) { return Err(format!("add_glow_text: unknown argument '{k}' (valid: {})", ARGS.join(", "))); } }
            let track_id = str_arg(a, "track_id")?.to_string(); let text = str_arg(a, "text")?.to_string();
            if !ss.project.tracks.iter().any(|t| t.id == track_id) { return Err(format!("no such track {track_id}")); }
            let start = num(a, "start", 0.0, 0.0, 1e7)?; let duration = num(a, "duration", 3.0, 0.1, 1e6)?;
            let k = ss.project.height.min(ss.project.width) as f64 / 1080.0; let size = num(a, "size", 110.0 * k, 1.0, 5000.0)?;
            let pulse_hz = num(a, "pulse_hz", 1.6, 0.1, 8.0)?;
            let color = rgb(a, "color", [1.0; 3])?; let glow = rgb(a, "glow_color", [1.0, 0.55, 0.85])?;
            let font = match a.get("font") { None => "Anton".to_string(), Some(v) => v.as_str().ok_or("font must be a string")?.to_string() };
            let mut c = clip(ss, Source::Text { text, font, size, color, letter_spacing: 0.0, stroke_width: 0.0, stroke_color: [0.0; 3], shadow: 0.0, align: "center".into() }, start, duration);
            // glow mode 1 (deep glow), catalog defaults except the tint (both ends from glow_color) and a radius scaled with the frame
            let mut params: BTreeMap<String, Prop> = BTreeMap::new();
            for (key, v) in [("mode", 1.0), ("radius", 12.0 * k.max(0.25)), ("threshold", 0.7), ("knee", 0.2), ("falloff", 0.0), ("iterations", 5.0), ("exposure", 0.0), ("linear", 1.0),
                ("color_a_r", glow[0]), ("color_a_g", glow[1]), ("color_a_b", glow[2]), ("color_b_r", glow[0]), ("color_b_g", glow[1]), ("color_b_b", glow[2])] { params.insert(key.into(), Prop::new(v)); }
            let mut amount = Prop::new(BASE); amount.expr.push(Op::Sin { freq: pulse_hz, amp: SWING, phase: 0.0 }); params.insert("amount".into(), amount);
            c.effects.push(Effect { id: "glow1".into(), kind: "glow".into(), params, strings: Default::default() });
            let f = 0.25f64.min(duration / 2.0 - 0.01).max(0.0); let op = c.props.get_mut("opacity").unwrap(); op.base = 0.0;
            op.set_key(0.0, 0.0, Ease::Out); op.set_key(f, 1.0, Ease::Out); op.set_key((duration - f).max(f + 0.01), 1.0, Ease::In); op.set_key(duration, 0.0, Ease::In);
            let id = c.id.clone();
            ss.project.tracks.iter_mut().find(|t| t.id == track_id).ok_or("no such track")?.clips.push(std::sync::Arc::new(c));
            Ok(json!({"clip_id": id, "pulse_hz": pulse_hz})) } },
] }
