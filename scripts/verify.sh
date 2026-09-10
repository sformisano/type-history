#!/usr/bin/env bash
set -euo pipefail

repository="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd -- "$repository"

export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$repository/target}"
if [[ "$CARGO_TARGET_DIR" != /* ]]; then
    export CARGO_TARGET_DIR="$repository/$CARGO_TARGET_DIR"
fi
typehistory_tmp="$(mktemp -d)"
trap 'rm -rf -- "$typehistory_tmp"' EXIT
export TMPDIR="$typehistory_tmp"

cargo fmt --all --check
for mode in plain redactable; do
    features=(--no-default-features)
    if [[ "$mode" == redactable ]]; then
        features+=(--all-features)
    fi
    CARGO_TARGET_DIR="$CARGO_TARGET_DIR/clippy" \
        cargo clippy --workspace --all-targets "${features[@]}" --locked -- -Dwarnings
    cargo test --workspace --all-targets "${features[@]}" --locked
    cargo test --workspace --doc "${features[@]}" --locked
    RUSTDOCFLAGS="${RUSTDOCFLAGS:-} -Dwarnings" \
        cargo doc --workspace --no-deps "${features[@]}" --locked
    cargo build --workspace --release "${features[@]}" --locked
done
cargo build -p cargo-typehistory --locked
export PATH="$CARGO_TARGET_DIR/debug:$PATH"
bash examples/invoice/verify.sh
