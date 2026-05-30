// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_diagnostics::json_escape;
use iec_profile::EditionProfile;

use crate::LanguageServiceOptions;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceCapabilities {
    pub profile: EditionProfile,
    pub source_formats: Vec<&'static str>,
    pub document_symbols: bool,
    pub completions: bool,
    pub hover: bool,
    pub diagnostics: bool,
    pub simulation: bool,
    pub generated_c: bool,
    pub plcopen_import: bool,
    pub workspace_analysis: bool,
    pub source_structure: bool,
    pub incremental_analysis: bool,
    pub symbol_index: bool,
    pub type_index: bool,
    pub code_actions: bool,
    pub formatter: bool,
    pub refactors: bool,
    pub graph_models: bool,
    pub debug_hooks: bool,
    pub generated_c_metadata: bool,
    pub diagnostic_subcodes: bool,
}

impl ServiceCapabilities {
    pub fn for_options(options: &LanguageServiceOptions) -> Self {
        Self {
            profile: options.profile,
            source_formats: vec!["st", "il", "ld", "fbd", "sfc", "xml"],
            document_symbols: true,
            completions: true,
            hover: true,
            diagnostics: true,
            simulation: true,
            generated_c: true,
            plcopen_import: true,
            workspace_analysis: true,
            source_structure: true,
            incremental_analysis: true,
            symbol_index: true,
            type_index: true,
            code_actions: true,
            formatter: true,
            refactors: true,
            graph_models: true,
            debug_hooks: true,
            generated_c_metadata: true,
            diagnostic_subcodes: true,
        }
    }

    pub fn to_json(&self) -> String {
        let formats = self
            .source_formats
            .iter()
            .map(|format| format!("\"{}\"", format))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"profile\":\"{}\",\"sourceFormats\":[{}],\"features\":{{\"documentSymbols\":{},\"completions\":{},\"hover\":{},\"diagnostics\":{},\"simulation\":{},\"generatedC\":{},\"plcopenImport\":{},\"workspaceAnalysis\":{},\"sourceStructure\":{},\"incrementalAnalysis\":{},\"symbolIndex\":{},\"typeIndex\":{},\"codeActions\":{},\"formatter\":{},\"refactors\":{},\"graphModels\":{},\"debugHooks\":{},\"generatedCMetadata\":{},\"diagnosticSubcodes\":{}}}}}",
            json_escape(&format!("{}", self.profile)),
            formats,
            self.document_symbols,
            self.completions,
            self.hover,
            self.diagnostics,
            self.simulation,
            self.generated_c,
            self.plcopen_import,
            self.workspace_analysis,
            self.source_structure,
            self.incremental_analysis,
            self.symbol_index,
            self.type_index,
            self.code_actions,
            self.formatter,
            self.refactors,
            self.graph_models,
            self.debug_hooks,
            self.generated_c_metadata,
            self.diagnostic_subcodes
        )
    }
}
