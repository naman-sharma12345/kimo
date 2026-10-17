//! v6 "eyes": read-only flash / photosensitivity measurement series. Raw numbers only: per-frame linear relative luminance,
//! red saturation, transitions and per-window transition counts. Thresholds are arguments; no pass/fail, no verdict.
//! Formulas (own code from the published definitions): sRGB EOTF per IEC 61966-2-1, relative luminance Y with Rec.709 / BT.709
//! coefficients on linear RGB, saturated red R/(R+G+B) and red quantity max(0, R-G-B)*320 as in the WCAG 2.x flash definitions.
use super::kit::*;
use crate::perception::analysis::{self, Image, Request};

/// sRGB EOTF (IEC 61966-2-1): encoded 0..1 -> linear 0..1.
pub fn srgb_to_linear(c: f64) -> f64 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
fn lut() -> [f64; 256] {
    let mut t = [0.; 256];
    for (i, v) in t.iter_mut().enumerate() {
        *v = srgb_to_linear(i as f64 / 255.);
    }
    t
}
/// Thresholds that shape the derived values (all arguments of the tool).
#[derive(Clone, Copy, Debug)]
pub struct Params {
    pub lum_delta: f64,
    pub dark_max: f64,
    pub red_delta: f64,
    pub red_ratio_min: f64,
    pub red_floor: f64,
}
impl Default for Params {
    fn default() -> Self {
        Params { lum_delta: 0.1, dark_max: 0.8, red_delta: 20., red_ratio_min: 0.8, red_floor: 0.01 }
    }
}
/// Per-pixel linear planes of one analysed frame, kept for the next frame's area comparison.
pub struct Planes {
    pub y: Vec<f64>,
    pub red_q: Vec<f64>,
}
/// One frame's raw measurements.
#[derive(Clone, Copy, Debug, Default)]
pub struct FrameStats {
    /// mean relative luminance Y (linear light, 0..1)
    pub luminance: f64,
    /// mean per-pixel R/(R+G+B), linear light; a pixel with R+G+B < red_floor counts as 0
    pub red_saturation: f64,
    /// mean per-pixel red quantity: max(0, R-G-B)*320 where R/(R+G+B) >= red_ratio_min, else 0
    pub red_quantity: f64,
    /// fraction of pixels whose own Y moved by >= lum_delta since the previous sample with the darker of the two < dark_max
    pub changed_area_fraction: f64,
    /// fraction of pixels whose own red quantity moved by >= red_delta since the previous sample
    pub red_changed_area_fraction: f64,
}
/// Measure one display-referred 8-bit image (rgb = u8/255). `prev` gives the area fractions; None -> 0.
pub fn measure(im: &Image, prev: Option<&Planes>, p: &Params) -> (FrameStats, Planes) {
    let t = lut();
    let n = im.w * im.h;
    crate::guard::reserve(n * 16);
    let (mut y, mut rq) = (Vec::with_capacity(n), Vec::with_capacity(n));
    let (mut sy, mut ss, mut sq) = (0., 0., 0.);
    for i in 0..n {
        if i % 65536 == 0 {
            crate::guard::check();
        }
        let l = |c: usize| t[((im.rgb[c][i] * 255.).round().clamp(0., 255.)) as usize];
        let (r, g, b) = (l(0), l(1), l(2));
        let yy = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let sum = r + g + b;
        let ratio = if sum >= p.red_floor && sum > 0. { r / sum } else { 0. };
        let q = if sum > 0. && r / sum >= p.red_ratio_min { (r - g - b).max(0.) * 320. } else { 0. };
        sy += yy;
        ss += ratio;
        sq += q;
        y.push(yy);
        rq.push(q);
    }
    let nn = n.max(1) as f64;
    let mut st = FrameStats { luminance: sy / nn, red_saturation: ss / nn, red_quantity: sq / nn, ..Default::default() };
    if let Some(pv) = prev.filter(|pv| pv.y.len() == n) {
        let (mut ca, mut cr) = (0usize, 0usize);
        for i in 0..n {
            let (a, b) = (pv.y[i], y[i]);
            if (a - b).abs() >= p.lum_delta && a.min(b) < p.dark_max {
                ca += 1;
            }
            if (pv.red_q[i] - rq[i]).abs() >= p.red_delta {
                cr += 1;
            }
        }
        st.changed_area_fraction = ca as f64 / nn;
        st.red_changed_area_fraction = cr as f64 / nn;
    }
    (st, Planes { y, red_q: rq })
}
/// Transitions in a series: a swing between successive confirmed extrema of amplitude >= delta (a reversal is confirmed once the
/// value moves >= delta back from the running extreme, so wiggles smaller than delta never split a swing). A swing counts when
/// min(start, end) < dark_max (pass f64::INFINITY for no darker-state condition). Returns per-sample +1 (rise) / -1 (fall) / 0,
/// placed on the sample where the swing first reached its new extreme. The last, unreversed swing counts too.
pub fn transitions(v: &[f64], delta: f64, dark_max: f64) -> Vec<i8> {
    let mut out = vec![0i8; v.len()];
    if v.is_empty() || !(delta > 0.) {
        return out;
    }
    let mark = |out: &mut Vec<i8>, a: f64, e: f64, idx: usize| {
        if (e - a).abs() >= delta && a.min(e) < dark_max {
            out[idx] = if e > a { 1 } else { -1 };
        }
    };
    let (mut lo, mut lo_i, mut hi, mut hi_i) = (v[0], 0usize, v[0], 0usize);
    let (mut dir, mut anchor, mut ext, mut ext_i) = (0i8, v[0], v[0], 0usize);
    for (i, &x) in v.iter().enumerate().skip(1) {
        match dir {
            0 => {
                if x < lo { lo = x; lo_i = i; }
                if x > hi { hi = x; hi_i = i; }
                if hi - lo >= delta {
                    if hi_i > lo_i { dir = 1; anchor = lo; ext = hi; ext_i = hi_i; } else { dir = -1; anchor = hi; ext = lo; ext_i = lo_i; }
                }
            }
            1 => {
                if x > ext { ext = x; ext_i = i; } else if ext - x >= delta {
                    mark(&mut out, anchor, ext, ext_i);
                    anchor = ext; dir = -1; ext = x; ext_i = i;
                }
            }
            _ => {
                if x < ext { ext = x; ext_i = i; } else if x - ext >= delta {
                    mark(&mut out, anchor, ext, ext_i);
                    anchor = ext; dir = 1; ext = x; ext_i = i;
                }
            }
        }
    }
    if dir != 0 {
        mark(&mut out, anchor, ext, ext_i);
    }
    out
}
/// Count nonzero events whose sample time falls in [start, start+window) for each window start.
pub fn window_counts(times: &[f64], events: &[i8], starts: &[f64], window: f64) -> Vec<usize> {
    let mut pre = vec![0usize; times.len() + 1];
    for i in 0..times.len() {
        pre[i + 1] = pre[i] + (events[i] != 0) as usize;
    }
    let eps = 1e-9;
    starts
        .iter()
        .map(|&s| {
            let a = times.partition_point(|&t| t < s - eps);
            let b = times.partition_point(|&t| t < s + window - eps);
            pre[b] - pre[a]
        })
        .collect()
}
/// Flashes: consecutive opposing transitions paired in order (1st+2nd, 3rd+4th, ...; a same-direction neighbour restarts the
/// pairing, an unpaired last transition is no flash). Each flash spans [time of its first transition, time of its second].
pub fn flash_spans(times: &[f64], events: &[i8]) -> Vec<(f64, f64)> {
    let ev: Vec<(f64, i8)> = times.iter().zip(events).filter(|(_, e)| **e != 0).map(|(t, e)| (*t, *e)).collect();
    let (mut out, mut i) = (vec![], 0);
    while i + 1 < ev.len() {
        if ev[i].1 != ev[i + 1].1 {
            out.push((ev[i].0, ev[i + 1].0));
            i += 2;
        } else {
            i += 1;
        }
    }
    out
}
/// Flashes per window: a flash counts once in EACH window its span touches, so a flash straddling a window edge counts in
/// both windows. Window = [start, start+window).
pub fn window_flash_counts(spans: &[(f64, f64)], starts: &[f64], window: f64) -> Vec<usize> {
    let eps = 1e-9;
    starts.iter().map(|&s| spans.iter().filter(|(a, b)| *a < s + window - eps && *b >= s - eps).count()).collect()
}
/// Flashes fully inside each window: both transitions of the flash have sample times in [start, start+window). Sibling of
/// `window_flash_counts` (which counts every window a flash touches); a flash straddling a window edge counts in neither
/// edge window here.
pub fn window_flash_counts_inside(spans: &[(f64, f64)], starts: &[f64], window: f64) -> Vec<usize> {
    let eps = 1e-9;
    starts.iter().map(|&s| spans.iter().filter(|(a, b)| *a >= s - eps && *b < s + window - eps).count()).collect()
}
fn numeric(a: &Value, k: &str, d: f64) -> Result<f64, String> {
    let v = a
        .get(k)
        .map(|v| v.as_f64().ok_or_else(|| format!("out_of_range: {k} must be numeric")))
        .transpose()?
        .unwrap_or(d);
    if !v.is_finite() {
        return Err(format!("out_of_range: {k} must be finite"));
    }
    Ok(v)
}
fn r6(x: f64) -> f64 {
    (x * 1e6).round() / 1e6
}
fn run(ss: &mut Session, a: &Value) -> Result<Value, String> {
    let source = a
        .get("source")
        .map(|v| v.as_str().ok_or("invalid_source: source string required"))
        .transpose()?
        .unwrap_or("timeline");
    let parsed = if source == "timeline" { "composite" } else { source };
    if parsed != "composite" && !parsed.starts_with("clip:") && !parsed.starts_with("file:") {
        return Err("invalid_source: Valid: timeline, clip:<id>, file:<path>".into());
    }
    let mut bounds = (0., ss.project.duration());
    if let Some(id) = parsed.strip_prefix("clip:") {
        let c = ss.project.find_clip(id).ok_or("invalid_source: missing clip")?;
        bounds = (c.start, c.start + c.duration);
    }
    if let Some(path) = parsed.strip_prefix("file:") {
        let (_, _, d, _) = crate::render::probe(&ss.project.resolve(path)).map_err(|e| format!("invalid_source: {e}"))?;
        bounds = (0., d);
    }
    let fps = ss.project.fps;
    let start = numeric(a, "start", bounds.0)?;
    let end = numeric(a, "end", bounds.1)?;
    let step = match a.get("step_frames") {
        None => 1u64,
        Some(v) => v.as_u64().filter(|s| *s >= 1).ok_or("out_of_range: step_frames must be an integer >= 1")?,
    };
    let window = numeric(a, "window_s", 1.0)?;
    let d = Params::default();
    let p = Params {
        lum_delta: numeric(a, "lum_delta", d.lum_delta)?,
        dark_max: numeric(a, "dark_max", d.dark_max)?,
        red_delta: numeric(a, "red_delta", d.red_delta)?,
        red_ratio_min: numeric(a, "red_ratio_min", d.red_ratio_min)?,
        red_floor: numeric(a, "red_floor", d.red_floor)?,
    };
    if start < bounds.0 - 1e-9
        || end > bounds.1 + 1e-9
        || end <= start
        || !(fps > 0.)
        || window <= 0.
        || !(p.lum_delta > 0. && p.lum_delta <= 1.)
        || !(0.0..=1.0).contains(&p.dark_max)
        || !(p.red_delta > 0.)
        || !(0.0..=1.0).contains(&p.red_ratio_min)
        || !(0.0..=3.0).contains(&p.red_floor)
    {
        return Err("out_of_range: require source-bounded start<end, window_s>0, 0<lum_delta<=1, dark_max 0..1, red_delta>0, red_ratio_min 0..1, red_floor 0..3".into());
    }
    let f0 = (start * fps - 1e-9).ceil().max(0.) as u64;
    let f1 = (end * fps - 1e-9).ceil() as u64; // exclusive
    let frames: Vec<u64> = (f0..f1).step_by(step as usize).collect();
    let n = frames.len();
    if n == 0 {
        return Err("out_of_range: no timeline frame inside [start,end)".into());
    }
    if n > 20000 {
        return Err(format!("too_many_frames: {n} exceeds cap 20000; raise step_frames or shorten the range"));
    }
    let hop = numeric(a, "window_hop_s", step as f64 / fps)?;
    if hop <= 0. {
        return Err("out_of_range: window_hop_s must be > 0".into());
    }
    let mut region: Option<[usize; 4]> = None;
    if let Some(r) = a.get("region") {
        let arr = r.as_array().filter(|x| x.len() == 4).ok_or("out_of_range: region must be [x,y,w,h] pixels")?;
        let mut q = [0usize; 4];
        for (k, v) in arr.iter().enumerate() {
            q[k] = v.as_u64().ok_or("out_of_range: region integer pixels required")? as usize;
        }
        region = Some(q);
    }
    let mut times = Vec::with_capacity(n);
    let mut stats = Vec::with_capacity(n);
    let mut prev: Option<Planes> = None;
    let mut dims = (0usize, 0usize);
    for &f in &frames {
        crate::guard::check();
        let t = f as f64 / fps;
        let mut im = analysis::render(ss, &Request { time: t, source: parsed.into() })?;
        if let Some(q) = region {
            if q[2] == 0 || q[3] == 0 || q[0].checked_add(q[2]).map_or(true, |v| v > im.w) || q[1].checked_add(q[3]).map_or(true, |v| v > im.h) {
                return Err(format!("out_of_range: region outside the {}x{} frame", im.w, im.h));
            }
            im = im.crop(q);
        }
        dims = (im.w, im.h);
        let (st, planes) = measure(&im, prev.as_ref(), &p);
        times.push(t);
        stats.push(st);
        prev = Some(planes);
    }
    let lum: Vec<f64> = stats.iter().map(|s| s.luminance).collect();
    let redq: Vec<f64> = stats.iter().map(|s| s.red_quantity).collect();
    let lt = transitions(&lum, p.lum_delta, p.dark_max);
    let rt = transitions(&redq, p.red_delta, f64::INFINITY);
    let last_start = end - window;
    let mut starts = vec![];
    if last_start >= start - 1e-9 {
        let mut k = 0u64;
        loop {
            let s = start + k as f64 * hop;
            if s > last_start + 1e-9 || starts.len() >= 20000 {
                break;
            }
            starts.push(s);
            k += 1;
        }
    } else {
        starts.push(start); // range shorter than one window: one window, its end is the range end
    }
    let wl = window_counts(&times, &lt, &starts, window);
    let wr = window_counts(&times, &rt, &starts, window);
    let (fl, fr) = (flash_spans(&times, &lt), flash_spans(&times, &rt));
    let wfl = window_flash_counts(&fl, &starts, window);
    let wfr = window_flash_counts(&fr, &starts, window);
    let wil = window_flash_counts_inside(&fl, &starts, window);
    let wir = window_flash_counts_inside(&fr, &starts, window);
    let col = |f: &dyn Fn(&FrameStats) -> f64| -> Value { json!(stats.iter().map(|s| r6(f(s))).collect::<Vec<_>>()) };
    let dl: Vec<f64> = (0..n).map(|i| if i == 0 { 0. } else { r6(lum[i] - lum[i - 1]) }).collect();
    let max_area = stats.iter().map(|s| s.changed_area_fraction).fold(0., f64::max);
    let max_red_area = stats.iter().map(|s| s.red_changed_area_fraction).fold(0., f64::max);
    Ok(json!({
        "source": source,
        "range": {"start": start, "end": end},
        "timeline_fps": fps,
        "step_frames": step,
        "samples": n,
        "analysed": {"w": dims.0, "h": dims.1, "region": region},
        "params_used": {
            "window_s": window, "window_hop_s": hop, "lum_delta": p.lum_delta, "dark_max": p.dark_max,
            "red_delta": p.red_delta, "red_ratio_min": p.red_ratio_min, "red_floor": p.red_floor,
            "max_resolvable_hz": fps / step as f64 / 2.,
            "linearisation": "sRGB EOTF (IEC 61966-2-1) on the rendered 8-bit display output",
            "luminance": "mean over pixels of Y = 0.2126 R + 0.7152 G + 0.0722 B (linear, Rec.709/BT.709)",
            "red_saturation": "mean over pixels of R/(R+G+B) (linear); a pixel with R+G+B < red_floor counts as 0",
            "red_quantity": "mean over pixels of max(0, R-G-B)*320 where R/(R+G+B) >= red_ratio_min, else 0",
            "transition": "swing between confirmed extrema of amplitude >= delta (reversal confirmed after >= delta back); luminance swings also need min(start,end) < dark_max; marked +1/-1 on the sample that first reached the new extreme",
            "changed_area_fraction": "share of pixels whose own Y moved >= lum_delta since the previous sample with the darker < dark_max",
            "red_changed_area_fraction": "share of pixels whose own red quantity moved >= red_delta since the previous sample",
            "windows": "transitions with sample time in [start, start+window_s); window starts every window_hop_s while start+window_s <= range end (a range shorter than window_s gives one window ending at the range end)",
            "flashes": "consecutive opposing transitions paired in order; lum_flashes / red_flashes count a flash once in each window its span [first, second transition] touches, so one straddling a window edge counts in both; *_flashes_fully_inside count only flashes whose two transitions both fall in [start, start+window_s)",
            "max_resolvable_hz_note": "max_resolvable_hz = sampling rate / 2 = timeline fps / step_frames / 2; flashing faster than this cannot be resolved from these samples"
        },
        "frames": {
            "time": times.iter().map(|t| r6(*t)).collect::<Vec<_>>(),
            "frame": frames,
            "luminance": col(&|s| s.luminance),
            "lum_delta": dl,
            "red_saturation": col(&|s| s.red_saturation),
            "red_quantity": col(&|s| s.red_quantity),
            "lum_transition": lt,
            "red_transition": rt,
            "changed_area_fraction": col(&|s| s.changed_area_fraction),
            "red_changed_area_fraction": col(&|s| s.red_changed_area_fraction)
        },
        "windows": {
            "start": starts.iter().map(|s| r6(*s)).collect::<Vec<_>>(),
            "end": starts.iter().map(|s| r6((s + window).min(end))).collect::<Vec<_>>(),
            "lum_transitions": wl,
            "red_transitions": wr,
            "lum_flashes": wfl,
            "red_flashes": wfr,
            "lum_flashes_fully_inside": wil,
            "red_flashes_fully_inside": wir
        },
        "totals": {
            "lum_transitions": lt.iter().filter(|x| **x != 0).count(),
            "red_transitions": rt.iter().filter(|x| **x != 0).count(),
            "lum_flashes": fl.len(),
            "red_flashes": fr.len(),
            "max_lum_transitions_in_window": wl.iter().copied().max().unwrap_or(0),
            "max_red_transitions_in_window": wr.iter().copied().max().unwrap_or(0),
            "max_lum_flashes_in_window": wfl.iter().copied().max().unwrap_or(0),
            "max_red_flashes_in_window": wfr.iter().copied().max().unwrap_or(0),
            "max_lum_flashes_fully_inside_window": wil.iter().copied().max().unwrap_or(0),
            "max_red_flashes_fully_inside_window": wir.iter().copied().max().unwrap_or(0),
            "max_changed_area_fraction": r6(max_area),
            "max_red_changed_area_fraction": r6(max_red_area)
        }
    }))
}
pub fn commands() -> Vec<Command> {
    vec![Command{name:"flash_series",mutates:false,description:"Measure flashing over a time range: per-frame linear luminance, red saturation, transitions and per-window transition counts. Read-only, raw numbers, no pass/fail or verdict. source timeline (default), clip:<id> or file:<path>; start/end default the whole source; samples every timeline frame, step_frames N for every Nth; optional region [x,y,w,h]. Pixels are decoded from the rendered 8-bit output with the sRGB EOTF; luminance = mean Rec.709 Y = .2126R+.7152G+.0722B in linear light; red_saturation = mean R/(R+G+B), pixels with R+G+B < red_floor (default .01) count 0; red_quantity = mean max(0,R-G-B)*320 over pixels with R/(R+G+B) >= red_ratio_min (default .8). A transition is a swing between confirmed extrema of at least lum_delta (default .1) with the darker end < dark_max (default .8), or at least red_delta (default 20) of red_quantity; marked +1/-1 on the frame reaching the new extreme. Per-frame changed_area_fraction and red_changed_area_fraction count pixels that moved by those thresholds since the previous sample. Windows of window_s (default 1) every window_hop_s (default one sample) report transition and flash counts with start/end times; a flash is two opposing transitions paired in order and counts once in each window its span touches, so a flash straddling a window edge counts in both; lum_flashes_fully_inside / red_flashes_fully_inside count only flashes with both transitions inside the window. params_used.max_resolvable_hz = fps/step_frames/2. Black to saturated red is also a 0.2126 luminance swing, so at the default lum_delta a red flash counts as a luminance flash too. Reference values for reading the counts, not applied here: Harding, ITU-R BT.1702 and WCAG 2.x treat more than 3 flashes in any 1 s as hazardous, a flash being a pair of opposing luminance changes of 10% or more of max luminance with the darker state below 0.80, or a pair of opposing transitions involving saturated red (WCAG 2.0 notes: R/(R+G+B) >= 0.8 and a (R-G-B)*320 change above 20; WCAG 2.2 uses a 0.2 change in CIE 1976 u'v'), when the flashing area exceeds 25% of the screen (WCAG: .25 of any 10 degree visual field). Caps 20000 samples. Solid colours in a test project must be [r,g,b] arrays: a hex string currently renders black (v5 engine bug, fix planned).",schema:obj(vec![("source",s("timeline default, clip:<id>, file:<path>")),("start",n("range start seconds, default source start")),("end",n("range end seconds, default source end")),("step_frames",json!({"type":"integer","minimum":1,"description":"sample every Nth timeline frame, default 1 = every frame"})),("region",json!({"type":"array","items":{"type":"integer"},"minItems":4,"maxItems":4,"description":"[x,y,w,h] pixels; default whole frame"})),("window_s",n("window length seconds, default 1")),("window_hop_s",n("window start spacing seconds, default one sample")),("lum_delta",n("luminance swing threshold 0..1, default .1")),("dark_max",n("darker-state ceiling 0..1, default .8")),("red_delta",n("red_quantity swing threshold, default 20")),("red_ratio_min",n("R/(R+G+B) for saturated red 0..1, default .8")),("red_floor",n("R+G+B floor for red_saturation, default .01"))],&[]),run}]
}
