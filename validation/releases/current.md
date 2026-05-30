# RoboC++ Release Validation Report

- Git commit: `2f346aaa163d9f837aa43b351636ba779205140e`
- Rust toolchain: `rustc 1.95.0 (59807616e 2026-04-14) (Homebrew)`
- Host: `macos/aarch64`
- CI workflow: `.github/workflows/rust.yml`
- Scheduled workflow: `.github/workflows/scheduled-validation.yml`
- Corpus source fixtures: `24`
- Accepted fixtures: `4`
- Rejected fixtures: `11`
- Runtime trace fixtures: `1`
- Generated-C fixtures: `1`
- PLCopen fixtures: `5`
- Stress fixtures: `1`
- Regression fixtures: `1`
- Fuzz-discovered regression fixtures: `0`
- Non-fuzz regression fixtures: `1`
- Generated differential cases: `12`
- Commercial external differential runs recorded: `0`
- Fuzz targets: `4`
- Required commands:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo check --workspace`
  - `cargo test --workspace`
  - `cargo run -p xtask -- validate-corpus`
  - `cargo run -p xtask -- validate-differential`
  - `cargo run -p xtask -- validate-robustness`
  - `cargo run -p xtask -- validate-sanitizers`
  - `cargo run -p xtask -- fuzz-smoke`
  - scheduled `cargo +nightly fuzz run <target> -- -max_total_time=1800`

## Versioned Readiness Status

```toml
conformance_profile = "iec61131-3:2003-strict"
conformance_status = "implemented-for-current-profile"
production_readiness_status = "validation-required"
safety_certification_status = "not-certified"
readiness_evidence_version = "2026.05"
```

## Known Production-Readiness Scope

RoboC++ is not safety-certified. Target deployment validation, tool qualification, hazard analysis, and certification evidence remain the responsibility of the deploying organization. Release notes must link to this report before publishing compiler-readiness claims.
