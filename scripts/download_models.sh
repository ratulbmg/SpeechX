#!/bin/bash
# Downloads the ASR models that get bundled into the app
# (tauri.conf.json's bundle.resources). Models are gitignored — 240MB+ of
# binaries, not something to commit — so any clean checkout (CI, or a
# fresh dev machine) needs to fetch them before `tauri build` can find
# what tauri.conf.json expects at ../models/<name>/ relative to
# src-tauri/, i.e. models/ at the repo root.
#
# Source: sherpa-onnx's own GitHub releases (k2-fsa/sherpa-onnx, the
# "asr-models" release tag). Only pulls the specific files each engine
# actually loads (asr/manager.rs) — each archive also ships a
# full-precision (non-int8) variant and test_wavs/ that we don't use and
# don't want bloating the bundle.
set -euo pipefail

cd "$(dirname "$0")/.."

BASE_URL="https://github.com/k2-fsa/sherpa-onnx/releases/download/asr-models"

mkdir -p models/sherpa-onnx-whisper-base
mkdir -p models/sherpa-onnx-streaming-zipformer-bn-vosk-2026-02-09

echo "Downloading Whisper base (int8)..."
curl -sL "$BASE_URL/sherpa-onnx-whisper-base.tar.bz2" | tar -xj \
  -C models/sherpa-onnx-whisper-base --strip-components=1 \
  sherpa-onnx-whisper-base/base-encoder.int8.onnx \
  sherpa-onnx-whisper-base/base-decoder.int8.onnx \
  sherpa-onnx-whisper-base/base-tokens.txt

echo "Downloading Bengali streaming Zipformer..."
curl -sL "$BASE_URL/sherpa-onnx-streaming-zipformer-bn-vosk-2026-02-09.tar.bz2" | tar -xj \
  -C models/sherpa-onnx-streaming-zipformer-bn-vosk-2026-02-09 --strip-components=1 \
  sherpa-onnx-streaming-zipformer-bn-vosk-2026-02-09/encoder.onnx \
  sherpa-onnx-streaming-zipformer-bn-vosk-2026-02-09/decoder.onnx \
  sherpa-onnx-streaming-zipformer-bn-vosk-2026-02-09/joiner.onnx \
  sherpa-onnx-streaming-zipformer-bn-vosk-2026-02-09/tokens.txt

echo "Models ready:"
du -sh models/*
