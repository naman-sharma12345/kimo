//! Selectable container subtitle streams from editable caption objects.
use super::kit::*;
use crate::model::{Project, Source};
use std::{
    path::{Path, PathBuf},
    process::Command as Cmd,
};
struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
fn scratch(parent: &Path) -> Result<Scratch, String> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    for _ in 0..100 {
        let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let path = parent.join(format!(".kimo-subs-{}-{id}", std::process::id()));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(Scratch(path)),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Err("cannot create subtitle scratch directory".into())
}
fn stamp(t: f64) -> String {
    let ms = (t.max(0.) * 1000.).round() as u64;
    format!(
        "{:02}:{:02}:{:02},{:03}",
        ms / 3600000,
        ms / 60000 % 60,
        ms / 1000 % 60,
        ms % 1000
    )
}
fn cues(p: &Project, group: &str, start: f64, end: f64) -> Result<(String, usize), String> {
    let g = p.editorial.captions.get(group).ok_or("no caption group")?;
    let mut cues = vec![];
    for id in &g.clips {
        let c = p
            .find_clip(id)
            .ok_or("caption cue deleted or nested; cannot export")?;
        let Source::Text { text, .. } = &c.source else {
            return Err("caption cue is not text".into());
        };
        let a = c.start.max(start) - start;
        let b = (c.start + c.duration).min(end) - start;
        if b <= a {
            continue;
        }
        if text.contains('<') || text.contains('>') {
            return Err(
                "literal angle brackets unsupported by plain SRT mux; edit text before export"
                    .into(),
            );
        }
        let text = text.replace('\r', "");
        if text.contains("\n\n") {
            return Err("caption text has blank lines; edit before SRT export".into());
        }
        cues.push((a, b, text));
    }
    cues.retain(|(a, b, _)| (b * 1000.).round() > (a * 1000.).round());
    cues.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut s = String::new();
    for (i, (a, b, text)) in cues.iter().enumerate() {
        if (b * 1000.).round() <= (a * 1000.).round() {
            continue;
        }
        s += &format!("{}\n{} --> {}\n{text}\n\n", i + 1, stamp(*a), stamp(*b));
    }
    Ok((s, cues.len()))
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let output = str_arg(a, "path")?;
    let target = Path::new(output);
    let ext = target
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
    if ext != "mp4" && ext != "mkv" {
        return Err("soft subtitles require .mp4 or .mkv".into());
    }
    let start = opt_num(a, "start", 0.);
    let end = opt_num(a, "end", ss.project.duration());
    if !start.is_finite()
        || !end.is_finite()
        || start < 0.
        || end <= start
        || end > ss.project.duration() + 1e-8
    {
        return Err("range must be within timeline".into());
    }
    let f0 = (start * ss.project.fps).round();
    let f1 = (end * ss.project.fps).round().max(f0 + 1.);
    let st = f0 / ss.project.fps;
    let en = f1 / ss.project.fps;
    let groups: Vec<String> = match a.get("caption_ids") {
        Some(v) => v
            .as_array()
            .ok_or("caption_ids array required")?
            .iter()
            .map(|v| {
                v.as_str()
                    .map(String::from)
                    .ok_or("caption id string required".into())
            })
            .collect::<Result<_, String>>()?,
        None => ss.project.editorial.captions.keys().cloned().collect(),
    };
    if groups.is_empty() || groups.len() > 32 {
        return Err("1..32 caption streams required".into());
    }
    if groups
        .iter()
        .collect::<std::collections::BTreeSet<_>>()
        .len()
        != groups.len()
    {
        return Err("caption IDs must be unique".into());
    }
    let crf = opt_num(a, "crf", 18.);
    if crf.fract() != 0. || !(0. ..=51.).contains(&crf) {
        return Err("crf integer 0..51 required".into());
    }
    let parent = target
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let tmp = scratch(parent)?;
    let mut counts = vec![];
    for (i, id) in groups.iter().enumerate() {
        let (s, n) = cues(&ss.project, id, st, en)?;
        if n == 0 {
            return Err(format!("caption group {id} has no cues in range"));
        }
        std::fs::write(tmp.0.join(format!("{i}.srt")), s).map_err(|e| e.to_string())?;
        counts.push(n);
    }
    let video = tmp.0.join("video.mp4");
    crate::render::export_range(
        &ss.project,
        video.to_str().ok_or("non-UTF8 output")?,
        crf as u32,
        st,
        en,
    )?;
    let mux = tmp.0.join(format!("final.{ext}"));
    let mut cmd = Cmd::new("ffmpeg");
    cmd.args(["-v", "error", "-y", "-i"]).arg(video);
    for i in 0..groups.len() {
        cmd.arg("-i").arg(tmp.0.join(format!("{i}.srt")));
    }
    cmd.args([
        "-map", "0:v:0", "-map", "0:a?", "-c:v", "copy", "-c:a", "copy",
    ]);
    for (i, id) in groups.iter().enumerate() {
        cmd.args([
            "-map",
            &format!("{}:0", i + 1),
            &format!("-metadata:s:s:{i}"),
            &format!("title={id}"),
            &format!("-disposition:s:{i}"),
            "0",
        ]);
    }
    cmd.args(["-c:s", if ext == "mp4" { "mov_text" } else { "srt" }]);
    if ext == "mp4" {
        cmd.args(["-movflags", "+faststart"]);
    }
    cmd.arg(&mux);
    crate::color::process::wait_command(
        &mut cmd,
        crate::color::process::encode_deadline((f1 - f0) as usize),
    )?;
    std::fs::rename(mux, target).map_err(|e| e.to_string())?;
    Ok(
        json!({"path":output,"caption_ids":groups,"cue_counts":counts,"subtitle_codec":if ext=="mp4"{"mov_text"}else{"subrip"},"start":st,"end":en,"burn_in":"existing per-group settings preserved","subtitle_style":"plain text; presets remain burn-in only"}),
    )
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"export_with_subtitles",description:"Render H.264/AAC with selectable text subtitle streams in MP4 (mov_text) or MKV (SRT), one per caption group. Independent of burn-in settings. Atomic publish after mux. Plain-text soft subs; caption styling stays burn-in. Optional range rounds to frames.",mutates:false,schema:obj(vec![("path",s(".mp4 or .mkv")),("caption_ids",json!({"type":"array","items":{"type":"string"},"description":"omit for all groups"})),("start",n("seconds")),("end",n("seconds")),("crf",n("0..51 integer, default18"))],&["path"]),run} ]
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timestamp() {
        assert_eq!(stamp(3661.25), "01:01:01,250");
        assert_eq!(stamp(0.9996), "00:00:01,000");
    }
}
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::commands::{dispatch, Session};
    fn call(s: &mut Session, n: &str, a: Value) -> Value {
        dispatch(s, n, &a).unwrap()
    }
    #[test]
    fn selectable_stream_range_and_atomic_failure() {
        let root = std::env::temp_dir().join(format!("kimo-soft-proof-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut s = Session::new();
        call(
            &mut s,
            "project_set",
            json!({"width":160,"height":90,"fps":25}),
        );
        let tr = call(&mut s, "add_track", json!({}))["track_id"].clone();
        call(
            &mut s,
            "add_clip",
            json!({"track_id":tr,"source_type":"solid","color":[0.1,0.2,0.3],"start":0.,"duration":2.}),
        );
        let srt = root.join("caption.srt");
        std::fs::write(&srt, "1\n00:00:00,250 --> 00:00:01,750\nEditable & plain\n").unwrap();
        let tr = call(&mut s, "add_track", json!({}))["track_id"].clone();
        let group = call(
            &mut s,
            "add_captions",
            json!({"track_id":tr,"source":"srt","path":srt}),
        )["caption_id"]
            .clone();
        call(
            &mut s,
            "burn_in",
            json!({"caption_id":group,"enabled":false}),
        );
        for ext in ["mp4", "mkv"] {
            let path = root.join(format!("proof.{ext}"));
            call(
                &mut s,
                "export_with_subtitles",
                json!({"path":path,"start":0.5,"end":1.5}),
            );
            let probe = Cmd::new("ffprobe")
                .args(["-v", "error", "-show_streams", "-of", "json"])
                .arg(&path)
                .output()
                .unwrap();
            let v: Value = serde_json::from_slice(&probe.stdout).unwrap();
            let sub = v["streams"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["codec_type"] == "subtitle")
                .unwrap();
            assert_eq!(
                sub["codec_name"],
                if ext == "mp4" { "mov_text" } else { "subrip" }
            );
            let data = Cmd::new("ffmpeg")
                .args(["-v", "error", "-i"])
                .arg(&path)
                .args(["-map", "0:s:0", "-f", "srt", "-"])
                .output()
                .unwrap();
            assert!(data.status.success());
            let text = String::from_utf8_lossy(&data.stdout);
            assert!(text.contains("00:00:00,000 --> 00:00:01,000"), "{text}");
            assert!(text.contains("Editable"));
            assert!(
                text.contains("Editable & plain"),
                "literal subtitle text lost: {text}"
            );
            assert!(path.exists());
            let before = std::fs::read(&path).unwrap();
            assert!(dispatch(
                &mut s,
                "export_with_subtitles",
                &json!({"path":path,"caption_ids":["absent"]})
            )
            .is_err());
            assert_eq!(std::fs::read(&path).unwrap(), before);
        }
        assert!(root.read_dir().unwrap().all(|p| !p
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".kimo-subs")));
        std::fs::remove_dir_all(root).unwrap();
    }
}
