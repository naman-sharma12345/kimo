//! export_markers: the timeline's markers as a review-notes CSV (name, colour, in/out/duration timecode and seconds, the
//! columns editors paste into a spreadsheet), a YouTube chapter list ("0:00 Intro" lines, starting at 0:00 as YouTube
//! requires), or JSON. Returned inline, or written to `path`.
use super::kit::*;

fn tc(seconds: f64, fps: f64) -> String {
    let fi = (fps.round() as i64).max(1); let tf = (seconds.max(0.0) * fi as f64).round() as i64;
    let (ff, ts) = (tf % fi, tf / fi); format!("{:02}:{:02}:{:02}:{ff:02}", ts / 3600, ts / 60 % 60, ts % 60)
}
fn clock(seconds: f64) -> String { let s = seconds.max(0.0).floor() as i64; if s >= 3600 { format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60) } else { format!("{}:{:02}", s / 60, s % 60) } }
fn csv_field(s: &str) -> String { if s.contains([',', '"', '\n', '\r']) { format!("\"{}\"", s.replace('"', "\"\"")) } else { s.to_string() } }

fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let format = a.get("format").and_then(|v| v.as_str()).unwrap_or("csv");
    let p = &ss.project; let fps = p.fps.max(1.0);
    let mut ms: Vec<&crate::model::Marker> = p.markers.iter().collect();
    if let Some(pre) = a.get("name_prefix").and_then(|v| v.as_str()) { ms.retain(|m| m.name.starts_with(pre)); }
    ms.sort_by(|x, y| x.time.partial_cmp(&y.time).unwrap_or(std::cmp::Ordering::Equal));
    let mut warnings: Vec<String> = vec![];
    let content = match format {
        "csv" => {
            let mut o = String::from("Marker Name,Color,In,Out,Duration,In (s),Duration (s)\n");
            for m in &ms { o += &format!("{},{},{},{},{},{:.3},{:.3}\n", csv_field(&m.name), csv_field(&m.color), tc(m.time, fps), tc(m.time + m.duration, fps), tc(m.duration, fps), m.time, m.duration); }
            o
        }
        "youtube_chapters" => {
            if ms.is_empty() { return Err("no markers to export (add_marker at each chapter start)".into()); }
            let mut lines: Vec<(f64, String)> = ms.iter().map(|m| (m.time, if m.name.trim().is_empty() { "Chapter".to_string() } else { m.name.trim().to_string() })).collect();
            if lines[0].0 > 0.5 { lines.insert(0, (0.0, "Intro".into())); warnings.push("the first marker is after 0:00, so an \"Intro\" chapter was added at 0:00 (YouTube needs one)".into()); } else { lines[0].0 = 0.0; }
            for w in lines.windows(2) { if w[1].0 - w[0].0 < 10.0 { warnings.push(format!("chapter \"{}\" is under 10 s long; YouTube ignores the list unless every chapter is at least 10 s", w[0].1)); } }
            if lines.len() < 3 { warnings.push("YouTube shows chapters only when there are at least three".into()); }
            lines.iter().map(|(t, n)| format!("{} {n}\n", clock(*t))).collect()
        }
        "json" => serde_json::to_string_pretty(&ms.iter().map(|m| json!({"name": m.name, "color": m.color, "time": m.time, "duration": m.duration, "in": tc(m.time, fps), "out": tc(m.time + m.duration, fps)})).collect::<Vec<_>>()).unwrap_or_default(),
        o => return Err(format!("format must be csv, youtube_chapters or json, got '{o}'")),
    };
    let mut r = json!({"format": format, "markers": ms.len(), "warnings": warnings});
    if let Some(path) = a.get("path").and_then(|v| v.as_str()) {
        if std::path::Path::new(path).exists() && !a.get("overwrite").and_then(|v| v.as_bool()).unwrap_or(false) { return Err(format!("{path} exists; pass overwrite: true to replace it")); }
        std::fs::write(path, &content).map_err(|e| format!("could not write {path}: {e}"))?;
        r["path"] = json!(path); r["bytes"] = json!(content.len());
    } else { r["content"] = json!(content); }
    Ok(r)
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "export_markers", description: "Export the timeline's markers: csv (review notes: name, colour, in/out/duration as timecode and seconds), youtube_chapters (\"0:00 Intro\" lines for a video description; adds a 0:00 chapter if needed and warns when YouTube would reject the list) or json. Returns the text, or writes it to path.", mutates: false,
        schema: obj(vec![("format", json!({"type":"string","enum":["csv","youtube_chapters","json"],"description":"default csv"})), ("path", s("write to this file instead of returning the text")), ("overwrite", json!({"type":"boolean"})), ("name_prefix", s("only markers whose name starts with this (e.g. \"Scene\")"))], &[]),
        run },
] }

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timecode_and_clock_format() {
        assert_eq!(tc(3661.5, 30.0), "01:01:01:15"); assert_eq!(clock(65.9), "1:05"); assert_eq!(clock(3725.0), "1:02:05");
        assert_eq!(csv_field("a, \"b\""), "\"a, \"\"b\"\"\"");
    }
}
