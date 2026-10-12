#!/usr/bin/env bash
# Builds the SBF program and runs the whole black-box suite against it.
#   ./run.sh                      # everything
#   ./run.sh --test 06_pipeline   # one file
#   OBLIO_FUZZ_SEEDS=200 OBLIO_FUZZ_STEPS=400 ./run.sh --test 07_invariants   # long soak
set -euo pipefail
HERE="$(cd "$(dirname "$0")" && pwd)"
cd "$HERE/.."
cargo build-sbf --manifest-path Cargo.toml
TARGET_DIR="$(cargo metadata --no-deps --format-version 1 | python3 -c 'import sys,json;print(json.load(sys.stdin)["target_directory"])')"
SO="$(find "$TARGET_DIR/deploy" -name oblio_contract.so | head -1)"
[ -f "$SO" ] || { echo "oblio_contract.so not found under $TARGET_DIR/deploy"; exit 1; }
export OBLIO_SO="$SO"
cd "$HERE"
cargo test "$@" -- --nocapture
