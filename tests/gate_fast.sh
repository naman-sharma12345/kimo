#!/bin/bash
# Fast tier: every unit push. Lib tests, golden/contract/registry/readme gates, build identity, cache, thread hygiene, fuzz smoke, mcp proofs.
cd "$(dirname "$0")/.." || exit 2
exec python3 tests/gate_run.py fast "$@"
