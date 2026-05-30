#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
BRIDGE_PID=""

cleanup() {
  if [[ -n "${BRIDGE_PID}" ]]; then
    kill "${BRIDGE_PID}" 2>/dev/null || true
  fi
}

trap cleanup EXIT INT TERM

cargo run -p rbcpp_target_bridge --manifest-path "${ROOT}/Cargo.toml" &
BRIDGE_PID=$!

cd "$(dirname "$0")/.."
exec bun run dev
