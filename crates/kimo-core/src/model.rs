use crate::keyframe::Prop;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Source {
    /// Media file decoded through ffmpeg.
    Media { asset: String },
    /// Flat color generator.
    Solid { color: [f64; 3] },
    /// Procedural gradient generator (c1 -> c2 vertical).
    Gradient { c1: [f64; 3], c2: [f64; 3] },
    /// Rendered text (TTF/OTF via ab_glyph). Animate with the `reveal` prop (typewriter) plus transform props.
    /// Procedural particles: kind sparks|snow|dust|embers|rain. Deterministic per seed.
    Emitter(crate::particles::Emitter),
    /// 3D glass shard burst (see shards.rs).
    ShardBurst(crate::shards::ShardBurst),
    SpeedLines(crate::speedlines::SpeedLines),
    GlitchConfetti(crate::confetti::GlitchConfetti),
    Scene3D(crate::scene3d::Scene3D),
    Particles { kind: String, count: u32, color: [f64; 3], size: f64, seed: u64 },
    /// Adjustment layer: its effects/mask/opacity apply to everything composited beneath it.
    Adjustment,
    /// Nested composition: its own tracks on a local clock (clip start = 0). Transform, effects, masks and opacity apply to the whole group.
    Precomp { tracks: Vec<Track> },
    /// Vector shape: rect | ellipse | polygon (sides) | line. Sizes in pixels, centered on canvas (offset with x/y props).
    Shape { shape: String, width: f64, height: f64, fill: [f64; 3], stroke_width: f64, stroke_color: [f64; 3], sides: u32, corner_radius: f64 },
    Text { text: String, font: String, size: f64, color: [f64; 3], letter_spacing: f64, stroke_width: f64, stroke_color: [f64; 3], shadow: f64, align: String },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Mask {
    /// rect | ellipse | linear (left-to-right gradient edge) ; coordinates normalized 0..1 of canvas.
    pub shape: String,
    pub props: BTreeMap<String, Prop>, // cx, cy, w, h, feather, rotation
    pub invert: bool,
    /// Full mask stack (see masks.rs). The legacy single shape above applies first when shape != "stack".
    #[serde(default)] pub layers: Vec<crate::masks::MaskLayer>,
    /// Track matte: another clip's alpha/luma gates this clip.
    #[serde(default)] pub matte: Option<crate::masks::Matte>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Effect {
    pub id: String,
    pub kind: String,
    pub params: BTreeMap<String, Prop>,
    #[serde(default)]
    pub strings: BTreeMap<String, String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Clip {
    pub id: String,
    pub source: Source,
    /// Position on the timeline, seconds.
    pub start: f64,
    pub duration: f64,
    /// Offset into the source media, seconds.
    pub in_point: f64,
    pub speed: f64,
    /// Optional speed ramp (multiplier over clip-local time); overrides `speed` for video.
    #[serde(default)]
    pub speed_curve: Option<Prop>,
    pub props: BTreeMap<String, Prop>, // opacity, x, y, scale, rotation
    pub effects: Vec<Effect>,
    #[serde(default)] pub grade_graph: Option<crate::color::node_graph::Graph>,
    pub blend: String,
    #[serde(default)]
    pub mask: Option<Mask>,
    #[serde(default)]
    pub time_mode: TimeMode,
    #[serde(default)]
    pub corners: Option<[[f64; 2]; 4]>,
    #[serde(default)]
    pub audio: Audio,
    /// Motion blur shutter angle in degrees (0 = off, 180 = film standard).
    #[serde(default)]
    pub motion_blur: f64,
    /// Echo / temporal smear (AE Echo, CC Wide Time, frame blending).
    #[serde(default)]
    pub echo: Option<Echo>,
    #[serde(default)] pub motion_path: Option<crate::motion::MotionPath>,
    /// Posterize time: quantize this clip's local time to N fps (0 = off).
    #[serde(default)]
    pub posterize_fps: f64,
    /// Rich text styling and per-character animators (text clips only).
    #[serde(default)]
    pub text_fx: Option<TextFx>,
    /// Transition INTO this clip from the previous clip on the track.
    pub transition_in: Option<Transition>,
}

#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum TimeMode { #[default] Normal, Reverse, /// hold the frame at clip-local time `at`
    Freeze { at: f64 } }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Audio { #[serde(default)] pub effects: Vec<crate::editorial::AudioEffect>, #[serde(default)] pub pan: f64, #[serde(default = "unity_prop")] pub duck_gain: Prop, pub mute: bool, pub volume: Prop, pub fade_in: f64, pub fade_out: f64, pub pitch: f64, pub highpass: f64, pub lowpass: f64, pub denoise: f64, pub compress: bool, pub echo: f64, pub reverse: bool }
fn unity_prop() -> Prop { Prop::new(1.0) }
impl Default for Audio { fn default() -> Self { Audio { effects: vec![], pan: 0.0, duck_gain: Prop::new(1.0), mute: false, volume: Prop::new(1.0), fade_in: 0.0, fade_out: 0.0, pitch: 0.0, highpass: 0.0, lowpass: 0.0, denoise: 0.0, compress: false, echo: 0.0, reverse: false } } }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transition { pub kind: String, pub duration: f64,
    #[serde(default = "d_ease")] pub ease: String, #[serde(default = "d_soft")] pub softness: f64, #[serde(default)] pub angle: f64,
    #[serde(default)] pub color: [f64; 3], #[serde(default = "d_one")] pub intensity: f64, #[serde(default = "d_count")] pub count: f64, #[serde(default)] pub seed: u32 }
fn d_ease() -> String { "in_out".into() } fn d_soft() -> f64 { 0.08 } fn d_one() -> f64 { 1.0 } fn d_count() -> f64 { 10.0 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Track { pub id: String, pub clips: Vec<Clip>, #[serde(default)] pub locked: bool, #[serde(default)] pub muted: bool, #[serde(default)] pub name: String }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Asset { pub id: String, pub path: String, pub width: u32, pub height: u32, pub duration: f64, pub fps: f64 }

/// Persisted auto-occluder anchor recipe: lets a reloaded project rebuild the surface reference frame from the source media.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OccSpec { pub src: String, pub src_start: f64, pub path: String, pub src_time: f64, pub q0: [[f64; 2]; 4], pub soft: f32, pub chroma: f32, pub w: usize, pub h: usize, #[serde(default)] pub adaptive: bool, #[serde(default)] pub f0: usize, #[serde(default = "one_speed")] pub speed: f64, #[serde(default = "one_tol")] pub tol: f32, #[serde(default = "def_min_area")] pub min_area: f32, #[serde(default)] pub temporal: usize, #[serde(default)] pub thin: f32, #[serde(default)] pub close: f32, #[serde(default)] pub ramp: Vec<f64>, #[serde(default)] pub lgate: f32, #[serde(default)] pub expand: f32 }
fn def_min_area() -> f32 { 0.0 }
fn one_tol() -> f32 { 1.0 }
fn one_speed() -> f64 { 1.0 }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Project {
    #[serde(default)] pub grade_scopes: crate::color::grade_scopes::Scopes,
    #[serde(default)] pub editorial: crate::editorial::Editorial,
    #[serde(default)] pub lights: BTreeMap<String, crate::lighting::LightDef>,
    #[serde(default)] pub materials: BTreeMap<String, crate::lighting::Material>,
    #[serde(default)] pub occluders: BTreeMap<String, OccSpec>,
    #[serde(default)] pub ocio_view_delivery: Option<crate::color::view_video::Selection>,
    #[serde(default)] pub color_pipeline: crate::color::pipeline::Pipeline,
    #[serde(default)] pub camera: BTreeMap<String, crate::keyframe::Prop>,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub assets: BTreeMap<String, Asset>,
    /// Bottom to top.
    pub tracks: Vec<Track>,
    #[serde(skip)]
    pub next_id: u64,
    #[serde(default)]
    pub markers: Vec<Marker>,
}

impl Default for Project {
    fn default() -> Self {
        Project { grade_scopes: Default::default(), lights: BTreeMap::new(), materials: BTreeMap::new(), editorial: Default::default(), ocio_view_delivery: None, occluders: BTreeMap::new(), color_pipeline: Default::default(), camera: BTreeMap::new(), name: "untitled".into(), width: 1280, height: 720, fps: 30.0, assets: BTreeMap::new(), tracks: vec![], next_id: 1, markers: vec![] }
    }
}

impl Clip {
    /// Map clip-local time to the local time whose frame should be shown (reverse/freeze).
    pub fn display_time(&self, lt: f64) -> f64 { match self.time_mode { TimeMode::Normal => lt, TimeMode::Reverse => (self.duration - lt).max(0.0), TimeMode::Freeze { at } => at } }
    /// Source-media time consumed after `lt` seconds of clip time.
    /// Source frame index shown at clip time `lt`; the 1e-6 frame epsilon absorbs quadrature rounding (0.36*25 -> 8.9999999) so boundaries pick the intended frame.
    pub fn source_index(&self, lt: f64, fps: f64) -> usize { ((self.source_time(lt.max(0.0)) * fps) + 1e-6).floor() as usize }
    pub fn source_time(&self, lt: f64) -> f64 {
        match &self.speed_curve { None => lt * self.speed, Some(p) => { let n = (lt * 240.0).ceil().max(1.0) as usize; let dt = lt / n as f64; (0..n).map(|i| p.at((i as f64 + 0.5) * dt).max(0.0) * dt).sum() } }
    }
}

impl Project {
    /// Collision-safe id counter after loading: one past the largest trailing number in any id-like string (keys and string values), never below 1000.
    pub fn recompute_next_id(&mut self) {
        fn walk(v: &serde_json::Value, m: &mut u64) {
            let tail = |s: &str| { let d: String = s.chars().rev().take_while(|c| c.is_ascii_digit()).collect::<Vec<_>>().into_iter().rev().collect(); if d.is_empty() || d.len() > 12 || s.len() > 64 { 0 } else { d.parse::<u64>().unwrap_or(0) } };
            match v { serde_json::Value::String(s) => *m = (*m).max(tail(s)), serde_json::Value::Array(a) => a.iter().for_each(|x| walk(x, m)), serde_json::Value::Object(o) => for (k, x) in o { *m = (*m).max(tail(k)); walk(x, m); }, _ => {} }
        }
        let mut m = 999u64; if let Ok(v) = serde_json::to_value(&*self) { walk(&v, &mut m); } self.next_id = m + 1;
    }
    pub fn gen_id(&mut self, p: &str) -> String { let i = self.next_id; self.next_id += 1; format!("{p}{i}") }
    pub fn duration(&self) -> f64 {
        self.tracks.iter().flat_map(|t| t.clips.iter()).map(|c| c.start + c.duration).fold(0.0, f64::max)
    }
    pub fn find_clip_mut(&mut self, id: &str) -> Option<&mut Clip> {
        self.tracks.iter_mut().flat_map(|t| t.clips.iter_mut()).find(|c| c.id == id)
    }
    pub fn find_clip(&self, id: &str) -> Option<&Clip> {
        self.tracks.iter().flat_map(|t| t.clips.iter()).find(|c| c.id == id)
    }
}

pub fn default_props() -> BTreeMap<String, Prop> {
    let mut m = BTreeMap::new();
    m.insert("opacity".into(), Prop::new(1.0));
    m.insert("x".into(), Prop::new(0.0));      // pixels offset
    m.insert("y".into(), Prop::new(0.0));
    m.insert("scale".into(), Prop::new(1.0));
    m.insert("rotation".into(), Prop::new(0.0)); // degrees
    m.insert("reveal".into(), Prop::new(1.0)); // text typewriter 0..1
    m
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Marker { pub id: String, pub time: f64, pub duration: f64, pub name: String, pub color: String }

/// Rich text style + After Effects-style per-unit animators for text clips.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextFx {
    pub weight: f64, pub uppercase: bool, pub line_mask: bool, pub italic_skew: f64, pub box_width: f64, pub line_spacing: f64, pub kerning: bool,
    pub bg: Option<[f64; 4]>, pub bg_pad: f64, pub bg_radius: f64,
    pub shadow_blur: f64, pub shadow_opacity: f64, pub shadow_color: [f64; 3], pub shadow_angle: f64,
    pub gradient: Option<([f64; 3], [f64; 3])>, pub glow: f64, pub glow_color: [f64; 3],
    pub animators: Vec<TextAnimator>,
    #[serde(default)] pub path: Option<crate::motion::MotionPath>, #[serde(default = "zp")] pub path_start: Prop, #[serde(default)] pub path_flip: bool,
}
fn zp() -> Prop { Prop::new(0.0) }
impl Default for TextFx { fn default() -> Self { TextFx { weight: 0.0, uppercase: false, line_mask: false, italic_skew: 0.0, box_width: 0.0, line_spacing: 1.15, kerning: true, bg: None, bg_pad: 24.0, bg_radius: 12.0, shadow_blur: 0.0, shadow_opacity: 0.6, shadow_color: [0.0; 3], shadow_angle: 45.0, gradient: None, glow: 0.0, glow_color: [1.0, 1.0, 1.0], animators: vec![], path: None, path_start: Prop::new(0.0), path_flip: false } } }
/// Range selector + properties applied to each unit (char/word/line) by selector amount. All values keyframable.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TextAnimator {
    pub id: String, pub unit: String, pub shape: String,
    pub start: Prop, pub end: Prop, pub offset: Prop, pub smoothness: Prop,
    pub x: Prop, pub y: Prop, pub scale: Prop, pub rotation: Prop, pub opacity: Prop, pub tracking: Prop, pub blur: Prop,
    /// Selector order: false = reading order, true = pseudo-random (AE 'Randomize Order'), stable per `seed`.
    #[serde(default)] pub random: bool, #[serde(default)] pub seed: u64,
}

/// Echo ghosts: `count` earlier (or later when spacing<0) copies of the layer at `spacing` seconds apart.
/// mode: composite (ghosts fade by decay^k), add / screen (light trails), average (equal-weight frame blending = temporal smear).
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Echo { pub count: u32, pub spacing: f64, pub decay: f64, pub mode: String }
