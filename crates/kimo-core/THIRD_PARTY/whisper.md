# Speech recognition for auto_captions

auto_captions runs a speech recognizer as a separate process. Neither the program nor the model is part of this
repository or linked into kimo-core; `tools/whisper/fetch.sh` downloads them into `~/.cache/kimo_models` (or `$KIMO_MODELS`).

- **whisper.cpp** (`whisper-cli`), https://github.com/ggml-org/whisper.cpp, tag v1.9.5, commit
  d1be6fde11ac6e0407606b4e42fe72d34add8037. MIT License, Copyright (c) 2023-2026 The ggml authors. The licence text is
  copied next to the binary as `whisper.cpp-LICENSE`.
- **Whisper model weights** (OpenAI Whisper, converted to ggml by the whisper.cpp project and published at
  https://huggingface.co/ggerganov/whisper.cpp). MIT License, Copyright (c) 2022 OpenAI. Files and sha256:
  - ggml-small-q5_1.bin ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb (default)
  - ggml-large-v3-turbo-q5_0.bin 394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2
  - ggml-base-q5_1.bin 422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898

Word-timing refinement (spectral-flux onsets, DTW bias correction), paging, keyword choice, Devanagari romanization and
the skin-tone band used to keep captions off a face are original code in `src/speech_words.rs`.
