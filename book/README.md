# Type History book

The book continues the README's shop example: add a currency, keep old receipts
readable, and freeze each version before storing it. Later chapters cover
integration with other types and the full field-history rules.

The book is not published as a hosted site. `.github/workflows/book.yml` builds the
HTML and link-checks it for pull requests and pushes to `main`. Its upload step
deliberately does not enable or deploy GitHub Pages, so no rendered URL exists
for other repositories to link. A link into `book/src/` reaches raw mdBook source
on GitHub. The [repository README](../README.md) describes this source version.

Start with the [introduction](src/introduction.md), browse the
[chapters](src/SUMMARY.md), or build a local HTML copy from the repository root:

```sh
cargo install mdbook --version '=0.5.4' --locked
cargo install lychee --version '=0.24.2' --locked
mdbook build book
lychee --config book/lychee.toml book/html
mdbook serve book --open
```

The generated site is in `book/html/`.
[Lychee](https://github.com/lycheeverse/lychee) checks links to local pages, images,
and sections in the generated site. External links are skipped, so the check
works offline.
