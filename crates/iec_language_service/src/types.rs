// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::BTreeMap;

use iec_diagnostics::json_escape;
use iec_ir::{canonical_identifier, DataTypeSpec, Expr, LibraryElement, Literal, Project};
use iec_stdlib::{standard_symbols, StandardSymbolKind};

use crate::source::SourceTokenKind;
use crate::{DocumentAnalysis, SourceRange, WorkspaceAnalysis};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeIndex {
    pub uri: String,
    pub entries: Vec<TypeInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
    pub name: Option<String>,
    pub kind: TypeInfoKind,
    pub type_name: String,
    pub detail: String,
    pub range: Option<SourceRange>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeInfoKind {
    Variable,
    Expression,
    FunctionCall,
    FunctionBlockInstance,
    ArrayIndex,
    StructureField,
    EnumLiteral,
    StandardFunctionOverload,
}

impl TypeIndex {
    pub fn type_at(&self, offset: usize) -> Option<TypeInfo> {
        self.entries
            .iter()
            .find(|entry| {
                entry
                    .range
                    .as_ref()
                    .is_some_and(|range| range.start <= offset && offset <= range.end)
            })
            .cloned()
    }

    pub fn to_json(&self) -> String {
        let entries = self
            .entries
            .iter()
            .map(TypeInfo::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"uri\":\"{}\",\"entries\":[{}]}}",
            json_escape(&self.uri),
            entries
        )
    }
}

impl TypeInfo {
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
            "{{\"name\":{},\"kind\":\"{}\",\"typeName\":\"{}\",\"detail\":\"{}\",\"range\":{}}}",
            name,
            self.kind.as_str(),
            json_escape(&self.type_name),
            json_escape(&self.detail),
            range
        )
    }
}

impl TypeInfoKind {
    pub fn as_str(self) -> &'static str {
        match self {
            TypeInfoKind::Variable => "variable",
            TypeInfoKind::Expression => "expression",
            TypeInfoKind::FunctionCall => "functionCall",
            TypeInfoKind::FunctionBlockInstance => "functionBlockInstance",
            TypeInfoKind::ArrayIndex => "arrayIndex",
            TypeInfoKind::StructureField => "structureField",
            TypeInfoKind::EnumLiteral => "enumLiteral",
            TypeInfoKind::StandardFunctionOverload => "standardFunctionOverload",
        }
    }
}

pub fn document_type_index(analysis: &DocumentAnalysis) -> TypeIndex {
    let mut variable_types = project_variable_types(&analysis.project);
    let mut entries = Vec::new();

    for symbol in &analysis.symbols {
        if matches!(
            symbol.kind,
            crate::SymbolKind::Variable | crate::SymbolKind::AccessPath
        ) {
            let type_name = variable_types
                .remove(&canonical_identifier(&symbol.name))
                .unwrap_or_else(|| type_from_symbol_detail(&symbol.detail));
            entries.push(TypeInfo {
                name: Some(symbol.name.clone()),
                kind: TypeInfoKind::Variable,
                type_name,
                detail: symbol.detail.clone(),
                range: symbol.range.clone(),
            });
        }
    }

    entries.extend(type_entries_from_project(&analysis.project));
    entries.extend(type_entries_from_tokens(analysis));

    TypeIndex {
        uri: analysis.uri.clone(),
        entries,
    }
}

pub fn workspace_type_indexes(analysis: &WorkspaceAnalysis) -> Vec<TypeIndex> {
    analysis.documents.iter().map(document_type_index).collect()
}

fn type_entries_from_project(project: &Project) -> Vec<TypeInfo> {
    let mut entries = Vec::new();
    for data_type in project.data_types() {
        match &data_type.spec {
            DataTypeSpec::Struct { fields } => {
                for field in fields {
                    entries.push(TypeInfo {
                        name: Some(field.name.original.clone()),
                        kind: TypeInfoKind::StructureField,
                        type_name: type_detail(&field.spec),
                        detail: format!(
                            "field {} of {}",
                            field.name.original, data_type.name.original
                        ),
                        range: None,
                    });
                }
            }
            DataTypeSpec::Enum { values } => {
                for value in values {
                    entries.push(TypeInfo {
                        name: Some(value.original.clone()),
                        kind: TypeInfoKind::EnumLiteral,
                        type_name: data_type.name.original.clone(),
                        detail: format!("enum literal of {}", data_type.name.original),
                        range: None,
                    });
                }
            }
            _ => {}
        }
    }

    for pou in project.pous() {
        for var in pou.variable_declarations() {
            if matches!(var.type_spec, DataTypeSpec::Array { .. }) {
                entries.push(TypeInfo {
                    name: Some(var.name.original.clone()),
                    kind: TypeInfoKind::ArrayIndex,
                    type_name: type_detail(&var.type_spec),
                    detail: "array variable supports indexed access".to_string(),
                    range: None,
                });
            }
            if let DataTypeSpec::Named(name) = &var.type_spec {
                if project.find_pou(&name.original).is_some() {
                    entries.push(TypeInfo {
                        name: Some(var.name.original.clone()),
                        kind: TypeInfoKind::FunctionBlockInstance,
                        type_name: name.original.clone(),
                        detail: format!("function block instance of {}", name.original),
                        range: None,
                    });
                }
            }
        }
        for statement in &pou.body.statements {
            collect_expression_types(statement, &mut entries);
        }
    }

    for symbol in standard_symbols() {
        let detail = match symbol.kind {
            StandardSymbolKind::Function => "standard function overload",
            StandardSymbolKind::FunctionBlock => "standard function block",
        };
        entries.push(TypeInfo {
            name: Some(symbol.name.to_string()),
            kind: TypeInfoKind::StandardFunctionOverload,
            type_name: standard_return_type(symbol.name).to_string(),
            detail: format!("{detail}; IEC clause {}", symbol.clause),
            range: None,
        });
    }
    entries
}

fn type_entries_from_tokens(analysis: &DocumentAnalysis) -> Vec<TypeInfo> {
    let variable_types = project_variable_types(&analysis.project);
    analysis
        .source
        .tokens
        .iter()
        .filter_map(|token| match token.kind {
            SourceTokenKind::Identifier | SourceTokenKind::DirectVariable => {
                let canonical = canonical_identifier(&token.lexeme);
                variable_types.get(&canonical).map(|type_name| TypeInfo {
                    name: Some(token.lexeme.clone()),
                    kind: TypeInfoKind::Expression,
                    type_name: type_name.clone(),
                    detail: "identifier expression".to_string(),
                    range: Some(token.range.clone()),
                })
            }
            SourceTokenKind::Number => Some(TypeInfo {
                name: None,
                kind: TypeInfoKind::Expression,
                type_name: if token.lexeme.contains('.') {
                    "REAL".to_string()
                } else {
                    "INT".to_string()
                },
                detail: "numeric literal".to_string(),
                range: Some(token.range.clone()),
            }),
            SourceTokenKind::StringLiteral => Some(TypeInfo {
                name: None,
                kind: TypeInfoKind::Expression,
                type_name: "STRING".to_string(),
                detail: "string literal".to_string(),
                range: Some(token.range.clone()),
            }),
            _ => None,
        })
        .collect()
}

fn collect_expression_types(statement: &iec_ir::Statement, entries: &mut Vec<TypeInfo>) {
    match statement {
        iec_ir::Statement::Assignment { value, .. } => collect_expr_type(value, entries),
        iec_ir::Statement::FbCall { name, args } => {
            entries.push(TypeInfo {
                name: Some(name.to_string()),
                kind: TypeInfoKind::FunctionCall,
                type_name: "FUNCTION_BLOCK".to_string(),
                detail: format!("call to {name}"),
                range: None,
            });
            for arg in args {
                if let Some(expr) = &arg.expr {
                    collect_expr_type(expr, entries);
                }
            }
        }
        iec_ir::Statement::If {
            branches,
            else_branch,
        } => {
            for (condition, body) in branches {
                collect_expr_type(condition, entries);
                for statement in body {
                    collect_expression_types(statement, entries);
                }
            }
            for statement in else_branch {
                collect_expression_types(statement, entries);
            }
        }
        iec_ir::Statement::For {
            from, to, by, body, ..
        } => {
            collect_expr_type(from, entries);
            collect_expr_type(to, entries);
            if let Some(by) = by {
                collect_expr_type(by, entries);
            }
            for statement in body {
                collect_expression_types(statement, entries);
            }
        }
        iec_ir::Statement::While { condition, body } => {
            collect_expr_type(condition, entries);
            for statement in body {
                collect_expression_types(statement, entries);
            }
        }
        iec_ir::Statement::Repeat { body, until } => {
            for statement in body {
                collect_expression_types(statement, entries);
            }
            collect_expr_type(until, entries);
        }
        _ => {}
    }
}

fn collect_expr_type(expr: &Expr, entries: &mut Vec<TypeInfo>) {
    let (type_name, detail) = infer_expr_type(expr);
    entries.push(TypeInfo {
        name: expression_name(expr),
        kind: match expr {
            Expr::Call { .. } => TypeInfoKind::FunctionCall,
            _ => TypeInfoKind::Expression,
        },
        type_name,
        detail,
        range: None,
    });
    match expr {
        Expr::Unary { expr, .. } => collect_expr_type(expr, entries),
        Expr::Binary { left, right, .. } => {
            collect_expr_type(left, entries);
            collect_expr_type(right, entries);
        }
        Expr::Call { args, .. } | Expr::StructLiteral(args) => {
            for arg in args {
                if let Some(expr) = &arg.expr {
                    collect_expr_type(expr, entries);
                }
            }
        }
        Expr::ArrayLiteral(elements) => {
            for element in elements {
                collect_expr_type(element, entries);
            }
        }
        Expr::Literal(_) | Expr::Variable(_) => {}
    }
}

fn infer_expr_type(expr: &Expr) -> (String, String) {
    match expr {
        Expr::Literal(literal) => (literal_type(literal), "literal".to_string()),
        Expr::Variable(variable) => (String::new(), format!("variable reference {variable}")),
        Expr::Unary { .. } | Expr::Binary { .. } => (
            "numeric_or_bool".to_string(),
            "operator expression".to_string(),
        ),
        Expr::Call { name, .. } => (
            standard_return_type(&name.original).to_string(),
            format!("call expression {}", name.original),
        ),
        Expr::ArrayLiteral(_) => ("ARRAY".to_string(), "array literal".to_string()),
        Expr::StructLiteral(_) => ("STRUCT".to_string(), "structure literal".to_string()),
    }
}

fn expression_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Variable(variable) => Some(variable.to_string()),
        Expr::Call { name, .. } => Some(name.original.clone()),
        _ => None,
    }
}

fn literal_type(literal: &Literal) -> String {
    match literal {
        Literal::Int(_) => "INT".to_string(),
        Literal::Real(_) => "REAL".to_string(),
        Literal::Bool(_) => "BOOL".to_string(),
        Literal::String(_) => "STRING".to_string(),
        Literal::WString(_) => "WSTRING".to_string(),
        Literal::DurationMs(_) => "TIME".to_string(),
        Literal::Date(_) => "DATE".to_string(),
        Literal::TimeOfDay(_) => "TIME_OF_DAY".to_string(),
        Literal::DateAndTime(_) => "DATE_AND_TIME".to_string(),
        Literal::Typed { type_name, .. } => type_name.original.clone(),
    }
}

fn standard_return_type(name: &str) -> &'static str {
    match canonical_identifier(name).as_str() {
        "GT" | "GE" | "EQ" | "NE" | "LE" | "LT" | "AND" | "OR" | "XOR" | "NOT" => "BOOL",
        "LEN" | "FIND" => "INT",
        "LEFT" | "RIGHT" | "MID" | "CONCAT" | "INSERT" | "DELETE" | "REPLACE" => "STRING",
        "TON" | "TOF" | "TP" | "CTU" | "CTD" | "CTUD" | "SR" | "RS" | "R_TRIG" | "F_TRIG" => {
            "FUNCTION_BLOCK"
        }
        _ => "ANY",
    }
}

fn project_variable_types(project: &Project) -> BTreeMap<String, String> {
    let mut types = BTreeMap::new();
    for element in &project.library_elements {
        match element {
            LibraryElement::Pou(pou) => {
                for var in pou.variable_declarations() {
                    types.insert(var.name.canonical.clone(), type_detail(&var.type_spec));
                }
            }
            LibraryElement::Configuration(configuration) => {
                for block in &configuration.var_blocks {
                    for var in &block.vars {
                        types.insert(var.name.canonical.clone(), type_detail(&var.type_spec));
                    }
                }
                for resource in &configuration.resources {
                    for block in &resource.var_blocks {
                        for var in &block.vars {
                            types.insert(var.name.canonical.clone(), type_detail(&var.type_spec));
                        }
                    }
                }
            }
            LibraryElement::DataType(_) => {}
        }
    }
    types
}

fn type_from_symbol_detail(detail: &str) -> String {
    detail
        .split(':')
        .nth(1)
        .map(|value| {
            value
                .split(" AT ")
                .next()
                .unwrap_or(value)
                .trim()
                .to_string()
        })
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "UNKNOWN".to_string())
}

fn type_detail(spec: &DataTypeSpec) -> String {
    match spec {
        DataTypeSpec::Elementary(elementary) => elementary.as_iec().to_string(),
        DataTypeSpec::Named(name) => name.original.clone(),
        DataTypeSpec::Array {
            ranges,
            element_type,
        } => {
            let ranges = ranges
                .iter()
                .map(|range| format!("{}..{}", range.low, range.high))
                .collect::<Vec<_>>()
                .join(", ");
            format!("ARRAY [{ranges}] OF {}", type_detail(element_type))
        }
        DataTypeSpec::Struct { fields } => {
            let fields = fields
                .iter()
                .map(|field| format!("{}: {}", field.name.original, type_detail(&field.spec)))
                .collect::<Vec<_>>()
                .join("; ");
            format!("STRUCT {fields} END_STRUCT")
        }
        DataTypeSpec::Enum { values } => {
            let values = values
                .iter()
                .map(|value| value.original.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("({values})")
        }
        DataTypeSpec::Subrange { base, range } => {
            format!("{} ({}..{})", base.as_iec(), range.low, range.high)
        }
        DataTypeSpec::String { wide, length } => {
            let name = if *wide { "WSTRING" } else { "STRING" };
            match length {
                Some(length) => format!("{name}[{length}]"),
                None => name.to_string(),
            }
        }
    }
}
