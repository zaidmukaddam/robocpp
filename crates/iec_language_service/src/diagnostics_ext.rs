// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_diagnostics::{json_escape, Diagnostic, DiagnosticCode};

use crate::{DocumentAnalysis, SourceRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticDescriptor {
    pub stable_code: String,
    pub category: String,
    pub subcode: String,
    pub message: String,
    pub labels: Vec<DiagnosticLabel>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiagnosticLabel {
    pub role: DiagnosticLabelRole,
    pub message: String,
    pub range: Option<SourceRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticLabelRole {
    Primary,
    Secondary,
}

impl DiagnosticDescriptor {
    pub fn to_json(&self) -> String {
        let labels = self
            .labels
            .iter()
            .map(DiagnosticLabel::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"stableCode\":\"{}\",\"category\":\"{}\",\"subcode\":\"{}\",\"message\":\"{}\",\"labels\":[{}]}}",
            json_escape(&self.stable_code),
            json_escape(&self.category),
            json_escape(&self.subcode),
            json_escape(&self.message),
            labels
        )
    }
}

impl DiagnosticLabel {
    pub fn to_json(&self) -> String {
        let range = self
            .range
            .as_ref()
            .map(SourceRange::to_json)
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"role\":\"{}\",\"message\":\"{}\",\"range\":{}}}",
            self.role.as_str(),
            json_escape(&self.message),
            range
        )
    }
}

impl DiagnosticLabelRole {
    pub fn as_str(self) -> &'static str {
        match self {
            DiagnosticLabelRole::Primary => "primary",
            DiagnosticLabelRole::Secondary => "secondary",
        }
    }
}

pub fn diagnostic_descriptors(analysis: &DocumentAnalysis) -> Vec<DiagnosticDescriptor> {
    analysis
        .diagnostics
        .iter()
        .map(|diagnostic| diagnostic_descriptor(diagnostic, analysis))
        .collect()
}

pub fn diagnostic_descriptor(
    diagnostic: &Diagnostic,
    analysis: &DocumentAnalysis,
) -> DiagnosticDescriptor {
    let subcode = diagnostic_subcode(diagnostic);
    let primary_range = diagnostic.span.as_ref().map(|span| SourceRange {
        uri: span.source.clone(),
        start: span.start,
        end: span.end,
        start_position: crate::position_at(&analysis.source.text, span.start),
        end_position: crate::position_at(&analysis.source.text, span.end),
    });
    let mut labels = vec![DiagnosticLabel {
        role: DiagnosticLabelRole::Primary,
        message: diagnostic.message.clone(),
        range: primary_range,
    }];
    if let Some(secondary) = related_symbol_range(diagnostic, analysis) {
        labels.push(DiagnosticLabel {
            role: DiagnosticLabelRole::Secondary,
            message: "related declaration".to_string(),
            range: Some(secondary),
        });
    }
    DiagnosticDescriptor {
        stable_code: format!("{}-{}", diagnostic.code.stable_id(), subcode),
        category: diagnostic.code.as_str().to_string(),
        subcode,
        message: diagnostic.message.clone(),
        labels,
    }
}

fn diagnostic_subcode(diagnostic: &Diagnostic) -> String {
    let message = diagnostic.message.to_ascii_lowercase();
    let subcode = if message.contains("unknown variable")
        || message.contains("unknown type")
        || message.contains("unknown function")
        || message.contains("unknown program")
        || message.contains("unknown target")
    {
        "unknown-symbol"
    } else if message.contains("duplicate") {
        "duplicate-declaration"
    } else if message.contains("type")
        && (message.contains("mismatch")
            || message.contains("cannot assign")
            || message.contains("requires"))
    {
        "type-mismatch"
    } else if message.contains("direct") || message.contains("location") {
        "invalid-direct-location"
    } else if message.contains("missing")
        && (message.contains("end_") || message.contains("closing"))
    {
        "missing-end-block"
    } else if message.contains("standard function") || message.contains("input parameter") {
        "standard-function-argument"
    } else if message.contains("read_only") || message.contains("read-only") {
        "read-only-access-write"
    } else {
        match diagnostic.code {
            DiagnosticCode::Io => "io",
            DiagnosticCode::Lexical => "lexical",
            DiagnosticCode::Syntax => "syntax",
            DiagnosticCode::Semantic => "semantic",
            DiagnosticCode::Compliance => "compliance",
            DiagnosticCode::Runtime => "runtime",
            DiagnosticCode::Unsupported => "unsupported",
        }
    };
    subcode.to_string()
}

fn related_symbol_range(
    diagnostic: &Diagnostic,
    analysis: &DocumentAnalysis,
) -> Option<SourceRange> {
    analysis
        .symbols
        .iter()
        .find(|symbol| {
            diagnostic.message.contains(&format!("'{}'", symbol.name))
                || diagnostic.message.contains(&symbol.name)
        })
        .and_then(|symbol| symbol.range.clone())
}
