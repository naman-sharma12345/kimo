//! Colour critic: shots that don't match their scene, strong colour casts, and skin pulled off the skin line.
//!
//! Each shot (a media clip with picture) is read from one draft frame at its midpoint, the same measure
//! check_color_consistency takes through get_scopes (mean luma and mean RGB), plus its mean CIELAB colour so the
//! match is judged perceptually (CIEDE2000). Scenes are split by markers named "scene..." (no such markers: the
//! whole timeline is one scene). A shot is off when it differs from its scene by a visible amount (dE00) AND past
//! check_color_consistency's luma or RGB tolerance. A cast is a frame whose pixels all lean the same way in a*/b*
//! (a colourful scene leans every way and is not a cast). Skin is read inside `vision::detect_faces` boxes on the
//! vectorscope: skin of every complexion sits near the I-line (about 123 degrees).
use super::{r3, Finding, Severity};
use crate::commands::Session;
use crate::model::{Clip, Project, Source};
use serde_json::json;
use std::collections::BTreeMap;
use std::sync::{LazyLock, Mutex};

pub const LUMA_TOL: f64 = 0.08;
pub const RGB_TOL: f64 = 0.06;
/// CIEDE2000 between a shot and its scene past which the cut is visible.
pub const MATCH_DE00: f64 = 8.0;
/// Mean a*/b* lean (CIELAB units) of a coherent cast.
pub const CAST_CHROMA: f64 = 12.0;
pub const CAST_COHERENCE: f64 = 0.75;
/// The skin (I-) line on the vectorscope, degrees from +Cb toward +Cr, and how far skin may stray from it.
pub const SKIN_LINE_DEG: f64 = 123.0;
pub const SKIN_TOL_DEG: f64 = 18.0;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Look {
    pub luma: f64,
    pub rgb: [f64; 3],
    pub lab: [f64; 3],
    /// a*/b* of the mean over mid-tone pixels, and how coherent the lean is (|mean| / mean |ab|, 0..1).
    pub lean: [f64; 2],
    pub coherence: f64,
    /// The most neutral 1/5 x 1/5 zone [x, y, w, h] in 0..1 (where a white-balance pick should sample).
    pub neutral_zone: [f64; 4],
    /// Per face: vectorscope hue (degrees) and chroma.
    pub skin: Vec<(f64, f64)>,
    pub faces_checked: bool,
}

fn lin(c: f64) -> f64 { if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) } }
/// sRGB (display, 0..1) to CIELAB, D65.
pub fn lab(rgb: [f64; 3]) -> [f64; 3] {
    let (r, g, b) = (lin(rgb[0]), lin(rgb[1]), lin(rgb[2]));
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f64| if t > 216.0 / 24389.0 { t.cbrt() } else { (24389.0 / 27.0 * t + 16.0) / 116.0 };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIEDE2000 colour difference (kL = kC = kH = 1).
pub fn de2000(l1: [f64; 3], l2: [f64; 3]) -> f64 {
    use std::f64::consts::PI;
    let (rad, deg) = (PI / 180.0, 180.0 / PI);
    let c1 = (l1[1] * l1[1] + l1[2] * l1[2]).sqrt(); let c2 = (l2[1] * l2[1] + l2[2] * l2[2]).sqrt();
    let cm = (c1 + c2) / 2.0; let g = 0.5 * (1.0 - (cm.powi(7) / (cm.powi(7) + 25f64.powi(7))).sqrt());
    let (a1, a2) = (l1[1] * (1.0 + g), l2[1] * (1.0 + g));
    let (c1p, c2p) = ((a1 * a1 + l1[2] * l1[2]).sqrt(), (a2 * a2 + l2[2] * l2[2]).sqrt());
    let hue = |b: f64, a: f64| if b == 0.0 && a == 0.0 { 0.0 } else { let h = b.atan2(a) * deg; if h < 0.0 { h + 360.0 } else { h } };
    let (h1, h2) = (hue(l1[2], a1), hue(l2[2], a2));
    let dl = l2[0] - l1[0]; let dc = c2p - c1p;
    let dh = if c1p * c2p == 0.0 { 0.0 } else if (h2 - h1).abs() <= 180.0 { h2 - h1 } else if h2 - h1 > 180.0 { h2 - h1 - 360.0 } else { h2 - h1 + 360.0 };
    let dhh = 2.0 * (c1p * c2p).sqrt() * (dh * rad / 2.0).sin();
    let lm = (l1[0] + l2[0]) / 2.0; let cmp = (c1p + c2p) / 2.0;
    let hm = if c1p * c2p == 0.0 { h1 + h2 } else if (h1 - h2).abs() <= 180.0 { (h1 + h2) / 2.0 } else if h1 + h2 < 360.0 { (h1 + h2 + 360.0) / 2.0 } else { (h1 + h2 - 360.0) / 2.0 };
    let t = 1.0 - 0.17 * ((hm - 30.0) * rad).cos() + 0.24 * ((2.0 * hm) * rad).cos() + 0.32 * ((3.0 * hm + 6.0) * rad).cos() - 0.20 * ((4.0 * hm - 63.0) * rad).cos();
    let dtheta = 30.0 * (-((hm - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (cmp.powi(7) / (cmp.powi(7) + 25f64.powi(7))).sqrt();
    let sl = 1.0 + 0.015 * (lm - 50.0).powi(2) / (20.0 + (lm - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cmp; let sh = 1.0 + 0.015 * cmp * t;
    let rt = -(2.0 * dtheta * rad).sin() * rc;
    ((dl / sl).powi(2) + (dc / sc).powi(2) + (dhh / sh).powi(2) + rt * (dc / sc) * (dhh / sh)).sqrt()
}

/// Vectorscope hue (degrees, +Cb toward +Cr) and chroma of a display RGB colour (BT.709, as get_scopes).
pub fn scope_hue(rgb: [f64; 3]) -> (f64, f64) {
    let y = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
    let (cb, cr) = ((rgb[2] - y) / 1.8556, (rgb[0] - y) / 1.5748);
    let h = cr.atan2(cb).to_degrees();
    (if h < 0.0 { h + 360.0 } else { h }, (cb * cb + cr * cr).sqrt())
}

/// Read one RGBA8 frame. `faces`: face boxes in pixels (x, y, w, h) to read skin in and leave out of the cast.
pub fn look(rgba: &[u8], w: usize, h: usize, faces: Option<&[[f64; 4]]>) -> Look {
    let n = (w * h).max(1);
    let inside = |x: usize, y: usize| faces.map(|fs| fs.iter().any(|f| (x as f64) >= f[0] && (x as f64) < f[0] + f[2] && (y as f64) >= f[1] && (y as f64) < f[1] + f[3])).unwrap_or(false);
    let (mut sum, mut luma) = ([0f64; 3], 0f64);
    let (mut sa, mut sb, mut mag, mut cnt) = (0f64, 0f64, 0f64, 0f64);
    let zones = 5usize;
    let mut zc = vec![(0f64, 0f64, 0usize); zones * zones]; // chroma sum, L sum, count
    for y in 0..h { for x in 0..w {
        let i = (y * w + x) * 4; if i + 2 >= rgba.len() { continue; }
        let c = [rgba[i] as f64 / 255.0, rgba[i + 1] as f64 / 255.0, rgba[i + 2] as f64 / 255.0];
        for k in 0..3 { sum[k] += c[k]; }
        luma += 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
        let l = lab(c);
        let z = &mut zc[(y * zones / h.max(1)) * zones + x * zones / w.max(1)];
        z.0 += (l[1] * l[1] + l[2] * l[2]).sqrt(); z.1 += l[0]; z.2 += 1;
        if l[0] < 15.0 || l[0] > 92.0 || inside(x, y) { continue; }
        sa += l[1]; sb += l[2]; mag += (l[1] * l[1] + l[2] * l[2]).sqrt(); cnt += 1.0;
    } }
    let rgb = [sum[0] / n as f64, sum[1] / n as f64, sum[2] / n as f64];
    let lean = if cnt > 0.0 { [sa / cnt, sb / cnt] } else { [0.0, 0.0] };
    let coherence = if mag > 0.0 { (lean[0].hypot(lean[1]) * cnt / mag).min(1.0) } else { 0.0 };
    let best = zc.iter().enumerate().filter(|(_, z)| z.2 > 0 && { let l = z.1 / z.2 as f64; (20.0..=92.0).contains(&l) })
        .min_by(|a, b| (a.1 .0 / a.1 .2 as f64).total_cmp(&(b.1 .0 / b.1 .2 as f64))).map(|(i, _)| i).unwrap_or(12);
    let zw = 1.0 / zones as f64;
    let mut skin = vec![];
    for f in faces.unwrap_or(&[]) {
        // the inner 60% of the box: cheeks and forehead, not hair or background
        let (x0, y0) = ((f[0] + f[2] * 0.2).max(0.0) as usize, (f[1] + f[3] * 0.2).max(0.0) as usize);
        let (x1, y1) = (((f[0] + f[2] * 0.8) as usize).min(w), ((f[1] + f[3] * 0.8) as usize).min(h));
        let (mut s, mut k) = ([0f64; 3], 0f64);
        for y in y0..y1 { for x in x0..x1 { let i = (y * w + x) * 4; for j in 0..3 { s[j] += rgba[i + j] as f64 / 255.0; } k += 1.0; } }
        if k > 0.0 { skin.push(scope_hue([s[0] / k, s[1] / k, s[2] / k])); }
    }
    Look { luma: luma / n as f64, rgb, lab: lab(rgb), lean, coherence, neutral_zone: [(best % zones) as f64 * zw, (best / zones) as f64 * zw, zw, zw], skin, faces_checked: faces.is_some() }
}

/// Shots: media clips with picture, in time order.
fn shots(p: &Project) -> Vec<std::sync::Arc<Clip>> {
    let mut v: Vec<_> = p.tracks.iter().flat_map(|t| t.clips.iter()).filter(|c| match &c.source {
        Source::Media { asset } => p.assets.get(asset).map(|a| a.width > 0 && a.kind.as_deref() != Some("audio")).unwrap_or(false), _ => false }).cloned().collect();
    v.sort_by(|a, b| a.start.total_cmp(&b.start));
    v
}

fn hash<T: std::hash::Hash>(v: &T) -> u64 { use std::hash::Hasher; let mut h = std::collections::hash_map::DefaultHasher::new(); v.hash(&mut h); h.finish() }
static LOOKS: LazyLock<Mutex<BTreeMap<u64, Look>>> = LazyLock::new(|| Mutex::new(BTreeMap::new()));

/// What the viewer sees at time t depends on every clip on screen then and the project-wide colour settings.
fn look_key(p: &Project, t: f64, faces: bool) -> u64 {
    let on: Vec<String> = p.tracks.iter().flat_map(|tr| tr.clips.iter()).filter(|c| c.start <= t && t < c.start + c.duration).map(|c| serde_json::to_string(c.as_ref()).unwrap_or_default()).collect();
    let mut g = serde_json::to_value(p).unwrap_or_default(); if let Some(o) = g.as_object_mut() { o.remove("tracks"); o.remove("markers"); }
    hash(&(on, g.to_string(), (t * 1000.0).round() as i64, faces))
}

fn measure(ss: &mut Session, t: f64, faces: bool) -> Option<Look> {
    let key = look_key(&ss.project, t, faces);
    if let Some(l) = LOOKS.lock().unwrap_or_else(|e| e.into_inner()).get(&key) { return Some(l.clone()); }
    let (rgba, w, h) = { let Session { project, cache, .. } = &mut *ss; crate::observe::draft(project, cache, t, 320).ok()? };
    let boxes: Option<Vec<[f64; 4]>> = if faces {
        let mut f = crate::effects::Frame::new(w, h);
        for (o, px) in f.d.iter_mut().zip(rgba.iter()) { *o = *px as f32 / 255.0; }
        let seen = crate::vision::detect_faces(&f, Some(crate::vision::FrameKey::of_pixels(&f)));
        if seen.fallback.is_some() { None } else { Some(seen.value.iter().filter(|b| b.score >= 0.6).map(|b| [b.bbox.x as f64, b.bbox.y as f64, b.bbox.w as f64, b.bbox.h as f64]).collect()) }
    } else { None };
    let l = look(&rgba, w, h, boxes.as_deref());
    let mut m = LOOKS.lock().unwrap_or_else(|e| e.into_inner());
    if m.len() >= 1024 { if let Some(k) = m.keys().next().cloned() { m.remove(&k); } }
    m.insert(key, l.clone());
    Some(l)
}

fn median(mut v: Vec<f64>) -> f64 { v.sort_by(f64::total_cmp); let n = v.len(); if n == 0 { 0.0 } else if n % 2 == 1 { v[n / 2] } else { (v[n / 2 - 1] + v[n / 2]) / 2.0 } }

/// Scene index of time t: how many "scene" markers are at or before it.
fn scene_of(p: &Project, t: f64) -> usize { p.markers.iter().filter(|m| m.name.trim().to_lowercase().starts_with("scene") && m.time <= t + 1e-6).count() }

/// Colour findings for the shots in [from, to]. `focus`: report only these clips (an edit reports what it touched;
/// their scene's other shots are still measured to compare against). Stops measuring at `deadline`; `faces` runs the
/// face detector for the skin check (slower).
pub fn check(ss: &mut Session, from: f64, to: f64, focus: Option<&[String]>, deadline: std::time::Instant, faces: bool) -> (Vec<Finding>, usize, usize) {
    let all = shots(&ss.project);
    let legacy = !ss.project.color_pipeline.enabled;
    let in_range: Vec<_> = all.iter().filter(|c| c.start < to && c.start + c.duration > from).cloned().collect();
    let wanted = |c: &Clip| focus.map(|f| f.iter().any(|x| x == &c.id)).unwrap_or(true);
    let scenes: std::collections::BTreeSet<usize> = in_range.iter().filter(|c| wanted(c)).map(|c| scene_of(&ss.project, c.start + c.duration / 2.0)).collect();
    // measure the focused shots first, then the rest of their scenes
    let mut order: Vec<_> = all.iter().filter(|c| scenes.contains(&scene_of(&ss.project, c.start + c.duration / 2.0))).cloned().collect();
    order.sort_by_key(|c| !wanted(c));
    let mut looks: Vec<(std::sync::Arc<Clip>, usize, Look)> = vec![];
    let (mut planned, mut done) = (0usize, 0usize);
    for c in order {
        planned += 1;
        if std::time::Instant::now() > deadline { continue; }
        let t = c.start + c.duration / 2.0;
        if let Some(l) = measure(ss, t, faces && wanted(&c)) { looks.push((c.clone(), scene_of(&ss.project, t), l)); done += 1; }
    }
    let mut out = vec![];
    for &sc in &scenes {
        let group: Vec<&(std::sync::Arc<Clip>, usize, Look)> = looks.iter().filter(|x| x.1 == sc).collect();
        if group.len() >= 2 {
            let med_lab = [median(group.iter().map(|x| x.2.lab[0]).collect()), median(group.iter().map(|x| x.2.lab[1]).collect()), median(group.iter().map(|x| x.2.lab[2]).collect())];
            let med_l = median(group.iter().map(|x| x.2.luma).collect());
            let med_rgb = [0, 1, 2].map(|k| median(group.iter().map(|x| x.2.rgb[k]).collect()));
            // two shots: the longer one is the reference; more: compare against the scene's median look
            let reference = if group.len() == 2 { if group[1].0.duration > group[0].0.duration { group[1] } else { group[0] } }
                else { group.iter().min_by(|a, b| de2000(a.2.lab, med_lab).total_cmp(&de2000(b.2.lab, med_lab))).copied().unwrap() };
            let (ref_lab, ref_l, ref_rgb) = if group.len() == 2 { (reference.2.lab, reference.2.luma, reference.2.rgb) } else { (med_lab, med_l, med_rgb) };
            for (c, _, l) in group.iter().map(|x| (&x.0, x.1, &x.2)) {
                if c.id == reference.0.id || !wanted(c) || !(c.start < to && c.start + c.duration > from) { continue; }
                let de = de2000(l.lab, ref_lab);
                let dl = (l.luma - ref_l).abs();
                let drgb = (0..3).map(|k| (l.rgb[k] - ref_rgb[k]).abs()).fold(0.0, f64::max);
                if de > MATCH_DE00 && (dl > LUMA_TOL || drgb > RGB_TOL) {
                    out.push(Finding::new("shot_mismatch", Severity::Warning, "color", format!("{} does not match its scene: dE00 {:.1} from {} ({} by {:.2} luma); the cut will jump. If it starts a new scene, add a marker named \"Scene ...\" at its start", c.id, de, if group.len() == 2 { format!("{}", reference.0.id) } else { "the scene's median look".into() }, if l.luma > ref_l { "brighter" } else { "darker" }, dl))
                        .clip(&c.id).range(c.start, c.start + c.duration)
                        .measured(json!({"de2000": r3(de), "luma": r3(l.luma), "scene_luma": r3(ref_l), "avg_rgb": l.rgb.map(r3), "scene_rgb": ref_rgb.map(r3), "scene": sc}))
                        .threshold(json!({"de2000": MATCH_DE00, "luma_tolerance": LUMA_TOL, "rgb_tolerance": RGB_TOL}))
                        .fix("match_shot_look", json!({"reference_clip_ids": [reference.0.id.clone()], "target_clip_ids": [c.id.clone()], "strength": 0.8})));
                }
            }
        }
    }
    for (c, _, l) in &looks {
        if !wanted(c) || !(c.start < to && c.start + c.duration > from) { continue; }
        let t = r3(c.start + c.duration / 2.0);
        let lean = l.lean[0].hypot(l.lean[1]);
        if lean > CAST_CHROMA && l.coherence > CAST_COHERENCE {
            let hue = l.lean[1].atan2(l.lean[0]).to_degrees();
            let name = match ((hue + 360.0) % 360.0) as i64 { 0..=35 | 330..=360 => "magenta/red", 36..=80 => "orange", 81..=110 => "yellow", 111..=170 => "green", 171..=250 => "cyan/blue", _ => "blue/violet" };
            let (tool, args) = if legacy { let z = l.neutral_zone; ("pick_white_balance", json!({"clip_id": c.id, "time": t, "x": r3(z[0]), "y": r3(z[1]), "w": r3(z[2]), "h": r3(z[3])})) } else { ("auto_grade", json!({"clip_ids": [c.id.clone()]})) };
            out.push(Finding::new("color_cast", Severity::Warning, "color", format!("{} has a strong {name} cast (a*/b* lean {lean:.0}, {:.0}% coherent): whites and skin will look wrong. Pick a neutral area with pick_white_balance, or auto_grade it; ignore if the tint is the intended look", c.id, l.coherence * 100.0))
                .clip(&c.id).at(t).measured(json!({"lean_ab": l.lean.map(r3), "lean_chroma": r3(lean), "coherence": r3(l.coherence)}))
                .threshold(json!({"lean_chroma": CAST_CHROMA, "coherence": CAST_COHERENCE})).fix(tool, args));
        }
        for &(hue, chroma) in &l.skin {
            let off = ((hue - SKIN_LINE_DEG + 540.0) % 360.0) - 180.0;
            if chroma > 0.015 && off.abs() > SKIN_TOL_DEG {
                out.push(Finding::new("skin_off_line", Severity::Warning, "color", format!("skin in {} sits {:.0} degrees {} the skin line on the vectorscope: faces look {}", c.id, off.abs(), if off > 0.0 { "past" } else { "short of" }, if off > 0.0 { "magenta/red" } else { "green/yellow" }))
                    .clip(&c.id).at(t).measured(json!({"skin_hue_deg": r3(hue), "chroma": r3(chroma)})).threshold(json!({"skin_line_deg": SKIN_LINE_DEG, "tolerance_deg": SKIN_TOL_DEG}))
                    .fix("auto_grade", json!({"clip_ids": [c.id.clone()]})));
            }
        }
    }
    (out, done, planned)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn img(w: usize, h: usize, rgb: [u8; 3]) -> Vec<u8> { (0..w * h).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect() }
    #[test]
    fn de2000_reference_pairs() {
        // Sharma, Wu & Dalal (2005) test data, pairs 1 and 17
        assert!((de2000([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485]) - 2.0425).abs() < 1e-3);
        assert!((de2000([50.0, 2.5, 0.0], [73.0, 25.0, -18.0]) - 27.1492).abs() < 1e-3);
        assert_eq!(de2000([40.0, 10.0, 5.0], [40.0, 10.0, 5.0]), 0.0);
    }
    #[test]
    fn grey_has_no_cast_and_a_tint_does() {
        let g = look(&img(20, 20, [128, 128, 128]), 20, 20, None);
        assert!(g.lean[0].hypot(g.lean[1]) < 1.0);
        let b = look(&img(20, 20, [112, 128, 168]), 20, 20, None);
        assert!(b.lean[0].hypot(b.lean[1]) > CAST_CHROMA && b.coherence > 0.95, "{b:?}");
        // half red, half green: leans two ways, not a cast
        let mut rg = img(20, 20, [180, 60, 60]); for px in rg.chunks_mut(4).skip(200) { px.copy_from_slice(&[60, 160, 60, 255]); }
        assert!(look(&rg, 20, 20, None).coherence < CAST_COHERENCE);
    }
    #[test]
    fn skin_sits_on_the_i_line() {
        let (h, _) = scope_hue([0.8, 0.6, 0.5]);
        assert!((h - SKIN_LINE_DEG).abs() < SKIN_TOL_DEG, "{h}");
        let (h, _) = scope_hue([0.6, 0.7, 0.5]); // green-shifted
        assert!((h - SKIN_LINE_DEG).abs() > SKIN_TOL_DEG, "{h}");
        let l = look(&img(20, 20, [204, 153, 128]), 20, 20, Some(&[[5.0, 5.0, 10.0, 10.0]]));
        assert_eq!(l.skin.len(), 1);
    }
}
