#!/usr/bin/env bash
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$script_dir"

rm -rf ./interface

npx --yes @openapitools/openapi-generator-cli@v2.38.0 generate \
  -i ../openapi.yaml \
  -g rust-axum \
  -o ./interface \
  --additional-properties=packageName=omnius-axus-interface,packageVersion=0.1.0

# Drop the build timestamp so that regenerating from the same openapi.yaml yields no diff.
sed -i.bak '/^- Build date: /d' ./interface/README.md
rm ./interface/README.md.bak

cargo fmt --all
