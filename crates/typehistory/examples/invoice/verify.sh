#!/usr/bin/env bash
set -euo pipefail
export RUSTUP_TOOLCHAIN=1.97.1

example_dir="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
repository="$(cd -- "$example_dir/../../../.." && pwd)"
: "${TMPDIR:?Set TMPDIR to an owned short temporary directory}"
: "${CARGO_TARGET_DIR:?Set CARGO_TARGET_DIR to the assigned checkout target}"
command -v cargo-typehistory >/dev/null
invoice_tmp="$(mktemp -d "$TMPDIR/invoice.XXXXXXXX")"
trap 'rm -rf -- "$invoice_tmp"' EXIT
consumer="$invoice_tmp/invoice-history"
mkdir -p "$consumer/src"

# Check that the complete README sources match the files used by this consumer.
for source in versions/v1.rs versions/v2.rs versions/v3.rs runtime-tests.rs nested-example.rs; do
    awk -v marker="<!-- example:$source -->" '
        $0 == marker { selected = 1; next }
        selected && /^```rust$/ { code = 1; next }
        code && /^```$/ { exit }
        code { print }
    ' "$example_dir/../../README.md" > "$invoice_tmp/readme.rs"
    cmp "$invoice_tmp/readme.rs" "$example_dir/$source"
done

# Restrict the path before interpolation so no shell or TOML escapes are needed.
case "$repository" in *[!a-zA-Z0-9_./-]*)
    printf '%s\n' 'The example requires a checkout path without spaces or special characters.' >&2
    exit 1
esac
cat > "$consumer/Cargo.toml" <<MANIFEST
[package]
name = "invoice-history"
version = "0.0.0"
edition = "2024"
publish = false
build = "build.rs"
[workspace]
[dependencies]
typehistory = { path = "$repository/crates/typehistory/typehistory" }
redactable = { version = "=0.12.0", features = ["json"] }
serde = { version = "1.0", features = ["derive"] }
serde_json = "1.0.149"
[build-dependencies]
typehistory-build = { path = "$repository/crates/typehistory/build" }
[profile.release_child]
inherits = "release"
debug-assertions = true
opt-level = 0
[profile.dev_child]
inherits = "dev"
debug-assertions = false
opt-level = 2
MANIFEST
printf 'format = 1\n' > "$consumer/typehistory.toml"
printf 'fn main() { typehistory_build::compile(); }\n' > "$consumer/build.rs"
touch "$consumer/src/lib.rs"
cd "$consumer"
cargo generate-lockfile --offline
cargo typehistory init --package invoice-history

install_source() {
    cp -- "$example_dir/versions/$1.rs" src/lib.rs.next
    mv -- src/lib.rs.next src/lib.rs
}

expect_failure() {
    local diagnostic="$1"
    shift
    if "$@" > "$invoice_tmp/failure.log" 2>&1; then
        cat "$invoice_tmp/failure.log"
        printf 'Expected failure: %s\n' "$*" >&2
        exit 1
    fi
    if ! rg --quiet --fixed-strings "$diagnostic" "$invoice_tmp/failure.log"; then
        cat "$invoice_tmp/failure.log"
        printf 'Missing diagnostic: %s\n' "$diagnostic" >&2
        exit 1
    fi
    printf 'Rejected as expected: %s\n' "$*"
}

install_source v1
cargo build --locked --offline
cargo typehistory freeze --package invoice-history
cp typehistory/schemas.json "$invoice_tmp/frozen-v1.json"

# Ordinary Cargo checks each malformed declaration against the frozen V1 ledger.
for mutation in missing_backfill old_key missing_cast conflicting_cast wrong_input wrong_output wrong_payload zero huge; do
    case "$mutation" in
        missing_backfill) edit='s/, backfill_value = 7_u32//'; diagnostic='an added field requires' ;;
        old_key) edit='s/previous_type = u32/from = u32/'; diagnostic='unknown history key' ;;
        missing_cast) edit='s/, cast_from_previous_value = widen//'; diagnostic='an updated field requires' ;;
        conflicting_cast) edit='s/backfill_value = 7_u32/backfill_value = 7_u32, cast_from_previous_payload = label/'; diagnostic='exactly one assignment rule' ;;
        wrong_input) edit='s/fn widen(value: u32)/fn widen(value: u16)/'; diagnostic='mismatched types' ;;
        wrong_output) edit='s/Result<u64, ConvertError>/Result<String, ConvertError>/; s/Ok(u64::from(value))/Ok(value.to_string())/'; diagnostic='mismatched types' ;;
        wrong_payload) edit='s/previous: \&InvoiceV1/previous: \&InvoiceV2/'; diagnostic='mismatched types' ;;
        zero) edit='s/updated_at = v2/updated_at = v0/'; diagnostic='history versions are positive' ;;
        huge) edit='s/updated_at = v2/updated_at = v4294967295/'; diagnostic='exceeds the one permitted successor V2' ;;
    esac
    sed "$edit" "$example_dir/versions/v2.rs" > src/lib.rs.next
    mv -- src/lib.rs.next src/lib.rs
    expect_failure "$diagnostic" cargo build --locked --offline
    cmp typehistory/schemas.json "$invoice_tmp/frozen-v1.json"
done

install_source v2
cargo build --locked --offline
expect_failure 'draft' env TYPEHISTORY_REQUIRE_FROZEN=1 cargo build --locked --offline
expect_failure 'draft' cargo build --release --locked --offline
expect_failure 'draft' cargo build --profile release_child --locked --offline
cargo build --profile dev_child --locked --offline
expect_failure 'must be unset or exactly 1' env TYPEHISTORY_REQUIRE_FROZEN=0 cargo build --locked --offline
cargo typehistory freeze --package invoice-history --type billing.invoice.issued --version 2
cp typehistory/schemas.json "$invoice_tmp/frozen-v2.json"

# Discard restores source only, including the declared predecessor type.
install_source v3
cargo build --locked --offline
install_source v2
cargo build --release --locked --offline
cmp typehistory/schemas.json "$invoice_tmp/frozen-v2.json"

install_source v3
cat "$example_dir/runtime-tests.rs" >> src/lib.rs
cargo test --lib --locked --offline
cargo typehistory freeze --package invoice-history --type billing.invoice.issued --version 3
cargo build --release --locked --offline
cp typehistory/schemas.json "$invoice_tmp/frozen-v3.json"

# Reset keeps the original shape until exact refreeze or matching-source undo.
cargo typehistory reset --package invoice-history --type billing.invoice.issued --version 3
expect_failure 'reset draft' cargo build --release --locked --offline
expect_failure 'reset reservations' cargo typehistory freeze --package invoice-history
cargo typehistory reset --undo --package invoice-history --type billing.invoice.issued --version 3
cmp typehistory/schemas.json "$invoice_tmp/frozen-v3.json"
cargo typehistory reset --package invoice-history --type billing.invoice.issued --version 3
cargo typehistory freeze --package invoice-history --type billing.invoice.issued --version 3
cargo typehistory check --package invoice-history

# A second history checks nested schema derivation with a matching closed codec.
cat "$example_dir/nested-example.rs" >> src/lib.rs
cargo test --lib --locked --offline
cargo typehistory freeze --package invoice-history

# Import a complete current-format ledger into a second owned empty package.
cp typehistory/schemas.json "$invoice_tmp/import.json"
import_consumer="$invoice_tmp/imported"
mkdir -p "$import_consumer/src"
cp Cargo.toml "$import_consumer/Cargo.toml"
cp Cargo.lock "$import_consumer/Cargo.lock"
cp typehistory.toml build.rs "$import_consumer/"
cp src/lib.rs "$import_consumer/src/lib.rs"
cd "$import_consumer"
cargo typehistory import --package invoice-history --from "$invoice_tmp/import.json"
cargo build --release --locked --offline
cargo typehistory check --package invoice-history

printf '%s\n' 'Invoice guide completed: V1/V2/V3, runtime, compiler failures, profiles, discard, reset, undo, freeze, and import.'
