#!/bin/bash
# Fetches the local speech recognizer used by auto_captions: builds whisper.cpp's whisper-cli (MIT) at a pinned commit and
# downloads one ggml Whisper model (OpenAI Whisper weights, MIT), checking its sha256. Nothing is committed to the repo.
#   bash tools/whisper/fetch.sh            # multilingual small, q5_1, 181 MiB (default)
#   MODEL=turbo bash tools/whisper/fetch.sh  # large-v3-turbo, q5_0, 547 MiB: much better on Hindi / Hinglish
#   MODEL=base  bash tools/whisper/fetch.sh  # base, q5_1, 57 MiB: fastest, weakest
# Destination: $KIMO_MODELS (default ~/.cache/kimo_models): bin/whisper-cli and ggml-<model>.bin. Needs git, cmake, a C++ compiler.
set -euo pipefail
DEST="${KIMO_MODELS:-$HOME/.cache/kimo_models}"; MODEL="${MODEL:-small}"
WHISPER_TAG=v1.9.5; WHISPER_COMMIT=d1be6fde11ac6e0407606b4e42fe72d34add8037
case "$MODEL" in
  small) FILE=ggml-small-q5_1.bin;          SHA=ae85e4a935d7a567bd102fe55afc16bb595bdb618e11b2fc7591bc08120411bb ;;
  turbo) FILE=ggml-large-v3-turbo-q5_0.bin; SHA=394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2 ;;
  base)  FILE=ggml-base-q5_1.bin;           SHA=422f1ae452ade6f30a004d7e5c6a43195e4433bc370bf23fac9cc591f01a8898 ;;
  *) echo "MODEL must be small, turbo or base" >&2; exit 2 ;;
esac
mkdir -p "$DEST/bin"
sha() { if command -v sha256sum >/dev/null; then sha256sum "$1" | cut -d' ' -f1; else shasum -a 256 "$1" | cut -d' ' -f1; fi; }
if [ ! -x "$DEST/bin/whisper-cli" ]; then
  SRC="$(mktemp -d)"; trap 'rm -rf "$SRC"' EXIT
  git clone -q --depth 1 --branch "$WHISPER_TAG" https://github.com/ggml-org/whisper.cpp "$SRC/whisper.cpp"
  [ "$(git -C "$SRC/whisper.cpp" rev-parse HEAD)" = "$WHISPER_COMMIT" ] || { echo "whisper.cpp $WHISPER_TAG is not the pinned commit" >&2; exit 1; }
  cmake -S "$SRC/whisper.cpp" -B "$SRC/build" -DCMAKE_BUILD_TYPE=Release -DBUILD_SHARED_LIBS=OFF -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_SERVER=OFF -DWHISPER_SDL2=OFF -DWHISPER_CURL=OFF >/dev/null
  cmake --build "$SRC/build" -j "${JOBS:-2}" --target whisper-cli >/dev/null
  cp "$SRC/build/bin/whisper-cli" "$DEST/bin/whisper-cli"; cp "$SRC/whisper.cpp/LICENSE" "$DEST/bin/whisper.cpp-LICENSE"; echo "$WHISPER_COMMIT" > "$DEST/bin/whisper.cpp-COMMIT"
fi
if [ ! -f "$DEST/$FILE" ] || [ "$(sha "$DEST/$FILE")" != "$SHA" ]; then
  curl -fL --retry 3 -o "$DEST/$FILE.part" "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/$FILE"
  [ "$(sha "$DEST/$FILE.part")" = "$SHA" ] || { rm -f "$DEST/$FILE.part"; echo "checksum mismatch for $FILE" >&2; exit 1; }
  mv "$DEST/$FILE.part" "$DEST/$FILE"
fi
echo "ready: $DEST/bin/whisper-cli + $DEST/$FILE"
