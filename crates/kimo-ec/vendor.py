"""Build crates/kimo-ec from a checkout of EffectCraft (MIT OR Apache-2.0).

Usage: EFFECTCRAFT_SRC=/path/to/effectcraft python crates/kimo-ec/vendor.py

Copies the effect modules listed in FX plus the colour, geometry, raster and blur code they use,
rewriting only crate paths, and writes a trimmed lib.rs with the shared effect types.
Re-run after editing FX; it overwrites crates/kimo-ec/src.
"""
import os, re, shutil, sys

SRC = os.environ.get("EFFECTCRAFT_SRC") or sys.exit("set EFFECTCRAFT_SRC to an EffectCraft checkout")
DST = os.path.dirname(os.path.abspath(__file__))
REV = os.popen(f'git -C "{SRC}" rev-parse HEAD').read().strip()

FX = [
    "util", "noise", "generate", "keying2", "channel2", "stylize2", "stylize3", "noise2", "noise3", "generate2",
    "perspective2", "distort2", "distort3", "distort4", "blur2", "color2", "color3", "transition2",
    "color_fx", "utility", "file_cache", "generate3", "transition", "sim", "sim2", "card3d", "psim", "ocio", "ocio_config", "sim3",
]

def read(p): return open(os.path.join(SRC, p), encoding="utf-8").read()
def write(p, s):
    p = os.path.join(DST, p); os.makedirs(os.path.dirname(p), exist_ok=True); open(p, "w", encoding="utf-8", newline="\n").write(s)

def strip_tests(s):
    s = re.sub(r"#\[cfg\(test\)\]\s*\n\s*(pub(\(crate\))? )?mod \w+;\n", "", s)
    """Drop `#[cfg(test)] mod name { ... }` blocks (brace-counted, skipping strings and chars)."""
    out = []; i = 0
    while True:
        m = re.search(r"#\[cfg\(test\)\]\s*\n\s*(pub(\(crate\))? )?mod \w+\s*\{", s[i:])
        if not m: out.append(s[i:]); break
        a = i + m.start(); b = i + m.end(); out.append(s[i:a]); depth = 1; j = b
        while j < len(s) and depth:
            c = s[j]
            if c == '"':
                j += 1
                while j < len(s) and s[j] != '"': j += 2 if s[j] == "\\" else 1
            elif c == "'" and j + 2 < len(s) and (s[j + 2] == "'" or (s[j + 1] == "\\" and "'" in s[j + 2:j + 5])):
                j = s.index("'", j + 2)
            elif s[j:j + 2] == "//":
                j = s.index("\n", j)
            elif c == "{": depth += 1
            elif c == "}": depth -= 1
            j += 1
        i = j
    return "".join(out)

HEADER = """// Vendored from EffectCraft (https://github.com/storytold/effectcraft, commit {rev}),
// Copyright (c) 2026 ArtCraft Team and the EffectCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-ec/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: crate paths
// rewritten (effectcraft_* -> crate::*), test modules removed{extra}.
"""

def paths(s, inner=None):
    s = s.replace("effectcraft_keyframe::Value", "crate::Value").replace("effectcraft_project::ParamUi", "crate::ParamUi")
    s = s.replace("effectcraft_project::cineon_log_to_lin", "crate::cineon_log_to_lin")
    s = re.sub(r"\beffectcraft_(color|raster|geom|track)::", r"crate::\1::", s)
    if inner:  # a file that was the root of its own crate: its `crate::` is now `crate::<inner>::`
        s = re.sub(r"\bcrate::(?!(color|raster|geom|track)::)", f"crate::{inner}::", s)
    return s

def main():
    sd = os.path.join(DST, "src")
    if os.path.isdir(sd):
        for e in os.listdir(sd):
            q = os.path.join(sd, e); shutil.rmtree(q) if os.path.isdir(q) else os.remove(q)
    for f in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE"]: shutil.copy(os.path.join(SRC, f), os.path.join(DST, f))
    # colour, geometry and raster crates
    for name in ["blend", "icc", "space"]:
        write(f"src/color/{name}.rs", HEADER.format(rev=REV, extra="") + paths(strip_tests(read(f"crates/color/src/{name}.rs")), "color"))
    write("src/color/mod.rs", HEADER.format(rev=REV, extra="") + paths(strip_tests(read("crates/color/src/lib.rs")), "color"))
    write("src/geom.rs", HEADER.format(rev=REV, extra="") + paths(strip_tests(read("crates/geom/src/lib.rs")), "geom"))
    r = strip_tests(read("crates/raster/src/lib.rs"))
    r = re.sub(r"^pub mod (channels3d|cuts|flow|inpaint|scopes|warp|yuv);\n", "", r, flags=re.M)
    r = re.sub(r"^pub use (channels3d|warp)::[^;]*;\n", "", r, flags=re.M)
    r = re.sub(r"^#!\[[^\]]*\]\n", "", r, flags=re.M)
    write("src/raster/mod.rs", HEADER.format(rev=REV, extra="; only the image, blur and noise parts kept") + paths(r, "raster"))
    write("src/raster/blur.rs", HEADER.format(rev=REV, extra="") + paths(strip_tests(read("crates/raster/src/blur.rs")), "raster"))
    # effect modules
    for m in FX:
        write(f"src/{m}.rs", HEADER.format(rev=REV, extra="") + paths(strip_tests(read(f"crates/effects/src/{m}.rs"))))
        d = os.path.join(SRC, "crates/effects/src", m)
        if os.path.isdir(d):
            for f in os.listdir(d):
                if f.endswith(".rs") and f != "tests.rs":
                    write(f"src/{m}/{f}", HEADER.format(rev=REV, extra="") + paths(strip_tests(read(f"crates/effects/src/{m}/{f}"))))
    # stroke font for Particle Playground's text (textfx itself needs the text engine)
    t = read("crates/effects/src/textfx.rs").split("\n")
    a = next(i for i, l in enumerate(t) if l.startswith("fn glyph_src")); b = next(i for i, l in enumerate(t) if l.startswith("fn parse_glyph"))
    e = next(i for i in range(b, len(t)) if t[i] == "}")
    write("src/textfx.rs", HEADER.format(rev=REV, extra="; only the stroke font (glyph_strokes) kept") + "\n".join(t[a:e + 1]) + "\n")
    # Sony Imageworks LUT parsers live in craft-cms (a git dependency): not vendored, those LUT files are refused
    lp = os.path.join(DST, "src/ocio/lut.rs"); ls_ = open(lp, encoding="utf-8").read()
    for fn_ in ["parse_spimtx", "parse_spi1d", "parse_spi3d"]:
        ls_ = re.sub(r"(pub fn " + fn_ + r"\(text: &str\) -> Option<\w+> \{)\n.*?\n\}", r"\1\n    let _ = text;\n    None\n}", ls_, count=1, flags=re.S)
    open(lp, "w", encoding="utf-8", newline="\n").write(ls_)
    # the KLT tracker (used by Rolling Shutter Repair / Warp-style motion measurement)
    for f in ["klt", "plane"]:
        write(f"src/track/{f}.rs", HEADER.format(rev=REV, extra="") + paths(strip_tests(read(f"crates/track/src/{f}.rs")), "track"))
    write("src/track/mod.rs", HEADER.format(rev=REV, extra="; only klt and plane kept") + "pub mod klt;\npub mod plane;\n")
    # lib.rs: shared types sliced from the effects crate root
    lib = read("crates/effects/src/lib.rs").split("\n")
    def lines(a, b): return "\n".join(lib[a - 1:b])
    def find(pat, start=1):
        for k in range(start - 1, len(lib)):
            if re.match(pat, lib[k]): return k + 1
        raise SystemExit(f"not found: {pat}")
    cat = find(r"pub const CATEGORIES"); fl = find(r"pub fn flatten_params"); buf = find(r"/// A layer image in flight|pub struct Buf");
    while lib[buf - 2].startswith("///") or lib[buf - 2].startswith("#["): buf -= 1
    spec_end = find(r"pub\(crate\) fn col\(") + 3
    dv = find(r"/// A parameter's default value"); ls = find(r"pub fn layer_source_id") + 3
    core = lines(cat, fl - 1).rsplit("\n///", 1)[0] + "\n\n" + lines(buf, spec_end) + "\n\n" + lines(dv, ls) + "\n"
    # host services kimo does not provide yet
    core = re.sub(r"\n    /// Auxiliary 3D channels.*?\n    fn aux\(&self\)[^}]*\}\n", "\n", core, flags=re.S)
    core = paths(core)
    mods = "\n".join(f"pub mod {m};" for m in FX)
    specs = "\n".join(f"        v.extend({m}::specs());" for m in FX if re.search(r"^pub (fn specs\(\)|use [^;]*\bspecs\b)", read(f"crates/effects/src/{m}.rs"), re.M))
    out = HEADER.format(rev=REV, extra="; lib.rs keeps only the shared effect types, and adds Value and ParamUi in place of the keyframe and project crates") + f"""//! EffectCraft's effects, vendored for kimo (see `kimo_core::ec_bridge` for how kimo calls them).
#![allow(clippy::all, dead_code, unused_imports, unused_variables, unused_mut)]
use std::collections::HashMap;
use std::sync::OnceLock;

pub mod color;
pub mod geom;
pub mod raster;
pub mod track;
pub mod textfx;
pub use raster::Image;
{mods}

/// A parameter value (EffectCraft's keyframe `Value`, without paths, text and gradients).
#[derive(Clone, Debug, PartialEq)]
pub enum Value {{ Scalar(f64), Vec2([f64; 2]), Vec3([f64; 3]), Color([f64; 4]), Bool(bool), Enum(u32), Layer(Option<u64>), Str(String) }}
impl Value {{
    pub fn components(&self) -> Vec<f64> {{ match self {{ Value::Scalar(v) => vec![*v], Value::Vec2(v) => v.to_vec(), Value::Vec3(v) => v.to_vec(), Value::Color(v) => v.to_vec(), Value::Bool(b) => vec![if *b {{ 1.0 }} else {{ 0.0 }}], Value::Enum(i) => vec![*i as f64], _ => vec![] }} }}
    pub fn as_f64(&self) -> f64 {{ self.components().first().copied().unwrap_or(0.0) }}
    pub fn as_vec2(&self) -> [f64; 2] {{ let c = self.components(); [c.first().copied().unwrap_or(0.0), c.get(1).copied().unwrap_or(0.0)] }}
    pub fn as_vec3(&self) -> [f64; 3] {{ let c = self.components(); [c.first().copied().unwrap_or(0.0), c.get(1).copied().unwrap_or(0.0), c.get(2).copied().unwrap_or(0.0)] }}
    pub fn as_color(&self) -> [f32; 4] {{ let c = self.components(); let g = |i: usize, d: f64| c.get(i).copied().unwrap_or(d) as f32; [g(0, 0.0), g(1, 0.0), g(2, 0.0), g(3, 1.0)] }}
    pub fn as_bool(&self) -> bool {{ match self {{ Value::Bool(b) => *b, v => v.as_f64() != 0.0 }} }}
    pub fn as_enum(&self) -> u32 {{ match self {{ Value::Enum(i) => *i, v => v.as_f64().max(0.0) as u32 }} }}
    pub fn as_layer(&self) -> Option<u64> {{ if let Value::Layer(l) = self {{ *l }} else {{ None }} }}
    pub fn as_str(&self) -> &str {{ if let Value::Str(s) = self {{ s }} else {{ "" }} }}
}}

/// How a parameter is shown (EffectCraft's project `ParamUi`); kimo uses it to name and range parameters.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum ParamUi {{ #[default] Number, Slider {{ min: f64, max: f64, slider_min: f64, slider_max: f64, decimals: u8 }}, Percent, Angle, Pixels, Point, Point3, Color, Checkbox, Popup {{ options: Vec<String> }}, Layer, Mask, Path, Text, File {{ filters: Vec<String> }}, Gradient, Hidden }}

{core}

/// Every vendored effect.
pub fn registry() -> &'static [EffectSpec] {{
    static R: OnceLock<Vec<EffectSpec>> = OnceLock::new();
    R.get_or_init(|| {{
        let mut v = Vec::new();
{specs}
        v.sort_by(|a, b| a.category.cmp(b.category).then(a.name.cmp(b.name)));
        v
    }})
}}
/// From EffectCraft's project crate (footage.rs).
pub fn cineon_log_to_lin(v: f32, black: f32, white: f32, scale: f32, gamma: f32, rolloff: f32) -> f32 {{
    // Kodak printing density: 0.002 density per code value, negative gamma 0.6.
    let exposure = |code: f32| 10f32.powf((code - white) * 0.002 / 0.6);
    let offset = exposure(black);
    let mut lin = ((exposure(v * 1023.0) - offset) / (1.0 - offset).max(1e-6)).max(0.0);
    lin = lin.powf(scale / gamma.max(0.01));
    let knee = 1.0 - (rolloff / 100.0).clamp(0.0, 0.99);
    if knee < 1.0 && lin > knee {{
        let w = 1.0 - knee;
        lin = knee + w * (1.0 - (-(lin - knee) / w).exp());
    }}
    lin
}}

pub use card3d::CompScene;
pub fn find(id: &str) -> Option<&'static EffectSpec> {{ registry().iter().find(|s| s.id == id) }}
pub fn apply(spec: &EffectSpec, ctx: &EffectCtx, buf: Buf) -> Buf {{ (spec.render)(ctx, buf) }}
"""
    write("src/lib.rs", out)
    write("Cargo.toml", """[package]
name = "kimo-ec"
version.workspace = true
edition = "2024"
license = "MIT OR Apache-2.0"
description = "EffectCraft's effects (MIT OR Apache-2.0), vendored for kimo"

[dependencies]
rayon = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
""")
    print("vendored", len(FX), "effect modules at", REV)

main()
