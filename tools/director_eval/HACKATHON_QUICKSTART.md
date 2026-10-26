# Hackathon quickstart: director mode over MCP

## DEMO RUNBOOK (read this first)

1. `git checkout muse-tune`
2. `bash tools/whisper/fetch.sh` **first** -- without it, `auto_captions` silently falls back to plain grey boxes with no keyword colour (no error, just the wrong look).
3. Build: `export CARGO_TARGET_DIR=/tmp/kimo_build` (fresh, not shared with another checkout -- a shared/stale target dir can fail the vendor cmake cache) `&& cargo build --profile gate --bin kimo-mcp -j2` (~4 min), then `mkdir -p bin && cp "$CARGO_TARGET_DIR/gate/kimo-mcp" bin/kimo-mcp`.
4. Run (Meta `meta/muse-spark-1.3-contributor` via OpenRouter, your own key):
   `OPENAI_BASE_URL=https://openrouter.ai/api/v1 OPENAI_API_KEY=<your OpenRouter key> python3 tools/director_eval/agent.py --kimo bin/kimo-mcp --brief <brief.txt> --footage <clip_dir> --out <out_dir> --model meta/muse-spark-1.3-contributor --adapter openai --system tools/director_eval/system.txt --doc tools/director_eval/director_mode.md --tools director --max-wall 1000`
5. Fallback if OpenRouter/Meta is unreachable (Fireworks stand-in, same loop): swap in `OPENAI_BASE_URL=https://api.fireworks.ai/inference/v1 OPENAI_API_KEY=<your Fireworks key>` and `--model accounts/fireworks/models/muse-spark-open`.
6. Expect ~15 min per clip on the stand-in (measured: real_review 16-21 min, real_vlog 13-15 min). Don't kill early -- it exports on its own.
7. Output: `<out_dir>/final.mp4`, plus `run.json` (steps/calls/cost) and `log.jsonl` (every step) in the same `<out_dir>`.

Kimo cuts a short from raw phone footage. In director mode, a small model drives it through
about 35 MCP tools instead of the full 447-tool registry. This page is the fastest path from a
clone to a real exported short: build, list the tools a small model actually sees, run the demo
driver on your own clips, and the handful of things that trip people up first.

See `director_mode.md` for the call sequence and `../../docs/SMALL_MODEL_PREP.md` for why
director mode exists and what it fixed.

## What's new tonight

- **`lint_edit` catches two more problems:**
  - `aspect_mismatch` (warning) -- a clip's own aspect doesn't match the canvas by more than 5%, so `add_clip`'s default `fit: "cover"` is silently cropping or padding it. The `fix` field already names the call: `auto_reframe` on that `clip_id`.
  - `text_over_subject` (warning) -- a title or caption sits over the frame's strongest subject (reuses `add_smart_punch`'s own saliency map). The `fix` field gives a new `y` to move it to. **Known limit: it only checks the single strongest subject in frame** -- a hard-edged object can still slip past it, so treat it as a hint, not a guarantee.
  Both are additive and non-mutating like the rest of `lint_edit` -- fix, then run `lint_edit` again to confirm.
- **New tool: `trim_filler_words(clip_id, spans?, words?, pad?, retime_captions?)`** ripple-deletes flagged word spans out of a clip's own footage and closes the gap. It's registered in the `timeline` group, **not yet on the director-mode list** -- call it by name, find it with `search_tools`, or use `mode: "flat"` to see the full registry.

- **New tool: `fit_clip_duration(clip_id, max_duration, min_silence_ms?, threshold_db?, safety_margin_s?, retime_captions?)`** shrinks one clip to a target length by trimming its own quietest pauses (the same RMS silence rule as `audio_probe`), largest pause first, partial on only the last one needed for an exact fit -- no manual silence-detect-then-trim loop. It never cuts into detected speech: if there isn't enough silence to reach the target it says so (`met_target: false`, `shortfall_seconds`) instead of forcing it. Captions retime with the cut, same as `trim_filler_words` (`retime_captions`, default true; it calls `trim_filler_words` internally). Also `timeline` group, not on the director list yet. On real phone footage, pass `threshold_db: "auto"` instead of a number -- it measures the clip's own noise floor and sets the threshold above it (capped under the clip's own speech level), so room tone doesn't hide every pause behind the plain -60 default; the result reports the measured floor and the threshold it chose. Pass `protect_words: true` (default false) when the clip has a transcript (from `auto_captions` or a bound take) and you want a guarantee, not just a good guess: it shrinks or drops any candidate pause that would overlap a transcribed word (padded by `word_pad_s`, default 0.05s), so a pause whose detected edge runs a little long never carries a cut into speech (`words_protected` on the result counts how many pauses that touched).

  ```json
  {"name": "fit_clip_duration", "arguments": {"clip_id": "clip2", "max_duration": 20.0}}
  ```

  ```json
  {"name": "trim_filler_words", "arguments": {"clip_id": "clip2", "words": ["um", "uh"]}}
  ```

  or give exact spans in source time (seconds since the clip's own `in_point` -- the same zero point `auto_captions` word timestamps use):

  ```json
  {"name": "trim_filler_words", "arguments": {"clip_id": "clip2", "spans": [{"start": 4.1, "end": 4.6}], "pad": 0.05}}
  ```

  **Captions now retime automatically.** If you've already run `auto_captions`, cutting filler words with `trim_filler_words` deletes any caption entirely inside a removed span, trims one straddling a cut, and shifts ones after it left to close the gap -- no need to re-run `auto_captions` after. Pass `"retime_captions": false` to get the old behaviour (captions untouched, drifting out of sync) if you want it.

- **New tool: `pick_thumbnail(clip_id?, count?, min_gap_s?, out_dir?, sample_fps?)`** samples a clip or the whole timeline (default) at `sample_fps` (default 4/s), scores each candidate on sharpness (`focus_series`' own measure), exposure and a cheap subject-centred saliency signal, excludes black/frozen/transition frames, and writes the top `count` (default 3) picks -- at least `min_gap_s` apart -- as PNGs at project resolution. Registered in the `output` group next to `contact_sheet`/`describe_frame`, not on the director-mode list.

  ```json
  {"name": "pick_thumbnail", "arguments": {"clip_id": "clip3", "count": 1}}
  ```

- **New tool: `drop_short_segments(track_id?, min_duration_s?, dry_run?, keep_first_and_last?)`** deletes clips shorter than `min_duration_s` (default 1.0s) on a track and ripples the later clips left to close the gaps -- the tiny leftover false-start fragments an `auto_rough_cut` pass leaves between real takes (on real phone footage: a handful of sub-1s clips among the keepers). `track_id` defaults to the track with the most media clips. `keep_first_and_last` (default true) protects a genuinely short opening/closing shot from being treated as a false start. `dry_run: true` reports what would be dropped without changing anything. Reuses the existing `ripple_delete` path one clip at a time, so captions get exactly today's handling (none) -- same as before this lane. Registered in the `timeline` group next to `ripple_delete`/`remove_silence_cuts`, not on the director-mode list. KF13: on real `auto_rough_cut` footage, `ripple_delete`'s own shift can strand the clip right after a drop (sub-millisecond float drift vs. its 1e-6 tolerance), leaving a real gap; `drop_short_segments` now detects and closes that residual gap itself (see `gaps_closed` in the result) without touching `ripple_delete`.

  ```json
  {"name": "drop_short_segments", "arguments": {"track_id": "track1", "min_duration_s": 1.0}}
  ```

- **New tool: `add_pip(source, at_s, duration_s, position?, size?, shape?, border_px?, border_color?, shadow?, animate_in?, animate_out?, source_in_s?, mute_source?)`** composites a second video as a picture-in-picture inset -- a corner webcam bubble, a floating avatar, or a TV-frame cutaway -- in one call instead of `add_clip` + `set_property` x3 + `add_mask` + `add_shape` by hand. `source` is a media path or an existing clip id (its asset is reused). `size` is a fraction of frame width (default 0.3); `shape` is `rect`/`rounded` (default)/`circle`; `animate_in` defaults to `pop` (~0.25s scale-overshoot), `animate_out` to `fade`. Registered in the `graphics` group next to `add_shape`, not on the director-mode list.

  ```json
  {"name": "add_pip", "arguments": {"source": "clip3", "at_s": 2.0, "duration_s": 4.0, "position": "bottom_right", "shape": "circle", "size": 0.3}}
  ```

- **New tool: `add_keyword_pops(words?, auto?, max_per_minute?, min_gap_s?, style?, position?, color?, accent_color?, animate_in?, sfx?, track_id?, dry_run?)`** pops one big emphasised word or phrase onto the screen as it's spoken, then clears -- the #1 on-screen graphic among top creators (research lane KR2: 6 of 8 sampled use it). `words: [{text, at_s, duration_s?}]` places exact pops; `auto: true` instead picks emphasis words from the loaded transcript's word timings (the same emphasis scorer `auto_captions` uses to bold caption keywords, `speech_words::keywords` -- NOT `add_smart_punch`, which is a purely visual camera punch-in with no word signal), capped at `max_per_minute` (default 4) and spaced at least `min_gap_s` (default 3) apart; with no transcript loaded it errors by name, pointing at `transcribe`/`transcript_import`. `style` is `bold_caps` (default, white caps with a dark outline), `boxed` (an accent box behind the text, built on `add_title(style="hook_box")`), or `typewriter` (character reveal). `position` defaults to `upper_third` so the text clears a selfie-framed face. Built entirely on `add_text`/`add_title`/`add_text_animator`/`set_keyframe` -- no text rendering or easing math reimplemented. Registered in the `text` group next to `add_title`/`add_text_animator`, not on the director-mode list. Does not touch captions, caption defaults, or the caption-ripple code.

  ```json
  {"name": "add_keyword_pops", "arguments": {"words": [{"text": "HUGE", "at_s": 4.2, "duration_s": 1.2}], "style": "bold_caps"}}
  ```

- **New tool: `add_arrow(target, from?, at_s, duration_s?, color?, thickness?, label?, curve?, animate?, track_id?, dry_run?)`** draws a bright animated arrow pointing at a spot on screen -- the "look here" callout Johnny Harris/Veritasium/MKBHD-style creators use (research lane KR2). `target` is the normalized `{x,y}` point the arrowhead touches; `from` defaults to auto (continuing the line from the frame center through `target` outward by ~25% of the frame's shorter side). `animate` defaults to `draw`: the shaft grows from tail to head over ~0.3s with a tiny overshoot-and-settle bounce, then the arrowhead pops in (`pop` and `fade` are also available). An optional `label` at the tail is styled like `add_keyword_pops`'s `bold_caps` (Anton heavy caps, dark outline, drop shadow), smaller since it's a secondary annotation. Built on `add_shape` (`line` for the shaft, `polygon sides:3` for the arrowhead) + `add_text` + `set_keyframe` -- no new rasterizer. Registered in the `graphics` group next to `add_shape`/`add_pip`, not on the director-mode list.

  ```json
  {"name": "add_arrow", "arguments": {"target": {"x": 0.3, "y": 0.4}, "at_s": 3.0, "label": "THIS ONE"}}
  ```

## Tight cut in one command

`demo/run_tight_cut_mcp.py` chains tonight's new tools end to end on a whole clip folder, over
the same MCP transport `run_demo_mcp.py` uses (modelled on it, same `mcp.py` client; does not
modify `run_demo_mcp.py` itself). For each clip: import, assemble on the timeline (`auto_rough_cut`,
same call `run_demo_mcp.py` uses), `auto_captions` (only if a whisper model is configured),
`trim_filler_words` with a short filler-word list (only if that clip has captions), then
`fit_clip_duration(threshold_db:"auto", protect_words:true)` targeting ~12% off each clip's own
on-timeline duration -- then one `export`. Prints a per-clip original/+filler/+fit table and
writes a JSON summary next to the export.

```
python3 tools/director_eval/demo/run_tight_cut_mcp.py <clips_dir> out.mp4 --kimo target/release/kimo-mcp
```

Real 6-clip run on `footage_r/real` (10 rough-cut segments after dead-air removal), measured
tonight:

Without a whisper model (`trim_filler_words` skipped cleanly, `fit_clip_duration` still runs --
`protect_words` is then a documented no-op, each row reports why):

```
clip          orig  +filler    +fit  matched  protected   met  caps s/t
-----------------------------------------------------------------------
clip1.mp4     4.03     4.03    3.55        0          0  True       0/0
...
TOTAL        48.73    48.73   43.59
```

With `KIMO_WHISPER_BIN`/`KIMO_WHISPER_MODEL` set: 48.73s -> 44.49s overall (8.7% cut; several
tiny rough-cut segments have no spare silence to give and honestly report `met_target: false`
rather than cutting speech). `words_matched` was 0 across every clip for the default filler list
(um/uh/like/...) -- not a bug: the real transcript (Hinglish/Urdu) genuinely has none of those
disfluencies on this footage; `fit_clip_duration`'s own silence-based cut still did the work, and
`protect_words` kept every cut off a transcribed word (`words_protected` > 0 on several clips).

**A real caption-sync gap this script surfaced (not fixed here, no Rust touched):**
`trim_filler_words`/`fit_clip_duration`'s `retime_captions` only retimes the captions linked to
the ONE clip passed as `clip_id` -- it does not know that cutting an EARLIER clip on a shared
track ripples every LATER clip's video left too. Repro: rough-cut two clips onto one track,
`auto_captions` on both segments, then `fit_clip_duration` a few tenths of a second off the
FIRST segment. `timeline_summary` shows the dialogue track correctly rippled (the second clip's
`start` moves left by the cut amount) but that clip's OWN captions (built before the cut) stay
exactly where they were -- now out of sync with their clip by the cut's width, and every caption
after that compounds further out of sync with each earlier cut. This is why the with-whisper
export above came out ~48.2s instead of the ~44.5s the per-clip numbers add up to: a late
caption, never retimed, is what still reaches past the (now shorter) dialogue track and sets the
project's overall duration. Call `fit_clip_duration`/`trim_filler_words` back-to-front (last clip
on the track first) to sidestep it today, or only on the LAST clip on a track.

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
with one-line descriptions written for a small model, not the full 447-tool registry. Every tool
is still callable by name under any profile. `tools/director_eval/mcp.py` is a ~60-line Python
MCP stdio client if you want to drive this yourself instead of through a chat client; see
`list_tools(mode=...)` and `call(name, args)`.

## 2b. Model preset: Meta Muse Spark 1.3 Contributor

`tools/director_eval/agent.py` already speaks plain OpenAI-compatible tool calling (`--adapter openai`), which
is exactly Muse Spark 1.3 Contributor's format on both OpenRouter and Meta's own API -- no adapter changes
needed. Facts read off OpenRouter (`openrouter.ai/meta/muse-spark-1.3-contributor`, permaslug
`meta/muse-spark-1.3-contributor-20260902`, 2026-10-26): 1,048,576 token context; input text+image+video+file,
output text only; **mandatory reasoning** -- every response includes a separate `reasoning_content` field
alongside `content`/`tool_calls` that the adapter correctly ignores for the loop but that still spends real
output-token budget before the actual call (watch `run.json`'s `tokens.out` against `--max-tokens`-equivalent
4096 hardcoded in `agent.py`'s `OpenAICompat.step`, if you raise it); supported_parameters include
`tools`/`tool_choice`/`structured_outputs` -- standard function calling, no quirks seen in the tool-call shape
itself. Pricing (OpenRouter list, 2026-10-26): input $0.10/M, output $0.20/M, cache_read $0.002/M tokens (see
`PRICES["meta/muse-spark-1.3-contributor"]` in `agent.py`).

With your own OpenRouter key (replace the key; add `HTTP-Referer`/`X-Title` headers if OpenRouter asks for
them):

```
export OPENAI_BASE_URL=https://openrouter.ai/api/v1
export OPENAI_API_KEY=sk-or-...
python3 tools/director_eval/agent.py --kimo target/release/kimo-mcp \
    --brief tools/director_eval/briefs/talk.txt --footage FOOTAGE/talk --out out/talk \
    --model meta/muse-spark-1.3-contributor --adapter openai --tools director \
    --system tools/director_eval/system.txt --doc tools/director_eval/director_mode.md
```

With Meta's own Contributor API key directly (once issued), the same command with
`OPENAI_BASE_URL` pointed at Meta's endpoint and `--model` set to whatever model id Meta's API expects --
check Meta's key-issuance email/dashboard for the exact base URL and model id, both of which were not
available while writing this.

Without a Contributor key yet, the closest stand-in on this box is Fireworks' open release,
through Tab's vendor proxy (`OPENAI_BASE_URL=<proxy base>/fireworks/inference/v1`,
`--model accounts/fireworks/models/muse-spark-open`) -- same adapter, same `--tools director`, substantially
cheaper, and close enough on tool-calling behavior to tune the harness against before a real key lands.

**Known failure mode on this family (confirmed on the Fireworks stand-in, real footage, both of two briefs):**
`lint_edit`'s own `fix.tool` field and `search_tools`'s matches are generated against the full ~447-tool
registry, not your ~35-tool director profile -- so they can name a tool (`set_text_style`,
`set_caption_style`) the model was never given as a callable function and therefore cannot actually invoke
through tool-calling, however many times it re-describes or re-searches it. Seen costing 26-63% of a run's
tool calls before the model gave up and used the real fix (`delete_clip` + recreate via `add_title`/
`add_text`'s own style args) on its own. `agent.py` now flags this inline the moment it happens
(`flag_uncallable_tools`, right where tool results are read) and `director_mode.md` has the same note for the
model itself -- if you see repeated `kimo_describe`/`search_tools` calls on the same name with no actual
`tools/call` for it ever appearing in `log.jsonl`, this is why.

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
clean edit -- budget for them. **Note: these numbers predate tonight's two new `lint_edit`
findings** (`aspect_mismatch`, `text_over_subject`) -- the extra per-clip checks measurably add to
`lint_edit`'s own per-call cost, so budget a bit above this table, not exactly to it.

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
