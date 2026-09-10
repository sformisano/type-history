# Typehistory

Typehistory keeps older JSON records readable as Rust fields change. Declare a
field history to generate historical types, adjacent conversions, and the current
type. Normal Cargo builds protect frozen wire shapes.

This repository contains six standalone Rust crates. It needs no Axis crates or
database. The packages are pre-release and remain unpublished; use a local
checkout or an exact Git revision.

- [Get started and API reference](crates/typehistory/README.md)
- [Executable guide](crates/typehistory/docs/guide.md)
- [Supported evolution cases](crates/typehistory/docs/evolution-matrix.md)
- [Complete invoice example](crates/typehistory/examples/invoice/README.md)

## Crates

| Crate | Role |
| --- | --- |
| `typehistory` | Public facade for record histories and supporting schemas |
| `typehistory-core` | Runtime decoding and structural wire contracts |
| `typehistory-macros` | Procedural macros |
| `typehistory-codegen` | Shared parsing, schema authority, and Rust generation |
| `typehistory-build` | Build checks and checked lifecycle operations |
| `cargo-typehistory` | Cargo subcommand for initializing, checking, and freezing histories |

## Development

Install Rust with rustup, plus Bash, Git, and ripgrep. The repository pins Rust
1.97.1 with rustfmt and Clippy. Fetch dependencies before running the complete
local gate:

```sh
cargo fetch --locked
bash scripts/verify.sh
```

The gate runs formatting, Clippy, workspace tests, doctests, public documentation,
and the complete invoice guide. The guide creates and removes its own consumer
workspace. Set `TMPDIR` to a short path on a disk with room for fixture builds.
Build output uses `target`, with Clippy output in `target/clippy`.

GitHub Actions runs the same script on pushes to `main` and pull requests.

## Origin and license

Extracted from [Axis at af7b96c3](https://github.com/sformisano/axis/tree/af7b96c3f0744cdfd170c50bb321d5dfd02be8ea/crates/typehistory).
Licensed under either [MIT](LICENSE-MIT) or [Apache 2.0](LICENSE-APACHE).
