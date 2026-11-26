#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo test
cargo clippy --all-targets -- -D warnings
cargo test --features samples,fake
cargo clippy --all-targets --features samples,fake -- -D warnings
pnpm --dir web run check
pnpm --dir web test
