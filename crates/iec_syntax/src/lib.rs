// SPDX-License-Identifier: MIT OR Apache-2.0

mod il;
mod lexer;
mod literal;
mod parser;
mod token;

#[cfg(test)]
mod tests;

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_profile::{EditionProfile, ImplementationParameters};

use lexer::Lexer;
use parser::Parser;

#[cfg(test)]
pub(crate) use literal::parse_hash_literal;

#[derive(Debug, Clone)]
pub struct ParseOutput {
    pub project: Project,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, Default)]
pub struct ParseOptions {
    pub implementation: ImplementationParameters,
}

pub fn parse_project(source_name: impl Into<String>, source: &str) -> ParseOutput {
    parse_project_with_options(source_name, source, &ParseOptions::default())
}

pub fn parse_project_with_options(
    source_name: impl Into<String>,
    source: &str,
    options: &ParseOptions,
) -> ParseOutput {
    let source_name = source_name.into();
    if source.len() > options.implementation.max_source_bytes {
        return ParseOutput {
            project: Project::new(EditionProfile::Iec61131_3_2003Strict),
            diagnostics: vec![Diagnostic::error(
                DiagnosticCode::Compliance,
                format!(
                    "source size {} bytes exceeds maximum {}",
                    source.len(),
                    options.implementation.max_source_bytes
                ),
                None,
            )],
        };
    }
    let mut lexer = Lexer::new(source_name.clone(), source, options.implementation.clone());
    let tokens = lexer.lex();
    let mut diagnostics = lexer.diagnostics.into_vec();
    let mut parser = Parser::new(source_name, source, tokens, options.implementation.clone());
    let project = parser.parse_project();
    diagnostics.extend(parser.diagnostics.into_vec());
    ParseOutput {
        project,
        diagnostics,
    }
}
