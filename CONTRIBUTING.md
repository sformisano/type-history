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
cargo run -p cargo-type-history --locked -- check --package invoice-history
```

Set `RUSTDOCFLAGS=-Dwarnings` when checking documentation, as CI does.
GitHub Actions runs these checks in both feature modes. Workspace tests cover the
[reference application](examples/invoice/README.md) and
[tutorial examples](crates/cargo-type-history/tests/standalone/tutorial.rs),
including lifecycle commands and expected compiler failures.
Normal builds use `target`; Clippy uses `target/clippy`.

## The book

The README is the short introduction and receipt walkthrough. The mdBook source
in `book/src/` holds the guides and reference. Rustdoc provides the API reference.
Add or reorder chapters in `book/src/SUMMARY.md`.

Install the same mdBook version used by the Book workflow:

```sh
cargo install mdbook --version '=0.5.4' --locked
mdbook build book
python3 scripts/check-book-links.py
mdbook serve book --open
```

Run these commands from the repository root. The link checker requires Python
3.11 or later. It checks generated local pages, assets, and heading fragments
without contacting external websites. `book/html/` is generated and ignored by Git.

The quick-start chapter includes the README's `journey` anchor. Edit that passage
in the README so both presentations stay in sync. Marked Rust snippets in the
README and book are compiled by the existing external-consumer tests:

```sh
cargo test -p cargo-type-history --test standalone --all-features --locked tutorial:: -- --test-threads=1
```

Those tests also exercise freezing and expected compiler failures. The redaction
tests compile the book's combined Redactable example. Keep snippet markers with
their code when moving content. The receipt tutorial compares the marked JSON
examples with generated ledger entries and serialized values, and checks the
documented compiler error. `mdbook test` alone cannot initialize these
consumer packages or reproduce their history across successive builds.

## Book deployment

The Book workflow builds and validates HTML, then uploads a `github-pages`
artifact. It has no deployment job or Pages write permissions. The repository's
Pages site has not been configured; package homepages and README links currently
lead to the book source. All crate manifests retain `publish = false`.

Before the first deployment, configure Pages for GitHub Actions and confirm its
reported URL. Then add the deployment job and use that confirmed URL for the
book links and package homepage. Keep each package's `documentation` URL pointed
at docs.rs for API reference. Update `output.html.site-url` if the configured
site uses a path other than `/type-history/`.
