//! Sound design: whooshes, risers, impacts, sub drops, reverse hits and clicks, synthesized on the spot (procedural, no
//! sampled or downloaded audio, so nothing to license), and placed on the timeline's cuts or a music track's bar grid.
//!
//! Each sound is rendered once per (kind, length) into the OS temp folder and reused. Timing follows how editors place
//! them: a whoosh peaks ON the cut, a riser and a reverse hit END on their mark, an impact and a sub drop START on it.
use super::kit::*;

pub const KINDS: [&str; 6] = ["whoosh", "riser", "impact", "sub_drop", "reverse_hit", "click"];

/// ffmpeg lavfi source + filter for a sound of `kind` lasting `d` seconds (48 kHz stereo).
fn recipe(kind: &str, d: f64) -> Option<String> {
    Some(match kind {
        // band-passed noise swelling to the middle and away, with a phaser for movement
        "whoosh" => format!("anoisesrc=d={d}:c=pink:a=0.7:r=48000,highpass=f=250,lowpass=f=7000,aphaser=type=t:speed=1.6,afade=t=in:d={:.3}:curve=qsin,afade=t=out:st={:.3}:d={:.3}:curve=qsin,volume=1.4", d * 0.55, d * 0.55, d * 0.45),
        // a rising sine sweep under filtered noise, growing to the end and stopping dead
        "riser" => format!("aevalsrc='0.35*sin(2*PI*(180*t+({k})*t*t))+0.25*(random(0)*2-1)*t/{d}':s=48000:d={d},highpass=f=120,afade=t=in:d={d}:curve=exp,volume=1.3", k = 1400.0 / (2.0 * d)),
        // a low thump with a fast noise transient
        "impact" => format!("aevalsrc='0.95*sin(2*PI*52*t)*exp(-5*t)+0.45*(random(0)*2-1)*exp(-38*t)':s=48000:d={d},lowpass=f=5000,volume=1.2"),
        // a falling sub sine (90 -> ~30 Hz)
        "sub_drop" => format!("aevalsrc='0.95*sin(2*PI*(90*t-({k})*t*t))*exp(-1.6*t)':s=48000:d={d},volume=1.3", k = 60.0 / (2.0 * d)),
        // an impact played backwards: it swells into its mark
        "reverse_hit" => format!("aevalsrc='0.9*sin(2*PI*60*t)*exp(-4*t)+0.4*(random(0)*2-1)*exp(-9*t)':s=48000:d={d},areverse,lowpass=f=6000,volume=1.2"),
        "click" => format!("aevalsrc='0.8*sin(2*PI*2200*t)*exp(-90*t)':s=48000:d={d}"),
        _ => return None,
    })
}
fn default_len(kind: &str) -> f64 { match kind { "whoosh" => 0.8, "riser" => 2.0, "impact" => 1.2, "sub_drop" => 1.5, "reverse_hit" => 1.0, _ => 0.08 } }
/// Where the sound starts relative to its mark: a whoosh peaks on it, a riser / reverse hit ends on it, the rest start on it.
fn lead(kind: &str, d: f64) -> f64 { match kind { "whoosh" => d * 0.55, "riser" | "reverse_hit" => d, _ => 0.0 } }

fn render(kind: &str, d: f64) -> Result<String, String> {
    let src = recipe(kind, d).ok_or_else(|| format!("kind must be one of {}", KINDS.join("|")))?;
    let path = std::env::temp_dir().join(format!("kimo-sfx-v1-{kind}-{}.wav", (d * 1000.0).round() as u64));
    if path.exists() { return Ok(path.to_string_lossy().into_owned()); }
    let tmp = path.with_extension("part.wav");
    let mut cmd = std::process::Command::new("ffmpeg");
    cmd.args(["-v", "error", "-y", "-f", "lavfi", "-i", &src, "-ac", "2", "-ar", "48000", "-c:a", "pcm_s16le"]).arg(&tmp);
    crate::color::process::wait_command(&mut cmd, std::time::Duration::from_secs(30)).map_err(|e| format!("could not synthesize {kind}: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| e.to_string())?;
    Ok(path.to_string_lossy().into_owned())
}

/// Places `kind` at each mark (timeline seconds) on one new audio track. Returns the track and clip ids.
fn place(ss: &mut Session, kind: &str, marks: &[f64], d: f64, volume: f64, track: Option<String>) -> Result<Value, String> {
    let path = render(kind, d)?;
    let asset = crate::commands::dispatch_nested(ss, "import_media", &json!({"path": path}))?["asset_id"].as_str().ok_or("import_media returned no asset")?.to_string();
    let tid = match track { Some(t) => t, None => { let t = crate::commands::dispatch_nested(ss, "add_track", &json!({"name": format!("SFX {kind}")}))?["track_id"].as_str().ok_or("add_track returned no track")?.to_string(); t } };
    let mut ids = vec![]; let mut last_end = f64::NEG_INFINITY;
    for &m in marks {
        let start = (m - lead(kind, d)).max(0.0); if start < last_end - 1e-6 { continue; } // never stack two on one track
        let r = crate::commands::dispatch_nested(ss, "add_audio_clip", &json!({"asset_id": asset, "track_id": tid, "start": (start * 1000.0).round() / 1000.0, "duration": d, "volume": volume}))?;
        if let Some(id) = r["clip_id"].as_str() { ids.push(id.to_string()); } last_end = start + d;
    }
    Ok(json!({"track_id": tid, "clip_ids": ids, "kind": kind, "length": d, "placed": ids.len()}))
}

fn add_sfx(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let kind = str_arg(a, "kind")?.to_string(); if !KINDS.contains(&kind.as_str()) { return Err(format!("kind must be one of {}", KINDS.join("|"))); }
    let d = opt_num(a, "duration", default_len(&kind)).clamp(0.03, 8.0); let vol = opt_num(a, "volume", 0.7).clamp(0.0, 4.0);
    let marks: Vec<f64> = match a.get("at") { Some(Value::Number(n)) => vec![n.as_f64().unwrap_or(0.0)], Some(Value::Array(v)) => v.iter().filter_map(|x| x.as_f64()).collect(), _ => return Err("at: a timeline time or a list of times".into()) };
    place(ss, &kind, &marks, d, vol, a.get("track_id").and_then(|v| v.as_str()).map(String::from))
}

fn asset_of(c: &crate::model::Clip) -> Option<&String> { if let crate::model::Source::Media { asset } = &c.source { Some(asset) } else { None } }

/// Cut points: where a video clip starts right where another ended (or after a gap), on the visual tracks.
fn cuts(ss: &Session) -> Vec<f64> {
    let mut v: Vec<f64> = ss.project.tracks.iter().flat_map(|t| t.clips.iter()).filter(|c| matches!(c.source, crate::model::Source::Media { .. }) && c.start > 0.05)
        .filter(|c| asset_of(c).and_then(|id| ss.project.assets.get(id)).map_or(false, |x| x.width > 0)).map(|c| c.start).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal)); v.dedup_by(|a, b| (*a - *b).abs() < 0.1); v
}

fn add_sfx_on(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let kind = a.get("kind").and_then(|v| v.as_str()).unwrap_or("whoosh").to_string(); if !KINDS.contains(&kind.as_str()) { return Err(format!("kind must be one of {}", KINDS.join("|"))); }
    let on = a.get("on").and_then(|v| v.as_str()).unwrap_or("cuts");
    let mut marks: Vec<f64> = match on {
        "cuts" => cuts(ss),
        "downbeats" | "sections" | "drops" => {
            let cid = str_arg(a, "music_clip_id").map_err(|_| "music_clip_id: the music clip whose bar grid to use".to_string())?.to_string();
            let c = ss.project.find_clip(&cid).ok_or_else(|| format!("no clip '{cid}'"))?.clone();
            let path = asset_of(&c).and_then(|id| ss.project.assets.get(id)).map(|x| x.path.clone()).ok_or("the music clip has no media file")?;
            let ms = super::music_structure::analyze(&path, c.in_point, c.duration * c.speed.abs().max(1e-6), 4)?;
            let key = match on { "downbeats" => "downbeats", "drops" => "drops", _ => "sections" };
            let src: Vec<f64> = if key == "sections" { ms["sections"].as_array().map(|v| v.iter().filter_map(|s| s["start"].as_f64()).collect()).unwrap_or_default() } else { ms[key].as_array().map(|v| v.iter().filter_map(|x| x.as_f64()).collect()).unwrap_or_default() };
            // source time -> timeline time
            src.into_iter().map(|t| c.start + (t - c.in_point) / c.speed.abs().max(1e-6)).filter(|t| *t >= c.start - 1e-6 && *t <= c.start + c.duration + 1e-6).collect()
        }
        o => return Err(format!("on must be cuts|downbeats|sections|drops, got '{o}'")),
    };
    let every = a.get("every").and_then(|v| v.as_u64()).unwrap_or(1).max(1) as usize; marks = marks.into_iter().step_by(every).collect();
    if let Some(m) = a.get("max").and_then(|v| v.as_u64()) { marks.truncate(m as usize); }
    if marks.is_empty() { return Ok(json!({"placed": 0, "note": format!("no {on} found"), "no_history": true})); }
    let d = opt_num(a, "duration", default_len(&kind)).clamp(0.03, 8.0); let vol = opt_num(a, "volume", 0.6).clamp(0.0, 4.0);
    let mut r = place(ss, &kind, &marks, d, vol, None)?; r["on"] = json!(on); Ok(r)
}

pub fn commands() -> Vec<Command> {
    let kinds = json!({"type":"string","enum":KINDS});
    vec![
        Command { name: "add_sfx", description: "Add a synthesized sound effect (whoosh, riser, impact, sub_drop, reverse_hit, click) at one or more timeline times. A whoosh peaks on its mark, a riser and a reverse hit end on it, an impact and a sub drop start on it. Procedural, nothing downloaded.", mutates: true,
            schema: obj(vec![("kind", kinds.clone()), ("at", json!({"description":"timeline seconds, or a list of them","anyOf":[{"type":"number"},{"type":"array","items":{"type":"number"}}]})), ("duration", n("seconds (default by kind)")), ("volume", n("default 0.7")), ("track_id", s("audio track (default: a new one)"))], &["kind","at"]),
            run: add_sfx },
        Command { name: "add_sfx_on", description: "Sound design in one call: a synthesized effect on every cut, or on a music clip's downbeats, section starts or drops (its bar grid from music_structure). e.g. whooshes on cuts, a riser into each section, an impact on each drop. every:N takes every Nth mark; max caps the count.", mutates: true,
            schema: obj(vec![("kind", kinds), ("on", json!({"type":"string","enum":["cuts","downbeats","sections","drops"],"description":"default cuts"})), ("music_clip_id", s("for downbeats/sections/drops: the music clip")), ("every", n("every Nth mark, default 1")), ("max", n("at most this many")), ("duration", n("seconds (default by kind)")), ("volume", n("default 0.6"))], &[]),
            run: add_sfx_on },
    ]
}
