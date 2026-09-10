# Contributing

Use the repository's pinned Rust toolchain. Run these commands from the repository root.
Set `TMPDIR` to a short disk path with room for temporary consumer builds.

```sh
cargo fetch --locked
cargo fmt --all --check
cargo clippy --workspace --all-targets --no-default-features --locked --target-dir target/clippy -- -Dwarnings
cargo test --workspace --all-targets --no-default-features --locked
cargo test --workspace --doc --no-default-features --locked
cargo doc --workspace --no-deps --no-default-features --locked
cargo build --workspace --release --no-default-features --locked

cargo clippy --workspace --all-targets --all-features --locked --target-dir target/clippy -- -Dwarnings
cargo test --workspace --all-targets --all-features --locked
cargo test --workspace --doc --all-features --locked
cargo doc --workspace --no-deps --all-features --locked
cargo build --workspace --release --all-features --locked

cargo run -p invoice-history --release --locked
cargo run -p cargo-typehistory --locked -- check --package invoice-history
```

Set `RUSTDOCFLAGS=-Dwarnings` when checking documentation, as CI does.
GitHub Actions runs these checks in both feature modes. Workspace tests cover the
[reference application](examples/invoice/README.md) and
[tutorial examples](crates/cargo-typehistory/tests/standalone/tutorial.rs),
including lifecycle commands and expected compiler failures.
Normal builds use `target`; Clippy uses `target/clippy`.
