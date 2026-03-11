#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

cargo build --release --target wasm32-wasip2

WASM_PATH="target/wasm32-wasip2/release/tidepool_tool.wasm"
OUT_DIR="$ROOT/dist"

mkdir -p "$OUT_DIR"
cp "$WASM_PATH" "$OUT_DIR/tidepool.wasm"
cp "$ROOT/tidepool-tool.capabilities.json" "$OUT_DIR/tidepool.capabilities.json"

echo "Built Tidepool BetterClaw tool:"
echo "  $OUT_DIR/tidepool.wasm"
echo "  $OUT_DIR/tidepool.capabilities.json"
