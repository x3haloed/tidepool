#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
cd "$ROOT"

echo "Building Tidepool BetterClaw channel..."

cargo build --release --target wasm32-wasip2

WASM_PATH="target/wasm32-wasip2/release/tidepool_channel.wasm"
OUT_DIR="$ROOT/dist"

mkdir -p "$OUT_DIR"

if [ -f "$WASM_PATH" ]; then
    if command -v wasm-tools >/dev/null 2>&1; then
        wasm-tools component new "$WASM_PATH" -o "$OUT_DIR/tidepool-channel.wasm" 2>/dev/null || cp "$WASM_PATH" "$OUT_DIR/tidepool-channel.wasm"
        wasm-tools strip "$OUT_DIR/tidepool-channel.wasm" -o "$OUT_DIR/tidepool-channel.wasm"
    else
        cp "$WASM_PATH" "$OUT_DIR/tidepool-channel.wasm"
    fi
    cp "$ROOT/tidepool.capabilities.json" "$OUT_DIR/tidepool.capabilities.json"
else
    echo "Error: WASM output not found at $WASM_PATH"
    exit 1
fi

echo "Built Tidepool BetterClaw channel:"
echo "  $OUT_DIR/tidepool-channel.wasm"
echo "  $OUT_DIR/tidepool.capabilities.json"
