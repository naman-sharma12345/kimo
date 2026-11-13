//! Editing critic: the cut itself. Flash clips, black gaps, clips that run past the end of their source (a frozen or
//! silent tail) and jump cuts.
use super::{frame, r3, Finding, Severity};
use crate::model::{Project, Source};
use serde_json::json;

/// Is this clip a picture (anything but an audio-only media file)?
pub fn is_picture(p: &Project, c: &crate::model::Clip) -> bool {
    match &c.source { Source::Media { asset } => p.assets.get(asset).map(|a| a.width > 0).unwrap_or(true), _ => true }
}

pub fn check(p: &Project) -> Vec<Finding> {
    let mut out = vec![];
    let fr = frame(p);
    let end = crate::ext::export_manifest::project_end(p);
    // flash clips: a picture on screen for under 3 frames reads as a glitch
    for t in &p.tracks { for c in &t.clips {
        if is_picture(p, c) && c.duration > 0.0 && c.duration < 3.0 * fr - 1e-9 {
            out.push(Finding::new("flash_clip", Severity::Warning, "editing", format!("{} is on screen for {:.0} frame(s); it reads as a glitch", c.id, (c.duration / fr).round()))
                .clip(&c.id).range(c.start, c.start + c.duration).measured(json!({"frames": r3(c.duration / fr)})).threshold(json!({"min_frames": 3}))
                .fix("delete_clip", json!({"clip_id": c.id})));
        }
    } }
    // black gaps: timeline time with no picture at all
    if end > 0.0 {
        let mut iv: Vec<(f64, f64, String)> = p.tracks.iter().flat_map(|t| t.clips.iter().filter(|c| is_picture(p, c)).map(move |c| (c.start, c.start + c.duration, t.id.clone()))).collect();
        iv.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let mut cursor = 0.0; let mut last_track = iv.first().map(|x| x.2.clone()).unwrap_or_default();
        for (s, e, tr) in &iv {
            if *s > cursor + fr * 0.5 {
                let (a, b) = (cursor, *s);
                out.push(Finding::new("black_gap", Severity::Error, "editing", format!("no picture from {a:.2} s to {b:.2} s: the export is black there"))
                    .range(a, b).measured(json!({"seconds": r3(b - a), "frames": r3((b - a) / fr)}))
                    .fix("close_gaps", json!({"track_ids": [last_track]})));
            }
            if *e > cursor { cursor = *e; last_track = tr.clone(); }
        }
    }
    // a media clip asking for more source than the file has: the last frame freezes and the sound stops
    for t in &p.tracks { for c in &t.clips {
        let Source::Media { asset } = &c.source else { continue };
        let Some(a) = p.assets.get(asset) else { continue };
        if a.duration <= 0.0 || c.time_remap.is_some() { continue; }
        let speed = if c.speed > 0.0 && c.speed.is_finite() { c.speed } else { 1.0 };
        let need = c.in_point + c.duration * speed;
        if need > a.duration + fr * speed * 1.5 {
            let fits = ((a.duration - c.in_point) / speed).max(fr);
            out.push(Finding::new("past_source_end", Severity::Error, "editing", format!("{} runs {:.2} s past the end of its file: the picture freezes and the sound stops for the last {:.2} s", c.id, (need - a.duration) / speed, (need - a.duration) / speed))
                .clip(&c.id).range(c.start + fits, c.start + c.duration).measured(json!({"needs_source_s": r3(need), "file_s": r3(a.duration)}))
                .fix("trim_clip", json!({"clip_id": c.id, "duration": r3(fits)})));
        }
    } }
    // jump cuts: the same shot cut to itself a moment later
    for t in &p.tracks {
        let mut cs: Vec<&crate::model::Clip> = t.clips.iter().map(|c| c.as_ref()).collect();
        cs.sort_by(|a, b| a.start.partial_cmp(&b.start).unwrap_or(std::cmp::Ordering::Equal));
        for w in cs.windows(2) {
            let (a, b) = (w[0], w[1]);
            let (Source::Media { asset: aa }, Source::Media { asset: ba }) = (&a.source, &b.source) else { continue };
            if aa != ba || !is_picture(p, a) || ((a.start + a.duration) - b.start).abs() > fr * 0.5 || b.transition_in.is_some() { continue; }
            let skipped = b.in_point - (a.in_point + a.duration * a.speed.max(0.01));
            if skipped > fr && skipped < 2.0 {
                out.push(Finding::new("jump_cut", Severity::Info, "editing", format!("{} cuts to {} {skipped:.2} s later in the same shot: a visible jump", a.id, b.id))
                    .clip(&b.id).at(b.start).measured(json!({"skipped_s": r3(skipped)}))
                    .fix("add_transition", json!({"clip_id": b.id, "kind": "dissolve", "duration": 0.2})));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::*;
    #[test]
    fn flash_clip_and_black_gap() {
        let mut ss = session();
        let t = track(&mut ss);
        solid(&mut ss, &t, 0.0, 2.0);
        let flash = solid(&mut ss, &t, 2.0, 2.0 / 30.0);
        solid(&mut ss, &t, 3.0, 2.0); // gap 2.067..3.0
        let f = check(&ss.project);
        assert!(f.iter().any(|x| x.code == "flash_clip" && x.clip_id.as_deref() == Some(flash.as_str())), "{:?}", codes(&f));
        let gap = f.iter().find(|x| x.code == "black_gap").expect("gap found");
        assert_eq!(gap.severity, Severity::Error);
        assert!((gap.range.unwrap()[0] - 2.067).abs() < 0.01 && (gap.range.unwrap()[1] - 3.0).abs() < 1e-6, "{:?}", gap.range);
    }
    #[test]
    fn clean_cut_has_no_findings() {
        let mut ss = session();
        let t = track(&mut ss);
        solid(&mut ss, &t, 0.0, 2.0);
        solid(&mut ss, &t, 2.0, 3.0);
        assert!(check(&ss.project).is_empty(), "{:?}", codes(&check(&ss.project)));
    }
    #[test]
    fn media_past_its_end_is_an_error_with_a_trim_fix() {
        let mut ss = session();
        ss.project.assets.insert("a1".into(), serde_json::from_value(json!({"id": "a1", "path": "x.mp4", "width": 1920, "height": 1080, "fps": 30.0, "duration": 4.0})).unwrap());
        let t = track(&mut ss);
        let id = call(&mut ss, "add_clip", json!({"track_id": t, "source_type": "media", "asset_id": "a1", "start": 0, "duration": 6}))["clip_id"].as_str().unwrap().to_string();
        let f = check(&ss.project);
        let pe = f.iter().find(|x| x.code == "past_source_end").expect("found");
        assert_eq!(pe.fix["args"]["clip_id"], id.as_str());
        assert!((pe.fix["args"]["duration"].as_f64().unwrap() - 4.0).abs() < 1e-6);
    }
}
