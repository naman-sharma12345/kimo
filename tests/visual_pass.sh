#!/bin/bash
# Visual tier (pass 2). Needs a built kimo-mcp (KIMO_BIN, default target/release/kimo-mcp) and ffmpeg. Writes ONLY under $TMPDIR/kimo-visual-<run>, never into the repo.
# Usage: tests/visual_pass.sh [feature ...]     Then look at the sheets listed in <out>/manifest.json and give a named verdict per feature.
# --determinism runs every scene with KIMO_THREADS=1 and 4 and requires byte-equal frames.
cd "$(dirname "$0")/.." || exit 2
OUT="${TMPDIR:-/tmp}/kimo-visual-$(date +%H%M%S)-$$"; mkdir -p "$OUT"
if [ "$1" = "--determinism" ]; then shift
  KIMO_THREADS=1 python3 tests/visual_pass.py --out "$OUT/t1" "$@" >/dev/null || exit 1; KIMO_THREADS=4 python3 tests/visual_pass.py --out "$OUT/t4" "$@" >/dev/null || exit 1
  python3 - "$OUT" <<'PY'
import json, sys
a = json.load(open(sys.argv[1] + "/t1/manifest.json")); b = json.load(open(sys.argv[1] + "/t4/manifest.json")); bad = 0
for x, y in zip(a, b):
    for f, g in zip(x["frames"], y["frames"]):
        if f["sha256"] != g["sha256"]: bad += 1; print("DIFF", x["feature"], f["label"])
print("determinism 1v4:", "PASS" if not bad else f"FAIL {bad}"); sys.exit(1 if bad else 0)
PY
  exit $?
fi
exec python3 tests/visual_pass.py --out "$OUT" "$@"
