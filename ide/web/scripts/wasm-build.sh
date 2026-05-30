#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

if ! command -v wasm-pack >/dev/null 2>&1; then
  echo "wasm-pack is not installed."
  echo "Install it with: brew install wasm-pack"
  exit 1
fi

if command -v rustup >/dev/null 2>&1; then
  rustup target add wasm32-unknown-unknown >/dev/null 2>&1 || true
  export PATH="$(dirname "$(rustup which rustc)"):${PATH}"
fi

wasm-pack build "$ROOT/../../crates/iec_language_service_wasm" \
  --target web \
  --out-dir "$ROOT/src/wasm/iec_language_service_wasm"
