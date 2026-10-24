# Director mode: the whole edit in about 15 calls

Kimo does the hard parts in one call each. You decide the story: clip order, which takes to drop, the hook title,
the caption words and any labels. Do the steps in this order and keep every path absolute.

Hackathon quickstart (build, run, env knobs, known limits): [HACKATHON_QUICKSTART.md](HACKATHON_QUICKSTART.md).

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
   text it was given -- with the exact `edit_caption {clip_id, text}` call to check and fix it. If no speech model is installed, `auto_captions` returns a clear error naming the fallback (skip captions and keep going, or `caption_from_words` with words you trust) -- never a crash.
8. Optional labels: `add_track {}` then `add_text {track_id, text, start, duration, size: 56}` (1-4 words, at most 3, size 48 or more,
   never at the same time as the title). Plain letters only: emoji and symbols are not in the font.
9. `finish_audio {}` levels the voice and mixes to -14 LUFS.
10. `lint_edit {}` lists real problems, each with the exact call that fixes it. Fix them, then run it again until it is clean.
11. `save_project {path}` then `export {path, crf: 20}` to the final.mp4 path in the brief.
12. `review_export {path}` shows frames from the exported file. Fix the single worst thing you see and export again.
   Do not rebuild the edit from scratch after exporting: change one thing, then export again right away.

`timeline_summary {}` shows the edit at any time. `undo {}` reverts the last call (a whole recipe call is one step).
Times are seconds on the timeline. Don't guess clip ids: use the ones returned to you.
