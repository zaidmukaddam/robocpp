// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use iec_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticCode, Span};
use iec_ir::*;
use iec_profile::{EditionProfile, ImplementationParameters};

#[derive(Debug, Clone)]
pub struct ParseOutput {
    pub project: Project,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct ParseOptions {
    pub implementation: ImplementationParameters,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            implementation: ImplementationParameters::default(),
        }
    }
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
    let mut lexer = Lexer::new(source_name.clone(), source, options.implementation.clone());
    let tokens = lexer.lex();
    let mut diagnostics = lexer.diagnostics.into_vec();
    let mut parser = Parser::new(source_name, source, tokens);
    let project = parser.parse_project();
    diagnostics.extend(parser.diagnostics.into_vec());
    ParseOutput {
        project,
        diagnostics,
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TokenKind {
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
enum Symbol {
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
struct Token {
    kind: TokenKind,
    lexeme: String,
    span: Span,
}

struct Lexer<'a> {
    source_name: String,
    source: &'a str,
    pos: usize,
    implementation: ImplementationParameters,
    diagnostics: DiagnosticBag,
}

impl<'a> Lexer<'a> {
    fn new(source_name: String, source: &'a str, implementation: ImplementationParameters) -> Self {
        Self {
            source_name,
            source,
            pos: 0,
            implementation,
            diagnostics: DiagnosticBag::new(),
        }
    }

    fn lex(&mut self) -> Vec<Token> {
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

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    source: &'a str,
    expression_stop: Option<usize>,
    diagnostics: DiagnosticBag,
}

impl<'a> Parser<'a> {
    fn new(_source_name: String, source: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            source,
            expression_stop: None,
            diagnostics: DiagnosticBag::new(),
        }
    }

    fn parse_project(&mut self) -> Project {
        let mut project = Project::new(EditionProfile::Iec61131_3_2003Strict);

        while !self.is_eof() {
            if self.match_keyword("TYPE") {
                project.library_elements.extend(
                    self.parse_type_section()
                        .into_iter()
                        .map(LibraryElement::DataType),
                );
            } else if self.match_keyword("FUNCTION") {
                project
                    .library_elements
                    .push(LibraryElement::Pou(self.parse_pou(PouStart::Function)));
            } else if self.match_keyword("FUNCTION_BLOCK") {
                project
                    .library_elements
                    .push(LibraryElement::Pou(self.parse_pou(PouStart::FunctionBlock)));
            } else if self.match_keyword("PROGRAM") {
                project
                    .library_elements
                    .push(LibraryElement::Pou(self.parse_pou(PouStart::Program)));
            } else if self.match_keyword("CONFIGURATION") {
                project
                    .library_elements
                    .push(LibraryElement::Configuration(self.parse_configuration()));
            } else if self.check_symbol(Symbol::Semicolon) {
                self.advance();
            } else {
                let token = self.current().clone();
                self.error_at(
                    &token,
                    format!("expected TYPE, FUNCTION, FUNCTION_BLOCK, PROGRAM, or CONFIGURATION; found '{}'", token.lexeme),
                );
                self.advance();
            }
        }

        project
    }

    fn parse_type_section(&mut self) -> Vec<DataTypeDeclaration> {
        let mut declarations = Vec::new();
        while !self.is_eof() && !self.match_keyword("END_TYPE") {
            let Some(name) = self.expect_identifier("expected type name") else {
                self.synchronize_to_semicolon();
                continue;
            };
            self.expect_symbol(Symbol::Colon, "expected ':' after type name");
            let spec = self.parse_type_spec();
            if self.match_symbol(Symbol::Assign) {
                let _ = self.parse_expression();
            }
            self.expect_symbol(Symbol::Semicolon, "expected ';' after type declaration");
            declarations.push(DataTypeDeclaration { name, spec });
        }
        declarations
    }

    fn parse_type_spec(&mut self) -> DataTypeSpec {
        if self.match_keyword("ARRAY") {
            self.expect_symbol(Symbol::LBracket, "expected '[' after ARRAY");
            let mut ranges = Vec::new();
            loop {
                let low = self.expect_signed_integer("expected array lower bound");
                self.expect_symbol(Symbol::Range, "expected '..' in array range");
                let high = self.expect_signed_integer("expected array upper bound");
                ranges.push(Subrange { low, high });
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::RBracket, "expected ']' after array ranges");
            self.expect_keyword("OF", "expected OF after ARRAY range");
            let element_type = Box::new(self.parse_type_spec());
            return DataTypeSpec::Array {
                ranges,
                element_type,
            };
        }

        if self.match_keyword("STRUCT") {
            let mut fields = Vec::new();
            while !self.is_eof() && !self.match_keyword("END_STRUCT") {
                let Some(name) = self.expect_identifier("expected structure field name") else {
                    self.synchronize_to_semicolon();
                    continue;
                };
                self.expect_symbol(Symbol::Colon, "expected ':' after field name");
                let spec = self.parse_type_spec();
                let initial_value = if self.match_symbol(Symbol::Assign) {
                    Some(self.parse_expression())
                } else {
                    None
                };
                self.expect_symbol(Symbol::Semicolon, "expected ';' after structure field");
                fields.push(StructField {
                    name,
                    spec,
                    initial_value,
                });
            }
            return DataTypeSpec::Struct { fields };
        }

        if self.match_symbol(Symbol::LParen) {
            let mut values = Vec::new();
            loop {
                if let Some(value) = self.expect_identifier("expected enumerated value") {
                    values.push(value);
                }
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::RParen, "expected ')' after enumerated values");
            return DataTypeSpec::Enum { values };
        }

        let Some(name) = self.expect_identifier("expected type specification") else {
            return DataTypeSpec::Named(Identifier::new("<error>"));
        };

        if self.match_symbol(Symbol::LParen) {
            if let Some(base) = ElementaryType::parse(&name.original) {
                let low = self.expect_signed_integer("expected subrange lower bound");
                self.expect_symbol(Symbol::Range, "expected '..' in subrange");
                let high = self.expect_signed_integer("expected subrange upper bound");
                self.expect_symbol(Symbol::RParen, "expected ')' after subrange");
                return DataTypeSpec::Subrange {
                    base,
                    range: Subrange { low, high },
                };
            }
        }

        if matches!(
            canonical_identifier(&name.original).as_str(),
            "STRING" | "WSTRING"
        ) {
            let wide = canonical_identifier(&name.original) == "WSTRING";
            let length = if self.match_symbol(Symbol::LBracket) {
                let value = self.expect_unsigned_integer("expected string length");
                self.expect_symbol(Symbol::RBracket, "expected ']' after string length");
                Some(value)
            } else {
                None
            };
            return DataTypeSpec::String { wide, length };
        }

        if let Some(elementary) = ElementaryType::parse(&name.original) {
            DataTypeSpec::Elementary(elementary)
        } else {
            DataTypeSpec::Named(name)
        }
    }

    fn parse_pou(&mut self, start: PouStart) -> Pou {
        let name = self
            .expect_identifier("expected POU name")
            .unwrap_or_else(|| Identifier::new("<error>"));

        let kind = match start {
            PouStart::Function => {
                self.expect_symbol(Symbol::Colon, "expected ':' after function name");
                let return_type = self.parse_type_spec();
                PouKind::Function { return_type }
            }
            PouStart::FunctionBlock => PouKind::FunctionBlock,
            PouStart::Program => PouKind::Program,
        };

        let mut var_blocks = Vec::new();
        while self.is_var_block_start() {
            var_blocks.push(self.parse_var_block());
        }

        let end_keyword = match start {
            PouStart::Function => "END_FUNCTION",
            PouStart::FunctionBlock => "END_FUNCTION_BLOCK",
            PouStart::Program => "END_PROGRAM",
        };
        let body = if self.match_keyword("LADDER") {
            let body = self.parse_textual_ladder_body("END_LADDER");
            self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
            body
        } else if self.match_keyword("FBD") {
            let body = self.parse_textual_fbd_body("END_FBD");
            self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
            body
        } else if self.is_sfc_statement_start() {
            let sfc = self.parse_sfc_body(end_keyword);
            self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
            PouBody {
                language: ImplementationLanguage::SequentialFunctionChart,
                statements: Vec::new(),
                networks: Vec::new(),
                sfc: Some(sfc),
            }
        } else {
            let statements = self.parse_statement_list(&[end_keyword]);
            self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
            PouBody::structured_text(statements)
        };

        Pou {
            name,
            kind,
            var_blocks,
            body,
        }
    }

    fn parse_textual_ladder_body(&mut self, end_keyword: &str) -> PouBody {
        let mut statements = Vec::new();
        let mut networks = Vec::new();

        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            let label = if self.match_keyword("RUNG") || self.match_keyword("NETWORK") {
                self.parse_optional_network_label()
            } else {
                None
            };
            let rung = self.parse_textual_ladder_rung(&["END_RUNG", "END_NETWORK", end_keyword]);
            statements.extend(rung.statements);
            networks.push(Network {
                label: label.or(rung.label),
                language: ImplementationLanguage::LadderDiagram,
                nodes: rung.nodes,
            });

            if self.match_keyword("END_RUNG") || self.match_keyword("END_NETWORK") {
                self.match_symbol(Symbol::Semicolon);
            }
        }

        self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
        self.match_symbol(Symbol::Semicolon);
        PouBody {
            language: ImplementationLanguage::LadderDiagram,
            statements,
            networks,
            sfc: None,
        }
    }

    fn parse_textual_fbd_body(&mut self, end_keyword: &str) -> PouBody {
        let mut statements = Vec::new();
        let mut networks = Vec::new();

        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            let label = if self.match_keyword("NETWORK") {
                self.parse_optional_network_label()
            } else {
                None
            };
            let network = self.parse_textual_fbd_network(&["END_NETWORK", end_keyword]);
            statements.extend(network.statements);
            networks.push(Network {
                label: label.or(network.label),
                language: ImplementationLanguage::FunctionBlockDiagram,
                nodes: network.nodes,
            });

            if self.match_keyword("END_NETWORK") {
                self.match_symbol(Symbol::Semicolon);
            }
        }

        self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
        self.match_symbol(Symbol::Semicolon);
        PouBody {
            language: ImplementationLanguage::FunctionBlockDiagram,
            statements,
            networks,
            sfc: None,
        }
    }

    fn parse_sfc_body(&mut self, end_keyword: &str) -> Sfc {
        let mut sfc = Sfc {
            steps: Vec::new(),
            transitions: Vec::new(),
            actions: Vec::new(),
        };

        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_keyword("INITIAL_STEP") {
                sfc.steps.push(self.parse_sfc_step(true, None));
            } else if self.match_keyword("STEP") {
                sfc.steps.push(self.parse_sfc_step(false, None));
            } else if self.is_labeled_sfc_step() {
                let name = self.current_identifier();
                self.advance();
                self.expect_symbol(Symbol::Colon, "expected ':' after SFC step label");
                let initial = if self.match_keyword("INITIAL_STEP") {
                    true
                } else {
                    self.expect_keyword("STEP", "expected STEP or INITIAL_STEP after SFC label");
                    false
                };
                sfc.steps.push(self.parse_sfc_step(initial, name));
            } else if self.match_keyword("TRANSITION") {
                sfc.transitions.push(self.parse_sfc_transition(None));
            } else if self.is_labeled_sfc_transition() {
                let name = self.current_identifier();
                self.advance();
                self.expect_symbol(Symbol::Colon, "expected ':' after SFC transition label");
                self.expect_keyword("TRANSITION", "expected TRANSITION after SFC label");
                sfc.transitions.push(self.parse_sfc_transition(name));
            } else if self.match_keyword("ACTION") {
                sfc.actions.push(self.parse_sfc_action(None));
            } else if self.is_labeled_sfc_action() {
                let name = self.current_identifier();
                self.advance();
                self.expect_symbol(Symbol::Colon, "expected ':' after SFC action label");
                self.expect_keyword("ACTION", "expected ACTION after SFC label");
                sfc.actions.push(self.parse_sfc_action(name));
            } else {
                let token = self.current().clone();
                self.error_at(
                    &token,
                    format!("unsupported or invalid SFC element '{}'", token.lexeme),
                );
                self.synchronize_to_semicolon();
                self.match_symbol(Symbol::Semicolon);
            }
        }

        sfc
    }

    fn parse_sfc_action(&mut self, labeled_name: Option<Identifier>) -> SfcAction {
        let is_labeled_form = labeled_name.is_some();
        let name = labeled_name.unwrap_or_else(|| {
            self.expect_identifier("expected action name")
                .unwrap_or_else(|| Identifier::new("<error>"))
        });
        let (qualifier, duration) = self.parse_sfc_action_qualifier();
        if is_labeled_form {
            self.match_symbol(Symbol::Colon);
        } else {
            self.expect_symbol(Symbol::Colon, "expected ':' after action name");
        }
        let body = self.parse_statement_list(&["END_ACTION"]);
        self.expect_keyword("END_ACTION", "expected END_ACTION");
        self.match_symbol(Symbol::Semicolon);
        SfcAction {
            name,
            qualifier,
            duration,
            body,
        }
    }

    fn parse_sfc_step(&mut self, initial: bool, labeled_name: Option<Identifier>) -> SfcStep {
        let is_labeled_form = labeled_name.is_some();
        let name = labeled_name.unwrap_or_else(|| {
            self.expect_identifier(if initial {
                "expected initial step name"
            } else {
                "expected step name"
            })
            .unwrap_or_else(|| Identifier::new("<error>"))
        });
        let actions = if self.match_symbol(Symbol::Colon) {
            let actions = self.parse_sfc_action_associations();
            self.expect_keyword("END_STEP", "expected END_STEP after SFC step actions");
            self.match_symbol(Symbol::Semicolon);
            actions
        } else if is_labeled_form && !self.check_symbol(Symbol::Semicolon) {
            let actions = self.parse_sfc_action_associations();
            self.expect_keyword("END_STEP", "expected END_STEP after SFC step actions");
            self.match_symbol(Symbol::Semicolon);
            actions
        } else {
            self.expect_symbol(
                Symbol::Semicolon,
                if initial {
                    "expected ';' after initial step"
                } else {
                    "expected ';' after step"
                },
            );
            Vec::new()
        };
        SfcStep {
            name,
            initial,
            kind: SfcStepKind::Step,
            actions,
        }
    }

    fn parse_sfc_action_associations(&mut self) -> Vec<SfcActionAssociation> {
        let mut actions = Vec::new();
        while !self.is_eof() && !self.check_keyword("END_STEP") {
            let name = self
                .expect_identifier("expected SFC action association name")
                .unwrap_or_else(|| Identifier::new("<error>"));
            let (qualifier, duration) = self.parse_sfc_action_qualifier();
            self.expect_symbol(
                Symbol::Semicolon,
                "expected ';' after SFC action association",
            );
            actions.push(SfcActionAssociation {
                name,
                qualifier: Some(qualifier),
                duration,
            });
        }
        actions
    }

    fn parse_sfc_transition(&mut self, prefixed_name: Option<Identifier>) -> SfcTransition {
        let name = if prefixed_name.is_some() {
            prefixed_name
        } else if self.current_identifier().is_some()
            && !self.check_keyword("FROM")
            && (self.peek_symbol(Symbol::Assign)
                || self
                    .tokens
                    .get(self.pos + 1)
                    .is_some_and(|token| matches!(&token.kind, TokenKind::Ident(value) if canonical_identifier(value) == "FROM"))
                || self.peek_symbol(Symbol::LParen))
        {
            let name = self.current_identifier();
            self.advance();
            name
        } else {
            None
        };

        let priority = if self.match_symbol(Symbol::LParen) {
            let mut priority = None;
            if self.match_keyword("PRIORITY") {
                self.expect_symbol(Symbol::Assign, "expected ':=' in SFC transition priority");
                priority = self.parse_integer_token("expected integer SFC transition priority");
            }
            while !self.is_eof() && !self.check_symbol(Symbol::RParen) {
                self.advance();
            }
            self.expect_symbol(Symbol::RParen, "expected ')' after SFC transition options");
            priority
        } else {
            None
        };

        if self.match_keyword("FROM") {
            let from = self.parse_sfc_step_list("expected SFC transition predecessor step");
            self.expect_keyword("TO", "expected TO in SFC transition");
            let to = self.parse_sfc_step_list("expected SFC transition successor step");
            let condition = if self.match_symbol(Symbol::Assign) {
                let condition = Some(self.parse_expression());
                self.expect_symbol(Symbol::Semicolon, "expected ';' after transition condition");
                condition
            } else if self.match_symbol(Symbol::Colon) {
                self.parse_sfc_transition_body_condition()
            } else {
                self.error_at(
                    &self.current().clone(),
                    "expected ':=' or ':' in SFC transition",
                );
                None
            };
            self.expect_keyword("END_TRANSITION", "expected END_TRANSITION");
            self.match_symbol(Symbol::Semicolon);
            SfcTransition {
                name,
                from,
                to,
                condition,
                priority,
            }
        } else {
            self.expect_symbol(Symbol::Assign, "expected ':=' in transition");
            let condition = Some(self.parse_expression());
            self.expect_symbol(Symbol::Semicolon, "expected ';' after transition");
            SfcTransition {
                name,
                from: Vec::new(),
                to: Vec::new(),
                condition,
                priority,
            }
        }
    }

    fn parse_sfc_transition_body_condition(&mut self) -> Option<Expr> {
        if self.match_keyword("LADDER") {
            return self.parse_textual_ladder_transition_condition("END_LADDER");
        }
        if self.match_keyword("FBD") {
            return self.parse_textual_fbd_transition_condition("END_FBD");
        }

        let mut accumulator = None;
        while !self.is_eof() && !self.check_keyword("END_TRANSITION") {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            let token = self.current().clone();
            if let Some(op) = self.current_il_op() {
                self.advance();
                let operand = if il_op_needs_operand(op)
                    && !self.check_keyword("END_TRANSITION")
                    && !self.check_symbol(Symbol::Semicolon)
                {
                    Some(self.parse_il_operand())
                } else {
                    None
                };
                accumulator = self.fold_il_expression(accumulator, op, operand, &token);
                self.match_symbol(Symbol::Semicolon);
                continue;
            }

            if accumulator.is_some() {
                self.error_at(
                    &token,
                    format!(
                        "expected IL instruction or END_TRANSITION in SFC transition body, found '{}'",
                        token.lexeme
                    ),
                );
                self.synchronize_to_keyword("END_TRANSITION");
                break;
            }

            let stop = self.line_end_after(token.span.start);
            accumulator = Some(self.parse_expression_until(stop));
            self.match_symbol(Symbol::Semicolon);
        }

        if accumulator.is_none() {
            self.error_at(
                &self.current().clone(),
                "SFC transition body requires an expression or IL accumulator body",
            );
        }
        accumulator
    }

    fn parse_textual_ladder_transition_condition(&mut self, end_keyword: &str) -> Option<Expr> {
        let mut conditions = Vec::new();
        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }
            if self.match_keyword("RUNG") || self.match_keyword("NETWORK") {
                self.parse_optional_network_label();
            }
            let rung = self.parse_textual_ladder_rung(&["END_RUNG", "END_NETWORK", end_keyword]);
            conditions.push(rung.condition);
            if self.match_keyword("END_RUNG") || self.match_keyword("END_NETWORK") {
                self.match_symbol(Symbol::Semicolon);
            }
        }
        self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
        self.match_symbol(Symbol::Semicolon);
        self.or_exprs(conditions)
    }

    fn parse_textual_fbd_transition_condition(&mut self, end_keyword: &str) -> Option<Expr> {
        let mut conditions = Vec::new();
        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }
            if self.match_keyword("NETWORK") {
                self.parse_optional_network_label();
            }

            while !self.is_eof() && !self.check_any_keyword(&["END_NETWORK", end_keyword]) {
                if self.match_symbol(Symbol::Semicolon) {
                    continue;
                }
                if self.match_keyword("OUT") || self.match_keyword("CONDITION") {
                    self.expect_symbol(Symbol::Assign, "expected ':=' after FBD transition OUT");
                    let expr = self.parse_expression();
                    self.expect_symbol(
                        Symbol::Semicolon,
                        "expected ';' after FBD transition output",
                    );
                    conditions.push(expr);
                } else {
                    let stop = self.line_end_after(self.current().span.start);
                    let expr = self.parse_expression_until(stop);
                    self.expect_symbol(
                        Symbol::Semicolon,
                        "expected ';' after FBD transition expression",
                    );
                    conditions.push(expr);
                }
            }

            if self.match_keyword("END_NETWORK") {
                self.match_symbol(Symbol::Semicolon);
            }
        }
        self.expect_keyword(end_keyword, format!("expected {end_keyword}"));
        self.match_symbol(Symbol::Semicolon);
        self.and_exprs(conditions)
    }

    fn parse_optional_network_label(&mut self) -> Option<String> {
        let label = self.current_identifier()?;
        if self.peek_symbol(Symbol::Colon) {
            self.advance();
            self.expect_symbol(Symbol::Colon, "expected ':' after network label");
            Some(label.original)
        } else {
            None
        }
    }

    fn parse_textual_ladder_rung(&mut self, stop_keywords: &[&str]) -> TextualLadderRung {
        let label = self.parse_optional_network_label();
        let mut condition = Expr::Literal(Literal::Bool(true));
        let mut statements = Vec::new();
        let mut nodes = vec![textual_network_node("leftPowerRail", 1, &[])];
        let mut node_index = 1usize;
        let mut last_node_id = "1".to_string();

        while !self.is_eof() && !self.check_any_keyword(stop_keywords) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            if self.match_keyword("CONTACT") {
                let expr = self.parse_expression();
                self.expect_symbol(Symbol::Semicolon, "expected ';' after LD CONTACT");
                condition = self.and_expr(condition, expr.clone());
                node_index += 1;
                let node_id = node_index.to_string();
                let mut attributes = vec![("connectionRefs", last_node_id.clone())];
                if let Some(variable) = expr_variable_name(&expr) {
                    attributes.push(("variable", variable));
                } else {
                    attributes.push(("expression", expr.to_string()));
                }
                nodes.push(textual_network_node("contact", node_index, &attributes));
                last_node_id = node_id;
                continue;
            }

            if self.match_keyword("CONTACT_NOT") || self.match_keyword("CONTACTN") {
                let raw_expr = self.parse_expression();
                let expr = Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(raw_expr.clone()),
                };
                self.expect_symbol(Symbol::Semicolon, "expected ';' after LD CONTACT_NOT");
                condition = self.and_expr(condition, expr.clone());
                node_index += 1;
                let node_id = node_index.to_string();
                let mut attributes = vec![
                    ("connectionRefs", last_node_id.clone()),
                    ("negated", "true".to_string()),
                ];
                if let Some(variable) = expr_variable_name(&raw_expr) {
                    attributes.push(("variable", variable));
                } else {
                    attributes.push(("expression", raw_expr.to_string()));
                }
                nodes.push(textual_network_node("contact", node_index, &attributes));
                last_node_id = node_id;
                continue;
            }

            if self.match_keyword("COIL") || self.match_keyword("COIL_NOT") {
                let negated = canonical_identifier(&self.previous().lexeme) == "COIL_NOT";
                let target = self.parse_variable_ref();
                self.expect_symbol(Symbol::Semicolon, "expected ';' after LD COIL");
                let value = if negated {
                    Expr::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(condition.clone()),
                    }
                } else {
                    condition.clone()
                };
                statements.push(Statement::Assignment {
                    target: target.clone(),
                    value,
                });
                node_index += 1;
                nodes.push(textual_network_node(
                    "coil",
                    node_index,
                    &[
                        ("connectionRefs", last_node_id.clone()),
                        ("variable", target.to_string()),
                    ],
                ));
                continue;
            }

            if self.match_keyword("SET") || self.match_keyword("RESET") {
                let set = canonical_identifier(&self.previous().lexeme) == "SET";
                let target = self.parse_variable_ref();
                self.expect_symbol(Symbol::Semicolon, "expected ';' after LD SET/RESET coil");
                statements.push(Statement::If {
                    branches: vec![(
                        condition.clone(),
                        vec![Statement::Assignment {
                            target: target.clone(),
                            value: Expr::Literal(Literal::Bool(set)),
                        }],
                    )],
                    else_branch: Vec::new(),
                });
                node_index += 1;
                nodes.push(textual_network_node(
                    "coil",
                    node_index,
                    &[
                        ("connectionRefs", last_node_id.clone()),
                        ("variable", target.to_string()),
                        ("storage", if set { "set" } else { "reset" }.to_string()),
                    ],
                ));
                continue;
            }

            let token = self.current().clone();
            self.error_at(
                &token,
                format!(
                    "unsupported or invalid textual LD element '{}'",
                    token.lexeme
                ),
            );
            self.synchronize_to_semicolon();
        }

        TextualLadderRung {
            label,
            condition,
            statements,
            nodes,
        }
    }

    fn parse_textual_fbd_network(&mut self, stop_keywords: &[&str]) -> TextualFbdNetwork {
        let label = self.parse_optional_network_label();
        let mut statements = Vec::new();
        let mut nodes = Vec::new();
        let mut node_index = 0usize;

        while !self.is_eof() && !self.check_any_keyword(stop_keywords) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            if self.match_keyword("OUT") || self.match_keyword("BLOCK") {
                let target = self.parse_variable_ref();
                self.expect_symbol(Symbol::Assign, "expected ':=' after FBD output target");
                let value = self.parse_expression();
                self.expect_symbol(Symbol::Semicolon, "expected ';' after FBD output");
                statements.push(Statement::Assignment {
                    target: target.clone(),
                    value: value.clone(),
                });
                node_index += 1;
                nodes.push(textual_network_node(
                    "outVariable",
                    node_index,
                    &[
                        ("expression", target.to_string()),
                        ("value", value.to_string()),
                    ],
                ));
                continue;
            }

            let statement = self.parse_statement();
            statements.push(statement);
            self.match_symbol(Symbol::Semicolon);
        }

        TextualFbdNetwork {
            label,
            statements,
            nodes,
        }
    }

    fn and_expr(&self, left: Expr, right: Expr) -> Expr {
        Expr::Binary {
            op: BinaryOp::And,
            left: Box::new(left),
            right: Box::new(right),
        }
    }

    fn and_exprs(&self, mut exprs: Vec<Expr>) -> Option<Expr> {
        if exprs.is_empty() {
            return None;
        }
        let first = exprs.remove(0);
        Some(
            exprs
                .into_iter()
                .fold(first, |left, right| self.and_expr(left, right)),
        )
    }

    fn or_exprs(&self, mut exprs: Vec<Expr>) -> Option<Expr> {
        if exprs.is_empty() {
            return None;
        }
        let first = exprs.remove(0);
        Some(exprs.into_iter().fold(first, |left, right| Expr::Binary {
            op: BinaryOp::Or,
            left: Box::new(left),
            right: Box::new(right),
        }))
    }

    fn parse_sfc_step_list(&mut self, message: &str) -> Vec<Identifier> {
        if self.match_symbol(Symbol::LParen) {
            let mut steps = Vec::new();
            while !self.is_eof() && !self.check_symbol(Symbol::RParen) {
                if let Some(step) = self.expect_identifier(message) {
                    steps.push(step);
                }
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::RParen, "expected ')' after SFC step list");
            steps
        } else {
            self.expect_identifier(message)
                .map(|step| vec![step])
                .unwrap_or_default()
        }
    }

    fn parse_integer_token(&mut self, message: &str) -> Option<i64> {
        let token = self.current().clone();
        match &token.kind {
            TokenKind::Number(text) => {
                self.advance();
                text.parse::<i64>().ok()
            }
            _ => {
                self.error_at(&token, message);
                None
            }
        }
    }

    fn parse_sfc_action_qualifier(&mut self) -> (SfcActionQualifier, Option<Literal>) {
        if !self.match_symbol(Symbol::LParen) {
            return (SfcActionQualifier::NonStored, None);
        }

        let qualifier = if let Some(name) = self.expect_identifier("expected SFC action qualifier")
        {
            SfcActionQualifier::parse(&name.original).unwrap_or_else(|| {
                let token = self.previous().clone();
                self.error_at(
                    &token,
                    format!("unknown SFC action qualifier '{}'", name.original),
                );
                SfcActionQualifier::NonStored
            })
        } else {
            SfcActionQualifier::NonStored
        };

        let duration = if self.match_symbol(Symbol::Comma) {
            let expr = self.parse_expression();
            if let Expr::Literal(literal) = expr {
                Some(literal)
            } else {
                let token = self.previous().clone();
                self.error_at(&token, "expected literal duration in SFC action qualifier");
                None
            }
        } else {
            None
        };
        self.expect_symbol(Symbol::RParen, "expected ')' after SFC action qualifier");
        (qualifier, duration)
    }

    fn parse_var_block(&mut self) -> VarBlock {
        let kind = match self.current_ident_upper().as_deref() {
            Some("VAR_INPUT") => VarBlockKind::Input,
            Some("VAR_OUTPUT") => VarBlockKind::Output,
            Some("VAR_IN_OUT") => VarBlockKind::InOut,
            Some("VAR_EXTERNAL") => VarBlockKind::External,
            Some("VAR_GLOBAL") => VarBlockKind::Global,
            Some("VAR_TEMP") => VarBlockKind::Temp,
            Some("VAR_ACCESS") => VarBlockKind::Access,
            Some("VAR_CONFIG") => VarBlockKind::Config,
            _ => VarBlockKind::Local,
        };
        self.advance();

        let mut constant = false;
        let mut retain = None;
        loop {
            if self.match_keyword("CONSTANT") {
                constant = true;
            } else if self.match_keyword("RETAIN") {
                retain = Some(RetainKind::Retain);
            } else if self.match_keyword("NON_RETAIN") {
                retain = Some(RetainKind::NonRetain);
            } else {
                break;
            }
        }

        let mut vars = Vec::new();
        while !self.is_eof() && !self.match_keyword("END_VAR") {
            if kind == VarBlockKind::Access && self.peek_symbol(Symbol::Colon) {
                vars.push(self.parse_access_decl());
            } else {
                vars.extend(self.parse_var_decl());
            }
        }

        VarBlock {
            kind,
            constant,
            retain,
            vars,
        }
    }

    fn parse_var_decl(&mut self) -> Vec<VarDecl> {
        let mut names = Vec::new();
        loop {
            let Some(name) = self.expect_identifier("expected variable name") else {
                self.synchronize_to_semicolon();
                return Vec::new();
            };
            let location = if self.match_keyword("AT") {
                match &self.current().kind {
                    TokenKind::DirectVariable(value)
                    | TokenKind::Ident(value)
                    | TokenKind::HashLiteral(value) => {
                        let value = value.clone();
                        self.advance();
                        Some(value)
                    }
                    _ => {
                        let token = self.current().clone();
                        self.error_at(&token, "expected location after AT");
                        None
                    }
                }
            } else {
                None
            };
            names.push((name, location));
            if !self.match_symbol(Symbol::Comma) {
                break;
            }
        }

        self.expect_symbol(Symbol::Colon, "expected ':' in variable declaration");
        let type_spec = self.parse_type_spec();
        let edge = if self.match_keyword("R_EDGE") {
            Some(EdgeQualifier::Rising)
        } else if self.match_keyword("F_EDGE") {
            Some(EdgeQualifier::Falling)
        } else {
            None
        };
        let initial_value = if self.match_symbol(Symbol::Assign) {
            Some(self.parse_expression())
        } else {
            None
        };
        self.expect_symbol(Symbol::Semicolon, "expected ';' after variable declaration");

        names
            .into_iter()
            .map(|(name, location)| VarDecl {
                name,
                location,
                access: None,
                edge,
                type_spec: type_spec.clone(),
                initial_value: initial_value.clone(),
            })
            .collect()
    }

    fn parse_access_decl(&mut self) -> VarDecl {
        let name = self
            .expect_identifier("expected access path name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        self.expect_symbol(Symbol::Colon, "expected ':' after access path name");
        let mut path = String::new();
        while !self.is_eof() && !self.check_symbol(Symbol::Colon) {
            path.push_str(&self.current().lexeme);
            self.advance();
        }
        if path.is_empty() {
            let token = self.current().clone();
            self.error_at(&token, "expected access path target");
        }
        self.expect_symbol(Symbol::Colon, "expected ':' after access path target");
        let type_spec = self.parse_type_spec();
        let direction = if self.match_keyword("READ_WRITE") {
            AccessDirection::ReadWrite
        } else {
            let _ = self.match_keyword("READ_ONLY");
            AccessDirection::ReadOnly
        };
        self.expect_symbol(Symbol::Semicolon, "expected ';' after access declaration");
        VarDecl {
            name,
            location: None,
            access: Some(AccessSpec { path, direction }),
            edge: None,
            type_spec,
            initial_value: None,
        }
    }

    fn parse_configuration(&mut self) -> Configuration {
        let name = self
            .expect_identifier("expected configuration name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        let mut var_blocks = Vec::new();
        let mut resources = Vec::new();
        while !self.is_eof() && !self.match_keyword("END_CONFIGURATION") {
            if self.match_keyword("RESOURCE") {
                resources.push(self.parse_resource());
            } else if self.is_var_block_start() {
                var_blocks.push(self.parse_var_block());
            } else {
                self.advance();
            }
        }
        Configuration {
            name,
            var_blocks,
            resources,
        }
    }

    fn parse_resource(&mut self) -> Resource {
        let name = self
            .expect_identifier("expected resource name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        if self.match_keyword("ON") {
            let _ = self.expect_identifier("expected resource type after ON");
        }

        let mut tasks = Vec::new();
        let mut var_blocks = Vec::new();
        let mut program_instances = Vec::new();
        while !self.is_eof() && !self.match_keyword("END_RESOURCE") {
            if self.match_keyword("TASK") {
                tasks.push(self.parse_task());
            } else if self.match_keyword("PROGRAM") {
                program_instances.push(self.parse_program_instance());
            } else if self.is_var_block_start() {
                var_blocks.push(self.parse_var_block());
            } else {
                self.advance();
            }
        }

        Resource {
            name,
            var_blocks,
            tasks,
            program_instances,
        }
    }

    fn parse_task(&mut self) -> Task {
        let name = self
            .expect_identifier("expected task name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        let mut single = None;
        let mut interval = None;
        let mut priority = None;

        if self.match_symbol(Symbol::LParen) {
            while !self.is_eof() && !self.check_symbol(Symbol::RParen) {
                let param = self
                    .expect_identifier("expected task parameter")
                    .unwrap_or_else(|| Identifier::new("<error>"));
                self.expect_symbol(Symbol::Assign, "expected ':=' in task parameter");
                let value = self.parse_expression();
                match param.canonical.as_str() {
                    "SINGLE" => {
                        single = Some(value);
                    }
                    "INTERVAL" => {
                        interval = Some(value);
                    }
                    "PRIORITY" => {
                        priority = Some(value);
                    }
                    _ => {}
                }
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::RParen, "expected ')' after task parameters");
        }
        self.expect_symbol(Symbol::Semicolon, "expected ';' after TASK declaration");

        Task {
            name,
            single,
            interval,
            priority,
        }
    }

    fn parse_program_instance(&mut self) -> ProgramInstance {
        let name = self
            .expect_identifier("expected program instance name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        let task = if self.match_keyword("WITH") {
            self.expect_identifier("expected task name after WITH")
        } else {
            None
        };
        self.expect_symbol(
            Symbol::Colon,
            "expected ':' in program instance declaration",
        );
        let program_type = self
            .expect_identifier("expected program type name")
            .unwrap_or_else(|| Identifier::new("<error>"));
        let args = if self.match_symbol(Symbol::LParen) {
            let args = self.parse_param_assignment_list(Symbol::RParen);
            self.expect_symbol(
                Symbol::RParen,
                "expected ')' after PROGRAM instance parameters",
            );
            args
        } else {
            Vec::new()
        };
        self.expect_symbol(
            Symbol::Semicolon,
            "expected ';' after PROGRAM instance declaration",
        );

        ProgramInstance {
            name,
            program_type,
            task,
            args,
        }
    }

    fn parse_statement_list(&mut self, stop_keywords: &[&str]) -> Vec<Statement> {
        let mut statements = Vec::new();
        while !self.is_eof() && !self.check_any_keyword(stop_keywords) {
            let statement = self.parse_statement();
            statements.push(statement);
            self.match_symbol(Symbol::Semicolon);
        }
        statements
    }

    fn parse_statement(&mut self) -> Statement {
        if self.match_symbol(Symbol::Semicolon) {
            return Statement::Empty;
        }

        if let Some(label) = self.current_identifier() {
            if self.peek_symbol(Symbol::Colon) {
                self.advance();
                self.expect_symbol(Symbol::Colon, "expected ':' after IL label");
                return Statement::IlLabel(label);
            }
        }

        if self.current_il_op().is_some()
            && !self.peek_symbol(Symbol::Assign)
            && !self.peek_attached_lparen()
        {
            return self.parse_il_instruction();
        }

        if self.match_keyword("IF") {
            return self.parse_if_statement();
        }
        if self.match_keyword("CASE") {
            return self.parse_case_statement();
        }
        if self.match_keyword("FOR") {
            return self.parse_for_statement();
        }
        if self.match_keyword("WHILE") {
            let condition = self.parse_expression();
            self.expect_keyword("DO", "expected DO in WHILE statement");
            let body = self.parse_statement_list(&["END_WHILE"]);
            self.expect_keyword("END_WHILE", "expected END_WHILE");
            return Statement::While { condition, body };
        }
        if self.match_keyword("REPEAT") {
            let body = self.parse_statement_list(&["UNTIL"]);
            self.expect_keyword("UNTIL", "expected UNTIL in REPEAT statement");
            let until = self.parse_expression();
            self.expect_keyword("END_REPEAT", "expected END_REPEAT");
            return Statement::Repeat { body, until };
        }
        if self.match_keyword("EXIT") {
            return Statement::Exit;
        }
        if self.match_keyword("RETURN") {
            return Statement::Return;
        }

        if matches!(
            &self.current().kind,
            TokenKind::Ident(_) | TokenKind::DirectVariable(_)
        ) {
            let target = self.parse_variable_ref();
            if self.match_symbol(Symbol::Assign) {
                let value = self.parse_expression();
                return Statement::Assignment { target, value };
            }
            if self.match_symbol(Symbol::LParen) {
                let args = self.parse_param_assignment_list(Symbol::RParen);
                self.expect_symbol(Symbol::RParen, "expected ')' after function block call");
                return Statement::FbCall { name: target, args };
            }
        }

        let token = self.current().clone();
        self.error_at(
            &token,
            format!("unsupported or invalid statement '{}'", token.lexeme),
        );
        self.synchronize_to_semicolon();
        Statement::Unsupported(token.lexeme)
    }

    fn parse_il_instruction(&mut self) -> Statement {
        let token = self.current().clone();
        let Some(op) = self.current_il_op() else {
            self.error_at(&token, "expected IL operator");
            self.advance();
            return Statement::Unsupported(token.lexeme);
        };
        self.advance();

        let needs_operand = !matches!(op, IlOp::Not | IlOp::Ret | IlOp::Retc | IlOp::Retcn);
        let operand = if needs_operand && !self.is_eof() && !self.next_token_starts_statement() {
            Some(self.parse_il_operand())
        } else {
            None
        };

        Statement::Il { op, operand }
    }

    fn parse_il_operand(&mut self) -> Expr {
        if self.check_symbol(Symbol::LParen) && self.next_token_is_il_op() {
            self.parse_il_parenthesized_expression()
        } else {
            let stop = self.line_end_after(self.current().span.start);
            self.parse_expression_until(stop)
        }
    }

    fn parse_il_parenthesized_expression(&mut self) -> Expr {
        self.expect_symbol(
            Symbol::LParen,
            "expected '(' in IL parenthesized expression",
        );
        let mut accumulator = None;

        while !self.is_eof() && !self.check_symbol(Symbol::RParen) {
            if self.match_symbol(Symbol::Semicolon) {
                continue;
            }

            let token = self.current().clone();
            let Some(op) = self.current_il_op() else {
                self.error_at(
                    &token,
                    format!(
                        "expected IL instruction inside parenthesized expression, found '{}'",
                        token.lexeme
                    ),
                );
                self.synchronize_to_il_expression_boundary();
                continue;
            };
            self.advance();

            let operand = if il_op_needs_operand(op)
                && !self.check_symbol(Symbol::Semicolon)
                && !self.check_symbol(Symbol::RParen)
            {
                Some(self.parse_il_operand())
            } else {
                None
            };

            accumulator = self.fold_il_expression(accumulator, op, operand, &token);
            self.match_symbol(Symbol::Semicolon);
        }

        self.expect_symbol(
            Symbol::RParen,
            "expected ')' after IL parenthesized expression",
        );
        accumulator.unwrap_or_else(|| {
            let token = self.previous().clone();
            self.error_at(&token, "empty IL parenthesized expression");
            Expr::Literal(Literal::Int(0))
        })
    }

    fn fold_il_expression(
        &mut self,
        accumulator: Option<Expr>,
        op: IlOp,
        operand: Option<Expr>,
        token: &Token,
    ) -> Option<Expr> {
        match op {
            IlOp::Ld => self.il_required_operand(op, operand, token),
            IlOp::Ldn => self
                .il_required_operand(op, operand, token)
                .map(|expr| Expr::Unary {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                }),
            IlOp::Not => accumulator.map(|expr| Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(expr),
            }),
            IlOp::And | IlOp::Andn | IlOp::Or | IlOp::Orn | IlOp::Xor | IlOp::Xorn => {
                let left = self.il_required_accumulator(accumulator, token)?;
                let mut right = self.il_required_operand(op, operand, token)?;
                if matches!(op, IlOp::Andn | IlOp::Orn | IlOp::Xorn) {
                    right = Expr::Unary {
                        op: UnaryOp::Not,
                        expr: Box::new(right),
                    };
                }
                let binary = match op {
                    IlOp::And | IlOp::Andn => BinaryOp::And,
                    IlOp::Or | IlOp::Orn => BinaryOp::Or,
                    IlOp::Xor | IlOp::Xorn => BinaryOp::Xor,
                    _ => unreachable!(),
                };
                Some(Expr::Binary {
                    op: binary,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            IlOp::Add
            | IlOp::Sub
            | IlOp::Mul
            | IlOp::Div
            | IlOp::Mod
            | IlOp::Gt
            | IlOp::Ge
            | IlOp::Eq
            | IlOp::Ne
            | IlOp::Le
            | IlOp::Lt => {
                let left = self.il_required_accumulator(accumulator, token)?;
                let right = self.il_required_operand(op, operand, token)?;
                let binary = match op {
                    IlOp::Add => BinaryOp::Add,
                    IlOp::Sub => BinaryOp::Sub,
                    IlOp::Mul => BinaryOp::Mul,
                    IlOp::Div => BinaryOp::Div,
                    IlOp::Mod => BinaryOp::Mod,
                    IlOp::Gt => BinaryOp::Greater,
                    IlOp::Ge => BinaryOp::GreaterEqual,
                    IlOp::Eq => BinaryOp::Equal,
                    IlOp::Ne => BinaryOp::NotEqual,
                    IlOp::Le => BinaryOp::LessEqual,
                    IlOp::Lt => BinaryOp::Less,
                    _ => unreachable!(),
                };
                Some(Expr::Binary {
                    op: binary,
                    left: Box::new(left),
                    right: Box::new(right),
                })
            }
            IlOp::St
            | IlOp::Stn
            | IlOp::S
            | IlOp::R
            | IlOp::Jmp
            | IlOp::Jmpc
            | IlOp::Jmpcn
            | IlOp::Cal
            | IlOp::Calc
            | IlOp::Calcn
            | IlOp::Ret
            | IlOp::Retc
            | IlOp::Retcn => {
                self.error_at(
                    token,
                    format!(
                        "IL {} instruction is not valid inside a parenthesized expression",
                        il_op_name(op)
                    ),
                );
                accumulator
            }
        }
    }

    fn il_required_accumulator(
        &mut self,
        accumulator: Option<Expr>,
        token: &Token,
    ) -> Option<Expr> {
        if accumulator.is_none() {
            self.error_at(
                token,
                "IL parenthesized expression operator requires a preceding accumulator",
            );
        }
        accumulator
    }

    fn il_required_operand(
        &mut self,
        op: IlOp,
        operand: Option<Expr>,
        token: &Token,
    ) -> Option<Expr> {
        if operand.is_none() {
            self.error_at(
                token,
                format!("IL {} instruction requires an operand", il_op_name(op)),
            );
        }
        operand
    }

    fn synchronize_to_il_expression_boundary(&mut self) {
        while !self.is_eof()
            && !self.check_symbol(Symbol::Semicolon)
            && !self.check_symbol(Symbol::RParen)
        {
            self.advance();
        }
        self.match_symbol(Symbol::Semicolon);
    }

    fn next_token_is_il_op(&self) -> bool {
        self.tokens
            .get(self.pos + 1)
            .and_then(|token| match &token.kind {
                TokenKind::Ident(value) => il_op_from_upper(&canonical_identifier(value)),
                _ => None,
            })
            .is_some()
    }

    fn current_il_op(&self) -> Option<IlOp> {
        let op = self.current_ident_upper()?;
        il_op_from_upper(&op)
    }
}

fn il_op_from_upper(op: &str) -> Option<IlOp> {
    let base_op = typed_il_base_op(op);
    match base_op {
        "LD" => Some(IlOp::Ld),
        "LDN" => Some(IlOp::Ldn),
        "ST" => Some(IlOp::St),
        "STN" => Some(IlOp::Stn),
        "S" => Some(IlOp::S),
        "R" => Some(IlOp::R),
        "AND" => Some(IlOp::And),
        "ANDN" => Some(IlOp::Andn),
        "OR" => Some(IlOp::Or),
        "ORN" => Some(IlOp::Orn),
        "XOR" => Some(IlOp::Xor),
        "XORN" => Some(IlOp::Xorn),
        "NOT" => Some(IlOp::Not),
        "ADD" => Some(IlOp::Add),
        "SUB" => Some(IlOp::Sub),
        "MUL" => Some(IlOp::Mul),
        "DIV" => Some(IlOp::Div),
        "MOD" => Some(IlOp::Mod),
        "GT" => Some(IlOp::Gt),
        "GE" => Some(IlOp::Ge),
        "EQ" => Some(IlOp::Eq),
        "NE" => Some(IlOp::Ne),
        "LE" => Some(IlOp::Le),
        "LT" => Some(IlOp::Lt),
        "JMP" => Some(IlOp::Jmp),
        "JMPC" => Some(IlOp::Jmpc),
        "JMPCN" => Some(IlOp::Jmpcn),
        "CAL" => Some(IlOp::Cal),
        "CALC" => Some(IlOp::Calc),
        "CALCN" => Some(IlOp::Calcn),
        "RET" => Some(IlOp::Ret),
        "RETC" => Some(IlOp::Retc),
        "RETCN" => Some(IlOp::Retcn),
        _ => None,
    }
}

fn il_op_needs_operand(op: IlOp) -> bool {
    !matches!(op, IlOp::Not | IlOp::Ret | IlOp::Retc | IlOp::Retcn)
}

fn il_op_name(op: IlOp) -> &'static str {
    match op {
        IlOp::Ld => "LD",
        IlOp::Ldn => "LDN",
        IlOp::St => "ST",
        IlOp::Stn => "STN",
        IlOp::S => "S",
        IlOp::R => "R",
        IlOp::And => "AND",
        IlOp::Andn => "ANDN",
        IlOp::Or => "OR",
        IlOp::Orn => "ORN",
        IlOp::Xor => "XOR",
        IlOp::Xorn => "XORN",
        IlOp::Not => "NOT",
        IlOp::Add => "ADD",
        IlOp::Sub => "SUB",
        IlOp::Mul => "MUL",
        IlOp::Div => "DIV",
        IlOp::Mod => "MOD",
        IlOp::Gt => "GT",
        IlOp::Ge => "GE",
        IlOp::Eq => "EQ",
        IlOp::Ne => "NE",
        IlOp::Le => "LE",
        IlOp::Lt => "LT",
        IlOp::Jmp => "JMP",
        IlOp::Jmpc => "JMPC",
        IlOp::Jmpcn => "JMPCN",
        IlOp::Cal => "CAL",
        IlOp::Calc => "CALC",
        IlOp::Calcn => "CALCN",
        IlOp::Ret => "RET",
        IlOp::Retc => "RETC",
        IlOp::Retcn => "RETCN",
    }
}

fn typed_il_base_op(op: &str) -> &str {
    let Some((base, suffix)) = op.split_once('_') else {
        return op;
    };
    if suffix.is_empty() || !is_il_type_suffix(suffix) {
        return op;
    }
    base
}

fn is_il_type_suffix(suffix: &str) -> bool {
    matches!(
        suffix,
        "BOOL"
            | "SINT"
            | "INT"
            | "DINT"
            | "LINT"
            | "USINT"
            | "UINT"
            | "UDINT"
            | "ULINT"
            | "REAL"
            | "LREAL"
            | "BYTE"
            | "WORD"
            | "DWORD"
            | "LWORD"
            | "STRING"
            | "WSTRING"
            | "TIME"
            | "DATE"
            | "TOD"
            | "TIME_OF_DAY"
            | "DT"
            | "DATE_AND_TIME"
    )
}

impl<'a> Parser<'a> {
    fn next_token_starts_statement(&self) -> bool {
        self.check_symbol(Symbol::Semicolon)
            || self.current_ident_upper().is_some_and(|keyword| {
                matches!(
                    keyword.as_str(),
                    "IF" | "CASE"
                        | "FOR"
                        | "WHILE"
                        | "REPEAT"
                        | "EXIT"
                        | "RETURN"
                        | "END_IF"
                        | "END_CASE"
                        | "END_FOR"
                        | "END_WHILE"
                        | "END_REPEAT"
                        | "END_PROGRAM"
                        | "END_FUNCTION"
                        | "END_FUNCTION_BLOCK"
                ) || self.current_il_op().is_some()
            })
    }

    fn parse_if_statement(&mut self) -> Statement {
        let mut branches = Vec::new();
        let first_condition = self.parse_expression();
        self.expect_keyword("THEN", "expected THEN in IF statement");
        let first_body = self.parse_statement_list(&["ELSIF", "ELSE", "END_IF"]);
        branches.push((first_condition, first_body));

        while self.match_keyword("ELSIF") {
            let condition = self.parse_expression();
            self.expect_keyword("THEN", "expected THEN after ELSIF");
            let body = self.parse_statement_list(&["ELSIF", "ELSE", "END_IF"]);
            branches.push((condition, body));
        }

        let else_branch = if self.match_keyword("ELSE") {
            self.parse_statement_list(&["END_IF"])
        } else {
            Vec::new()
        };
        self.expect_keyword("END_IF", "expected END_IF");

        Statement::If {
            branches,
            else_branch,
        }
    }

    fn parse_case_statement(&mut self) -> Statement {
        let selector = self.parse_expression();
        self.expect_keyword("OF", "expected OF in CASE statement");
        let mut cases = Vec::new();

        while !self.is_eof() && !self.check_any_keyword(&["ELSE", "END_CASE"]) {
            let mut labels = Vec::new();
            loop {
                let low = self.parse_expression();
                if self.match_symbol(Symbol::Range) {
                    let high = self.parse_expression();
                    labels.push(CaseLabel::Range(low, high));
                } else {
                    labels.push(CaseLabel::Single(low));
                }
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::Colon, "expected ':' after CASE labels");
            let body = self.parse_case_body();
            cases.push((labels, body));
        }

        let else_branch = if self.match_keyword("ELSE") {
            self.parse_statement_list(&["END_CASE"])
        } else {
            Vec::new()
        };
        self.expect_keyword("END_CASE", "expected END_CASE");

        Statement::Case {
            selector,
            cases,
            else_branch,
        }
    }

    fn parse_case_body(&mut self) -> Vec<Statement> {
        let mut statements = Vec::new();
        while !self.is_eof()
            && !self.check_any_keyword(&["ELSE", "END_CASE"])
            && !self.current_starts_case_clause()
        {
            let statement = self.parse_statement();
            statements.push(statement);
            self.match_symbol(Symbol::Semicolon);
        }
        statements
    }

    fn current_starts_case_clause(&self) -> bool {
        let mut pos = self.pos;
        while pos < self.tokens.len() {
            match &self.tokens[pos].kind {
                TokenKind::Symbol(Symbol::Colon) => return true,
                TokenKind::Symbol(Symbol::Assign | Symbol::Semicolon) | TokenKind::Eof => {
                    return false;
                }
                TokenKind::Ident(value)
                    if matches!(
                        canonical_identifier(value).as_str(),
                        "ELSE"
                            | "END_CASE"
                            | "IF"
                            | "CASE"
                            | "FOR"
                            | "WHILE"
                            | "REPEAT"
                            | "EXIT"
                            | "RETURN"
                    ) =>
                {
                    return false;
                }
                _ => {
                    pos += 1;
                }
            }
        }
        false
    }

    fn parse_for_statement(&mut self) -> Statement {
        let control = self
            .expect_identifier("expected FOR control variable")
            .unwrap_or_else(|| Identifier::new("<error>"));
        self.expect_symbol(Symbol::Assign, "expected ':=' in FOR statement");
        let from = self.parse_expression();
        self.expect_keyword("TO", "expected TO in FOR statement");
        let to = self.parse_expression();
        let by = if self.match_keyword("BY") {
            Some(self.parse_expression())
        } else {
            None
        };
        self.expect_keyword("DO", "expected DO in FOR statement");
        let body = self.parse_statement_list(&["END_FOR"]);
        self.expect_keyword("END_FOR", "expected END_FOR");
        Statement::For {
            control,
            from,
            to,
            by,
            body,
        }
    }

    fn parse_expression(&mut self) -> Expr {
        self.parse_or()
    }

    fn parse_expression_until(&mut self, stop: usize) -> Expr {
        let previous = self.expression_stop;
        self.expression_stop = Some(previous.map_or(stop, |existing| existing.min(stop)));
        let expr = self.parse_expression();
        self.expression_stop = previous;
        expr
    }

    fn at_expression_stop(&self) -> bool {
        self.expression_stop
            .is_some_and(|stop| self.current().span.start >= stop)
    }

    fn line_end_after(&self, offset: usize) -> usize {
        self.source[offset..]
            .find('\n')
            .map(|relative| offset + relative)
            .unwrap_or(self.source.len())
    }

    fn parse_or(&mut self) -> Expr {
        let mut expr = self.parse_xor();
        while !self.at_expression_stop() && self.match_keyword("OR") {
            let right = self.parse_xor();
            expr = Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_xor(&mut self) -> Expr {
        let mut expr = self.parse_and();
        while !self.at_expression_stop() && self.match_keyword("XOR") {
            let right = self.parse_and();
            expr = Expr::Binary {
                op: BinaryOp::Xor,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_and(&mut self) -> Expr {
        let mut expr = self.parse_equality();
        while !self.at_expression_stop()
            && (self.match_keyword("AND") || self.match_symbol(Symbol::Amp))
        {
            let right = self.parse_equality();
            expr = Expr::Binary {
                op: BinaryOp::And,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_equality(&mut self) -> Expr {
        let mut expr = self.parse_comparison();
        loop {
            if self.at_expression_stop() {
                break;
            }
            let op = if self.match_symbol(Symbol::Eq) {
                Some(BinaryOp::Equal)
            } else if self.match_symbol(Symbol::Ne) {
                Some(BinaryOp::NotEqual)
            } else {
                None
            };

            let Some(op) = op else { break };
            let right = self.parse_comparison();
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_comparison(&mut self) -> Expr {
        let mut expr = self.parse_add();
        loop {
            if self.at_expression_stop() {
                break;
            }
            let op = if self.match_symbol(Symbol::Lt) {
                Some(BinaryOp::Less)
            } else if self.match_symbol(Symbol::Le) {
                Some(BinaryOp::LessEqual)
            } else if self.match_symbol(Symbol::Gt) {
                Some(BinaryOp::Greater)
            } else if self.match_symbol(Symbol::Ge) {
                Some(BinaryOp::GreaterEqual)
            } else {
                None
            };

            let Some(op) = op else { break };
            let right = self.parse_add();
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_add(&mut self) -> Expr {
        let mut expr = self.parse_term();
        loop {
            if self.at_expression_stop() {
                break;
            }
            let op = if self.match_symbol(Symbol::Plus) {
                Some(BinaryOp::Add)
            } else if self.match_symbol(Symbol::Minus) {
                Some(BinaryOp::Sub)
            } else {
                None
            };

            let Some(op) = op else { break };
            let right = self.parse_term();
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_term(&mut self) -> Expr {
        let mut expr = self.parse_unary();
        loop {
            if self.at_expression_stop() {
                break;
            }
            let op = if self.match_symbol(Symbol::Star) {
                Some(BinaryOp::Mul)
            } else if self.match_symbol(Symbol::Slash) {
                Some(BinaryOp::Div)
            } else if self.match_keyword("MOD") {
                Some(BinaryOp::Mod)
            } else {
                None
            };

            let Some(op) = op else { break };
            let right = self.parse_unary();
            expr = Expr::Binary {
                op,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_power(&mut self) -> Expr {
        let mut expr = self.parse_primary();
        if !self.at_expression_stop() && self.match_symbol(Symbol::Power) {
            let right = self.parse_unary();
            expr = Expr::Binary {
                op: BinaryOp::Power,
                left: Box::new(expr),
                right: Box::new(right),
            };
        }
        expr
    }

    fn parse_unary(&mut self) -> Expr {
        if self.match_symbol(Symbol::Plus) {
            return self.parse_unary();
        }
        if self.match_symbol(Symbol::Minus) {
            return Expr::Unary {
                op: UnaryOp::Neg,
                expr: Box::new(self.parse_unary()),
            };
        }
        if self.match_keyword("NOT") {
            return Expr::Unary {
                op: UnaryOp::Not,
                expr: Box::new(self.parse_unary()),
            };
        }
        self.parse_power()
    }

    fn parse_primary(&mut self) -> Expr {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Number(ref value) => {
                self.advance();
                self.parse_number_literal_token(&token, value)
            }
            TokenKind::StringLiteral(value) => {
                self.advance();
                Expr::Literal(Literal::String(value))
            }
            TokenKind::WStringLiteral(value) => {
                self.advance();
                Expr::Literal(Literal::WString(value))
            }
            TokenKind::HashLiteral(ref value) => {
                self.advance();
                Expr::Literal(self.parse_hash_literal_token(&token, value))
            }
            TokenKind::DirectVariable(_) => Expr::Variable(self.parse_variable_ref()),
            TokenKind::Ident(value) => {
                let upper = canonical_identifier(&value);
                if upper == "TRUE" || upper == "FALSE" {
                    self.advance();
                    return Expr::Literal(Literal::Bool(upper == "TRUE"));
                }

                self.advance();
                let ident = Identifier::new(value);
                if self.match_symbol(Symbol::LParen) {
                    let args = self.parse_param_assignment_list(Symbol::RParen);
                    self.expect_symbol(Symbol::RParen, "expected ')' after function call");
                    Expr::Call { name: ident, args }
                } else {
                    Expr::Variable(self.finish_variable_ref(ident))
                }
            }
            TokenKind::Symbol(Symbol::LParen) => {
                self.advance();
                if self
                    .current_identifier()
                    .is_some_and(|_| self.peek_symbol(Symbol::Assign))
                {
                    return self.parse_struct_literal();
                }
                let expr = self.parse_expression();
                self.expect_symbol(Symbol::RParen, "expected ')' after expression");
                expr
            }
            TokenKind::Symbol(Symbol::LBracket) => self.parse_array_literal(),
            _ => {
                self.error_at(
                    &token,
                    format!("expected expression, found '{}'", token.lexeme),
                );
                self.advance();
                Expr::Literal(Literal::Int(0))
            }
        }
    }

    fn parse_array_literal(&mut self) -> Expr {
        self.expect_symbol(Symbol::LBracket, "expected '[' in array literal");
        let mut elements = Vec::new();
        if self.check_symbol(Symbol::RBracket) {
            self.advance();
            return Expr::ArrayLiteral(elements);
        }
        loop {
            self.parse_array_literal_element(&mut elements);
            if !self.match_symbol(Symbol::Comma) {
                break;
            }
        }
        self.expect_symbol(Symbol::RBracket, "expected ']' after array literal");
        Expr::ArrayLiteral(elements)
    }

    fn parse_array_literal_element(&mut self, elements: &mut Vec<Expr>) {
        if matches!(self.current().kind, TokenKind::Number(_)) && self.peek_symbol(Symbol::LParen) {
            let count = self.expect_unsigned_integer("expected array repetition count");
            self.expect_symbol(Symbol::LParen, "expected '(' after array repetition count");
            let value = self.parse_expression();
            self.expect_symbol(Symbol::RParen, "expected ')' after array repetition value");
            elements.extend((0..count).map(|_| value.clone()));
            return;
        }

        elements.push(self.parse_expression());
    }

    fn parse_struct_literal(&mut self) -> Expr {
        let mut fields = Vec::new();
        if self.check_symbol(Symbol::RParen) {
            self.advance();
            return Expr::StructLiteral(fields);
        }

        loop {
            let Some(name) = self.expect_identifier("expected structure initializer field name")
            else {
                self.synchronize_to_semicolon();
                break;
            };
            self.expect_symbol(Symbol::Assign, "expected ':=' in structure initializer");
            let expr = self.parse_expression();
            fields.push(ParamAssignment {
                name: Some(name),
                output: false,
                negated: false,
                expr: Some(expr),
                variable: None,
            });
            if !self.match_symbol(Symbol::Comma) {
                break;
            }
        }

        self.expect_symbol(Symbol::RParen, "expected ')' after structure initializer");
        Expr::StructLiteral(fields)
    }

    fn parse_param_assignment_list(&mut self, end: Symbol) -> Vec<ParamAssignment> {
        let mut args = Vec::new();
        if self.check_symbol(end) {
            return args;
        }

        loop {
            let negated = self.match_keyword("NOT");
            if let Some(name) = self.current_identifier() {
                if self.peek_symbol(Symbol::Assign) {
                    self.advance();
                    self.advance();
                    let expr = self.parse_expression();
                    args.push(ParamAssignment {
                        name: Some(name),
                        output: false,
                        negated,
                        expr: Some(expr),
                        variable: None,
                    });
                } else if self.peek_symbol(Symbol::Arrow) {
                    self.advance();
                    self.advance();
                    let variable = self.parse_variable_ref();
                    args.push(ParamAssignment {
                        name: Some(name),
                        output: true,
                        negated,
                        expr: None,
                        variable: Some(variable),
                    });
                } else {
                    let expr = self.parse_expression();
                    args.push(ParamAssignment {
                        name: None,
                        output: false,
                        negated,
                        expr: Some(expr),
                        variable: None,
                    });
                }
            } else {
                let expr = self.parse_expression();
                args.push(ParamAssignment {
                    name: None,
                    output: false,
                    negated,
                    expr: Some(expr),
                    variable: None,
                });
            }

            if !self.match_symbol(Symbol::Comma) {
                break;
            }
        }
        args
    }

    fn parse_variable_ref(&mut self) -> VariableRef {
        let token = self.current().clone();
        match token.kind {
            TokenKind::DirectVariable(value) => {
                self.advance();
                VariableRef::direct(value)
            }
            TokenKind::Ident(value) => {
                self.advance();
                self.finish_variable_ref(Identifier::new(value))
            }
            _ => {
                self.error_at(&token, "expected variable reference");
                self.advance();
                VariableRef::named("<error>")
            }
        }
    }

    fn finish_variable_ref(&mut self, first: Identifier) -> VariableRef {
        let mut path = vec![first];
        let mut indices = vec![self.parse_index_suffix()];
        while self.match_symbol(Symbol::Dot) {
            if let Some(part) = self.expect_identifier("expected field name after '.'") {
                path.push(part);
                indices.push(self.parse_index_suffix());
            } else {
                break;
            }
        }
        VariableRef {
            path,
            indices,
            direct: None,
        }
    }

    fn parse_index_suffix(&mut self) -> Vec<Expr> {
        let mut indices = Vec::new();
        while self.match_symbol(Symbol::LBracket) {
            if self.check_symbol(Symbol::RBracket) {
                self.advance();
                continue;
            }
            loop {
                indices.push(self.parse_expression());
                if !self.match_symbol(Symbol::Comma) {
                    break;
                }
            }
            self.expect_symbol(Symbol::RBracket, "expected ']' after array index");
        }
        indices
    }

    fn is_var_block_start(&self) -> bool {
        matches!(
            self.current_ident_upper().as_deref(),
            Some(
                "VAR"
                    | "VAR_INPUT"
                    | "VAR_OUTPUT"
                    | "VAR_IN_OUT"
                    | "VAR_EXTERNAL"
                    | "VAR_GLOBAL"
                    | "VAR_TEMP"
                    | "VAR_ACCESS"
                    | "VAR_CONFIG"
            )
        )
    }

    fn is_sfc_statement_start(&self) -> bool {
        matches!(
            self.current_ident_upper().as_deref(),
            Some("INITIAL_STEP" | "STEP" | "TRANSITION" | "ACTION")
        ) || self.is_labeled_sfc_step()
            || self.is_labeled_sfc_transition()
            || self.is_labeled_sfc_action()
    }

    fn is_labeled_sfc_step(&self) -> bool {
        self.current_identifier().is_some()
            && self.peek_symbol(Symbol::Colon)
            && self
                .tokens
                .get(self.pos + 2)
                .is_some_and(|token| match &token.kind {
                    TokenKind::Ident(value) => matches!(
                        canonical_identifier(value).as_str(),
                        "STEP" | "INITIAL_STEP"
                    ),
                    _ => false,
                })
    }

    fn is_labeled_sfc_transition(&self) -> bool {
        self.current_identifier().is_some()
            && self.peek_symbol(Symbol::Colon)
            && self
                .tokens
                .get(self.pos + 2)
                .is_some_and(|token| match &token.kind {
                    TokenKind::Ident(value) => canonical_identifier(value) == "TRANSITION",
                    _ => false,
                })
    }

    fn is_labeled_sfc_action(&self) -> bool {
        self.current_identifier().is_some()
            && self.peek_symbol(Symbol::Colon)
            && self
                .tokens
                .get(self.pos + 2)
                .is_some_and(|token| match &token.kind {
                    TokenKind::Ident(value) => canonical_identifier(value) == "ACTION",
                    _ => false,
                })
    }

    fn current(&self) -> &Token {
        &self.tokens[self.pos]
    }

    fn previous(&self) -> &Token {
        &self.tokens[self.pos.saturating_sub(1)]
    }

    fn is_eof(&self) -> bool {
        matches!(&self.current().kind, TokenKind::Eof)
    }

    fn advance(&mut self) -> &Token {
        if !self.is_eof() {
            self.pos += 1;
        }
        &self.tokens[self.pos.saturating_sub(1)]
    }

    fn current_identifier(&self) -> Option<Identifier> {
        match &self.current().kind {
            TokenKind::Ident(value) => Some(Identifier::new(value.clone())),
            _ => None,
        }
    }

    fn current_ident_upper(&self) -> Option<String> {
        match &self.current().kind {
            TokenKind::Ident(value) => Some(canonical_identifier(value)),
            _ => None,
        }
    }

    fn check_keyword(&self, keyword: &str) -> bool {
        self.current_ident_upper()
            .is_some_and(|value| value == keyword)
    }

    fn check_any_keyword(&self, keywords: &[&str]) -> bool {
        keywords.iter().any(|keyword| self.check_keyword(keyword))
    }

    fn match_keyword(&mut self, keyword: &str) -> bool {
        if self.check_keyword(keyword) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_keyword(&mut self, keyword: &str, message: impl Into<String>) -> bool {
        if self.match_keyword(keyword) {
            true
        } else {
            let token = self.current().clone();
            self.error_at(&token, message);
            false
        }
    }

    fn check_symbol(&self, symbol: Symbol) -> bool {
        matches!(&self.current().kind, TokenKind::Symbol(current) if *current == symbol)
    }

    fn peek_symbol(&self, symbol: Symbol) -> bool {
        matches!(
            self.tokens.get(self.pos + 1).map(|token| &token.kind),
            Some(TokenKind::Symbol(current)) if *current == symbol
        )
    }

    fn peek_attached_lparen(&self) -> bool {
        matches!(
            self.tokens.get(self.pos + 1),
            Some(Token {
                kind: TokenKind::Symbol(Symbol::LParen),
                span,
                ..
            }) if self.current().span.end == span.start
        )
    }

    fn match_symbol(&mut self, symbol: Symbol) -> bool {
        if self.check_symbol(symbol) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn expect_symbol(&mut self, symbol: Symbol, message: impl Into<String>) -> bool {
        if self.match_symbol(symbol) {
            true
        } else {
            let token = self.current().clone();
            self.error_at(&token, message);
            false
        }
    }

    fn expect_identifier(&mut self, message: impl Into<String>) -> Option<Identifier> {
        match &self.current().kind {
            TokenKind::Ident(value) => {
                let value = Identifier::new(value.clone());
                self.advance();
                Some(value)
            }
            _ => {
                let token = self.current().clone();
                self.error_at(&token, message);
                None
            }
        }
    }

    fn expect_signed_integer(&mut self, message: impl Into<String>) -> i64 {
        let negative = self.match_symbol(Symbol::Minus);
        let token = self.current().clone();
        match token.kind {
            TokenKind::Number(ref value) => {
                self.advance();
                let parsed = value.replace('_', "").parse::<i64>().unwrap_or_else(|_| {
                    self.error_at(&token, "invalid integer literal");
                    0
                });
                if negative {
                    -parsed
                } else {
                    parsed
                }
            }
            _ => {
                self.error_at(&token, message);
                0
            }
        }
    }

    fn expect_unsigned_integer(&mut self, message: impl Into<String>) -> usize {
        let token = self.current().clone();
        match token.kind {
            TokenKind::Number(ref value) => {
                self.advance();
                value.replace('_', "").parse::<usize>().unwrap_or_else(|_| {
                    self.error_at(&token, "invalid unsigned integer literal");
                    0
                })
            }
            _ => {
                self.error_at(&token, message);
                0
            }
        }
    }

    fn synchronize_to_semicolon(&mut self) {
        while !self.is_eof() && !self.check_symbol(Symbol::Semicolon) {
            self.advance();
        }
        self.match_symbol(Symbol::Semicolon);
    }

    fn synchronize_to_keyword(&mut self, keyword: &str) {
        while !self.is_eof() && !self.check_keyword(keyword) {
            self.advance();
        }
    }

    fn error_at(&mut self, token: &Token, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::Syntax,
            message,
            Some(token.span.clone()),
        ));
    }

    fn parse_number_literal_token(&mut self, token: &Token, raw: &str) -> Expr {
        let (expr, diagnostics) = parse_number_literal_checked(raw);
        for message in diagnostics {
            self.error_at(token, message);
        }
        expr
    }

    fn parse_hash_literal_token(&mut self, token: &Token, raw: &str) -> Literal {
        let (literal, diagnostics) = parse_hash_literal_checked(raw);
        for message in diagnostics {
            self.error_at(token, message);
        }
        literal
    }
}

#[derive(Debug, Clone, Copy)]
enum PouStart {
    Function,
    FunctionBlock,
    Program,
}

struct TextualLadderRung {
    label: Option<String>,
    condition: Expr,
    statements: Vec<Statement>,
    nodes: Vec<NetworkNode>,
}

struct TextualFbdNetwork {
    label: Option<String>,
    statements: Vec<Statement>,
    nodes: Vec<NetworkNode>,
}

fn textual_network_node(
    kind: impl Into<String>,
    index: usize,
    attributes: &[(&str, String)],
) -> NetworkNode {
    let mut map = BTreeMap::new();
    map.insert("localId".to_string(), index.to_string());
    for (name, value) in attributes {
        map.insert((*name).to_string(), value.clone());
    }
    NetworkNode {
        id: index.to_string(),
        kind: kind.into(),
        attributes: map,
    }
}

fn expr_variable_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Variable(variable) => Some(variable.to_string()),
        _ => None,
    }
}

fn parse_number_literal_checked(raw: &str) -> (Expr, Vec<String>) {
    let mut diagnostics = Vec::new();
    if !valid_underscore_placement(raw) {
        diagnostics.push(format!(
            "invalid underscore placement in numeric literal '{raw}'"
        ));
    }
    let normalized = raw.replace('_', "");
    if normalized.contains('.') || normalized.contains('e') || normalized.contains('E') {
        match normalized.parse::<f64>() {
            Ok(value) if value.is_finite() => (Expr::Literal(Literal::Real(value)), diagnostics),
            _ => {
                diagnostics.push(format!("invalid real literal '{raw}'"));
                (Expr::Literal(Literal::Real(0.0)), diagnostics)
            }
        }
    } else {
        match normalized.parse::<i64>() {
            Ok(value) => (Expr::Literal(Literal::Int(value)), diagnostics),
            Err(_) => {
                diagnostics.push(format!("invalid integer literal '{raw}'"));
                (Expr::Literal(Literal::Int(0)), diagnostics)
            }
        }
    }
}

#[cfg(test)]
fn parse_hash_literal(raw: &str) -> Literal {
    parse_hash_literal_checked(raw).0
}

fn parse_hash_literal_checked(raw: &str) -> (Literal, Vec<String>) {
    let mut diagnostics = Vec::new();
    let Some((prefix, value)) = raw.split_once('#') else {
        diagnostics.push(format!("invalid typed literal '{raw}'"));
        return (
            Literal::Typed {
                type_name: Identifier::new("<literal>"),
                value: raw.to_string(),
            },
            diagnostics,
        );
    };
    let prefix_upper = canonical_identifier(prefix);

    let literal = match prefix_upper.as_str() {
        "TRUE" => Literal::Bool(true),
        "FALSE" => Literal::Bool(false),
        "BOOL" => match parse_bool_literal_value(value) {
            Some(value) => Literal::Bool(value),
            None => {
                diagnostics.push(format!("invalid BOOL literal value '{value}'"));
                Literal::Bool(false)
            }
        },
        "T" | "TIME" => match parse_duration_ms_checked(value) {
            Ok(value) => Literal::DurationMs(value),
            Err(message) => {
                diagnostics.push(message);
                Literal::DurationMs(0)
            }
        },
        "D" | "DATE" => {
            if parse_date_days(value).is_none() {
                diagnostics.push(format!("invalid DATE literal '{raw}'"));
            }
            Literal::Date(value.to_string())
        }
        "TOD" | "TIME_OF_DAY" => {
            if parse_time_of_day_ms(value).is_none() {
                diagnostics.push(format!("invalid TIME_OF_DAY literal '{raw}'"));
            }
            Literal::TimeOfDay(value.to_string())
        }
        "DT" | "DATE_AND_TIME" => {
            if parse_date_time_ms(value).is_none() {
                diagnostics.push(format!("invalid DATE_AND_TIME literal '{raw}'"));
            }
            Literal::DateAndTime(value.to_string())
        }
        "STRING" => Literal::String(decode_typed_character_string(
            raw,
            value,
            false,
            &mut diagnostics,
        )),
        "WSTRING" => Literal::WString(decode_typed_character_string(
            raw,
            value,
            true,
            &mut diagnostics,
        )),
        "2" => parse_based_int_literal(value, 2, raw, &mut diagnostics),
        "8" => parse_based_int_literal(value, 8, raw, &mut diagnostics),
        "16" => parse_based_int_literal(value, 16, raw, &mut diagnostics),
        _ => Literal::Typed {
            type_name: Identifier::new(prefix),
            value: normalize_typed_literal_value(prefix, value, &mut diagnostics)
                .unwrap_or_else(|| value.to_string()),
        },
    };
    (literal, diagnostics)
}

fn decode_typed_character_string(
    full_raw: &str,
    raw: &str,
    wide: bool,
    diagnostics: &mut Vec<String>,
) -> String {
    let Some(quote @ ('\'' | '"')) = raw.chars().next() else {
        diagnostics.push(format!("invalid typed string literal '{full_raw}'"));
        return raw.to_string();
    };
    if !raw.ends_with(quote) || raw.len() == quote.len_utf8() {
        diagnostics.push(format!("unterminated typed string literal '{full_raw}'"));
        return raw.trim_matches(quote).to_string();
    }

    let body = &raw[quote.len_utf8()..raw.len() - quote.len_utf8()];
    decode_character_string_body(full_raw, body, quote, wide, diagnostics)
}

fn decode_character_string_body(
    full_raw: &str,
    body: &str,
    _quote: char,
    wide: bool,
    diagnostics: &mut Vec<String>,
) -> String {
    let mut value = String::new();
    let mut chars = body.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '$' {
            if ch.is_control() {
                diagnostics.push(format!(
                    "unescaped control character {} in character string literal '{full_raw}'",
                    control_char_label(ch)
                ));
            }
            if !wide && (ch as u32) > 0xFF {
                diagnostics.push(format!(
                    "character {} exceeds single-byte STRING range in literal '{full_raw}'",
                    control_char_label(ch)
                ));
            }
            value.push(ch);
            continue;
        }

        let Some(escaped) = chars.peek().copied() else {
            diagnostics.push(format!(
                "unterminated character string escape in literal '{full_raw}'"
            ));
            break;
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
            chars.next();
            continue;
        }

        if escaped.is_ascii_hexdigit() {
            let required_digits = if wide { 4 } else { 2 };
            let mut digits = String::new();
            while digits.len() < required_digits {
                let Some(hex) = chars.peek().copied() else {
                    break;
                };
                if !hex.is_ascii_hexdigit() {
                    break;
                }
                digits.push(hex);
                chars.next();
            }
            if digits.len() != required_digits {
                diagnostics.push(format!(
                    "invalid character string hex escape '${digits}' in literal '{full_raw}': expected {required_digits} hexadecimal digit(s)"
                ));
                continue;
            }
            let code = u32::from_str_radix(&digits, 16).unwrap_or(0);
            if let Some(decoded) = char::from_u32(code) {
                value.push(decoded);
            } else {
                diagnostics.push(format!(
                    "invalid character code '${digits}' in literal '{full_raw}'"
                ));
            }
            continue;
        }

        diagnostics.push(format!(
            "invalid character string escape '${escaped}' in literal '{full_raw}'"
        ));
        chars.next();
    }
    value
}

fn parse_duration_ms_checked(raw: &str) -> Result<i128, String> {
    let mut chars = raw.replace('_', "").to_ascii_lowercase();
    let sign = if chars.starts_with('-') {
        chars.remove(0);
        -1_i128
    } else {
        1_i128
    };

    let mut rest = chars.as_str();
    if rest.is_empty() {
        return Err(format!("invalid duration literal 'T#{raw}'"));
    }

    let mut total = 0.0_f64;
    let mut previous_rank = 6_u8;
    let mut saw_component = false;
    while !rest.is_empty() {
        let number_len = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
            .map(char::len_utf8)
            .sum::<usize>();
        if number_len == 0 {
            return Err(format!("invalid duration literal 'T#{raw}'"));
        }
        let number_text = &rest[..number_len];
        if !valid_decimal_component(number_text) {
            return Err(format!("invalid duration component '{number_text}'"));
        }
        let number = number_text
            .parse::<f64>()
            .map_err(|_| format!("invalid duration component '{number_text}'"))?;
        rest = &rest[number_len..];
        let (factor, consumed, rank, unit) = if rest.starts_with("ms") {
            (1.0, 2, 1, "ms")
        } else if rest.starts_with('d') {
            (86_400_000.0, 1, 5, "d")
        } else if rest.starts_with('h') {
            (3_600_000.0, 1, 4, "h")
        } else if rest.starts_with('m') {
            (60_000.0, 1, 3, "m")
        } else if rest.starts_with('s') {
            (1_000.0, 1, 2, "s")
        } else {
            return Err(format!("invalid duration unit in 'T#{raw}'"));
        };
        if rank >= previous_rank {
            return Err(format!(
                "duration components must be ordered largest to smallest in 'T#{raw}'"
            ));
        }
        let has_more = rest.get(consumed..).is_some_and(|tail| !tail.is_empty());
        if has_more && number_text.contains('.') {
            return Err(format!(
                "fractional duration component '{number_text}{unit}' must be last"
            ));
        }
        if has_more || previous_rank != 6 {
            match unit {
                "h" if number >= 24.0 => {
                    return Err(format!("duration hours component {number_text} exceeds 23"));
                }
                "m" | "s" if number >= 60.0 => {
                    return Err(format!(
                        "duration {unit} component {number_text} exceeds 59"
                    ));
                }
                "ms" if number >= 1000.0 => {
                    return Err(format!(
                        "duration milliseconds component {number_text} exceeds 999"
                    ));
                }
                _ => {}
            }
        }
        saw_component = true;
        previous_rank = rank;
        total += number * factor;
        rest = &rest[consumed..];
    }

    if !saw_component {
        return Err(format!("invalid duration literal 'T#{raw}'"));
    }
    Ok(sign * total.round() as i128)
}

fn parse_bool_literal_value(raw: &str) -> Option<bool> {
    match canonical_identifier(raw).as_str() {
        "1" | "TRUE" => Some(true),
        "0" | "FALSE" => Some(false),
        _ => None,
    }
}

fn control_char_label(ch: char) -> String {
    format!("U+{:04X}", ch as u32)
}

fn parse_based_int_literal(
    raw: &str,
    base: u32,
    full_raw: &str,
    diagnostics: &mut Vec<String>,
) -> Literal {
    match parse_based_i64(raw, base) {
        Ok(value) => Literal::Int(value),
        Err(message) => {
            diagnostics.push(format!("{message} in literal '{full_raw}'"));
            Literal::Int(0)
        }
    }
}

fn normalize_typed_literal_value(
    type_name: &str,
    raw: &str,
    diagnostics: &mut Vec<String>,
) -> Option<String> {
    let elementary = ElementaryType::parse(type_name)?;
    match elementary {
        ElementaryType::Bool => parse_bool_literal_value(raw).map(|value| {
            if value {
                "1".to_string()
            } else {
                "0".to_string()
            }
        }),
        ElementaryType::Sint
        | ElementaryType::Int
        | ElementaryType::Dint
        | ElementaryType::Lint
        | ElementaryType::Usint
        | ElementaryType::Uint
        | ElementaryType::Udint
        | ElementaryType::Ulint
        | ElementaryType::Byte
        | ElementaryType::Word
        | ElementaryType::Dword
        | ElementaryType::Lword => match parse_integer_text_i128(raw) {
            Ok(value) => Some(value.to_string()),
            Err(message) => {
                diagnostics.push(format!("{message} in typed literal '{type_name}#{raw}'"));
                Some("0".to_string())
            }
        },
        ElementaryType::Real | ElementaryType::Lreal => {
            let normalized = raw.replace('_', "");
            if !valid_underscore_placement(raw) || normalized.parse::<f64>().is_err() {
                diagnostics.push(format!("invalid real typed literal '{type_name}#{raw}'"));
                Some("0.0".to_string())
            } else {
                Some(normalized)
            }
        }
        ElementaryType::Time => parse_duration_ms_checked(raw)
            .map(|value| value.to_string())
            .map_err(|message| diagnostics.push(message))
            .ok(),
        ElementaryType::Date | ElementaryType::TimeOfDay | ElementaryType::DateAndTime => None,
        ElementaryType::String | ElementaryType::WString => None,
    }
}

fn parse_integer_text_i128(raw: &str) -> Result<i128, String> {
    if let Some((base, digits)) = raw.split_once('#') {
        let base = match canonical_identifier(base).as_str() {
            "2" => 2,
            "8" => 8,
            "16" => 16,
            other => return Err(format!("unsupported integer base '{other}'")),
        };
        return parse_based_i128(digits, base);
    }
    if !valid_underscore_placement(raw) {
        return Err(format!(
            "invalid underscore placement in integer literal '{raw}'"
        ));
    }
    raw.replace('_', "")
        .parse::<i128>()
        .map_err(|_| format!("invalid integer literal '{raw}'"))
}

fn parse_based_i64(raw: &str, base: u32) -> Result<i64, String> {
    parse_based_i128(raw, base).and_then(|value| {
        i64::try_from(value).map_err(|_| format!("based literal '{raw}' is outside LINT range"))
    })
}

fn parse_based_i128(raw: &str, base: u32) -> Result<i128, String> {
    if !valid_underscore_placement(raw) {
        return Err(format!(
            "invalid underscore placement in based literal '{raw}'"
        ));
    }
    let digits = raw.replace('_', "");
    if digits.is_empty() {
        return Err("empty based literal".to_string());
    }
    if !digits.chars().all(|ch| ch.is_digit(base)) {
        return Err(format!("invalid base-{base} digit sequence '{raw}'"));
    }
    i128::from_str_radix(&digits, base).map_err(|_| format!("based literal '{raw}' is too large"))
}

fn valid_underscore_placement(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.first() == Some(&b'_') || bytes.last() == Some(&b'_') {
        return false;
    }
    !bytes.windows(2).any(|pair| pair == b"__")
}

fn valid_decimal_component(raw: &str) -> bool {
    if !valid_underscore_placement(raw) {
        return false;
    }
    let mut dot_count = 0_u8;
    let mut digit_count = 0_usize;
    for ch in raw.chars() {
        if ch == '.' {
            dot_count += 1;
            if dot_count > 1 {
                return false;
            }
        } else if ch.is_ascii_digit() || ch == '_' {
            if ch.is_ascii_digit() {
                digit_count += 1;
            }
        } else {
            return false;
        }
    }
    digit_count > 0
}

fn parse_date_time_ms(input: &str) -> Option<i128> {
    if input.len() < 11 {
        return None;
    }
    let date = parse_date_days(input.get(..10)?)? as i128;
    let separator = input.as_bytes().get(10).copied()?;
    if separator != b'-' && separator != b'T' && separator != b't' {
        return None;
    }
    Some(date * 86_400_000 + parse_time_of_day_ms(input.get(11..)?)?)
}

fn parse_date_days(input: &str) -> Option<i64> {
    let mut parts = input.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some()
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
    {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

fn parse_time_of_day_ms(input: &str) -> Option<i128> {
    let mut parts = input.split(':');
    let hour = parts.next()?.parse::<i128>().ok()?;
    let minute = parts.next()?.parse::<i128>().ok()?;
    let second_part = parts.next()?;
    if parts.next().is_some() || !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    let (second, millis) = if let Some((seconds, fraction)) = second_part.split_once('.') {
        if fraction.is_empty() || !fraction.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        let mut millis_text = fraction.chars().take(3).collect::<String>();
        while millis_text.len() < 3 {
            millis_text.push('0');
        }
        (
            seconds.parse::<i128>().ok()?,
            millis_text.parse::<i128>().ok()?,
        )
    } else {
        (second_part.parse::<i128>().ok()?, 0)
    };
    if !(0..=59).contains(&second) {
        return None;
    }
    Some(((hour * 60 + minute) * 60 + second) * 1000 + millis)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use iec_profile::ImplementationParameters;

    use super::*;

    #[test]
    fn parses_simple_program() {
        let source = r#"
            PROGRAM Demo
            VAR
                A : INT := 1;
                B : BOOL;
            END_VAR
            IF A < 5 THEN
                A := A + 1;
            ELSE
                B := TRUE;
            END_IF;
            END_PROGRAM
        "#;

        let output = parse_project("test.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.project.pous().count(), 1);
        let pou = output.project.first_program().unwrap();
        assert_eq!(pou.name.canonical, "DEMO");
        assert_eq!(pou.variable_declarations().count(), 2);
    }

    #[test]
    fn enforces_comment_and_pragma_implementation_limits() {
        let source = r#"
            { vendor_hint }
            PROGRAM Demo
            (* longer than the configured limit *)
            VAR A : INT; END_VAR
            END_PROGRAM
        "#;
        let output = parse_project_with_options(
            "limits.st",
            source,
            &ParseOptions {
                implementation: ImplementationParameters {
                    max_comment_length: 8,
                    pragmas_enabled: false,
                    ..ImplementationParameters::default()
                },
            },
        );
        assert!(output.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("pragmas are disabled by implementation parameters")));
        assert!(output
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("comment length")));

        let output = parse_project_with_options(
            "limits_ok.st",
            "{ vendor_hint } PROGRAM Demo VAR A : INT; END_VAR END_PROGRAM",
            &ParseOptions {
                implementation: ImplementationParameters {
                    pragmas_enabled: true,
                    ..ImplementationParameters::default()
                },
            },
        );
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
    }

    #[test]
    fn parses_duration_literal() {
        assert_eq!(parse_hash_literal("T#1s"), Literal::DurationMs(1000));
        assert_eq!(
            parse_hash_literal("TIME#2m_500ms"),
            Literal::DurationMs(120500)
        );
    }

    #[test]
    fn lexes_typed_enum_case_label_before_colon() {
        let source = r#"
            TYPE Mode : (Idle, Run); END_TYPE
            PROGRAM Demo
            VAR State : Mode := Idle; END_VAR
            CASE State OF
                Mode#Run: State := Idle;
            END_CASE;
            END_PROGRAM
        "#;
        let output = parse_project("typed_enum_case_label.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();
        let Statement::Case { cases, .. } = &pou.body.statements[0] else {
            panic!("expected CASE");
        };
        let CaseLabel::Single(Expr::Literal(Literal::Typed { type_name, value })) = &cases[0].0[0]
        else {
            panic!("expected typed enum literal label");
        };
        assert_eq!(type_name.original, "Mode");
        assert_eq!(value, "Run");
    }

    #[test]
    fn diagnoses_invalid_literal_forms() {
        let source = r#"
            PROGRAM BadLiterals
            VAR
                A : INT := 0;
                B : TIME := T#0ms;
                C : DATE := D#1970-01-01;
                D : TIME_OF_DAY := TOD#00:00:00;
                E : BOOL := FALSE;
                F : TIME := T#0ms;
            END_VAR
            A := 2#102;
            B := T#1h_75m;
            C := D#2023-02-29;
            D := TOD#24:00:00;
            E := BOOL#YES;
            F := T#1m_1h;
            F := T#1.5h_1m;
            F := T#1s_1000ms;
            END_PROGRAM
        "#;
        let output = parse_project("bad_literals.st", source);
        let messages = output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();

        assert!(messages
            .iter()
            .any(|message| message.contains("invalid base-2 digit sequence")));
        assert!(messages
            .iter()
            .any(|message| message.contains("duration m component 75 exceeds 59")));
        assert!(messages
            .iter()
            .any(|message| message
                .contains("duration components must be ordered largest to smallest")));
        assert!(messages
            .iter()
            .any(|message| message.contains("fractional duration component '1.5h' must be last")));
        assert!(messages
            .iter()
            .any(|message| message.contains("duration milliseconds component 1000 exceeds 999")));
        assert!(messages
            .iter()
            .any(|message| message.contains("invalid DATE literal")));
        assert!(messages
            .iter()
            .any(|message| message.contains("invalid TIME_OF_DAY literal")));
        assert!(messages
            .iter()
            .any(|message| message.contains("invalid BOOL literal value")));
    }

    #[test]
    fn parses_wstring_literals_distinct_from_string_literals() {
        let source = r#"
            PROGRAM WideText
            VAR
                Narrow : STRING[8] := 'robot';
                Wide : WSTRING[8] := "robot";
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("wstring.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();
        let vars = pou.variable_declarations().collect::<Vec<_>>();
        assert!(matches!(
            vars[0].initial_value,
            Some(Expr::Literal(Literal::String(_)))
        ));
        assert!(matches!(
            vars[1].initial_value,
            Some(Expr::Literal(Literal::WString(_)))
        ));
    }

    #[test]
    fn parses_iec_character_string_escapes() {
        let source = r#"
            PROGRAM Escapes
            VAR
                Narrow : STRING[8] := 'A$0A$27$$';
                Wide : WSTRING[8] := "$0041$000A$0022$$";
                TypedNarrow : STRING[8] := STRING#'OK$21';
                TypedWide : WSTRING[8] := WSTRING#'A$000A';
                NarrowQuoted : STRING[8] := 'A$"B$'';
                WideQuoted : WSTRING[8] := "A$'B$"";
                TypedNarrowQuoted : STRING[8] := STRING#'A$"B$'';
                TypedWideQuoted : WSTRING[8] := WSTRING#"A$'B$"";
                NamedEscapes : STRING[8] := '$L$N$P$R$T$$$'$"';
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("string_escapes.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();
        let vars = pou.variable_declarations().collect::<Vec<_>>();
        assert_eq!(
            vars[0].initial_value,
            Some(Expr::Literal(Literal::String("A\n'$".to_string())))
        );
        assert_eq!(
            vars[1].initial_value,
            Some(Expr::Literal(Literal::WString("A\n\"$".to_string())))
        );
        assert_eq!(
            vars[2].initial_value,
            Some(Expr::Literal(Literal::String("OK!".to_string())))
        );
        assert_eq!(
            vars[3].initial_value,
            Some(Expr::Literal(Literal::WString("A\n".to_string())))
        );
        assert_eq!(
            vars[4].initial_value,
            Some(Expr::Literal(Literal::String("A\"B'".to_string())))
        );
        assert_eq!(
            vars[5].initial_value,
            Some(Expr::Literal(Literal::WString("A'B\"".to_string())))
        );
        assert_eq!(
            vars[6].initial_value,
            Some(Expr::Literal(Literal::String("A\"B'".to_string())))
        );
        assert_eq!(
            vars[7].initial_value,
            Some(Expr::Literal(Literal::WString("A'B\"".to_string())))
        );
        assert_eq!(
            vars[8].initial_value,
            Some(Expr::Literal(Literal::String(
                "\n\n\u{000C}\r\t$'\"".to_string()
            )))
        );
    }

    #[test]
    fn diagnoses_invalid_character_string_escapes() {
        let source = r#"
            PROGRAM BadEscapes
            VAR
                BadCommon : STRING[8] := 'bad$Q';
                BadSingleHex : STRING[8] := 'bad$0G';
                BadWideHex : WSTRING[8] := "$00Q1";
                BadLine : STRING[16] := 'bad
line';
                BadTypedLine : STRING[16] := STRING#'bad
line';
                BadSingleByte : STRING[8] := 'badλ';
                BadTypedSingleByte : STRING[8] := STRING#'badλ';
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("bad_string_escapes.st", source);
        let messages = output
            .diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect::<Vec<_>>();
        assert!(messages
            .iter()
            .any(|message| message.contains("invalid character string escape '$Q'")));
        assert!(messages.iter().any(|message| message.contains(
            "invalid character string hex escape '$0': expected 2 hexadecimal digit(s)"
        )));
        assert!(messages.iter().any(|message| message.contains(
            "invalid character string hex escape '$00': expected 4 hexadecimal digit(s)"
        )));
        assert_eq!(
            messages
                .iter()
                .filter(|message| message
                    .contains("unescaped control character U+000A in character string literal"))
                .count(),
            2
        );
        assert_eq!(
            messages
                .iter()
                .filter(
                    |message| message.contains("character U+03BB exceeds single-byte STRING range")
                )
                .count(),
            2
        );
    }

    #[test]
    fn parses_derived_types_and_control_flow() {
        let source = r#"
            TYPE
                Speed : INT := 0;
                Mode : (Idle, Run, Fault);
                Window : STRUCT
                    Low : INT := 1;
                    High : INT := 10;
                END_STRUCT;
                Buffer : ARRAY [1..4] OF INT;
            END_TYPE

            PROGRAM Demo
            VAR
                I : INT := 0;
                Total : INT := 0;
                Done : BOOL := FALSE;
                Values : ARRAY [1..3] OF INT := [1, 2, 3];
                Limits : Window := (Low := 2, High := 8);
            END_VAR

            FOR I := 1 TO 3 DO
                Total := Total + I;
            END_FOR;

            WHILE Total < 10 DO
                Total := Total + 1;
            END_WHILE;

            REPEAT
                Total := Total - 1;
            UNTIL Total = 9
            END_REPEAT;

            CASE Total OF
                0..8: Done := FALSE;
                9: Done := TRUE;
                ELSE Done := FALSE;
            END_CASE;
            END_PROGRAM
        "#;

        let output = parse_project("control.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        assert_eq!(output.project.data_types().count(), 4);
        let pou = output.project.first_program().unwrap();
        assert!(pou
            .variable_declarations()
            .any(|var| matches!(var.initial_value, Some(Expr::ArrayLiteral(_)))));
        assert!(pou
            .variable_declarations()
            .any(|var| matches!(var.initial_value, Some(Expr::StructLiteral(_)))));
        assert_eq!(pou.body.statements.len(), 4);
    }

    #[test]
    fn parses_repeated_array_initializers() {
        let source = r#"
            PROGRAM Demo
            VAR
                Values : ARRAY [1..5] OF INT := [2(1), 3(5)];
            END_VAR
            END_PROGRAM
        "#;

        let output = parse_project("array_repeat.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();
        let values = pou
            .variable_declarations()
            .find(|var| var.name.canonical == "VALUES")
            .and_then(|var| var.initial_value.as_ref())
            .expect("Values initializer should parse");
        let Expr::ArrayLiteral(elements) = values else {
            panic!("expected array literal");
        };
        assert_eq!(elements.len(), 5);
    }

    #[test]
    fn parses_basic_instruction_list_statements() {
        let source = r#"
            PROGRAM IlDemo
            VAR
                A : INT := 1;
                B : INT := 2;
                C : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR
            LD A;
            ADD B;
            Done:
            ST C;
            LD TRUE;
            AND (C > 0);
            ST Flag;
            END_PROGRAM
        "#;

        let output = parse_project("il.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();
        assert!(matches!(pou.body.statements[0], Statement::Il { .. }));
        assert!(matches!(pou.body.statements[2], Statement::IlLabel(_)));
        assert_eq!(pou.body.statements.len(), 7);
    }

    #[test]
    fn parses_line_oriented_instruction_list_without_semicolons() {
        let source = r#"
            PROGRAM LineIlDemo
            VAR
                A : INT := 3;
                B : INT := 4;
                C : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            LD A
            ADD B
            Done:
            ST C
            LD TRUE
            AND (C > 0)
            ST Flag
            END_PROGRAM
        "#;

        let output = parse_project("line_il.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let statements = &output.project.first_program().unwrap().body.statements;
        assert_eq!(statements.len(), 7);
        assert!(matches!(statements[0], Statement::Il { op: IlOp::Ld, .. }));
        assert!(matches!(statements[2], Statement::IlLabel(_)));
        assert!(matches!(statements[6], Statement::Il { op: IlOp::St, .. }));
    }

    #[test]
    fn parses_typed_instruction_list_mnemonics() {
        let source = r#"
            PROGRAM TypedIlDemo
            VAR
                A : INT := 3;
                B : INT := 4;
                C : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            LD_INT A;
            ADD_INT B;
            ST_INT C;
            LD_BOOL TRUE;
            AND_BOOL (C = 7);
            ST_BOOL Flag;
            END_PROGRAM
        "#;

        let output = parse_project("typed_il.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let statements = &output.project.first_program().unwrap().body.statements;
        assert!(matches!(statements[0], Statement::Il { op: IlOp::Ld, .. }));
        assert!(matches!(statements[1], Statement::Il { op: IlOp::Add, .. }));
        assert!(matches!(statements[5], Statement::Il { op: IlOp::St, .. }));
    }

    #[test]
    fn parses_instruction_list_parenthesized_expression_lists() {
        let source = r#"
            PROGRAM NestedIlDemo
            VAR
                A : BOOL := TRUE;
                B : BOOL := FALSE;
                C : BOOL := TRUE;
                Out : BOOL := FALSE;
            END_VAR

            LD TRUE
            AND (
                LD A
                OR (
                    LD B
                    ANDN C
                )
            )
            ST Out
            END_PROGRAM
        "#;

        let output = parse_project("nested_il.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let statements = &output.project.first_program().unwrap().body.statements;
        let Statement::Il {
            op: IlOp::And,
            operand: Some(Expr::Binary { op, .. }),
        } = &statements[1]
        else {
            panic!("expected nested IL expression to lower into a binary operand");
        };
        assert_eq!(*op, BinaryOp::Or);
    }

    #[test]
    fn parses_configuration_resources_tasks_and_program_instances() {
        let source = r#"
            PROGRAM Demo
            VAR A : INT := 0; END_VAR
            A := A + 1;
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                Shared : INT := 1;
            END_VAR
            VAR_ACCESS
                SharedAccess : Shared : INT READ_WRITE;
            END_VAR
            RESOURCE Cpu ON PLC
                VAR_CONFIG
                    Tunable : INT := 2;
                END_VAR
                VAR_ACCESS
                    InputBit : %IX1.1 : BOOL READ_ONLY;
                    MainA : Main.A : INT;
                END_VAR
                TASK Fast(SINGLE := Shared > 0, INTERVAL := T#10ms, PRIORITY := 1);
                PROGRAM Main WITH Fast : Demo(A := 5);
            END_RESOURCE
            END_CONFIGURATION
        "#;

        let output = parse_project("config.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let configuration = output
            .project
            .library_elements
            .iter()
            .find_map(|element| {
                if let LibraryElement::Configuration(configuration) = element {
                    Some(configuration)
                } else {
                    None
                }
            })
            .unwrap();
        assert_eq!(configuration.var_blocks.len(), 2);
        assert_eq!(configuration.resources.len(), 1);
        assert_eq!(configuration.resources[0].var_blocks.len(), 2);
        assert_eq!(configuration.resources[0].tasks.len(), 1);
        assert!(matches!(
            configuration.resources[0].tasks[0].single,
            Some(Expr::Binary {
                op: BinaryOp::Greater,
                ..
            })
        ));
        assert_eq!(configuration.resources[0].program_instances.len(), 1);
        assert_eq!(
            configuration.resources[0].program_instances[0].args.len(),
            1
        );
        let access = &configuration.var_blocks[1].vars[0].access;
        assert!(matches!(
            access.as_ref().map(|access| access.direction),
            Some(AccessDirection::ReadWrite)
        ));
        assert_eq!(
            configuration.resources[0].var_blocks[1].vars[0]
                .access
                .as_ref()
                .map(|access| access.path.as_str()),
            Some("%IX1.1")
        );
    }

    #[test]
    fn parses_textual_sfc_body() {
        let source = r#"
            PROGRAM Sequence
            VAR
                Ready : BOOL := TRUE;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start:
                MarkDone(P);
            END_STEP;
            Running: STEP
                MarkDone(L, T#5ms);
            END_STEP;
            STEP DoneStep;
            Go: TRANSITION FROM Start TO (Running, DoneStep) := Ready;
            END_TRANSITION;
            MarkDone: ACTION (L, T#5ms)
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;

        let output = parse_project("sfc.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();
        assert_eq!(
            pou.body.language,
            ImplementationLanguage::SequentialFunctionChart
        );
        let sfc = pou.body.sfc.as_ref().unwrap();
        assert_eq!(sfc.steps.len(), 3);
        assert!(sfc.steps[0].initial);
        assert_eq!(sfc.steps[0].actions.len(), 1);
        assert_eq!(sfc.steps[0].actions[0].name.canonical, "MARKDONE");
        assert_eq!(
            sfc.steps[0].actions[0].qualifier,
            Some(SfcActionQualifier::Pulse)
        );
        assert_eq!(sfc.steps[1].name.canonical, "RUNNING");
        assert_eq!(sfc.steps[1].actions.len(), 1);
        assert_eq!(
            sfc.steps[1].actions[0].qualifier,
            Some(SfcActionQualifier::TimeLimited)
        );
        assert_eq!(
            sfc.steps[1].actions[0].duration,
            Some(Literal::DurationMs(5))
        );
        assert_eq!(sfc.transitions.len(), 1);
        assert_eq!(
            sfc.transitions[0]
                .name
                .as_ref()
                .map(|name| name.canonical.as_str()),
            Some("GO")
        );
        assert_eq!(sfc.transitions[0].from[0].canonical, "START");
        assert_eq!(sfc.transitions[0].to.len(), 2);
        assert_eq!(sfc.transitions[0].to[1].canonical, "DONESTEP");
        assert_eq!(sfc.actions.len(), 1);
        assert_eq!(sfc.actions[0].qualifier, SfcActionQualifier::TimeLimited);
        assert_eq!(sfc.actions[0].duration, Some(Literal::DurationMs(5)));
        assert_eq!(sfc.actions[0].body.len(), 1);
    }

    #[test]
    fn parses_textual_sfc_il_transition_bodies() {
        let source = r#"
            PROGRAM Sequence
            VAR
                Count : INT := 2;
            END_VAR
            INITIAL_STEP Start;
            STEP Done;
            TRANSITION FROM Start TO Done:
                LD Count
                GE 2
            END_TRANSITION;
            END_PROGRAM
        "#;

        let output = parse_project("sfc_il_transition_body.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();
        let transition = &pou.body.sfc.as_ref().unwrap().transitions[0];
        assert!(matches!(
            transition.condition,
            Some(Expr::Binary {
                op: BinaryOp::GreaterEqual,
                ..
            })
        ));
    }

    #[test]
    fn parses_native_textual_ladder_and_fbd_bodies() {
        let ladder = r#"
            PROGRAM NativeLd
            VAR
                Start : BOOL := TRUE;
                Stop : BOOL := FALSE;
                Motor : BOOL := FALSE;
                Latched : BOOL := FALSE;
            END_VAR
            LADDER
            RUNG MotorRun:
                CONTACT Start;
                CONTACT_NOT Stop;
                COIL Motor;
            END_RUNG;
            RUNG Latch:
                CONTACT Start;
                SET Latched;
            END_RUNG;
            END_LADDER
            END_PROGRAM
        "#;
        let output = parse_project("native_ladder.ld", ladder);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();
        assert_eq!(pou.body.language, ImplementationLanguage::LadderDiagram);
        assert_eq!(pou.body.networks.len(), 2);
        assert_eq!(pou.body.statements.len(), 2);
        assert!(matches!(
            pou.body.statements.first(),
            Some(Statement::Assignment { target, .. }) if target.to_string() == "Motor"
        ));

        let fbd = r#"
            PROGRAM NativeFbd
            VAR
                A : INT := 2;
                B : INT := 3;
                C : INT := 0;
                Ready : BOOL := FALSE;
            END_VAR
            FBD
            NETWORK Sum:
                OUT C := ADD(A, B);
                OUT Ready := C >= 5;
            END_NETWORK;
            END_FBD
            END_PROGRAM
        "#;
        let output = parse_project("native_fbd.fbd", fbd);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();
        assert_eq!(
            pou.body.language,
            ImplementationLanguage::FunctionBlockDiagram
        );
        assert_eq!(pou.body.networks.len(), 1);
        assert_eq!(pou.body.statements.len(), 2);
    }

    #[test]
    fn parses_native_ld_and_fbd_sfc_transition_bodies() {
        let source = r#"
            PROGRAM Sequence
            VAR
                Count : INT := 2;
            END_VAR
            INITIAL_STEP Start;
            STEP Middle;
            STEP Done;
            TRANSITION FROM Start TO Middle:
                LADDER
                RUNG Ready:
                    CONTACT Count >= 2;
                END_RUNG;
                END_LADDER
            END_TRANSITION;
            TRANSITION FROM Middle TO Done:
                FBD
                NETWORK Ready:
                    OUT := Count >= 2;
                END_NETWORK;
                END_FBD
            END_TRANSITION;
            END_PROGRAM
        "#;

        let output = parse_project("sfc_native_ld_fbd_transition.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();
        let transitions = &pou.body.sfc.as_ref().unwrap().transitions;
        assert_eq!(transitions.len(), 2);
        assert!(matches!(
            transitions[0].condition,
            Some(Expr::Binary {
                op: BinaryOp::And,
                ..
            })
        ));
        assert!(matches!(
            transitions[1].condition,
            Some(Expr::Binary {
                op: BinaryOp::GreaterEqual,
                ..
            })
        ));
    }

    #[test]
    fn diagnoses_unsupported_statements_during_parsing() {
        let source = r#"
            PROGRAM BadStatement
            VAR
                A : INT;
            END_VAR
            GOTO Somewhere;
            END_PROGRAM
        "#;

        let output = parse_project("unsupported_statement.st", source);
        assert!(output.diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("unsupported or invalid statement")));
        let pou = output.project.first_program().unwrap();
        assert!(matches!(
            pou.body.statements.first(),
            Some(Statement::Unsupported(_))
        ));
    }

    #[test]
    fn parses_function_block_input_edge_qualifiers() {
        let source = r#"
            FUNCTION_BLOCK EdgeInputs
            VAR_INPUT
                Start : BOOL R_EDGE;
                Stop : BOOL F_EDGE;
            END_VAR
            END_FUNCTION_BLOCK
        "#;

        let output = parse_project("edge_qualifiers.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.find_pou("EdgeInputs").unwrap();
        let vars = pou.variable_declarations().collect::<Vec<_>>();
        assert_eq!(vars[0].edge, Some(EdgeQualifier::Rising));
        assert_eq!(vars[1].edge, Some(EdgeQualifier::Falling));
    }

    #[test]
    fn parser_golden_corpus_covers_language_families() {
        let cases = [
            (
                "structured-text",
                r#"
                PROGRAM Demo
                VAR A : INT := 0; END_VAR
                IF A < 5 THEN A := A + 1; END_IF;
                END_PROGRAM
                "#,
                ImplementationLanguage::StructuredText,
            ),
            (
                "data-types",
                r#"
                TYPE
                    Percent : INT(0..100);
                    Mode : (Idle, Armed, Fault);
                    Pair : STRUCT
                        Low : Percent := 1;
                        High : Percent := 99;
                    END_STRUCT;
                    Samples : ARRAY [1..2, 0..1] OF Percent;
                END_TYPE
                PROGRAM Demo
                VAR
                    Window : Pair := (Low := 10, High := 20);
                    State : Mode := Armed;
                    Grid : Samples := [1, 2, 3, 4];
                END_VAR
                Window.Low := Grid[1, 0];
                END_PROGRAM
                "#,
                ImplementationLanguage::StructuredText,
            ),
            (
                "functions",
                r#"
                FUNCTION Scale : INT
                VAR_INPUT
                    Input : INT;
                    Factor : INT;
                END_VAR
                VAR_TEMP
                    Temp : INT;
                END_VAR
                Temp := Input * Factor;
                Scale := Temp;
                RETURN;
                END_FUNCTION

                PROGRAM Demo
                VAR
                    Out : INT := 0;
                END_VAR
                Out := Scale(Input := 2, Factor := 3);
                END_PROGRAM
                "#,
                ImplementationLanguage::StructuredText,
            ),
            (
                "function-blocks",
                r#"
                FUNCTION_BLOCK Accumulator
                VAR_INPUT
                    Enable : BOOL;
                END_VAR
                VAR_IN_OUT
                    Total : INT;
                END_VAR
                VAR_OUTPUT
                    Done : BOOL;
                END_VAR
                IF Enable THEN
                    Total := Total + 1;
                END_IF;
                Done := Total >= 2;
                END_FUNCTION_BLOCK

                PROGRAM Demo
                VAR
                    Fb : Accumulator;
                    Count : INT := 0;
                    Done : BOOL := FALSE;
                END_VAR
                Fb(Enable := TRUE, Total := Count, Done => Done);
                END_PROGRAM
                "#,
                ImplementationLanguage::StructuredText,
            ),
            (
                "instruction-list",
                r#"
                PROGRAM Demo
                VAR A : INT := 0; END_VAR
                LD 1;
                ST A;
                END_PROGRAM
                "#,
                ImplementationLanguage::StructuredText,
            ),
            (
                "textual-sfc",
                r#"
                PROGRAM Demo
                VAR Ready : BOOL := TRUE; END_VAR
                INITIAL_STEP Start;
                STEP Run;
                TRANSITION T1 := Ready;
                ACTION Run(P):
                    Ready := FALSE;
                END_ACTION;
                END_PROGRAM
                "#,
                ImplementationLanguage::SequentialFunctionChart,
            ),
            (
                "configuration",
                r#"
                PROGRAM Demo END_PROGRAM
                CONFIGURATION Plant
                RESOURCE Cpu ON PLC
                    TASK Fast(INTERVAL := T#10ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Demo;
                END_RESOURCE
                END_CONFIGURATION
                "#,
                ImplementationLanguage::StructuredText,
            ),
        ];

        for (name, source, expected_language) in cases {
            let output = parse_project(format!("golden_{name}.st"), source);
            assert!(
                output.diagnostics.is_empty(),
                "{name}: {:?}",
                output.diagnostics
            );
            let pou = output
                .project
                .first_program()
                .expect("program should parse");
            assert_eq!(pou.body.language, expected_language, "{name}");
        }
    }

    #[test]
    fn pseudo_fuzz_corpus_covers_literals_comments_identifiers_and_precedence() {
        let mut seed = 0x6113_1200_3_u64;

        for index in 0..96_u64 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let a = (seed & 0x3f) as i64;
            let b = ((seed >> 8) & 0x1f) as i64 + 1;
            let c = ((seed >> 16) & 0x0f) as i64;
            let comment = if index % 3 == 0 {
                "(* generated literal/comment case *)"
            } else if index % 3 == 1 {
                "(* alternate generated comment *)"
            } else {
                ""
            };
            let source = format!(
                r#"
                {comment}
                PROGRAM Fuzz{index}
                VAR
                    Value_{index} : DINT := {a};
                    Flag_{index} : BOOL := FALSE;
                    Text_{index} : STRING[32] := 'seed$20{index}';
                END_VAR
                Value_{index} := (({a} + {b}) * ({c} + 1)) - ({b} ** 2);
                Flag_{index} := (Value_{index} >= 0) AND NOT FALSE;
                Text_{index} := CONCAT(LEFT(Text_{index}, 4), RIGHT('robot', 2));
                END_PROGRAM
                "#
            );
            let output = parse_project(format!("pseudo_fuzz_{index}.st"), &source);
            assert!(
                output.diagnostics.is_empty(),
                "case {index}: {:?}",
                output.diagnostics
            );
            let pou = output.project.first_program().unwrap();
            assert_eq!(pou.name.canonical, format!("FUZZ{index}"));
            assert_eq!(pou.body.statements.len(), 3);
        }
    }

    #[test]
    fn literal_property_corpus_parses_generated_values() {
        for index in 0..64_i64 {
            let signed = index * 257 - 4096;
            let duration = index * 17 + 1;
            let source = format!(
                r#"
                PROGRAM Literals
                VAR
                    A : LINT := 0;
                    B : REAL := 0.0;
                    C : TIME := T#0ms;
                    D : STRING[32] := '';
                END_VAR
                A := {signed};
                B := {index}.25;
                C := T#{duration}ms;
                D := 'case_{index}';
                END_PROGRAM
                "#
            );
            let output = parse_project(format!("literal_property_{index}.st"), &source);
            assert!(
                output.diagnostics.is_empty(),
                "case {index}: {:?}",
                output.diagnostics
            );
            let pou = output.project.first_program().unwrap();
            assert_eq!(pou.body.statements.len(), 4);
        }
    }

    #[test]
    fn comment_and_identifier_property_corpus_parses_generated_programs() {
        let identifier_cases = [
            ("CamelCase", "CAMELCASE"),
            ("snake_case", "SNAKE_CASE"),
            ("MIXED_123_Name", "MIXED_123_NAME"),
            ("CaseFold", "CASEFOLD"),
        ];
        let comment_cases = [
            ("(* leading block comment *)", ""),
            ("", "(* trailing statement comment *)"),
            ("(* implementation note *)", ""),
            ("", "(* after assignment *)"),
        ];

        for (index, (name, canonical)) in identifier_cases.into_iter().enumerate() {
            let (prefix_comment, suffix_comment) = comment_cases[index];
            let source = format!(
                r#"
                {prefix_comment}
                PROGRAM Commented
                VAR
                    {name} : INT := 0;
                END_VAR
                {name} := {name} + 1; {suffix_comment}
                END_PROGRAM
                "#
            );
            let output = parse_project(format!("comment_identifier_{index}.st"), &source);
            assert!(
                output.diagnostics.is_empty(),
                "case {index}: {:?}",
                output.diagnostics
            );
            let pou = output.project.first_program().unwrap();
            let var = pou.variable_declarations().next().unwrap();
            assert_eq!(var.name.original, name);
            assert_eq!(var.name.canonical, canonical);
            assert_eq!(pou.body.statements.len(), 1);
        }
    }

    #[test]
    fn operator_precedence_property_corpus_builds_expected_ast_shapes() {
        let source = r#"
            PROGRAM Precedence
            VAR
                A : INT := 0;
                B : BOOL := FALSE;
                C : INT := 0;
            END_VAR
            A := 1 + 2 * 3 ** 2;
            B := TRUE OR FALSE XOR TRUE AND NOT FALSE;
            C := +1 + +(+2);
            END_PROGRAM
        "#;
        let output = parse_project("precedence_property.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let pou = output.project.first_program().unwrap();

        let Statement::Assignment { value: numeric, .. } = &pou.body.statements[0] else {
            panic!("first statement should be assignment");
        };
        assert_binary_shape(numeric, BinaryOp::Add);
        let Expr::Binary { right, .. } = numeric else {
            unreachable!();
        };
        assert_binary_shape(right, BinaryOp::Mul);
        let Expr::Binary { right, .. } = right.as_ref() else {
            unreachable!();
        };
        assert_binary_shape(right, BinaryOp::Power);

        let Statement::Assignment { value: boolean, .. } = &pou.body.statements[1] else {
            panic!("second statement should be assignment");
        };
        assert_binary_shape(boolean, BinaryOp::Or);
        let Expr::Binary { right, .. } = boolean else {
            unreachable!();
        };
        assert_binary_shape(right, BinaryOp::Xor);
        let Expr::Binary { right, .. } = right.as_ref() else {
            unreachable!();
        };
        assert_binary_shape(right, BinaryOp::And);
        let Expr::Binary { right, .. } = right.as_ref() else {
            unreachable!();
        };
        assert!(matches!(
            right.as_ref(),
            Expr::Unary {
                op: UnaryOp::Not,
                ..
            }
        ));

        let Statement::Assignment { value: plus, .. } = &pou.body.statements[2] else {
            panic!("third statement should be assignment");
        };
        assert_binary_shape(plus, BinaryOp::Add);
    }

    fn assert_binary_shape(expr: &Expr, expected: BinaryOp) {
        assert!(
            matches!(expr, Expr::Binary { op, .. } if *op == expected),
            "expected {expected:?}, got {expr:?}"
        );
    }
}
