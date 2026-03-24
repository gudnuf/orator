#!/usr/bin/env bash
set -euo pipefail

MODEL_DIR="$(cd "$(dirname "$0")/.." && pwd)/models"
MODEL_NAME="sherpa-onnx-streaming-zipformer-en-kroko-2025-08-06"
MODEL_URL="https://huggingface.co/csukuangfj/$MODEL_NAME"

if [ -d "$MODEL_DIR/$MODEL_NAME" ] && [ -f "$MODEL_DIR/$MODEL_NAME/encoder.onnx" ]; then
    echo "Model already downloaded at $MODEL_DIR/$MODEL_NAME"
    exit 0
fi

mkdir -p "$MODEL_DIR"

if command -v git-lfs &>/dev/null || git lfs version &>/dev/null 2>&1; then
    echo "Cloning $MODEL_NAME via git (with LFS)..."
    GIT_LFS_SKIP_SMUDGE=0 git clone "$MODEL_URL" "$MODEL_DIR/$MODEL_NAME"
else
    echo "Error: git-lfs is required to download the model."
    echo "Install it with: brew install git-lfs && git lfs install"
    echo "Or with nix:     nix-shell -p git-lfs --run 'git lfs install'"
    exit 1
fi

echo "Model ready at $MODEL_DIR/$MODEL_NAME"
