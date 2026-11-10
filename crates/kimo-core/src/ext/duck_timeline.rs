//! duck_timeline (v7.5): auto_duck ducks exactly one music `track` under one dialogue `under` track per
//! call. A real timeline often has more than one dialogue track (two lavs, a boom plus a lav, an
//! interviewer and a guest) and more than one music bed (a theme plus a stinger); getting every pair
//! covered today needs one auto_duck call per (music, dialogue) combination, and a line said on EITHER
//! dialogue track still only ducks the ONE music track that call named -- there is no way to say "duck
//! every music track under whichever dialogue track is talking" in one call.
//!
//! duck_timeline names every dialogue track and every music track once. It runs kimo's own VAD once per
//! dialogue track (the same detector auto_duck's own `detect: "speech"` mode already uses) to get that
//! track's speech windows, concatenates the windows across every dialogue track, and hands the union to
//! EVERY named music track through auto_duck's own existing `speech_windows` path -- one `dispatch_nested`
//! call to `auto_duck` per music track, each with the same combined window list. auto_duck's ducking math
//! (envelope, attack/release, the `duck_gain` keyframes) is not reimplemented here at all; this tool is
//! purely the "detect once across N tracks, apply to M tracks" layer on top of it.
use super::kit::*;
use crate::commands::dispatch_nested as call;

fn ids_arg(a: &Value, k: &str) -> Result<Vec<String>, String> {
    a.get(k).and_then(|v| v.as_array()).ok_or_else(|| crate::errors::arg_err(a, k, "array of track id strings"))?
        .iter().map(|v| v.as_str().map(String::from).ok_or_else(|| crate::errors::arg_err(a, k, "array of track id strings")))
        .collect()
}

fn speech_windows_for(ss: &Session, track_id: &str, hold: f64) -> Result<Vec<(f64, f64)>, String> {
    let mut p = ss.project.clone();
    p.tracks.retain(|t| t.id == track_id);
    if p.tracks.is_empty() { return Err(format!("no such track '{track_id}'")); }
    let pcm = crate::audio::mix_strict(&p).map_err(|e| e.to_string())?;
    let vad = crate::perception::vad::analyze(&pcm, &crate::perception::vad::Params::default())
        .map_err(|e| format!("speech detection on track '{track_id}': {e}"))?;
    Ok(vad.speech.iter().filter(|w| w.end > w.start).map(|w| (w.start, w.end + hold)).collect())
}

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let dialogue_tracks = ids_arg(a, "dialogue_tracks")?;
    let music_tracks = ids_arg(a, "music_tracks")?;
    if dialogue_tracks.is_empty() { return Err("dialogue_tracks is empty: name at least one dialogue track".into()); }
    if music_tracks.is_empty() { return Err("music_tracks is empty: name at least one music track".into()); }
    for m in &music_tracks {
        if dialogue_tracks.contains(m) { return Err(format!("track '{m}' is named in both dialogue_tracks and music_tracks: a track cannot duck under its own speech")); }
    }
    for tid in dialogue_tracks.iter().chain(music_tracks.iter()) {
        if !ss.project.tracks.iter().any(|t| &t.id == tid) { return Err(format!("no such track '{tid}'")); }
    }
    let amount_db = a.get("amount_db").and_then(|v| v.as_f64()).unwrap_or(-12.0);
    let attack = a.get("attack").and_then(|v| v.as_f64()).unwrap_or(0.1);
    let release = a.get("release").and_then(|v| v.as_f64()).unwrap_or(0.3);
    let hold = a.get("hold").and_then(|v| v.as_f64()).unwrap_or(0.0);
    if !(0.0..=5.0).contains(&hold) { return Err("hold must be 0..5".into()); }

    // One VAD pass per dialogue track, concatenated raw (un-merged -- auto_duck's own speech_windows path
    // sorts and merges overlapping windows itself, the same as its single-track detect:"speech" branch).
    let mut windows: Vec<Value> = vec![];
    for tid in &dialogue_tracks {
        for (s, e) in speech_windows_for(ss, tid, hold)? { windows.push(json!([s, e])); }
    }

    let mut ducked = 0usize;
    let mut per_track: Vec<Value> = vec![];
    for m in &music_tracks {
        let r = call(ss, "auto_duck", &json!({"track": m, "speech_windows": windows, "amount_db": amount_db, "attack": attack, "release": release}))?;
        ducked += 1;
        per_track.push(json!({"track_id": m, "clips": r.get("clips").cloned().unwrap_or(json!(0)), "speech_windows_used": r.get("speech_windows").cloned().unwrap_or(json!(0))}));
    }

    Ok(json!({
        "music_tracks_ducked": ducked,
        "dialogue_tracks_used": dialogue_tracks.len(),
        "speech_windows_detected": windows.len(),
        "per_music_track": per_track,
        "amount_db": amount_db,
    }))
}

pub fn commands() -> Vec<Command> {
    vec![Command {
        name: "duck_timeline",
        description: "Duck every named music track under speech from ANY of the named dialogue tracks, in one call. auto_duck ducks exactly one music track under one dialogue track per call; a timeline with more than one dialogue track (two lavs, a boom plus a lav) or more than one music bed needs one auto_duck call per pair today, and no single auto_duck call can say \"duck this music under whichever of these two people is talking\". duck_timeline runs kimo's own speech detector once per dialogue_tracks entry, combines every detected speech window across all of them, and hands that combined window list to auto_duck's own speech_windows argument once per music track -- auto_duck's ducking math is reused, not duplicated. A track named in both lists is an error. amount_db/attack/release/hold match auto_duck's own speech-mode defaults (-12dB, 0.1/0.3s, hold 0).",
        mutates: true,
        schema: obj(vec![
            ("dialogue_tracks", json!({"type":"array","items":{"type":"string"},"description":"every track whose speech should duck the music, at least one"})),
            ("music_tracks", json!({"type":"array","items":{"type":"string"},"description":"every track to duck, at least one"})),
            ("amount_db", n("how far to duck, dB, default -12")),
            ("attack", n("seconds to duck down, default 0.1")),
            ("release", n("seconds to come back up, default 0.3")),
            ("hold", n("seconds the duck stays down after speech ends before releasing, default 0, max 5")),
        ], &["dialogue_tracks", "music_tracks"]),
        run,
    }]
}
