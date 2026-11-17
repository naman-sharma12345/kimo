//! Tool index: how an AI finds kimo's tools. No tool changes here; this is the layer between a plain-language need
//! ("make the music quieter when people talk") and a correct first call.
//!
//! - Ranking is BM25F over four fields of every tool: name words, argument names, the first sentence and the full
//!   description, so a word in a tool's name counts far more than the same word deep in some other tool's notes, and
//!   words every tool uses ("clip", "add") count for little.
//! - Queries are read the way editors talk: phrases ("slow motion", "green screen", "j cut"), an editing vocabulary
//!   mapped to the words the tools use ("quieter" -> volume, duck), UK/US spelling, plural and -ing forms, and typos
//!   (one or two letters off).
//! - A small prior lifts the everyday tools and lowers the expert/raw ones (inspect_*, *_raw, OCIO/ACES internals),
//!   so "colour grade" finds auto_grade before an ACES reference renderer.
//! - Every hit can carry its compact argument signature, so search goes straight to a call.
use crate::commands::{registry_ref, Command};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::OnceLock;

const STOP: &[&str] = &["a", "an", "the", "to", "of", "for", "and", "or", "in", "on", "with", "at", "by", "from", "into", "it", "its", "this", "that",
    "these", "those", "my", "me", "i", "we", "our", "you", "your", "is", "are", "be", "make", "please", "can", "want", "some", "so", "then", "there",
    "here", "them", "they", "bit", "just", "really", "little", "get", "put", "do", "does", "how", "what", "which", "when", "where", "while", "would",
    "should", "could", "like", "something", "someone", "s", "need", "let", "lets", "us", "one", "thing", "way", "kind", "sort", "too", "very"];

/// UK and other spellings, mapped before stemming.
const SPELL: &[(&str, &str)] = &[("colour", "color"), ("colours", "colors"), ("coloured", "colored"), ("grey", "gray"), ("greyscale", "grayscale"),
    ("stabilise", "stabilize"), ("stabilised", "stabilized"), ("stabilisation", "stabilization"), ("stabiliser", "stabilizer"), ("normalise", "normalize"),
    ("normalised", "normalized"), ("centre", "center"), ("centred", "centered"), ("analyse", "analyze"), ("organise", "organize"), ("synchronise", "synchronize"),
    ("recognise", "recognize"), ("optimise", "optimize"), ("visualise", "visualize"), ("saturate", "saturation"), ("subtitels", "subtitles"),
    ("vidoe", "video"), ("captoins", "captions"), ("slowmo", "slow speed"), ("slomo", "slow speed"), ("bw", "monochrome grayscale saturation")];

/// Phrases first: they carry a meaning their words don't. Each is replaced by the words the tools use.
const PHRASES: &[(&str, &str)] = &[
    ("slow motion", "slow speed"), ("slow-mo", "slow speed"), ("slo-mo", "slow speed"), ("speed up", "speed fast"), ("time lapse", "speed fast"),
    ("timelapse", "speed fast"), ("green screen", "chroma key"), ("blue screen", "chroma key"), ("greenscreen", "chroma key"),
    ("black and white", "monochrome grayscale saturation"), ("black & white", "monochrome grayscale saturation"), ("b&w", "monochrome grayscale saturation"),
    ("picture in picture", "pip"), ("picture-in-picture", "pip"), ("lower third", "title lower third"), ("lower thirds", "title lower third"),
    ("b-roll", "broll"), ("b roll", "broll"), ("j cut", "split edit"), ("j-cut", "split edit"), ("l cut", "split edit"), ("l-cut", "split edit"),
    ("dolly zoom", "vertigo zoom"), ("fade to black", "fade transition dip black"), ("fade from black", "fade transition dip black"),
    ("dip to black", "fade transition dip black"), ("jump cut", "punch morph cut"), ("rough cut", "rough cut"), ("talking head", "talking speech rough"),
    ("background music", "music audio"), ("background noise", "noise denoise"), ("remove background", "matte key roto"),
    ("background removal", "matte key roto"), ("close the gap", "ripple gap"), ("closes the gap", "ripple gap"), ("letter by letter", "animator"),
    ("word by word", "animator"), ("scene change", "scene cut detect"), ("scene changes", "scene cut detect"), ("shot change", "scene cut detect"),
    ("camera angle", "multicam angle"), ("camera angles", "multicam angle"), ("white balance", "white balance"), ("motion blur", "motion blur"),
    ("type on", "animator typewriter"), ("count up", "counter"), ("count down", "counter"), ("countdown", "counter"), ("frame rate", "fps"),
    ("aspect ratio", "aspect reframe"), ("lens distortion", "undistort lens"), ("fish eye", "undistort lens"), ("fisheye", "undistort lens"),
    ("cut out", "matte roto mask"), ("play backwards", "reverse"), ("play it backwards", "reverse"), ("audio before the video", "split edit"),
    ("starts before the video", "split edit"), ("shorter than", "fit max duration"), ("no longer than", "fit max duration"), ("under 30 seconds", "fit max duration"), ("play the clip backwards", "reverse"), ("make it vertical", "vertical reframe aspect"), ("one call", ""),
];

/// What editors say -> the words kimo's tools use. Keys and values are matched after spelling and stemming.
const SYN: &[(&[&str], &[&str])] = &[
    (&["quiet", "quieter", "softer", "lower", "duck", "ducking", "dip"], &["volume", "duck", "gain"]),
    (&["loud", "louder", "boost", "amplify"], &["volume", "gain", "loudness"]),
    (&["music", "song", "soundtrack", "bgm"], &["music", "audio"]),
    (&["voice", "speech", "talk", "talking", "speaker", "speaking", "dialogue", "dialog", "narration", "voiceover", "vo", "people"], &["speech", "dialogue", "voice"]),
    (&["silence", "silent", "dead", "pause", "pauses", "air"], &["silence", "pause", "speech"]),
    (&["slow", "faster", "fast", "quick", "quicker", "slower"], &["speed"]),
    (&["backward", "backwards", "reverse", "rewind"], &["reverse", "time"]),
    (&["shaky", "wobbly", "handheld", "steady", "steadier", "jittery"], &["stabilize"]),
    (&["subtitle", "subtitles", "caption", "captions", "cc", "srt", "vtt"], &["caption", "subtitle"]),
    (&["chroma", "keyer", "keying"], &["key", "chroma", "matte"]),
    (&["isolate", "cutout", "silhouette"], &["matte", "mask", "key"]),
    (&["face", "faces", "person", "subject", "object"], &["detect", "subject", "face", "track"]),
    (&["tiktok", "reels", "reel", "shorts", "vertical", "portrait", "instagram", "9x16"], &["reframe", "aspect", "vertical"]),
    (&["widescreen", "horizontal", "landscape", "youtube", "16x9"], &["aspect", "reframe", "preset"]),
    (&["zoom", "push"], &["scale", "punch", "zoom"]),
    (&["rotate", "spin", "tilt", "turn"], &["rotation"]),
    (&["fade", "fading"], &["transition", "fade", "opacity", "volume"]),
    (&["dissolve", "crossfade", "xfade", "cross"], &["transition", "dissolve"]),
    (&["wipe", "swipe"], &["transition", "wipe"]),
    (&["title", "titles", "heading", "headline", "intro", "opener"], &["title", "text"]),
    (&["font", "typeface", "typography"], &["font", "text"]),
    (&["logo", "watermark", "bug"], &["watermark", "logo"]),
    (&["sticker", "emoji", "icon", "clipart", "illustration"], &["asset", "icon", "search"]),
    (&["arrow", "pointer", "callout", "annotate", "annotation", "point"], &["arrow"]),
    (&["box", "rectangle", "square", "outline"], &["shape", "box", "rect", "detection"]),
    (&["circle", "oval"], &["ellipse", "shape"]),
    (&["grade", "grading", "lut", "tone", "color", "colors"], &["grade", "color"]),
    (&["warm", "warmer", "cool", "cooler", "temperature", "tint", "orange", "blue"], &["temperature", "white", "balance", "grade"]),
    (&["bright", "brighter", "dark", "darker", "underexposed", "overexposed", "exposure", "dim"], &["exposure", "brightness"]),
    (&["vivid", "saturated", "desaturate", "dull", "punchy"], &["saturation"]),
    (&["monochrome", "grayscale"], &["saturation", "monochrome"]),
    (&["cinematic", "film", "filmic"], &["grade", "letterbox", "grain"]),
    (&["bars", "letterbox", "widescreen"], &["letterbox"]),
    (&["noise", "hiss", "hum", "buzz", "denoise"], &["denoise", "noise"]),
    (&["echo", "reverb", "room", "echoey"], &["reverb", "audio", "effect"]),
    (&["eq", "bass", "treble", "equalizer", "equaliser"], &["eq", "audio", "effect"]),
    (&["loudness", "lufs", "normalize", "level", "levels"], &["loudness", "normalize"]),
    (&["beat", "beats", "bpm", "rhythm", "tempo"], &["beat"]),
    (&["cut", "chop"], &["split", "trim", "cut"]),
    (&["split", "razor", "blade"], &["split"]),
    (&["delete", "remove", "erase", "drop", "get rid"], &["delete", "remove"]),
    (&["gap", "gaps", "hole", "holes", "space"], &["gap", "close"]),
    (&["move", "nudge", "drag", "later", "earlier"], &["move", "slide", "start"]),
    (&["shorten", "shorter", "longer", "lengthen", "extend"], &["trim", "duration"]),
    (&["duplicate", "copy", "clone"], &["duplicate", "copy"]),
    (&["loop", "repeat", "times"], &["loop"]),
    (&["freeze", "hold", "still"], &["freeze"]),
    (&["export", "render", "output", "deliver", "mp4", "mov", "h264", "video"], &["export"]),
    (&["thumbnail", "cover", "poster"], &["thumbnail"]),
    (&["scene", "scenes"], &["scene", "cut", "detect"]),
    (&["sync", "align", "synchronize"], &["sync"]),
    (&["multicam", "angle", "cameras"], &["multicam", "angle"]),
    (&["undo", "revert", "oops", "back"], &["undo"]),
    (&["save"], &["save", "project"]),
    (&["open", "load"], &["load", "project"]),
    (&["import", "bring", "file"], &["import", "media"]),
    (&["montage", "compilation", "highlight", "highlights"], &["montage", "best", "segments"]),
    (&["rough", "assemble", "assembly"], &["rough", "auto"]),
    (&["broll", "cutaway", "cutaways"], &["broll"]),
    (&["keyframe", "keyframes", "animate", "animation"], &["keyframe"]),
    (&["ease", "easing", "smooth", "smoother"], &["bezier", "ease", "keyframe"]),
    (&["wiggle", "jiggle", "shake"], &["wiggle", "modifier", "shake"]),
    (&["glow", "neon", "glowing", "bloom"], &["glow"]),
    (&["lens", "distortion"], &["undistort", "lens"]),
    (&["scope", "scopes", "waveform", "vectorscope", "histogram", "parade"], &["scope"]),
    (&["check", "review", "mistake", "mistakes", "error", "errors", "problem", "problems", "qa", "lint", "wrong"], &["review", "lint", "check"]),
    (&["see", "view", "preview", "screenshot", "frame", "show"], &["render", "frame", "look"]),
    (&["chapter", "chapters"], &["chapter"]),
    (&["replace", "swap", "relink"], &["replace"]),
    (&["mute", "unmute"], &["mute", "track"]),
    (&["layer", "lane"], &["track"]),
    (&["resolution", "4k", "1080p", "720p", "canvas", "size", "fps"], &["project", "width", "height", "canvas"]),
    (&["chart", "graph", "csv", "data"], &["chart"]),
    (&["counter", "number", "numbers", "counting"], &["counter"]),
    (&["roto", "rotoscope", "rotoscoping"], &["roto", "matte", "mask"]),
    (&["long", "length", "duration", "runtime"], &["duration", "summary", "stats"]),
    (&["clips", "timeline", "overview", "contents"], &["timeline", "summary"]),
    (&["particles", "particle", "snow", "rain", "sparks", "embers", "dust", "confetti"], &["particles", "emitter"]),
    (&["impact", "hit", "earthquake"], &["shake", "camera"]),
    (&["3d", "three"], &["scene3d", "camera"]),
    (&["dolly", "vertigo"], &["vertigo", "zoom"]),
    (&["typewriter", "typing"], &["animator", "prompt", "type"]),
    (&["curve", "curved", "arc"], &["path", "curve"]),
    (&["letter", "letters", "character", "characters"], &["animator", "text"]),
    (&["punch", "punchin"], &["punch"]),
    (&["um", "ums", "uh", "uhs", "filler", "stutter", "disfluency"], &["filler"]),
    (&["pip", "inset"], &["pip"]),
    (&["best", "pick", "choose"], &["best", "pick", "score"]),
    (&["exactly", "fit"], &["fit", "duration", "stretch"]),
    (&["transcript", "words", "word"], &["transcript", "word"]),
    (&["misheard", "typo"], &["edit", "fix"]),
    (&["correct", "correction", "corrected"], &["grade", "color", "auto"]),
    (&["automatic", "automatically", "auto", "automate"], &["auto"]),
    (&["bigger", "larger", "smaller", "size"], &["size", "style", "scale"]),
    (&["blur", "blurry"], &["blur", "mask"]),
    (&["follow", "stick", "attach", "track", "tracking", "moving"], &["track", "pin"]),
];

fn spell(w: &str) -> &str { SPELL.iter().find(|(a, _)| *a == w).map(|(_, b)| *b).unwrap_or(w) }

/// Light stemming, the same for tools and queries: plurals and -ing. Anything finer is left to prefix matching.
pub fn stem(w: &str) -> String {
    let w = w.to_lowercase();
    let n = w.len();
    if n > 4 && w.ends_with("ies") { return format!("{}y", &w[..n - 3]); }
    if n > 5 && (w.ends_with("ches") || w.ends_with("shes") || w.ends_with("xes") || w.ends_with("sses")) { return w[..n - 2].to_string(); }
    if n > 3 && w.ends_with('s') && !w.ends_with("ss") && !w.ends_with("us") && !w.ends_with("is") { return w[..n - 1].to_string(); }
    if n > 5 && w.ends_with("ing") { let mut s = w[..n - 3].to_string(); let b = s.as_bytes(); if b.len() > 2 && b[b.len() - 1] == b[b.len() - 2] && !matches!(b[b.len() - 1], b'l' | b's' | b'z') { s.pop(); } return s; }
    w
}

fn words(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(|w| w.to_lowercase()).collect()
}
fn doc_terms(text: &str) -> Vec<String> { words(text).iter().map(|w| stem(spell(w))).filter(|w| !STOP.contains(&w.as_str())).collect() }

fn first_sentence(d: &str) -> &str { let e = d.find(". ").map(|i| i + 1).unwrap_or(d.len()); &d[..e] }

/// The everyday tools (most edits) get a small lift; expert/raw tools (pipelines, buffers, reference renderers) a small drop.
pub fn prior(name: &str) -> f64 {
    if crate::toolgroups::CORE.contains(&name) || crate::toolgroups::DIRECTOR.iter().any(|(n, _)| *n == name) { return 1.25; }
    let expert = name.ends_with("_raw") || name.starts_with("inspect_") || name.starts_with("sample_") || name.starts_with("gpu") || name.starts_with("evaluate_")
        || name.starts_with("audit_") || name.starts_with("validate_hdr") || name.starts_with("query_") || name.starts_with("transform_")
        || ["vertex_stream", "ocio", "aces", "sdf", "bvh", "hlg", "mesh_", "_mesh", "wgpu", "float", "high_bit", "grade_graph", "grade_node", "shared_grade",
            "_lut", "asc_cdl", "effect_uv", "scene3d_", "homography", "residual", "lk_", "ecc_", "corner_disagreement", "error_field", "film_response",
            "color_pipeline", "color_delivery", "color_frame", "linear", "hdr", "camera_clip", "camera_project", "particle_simulate", "gltf", "cache_"]
            .iter().any(|p| name.contains(p));
    if expert { 0.6 } else { 1.0 }
}

const FIELDS: usize = 4; // name, argument names, first sentence, full description
const WEIGHT: [f64; FIELDS] = [6.0, 1.6, 2.4, 1.0];
const B: [f64; FIELDS] = [0.3, 0.5, 0.6, 0.75];
const K1: f64 = 1.4;
const NAME_BONUS: f64 = 3.0;

struct Doc { tf: [HashMap<String, f64>; FIELDS], len: [f64; FIELDS] }
struct Index { names: Vec<String>, docs: Vec<Doc>, df: HashMap<String, usize>, avg: [f64; FIELDS], vocab: Vec<String> }

/// The registry's index, built once.
fn index() -> &'static Index {
    static IX: OnceLock<Index> = OnceLock::new();
    IX.get_or_init(|| build(registry_ref().iter().map(|c| {
        let args: String = c.schema.get("properties").and_then(|p| p.as_object()).map(|o| o.keys().cloned().collect::<Vec<_>>().join(" ")).unwrap_or_default();
        (c.name.to_string(), args, c.description.to_string())
    }).collect()))
}

/// Index any list of (name, argument names, description): the registry, or the effect/transition catalogs.
fn build(items: Vec<(String, String, String)>) -> Index {
    let mut docs = vec![]; let mut names = vec![]; let mut df: HashMap<String, usize> = HashMap::new(); let mut tot = [0f64; FIELDS];
    for (name, args, desc) in items {
        let fields = [doc_terms(&name), doc_terms(&args), doc_terms(first_sentence(&desc)), doc_terms(&desc)];
        let mut tf: [HashMap<String, f64>; FIELDS] = Default::default(); let mut len = [0f64; FIELDS]; let mut seen: HashSet<String> = HashSet::new();
        for (f, ts) in fields.iter().enumerate() { len[f] = ts.len() as f64; tot[f] += len[f]; for t in ts { *tf[f].entry(t.clone()).or_default() += 1.0; seen.insert(t.clone()); } }
        for t in seen { *df.entry(t).or_default() += 1; }
        docs.push(Doc { tf, len }); names.push(name);
    }
    let n = docs.len().max(1) as f64;
    let avg = [tot[0] / n, tot[1] / n, tot[2] / n, tot[3] / n];
    let mut vocab: Vec<String> = df.keys().cloned().collect(); vocab.sort();
    Index { names, docs, df, avg, vocab }
}

/// BM25F of every document of `ix` against weighted query terms; (document index, score) for documents that match.
fn score_all(ix: &Index, terms: &[(String, f64)]) -> Vec<(usize, f64)> {
    let n = ix.docs.len() as f64;
    ix.docs.iter().enumerate().filter_map(|(i, d)| {
        let mut sc = 0.0;
        for (t, qw) in terms {
            let Some(&df) = ix.df.get(t) else { continue };
            let idf = ((n - df as f64 + 0.5) / (df as f64 + 0.5) + 1.0).ln();
            let mut tfw = 0.0;
            for f in 0..FIELDS { if let Some(&tf) = d.tf[f].get(t) { tfw += WEIGHT[f] * tf / (1.0 - B[f] + B[f] * d.len[f] / ix.avg[f].max(1.0)); } }
            if tfw > 0.0 { sc += qw * idf * tfw * (K1 + 1.0) / (tfw + K1); }
        }
        // a query word in the name itself is the strongest single sign of the right tool, whatever its idf
        for (t, qw) in terms { if d.tf[0].contains_key(t) { sc += NAME_BONUS * qw; } }
        (sc > 0.0).then_some((i, sc))
    }).collect()
}

/// Effects and transitions ranked against a query with the same reading of it as the tools: (type, kind, summary, score).
pub fn rank_catalog(q: &str, effects: &Value, transitions: &Value) -> Vec<(&'static str, String, String, f64)> {
    let mut items = vec![]; let mut kinds = vec![];
    for e in effects.as_array().into_iter().flatten() {
        let kind = e["kind"].as_str().unwrap_or("").to_string(); if kind.is_empty() { continue; }
        let params: String = e["params"].as_array().into_iter().flatten().filter_map(|p| p["name"].as_str()).collect::<Vec<_>>().join(" ");
        let d = e["description"].as_str().unwrap_or("").to_string();
        items.push((kind.clone(), params, d.clone())); kinds.push(("effect", kind, d));
    }
    for t in transitions.as_array().into_iter().flatten() {
        let kind = t.as_str().map(String::from).or_else(|| t["kind"].as_str().map(String::from)).unwrap_or_default(); if kind.is_empty() { continue; }
        let d = t["description"].as_str().map(String::from).unwrap_or_else(|| format!("{} transition", kind.replace('_', " ")));
        items.push((kind.clone(), String::new(), d.clone())); kinds.push(("transition", kind, d));
    }
    if items.is_empty() { return vec![]; }
    let ix = build(items);
    let mut out: Vec<(&'static str, String, String, f64)> = score_all(&ix, &expand(q)).into_iter().map(|(i, sc)| { let (ty, k, d) = &kinds[i]; (*ty, k.clone(), d.clone(), sc) }).collect();
    out.sort_by(|a, b| b.3.total_cmp(&a.3)); out
}

/// Optimal string alignment distance (a swapped pair of letters is one edit), stopping early past `max`.
fn osa(a: &str, b: &str, max: usize) -> usize {
    let (a, b): (Vec<char>, Vec<char>) = (a.chars().collect(), b.chars().collect());
    if a.len().abs_diff(b.len()) > max { return max + 1; }
    let mut d = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 0..=a.len() { d[i][0] = i; } for j in 0..=b.len() { d[0][j] = j; }
    for i in 1..=a.len() { let mut row_min = usize::MAX; for j in 1..=b.len() {
        let cost = usize::from(a[i - 1] != b[j - 1]);
        let mut v = (d[i - 1][j] + 1).min(d[i][j - 1] + 1).min(d[i - 1][j - 1] + cost);
        if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] { v = v.min(d[i - 2][j - 2] + 1); }
        d[i][j] = v; row_min = row_min.min(v); }
        if row_min > max { return max + 1; } }
    d[a.len()][b.len()]
}

/// A query becomes weighted terms: its own words (1.0), the vocabulary's reading of them (0.7), close spellings of words
/// no tool uses (0.8), and other forms of the same word (0.6, shared prefix of at least 4 letters).
pub fn expand(q: &str) -> Vec<(String, f64)> {
    let ix = index();
    // words only (keeping & for "b&w"), single spaces, padded so a phrase matches whole words
    let clean: String = q.to_lowercase().chars().map(|c| if c.is_alphanumeric() || c == '&' { c } else { ' ' }).collect();
    let mut text = format!(" {} ", clean.split_whitespace().collect::<Vec<_>>().join(" "));
    for (p, r) in PHRASES { let pp = format!(" {} ", p.replace('-', " ")); if text.contains(&pp) { text = text.replace(&pp, &format!(" {r} ")); } }
    for (p, r) in [(" 9 16 ", " vertical reframe aspect "), (" 16 9 ", " aspect reframe "), (" 1 1 ", " square aspect reframe ")] { text = text.replace(p, r); }
    let mut out: HashMap<String, f64> = HashMap::new();
    let add = |t: String, w: f64, out: &mut HashMap<String, f64>| { if t.is_empty() || STOP.contains(&t.as_str()) { return; } let e = out.entry(t).or_insert(0.0); if w > *e { *e = w; } };
    let pre4 = |x: &str| -> String { x.chars().take(4).collect() };
    let raw: Vec<String> = words(&text).into_iter().flat_map(|w| spell(&w).split(' ').map(String::from).collect::<Vec<_>>()).collect();
    for w in &raw {
        if STOP.contains(&w.as_str()) { continue; }
        let s = stem(w);
        let known = ix.df.contains_key(&s);
        let mut base = vec![s.clone()];
        if !known && s.len() >= 5 {
            let max = if s.len() >= 8 { 2 } else { 1 };
            let mut best: Option<(usize, usize, &String)> = None;
            for v in &ix.vocab { if v.len() < 4 { continue; } let d = osa(&s, v, max); if d <= max { let df = ix.df[v]; if best.map_or(true, |(bd, bdf, _)| d < bd || (d == bd && df > bdf)) { best = Some((d, df, v)); } } }
            if let Some((_, _, v)) = best { add(v.clone(), 0.8, &mut out); base.push(v.clone()); }
        }
        add(s.clone(), 1.0, &mut out);
        for b in &base { for (keys, vals) in SYN { if keys.iter().any(|k| stem(spell(k)) == *b || *k == w.as_str()) { for v in *vals { add(stem(v), 0.7, &mut out); } } } }
        if s.chars().count() >= 4 { let p = pre4(&s); for v in &ix.vocab { if v != &s && v.chars().count() >= 4 && pre4(v) == p && (v.starts_with(s.as_str()) || s.starts_with(v.as_str())) && v.len().abs_diff(s.len()) <= 3 { add(v.clone(), 0.6, &mut out); } } }
    }
    let mut v: Vec<(String, f64)> = out.into_iter().collect(); v.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0))); v
}

/// Ranked tools for a query: (name, score), best first.
pub fn rank(q: &str) -> Vec<(&'static str, f64)> {
    let ix = index(); let terms = expand(q); let reg = registry_ref();
    let exact = q.trim().to_lowercase().replace([' ', '-'], "_");
    let mut out: Vec<(&'static str, f64)> = score_all(ix, &terms).into_iter().map(|(i, mut sc)| {
        let name = reg[i].name;
        if name == exact { sc *= 3.0; }
        (name, sc * prior(name))
    }).collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(b.0)));
    out
}

fn type_of(v: &Value) -> String {
    if let Some(e) = v.get("enum").and_then(|e| e.as_array()) { let s: Vec<String> = e.iter().take(8).map(|x| x.as_str().map(String::from).unwrap_or_else(|| x.to_string())).collect(); return s.join("|") + if e.len() > 8 { "|…" } else { "" }; }
    match v.get("type") {
        Some(Value::String(t)) if t == "array" => { let it = v.get("items").map(type_of).unwrap_or_default(); if it.is_empty() || it == "any" { "array".into() } else { format!("[{it}]") } }
        Some(Value::String(t)) => t.clone(),
        Some(Value::Array(ts)) => ts.iter().filter_map(|t| t.as_str()).collect::<Vec<_>>().join("|"),
        _ => "any".into(),
    }
}
fn clip(s: &str, max: usize) -> String { if s.chars().count() <= max { return s.to_string(); } let t: String = s.chars().take(max).collect(); let cut = t.rfind(' ').unwrap_or(t.len()); format!("{}…", t[..cut].trim_end_matches([',', ';', ' '])) }

/// Compact argument signature, required first: ["clip_id: string (required)", "volume: number — 1 = unchanged", …].
/// Enough to make a correct call without a describe round trip; kimo_describe still has the full schema.
pub fn signature(c: &Command) -> Vec<String> {
    let req: Vec<&str> = c.schema.get("required").and_then(|r| r.as_array()).map(|a| a.iter().filter_map(|x| x.as_str()).collect()).unwrap_or_default();
    let Some(p) = c.schema.get("properties").and_then(|p| p.as_object()) else { return vec![] };
    let line = |k: &str, v: &Value, r: bool| { let d = v.get("description").and_then(|d| d.as_str()).map(|d| format!(" — {}", clip(d, 64))).unwrap_or_default(); format!("{k}: {}{}{d}", type_of(v), if r { " (required)" } else { "" }) };
    let mut out: Vec<String> = req.iter().filter_map(|k| p.get(*k).map(|v| line(k, v, true))).collect();
    out.extend(p.iter().filter(|(k, _)| !req.contains(&k.as_str())).map(|(k, v)| line(k, v, false)));
    out
}
pub fn signature_of(name: &str) -> Option<Vec<String>> { registry_ref().iter().find(|c| c.name == name).map(signature) }

/// Closest tool names to a mistyped or guessed one (edit distance, then shared words), best first.
pub fn near_names(name: &str, n: usize) -> Vec<&'static str> {
    let q = name.to_lowercase();
    let mut v: Vec<(usize, &'static str)> = registry_ref().iter().map(|c| { let d = osa(&q, c.name, 4); let shared = q.split('_').filter(|w| w.len() > 2 && c.name.split('_').any(|x| x == *w)).count(); (d.saturating_sub(shared * 2), c.name) }).collect();
    v.sort(); let close: Vec<&'static str> = v.iter().filter(|(d, _)| *d <= 3).take(n).map(|x| x.1).collect();
    if !close.is_empty() { return close; }
    rank(&q.replace('_', " ")).into_iter().take(n).map(|x| x.0).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test] fn stems_agree() { assert_eq!(stem("captions"), "caption"); assert_eq!(stem("cutting"), "cut"); assert_eq!(stem("matches"), "match"); assert_eq!(stem("speed"), "speed"); assert_eq!(stem("press"), "press"); }
    #[test] fn typo_finds_the_word() { let t = expand("trasition"); assert!(t.iter().any(|(w, _)| w == "transition"), "{t:?}"); }
    #[test] fn phrase_beats_words() { let r = rank("slow motion"); assert!(r.iter().take(3).any(|(n, _)| n.contains("speed")), "{:?}", &r[..5.min(r.len())]); }
    #[test] fn exact_name_is_first() { assert_eq!(rank("split_clip").first().map(|x| x.0), Some("split_clip")); }
    #[test] fn signature_puts_required_first() { let s = signature_of("move_clip").unwrap(); assert!(s[0].contains("(required)"), "{s:?}"); }
    #[test] fn near_names_fix_a_typo() { assert_eq!(near_names("spilt_clip", 1), vec!["split_clip"]); }
}
