#!/usr/bin/env bash
# Run the same checks CI does. Pass before you claim a change is done (AGENTS.md §6).
set -euo pipefail
cd "$(dirname "$0")/.."

echo "== cargo fmt --check =="
cargo fmt --all --check

echo "== cargo clippy (warnings as errors) =="
cargo clippy --workspace --all-targets --locked -- -D warnings

echo "== cargo test =="
cargo test --workspace --locked

echo "All checks passed."
