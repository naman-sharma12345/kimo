# Captions, audio mixing, multicam

All features below are registered MCP tools. Project `editorial` metadata and the
new audio fields use serde defaults, so old project JSON remains loadable.

## Captions

`add_captions(track_id, source, path, style)` calls the existing SRT importer.
`source="transcribe"` first calls the existing Whisper CLI transcription tool and
requires `out_srt`. Whisper must be installed separately. The new tools do not
claim to implement a speech model. SRT is validated before import; malformed cues
fail rather than silently dropping text.

Styles: `subtitle`, `boxed`, `yellow`. Optional font path, size and vertical pixel
offset. Cues remain normal timed text clips, editable through `edit_caption` or
existing clip/keyframe tools. `list_captions` returns groups and cue IDs.
`set_caption_style` applies a preset to a group. `burn_in(caption_id, enabled)`
controls caption visibility in preview and rendered exports, without deleting
editable objects. `export_with_subtitles` adds independent selectable streams (see below).

## Audio

`add_audio_effect(clip_id, kind, params)` validates a parameterized effect:

| Kind | Parameters and defaults |
|---|---|
| eq | frequency 1000 Hz, width 1 Q, gain_db 0 |
| compressor | threshold_db -18, ratio 3, attack_ms 10, release_ms 200, makeup_db 0 |
| deesser | intensity .5, max .5, frequency .5 (FFmpeg normalized controls) |
| reverb | decay .3, delay_ms 60 (room scaling, 6..240 ms, 0.1..4 room scale) |
| denoise | reduction_db 12, floor_db -50 |

EQ/compressor/deesser/denoise use FFmpeg filters in insertion order. Reverb uses
stereo Schroeder damped combs and allpasses on decoded PCM after the FFmpeg chain.
This ordering is explicit: reverb is not interleaved with the FFmpeg filters.
Reverb output is cropped at the clip end; it does not extend timeline duration.
`remove_audio_effect` removes by index. Saved effects affect audio export and video
exports through both legacy and strict mixers.

`set_audio_pan` is stereo balance, -1 left through +1 right, attenuating the
opposite channel. It is not a constant-power mono-to-stereo spatializer.

`measure_loudness` reports EBU R128 stereo integrated loudness using 48 kHz
K-weighting, 400 ms blocks/100 ms hops, -70 LUFS absolute and -10 LU relative gates.
Clips measure post-filter but pre-volume/pan. Without clip_id, it measures the final
mixed/limited timeline. Silent or <400 ms PCM returns null. The reported peak is
sample peak. Optional `true_peak=true` adds a four-times oversampled estimate
using a normalized 64-tap Blackman-windowed sinc for each fractional phase.
It includes original sample peaks, detects intersample overs, and is not a
certified meter. Normalization still guards sample peak, not true peak.

`normalize_loudness(clip_id, target_lufs)` replaces volume with a static measured
gain, refusing targets that exceed -1 dBFS sample headroom. Compress first or lower
the target on refusal. Existing volume keys are intentionally replaced; duck gain
is separate. Remeasure the final timeline, since mixing and limiting can change it.

`auto_duck(track, under)` measures the other track's audible RMS in 50 ms blocks,
then writes separate attack/release-smoothed `duck_gain` keys. Volume keys are
preserved. It is a baked envelope, not a runtime sidechain: rerun after dialogue
edits. Muted tracks are excluded from audio mix in both paths.

## Multicam

`sync_clips(clip_ids, by="audio")` uses unprocessed source-in-point RMS envelopes
at 5 ms hops with bounded lag correlation. It rejects silence, weak correlations,
and ambiguous repeating envelopes before changing starts. Defaults: first 30 s,
max_offset 5 s, min_correlation .75. This is an incremental CPU sync algorithm,
not sample-accurate phase alignment or a speech-aware matcher.

`by="timecode"` takes explicit source-in-point `timecodes` matching clip order,
SMPTE `HH:MM:SS:FF`, integer fps by default. `drop_frame=true` uses
`HH:MM:SS;FF` at 30000/1001 or 60000/1001 fps, including rounded 29.97/59.94
within .001 fps. It counts frame labels rather than pretending nominal 30/60
is the actual rate. Skipped frame labels at non-tenth-minute boundaries are
rejected. Embedded metadata extraction and midnight wrap are not implemented. Normal 1x clips only for either sync mode.

`add_multicam(angles, audio_angle=0)` moves the source clips into a persistent group
and creates a dedicated track covering their common synchronized interval.
Original angle recipes are retained in project metadata. First video angle is 0.
A continuous fixed audio angle is copied to an invisible clip; generated video
segments are audio-muted to prevent doubling. Edges outside common overlap are
not silently guessed or filled.

`switch_angle(multicam_id, t, angle)` uses absolute timeline seconds and zero-based
angle indexes. Video cuts rebuild that group's generated track. Audio remains
continuous from the chosen fixed angle. Manual edits on the generated track will
be replaced by later switches. `list_multicam` exposes recipes and switch lists.

New mutating tools restore the project on ordinary errors, in addition to the
existing sandbox panic rollback. Undo creates one snapshot per operation, not
nested snapshots for the existing import/transcribe calls.

## Proof

`cargo test --workspace` covers baseline behavior and nine new editorial tests.
The tests run all five effects through decoding, compare LUFS to FFmpeg loudnorm
within 0.15 LU, verify normalization, stereo pan and ducking, test audio sync on
actual modulated PCM and ambiguity refusal, verify non-drop timecodes, continuous
multicam audio, cue editing/persistence/undo and registry uniqueness. An impulse
reverb test verifies diffuse tail, channel decorrelation, and finite samples.

`python3 tests/gap4_proof.py --binary target/debug/kimo-mcp --output /tmp/gap4-proof`
runs a stdio MCP session, renders a red-to-blue multicam switch and caption burn-in
on/off, exports audio and saves the project. Requires FFmpeg and Pillow. These are
synthetic behavior proofs, not a commercial-editor parity claim or a listening
quality assessment. Whisper transcription is not exercised without a model/CLI.


## Selectable subtitle export

`export_with_subtitles(path, caption_ids?, start?, end?, crf?)` renders H.264/AAC
then muxes one subtitle stream per selected group: MP4 `mov_text`, MKV SubRip.
Omit caption_ids for all groups (1..32). Existing burn-in settings are independent:
turn them off for soft-only output, or leave on for both. Selectable text keeps
cue content and timing, not the burn-in styling presets. Titles identify groups;
no language or default-on preference is guessed. Unicode is passed as UTF-8.
Range clips and shifts cue times to zero, matching the rounded video-frame range.
Missing/deleted/nested cues, literal angle brackets (SRT markup ambiguity), blank text separators, or groups with no cues in range
fail before export. Temporary files are removed; final output replaces its target
only after successful mux, preserving an earlier target on failure.

This export uses the existing ordinary H.264 SDR delivery contract, not the
high-bit/HDR preset path. It does not support ASS styling or subtitle editor
metadata. Individual players decide how to present/select the streams.

Follow-up tests verify MP4/MKV subtitle codecs and extracted cue timings,
atomic failure preservation, true peak against FFmpeg ebur128 on three tones
within .3dB, a known intersample-over fixture, and 29.97/59.94 label boundaries.
`tests/gap4_followup_proof.py` runs these surfaces over MCP, then renders the
same soft-sub MP4 without subtitles and with its selectable stream enabled.
