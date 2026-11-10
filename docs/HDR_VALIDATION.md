# HDR numerical validation (review workstream)

Base: 205e0137f4018895d59ac90f4008cca2c65bcf21. This is numerical and decoded-signal evidence, not display calibration, mastering certification, dynamic metadata support or HDR industry parity.

## Explicit tools and APIs

- `validate_hdr_transfer_math`: fixed inclusive 65536-sample grid. PQ compares both directions with independent rational BT.2100 constants, at reference whites 1/100/203/1000/10000 nits. HLG reference computes b/c from a, unlike existing rounded implementation constants. PQ black encode is approximately7.30956e-7, not exact code zero; report includes this rather than removing the endpoint.
- `evaluate_hlg_reference_display`: separate read-only Rec2020 scene-linear RGB to display-linear RGB nits OOTF. Uses luminance gamma, not channel gamma. Zero-black reference surround only,400..2000 nit peak, gamma1.2 at1000. Unsupported extended-range peak, nonfinite input and RGB excursions return errors. Does not change transfer7, project defaults or encoding paths. The API also applies to explicitly declared Rec2020 scene-linear frames, preflighting before writes and preserving alpha.
- `audit_pq_linear`: pre-encoding diagnostic on explicitly declared Rec2020 display-linear RGB in reference-white units. Counts negative RGB (gamut cone excursions), over10000nit range excursions, nonfinite/invalid alpha. Content-light comparison is unavailable for invalid/negative frames. Transparent pixels excluded, alpha not used as a light multiplier. This is NOT whole-clip metadata measurement. `max_fall_exceeded` uses mean(maxRGB), not CIE Y. Zero content-light metadata is unspecified. Exceeding mastering peak is informational, not confused with exceeding MaxCLL.

## Outside-engine proof

From repo root, with cargo/python3/ffmpeg/libx265/ffprobe installed:

```
cargo test -p kimo-core --test hdr_validation --test color_managed --test color_metadata --test color_delivery --test color_pipeline --test video_delivery
python3 tools/hdr_validate.py fixture /tmp/kimo-hdr-proof
python3 tools/hdr_validate.py audit /tmp/kimo-hdr-proof/pq-hevc.mp4 --max-frames 500
```

The Python oracle imports no engine code. It supplies known-nit linear float ramps/checkers to a Rust example which exercises TIFF import, timeline, configured output transform and both high-bit exporters. It independently decodes raw RGB16 or10bit YUV420 with declared narrow-range BT2020NC matrix, then PQ to nits. Reports max/p50/p95/p99 error, neutral-ramp and interior-neutral errors, independent ideal quantization floors, clipping probes, tags, range excursions and first-frame static SEI. No metadata rewrite.

Whole-clip audit includes every decoded active-image pixel, including black, in maxRGB MaxCLL/mean(maxRGB) MaxFALL. No alpha exists in supported encoded formats. Cropping active-image area is explicit (`--crop x y width height`); letterbox bars are not inferred. Missing/unsupported tags or formats fail rather than guessing. Negative/above-one reconstructed RGB is counted, then clamped ONLY for nominal PQ EOTF measurement. Thus metadata numbers are nominal-domain decoded estimates, not calibrated luminance. Above-domain values are not disguised as valid. Bounded frames/pixels/time; truncation fails rather than claiming whole-clip completion. First-frame metadata is compared, changes across the clip are not checked.

## Measured in this environment

Rust1.99.0 debug, ffmpeg4.4.2. 38 focused tests passed (7 new). Deterministic128x64,2-frame fixture, HEVC CRF0:

| Evidence | Maximum error |
| --- | ---: |
| PQ reference decode |9.094947e-13 nit|
| PQ reference encode |3.141931e-14 code|
| PQ code roundtrip including black |7.309559e-7 code|
| HLG reference decode |2.568199e-9 scene units|
| HLG reference encode |4.704381e-10 code|
| HLG code roundtrip |1.110223e-16 code|
| HLG OOTF neutral/luminance sweep |4.547474e-13 nit|
| FFV1 RGB16 decoded fixture |0.046342 nit|
| HEVC10 decoded fixture, all pixels |331.276617 nit|

FFV1 p50=0.000091728nit,p95=p99=0.046342nit, matching independent16bit quantization floor. HEVC p50=0.014580nit,p95=p99=144.451523nit. HEVC is not asserted equivalent to lossless; YUV420 chroma reconstruction at saturated-color edges contributes to larger errors. Neutral-ramp max error is0.013212nit FFV1/36.236358nit HEVC; interior-neutral error is0.013212/25.443533nit, so even away from chroma edges HEVC exceeds the ideal neutral quantization floor. This remains an explicit unqualified delivery limitation. Ideal876-step neutral quantization floor max14.717702nit is measured separately, not used to excuse codec/matrix/chroma error.

Fixture declares deliberately inconsistent1000nit MaxCLL/400nit MaxFALL. Decoded HEVC reports10000/1815.281229nit and flags both; FFV1 has no declared static metadata. Mastering SEI survives with expected chromaticities/min/max units. HEVC reconstructed RGB has3458 negative and6144 above1 channel values, plus2048 nominal YUV code excursions. These are visible evidence, not certification or an unproven diagnosis of an exporter defect. Deliberate negative and over10000nit source channels are counted as reference clipping probes.

Visual inspection of encoded PNG confirms intended eight-block top ramp and eight-block bottom checkers. It is an encoded-value view, not HDR appearance proof.

## Sources inspected

- ITU-R BT.2100-3, Tables4/5 and Note5f: https://www.itu.int/dms_pubrec/itu-r/rec/bt/R-REC-BT.2100-3-202502-I%21%21PDF-E.pdf
- CTA-861.3-A Annex A (primary standard, hosted copy): https://www.avsforum.com/attachments/hdr-static-metadata-extensions-pdf.3191945/
- Independent implementation documentation confirming maxRGB-based content-light levels and unknown-zero semantics: https://psychtoolbox.org/docs/ComputeHDRStaticMetadataType1ContentLightLevels

Not proven: real calibrated display output, black lift/surround compensation, extended-range HLG gamma, all HDR delivery formats, static metadata consistency on every frame, dynamic HDR metadata, whole production-clip quality or full pipeline suite.
