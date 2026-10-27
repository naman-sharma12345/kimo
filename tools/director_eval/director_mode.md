# Director mode: the whole edit in about 15 calls

Kimo does the hard parts in one call each. You decide the story: clip order, which takes to drop, the hook title,
the caption words and any labels. Do the steps in this order and keep every path absolute.

1. `project_set {width: 720, height: 1280, fps: 30}`
2. `import_media {path}` for each clip, with these exact paths (copy them, do not shorten them):
   {footage_list}
   Keep each asset_id and duration.
3. `auto_rough_cut {asset_ids: [...in story order...]}` lays the talking clips back to back with the dead air cut out.
   It returns `track_id` and `segments` (each with asset_id, index, seconds). To drop a repeated line or a false start, call it again with
   `{asset_ids, track_id, drop: [{asset_id, index}]}`. A short is tighter than the raw footage: drop repeats.
   Blurry and whip-pan edges are trimmed for you. A segment with `similar_to` looks like one already in the cut: drop it, or pass `dedupe: true`.
4. `auto_grade {}` fixes dark or tinted shots and leaves good ones alone.
5. `stabilize {clip_id}` only for a clip that is clearly shaky (optional; each call takes a few seconds).
6. Hook title: `add_track {}`, then `add_title {track_id, style: "impact", text: "<2-4 word hook>", start: 0, duration: 1.8}`.
   One title only. To change it, `delete_clip {clip_id}` the old one first; never stack a second title on top.
7. Captions: if the brief gives you no transcript (or you do not trust the one it gives you, e.g. accented or informal
   speech), prefer `auto_captions {clip_ids: [<the rough-cut segments' clip ids>]}` -- it recognizes the speech itself
   (beam search, a Hinglish prompt, a Hindi pass for Hindi/Urdu/Punjabi/Marathi) and already flags words it heard with
   low confidence (`low_confidence_words` in the result). Otherwise, `caption_from_words {track_id: <the rough-cut track>,
   by_asset: {"<asset_id>": "<the words spoken in that clip>", ...}}` times YOUR words to the detected speech, each clip's
   words only over that clip's shots -- write clean, correct words for each clip (an auto-transcript in the brief may be
   wrong: never put nonsense on screen). Either way, run `lint_edit` before exporting: it now also flags
   `caption_low_confidence` -- a cue whose audio kimo's own recognizer genuinely cannot make out confidently, whatever
   text it was given -- with the exact `edit_caption {clip_id, text}` call to check and fix it. If no speech model is installed, `auto_captions` returns a clear error naming the fallback -- but prefer calling it a second time with `words: [{text, start, end}]` you supply yourself (even a rough even split of the brief's known sentence across the clip's detected speech window, from `auto_rough_cut`'s own `speech`/`segments` ranges, is fine) over switching to `caption_from_words`: `caption_from_words` has no `keyword_color` at all, so it cannot produce the keyword-pop styling the hard rules require, while `auto_captions` with explicit `words` still gets you `style`, `keywords` and `keyword_color` with zero dependency on a recognizer being installed.
8. Optional labels: `add_track {}` then `add_text {track_id, text, start, duration, size: 56}` (1-4 words, at most 3, size 48 or more,
   never at the same time as the title). Plain letters only: emoji and symbols are not in the font.
9. `finish_audio {}` levels the voice and mixes to -14 LUFS.
10. `lint_edit {}` lists real problems, each with the exact call that fixes it. Fix them, then run it again until it is clean.
11. `save_project {path}` then `export {path, crf: 20}` to the final.mp4 path in the brief.
12. `review_export {path}` shows frames from the exported file. Fix the single worst thing you see and export again.
   Do not rebuild the edit from scratch after exporting: change one thing, then export again right away.

`timeline_summary {}` shows the edit at any time. `undo {}` reverts the last call (a whole recipe call is one step).
Times are seconds on the timeline. Don't guess clip ids: use the ones returned to you.

## A note on `lint_edit`'s `fix.tool` and `search_tools`

Director mode gives you about 35 tools, declared to you up front. `lint_edit`'s own `fix` field and
`search_tools`'s matches are generated against kimo's **full** ~447-tool registry, not this curated list --
so they can and do name a tool (seen in practice: `set_text_style`, `set_caption_style`) that isn't one of
your 35. If your tool-calling runs through a function-calling API (the usual way), you can only ever emit a
call for a function that was declared to you -- a name outside this list is real and `kimo_describe` will
happily explain it, but you cannot actually call it here, no matter how many times you look it up.

If you hit `text_outside_safe_area`, `text_over_subject`, or a font/style complaint from `lint_edit` whose
fix names a tool not in your list: don't keep searching or describing it. Instead, `delete_clip` the
offending text/title clip and recreate it with `add_title`'s or `add_text`'s own styling args (`font`,
`color`, `size`, `shadow`, `letter_spacing`, shorter `text` for width) at creation time, then use
`set_property` only for position/scale/opacity/rotation afterward. `font_substituted` on its own is
informational (a safe fallback font is already drawing) -- nothing to fix unless you were asked for an exact
font.
