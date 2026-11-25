"""Copy craft-libs' craft-time, craft-xml and craft-timeline crates (MIT OR Apache-2.0) into third_party/craft-libs,
source unchanged except for a provenance header and dropped test modules; manifests made standalone.

Usage: CRAFT_LIBS_SRC=/path/to/craft-libs python third_party/craft-libs/vendor.py
"""
import os, re, shutil, sys

SRC = os.environ.get("CRAFT_LIBS_SRC") or sys.exit("set CRAFT_LIBS_SRC to a craft-libs checkout")
DST = os.path.dirname(os.path.abspath(__file__))
REV = os.popen(f'git -C "{SRC}" rev-parse HEAD').read().strip()
CRATES = ["time", "xml", "timeline"]
VERS = {"serde": '"1"', "serde_json": '"1"', "thiserror": '"2"'}

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
    print("vendored", CRATES, "at", REV)

main()
