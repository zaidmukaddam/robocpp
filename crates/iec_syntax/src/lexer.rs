// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticCode, Span};
use iec_ir::canonical_identifier;
use iec_profile::ImplementationParameters;

use crate::literal::control_char_label;
use crate::token::{Symbol, Token, TokenKind};

pub(crate) struct Lexer<'a> {
    source_name: String,
    source: &'a str,
    pos: usize,
    implementation: ImplementationParameters,
    pub(crate) diagnostics: DiagnosticBag,
}

impl<'a> Lexer<'a> {
    pub(crate) fn new(
        source_name: String,
        source: &'a str,
        implementation: ImplementationParameters,
    ) -> Self {
        Self {
            source_name,
            source,
            pos: 0,
            implementation,
            diagnostics: DiagnosticBag::new(),
        }
    }

    pub(crate) fn lex(&mut self) -> Vec<Token> {
        let mut tokens = Vec::new();

        while self.peek().is_some() {
            self.skip_ws_comments_and_pragmas();
            let Some(ch) = self.peek() else { break };
            let start = self.pos;

            let token = if is_ident_start(ch) {
                self.lex_identifier_or_hash_literal(start)
            } else if ch.is_ascii_digit() {
                self.lex_number_or_hash_literal(start)
            } else {
                match ch {
                    '\'' | '"' => self.lex_string(start, ch),
                    '%' => self.lex_direct_variable(start),
                    ':' => {
                        self.advance();
                        if self.match_char('=') {
                            self.token(start, TokenKind::Symbol(Symbol::Assign))
                        } else {
                            self.token(start, TokenKind::Symbol(Symbol::Colon))
                        }
                    }
                    '=' => {
                        self.advance();
                        if self.match_char('>') {
                            self.token(start, TokenKind::Symbol(Symbol::Arrow))
                        } else {
                            self.token(start, TokenKind::Symbol(Symbol::Eq))
                        }
                    }
                    ';' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::Semicolon))
                    }
                    ',' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::Comma))
                    }
                    '(' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::LParen))
                    }
                    ')' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::RParen))
                    }
                    '[' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::LBracket))
                    }
                    ']' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::RBracket))
                    }
                    '.' => {
                        self.advance();
                        if self.match_char('.') {
                            self.token(start, TokenKind::Symbol(Symbol::Range))
                        } else {
                            self.token(start, TokenKind::Symbol(Symbol::Dot))
                        }
                    }
                    '+' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::Plus))
                    }
                    '-' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::Minus))
                    }
                    '*' => {
                        self.advance();
                        if self.match_char('*') {
                            self.token(start, TokenKind::Symbol(Symbol::Power))
                        } else {
                            self.token(start, TokenKind::Symbol(Symbol::Star))
                        }
                    }
                    '/' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::Slash))
                    }
                    '<' => {
                        self.advance();
                        if self.match_char('=') {
                            self.token(start, TokenKind::Symbol(Symbol::Le))
                        } else if self.match_char('>') {
                            self.token(start, TokenKind::Symbol(Symbol::Ne))
                        } else {
                            self.token(start, TokenKind::Symbol(Symbol::Lt))
                        }
                    }
                    '>' => {
                        self.advance();
                        if self.match_char('=') {
                            self.token(start, TokenKind::Symbol(Symbol::Ge))
                        } else {
                            self.token(start, TokenKind::Symbol(Symbol::Gt))
                        }
                    }
                    '&' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::Amp))
                    }
                    '#' => {
                        self.advance();
                        self.token(start, TokenKind::Symbol(Symbol::Hash))
                    }
                    _ => {
                        self.advance();
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Lexical,
                            format!("unexpected character '{ch}'"),
                            Some(Span::new(&self.source_name, start, self.pos, self.source)),
                        ));
                        continue;
                    }
                }
            };

            tokens.push(token);
        }

        tokens.push(Token {
            kind: TokenKind::Eof,
            lexeme: String::new(),
            span: Span::new(&self.source_name, self.pos, self.pos, self.source),
        });
        tokens
    }

    fn skip_ws_comments_and_pragmas(&mut self) {
        loop {
            while self.peek().is_some_and(char::is_whitespace) {
                self.advance();
            }

            if self.starts_with("(*") {
                let start = self.pos;
                self.pos += 2;
                let mut nested = false;
                while self.peek().is_some() && !self.starts_with("*)") {
                    if self.starts_with("(*") {
                        nested = true;
                    }
                    self.advance();
                }

                if nested {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Lexical,
                        "nested comments are not allowed in IEC 61131-3:2003",
                        Some(Span::new(&self.source_name, start, self.pos, self.source)),
                    ));
                }

                if self.starts_with("*)") {
                    self.pos += 2;
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Lexical,
                        "unterminated comment",
                        Some(Span::new(&self.source_name, start, self.pos, self.source)),
                    ));
                }
                let length = self.pos.saturating_sub(start);
                if length > self.implementation.max_comment_length {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Compliance,
                        format!(
                            "comment length {length} exceeds maximum {}",
                            self.implementation.max_comment_length
                        ),
                        Some(Span::new(&self.source_name, start, self.pos, self.source)),
                    ));
                }
                continue;
            }

            if self.starts_with("{") {
                let start = self.pos;
                self.advance();
                while self.peek().is_some() && !self.starts_with("}") {
                    self.advance();
                }
                if self.starts_with("}") {
                    self.advance();
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Lexical,
                        "unterminated pragma",
                        Some(Span::new(&self.source_name, start, self.pos, self.source)),
                    ));
                }
                if !self.implementation.pragmas_enabled {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Compliance,
                        "pragmas are disabled by implementation parameters",
                        Some(Span::new(&self.source_name, start, self.pos, self.source)),
                    ));
                }
                continue;
            }

            break;
        }
    }

    fn lex_identifier_or_hash_literal(&mut self, start: usize) -> Token {
        self.advance();
        while self.peek().is_some_and(is_ident_continue) {
            self.advance();
        }

        if self.peek() == Some('#') {
            let prefix = canonical_identifier(&self.source[start..self.pos]);
            self.advance();
            if matches!(prefix.as_str(), "STRING" | "WSTRING")
                && matches!(self.peek(), Some('\'' | '"'))
            {
                self.consume_typed_string_literal_tail();
            } else {
                self.consume_hash_literal_tail(hash_literal_allows_colon(&prefix));
            }
            self.token(
                start,
                TokenKind::HashLiteral(self.source[start..self.pos].to_string()),
            )
        } else {
            self.token(
                start,
                TokenKind::Ident(self.source[start..self.pos].to_string()),
            )
        }
    }

    fn lex_number_or_hash_literal(&mut self, start: usize) -> Token {
        self.advance();
        while self
            .peek()
            .is_some_and(|ch| ch.is_ascii_digit() || ch == '_')
        {
            self.advance();
        }

        if self.peek() == Some('#') {
            self.advance();
            self.consume_hash_literal_tail(false);
            return self.token(
                start,
                TokenKind::HashLiteral(self.source[start..self.pos].to_string()),
            );
        }

        if self.peek() == Some('.') && !self.starts_with("..") {
            self.advance();
            while self
                .peek()
                .is_some_and(|ch| ch.is_ascii_digit() || ch == '_')
            {
                self.advance();
            }
        }

        if matches!(self.peek(), Some('e' | 'E')) {
            self.advance();
            if matches!(self.peek(), Some('+' | '-')) {
                self.advance();
            }
            while self
                .peek()
                .is_some_and(|ch| ch.is_ascii_digit() || ch == '_')
            {
                self.advance();
            }
        }

        self.token(
            start,
            TokenKind::Number(self.source[start..self.pos].to_string()),
        )
    }

    fn consume_hash_literal_tail(&mut self, allow_colon: bool) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace()
                || matches!(ch, ';' | ',' | ')' | ']' | '(' | '[')
                || (!allow_colon
                    && ch == ':'
                    && !self.peek_next().is_some_and(|next| next.is_ascii_digit()))
            {
                break;
            }
            self.advance();
        }
    }

    fn consume_typed_string_literal_tail(&mut self) {
        let Some(quote @ ('\'' | '"')) = self.advance() else {
            return;
        };
        while let Some(ch) = self.peek() {
            if ch == '$' {
                self.advance();
                let _ = self.advance();
                continue;
            }
            self.advance();
            if ch == quote {
                break;
            }
        }
    }

    fn lex_string(&mut self, start: usize, quote: char) -> Token {
        self.advance();
        let mut value = String::new();
        while let Some(ch) = self.peek() {
            if ch == quote {
                self.advance();
                let kind = if quote == '"' {
                    TokenKind::WStringLiteral(value)
                } else {
                    TokenKind::StringLiteral(value)
                };
                return self.token(start, kind);
            }

            if ch == '$' {
                let escape_start = self.pos;
                self.advance();
                self.lex_string_escape(escape_start, quote, &mut value);
            } else {
                if ch.is_control() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Lexical,
                        format!(
                            "unescaped control character {} in character string literal",
                            control_char_label(ch)
                        ),
                        Some(Span::new(
                            &self.source_name,
                            self.pos,
                            self.pos + ch.len_utf8(),
                            self.source,
                        )),
                    ));
                }
                if quote == '\'' && (ch as u32) > 0xFF {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Lexical,
                        format!(
                            "character {} exceeds single-byte STRING range in character string literal",
                            control_char_label(ch)
                        ),
                        Some(Span::new(
                            &self.source_name,
                            self.pos,
                            self.pos + ch.len_utf8(),
                            self.source,
                        )),
                    ));
                }
                value.push(ch);
                self.advance();
            }
        }

        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::Lexical,
            "unterminated string literal",
            Some(Span::new(&self.source_name, start, self.pos, self.source)),
        ));
        let kind = if quote == '"' {
            TokenKind::WStringLiteral(value)
        } else {
            TokenKind::StringLiteral(value)
        };
        self.token(start, kind)
    }

    fn lex_string_escape(&mut self, escape_start: usize, quote: char, value: &mut String) {
        let Some(escaped) = self.peek() else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Lexical,
                "unterminated character string escape",
                Some(Span::new(
                    &self.source_name,
                    escape_start,
                    self.pos,
                    self.source,
                )),
            ));
            return;
        };

        let decoded = match escaped {
            '$' => Some('$'),
            '\'' => Some('\''),
            '"' => Some('"'),
            'L' | 'l' | 'N' | 'n' => Some('\n'),
            'P' | 'p' => Some('\u{000C}'),
            'R' | 'r' => Some('\r'),
            'T' | 't' => Some('\t'),
            _ => None,
        };
        if let Some(decoded) = decoded {
            value.push(decoded);
            self.advance();
            return;
        }

        if escaped.is_ascii_hexdigit() {
            self.lex_hex_string_escape(escape_start, quote, value);
            return;
        }

        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::Lexical,
            format!("invalid character string escape '${escaped}'"),
            Some(Span::new(
                &self.source_name,
                escape_start,
                self.pos + escaped.len_utf8(),
                self.source,
            )),
        ));
        self.advance();
    }

    fn lex_hex_string_escape(&mut self, escape_start: usize, quote: char, value: &mut String) {
        let required_digits = if quote == '\'' { 2 } else { 4 };
        let mut digits = String::new();
        while digits.len() < required_digits {
            let Some(ch) = self.peek() else {
                break;
            };
            if !ch.is_ascii_hexdigit() {
                break;
            }
            digits.push(ch);
            self.advance();
        }

        if digits.len() != required_digits {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Lexical,
                format!(
                    "invalid character string hex escape '${digits}': expected {required_digits} hexadecimal digit(s)"
                ),
                Some(Span::new(
                    &self.source_name,
                    escape_start,
                    self.pos,
                    self.source,
                )),
            ));
            return;
        }

        let code = u32::from_str_radix(&digits, 16).unwrap_or(0);
        if let Some(ch) = char::from_u32(code) {
            value.push(ch);
        } else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Lexical,
                format!("invalid character code '${digits}' in character string literal"),
                Some(Span::new(
                    &self.source_name,
                    escape_start,
                    self.pos,
                    self.source,
                )),
            ));
        }
    }

    fn lex_direct_variable(&mut self, start: usize) -> Token {
        self.advance();
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() || matches!(ch, ';' | ',' | ')' | '(' | '[' | ']') {
                break;
            }
            self.advance();
        }
        self.token(
            start,
            TokenKind::DirectVariable(self.source[start..self.pos].to_string()),
        )
    }

    fn token(&self, start: usize, kind: TokenKind) -> Token {
        Token {
            kind,
            lexeme: self.source[start..self.pos].to_string(),
            span: Span::new(&self.source_name, start, self.pos, self.source),
        }
    }

    fn starts_with(&self, value: &str) -> bool {
        self.source[self.pos..].starts_with(value)
    }

    fn peek(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn peek_next(&self) -> Option<char> {
        self.source[self.pos..].chars().nth(1)
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.peek()?;
        self.pos += ch.len_utf8();
        Some(ch)
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn hash_literal_allows_colon(prefix: &str) -> bool {
    matches!(prefix, "TOD" | "TIME_OF_DAY" | "DT" | "DATE_AND_TIME")
}
