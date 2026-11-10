# Raw audio

## Layout and precision

All read API JSON arrays are `data[channel][sample]`. Base64/raw/NPY bytes are channel-major little-endian IEEE754 f32: all samples of channel0, then channel1. NPY shape is `(channels, samples)` in C order. Hash is SHA256 of those same planar bytes, without headers. WAV files use interleaved little-endian float32, format tag3. The existing mixer and export use interleaved stereo f32 at48000. Never interpret planar raw bytes as stereo-interleaved PCM.

Imports default to planar. Explicit `layout:"interleaved"` accepts flat JSON or interleaved base64, then converts to internal planar import buffers. `base64_s16le` divides by32768. `base64_f64le` rounds to f32 and rejects conversion overflow; f64 read is not offered. JSON round-trips finite f32 values. Signed headroom above1 survives import and post_filter reads; clipped_count is informational. There is no automatic normalization or clipping.

Read rate accepts only48000. A native24k mono file is decoded through the existing strict stereo48k mixer path, with native rate/channels reported in source_info. No new resampler is introduced. Channel selectors all/0/1/[0,1]/mono select the decoded form; mono averages L/R. Import stores native float32 WAV rates1000..384000 and channel counts1..64, then existing media decode handles playback. Bit-equal import->source-read is claimed only for conformant48000 stereo f32 input.

## Targets and stages

Targets are clip:<id>, track:<id>, mix, file:<path>. Clip start/end are clip-local seconds, track/mix timeline, file source. source clears clip audio controls before existing strict decode; post_filter includes filters before volume/pan/duck; post_mix includes those controls and existing mixer limiter. Track/mix support post_mix only. A full decode is subject to total max_samples even if a small returned slice is requested. max_samples is a per-call total across channels, exposed through effective limits and set_limits. JSON reads cap4096 samples/channel; choose base64 or file for more. Peaks return channel-major `[min,max,rms]` buckets using integer equal partitions.

Assets are durable hash-addressed WAVs in project assets/audio, KIMO_ASSET_DIR/audio, XDG_DATA_HOME/kimo/assets/audio or ~/.local/share/kimo/assets/audio in that order. Output reports store rule and absolute path. Imports are one project undo step; file remains available for redo, deduplicated by hash. Asset metadata records kind audio, origin import_samples and sample hash. Existing dependency collection copies the file.

## Raw clip effects

add_audio_effect supports biquad, fir and mix_matrix. Biquad sections are `{b0,b1,b2,a1,a2}`, a0=1, DF-II transposed f64 state, f32 samples. FIR is `sum(coeffs[k]*x[n-k])` with zero prehistory and max65536 coefficients. Matrix is fixed-stereo2x2, out[o]=sum(matrix[o,i]*in[i]), f64 sum then f32 output, unclamped headroom. Defaults identity. Raw finite coefficients have no cutoff/Q clamp. No frequency/Q convenience exists; any future convenience must reject cutoff>=23500 or Q<=0.05 rather than silently clamp.

set_audio_effect_key applies linear clip-local keys to sections.N.b0/b1/b2/a1/a2, coeffs.N and matrix.OUT.IN. Feedback instability is not creatively guarded; non-finite output is rejected, never replaced with zero on reads. Existing strict mixer limiter applies only post_mix/export, not raw filter headroom.

audio_filter_response computes pure analytic magnitude/phase for static sections or FIR. Frequencies are below Nyquist. Zero-magnitude dB is null representing negative infinity. Measured rendered-sine response at100/1000/10000Hz agrees within0.2dB and native DSP within0.05dB for the tested filter. PCM16/24 export tests bound DC mean to1LSB and zero-crossing delta error to2LSB; this is a fixture result, not a blanket guarantee for all signals or integer export settings.

Asset paths include native shape: `audio/<rate>hz-<channels>ch/<hash>.wav`. Results include `asset_key="<rate>hz-<channels>ch/<hash>"`. The same planar PCM byte hash can describe different native shapes, so those are distinct assets by design. Dedup applies within identical rate/channel shape. Both stored WAVs preserve their imported samples; only conformant48kstereo read matches native bytes. Nonconformant reads match existing strict decode, not native bytes.
