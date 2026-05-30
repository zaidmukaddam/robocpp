// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_diagnostics::json_escape;

use crate::diagnostics_ext::diagnostic_descriptor;
use crate::symbols::TextEdit;
use crate::{range_from_offsets, DocumentAnalysis, SourceRange};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodeAction {
    pub title: String,
    pub kind: CodeActionKind,
    pub diagnostic_subcode: Option<String>,
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeActionKind {
    QuickFix,
    Refactor,
    Source,
}

impl CodeAction {
    pub fn to_json(&self) -> String {
        let subcode = self
            .diagnostic_subcode
            .as_ref()
            .map(|subcode| format!("\"{}\"", json_escape(subcode)))
            .unwrap_or_else(|| "null".to_string());
        let edits = self
            .edits
            .iter()
            .map(TextEdit::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"title\":\"{}\",\"kind\":\"{}\",\"diagnosticSubcode\":{},\"edits\":[{}]}}",
            json_escape(&self.title),
            self.kind.as_str(),
            subcode,
            edits
        )
    }
}

impl CodeActionKind {
    pub fn as_str(self) -> &'static str {
        match self {
            CodeActionKind::QuickFix => "quickfix",
            CodeActionKind::Refactor => "refactor",
            CodeActionKind::Source => "source",
        }
    }
}

pub fn code_actions(analysis: &DocumentAnalysis) -> Vec<CodeAction> {
    let mut actions = Vec::new();
    for diagnostic in &analysis.diagnostics {
        let descriptor = diagnostic_descriptor(diagnostic, analysis);
        let subcode = descriptor.subcode.clone();
        match subcode.as_str() {
            "unknown-symbol" => {
                if let Some(name) = quoted_name(&diagnostic.message) {
                    actions.push(CodeAction {
                        title: format!("Declare local variable '{name}'"),
                        kind: CodeActionKind::QuickFix,
                        diagnostic_subcode: Some(subcode.clone()),
                        edits: insertion_for_local_variable(analysis, &name),
                    });
                }
            }
            "duplicate-declaration" => actions.push(CodeAction {
                title: "Review duplicate declaration".to_string(),
                kind: CodeActionKind::QuickFix,
                diagnostic_subcode: Some(subcode.clone()),
                edits: Vec::new(),
            }),
            "type-mismatch" => {
                if let Some(name) = quoted_name(&diagnostic.message) {
                    actions.push(CodeAction {
                        title: format!("Inspect inferred type for '{name}'"),
                        kind: CodeActionKind::QuickFix,
                        diagnostic_subcode: Some(subcode.clone()),
                        edits: Vec::new(),
                    });
                }
            }
            "invalid-direct-location" => actions.push(CodeAction {
                title: "Normalize direct-variable address form".to_string(),
                kind: CodeActionKind::QuickFix,
                diagnostic_subcode: Some(subcode.clone()),
                edits: diagnostic
                    .span
                    .as_ref()
                    .map(|span| TextEdit {
                        range: SourceRange {
                            uri: span.source.clone(),
                            start: span.start,
                            end: span.end,
                            start_position: crate::position_at(&analysis.source.text, span.start),
                            end_position: crate::position_at(&analysis.source.text, span.end),
                        },
                        new_text: "%M0".to_string(),
                    })
                    .into_iter()
                    .collect(),
            }),
            "missing-end-block" => actions.push(CodeAction {
                title: "Insert missing END block marker".to_string(),
                kind: CodeActionKind::QuickFix,
                diagnostic_subcode: Some(subcode.clone()),
                edits: vec![TextEdit {
                    range: end_insertion_range(analysis),
                    new_text: "\nEND_PROGRAM\n".to_string(),
                }],
            }),
            "standard-function-argument" => actions.push(CodeAction {
                title: "Use formal standard-function argument names".to_string(),
                kind: CodeActionKind::QuickFix,
                diagnostic_subcode: Some(subcode.clone()),
                edits: Vec::new(),
            }),
            "read-only-access-write" => actions.push(CodeAction {
                title: "Change access path to READ_WRITE".to_string(),
                kind: CodeActionKind::QuickFix,
                diagnostic_subcode: Some(subcode.clone()),
                edits: replace_read_only_edits(analysis),
            }),
            _ => {}
        }
    }
    actions.push(CodeAction {
        title: "Format document".to_string(),
        kind: CodeActionKind::Source,
        diagnostic_subcode: None,
        edits: Vec::new(),
    });
    actions
}

fn insertion_for_local_variable(analysis: &DocumentAnalysis, name: &str) -> Vec<TextEdit> {
    let Some(insert_offset) = local_var_insert_offset(&analysis.source.text) else {
        return Vec::new();
    };
    vec![TextEdit {
        range: range_from_offsets(
            &analysis.uri,
            &analysis.source.text,
            insert_offset,
            insert_offset,
        ),
        new_text: format!("    {name} : BOOL;\n"),
    }]
}

fn local_var_insert_offset(text: &str) -> Option<usize> {
    if let Some(end_var) = text.find("END_VAR") {
        return Some(
            text[..end_var]
                .rfind('\n')
                .map(|offset| offset + 1)
                .unwrap_or(end_var),
        );
    }
    let program_line_end = text.find('\n')?;
    Some(program_line_end + 1)
}

fn end_insertion_range(analysis: &DocumentAnalysis) -> SourceRange {
    range_from_offsets(
        &analysis.uri,
        &analysis.source.text,
        analysis.source.text.len(),
        analysis.source.text.len(),
    )
}

fn replace_read_only_edits(analysis: &DocumentAnalysis) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let mut offset = 0;
    while let Some(relative) = analysis.source.text[offset..].find("READ_ONLY") {
        let start = offset + relative;
        let end = start + "READ_ONLY".len();
        edits.push(TextEdit {
            range: range_from_offsets(&analysis.uri, &analysis.source.text, start, end),
            new_text: "READ_WRITE".to_string(),
        });
        offset = end;
    }
    edits
}

fn quoted_name(message: &str) -> Option<String> {
    let start = message.find('\'')? + 1;
    let end = message[start..].find('\'')?;
    Some(message[start..start + end].to_string())
}
