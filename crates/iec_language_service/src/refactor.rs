// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_diagnostics::json_escape;
use iec_ir::canonical_identifier;

use crate::symbols::{document_symbol_index, TextEdit};
use crate::{range_from_offsets, DocumentAnalysis};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefactorPlan {
    pub title: String,
    pub edits: Vec<TextEdit>,
    pub warnings: Vec<String>,
}

impl RefactorPlan {
    pub fn to_json(&self) -> String {
        let edits = self
            .edits
            .iter()
            .map(TextEdit::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let warnings = self
            .warnings
            .iter()
            .map(|warning| format!("\"{}\"", json_escape(warning)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"title\":\"{}\",\"edits\":[{}],\"warnings\":[{}]}}",
            json_escape(&self.title),
            edits,
            warnings
        )
    }
}

pub fn rename_symbol_plan(
    analysis: &DocumentAnalysis,
    offset: usize,
    new_name: &str,
) -> RefactorPlan {
    let validation =
        document_symbol_index(analysis).validate_rename(&analysis.uri, offset, new_name);
    RefactorPlan {
        title: format!("Rename symbol to {new_name}"),
        edits: validation.edits,
        warnings: (!validation.valid)
            .then_some(validation.message)
            .into_iter()
            .collect(),
    }
}

pub fn change_variable_type_plan(
    analysis: &DocumentAnalysis,
    variable_name: &str,
    new_type: &str,
) -> RefactorPlan {
    let canonical = canonical_identifier(variable_name);
    let mut edits = Vec::new();
    for symbol in &analysis.symbols {
        if canonical_identifier(&symbol.name) != canonical {
            continue;
        }
        let Some(range) = &symbol.range else {
            continue;
        };
        let line_start = analysis.source.text[..range.start]
            .rfind('\n')
            .map(|offset| offset + 1)
            .unwrap_or(0);
        let line_end = analysis.source.text[range.end..]
            .find('\n')
            .map(|offset| range.end + offset)
            .unwrap_or(analysis.source.text.len());
        let line = &analysis.source.text[line_start..line_end];
        if let Some(colon) = line.find(':') {
            let type_start = line_start + colon + 1;
            let type_end = line_start
                + line[colon + 1..]
                    .find([';', ':'].as_slice())
                    .map(|offset| colon + 1 + offset)
                    .unwrap_or(line.len());
            edits.push(TextEdit {
                range: range_from_offsets(
                    &analysis.uri,
                    &analysis.source.text,
                    type_start,
                    type_end,
                ),
                new_text: format!(" {new_type}"),
            });
        }
    }
    RefactorPlan {
        title: format!("Change {variable_name} type to {new_type}"),
        warnings: if edits.is_empty() {
            vec![format!("variable '{variable_name}' was not found")]
        } else {
            Vec::new()
        },
        edits,
    }
}

pub fn introduce_variable_plan(
    analysis: &DocumentAnalysis,
    expression: &str,
    variable_name: &str,
    type_name: &str,
) -> RefactorPlan {
    let mut edits = Vec::new();
    if let Some(insert_offset) = var_insert_offset(&analysis.source.text) {
        edits.push(TextEdit {
            range: range_from_offsets(
                &analysis.uri,
                &analysis.source.text,
                insert_offset,
                insert_offset,
            ),
            new_text: format!("    {variable_name} : {type_name};\n"),
        });
    }
    let mut offset = 0;
    while let Some(relative) = analysis.source.text[offset..].find(expression) {
        let start = offset + relative;
        let end = start + expression.len();
        edits.push(TextEdit {
            range: range_from_offsets(&analysis.uri, &analysis.source.text, start, end),
            new_text: variable_name.to_string(),
        });
        offset = end;
    }
    RefactorPlan {
        title: format!("Introduce variable {variable_name}"),
        warnings: if edits.len() <= 1 {
            vec![format!(
                "expression '{expression}' was not found repeatedly"
            )]
        } else {
            Vec::new()
        },
        edits,
    }
}

pub fn extract_pou_plan(
    analysis: &DocumentAnalysis,
    start: usize,
    end: usize,
    new_pou_name: &str,
) -> RefactorPlan {
    let selection = analysis.source.text
        [start.min(analysis.source.text.len())..end.min(analysis.source.text.len())]
        .trim();
    let new_pou = format!("\nPROGRAM {new_pou_name}\n{selection}\nEND_PROGRAM\n");
    let call_text = format!("{new_pou_name}();");
    RefactorPlan {
        title: format!("Extract POU {new_pou_name}"),
        edits: vec![
            TextEdit {
                range: range_from_offsets(&analysis.uri, &analysis.source.text, start, end),
                new_text: call_text,
            },
            TextEdit {
                range: range_from_offsets(
                    &analysis.uri,
                    &analysis.source.text,
                    analysis.source.text.len(),
                    analysis.source.text.len(),
                ),
                new_text: new_pou,
            },
        ],
        warnings: Vec::new(),
    }
}

fn var_insert_offset(text: &str) -> Option<usize> {
    text.find("END_VAR").map(|end_var| {
        text[..end_var]
            .rfind('\n')
            .map(|offset| offset + 1)
            .unwrap_or(end_var)
    })
}
