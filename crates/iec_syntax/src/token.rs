// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_diagnostics::Span;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TokenKind {
    Ident(String),
    Number(String),
    StringLiteral(String),
    WStringLiteral(String),
    HashLiteral(String),
    DirectVariable(String),
    Symbol(Symbol),
    Eof,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Symbol {
    Colon,
    Semicolon,
    Assign,
    Comma,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Dot,
    Range,
    Plus,
    Minus,
    Star,
    Slash,
    Power,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Amp,
    Arrow,
    Hash,
}

#[derive(Debug, Clone)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) lexeme: String,
    pub(crate) span: Span,
}
