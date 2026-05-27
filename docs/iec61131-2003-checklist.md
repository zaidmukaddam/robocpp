# IEC 61131-3:2003 Conformance Checklist

This checklist summarizes the current 2003-profile support evidence. The
machine-readable source is `iec_profile::ComplianceMatrix`; use `rbcpp
compliance` and `rbcpp todos` to inspect the current status.

| Clause / table | Matrix IDs | Status | Evidence |
| --- | --- | --- | --- |
| 2.1 common elements and lexical rules | `common.*` | Implemented | Lexer/parser tests cover characters, identifiers, keywords, whitespace, comments, and pragmas. |
| 2.2 literals | `literals.*` | Implemented | Syntax, semantic, runtime, and C parity tests cover numeric, string/WSTRING, duration, DATE, TOD, and DT literals. |
| 2.3 data types | `types.*` | Implemented | Semantic/runtime/C tests cover elementary, generic, alias, enum, subrange, array, structure, and bounded string types. |
| 2.4 variables | `variables.*` | Implemented | Declaration, initialization, retain, constant, direct-location, incomplete-location, access-path, and external-variable tests cover supported paths. |
| 2.5 POUs and standard library | `pou.*`, `stdlib.*` | Implemented | User functions, function blocks, programs, EN/ENO, conversion, numeric, bit, string, date/time, enum, bistable, edge, counter, timer, and communication hook tests cover interpreter and C parity. |
| 2.6 SFC steps/actions/sequence | `sfc.steps`, `sfc.actions.*`, `sfc.sequence_evolution`, `sfc.compliance_sets` | Implemented | Parser, semantic, runtime, PLCopen, and generated-C tests cover steps, actions, qualifiers, divergence/convergence, priorities, and compliance reporting. |
| 2.6 SFC transitions | `sfc.transitions` | Implemented | ST-expression textual transitions, textual IL accumulator transition bodies, native textual LADDER transition bodies, native textual FBD transition outputs, and PLCopen graphical transition topology are covered. |
| 2.7 configurations/resources/tasks | `configuration.*` | Implemented | Runtime tests cover globals, resources, tasks, interval scheduling, `SINGLE` event scheduling, program-instance initializers, output bindings, access paths, direct locations, and mixed scheduling stress cases. |
| 3.2 Instruction List | `language.il.*` | Implemented | Parser, semantic, runtime, and generated-C tests cover line-oriented IL, typed mnemonics, operators, parenthesized operands, calls, jumps, and returns. |
| 3.3 Structured Text | `language.st.*` | Implemented | Parser, semantic, runtime, and generated-C tests cover expressions, assignments, calls, IF/CASE, FOR/WHILE/REPEAT, RETURN, and EXIT diagnostics. |
| 4.2 Ladder Diagram | `language.ld.*` | Implemented | Native textual LD and PLCopen XML import/export/lowering cover LD contacts, coils, set/reset behavior, negated contacts, deterministic rung/network ordering, and generated-C parity. |
| 4.3 Function Block Diagram | `language.fbd.*` | Implemented | Native textual FBD and PLCopen XML import/export/lowering cover data-flow outputs, nested call expressions, acyclic graphical data-flow graphs, formal wiring, connectors, feedback diagnostics, and generated-C parity. |
| PLCopen XML 2.01 exchange | `plcopen.*` | Implemented | Round-trip tests cover project metadata, vendor extensions, POUs, data types, variables, configurations, LD, FBD, and SFC. |
| Backends and diagnostics | `backend.*`, `diagnostics.*`, `parameters.*` | Implemented | Interpreter, generated-C, target ABI, diagnostics, Annex D reporting, and synchronized compliance reporting are covered by regression tests. |

## Open Conformance Items

`rbcpp todos` is expected to report zero open items for both the scoped profile
view and the full compiler completion view.
