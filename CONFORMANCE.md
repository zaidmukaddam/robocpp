# RoboC++ Support And Conformance Notes

RoboC++ is a complete IEC 61131-3:2003 language compiler for the repository's
current profile. The authoritative machine-readable status is the
`iec_profile::ComplianceMatrix` surfaced by:

```sh
rbcpp compliance
rbcpp todos
```

## Source Formats

| Format | Current status |
| --- | --- |
| Structured Text (`.st`) | Implemented for POU, declaration, expression, statement, configuration, and runtime paths covered by the regression suite. |
| Instruction List (`.il`) | Implemented for line-oriented and semicolon-delimited IL, including typed mnemonics, jumps, returns, calls, and generated-C parity coverage. |
| Textual SFC (`.sfc` and embedded SFC) | Implemented. ST-expression transitions, textual IL accumulator transition bodies, native textual LADDER transition bodies, native textual FBD transition outputs, steps, actions, action associations, qualifiers, divergence/convergence, priorities, and generated-C parity are covered. |
| Native textual LD (`.ld` and embedded `LADDER`) | Implemented. `LADDER`/`RUNG` bodies with `CONTACT`, `CONTACT_NOT`, `COIL`, `SET`, and `RESET` lower to normalized IR with parser, interpreter, generated-C, CLI, and example coverage. |
| Native textual FBD (`.fbd` and embedded `FBD`) | Implemented. `FBD`/`NETWORK` bodies with `OUT target := expression` data-flow outputs lower to normalized IR with parser, interpreter, generated-C, CLI, and example coverage. |
| PLCopen SFC (`.xml`) | Implemented for the imported/exported graphical SFC structures covered by tests, including branch connectors, jumps, macro steps, and action blocks. |
| PLCopen LD (`.xml`) | Implemented. PLCopen XML LD import/export and lowering cover rails, contacts, coils, branches, edge contacts, set/reset coils, connectors, and generated-C parity. |
| PLCopen FBD (`.xml`) | Implemented. PLCopen XML FBD import/export and lowering cover acyclic data-flow graphs, formal wiring, connectors, multi-output blocks, feedback diagnostics, and generated-C parity. |

## Runtime Semantics

The deterministic runtime executes scan cycles for programs and configurations.
Configuration execution keeps shared state for configuration globals, resource
globals, direct locations, scheduled program instances, access-path writes, and
program-instance output bindings. Direct locations such as `%QX0.0` are shared
through configuration-level direct state so one scheduled program can publish a
located output that another scheduled program reads in the same scan according
to task ordering.

## Target HAL Scope

Generated C exposes hooks for located I/O, direct-variable storage, retained
state load/save, scan lifecycle callbacks, watchdog integration, and monotonic
cycle time. The `rbcpp_target` crate provides deployment adapters and testable
mapping layers for file-backed I/O, Modbus, EtherCAT PDO images, ROS 2 topic and
parameter bridges, retained-state files, access-path bindings, watchdog helpers,
supervisor reports, and non-certified safety gating.

These target helpers are integration scaffolding. They are not a safety-certified
PLC or robot-controller runtime, and the project should not claim certified
safety behavior without external certification evidence.

## Verification Gates

Before release, run and keep passing:

```sh
cargo fmt --check
cargo clippy --workspace --all-targets
cargo check --workspace
cargo test --workspace
```

Generated C is compiled and executed for representative programs in CI before
release claims are made.
