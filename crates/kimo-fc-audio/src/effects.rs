// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit 25979b15a419d4457fb67ea5e10c1a15d2e56b62), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
//! Audio effects and their registry.

pub(crate) mod channel;
pub(crate) mod declick;
pub(crate) mod dynamics;
pub(crate) mod eq;
pub(crate) mod essential;
pub(crate) mod filters;
pub(crate) mod modulation;
pub(crate) mod multiband;
pub(crate) mod pitch;
pub(crate) mod restoration;
pub(crate) mod reverbs;
pub(crate) mod special;
mod stft;
pub(crate) mod time;
pub(crate) mod util;

pub(crate) use channel::{Amplify, Balance, ChannelVolume, Invert, Route, Routing};
pub(crate) use declick::ClickRemover;
pub(crate) use dynamics::Gate;
pub use dynamics::{Compressor, Limiter};
pub(crate) use eq::{ParametricEq, SimpleEq};
pub use essential::DeEsser;
pub(crate) use essential::{DeReverb, SpeechEnhance, StereoWidth};
pub(crate) use filters::{FftFilter, FullParametricEq, GraphicEq, NotchFilter, ScientificFilter};
pub(crate) use modulation::{AnalogDelay, ChorusFlanger, Flanger, MultitapDelay, Phaser};
pub(crate) use multiband::{DynamicsRack, MultibandCompressor, TubeCompressor};
pub use pitch::PitchShifter;
pub(crate) use restoration::{DeHum, DeNoise};
pub(crate) use reverbs::{ConvolutionReverb, SurroundReverb};
pub use special::LoudnessMeterFx;
pub(crate) use special::{AmbisonicsPanner, Binauralizer, ChannelMixer, Distortion, GuitarSuite, Mastering, Mute, StereoExpander, VocalEnhancer};
pub(crate) use time::{Delay, Reverb};

use crate::{AudioEffect, ParamSpec};

/// Effect category (for menus / the Effects panel's Audio Effects bin).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Category {
    Filter,
    Dynamics,
    Time,
    Channel,
    Restoration,
    Pitch,
    Modulation,
    Reverb,
    Special,
}

/// Registry entry describing one effect type.
#[derive(Clone, Copy)]
pub struct EffectInfo {
    pub id: &'static str,
    pub name: &'static str,
    pub category: Category,
    pub params: &'static [ParamSpec],
    /// Construct an instance for `(sample_rate, channels)`.
    pub create: fn(f32, usize) -> Box<dyn AudioEffect>,
}

impl std::fmt::Debug for EffectInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectInfo")
            .field("id", &self.id)
            .field("name", &self.name)
            .field("category", &self.category)
            .field("params", &self.params.len())
            .finish()
    }
}

macro_rules! entry {
    ($id:literal, $name:literal, $cat:ident, $ty:ty) => {
        EffectInfo { id: $id, name: $name, category: Category::$cat, params: <$ty>::PARAMS, create: |sr, ch| Box::new(<$ty>::new(sr, ch)) }
    };
}

static REGISTRY: &[EffectInfo] = &[
    entry!("parametric_eq", "Parametric Equalizer", Filter, ParametricEq),
    entry!("simple_eq", "Simple 3-Band EQ", Filter, SimpleEq),
    entry!("compressor", "Compressor", Dynamics, Compressor),
    entry!("gate", "Expander / Gate", Dynamics, Gate),
    entry!("limiter", "Hard Limiter", Dynamics, Limiter),
    entry!("delay", "Delay", Time, Delay),
    entry!("reverb", "Reverb", Time, Reverb),
    entry!("amplify", "Amplify", Channel, Amplify),
    entry!("channel_volume", "Channel Volume", Channel, ChannelVolume),
    entry!("balance", "Balance / Pan", Channel, Balance),
    entry!("invert", "Invert", Channel, Invert),
    EffectInfo {
        id: "swap_channels",
        name: "Swap Channels",
        category: Category::Channel,
        params: Route::PARAMS,
        create: |_, _| Box::new(Route::new(Routing::Swap)),
    },
    EffectInfo {
        id: "fill_left",
        name: "Fill Left with Right",
        category: Category::Channel,
        params: Route::PARAMS,
        create: |_, _| Box::new(Route::new(Routing::FillLeft)),
    },
    EffectInfo {
        id: "fill_right",
        name: "Fill Right with Left",
        category: Category::Channel,
        params: Route::PARAMS,
        create: |_, _| Box::new(Route::new(Routing::FillRight)),
    },
    entry!("dehum", "DeHum", Restoration, DeHum),
    entry!("denoise", "DeNoise", Restoration, DeNoise),
    entry!("pitch_shifter", "Pitch Shifter", Pitch, PitchShifter),
    entry!("deesser", "DeEsser", Restoration, DeEsser),
    entry!("dereverb", "DeReverb", Restoration, DeReverb),
    entry!("speech_enhance", "Enhance Speech", Filter, SpeechEnhance),
    entry!("stereo_width", "Stereo Width", Channel, StereoWidth),
    // Premiere audio-effect set (M7.7)
    entry!("graphic_eq_10", "Graphic Equalizer (10 Bands)", Filter, GraphicEq<10>),
    entry!("graphic_eq_20", "Graphic Equalizer (20 Bands)", Filter, GraphicEq<20>),
    entry!("graphic_eq_30", "Graphic Equalizer (30 Bands)", Filter, GraphicEq<30>),
    entry!("parametric_eq_full", "Parametric Equalizer (full)", Filter, FullParametricEq),
    entry!("notch_filter", "Notch Filter", Filter, NotchFilter),
    entry!("scientific_filter", "Scientific Filter", Filter, ScientificFilter),
    entry!("fft_filter", "FFT Filter", Filter, FftFilter),
    entry!("dynamics_rack", "Dynamics", Dynamics, DynamicsRack),
    entry!("multiband_compressor", "Multiband Compressor", Dynamics, MultibandCompressor),
    entry!("tube_compressor", "Tube-modeled Compressor", Dynamics, TubeCompressor),
    entry!("chorus_flanger", "Chorus/Flanger", Modulation, ChorusFlanger),
    entry!("flanger", "Flanger", Modulation, Flanger),
    entry!("phaser", "Phaser", Modulation, Phaser),
    entry!("analog_delay", "Analog Delay", Time, AnalogDelay),
    entry!("multitap_delay", "Multitap Delay", Time, MultitapDelay),
    entry!("convolution_reverb", "Convolution Reverb", Reverb, ConvolutionReverb),
    entry!("surround_reverb", "Surround Reverb", Reverb, SurroundReverb),
    entry!("click_remover", "Automatic Click Remover", Restoration, ClickRemover),
    entry!("channel_mixer", "Channel Mixer", Channel, ChannelMixer),
    entry!("distortion", "Distortion", Special, Distortion),
    entry!("guitar_suite", "GuitarSuite", Special, GuitarSuite),
    entry!("mastering", "Mastering", Special, Mastering),
    entry!("vocal_enhancer", "Vocal Enhancer", Special, VocalEnhancer),
    entry!("stereo_expander", "Stereo Expander", Channel, StereoExpander),
    entry!("binauralizer", "Binauralizer - Ambisonics", Special, Binauralizer),
    entry!("ambisonics_panner", "Panner - Ambisonics", Special, AmbisonicsPanner),
    entry!("loudness_meter", "Loudness Meter", Special, LoudnessMeterFx),
    entry!("mute", "Mute", Channel, Mute),
];

/// All registered effects.
pub fn effects() -> &'static [EffectInfo] {
    REGISTRY
}

/// Look up an effect by id.
pub fn effect_info(id: &str) -> Option<&'static EffectInfo> {
    REGISTRY.iter().find(|e| e.id == id)
}

/// Instantiate an effect by id.
pub fn create_effect(id: &str, sample_rate: f32, channels: usize) -> Option<Box<dyn AudioEffect>> {
    effect_info(id).map(|e| (e.create)(sample_rate, channels))
}



