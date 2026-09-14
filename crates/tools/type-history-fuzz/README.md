# Codec fuzzing

Fuzzing feeds generated inputs into the public decoders to find cases the fixed
tests may miss. These targets use the generated invoice history. For an accepted
invoice, they also check the converted value against an independently written
expected result:

| Target | Input and checks |
| --- | --- |
| `invoice_json` | Arbitrary JSON; accepted records retain their version and values through both codecs and produce the independently expected conversion result. |
| `invoice_msgpack` | Arbitrary MessagePack with the same checks, including binary lengths and nesting. |
| `invoice_values` | Generated counts, strings, and revisions; all three invoice versions must decode correctly, while unsupported versions must fail. |
| `schema_json` | Arbitrary public schema JSON; accepted shapes survive serialization and decoding, and normalizing twice gives the same result as normalizing once. |

Ordinary decoding errors are expected. Panics, sanitizer findings, failed
assertions, timeouts, and memory-limit failures stop the run and require
inspection. The reference invoice intentionally rejects conversion of count
13. The test's independent conversion check, called its *oracle*, expects that
error and checks the reported versions.

Install the development tools without changing the project's default compiler:

```sh
rustup toolchain install nightly --profile minimal --component rust-src
cargo install cargo-fuzz --version 0.13.2 --locked
```

From the repository root, first run the deterministic oracle tests:

```sh
cargo test --manifest-path crates/tools/type-history-fuzz/Cargo.toml --lib --locked
```

The following loop runs each target for up to two minutes, with limits on input
size, memory, and time spent on a single input. The first directory collects
inputs worth trying again, called the *corpus*. The second provides the checked-in
starting inputs, called *seeds*, which remain unchanged:

```sh
for target in invoice_json invoice_msgpack invoice_values schema_json; do
  mkdir -p "crates/tools/type-history-fuzz/corpus/$target"
  cargo +nightly fuzz run "$target" --fuzz-dir crates/tools/type-history-fuzz --target-dir target \
    "crates/tools/type-history-fuzz/corpus/$target" "crates/tools/type-history-fuzz/seeds/$target" -- \
    -max_total_time=120 -max_len=65536 -timeout=5 -rss_limit_mb=1024 \
    -seed=1 -print_final_stats=1 || exit "$?"
done
```

Retain any failure input and add a deterministic regression when correcting its
cause. Replay one saved input by passing its file path in place of the corpus
directories. Do not replace fixed historical fixtures when a serializer changes.

The input, time, and memory caps bound this campaign; they are not application
support limits. A completed finite run is evidence about its explored inputs,
not proof that every possible input is safe. Record the compiler version,
locked dependencies, target, flags, seed, executions, and sanitizer outcome with
each result. Fuzzing instrumentation uses nightly and is separate from the
library's supported stable compiler.
