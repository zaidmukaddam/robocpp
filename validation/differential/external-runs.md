# External Differential Runs

No licensed commercial-tool run is required for normal contributor CI.

For release-readiness claims, record external runs here or in the generated
release report:

As of 2026-05-29, no commercial external differential run has been recorded.
The table below currently records the internal generated interpreter/C
differential suite only.

| Date | Tool | Version | Fixture | Result | Dialect notes |
| --- | --- | --- | --- | --- | --- |
| 2026-05-29 | RoboC++ internal generated suite | current workspace | `cargo run -p xtask -- validate-differential` | passing | Interpreter and generated C matched for generated arithmetic, conversions, arrays, structs, strings, timers, counters, SFC, IL, LD, FBD, and task/resource cases. |
