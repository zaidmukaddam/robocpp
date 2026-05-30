# Differential Validation

Run the generated cross-backend suite with:

```sh
cargo run -p xtask -- validate-differential
```

The suite generates representative IEC programs for arithmetic, conversions,
arrays, structs, strings, timers, counters, SFC, IL, native textual LD, native
textual FBD, and configurations/resources/tasks. Each case is parsed, checked by
semantic analysis, executed by the interpreter, emitted as C, compiled, run, and
compared against the interpreter trace.

The suite also runs metamorphic checks where formatting changes, declaration
ordering, and harmless parentheses must not change runtime behavior.

Checked-in external PLCopen fixtures are validated through
`cargo run -p xtask -- validate-corpus`. The current fixture set includes a
MIT-licensed CODESYS V3.5 SP10 Patch 4 PLCopen XML export at
`validation/corpus/plcopen/vendor/codesys_single_responsibility.xml`; expected
unsupported-dialect diagnostics are recorded in the adjacent `.diag` file, and
provenance/license text are recorded in
`validation/corpus/plcopen/vendor/THIRD_PARTY_NOTICES.md`.

## External Tool Comparison Candidates

External comparisons are manual or lab-specific because commercial tools often
require licensed installations, project-specific runtimes, and GUI automation.
Use these tools as second-opinion checks when available:

| Tool | Use | Source |
| --- | --- | --- |
| PLCopen XML / IEC 61131-10 | Neutral exchange baseline for exported/imported IEC projects. | https://www.plcopen.org/standards/logic/iec-61131-10/ |
| CODESYS Development System | Import/export PLCopen XML and compare accepted language behavior against a commercial IEC 61131-3 implementation. | https://content.helpme-codesys.com/en/CODESYS%20Development%20System/_cds_project_export_import.html |
| CODESYS Export PLCopenXML command | Export selected project objects in PLCopen XML format for RoboC++ import tests. | https://content.helpme-codesys.com/en/CODESYS%20Development%20System/_cds_cmd_export_plcopenxml.html |
| Beckhoff TwinCAT 3 | Compare PLC project and XML-level changes; useful when testing TwinCAT-origin XML or source imports. | https://infosys.beckhoff.com/content/1033/project_compare_tool/7609457291.html |
| IronPLC | Open-source second-opinion parser/checker for ST and PLCopen XML subsets. | https://www.ironplc.com/reference/compiler/source-formats/plcopen-xml.html |
| OpenPLC / Autonomy Edge | Open IEC 61131-style ST environment for smoke-level source behavior checks. | https://edge.autonomylogic.com/docs/openplc-editor/programming-languages/structured-text/st-basics |

Record every external comparison in `validation/differential/external-runs.md`
or the release validation report, including tool version, import/export format,
accepted dialect extensions, rejected features, and any manual edits needed.
