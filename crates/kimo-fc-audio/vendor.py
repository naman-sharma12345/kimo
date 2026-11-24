"""Build crates/kimo-fc-audio from a checkout of FilmCraft (MIT OR Apache-2.0): its audio-dsp crate, unchanged except
for a provenance header and removed test modules.

Usage: FILMCRAFT_SRC=/path/to/filmcraft python crates/kimo-fc-audio/vendor.py
"""
import os, re, shutil, sys

SRC = os.environ.get("FILMCRAFT_SRC") or sys.exit("set FILMCRAFT_SRC to a FilmCraft checkout")
DST = os.path.dirname(os.path.abspath(__file__))
REV = os.popen(f'git -C "{SRC}" rev-parse HEAD').read().strip()
HEADER = f"""// Vendored from FilmCraft (https://github.com/storytold/filmcraft, commit {REV}), crates/audio-dsp,
// Copyright (c) 2026 ArtCraft Team and the FilmCraft contributors, licensed MIT OR Apache-2.0
// (see crates/kimo-fc-audio/LICENSE-MIT, LICENSE-APACHE and NOTICE). Changes for kimo: test modules removed.
"""

def strip_tests(s):
    s = re.sub(r"#\[cfg\(test\)\]\s*\n\s*(pub(\(crate\))? )?mod \w+;\n", "", s)
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

def main():
    root = os.path.join(SRC, "crates", "audio-dsp", "src"); sd = os.path.join(DST, "src")
    if os.path.isdir(sd):
        for e in os.listdir(sd):
            q = os.path.join(sd, e); shutil.rmtree(q) if os.path.isdir(q) else os.remove(q)
    for f in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE"]: shutil.copy(os.path.join(SRC, f), os.path.join(DST, f))
    n = 0
    for dp, dn, fn in os.walk(root):
        for f in fn:
            if not f.endswith(".rs") or f == "tests.rs" or f.endswith("_tests.rs"): continue
            rel = os.path.relpath(os.path.join(dp, f), root)
            s = strip_tests(open(os.path.join(dp, f), encoding="utf-8").read())
            s = re.sub(r"^#!\[cfg_attr\(not\(test\)[^\n]*\n", "", s, flags=re.M)
            s = re.sub(r"^#\[cfg\(test\)\]\s*\n\s*mod \w+;\n", "", s, flags=re.M)
            p = os.path.join(sd, rel); os.makedirs(os.path.dirname(p), exist_ok=True)
            open(p, "w", encoding="utf-8", newline="\n").write(HEADER + s); n += 1
    open(os.path.join(DST, "Cargo.toml"), "w", encoding="utf-8", newline="\n").write("""[package]
name = "kimo-fc-audio"
version.workspace = true
edition = "2024"
license = "MIT OR Apache-2.0"
description = "FilmCraft's audio DSP (MIT OR Apache-2.0), vendored for kimo"

[dependencies]
""")
    print("vendored", n, "files at", REV)

main()
