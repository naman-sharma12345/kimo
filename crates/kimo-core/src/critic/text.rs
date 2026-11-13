//! Text critic: titles and captions a viewer cannot read. Too fast for the reading speed, outside title-safe (cut off on
//! TVs and covered by phone UI), and characters the font cannot draw.
use super::{frame, r3, Finding, Severity};
use crate::model::{Project, Source};
use serde_json::json;

/// Comfortable reading speed for on-screen text, characters per second (subtitle guidelines use 15-17).
pub const READ_CPS: f64 = 17.0;
/// Title-safe margin on each side, as a share of the canvas.
pub const TITLE_SAFE: f64 = 0.05;

/// Rough on-screen box of a text clip at clip-local time `lt`: (left, top, right, bottom) in canvas pixels. Uses the
/// renderer's own resolved transform (parenting included); the glyph box is estimated from the font size.
pub fn approx_box(p: &Project, c: &crate::model::Clip, lt: f64) -> Option<(f64, f64, f64, f64)> {
    let Source::Text { text, size, .. } = &c.source else { return None };
    let xf = crate::render::resolve_xform(p, c, lt);
    let sz = size * xf.scale;
    let lines: Vec<&str> = text.split('\n').collect();
    let longest = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64;
    let (w, h) = (longest * sz * 0.56, lines.len() as f64 * sz * 1.2);
    let (cx, cy) = (p.width as f64 / 2.0 + xf.x, p.height as f64 / 2.0 + xf.y);
    Some((cx - w / 2.0, cy - h / 2.0, cx + w / 2.0, cy + h / 2.0))
}

pub fn check(p: &Project) -> Vec<Finding> {
    let mut out = vec![];
    let fr = frame(p);
    let (w, h) = (p.width as f64, p.height as f64);
    let captions: Vec<&String> = p.editorial.captions.values().flat_map(|g| g.clips.iter()).collect();
    for tr in &p.tracks { for c in &tr.clips {
        let Source::Text { text, font, .. } = &c.source else { continue };
        let chars = text.chars().filter(|ch| !ch.is_whitespace()).count() as f64;
        if chars == 0.0 { continue; }
        // reading speed (captions follow speech, so only titles are held to it)
        let need = chars / READ_CPS + 0.3;
        if !captions.contains(&&c.id) && c.duration + fr * 0.5 < need {
            out.push(Finding::new("text_too_fast", Severity::Warning, "text", format!("'{}' is on screen {:.2} s; reading {} characters needs about {:.1} s", short(text), c.duration, chars as u32, need))
                .clip(&c.id).range(c.start, c.start + c.duration).measured(json!({"seconds": r3(c.duration), "chars_per_s": r3(chars / c.duration.max(fr))})).threshold(json!({"max_chars_per_s": READ_CPS}))
                .fix("trim_clip", json!({"clip_id": c.id, "duration": r3(need)})));
        }
        // title safe, at the middle of the clip
        if let Some((l, t, r, b)) = approx_box(p, c, c.duration / 2.0) {
            let (ml, mt) = (w * TITLE_SAFE, h * TITLE_SAFE);
            let over = [(ml - l).max(0.0), (mt - t).max(0.0), (r - (w - ml)).max(0.0), (b - (h - mt)).max(0.0)];
            if over.iter().any(|o| *o > 2.0) {
                let fit = ((w - 2.0 * ml) / (r - l).max(1.0)).min((h - 2.0 * mt) / (b - t).max(1.0)).min(1.0);
                let mut f = Finding::new("text_outside_title_safe", Severity::Warning, "text", format!("'{}' runs outside the title-safe area by up to {:.0} px: it gets cut off on TVs and covered by phone UI", short(text), over.iter().cloned().fold(0.0, f64::max)))
                    .clip(&c.id).at(c.start + c.duration / 2.0).measured(json!({"box": [r3(l), r3(t), r3(r), r3(b)], "over_px": over.map(r3)})).threshold(json!({"margin": TITLE_SAFE}));
                f = if fit < 0.98 { if let Source::Text { size, .. } = &c.source { f.fix("set_text", json!({"clip_id": c.id, "size": r3(size * fit * 0.97)})) } else { f } }
                    else { f.fix("set_property", json!({"clip_id": c.id, "property": if over[0] > 0.0 || over[2] > 0.0 { "x" } else { "y" }, "value": 0.0})) };
                out.push(f);
            }
        }
        let miss = crate::text::missing_glyphs(text, font, c.text_fx.as_ref().map(|x| x.weight).unwrap_or(0.0));
        if !miss.is_empty() {
            let m: String = miss.iter().collect();
            out.push(Finding::new("missing_glyph", Severity::Warning, "text", format!("the font {font} has no glyph for '{m}': those characters are left out of the picture. Pick a font that has them (list_fonts; e.g. Nirmala UI for Devanagari on Windows, Noto Sans for most scripts)"))
                .clip(&c.id).at(c.start).measured(json!({"chars": m})));
        }
    } }
    out
}

fn short(t: &str) -> String { let s: String = t.chars().take(28).collect(); if t.chars().count() > 28 { format!("{s}…") } else { s } }

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    #[test]
    fn fast_and_offscreen_text() {
        let mut ss = session();
        let t = track(&mut ss);
        let fast = text(&mut ss, &t, "This sentence is far too long to read in half a second", 0.0, 0.5);
        let off = text(&mut ss, &t, "Edge", 2.0, 3.0);
        call(&mut ss, "set_property", json!({"clip_id": off, "property": "x", "value": 900.0}));
        let f = check(&ss.project);
        assert!(f.iter().any(|x| x.code == "text_too_fast" && x.clip_id.as_deref() == Some(fast.as_str())), "{:?}", codes(&f));
        assert!(f.iter().any(|x| x.code == "text_outside_title_safe" && x.clip_id.as_deref() == Some(off.as_str())), "{:?}", codes(&f));
    }
    #[test]
    fn a_held_centred_title_is_clean() {
        let mut ss = session();
        let t = track(&mut ss);
        text(&mut ss, &t, "Hello", 0.0, 3.0);
        assert!(check(&ss.project).iter().all(|x| x.code == "missing_glyph"), "{:?}", codes(&check(&ss.project)));
    }
}
