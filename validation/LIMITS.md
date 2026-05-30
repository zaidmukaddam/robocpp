# Validation Limits

These are current validation targets for robustness testing. They are not safety
certification limits and they do not replace deployment-specific timing, memory,
or target HAL validation.

Hard user-facing diagnostics exist for source size, PLCopen XML size, PLCopen
XML node count, PLCopen XML nesting depth, PLCopen XML text/attribute length,
expression depth, statement depth, symbol count, POU count, variable count,
generated-C size, and scan-cycle limits. `cargo run -p xtask --
validate-robustness` exercises those diagnostics plus timeout budgets and
large-input growth checks.

| Area | Current validation target |
| --- | --- |
| Text source size | Default maximum: 1 MiB; over-limit input emits a diagnostic before lexing. |
| PLCopen XML size | Default maximum: 1 MiB; over-limit input emits a diagnostic before import. |
| PLCopen XML node count | Default maximum: 150,000 XML nodes, using `max_plcopen_xml_nodes`. |
| PLCopen XML nesting depth | Default maximum: 256 XML element levels, using `max_plcopen_xml_depth`. |
| PLCopen XML text length | Default maximum: 65,535 bytes per text node, using `max_plcopen_xml_text_bytes`. |
| PLCopen XML attribute length | Default maximum: 65,535 bytes per attribute value, using `max_plcopen_xml_attribute_bytes`. |
| Expression nesting depth | At least 20 nested expression levels in CI corpus. |
| Symbol count | Default maximum: 150,000 named symbols. |
| POU count | Default maximum: 10,000 POUs. |
| Variable count | Default maximum: 100,000 declarations. |
| Generated-C output size | Default maximum: 1 MiB. |
| Runtime scan cycles | Default maximum: 10,000 cycles. |

The checked-in CI corpus starts smaller so normal contributor feedback remains
fast. Larger generated stress sweeps should be run as scheduled or release
validation jobs and summarized in `validation/releases/`.
