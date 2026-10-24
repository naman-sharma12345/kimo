# Hackathon quickstart: director mode over MCP

Kimo cuts a short from raw phone footage. In director mode, a small model drives it through
about 35 MCP tools instead of the full 445-tool registry. This page is the fastest path from a
clone to a real exported short: build, list the tools a small model actually sees, run the demo
driver on your own clips, and the handful of things that trip people up first.

See `director_mode.md` for the call sequence and `../../docs/SMALL_MODEL_PREP.md` for why
director mode exists and what it fixed.

## 1. Build kimo-mcp

```
cargo build --release --bin kimo-mcp
```

Quick check that it starts and speaks MCP:

```
echo '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}' | target/release/kimo-mcp
```

Prints `"serverInfo":{"commit":"...","name":"kimo",...}`. See the main `README.md`
("Prerequisites") for `cmake` / a C++ compiler / `ffmpeg` if the build itself fails.

## 2. Start it over stdio and list the director-mode tools

Your MCP client (Claude Code, Codex, Cursor, or your own harness) launches `kimo-mcp` for you --
nothing to run standing-alone. `tools/list` takes a `mode` param that overrides `KIMO_TOOLS`:

```json
{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{"mode":"director"}}
```

This returns **35 tools** -- the recipe tools from `director_mode.md` (`auto_rough_cut`,
`auto_grade`, `auto_captions`, `caption_from_words`, `finish_audio`, `lint_edit`, `export`, ...)
with one-line descriptions written for a small model, not the full 445-tool registry. Every tool
is still callable by name under any profile. `tools/director_eval/mcp.py` is a ~60-line Python
MCP stdio client if you want to drive this yourself instead of through a chat client; see
`list_tools(mode=...)` and `call(name, args)`.

## 3. Run the demo on a clip folder

`run_demo_mcp.py` makes the exact director-mode call sequence over real `tools/call` JSON-RPC
against a spawned `kimo-mcp` -- the actual transport a small model drives, not the CLI:

```
python3 tools/director_eval/demo/run_demo_mcp.py <clips_dir> out.mp4 \
    --kimo target/release/kimo-mcp --report report.json
```

`clips_dir` holds numbered 9:16 talking-head clips (`clip1.mp4`, `clip2.mp4`, ...).
`director_recipe_example.json` next to the script is the recipe this demo ships with (clip order,
hook text, which clip gets the smart-punch highlight); point `--recipe path.json` at your own for
a different footage set. See `demo/DEMO.md` for the full call-by-call breakdown.

On a real 5-clip set (6th clip dropped as off-topic filler) this produces 19 MCP calls and a
14.1 MB, 41.9s, 720x1280 export.

## 4. Captions: with and without a speech model

`auto_captions` is kimo's own speech recognition (beam search, a Hinglish prompt, a Hindi/Urdu/
Punjabi/Marathi pass) -- the documented way to caption a clip you don't have, or don't trust, a
transcript for (see `director_mode.md` step 7). It needs a local Whisper model on disk.

**To enable it:**

```
bash tools/whisper/fetch.sh            # multilingual small, q5_1, ~181 MiB, into ~/.cache/kimo_models (default)
MODEL=turbo bash tools/whisper/fetch.sh  # large-v3-turbo: ~6x slower, ties small on Hinglish word accuracy
MODEL=base  bash tools/whisper/fetch.sh  # fastest, weakest
```

or point at a model you already have: `KIMO_WHISPER_BIN=/path/to/whisper-cli` and
`KIMO_WHISPER_MODEL=/path/to/ggml-*.bin`.

**Without a model installed**, `auto_captions` does not crash -- it returns a clear, actionable
error over MCP:

> no speech model is installed, so auto_captions can't transcribe this audio: skip captions (ship
> the edit without them), or call caption_from_words with words you already trust, by_asset per
> clip (it flags, rather than guesses, any cue the audio is hard to verify). auto_captions also
> takes words:[{text,start,end}] directly if you'd rather call it again that way. To add a
> recognizer later: run tools/whisper/fetch.sh once ...

`run_demo_mcp.py` (and `run_demo.py`) treat exactly this error as expected: the edit ships
without captions, reported honestly in the JSON report, instead of the whole run aborting. A
small model restricted to the director tool list should do the same -- skip captions, or call
`caption_from_words` with words it already trusts.

## 5. Env knobs

| Variable | Effect |
|---|---|
| `KIMO_ANALYSIS_PROXY=off` | Disables the analysis-only decode proxy entirely; `get_scopes` / `lint_edit`'s picture scan decode the original media directly, exactly as before this existed. Export never uses this proxy either way. |
| `KIMO_ANALYSIS_PROXY_CACHE=off` | Disables the proxy's disk cache (a proxy is still built, in-memory per call, when eligible). |
| `KIMO_ANALYSIS_PROXY_CACHE=<dir>` | Puts proxy files in `<dir>` instead of the default. |
| (default) | Cached under `<project dir>/.kimo_cache/analysis_proxy`, keyed by source path + size + mtime + project fps. Only built for assets whose own max keyframe gap exceeds 2.0s (phone clips are often one keyframe every several seconds). |
| `KIMO_ANALYSIS_PROXY_CACHE_MAX_MB` | Cache size budget, default 2048 MB; pruned oldest-build-first past that. |
| `KIMO_WHISPER_BIN`, `KIMO_WHISPER_MODEL` | Point `auto_captions` at a whisper-cli binary and ggml model without running `fetch.sh`. |
| `KIMO_TOOLS` | Default `tools/list` profile when a client doesn't pass `mode` (`grouped`, `core`, `flat`, `director`, or a comma list of groups). |
| `KIMO_SPECTATOR=off` | No telemetry to the spectator app; harmless and recommended for a scripted/headless run. |

## 6. Typical latencies (real 5-clip demo project, 720x1280, two `lint_edit` passes)

| Call | ms (measured) |
|---|---|
| `auto_rough_cut` | ~2.7-3.0s |
| `lint_edit` (per pass) | ~12-13s |
| `export` | ~45-48s |

(From the MCP call reports in tasks `171c602f` and `a0060384`, and reproduced fresh for this
doc.) `lint_edit` and `export` dominate a loop that calls `lint_edit` repeatedly to converge on a
clean edit -- budget for them.

## 7. Running the tests

`--release` is recommended for speed (`cargo build --release` / `cargo test --release -p kimo-core`).
A plain debug-mode `cargo test -p kimo-core --test director_small` also passes: the one timing
check in it (`lint_edit_scans_a_long_clip_without_rescanning_from_its_start_each_sample`) uses a
higher ceiling in a debug build, since `lint_edit` genuinely takes longer there -- it still fails
if the old quadratic rescan it guards against comes back.

## Known limits

- **`lint_edit`'s cost floor is engine-wide, not a picture-scan bug.** Two real speedups landed
  (a decode-cache pre-warm, a near-lossless short-GOP analysis proxy for long-GOP phone clips, a
  removed duplicate prepare call, a memoized dialogue-track audio decode) took it from ~20.2s to
  ~12.3s/call on this demo project. What's left is full-resolution frame compositing (grading,
  text, smart-punch effects redrawn per sampled frame) plus a new `ffmpeg` process spawned per
  real decode -- both are used by every render/preview path, not just `lint_edit`'s picture scan,
  so fixing them needs engine-wide care, not a `lint_edit`-local change.
- **Captions need a model on disk.** With none, `auto_captions` fails clearly and the demo
  continues without captions -- see section 4. Don't invent caption text from a guess; use
  `caption_from_words` only with words you actually trust.
- **`clip4` is dropped as filler in the shipped demo recipe** (off-topic, not part of the
  "favorite bottles" story) -- expected, not a bug if your own recipe imports fewer clips than
  your folder holds.
