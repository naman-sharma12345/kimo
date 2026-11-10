//! Insert vs overwrite on `add_clip` lives on `add_clip` itself (see commands/tools_0.rs's `mode` param,
//! built on `crate::edit::do_insert_gap` / `do_lift_range`). This file is its two remaining pieces
//! (FEATURES.md "...three/four-point edit, source in/out marks"):
//!
//! - Source in/out marks: `mark_source` / `clear_source_marks` / `get_source_marks` manage an ephemeral,
//!   per-asset (in, out) pair on `Session.source_marks` -- a real NLE's source-viewer marks, not part of the
//!   saved project (Session is never serialized; a project round-trip through save/load simply has none).
//! - Three/four-point edit: `edit_points` places a clip given any 3 of {source_in, source_out, timeline_in,
//!   timeline_out} (the 4th is derived) or all 4 (speed is adjusted to fit when the two spans differ and
//!   fit:true is passed, else it is a clear error naming the mismatch). source_in/source_out fall back to a
//!   stored source mark on the asset when omitted, so marking a source then calling edit_points with only the
//!   two timeline points is a real, working 3-point edit. Delegates to `add_clip` (mode: insert/overwrite)
//!   for the actual placement, so it is byte-identical to calling add_clip by hand with the derived numbers.
use super::kit::*;

const EPS: f64 = 1e-6;

fn mark_of(ss: &Session, asset_id: &str) -> (Option<f64>, Option<f64>) { ss.source_marks.get(asset_id).copied().unwrap_or((None, None)) }

pub fn commands() -> Vec<Command> { vec![
    Command { name: "mark_source", description: "Set a source in and/or out mark (seconds) on an asset, for edit_points' three-point edit. Omitting one keeps the other mark as it was.", mutates: false,
        schema: obj(vec![("asset_id", s("asset")), ("in_point", n("source seconds, optional")), ("out_point", n("source seconds, optional"))], &["asset_id"]),
        run: |ss, a| {
            let id = str_arg(a, "asset_id")?;
            if !ss.project.assets.contains_key(id) { return Err(format!("no asset '{id}'")); }
            if a.get("in_point").is_none() && a.get("out_point").is_none() { return Err("pass in_point, out_point, or both".into()); }
            let (mut ip, mut op) = mark_of(ss, id);
            if let Some(v) = a.get("in_point") { let v = v.as_f64().filter(|v| v.is_finite() && *v >= 0.0).ok_or("in_point must be a finite number >= 0")?; ip = Some(v); }
            if let Some(v) = a.get("out_point") { let v = v.as_f64().filter(|v| v.is_finite() && *v >= 0.0).ok_or("out_point must be a finite number >= 0")?; op = Some(v); }
            if let (Some(i), Some(o)) = (ip, op) { if o <= i { return Err(format!("out_point {o} must be > in_point {i}")); } }
            let dur = ss.project.assets[id].duration;
            if let Some(o) = op { if dur > 0.0 && o > dur + EPS { return Err(format!("out_point {o} is past the asset's known duration {dur}")); } }
            ss.source_marks.insert(id.to_string(), (ip, op));
            Ok(json!({"asset_id": id, "in_point": ip, "out_point": op}))
        } },
    Command { name: "clear_source_marks", description: "Clear the source in/out marks on one asset (asset_id), or every asset's marks when asset_id is omitted.", mutates: false,
        schema: obj(vec![("asset_id", s("asset, optional: omit to clear every mark"))], &[]),
        run: |ss, a| {
            match a.get("asset_id").and_then(|v| v.as_str()) {
                Some(id) => { let had = ss.source_marks.remove(id).is_some(); Ok(json!({"cleared": had, "asset_id": id})) }
                None => { let n = ss.source_marks.len(); ss.source_marks.clear(); Ok(json!({"cleared_count": n})) }
            }
        } },
    Command { name: "get_source_marks", description: "Read the source in/out marks on one asset (asset_id), or every asset that has any mark set when asset_id is omitted.", mutates: false,
        schema: obj(vec![("asset_id", s("asset, optional: omit to list every marked asset"))], &[]),
        run: |ss, a| {
            match a.get("asset_id").and_then(|v| v.as_str()) {
                Some(id) => { let (ip, op) = mark_of(ss, id); Ok(json!({"asset_id": id, "in_point": ip, "out_point": op})) }
                None => { let marks: Vec<Value> = ss.source_marks.iter().map(|(id, (ip, op))| json!({"asset_id": id, "in_point": ip, "out_point": op})).collect(); Ok(json!({"marks": marks})) }
            }
        } },
    Command { name: "edit_points", description: "Three- or four-point edit: place a media clip on track_id given any 3 of source_in/source_out/timeline_in/timeline_out (the 4th is derived) or all 4.", mutates: true,
        schema: obj(vec![("track_id", s("target track")), ("asset_id", s("media asset")),
            ("source_in", n("source seconds; omitted falls back to a mark_source in_point on this asset")), ("source_out", n("source seconds; omitted falls back to a mark_source out_point on this asset")),
            ("timeline_in", n("timeline seconds")), ("timeline_out", n("timeline seconds")),
            ("fit", json!({"type":"boolean","description":"four-point edit only: when the source span and timeline span differ, adjust playback speed to fit instead of erroring"})),
            ("mode", json!({"type":"string","enum":["insert","overwrite"],"description":"passed straight to add_clip; default overwrite (clears the destination span, nothing ripples)"})),
            ("ripple_tracks", json!({"type":"array","items":{"type":"string"},"description":"mode:\"insert\" only, forwarded to add_clip"}))], &["track_id","asset_id"]),
        run: |ss, a| {
            let tid = str_arg(a, "track_id")?.to_string();
            let aid = str_arg(a, "asset_id")?.to_string();
            if !ss.project.assets.contains_key(&aid) { return Err(format!("no asset '{aid}'")); }
            let fit = a.get("fit").and_then(|v| v.as_bool()).unwrap_or(false);
            let mode = match a.get("mode") { None | Some(Value::Null) => "overwrite".to_string(), Some(Value::String(s)) if s == "insert" || s == "overwrite" => s.clone(), Some(Value::String(s)) => return Err(format!("mode must be insert or overwrite, got '{s}'")), _ => return Err("mode must be a string".into()) };
            let ripple_tracks: Vec<String> = match a.get("ripple_tracks") { None | Some(Value::Null) => vec![], Some(Value::Array(v)) => v.iter().map(|x| x.as_str().map(String::from).ok_or_else(|| "ripple_tracks must be an array of strings".to_string())).collect::<Result<_,_>>()?, _ => return Err("ripple_tracks must be an array of strings".into()) };

            let marks = mark_of(ss, &aid);
            let arg_f64 = |k: &str| -> Result<Option<f64>, String> { match a.get(k) { None | Some(Value::Null) => Ok(None), Some(v) => v.as_f64().filter(|v| v.is_finite()).map(Some).ok_or_else(|| format!("{k} must be a finite number")) } };
            let from_mark = |explicit: Option<f64>, mark: Option<f64>| explicit.or(mark);
            let source_in = from_mark(arg_f64("source_in")?, marks.0);
            let source_out = from_mark(arg_f64("source_out")?, marks.1);
            let timeline_in = arg_f64("timeline_in")?;
            let timeline_out = arg_f64("timeline_out")?;

            let given = [source_in.is_some(), source_out.is_some(), timeline_in.is_some(), timeline_out.is_some()];
            let n_given = given.iter().filter(|x| **x).count();
            if n_given < 3 {
                return Err(format!("edit_points needs 3 of source_in/source_out/timeline_in/timeline_out for a three-point edit (or all 4 for a four-point edit); got {n_given} (source_in/source_out can come from mark_source -- none set on '{aid}' right now)"));
            }

            let (speed, si, so, ti, to) = match (source_in, source_out, timeline_in, timeline_out) {
                (Some(si), Some(so), Some(ti), None) => { if so <= si { return Err(format!("source_out {so} must be > source_in {si}")); } (1.0, si, so, ti, ti + (so - si)) }
                (Some(si), Some(so), None, Some(to)) => { if so <= si { return Err(format!("source_out {so} must be > source_in {si}")); } let span = so - si; if to <= span { return Err(format!("timeline_out {to} is too small for a derived timeline_in (source span {span}s)")); } (1.0, si, so, to - span, to) }
                (Some(si), None, Some(ti), Some(to)) => { if to <= ti { return Err(format!("timeline_out {to} must be > timeline_in {ti}")); } (1.0, si, si + (to - ti), ti, to) }
                (None, Some(so), Some(ti), Some(to)) => { if to <= ti { return Err(format!("timeline_out {to} must be > timeline_in {ti}")); } let span = to - ti; if so <= span { return Err(format!("source_out {so} is too small for a derived source_in (timeline span {span}s)")); } (1.0, so - span, so, ti, to) }
                (Some(si), Some(so), Some(ti), Some(to)) => {
                    if so <= si { return Err(format!("source_out {so} must be > source_in {si}")); }
                    if to <= ti { return Err(format!("timeline_out {to} must be > timeline_in {ti}")); }
                    let (src_span, tl_span) = (so - si, to - ti);
                    if (src_span - tl_span).abs() > EPS {
                        if !fit { return Err(format!("four-point edit: source span {src_span:.3}s and timeline span {tl_span:.3}s differ; pass fit:true to adjust playback speed, or drop one of the four marks for a three-point edit")); }
                        (src_span / tl_span, si, so, ti, to)
                    } else { (1.0, si, so, ti, to) }
                }
                _ => unreachable!("n_given >= 3 guarantees exactly one or zero of the four is None"),
            };
            if si < 0.0 { return Err(format!("source_in {si} must be >= 0")); }
            if ti < 0.0 { return Err(format!("timeline_in {ti} must be >= 0")); }
            let dur = ss.project.assets[&aid].duration;
            if dur > 0.0 && so > dur + EPS { return Err(format!("source_out {so} is past the asset's known duration {dur}")); }

            let mut place = json!({"track_id": tid, "source_type": "media", "asset_id": aid, "start": ti, "duration": to - ti, "in_point": si, "speed": speed, "mode": mode});
            if !ripple_tracks.is_empty() { place["ripple_tracks"] = json!(ripple_tracks); }
            let r = crate::commands::dispatch(ss, "add_clip", &place)?;
            Ok(json!({"clip_id": r["clip_id"], "source_in": si, "source_out": so, "timeline_in": ti, "timeline_out": to, "speed": speed, "point_count": n_given}))
        } },
] }
