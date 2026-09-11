# Type History book

The book covers setup, the generated types, field changes, conversions, freezing,
and integrations. Browse the [chapters](src/SUMMARY.md), or build the HTML book:

```sh
cargo install mdbook --version '=0.5.4' --locked
mdbook build book
mdbook serve book --open
```

Run these commands from the repository root. The generated site is in `book/html/`.
See [Contributing](../CONTRIBUTING.md#the-book) for link checks and example tests.
