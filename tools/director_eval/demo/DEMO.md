# Director-mode demo: one script, real footage, no code changes

`run_demo.sh <clips_dir> <out.mp4>` runs the exact kimo-CLI call sequence a small
model in **director mode** would make (see `../director_mode.md` and
`docs/DIRECTOR_QUICKSTART.md`) over a folder of real clips -- no live/network
model call, nothing hand-run, nothing new in `kimo-core`.

```
tools/director_eval/demo/run_demo.sh /path/to/clips out.mp4
```

`clips_dir` holds numbered 9:16 talking-head clips (`clip1.mp4`, `clip2.mp4`, ...).
`director_recipe_example.json` next to the script is the recipe this demo ships
with: which clips to use and in what order, the hook title, the honest per-clip
caption text, and which clip gets the smart-punch + glow-text highlight -- the
shape of JSON a small model in director mode would emit. A different footage set
needs its own recipe file (`--recipe path.json` or `$RECIPE`): the words people
actually say can't be guessed from the file names.

## What it runs, one call per step

1. `project_set` -- 720x1280, 30fps.
2. `import_media` for each clip named in the recipe (`clips`), in story order.
   A clip named in the recipe's `dropped_clips` is intentionally left out --
   picking the best shots *is* part of the recipe.
3. `auto_rough_cut` -- lays the talking clips back to back, dead air cut out.
4. `auto_grade` -- per-clip exposure / white-balance fix.
5. `add_track` + `add_title {style: hook_box}` -- the hook, from the recipe's
   `hook` text.
6. `caption_from_words {by_asset: {...}}` -- captions from the recipe's
   `captions` (clean words per clip, timed to detected speech automatically).
7. `add_smart_punch` + `add_glow_text` on the recipe's `highlight` clip, if one
   is named.
8. `finish_audio` -- dialogue cleaned and levelled, mixed to -14 LUFS.
9. `lint_edit`, applying the first non-cosmetic problem's own suggested fix
   call (up to 3 passes) until clean, except the known `font_substituted`
   caveat below.
10. `export {crf: 20}` to the given output path.

## Proven on real footage

This was run end-to-end against a real 5-clip talking-head set (a sixth clip
dropped as off-topic filler): 8 rough-cut segments, 41.9s, exported to a
13.4-16 MB 9:16 mp4, `lint_edit` clean apart from the font caveat below. The
project file, the asset ids, the exact clip-to-caption mapping and the smart-
punch highlight clip all came from `director_recipe_example.json` -- nothing
footage-specific is hardcoded in `run_demo.py`.

## Honest caveats

- **Fallback font.** The preset fonts (Anton, Montserrat, ...) aren't fetched
  by this script (`fonts/fetch.sh` pulls them from Google Fonts, which this
  demo does not run, to stay deterministic/offline). Titles render in DejaVu
  Sans instead -- `lint_edit` flags this as `font_substituted` and the script
  leaves it; cosmetic only, inside the safe area.
- **Captions are honest paraphrases, not a transcript fix.** On accented or
  informal speech, raw auto-transcription can produce real nonsense fragments.
  `director_mode.md`'s own rule is "never put nonsense on screen": a recipe's
  `captions` should be short, honest, correct words for what was actually
  said -- generic phrasing where a specific word can't be made out, never an
  invented claim about the subject.
- **This is a scripted deterministic run, not a live small model deciding.**
  The ~18-19 tool director-mode profile, `docs/DIRECTOR_QUICKSTART.md`,
  `docs/SMALL_MODEL_PREP.md` and the `director_eval` harness are what's
  already proven to let an actual small model make these same decisions on
  its own (see `tools/director_eval/README.md`); this script proves the
  mechanics run end to end, reproducibly, outside that harness's synthetic
  eval sets.
- **No media is committed here.** Bring your own `clips_dir`.
