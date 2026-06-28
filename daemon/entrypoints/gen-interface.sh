#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"

rm -rf ./interface

npx @openapitools/openapi-generator-cli@v2.38.0 generate \
  -i ../openapi.yaml \
  -g rust-axum \
  -o ./interface \
  --additional-properties=packageName=omnius-axus-interface,packageVersion=0.1.0
