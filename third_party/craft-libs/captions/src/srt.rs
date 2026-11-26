// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/captions,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! SubRip (`.srt`).
//!
//! ```text
//! 1
//! 00:00:01,000 --> 00:00:03,500
//! First line
//! <i>second line</i>
//!
//! 2
//! …
//! ```
//!
//! Reading accepts what real files contain: missing or wrong indexes, `.` instead of `,`, 1–3
//! digit fractions, missing hours, extra spaces, trailing SSA position coordinates
//! (`X1:… Y2:…`), and cues not separated by a blank line (a new cue starts at a timing line, or at
//! an integer line directly followed by one). Writing renumbers from 1 and rounds to milliseconds.

use crate::time::Tick;

use crate::{Cue, Document, Error, Result, format_clock, parse_clock};

/// Parse `start --> end[ settings]` (SRT or VTT timing line). Returns (start, end, settings).
pub fn parse_timing_line(line: &str) -> Option<(Tick, Tick, &str)> {
    let (a, rest) = line.split_once("-->")?;
    let start = parse_clock(a.trim())?;
    let rest = rest.trim_start();
    let end_len = rest.find(char::is_whitespace).unwrap_or(rest.len());
    let end = parse_clock(&rest[..end_len])?;
    Some((start, end, rest[end_len..].trim()))
}

fn is_index(line: &str) -> bool {
    let l = line.trim();
    !l.is_empty() && l.len() <= 10 && l.bytes().all(|b| b.is_ascii_digit())
}

const ZWSP: char = '\u{200B}';

/// Keep a text line from reading as a timing line: such a line gains a leading zero-width space.
fn escape(line: &str) -> String {
    if parse_timing_line(line.trim_start_matches(ZWSP)).is_some() { format!("{ZWSP}{line}") } else { line.to_string() }
}

/// Reverse [`escape`]: drop one leading zero-width space when the rest still looks like a timing line.
fn unescape_line(line: &str) -> &str {
    match line.strip_prefix(ZWSP) {
        Some(r) if parse_timing_line(r.trim_start_matches(ZWSP)).is_some() => r,
        _ => line,
    }
}

pub fn parse(text: &str) -> Result<Document> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let lines: Vec<&str> = text.lines().collect();
    let mut doc = Document::default();
    let mut i = 0;
    let mut found_any = false;
    while i < lines.len() {
        let Some((start, end, _settings)) = parse_timing_line(lines[i]) else {
            let l = lines[i].trim();
            if !l.is_empty() && !is_index(l) && found_any {
                doc.warnings.push(format!("line {}: ignored stray text \"{l}\"", i + 1));
            }
            i += 1;
            continue;
        };
        found_any = true;
        i += 1;
        let mut text_lines: Vec<&str> = Vec::new();
        while i < lines.len() {
            let l = lines[i];
            if l.trim().is_empty() {
                break;
            }
            if parse_timing_line(l).is_some() {
                break;
            }
            if is_index(l) && lines.get(i + 1).is_some_and(|n| parse_timing_line(n).is_some()) {
                break;
            }
            text_lines.push(unescape_line(l.trim_end()));
            i += 1;
        }
        if end <= start {
            doc.warnings.push(format!("cue at {} has end before start; skipped", format_clock(start, ',')));
            continue;
        }
        doc.cues.push(Cue { start, end, text: text_lines.join("\n"), ..Default::default() });
    }
    if !found_any && !text.trim().is_empty() {
        return Err(Error::NotFormat("SubRip"));
    }
    Ok(doc)
}

pub fn write(doc: &Document) -> String {
    let mut out = String::new();
    for (n, c) in doc.cues.iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n", n + 1, format_clock(c.start, ','), format_clock(c.end, ',')));
        // blank lines inside text would end the cue: drop them
        for l in c.text.lines().filter(|l| !l.trim().is_empty()) {
            // stored entities stay as they are, so a literal bracket does not become a tag on reimport
            out.push_str(&escape(l));
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

