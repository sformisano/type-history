# Type History book

The book continues the README's shop example: add a currency, keep old receipts
readable, and freeze each version before storing it. Later chapters cover
integration with other types and the full field-history rules.

The book is not published as a hosted site. `.github/workflows/book.yml` builds the
HTML and link-checks it on every push, and its upload step deliberately does not
enable or deploy GitHub Pages, so there is no rendered URL to link from another
repository. A link into `book/src/` reaches raw mdBook source on GitHub. The
address that answers "which artifact is this?" is
[the README at the pinned revision](https://github.com/sformisano/type-history/blob/fdfda383685bad2253f4d7dc1dfbc1d5de2fd8f3/README.md).

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
