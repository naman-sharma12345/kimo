# Small-model prep: director mode

Kimo exposes several hundred tools. A frontier model can find its way through them. A small, fast model mostly can't: it spends
its calls on discovery, mistakes, and repairing its own errors. Director mode is a smaller surface for those models: one-call
recipes for the hard parts, a curated profile of about 18 tools, a lint that names the fix, and a short step guide. The model
decides the story. Kimo does the mechanics.

## What we measured (real set: six phone clips, a 9:16 "my favorite bottles" short)

The judge sees 16 frames of each edit, plus two anchors: the raw clips joined end to end (bad) and the Opus edit (reference).
It scores out of 10, two samples per edit. v2 composite = 0.65 x judge% + 0.35 x measured checks (exposure, loudness, peaks,
dead air, caption sync, cuts on rhythm, text in the safe area). It is capped at 40 when the judge gives 3/10 or less.

| run | tools | judge /10 | v2 /100 | calls | tool errors |
|---|---|---|---|---|---|
| Opus, raw tools (R0) | grouped, full | 5 | 59.1 | | |
| Haiku 4.5, raw tools (R1) | grouped, full | 3 | 40 | | |
| Muse Spark Open, raw tools (R2) | grouped, full | 3 | 40 | 121 | 28 |
| Muse Spark Open, director mode (D1) | director, 17 | 4 | 55.2 | 92 | 1 |
| Muse Spark Open, director mode (D2) | director, 18, fonts installed | 4.5 | 59.4 | 80 | 0 |
| Muse Spark Open, director mode (D3) | director, 19, blur trim, per-clip captions, balanced cues | 6 | 69.9 | 34 | 0 |
| raw clips joined (anchor) | | 1 | 0 | | |

In D2, Muse matched Opus-on-raw-tools on the v2 composite (59.4 vs 59.1). In D3 (blur trimming, captions per source clip,
balanced two-line cues, grades updated in place) it passes it: judge 6/10 against 5/10, v2 69.9, in 34 calls with no failed
tool call, about $0.16 per edit. Remaining judge complaints: some motion blur kept inside segments, text placed over the
subject, a weak hook.

## What the small models got wrong, and what Kimo does about it

| failure (seen in the eval) | fix in Kimo |
|---|---|
| black frames: a dissolve on a butt cut fades in from nothing | lint `transition_from_black` names the cut, and `remove_transition` fixes it |
| an emoji drawn as a box (the font has no glyph for it) | text skips glyph 0; lint `missing_glyph` names the clip |
| captions and labels too small to read on a phone | lint `text_too_small` (minimum = min(w,h)/18) |
| a second title stacked on the first, unreadable | lint `text_overlap` with `delete_clip` as the fix |
| captions drift onto the wrong shots | `caption_from_words {by_asset: {asset_id: words}}` keeps each clip's words on that clip's shots |
| captions put real ASR nonsense on screen (accented/informal speech: "bohozhada", "kettlebanyan") | `auto_captions` recognizes the speech itself (Hinglish prompt, beam search, a Hindi pass) instead of trusting an outside brief transcript; either way `lint_edit`'s `caption_low_confidence` flags a cue the recognizer genuinely cannot make out, with the `edit_caption` call to fix it |
| captions cut into tiny pieces with a lone last word | balanced line breaks (fewest slack, never one dangling word, a comma preferred as a break), two-line cues, cue ends snapped to speech pauses, no cue shorter than 0.8 s |
| blurry and whip-pan shots kept | `auto_rough_cut` measures each frame (Laplacian variance, frame change) and trims soft edges of a segment, drops a mostly soft one (`soft: "flag"` only reports) |
| repeated near-identical shots | `auto_rough_cut` marks a segment `similar_to` another clip's kept segment (64-bit difference hash); `dedupe: true` drops it |
| re-grading an already crushed grade | `auto_grade` resets the wheels when an earlier grade crushed a well-exposed source |
| a preset font missing, so a fallback font is used | `add_title` and lint say so; set KIMO_FONTS or run fonts/fetch.sh |

## Harness lessons (tools/director_eval)

- Small models end their turn without exporting. The harness enters a finish phase at 72% of the call or time budget (save, then
  export; previews are refused until final.mp4 exists). It nudges once if the model stops early, and exports itself as a last
  resort (`harness_finished` in run.json).
- Long sessions can OOM the Kimo process during export on a shared 4 GB box. The harness autosaves the project after every change
  and reloads it if the server restarts (`kimo_restarts`).
- Muse kept rebuilding the edit after a good export (D2: its last 50 calls rebuilt without exporting again). The guide now says:
  change one thing, then export again.
- Nemotron 30B-A3B mangled absolute paths. The guide now lists every footage path in full. Nemotron is dropped from this round.

## Next

1. Soft stretches in the middle of a segment are only reported. Splitting a segment there would cut words, which needs word timing.
2. Title presets that read as a designed hook on 9:16, not plain white type.
3. Done: `auto_captions` (word timings, Hinglish-aware) has landed; director mode prefers it when no trustworthy transcript
   is given. `caption_from_words` stays for given text, and now (KA1) cross-checks that text against the recognizer's own
   confidence on the same audio, flagging a cue (`caption_low_confidence` in `lint_edit`) it genuinely cannot make out.
4. Done: text placement that avoids the subject (titles and labels landed on the bottles in D3). `lint_edit`'s
   `text_over_subject` finding now warns when a title/caption overlaps the frame's strongest subject (reusing
   `add_smart_punch`'s saliency map) and suggests a new `y`. Limit: it only checks the single strongest subject in
   frame, so a hard-edged object can still be missed.
5. Done: filler-word trim. The new `trim_filler_words(clip_id, spans?, words?, pad?)` tool (in the `timeline` group,
   not yet on the director-mode list) ripple-deletes flagged word spans from a clip's own footage. It now retimes
   caption/text clips `auto_captions` built from that clip's own words too (`retime_captions`, default true): deletes
   any entirely inside a removed span, trims one straddling a cut, shifts ones after it left to close the gap --
   `auto_captions` no longer needs to be re-run after trimming. Pass `retime_captions: false` for the old (untouched,
   drifting) behaviour.

## Running it

    python3 tools/director_eval/run_eval.py plan.json out_dir      # one model at a time on a 4 GB box
    # plan entries: model, adapter, kimo (binary), docs [director_mode.md], tools "director", extra ["--allow", ..., "--fonts", dir]
