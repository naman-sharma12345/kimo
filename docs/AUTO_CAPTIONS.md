# Automatic captions

`auto_captions` turns the speech in the talking clips into word-synced, animated caption pages in one call.

```
auto_captions {}                                   # every audible media clip, style pop, language auto
auto_captions {"style": "karaoke", "hint": "Kimo"} # hint = names or terms the speaker says
edit_caption_words {"edits": [{"find": "kimmo", "text": "Kimo"}]}
export_captions {"path": "/out/captions.srt"}
```

## Setup

Speech recognition runs on your machine: whisper.cpp's `whisper-cli` (MIT) with an OpenAI Whisper model (MIT weights).
Neither ships in the repo. Fetch them once:

```
bash tools/whisper/fetch.sh               # small multilingual model, 181 MiB (default)
MODEL=turbo bash tools/whisper/fetch.sh   # large-v3-turbo, 547 MiB: better on Hindi / Hinglish
```

They land in `~/.cache/kimo_models` (or `$KIMO_MODELS`). `KIMO_WHISPER_BIN` and `KIMO_WHISPER_MODEL` override the paths.
The model file's sha256 is checked on download. If no recognizer is found, the tool says how to get one. You can also
pass `words: [{text, start, end}]` to caption words you already have.

## What the call does

1. Reads each clip's audio (its in point, duration and speed) at 16 kHz mono.
2. Identifies the language (`language: "auto"`), then runs one recognition pass in the right mode. Hindi, Urdu,
   Punjabi and Marathi speech is written in Roman letters, which is how Hinglish is typed (`script: "native"` keeps
   Devanagari). English words stay English. Common Hinglish words the recognizer spells several ways get one spelling
   (bahut / bohat -> bohot, pani -> paani, ek dum -> ekdum), keeping capitals and punctuation; `normalize_spelling: false`
   keeps what was heard. English-mode captions are never respelled.
3. Refines word start times against the audio. whisper's timestamps run late, so the start is corrected for that
   bias and then snapped to the nearest speech onset in the waveform.
4. Splits the words into pages, at most N words per line (pop: 3). A page ends at sentence ends, clause commas and
   pauses, and a short last word is not left alone on its own page. 1-2 stress words per page are picked as keywords.
5. Places the captions in the lower band of the frame (70% down on 9:16, clear of the app button strip). They move to
   the top when a face sits low in the shot.
6. Builds one text clip per page on a new "Captions" track. Each word is bound to a transcript word, so
   `transcript_edit_word`, `validate_captions`, `export_with_subtitles` and burn-in work as usual.

## Styles

| style | look | motion |
|---|---|---|
| `pop` (default) | Montserrat Black, caps, black outline; spoken word yellow, keyword green | each word pops in (overshoot) as it is spoken |
| `karaoke` | two lines, white; spoken words fill yellow | the active word pulses |
| `boxed` | caps on a dark rounded box; spoken word green, keyword yellow | the active word pops |
| `impact` | Anton, very large, caps, 2 words | each page punches in |
| `clean` | Inter Bold, sentence case, soft shadow | pages fade in, no word motion |

You can override `color`, `active_color`, `keyword_color`, `font`, `size`, `max_words_per_line`, `lines`, `highlight`
and `keywords`. Use `replace: <caption_id>` to swap in a new style group.

## Measured accuracy (2026-10-21)

| test | result |
|---|---|
| English speech with known word times (espeak-ng), full tool path | WER 1.8%. Word-start error: mean 40 ms, median 26 ms, max 138 ms. Raw whisper timestamps: 248 ms mean, 481 ms max |
| Naman's six Hinglish phone clips (153 words), small model, against a hand-checked Roman reference | WER 45.8% strict, 39.9% ignoring spelling variants. Hindi mode (Devanagari) dropped words. The same clips through a hosted whisper-1 scored 23.5% / 18.3% |
| The same clips with the spelling normaliser (full tool path) | WER 39.2% strict (was 47.1%), 35.3% ignoring spelling variants (was 38.6%). The word table was written after reading these clips' output and is scored on the same words, so expect a smaller gain on new speech |

The small model is weak on Hinglish. `MODEL=turbo` is the upgrade for Hindi speakers. To fix a word without moving its
timing or animation, use `edit_caption_words`. `fast: true` decodes greedily: about 3x faster, at 52% WER on the same
clips.
