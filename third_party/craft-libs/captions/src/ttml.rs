// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit 249a41bd9306a1b34330ac94e596a1a8b881378a), crates/captions,
// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0
// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! TTML (W3C Timed Text Markup Language): IMSC1 Text profile (`.ttml`, written per TTML2 /
//! IMSC 1.1 Text) and DFXP (`.dfxp`, TTML1's Distribution Format Exchange Profile).
//!
//! **Writing.** One `<p>` per cue inside `body/div`, lines separated by `<br/>`; `<i>`, `<b>` and
//! `<u>` become `<span>`s with `tts:fontStyle="italic"`, `tts:fontWeight="bold"` and
//! `tts:textDecoration="underline"`. IMSC1 files signal `ttp:profile` (the IMSC1 Text profile
//! designator), a bottom region and a default style. With a frame rate, IMSC1 times are frame
//! offsets (`123f`) at `ttp:frameRate` / `ttp:frameRateMultiplier`, which are exact; without
//! one, and always for DFXP (for older players), times are clock times with milliseconds.
//!
//! **Reading** accepts TTML1 2010, the 2006 `ttaf1` drafts and TTML2:
//! clock times (`HH:MM:SS`, `.fraction`, `:FF` frames and `.subframes`), offset times (`h`, `m`,
//! `s`, `ms`, `f`, `t` with `ttp:tickRate`), `begin` / `end` / `dur`, nested `par` time
//! containers (`body` / `div` begins add up and their ends clip what is inside), timed `<span>`s inside an untimed `<p>`, `<br/>`, whitespace
//! collapsing, styles referenced by id (italic / bold / underline) and `ttm:agent` / `xml:id`.

use std::collections::{HashMap, HashSet};

use crate::time::{FrameRate, TICKS_PER_SECOND, Tick};

use craft_xml::{El, Limits, Node, Options, UnknownEntity};

use crate::{Cue, Document, Error, Result};

/// Which TTML flavour to write.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flavor {
    /// IMSC1 Text profile (TTML2 namespaces).
    Imsc1,
    /// DFXP (TTML1).
    Dfxp,
}

const NS_TT: &str = "http://www.w3.org/ns/ttml";
const NS_TTP: &str = "http://www.w3.org/ns/ttml#parameter";
const NS_TTS: &str = "http://www.w3.org/ns/ttml#styling";
const NS_TTM: &str = "http://www.w3.org/ns/ttml#metadata";
const NS_TTM_DRAFT: &str = "http://www.w3.org/2006/10/ttaf1#metadata";
const NS_TTM_DRAFT_APRIL: &str = "http://www.w3.org/2006/04/ttaf1#metadata";
const IMSC1_TEXT: &str = "http://www.w3.org/ns/ttml/profile/imsc1/text";
const DFXP_PROFILE: &str = "http://www.w3.org/ns/ttml/profile/dfxp-presentation";

/// TTML is normally only a few containers deep. Keep the XML reader and our recursive content
/// walkers safely below a native-stack-sized nesting level for untrusted caption files.
const MAX_XML_DEPTH: usize = 128;

/// How names are read: by namespace, whatever prefixes the file uses. The 2006 drafts keep
/// their own prefixes because the April draft spells agent ids differently.
const CANONICAL: craft_xml::Canonical<'static> = craft_xml::Canonical::new(&[
    ("tt", NS_TT),
    ("tt10", "http://www.w3.org/2006/10/ttaf1"),
    ("tt04", "http://www.w3.org/2006/04/ttaf1"),
    ("ttm", NS_TTM),
    ("ttm10", NS_TTM_DRAFT),
    ("ttm04", NS_TTM_DRAFT_APRIL),
    ("tts", NS_TTS),
    ("tts10", "http://www.w3.org/2006/10/ttaf1#style"),
    ("tts10s", "http://www.w3.org/2006/10/ttaf1#styling"),
    ("tts04", "http://www.w3.org/2006/04/ttaf1#style"),
    ("tts04s", "http://www.w3.org/2006/04/ttaf1#styling"),
]);

/// The line (from 1) of the root element's start tag, for error messages.
fn root_line(text: &str) -> usize {
    let mut rest = text;
    let mut line = 1usize;
    while let Some(i) = rest.find('<') {
        line = line.saturating_add(rest.get(..i).map_or(0, |t| t.matches('\n').count()));
        rest = rest.get(i..).unwrap_or("");
        let close = if rest.starts_with("<!--") {
            "-->"
        } else if rest.starts_with("<?") {
            "?>"
        } else if rest.starts_with("<!") {
            ">"
        } else {
            break;
        };
        let Some(end) = rest.find(close) else { break };
        let skipped = rest.get(..end.saturating_add(close.len())).unwrap_or("");
        line = line.saturating_add(skipped.matches('\n').count());
        rest = rest.get(skipped.len()..).unwrap_or("");
    }
    line
}

fn esc(s: &str) -> String {
    let mut o = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => o.push_str("&amp;"),
            '<' => o.push_str("&lt;"),
            '>' => o.push_str("&gt;"),
            '"' => o.push_str("&quot;"),
            '\'' => o.push_str("&apos;"),
            // XML 1.0 forbids these (C0 controls, U+FFFE, U+FFFF), even escaped
            c if !craft_xml::is_xml_char(c) => {}
            c => o.push(c),
        }
    }
    o
}

fn clock(t: Tick) -> String {
    crate::format_clock(t, '.')
}

/// Time expression for `t`: frames at `rate` when it is on a frame boundary, else clock time.
fn time_expr(t: Tick, rate: Option<FrameRate>) -> String {
    match rate {
        Some(r) if r.tick_of(r.frame_at(t)) == t => format!("{}f", r.frame_at(t)),
        _ => clock(t),
    }
}

/// Memoised "next occurrence of a quote byte" searches, so unterminated quotes are not rescanned.
#[derive(Default)]
struct QuoteCache {
    found: [Option<(usize, Option<usize>)>; 2],
}

impl QuoteCache {
    /// Index of the next `quote` byte at or after `from`.
    fn next(&mut self, bytes: &[u8], quote: u8, from: usize) -> Option<usize> {
        let slot = usize::from(quote == b'\'');
        if let Some((searched, result)) = self.found[slot]
            && searched <= from
            && result.is_none_or(|r| r >= from)
        {
            return result;
        }
        let result = bytes.get(from..).and_then(|t| t.iter().position(|&c| c == quote)).map(|i| from + i);
        self.found[slot] = Some((from, result));
        result
    }
}

/// Parses a tag starting at `bytes[start] == b'<'`: optional `/` (closing), a name of ASCII
/// letters/digits starting with a letter immediately after, then whitespace-separated
/// `name="value"` / `name='value'` attributes, optional `/` before `>`. A `>` inside a quoted
/// value does not end the tag. Returns `(index after '>', closing, lowercase name)`; the name is empty for a self-closing tag.
fn parse_tag(bytes: &[u8], start: usize, cache: &mut QuoteCache) -> Option<(usize, bool, String)> {
    let mut i = start + 1;
    let closing = bytes.get(i) == Some(&b'/');
    if closing {
        i += 1;
    }
    let name_start = i;
    if !bytes.get(i)?.is_ascii_alphabetic() {
        return None;
    }
    while bytes.get(i).is_some_and(u8::is_ascii_alphanumeric) {
        i += 1;
    }
    let name = String::from_utf8_lossy(bytes.get(name_start..i)?).to_ascii_lowercase();
    loop {
        let ws_start = i;
        while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
            i += 1;
        }
        match bytes.get(i)? {
            b'>' => return Some((i + 1, closing, name)),
            // self-closing: no markup
            b'/' if !closing && bytes.get(i + 1) == Some(&b'>') => return Some((i + 2, closing, String::new())),
            _ => {}
        }
        if i == ws_start {
            return None;
        }
        let key_start = i;
        while bytes.get(i).is_some_and(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b':' | b'_')) {
            i += 1;
        }
        if i == key_start || bytes.get(i) != Some(&b'=') {
            return None;
        }
        let quote = *bytes.get(i + 1)?;
        if quote != b'"' && quote != b'\'' {
            return None;
        }
        i = cache.next(bytes, quote, i + 2)? + 1;
    }
}

/// Caption text (`\n` lines, `<i>`/`<b>`/`<u>` tags) → TTML inline content.
fn inline(text: &str) -> String {
    let mut out = String::new();
    // Keep the logical style state separate from what has been written. There are only three
    // supported styles, so duplicate or crossing input tags cannot grow an unbounded span stack.
    let mut active: Vec<&'static str> = Vec::new();
    let mut emitted: Vec<&'static str> = Vec::new();
    let sync = |out: &mut String, active: &[&'static str], emitted: &mut Vec<&'static str>| {
        if *emitted != active {
            for _ in emitted.drain(..) {
                out.push_str("</span>");
            }
            for attr in active {
                out.push_str("<span ");
                out.push_str(attr);
                out.push('>');
                emitted.push(attr);
            }
        }
    };
    let mut rest = text;
    let mut cache = QuoteCache::default();
    while !rest.is_empty() {
        // Only a well-formed tag is markup; "1 < 2 and 3 > 2" and "a<b and c>d" are text.
        if rest.starts_with('<')
            && let Some((end, closing, name)) = parse_tag(text.as_bytes(), text.len() - rest.len(), &mut cache)
        {
            let attr = match name.as_str() {
                "i" => Some("tts:fontStyle=\"italic\""),
                "b" => Some("tts:fontWeight=\"bold\""),
                "u" => Some("tts:textDecoration=\"underline\""),
                _ => None,
            };
            if let Some(a) = attr {
                if closing {
                    active.retain(|style| *style != a);
                } else if !active.contains(&a) {
                    active.push(a);
                }
            }
            rest = text.get(end..).unwrap_or("");
            continue;
        }
        let Some(ch) = rest.chars().next() else { break };
        sync(&mut out, &active, &mut emitted);
        // a stored entity is the character it names, escaped once for XML
        if let Some((decoded, len)) = crate::entity_at(rest) {
            out.push_str(&esc(&decoded.to_string()));
            rest = rest.get(len..).unwrap_or("");
            continue;
        }
        if ch == '\n' {
            out.push_str("<br/>");
        } else {
            out.push_str(&esc(&ch.to_string()));
        }
        rest = &rest[ch.len_utf8()..];
    }
    for _ in emitted {
        out.push_str("</span>");
    }
    out
}

/// An XML name without a colon (Namespaces in XML, NCName).
fn is_ncname(s: &str) -> bool {
    craft_xml::is_name(s) && !s.contains(':')
}

fn cue_ids(doc: &Document) -> Vec<String> {
    // Reserve every valid explicit ID before allocating fallbacks, including IDs on later cues.
    let reserved: HashSet<&str> =
        ["s1", "r1"].into_iter().chain(doc.cues.iter().filter_map(|cue| cue.id.as_deref()).filter(|id| is_ncname(id))).collect();
    let mut used: HashSet<String> = ["s1".into(), "r1".into()].into_iter().collect();
    doc.cues
        .iter()
        .enumerate()
        .map(|(index, cue)| {
            if let Some(id) = cue.id.as_deref().filter(|id| is_ncname(id))
                && used.insert(id.to_string())
            {
                return id.to_string();
            }
            let mut id = format!("c{}", index.saturating_add(1));
            // Each collision consumes a distinct reserved or used ID, so this loop is bounded by their combined size.
            while reserved.contains(id.as_str()) || used.contains(&id) {
                id.push('_');
            }
            used.insert(id.clone());
            id
        })
        .collect()
}

/// Write `doc` as TTML. `rate` (IMSC1 only) makes times exact frame offsets; `lang` is the
/// document language (`xml:lang`).
pub fn write(doc: &Document, flavor: Flavor, rate: Option<FrameRate>, lang: &str) -> String {
    let mut o = String::from("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    let rate = if flavor == Flavor::Imsc1 { rate } else { None };
    o.push_str(&format!("<tt xmlns=\"{NS_TT}\" xmlns:ttp=\"{NS_TTP}\" xmlns:tts=\"{NS_TTS}\" xmlns:ttm=\"{NS_TTM}\""));
    o.push_str(&format!(" ttp:profile=\"{}\" ttp:timeBase=\"media\"", if flavor == Flavor::Imsc1 { IMSC1_TEXT } else { DFXP_PROFILE }));
    if let Some(r) = rate {
        let base = r.timecode_base();
        o.push_str(&format!(" ttp:frameRate=\"{base}\""));
        // effective rate = base × multiplier
        let (mn, md) = (r.num, r.den * base);
        let g = gcd(mn, md);
        if (mn / g, md / g) != (1, 1) {
            o.push_str(&format!(" ttp:frameRateMultiplier=\"{} {}\"", mn / g, md / g));
        }
    }
    o.push_str(&format!(" xml:lang=\"{}\">\n", esc(if lang.is_empty() { "en" } else { lang })));
    // Keep agent identifiers distinct from the IDs already emitted for cues, style and region.
    let cue_ids = cue_ids(doc);
    let mut ids: HashSet<String> = ["s1".into(), "r1".into()].into_iter().collect();
    ids.extend(cue_ids.iter().cloned());
    let mut speakers = Vec::new();
    let mut speaker_ids = HashMap::new();
    for speaker in doc.cues.iter().filter_map(|c| c.speaker.as_deref()) {
        if speaker_ids.contains_key(speaker) {
            continue;
        }
        let mut id = format!("agent{}", speakers.len() + 1);
        // Each collision consumes a distinct reserved ID, so this is bounded by ids.len().
        while ids.contains(&id) {
            id.push('_');
        }
        ids.insert(id.clone());
        speaker_ids.insert(speaker, id.clone());
        speakers.push((speaker, id));
    }
    o.push_str("  <head>\n");
    for (speaker, id) in speakers {
        o.push_str(&format!(
            "    <ttm:agent xml:id=\"{id}\" type=\"other\"><ttm:name type=\"full\" xml:space=\"preserve\">{}</ttm:name></ttm:agent>\n",
            // XML 1.0 §2.11 normalizes literal CR even with xml:space="preserve".
            esc(speaker).replace('\r', "&#13;")
        ));
    }
    o.push_str("    <styling>\n      <style xml:id=\"s1\" tts:color=\"white\" tts:fontFamily=\"proportionalSansSerif\" tts:fontSize=\"100%\" tts:textAlign=\"center\"/>\n    </styling>\n");
    o.push_str(
        "    <layout>\n      <region xml:id=\"r1\" tts:origin=\"10% 10%\" tts:extent=\"80% 80%\" tts:displayAlign=\"after\"/>\n    </layout>\n",
    );
    o.push_str("  </head>\n  <body region=\"r1\" style=\"s1\">\n    <div>\n");
    for (c, id) in doc.cues.iter().zip(cue_ids) {
        let agent =
            c.speaker.as_deref().and_then(|s| speaker_ids.get(s)).map(|id| format!(" ttm:role=\"x-speaker\" ttm:agent=\"{id}\"")).unwrap_or_default();
        o.push_str(&format!(
            "      <p xml:id=\"{}\" begin=\"{}\" end=\"{}\"{agent}>{}</p>\n",
            esc(&id),
            time_expr(c.start, rate),
            time_expr(c.end, rate),
            inline(&c.text)
        ));
    }
    o.push_str("    </div>\n  </body>\n</tt>\n");
    o
}

fn gcd(a: i64, b: i64) -> i64 {
    if b == 0 { a.abs().max(1) } else { gcd(b, a % b) }
}

// ------------------------------------------------------------------------------------------------
// Reader
// ------------------------------------------------------------------------------------------------

/// Timing parameters from the root element.
#[derive(Clone, Copy)]
struct Timing {
    /// Effective frame rate (frameRate × multiplier).
    rate: FrameRate,
    /// Nominal frames per second for `HH:MM:SS:FF`.
    base: i64,
    sub_frame_rate: i64,
    /// Ticks per second, rational when derived from the effective frame rate.
    tick_rate: FrameRate,
}

fn positive_integer(s: &str) -> Option<i64> {
    let s = s.trim();
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    s.parse::<i64>().ok().filter(|v| *v > 0)
}

fn parse_ratio(s: &str) -> Option<(i64, i64)> {
    let mut it = s.split_whitespace().map(positive_integer);
    let a = it.next()??;
    let b = it.next()??;
    it.next().is_none().then_some((a, b))
}

/// The local part of a canonical name.
fn local(name: &str) -> &str {
    name.rsplit_once(':').map_or(name, |(_, l)| l)
}

/// The canonical prefix of a name (its namespace), `""` for none.
fn prefix(name: &str) -> &str {
    name.rsplit_once(':').map_or("", |(p, _)| p)
}

/// The first attribute with this local name, in any namespace.
fn attr<'a>(n: &'a El, name: &str) -> Option<&'a str> {
    n.attrs.iter().find(|(k, _)| local(k) == name).map(|(_, v)| v.as_str())
}

/// Is this canonical prefix one of the TTML metadata namespaces?
fn metadata_namespace(prefix: &str) -> bool {
    matches!(prefix, "ttm" | "ttm10" | "ttm04")
}

fn is_metadata(n: &El, name: &str) -> bool {
    local(&n.name) == name && metadata_namespace(prefix(&n.name))
}

/// Significant TTML agents are children of head or of head's metadata children (TTML1 §12.1.5).
fn speaker_names(root: &El) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let content = |n: &El, name: &str| local(&n.name) == name && prefix(&n.name) == prefix(&root.name);
    let Some(head) = root.elements().find(|n| content(n, "head")) else {
        return out;
    };
    // head's own agents and those of its metadata children, in document order
    let agents = head.elements().flat_map(|n| {
        let inner = if content(n, "metadata") { Some(n.elements()) } else { None };
        std::iter::once(n).chain(inner.into_iter().flatten())
    });
    for agent in agents.filter(|n| is_metadata(n, "agent")) {
        // The April 2006 draft defines an unqualified id, before TTML adopted xml:id.
        let id = if prefix(&agent.name) == "ttm04" { agent.attr("id") } else { agent.attr("xml:id") };
        let Some(id) = id else { continue };
        let names: Vec<_> = agent.elements().filter(|n| is_metadata(n, "name")).map(|n| (n.attr("type"), n.text())).collect();
        let name = names
            .iter()
            .find(|(kind, _)| *kind == Some("full"))
            .or_else(|| names.iter().find(|(_, text)| !text.is_empty()))
            .or_else(|| names.first());
        if let Some((_, name)) = name {
            // An explicitly empty full name remains empty, including our own Some("") speakers.
            out.insert(id.to_string(), name.to_string());
        }
    }
    out
}

fn speaker(n: &El, names: &HashMap<String, String>) -> Option<String> {
    let refs =
        n.attrs.iter().find(|(k, _)| local(k) == "agent" && metadata_namespace(prefix(k))).map(|(_, v)| v.as_str()).or_else(|| n.attr("agent"));
    refs.map(|refs| {
        // A cue has one speaker label. Keep document-defined long names and repeated
        // references from multiplying into an unbounded per-cue allocation.
        const MAX_REFS: usize = 64;
        const MAX_LABEL_BYTES: usize = 4096;
        let mut out = String::new();
        for id in refs.split_whitespace().take(MAX_REFS) {
            if !out.is_empty() {
                if out.len() + 2 > MAX_LABEL_BYTES {
                    break;
                }
                out.push_str(", ");
            }
            let name = names.get(id).map(String::as_str).unwrap_or(id);
            if names.contains_key(id) {
                let room = MAX_LABEL_BYTES.saturating_sub(out.len());
                let mut end = 0;
                for (i, ch) in name.char_indices() {
                    if i + ch.len_utf8() > room {
                        break;
                    }
                    end = i + ch.len_utf8();
                }
                out.push_str(&name[..end]);
            } else {
                for ch in id.trim_start_matches('_').chars() {
                    if out.len() + ch.len_utf8() > MAX_LABEL_BYTES {
                        break;
                    }
                    out.push(if ch == '_' { ' ' } else { ch });
                }
            }
            if out.len() == MAX_LABEL_BYTES {
                break;
            }
        }
        out
    })
}

fn timing(root: &El, line: usize) -> Result<Timing> {
    let invalid = |name: &str| Error::Syntax { line, msg: format!("invalid TTML timing parameter {name}") };
    let integer = |name: &str, default| match attr(root, name) {
        Some(v) => positive_integer(v).ok_or_else(|| invalid(name)),
        None => Ok(default),
    };
    let base = integer("frameRate", 30)?;
    let (mn, md) = match attr(root, "frameRateMultiplier") {
        Some(v) => parse_ratio(v).ok_or_else(|| invalid("frameRateMultiplier"))?,
        None => (1, 1),
    };
    let rate = FrameRate::new(base.checked_mul(mn).ok_or_else(|| invalid("frameRateMultiplier"))?, md);
    let sub = integer("subFrameRate", 1)?;
    let tick_rate = match attr(root, "tickRate") {
        Some(v) => FrameRate::new(positive_integer(v).ok_or_else(|| invalid("tickRate"))?, 1),
        // TTML1 §6.2.10: absent tickRate uses the effective frame rate times subFrameRate.
        None if attr(root, "frameRate").is_some() => FrameRate::new(rate.num.checked_mul(sub).ok_or_else(|| invalid("subFrameRate"))?, rate.den),
        None => FrameRate::new(1, 1),
    };
    Ok(Timing { rate, base, sub_frame_rate: sub, tick_rate })
}

/// Seconds as an exact decimal string → ticks.
fn decimal_ticks(int: &str, frac: &str, unit_ticks: i128) -> Option<i128> {
    let i: i128 = if int.is_empty() { 0 } else { int.parse().ok()? };
    let mut t = i.checked_mul(unit_ticks)?;
    if !frac.is_empty() {
        if frac.len() > 12 || !frac.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let f: i128 = frac.parse().ok()?;
        t = t.checked_add(f.checked_mul(unit_ticks)? / 10i128.pow(frac.len() as u32))?;
    }
    Some(t)
}

/// Parse a TTML time expression into ticks.
fn parse_time(s: &str, tm: Timing) -> Option<Tick> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let tps = TICKS_PER_SECOND as i128;
    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        let num =
            |p: &str| -> Option<i128> { (!p.is_empty() && p.len() <= 6 && p.bytes().all(|b| b.is_ascii_digit())).then(|| p.parse().ok()).flatten() };
        let (h, m) = (num(parts.first()?)?, num(parts.get(1)?)?);
        let sec_part = parts.get(2)?;
        let (si, sf) = sec_part.split_once('.').unwrap_or((sec_part, ""));
        let seconds = num(si)?;
        if m >= 60 || seconds >= 60 {
            return None;
        }
        let mut t = (h * 3600 + m * 60) * tps + decimal_ticks(si, sf, tps)?;
        if let Some(fp) = parts.get(3) {
            if parts.len() > 4 {
                return None;
            }
            let (fi, fs) = fp.split_once('.').unwrap_or((fp, ""));
            let frames = num(fi)?;
            let sub: i128 = if fs.is_empty() { 0 } else { num(fs)? };
            if frames >= i128::from(tm.base) || sub >= i128::from(tm.sub_frame_rate) {
                return None;
            }
            // frames at the effective rate; sub-frames divide a frame
            let ft = tm.rate.tick_of(1).0 as i128;
            t += frames * ft + sub * ft / tm.sub_frame_rate.max(1) as i128;
        }
        return i64::try_from(t).ok().map(Tick);
    }
    let unit_start = s.find(|c: char| c.is_ascii_alphabetic())?;
    let (v, unit) = s.split_at(unit_start);
    let (vi, vf) = v.split_once('.').unwrap_or((v, ""));
    if vi.is_empty() || !vi.bytes().all(|b| b.is_ascii_digit()) || vi.len() > 15 {
        return None;
    }
    let t = match unit {
        "h" => decimal_ticks(vi, vf, tps * 3600)?,
        "m" => decimal_ticks(vi, vf, tps * 60)?,
        "s" => decimal_ticks(vi, vf, tps)?,
        "ms" => decimal_ticks(vi, vf, tps / 1000)?,
        "f" => {
            let n: i128 = vi.parse().ok()?;
            let fd = tm.rate.tick_of(1).0 as i128;
            let whole = tm.rate.tick_of(i64::try_from(n).ok()?);
            // tick_of saturates; a saturated count overflowed rather than named a real time
            if whole == Tick(i64::MAX) {
                return None;
            }
            whole.0 as i128 + decimal_ticks("0", vf, fd)?
        }
        "t" => decimal_ticks(vi, vf, tps * i128::from(tm.tick_rate.den))? / i128::from(tm.tick_rate.num),
        _ => return None,
    };
    i64::try_from(t).ok().map(Tick)
}

#[derive(Clone, Copy, Default, PartialEq)]
struct Fmt {
    italic: bool,
    bold: bool,
    underline: bool,
}

/// An explicit `inherit` restores the parent value, even after earlier style references.
#[derive(Clone, Copy)]
enum FmtValue {
    Set(bool),
    Inherit,
}

impl FmtValue {
    fn resolve(self, inherited: bool) -> bool {
        match self {
            Self::Set(value) => value,
            Self::Inherit => inherited,
        }
    }
}

/// The style properties an element or a `<style>` sets; `None` leaves the inherited value.
#[derive(Clone, Copy, Default)]
struct FmtSpec {
    italic: Option<FmtValue>,
    bold: Option<FmtValue>,
    underline: Option<FmtValue>,
}

impl FmtSpec {
    fn apply(self, fmt: &mut Fmt, parent: Fmt) {
        if let Some(v) = self.italic {
            fmt.italic = v.resolve(parent.italic);
        }
        if let Some(v) = self.bold {
            fmt.bold = v.resolve(parent.bold);
        }
        if let Some(v) = self.underline {
            fmt.underline = v.resolve(parent.underline);
        }
    }
}

/// A styling attribute: from the TTML styling namespaces in a fixed order (whatever the order in
/// the file), else unqualified for compatibility. Other namespaces are ignored.
fn styling_attr<'a>(n: &'a El, name: &str) -> Option<&'a str> {
    ["tts", "tts10", "tts10s", "tts04", "tts04s"].into_iter().find_map(|p| n.attr(&format!("{p}:{name}"))).or_else(|| n.attr(name))
}

/// Is this element in a TTML content namespace (or none)?
fn content_element(n: &El, name: &str) -> bool {
    local(&n.name) == name && matches!(prefix(&n.name), "" | "tt" | "tt10" | "tt04")
}

fn style_of(n: &El, styles: &HashMap<String, FmtSpec>, parent: Fmt) -> Fmt {
    let mut f = parent;
    if let Some(refs) = n.attr("style") {
        for r in refs.split_whitespace() {
            if let Some(s) = styles.get(r) {
                s.apply(&mut f, parent);
            }
        }
    }
    own_fmt(n).apply(&mut f, parent);
    f
}

fn own_fmt(n: &El) -> FmtSpec {
    FmtSpec {
        italic: styling_attr(n, "fontStyle").and_then(|v| match v.trim() {
            "inherit" => Some(FmtValue::Inherit),
            "italic" | "oblique" => Some(FmtValue::Set(true)),
            "normal" | "reverseOblique" => Some(FmtValue::Set(false)),
            _ => None,
        }),
        bold: styling_attr(n, "fontWeight").and_then(|v| match v.trim() {
            "inherit" => Some(FmtValue::Inherit),
            "bold" => Some(FmtValue::Set(true)),
            "normal" => Some(FmtValue::Set(false)),
            _ => None,
        }),
        underline: styling_attr(n, "textDecoration").and_then(|v| {
            let v = v.trim();
            if v == "inherit" {
                Some(FmtValue::Inherit)
            } else if v.contains("noUnderline") || v == "none" {
                Some(FmtValue::Set(false))
            } else if v.contains("underline") {
                Some(FmtValue::Set(true))
            } else {
                None
            }
        }),
    }
}

/// Text being collected from a `<p>` or timed `<span>`. XML whitespace collapses to one space in
/// the style of its first character, written only between visible text. Line breaks are kept,
/// each in the style it was in, also at the start and end of a cue.
struct Out {
    s: String,
    pending: Option<Fmt>,
    has_text: bool,
    breaks: Vec<Fmt>,
    tags: Vec<&'static str>,
}

impl Out {
    fn flush(&mut self) {
        if !self.breaks.is_empty() {
            for fmt in std::mem::take(&mut self.breaks) {
                self.set_fmt(fmt);
                self.s.push('\n');
            }
        } else if self.has_text
            && let Some(fmt) = self.pending
        {
            self.set_fmt(fmt);
            self.s.push(' ');
        }
        self.pending = None;
    }

    /// Keep the common tag prefix open and close removed tags in reverse order to preserve nesting.
    fn set_fmt(&mut self, fmt: Fmt) {
        let tags = [(fmt.italic, "i"), (fmt.bold, "b"), (fmt.underline, "u")].into_iter().filter(|x| x.0).map(|x| x.1);
        let common = self.tags.iter().copied().zip(tags.clone()).take_while(|(a, b)| a == b).count();
        for t in self.tags.iter().skip(common).rev() {
            self.s.push_str(&format!("</{t}>"));
        }
        self.tags.truncate(common);
        for t in tags.skip(common) {
            self.s.push_str(&format!("<{t}>"));
            self.tags.push(t);
        }
    }
}

/// Collect a `<p>`'s (or `<span>`'s) text with `<i>`/`<b>`/`<u>` tags, collapsing whitespace.
/// An explicit stack keeps nested inline content off the call stack.
fn collect<'a>(n: &'a El, styles: &HashMap<String, FmtSpec>, fmt: Fmt, out: &mut Out) {
    // a child `<span>` takes its own style on top of its parent's
    let push_children = |pending: &mut Vec<(&'a Node, Fmt)>, e: &'a El, fmt: Fmt| {
        for child in e.children.iter().rev() {
            let child_fmt = match child {
                Node::El(c) if local(&c.name) == "span" => style_of(c, styles, fmt),
                _ => fmt,
            };
            pending.push((child, child_fmt));
        }
    };
    let mut pending: Vec<(&Node, Fmt)> = Vec::new();
    push_children(&mut pending, n, fmt);
    while let Some((node, fmt)) = pending.pop() {
        match node {
            Node::Text(text) | Node::CData(text) => {
                for (i, ch) in text.char_indices() {
                    if is_xml_space(ch) {
                        out.pending.get_or_insert(fmt);
                    } else {
                        out.flush();
                        out.set_fmt(fmt);
                        // Cue text carries its own `<i>`/`<b>`/`<u>` tags, so a literal `<` is kept
                        // as `&lt;` (as a WebVTT import keeps its entities), and a literal `&` that
                        // would read as an entity ("&lt;" typed as text) as `&amp;`; plain text and
                        // the writers decode them. Any other `&`, and a `>`, stay as they are.
                        match ch {
                            '<' => out.s.push_str("&lt;"),
                            '&' if crate::entity_at(text.get(i..).unwrap_or("")).is_some() => out.s.push_str("&amp;"),
                            _ => out.s.push(ch),
                        }
                        out.has_text = true;
                    }
                }
            }
            Node::El(e) if local(&e.name) == "br" => {
                // a collapsible space next to a hard line break goes
                out.pending = None;
                out.breaks.push(fmt);
            }
            Node::El(e) => push_children(&mut pending, e, fmt),
            _ => {}
        }
    }
}

/// XML whitespace: only these characters collapse (U+00A0 and other Unicode spaces are content).
fn is_xml_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r')
}

/// The cue text of `n` in style `fmt`: empty when it has no visible text (only spaces or breaks).
fn cue_text(n: &El, styles: &HashMap<String, FmtSpec>, fmt: Fmt) -> String {
    let mut o = Out { s: String::new(), pending: None, has_text: false, breaks: Vec::new(), tags: Vec::new() };
    collect(n, styles, fmt, &mut o);
    if !o.has_text {
        return String::new();
    }
    // trailing breaks stay; a trailing space does not
    o.pending = None;
    o.flush();
    o.set_fmt(Fmt::default());
    o.s
}

pub fn parse(text: &str) -> Result<Document> {
    let limits = Limits { max_depth: MAX_XML_DEPTH, ..Limits::DEFAULT };
    let options = Options { unknown_entity: UnknownEntity::Error, ..Options::canonical(&CANONICAL).strict().with_limits(limits) };
    let root = match options.parse(text.as_bytes()) {
        Ok(root) => root,
        Err(craft_xml::Error::TooDeep(_)) => {
            return Err(Error::Syntax { line: 1, msg: format!("TTML XML nesting exceeds {MAX_XML_DEPTH} elements") });
        }
        Err(e) if e.is_limit() => return Err(Error::Syntax { line: 1, msg: e.to_string() }),
        Err(_) => return Err(Error::NotFormat("TTML")),
    };
    let root = &root;
    if local(&root.name) != "tt" {
        return Err(Error::NotFormat("TTML"));
    }
    let tm = timing(root, root_line(text))?;
    let speakers = speaker_names(root);
    let mut styles = HashMap::new();
    for s in root.descendants().filter(|n| content_element(n, "style")) {
        if let Some(id) = s.attr("xml:id").or_else(|| s.attr("id")) {
            styles.insert(id.to_string(), own_fmt(s));
        }
    }
    let mut warnings = Vec::new();
    let language = language(root, &mut warnings);
    let mut doc = Document { warnings, language, ..Default::default() };
    let Some(body) = root.elements().find(|n| local(&n.name) == "body") else {
        return Ok(doc);
    };
    if std::iter::once(root).chain(root.descendants()).any(|n| attr(n, "timeContainer").is_some_and(|v| v.trim() == "seq")) {
        doc.warnings.push("timeContainer=\"seq\" is not supported; its children are timed as in a par container".into());
    }
    // the body is a time container like a div: its begin offsets, and its end clips, everything inside
    let t = times(body, Scope { begin: Tick::ZERO, end: None }, tm, &mut doc);
    walk(
        body,
        Scope { begin: t.begin.unwrap_or(Tick::ZERO), end: t.end },
        &tm,
        &styles,
        &speakers,
        style_of(body, &styles, Fmt::default()),
        &mut doc,
    );
    doc.cues.sort_by_key(|c| (c.start, c.end));
    Ok(doc)
}

/// The longest language tag kept from a file: RFC 5646 §4.4.1 asks every implementation to
/// take tags of at least 35 characters.
const MAX_LANGUAGE: usize = 35;

/// The document language: `xml:lang` on `tt`, when it is a language tag (empty means unknown).
fn language(root: &El, warnings: &mut Vec<String>) -> Option<String> {
    let lang = root.attr("xml:lang")?.trim();
    if lang.is_empty() {
        return None;
    }
    if lang.len() > MAX_LANGUAGE || !lang.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
        warnings.push("ignored xml:lang: not a language tag".into());
        return None;
    }
    Some(lang.to_string())
}

/// The active interval of the enclosing time containers (`par`): child times are offsets from
/// `begin`, and nothing inside outlasts `end` (None when no ancestor sets an end).
#[derive(Clone, Copy)]
struct Scope {
    begin: Tick,
    end: Option<Tick>,
}

/// Whether `n` has timing attributes of its own.
fn has_timing(n: &El) -> bool {
    ["begin", "end", "dur"].into_iter().any(|k| attr(n, k).is_some())
}

/// Timing of an element in its parent container.
struct Times {
    /// The begin, when the element sets one.
    begin: Option<Tick>,
    /// Its own end (`end`, or `begin` + `dur`, the earlier of the two) clipped to the parent's
    /// end, else the parent's end.
    end: Option<Tick>,
    /// A timing attribute did not parse (and was warned about).
    bad: bool,
}

fn times(n: &El, parent: Scope, tm: Timing, doc: &mut Document) -> Times {
    let mut bad = false;
    let mut p = |k: &str, doc: &mut Document| -> Option<Tick> {
        let v = attr(n, k)?;
        let t = parse_time(v, tm);
        if t.is_none() {
            bad = true;
            doc.warnings.push(format!("bad time expression \"{v}\""));
        }
        t
    };
    let begin = p("begin", doc).map(|b| parent.begin + b);
    let own_end = match (p("end", doc).map(|e| parent.begin + e), p("dur", doc).map(|d| begin.unwrap_or(parent.begin) + d)) {
        (Some(e), Some(d)) => Some(e.min(d)),
        (e, d) => e.or(d),
    };
    let end = match (own_end, parent.end) {
        (Some(e), Some(pe)) => Some(e.min(pe)),
        (e, pe) => e.or(pe),
    };
    Times { begin, end, bad }
}

fn walk(n: &El, parent: Scope, tm: &Timing, styles: &HashMap<String, FmtSpec>, speakers: &HashMap<String, String>, fmt: Fmt, doc: &mut Document) {
    for c in n.elements() {
        let f = style_of(c, styles, fmt);
        match local(&c.name) {
            "div" => {
                let t = times(c, parent, *tm, doc);
                walk(c, Scope { begin: t.begin.unwrap_or(parent.begin), end: t.end }, tm, styles, speakers, f, doc);
            }
            "p" => {
                let t = times(c, parent, *tm, doc);
                if t.bad {
                    continue;
                }
                let (b, e) = (t.begin.unwrap_or(parent.begin), t.end);
                let cue_speaker = speaker(c, speakers);
                let id = attr(c, "id").map(str::to_string);
                let spans: Vec<&El> = c.elements().filter(|x| local(&x.name) == "span" && has_timing(x)).collect();
                // a timed paragraph, or one without timed spans in a container with an end, is one cue
                let whole = if has_timing(c) || spans.is_empty() { e } else { None };
                match whole {
                    Some(e) => {
                        let text = cue_text(c, styles, f);
                        if e > b && !text.is_empty() {
                            doc.cues.push(Cue { start: b, end: e, text, speaker: cue_speaker, id, ..Default::default() });
                        }
                    }
                    None => {
                        // untimed paragraph: its timed spans are the cues
                        let scope = Scope { begin: b, end: e };
                        for sp in spans {
                            let st = times(sp, scope, *tm, doc);
                            let sb = st.begin.unwrap_or(scope.begin);
                            if let (false, Some(se)) = (st.bad, st.end) {
                                let text = cue_text(sp, styles, style_of(sp, styles, f));
                                if se > sb && !text.is_empty() {
                                    doc.cues.push(Cue {
                                        start: sb,
                                        end: se,
                                        text,
                                        speaker: speaker(sp, speakers).or_else(|| cue_speaker.clone()),
                                        ..Default::default()
                                    });
                                }
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

