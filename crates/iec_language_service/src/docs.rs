// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_ir::canonical_identifier;
use iec_stdlib::standard_symbols;

use crate::source::SourceTokenKind;
use crate::{DocumentAnalysis, Hover};

pub fn hover_for_offset(analysis: &DocumentAnalysis, offset: usize) -> Option<Hover> {
    if let Some(symbol_hover) = analysis.symbol_hover_at(offset) {
        return Some(symbol_hover);
    }
    let token = analysis.source.identifier_at(offset)?;
    if matches!(token.kind, SourceTokenKind::Keyword) {
        return keyword_documentation(&token.lexeme).map(|contents| Hover {
            contents,
            range: Some(token.range.clone()),
        });
    }
    let canonical = canonical_identifier(&token.lexeme);
    if let Some(symbol) = standard_symbols()
        .iter()
        .find(|symbol| canonical_identifier(symbol.name) == canonical)
    {
        return Some(Hover {
            contents: standard_symbol_documentation(symbol.name),
            range: Some(token.range.clone()),
        });
    }
    elementary_type_documentation(&token.lexeme).map(|contents| Hover {
        contents,
        range: Some(token.range.clone()),
    })
}

pub fn keyword_documentation(keyword: &str) -> Option<String> {
    let canonical = canonical_identifier(keyword);
    let text = match canonical.as_str() {
        "PROGRAM" => "Declares an executable program organization unit.",
        "FUNCTION" => "Declares a stateless POU with a return variable.",
        "FUNCTION_BLOCK" => "Declares a stateful function block type.",
        "VAR" => "Starts a local variable declaration block.",
        "VAR_INPUT" => "Starts input interface declarations for a POU.",
        "VAR_OUTPUT" => "Starts output interface declarations for a POU.",
        "VAR_IN_OUT" => "Starts by-reference input/output interface declarations.",
        "VAR_GLOBAL" => "Starts global variable declarations.",
        "VAR_ACCESS" => "Starts named access-path declarations.",
        "IF" => "Starts a conditional Structured Text statement.",
        "CASE" => "Starts a multi-branch Structured Text statement.",
        "FOR" => "Starts a counted iteration statement.",
        "WHILE" => "Starts a pre-test loop.",
        "REPEAT" => "Starts a post-test loop.",
        "INITIAL_STEP" => "Declares the initial step of a textual SFC body.",
        "STEP" => "Declares an SFC step.",
        "TRANSITION" => "Declares an SFC transition.",
        "ACTION" => "Declares an SFC action body.",
        "READ_ONLY" => "Declares an access path that may be read but not written.",
        "READ_WRITE" => "Declares an access path that may be read and written.",
        _ => return None,
    };
    Some(format!("IEC keyword `{}`\n\n{text}", canonical))
}

pub fn standard_symbol_documentation(name: &str) -> String {
    let canonical = canonical_identifier(name);
    let description = match canonical.as_str() {
        "ABS" => "Returns the absolute value of a numeric input.",
        "SQRT" => "Returns the square root of a numeric input.",
        "ADD" => "Adds two or more numeric inputs.",
        "SUB" => "Subtracts one numeric input from another.",
        "MUL" => "Multiplies numeric inputs.",
        "DIV" => "Divides one numeric input by another.",
        "MOD" => "Returns an integer remainder.",
        "MOVE" => "Returns the input value using IEC overloaded typing.",
        "LIMIT" => "Clamps an input between minimum and maximum bounds.",
        "SEL" => "Selects between two values from a boolean selector.",
        "TON" => "On-delay timer function block.",
        "TOF" => "Off-delay timer function block.",
        "TP" => "Pulse timer function block.",
        "CTU" => "Count-up counter function block.",
        "CTD" => "Count-down counter function block.",
        "CTUD" => "Count-up/count-down counter function block.",
        "R_TRIG" => "Rising-edge detector function block.",
        "F_TRIG" => "Falling-edge detector function block.",
        _ => "IEC standard library symbol.",
    };
    let clause = standard_symbols()
        .iter()
        .find(|symbol| canonical_identifier(symbol.name) == canonical)
        .map(|symbol| symbol.clause)
        .unwrap_or("unknown");
    format!(
        "IEC standard `{}`\n\n{description}\n\nClause {clause}",
        canonical
    )
}

pub fn elementary_type_documentation(name: &str) -> Option<String> {
    let canonical = canonical_identifier(name);
    let description = match canonical.as_str() {
        "BOOL" => "Boolean value.",
        "SINT" | "INT" | "DINT" | "LINT" => "Signed integer value.",
        "USINT" | "UINT" | "UDINT" | "ULINT" => "Unsigned integer value.",
        "REAL" | "LREAL" => "Floating-point value.",
        "BYTE" | "WORD" | "DWORD" | "LWORD" => "Bit-string value.",
        "STRING" => "Single-byte character string.",
        "WSTRING" => "Wide character string.",
        "TIME" => "Duration value.",
        "DATE" => "Date value.",
        "TIME_OF_DAY" => "Time-of-day value.",
        "DATE_AND_TIME" => "Date and time value.",
        _ => return None,
    };
    Some(format!("IEC type `{}`\n\n{description}", canonical))
}

pub fn compliance_profile_note() -> &'static str {
    "The default language-service profile follows the implemented IEC 61131-3:2003 subset and reports compliance diagnostics when implementation limits are exceeded."
}
