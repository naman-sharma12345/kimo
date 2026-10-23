#!/usr/bin/env bash
# Kimo director demo: director-mode's recipe tools, called one at a time via the
# kimo CLI, over a folder of real clips -- no live/network model call, no code
# changes needed. See ../director_mode.md and docs/DIRECTOR_QUICKSTART.md for the
# guide a small model follows to emit the same call sequence itself.
#
# Usage: run_demo.sh <clips_dir> <out.mp4>
#   clips_dir: a folder of numbered 9:16 talking-head clips (clip1.mp4, ...)
#   out.mp4:   where the exported short goes
#
# Env:
#   KIMO_BIN   path to the kimo CLI binary (default: kimo on PATH)
#   RECIPE     path to a recipe JSON (default: director_recipe_example.json next
#              to this script -- matches the clips this demo shipped with; a
#              different footage set needs its own recipe, since the captions
#              are the actual honest words people say, not guessed)
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [ $# -lt 2 ]; then
  echo "usage: $0 <clips_dir> <out.mp4>" >&2
  exit 2
fi
exec python3 "$HERE/run_demo.py" "$1" "$2" ${RECIPE:+--recipe "$RECIPE"}
