// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/timeline,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Avid Log Exchange (`.ale`): a tab-delimited clip log.
//!
//! Written from Avid's published description of the format: three sections introduced by a line
//! holding only `Heading`, `Column` or `Data`, separated by blank lines.
//!
//! ```text
//! Heading
//! FIELD_DELIM  TABS
//! VIDEO_FORMAT  1080
//! AUDIO_FORMAT  48khz
//! FPS  23.976
//!
//! Column
//! (fields are separated by tab characters)
//! Name  Tracks  Start  End  Tape  Source File  Description  …
//!
//! Data
//! Beach  VA1A2  01:00:00:00  01:00:10:00  …
//! ```
//!
//! `Tracks` lists the clip's tracks (`V`, `A1`…`A16`); `Start` and `End` are SMPTE timecode at the
//! heading's `FPS`, `End` exclusive. Values never contain tabs or line breaks (they are replaced by
//! spaces). Lines end with CR LF (Avid's own files); the reader accepts LF and CR too.

use crate::time::{FrameRate, Tick, parse_timecode};

/// A parsed or to-be-written ALE file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AleDoc {
    /// Heading fields in order (`FIELD_DELIM`, `VIDEO_FORMAT`, `AUDIO_FORMAT`, `FPS`, …).
    pub heading: Vec<(String, String)>,
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
}

impl AleDoc {
    pub fn heading(&self, key: &str) -> Option<&str> {
        self.heading.iter().find(|(k, _)| k.eq_ignore_ascii_case(key)).map(|(_, v)| v.as_str())
    }
    /// The value of `column` in row `row`.
    pub fn get(&self, row: usize, column: &str) -> Option<&str> {
        let c = self.columns.iter().position(|x| x.eq_ignore_ascii_case(column))?;
        self.rows.get(row)?.get(c).map(String::as_str)
    }
    /// The heading's frame rate (`FPS`).
    pub fn rate(&self) -> Option<FrameRate> {
        let f = self.heading("FPS").and_then(|f| f.trim().parse::<f64>().ok())?;
        // Reject NaN, infinite, non-positive and absurdly large rates (`from_f64` saturates or yields 0/1 for them).
        if !f.is_finite() || f <= 0.0 || f > 100_000.0 {
            return None;
        }
        let r = FrameRate::from_f64(f);
        (r.num > 0 && r.den > 0).then_some(r)
    }
}

/// The columns every log starts with, in order. `Tape` is the tape or reel name.
pub const STANDARD_COLUMNS: [&str; 6] = ["Name", "Tracks", "Start", "End", "Tape", "Source File"];

/// Metadata columns commonly written after [`STANDARD_COLUMNS`].
pub const EXTRA_COLUMNS: [&str; 6] = ["Description", "Scene", "Take", "Shot", "Log Note", "Comment"];

/// A value as it is written: tabs and line breaks become spaces.
pub fn clean(s: &str) -> String {
    s.replace(['\t', '\r', '\n'], " ")
}

/// `FPS` heading text for a rate.
pub fn fps_text(r: FrameRate) -> String {
    let f = r.as_f64();
    if (f - f.round()).abs() < 1e-6 { format!("{}", f.round() as i64) } else { format!("{:.3}", f).trim_end_matches('0').to_string() }
}

/// `VIDEO_FORMAT` heading text for a frame height.
pub fn video_format(height: u32) -> &'static str {
    match height {
        1080 => "1080",
        720 => "720",
        480 | 486 => "NTSC",
        576 => "PAL",
        _ => "CUSTOM",
    }
}

/// The heading of a log at `rate` whose picture is `height` lines high and whose sound is sampled at
/// `sample_rate` Hz: `FIELD_DELIM`, `VIDEO_FORMAT`, `AUDIO_FORMAT` and `FPS`.
pub fn standard_heading(rate: FrameRate, height: u32, sample_rate: u32) -> Vec<(String, String)> {
    vec![
        ("FIELD_DELIM".to_string(), "TABS".to_string()),
        ("VIDEO_FORMAT".to_string(), video_format(height).to_string()),
        ("AUDIO_FORMAT".to_string(), if sample_rate == 44_100 { "44khz" } else { "48khz" }.to_string()),
        ("FPS".to_string(), fps_text(rate)),
    ]
}

/// The `Tracks` value of a clip: `V` when it has picture, then `A1`… for each sound channel (at most 16).
pub fn tracks_text(video: bool, audio_channels: u32) -> String {
    let mut tracks = String::new();
    if video {
        tracks.push('V');
    }
    for c in 1..=audio_channels.min(16) {
        tracks.push_str(&format!("A{c}"));
    }
    tracks
}

/// Serialise (CR LF line ends).
pub fn write(doc: &AleDoc) -> String {
    let mut out = String::from("Heading\r\n");
    for (k, v) in &doc.heading {
        out.push_str(&format!("{}\t{}\r\n", clean(k), clean(v)));
    }
    out.push_str("\r\nColumn\r\n");
    out.push_str(&doc.columns.iter().map(|c| clean(c)).collect::<Vec<_>>().join("\t"));
    out.push_str("\r\n\r\nData\r\n");
    for r in &doc.rows {
        out.push_str(&r.iter().map(|c| clean(c)).collect::<Vec<_>>().join("\t"));
        out.push_str("\r\n");
    }
    out
}

/// Read an ALE file.
pub fn parse(text: &str) -> Result<AleDoc, String> {
    #[derive(PartialEq)]
    enum Sec {
        None,
        Heading,
        Column,
        Data,
    }
    let mut doc = AleDoc::default();
    let mut sec = Sec::None;
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    for line in text.lines() {
        match line.trim() {
            "Heading" => {
                sec = Sec::Heading;
                continue;
            }
            "Column" => {
                sec = Sec::Column;
                continue;
            }
            "Data" => {
                sec = Sec::Data;
                continue;
            }
            "" => continue,
            _ => {}
        }
        match sec {
            Sec::None => return Err("not an ALE file (no Heading section)".into()),
            Sec::Heading => {
                let mut it = line.splitn(2, '\t');
                let k = it.next().unwrap_or_default().trim().to_string();
                doc.heading.push((k, it.next().unwrap_or_default().trim().to_string()));
            }
            Sec::Column => doc.columns.extend(line.split('\t').map(|c| c.trim().to_string())),
            Sec::Data => doc.rows.push(line.split('\t').map(str::to_string).collect()),
        }
    }
    if doc.columns.is_empty() {
        return Err("the ALE file has no Column section".into());
    }
    Ok(doc)
}

/// Start and end (exclusive) of a data row, in ticks at the heading's rate.
pub fn row_range(doc: &AleDoc, row: usize) -> Option<(Tick, Tick)> {
    let rate = doc.rate()?;
    let a = parse_timecode(doc.get(row, "Start")?, rate, false, 0).ok()?;
    let b = parse_timecode(doc.get(row, "End")?, rate, false, 0).ok()?;
    Some((rate.tick_of(a), rate.tick_of(b)))
}

