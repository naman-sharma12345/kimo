//! auto_reframe (Premiere Pro Auto Reframe): see reframe.rs. One call analyses a media clip and keys a canvas-shaped crop window
//! that follows its subject; x / y set a fixed window instead, remove goes back to the clip's ordinary fit.
use super::kit::*;
use crate::model::*;

pub fn commands() -> Vec<Command> { vec![
    Command { name: "auto_reframe", description: "Auto Reframe: crop a shot to the canvas aspect (e.g. 16:9 into 9:16) following its subject, decoded sharp at full resolution.", mutates: true,
        schema: obj(vec![("clip_id", s("media clip (video or image) whose aspect differs from the canvas")),
            ("motion", json!({"type":"string","enum":crate::reframe::MOTIONS,"description":"how the virtual camera follows: static (one fixed window), slow, default, fast"})),
            ("x", n("optional fixed window instead of tracking: 0 = left edge, 0.5 = centre, 1 = right edge")),
            ("y", n("optional fixed window for tall sources: 0 = top, 0.5 = centre, 1 = bottom")),
            ("remove", json!({"type":"boolean","description":"true removes the reframe window (back to the clip's normal fit)"}))], &["clip_id"]),
        run: |ss, a| {
            let id = str_arg(a, "clip_id")?.to_string();
            for k in a.as_object().map(|o| o.keys().cloned().collect::<Vec<_>>()).unwrap_or_default() { if !["clip_id", "motion", "x", "y", "remove"].contains(&k.as_str()) { return Err(format!("auto_reframe: unknown argument '{k}' (valid: clip_id, motion, x, y, remove)")); } }
            let clip = ss.project.find_clip(&id).ok_or_else(|| format!("no such clip {id}"))?;
            if !matches!(clip.source, Source::Media { .. }) { return Err(format!("auto_reframe needs a media (video or image) clip: '{id}' is not one")); }
            if a.get("remove").and_then(|v| v.as_bool()).unwrap_or(false) {
                let had = crate::reframe::active(clip); let c = ss.project.find_clip_mut(&id).unwrap(); for k in crate::reframe::PROPS { c.props.remove(k); }
                return Ok(json!({"clip_id": id, "removed": had})); }
            let fixed = |k: &str| -> Result<Option<f64>, String> { match a.get(k) { None => Ok(None), Some(v) if v.is_null() => Ok(None), Some(v) => { let x = v.as_f64().ok_or(format!("{k} must be a number 0..1"))?; if !(0.0..=1.0).contains(&x) { return Err(format!("{k} must be between 0 and 1 (0.5 = centre), got {x}")); } Ok(Some(x)) } } };
            let (fx, fy) = (fixed("x")?, fixed("y")?);
            if fx.is_some() || fy.is_some() {
                if a.get("motion").is_some_and(|v| !v.is_null()) { return Err("give either motion (track the subject) or x/y (a fixed window), not both".into()); }
                let c = ss.project.find_clip_mut(&id).unwrap(); use crate::keyframe::Prop;
                c.props.insert("reframe_x".into(), Prop::new(fx.unwrap_or(0.5))); c.props.insert("reframe_y".into(), Prop::new(fy.unwrap_or(0.5)));
                return Ok(json!({"clip_id": id, "fixed": [fx.unwrap_or(0.5), fy.unwrap_or(0.5)]})); }
            let motion = match a.get("motion") { None => "default".to_string(), Some(v) if v.is_null() => "default".to_string(), Some(v) => v.as_str().ok_or("motion must be a string: static, slow, default or fast")?.to_string() };
            let plan = crate::reframe::analyze(&ss.project, &id, &motion)?;
            let c = ss.project.find_clip_mut(&id).unwrap(); crate::reframe::write(c, plan.axis_x, &plan.keys);
            Ok(plan.report) } },
] }
