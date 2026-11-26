// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/captions,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! WebVTT (`.vtt`).
//!
//! ```text
//! WEBVTT
//!
//! STYLE
//! ::cue { color: yellow }
//!
//! intro
//! 00:00:01.000 --> 00:00:03.500 line:90% align:center
//! <v Ann>Hello there
//! ```
//!
//! Kept on read: cue identifiers, cue settings (verbatim), a leading `<v Speaker>` voice span (as
//! the speaker), inline markup in the text, and STYLE / REGION / NOTE blocks. A missing `WEBVTT`
//! signature is tolerated. Timestamps may omit hours.

use crate::srt::parse_timing_line;
use crate::{Cue, Document, Error, Result, format_clock};

/// Split a leading `<v Name>` / `<v.class Name>` voice span off cue text.
pub fn split_voice(text: &str) -> (Option<String>, String) {
    let t = text.trim_start();
    if let Some(rest) = t.strip_prefix("<v")
        && (rest.starts_with(' ') || rest.starts_with('.') || rest.starts_with('\t'))
        && let Some(close) = rest.find('>')
    {
        let inner = &rest[..close];
        // skip classes: `.loud.x Name`
        let name = match inner.find([' ', '\t']) {
            Some(sp) => inner[sp..].trim(),
            None => "",
        };
        let body = &rest[close + 1..];
        // a closing </v> at the very end goes too; one in the middle means several voices: keep all
        let body = match body.find("</v>") {
            Some(p) if p + 4 == body.trim_end().len() => body[..p].to_string(),
            Some(_) => return (None, text.to_string()),
            None => body.to_string(),
        };
        if body.contains("<v ") || body.contains("<v.") || body.contains("<v\t") {
            return (None, text.to_string());
        }
        return ((!name.is_empty()).then(|| name.to_string()), body);
    }
    (None, text.to_string())
}

/// Undo [`write`]'s cue-text escape, so `A --> B` comes back as `A --> B` and not `A --&gt; B`.
///
/// Only the one substitution the writer makes is reversed: a literal `-->` in cue text would read
/// as a timing line to anything scanning the block, so it is written `--&gt;`. Other entities
/// (`&amp;`, `&lt;`, …) are cue markup and stay verbatim, as `parse` has always kept them.
fn unescape_text(text: &str) -> String {
    rewrite_arrows(text, false)
}

/// Apply [`write`]'s cue-text escape. `-->` becomes `--&gt;`, and a literal `--&gt;` (or the same
/// with more `&amp;` in front of `gt;`) gains one `&amp;`, so the escape is reversible.
fn escape_text(text: &str) -> String {
    rewrite_arrows(text, true)
}

/// Treat `--&` + `amp;`×k + `gt;` as level k, and `-->` as level -1. Escaping raises the level by
/// one, unescaping lowers it by one; anything else is copied unchanged.
fn rewrite_arrows(text: &str, escape: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(after) = rest.strip_prefix("--") {
            if escape && after.starts_with('>') {
                out.push_str("--&gt;");
                rest = &after[1..];
                continue;
            }
            let mut tail = after.strip_prefix('&').unwrap_or("\0");
            let mut k = 0usize;
            while let Some(t) = tail.strip_prefix("amp;") {
                tail = t;
                k += 1;
            }
            if let Some(t) = tail.strip_prefix("gt;") {
                if escape {
                    out.push_str("--&");
                    out.push_str(&"amp;".repeat(k + 1));
                    out.push_str("gt;");
                } else if k == 0 {
                    out.push_str("-->");
                } else {
                    out.push_str("--&");
                    out.push_str(&"amp;".repeat(k - 1));
                    out.push_str("gt;");
                }
                rest = t;
                continue;
            }
        }
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    out
}

pub fn parse(text: &str) -> Result<Document> {
    let mut doc = Document::default();
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut lines = text.lines().peekable();
    // signature + header text
    let has_sig = lines.peek().is_some_and(|&l| l == "WEBVTT" || l.starts_with("WEBVTT ") || l.starts_with("WEBVTT\t"));
    if has_sig {
        for l in lines.by_ref() {
            if l.trim().is_empty() {
                break;
            }
        }
    }
    // blocks
    let mut block: Vec<&str> = Vec::new();
    let mut blocks: Vec<Vec<&str>> = Vec::new();
    for l in lines {
        if l.trim().is_empty() {
            if !block.is_empty() {
                blocks.push(std::mem::take(&mut block));
            }
        } else {
            block.push(l);
        }
    }
    if !block.is_empty() {
        blocks.push(block);
    }
    let mut seen_cue = false;
    for b in blocks {
        let first = b[0].trim();
        let timing_idx = b.iter().position(|line| parse_timing_line(line).is_some());
        if timing_idx.is_none() && (first == "NOTE" || first.starts_with("NOTE ") || first.starts_with("NOTE\t")) {
            doc.blocks.push(b.join("\n"));
            continue;
        }
        if !seen_cue && timing_idx.is_none() && (first == "STYLE" || first == "REGION") {
            doc.blocks.push(b.join("\n"));
            continue;
        }
        let Some(timing_idx) = timing_idx else {
            doc.warnings.push(format!("ignored block \"{first}\""));
            continue;
        };
        let id = (timing_idx > 0).then(|| b[..timing_idx].join("\n").trim().to_string());
        let timing = &b[timing_idx];
        let Some((start, end, settings)) = parse_timing_line(timing) else {
            doc.warnings.push(format!("bad timing line \"{}\"", timing.trim()));
            continue;
        };
        seen_cue = true;
        if end <= start {
            doc.warnings.push(format!("cue at {} has end before start; skipped", format_clock(start, '.')));
            continue;
        }
        let raw = b[timing_idx + 1..].iter().map(|l| l.trim_end()).collect::<Vec<_>>().join("\n");
        let (speaker, text) = split_voice(&unescape_text(&raw));
        doc.cues.push(Cue { start, end, text, speaker, id, settings: settings.to_string() });
    }
    if !has_sig && doc.cues.is_empty() && !text.trim().is_empty() {
        return Err(Error::NotFormat("WebVTT"));
    }
    Ok(doc)
}

pub fn write(doc: &Document) -> String {
    let mut out = String::from("WEBVTT\n\n");
    for b in &doc.blocks {
        let b = b.trim_end();
        if !b.is_empty() {
            out.push_str(b);
            out.push_str("\n\n");
        }
    }
    for c in &doc.cues {
        if let Some(id) = c.id.as_deref().map(str::trim).filter(|i| !i.is_empty() && !i.contains("-->")) {
            out.push_str(&id.replace(['\r', '\n'], " "));
            out.push('\n');
        }
        out.push_str(&format!("{} --> {}", format_clock(c.start, '.'), format_clock(c.end, '.')));
        if !c.settings.trim().is_empty() {
            out.push(' ');
            out.push_str(c.settings.trim());
        }
        out.push('\n');
        let mut text = String::new();
        if let Some(s) = c.speaker.as_deref().filter(|s| !s.is_empty()) {
            // `>` would end the voice tag early and spill the rest of the name into the cue text, so it
            // is written as the lookalike `›` (U+203A), and a line break as a space. A name ending
            // in `-` gets a space before the closing `>`, so the two never read as an arrow.
            let name = s.replace('>', "\u{203a}").replace(['\r', '\n'], " ");
            text.push_str(&format!("<v {name}{}>", if name.ends_with('-') { " " } else { "" }));
        }
        text.push_str(&c.text);
        for l in text.lines().filter(|l| !l.trim().is_empty()) {
            out.push_str(&escape_text(l));
            out.push('\n');
        }
        out.push('\n');
    }
    out
}

