# Caption and preview QA

Run after RV-8 and RV-25 with one cargo process:

    cargo test -j1 --profile gate -p kimo-core --test qa_fuzz --test transcript_parse -- --test-threads=1

qa_fuzz includes an intentionally strict WebVTT entity round-trip regression. It is red without the companion parser fix, not a golden to regenerate. Existing transcript_parse mutation fuzz uses 20,000 inputs x 6 formats x strict/repair options = 240,000 parser calls. Additional invariants cover 500 overlap cases, 400 subtitle timing round-trips, short adjacent sentence groups, preview argument edges, Unicode and 10k+ character inputs.

The native frame proof requires the supplied composite project and its exact footage/fonts. No substitutions or path-string edits are made by the proof. Put its referenced assets at valid paths before running:

    KIMO_QA_PROJECT=/path/to/composite.kimo.json KIMO_BIN=target/gate/kimo-mcp python3 tests/qa_native_baseline.py

It asserts native t=5 PNG SHA256 ace7fc03852e549edf8b5c12db8454c2ffdd8bd650ef2f3439efa00b6b110de9, then asserts the unlabelled contact_sheet tile equals an independently computed integer box downscale with zero differing pixels. A contact sheet is capped at 320px, so its full image cannot equal the native canvas hash. No exports are performed.

The 30-frame golden suite needs Montserrat installed via fonts/fetch.sh. Without it, the default Montserrat silently falls back to DejaVu Sans and only text scene pixels change. Do not regenerate goldens for that environment mismatch.
