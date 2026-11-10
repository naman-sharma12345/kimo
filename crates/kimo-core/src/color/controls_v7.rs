//! Expression-control effects (FEATURES.md L473, After Effects "Expression Controls"): a value with no
//! visual effect on the frame by itself, meant to be read by another property's expression -- a
//! set_expression_script link("clip:effect_id.param") call, or a direct keyframe/animate_from_data binding
//! that treats this control's own param as just another animatable number. apply() is a pure identity (the
//! frame is untouched) for every kind here: the point of a control is to be a named, keyframeable value
//! sitting on a clip, not to render anything.
//!
//! Six of the seven AE expression controls are shipped as plain numeric params (fully keyframeable,
//! animate_from_data-able, essential-properties-taggable, link()-able -- they get everything for free by
//! being ordinary effect params). layer_control is the honest gap: it would hold a REFERENCE to another
//! clip rather than a number, and link("clip:property") already takes a clip reference directly as its own
//! argument, so a dedicated layer_control effect would duplicate that with no new capability -- not shipped.
//!
//! A control's param, like every effect param in this project, is unset until something writes it:
//! add_effect does not pre-populate a kind's catalog defaults into the clip, it only stores whatever
//! "params" the caller gives it. A fresh control is readable immediately through set_keyframe (which
//! creates the Prop on first use) but NOT through sample_property until a value exists -- pass the
//! documented default as add_effect's own "params" to make it readable right away, same as any effect.
use crate::effects::{Frame, Params};

type C = (&'static str, f64, &'static str);
fn e(kind: &'static str, desc: &'static str, params: &'static [C]) -> super::Entry { (kind, desc, params) }

const SLIDER: &[C] = &[("value", 0.0, "the exposed number -- no inherent range, interpret it however your expression wants")];
const CHECKBOX: &[C] = &[("value", 0.0, "0 (off) or 1 (on); any nonzero reads as on")];
const ANGLE: &[C] = &[("value", 0.0, "degrees")];
const POINT: &[C] = &[("x", 0.0, ""), ("y", 0.0, "")];
const COLOR: &[C] = &[("r", 1.0, "0..1"), ("g", 1.0, "0..1"), ("b", 1.0, "0..1"), ("a", 1.0, "0..1")];
const DROPDOWN: &[C] = &[("index", 0.0, "0-based selection; option labels are not stored here, track them in your own notes or clip tags")];

pub fn catalog() -> Vec<super::Entry> { vec![
    e("slider_control", "Expression control (FEATURES.md L473): a number with no visual effect by itself, read from another property's expression via link(\"clip:effect_id.value\") or sampled directly -- After Effects' Slider Control.", SLIDER),
    e("checkbox_control", "Expression control: a 0/1 toggle with no visual effect by itself, read the same way as slider_control -- After Effects' Checkbox Control.", CHECKBOX),
    e("angle_control", "Expression control: a degrees value with no visual effect by itself -- After Effects' Angle Control.", ANGLE),
    e("point_control", "Expression control: an (x, y) pair with no visual effect by itself, read per-axis (effect_id.x / effect_id.y) -- After Effects' Point Control.", POINT),
    e("color_control", "Expression control: an RGBA color with no visual effect by itself, read per-channel (effect_id.r / .g / .b / .a) -- After Effects' Color Control.", COLOR),
    e("dropdown_control", "Expression control: a numeric selection index with no visual effect by itself -- After Effects' Dropdown Menu Control, minus the stored option labels (track those yourself).", DROPDOWN),
] }

pub fn apply(kind: &str, _pr: &Params, _f: &mut Frame, _t: f64, _seed: u64) -> Result<(), String> {
    match kind {
        "slider_control" | "checkbox_control" | "angle_control" | "point_control" | "color_control" | "dropdown_control" => Ok(()),
        other => Err(format!("controls_v7::apply: unknown kind '{other}'")),
    }
}
