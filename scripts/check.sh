#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo test
cargo clippy --all-targets -- -D warnings
cargo test --features samples,fake
cargo clippy --all-targets --features samples,fake -- -D warnings
if [ "$(uname -s)" = "Darwin" ]; then
  : "${PDFIUM_STATIC_LIB_PATH:=$PWD/local/pdfium/pdfium-mac-arm64/lib}"
  export PDFIUM_STATIC_LIB_PATH
  cargo test --features samples,fake,pdfium
  cargo clippy --all-targets --features samples,fake,pdfium -- -D warnings
fi
pnpm --dir web run check
pnpm --dir web test
