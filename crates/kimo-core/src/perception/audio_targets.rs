//! Shared target acquisition: the strict decoder/mixer used by audio_probe, no alternate decoder.
use crate::{commands::Session, model::Source};
use serde_json::Value;
pub struct AudioTarget {
    pub pcm: Vec<f32>,
    pub origin: f64,
    pub source: &'static str,
}
fn error(e: crate::audio::AudioError) -> String {
    match e {
        crate::audio::AudioError::InvalidInput(e) if e.starts_with("invalid_filter:") => e,
        crate::audio::AudioError::NoAudioTrack => "no_audio: target has no audio stream".into(),
        other => format!("decode_failed: {other}"),
    }
}
pub fn decode(ss: &Session, target: &Value) -> Result<AudioTarget, String> {
    let value = target.as_str();
    let clip = target
        .get("clip_id")
        .and_then(Value::as_str)
        .or_else(|| value.filter(|id| ss.project.find_clip(id).is_some()));
    if let Some(id) = clip {
        let c = ss.project.find_clip(id).ok_or("no_audio: clip not found")?;
        let Source::Media { asset } = &c.source else {
            return Err("no_audio: media clip required".into());
        };
        let a = ss
            .project
            .assets
            .get(asset)
            .ok_or("decode_failed: asset missing")?;
        return Ok(AudioTarget {
            pcm: crate::audio::strict_decode(&ss.project.resolve(&a.path), c).map_err(error)?,
            origin: c.start,
            source: "clip post-filter pre-volume/pan",
        });
    }
    let track = target
        .get("track_id")
        .and_then(Value::as_str)
        .or_else(|| value.filter(|id| ss.project.tracks.iter().any(|t| t.id == *id)));
    if let Some(id) = track {
        let mut p = ss.project.clone();
        p.tracks.retain(|t| t.id == id);
        if p.tracks.is_empty() {
            return Err("no_audio: track not found".into());
        }
        return Ok(AudioTarget {
            pcm: crate::audio::mix_strict(&p).map_err(error)?,
            origin: 0.,
            source: "track post mix/limiter",
        });
    }
    if target.is_null()
        || value == Some("mix")
        || target.get("mix").and_then(Value::as_bool) == Some(true)
    {
        return Ok(AudioTarget {
            pcm: crate::audio::mix_strict(&ss.project).map_err(error)?,
            origin: 0.,
            source: "timeline post mix/limiter",
        });
    }
    let path = target
        .get("path")
        .and_then(Value::as_str)
        .or(value)
        .ok_or("unknown target. Valid: clip_id, track_id, mix, path")?;
    let path = ss.project.resolve(path);
    let (_, _, duration, _) =
        crate::render::probe(&path).map_err(|e| format!("decode_failed: {e}"))?;
    // Obtain a standard default clip model, without mutating the caller's session.
    let mut tmp = Session::new();
    let tid = crate::commands::dispatch(&mut tmp, "add_track", &serde_json::json!({}))?["track_id"]
        .clone();
    crate::commands::dispatch(
        &mut tmp,
        "add_clip",
        &serde_json::json!({"track_id":tid,"source_type":"solid","start":0,"duration":duration}),
    )?;
    let c = &tmp.project.tracks[0].clips[0];
    Ok(AudioTarget {
        pcm: crate::audio::strict_decode(&path, c).map_err(error)?,
        origin: 0.,
        source: "path decoded source seconds",
    })
}
