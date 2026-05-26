# RoboC++

RoboC++ is a clean Rust implementation scaffold for an IEC 61131-3 toolchain.
The initial target is IEC 61131-3:2003, developed against local reference material, with profile
gates for future 2013 and 2025 support.

The first usable product is a compiler-oriented CLI:

```sh
rbcpp check examples/counter.st
rbcpp run examples/counter.st --cycles 3
rbcpp build-c examples/counter.st -o build/counter.c
rbcpp import-plcopen project.xml
rbcpp export-plcopen examples/counter.st -o project.xml
rbcpp compliance
```

The implementation is intentionally dependency-light at this stage: all crates use only the Rust
standard library so the core architecture can be reviewed and evolved without external parser or
XML dependencies.

## Workspace Layout

- `iec_diagnostics`: spans, diagnostics, and human/JSON rendering.
- `iec_profile`: IEC edition profiles, implementation-dependent parameters, compliance matrix.
- `iec_ir`: normalized project/type/POU/statement/expression model.
- `iec_syntax`: IEC textual lexer and Structured Text parser foundation.
- `iec_semantics`: symbol and type checking foundation.
- `iec_stdlib`: standard function catalog and initial evaluator.
- `iec_runtime`: deterministic scan-cycle interpreter foundation.
- `iec_c`: portable C backend foundation.
- `iec_plcopen`: PLCopen XML 2.01 import/export foundation.
- `rbcpp_cli`: `rbcpp` command line interface.

## Current Scope

This is the first implementation slice, not a completed full-standard compiler. It establishes
the crate boundaries and working end-to-end flow for simple IEC programs, plus explicit compliance
tracking for the rest of the IEC 61131-3:2003 surface.
