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
            self.advance();
            self.consume_hash_literal_tail();
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
            self.consume_hash_literal_tail();
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

    fn consume_hash_literal_tail(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() || matches!(ch, ';' | ',' | ')' | ']' | '(' | '[') {
                break;
            }
            self.advance();
        }
    }

    fn lex_string(&mut self, start: usize, quote: char) -> Token {
        self.advance();
        let mut value = String::new();
        while let Some(ch) = self.peek() {
            if ch == quote {
                self.advance();
                return self.token(start, TokenKind::StringLiteral(value));
            }

            if ch == '$' {
                self.advance();
                if let Some(escaped) = self.peek() {
                    let decoded = match escaped {
                        '$' => '$',
                        '\'' => '\'',
                        '"' => '"',
                        'L' | 'l' | 'N' | 'n' => '\n',
                        'P' | 'p' => '\u{000C}',
                        'R' | 'r' => '\r',
                        'T' | 't' => '\t',
                        other => other,
                    };
                    value.push(decoded);
                    self.advance();
                }
            } else {
                value.push(ch);
                self.advance();
            }
        }

        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::Lexical,
            "unterminated string literal",
            Some(Span::new(&self.source_name, start, self.pos, self.source)),
        ));
        self.token(start, TokenKind::StringLiteral(value))
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

struct Parser<'a> {
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: DiagnosticBag,
    _phantom: std::marker::PhantomData<&'a ()>,
}

impl<'a> Parser<'a> {
    fn new(_source_name: String, _source: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            pos: 0,
            diagnostics: DiagnosticBag::new(),
            _phantom: std::marker::PhantomData,
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
        let body = if self.is_sfc_statement_start() {
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

    fn parse_sfc_body(&mut self, end_keyword: &str) -> Sfc {
        let mut sfc = Sfc {
            steps: Vec::new(),
            transitions: Vec::new(),
            actions: Vec::new(),
        };

        while !self.is_eof() && !self.check_keyword(end_keyword) {
            if self.match_keyword("INITIAL_STEP") {
                let name = self
                    .expect_identifier("expected initial step name")
                    .unwrap_or_else(|| Identifier::new("<error>"));
                self.expect_symbol(Symbol::Semicolon, "expected ';' after initial step");
                sfc.steps.push(SfcStep {
                    name,
                    initial: true,
                });
            } else if self.match_keyword("STEP") {
                let name = self
                    .expect_identifier("expected step name")
                    .unwrap_or_else(|| Identifier::new("<error>"));
                self.expect_symbol(Symbol::Semicolon, "expected ';' after step");
                sfc.steps.push(SfcStep {
                    name,
                    initial: false,
                });
            } else if self.match_keyword("TRANSITION") {
                let name =
                    if self.current_identifier().is_some() && self.peek_symbol(Symbol::Assign) {
                        let name = self.current_identifier();
                        self.advance();
                        name
                    } else {
                        None
                    };
                self.expect_symbol(Symbol::Assign, "expected ':=' in transition");
                let condition = Some(self.parse_expression());
                self.expect_symbol(Symbol::Semicolon, "expected ';' after transition");
                sfc.transitions.push(SfcTransition { name, condition });
            } else if self.match_keyword("ACTION") {
                let name = self
                    .expect_identifier("expected action name")
                    .unwrap_or_else(|| Identifier::new("<error>"));
                let (qualifier, duration) = self.parse_sfc_action_qualifier();
                self.expect_symbol(Symbol::Colon, "expected ':' after action name");
                let body = self.parse_statement_list(&["END_ACTION"]);
                self.expect_keyword("END_ACTION", "expected END_ACTION");
                self.match_symbol(Symbol::Semicolon);
                sfc.actions.push(SfcAction {
                    name,
                    qualifier,
                    duration,
                    body,
                });
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
            vars.extend(self.parse_var_decl());
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
                type_spec: type_spec.clone(),
                initial_value: initial_value.clone(),
            })
            .collect()
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
                    "INTERVAL" => {
                        if let Expr::Literal(literal) = value {
                            interval = Some(literal);
                        }
                    }
                    "PRIORITY" => {
                        if let Expr::Literal(Literal::Int(value)) = value {
                            priority = Some(value.max(0) as u32);
                        }
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
        if self.match_symbol(Symbol::LParen) {
            while !self.is_eof() && !self.match_symbol(Symbol::RParen) {
                self.advance();
            }
        }
        self.expect_symbol(
            Symbol::Semicolon,
            "expected ';' after PROGRAM instance declaration",
        );

        ProgramInstance {
            name,
            program_type,
            task,
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

        if self.current_il_op().is_some() && !self.peek_symbol(Symbol::Assign) {
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
            Some(self.parse_expression())
        } else {
            None
        };

        Statement::Il { op, operand }
    }

    fn current_il_op(&self) -> Option<IlOp> {
        let op = self.current_ident_upper()?;
        match op.as_str() {
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

    fn parse_or(&mut self) -> Expr {
        let mut expr = self.parse_xor();
        while self.match_keyword("OR") {
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
        while self.match_keyword("XOR") {
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
        while self.match_keyword("AND") || self.match_symbol(Symbol::Amp) {
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
        if self.match_symbol(Symbol::Power) {
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
            TokenKind::Number(value) => {
                self.advance();
                parse_number_literal(&value)
            }
            TokenKind::StringLiteral(value) => {
                self.advance();
                Expr::Literal(Literal::String(value))
            }
            TokenKind::HashLiteral(value) => {
                self.advance();
                Expr::Literal(parse_hash_literal(&value))
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
            elements.push(self.parse_expression());
            if !self.match_symbol(Symbol::Comma) {
                break;
            }
        }
        self.expect_symbol(Symbol::RBracket, "expected ']' after array literal");
        Expr::ArrayLiteral(elements)
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
        )
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

    fn error_at(&mut self, token: &Token, message: impl Into<String>) {
        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::Syntax,
            message,
            Some(token.span.clone()),
        ));
    }
}

#[derive(Debug, Clone, Copy)]
enum PouStart {
    Function,
    FunctionBlock,
    Program,
}

fn parse_number_literal(raw: &str) -> Expr {
    let normalized = raw.replace('_', "");
    if normalized.contains('.') || normalized.contains('e') || normalized.contains('E') {
        Expr::Literal(Literal::Real(normalized.parse::<f64>().unwrap_or(0.0)))
    } else {
        Expr::Literal(Literal::Int(normalized.parse::<i64>().unwrap_or(0)))
    }
}

fn parse_hash_literal(raw: &str) -> Literal {
    let Some((prefix, value)) = raw.split_once('#') else {
        return Literal::Typed {
            type_name: Identifier::new("<literal>"),
            value: raw.to_string(),
        };
    };
    let prefix_upper = canonical_identifier(prefix);

    match prefix_upper.as_str() {
        "TRUE" => Literal::Bool(true),
        "FALSE" => Literal::Bool(false),
        "BOOL" => Literal::Bool(matches!(value, "1" | "TRUE" | "true")),
        "T" | "TIME" => Literal::DurationMs(parse_duration_ms(value)),
        "D" | "DATE" => Literal::Date(value.to_string()),
        "TOD" | "TIME_OF_DAY" => Literal::TimeOfDay(value.to_string()),
        "DT" | "DATE_AND_TIME" => Literal::DateAndTime(value.to_string()),
        "2" => Literal::Int(i64::from_str_radix(&value.replace('_', ""), 2).unwrap_or(0)),
        "8" => Literal::Int(i64::from_str_radix(&value.replace('_', ""), 8).unwrap_or(0)),
        "16" => Literal::Int(i64::from_str_radix(&value.replace('_', ""), 16).unwrap_or(0)),
        _ => Literal::Typed {
            type_name: Identifier::new(prefix),
            value: value.to_string(),
        },
    }
}

fn parse_duration_ms(raw: &str) -> i128 {
    let mut chars = raw.replace('_', "").to_ascii_lowercase();
    let sign = if chars.starts_with('-') {
        chars.remove(0);
        -1_i128
    } else {
        1_i128
    };

    let mut rest = chars.as_str();
    let mut total = 0.0_f64;
    while !rest.is_empty() {
        let number_len = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
            .map(char::len_utf8)
            .sum::<usize>();
        if number_len == 0 {
            break;
        }
        let number = rest[..number_len].parse::<f64>().unwrap_or(0.0);
        rest = &rest[number_len..];
        let (factor, consumed) = if rest.starts_with("ms") {
            (1.0, 2)
        } else if rest.starts_with('d') {
            (86_400_000.0, 1)
        } else if rest.starts_with('h') {
            (3_600_000.0, 1)
        } else if rest.starts_with('m') {
            (60_000.0, 1)
        } else if rest.starts_with('s') {
            (1_000.0, 1)
        } else {
            (1.0, 0)
        };
        total += number * factor;
        if consumed == 0 {
            break;
        }
        rest = &rest[consumed..];
    }

    sign * total.round() as i128
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
            RESOURCE Cpu ON PLC
                VAR_CONFIG
                    Tunable : INT := 2;
                END_VAR
                TASK Fast(INTERVAL := T#10ms, PRIORITY := 1);
                PROGRAM Main WITH Fast : Demo;
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
        assert_eq!(configuration.var_blocks.len(), 1);
        assert_eq!(configuration.resources.len(), 1);
        assert_eq!(configuration.resources[0].var_blocks.len(), 1);
        assert_eq!(configuration.resources[0].tasks.len(), 1);
        assert_eq!(configuration.resources[0].program_instances.len(), 1);
    }

    #[test]
    fn parses_textual_sfc_body() {
        let source = r#"
            PROGRAM Sequence
            VAR
                Ready : BOOL := TRUE;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start;
            STEP Run;
            TRANSITION Go := Ready;
            ACTION MarkDone(L, T#5ms):
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
        assert_eq!(sfc.steps.len(), 2);
        assert!(sfc.steps[0].initial);
        assert_eq!(sfc.transitions.len(), 1);
        assert_eq!(sfc.actions.len(), 1);
        assert_eq!(sfc.actions[0].qualifier, SfcActionQualifier::TimeLimited);
        assert_eq!(sfc.actions[0].duration, Some(Literal::DurationMs(5)));
        assert_eq!(sfc.actions[0].body.len(), 1);
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
}
