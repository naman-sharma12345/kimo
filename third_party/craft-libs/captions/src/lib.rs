#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/captions,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Caption and subtitle files behind one neutral model.
//!
//! - **Files:** [SubRip](srt) (`.srt`), [WebVTT](vtt) (`.vtt`), [Scenarist SCC](scc) (`.scc`,
//!   CEA-608 pop-on, roll-up and paint-on on read; pop-on on write), [MacCaption MCC](mcc)
//!   (`.mcc`, SMPTE ST 334-2 packets with CEA-608 and CEA-708), [EBU STL](stl) (`.stl`, EBU
//!   Tech 3264), [ASS/SSA](ass) (`.ass`, `.ssa`) and [TTML](ttml) (`.ttml` IMSC1 Text profile, `.dfxp` DFXP) parse into a
//!   [`Document`] of [`Cue`]s and write back. Readers are forgiving (BOM, UTF-16, Windows-1252
//!   fallback, CRLF/CR, missing SRT indexes, `,` or `.` milliseconds, missing hours, no blank
//!   line between cues); writers are strict. WebVTT cue identifiers, cue settings, `<v Speaker>`
//!   voices and STYLE/REGION/NOTE blocks are kept.
//! - **Byte pairs:** [`cea608`] has the CEA-608 character sets and control codes;
//!   [`scc::decode_pairs`] turns a timed stream of 608 byte pairs into cues.
//! - **Time:** cue times are exact [`Tick`]s ([`time`]). SRT/VTT milliseconds and SCC 29.97 fps
//!   timecodes (drop-frame or not) convert exactly; [`Document::snap_to_frames`] snaps to a
//!   frame grid.
//! - **Limits:** input is at most [`MAX_INPUT`] bytes and a document holds at most [`MAX_CUES`]
//!   cues; TTML is read through `craft-xml` with its nesting, node and attribute limits.
//!
//! ```
//! use craft_captions::{Format, WriteOptions, detect, parse, write};
//! let srt = b"1\n00:00:01,000 --> 00:00:02,500\nHello\n";
//! assert_eq!(detect(srt, Some("srt")), Some(Format::Srt));
//! let doc = parse(srt, Format::Srt)?;
//! assert_eq!(doc.cues[0].text, "Hello");
//! let vtt = write(&doc, Format::WebVtt, WriteOptions::default());
//! assert!(vtt.starts_with(b"WEBVTT"));
//! # Ok::<(), craft_captions::Error>(())
//! ```

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
struct ReadmeExamples;

pub mod ass;
pub mod cea608;
pub mod jacosub;
pub mod mcc;
#[cfg(feature = "mov-text")]
pub mod mov_text;
pub mod pjs;
pub mod realtext;
pub mod scc;
pub mod srt;
pub mod stl;
mod text;
/// Exact time: `craft-timeline`'s ticks, frame rates and timecode fields.
pub use craft_timeline::time;
pub mod ttml;
pub mod vtt;

pub use text::{decode_entities, entity_at, plain_text};
pub use time::{FrameRate, TICKS_PER_SECOND, Tick};

/// The creating program MCC and EBU STL files name when the caller gives none.
pub const PROGRAM: &str = "craft-captions";

/// The largest caption file [`parse`] reads, in bytes.
pub const MAX_INPUT: usize = 64 << 20;

/// The most cues a parsed [`Document`] holds; a reader stops there with a warning.
pub const MAX_CUES: usize = 1_000_000;

/// A caption file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Format {
    Srt,
    WebVtt,
    Scc,
    /// MacCaption MCC (CEA-608 + CEA-708 in SMPTE ST 334-2 packets).
    Mcc,
    /// EBU STL (EBU Tech 3264).
    Stl,
    /// TTML, IMSC1 Text profile.
    Ttml,
    /// DFXP (TTML1).
    Dfxp,
    /// Advanced SubStation Alpha (also reads SubStation Alpha).
    Ass,
    /// Phoenix Japanimation Society (PJS) subtitles.
    Pjs,
    /// RealText streaming timed text markup.
    RealText,
    /// JACOsub scriptable subtitles.
    JacoSub,
}

impl Format {
    pub const ALL: [Format; 11] = [
        Format::Srt,
        Format::WebVtt,
        Format::Scc,
        Format::Mcc,
        Format::Stl,
        Format::Ttml,
        Format::Dfxp,
        Format::Ass,
        Format::Pjs,
        Format::RealText,
        Format::JacoSub,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Format::Srt => "SubRip (SRT)",
            Format::WebVtt => "WebVTT",
            Format::Scc => "Scenarist SCC (CEA-608)",
            Format::Mcc => "MacCaption MCC (CEA-608/708)",
            Format::Stl => "EBU STL",
            Format::Ttml => "TTML (IMSC1)",
            Format::Dfxp => "DFXP (TTML1)",
            Format::Ass => "Advanced SubStation Alpha (ASS)",
            Format::Pjs => "PJS",
            Format::RealText => "RealText",
            Format::JacoSub => "JACOsub",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Format::Srt => "srt",
            Format::WebVtt => "vtt",
            Format::Scc => "scc",
            Format::Mcc => "mcc",
            Format::Stl => "stl",
            Format::Ttml => "ttml",
            Format::Dfxp => "dfxp",
            Format::Ass => "ass",
            Format::Pjs => "pjs",
            Format::RealText => "rt",
            Format::JacoSub => "jss",
        }
    }
    pub fn from_name(s: &str) -> Option<Format> {
        match s.trim_start_matches('.').to_ascii_lowercase().as_str() {
            "srt" | "subrip" => Some(Format::Srt),
            "vtt" | "webvtt" => Some(Format::WebVtt),
            "scc" | "cea608" | "cea-608" => Some(Format::Scc),
            "mcc" | "maccaption" | "cea708" | "cea-708" => Some(Format::Mcc),
            "stl" | "ebu-stl" | "ebustl" | "ebu" => Some(Format::Stl),
            "ttml" | "imsc" | "imsc1" | "xml" => Some(Format::Ttml),
            "dfxp" => Some(Format::Dfxp),
            "ass" | "ssa" => Some(Format::Ass),
            "pjs" => Some(Format::Pjs),
            "rt" | "realtext" => Some(Format::RealText),
            "jss" | "js" | "jacosub" => Some(Format::JacoSub),
            _ => None,
        }
    }
    /// Whether the format is a binary file (EBU STL).
    pub fn is_binary(self) -> bool {
        self == Format::Stl
    }
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("not a {0} file")]
    NotFormat(&'static str),
    #[error("line {line}: {msg}")]
    Syntax { line: usize, msg: String },
    /// The input is longer than [`MAX_INPUT`].
    #[error("the caption file is larger than {0} bytes")]
    TooLarge(usize),
}

pub type Result<T> = std::result::Result<T, Error>;

/// One caption cue.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Cue {
    pub start: Tick,
    pub end: Tick,
    /// Text as written (lines separated by `\n`, inline tags kept).
    pub text: String,
    /// WebVTT voice (`<v Name>`).
    pub speaker: Option<String>,
    /// WebVTT cue identifier.
    pub id: Option<String>,
    /// WebVTT cue settings, verbatim.
    pub settings: String,
}

/// A parsed caption file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Document {
    pub cues: Vec<Cue>,
    /// WebVTT STYLE / REGION / NOTE blocks (verbatim, without trailing blank lines).
    pub blocks: Vec<String>,
    /// Non-fatal problems found while reading (skipped cues…).
    pub warnings: Vec<String>,
    /// SCC / MCC: the time code label of time zero, read as a time ([`label_time`]: a sequence's
    /// start timecode on export). The writers count their time codes from the label it reads as,
    /// so 01:00:00:00 at 25 fps is 01:00:00;00 or 01:00:00:00 at 29.97. Readers leave it at zero.
    pub start_timecode: Tick,
    /// SCC / MCC read from a file: the frame rate of its time code labels and whether they are
    /// drop-frame. [`Document::count_from`] needs it.
    pub labels: Option<(FrameRate, bool)>,
    /// BCP 47 language tag of the text, when known: TTML's `xml:lang` on import, the caption
    /// track's language on export. TTML and DFXP write it (else `en`).
    pub language: Option<String>,
}

/// Frame `frame` of a `rate` time code (drop-frame or not) as its label read as a time: hours,
/// minutes, seconds and the frame's share of a second at the label's nominal rate (25 for 25 fps,
/// 30 for 29.97). Negative frames read as zero.
pub fn label_time(frame: i64, rate: FrameRate, drop_frame: bool) -> Tick {
    let base = rate.timecode_base();
    let (_, h, m, s, f) = crate::time::frames_to_fields(frame.max(0), rate, drop_frame && rate.supports_drop_frame());
    let seconds = h.saturating_mul(3600).saturating_add(m.saturating_mul(60)).saturating_add(s);
    Tick(TICKS_PER_SECOND.saturating_mul(seconds)) + FrameRate::new(base, 1).tick_of(f)
}

/// The frame of a `rate` time code (drop-frame or not) whose label reads as `label` (as
/// [`label_time`] gives it), to the nearest frame of the label's nominal rate.
pub fn label_frame(label: Tick, rate: FrameRate, drop_frame: bool) -> i64 {
    let nominal = FrameRate::new(rate.timecode_base(), 1);
    let n = nominal.frame_at(nominal.snap_nearest(label.max(Tick::ZERO)));
    let (_, h, m, s, f) = crate::time::frames_to_fields(n, nominal, false);
    crate::time::fields_to_frames(h, m, s, f, rate, drop_frame && rate.supports_drop_frame())
}

impl Document {
    /// SCC / MCC read from a file: count cue times from the time code labelled `start` (as
    /// [`Document::start_timecode`]) rather than from 00:00:00:00, the inverse of what the writers
    /// do. Only when every cue is at or after it, so a file that counts from zero keeps its times.
    /// Returns whether the cues moved.
    pub fn count_from(&mut self, start: Tick) -> bool {
        let Some((rate, drop_frame)) = self.labels else { return false };
        let origin = rate.tick_of(label_frame(start, rate, drop_frame));
        if origin <= Tick::ZERO || self.cues.iter().any(|c| c.start < origin) {
            return false;
        }
        for c in &mut self.cues {
            c.start -= origin;
            c.end -= origin;
        }
        true
    }

    /// Snap cue times to frame boundaries of `rate` (nearest), keeping every cue at least one frame
    /// long and removing overlaps (a cue is cut where the next one starts).
    pub fn snap_to_frames(&mut self, rate: FrameRate) {
        for c in &mut self.cues {
            c.start = rate.snap_nearest(c.start);
            c.end = rate.snap_nearest(c.end).max(c.start + rate.frame_duration());
        }
        self.normalize();
    }

    /// Sort by start, drop empty cues and cut overlaps.
    pub fn normalize(&mut self) {
        self.cues.retain(|c| c.end > c.start);
        self.cues.sort_by_key(|c| (c.start, c.end));
        let mut out: Vec<Cue> = Vec::with_capacity(self.cues.len());
        for c in self.cues.drain(..) {
            let mut c = c;
            if let Some(prev) = out.last_mut()
                && c.start < prev.end
            {
                if c.start > prev.start {
                    prev.end = c.start;
                } else {
                    // same start: keep the first, shift this one after it
                    c.start = prev.end;
                    if c.end <= c.start {
                        self.warnings.push(format!("dropped overlapping cue \"{}\"", c.text.lines().next().unwrap_or("")));
                        continue;
                    }
                }
            }
            out.push(c);
        }
        self.cues = out;
    }
}

/// Guess a caption format from content (and the file extension as a hint).
pub fn detect(bytes: &[u8], ext: Option<&str>) -> Option<Format> {
    if stl::sniff(bytes) {
        return Some(Format::Stl);
    }
    let text = decode_text(&bytes[..bytes.len().min(4096)]);
    let head = text.trim_start_matches('\u{feff}').trim_start();
    if head.starts_with("WEBVTT") {
        return Some(Format::WebVtt);
    }
    if head.starts_with("Scenarist_SCC") {
        return Some(Format::Scc);
    }
    if head.starts_with("File Format=MacCaption_MCC") {
        return Some(Format::Mcc);
    }
    if head.starts_with("[Script Info]") {
        return Some(Format::Ass);
    }
    if realtext::is_realtext(head) {
        return Some(Format::RealText);
    }
    let first_lines: Vec<&str> = head.lines().map(str::trim).filter(|l| !l.is_empty()).take(3).collect();
    if first_lines.first().is_some_and(|l| pjs::split_cue(l).is_some()) {
        return Some(Format::Pjs);
    }
    if first_lines.iter().any(|l| jacosub::is_timed_line(l)) || (head.starts_with('#') && (head.contains("TimeRes") || head.contains("#T"))) {
        return Some(Format::JacoSub);
    }
    let ext = ext.map(|e| e.trim_start_matches('.').to_ascii_lowercase());
    if (head.starts_with('<') && (head.contains("<tt ") || head.contains("<tt>") || head.contains(":tt ")))
        && (head.contains("ns/ttml") || head.contains("ttaf1"))
    {
        let legacy = head.contains("ttaf1") || head.contains("dfxp");
        return Some(if ext.as_deref() == Some("dfxp") || (legacy && ext.as_deref() != Some("ttml")) { Format::Dfxp } else { Format::Ttml });
    }
    let has_arrow_timing = head.lines().take(40).any(|l| srt::parse_timing_line(l).is_some());
    match ext.as_deref() {
        Some("srt") if has_arrow_timing || head.is_empty() => Some(Format::Srt),
        Some("vtt") if has_arrow_timing => Some(Format::WebVtt),
        Some("srt" | "vtt" | "scc" | "mcc" | "stl" | "ttml" | "dfxp" | "xml" | "ass" | "ssa" | "pjs" | "rt" | "jss" | "js") => None,
        _ if has_arrow_timing => Some(Format::Srt),
        _ => None,
    }
}

/// Parse caption bytes.
pub fn parse(bytes: &[u8], format: Format) -> Result<Document> {
    if bytes.len() > MAX_INPUT {
        return Err(Error::TooLarge(MAX_INPUT));
    }
    let mut doc = parse_unbounded(bytes, format)?;
    cap_cues(&mut doc, MAX_CUES);
    Ok(doc)
}

fn cap_cues(doc: &mut Document, max: usize) {
    if doc.cues.len() > max {
        doc.cues.truncate(max);
        doc.warnings.push(format!("more than {max} cues: the rest were skipped"));
    }
}

fn parse_unbounded(bytes: &[u8], format: Format) -> Result<Document> {
    if format == Format::Stl {
        return stl::parse(bytes);
    }
    let text = decode_text(bytes);
    match format {
        Format::Srt => srt::parse(&text),
        Format::WebVtt => vtt::parse(&text),
        Format::Scc => scc::parse(&text),
        Format::Mcc => mcc::parse(&text),
        Format::Ttml | Format::Dfxp => ttml::parse(&text),
        Format::Ass => ass::parse(&text),
        Format::Pjs => pjs::parse(&text),
        Format::RealText => realtext::parse(&text),
        Format::JacoSub => jacosub::parse(&text),
        Format::Stl => stl::parse(bytes),
    }
}

/// Options for [`write`].
#[derive(Clone, Copy, Debug)]
pub struct WriteOptions {
    /// SCC / MCC: drop-frame timecode (the usual choice for 29.97 fps).
    pub drop_frame: bool,
    /// The sequence frame rate: TTML (IMSC1) writes exact frame times at it; EBU STL picks
    /// `STL25.01` or `STL30.01` from it (None = 25 fps). Other formats ignore it.
    pub rate: Option<FrameRate>,
    /// MCC / EBU STL: the name written as the creating program ([`PROGRAM`] by default).
    pub program: &'static str,
    /// MCC: a packet on every frame from the start to this time (the end of the programme), with
    /// no caption data where there is none, as SDI and frame-wrapped MXF carry one per frame.
    /// The frames are MCC's 29.97 fps ones, so this is one packet per video frame only for
    /// 29.97 fps video. None (the default) writes only the frames that carry caption data.
    pub every_frame_until: Option<Tick>,
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self { drop_frame: true, rate: None, program: PROGRAM, every_frame_until: None }
    }
}

/// Write a document.
pub fn write(doc: &Document, format: Format, opts: WriteOptions) -> Vec<u8> {
    match format {
        Format::Srt => srt::write(doc).into_bytes(),
        Format::WebVtt => vtt::write(doc).into_bytes(),
        Format::Scc => scc::write(doc, opts.drop_frame).into_bytes(),
        Format::Mcc => mcc::write_padded(doc, opts.drop_frame, true, true, mcc_pad_frames(opts), opts.program).into_bytes(),
        Format::Stl => stl::write(doc, opts.rate, opts.program),
        Format::Ttml => ttml::write(doc, ttml::Flavor::Imsc1, opts.rate, doc.language.as_deref().unwrap_or("")).into_bytes(),
        Format::Dfxp => ttml::write(doc, ttml::Flavor::Dfxp, opts.rate, doc.language.as_deref().unwrap_or("")).into_bytes(),
        Format::Ass => ass::write(doc).into_bytes(),
        Format::Pjs => pjs::write(doc).into_bytes(),
        Format::RealText => realtext::write(doc).into_bytes(),
        Format::JacoSub => jacosub::write(doc).into_bytes(),
    }
}

/// How many characters [`charset_warnings`] names one by one per caption stream; the rest share one warning.
const LISTED_CHARS: usize = 20;
/// How many cut captions [`charset_warnings`] names one by one per caption stream; the rest share one warning.
const LISTED_CUTS: usize = 20;
/// The rows of 32 columns CEA-608 shows ([`scc::caption_rows`] keeps at most this many).
const ROWS_608: usize = 15;

/// What writing `doc` as `format` loses of its text, the characters the caption data cannot carry
/// and the text cut to fit. EBU STL writes `?` for the characters ISO/IEC 6937 has no code for.
/// SCC and MCC drop the characters CEA-608 has no code for (…) or send a close one (’ as '), and
/// MCC's CEA-708 service sends `?` for the characters it has no code for. One warning per such
/// character, in order of first use, with how often and the first caption that has it. CEA-608
/// shows at most 15 rows of 32 characters and MCC's CEA-708 window 12 rows: one warning per
/// caption cut to fit, with how many characters it lost (spaces not counted) and where the lost
/// text starts. Other formats: none.
pub fn charset_warnings(doc: &Document, format: Format, opts: WriteOptions) -> Vec<String> {
    let mut out = Vec::new();
    // the time code the file has for a caption: counted from the start time code, as the writers do
    let origin = label_frame(doc.start_timecode, scc::RATE, opts.drop_frame);
    let at_608 = |t: Tick| scc::format_timecode(scc::frame_of(t).saturating_add(origin), opts.drop_frame);
    // the rows each writer loads, so the warnings name exactly what the files leave out
    if matches!(format, Format::Scc | Format::Mcc) {
        lossy_chars(doc, "CEA-608", |text| changed_in_rows(&scc::caption_rows(text), cea608::shown_as), at_608, &mut out);
        cut_rows(doc, &format!("CEA-608 shows {ROWS_608} rows of 32 characters"), scc::caption_rows, at_608, &mut out);
    }
    if format == Format::Mcc {
        let shown_as = |c| Some(mcc::shown_as_708(c));
        lossy_chars(doc, "CEA-708", |text| changed_in_rows(&mcc::window_rows(text), shown_as), at_608, &mut out);
        let limit = format!("The CEA-708 window holds {} rows of 32 characters", mcc::WINDOW_ROWS);
        cut_rows(doc, &limit, mcc::window_rows, at_608, &mut out);
    }
    if format == Format::Stl {
        let unencodable = |text: &str| -> Vec<(char, Option<char>)> { stl::unencodable(text).into_iter().map(|c| (c, Some('?'))).collect() };
        lossy_chars(doc, "EBU STL (ISO/IEC 6937)", unencodable, |t| stl::timecode(t, opts.rate), &mut out);
    }
    out
}

/// The characters of the `rows` a writer loads that `shown_as` changes, with what a decoder shows
/// instead.
fn changed_in_rows(rows: &[String], shown_as: impl Fn(char) -> Option<char>) -> Vec<(char, Option<char>)> {
    rows.iter().flat_map(|r| r.chars()).map(|c| (c, shown_as(c))).filter(|(c, shown)| *shown != Some(*c)).collect()
}

/// `n` of `what`, singular for one.
fn count(n: usize, what: &str) -> String {
    if n == 1 { format!("1 {what}") } else { format!("{n} {what}s") }
}

/// Warnings for the captions of `doc` whose text the writer's `rows_of` (the rows it loads for a
/// caption's text) does not hold in full, as `limit` says: how many characters each loses, not
/// counting spaces, and the start of the lost text.
fn cut_rows(doc: &Document, limit: &str, rows_of: impl Fn(&str) -> Vec<String>, at: impl Fn(Tick) -> String, out: &mut Vec<String>) {
    let visible = |s: &str| s.chars().filter(|c| !c.is_whitespace()).count();
    let mut cues: Vec<&Cue> = doc.cues.iter().filter(|c| c.end > c.start).collect();
    cues.sort_by_key(|c| c.start);
    // (start of the caption, the text it loses)
    let mut cut: Vec<(Tick, String)> = Vec::new();
    for cue in cues {
        let text = plain_text(&cue.text);
        // the rows keep a prefix of the text: the lost part starts at the first character past them
        let kept: usize = rows_of(&cue.text).iter().map(|r| visible(r)).sum();
        if let Some((i, _)) = text.char_indices().filter(|(_, c)| !c.is_whitespace()).nth(kept) {
            cut.push((cue.start, text[i..].to_string()));
        }
    }
    for (start, lost) in cut.iter().take(LISTED_CUTS) {
        // the first characters of the lost text, on one line
        let lost_text = lost.split_whitespace().collect::<Vec<_>>().join(" ");
        let mut from: String = lost_text.chars().take(24).collect();
        if from.len() < lost_text.len() {
            from.push('…');
        }
        let n = visible(lost);
        let what = if n == 1 { "its last character".to_string() } else { format!("its last {n} characters") };
        out.push(format!("{limit}: the caption at {} lost {what}, from \"{from}\"", at(*start)));
    }
    if let Some(rest) = cut.get(LISTED_CUTS..).filter(|rest| !rest.is_empty()) {
        let n = rest.iter().fold(0usize, |n, (_, lost)| n.saturating_add(visible(lost)));
        let captions = if rest.len() == 1 { "caption was" } else { "captions were" };
        out.push(format!("{limit}: {} more {captions} cut to fit too, losing {}", rest.len(), count(n, "character")));
    }
}

/// Warnings for the characters of `doc` that the writer changes (`lost_in` gives them for a
/// caption's text, with what a decoder shows instead), timed by `at`.
fn lossy_chars(
    doc: &Document,
    stream: &str,
    lost_in: impl Fn(&str) -> Vec<(char, Option<char>)>,
    at: impl Fn(Tick) -> String,
    out: &mut Vec<String>,
) {
    // (character, what a decoder shows instead, count, start of the first caption with it)
    let mut lost: Vec<(char, Option<char>, usize, Tick)> = Vec::new();
    let mut index: std::collections::HashMap<char, usize> = std::collections::HashMap::new();
    let mut cues: Vec<&Cue> = doc.cues.iter().filter(|c| c.end > c.start).collect();
    cues.sort_by_key(|c| c.start);
    for cue in cues {
        for (c, shown) in lost_in(&cue.text) {
            match index.get(&c).and_then(|&i| lost.get_mut(i)) {
                Some(entry) => entry.2 = entry.2.saturating_add(1),
                None => {
                    index.insert(c, lost.len());
                    lost.push((c, shown, 1, cue.start));
                }
            }
        }
    }
    let times = |n: usize| if n == 1 { "once".to_string() } else { format!("{n} times") };
    for &(c, shown, n, start) in lost.iter().take(LISTED_CHARS) {
        let what = shown.map_or_else(|| "dropped".to_string(), |s| format!("written as \"{s}\""));
        out.push(format!("{stream} has no \"{c}\" (U+{:04X}): {what} {}, first in the caption at {}", c as u32, times(n), at(start)));
    }
    if let Some(rest) = lost.get(LISTED_CHARS..).filter(|rest| !rest.is_empty()) {
        let n = rest.iter().fold(0usize, |n, e| n.saturating_add(e.2));
        out.push(format!("{stream} has no code for {} more characters either: changed or dropped {}", rest.len(), times(n)));
    }
}

/// [`write`] into `out`. MCC goes out line by line, so a long `every_frame_until` export is never
/// held in memory; the other formats are small and written whole.
pub fn write_to(out: &mut dyn std::io::Write, doc: &Document, format: Format, opts: WriteOptions) -> std::io::Result<()> {
    match format {
        Format::Mcc => mcc::write_padded_to(out, doc, opts.drop_frame, true, true, mcc_pad_frames(opts), opts.program),
        _ => out.write_all(&write(doc, format, opts)),
    }
}

/// The MCC frames `opts.every_frame_until` asks for: those that start before it, so a last
/// partial frame gets its packet too.
fn mcc_pad_frames(opts: WriteOptions) -> i64 {
    opts.every_frame_until.map_or(0, |end| {
        let f = mcc::RATE.frame_at(end);
        if mcc::RATE.tick_of(f) < end { f.saturating_add(1) } else { f }
    })
}

/// Decode text bytes: UTF-8 (BOM stripped), UTF-16 with BOM, else Windows-1252. Line endings are
/// normalised to `\n`.
pub fn decode_text(bytes: &[u8]) -> String {
    let s = if let Some(rest) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        String::from_utf8_lossy(rest).into_owned()
    } else if bytes.starts_with(&[0xff, 0xfe]) || bytes.starts_with(&[0xfe, 0xff]) {
        let le = bytes[0] == 0xff;
        let units: Vec<u16> = bytes[2..]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) })
            .collect();
        String::from_utf16_lossy(&units)
    } else {
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_string(),
            Err(_) => bytes.iter().map(|&b| cp1252(b)).collect(),
        }
    };
    let s = s.strip_prefix('\u{feff}').map(str::to_string).unwrap_or(s);
    if s.contains('\r') { s.replace("\r\n", "\n").replace('\r', "\n") } else { s }
}

fn cp1252(b: u8) -> char {
    const HI: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—',
        '˜', '™', 'š', '›', 'œ', '\u{9d}', 'ž', 'Ÿ',
    ];
    if (0x80..0xa0).contains(&b) { HI[(b - 0x80) as usize] } else { b as char }
}

pub(crate) const TICKS_PER_MS: i64 = TICKS_PER_SECOND / 1000;

/// Parse `[H…:]MM:SS[.,]fff` (the fraction may have 1–9 digits) into ticks.
pub fn parse_clock(s: &str) -> Option<Tick> {
    let s = s.trim();
    let (hms, frac) = match s.find([',', '.']) {
        Some(i) => (&s[..i], &s[i + 1..]),
        None => (s, ""),
    };
    let parts: Vec<&str> = hms.split(':').collect();
    let num = |p: &str| -> Option<i64> { if !p.is_empty() && p.len() <= 9 && p.bytes().all(|b| b.is_ascii_digit()) { p.parse().ok() } else { None } };
    let (h, m, sec) = match parts.as_slice() {
        [h, m, s] => (num(h)?, num(m)?, num(s)?),
        [m, s] => (0, num(m)?, num(s)?),
        _ => return None,
    };
    if m >= 60 || sec >= 60 || h > 9999 {
        return None;
    }
    let mut ticks = ((h * 60 + m) * 60 + sec) * TICKS_PER_SECOND;
    if !frac.is_empty() {
        if frac.len() > 9 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let d: i64 = frac.parse().ok()?;
        ticks += (d as i128 * TICKS_PER_SECOND as i128 / 10i128.pow(frac.len() as u32)) as i64;
    }
    Some(Tick(ticks))
}

/// Format ticks as `HH:MM:SS<sep>mmm` (rounded to the nearest millisecond).
pub fn format_clock(t: Tick, sep: char) -> String {
    // The rounding addition must not overflow for an input-derived Tick near i64::MAX.
    let ms = (i128::from(t.0.max(0)) + i128::from(TICKS_PER_MS) / 2) / i128::from(TICKS_PER_MS);
    let (h, m, s, f) = (ms / 3_600_000, (ms / 60_000) % 60, (ms / 1000) % 60, ms % 1000);
    format!("{h:02}:{m:02}:{s:02}{sep}{f:03}")
}

