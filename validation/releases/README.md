# Release Evidence

Generate a release validation report before publishing a compiler-readiness
claim:

```sh
cargo run -p xtask -- release-report --output validation/releases/<release>.md
```

Each report should be linked from release notes and should record the exact
commit, toolchain, command set, corpus size, known production-readiness gaps, and
non-certification scope.

Release notes must include a link to the generated validation report before any
compiler-readiness claim is made.
