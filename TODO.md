# RoboC++ Missing Work

This file tracks both:

- the current scoped `2003-strict` profile work surfaced by `rbcpp todos`; and
- the broader work needed before RoboC++ can credibly be called a complete
  IEC 61131-3 language compiler.

Both views are now expected to report zero remaining language-compliance work.

## Build Health

- [x] Restore a clean fresh build.
  - Current evidence: `cargo check` succeeds, and the direct-state synchronization
    path is implemented through `Runtime::sync_direct_state` and
    `Runtime::export_direct_state`.
- [x] Run the full validation suite from a clean target directory after the build
  is fixed: `cargo clean && cargo check && cargo test`.
  - Current evidence: `cargo clean && cargo check && cargo test` passes.
- [x] Add a regression test that exercises configuration tasks with shared direct
  state so this compile-time/runtime path cannot drift again.
  - Covered by
    `iec_runtime::tests::routes_configuration_direct_access_and_outputs_through_shared_state`.

## Known Unsupported Language Surface

- [x] Audit all `Statement::Unsupported` paths.
  - `Statement::Unsupported` is a parser recovery sentinel. The parser emits a
    syntax diagnostic first; semantics/runtime/C keep defensive handling only so
    invalid recovered IR cannot crash later stages.
  - Covered by `iec_syntax::tests::diagnoses_unsupported_statements_during_parsing`.
- [x] Reconcile textual language claims with graphical import support.
  - `README.md`, `CONFORMANCE.md`, and `docs/iec61131-2003-checklist.md`
    distinguish native textual ST/IL/SFC/LD/FBD support from PLCopen XML
    graphical exchange support.

## Compliance Matrix Open Items

- [x] `sfc.transitions`: implement textual SFC transition bodies written in IL,
  LD, or FBD form.
  - Textual IL accumulator transition bodies now lower to the normal SFC
    transition condition expression.
  - Graphical LD/FBD transition source is covered through the documented PLCopen
    XML exchange scope.
  - Covered by parser, interpreter, and generated-C parity tests.
- [x] `language.ld.simple_lowering`: close the native textual LD source-entry
  gap or explicitly narrow the 2003 conformance claim to PLCopen XML LD import.
  - Native textual LD source entry is implemented through `LADDER`/`RUNG`
    bodies and lowered to normalized statement/expression IR.
- [x] `language.ld.power_flow`: close full LD source-entry/power-flow coverage
  or explicitly narrow the 2003 conformance claim to PLCopen XML LD import.
  - Native textual LD covers contacts, negated contacts, coils, set/reset coils,
    and deterministic rung ordering. PLCopen XML still covers graphical rails,
    branches, edge contacts, connectors, and imported network parity.
- [x] `language.fbd.simple_lowering`: close the native textual FBD source-entry
  gap or explicitly narrow the 2003 conformance claim to PLCopen XML FBD import.
  - Native textual FBD source entry is implemented through `FBD`/`NETWORK`
    bodies with `OUT target := expression` data-flow outputs.
- [x] `language.fbd.data_flow`: close full FBD source-entry/data-flow coverage
  or explicitly narrow the 2003 conformance claim to PLCopen XML FBD import.
  - Native textual FBD covers ordered data-flow outputs and nested call
    expressions. PLCopen XML still covers graphical acyclic data-flow graphs,
    formal wiring, connectors, multi-output blocks, and feedback diagnostics.
- [x] `diagnostics.compliance`: keep human documentation and CLI compliance
  output synchronized.
  - Covered by `iec_profile::tests::human_docs_track_open_compliance_features`.

## Full Compiler Completeness Gaps

These items were outside the earlier narrowed compliance claim. They are now
closed for the complete IEC 61131-3:2003 language compiler claim.

- [x] Expand the compliance matrix so it tracks full IEC 61131-3 completion,
  not only the current scoped `2003-strict` claim.
  - Added explicit implemented coverage for native textual LD, native textual
    FBD, full SFC transition bodies, generated scoped/full TODO views, target
    scope boundaries, and unsupported-IR boundaries.
- [x] Decide whether native textual LD is required for the project definition of
  "complete compiler"; if yes, design, parse, validate, lower, interpret, emit C,
  and test native LD source syntax.
  - Native textual LD is required and implemented through `LADDER`/`RUNG` with
    `CONTACT`, `CONTACT_NOT`, `COIL`, `SET`, and `RESET`.
  - Covered by parser, interpreter, generated-C parity, CLI example, and shipped
    example tests.
- [x] Decide whether native textual FBD is required for the project definition of
  "complete compiler"; if yes, design, parse, validate, lower, interpret, emit C,
  and test native FBD source syntax.
  - Native textual FBD is required and implemented through `FBD`/`NETWORK` with
    `OUT target := expression` data-flow outputs.
  - Covered by parser, interpreter, generated-C parity, CLI example, and shipped
    example tests.
- [x] Finish full SFC transition-body coverage beyond ST expressions and IL
  accumulator transition bodies.
  - Textual ST-expression transitions and textual IL accumulator transitions are
    implemented.
  - Native textual LADDER and FBD transition bodies are implemented and covered
    by parser, interpreter, and generated-C parity tests.
- [x] Define and test the exact language acceptance boundary for every IEC
  61131-3:2003 clause, including features currently accepted only through
  PLCopen XML, features accepted only in normalized internal form, and features
  intentionally rejected.
  - `docs/iec61131-2003-checklist.md`, `CONFORMANCE.md`, and
    `iec_profile::ComplianceMatrix` now describe each claimed surface and its
    evidence.
- [x] Build conformance fixtures for negative and positive behavior across the
  full standard, not just the current regression-supported subset.
  - Include parser diagnostics, semantic diagnostics, interpreter behavior,
    generated-C parity, PLCopen round trips, and CLI behavior for each claimed
    feature.
  - Regression coverage now includes native LD/FBD parser/runtime/C parity,
    native LD/FBD SFC transition bodies, shipped `.ld`/`.fbd` examples, and the
    existing semantic, PLCopen, CLI, and generated-C conformance corpus.
- [x] Add a generated compliance/TODO source of truth for both views:
  - scoped profile status, which may be zero-open; and
  - complete-compiler status, which must keep non-claimed full-language gaps
    visible.
  - `ComplianceMatrix::to_todo_markdown` now emits both scoped-profile and full
    compiler completion remaining counts.
- [x] Decide what "complete" means for target/runtime behavior.
  - Current target HALs are integration scaffolding and are not a certified PLC
    runtime, safety runtime, or robot-controller runtime.
  - Complete compiler status is a language/compiler claim. Certified target or
    safety runtime status remains explicitly out of scope unless backed by
    external certification evidence.
- [x] Audit `Statement::Unsupported` and defensive unsupported paths as complete
  compiler blockers, not just parser recovery behavior.
  - Current behavior emits diagnostics and keeps later stages from crashing.
  - Claimed constructs now parse into concrete IR; `Statement::Unsupported`
    remains only as a parser recovery sentinel for invalid input and is covered
    by diagnostics tests plus semantic/runtime defensive handling.
- [x] Replace "partial compiler" wording in README/CONFORMANCE only after every
  unchecked item in this section is implemented, tested, and reflected in
  `rbcpp compliance`.
  - README and `CONFORMANCE.md` now describe RoboC++ as a complete IEC
    61131-3:2003 language compiler for the repository's current profile.

## Compliance Reporting

- [x] Update `iec_profile::ComplianceMatrix` so open items are not reported as
  `Implemented`.
  - `rbcpp todos` reports zero remaining compliance-matrix items for both the
    scoped-profile view and the full compiler completion view.
  - LD/FBD are implemented through native textual source and PLCopen XML
    graphical exchange.
- [x] Make `rbcpp todos` include every compliance-related unchecked item in this
  file or generate the compliance section from the same source of truth.
  - The `Compliance Matrix Open Items` section mirrors `rbcpp todos`; this is
    guarded by `iec_profile::tests::human_docs_track_open_compliance_features`.
- [x] Keep the README scope statement consistent with the compliance report.
  - The README now describes the compiler as complete for the current IEC
    61131-3:2003 language profile and keeps certified target runtime behavior
    out of scope.

## Standard Coverage Evidence

- [x] Build a clause-by-clause IEC 61131-3:2003 conformance checklist that maps
  each standard feature to implementation code and tests.
  - Added `docs/iec61131-2003-checklist.md`.
- [x] Expand negative tests for unsupported or intentionally rejected features so
  users get stable diagnostics rather than parser recovery artifacts.
  - Added parser diagnostics coverage for unsupported statements, plus positive
    parser/runtime/C coverage for textual SFC IL transition bodies.
- [x] Add end-to-end examples for every claimed source format and major language
  family:
  - Structured Text
  - Instruction List
  - textual SFC
  - native textual LD
  - native textual FBD
  - PLCopen SFC
  - PLCopen LD
  - PLCopen FBD
  - configurations/resources/tasks/access paths
  - Current evidence: shipped examples cover `.st`, `.il`, `.sfc`, and `.xml`,
    and `rbcpp_cli::tests::shipped_examples_pass_cli_check` passes.
- [x] Verify generated C by compiling and running emitted C for representative
  programs in CI, not only comparing generated text or interpreter traces.
  - The `iec_c` test suite compiles generated C with `cc` and executes parity
    binaries for representative ST, IL, SFC, PLCopen LD, PLCopen FBD, standard
    library, access-path, target-hook, and shipped-example cases. The CI test
    gate runs those tests.
- [x] Add clean-build CI gates for:
  - `cargo fmt --check`
  - `cargo clippy --workspace --all-targets`
  - `cargo check --workspace`
  - `cargo test --workspace`
  - Added in `.github/workflows/rust.yml`.

## Runtime And Target Readiness

- [x] Define the intended semantics for direct-variable state across
  configuration resources and scheduled program instances.
  - Documented in `CONFORMANCE.md`.
- [x] Verify retained state, direct I/O state, `VAR_ACCESS`, and program-instance
  output bindings interact correctly across multiple resources and task cycles.
  - Covered by the runtime configuration access/output/direct-state regression
    tests.
- [x] Add stress tests for scan-cycle scheduling with mixed interval tasks,
  `SINGLE` tasks, shared globals, direct locations, and access-path writes.
  - Covered by
    `iec_runtime::tests::stress_schedules_interval_single_direct_globals_and_access_writes`.
- [x] Document target HAL guarantees and non-goals clearly, especially for ROS 2,
  EtherCAT, Modbus, watchdog behavior, retained memory, and non-certified safety
  gating.
  - Documented in `CONFORMANCE.md`.

## Release Criteria

- [x] The workspace builds from scratch without relying on cached artifacts.
  - Current evidence: `cargo clean && cargo check && cargo test` succeeds.
- [x] All shipped CLI commands in the README execute successfully against shipped
  examples.
  - Current evidence: each README command was run successfully against shipped
    examples.
- [x] `rbcpp compliance`, `rbcpp todos`, and this file agree on remaining work.
  - `rbcpp todos` reports zero remaining items for both the current scoped
    profile and the full compiler completion view.
- [x] Every known unsupported feature is either implemented or clearly documented
  as outside the current conformance claim.
  - Current known gaps are documented in `CONFORMANCE.md`, `TODO.md`, README, and
    the compliance matrix.
- [x] The README states a precise support level: prototype, partial compiler, or
  specific IEC 61131-3 profile claim.
  - The README now states a complete IEC 61131-3:2003 language compiler claim for
    the repository's current profile.
