//! Audio critic: what the mix sounds like. Integrated loudness off the delivery target, true peak over -1 dBTP,
//! clips whose own audio clips (the source flat-topped, or a gain that drives it past full scale into the master
//! limiter), and music masking speech (the music bed within 10 dB of the dialogue while someone talks).
//!
//! Measured on PCM: the delivered mix (`audio::mix` when the master speech leveler is on, so the numbers match the
//! export; otherwise `preview_stream::mix_window` over just the span) and, for masking, the dialogue-only and
//! music-only mixes of the same span. Roles follow finish_audio: audio-only clips (or a track named "music") are
//! music unless the speech detector hears speech in them; a clip the detector hears speech in is dialogue.
//! Per edit, only the edited span is measured, and results are cached by (project hash, span, target).
use super::{r3, Finding, Severity};
use crate::model::{Clip, Project, Source};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{Arc, LazyLock, Mutex};

pub const DEFAULT_TARGET_LUFS: f64 = -14.0;
/// Integrated loudness within this many LU of the target is on target.
pub const LOUDNESS_TOL_LU: f64 = 1.0;
pub const MAX_TRUE_PEAK_DBTP: f64 = -1.0;
/// Music must sit at least this far under the dialogue while someone speaks.
pub const MIN_SPEECH_OVER_MUSIC_DB: f64 = 10.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role { Dialogue, Music, Other }

#[derive(Clone, Debug)]
struct ClipSound { speech_ratio: f64, peak: f64, frames: usize, audio_only: bool, pcm_peak_frames: usize }

/// One clip's own audio, decoded as the mix decodes it (effects included, before volume), cached by its content.
fn clip_sound(p: &Project, c: &Clip) -> Option<Arc<ClipSound>> {
    let Source::Media { asset } = &c.source else { return None };
    let a = p.assets.get(asset)?;
    let path = p.resolve(&a.path);
    let mut probe = c.clone(); probe.audio.volume = crate::keyframe::Prop::new(1.0); probe.audio.duck_gain = crate::keyframe::Prop::new(1.0); probe.start = 0.0;
    let key = hash(&(path.clone(), serde_json::to_string(&probe).unwrap_or_default()));
    if let Some(hit) = SOUNDS.lock().unwrap_or_else(|e| e.into_inner()).get(&key) { return hit.clone(); }
    let (v, _) = crate::audio::audio_view(c);
    let pcm = crate::audio::decode_clip(&path, &v);
    let out = if pcm.len() < 2 { None } else {
        let peak = pcm.iter().fold(0f32, |m, x| m.max(x.abs())) as f64;
        let speech_ratio = crate::perception::vad::analyze(&pcm, &Default::default()).map(|r| r.speech_ratio).unwrap_or(0.0);
        let (_, at_full) = crate::editorial::count_clipped(&pcm, 0.999);
        Some(Arc::new(ClipSound { speech_ratio, peak, frames: pcm.len() / 2, audio_only: a.width == 0 || a.kind.as_deref() == Some("audio"), pcm_peak_frames: at_full }))
    };
    let mut m = SOUNDS.lock().unwrap_or_else(|e| e.into_inner());
    if m.len() >= 512 { if let Some(k) = m.keys().next().cloned() { m.remove(&k); } }
    m.insert(key, out.clone());
    out
}
static SOUNDS: LazyLock<Mutex<BTreeMap<u64, Option<Arc<ClipSound>>>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));
static SPANS: LazyLock<Mutex<BTreeMap<(u64, i64, i64, i64), Vec<Finding>>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

fn hash<T: std::hash::Hash>(v: &T) -> u64 { use std::hash::Hasher; let mut h = std::collections::hash_map::DefaultHasher::new(); v.hash(&mut h); h.finish() }

/// Clips whose sound reaches the mix: media clips on audible tracks, not muted.
fn audible(p: &Project) -> Vec<(String, Arc<Clip>)> {
    p.tracks.iter().filter(|t| !crate::model::effective_mute(&p.tracks, t)).flat_map(|t| t.clips.iter().filter(|c| matches!(c.source, Source::Media { .. }) && !c.audio.mute).map(move |c| (t.id.clone(), c.clone()))).collect()
}

pub fn role(p: &Project, track_id: &str, c: &Clip) -> Role {
    let named_music = p.tracks.iter().find(|t| t.id == track_id).map(|t| { let n = t.name.to_lowercase(); n.contains("music") || n.contains("song") || n.contains("bed") }).unwrap_or(false);
    let Some(s) = clip_sound(p, c) else { return Role::Other };
    if named_music { return Role::Music; }
    // speech detection fires now and then on engine, traffic and crowd noise; a clip has to be mostly speech to count as dialogue
    if s.speech_ratio >= 0.35 { return Role::Dialogue; }
    if s.audio_only { Role::Music } else { Role::Other }
}

/// The timeline mix of [a, b) with only the clips `keep` accepts, interleaved stereo at `audio::SR`.
fn mix_of(p: &Project, a: f64, b: f64, keep: &dyn Fn(&str, &Clip) -> bool) -> Vec<f32> {
    let mut q = p.clone();
    for t in q.tracks.iter_mut() { let tid = t.id.clone(); t.clips.retain(|c| keep(&tid, c)); }
    if q.master_speech_level > 0.0 {
        // the leveler runs over the whole timeline at export: measure what the export hears
        let all = crate::audio::mix(&q);
        let sr = crate::audio::SR as f64;
        let (i, j) = (((a * sr) as usize * 2).min(all.len()), ((b * sr) as usize * 2).min(all.len()));
        all[i..j].to_vec()
    } else { crate::preview_stream::mix_window(&q, a, b) }
}

fn db(e: f64) -> f64 { if e > 0.0 { 10.0 * e.log10() } else { f64::NEG_INFINITY } }

/// True peak (dBTP) of the mix, reconstructing only around samples within 3 dB of full scale (an intersample over
/// needs a hot sample next to it), so a long quiet mix costs one pass.
fn true_peak(pcm: &[f32]) -> Option<f64> {
    let hot = 10f32.powf(-3.0 / 20.0);
    let frames = pcm.len() / 2;
    let mut segs: Vec<(usize, usize)> = vec![];
    for i in 0..frames { if pcm[i * 2].abs() >= hot || pcm[i * 2 + 1].abs() >= hot {
        let (s, e) = (i.saturating_sub(64), (i + 65).min(frames));
        match segs.last_mut() { Some(l) if s <= l.1 => l.1 = e, _ => segs.push((s, e)) }
    } }
    let mut peak: Option<f64> = None; let mut budget = crate::audio::SR * 4;
    for (s, e) in segs { if budget == 0 { break; } let e = e.min(s + budget); budget -= e - s;
        if let Ok(Some(tp)) = crate::editorial::true_peak_4x(&pcm[s * 2..e * 2]) { peak = Some(peak.map_or(tp, |p: f64| p.max(tp))); } }
    peak
}

#[derive(Clone, Copy, Debug)]
pub struct Opts { pub target_lufs: f64 }
impl Default for Opts { fn default() -> Self { Opts { target_lufs: DEFAULT_TARGET_LUFS } } }

/// Every audio finding for [a, b] of the timeline. Cached by (project, span, target).
pub fn check_span(p: &Project, a: f64, b: f64, o: Opts) -> Vec<Finding> {
    let end = crate::ext::export_manifest::project_end(p);
    let (a, b) = (a.max(0.0), b.min(end));
    if b - a < 0.05 { return vec![]; }
    let key = (hash(&serde_json::to_string(p).unwrap_or_default()), (a * 1000.0).round() as i64, (b * 1000.0).round() as i64, (o.target_lufs * 100.0).round() as i64);
    if let Some(hit) = SPANS.lock().unwrap_or_else(|e| e.into_inner()).get(&key) { return hit.clone(); }
    let out = measure(p, a, b, o);
    let mut m = SPANS.lock().unwrap_or_else(|e| e.into_inner());
    if m.len() >= 256 { if let Some(k) = m.keys().next().cloned() { m.remove(&k); } }
    m.insert(key, out.clone());
    out
}

fn measure(p: &Project, a: f64, b: f64, o: Opts) -> Vec<Finding> {
    let mut out = vec![];
    let clips: Vec<(String, Arc<Clip>)> = audible(p).into_iter().filter(|(_, c)| c.start < b && c.start + c.duration > a).collect();
    if clips.is_empty() { return out; }
    let mut roles: BTreeMap<String, (Role, String)> = BTreeMap::new();
    // 1. clips that clip: the source flat-topped at full scale, or the clip's own gain pushing it past full scale
    for (tid, c) in &clips {
        let Some(s) = clip_sound(p, c) else { continue };
        roles.insert(c.id.clone(), (role(p, tid, c), tid.clone()));
        let gain = c.audio.volume.keys.iter().map(|k| k.v).fold(c.audio.volume.base, f64::max).max(0.0);
        let driven = s.peak * gain;
        let source_clipped = s.pcm_peak_frames >= 4 && s.pcm_peak_frames as f64 >= s.frames as f64 * 1e-4;
        // a clip whose own chain ends in a limiter (or a compressor) does not reach the output at volume x source peak:
        // the chain holds it down (finish_audio's dialogue chain ends in a -1.5 dB limiter). The mix's true-peak check
        // below still catches a real over; only a source that is clipped in the file itself is flagged per clip then.
        let chained = c.audio.effects.iter().any(|e| matches!(e, crate::editorial::AudioEffect::Limiter { .. } | crate::editorial::AudioEffect::Compressor { .. }));
        let driven = if chained { 0.0 } else { driven };
        if source_clipped || driven >= 1.0 {
            let target_gain = r3((0.708 / s.peak.max(1e-6)).min(gain.max(0.01)) * if source_clipped && driven < 1.0 { 0.85 } else { 1.0 });
            let msg = if source_clipped { format!("{} is clipped in its source ({} sample frames flat at full scale): it will sound distorted; lower it to give the limiter headroom, and replace the take if there is a clean one", c.id, s.pcm_peak_frames) }
                else { format!("{}'s volume {:.2} drives its peaks to {:+.1} dBFS: the master limiter squashes them and it sounds crushed", c.id, gain, 20.0 * driven.log10()) };
            out.push(Finding::new("audio_clipping", Severity::Warning, "audio", msg).clip(&c.id).range(c.start, c.start + c.duration)
                .measured(json!({"source_peak_dbfs": r3(20.0 * s.peak.max(1e-9).log10()), "gain": r3(gain), "clipped_frames": s.pcm_peak_frames}))
                .threshold(json!({"max_peak_dbfs": 0.0}))
                .fix("set_audio", json!({"clip_id": c.id, "volume": target_gain})));
        }
    }
    // 2. the mix: integrated loudness and true peak
    let mix = mix_of(p, a, b, &|_, _| true);
    let whole = a <= 1e-6 && b >= crate::ext::export_manifest::project_end(p) - 1e-3;
    if b - a >= 3.0 {
        if let Some(l) = crate::editorial::loudness(&mix) {
            let off = l - o.target_lufs;
            if off.abs() > LOUDNESS_TOL_LU {
                let what = if whole { "the mix" } else { "this part of the mix" };
                out.push(Finding::new("mix_loudness_off_target", Severity::Warning, "audio", format!("{what} measures {:.1} LUFS integrated, {:.1} LU {} the {:.0} LUFS target: {}", l, off.abs(), if off > 0.0 { "over" } else { "under" }, o.target_lufs, if off > 0.0 { "platforms will turn it down and it sounds squashed" } else { "it will play quieter than everything around it" }))
                    .range(a, b).measured(json!({"integrated_lufs": r3(l)})).threshold(json!({"target_lufs": o.target_lufs, "tolerance_lu": LOUDNESS_TOL_LU}))
                    .fix("finish_audio", json!({"target_lufs": o.target_lufs})));
            }
        }
    }
    if let Some(tp) = true_peak(&mix) {
        if tp > MAX_TRUE_PEAK_DBTP {
            out.push(Finding::new("true_peak_over", Severity::Warning, "audio", format!("the mix peaks at {tp:+.1} dBTP, over the {MAX_TRUE_PEAK_DBTP} dBTP ceiling: it will distort after the platform's encoder"))
                .range(a, b).measured(json!({"true_peak_dbtp": r3(tp)})).threshold(json!({"max_dbtp": MAX_TRUE_PEAK_DBTP}))
                .fix("finish_audio", json!({"target_lufs": r3((o.target_lufs - (tp - MAX_TRUE_PEAK_DBTP)).max(-30.0))})));
        }
    }
    // 3. music masking speech
    let is = |r: Role| { let roles = roles.clone(); move |_: &str, c: &Clip| roles.get(&c.id).map(|x| x.0 == r).unwrap_or(false) };
    let (dia_tracks, mus_tracks): (Vec<String>, Vec<String>) = {
        let mut d: Vec<String> = roles.values().filter(|x| x.0 == Role::Dialogue).map(|x| x.1.clone()).collect(); d.sort(); d.dedup();
        let mut m: Vec<String> = roles.values().filter(|x| x.0 == Role::Music).map(|x| x.1.clone()).collect(); m.sort(); m.dedup();
        (d, m)
    };
    if !dia_tracks.is_empty() && !mus_tracks.is_empty() {
        let dia = mix_of(p, a, b, &is(Role::Dialogue));
        let mus = mix_of(p, a, b, &is(Role::Music));
        let speech = crate::perception::vad::analyze(&dia, &crate::perception::vad::Params { pad_ms: 0.0, ..Default::default() }).map(|r| r.speech).unwrap_or_default();
        let sr = crate::audio::SR as f64;
        let (mut ed, mut em, mut worst): (f64, f64, Option<(f64, f64, f64)>) = (0.0, 0.0, None);
        let (mut first, mut last) = (f64::MAX, f64::MIN);
        for s in speech.iter().filter(|s| s.end - s.start >= 0.3) {
            let (i, j) = (((s.start * sr) as usize * 2).min(dia.len()), ((s.end * sr) as usize * 2).min(dia.len()).min(mus.len()));
            if j <= i + 2 { continue; }
            let (Some(kd), Some(km)) = (crate::editorial::k_energy(&dia[i..j]), crate::editorial::k_energy(&mus[i..j])) else { continue };
            let (d, m) = (kd.iter().sum::<f64>(), km.iter().sum::<f64>());
            ed += d; em += m;
            let gap = db(d) - db(m);
            if gap < MIN_SPEECH_OVER_MUSIC_DB { first = first.min(a + s.start); last = last.max(a + s.end); }
            if worst.map(|w| gap < w.2).unwrap_or(true) { worst = Some((a + s.start, a + s.end, gap)); }
        }
        let gap = db(ed) - db(em);
        if ed > 0.0 && gap.is_finite() && gap < MIN_SPEECH_OVER_MUSIC_DB {
            let (fix_tool, fix_args) = if dia_tracks.iter().any(|d| mus_tracks.contains(d)) {
                ("finish_audio", json!({"target_lufs": o.target_lufs, "music_clip_ids": roles.iter().filter(|x| x.1 .0 == Role::Music).map(|x| x.0.clone()).collect::<Vec<_>>()}))
            } else {
                ("duck_timeline", json!({"dialogue_tracks": dia_tracks, "music_tracks": mus_tracks, "amount_db": r3((-(MIN_SPEECH_OVER_MUSIC_DB + 4.0 - gap)).clamp(-40.0, -6.0))}))
            };
            let w = worst.unwrap_or((a, b, gap));
            out.push(Finding::new("music_masks_speech", Severity::Warning, "audio", format!("while someone speaks the music sits only {gap:.1} dB under the dialogue (needs {MIN_SPEECH_OVER_MUSIC_DB:.0} dB or more): the words get lost; worst at {:.2}-{:.2} s ({:.1} dB)", w.0, w.1, w.2))
                .range(if first < last { first } else { a }, if first < last { last } else { b })
                .measured(json!({"speech_over_music_db": r3(gap), "worst": {"range": [r3(w.0), r3(w.1)], "db": r3(w.2)}, "speech_windows": speech.len()}))
                .threshold(json!({"min_speech_over_music_db": MIN_SPEECH_OVER_MUSIC_DB}))
                .fix(fix_tool, fix_args));
        }
    }
    out
}

/// The audio findings for an edit: only when it touched a clip with sound, measured over the spans it touched.
pub fn check_edit(p: &Project, spans: &[[f64; 2]], touched: &[String]) -> Vec<Finding> {
    let sound = touched.iter().any(|id| p.find_clip(id).map(|c| matches!(c.source, Source::Media { .. })).unwrap_or(true));
    if !sound { return vec![]; }
    // an edit hears at most a minute of what it touched; review measures the whole mix
    spans.iter().flat_map(|s| check_span(p, s[0], s[1].min(s[0] + 60.0), Opts::default())).collect()
}

/// Delivery loudness by platform (LUFS integrated).
pub fn platform_target(name: &str) -> Option<f64> {
    match name.to_lowercase().as_str() {
        "youtube" | "tiktok" | "reels" | "instagram" | "shorts" | "social" | "spotify" => Some(-14.0),
        "podcast" | "apple_podcasts" | "apple" => Some(-16.0),
        "broadcast" | "ebu" | "r128" | "tv" => Some(-23.0),
        "atsc" | "a85" => Some(-24.0),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn platform_targets() {
        assert_eq!(platform_target("YouTube"), Some(-14.0));
        assert_eq!(platform_target("podcast"), Some(-16.0));
        assert_eq!(platform_target("broadcast"), Some(-23.0));
        assert_eq!(platform_target("nope"), None);
    }
    #[test]
    fn true_peak_skips_quiet_mixes_and_reads_hot_ones() {
        let quiet: Vec<f32> = (0..96000).map(|i| 0.1 * ((i / 2) as f32 * 0.13).sin()).collect();
        assert_eq!(true_peak(&quiet), None);
        let hot: Vec<f32> = (0..96000).map(|i| 0.99 * ((i / 2) as f32 * 0.13).sin()).collect();
        assert!(true_peak(&hot).unwrap() > -1.0);
    }
}
