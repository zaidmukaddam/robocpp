# PLCopen Vendor Dialect Record

| Dialect/source | Fixture | Status | Notes |
| --- | --- | --- | --- |
| Generic PLCopen 2.01 | `validation/corpus/plcopen/roundtrip/simple_fbd.xml` | accepted | Baseline namespace and FBD data-flow import/export. |
| Vendor `addData` metadata | `validation/corpus/plcopen/vendor/vendor_adddata.xml` | accepted | Project-level and POU-level vendor metadata are accepted as PLCopen extension data. |
| CODESYS V3.5 SP10 Patch 4 PLCopen XML | `validation/corpus/plcopen/vendor/codesys_single_responsibility.xml` | imported with expected diagnostics | MIT-licensed third-party CODESYS export from RoDoerIng/PlcOpen. RoboC++ imports it best-effort, then records unsupported CODESYS/OOP dialect features and the TC6 2.00 namespace in `codesys_single_responsibility.diag`. |
| Beckhoff TwinCAT 3 XML/project data | external release fixture | documented candidate | TwinCAT XML comparison can distinguish XML-level changes; record any TwinCAT-specific metadata preserved or intentionally ignored. |
| OpenPLC/Beremiz-origin PLCopen XML | external release fixture | documented candidate | Commit only license-compatible minimized fixtures; otherwise record URL, version, hash, and import result in release evidence. |
