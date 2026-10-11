//! kimo-core: project model, keyframes, command registry, CPU renderer.
//! Every user-visible operation is a registered Command; the MCP server
//! is generated from the registry, so nothing can exist without a tool.
pub mod ext;
pub mod trackmath;
pub mod shapemods;
pub mod paths;
pub mod layerstyles;
pub mod lighting;
pub mod sfm;
pub mod matting;
pub mod matte;
pub mod keyframe;
pub mod hud;
pub mod flow;
pub mod planar;
pub mod motion;
pub mod masks;
pub mod confetti;
pub mod shards;
pub mod shard_look;
pub mod scene3d;
pub mod physics;
pub mod presets;
pub mod three_d;
pub mod glass;
pub mod model;
pub mod commands;
pub mod render;
pub mod effects;
pub mod text;
pub mod particles;
pub mod titles;
pub mod transitions;
pub mod audio;
pub mod tracking; pub mod roto;
pub mod edit;
pub mod effects2;
pub mod color;
pub mod keying;
pub mod colortools;
pub mod subject_preview;

pub mod guard;
pub mod speedlines;
pub mod par;
pub mod fracture;

pub mod long_gap;

pub mod editorial;

pub mod timecode;
