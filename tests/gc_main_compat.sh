#!/bin/bash
# G-c: frozen main (425a2e8) vs this tree, same golden scenes. Scratch output goes to a mktemp dir under ${TMPDIR:-/tmp}.
# usage: tests/gc_main_compat.sh [MAIN_WORKTREE]   (default ${TMPDIR:-/tmp}/mainwt, a git worktree of 425a2e8 with fonts symlinked; one cargo build at a time)
# Rules: scenes fx_stack and glass_post are SKIPPED (their effects are absent on main). Every other scene must match main on f32 hash and rgba8 hash.
# Allowed, explicit exceptions: scene "shapes" (RV-1, alpha 0.8 ellipse): f32 may differ by <= 1e-6 max abs, rgba8 must match.
# Known non-frame diffs: project-JSON hash of "shapes" (RV-1 stores fill_alpha as its own prop) and of "shards" (refract_dispersion field, not seen in this run); frames must still match. Anything else fails.
set -e
HERE=$(cd "$(dirname "$0")/.." && pwd); MAIN=${1:-${TMPDIR:-/tmp}/mainwt}; export PATH=$HOME/.cargo/bin:$PATH CARGO_BUILD_JOBS=1
GC=$(mktemp -d "${TMPDIR:-/tmp}/kimo-gc.XXXXXX"); trap 'rm -rf "$GC"' EXIT
cp "$HERE/crates/kimo-core/tests/gc_dump.rs" "$MAIN/crates/kimo-core/tests/gc_dump.rs"
(cd "$MAIN" && CARGO_TARGET_DIR=${MAIN_TARGET:-${TMPDIR:-/tmp}/kimo-maintarget} GC_OUT=$GC/main cargo test --release -p kimo-core --test gc_dump >$GC/main.log 2>&1)
(cd "$HERE" && CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$HERE/target} GC_OUT=$GC/v3 cargo test --release -p kimo-core --test gc_dump >$GC/v3.log 2>&1)
export GC
python3 - <<'PY'
import os; GC = os.environ['GC']
import struct, sys
SKIP = {'fx_stack', 'glass_post'}; ALLOW_F32 = {'shapes': 1e-6}; PROJ_OK = {'shapes', 'shards'}
def load(d):
    r = {}
    for l in open(f'{GC}/{d}/hashes.txt').read().split('\n'):
        if l.strip(): k, *v = l.split(); r[k] = v
    return r
m, v = load('main'), load('v3'); bad = 0; print('%-14s %-8s %s' % ('scene', 'result', 'detail'))
names = []
for k in v:
    n = k.split('@')[0]
    if n not in names: names.append(n)
for n in names:
    if n in SKIP: print('%-14s %-8s %s' % (n, 'SKIPPED', 'effect absent on main')); continue
    if m.get(n) == ['SKIP']: print('%-14s %-8s %s' % (n, 'SKIPPED', 'scene cannot be built on main')); continue
    notes = []; fail = False
    if m.get(n) != v.get(n): notes.append('project JSON hash differs' + (' (documented)' if n in PROJ_OK else ''))
    if m.get(n) != v.get(n) and n not in PROJ_OK: fail = True
    for t in ('0.1', '0.5', '0.9'):
        k = f'{n}@{t}'; mf, vf = m[k], v[k]
        if mf == vf: continue
        a = open(f'{GC}/main/{k}.f32', 'rb').read(); b = open(f'{GC}/v3/{k}.f32', 'rb').read(); fl = struct.unpack(f'{len(a)//4}f', a); gl = struct.unpack(f'{len(b)//4}f', b)
        mx = max(abs(x - y) for x, y in zip(fl, gl)) if len(fl) == len(gl) else float('inf')
        if n in ALLOW_F32 and mf[1] == vf[1] and mx <= ALLOW_F32[n]: notes.append(f'{t}: f32 differs, max abs {mx:.3g}, rgba8 equal (allowed)')
        else: fail = True; notes.append(f'{t}: DIVERGES max abs {mx:.3g}, rgba8 {"equal" if mf[1]==vf[1] else "differs"}')
    bad += fail; print('%-14s %-8s %s' % (n, 'FAIL' if fail else 'ok', '; '.join(notes) or 'frames identical'))
print('G-c: %s' % ('FAIL' if bad else 'PASS')); sys.exit(1 if bad else 0)
PY
