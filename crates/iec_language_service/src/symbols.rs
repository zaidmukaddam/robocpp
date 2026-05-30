// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::json_escape;
use iec_ir::{
    canonical_identifier, Expr, LibraryElement, ParamAssignment, Project, Statement, VariableRef,
};
use iec_stdlib::{standard_symbols, StandardSymbolKind};

use crate::source::SourceTokenKind;
use crate::{
    CompletionItem, DocumentAnalysis, DocumentSymbol, SourceRange, SymbolKind, WorkspaceAnalysis,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolIndex {
    pub definitions: Vec<SymbolDefinition>,
    pub references: Vec<SymbolReference>,
    pub scopes: Vec<ScopeInfo>,
    pub direct_locations: Vec<DirectLocationSymbol>,
    pub access_paths: Vec<AccessPathSymbol>,
    pub standard_symbols: Vec<SymbolDefinition>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolDefinition {
    pub name: String,
    pub canonical_name: String,
    pub kind: SymbolKind,
    pub detail: String,
    pub uri: String,
    pub range: Option<SourceRange>,
    pub container_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolReference {
    pub name: String,
    pub canonical_name: String,
    pub uri: String,
    pub range: SourceRange,
    pub definition: Option<SourceRange>,
    pub container_name: Option<String>,
    pub write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScopeInfo {
    pub name: String,
    pub uri: String,
    pub range: Option<SourceRange>,
    pub symbols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirectLocationSymbol {
    pub location: String,
    pub uri: String,
    pub range: Option<SourceRange>,
    pub type_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPathSymbol {
    pub name: String,
    pub uri: String,
    pub target: String,
    pub direction: String,
    pub range: Option<SourceRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameValidation {
    pub valid: bool,
    pub message: String,
    pub edits: Vec<TextEdit>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextEdit {
    pub range: SourceRange,
    pub new_text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallHierarchyItem {
    pub name: String,
    pub kind: SymbolKind,
    pub uri: String,
    pub range: Option<SourceRange>,
    pub calls: Vec<String>,
    pub callers: Vec<String>,
}

impl SymbolIndex {
    pub fn definition_at(&self, uri: &str, offset: usize) -> Option<SymbolDefinition> {
        let reference = self.references.iter().find(|reference| {
            reference.uri == uri && reference.range.start <= offset && offset <= reference.range.end
        })?;
        self.definitions
            .iter()
            .filter(|definition| definition.canonical_name == reference.canonical_name)
            .find(|definition| definition.uri == uri)
            .or_else(|| {
                self.definitions
                    .iter()
                    .find(|definition| definition.canonical_name == reference.canonical_name)
            })
            .cloned()
    }

    pub fn references_for(&self, name: &str) -> Vec<SymbolReference> {
        let canonical = canonical_identifier(name);
        self.references
            .iter()
            .filter(|reference| reference.canonical_name == canonical)
            .cloned()
            .collect()
    }

    pub fn workspace_symbols(&self, query: &str) -> Vec<SymbolDefinition> {
        let canonical_query = canonical_identifier(query);
        let mut symbols = self
            .definitions
            .iter()
            .chain(self.standard_symbols.iter())
            .filter(|definition| {
                canonical_query.is_empty()
                    || definition.canonical_name.contains(&canonical_query)
                    || canonical_identifier(&definition.detail).contains(&canonical_query)
            })
            .cloned()
            .collect::<Vec<_>>();
        symbols.sort_by(|left, right| {
            left.name
                .cmp(&right.name)
                .then_with(|| left.uri.cmp(&right.uri))
        });
        symbols
    }

    pub fn validate_rename(&self, uri: &str, offset: usize, new_name: &str) -> RenameValidation {
        if !is_valid_identifier(new_name) {
            return RenameValidation {
                valid: false,
                message: format!("'{new_name}' is not a valid IEC identifier"),
                edits: Vec::new(),
            };
        }
        let Some(definition) = self.definition_at(uri, offset) else {
            return RenameValidation {
                valid: false,
                message: "no symbol at the requested location".to_string(),
                edits: Vec::new(),
            };
        };
        if matches!(
            definition.kind,
            SymbolKind::Keyword
                | SymbolKind::ElementaryType
                | SymbolKind::StandardFunction
                | SymbolKind::StandardFunctionBlock
        ) {
            return RenameValidation {
                valid: false,
                message: format!("{} cannot be renamed", definition.name),
                edits: Vec::new(),
            };
        }
        let canonical_new = canonical_identifier(new_name);
        let conflicts = self.definitions.iter().any(|candidate| {
            candidate.canonical_name == canonical_new
                && candidate.container_name == definition.container_name
                && candidate.uri == definition.uri
                && candidate.range != definition.range
        });
        if conflicts {
            return RenameValidation {
                valid: false,
                message: format!("'{new_name}' already exists in this scope"),
                edits: Vec::new(),
            };
        }
        let edits = self
            .references_for(&definition.name)
            .into_iter()
            .map(|reference| TextEdit {
                range: reference.range,
                new_text: new_name.to_string(),
            })
            .collect();
        RenameValidation {
            valid: true,
            message: "rename is valid".to_string(),
            edits,
        }
    }

    pub fn to_json(&self) -> String {
        let definitions = self
            .definitions
            .iter()
            .map(SymbolDefinition::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let references = self
            .references
            .iter()
            .map(SymbolReference::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let scopes = self
            .scopes
            .iter()
            .map(ScopeInfo::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let direct_locations = self
            .direct_locations
            .iter()
            .map(DirectLocationSymbol::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let access_paths = self
            .access_paths
            .iter()
            .map(AccessPathSymbol::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"definitions\":[{}],\"references\":[{}],\"scopes\":[{}],\"directLocations\":[{}],\"accessPaths\":[{}]}}",
            definitions, references, scopes, direct_locations, access_paths
        )
    }
}

impl SymbolDefinition {
    pub fn from_symbol(uri: &str, symbol: &DocumentSymbol) -> Self {
        Self {
            name: symbol.name.clone(),
            canonical_name: canonical_identifier(&symbol.name),
            kind: symbol.kind.clone(),
            detail: symbol.detail.clone(),
            uri: uri.to_string(),
            range: symbol.range.clone(),
            container_name: symbol.container_name.clone(),
        }
    }

    pub fn to_json(&self) -> String {
        let range = self
            .range
            .as_ref()
            .map(SourceRange::to_json)
            .unwrap_or_else(|| "null".to_string());
        let container = self
            .container_name
            .as_ref()
            .map(|container| format!("\"{}\"", json_escape(container)))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"name\":\"{}\",\"kind\":\"{}\",\"detail\":\"{}\",\"uri\":\"{}\",\"range\":{},\"containerName\":{}}}",
            json_escape(&self.name),
            self.kind.as_str(),
            json_escape(&self.detail),
            json_escape(&self.uri),
            range,
            container
        )
    }
}

impl SymbolReference {
    pub fn to_json(&self) -> String {
        let definition = self
            .definition
            .as_ref()
            .map(SourceRange::to_json)
            .unwrap_or_else(|| "null".to_string());
        let container = self
            .container_name
            .as_ref()
            .map(|container| format!("\"{}\"", json_escape(container)))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"name\":\"{}\",\"uri\":\"{}\",\"range\":{},\"definition\":{},\"containerName\":{},\"write\":{}}}",
            json_escape(&self.name),
            json_escape(&self.uri),
            self.range.to_json(),
            definition,
            container,
            self.write
        )
    }
}

impl ScopeInfo {
    pub fn to_json(&self) -> String {
        let range = self
            .range
            .as_ref()
            .map(SourceRange::to_json)
            .unwrap_or_else(|| "null".to_string());
        let symbols = self
            .symbols
            .iter()
            .map(|symbol| format!("\"{}\"", json_escape(symbol)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"name\":\"{}\",\"uri\":\"{}\",\"range\":{},\"symbols\":[{}]}}",
            json_escape(&self.name),
            json_escape(&self.uri),
            range,
            symbols
        )
    }
}

impl DirectLocationSymbol {
    pub fn to_json(&self) -> String {
        let range = self
            .range
            .as_ref()
            .map(SourceRange::to_json)
            .unwrap_or_else(|| "null".to_string());
        let type_name = self
            .type_name
            .as_ref()
            .map(|type_name| format!("\"{}\"", json_escape(type_name)))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"location\":\"{}\",\"uri\":\"{}\",\"range\":{},\"typeName\":{}}}",
            json_escape(&self.location),
            json_escape(&self.uri),
            range,
            type_name
        )
    }
}

impl AccessPathSymbol {
    pub fn to_json(&self) -> String {
        let range = self
            .range
            .as_ref()
            .map(SourceRange::to_json)
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"name\":\"{}\",\"uri\":\"{}\",\"target\":\"{}\",\"direction\":\"{}\",\"range\":{}}}",
            json_escape(&self.name),
            json_escape(&self.uri),
            json_escape(&self.target),
            json_escape(&self.direction),
            range
        )
    }
}

impl RenameValidation {
    pub fn to_json(&self) -> String {
        let edits = self
            .edits
            .iter()
            .map(TextEdit::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"valid\":{},\"message\":\"{}\",\"edits\":[{}]}}",
            self.valid,
            json_escape(&self.message),
            edits
        )
    }
}

impl TextEdit {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"range\":{},\"newText\":\"{}\"}}",
            self.range.to_json(),
            json_escape(&self.new_text)
        )
    }
}

impl CallHierarchyItem {
    pub fn to_json(&self) -> String {
        let range = self
            .range
            .as_ref()
            .map(SourceRange::to_json)
            .unwrap_or_else(|| "null".to_string());
        let calls = self
            .calls
            .iter()
            .map(|call| format!("\"{}\"", json_escape(call)))
            .collect::<Vec<_>>()
            .join(",");
        let callers = self
            .callers
            .iter()
            .map(|caller| format!("\"{}\"", json_escape(caller)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"name\":\"{}\",\"kind\":\"{}\",\"uri\":\"{}\",\"range\":{},\"calls\":[{}],\"callers\":[{}]}}",
            json_escape(&self.name),
            self.kind.as_str(),
            json_escape(&self.uri),
            range,
            calls,
            callers
        )
    }
}

pub fn document_symbol_index(analysis: &DocumentAnalysis) -> SymbolIndex {
    workspace_symbol_index_from_documents(std::slice::from_ref(analysis), &analysis.project)
}

pub fn workspace_symbol_index(analysis: &WorkspaceAnalysis) -> SymbolIndex {
    workspace_symbol_index_from_documents(&analysis.documents, &analysis.merged_project)
}

pub fn workspace_symbol_index_from_documents(
    documents: &[DocumentAnalysis],
    project: &Project,
) -> SymbolIndex {
    let mut definitions = Vec::new();
    let mut definition_by_name = BTreeMap::<String, SourceRange>::new();
    let mut scopes = Vec::new();
    let mut direct_locations = Vec::new();
    let mut access_paths = Vec::new();

    for document in documents {
        for symbol in &document.symbols {
            let definition = SymbolDefinition::from_symbol(&document.uri, symbol);
            if let Some(range) = &definition.range {
                definition_by_name
                    .entry(definition.canonical_name.clone())
                    .or_insert_with(|| range.clone());
            }
            definitions.push(definition);
        }
        scopes.extend(scopes_for_document(document));
        direct_locations.extend(direct_locations_for_document(document));
        access_paths.extend(access_paths_for_document(document));
    }

    let references = documents
        .iter()
        .flat_map(|document| references_for_document(document, &definition_by_name))
        .collect::<Vec<_>>();

    let standard_symbols = standard_symbols()
        .iter()
        .map(|symbol| SymbolDefinition {
            name: symbol.name.to_string(),
            canonical_name: canonical_identifier(symbol.name),
            kind: match symbol.kind {
                StandardSymbolKind::Function => SymbolKind::StandardFunction,
                StandardSymbolKind::FunctionBlock => SymbolKind::StandardFunctionBlock,
            },
            detail: format!("IEC standard library clause {}", symbol.clause),
            uri: "iec://stdlib".to_string(),
            range: None,
            container_name: Some("IEC standard library".to_string()),
        })
        .collect();

    let mut index = SymbolIndex {
        definitions,
        references,
        scopes,
        direct_locations,
        access_paths,
        standard_symbols,
    };
    add_pou_member_scopes(project, &mut index);
    index
}

pub fn call_hierarchy(analysis: &WorkspaceAnalysis) -> Vec<CallHierarchyItem> {
    let index = workspace_symbol_index(analysis);
    let mut calls = BTreeMap::<String, BTreeSet<String>>::new();
    for element in &analysis.merged_project.library_elements {
        if let LibraryElement::Pou(pou) = element {
            let mut pou_calls = BTreeSet::new();
            collect_calls_in_statements(&pou.body.statements, &mut pou_calls);
            if let Some(sfc) = &pou.body.sfc {
                for action in &sfc.actions {
                    collect_calls_in_statements(&action.body, &mut pou_calls);
                }
            }
            calls.insert(pou.name.original.clone(), pou_calls);
        }
    }
    let mut callers = BTreeMap::<String, BTreeSet<String>>::new();
    for (caller, callees) in &calls {
        for callee in callees {
            callers
                .entry(callee.clone())
                .or_default()
                .insert(caller.clone());
        }
    }
    index
        .definitions
        .iter()
        .filter(|definition| {
            matches!(
                definition.kind,
                SymbolKind::Function | SymbolKind::FunctionBlock | SymbolKind::Program
            )
        })
        .map(|definition| CallHierarchyItem {
            name: definition.name.clone(),
            kind: definition.kind.clone(),
            uri: definition.uri.clone(),
            range: definition.range.clone(),
            calls: calls
                .get(&definition.name)
                .map(|items| items.iter().cloned().collect())
                .unwrap_or_default(),
            callers: callers
                .get(&definition.name)
                .map(|items| items.iter().cloned().collect())
                .unwrap_or_default(),
        })
        .collect()
}

pub fn completion_items_from_index(index: &SymbolIndex) -> Vec<CompletionItem> {
    let mut seen = BTreeSet::new();
    let mut completions = Vec::new();
    for definition in index
        .definitions
        .iter()
        .chain(index.standard_symbols.iter())
    {
        if seen.insert(definition.canonical_name.clone()) {
            completions.push(CompletionItem {
                label: definition.name.clone(),
                kind: definition.kind.clone(),
                detail: definition.detail.clone(),
            });
        }
    }
    completions
}

fn references_for_document(
    document: &DocumentAnalysis,
    definition_by_name: &BTreeMap<String, SourceRange>,
) -> Vec<SymbolReference> {
    document
        .source
        .tokens
        .iter()
        .filter(|token| {
            matches!(
                token.kind,
                SourceTokenKind::Identifier
                    | SourceTokenKind::Keyword
                    | SourceTokenKind::DirectVariable
            )
        })
        .map(|token| {
            let canonical = canonical_identifier(&token.lexeme);
            SymbolReference {
                name: token.lexeme.clone(),
                canonical_name: canonical.clone(),
                uri: document.uri.clone(),
                range: token.range.clone(),
                definition: definition_by_name.get(&canonical).cloned(),
                container_name: document
                    .symbols
                    .iter()
                    .filter_map(|symbol| {
                        symbol
                            .range
                            .as_ref()
                            .map(|range| (range.start, symbol.name.clone()))
                    })
                    .take_while(|(start, _)| *start <= token.range.start)
                    .last()
                    .map(|(_, name)| name),
                write: is_write_reference(&document.source.text, token.range.start),
            }
        })
        .collect()
}

fn scopes_for_document(document: &DocumentAnalysis) -> Vec<ScopeInfo> {
    let mut scopes = BTreeMap::<String, ScopeInfo>::new();
    for symbol in &document.symbols {
        if matches!(
            symbol.kind,
            SymbolKind::Program
                | SymbolKind::Function
                | SymbolKind::FunctionBlock
                | SymbolKind::Configuration
                | SymbolKind::Resource
        ) {
            scopes.insert(
                symbol.name.clone(),
                ScopeInfo {
                    name: symbol.name.clone(),
                    uri: document.uri.clone(),
                    range: symbol.range.clone(),
                    symbols: Vec::new(),
                },
            );
        } else if let Some(container) = &symbol.container_name {
            scopes
                .entry(container.clone())
                .or_insert_with(|| ScopeInfo {
                    name: container.clone(),
                    uri: document.uri.clone(),
                    range: None,
                    symbols: Vec::new(),
                })
                .symbols
                .push(symbol.name.clone());
        }
    }
    scopes.into_values().collect()
}

fn direct_locations_for_document(document: &DocumentAnalysis) -> Vec<DirectLocationSymbol> {
    document
        .symbols
        .iter()
        .filter_map(|symbol| {
            let at = symbol.detail.find(" AT ")?;
            let location = symbol.detail[at + 4..]
                .split_whitespace()
                .next()
                .unwrap_or_default()
                .to_string();
            Some(DirectLocationSymbol {
                location,
                uri: document.uri.clone(),
                range: symbol.range.clone(),
                type_name: symbol.detail.split(':').nth(1).map(|value| {
                    value
                        .split(" AT ")
                        .next()
                        .unwrap_or(value)
                        .trim()
                        .to_string()
                }),
            })
        })
        .collect()
}

fn access_paths_for_document(document: &DocumentAnalysis) -> Vec<AccessPathSymbol> {
    document
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::AccessPath)
        .filter_map(|symbol| {
            let (prefix, target) = symbol.detail.split_once(" -> ")?;
            let direction = if prefix.contains("READ_WRITE") {
                "READ_WRITE"
            } else {
                "READ_ONLY"
            };
            Some(AccessPathSymbol {
                name: symbol.name.clone(),
                uri: document.uri.clone(),
                target: target.to_string(),
                direction: direction.to_string(),
                range: symbol.range.clone(),
            })
        })
        .collect()
}

fn add_pou_member_scopes(project: &Project, index: &mut SymbolIndex) {
    let mut known = index
        .scopes
        .iter()
        .map(|scope| scope.name.clone())
        .collect::<BTreeSet<_>>();
    for pou in project.pous() {
        if known.insert(pou.name.original.clone()) {
            index.scopes.push(ScopeInfo {
                name: pou.name.original.clone(),
                uri: String::new(),
                range: None,
                symbols: pou
                    .variable_declarations()
                    .map(|var| var.name.original.clone())
                    .collect(),
            });
        }
    }
}

fn collect_calls_in_statements(statements: &[Statement], calls: &mut BTreeSet<String>) {
    for statement in statements {
        match statement {
            Statement::Assignment { value, .. } => collect_calls_in_expr(value, calls),
            Statement::FbCall { name, args } => {
                calls.insert(name.to_string());
                collect_calls_in_args(args, calls);
            }
            Statement::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    collect_calls_in_expr(condition, calls);
                    collect_calls_in_statements(body, calls);
                }
                collect_calls_in_statements(else_branch, calls);
            }
            Statement::Case {
                selector,
                cases,
                else_branch,
            } => {
                collect_calls_in_expr(selector, calls);
                for (labels, body) in cases {
                    for label in labels {
                        match label {
                            iec_ir::CaseLabel::Single(expr) => collect_calls_in_expr(expr, calls),
                            iec_ir::CaseLabel::Range(low, high) => {
                                collect_calls_in_expr(low, calls);
                                collect_calls_in_expr(high, calls);
                            }
                        }
                    }
                    collect_calls_in_statements(body, calls);
                }
                collect_calls_in_statements(else_branch, calls);
            }
            Statement::For {
                from, to, by, body, ..
            } => {
                collect_calls_in_expr(from, calls);
                collect_calls_in_expr(to, calls);
                if let Some(by) = by {
                    collect_calls_in_expr(by, calls);
                }
                collect_calls_in_statements(body, calls);
            }
            Statement::While { condition, body } => {
                collect_calls_in_expr(condition, calls);
                collect_calls_in_statements(body, calls);
            }
            Statement::Repeat { body, until } => {
                collect_calls_in_statements(body, calls);
                collect_calls_in_expr(until, calls);
            }
            Statement::Il { operand, .. } => {
                if let Some(operand) = operand {
                    collect_calls_in_expr(operand, calls);
                }
            }
            Statement::Empty
            | Statement::IlLabel(_)
            | Statement::Exit
            | Statement::Return
            | Statement::Unsupported(_) => {}
        }
    }
}

fn collect_calls_in_expr(expr: &Expr, calls: &mut BTreeSet<String>) {
    match expr {
        Expr::Call { name, args } => {
            calls.insert(name.original.clone());
            collect_calls_in_args(args, calls);
        }
        Expr::Unary { expr, .. } => collect_calls_in_expr(expr, calls),
        Expr::Binary { left, right, .. } => {
            collect_calls_in_expr(left, calls);
            collect_calls_in_expr(right, calls);
        }
        Expr::ArrayLiteral(elements) => {
            for element in elements {
                collect_calls_in_expr(element, calls);
            }
        }
        Expr::StructLiteral(args) => collect_calls_in_args(args, calls),
        Expr::Literal(_) | Expr::Variable(_) => {}
    }
}

fn collect_calls_in_args(args: &[ParamAssignment], calls: &mut BTreeSet<String>) {
    for arg in args {
        if let Some(expr) = &arg.expr {
            collect_calls_in_expr(expr, calls);
        }
    }
}

fn is_write_reference(text: &str, start: usize) -> bool {
    text[start..].trim_start().starts_with(":=")
        || text[..start]
            .rsplit_once('\n')
            .map(|(_, tail)| tail.contains(":="))
            .unwrap_or_else(|| text[..start].contains(":="))
}

fn is_valid_identifier(input: &str) -> bool {
    let mut chars = input.chars();
    chars
        .next()
        .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

#[allow(dead_code)]
fn _variable_ref_name(variable: &VariableRef) -> String {
    variable.to_string()
}
