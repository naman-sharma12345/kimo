#!/bin/bash
# Runs proof scripts (default: tests/*_proof.py) and fails loudly if any of them leaves a stray file in the repo.
cd "$(dirname "$0")/.." || exit 2
before=$(git status --porcelain | sort)
fail=0
for f in ${@:-tests/*_proof.py}; do
  pkill kimo-mcp 2>/dev/null
  timeout "${PROOF_TIMEOUT:-150}" python3 "$f" > /tmp/proof_out.txt 2>&1; rc=$?
  after=$(git status --porcelain | sort)
  if [ "$before" != "$after" ]; then echo "STRAY FILES after $f:"; diff <(echo "$before") <(echo "$after"); fail=1; before=$after; fi
  [ $rc -ne 0 ] && { echo "FAIL rc=$rc $f"; fail=1; }
done
[ -z "$1" ] && command -v cargo >/dev/null && { cargo test --release -p kimo-core --test render_state_leak >/tmp/proof_leak.txt 2>&1 || { echo "FAIL render_state_leak (see /tmp/proof_leak.txt)"; fail=1; }; }
[ -z "$1" ] && [ -d "${MAIN_WT:-/tmp/mainwt}" ] && { bash tests/gc_main_compat.sh "${MAIN_WT:-/tmp/mainwt}" >/tmp/proof_gc.txt 2>&1 || { echo "FAIL G-c main compat (see /tmp/proof_gc.txt)"; fail=1; }; }
exit $fail
