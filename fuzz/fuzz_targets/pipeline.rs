// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use iec_c::generate_c;
use iec_diagnostics::Severity;
use iec_semantics::{check_project, CheckOptions};
use iec_syntax::parse_project;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(source) = std::str::from_utf8(data) {
        let parsed = parse_project("fuzz.st", source);
        if has_error(&parsed.diagnostics) {
            return;
        }
        let diagnostics = check_project(&parsed.project, &CheckOptions::default());
        if has_error(&diagnostics) {
            return;
        }
        let _ = generate_c(&parsed.project, None);
    }
});

fn has_error(diagnostics: &[iec_diagnostics::Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
}
