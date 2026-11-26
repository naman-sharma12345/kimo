"""Copy craft-libs' craft-time, craft-xml and craft-timeline crates (MIT OR Apache-2.0) into third_party/craft-libs,
source unchanged except for a provenance header and dropped test modules; manifests made standalone.

Usage: CRAFT_LIBS_SRC=/path/to/craft-libs python third_party/craft-libs/vendor.py
"""
import os, re, shutil, sys

SRC = os.environ.get("CRAFT_LIBS_SRC") or sys.exit("set CRAFT_LIBS_SRC to a craft-libs checkout")
DST = os.path.dirname(os.path.abspath(__file__))
REV = os.popen(f'git -C "{SRC}" rev-parse HEAD').read().strip()
CRATES = ["time", "xml", "timeline", "captions", "cfb", "aaf"]
VERS = {"serde": '"1"', "serde_json": '"1"', "thiserror": '"2"'}

def strip_tests(s):
    """Drop `#[cfg(test)] mod name;` and `#[cfg(test)] mod name { ... }`. The sources are rustfmt-formatted, so a
    top-level module ends at the first line that is exactly `}` (strings in tests can hold any braces)."""
    s = s.replace("\r\n", "\n")
    s = re.sub(r"#\[cfg\(test\)\]\s*\n\s*(pub(\(crate\))? )?mod \w+;\n", "", s)
    lines = s.split("\n"); out = []; i = 0
    while i < len(lines):
        if lines[i].strip() == "#[cfg(test)]" and i + 1 < len(lines) and re.match(r"(pub(\(crate\))? )?mod \w+\s*\{\s*$", lines[i + 1]):
            j = i + 2
            while j < len(lines) and lines[j] != "}": j += 1
            i = j + 1; continue
        out.append(lines[i]); i += 1
    return "\n".join(out)

def manifest(text):
    text = re.sub(r"^version\.workspace = true", 'version = "0.1.0"', text, flags=re.M)
    text = re.sub(r"^edition\.workspace = true", 'edition = "2024"', text, flags=re.M)
    text = re.sub(r"^license\.workspace = true", 'license = "MIT OR Apache-2.0"', text, flags=re.M)
    text = re.sub(r"^(rust-version|repository)\.workspace = true\n", "", text, flags=re.M)
    text = re.sub(r"\n\[lints\]\nworkspace = true\n", "\n", text)
    text = re.sub(r"\n\[dev-dependencies\]\n(?:[^\n]+\n?)*", "\n", text)
    text = re.sub(r"\n\[\[test\]\]\n(?:[^\n]+\n?)*", "\n", text)
    text = re.sub(r"^craft-(\w+)(\.workspace = true| = \{ workspace = true(.*?) \})", lambda m: f'craft-{m.group(1)} = {{ path = "../{m.group(1)}"{m.group(3) or ""} }}', text, flags=re.M)
    def dep(m):
        name, rest = m.group(1), m.group(3) or ""
        return f"{name} = {{ version = {VERS[name]}{rest} }}" if rest else f"{name} = {VERS[name]}"
    text = re.sub(r"^(serde_json|serde|thiserror)(\.workspace = true| = \{ workspace = true(.*?) \})", dep, text, flags=re.M)
    return text

def main():
    for c in CRATES:
        src, dst = os.path.join(SRC, "crates", c), os.path.join(DST, c)
        if os.path.isdir(dst): shutil.rmtree(dst)
        for dp, dn, fn in os.walk(os.path.join(src, "src")):
            for f in fn:
                if not f.endswith(".rs") or f == "tests.rs" or "tests" in os.path.relpath(dp, src).split(os.sep): continue
                rel = os.path.relpath(os.path.join(dp, f), src)
                s = strip_tests(open(os.path.join(dp, f), encoding="utf-8").read())
                head = (f"// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit {REV}), crates/{c},\n"
                        "// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0\n"
                        "// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.\n")
                if s.startswith("#!["):  # inner attributes must stay first
                    first = s.index("\n") + 1; s = s[:first] + head + s[first:]
                else: s = head + s
                p = os.path.join(dst, rel); os.makedirs(os.path.dirname(p), exist_ok=True); open(p, "w", encoding="utf-8", newline="\n").write(s)
        open(os.path.join(dst, "Cargo.toml"), "w", encoding="utf-8", newline="\n").write(manifest(open(os.path.join(src, "Cargo.toml"), encoding="utf-8").read()))
    for f in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE"]: shutil.copy(os.path.join(SRC, f), os.path.join(DST, f))
    audio_lite()
    print("vendored", CRATES, "and craft-audio (PCM wrappers only) at", REV)

def audio_lite():
    """craft-aaf needs only craft-audio's integer-PCM WAVE/AIFF wrappers; the full crate (decoders, symphonia) is not
    vendored. This builds a crate of the same name from those three source files."""
    src = os.path.join(SRC, "crates", "audio", "src"); dst = os.path.join(DST, "audio")
    if os.path.isdir(dst): shutil.rmtree(dst)
    os.makedirs(os.path.join(dst, "src"))
    head = (f"// Vendored from craft-libs (https://github.com/storytold/craft-libs, commit {REV}), crates/audio/src/{{f}},\n"
            "// Copyright (c) 2026 ArtCraft Team and the craft-libs contributors, licensed MIT OR Apache-2.0\n"
            "// (see third_party/craft-libs/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: {c}.\n")
    ep = strip_tests(open(os.path.join(src, "essence_pcm.rs"), encoding="utf-8").read())
    open(os.path.join(dst, "src", "essence_pcm.rs"), "w", encoding="utf-8", newline="\n").write(head.format(f="essence_pcm.rs", c="test modules removed") + ep)
    wr = strip_tests(open(os.path.join(src, "write", "raw.rs"), encoding="utf-8").read())
    ai = open(os.path.join(src, "write", "aiff.rs"), encoding="utf-8").read()
    m = re.search(r"pub\(crate\) fn u32_to_extended\(r: u32\) -> \[u8; 10\] \{.*?\n\}\n", ai, re.S)
    wr = wr.replace("use super::aiff::u32_to_extended;\n", "/// From craft-audio's write/aiff.rs.\n" + m.group(0).replace("pub(crate) fn", "fn"))
    open(os.path.join(dst, "src", "write_raw.rs"), "w", encoding="utf-8", newline="\n").write(head.format(f="write/raw.rs", c="u32_to_extended copied in from write/aiff.rs; test modules removed") + wr)
    lib = head.format(f="lib.rs", c="only the integer-PCM WAVE/AIFF wrappers craft-aaf uses; wav_pcm is built on essence_pcm::parse_wav") + """//! craft-audio, reduced to the integer-PCM WAVE and AIFF wrappers interchange files embed (craft-aaf's needs).
pub mod essence_pcm;
mod write_raw;
pub use write_raw::{pcm_aiff_file, pcm_aiff_header, pcm_wav_file, pcm_wav_header};

/// Integer PCM taken out of a WAV file, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawPcm { pub data: Vec<u8>, pub channels: u16, pub sample_rate: u32, pub bits: u16 }

/// The integer PCM of a WAV file, bit for bit; `None` for anything else.
pub fn wav_pcm(bytes: &[u8]) -> Option<RawPcm> {
    essence_pcm::parse_wav(bytes).map(|(data, channels, sample_rate, bits)| RawPcm { data, channels, sample_rate, bits })
}
"""
    open(os.path.join(dst, "src", "lib.rs"), "w", encoding="utf-8", newline="\n").write(lib)
    open(os.path.join(dst, "Cargo.toml"), "w", encoding="utf-8", newline="\n").write("""[package]
name = "craft-audio"
version = "0.1.0"
edition = "2024"
license = "MIT OR Apache-2.0"
description = "craft-libs' craft-audio reduced to its integer-PCM WAVE/AIFF wrappers (vendored for kimo)"

[features]
decode-native = []
pcm-encode = []

[dependencies]
""")

main()
