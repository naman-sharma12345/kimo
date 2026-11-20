//! Hybrid export: render only the parts of the edit that need kimo's renderer.
//!
//! The chunked export (export_job) cuts the timeline into fixed chunks. This module marks each frame of the timeline
//! as **clean** (exactly one plain media clip on screen: no effect, transform, text, grade, speed change, overlap,
//! still or HDR) or **dirty** (anything else, rounded outward to whole frames). A chunk whose frames are all clean is encoded by
//! ffmpeg directly from the source files (decode, the renderer's own fps and fit filters, the renderer's own x264
//! settings) instead of going frame by frame through the compositor. Consecutive clean chunks form a **run**, encoded
//! in ONE ffmpeg process that writes one file per chunk (segment muxer, a key frame forced at every chunk start), so
//! the runs' chunks and the rendered chunks join with the existing stream-copy join.
//!
//! A clip whose only setting is an incoming transition stays clean outside the transition: only its first
//! `duration` seconds are dirty. Safety nets live in export_job: a run that fails is rendered
//! the normal way, and if the join refuses the mix of chunks (different codec headers) the clean chunks are rendered
//! again and the join retried.
use serde_json::Value;

/// One clean clip: timeline frames [ks, ke), decoded from `in_point` exactly like the renderer's decode().
#[derive(Clone, Debug, PartialEq)]
pub struct PicClip { pub id: String, pub path: String, pub ks: u64, pub ke: u64, pub in_point: f64, pub pre: String, pub fps_filter: String, pub fit: String }

/// The timeline's clean clips (sorted by start) and its dirty frame ranges [a, b).
#[derive(Clone, Debug, Default)]
pub struct PicMap { pub clean: Vec<PicClip>, pub dirty: Vec<(u64, u64)>,
    /// Clean clips with no transition at all: drawn as plain pixels wherever they are, even under other layers
    /// (the renderer's base-layer shortcut, render::base_layer).
    pub simple: std::collections::HashSet<String> }

/// Project-wide settings that change every frame, so nothing can skip the renderer. None = no such setting.
pub fn global_reason(p: &crate::model::Project) -> Option<String> {
    let pj = serde_json::to_value(p).ok()?;
    if p.color_pipeline.enabled { return Some("colour management is on".into()); }
    if !super::export_fast::pristine(&pj["grade_scopes"]["timeline"]) || !super::export_fast::pristine(&pj["grade_scopes"]["clip_bindings"]) { return Some("a grade is applied".into()); }
    if !p.links.is_empty() || !p.expr_scripts.is_empty() || !p.hooks.is_empty() { return Some("the project has live links, expressions or hooks".into()); }
    if !p.occluders.is_empty() || p.comp3d.is_some() || p.ocio_view_delivery.is_some() { return Some("the project uses occluders, 3D compositing or an OCIO view".into()); }
    if p.linear_blend { return Some("linear-light blending is on (the renderer dithers every frame)".into()); }
    if p.tracks.iter().any(|t| t.solo) { return Some("a track is soloed".into()); }
    if p.fps <= 0.0 { return Some("the project has no frame rate".into()); }
    None
}

/// The clip properties a plain clip may carry (at their defaults, which clip_simple checks).
const PLAIN_PROPS: &[&str] = &["opacity", "reveal", "rotation", "scale", "x", "y", "fit"];

/// Mark every frame clean or dirty. `Err` = a project-wide reason, so every chunk renders.
pub fn picture_map(p: &crate::model::Project) -> Result<PicMap, String> {
    if let Some(r) = global_reason(p) { return Err(r); }
    let pj = serde_json::to_value(p).map_err(|e| e.to_string())?;
    let fps = p.fps;
    let on_grid = |t: f64| ((t * fps) - (t * fps).round()).abs() < 1e-3;
    let mut m = PicMap::default();
    // spans already round outward to whole frames, and a transition only reads other clips inside the incoming clip span
    let mut dirty = |a: u64, b: u64| m.dirty.push((a, b));
    let mut clean = vec![]; let mut m_simple = std::collections::HashSet::new();
    let hidden = crate::render::hidden_ids(p);
    for (ti, t) in pj["tracks"].as_array().into_iter().flatten().enumerate() {
        for (ci, c) in t["clips"].as_array().into_iter().flatten().enumerate() {
            let (start, dur) = (c["start"].as_f64().unwrap_or(0.0), c["duration"].as_f64().unwrap_or(0.0));
            if dur <= 0.0 { continue; }
            let (f0, f1) = (((start * fps) + 1e-6).floor().max(0.0) as u64, ((start + dur) * fps - 1e-6).ceil().max(0.0) as u64);
            if c["source"]["type"] != "media" || hidden.contains(c["id"].as_str().unwrap_or("")) { dirty(f0, f1); continue; }
            let asset = &pj["assets"][c["source"]["asset"].as_str().or(c["source"]["asset_id"].as_str()).unwrap_or("")];
            if asset["width"].as_u64().unwrap_or(0) == 0 { if asset.is_null() { dirty(f0, f1); } continue; } // sound only: no picture
            let mut bare = c.clone();
            let tr = c["transition_in"]["duration"].as_f64().filter(|_| !c["transition_in"].is_null());
            if let Some(o) = bare.as_object_mut() { o.remove("transition_in"); }
            let typed = p.tracks.get(ti).and_then(|t| t.clips.get(ci));
            let path = p.resolve(asset["path"].as_str().unwrap_or(""));
            let ok = super::export_fast::clip_simple(&bare).filter(|_| typed.is_some_and(|t| !crate::reframe::active(t) && t.props.keys().all(|k| PLAIN_PROPS.contains(&k.as_str()))))
                .filter(|_| on_grid(start) && on_grid(start + dur) && !crate::render::is_still_path(&path))
                .filter(|_| c["in_point"].as_f64().unwrap_or(0.0) + dur <= asset["duration"].as_f64().unwrap_or(0.0) + 0.5 / fps)
                .filter(|_| !(p.hdr_tonemap && crate::render::hdr_source(&path).is_some()));
            let (Some(sm), Some(typed)) = (ok, typed) else { dirty(f0, f1); continue };
            let (ks, ke) = ((start * fps).round() as u64, ((start + dur) * fps).round() as u64);
            if let Some(d) = tr { dirty(ks, (ks + (d * fps).ceil() as u64).min(ke)); } else { m_simple.insert(c["id"].as_str().unwrap_or("").to_string()); }
            let pre = if p.color_tag_sanity && crate::render::mistagged_hd_source(&path) { "setparams=colorspace=bt709:color_primaries=bt709:color_trc=bt709,".to_string() } else { String::new() };
            clean.push(PicClip { id: c["id"].as_str().unwrap_or("").to_string(), fps_filter: crate::render::conform_fps_filter(p, typed, &path, 1.0, false, None), fit: crate::render::clip_fit_filter(p.width, p.height, sm.fit, false),
                path, ks, ke, in_point: c["in_point"].as_f64().unwrap_or(0.0), pre });
        }
    }
    clean.sort_by_key(|c| c.ks);
    // two clips on screen at once composite: the overlap is dirty
    for i in 0..clean.len() { for j in i + 1..clean.len() { if clean[j].ks >= clean[i].ke { break; } dirty(clean[j].ks, clean[i].ke.min(clean[j].ke)); } }
    m.clean = clean; m.simple = m_simple;
    m.dirty.sort();
    Ok(m)
}

/// The clean clips that tile frames [f0, f1): (clip index, first clip frame, frame count). None when any frame in the
/// range is dirty or has no picture (a gap).
pub fn segments(m: &PicMap, f0: u64, f1: u64) -> Option<Vec<(usize, u64, u64)>> {
    if f1 <= f0 || m.dirty.iter().any(|&(a, b)| a < f1 && b > f0) { return None; }
    let mut out = vec![]; let mut at = f0;
    while at < f1 {
        let i = m.clean.iter().position(|c| c.ks <= at && at < c.ke)?;
        let c = &m.clean[i]; let end = c.ke.min(f1);
        out.push((i, at - c.ks, end - at)); at = end;
    }
    Some(out)
}

/// The ffmpeg argv for one run of consecutive clean chunks covering frames [f0, f1), chunks of `cf` frames numbered
/// from `first_idx`, written to `dir/chunk_%05d.partial.mp4`. `enc`/`vf`: render::chunk_encoder_args.
pub fn run_argv(m: &PicMap, f0: u64, f1: u64, cf: u64, first_idx: u64, fps: f64, enc: &[String], vf: &str, dir: &std::path::Path) -> Option<Vec<String>> {
    let segs = segments(m, f0, f1)?;
    let mut a: Vec<String> = ["-v", "error", "-y", "-nostdin"].map(String::from).to_vec();
    let mut g = vec![];
    for (k, (ci, skip, n)) in segs.iter().enumerate() {
        let c = &m.clean[*ci];
        // the renderer's decode(): seek to the clip's in-point, conform, then count frames from there
        a.extend(["-ss".into(), format!("{}", c.in_point), "-t".into(), format!("{:.6}", (skip + n + 2) as f64 / fps), "-i".into(), c.path.clone()]);
        g.push(format!("[{k}:v]{}setpts=PTS/1,{},tpad=stop_mode=clone:stop=-1,trim=start_frame={skip}:end_frame={},setpts=PTS-STARTPTS,{},format=rgba[v{k}]", c.pre, c.fps_filter, skip + n, c.fit));
    }
    let labels: String = (0..segs.len()).map(|k| format!("[v{k}]")).collect();
    // setsar=0: the rendered chunks come from raw frames with no aspect ratio, so x264 writes none; a 1:1 here would change
    // the codec headers (SPS) and the join could not stream-copy the two kinds of chunk together
    g.push(format!("{labels}concat=n={}:v=1:a=0,{vf},setsar=0[vout]", segs.len()));
    a.extend(["-filter_complex".into(), g.join(";"), "-map".into(), "[vout]".into()]);
    a.extend(enc.iter().cloned());
    let cuts: Vec<String> = (1..).map(|i| i * cf).take_while(|&x| x < f1 - f0).map(|x| x.to_string()).collect();
    a.extend(["-force_key_frames".into(), format!("expr:eq(mod(n,{cf}),0)"), "-forced-idr".into(), "1".into(), "-frames:v".into(), (f1 - f0).to_string(), "-an".into()]);
    a.extend(["-f".into(), "segment".into(), "-segment_format".into(), "mp4".into(), "-reset_timestamps".into(), "1".into(), "-segment_start_number".into(), first_idx.to_string()]);
    if !cuts.is_empty() { a.extend(["-segment_frames".into(), cuts.join(",")]); } else { a.extend(["-segment_time".into(), "999999".into()]); }
    a.push(dir.join("chunk_%05d.partial.mp4").to_string_lossy().into_owned());
    Some(a)
}

/// A run: consecutive chunk indexes [first, last] covering frames [f0, f1).
#[derive(Clone, Debug, PartialEq)]
pub struct Run { pub first: u64, pub last: u64, pub f0: u64, pub f1: u64 }

/// Group the chunks to (re)make into runs of consecutive clean chunks. `chunks`: (index, [f0, f1), needs making).
pub fn runs(m: &PicMap, chunks: &[(u64, [u64; 2], bool)]) -> Vec<Run> {
    let mut out: Vec<Run> = vec![];
    for &(idx, r, todo) in chunks {
        if !todo || segments(m, r[0], r[1]).is_none() { continue; }
        match out.last_mut() { Some(run) if run.last + 1 == idx && run.f1 == r[0] => { run.last = idx; run.f1 = r[1]; } _ => out.push(Run { first: idx, last: idx, f0: r[0], f1: r[1] }) }
    }
    out
}

pub fn summary(m: &Result<PicMap, String>, clean_chunks: usize, total_chunks: usize) -> Value {
    match m {
        Ok(_) => serde_json::json!({"clean_chunks": clean_chunks, "rendered_chunks": total_chunks - clean_chunks}),
        Err(why) => serde_json::json!({"clean_chunks": 0, "why_not": why}),
    }
}

/// No tools of its own: export_job uses it.
pub fn commands() -> Vec<crate::commands::Command> { vec![] }

#[cfg(test)]
mod tests {
    use super::*;
    fn clip(path: &str, ks: u64, ke: u64, inp: f64) -> PicClip { PicClip { id: path.into(), path: path.into(), ks, ke, in_point: inp, pre: String::new(), fps_filter: "fps=25".into(), fit: "scale=160:90".into() } }
    fn map(clean: Vec<PicClip>, dirty: Vec<(u64, u64)>) -> PicMap { PicMap { clean, dirty, simple: Default::default() } }

    #[test]
    fn segments_tile_clean_ranges_and_refuse_dirty_or_empty_ones() {
        let m = map(vec![clip("a.mp4", 0, 100, 0.0), clip("b.mp4", 100, 250, 2.0)], vec![(140, 160)]);
        assert_eq!(segments(&m, 0, 100), Some(vec![(0, 0, 100)]));
        assert_eq!(segments(&m, 50, 130), Some(vec![(0, 50, 50), (1, 0, 30)]), "a chunk across a cut uses both clips");
        assert_eq!(segments(&m, 100, 150), None, "a dirty frame anywhere renders the chunk");
        assert_eq!(segments(&m, 200, 250), Some(vec![(1, 100, 50)]));
        assert_eq!(segments(&m, 240, 300), None, "past the last clip there is no picture (black needs the renderer)");
    }
    #[test]
    fn runs_join_consecutive_clean_chunks_only() {
        let m = map(vec![clip("a.mp4", 0, 500, 0.0)], vec![(150, 170)]);
        let ch: Vec<(u64, [u64; 2], bool)> = (0..5).map(|i| (i, [i * 100, (i + 1) * 100], i != 3)).collect();
        // chunk 1 is dirty (frames 150-170), chunk 3 is already reused
        assert_eq!(runs(&m, &ch), vec![Run { first: 0, last: 0, f0: 0, f1: 100 }, Run { first: 2, last: 2, f0: 200, f1: 300 }, Run { first: 4, last: 4, f0: 400, f1: 500 }]);
        let all: Vec<(u64, [u64; 2], bool)> = (0..5).map(|i| (i, [i * 100, (i + 1) * 100], true)).collect();
        assert_eq!(runs(&map(vec![clip("a.mp4", 0, 500, 0.0)], vec![]), &all), vec![Run { first: 0, last: 4, f0: 0, f1: 500 }], "one ffmpeg for the lot");
    }
    #[test]
    fn a_run_is_one_ffmpeg_writing_one_file_per_chunk() {
        let m = map(vec![clip("a.mp4", 0, 120, 1.5), clip("b.mp4", 120, 300, 0.0)], vec![]);
        let enc: Vec<String> = ["-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18"].map(String::from).to_vec();
        let a = run_argv(&m, 100, 300, 100, 1, 25.0, &enc, "scale=out_color_matrix=bt709:out_range=tv", std::path::Path::new("d")).unwrap();
        let s = a.join(" ");
        assert!(s.contains("-ss 1.5 -t 4.880000 -i a.mp4"), "seeks to the in-point and decodes up to the chunk, like the renderer: {s}");
        assert!(s.contains("trim=start_frame=100:end_frame=120"), "{s}");
        assert!(s.contains("[1:v]setpts=PTS/1,fps=25,tpad=stop_mode=clone:stop=-1,trim=start_frame=0:end_frame=180"), "{s}");
        assert!(s.contains("concat=n=2:v=1:a=0,scale=out_color_matrix=bt709:out_range=tv,setsar=0[vout]"), "the renderer encoder's own colour conversion: {s}");
        assert!(s.contains("-c:v libx264 -pix_fmt yuv420p -crf 18"), "the rendered chunks' encoder settings: {s}");
        assert!(s.contains("-force_key_frames expr:eq(mod(n,100),0)") && s.contains("-segment_frames 100 ") && s.contains("-segment_start_number 1 "), "{s}");
        assert!(s.contains("-frames:v 200 -an"), "{s}");
        assert!(a.last().unwrap().ends_with("chunk_%05d.partial.mp4"));
        assert!(run_argv(&map(vec![], vec![]), 0, 10, 10, 0, 25.0, &enc, "x", std::path::Path::new("d")).is_none());
    }
}
