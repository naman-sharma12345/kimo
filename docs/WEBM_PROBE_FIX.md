# WebM import duration fix

Base: main 177506a. Scope: metadata probe only; decoder behavior unchanged.

## Reproduction and cause

Original faceocc2 fixture:
https://raw.githubusercontent.com/opencv/opencv_extra/5.x/testdata/cv/tracking/faceocc2/data/faceocc2.webm

ffprobe's selected video stream has width 320, height 240, frame rate 25/1,
and missing duration. Container duration is 32.480000 seconds.
Old CSV output is `320,240,25/1,N/A`. The old probe falls back to container
metadata only when there are fewer than three fields. Four valid video fields
therefore skip fallback and parse N/A as zero. import_media stores duration=0.
Using that returned duration to create a clip makes tracking reject it.

## Fix

One ffprobe JSON invocation queries selected video-stream metadata and
container duration together. Named field parsing avoids positional CSV
assumptions. A positive finite stream duration remains preferred; when absent,
invalid or zero, use a positive finite container duration. Finite zero remains
supported for still images. Audio-only media retains zero dimensions/fps and
uses container duration. Check ffprobe's exit status and JSON parse errors;
invalid video dimensions are errors rather than synthetic zero-size videos.
Known fractional frame rates stay exact. Unknown frame rates retain the
existing 30fps default. Existing TIFF special case is unchanged.

No full-media decoding to estimate duration. A container duration is the
container's span, potentially longer than a selected stream in a multi-stream
file; this fix is not an exact packet-count or VFR-timestamp duration engine.
When both duration fields are unavailable for video, the existing zero-duration
behavior remains (including undated still images). No invented duration.

## Tests run

- Real original WebM regression on unmodified 177506a: FAILS at import duration
  assertion before decode (expected 32.48 seconds).
- Same regression after patch: PASSES. import_media returns 320x240, 25fps,
  32.48 seconds; a clip created from that duration decodes a late window and
  renders at 32.40 seconds without re-encoding.
- 95 core-library tests pass, including two new named-metadata parser tests.
- Three automatic integration tests pass: generated VP8 WebM lacking stream
  duration, PNG/audio compatibility, invalid-path import without mutation.
- Eleven existing decode/cache regressions pass: color_cache (5),
  color_decode_phase (4), color_ramp_window (2).

Reproduce the external-fixture regression explicitly (not silently skipped):

```
curl -L https://raw.githubusercontent.com/opencv/opencv_extra/5.x/testdata/cv/tracking/faceocc2/data/faceocc2.webm -o /tmp/faceocc2.webm
KIMO_FACEOCC2_WEBM=/tmp/faceocc2.webm cargo test -p kimo-core --test webm_probe -- --ignored
```

The real-footage test is ignored in the default suite to avoid network access
and a multi-megabyte fixture in the repository. It requires the original
fixture explicitly; a missing environment variable fails that test. The
normal generated-WebM regression requires ffmpeg/libvpx and ffprobe.
