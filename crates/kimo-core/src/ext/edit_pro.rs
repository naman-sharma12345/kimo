//! Three-point editing and exact timecode. insert_edit / overwrite_edit put a source range (in and out, or in and a
//! duration) onto a track at a time, the way an NLE's insert (",") and overwrite (".") do: insert pushes later
//! material along (on the track, or on every unlocked track with sync locks), overwrite replaces what is there.
//! timecode converts between seconds, frames and SMPTE timecode, with exact 29.97 / 59.94 drop-frame counting
//! (craft-libs' craft-time).
use super::kit::*;
use craft_timeline::time::{format_timecode_frames, parse_timecode, FrameRate};

fn range(ss: &Session, a: &Value) -> Result<(String, f64, f64), String> {
    let asset = str_arg(a, "asset_id")?.to_string(); let src_in = opt_num(a, "source_in", 0.0);
    let dur = match (a.get("source_out").and_then(|v| v.as_f64()), a.get("duration").and_then(|v| v.as_f64())) { (Some(o), None) => o - src_in, (None, Some(d)) => d, (Some(_), Some(_)) => return Err("give source_out or duration, not both".into()), (None, None) => return Err("give source_out (the source out point) or duration".into()) };
    let ad = ss.project.assets.get(&asset).ok_or_else(|| format!("no asset '{asset}'"))?.duration;
    if !(src_in >= 0.0) || !(dur > 0.0) { return Err("need 0 <= source_in < source_out".into()); }
    if ad > 0.0 && src_in + dur > ad + 1e-6 { return Err(format!("the range ends at {:.3} s, past the media's {:.3} s", src_in + dur, ad)); }
    Ok((asset, src_in, dur))
}
fn place(ss: &mut Session, tid: &str, asset: &str, at: f64, src_in: f64, dur: f64) -> Result<String, String> {
    Ok(crate::commands::dispatch_nested(ss, "add_clip", &json!({"track_id": tid, "source_type": "media", "asset_id": asset, "start": at, "duration": dur, "in_point": src_in}))?["clip_id"].as_str().unwrap_or_default().to_string())
}

fn insert_edit(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let (asset, src_in, dur) = range(ss, a)?; let tid = str_arg(a, "track_id")?.to_string(); let at = num_arg(a, "at")?;
    if !ss.project.tracks.iter().any(|t| t.id == tid) { return Err(format!("no track '{tid}'")); }
    let all = a.get("sync").and_then(|v| v.as_str()).unwrap_or("track") == "all";
    let tracks: Vec<String> = if all { ss.project.tracks.iter().filter(|t| !t.locked).map(|t| t.id.clone()).collect() } else { vec![tid.clone()] };
    for t in &tracks { crate::commands::dispatch_nested(ss, "insert_gap", &json!({"track_id": t, "at": at, "duration": dur}))?; }
    let cid = place(ss, &tid, &asset, at, src_in, dur)?;
    let mk: Vec<Value> = ss.project.markers.iter().filter(|m| all && m.time >= at).map(|m| json!(m.id)).collect(); if all { for m in ss.project.markers.iter_mut().filter(|m| m.time >= at) { m.time += dur; } }
    Ok(json!({"clip_id": cid, "inserted": dur, "rippled_tracks": tracks, "markers_moved": mk.len()}))
}
fn overwrite_edit(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let (asset, src_in, dur) = range(ss, a)?; let tid = str_arg(a, "track_id")?.to_string(); let at = num_arg(a, "at")?;
    if !ss.project.tracks.iter().any(|t| t.id == tid) { return Err(format!("no track '{tid}'")); }
    let covered = ss.project.tracks.iter().find(|t| t.id == tid).map_or(false, |t| t.clips.iter().any(|c| c.start < at + dur - 1e-9 && c.start + c.duration > at + 1e-9));
    if covered { crate::commands::dispatch_nested(ss, "lift_range", &json!({"track_id": tid, "start": at, "end": at + dur, "extract": false}))?; }
    let cid = place(ss, &tid, &asset, at, src_in, dur)?;
    Ok(json!({"clip_id": cid, "replaced": if covered { dur } else { 0.0 }}))
}

fn timecode(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let fps = opt_num(a, "fps", ss.project.fps); if !(fps > 0.0) { return Err("fps must be > 0".into()); }
    let rate = FrameRate::from_f64(fps); let df_ok = (fps - 29.97).abs() < 0.01 || (fps - 59.94).abs() < 0.01;
    let drop = a.get("drop_frame").and_then(|v| v.as_bool()).unwrap_or(df_ok); if drop && !df_ok { return Err("drop-frame timecode exists only at 29.97 and 59.94 fps".into()); }
    let frame = match (a.get("seconds").and_then(|v| v.as_f64()), a.get("timecode").and_then(|v| v.as_str()), a.get("frames").and_then(|v| v.as_i64())) {
        (Some(s), None, None) => (s * rate.as_f64()).round() as i64,
        (None, Some(tc), None) => parse_timecode(tc, rate, drop, 0).map_err(|e| format!("bad timecode '{tc}': {e:?}"))?,
        (None, None, Some(f)) => f,
        _ => return Err("give exactly one of seconds, timecode or frames".into()),
    };
    Ok(json!({"frames": frame, "seconds": frame as f64 / rate.as_f64(), "timecode": format_timecode_frames(frame, rate, drop), "fps": rate.as_f64(), "drop_frame": drop,
        "non_drop_timecode": format_timecode_frames(frame, rate, false)}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "insert_edit", description: "Three-point insert: put asset_id's source range (source_in plus source_out or duration) on track_id at `at`, pushing later material along (sync: track, or all = every unlocked track and the markers, like sync locks). A clip spanning `at` is split.", mutates: true,
        schema: obj(vec![("asset_id", s("source media")), ("source_in", n("source in point s, default 0")), ("source_out", n("source out point s")), ("duration", n("or the length s")), ("track_id", s("target track")), ("at", n("timeline s")), ("sync", json!({"type":"string","enum":["track","all"]}))], &["asset_id","track_id","at"]),
        run: insert_edit },
    Command { name: "overwrite_edit", description: "Three-point overwrite: put asset_id's source range on track_id at `at`, replacing whatever is there (it is lifted, nothing moves).", mutates: true,
        schema: obj(vec![("asset_id", s("source media")), ("source_in", n("source in point s, default 0")), ("source_out", n("source out point s")), ("duration", n("or the length s")), ("track_id", s("target track")), ("at", n("timeline s"))], &["asset_id","track_id","at"]),
        run: overwrite_edit },
    Command { name: "timecode", description: "Convert between seconds, frames and SMPTE timecode at a frame rate (default the project's), with exact drop-frame at 29.97 and 59.94 (default there). Give one of seconds, timecode (01:00:00;00 or 01:00:00:00) or frames.", mutates: false,
        schema: obj(vec![("seconds", n("time s")), ("timecode", s("SMPTE timecode")), ("frames", n("frame count")), ("fps", n("frame rate")), ("drop_frame", json!({"type":"boolean"}))], &[]),
        run: timecode },
] }
