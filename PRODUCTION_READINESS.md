# RoboC++ Production Compiler Readiness Plan

This file tracks the extra evidence needed before RoboC++ should be described as
production-compiler quality for industrial robotics, embedded controls, or other
high-consequence IEC 61131 deployments.

The current repository already has a strong language-conformance story for its
claimed IEC 61131-3:2003 profile. That is not the same as production compiler
readiness. Production readiness means the project can show durable evidence that
its parser, semantic analyzer, runtime, PLCopen XML handling, generated C, and
target integration boundaries behave correctly under hostile inputs, large
programs, vendor dialects, and repeated release pressure.

This plan is not a safety-certification plan. Certified safety claims require an
external certification process, a defined safety lifecycle, tool qualification,
hazard analysis, traceable requirements, and target-specific validation outside
this repository.

## Current Positioning

RoboC++ can credibly be positioned as:

- a reviewable, auditable, vendor-independent IEC 61131 toolchain;
- a compiler/runtime toolkit for teams willing to own validation;
- a conformance-oriented implementation with tests, examples, generated-C
  parity checks, CLI checks, and explicit target HAL non-goals.

RoboC++ should not yet be positioned as:

- a drop-in replacement for a certified commercial PLC IDE;
- a safety-certified compiler or runtime;
- a tool that can be trusted for safety-critical code without independent
  validation by the deploying team.

## Evidence Already Present

The repository already includes these foundations:

- `CONFORMANCE.md` and `docs/iec61131-2003-checklist.md` document the current
  IEC 61131-3:2003 profile and its evidence.
- The Rust CI workflow runs `cargo fmt --check`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo check --workspace`, and `cargo test
  --workspace`.
- The test suite covers parser, semantic, runtime, generated-C, PLCopen,
  target-adapter, standard-library, CLI, and language-service behavior.
- Shipped examples cover `.st`, `.il`, `.sfc`, `.ld`, `.fbd`, and `.xml`
  source families.
- `CONFORMANCE.md` already states that target helpers are integration
  scaffolding, not certified PLC or robot-controller safety runtimes.
- `validation/` now contains a production-readiness corpus, release evidence
  area, deployment validation template, and documented validation commands.
- `xtask` provides `validate-corpus`, `fuzz-smoke`, and `release-report`
  commands.
- `fuzz/` contains first-class `cargo-fuzz` targets for textual parsing,
  PLCopen XML import, parse/semantic/codegen pipeline coverage, and PLCopen
  round trips.
- PLCopen XML import is parsed and canonicalized through a real XML parser with
  DTD/entity rejection and PLCopen-specific node-count, depth, text, and
  attribute bounds before project lowering.
- CI runs the validation corpus and bounded fuzz smoke checks in addition to the
  normal Rust gates.

## Production Readiness Gaps

These are the main residual gaps after the current validation work:

- Hardware-in-the-loop evidence is still deployment-specific and must be supplied
  by the integrator for a concrete robot, machine, or embedded product.
- Commercial-tool differential runs require licensed tools and should be
  recorded in release evidence when available; no commercial external run is
  recorded in the current repository evidence.
- The compiler crates have been split into reviewable modules, C generation uses
  fallible emission paths, and PLCopen lowering now traverses the validated DOM;
  release gates should keep these properties from regressing.
- Safety certification remains outside this repository and must not be claimed
  without an external certification process.

## Readiness Milestones

### M0: Honest Public Positioning

Goal: Make the support level impossible to misread.

- [x] Add a short "Production Readiness" section to `README.md` linking to this
  file.
- [x] Keep `CONFORMANCE.md` focused on language support and make this file the
  place for production-readiness evidence.
- [x] Use this wording consistently:
  - "complete for the repository's current IEC 61131-3:2003 language profile";
  - "not safety-certified";
  - "requires deployment-specific validation";
  - "not a certified PLC IDE replacement."
- [x] Add a release checklist item requiring this file to be reviewed before
  changing compiler-readiness claims.

Acceptance gate:

- A new user can read `README.md`, `CONFORMANCE.md`, and this file and understand
  the difference between language-profile completeness, production compiler
  readiness, and safety certification.

### M1: Corpus Layout And Regression Evidence

Goal: Make validation inputs visible, reviewable, and easy to extend.

- [x] Create a top-level `validation/` directory.
- [x] Add `validation/corpus/accepted/` for valid programs that must parse and
  type-check.
- [x] Add `validation/corpus/rejected/` for invalid programs with expected
  diagnostic snapshots.
- [x] Add `validation/corpus/runtime/` for interpreter behavior fixtures.
- [x] Add `validation/corpus/c-parity/` for programs that must type-check,
  generate C, and compile as C11.
- [x] Add `validation/corpus/plcopen/roundtrip/` for XML import/export fixtures.
- [x] Add `validation/corpus/plcopen/vendor/` for real-world vendor XML exports
  when licensing permits.
- [x] Add `validation/corpus/stress/` for large or deeply nested inputs.
- [x] Add a runner command, for example `cargo xtask validate-corpus`, that
  executes all corpus checks locally and in CI.
- [x] Require every corpus fixture to declare the feature or clause it covers.

Acceptance gate:

- CI can run the complete corpus with one command.
- A failing corpus case tells the contributor whether the failure is parse,
  diagnostics, semantics, runtime, C generation, XML import/export, or CLI
  behavior.

### M2: Fuzzing

Goal: Prove the frontends and compiler pipeline survive untrusted input.

- [x] Add `cargo-fuzz` or another first-class Rust fuzzing setup under
  `fuzz/`.
- [x] Add a textual parser fuzz target for `.st`, `.il`, `.sfc`, `.ld`, and
  `.fbd` inputs.
- [x] Add a PLCopen XML fuzz target for malformed and adversarial XML.
- [x] Add a parse-plus-semantic fuzz target that verifies diagnostics are
  returned instead of panics.
- [x] Add a parse-plus-codegen fuzz target that ensures invalid input never
  reaches generated C without diagnostics.
- [x] Add a PLCopen import/export round-trip fuzz target for valid generated
  project shapes.
- [x] Seed fuzzing from `examples/` and `validation/corpus/`.
- [x] Store minimized crashing inputs in `validation/corpus/regressions/`.
- [x] Label regression fixture origin so real fuzz trophies are distinguishable
  from seeded robustness and limit fixtures.
- [x] Add a short CI fuzz smoke job that runs each target for a bounded time.
- [x] Add a scheduled long-running fuzz job or document the external fuzzing
  service used by maintainers.

Acceptance gate:

- Every parser and PLCopen XML bug found by fuzzing becomes a permanent
  regression fixture.
- The project can report the latest fuzz duration, target list, crash count, and
  corpus size for each release.

### M3: Differential And Cross-Backend Testing

Goal: Catch semantic drift by comparing independent execution paths.

- [x] Add a generated test-program suite for arithmetic, conversions, arrays,
  structs, strings, timers, counters, SFC, IL, LD, FBD, tasks, and resources.
- [x] Run each generated program through parser, semantics, interpreter, and
  generated C.
- [x] Compare interpreter traces against generated-C traces.
- [x] Add metamorphic tests where formatting, harmless parentheses, declaration
  ordering, and equivalent expressions must not change behavior.
- [x] Build PLCopen XML round-trip tests that compare normalized project shape
  rather than raw XML text.
- [x] Document which commercial or open IEC 61131 tools can be used for manual
  or automated differential checks.
- [x] Make release evidence report whether commercial external differential
  runs have been recorded.
- [x] Import representative vendor exports and record any accepted dialect
  extensions or intentionally rejected features.

Acceptance gate:

- A compiler change that alters runtime behavior without updating an expected
  trace fails CI.
- PLCopen round-trip tests verify normalized behavior, not just successful
  serialization.

### M4: Diagnostics And Error Recovery Stability

Goal: Make invalid input behavior predictable and auditable.

- [x] Add diagnostic snapshot tests for common syntax errors in each supported
  source family.
- [x] Add semantic diagnostic snapshot tests for type errors, scoping errors,
  invalid calls, invalid task/resource configuration, and bad access paths.
- [x] Add malformed PLCopen XML diagnostic snapshots.
- [x] Add tests that verify parser recovery does not create executable IR for
  unsupported or invalid constructs.
- [x] Add a diagnostic compatibility policy that explains when message text,
  spans, and codes may change.

Acceptance gate:

- User-facing diagnostics are stable enough for IDE integration and regression
  review.
- Invalid input cannot silently become valid executable behavior through parser
  recovery.

### M5: Robustness, Limits, And Sanitizers

Goal: Define and test compiler behavior at scale and at boundaries.

- [x] Define supported limits for source size, XML size, nesting depth, symbol
  count, POU count, variable count, and generated-C output size.
- [x] Add stress fixtures near those limits.
- [x] Add explicit diagnostics for inputs that exceed supported limits.
- [x] Run sanitizer builds where practical, including AddressSanitizer or
  UndefinedBehaviorSanitizer for generated C test binaries.
- [x] Add timeout protection for parser, XML import, semantic analysis,
  interpreter execution, and generated-C compilation tests.
- [x] Add memory-growth checks for large PLCopen XML and large generated-C
  projects.

Acceptance gate:

- Oversized or adversarial inputs fail with controlled diagnostics rather than
  unbounded CPU, memory growth, panics, or generated invalid artifacts.

### M6: Release Evidence

Goal: Make every release auditable after the fact.

- [x] Add `validation/releases/` for generated release-readiness reports.
- [x] Record the exact git commit, Rust toolchain, CI workflow, and
  command set used for release validation.
- [x] Record counts for corpus fixtures and the command set used for release
  validation.
- [x] Record open known issues and unsupported production-readiness items.
- [x] Require release notes to link to the validation report.
- [x] Version the conformance profile and production-readiness status
  separately.

Acceptance gate:

- A user can audit what was validated for a specific release without trusting a
  vague "tests pass" claim.

### M7: Deployment Validation Boundary

Goal: Help robotics and embedded teams validate their own target integration.

- [x] Add target integration validation templates for scan timing, retained
  state, I/O mapping, watchdog policy, E-stop/protective-stop gating, operator
  enable, startup state, shutdown state, and fault recovery.
- [x] Add examples that show how generated C should be wrapped in a target HAL
  with explicit validation points.
- [x] Add simulator-in-the-loop tests for representative Modbus, EtherCAT PDO
  image, ROS 2 bridge, and file-backed I/O flows.
- [x] Document which behavior is guaranteed by the compiler and which behavior
  is the integrator's responsibility.
- [x] Add a "do not use for safety-critical operation without independent
  validation and certification" notice to deployment-facing docs.

Acceptance gate:

- A target integrator can produce a validation checklist for their robot,
  machine, or embedded product using repository-provided templates.

### M8: PLCopen XML Hardening

Goal: Make PLCopen exchange robust against malformed or hostile XML before any
PLCopen project lowering runs.

- [x] Add a real XML pre-parse layer for PLCopen imports.
- [x] Canonicalize parsed PLCopen XML before lowering so prefixed PLCopen
  elements and single-quoted attributes are handled by the XML stack.
- [x] Split PLCopen XML limits into explicit implementation parameters for
  XML node count, nesting depth, text bytes, and attribute bytes.
- [x] Reject DTDs, entity declarations, malformed XML, unknown namespace
  prefixes, excessive XML node count, excessive nesting depth, oversized text
  nodes, and oversized attribute values.
- [x] Bound PLCopen `arrayValue` `repetitionValue` expansion with the
  implementation array-element limit.
- [x] Preserve nonstandard namespace declarations needed by vendor `addData`
  payloads during import/export round trips, including declarations below the
  project root.
- [x] Add hostile PLCopen XML fixtures to the validation corpus.

Acceptance gate:

- Hostile XML fails with controlled diagnostics before custom PLCopen lowering.
- Vendor XML metadata that is accepted by the importer remains well-formed after
  export.

## Definition Of Done

RoboC++ should not be called production-compiler quality for non-certified
deployments until all of these remain true and the release validation gates pass:

- [x] The conformance checklist, corpus, fuzz targets, differential tests, and
  release evidence agree on the supported language/profile surface.
- [x] Fuzzing has run long enough to be meaningful, and all found crashes are
  fixed or explicitly documented with minimized reproducers.
- [x] PLCopen XML import/export has hostile-input, round-trip, and vendor-fixture
  coverage.
- [x] Interpreter and generated-C behavior are compared across a broad generated
  program suite.
- [x] Diagnostics for invalid input are stable, snapshot-tested, and do not hide
  executable recovered IR.
- [x] Resource limits are documented and tested.
- [x] Release validation reports are generated and linked from release notes.
- [x] Documentation remains explicit that safety certification and target
  deployment validation are outside the compiler's automatic guarantees.

## Suggested First Pull Request

The highest-leverage first PR is now implemented:

- create `validation/corpus/` with accepted, rejected, runtime, C-parity,
  PLCopen, stress, and regression subdirectories;
- copy a few existing examples into the corpus as initial fixtures;
- add a corpus runner that checks parsing, diagnostics, semantics, runtime, and
  generated-C parity for those fixtures;
- add the runner to CI;
- link this file from `README.md`.

Fuzzing scaffolding is also present under `fuzz/`, with a scheduled workflow for
longer runs. Ongoing maintenance should keep feeding every minimized crash back
into `validation/corpus/regressions/`.
