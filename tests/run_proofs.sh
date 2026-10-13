#!/bin/bash
# Runs proof scripts (default: tests/*_proof.py) and fails loudly if any of them leaves a stray file in the repo.
HERE=$(cd "$(dirname "$0")/.." && pwd) || exit 2; cd "$HERE" || exit 2
SCR=$(mktemp -d "${TMPDIR:-/tmp}/kimo-proofs.XXXXXX"); trap 'rm -rf "$SCR"' EXIT
MAIN_WT=${MAIN_WT:-${TMPDIR:-/tmp}/mainwt}
before=$(git status --porcelain | sort)
fail=0
for f in ${@:-tests/*_proof.py}; do
  pkill kimo-mcp 2>/dev/null
  timeout "${PROOF_TIMEOUT:-150}" python3 "$f" > $SCR/proof_out.txt 2>&1; rc=$?
  after=$(git status --porcelain | sort)
  if [ "$before" != "$after" ]; then echo "STRAY FILES after $f:"; diff <(echo "$before") <(echo "$after"); fail=1; before=$after; fi
  [ $rc -ne 0 ] && { echo "FAIL rc=$rc $f"; fail=1; }
done
[ -z "$1" ] && command -v cargo >/dev/null && { cargo test --release -p kimo-core --test render_state_leak >$SCR/proof_leak.txt 2>&1 || { echo "FAIL render_state_leak (see $SCR/proof_leak.txt)"; fail=1; }; }
[ -z "$1" ] && [ -d "$MAIN_WT" ] && { bash tests/gc_main_compat.sh "$MAIN_WT" >$SCR/proof_gc.txt 2>&1 || { echo "FAIL G-c main compat (see $SCR/proof_gc.txt)"; fail=1; }; }
exit $fail
