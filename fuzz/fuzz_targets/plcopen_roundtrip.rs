// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use iec_diagnostics::Severity;
use iec_plcopen::{export_plcopen_xml, import_plcopen_xml};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(xml) = std::str::from_utf8(data) {
        let imported = import_plcopen_xml("fuzz.xml", xml);
        if has_error(&imported.diagnostics) {
            return;
        }
        let exported = export_plcopen_xml(&imported.project);
        let _ = import_plcopen_xml("roundtrip.xml", &exported);
    }
});

fn has_error(diagnostics: &[iec_diagnostics::Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
}
