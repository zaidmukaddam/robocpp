# RoboC++ Validation Corpus

This directory contains production-readiness validation inputs. These fixtures
are separate from examples: examples show users how to write programs, while the
validation corpus records the inputs that release and compiler-readiness checks
must keep stable.

Every source fixture must include a `validation:` metadata comment in the first
20 lines with at least:

- `feature=...`
- `clause=...`

Regression fixtures should also include `origin=...`; use `origin=fuzz` only
for minimized inputs that were actually discovered by a fuzz target. Seeded
limit or adversarial fixtures should use a more precise origin such as
`origin=seeded-limit`.

Runtime fixtures can also include `cycles=...` and `program=...`.

Run all corpus checks with:

```sh
cargo run -p xtask -- validate-corpus
```

Run bounded parser/XML fuzz smoke checks with:

```sh
cargo run -p xtask -- fuzz-smoke
```

Run generated cross-backend and robustness checks with:

```sh
cargo run -p xtask -- validate-differential
cargo run -p xtask -- validate-robustness
cargo run -p xtask -- validate-sanitizers
```

Generate a release evidence report with:

```sh
cargo run -p xtask -- release-report --output validation/releases/<release>.md
```

Robustness validation targets are tracked in `validation/LIMITS.md`.
Versioned readiness status is tracked in `validation/STATUS.toml`.
Diagnostic compatibility policy is tracked in `validation/DIAGNOSTICS.md`.

## Corpus Layout

- `corpus/accepted`: valid programs that must parse and type-check.
- `corpus/rejected`: invalid programs with `.diag` expected diagnostic
  substrings.
- `corpus/runtime`: programs with `.trace` interpreter expectations.
- `corpus/c-parity`: programs that must type-check, generate C, and compile as
  C11.
- `corpus/plcopen/roundtrip`: PLCopen XML files that must import, export, and
  re-import without changing normalized project shape.
- `corpus/plcopen/vendor`: real vendor exports when licensing permits.
- `corpus/plcopen/hostile`: malformed or policy-rejected PLCopen XML with
  `.diag` expected diagnostic substrings.
- `corpus/stress`: large, nested, or boundary inputs.
- `corpus/regressions`: minimized inputs for bugs found by fuzzing or users,
  plus clearly labeled seeded regression fixtures for known robustness limits.
