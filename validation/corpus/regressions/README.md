# Regression Fixtures

Minimized parser, PLCopen, semantic, runtime, or generated-C bugs found by
fuzzing and user reports belong here. Rejected fixtures should include a `.diag`
sidecar with expected diagnostic substrings.

Fixture metadata must include `origin=...`. Use `origin=fuzz` only for a
minimized input that came from a real fuzz crash, timeout, or sanitizer finding.
Seeded boundary cases that prevent regressions in known limits should use
`origin=seeded-limit`, and user reports should use `origin=user-report`.
