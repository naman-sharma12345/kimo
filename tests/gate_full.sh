#!/bin/bash
# Full tier: phase boundaries, hot-set pushes (guard, alloc/memory, export/mux/audio, save/load, dispatch/errors, par/RenderCtx), before merge offers. Also runs G-c.
HERE=$(cd "$(dirname "$0")/.." && pwd) || exit 2; cd "$HERE" || exit 2
SCR=$(mktemp -d "${TMPDIR:-/tmp}/kimo-gate.XXXXXX"); trap 'rm -rf "$SCR"' EXIT
MAIN_WT=${MAIN_WT:-${TMPDIR:-/tmp}/mainwt}
python3 tests/gate_run.py full "$@"; rc=$?
[ -d "$MAIN_WT" ] && { bash tests/gc_main_compat.sh "$MAIN_WT" > $SCR/gate_gc.txt 2>&1 && echo "G-c: PASS" || { echo "G-c: FAIL (see $SCR/gate_gc.txt)"; rc=1; }; }
exit $rc
