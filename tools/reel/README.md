# tools/reel — Kimo MCP reel assembly pipeline

Turns one recorded take of the 10-line Kimo reel script into a finished vertical reel
(1080x1920, 30 fps, H.264 + AAC) where **every edit is a Kimo MCP tool call**: cuts,
captions, motion graphics, colour grade, SFX placement, voice chain, ducking, loudness and
the final encode all go through `kimo-mcp` over stdio (~984 calls for the v2 reel, 0 clicks).

Outside Kimo: faster-whisper (timing alignment only — captions always use the exact script
strings), numpy (synthesized whoosh/hit/tick/riser SFX and the music bed), and ffmpeg (audio
extraction, loudness probe of the bed, and three JPEG stills from the export).

## Pipeline

1. **probe** — ffprobe the take (rotation metadata is reported; Kimo handles rotation on decode).
2. **audio** — 16 kHz mono wav for ASR (cached per input in the work dir).
3. **asr.py** — silero VAD chunks + faster-whisper `small` per chunk with word timestamps (cached).
4. **align.py** — per-line take selection: fuzzy word alignment against `script.json` + DP over
   chunks in order; tolerates wording drift, Hinglish and retakes, picks the clearest complete take.
5. **cut list** — lines laid back to back with small gaps, transitions on lines 4/5/7/9, end card +2.9 s.
6. **sfx.py** — numpy-synthesized SFX and a music bed (no external assets).
7. **reelbuild.py** — all Kimo calls. A stub pass first counts the calls exactly, so the on-screen
   tool-call counter equals the real count (including the export call).
8. **export** — Kimo `export`, then three stills next to the output (`<out>_s1..s3.jpg`).

Layout: 0–8.5 s hook (lines 1–3, punch-in cuts + whooshes) · 8.6–15 s spectator-style GUI demo
(line 4: colour wheels driving a real `color_wheels` grade, faders, timeline blade cuts, live log of
this run's tool calls) · 15–30 s proof (call counter, ZERO CLICKS, grade/motion/sound chips, Rust
badge) · end card (Kimo mark drawn from shapes, line 10). Voice chain: HPF 80 Hz, denoise, 3:1 comp,
de-esser, presence EQ, normalize −14 LUFS; 8 ms splice fades over continuous room tone; bed
`auto_duck` −3.5 dB (50 ms / 250 ms) under speech.

## Requirements

- python3 with `numpy`, `faster-whisper` (downloads the `small` model on first run) and `fontTools`
  (+ `brotli`, to convert the bundled woff2 fonts)
- `ffmpeg` / `ffprobe` on PATH
- a built `kimo-mcp`: `cargo build --release -p kimo-mcp`
- the repo checkout itself: `examples/mcp_client.py` (stdio client) and the SIL OFL fonts in
  `crates/kimo-gui/ui/fonts` (Geist, Geist Mono; covers — … ’ “ ” ·)

## Usage

```sh
python3 tools/reel/assemble.py take.mp4                      # -> ./kimo_reel.mp4, work in ./reel_work
python3 tools/reel/assemble.py take.mp4 --out reel.mp4 --crf 20
python3 tools/reel/assemble.py take.mp4 --out reel_wa.mp4 --crf 27   # ~7 MB copy (WhatsApp inline)
python3 tools/reel/assemble.py take.mp4 --no-export --frames 2,10,20,35   # build only, dev PNGs
```

Options (env fallbacks in brackets):

| flag | default |
|---|---|
| `--out` [`KIMO_REEL_OUT`] | `./kimo_reel.mp4` |
| `--work` [`KIMO_REEL_WORK`] | `./reel_work` (ASR cache, SFX, call logs, `picks.json`, `result.json`) |
| `--kimo-mcp` [`KIMO_MCP`] | `<repo>/target/release/kimo-mcp` |
| `--fonts` [`KIMO_FONTS`] | `<work>/fonts`, built from the repo's bundled OFL fonts (`fonts.py`) |
| `--script` | `tools/reel/script.json` |
| `--crf` | `20` |

The layout expects all 10 lines to be found in the take; the run stops if one is missing.

## Script (`script.json`)

One line = one cut + one caption. Captions are burned from these exact strings (straight
apostrophes are set as typographic ’); ASR text is never used for captions.

1. I built a video editor… that edits by itself.
2. It's called Kimo.
3. No timeline. No dragging clips.
4. You just tell it what you want — and AI makes every cut, every caption, every effect.
5. The reel you're watching right now?
6. Kimo made it. Two hundred tool calls. Zero clicks.
7. Color grade. Motion graphics. Sound mix. All of it.
8. And it's built from scratch. In Rust.
9. Editing is about to change.
10. This is Kimo.

## Performance note

Kimo's HSL effect costs ~3 s/frame at 1080x1920 on CPU (engine perf debt), so the reel uses
keyframed saturation instead; export ~8.5 min on 4 CPUs.
