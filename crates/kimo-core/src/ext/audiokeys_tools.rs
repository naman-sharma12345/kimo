//! audio_to_keyframes (After Effects' Convert Audio to Keyframes): drive any animatable property from a clip's loudness.
use super::kit::*;
use crate::keyframe::{Ease, Prop};
use crate::model::*;

const ARGS: &[&str] = &["clip_id", "property", "effect_id", "source_clip_id", "min", "max", "channel", "band", "window", "smoothing", "attack", "release", "response", "floor_db", "ceiling_db", "start", "end"];
const CLIP_PROPS: &[&str] = &["opacity", "x", "y", "scale", "rotation", "rot_x", "rot_y", "pos_z", "fill_alpha", "stroke_alpha", "gradient_angle", "flow_blur", "reveal"];

fn num(a: &Value, k: &str, d: f64) -> Result<f64, String> { match a.get(k) { None => Ok(d), Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| format!("{k} must be a finite number")) } }
fn word(a: &Value, k: &str, d: &str, ok: &[&str]) -> Result<String, String> { let v = match a.get(k) { None => d.to_string(), Some(v) => v.as_str().ok_or_else(|| format!("{k} must be a string"))?.to_string() };
    if ok.contains(&v.as_str()) { Ok(v) } else { Err(format!("{k} must be one of {}, got '{v}'", ok.join(", "))) } }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "audio_to_keyframes", description: "Convert Audio to Keyframes: a clip's loudness (RMS, smoothing, attack/release) keys any property or effect param between min and max.", mutates: true,
        schema: obj(vec![("clip_id", s("the clip whose property is keyed")),
            ("property", s("opacity|x|y|scale|rotation|... or, with effect_id, an effect param (e.g. radius of a blur)")),
            ("effect_id", s("optional: key a param of this effect on clip_id")),
            ("source_clip_id", s("the clip whose audio is read (timeline-aligned), default clip_id")),
            ("min", n("value at the floor (silence), default the property's current value")), ("max", n("value at the ceiling (loudest)")),
            ("channel", json!({"type":"string","enum":crate::audiokeys::CHANNELS,"description":"default mix"})),
            ("band", json!({"type":"string","enum":crate::audiokeys::BANDS,"description":"low < 150 Hz (kick/bass), mid 300-3000 Hz, high > 4 kHz, default full"})),
            ("window", n("RMS window seconds, default one frame (at least 0.02)")), ("smoothing", n("moving-average seconds, default 0")),
            ("attack", n("rise time constant seconds, default 0 (instant)")), ("release", n("fall time constant seconds, default 0 (instant)")),
            ("response", json!({"type":"string","enum":["db","linear"],"description":"loudness in dB (default) or linear amplitude"})),
            ("floor_db", n("dB mapped to min, default -50 (db response)")), ("ceiling_db", n("dB (or linear amplitude) mapped to max, default the loudest point")),
            ("start", n("clip-local seconds to start keying, default 0")), ("end", n("clip-local seconds to stop, default the clip's end"))], &["clip_id", "property", "max"]),
        run: |ss, a| {
            let o = a.as_object().ok_or("audio_to_keyframes: arguments must be an object")?;
            for k in o.keys() { if !ARGS.contains(&k.as_str()) { return Err(format!("audio_to_keyframes: unknown argument '{k}' (valid: {})", ARGS.join(", "))); } }
            let cid = str_arg(a, "clip_id")?.to_string(); let prop = str_arg(a, "property")?.to_string();
            let eid = match a.get("effect_id") { None => None, Some(v) => Some(v.as_str().ok_or("effect_id must be a string")?.to_string()) };
            let sid = match a.get("source_clip_id") { None => cid.clone(), Some(v) => v.as_str().ok_or("source_clip_id must be a clip id")?.to_string() };
            let max = a.get("max").and_then(|v| v.as_f64()).filter(|x| x.is_finite()).ok_or("audio_to_keyframes needs max: the property's value at the loudest point")?;
            let channel = word(a, "channel", "mix", crate::audiokeys::CHANNELS)?; let band = word(a, "band", "full", crate::audiokeys::BANDS)?; let response = word(a, "response", "db", &["db", "linear"])?;
            let fps = ss.project.fps.max(1.0);
            let window = num(a, "window", (1.0 / fps).max(0.02))?; if !(0.001..=2.0).contains(&window) { return Err(format!("window must be 0.001..2 seconds, got {window}")); }
            let (smoothing, attack, release) = (num(a, "smoothing", 0.0)?, num(a, "attack", 0.0)?, num(a, "release", 0.0)?);
            for (k, v) in [("smoothing", smoothing), ("attack", attack), ("release", release)] { if !(0.0..=10.0).contains(&v) { return Err(format!("{k} must be 0..10 seconds, got {v}")); } }
            let floor_db = num(a, "floor_db", -50.0)?; if !(-120.0..0.0).contains(&floor_db) { return Err(format!("floor_db must be between -120 and 0, got {floor_db}")); }
            let ceiling = match a.get("ceiling_db") { None => None, Some(v) => Some(v.as_f64().filter(|x| x.is_finite()).ok_or("ceiling_db must be a number")?) };
            if let Some(cv) = ceiling { if response == "db" && cv <= floor_db { return Err(format!("ceiling_db {cv} must be above floor_db {floor_db}")); } if response == "linear" && cv <= 0.0 { return Err("ceiling_db is a linear amplitude above 0 with response linear".into()); } }
            let c = ss.project.find_clip(&cid).ok_or_else(|| format!("no such clip {cid}"))?;
            let (start, end) = (num(a, "start", 0.0)?, num(a, "end", c.duration)?);
            if start < 0.0 || end > c.duration + 1e-9 || end <= start { return Err(format!("start/end must satisfy 0 <= start < end <= {:.3} (clip {cid}'s length), got {start}..{end}", c.duration)); }
            let base = match &eid {
                None => { if !CLIP_PROPS.contains(&prop.as_str()) { return Err(format!("unknown clip property '{prop}' (valid: {}); to key an effect param pass effect_id", CLIP_PROPS.join(", "))); }
                    c.props.get(&prop).map(|p| p.base).unwrap_or(match prop.as_str() { "opacity" | "scale" | "fill_alpha" | "stroke_alpha" => 1.0, _ => 0.0 }) }
                Some(id) => { let e = c.effects.iter().find(|x| &x.id == id).ok_or_else(|| format!("no such effect {id} on clip {cid}"))?;
                    let listed = crate::effects::CATALOG.iter().chain(crate::effects2::CATALOG2.iter()).chain(crate::color::catalog3().iter()).find(|k| k.0 == e.kind).map(|k| k.2.iter().map(|p| (p.0, p.1)).collect::<Vec<_>>());
                    match (e.params.get(&prop), &listed) { (Some(p), _) => p.base, (None, Some(ps)) => ps.iter().find(|p| p.0 == prop).map(|p| p.1).ok_or_else(|| format!("'{prop}' is not a param of {} (listed: {})", e.kind, ps.iter().map(|p| p.0).collect::<Vec<_>>().join(", ")))?, (None, None) => 0.0 } } };
            let min = num(a, "min", base)?;
            let (t_start, t_dur) = (c.start, c.duration);
            let src = ss.project.find_clip(&sid).ok_or_else(|| format!("no such clip {sid} (source_clip_id)"))?;
            let Source::Media { asset } = &src.source else { return Err(format!("source clip {sid} is not a media clip: its audio cannot be read")) };
            let ast = ss.project.assets.get(asset).ok_or_else(|| format!("asset {asset} missing"))?;
            let pcm = crate::audio::decode_clip(&ss.project.resolve(&ast.path), src);
            if pcm.len() < 2 { return Err(format!("clip {sid} has no audio to read")); }
            let sig = crate::audiokeys::band_signal(&pcm, crate::audio::SR as f64, &channel, &band);
            let n = (((end - start) * fps) + 1e-6).floor() as usize + 1; let lts: Vec<f64> = (0..n).map(|i| (start + i as f64 / fps).min(end)).collect();
            let src_t: Vec<f64> = lts.iter().map(|lt| t_start + lt - src.start).collect();
            let rms = crate::audiokeys::rms_at(&sig, crate::audio::SR as f64, &src_t, window);
            let (env, ceil) = crate::audiokeys::shape(&rms, 1.0 / fps, &crate::audiokeys::Shape { db: response == "db", floor_db, ceiling, attack, release, smoothing });
            let _ = t_dur;
            let c = ss.project.find_clip_mut(&cid).unwrap();
            let pr: &mut Prop = match &eid { Some(id) => c.effects.iter_mut().find(|x| &x.id == id).unwrap().params.entry(prop.clone()).or_insert(Prop::new(base)), None => c.props.entry(prop.clone()).or_insert(Prop::new(base)) };
            pr.keys.retain(|k| k.t < start - 1e-6 || k.t > end + 1e-6);
            for (lt, e) in lts.iter().zip(&env) { pr.set_key(*lt, min + (max - min) * e, Ease::Linear); }
            let loud = env.iter().filter(|e| **e > 0.5).count() as f64 / env.len().max(1) as f64; let mean = env.iter().sum::<f64>() / env.len().max(1) as f64;
            Ok(json!({"clip_id": cid, "property": prop, "keys": env.len(), "span": [start, end], "min": min, "max": max, "response": response,
                "floor_db": if response == "db" { json!(floor_db) } else { json!(null) }, "ceiling": ceil, "mean_level": (mean * 1000.0).round() / 1000.0, "loud_fraction": (loud * 1000.0).round() / 1000.0,
                "note": "one linear key per frame; simplify_curve thins them" })) } },
] }
