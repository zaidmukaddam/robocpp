# RoboC++ Fuzzing

This is a separate `cargo-fuzz` workspace so normal workspace checks do not
require nightly Rust or libFuzzer.

Install and run:

```sh
cargo install cargo-fuzz
cd fuzz
cargo fuzz run textual_parser
cargo fuzz run plcopen_xml
cargo fuzz run pipeline
cargo fuzz run plcopen_roundtrip
cargo fuzz run plcopen_dom_lowering
```

Minimized crashing inputs should be copied into
`../validation/corpus/regressions/` with a stable expected diagnostic or fixed
as accepted fixtures, depending on the bug.

Seed corpora live in `fuzz/corpus/*`. Keep them synchronized with representative
files from `examples/` and `validation/corpus/`.

## Scheduled Fuzzing

`.github/workflows/scheduled-validation.yml` runs each fuzz target weekly and on
manual dispatch with:

```sh
cargo +nightly fuzz run <target> -- -max_total_time=1800
```

When a scheduled run finds a crash, minimize it with `cargo fuzz tmin`, fix the
bug, and commit the minimized input under `validation/corpus/regressions/` so the
normal corpus gate prevents regressions.
