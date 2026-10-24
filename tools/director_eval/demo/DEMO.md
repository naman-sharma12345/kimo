# Director-mode demo: one script, real footage, no code changes

`run_demo.sh <clips_dir> <out.mp4>` runs the exact kimo-CLI call sequence a small
model in **director mode** would make (see `../director_mode.md` and
`docs/DIRECTOR_QUICKSTART.md`) over a folder of real clips -- no live/network
model call, nothing hand-run, nothing new in `kimo-core`.

```
tools/director_eval/demo/run_demo.sh /path/to/clips out.mp4
```

`run_demo_mcp.py` runs the identical call sequence over real MCP `tools/call` JSON-RPC against a
spawned `kimo-mcp` (stdio), instead of shelling out to the `kimo` CLI -- the actual transport a
small model drives director mode through. It lists `tools/list {mode: "director"}` first (the
profile a small model actually sees), records each call's latency and any tool error in a JSON
report (`--report path.json`), and treats "no speech model installed" on `auto_captions` as a
handled outcome: the edit ships without captions (reported honestly, not invented) instead of the
whole run aborting.

```
tools/director_eval/demo/run_demo_mcp.py /path/to/clips out.mp4 --kimo /path/to/kimo-mcp --report report.json
```

`clips_dir` holds numbered 9:16 talking-head clips (`clip1.mp4`, `clip2.mp4`, ...).
`director_recipe_example.json` next to the script is the recipe this demo ships
with: which clips to use and in what order, the hook title, caption options
(style/hint for kimo's own recognizer -- see below), and which clip gets the
smart-punch + glow-text highlight -- the shape of JSON a small model in director
mode would emit. A different footage set needs its own recipe file
(`--recipe path.json` or `$RECIPE`) for the hook/highlight text and clip order;
it does **not** need its own captions -- captions come from kimo's own speech
recognition, not a hand-typed transcript (see below).

Hackathon quickstart (build, run, env knobs, known limits): [../HACKATHON_QUICKSTART.md](../HACKATHON_QUICKSTART.md).

## What it runs, one call per step

1. `project_set` -- 720x1280, 30fps.
2. `import_media` for each clip named in the recipe (`clips`), in story order.
   A clip named in the recipe's `dropped_clips` is intentionally left out --
   picking the best shots *is* part of the recipe.
3. `auto_rough_cut` -- lays the talking clips back to back, dead air cut out.
4. `auto_grade` -- per-clip exposure / white-balance fix.
5. `add_track` + `add_title {style: hook_box}` -- the hook, from the recipe's
   `hook` text.
6. `auto_captions {clip_ids: [...the rough-cut's own segments...]}` -- kimo's
   own speech recognition (beam search, a Hinglish prompt, a Hindi/Urdu/
   Punjabi/Marathi pass, per-word confidence), styled/hinted from the recipe's
   `captions` block. No hand-typed transcript, no `caption_from_words`, no
   `edit_caption` paraphrases: this is what the speaker actually said, as
   kimo's recognizer actually heard it.
7. `transcript_get {include_words: true}` on the caption's transcript --
   this script's own read of which words came back flagged `low_confidence`,
   so they can be listed honestly in the run's output (see below) instead of
   silently trusted or silently replaced.
8. `add_smart_punch` + `add_glow_text` on the recipe's `highlight` clip, if one
   is named.
9. `finish_audio` -- dialogue cleaned and levelled, mixed to -14 LUFS.
10. `lint_edit`, applying the first non-cosmetic problem's own suggested fix
    call (up to 3 passes) until clean, except the known `font_substituted`
    caveat below. When `lint_edit`'s fix is a template for a human/small model
    to fill in (marked by `<angle-bracket>` placeholders -- e.g. "re-caption
    this with the real words") the script uses the concrete `or` alternative
    instead (a position/size fix, never invented caption text); if neither is
    usable it stops and reports rather than guess.
11. `export {crf: 20}` to the given output path, and a small JSON report
    (`captions: {total_words, low_confidence_count, low_confidence_words}`)
    is printed alongside the normal export result.

## Proven on real footage

Run end-to-end against Naman's real 5-clip talking-head set (a sixth clip
dropped as off-topic filler): 8 rough-cut segments, 41.9s, exported to a
14.3 MB 9:16 mp4, `lint_edit` clean apart from the font caveat below.

**Caption quality.** kimo's own recognizer (the multilingual `ggml-small-q5_1`
model already on disk, Hinglish prompt, Hindi-pass routing -- nothing
downloaded) heard 127 words at an average confidence of 0.66, language-tagged
`ur` (Hinglish/Hindi content written in Roman letters) with a touch of English
and Bengali-leaning phonemes in the mix (`mode: hinglish+en+bn`). 32 of those
127 words (25%) came back flagged `low_confidence` and are listed, verbatim,
with their timestamps, in this run's own report -- not invented replacements,
not silently hidden. Reading the actual frames (`proof/kd2_contact_sheet.png`
in the task this demo was built under): the captions are coherent, honest
Hinglish as spoken -- "isme paani boh zyada aata hai, lekin ye plastic ki hai"
("this has too much water in it, but it's made of plastic"), "ye khola yahan
se" ("open this from here") -- not the real nonsense an outside, non-Hinglish
transcript produced on this same footage before (`bohozhada`, `kettlebanyan`,
`panipitam`). The rough patches that remain are honestly rough, not invented:
a run of flagged words ("badu", "aar", "yo" around 30s) lands on screen exactly
as the recognizer heard it, flagged, at the exact point a listen confirms the
audio itself is hard to make out -- the system told the truth about where it
struggled instead of papering over it with a guess.

## Honest caveats

- **Fallback font.** The preset fonts (Anton, Montserrat, ...) aren't fetched
  by this script (`fonts/fetch.sh` pulls them from Google Fonts, which this
  demo does not run, to stay deterministic/offline). Titles render in DejaVu
  Sans instead -- `lint_edit` flags this as `font_substituted` and the script
  leaves it; cosmetic only, inside the safe area.
- **25% of caption words are flagged low-confidence on this footage** (32 of
  127; see above) -- real, informal, accented speech is genuinely hard to
  recognize in spots. This script reports exactly which words and when rather
  than guessing a fix; a human (or a small model with the real audio in
  front of it) is the one who should decide whether to touch them.
- **This is a scripted deterministic run, not a live small model deciding.**
  The ~18-19 tool director-mode profile, `docs/DIRECTOR_QUICKSTART.md`,
  `docs/SMALL_MODEL_PREP.md` and the `director_eval` harness are what's
  already proven to let an actual small model make these same decisions on
  its own (see `tools/director_eval/README.md`); this script proves the
  mechanics run end to end, reproducibly, outside that harness's synthetic
  eval sets.
- **No media is committed here.** Bring your own `clips_dir`.
