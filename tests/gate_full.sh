#!/bin/bash
# Full tier: phase boundaries, hot-set pushes (guard, alloc/memory, export/mux/audio, save/load, dispatch/errors, par/RenderCtx), before merge offers. Also runs G-c.
cd "$(dirname "$0")/.." || exit 2
python3 tests/gate_run.py full "$@"; rc=$?
[ -d "${MAIN_WT:-/tmp/mainwt}" ] && { bash tests/gc_main_compat.sh "${MAIN_WT:-/tmp/mainwt}" > /tmp/gate_gc.txt 2>&1 && echo "G-c: PASS" || { echo "G-c: FAIL (see /tmp/gate_gc.txt)"; rc=1; }; }
exit $rc
