#!/bin/sh
set -eu

cd "$(dirname "$0")/.."
cargo test
cargo clippy --all-targets -- -D warnings
cargo test --features samples
cargo clippy --all-targets --features samples -- -D warnings
