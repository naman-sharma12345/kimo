#!/usr/bin/env python3
"""Gate manifest test: every test binary and every tests/*.py file is listed exactly once in tests/gate_manifest.txt, with a valid tier,
and every helper/skip entry has a reason. Fails on a new unclassified test file. Writes nothing."""
import glob, os, sys
root = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..")
seen, errs = {}, []
for n, line in enumerate(open(os.path.join(root, "tests/gate_manifest.txt")), 1):
    s = line.strip()
    if not s or s.startswith("#"): continue
    body, _, reason = s.partition("#"); f = body.split()
    if len(f) != 3 or f[0] not in ("rust", "py") or f[2] not in ("fast", "full", "helper", "skip"): errs.append(f"line {n}: bad entry '{s}'"); continue
    if f[2] in ("helper", "skip") and not reason.strip(): errs.append(f"line {n}: {f[1]} is {f[2]} without a reason")
    k = (f[0], f[1])
    if k in seen: errs.append(f"line {n}: {f[1]} listed twice (lines {seen[k][0]} and {n})")
    seen[k] = (n, f[2])
have = {("rust", os.path.basename(p)[:-3]) for p in glob.glob(os.path.join(root, "crates/kimo-core/tests/*.rs"))} | {("py", os.path.basename(p)) for p in glob.glob(os.path.join(root, "tests/*.py"))}
for k in sorted(have - set(seen)): errs.append(f"unclassified: {k[0]} {k[1]} (add it to tests/gate_manifest.txt)")
for k in sorted(set(seen) - have): errs.append(f"listed but missing on disk: {k[0]} {k[1]}")
n = {t: sum(1 for v in seen.values() if v[1] == t) for t in ("fast", "full", "helper", "skip")}
print("gate_check:", len(seen), "entries", n)
for e in errs: print("FAIL", e)
sys.exit(1 if errs else 0)
