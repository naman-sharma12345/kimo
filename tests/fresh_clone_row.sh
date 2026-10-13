#!/bin/bash
# Fresh-clone row: clone HEAD to a different temp path and run the tier entrypoints in list mode. Exit 0 only if the clone needs nothing from this checkout's path or from /home.
# Also fails when any tracked script or test still hardcodes a developer path (rg '(/tmp/kimo|/home/)' tests/).
HERE=$(cd "$(dirname "$0")/.." && pwd); T=$(mktemp -d "${TMPDIR:-/tmp}/kimo-fresh.XXXXXX"); trap 'rm -rf "$T"' EXIT
git clone -q --no-hardlinks "$HERE" "$T/clone-other-path" || exit 2
cd "$T/clone-other-path" || exit 2
for tier in fast full; do python3 tests/gate_run.py $tier --list > "$T/$tier.txt" 2>&1 || { echo "FAIL gate_run $tier --list"; tail -5 "$T/$tier.txt"; exit 1; }; head -1 "$T/$tier.txt"; done
for s in tests/*.sh; do bash -n "$s" || { echo "FAIL syntax $s"; exit 1; }; done
python3 -m py_compile tests/*.py || exit 1
if grep -rEn '(/tmp/kimo|/home/)' tests/ --include='*.py' --include='*.sh' | grep -v 'tests/fresh_clone_row.sh'; then echo "FAIL hardcoded developer path in tests/"; exit 1; fi
echo "fresh clone row: ok"
