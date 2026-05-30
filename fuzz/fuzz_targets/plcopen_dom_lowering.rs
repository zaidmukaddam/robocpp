// SPDX-License-Identifier: MIT OR Apache-2.0

#![no_main]

use iec_diagnostics::Severity;
use iec_language_service::{
    analyze_document, document_graph_model, generated_c_metadata, validate_graph_model,
    DocumentInput, LanguageServiceOptions,
};
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(xml) = std::str::from_utf8(data) {
        let analysis = analyze_document(
            DocumentInput::new("fuzz.xml", xml).with_language_id("xml"),
            &LanguageServiceOptions::default(),
        );
        if has_error(&analysis.diagnostics) {
            return;
        }

        let graph = document_graph_model(&analysis);
        let _ = graph.to_json();
        let _ = validate_graph_model(&graph).to_json();
        let _ = generated_c_metadata(&analysis.project, "fuzz.c").to_json();
    }
});

fn has_error(diagnostics: &[iec_diagnostics::Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == Severity::Error)
}
