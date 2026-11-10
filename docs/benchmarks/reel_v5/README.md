# reel_v5: benchmark composition profile

A 25 s Kimo v5 launch teaser, built entirely through MCP tool calls against a fresh headless engine (kimo 3.0.0, commit 27e92ac). The v5 spectator GUI was recorded the whole time, in one continuous take.

- 493 calls (492 build + 1 export): 493 OK, 0 errors, 0 retries.
- 35 distinct tools out of 378 registered. This is a benchmark composition profile, not full surface coverage.
- The audio bed was synthesized outside Kimo (numpy). Kimo handled the import, EQ, compression, pan, volume keys, loudness measurement and normalization.
- The videos are archived outside the repo. Their checksums are in SHA256SUMS.txt.
- CALL_AUDIT.txt has the full 8-section reconciliation and its gaps:
  - the driver did not log JSON-RPC ids;
  - the recorded page had no GUI receive counter;
  - preview lag was about 2 revs;
  - the viewer held its last frame during the export.
- scripts/ holds the driver (build.py, run.py), the SSE tap, the GUI replay and the speed-ramp tools as they were run. Their paths are absolute to the box they ran on.
