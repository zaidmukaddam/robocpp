// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_diagnostics::json_escape;
use iec_ir::canonical_identifier;

use crate::symbols::TextEdit;
use crate::{range_from_offsets, DocumentInput};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormattedDocument {
    pub uri: String,
    pub text: String,
    pub edits: Vec<TextEdit>,
}

impl FormattedDocument {
    pub fn to_json(&self) -> String {
        let edits = self
            .edits
            .iter()
            .map(TextEdit::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"uri\":\"{}\",\"text\":\"{}\",\"edits\":[{}]}}",
            json_escape(&self.uri),
            json_escape(&self.text),
            edits
        )
    }
}

pub fn format_document(input: DocumentInput) -> FormattedDocument {
    if input.uri.ends_with(".xml") || input.language_id.as_deref() == Some("xml") {
        return FormattedDocument {
            uri: input.uri,
            text: input.text,
            edits: Vec::new(),
        };
    }

    let formatted = format_textual_iec(&input.text);
    let edits = if formatted == input.text {
        Vec::new()
    } else {
        vec![TextEdit {
            range: range_from_offsets(&input.uri, &input.text, 0, input.text.len()),
            new_text: formatted.clone(),
        }]
    };
    FormattedDocument {
        uri: input.uri,
        text: formatted,
        edits,
    }
}

fn format_textual_iec(text: &str) -> String {
    let mut out = String::new();
    let mut indent = 0usize;
    let mut in_multiline_comment = false;

    for original_line in text.lines() {
        let trimmed_end = original_line.trim_end();
        let trimmed = trimmed_end.trim_start();
        if trimmed.is_empty() {
            out.push('\n');
            continue;
        }

        if in_multiline_comment {
            push_indented(&mut out, indent, trimmed);
            if trimmed.contains("*)") {
                in_multiline_comment = false;
            }
            continue;
        }
        if trimmed.starts_with("(*") {
            push_indented(&mut out, indent, trimmed);
            if !trimmed.contains("*)") {
                in_multiline_comment = true;
            }
            continue;
        }
        if trimmed.starts_with("//") {
            push_indented(&mut out, indent, trimmed);
            continue;
        }

        let canonical = canonical_identifier(trimmed);
        if decreases_indent(&canonical) {
            indent = indent.saturating_sub(1);
        }
        push_indented(&mut out, indent, normalize_spacing(trimmed).as_str());
        if increases_indent(&canonical) {
            indent += 1;
        }
        if middle_block_keyword(&canonical) {
            indent += 1;
        }
    }

    if text.ends_with('\n') || !out.is_empty() {
        out
    } else {
        out.trim_end_matches('\n').to_string()
    }
}

fn push_indented(out: &mut String, indent: usize, line: &str) {
    out.push_str(&"    ".repeat(indent));
    out.push_str(line);
    out.push('\n');
}

fn normalize_spacing(line: &str) -> String {
    let mut text = line.split_whitespace().collect::<Vec<_>>().join(" ");
    for (from, to) in [
        (" :=", " :="),
        (":=", " := "),
        (" ;", ";"),
        (" ,", ","),
        ("( ", "("),
        (" )", ")"),
        ("[ ", "["),
        (" ]", "]"),
    ] {
        text = text.replace(from, to);
    }
    while text.contains("  ") {
        text = text.replace("  ", " ");
    }
    text
}

fn decreases_indent(canonical: &str) -> bool {
    canonical.starts_with("END_")
        || matches!(
            canonical,
            "END_VAR"
                | "END_TYPE"
                | "END_IF"
                | "END_CASE"
                | "END_FOR"
                | "END_WHILE"
                | "END_REPEAT"
                | "END_LADDER"
                | "END_RUNG"
                | "END_FBD"
                | "END_NETWORK"
                | "END_STEP"
                | "END_TRANSITION"
                | "END_ACTION"
                | "ELSE"
        )
        || canonical.starts_with("ELSIF ")
        || canonical.starts_with("UNTIL ")
}

fn increases_indent(canonical: &str) -> bool {
    canonical.starts_with("PROGRAM ")
        || canonical.starts_with("FUNCTION ")
        || canonical.starts_with("FUNCTION_BLOCK ")
        || canonical.starts_with("CONFIGURATION ")
        || canonical.starts_with("RESOURCE ")
        || canonical == "VAR"
        || canonical.starts_with("VAR_")
        || canonical.starts_with("TYPE")
        || canonical.starts_with("IF ")
        || canonical.starts_with("CASE ")
        || canonical.starts_with("FOR ")
        || canonical.starts_with("WHILE ")
        || canonical.starts_with("REPEAT")
        || canonical.starts_with("LADDER")
        || canonical.starts_with("RUNG")
        || canonical.starts_with("FBD")
        || canonical.starts_with("NETWORK")
        || canonical.starts_with("INITIAL_STEP")
        || canonical.starts_with("STEP ")
        || canonical.starts_with("TRANSITION")
        || canonical.starts_with("ACTION ")
}

fn middle_block_keyword(canonical: &str) -> bool {
    canonical == "ELSE" || canonical.starts_with("ELSIF ") || canonical.starts_with("UNTIL ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn leaves_xml_unformatted() {
        let input = DocumentInput::new("project.xml", "<project />").with_language_id("xml");
        assert_eq!(format_document(input).text, "<project />");
    }
}
