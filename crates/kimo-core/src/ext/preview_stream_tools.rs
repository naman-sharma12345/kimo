//! `preview_stream`: real-time preview with sound for an editor's viewer (crate::preview_stream). One action with an `op`,
//! because the ops share one background session: start, update (playhead / speed / play state, also the poll), frame
//! (one scrub frame now), status and stop. Read-only: never changes the project; writes only into the caller's `dir`.
use super::kit::*;

fn f(a: &Value, k: &str) -> Option<f64> { a.get(k).and_then(|v| v.as_f64()).filter(|x| x.is_finite()) }

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let op = a.get("op").and_then(|v| v.as_str()).unwrap_or("update");
    let t = f(a, "time").unwrap_or(0.0).max(0.0);
    let rate = f(a, "rate");
    if let Some(r) = rate { if r == 0.0 || r.abs() > 16.0 { return Err("out_of_range: rate must be between -16 and 16 and not 0 (1 = normal speed, 2 = double, -1 = reverse)".into()); } }
    let playing = a.get("playing").and_then(|v| v.as_bool());
    match op {
        "start" => {
            let dir = a.get("dir").and_then(|v| v.as_str()).ok_or("dir is required: the directory the frames and audio chunks are written to")?;
            let scale = f(a, "scale").unwrap_or(0.5);
            crate::preview_stream::start(&ss.project, std::path::Path::new(dir), scale, t, rate.unwrap_or(1.0), playing.unwrap_or(false))
        }
        "update" | "poll" | "seek" => crate::preview_stream::update(&ss.project, t, rate, playing, op == "seek"),
        "frame" => crate::preview_stream::frame(&ss.project, t),
        "status" => Ok(crate::preview_stream::status().unwrap_or(json!({"running": false}))),
        "stop" => Ok(json!({"stopped": crate::preview_stream::stop()})),
        o => Err(format!("unknown op '{o}': start, update, seek, frame, status or stop")),
    }
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "preview_stream", mutates: false,
        description: "Real-time preview with sound for a viewer: renders ahead of the playhead at draft size into a ring buffer. op start {dir, time, scale 0.5 half / 0.25 quarter, rate, playing} begins a session that writes frames (top-down 32-bit BMP) and 1-second 48 kHz WAV chunks of the mix into dir; op update {time, playing, rate} (call it a few times a second while playing) moves the playhead, frees frames behind it and returns the frames and chunks ready ahead; op seek {time, rate} jumps; op frame {time} renders one scrub frame now; op status; op stop deletes the files. Decodes through preview proxies when enabled, else the short-GOP analysis proxies once they exist. Editing the project restarts the session on the next update.",
        schema: obj(vec![
            ("op", json!({"type": "string", "enum": ["start", "update", "seek", "frame", "status", "stop"], "description": "default update"})),
            ("dir", json!({"type": "string", "description": "start: directory for the frame and audio files (created if missing)"})),
            ("time", n("playhead, seconds")),
            ("scale", n("start: 0.5 (half, default) or 0.25 (quarter) of the project size")),
            ("rate", n("speed: 1 normal, 2 / 4 fast forward (every 2nd / 4th frame), negative = reverse; sound only at 1")),
            ("playing", json!({"type": "boolean", "description": "playing renders the whole ring ahead; paused only primes a few frames"})),
        ], &[]),
        run },
] }
