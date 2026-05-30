// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_diagnostics::{json_escape, Diagnostic};
use iec_ir::{canonical_identifier, ImplementationLanguage, LibraryElement};

use crate::{
    range_from_offsets, DocumentAnalysis, DocumentInput, Position, SourceRange, SymbolKind,
    KEYWORDS,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceDocument {
    pub uri: String,
    pub text: String,
    pub tokens: Vec<SourceToken>,
    pub nodes: Vec<SourceNode>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceToken {
    pub kind: SourceTokenKind,
    pub lexeme: String,
    pub range: SourceRange,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceTokenKind {
    Identifier,
    Keyword,
    Number,
    StringLiteral,
    DirectVariable,
    Comment,
    Pragma,
    Whitespace,
    Symbol,
    XmlTag,
    XmlText,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceNode {
    pub kind: SourceNodeKind,
    pub name: Option<String>,
    pub detail: String,
    pub range: SourceRange,
    pub children: Vec<SourceNode>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceNodeKind {
    Document,
    Declaration,
    PouBody,
    Statement,
    Expression,
    Variable,
    GraphNode,
    PlcOpenNode,
    Comment,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMappedObject {
    pub path: String,
    pub name: Option<String>,
    pub kind: SourceMappedObjectKind,
    pub detail: String,
    pub range: Option<SourceRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceMappedObjectKind {
    DataTypeDeclaration,
    PouDeclaration,
    VariableDeclaration,
    ConfigurationDeclaration,
    ResourceDeclaration,
    TaskDeclaration,
    ProgramInstanceDeclaration,
    Statement,
    Expression,
    PouBody,
    SfcStep,
    SfcTransition,
    SfcAction,
    LdRung,
    FbdNetwork,
    PlcOpenNode,
    GeneratedHelperDeclaration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceMap {
    pub uri: String,
    pub objects: Vec<SourceMappedObject>,
}

impl SourceDocument {
    pub fn token_at(&self, offset: usize) -> Option<&SourceToken> {
        self.tokens
            .iter()
            .find(|token| token.range.start <= offset && offset <= token.range.end)
    }

    pub fn identifier_at(&self, offset: usize) -> Option<&SourceToken> {
        self.token_at(offset).filter(|token| {
            matches!(
                token.kind,
                SourceTokenKind::Identifier
                    | SourceTokenKind::Keyword
                    | SourceTokenKind::DirectVariable
            )
        })
    }

    pub fn to_json(&self) -> String {
        let tokens = self
            .tokens
            .iter()
            .map(SourceToken::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let nodes = self
            .nodes
            .iter()
            .map(SourceNode::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"uri\":\"{}\",\"tokens\":[{}],\"nodes\":[{}],\"diagnostics\":{}}}",
            json_escape(&self.uri),
            tokens,
            nodes,
            iec_diagnostics::diagnostics_to_json(&self.diagnostics)
        )
    }
}

impl SourceToken {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"kind\":\"{}\",\"lexeme\":\"{}\",\"range\":{}}}",
            self.kind.as_str(),
            json_escape(&self.lexeme),
            self.range.to_json()
        )
    }
}

impl SourceTokenKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceTokenKind::Identifier => "identifier",
            SourceTokenKind::Keyword => "keyword",
            SourceTokenKind::Number => "number",
            SourceTokenKind::StringLiteral => "stringLiteral",
            SourceTokenKind::DirectVariable => "directVariable",
            SourceTokenKind::Comment => "comment",
            SourceTokenKind::Pragma => "pragma",
            SourceTokenKind::Whitespace => "whitespace",
            SourceTokenKind::Symbol => "symbol",
            SourceTokenKind::XmlTag => "xmlTag",
            SourceTokenKind::XmlText => "xmlText",
            SourceTokenKind::Unknown => "unknown",
        }
    }
}

impl SourceNode {
    pub fn to_json(&self) -> String {
        let name = self
            .name
            .as_ref()
            .map(|name| format!("\"{}\"", json_escape(name)))
            .unwrap_or_else(|| "null".to_string());
        let children = self
            .children
            .iter()
            .map(SourceNode::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"kind\":\"{}\",\"name\":{},\"detail\":\"{}\",\"range\":{},\"children\":[{}]}}",
            self.kind.as_str(),
            name,
            json_escape(&self.detail),
            self.range.to_json(),
            children
        )
    }
}

impl SourceNodeKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceNodeKind::Document => "document",
            SourceNodeKind::Declaration => "declaration",
            SourceNodeKind::PouBody => "pouBody",
            SourceNodeKind::Statement => "statement",
            SourceNodeKind::Expression => "expression",
            SourceNodeKind::Variable => "variable",
            SourceNodeKind::GraphNode => "graphNode",
            SourceNodeKind::PlcOpenNode => "plcOpenNode",
            SourceNodeKind::Comment => "comment",
        }
    }
}

impl SourceMap {
    pub fn to_json(&self) -> String {
        let objects = self
            .objects
            .iter()
            .map(SourceMappedObject::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"uri\":\"{}\",\"objects\":[{}]}}",
            json_escape(&self.uri),
            objects
        )
    }
}

impl SourceMappedObject {
    pub fn to_json(&self) -> String {
        let name = self
            .name
            .as_ref()
            .map(|name| format!("\"{}\"", json_escape(name)))
            .unwrap_or_else(|| "null".to_string());
        let range = self
            .range
            .as_ref()
            .map(SourceRange::to_json)
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"path\":\"{}\",\"name\":{},\"kind\":\"{}\",\"detail\":\"{}\",\"range\":{}}}",
            json_escape(&self.path),
            name,
            self.kind.as_str(),
            json_escape(&self.detail),
            range
        )
    }
}

impl SourceMappedObjectKind {
    pub fn as_str(self) -> &'static str {
        match self {
            SourceMappedObjectKind::DataTypeDeclaration => "dataTypeDeclaration",
            SourceMappedObjectKind::PouDeclaration => "pouDeclaration",
            SourceMappedObjectKind::VariableDeclaration => "variableDeclaration",
            SourceMappedObjectKind::ConfigurationDeclaration => "configurationDeclaration",
            SourceMappedObjectKind::ResourceDeclaration => "resourceDeclaration",
            SourceMappedObjectKind::TaskDeclaration => "taskDeclaration",
            SourceMappedObjectKind::ProgramInstanceDeclaration => "programInstanceDeclaration",
            SourceMappedObjectKind::Statement => "statement",
            SourceMappedObjectKind::Expression => "expression",
            SourceMappedObjectKind::PouBody => "pouBody",
            SourceMappedObjectKind::SfcStep => "sfcStep",
            SourceMappedObjectKind::SfcTransition => "sfcTransition",
            SourceMappedObjectKind::SfcAction => "sfcAction",
            SourceMappedObjectKind::LdRung => "ldRung",
            SourceMappedObjectKind::FbdNetwork => "fbdNetwork",
            SourceMappedObjectKind::PlcOpenNode => "plcOpenNode",
            SourceMappedObjectKind::GeneratedHelperDeclaration => "generatedHelperDeclaration",
        }
    }
}

pub fn analyze_source_document(
    input: &DocumentInput,
    diagnostics: &[Diagnostic],
) -> SourceDocument {
    let tokens = lex_preserving_tokens(&input.uri, &input.text);
    let mut nodes = recover_nodes_from_tokens(&input.uri, &input.text, &tokens);
    if input.uri.ends_with(".xml") || input.language_id.as_deref() == Some("xml") {
        nodes.extend(recover_plcopen_nodes(&input.uri, &input.text));
    }
    SourceDocument {
        uri: input.uri.clone(),
        text: input.text.clone(),
        tokens,
        nodes,
        diagnostics: diagnostics.to_vec(),
    }
}

pub fn source_map_for_analysis(analysis: &DocumentAnalysis) -> SourceMap {
    let mut objects = Vec::new();
    for symbol in &analysis.symbols {
        let kind = match symbol.kind {
            SymbolKind::DataType => SourceMappedObjectKind::DataTypeDeclaration,
            SymbolKind::Function | SymbolKind::FunctionBlock | SymbolKind::Program => {
                SourceMappedObjectKind::PouDeclaration
            }
            SymbolKind::Configuration => SourceMappedObjectKind::ConfigurationDeclaration,
            SymbolKind::Resource => SourceMappedObjectKind::ResourceDeclaration,
            SymbolKind::Task => SourceMappedObjectKind::TaskDeclaration,
            SymbolKind::ProgramInstance => SourceMappedObjectKind::ProgramInstanceDeclaration,
            SymbolKind::Variable | SymbolKind::AccessPath => {
                SourceMappedObjectKind::VariableDeclaration
            }
            SymbolKind::SfcStep => SourceMappedObjectKind::SfcStep,
            SymbolKind::SfcAction => SourceMappedObjectKind::SfcAction,
            SymbolKind::StandardFunction
            | SymbolKind::StandardFunctionBlock
            | SymbolKind::Keyword
            | SymbolKind::ElementaryType => continue,
        };
        let path = symbol
            .container_name
            .as_ref()
            .map(|container| format!("{container}.{}", symbol.name))
            .unwrap_or_else(|| symbol.name.clone());
        objects.push(SourceMappedObject {
            path,
            name: Some(symbol.name.clone()),
            kind,
            detail: symbol.detail.clone(),
            range: symbol.range.clone(),
        });
    }

    for element in &analysis.project.library_elements {
        if let LibraryElement::Pou(pou) = element {
            if let Some(body_range) = body_range_for_pou(&analysis.uri, &analysis.source.text, pou)
            {
                objects.push(SourceMappedObject {
                    path: format!("{}.body", pou.name.original),
                    name: Some(pou.name.original.clone()),
                    kind: SourceMappedObjectKind::PouBody,
                    detail: language_label(pou.body.language).to_string(),
                    range: Some(body_range),
                });
            }
            for (index, statement) in pou.body.statements.iter().enumerate() {
                let needle = statement_needle(statement);
                let range = needle.as_deref().and_then(|needle| {
                    find_text_range(&analysis.uri, &analysis.source.text, needle)
                });
                objects.push(SourceMappedObject {
                    path: format!("{}.statement[{index}]", pou.name.original),
                    name: None,
                    kind: SourceMappedObjectKind::Statement,
                    detail: statement_detail(statement),
                    range,
                });
            }
            for network in &pou.body.networks {
                let kind = match network.language {
                    ImplementationLanguage::LadderDiagram => SourceMappedObjectKind::LdRung,
                    ImplementationLanguage::FunctionBlockDiagram => {
                        SourceMappedObjectKind::FbdNetwork
                    }
                    _ => SourceMappedObjectKind::Statement,
                };
                objects.push(SourceMappedObject {
                    path: format!(
                        "{}.network[{}]",
                        pou.name.original,
                        network.label.as_deref().unwrap_or("0")
                    ),
                    name: network.label.clone(),
                    kind,
                    detail: language_label(network.language).to_string(),
                    range: network.label.as_deref().and_then(|label| {
                        find_text_range(&analysis.uri, &analysis.source.text, label)
                    }),
                });
                for node in &network.nodes {
                    let range = find_text_range(&analysis.uri, &analysis.source.text, &node.id);
                    objects.push(SourceMappedObject {
                        path: format!("{}.network.node[{}]", pou.name.original, node.id),
                        name: Some(node.id.clone()),
                        kind: SourceMappedObjectKind::PlcOpenNode,
                        detail: node.kind.clone(),
                        range,
                    });
                }
            }
            if let Some(sfc) = &pou.body.sfc {
                for (index, transition) in sfc.transitions.iter().enumerate() {
                    let name = transition
                        .name
                        .as_ref()
                        .map(|name| name.original.clone())
                        .unwrap_or_else(|| format!("transition{index}"));
                    objects.push(SourceMappedObject {
                        path: format!("{}.transition[{name}]", pou.name.original),
                        name: Some(name.clone()),
                        kind: SourceMappedObjectKind::SfcTransition,
                        detail: "SFC transition".to_string(),
                        range: find_text_range(&analysis.uri, &analysis.source.text, &name),
                    });
                }
            }
        }
    }

    SourceMap {
        uri: analysis.uri.clone(),
        objects,
    }
}

fn lex_preserving_tokens(uri: &str, text: &str) -> Vec<SourceToken> {
    let mut tokens = Vec::new();
    let mut pos = 0;
    while pos < text.len() {
        let ch = text[pos..].chars().next().unwrap();
        let start = pos;
        if ch.is_whitespace() {
            pos += ch.len_utf8();
            while pos < text.len() && text[pos..].chars().next().is_some_and(char::is_whitespace) {
                pos += text[pos..].chars().next().unwrap().len_utf8();
            }
            push_token(
                uri,
                text,
                &mut tokens,
                SourceTokenKind::Whitespace,
                start,
                pos,
            );
        } else if text[start..].starts_with("(*") {
            pos += 2;
            while pos < text.len() && !text[pos..].starts_with("*)") {
                pos += text[pos..].chars().next().unwrap().len_utf8();
            }
            pos = (pos + 2).min(text.len());
            push_token(uri, text, &mut tokens, SourceTokenKind::Comment, start, pos);
        } else if text[start..].starts_with("//") {
            pos += 2;
            while pos < text.len() && !text[pos..].starts_with('\n') {
                pos += text[pos..].chars().next().unwrap().len_utf8();
            }
            push_token(uri, text, &mut tokens, SourceTokenKind::Comment, start, pos);
        } else if ch == '{' {
            pos += 1;
            while pos < text.len() && !text[pos..].starts_with('}') {
                pos += text[pos..].chars().next().unwrap().len_utf8();
            }
            pos = (pos + 1).min(text.len());
            push_token(uri, text, &mut tokens, SourceTokenKind::Pragma, start, pos);
        } else if ch == '\'' || ch == '"' {
            let quote = ch;
            pos += quote.len_utf8();
            while pos < text.len() {
                let next = text[pos..].chars().next().unwrap();
                pos += next.len_utf8();
                if next == quote {
                    break;
                }
            }
            push_token(
                uri,
                text,
                &mut tokens,
                SourceTokenKind::StringLiteral,
                start,
                pos,
            );
        } else if ch == '%' {
            pos += 1;
            while pos < text.len()
                && text[pos..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_ascii_alphanumeric() || next == '.' || next == '_')
            {
                pos += text[pos..].chars().next().unwrap().len_utf8();
            }
            push_token(
                uri,
                text,
                &mut tokens,
                SourceTokenKind::DirectVariable,
                start,
                pos,
            );
        } else if ch == '<' {
            pos += 1;
            while pos < text.len() && !text[pos..].starts_with('>') {
                pos += text[pos..].chars().next().unwrap().len_utf8();
            }
            pos = (pos + 1).min(text.len());
            push_token(uri, text, &mut tokens, SourceTokenKind::XmlTag, start, pos);
        } else if is_ident_start(ch) {
            pos += ch.len_utf8();
            while pos < text.len() && text[pos..].chars().next().is_some_and(is_ident_continue) {
                pos += text[pos..].chars().next().unwrap().len_utf8();
            }
            let lexeme = &text[start..pos];
            let kind = if KEYWORDS
                .iter()
                .any(|keyword| canonical_identifier(keyword) == canonical_identifier(lexeme))
            {
                SourceTokenKind::Keyword
            } else {
                SourceTokenKind::Identifier
            };
            push_token(uri, text, &mut tokens, kind, start, pos);
        } else if ch.is_ascii_digit() {
            pos += ch.len_utf8();
            while pos < text.len()
                && text[pos..]
                    .chars()
                    .next()
                    .is_some_and(|next| next.is_ascii_alphanumeric() || next == '_' || next == '.')
            {
                pos += text[pos..].chars().next().unwrap().len_utf8();
            }
            push_token(uri, text, &mut tokens, SourceTokenKind::Number, start, pos);
        } else if ch == '>'
            || ch == '/'
            || ch == ':'
            || ch == ';'
            || ch == ','
            || ch == '('
            || ch == ')'
            || ch == '['
            || ch == ']'
            || ch == '.'
            || ch == '+'
            || ch == '-'
            || ch == '*'
            || ch == '='
            || ch == '&'
        {
            pos += ch.len_utf8();
            push_token(uri, text, &mut tokens, SourceTokenKind::Symbol, start, pos);
        } else {
            pos += ch.len_utf8();
            push_token(uri, text, &mut tokens, SourceTokenKind::Unknown, start, pos);
        }
    }
    tokens
}

fn push_token(
    uri: &str,
    text: &str,
    tokens: &mut Vec<SourceToken>,
    kind: SourceTokenKind,
    start: usize,
    end: usize,
) {
    tokens.push(SourceToken {
        kind,
        lexeme: text[start..end].to_string(),
        range: range_from_offsets(uri, text, start, end),
    });
}

fn recover_nodes_from_tokens(uri: &str, text: &str, tokens: &[SourceToken]) -> Vec<SourceNode> {
    let mut nodes = Vec::new();
    if !text.is_empty() {
        nodes.push(SourceNode {
            kind: SourceNodeKind::Document,
            name: None,
            detail: "recoverable source document".to_string(),
            range: range_from_offsets(uri, text, 0, text.len()),
            children: Vec::new(),
        });
    }
    for token in tokens {
        match token.kind {
            SourceTokenKind::Comment => nodes.push(SourceNode {
                kind: SourceNodeKind::Comment,
                name: None,
                detail: "comment".to_string(),
                range: token.range.clone(),
                children: Vec::new(),
            }),
            SourceTokenKind::Keyword if declaration_keyword(&token.lexeme) => {
                nodes.push(SourceNode {
                    kind: SourceNodeKind::Declaration,
                    name: next_identifier_name(tokens, token.range.end),
                    detail: token.lexeme.clone(),
                    range: line_range(uri, text, token.range.start),
                    children: Vec::new(),
                })
            }
            SourceTokenKind::DirectVariable => nodes.push(SourceNode {
                kind: SourceNodeKind::Variable,
                name: Some(token.lexeme.clone()),
                detail: "direct variable".to_string(),
                range: token.range.clone(),
                children: Vec::new(),
            }),
            _ => {}
        }
    }
    for range in statement_line_ranges(uri, text) {
        nodes.push(SourceNode {
            kind: SourceNodeKind::Statement,
            name: None,
            detail: text[range.start..range.end].trim().to_string(),
            range,
            children: Vec::new(),
        });
    }
    nodes
}

fn recover_plcopen_nodes(uri: &str, text: &str) -> Vec<SourceNode> {
    let mut nodes = Vec::new();
    let mut offset = 0;
    while let Some(relative) = text[offset..].find("localId=\"") {
        let start = offset + relative;
        let value_start = start + "localId=\"".len();
        let Some(value_end_relative) = text[value_start..].find('"') else {
            break;
        };
        let value_end = value_start + value_end_relative;
        let tag_start = text[..start].rfind('<').unwrap_or(start);
        let tag_end = text[value_end..]
            .find('>')
            .map(|end| value_end + end + 1)
            .unwrap_or(value_end);
        nodes.push(SourceNode {
            kind: SourceNodeKind::PlcOpenNode,
            name: Some(text[value_start..value_end].to_string()),
            detail: "PLCopen localId".to_string(),
            range: range_from_offsets(uri, text, tag_start, tag_end),
            children: Vec::new(),
        });
        offset = tag_end;
    }
    nodes
}

fn body_range_for_pou(uri: &str, text: &str, pou: &iec_ir::Pou) -> Option<SourceRange> {
    let name_offset = text.find(&pou.name.original)?;
    let end_keyword = match pou.kind {
        iec_ir::PouKind::Function { .. } => "END_FUNCTION",
        iec_ir::PouKind::FunctionBlock => "END_FUNCTION_BLOCK",
        iec_ir::PouKind::Program => "END_PROGRAM",
    };
    let end = text[name_offset..]
        .find(end_keyword)
        .map(|offset| name_offset + offset + end_keyword.len())
        .unwrap_or(text.len());
    Some(range_from_offsets(uri, text, name_offset, end))
}

fn statement_line_ranges(uri: &str, text: &str) -> Vec<SourceRange> {
    let mut ranges = Vec::new();
    let mut line_start = 0;
    for (index, ch) in text.char_indices() {
        if ch == '\n' {
            maybe_push_statement_range(uri, text, line_start, index, &mut ranges);
            line_start = index + 1;
        }
    }
    maybe_push_statement_range(uri, text, line_start, text.len(), &mut ranges);
    ranges
}

fn maybe_push_statement_range(
    uri: &str,
    text: &str,
    start: usize,
    end: usize,
    ranges: &mut Vec<SourceRange>,
) {
    let line = text[start..end].trim();
    if line.is_empty() || line.starts_with("(*") || line.starts_with("//") {
        return;
    }
    let canonical = canonical_identifier(line);
    if canonical.contains(":=")
        || canonical.starts_with("IF ")
        || canonical.starts_with("FOR ")
        || canonical.starts_with("WHILE ")
        || canonical.starts_with("REPEAT")
        || canonical.starts_with("CASE ")
        || canonical.starts_with("TRANSITION")
        || canonical.starts_with("CONTACT")
        || canonical.starts_with("COIL")
    {
        ranges.push(line_range(uri, text, start));
    }
}

fn line_range(uri: &str, text: &str, offset: usize) -> SourceRange {
    let start = text[..offset]
        .rfind('\n')
        .map(|index| index + 1)
        .unwrap_or(0);
    let end = text[offset..]
        .find('\n')
        .map(|index| offset + index)
        .unwrap_or(text.len());
    range_from_offsets(uri, text, start, end)
}

fn next_identifier_name(tokens: &[SourceToken], after: usize) -> Option<String> {
    tokens
        .iter()
        .filter(|token| token.range.start >= after)
        .find(|token| matches!(token.kind, SourceTokenKind::Identifier))
        .map(|token| token.lexeme.clone())
}

fn declaration_keyword(text: &str) -> bool {
    matches!(
        canonical_identifier(text).as_str(),
        "TYPE"
            | "FUNCTION"
            | "FUNCTION_BLOCK"
            | "PROGRAM"
            | "CONFIGURATION"
            | "RESOURCE"
            | "TASK"
            | "VAR"
            | "VAR_INPUT"
            | "VAR_OUTPUT"
            | "VAR_IN_OUT"
            | "VAR_GLOBAL"
            | "VAR_ACCESS"
            | "VAR_CONFIG"
            | "INITIAL_STEP"
            | "STEP"
            | "ACTION"
            | "TRANSITION"
    )
}

fn find_text_range(uri: &str, text: &str, needle: &str) -> Option<SourceRange> {
    let start = text.find(needle)?;
    Some(range_from_offsets(uri, text, start, start + needle.len()))
}

fn statement_needle(statement: &iec_ir::Statement) -> Option<String> {
    match statement {
        iec_ir::Statement::Assignment { target, .. } => Some(format!("{target}")),
        iec_ir::Statement::FbCall { name, .. } => Some(format!("{name}")),
        iec_ir::Statement::For { control, .. } => Some(control.original.clone()),
        iec_ir::Statement::IlLabel(label) => Some(label.original.clone()),
        iec_ir::Statement::Unsupported(text) => Some(text.clone()),
        _ => None,
    }
}

fn statement_detail(statement: &iec_ir::Statement) -> String {
    match statement {
        iec_ir::Statement::Empty => "empty statement".to_string(),
        iec_ir::Statement::Assignment { target, value } => {
            format!("assignment {target} := {value}")
        }
        iec_ir::Statement::FbCall { name, .. } => format!("function block call {name}"),
        iec_ir::Statement::If { .. } => "IF statement".to_string(),
        iec_ir::Statement::Case { .. } => "CASE statement".to_string(),
        iec_ir::Statement::For { control, .. } => format!("FOR loop controlled by {control}"),
        iec_ir::Statement::While { .. } => "WHILE loop".to_string(),
        iec_ir::Statement::Repeat { .. } => "REPEAT loop".to_string(),
        iec_ir::Statement::Il { op, .. } => format!("IL {op:?}"),
        iec_ir::Statement::IlLabel(label) => format!("IL label {}", label.original),
        iec_ir::Statement::Exit => "EXIT".to_string(),
        iec_ir::Statement::Return => "RETURN".to_string(),
        iec_ir::Statement::Unsupported(text) => format!("unsupported statement {text}"),
    }
}

fn language_label(language: ImplementationLanguage) -> &'static str {
    match language {
        ImplementationLanguage::StructuredText => "Structured Text",
        ImplementationLanguage::InstructionList => "Instruction List",
        ImplementationLanguage::SequentialFunctionChart => "Sequential Function Chart",
        ImplementationLanguage::LadderDiagram => "Ladder Diagram",
        ImplementationLanguage::FunctionBlockDiagram => "Function Block Diagram",
        ImplementationLanguage::External => "External",
    }
}

fn is_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_'
}

fn is_ident_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

#[allow(dead_code)]
fn _position_for_debug(_position: &Position) {}
