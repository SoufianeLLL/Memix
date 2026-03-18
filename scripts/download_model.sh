#!/bin/bash
set -euo pipefail

MODEL_DIR="$(dirname "$0")/../daemon/models/all-MiniLM-L6-v2"
BASE="https://huggingface.co/sentence-transformers/all-MiniLM-L6-v2/resolve/main"

mkdir -p "$MODEL_DIR/onnx"

curl_download() {
  local source_url="$1"
  local dest_path="$2"
  curl --fail --show-error --location \
    --retry 5 \
    --retry-delay 2 \
    --retry-all-errors \
    --connect-timeout 15 \
    --max-time 300 \
    "$source_url" \
    -o "$dest_path"
}

curl_download "$BASE/tokenizer.json" "$MODEL_DIR/tokenizer.json"
curl_download "$BASE/config.json" "$MODEL_DIR/config.json"
curl_download "$BASE/tokenizer_config.json" "$MODEL_DIR/tokenizer_config.json"
curl_download "$BASE/special_tokens_map.json" "$MODEL_DIR/special_tokens_map.json"
curl_download "$BASE/onnx/model.onnx" "$MODEL_DIR/onnx/model.onnx"

ls -lh "$MODEL_DIR"
ls -lh "$MODEL_DIR/onnx"
