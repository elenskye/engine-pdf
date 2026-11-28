#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
# 测试不碰所有 App 共用的花钱账本（engine-core spend）
ENGINE_LEDGER_DIR=$(mktemp -d "${TMPDIR:-/tmp}/engine-ledger-check.XXXXXX")
export ENGINE_LEDGER_DIR
trap 'rm -rf "$ENGINE_LEDGER_DIR"' EXIT
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
# web/node_modules 能清（scripts/clean.sh 发版那一档），没有就照锁文件装回来
[ -d web/node_modules ] || pnpm --dir web install --frozen-lockfile
pnpm --dir web run check
pnpm --dir web test
