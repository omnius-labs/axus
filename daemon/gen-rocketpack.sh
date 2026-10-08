#!/usr/bin/env bash
set -euo pipefail

daemon_dir="$(cd -- "$(dirname -- "$0")" && pwd)"
agent_temp_dir="${AGENT_TEMP_DIR:-/tmp}"
if [[ -z "${AGENT_TEMP_DIR:-}" ]]; then
  echo "AGENT_TEMP_DIR is unset; using /tmp" >&2
fi
mkdir -p "$agent_temp_dir"
agent_temp_dir="$(cd -- "$agent_temp_dir" && pwd)"

cd -- "$daemon_dir/refs/core-rs"
RUSTC_WRAPPER= CARGO_TARGET_DIR="$agent_temp_dir/axus-rocketpack-compiler-target" \
  cargo run --locked -p omnius-core-rocketpack-compiler -- compile "$daemon_dir"

cd -- "$daemon_dir"
rustfmt --edition 2024 modules/engine/src/generated.rs
while IFS= read -r -d '' source; do
  rustfmt --edition 2024 "$source"
done < <(rg --files --hidden modules/engine/src/generated -g '*.rs' -0)
