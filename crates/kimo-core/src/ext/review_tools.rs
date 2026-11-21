//! `review`: the whole edit judged at once. Every timeline critic (editing, transitions, text, motion), sampled
//! frames through the picture critic (black frames, clipping, crush), every shot through the colour critic (scene
//! match, casts, skin) and the mix through the audio critic (loudness, true peak, clipping, music over speech), as
//! findings sorted most severe first, each with the call that fixes it. Run it before export; the per-edit `observe` result covers only what one edit touched.
use super::kit::*;
use crate::critic::{self, picture, Severity};

fn review(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let t0 = std::time::Instant::now();
    let end = crate::ext::export_manifest::project_end(&ss.project);
    if end <= 0.0 { return Err("the timeline is empty".into()); }
    let (from, to) = match a.get("range").and_then(|v| v.as_array()) { Some(r) if r.len() == 2 => (r[0].as_f64().unwrap_or(0.0).max(0.0), r[1].as_f64().unwrap_or(end).min(end)), _ => (0.0, end) };
    let domains: Option<Vec<String>> = a.get("domains").and_then(|v| v.as_array()).map(|d| d.iter().filter_map(|x| x.as_str().map(String::from)).collect());
    let min_sev = a.get("severity").and_then(|v| v.as_str()).and_then(Severity::parse).unwrap_or(Severity::Info);
    let budget_ms = opt_num(a, "budget_s", 60.0).clamp(0.0, 1800.0) * 1000.0;
    let frames = opt_num(a, "frames", 40.0).clamp(0.0, 400.0) as usize;
    let mut f = critic::timeline_in(&ss.project, &[[from, to]]);
    // sampled pictures
    let fr = critic::frame(&ss.project);
    let mut sampled = 0usize; let mut planned = 0usize;
    if frames > 0 && domains.as_ref().map(|d| d.iter().any(|x| x == "picture" || x == "color")).unwrap_or(true) {
        let step = ((to - from) / frames as f64).max(fr);
        let mut t = from + step / 2.0;
        while t < to {
            planned += 1;
            if (t0.elapsed().as_millis() as f64) < budget_ms {
                let r = { let Session { project, cache, .. } = &mut *ss; crate::observe::draft(project, cache, t, 320) };
                if let Ok((rgba, _, _)) = r { f.extend(picture::check_frame(t, &picture::stats(&rgba), None)); sampled += 1; }
            }
            t += step;
        }
    }
    let wants = |d: &str| domains.as_ref().map(|x| x.iter().any(|y| y == d)).unwrap_or(true);
    let mut shots = json!(null);
    if wants("color") && frames > 0 {
        let deadline = t0 + std::time::Duration::from_millis(budget_ms as u64);
        let (cf, done, planned) = critic::color::check(ss, from, to, None, deadline, a.get("faces").and_then(|v| v.as_bool()).unwrap_or(true));
        f.extend(cf);
        shots = json!({"measured": done, "planned": planned});
    }
    let mut loudness_target = critic::audio::DEFAULT_TARGET_LUFS;
    if let Some(pl) = a.get("platform").and_then(|v| v.as_str()) { loudness_target = critic::audio::platform_target(pl).ok_or_else(|| format!("unknown platform '{pl}': youtube|tiktok|reels|shorts|podcast|broadcast"))?; }
    if let Some(v) = a.get("loudness_target").and_then(|v| v.as_f64()) { loudness_target = v.clamp(-40.0, -5.0); }
    if wants("audio") { f.extend(critic::audio::check_span(&ss.project, from, to, critic::audio::Opts { target_lufs: loudness_target })); }
    let mut f: Vec<_> = f.into_iter().filter(|x| x.severity >= min_sev && domains.as_ref().map(|d| d.iter().any(|y| y == x.domain)).unwrap_or(true)).collect();
    // one black-frame finding per run of black samples is enough
    let mut last_black = f64::MIN;
    f.retain(|x| { if x.code != "black_frame" { return true; } let t = x.at.unwrap_or(0.0); let keep = t - last_black > 2.0; last_black = t; keep });
    critic::sort(&mut f);
    Ok(json!({"ok": f.iter().all(|x| x.severity < Severity::Warning), "counts": critic::counts(&f), "findings": f,
        "covered": {"range": [critic::r3(from), critic::r3(to)], "frames_sampled": sampled, "frames_planned": planned, "shots": shots, "loudness_target_lufs": loudness_target, "complete": sampled == planned && shots.get("measured") == shots.get("planned")},
        "ms": t0.elapsed().as_millis() as u64,
        "next": "fix errors first (each finding's fix is a ready call), then warnings; edits report their own findings in their observe result"}))
}

pub fn commands() -> Vec<Command> { vec![
    Command { name: "review", description: "Judge the whole edit (or a range) for mistakes in picture, colour, sound, cuts, transitions, text and motion, each with its fix. It checks editing (flash clips, black gaps, clips past their source, jump cuts), transitions (dips to black, too short, too long for the shots or the pace, moving without motion blur), text (too fast to read, outside title safe, missing glyphs), motion (jitter, pops, linear moves, off-screen mid-move), sampled frames (black, clipped highlights, crushed shadows), colour (shots that don't match their scene, strong casts, skin off the skin line; split scenes with markers named \"Scene ...\") and audio (mix loudness off target, true peak over -1 dBTP, clipping, music masking speech). Findings are sorted most severe first, each with the exact call that fixes it. Run before export. Every edit also returns its own `observe` result (what it changed, before/after frames, findings for what it touched); pass observe: off|numbers|fast|full and expect: [...] on any edit (a failed expect rolls the edit back).", mutates: false,
        schema: obj(vec![("range", json!({"type":"array","items":{"type":"number"},"description":"[start, end] seconds (default the whole timeline)"})),
            ("domains", json!({"type":"array","items":{"type":"string"},"description":"limit to editing|transitions|text|motion|picture|color|audio"})),
            ("platform", s("delivery loudness target: youtube|tiktok|reels|shorts (-14 LUFS, default), podcast (-16), broadcast (-23)")),
            ("loudness_target", n("integrated LUFS target for the mix (overrides platform)")),
            ("faces", json!({"type":"boolean","description":"run the face detector for the skin-tone check (default true)"})),
            ("severity", s("lowest severity to report: info (default) | warning | error")),
            ("frames", n("frames to sample for the picture checks, default 40 (0 = no picture or colour checks)")),
            ("budget_s", n("time budget for the frame samples, seconds, default 60"))], &[]),
        run: review },
] }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::critic::testkit::*;
    #[test]
    fn review_reports_seeded_mistakes_sorted() {
        let mut ss = session();
        let t = track(&mut ss);
        solid(&mut ss, &t, 0.0, 2.0);
        solid(&mut ss, &t, 3.0, 2.0); // black gap 2..3
        let tt = track(&mut ss);
        text(&mut ss, &tt, "Far too much text to read in a third of a second", 3.5, 0.3);
        let r = review(&mut ss, &json!({"frames": 0})).unwrap();
        let codes: Vec<&str> = r["findings"].as_array().unwrap().iter().map(|f| f["code"].as_str().unwrap()).collect();
        assert_eq!(codes[0], "black_gap", "errors first: {codes:?}");
        assert!(codes.contains(&"text_too_fast"), "{codes:?}");
        assert_eq!(r["ok"], false);
        let only_text = review(&mut ss, &json!({"frames": 0, "domains": ["text"]})).unwrap();
        assert!(only_text["findings"].as_array().unwrap().iter().all(|f| f["domain"] == "text"));
    }
}
