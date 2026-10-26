//! add_arrow (KF17): the animated arrow/callout -- a bright arrow that draws in and points at a spot on
//! screen, with an optional short label at its tail. Research lane KR2 (see
//! /home/user/tab/files/KR2_youtube_graphics_research.pdf) lists this "look here" arrow/callout as a top
//! missing graphic among Johnny Harris / Veritasium / MKBHD-style creators. This is a NEW, ADDITIVE tool:
//! it touches nothing caption-related and no other existing tool's code or defaults, and it is not on the
//! DIRECTOR list.
//!
//! Existing-tool check (done first, before writing anything): grepped golden_registry.txt and every
//! ext/*.rs / commands/tools_*.rs for arrow/callout/pointer/annotate. No hits that are actually an arrow
//! or callout tool (the handful of textual matches are unrelated: JSON `.pointer()` calls, a
//! `track_long_gap` appearance tracker, etc). Genuinely missing.
//!
//! Built entirely on EXISTING primitives through `dispatch_nested`, no new rasterizer:
//!   - the shaft is `add_shape` with `shape: "line"` (already supports a plain horizontal bar: `width` =
//!     length, `height` = thickness, see shapemods.rs's `sd` and commands/tools_1.rs's add_shape schema --
//!     "rect, ellipse, polygon (sides), line" was already there, so the brief's fallback of composing a
//!     rotated rect was not needed), rotated into place with `set_property`/`set_keyframe rotation`.
//!   - the arrowhead is `add_shape` with `shape: "polygon", sides: 3` (an existing equilateral-triangle
//!     primitive), rotated to point from tail to target.
//!   - the optional label is `add_text`, styled like `add_keyword_pops`'s `bold_caps` (KF16 polish: the
//!     bundled Anton heavy-caps font, a thick dark stroke, a drop shadow), sized smaller since it is a
//!     secondary annotation next to a graphic, not the graphic itself.
//!   - every animation (draw-in, pop, fade, the end-of-life fade-out) is `set_keyframe` on the clips'
//!     existing `x`/`y`/`scale`/`opacity` transform props; no keyframe or easing math is reimplemented.
//!
//! The "draw" animation (shaft grows from tail to head, then a tiny bounce) needs one piece of geometry:
//! `width`/`height` on a shape clip are NOT animatable transform props (see curves.rs's `CLIP_PROPS` --
//! only opacity/x/y/scale/rotation/rot_x/rot_y/pos_z/backface/fill_alpha/stroke_alpha/gradient_angle/
//! flow_blur/flow_quality/pin_gain/reveal/path are), so the shaft's own `width` cannot be keyframed
//! directly. Scale and rotation on any clip pivot at the SHAPE'S OWN CENTER (render.rs: `transform_bb`
//! scales/rotates the un-translated source frame -- where the shape is always drawn centered on the
//! canvas -- about the canvas center, and only THEN shifts by the `x`/`y` props; since the shape's local
//! center coincides with the canvas center before that shift, translating afterwards does not move the
//! pivot relative to the shape, so scale/rotate is effectively "about this clip's own center, then
//! placed"). So a full-length static shaft scaled by `s` about its own center spans
//! `center -/+ s*(length/2)` on each side. Keeping the TAIL fixed while the shaft grows to the head is
//! then pure algebra: if the clip's `x`/`y` center is ALSO keyframed, in lockstep with `scale`, from the
//! tail point (at scale 0, so the degenerate zero-length shaft sits exactly at the tail) to the true
//! segment midpoint (at scale 1, so the full-length shaft spans tail..head), the near edge of the shaft
//! stays pinned at the tail for every scale value in between, and the far edge sweeps from the tail to
//! the head -- i.e. draws in. Using the SAME easing on the `scale` key and the `x`/`y` keys (so they
//! share one interpolation curve) keeps that identity exact at every intermediate frame, not just the
//! endpoints. `set_keyframe`'s ease semantics (confirmed against `add_pip.rs`'s own
//! `animate_prop`/`pop_animation_scale_overshoots_then_settles_by_a_quarter_second` test, which this tool's
//! `draw`/`pop` animations copy): the ease named on a key governs the segment LEAVING that key, so the
//! overshoot ease ("out_back") belongs on the EARLIER key of the rising segment, not the later one --
//! putting `out_back` on the key at t=0 and `hold` on the key at the end of the rise is what actually
//! produces the "quick draw then a tiny bounce" overshoot-and-settle this tool asks for.
use super::kit::*;
use crate::commands::{color, dispatch_nested as call};

const ANIMATES: [&str; 3] = ["draw", "pop", "fade"];
/// Bright yellow #FFD400, the brief's default arrow colour.
const DEFAULT_COLOR: [f64; 3] = [1.0, 212.0 / 255.0, 0.0];
const DRAW_IN_S: f64 = 0.3;
const FADE_OUT_LEAD_S: f64 = 0.35;

fn bool_arg_opt(a: &Value, k: &str) -> Result<Option<bool>, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_bool().map(Some).ok_or_else(|| crate::errors::arg_err(a, k, "boolean")) }
}
fn opt_num_checked(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d), Some(v) => v.as_f64().filter(|x| x.is_finite()).ok_or_else(|| crate::errors::arg_err(a, k, "number")) }
}
fn enum_arg(a: &Value, k: &str, d: &str, ok: &[&str]) -> Result<String, String> {
    match a.get(k) { None | Some(Value::Null) => Ok(d.to_string()),
        Some(Value::String(s)) if ok.contains(&s.as_str()) => Ok(s.clone()),
        Some(v) => Err(format!("{k} must be one of {}, got {v}", ok.join(", "))) }
}
/// A normalized {x,y} point, 0..1 when `clamp01` -- `target` must land on the frame (`clamp01: true`);
/// `from`, when given explicitly, is allowed to sit a little outside the frame (an arrow sensibly starts
/// just off-screen), so it is only checked finite there (`clamp01: false`).
fn point_arg(a: &Value, k: &str, clamp01: bool) -> Result<Option<(f64, f64)>, String> {
    match a.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(v) => {
            let x = v.get("x").and_then(|x| x.as_f64()).filter(|x| x.is_finite()).ok_or_else(|| format!("{k}.x must be a finite number"))?;
            let y = v.get("y").and_then(|y| y.as_f64()).filter(|y| y.is_finite()).ok_or_else(|| format!("{k}.y must be a finite number"))?;
            if clamp01 && !(0.0..=1.0).contains(&x) { return Err(format!("{k}.x must be 0..1 (normalized), got {x}")); }
            if clamp01 && !(0.0..=1.0).contains(&y) { return Err(format!("{k}.y must be 0..1 (normalized), got {y}")); }
            Ok(Some((x, y)))
        }
    }
}

/// `from`'s auto default (brief: "auto from the nearest frame corner/side toward target, length ~25% of
/// frame min dim"): continue the ray from the frame's center through `target`, outward past it, by 25%
/// of the frame's shorter side -- the direction toward whichever edge/corner is closest to `target` in
/// that direction, without a separate corner/side lookup table. `target` at dead-center (no direction)
/// falls back to a top-left diagonal, a sensible default start corner.
fn default_from_px(target_px: (f64, f64), fw: f64, fh: f64) -> (f64, f64) {
    let center = (fw / 2.0, fh / 2.0);
    let (ddx, ddy) = (target_px.0 - center.0, target_px.1 - center.1);
    let mag = (ddx * ddx + ddy * ddy).sqrt();
    let (dirx, diry) = if mag > 1e-6 { (ddx / mag, ddy / mag) } else { (-std::f64::consts::FRAC_1_SQRT_2, -std::f64::consts::FRAC_1_SQRT_2) };
    let len = 0.25 * fw.min(fh);
    (target_px.0 + dirx * len, target_px.1 + diry * len)
}

/// Keyframes `property` from `(v0, ease_rise)` at t=0 to `(v1, "hold")` at t=`rise_s`, i.e. the rising
/// segment uses `ease_rise` (see module doc: ease governs the segment LEAVING the key it's set on).
fn rise(ss: &mut Session, id: &str, property: &str, v0: f64, v1: f64, rise_s: f64, ease_rise: &str) -> Result<(), String> {
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": property, "time": 0.0, "value": v0, "ease": ease_rise}))?;
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": property, "time": rise_s, "value": v1, "ease": "hold"}))?;
    Ok(())
}
/// Opacity 1 -> 0 starting `duration_s - FADE_OUT_LEAD_S` (clamped >= 0.05s in), ending exactly at the
/// clip's own `duration_s` (clip-local, so it reaches 0 right as the clip ends).
fn fade_out(ss: &mut Session, id: &str, duration_s: f64) -> Result<(), String> {
    let start = (duration_s - FADE_OUT_LEAD_S).max(0.05).min(duration_s);
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": start, "value": 1.0, "ease": "in"}))?;
    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": duration_s, "value": 0.0, "ease": "in"}))?;
    Ok(())
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    const KNOWN: &[&str] = &["target", "from", "at_s", "duration_s", "color", "thickness", "label", "curve", "animate", "track_id", "dry_run"];
    if let Some(o) = a.as_object() { for k in o.keys() { if !KNOWN.contains(&k.as_str()) { return Err(format!("add_arrow: unknown argument '{k}'")); } } } else { return Err("add_arrow: arguments must be an object".into()); }

    let target = point_arg(a, "target", true)?.ok_or_else(|| crate::errors::arg_err(a, "target", "{x,y} normalized 0..1"))?;
    let from_explicit = point_arg(a, "from", false)?;
    let at_s = a.get("at_s").and_then(|v| v.as_f64()).filter(|x| x.is_finite()).ok_or_else(|| crate::errors::arg_err(a, "at_s", "number"))?;
    if at_s < 0.0 { return Err("add_arrow: at_s must be >= 0".into()); }
    let duration_s = opt_num_checked(a, "duration_s", 2.0)?;
    if !(duration_s > 0.0) { return Err("add_arrow: duration_s must be a finite number > 0".into()); }
    let color = color(a, "color", DEFAULT_COLOR)?;
    let label = match a.get("label") { None | Some(Value::Null) => None, Some(Value::String(s)) if !s.trim().is_empty() => Some(s.clone()), Some(Value::String(_)) => return Err("add_arrow: label must not be empty".into()), _ => return Err(crate::errors::arg_err(a, "label", "string")) };
    let curve = bool_arg_opt(a, "curve")?.unwrap_or(false);
    let animate = enum_arg(a, "animate", "draw", &ANIMATES)?;
    let dry_run = bool_arg_opt(a, "dry_run")?.unwrap_or(false);

    let fw = ss.project.width as f64;
    let fh = ss.project.height as f64;
    let target_px = (target.0 * fw, target.1 * fh);
    let from_px = match from_explicit { Some((fx, fy)) => (fx * fw, fy * fh), None => default_from_px(target_px, fw, fh) };
    let thickness = match a.get("thickness") { None | Some(Value::Null) => (fw.min(fh) * 0.012).max(5.0), Some(v) => v.as_f64().filter(|x| x.is_finite() && *x > 0.0).ok_or_else(|| "add_arrow: thickness must be a finite number > 0".to_string())? };
    let head_r = (thickness * 4.2).max(16.0);

    let (dx, dy) = (target_px.0 - from_px.0, target_px.1 - from_px.1);
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1.0 { return Err("add_arrow: from and target are the same point (or too close) -- give target a point further from from, or drop from to use the auto default".into()); }
    let (ux, uy) = (dx / len, dy / len);
    let angle_deg = dy.atan2(dx).to_degrees();

    if dry_run {
        return Ok(json!({"dry_run": true, "target": {"x": target.0, "y": target.1}, "from_px": {"x": from_px.0, "y": from_px.1}, "target_px": {"x": target_px.0, "y": target_px.1}, "length_px": len, "thickness_px": thickness, "animate": animate, "curve": curve, "has_label": label.is_some(), "no_history": true}));
    }

    let track_id = match a.get("track_id") {
        None | Some(Value::Null) => call(ss, "add_track", &json!({}))?["track_id"].as_str().ok_or("add_track returned no track_id")?.to_string(),
        Some(Value::String(s)) if !s.is_empty() => s.clone(),
        _ => return Err(crate::errors::arg_err(a, "track_id", "string")),
    };
    if !ss.project.tracks.iter().any(|t| t.id == track_id) { return Err(format!("no such track '{track_id}'")); }

    // Shaft end pulled back from the target by most of the arrowhead's radius so the triangle's own base
    // sits over the shaft's tip instead of the shaft poking through the point.
    let shaft_end_px = (target_px.0 - ux * head_r * 0.75, target_px.1 - uy * head_r * 0.75);
    let mid_px = ((from_px.0 + shaft_end_px.0) / 2.0, (from_px.1 + shaft_end_px.1) / 2.0);
    let _ = mid_px;
    let cx = fw / 2.0; let cy = fh / 2.0;

    // Cheap bend (curve: true): two straight segments meeting at a point offset perpendicular from the
    // straight midpoint, instead of one -- no new path/Bezier primitive, just a second `add_shape` line
    // with its own length/rotation, same as the single-segment case.
    let bend_px = if curve {
        let perp = (-uy, ux); // unit perpendicular to the from->target direction
        let bend_amount = len * 0.12;
        let m = ((from_px.0 + shaft_end_px.0) / 2.0, (from_px.1 + shaft_end_px.1) / 2.0);
        Some((m.0 + perp.0 * bend_amount, m.1 + perp.1 * bend_amount))
    } else { None };

    let mut shaft_ids: Vec<String> = vec![];
    let make_shaft = |ss: &mut Session, p0: (f64, f64), p1: (f64, f64)| -> Result<String, String> {
        let (sdx, sdy) = (p1.0 - p0.0, p1.1 - p0.1);
        let slen = (sdx * sdx + sdy * sdy).sqrt().max(1.0);
        let sangle = sdy.atan2(sdx).to_degrees();
        let smid = ((p0.0 + p1.0) / 2.0, (p0.1 + p1.1) / 2.0);
        let id = call(ss, "add_shape", &json!({"track_id": track_id, "shape": "line", "start": at_s, "duration": duration_s,
            "width": slen, "height": thickness, "fill": color}))?["clip_id"].as_str().ok_or("add_shape returned no clip_id")?.to_string();
        call(ss, "set_property", &json!({"clip_id": id, "property": "rotation", "value": sangle}))?;
        match animate.as_str() {
            "draw" => {
                // Grows from p0 (collapsed, scale 0) to the full segment (scale 1, centered at smid) --
                // see module doc for why keeping x/y in lockstep with scale pins the near end at p0.
                rise(ss, &id, "scale", 0.0, 1.0, DRAW_IN_S, "out_back")?;
                rise(ss, &id, "x", p0.0 - cx, smid.0 - cx, DRAW_IN_S, "out_back")?;
                rise(ss, &id, "y", p0.1 - cy, smid.1 - cy, DRAW_IN_S, "out_back")?;
            }
            "pop" => {
                call(ss, "set_property", &json!({"clip_id": id, "property": "x", "value": smid.0 - cx}))?;
                call(ss, "set_property", &json!({"clip_id": id, "property": "y", "value": smid.1 - cy}))?;
                rise(ss, &id, "scale", 0.0, 1.0, DRAW_IN_S, "out_back")?;
                rise(ss, &id, "opacity", 0.0, 1.0, DRAW_IN_S.min(0.15), "out")?;
            }
            "fade" => {
                call(ss, "set_property", &json!({"clip_id": id, "property": "x", "value": smid.0 - cx}))?;
                call(ss, "set_property", &json!({"clip_id": id, "property": "y", "value": smid.1 - cy}))?;
                rise(ss, &id, "opacity", 0.0, 1.0, 0.25, "out")?;
            }
            other => return Err(format!("unknown animate '{other}'")),
        }
        fade_out(ss, &id, duration_s)?;
        Ok(id)
    };
    match bend_px {
        Some(b) => { shaft_ids.push(make_shaft(ss, from_px, b)?); shaft_ids.push(make_shaft(ss, b, shaft_end_px)?); }
        None => { shaft_ids.push(make_shaft(ss, from_px, shaft_end_px)?); }
    }

    // Arrowhead: an equilateral triangle (add_shape polygon, sides=3). shapemods.rs's polygon SDF
    // (`a = py.atan2(px) + PI/2`) puts the first vertex pointing in the local -y (up-screen) direction at
    // rotation 0, so the rotation that makes the tip point along the arrow's own direction (angle_deg,
    // measured the same atan2(dy,dx) way as the shaft's own rotation) is angle_deg + 90.
    let head_angle_deg = angle_deg + 90.0;
    let head_id = call(ss, "add_shape", &json!({"track_id": track_id, "shape": "polygon", "sides": 3, "start": at_s, "duration": duration_s,
        "width": head_r * 2.0, "height": head_r * 2.0, "fill": color}))?["clip_id"].as_str().ok_or("add_shape returned no clip_id")?.to_string();
    call(ss, "set_property", &json!({"clip_id": head_id, "property": "rotation", "value": head_angle_deg}))?;
    call(ss, "set_property", &json!({"clip_id": head_id, "property": "x", "value": target_px.0 - cx}))?;
    call(ss, "set_property", &json!({"clip_id": head_id, "property": "y", "value": target_px.1 - cy}))?;
    match animate.as_str() {
        "draw" => {
            // Appears right as the shaft's draw-in finishes rather than being visible from t=0.
            call(ss, "set_keyframe", &json!({"clip_id": head_id, "property": "opacity", "time": 0.0, "value": 0.0, "ease": "hold"}))?;
            call(ss, "set_keyframe", &json!({"clip_id": head_id, "property": "opacity", "time": (DRAW_IN_S - 0.08).max(0.0), "value": 0.0, "ease": "out"}))?;
            call(ss, "set_keyframe", &json!({"clip_id": head_id, "property": "opacity", "time": DRAW_IN_S, "value": 1.0, "ease": "hold"}))?;
            rise(ss, &head_id, "scale", 0.0, 1.0, DRAW_IN_S.max(0.08), "out_back")?;
        }
        "pop" => { rise(ss, &head_id, "scale", 0.0, 1.0, DRAW_IN_S, "out_back")?; rise(ss, &head_id, "opacity", 0.0, 1.0, DRAW_IN_S.min(0.15), "out")?; }
        "fade" => { rise(ss, &head_id, "opacity", 0.0, 1.0, 0.25, "out")?; }
        _ => unreachable!(),
    }
    fade_out(ss, &head_id, duration_s)?;

    // Optional label at the tail: add_keyword_pops's bold_caps look (Anton heavy caps, thick dark
    // outline, drop shadow -- KF16 polish), sized smaller since it sits beside the graphic, not as it.
    let label_id = match &label {
        None => None,
        Some(text) => {
            let text_upper = text.to_uppercase();
            let size = (fh * 0.05).clamp(18.0, 90.0);
            let (tw, th) = crate::text::measure(&text_upper, "Anton", size, 0.0, 0.0, "center", 0.0)?;
            // Placed beyond the tail, continuing OUTWARD along the from->target direction reversed, so the
            // label sits clear of the shaft instead of overlapping it -- then clamped to stay fully inside
            // the frame (the auto `from` default can land close to an edge, and continuing outward from an
            // edge point would otherwise push the label off-canvas, invisible).
            let (raw_lx, raw_ly) = (from_px.0 - ux * (tw * 0.55 + 14.0), from_px.1 - uy * (th * 0.55 + 14.0));
            let margin = 6.0;
            let label_px = (raw_lx.clamp(tw / 2.0 + margin, (fw - tw / 2.0 - margin).max(tw / 2.0 + margin)), raw_ly.clamp(th / 2.0 + margin, (fh - th / 2.0 - margin).max(th / 2.0 + margin)));
            let stroke_w = (size * 0.09).max(2.0);
            let shadow_px = (size * 0.05).max(2.0);
            let id = call(ss, "add_text", &json!({"track_id": track_id, "text": text_upper, "start": at_s, "duration": duration_s,
                "font": "Anton", "size": size, "color": [1.0, 1.0, 1.0], "stroke_width": stroke_w, "stroke_color": [0.03, 0.03, 0.03], "shadow": shadow_px}))?["clip_id"].as_str().ok_or("add_text returned no clip_id")?.to_string();
            call(ss, "set_property", &json!({"clip_id": id, "property": "x", "value": label_px.0 - cx}))?;
            call(ss, "set_property", &json!({"clip_id": id, "property": "y", "value": label_px.1 - cy}))?;
            match animate.as_str() {
                "draw" => { call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": 0.0, "value": 0.0, "ease": "hold"}))?;
                    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": DRAW_IN_S, "value": 0.0, "ease": "out"}))?;
                    call(ss, "set_keyframe", &json!({"clip_id": id, "property": "opacity", "time": DRAW_IN_S + 0.15, "value": 1.0, "ease": "hold"}))?;
                    rise(ss, &id, "scale", 0.0, 1.0, DRAW_IN_S.max(0.08), "out_back")?; }
                "pop" => { rise(ss, &id, "scale", 0.0, 1.0, DRAW_IN_S, "out_back")?; rise(ss, &id, "opacity", 0.0, 1.0, DRAW_IN_S.min(0.15), "out")?; }
                "fade" => { rise(ss, &id, "opacity", 0.0, 1.0, 0.25, "out")?; }
                _ => unreachable!(),
            }
            fade_out(ss, &id, duration_s)?;
            Some(id)
        }
    };

    Ok(json!({
        "dry_run": false, "track_id": track_id, "target": {"x": target.0, "y": target.1}, "from_px": {"x": from_px.0, "y": from_px.1},
        "target_px": {"x": target_px.0, "y": target_px.1}, "length_px": len, "thickness_px": thickness, "animate": animate, "curve": curve,
        "shaft_clip_ids": shaft_ids, "head_clip_id": head_id, "label_clip_id": label_id,
    }))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "add_arrow",
        description: "Draws a bright animated arrow pointing at a spot on screen, with an optional short label at its tail. The \"look here\" callout Johnny Harris/Veritasium/MKBHD-style creators use (research lane KR2): target is the normalized {x,y} point the arrowhead touches; from defaults to auto (continuing the line from the frame center through target outward by ~25% of the frame's shorter side, so it comes from whichever edge/corner is in that direction). animate (default draw): the shaft grows from tail to head over ~0.3s with a tiny overshoot-and-settle bounce, then the arrowhead pops in; pop and fade are also available. label, when given, sits at the tail styled like add_keyword_pops's bold_caps (Anton heavy caps, dark outline, drop shadow), smaller since it is a secondary annotation next to the graphic, not the graphic itself. Built entirely on add_shape (line for the shaft, polygon sides:3 for the arrowhead) + add_text + set_keyframe through dispatch_nested -- no new rasterizer. Deterministic (same input, same geometry and keys) and undoable with undo as one step, like any other mutating tool.",
        mutates: true,
        schema: obj(vec![
            ("target", json!({"type":"object","properties":{"x":n("0..1"),"y":n("0..1")},"required":["x","y"],"description":"normalized 0..1 point the arrowhead touches"})),
            ("from", json!({"type":"object","properties":{"x":n("0..1"),"y":n("0..1")},"required":["x","y"],"description":"normalized 0..1 tail point; default: auto, continuing the line from the frame center through target outward by ~25% of the frame's shorter side (so it comes from whichever edge/corner is in that direction)"})),
            ("at_s", n("seconds, clip start")),
            ("duration_s", n("seconds on screen, default 2.0")),
            ("color", crate::commands::col_schema("default bright yellow #FFD400", false)),
            ("thickness", n("shaft thickness in px; default scaled to the frame (~1.2% of the shorter side, floor 5px)")),
            ("label", s("optional short text at the tail, styled like add_keyword_pops's bold_caps (Anton heavy caps, dark outline, drop shadow), smaller since it's a secondary annotation")),
            ("curve", json!({"type":"boolean","description":"default false. true bends the arrow through a slight midpoint offset (two straight segments, not a full Bezier path) instead of one straight shaft"})),
            ("animate", json!({"type":"string","enum":ANIMATES,"description":"draw (default): the shaft grows from tail to head over ~0.3s with a tiny overshoot-and-settle bounce, then the arrowhead pops in. pop: shaft, head and label all scale-overshoot in together. fade: opacity only, no motion."})),
            ("track_id", s("track to place the arrow on; default: a freshly created track")),
            ("dry_run", json!({"type":"boolean","description":"report the geometry that would be placed, change nothing. Default false."})),
        ], &["target", "at_s"]),
        run,
    }]
}
