# Type History book

The book continues the README's shop example: add a currency, keep old receipts
readable, and freeze each version before storing it. Later chapters cover
integration with other types and the full field-history rules.

Start with the [introduction](src/introduction.md), browse the
[chapters](src/SUMMARY.md), or build a local HTML copy:

```sh
cargo install mdbook --version '=0.5.4' --locked
cargo install lychee --version '=0.24.2' --locked
mdbook build book
lychee --config book/lychee.toml book/html
mdbook serve book --open
```

Run these commands from the repository root. The generated site is in `book/html/`.
[Lychee](https://github.com/lycheeverse/lychee) checks links to local pages, images,
and sections in the generated site. External links are skipped, so the check
works offline.
