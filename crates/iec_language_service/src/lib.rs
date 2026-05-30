// SPDX-License-Identifier: MIT OR Apache-2.0

mod actions;
mod c_metadata;
mod capabilities;
mod debug;
mod diagnostics_ext;
mod docs;
mod formatter;
mod graph;
mod incremental;
mod refactor;
mod runtime_service;
mod source;
mod symbols;
mod types;
mod workspace;

pub use actions::{code_actions, CodeAction, CodeActionKind};
pub use c_metadata::{
    generate_c_artifact, generated_c_metadata, CAccessPath, CDebugSymbol, CEntrypoint, CIoSymbol,
    CStateField, GeneratedCArtifact, GeneratedCMetadata,
};
pub use capabilities::ServiceCapabilities;
pub use debug::{
    debug_document, DebugAccessPath, DebugAccessWrite, DebugCycle, DebugOptions, DebugTrace,
};
pub use diagnostics_ext::{
    diagnostic_descriptor, diagnostic_descriptors, DiagnosticDescriptor, DiagnosticLabel,
    DiagnosticLabelRole,
};
pub use docs::{
    compliance_profile_note, elementary_type_documentation, keyword_documentation,
    standard_symbol_documentation,
};
pub use formatter::{format_document, FormattedDocument};
pub use graph::{
    document_graph_model, validate_graph_model, workspace_graph_model, GraphEdge, GraphModel,
    GraphNetwork, GraphNode, GraphPoint, GraphPou, GraphSize, GraphValidation, PlcOpenLayout,
    SfcActionNode, SfcGraph, SfcStepNode, SfcTransitionNode,
};
pub use incremental::{CachedDocument, DependencyEdge, IncrementalCache, IncrementalUpdate};
pub use refactor::{
    change_variable_type_plan, extract_pou_plan, introduce_variable_plan, rename_symbol_plan,
    RefactorPlan,
};
pub use runtime_service::{
    simulate_document, DocumentSimulation, SimulationCycle, SimulationVariable,
};
pub use source::{
    analyze_source_document, source_map_for_analysis, SourceDocument, SourceMap,
    SourceMappedObject, SourceMappedObjectKind, SourceNode, SourceNodeKind, SourceToken,
    SourceTokenKind,
};
pub use symbols::{
    call_hierarchy, completion_items_from_index, document_symbol_index, workspace_symbol_index,
    AccessPathSymbol, CallHierarchyItem, DirectLocationSymbol, RenameValidation, ScopeInfo,
    SymbolDefinition, SymbolIndex, SymbolReference, TextEdit,
};
pub use types::{document_type_index, workspace_type_indexes, TypeIndex, TypeInfo, TypeInfoKind};
pub use workspace::{
    analyze_workspace, analyze_workspace_model, merge_projects, workspace_diagnostics_json,
    workspace_document_count, WorkspaceAnalysis, WorkspaceDiagnosticEntry, WorkspaceFile,
    WorkspaceFileKind, WorkspaceModel, WorkspaceRoot,
};

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

use iec_diagnostics::{diagnostics_to_json, json_escape, Diagnostic};
use iec_ir::{
    canonical_identifier, AccessDirection, DataTypeSpec, Identifier, ImplementationLanguage,
    LibraryElement, Pou, PouKind, Project, Sfc, VarBlockKind, VarDecl,
};
use iec_plcopen::import_plcopen_xml;
use iec_profile::{EditionProfile, ImplementationParameters};
use iec_semantics::{check_project, CheckOptions};
use iec_stdlib::{standard_symbols, StandardSymbolKind};
use iec_syntax::{parse_project_with_options, ParseOptions};

#[derive(Debug, Clone, Default)]
pub struct LanguageServiceOptions {
    pub profile: EditionProfile,
    pub implementation: ImplementationParameters,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentInput {
    pub uri: String,
    pub text: String,
    pub language_id: Option<String>,
}

impl DocumentInput {
    pub fn new(uri: impl Into<String>, text: impl Into<String>) -> Self {
        Self {
            uri: uri.into(),
            text: text.into(),
            language_id: None,
        }
    }

    pub fn with_language_id(mut self, language_id: impl Into<String>) -> Self {
        self.language_id = Some(language_id.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceRange {
    pub uri: String,
    pub start: usize,
    pub end: usize,
    pub start_position: Position,
    pub end_position: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SymbolKind {
    DataType,
    Function,
    FunctionBlock,
    Program,
    Configuration,
    Resource,
    Task,
    ProgramInstance,
    Variable,
    AccessPath,
    SfcStep,
    SfcAction,
    StandardFunction,
    StandardFunctionBlock,
    Keyword,
    ElementaryType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSymbol {
    pub name: String,
    pub kind: SymbolKind,
    pub detail: String,
    pub container_name: Option<String>,
    pub range: Option<SourceRange>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    pub kind: SymbolKind,
    pub detail: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hover {
    pub contents: String,
    pub range: Option<SourceRange>,
}

#[derive(Debug, Clone)]
pub struct DocumentAnalysis {
    pub uri: String,
    pub project: Project,
    pub diagnostics: Vec<Diagnostic>,
    pub source: SourceDocument,
    pub source_map: SourceMap,
    pub symbols: Vec<DocumentSymbol>,
    pub completions: Vec<CompletionItem>,
}

impl DocumentAnalysis {
    pub fn hover_at(&self, offset: usize) -> Option<Hover> {
        docs::hover_for_offset(self, offset)
    }

    pub fn symbol_hover_at(&self, offset: usize) -> Option<Hover> {
        self.symbols
            .iter()
            .filter_map(|symbol| symbol.range.as_ref().map(|range| (symbol, range)))
            .find(|(_, range)| range.start <= offset && offset <= range.end)
            .map(|(symbol, range)| Hover {
                contents: symbol_hover(symbol),
                range: Some(range.clone()),
            })
    }

    pub fn completions_with_prefix(&self, prefix: &str) -> Vec<CompletionItem> {
        let canonical_prefix = canonical_identifier(prefix);
        self.completions
            .iter()
            .filter(|item| {
                canonical_prefix.is_empty()
                    || canonical_identifier(&item.label).starts_with(&canonical_prefix)
            })
            .cloned()
            .collect()
    }

    pub fn to_json(&self) -> String {
        let symbols = self
            .symbols
            .iter()
            .map(DocumentSymbol::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let completions = self
            .completions
            .iter()
            .map(CompletionItem::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"uri\":\"{}\",\"diagnostics\":{},\"source\":{},\"sourceMap\":{},\"symbols\":[{}],\"completions\":[{}]}}",
            json_escape(&self.uri),
            diagnostics_to_json(&self.diagnostics),
            self.source.to_json(),
            self.source_map.to_json(),
            symbols,
            completions
        )
    }
}

impl SourceRange {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"uri\":\"{}\",\"start\":{},\"end\":{},\"startPosition\":{},\"endPosition\":{}}}",
            json_escape(&self.uri),
            self.start,
            self.end,
            self.start_position.to_json(),
            self.end_position.to_json()
        )
    }
}

impl Position {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"line\":{},\"character\":{}}}",
            self.line, self.character
        )
    }
}

impl SymbolKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            SymbolKind::DataType => "dataType",
            SymbolKind::Function => "function",
            SymbolKind::FunctionBlock => "functionBlock",
            SymbolKind::Program => "program",
            SymbolKind::Configuration => "configuration",
            SymbolKind::Resource => "resource",
            SymbolKind::Task => "task",
            SymbolKind::ProgramInstance => "programInstance",
            SymbolKind::Variable => "variable",
            SymbolKind::AccessPath => "accessPath",
            SymbolKind::SfcStep => "sfcStep",
            SymbolKind::SfcAction => "sfcAction",
            SymbolKind::StandardFunction => "standardFunction",
            SymbolKind::StandardFunctionBlock => "standardFunctionBlock",
            SymbolKind::Keyword => "keyword",
            SymbolKind::ElementaryType => "elementaryType",
        }
    }
}

impl DocumentSymbol {
    pub fn to_json(&self) -> String {
        let container = self
            .container_name
            .as_ref()
            .map(|container| format!("\"{}\"", json_escape(container)))
            .unwrap_or_else(|| "null".to_string());
        let range = self
            .range
            .as_ref()
            .map(SourceRange::to_json)
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"name\":\"{}\",\"kind\":\"{}\",\"detail\":\"{}\",\"containerName\":{},\"range\":{}}}",
            json_escape(&self.name),
            self.kind.as_str(),
            json_escape(&self.detail),
            container,
            range
        )
    }
}

impl CompletionItem {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"label\":\"{}\",\"kind\":\"{}\",\"detail\":\"{}\"}}",
            json_escape(&self.label),
            self.kind.as_str(),
            json_escape(&self.detail)
        )
    }
}

impl Hover {
    pub fn to_json(&self) -> String {
        let range = self
            .range
            .as_ref()
            .map(SourceRange::to_json)
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"contents\":\"{}\",\"range\":{}}}",
            json_escape(&self.contents),
            range
        )
    }
}

#[derive(Debug, Default, Clone)]
pub struct LanguageService {
    options: LanguageServiceOptions,
}

impl LanguageService {
    pub fn new(options: LanguageServiceOptions) -> Self {
        Self { options }
    }

    pub fn analyze_document(&self, input: DocumentInput) -> DocumentAnalysis {
        analyze_document(input, &self.options)
    }

    pub fn analyze_path(&self, path: impl AsRef<Path>) -> Result<DocumentAnalysis, String> {
        let path = path.as_ref();
        let text = fs::read_to_string(path).map_err(|err| err.to_string())?;
        let input = DocumentInput::new(path.to_string_lossy(), text);
        Ok(self.analyze_document(input))
    }

    pub fn simulate_document(&self, input: DocumentInput, cycles: usize) -> DocumentSimulation {
        simulate_document(input, &self.options, cycles)
    }

    pub fn analyze_workspace(&self, inputs: Vec<DocumentInput>) -> WorkspaceAnalysis {
        analyze_workspace(inputs, &self.options)
    }

    pub fn analyze_workspace_model(&self, model: WorkspaceModel) -> WorkspaceAnalysis {
        analyze_workspace_model(model, &self.options)
    }

    pub fn load_workspace(&self, root: impl AsRef<Path>) -> Result<WorkspaceAnalysis, String> {
        let model = WorkspaceModel::load_from_root(root)?;
        Ok(self.analyze_workspace_model(model))
    }

    pub fn capabilities(&self) -> ServiceCapabilities {
        ServiceCapabilities::for_options(&self.options)
    }

    pub fn generate_document_c(&self, input: DocumentInput) -> Result<String, Vec<Diagnostic>> {
        runtime_service::generate_document_c_from_input(input, &self.options)
    }

    pub fn generate_document_c_artifact(&self, input: DocumentInput) -> GeneratedCArtifact {
        generate_c_artifact(input, &self.options)
    }

    pub fn debug_document(&self, input: DocumentInput, debug: DebugOptions) -> DebugTrace {
        debug_document(input, &self.options, debug)
    }

    pub fn format_document(&self, input: DocumentInput) -> FormattedDocument {
        format_document(input)
    }

    pub fn document_symbol_index(&self, input: DocumentInput) -> SymbolIndex {
        document_symbol_index(&self.analyze_document(input))
    }

    pub fn workspace_symbol_index(&self, inputs: Vec<DocumentInput>) -> SymbolIndex {
        workspace_symbol_index(&self.analyze_workspace(inputs))
    }

    pub fn goto_definition(&self, input: DocumentInput, offset: usize) -> Option<SymbolDefinition> {
        let analysis = self.analyze_document(input);
        document_symbol_index(&analysis).definition_at(&analysis.uri, offset)
    }

    pub fn find_references(&self, input: DocumentInput, offset: usize) -> Vec<SymbolReference> {
        let analysis = self.analyze_document(input);
        let index = document_symbol_index(&analysis);
        let Some(definition) = index.definition_at(&analysis.uri, offset) else {
            return Vec::new();
        };
        index.references_for(&definition.name)
    }

    pub fn workspace_symbols(
        &self,
        inputs: Vec<DocumentInput>,
        query: &str,
    ) -> Vec<SymbolDefinition> {
        workspace_symbol_index(&self.analyze_workspace(inputs)).workspace_symbols(query)
    }

    pub fn validate_rename(
        &self,
        input: DocumentInput,
        offset: usize,
        new_name: &str,
    ) -> RenameValidation {
        let analysis = self.analyze_document(input);
        document_symbol_index(&analysis).validate_rename(&analysis.uri, offset, new_name)
    }

    pub fn call_hierarchy(&self, inputs: Vec<DocumentInput>) -> Vec<CallHierarchyItem> {
        call_hierarchy(&self.analyze_workspace(inputs))
    }

    pub fn document_type_index(&self, input: DocumentInput) -> TypeIndex {
        document_type_index(&self.analyze_document(input))
    }

    pub fn graph_model(&self, input: DocumentInput) -> GraphModel {
        document_graph_model(&self.analyze_document(input))
    }

    pub fn validate_graph_edits(&self, input: DocumentInput) -> GraphValidation {
        let model = self.graph_model(input);
        validate_graph_model(&model)
    }

    pub fn code_actions(&self, input: DocumentInput) -> Vec<CodeAction> {
        code_actions(&self.analyze_document(input))
    }
}

pub fn analyze_document(
    input: DocumentInput,
    options: &LanguageServiceOptions,
) -> DocumentAnalysis {
    let uri = input.uri.clone();
    let mut parsed = if is_plcopen_document(&input) {
        let imported = import_plcopen_xml(&input.uri, &input.text);
        ParsedDocument {
            project: imported.project,
            diagnostics: imported.diagnostics,
        }
    } else {
        let output = parse_project_with_options(
            input.uri.clone(),
            &input.text,
            &ParseOptions {
                implementation: options.implementation.clone(),
            },
        );
        ParsedDocument {
            project: output.project,
            diagnostics: output.diagnostics,
        }
    };
    parsed.project.profile = options.profile;

    if !has_error_diagnostics(&parsed.diagnostics) {
        parsed.diagnostics.extend(check_project(
            &parsed.project,
            &CheckOptions {
                profile: options.profile,
                implementation: options.implementation.clone(),
            },
        ));
    }

    let symbols = collect_symbols(&input.uri, &input.text, &parsed.project);
    let completions = collect_completions(&parsed.project);
    let source = analyze_source_document(&input, &parsed.diagnostics);
    let mut analysis = DocumentAnalysis {
        uri,
        project: parsed.project,
        diagnostics: parsed.diagnostics,
        source,
        source_map: SourceMap {
            uri: input.uri,
            objects: Vec::new(),
        },
        symbols,
        completions,
    };
    analysis.source_map = source_map_for_analysis(&analysis);
    analysis
}

#[derive(Debug, Clone)]
struct ParsedDocument {
    project: Project,
    diagnostics: Vec<Diagnostic>,
}

fn is_plcopen_document(input: &DocumentInput) -> bool {
    input
        .language_id
        .as_deref()
        .is_some_and(|language_id| language_id.eq_ignore_ascii_case("xml"))
        || input
            .uri
            .rsplit('.')
            .next()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
}

pub(crate) fn has_error_diagnostics(diagnostics: &[Diagnostic]) -> bool {
    diagnostics
        .iter()
        .any(|diagnostic| diagnostic.severity == iec_diagnostics::Severity::Error)
}

fn collect_symbols(uri: &str, text: &str, project: &Project) -> Vec<DocumentSymbol> {
    let mut ranges = RangeIndex::new(uri, text);
    let mut symbols = Vec::new();

    for element in &project.library_elements {
        match element {
            LibraryElement::DataType(data_type) => {
                push_symbol(
                    &mut symbols,
                    &mut ranges,
                    &data_type.name,
                    SymbolKind::DataType,
                    type_detail(&data_type.spec),
                    None,
                );
            }
            LibraryElement::Pou(pou) => {
                collect_pou_symbols(&mut symbols, &mut ranges, pou);
            }
            LibraryElement::Configuration(configuration) => {
                push_symbol(
                    &mut symbols,
                    &mut ranges,
                    &configuration.name,
                    SymbolKind::Configuration,
                    "CONFIGURATION".to_string(),
                    None,
                );
                let configuration_name = Some(configuration.name.original.clone());
                collect_var_blocks(
                    &mut symbols,
                    &mut ranges,
                    &configuration.var_blocks,
                    configuration_name.clone(),
                );
                for resource in &configuration.resources {
                    push_symbol(
                        &mut symbols,
                        &mut ranges,
                        &resource.name,
                        SymbolKind::Resource,
                        "RESOURCE".to_string(),
                        configuration_name.clone(),
                    );
                    let resource_name = Some(resource.name.original.clone());
                    collect_var_blocks(
                        &mut symbols,
                        &mut ranges,
                        &resource.var_blocks,
                        resource_name.clone(),
                    );
                    for task in &resource.tasks {
                        push_symbol(
                            &mut symbols,
                            &mut ranges,
                            &task.name,
                            SymbolKind::Task,
                            "TASK".to_string(),
                            resource_name.clone(),
                        );
                    }
                    for instance in &resource.program_instances {
                        push_symbol(
                            &mut symbols,
                            &mut ranges,
                            &instance.name,
                            SymbolKind::ProgramInstance,
                            format!("PROGRAM {}", instance.program_type.original),
                            resource_name.clone(),
                        );
                    }
                }
            }
        }
    }

    symbols.sort_by(|left, right| {
        left.range
            .as_ref()
            .map(|range| range.start)
            .cmp(&right.range.as_ref().map(|range| range.start))
            .then_with(|| left.name.cmp(&right.name))
    });
    symbols
}

fn collect_pou_symbols(symbols: &mut Vec<DocumentSymbol>, ranges: &mut RangeIndex<'_>, pou: &Pou) {
    let (kind, detail) = match &pou.kind {
        PouKind::Function { return_type } => (
            SymbolKind::Function,
            format!("FUNCTION : {}", type_detail(return_type)),
        ),
        PouKind::FunctionBlock => (SymbolKind::FunctionBlock, "FUNCTION_BLOCK".to_string()),
        PouKind::Program => (SymbolKind::Program, "PROGRAM".to_string()),
    };
    push_symbol(symbols, ranges, &pou.name, kind, detail, None);
    collect_var_blocks(
        symbols,
        ranges,
        &pou.var_blocks,
        Some(pou.name.original.clone()),
    );
    collect_body_symbols(
        symbols,
        ranges,
        &pou.name.original,
        &pou.body.language,
        &pou.body.sfc,
    );
}

fn collect_body_symbols(
    symbols: &mut Vec<DocumentSymbol>,
    ranges: &mut RangeIndex<'_>,
    container: &str,
    language: &ImplementationLanguage,
    sfc: &Option<Sfc>,
) {
    if !matches!(language, ImplementationLanguage::SequentialFunctionChart) {
        return;
    }
    let Some(sfc) = sfc else {
        return;
    };

    for step in &sfc.steps {
        push_symbol(
            symbols,
            ranges,
            &step.name,
            SymbolKind::SfcStep,
            "SFC step".to_string(),
            Some(container.to_string()),
        );
    }
    for action in &sfc.actions {
        push_symbol(
            symbols,
            ranges,
            &action.name,
            SymbolKind::SfcAction,
            "SFC action".to_string(),
            Some(container.to_string()),
        );
    }
}

fn collect_var_blocks(
    symbols: &mut Vec<DocumentSymbol>,
    ranges: &mut RangeIndex<'_>,
    blocks: &[iec_ir::VarBlock],
    container_name: Option<String>,
) {
    for block in blocks {
        for var in &block.vars {
            collect_var_symbol(symbols, ranges, block.kind, var, container_name.clone());
        }
    }
}

fn collect_var_symbol(
    symbols: &mut Vec<DocumentSymbol>,
    ranges: &mut RangeIndex<'_>,
    block_kind: VarBlockKind,
    var: &VarDecl,
    container_name: Option<String>,
) {
    let kind = if block_kind == VarBlockKind::Access || var.access.is_some() {
        SymbolKind::AccessPath
    } else {
        SymbolKind::Variable
    };
    let mut detail = format!(
        "{} : {}",
        var_block_kind_label(block_kind),
        type_detail(&var.type_spec)
    );
    if let Some(location) = &var.location {
        detail.push_str(" AT ");
        detail.push_str(location);
    }
    if let Some(access) = &var.access {
        detail.push(' ');
        detail.push_str(match access.direction {
            AccessDirection::ReadOnly => "READ_ONLY",
            AccessDirection::ReadWrite => "READ_WRITE",
        });
        detail.push_str(" -> ");
        detail.push_str(&access.path);
    }
    push_symbol(symbols, ranges, &var.name, kind, detail, container_name);
}

fn push_symbol(
    symbols: &mut Vec<DocumentSymbol>,
    ranges: &mut RangeIndex<'_>,
    name: &Identifier,
    kind: SymbolKind,
    detail: String,
    container_name: Option<String>,
) {
    symbols.push(DocumentSymbol {
        name: name.original.clone(),
        kind,
        detail,
        container_name,
        range: ranges.claim_identifier(&name.original),
    });
}

fn collect_completions(project: &Project) -> Vec<CompletionItem> {
    let mut seen = BTreeSet::new();
    let mut items = Vec::new();

    for keyword in KEYWORDS {
        push_completion(
            &mut items,
            &mut seen,
            keyword,
            SymbolKind::Keyword,
            "IEC keyword",
        );
    }
    for ty in ELEMENTARY_TYPES {
        push_completion(
            &mut items,
            &mut seen,
            ty,
            SymbolKind::ElementaryType,
            "IEC elementary type",
        );
    }
    for qualifier in SFC_ACTION_QUALIFIERS {
        push_completion(
            &mut items,
            &mut seen,
            qualifier,
            SymbolKind::Keyword,
            "SFC action qualifier",
        );
    }
    for direct_form in DIRECT_VARIABLE_FORMS {
        push_completion(
            &mut items,
            &mut seen,
            direct_form,
            SymbolKind::AccessPath,
            "direct-variable address form",
        );
    }
    for symbol in standard_symbols() {
        let kind = match symbol.kind {
            StandardSymbolKind::Function => SymbolKind::StandardFunction,
            StandardSymbolKind::FunctionBlock => SymbolKind::StandardFunctionBlock,
        };
        push_completion(
            &mut items,
            &mut seen,
            symbol.name,
            kind,
            &format!("IEC standard library clause {}", symbol.clause),
        );
    }
    for element in &project.library_elements {
        match element {
            LibraryElement::DataType(data_type) => {
                push_completion(
                    &mut items,
                    &mut seen,
                    &data_type.name.original,
                    SymbolKind::DataType,
                    &type_detail(&data_type.spec),
                );
                match &data_type.spec {
                    DataTypeSpec::Struct { fields } => {
                        for field in fields {
                            push_completion(
                                &mut items,
                                &mut seen,
                                &field.name.original,
                                SymbolKind::Variable,
                                &format!(
                                    "field of {} : {}",
                                    data_type.name.original,
                                    type_detail(&field.spec)
                                ),
                            );
                        }
                    }
                    DataTypeSpec::Enum { values } => {
                        for value in values {
                            push_completion(
                                &mut items,
                                &mut seen,
                                &value.original,
                                SymbolKind::DataType,
                                &format!("enum value of {}", data_type.name.original),
                            );
                        }
                    }
                    _ => {}
                }
            }
            LibraryElement::Pou(pou) => {
                let (kind, detail) = match &pou.kind {
                    PouKind::Function { return_type } => (
                        SymbolKind::Function,
                        format!("FUNCTION : {}", type_detail(return_type)),
                    ),
                    PouKind::FunctionBlock => {
                        (SymbolKind::FunctionBlock, "FUNCTION_BLOCK".to_string())
                    }
                    PouKind::Program => (SymbolKind::Program, "PROGRAM".to_string()),
                };
                push_completion(&mut items, &mut seen, &pou.name.original, kind, &detail);
                for var in pou.variable_declarations() {
                    push_completion(
                        &mut items,
                        &mut seen,
                        &var.name.original,
                        SymbolKind::Variable,
                        &type_detail(&var.type_spec),
                    );
                }
            }
            LibraryElement::Configuration(configuration) => {
                push_completion(
                    &mut items,
                    &mut seen,
                    &configuration.name.original,
                    SymbolKind::Configuration,
                    "CONFIGURATION",
                );
            }
        }
    }

    items.sort_by(|left, right| left.label.cmp(&right.label));
    items
}

fn push_completion(
    items: &mut Vec<CompletionItem>,
    seen: &mut BTreeSet<String>,
    label: &str,
    kind: SymbolKind,
    detail: &str,
) {
    if seen.insert(canonical_identifier(label)) {
        items.push(CompletionItem {
            label: label.to_string(),
            kind,
            detail: detail.to_string(),
        });
    }
}

fn symbol_hover(symbol: &DocumentSymbol) -> String {
    if let Some(container) = &symbol.container_name {
        format!(
            "{} `{}` in `{}`\n\n{}",
            symbol_kind_label(&symbol.kind),
            symbol.name,
            container,
            symbol.detail
        )
    } else {
        format!(
            "{} `{}`\n\n{}",
            symbol_kind_label(&symbol.kind),
            symbol.name,
            symbol.detail
        )
    }
}

fn symbol_kind_label(kind: &SymbolKind) -> &'static str {
    match kind {
        SymbolKind::DataType => "Data type",
        SymbolKind::Function => "Function",
        SymbolKind::FunctionBlock => "Function block",
        SymbolKind::Program => "Program",
        SymbolKind::Configuration => "Configuration",
        SymbolKind::Resource => "Resource",
        SymbolKind::Task => "Task",
        SymbolKind::ProgramInstance => "Program instance",
        SymbolKind::Variable => "Variable",
        SymbolKind::AccessPath => "Access path",
        SymbolKind::SfcStep => "SFC step",
        SymbolKind::SfcAction => "SFC action",
        SymbolKind::StandardFunction => "Standard function",
        SymbolKind::StandardFunctionBlock => "Standard function block",
        SymbolKind::Keyword => "Keyword",
        SymbolKind::ElementaryType => "Elementary type",
    }
}

fn var_block_kind_label(kind: VarBlockKind) -> &'static str {
    match kind {
        VarBlockKind::Local => "VAR",
        VarBlockKind::Input => "VAR_INPUT",
        VarBlockKind::Output => "VAR_OUTPUT",
        VarBlockKind::InOut => "VAR_IN_OUT",
        VarBlockKind::External => "VAR_EXTERNAL",
        VarBlockKind::Global => "VAR_GLOBAL",
        VarBlockKind::Temp => "VAR_TEMP",
        VarBlockKind::Access => "VAR_ACCESS",
        VarBlockKind::Config => "VAR_CONFIG",
    }
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

#[derive(Debug)]
struct RangeIndex<'a> {
    uri: &'a str,
    text: &'a str,
    claimed: BTreeMap<String, usize>,
}

impl<'a> RangeIndex<'a> {
    fn new(uri: &'a str, text: &'a str) -> Self {
        Self {
            uri,
            text,
            claimed: BTreeMap::new(),
        }
    }

    fn claim_identifier(&mut self, name: &str) -> Option<SourceRange> {
        let start_at = self
            .claimed
            .get(&canonical_identifier(name))
            .copied()
            .unwrap_or(0);
        let offset = find_identifier_at_or_after(self.text, name, start_at)?;
        self.claimed.insert(
            canonical_identifier(name),
            offset.saturating_add(name.len()),
        );
        Some(SourceRange {
            uri: self.uri.to_string(),
            start: offset,
            end: offset + name.len(),
            start_position: position_at(self.text, offset),
            end_position: position_at(self.text, offset + name.len()),
        })
    }
}

fn find_identifier_at_or_after(text: &str, needle: &str, start_at: usize) -> Option<usize> {
    let canonical_needle = canonical_identifier(needle);
    text.char_indices()
        .skip_while(|(offset, _)| *offset < start_at)
        .find_map(|(offset, _)| {
            let candidate = text.get(offset..offset + needle.len())?;
            if canonical_identifier(candidate) == canonical_needle
                && is_identifier_boundary(text, offset)
                && is_identifier_boundary(text, offset + needle.len())
            {
                Some(offset)
            } else {
                None
            }
        })
}

fn is_identifier_boundary(text: &str, offset: usize) -> bool {
    if offset == 0 || offset >= text.len() {
        return true;
    }
    let before = text[..offset].chars().next_back();
    let after = text[offset..].chars().next();
    !before.is_some_and(is_identifier_part) || !after.is_some_and(is_identifier_part)
}

fn is_identifier_part(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

pub(crate) fn range_from_offsets(uri: &str, text: &str, start: usize, end: usize) -> SourceRange {
    SourceRange {
        uri: uri.to_string(),
        start,
        end,
        start_position: position_at(text, start),
        end_position: position_at(text, end),
    }
}

pub(crate) fn position_at(text: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut character = 0;
    for ch in text[..offset.min(text.len())].chars() {
        if ch == '\n' {
            line += 1;
            character = 0;
        } else {
            character += 1;
        }
    }
    Position { line, character }
}

pub(crate) const ELEMENTARY_TYPES: &[&str] = &[
    "BOOL",
    "SINT",
    "INT",
    "DINT",
    "LINT",
    "USINT",
    "UINT",
    "UDINT",
    "ULINT",
    "REAL",
    "LREAL",
    "BYTE",
    "WORD",
    "DWORD",
    "LWORD",
    "STRING",
    "WSTRING",
    "TIME",
    "DATE",
    "TIME_OF_DAY",
    "DATE_AND_TIME",
];

pub(crate) const KEYWORDS: &[&str] = &[
    "TYPE",
    "END_TYPE",
    "FUNCTION",
    "END_FUNCTION",
    "FUNCTION_BLOCK",
    "END_FUNCTION_BLOCK",
    "PROGRAM",
    "END_PROGRAM",
    "CONFIGURATION",
    "END_CONFIGURATION",
    "RESOURCE",
    "END_RESOURCE",
    "TASK",
    "VAR",
    "VAR_INPUT",
    "VAR_OUTPUT",
    "VAR_IN_OUT",
    "VAR_EXTERNAL",
    "VAR_GLOBAL",
    "VAR_TEMP",
    "VAR_ACCESS",
    "VAR_CONFIG",
    "END_VAR",
    "CONSTANT",
    "RETAIN",
    "NON_RETAIN",
    "IF",
    "THEN",
    "ELSIF",
    "ELSE",
    "END_IF",
    "CASE",
    "OF",
    "END_CASE",
    "FOR",
    "TO",
    "BY",
    "DO",
    "END_FOR",
    "WHILE",
    "END_WHILE",
    "REPEAT",
    "UNTIL",
    "END_REPEAT",
    "EXIT",
    "RETURN",
    "LADDER",
    "END_LADDER",
    "RUNG",
    "END_RUNG",
    "CONTACT",
    "CONTACT_NOT",
    "COIL",
    "SET",
    "RESET",
    "FBD",
    "END_FBD",
    "NETWORK",
    "END_NETWORK",
    "OUT",
    "INITIAL_STEP",
    "STEP",
    "END_STEP",
    "TRANSITION",
    "END_TRANSITION",
    "ACTION",
    "END_ACTION",
    "FROM",
    "TO",
    "READ_ONLY",
    "READ_WRITE",
];

const SFC_ACTION_QUALIFIERS: &[&str] =
    &["N", "S", "R", "P", "P0", "P1", "L", "D", "SD", "DS", "SL"];

const DIRECT_VARIABLE_FORMS: &[&str] = &["%I", "%Q", "%M", "%IX0.0", "%QX0.0", "%MW0", "%MD0"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analyzes_symbols_diagnostics_and_completions() {
        let source = r#"
PROGRAM Demo
VAR
    Count : INT;
END_VAR
Count := Count + 1;
END_PROGRAM
"#;
        let service = LanguageService::default();
        let analysis = service.analyze_document(DocumentInput::new("demo.st", source));

        assert!(
            analysis.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            analysis.diagnostics
        );
        assert!(analysis
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Demo" && symbol.kind == SymbolKind::Program));
        assert!(analysis
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Count" && symbol.kind == SymbolKind::Variable));
        assert!(analysis
            .completions_with_prefix("COU")
            .iter()
            .any(|completion| completion.label == "Count"));
        assert!(analysis
            .completions_with_prefix("TO")
            .iter()
            .any(|completion| completion.label == "TON"));
    }

    #[test]
    fn hover_reports_symbol_detail() {
        let source = "PROGRAM Demo VAR Flag : BOOL; END_VAR END_PROGRAM";
        let analysis = analyze_document(
            DocumentInput::new("hover.st", source),
            &LanguageServiceOptions::default(),
        );
        let flag = analysis
            .symbols
            .iter()
            .find(|symbol| symbol.name == "Flag")
            .and_then(|symbol| symbol.range.as_ref())
            .expect("Flag range");

        let hover = analysis.hover_at(flag.start).expect("hover");
        assert!(hover.contents.contains("Variable `Flag`"));
        assert!(hover.contents.contains("BOOL"));
    }

    #[test]
    fn xml_documents_use_plcopen_import_path() {
        let xml = r#"
<project xmlns="http://www.plcopen.org/xml/tc6_0201">
  <types>
    <pous>
      <pou name="Demo" pouType="program">
        <interface />
        <body><ST><xhtml xmlns="http://www.w3.org/1999/xhtml"></xhtml></ST></body>
      </pou>
    </pous>
  </types>
</project>
"#;
        let analysis = analyze_document(
            DocumentInput::new("project.xml", xml).with_language_id("xml"),
            &LanguageServiceOptions::default(),
        );

        assert!(analysis
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Demo" && symbol.kind == SymbolKind::Program));
    }

    #[test]
    fn exposes_sfc_steps_and_actions() {
        let source = r#"
PROGRAM Sequence
INITIAL_STEP Start;
ACTION RunAction(N):
END_ACTION
END_PROGRAM
"#;
        let analysis = analyze_document(
            DocumentInput::new("sequence.sfc", source),
            &LanguageServiceOptions::default(),
        );

        assert!(
            analysis.diagnostics.is_empty(),
            "unexpected diagnostics: {:?}",
            analysis.diagnostics
        );
        assert!(analysis
            .symbols
            .iter()
            .any(|symbol| symbol.name == "Start" && symbol.kind == SymbolKind::SfcStep));
        assert!(analysis
            .symbols
            .iter()
            .any(|symbol| symbol.name == "RunAction" && symbol.kind == SymbolKind::SfcAction));
    }

    #[test]
    fn simulates_document_and_generates_c() {
        let source = r#"
PROGRAM Counter
VAR
    Count : INT := 0;
END_VAR
Count := Count + 1;
END_PROGRAM
"#;
        let simulation = simulate_document(
            DocumentInput::new("counter.st", source),
            &LanguageServiceOptions::default(),
            3,
        );

        assert!(simulation.diagnostics.is_empty());
        assert_eq!(simulation.program, "Counter");
        assert_eq!(simulation.cycles.len(), 3);
        assert!(simulation.generated_c.contains("counter_scan"));
    }

    #[test]
    fn analyzes_workspace_documents_independently() {
        let workspace = analyze_workspace(
            vec![
                DocumentInput::new("a.st", "PROGRAM A END_PROGRAM"),
                DocumentInput::new("b.st", "PROGRAM B END_PROGRAM"),
            ],
            &LanguageServiceOptions::default(),
        );

        assert_eq!(workspace_document_count(&workspace), 2);
        assert!(workspace
            .documents
            .iter()
            .any(|document| document.uri == "a.st"));
        assert!(workspace
            .documents
            .iter()
            .any(|document| document.uri == "b.st"));
    }

    #[test]
    fn reports_service_capabilities() {
        let capabilities = ServiceCapabilities::for_options(&LanguageServiceOptions::default());
        assert!(capabilities.simulation);
        assert!(capabilities.generated_c);
        assert!(capabilities.to_json().contains("documentSymbols"));
    }

    #[test]
    fn fixture_counter_st_runs_through_language_service() {
        let source = include_str!("../tests/fixtures/counter.st");
        let analysis = analyze_document(
            DocumentInput::new("counter.st", source),
            &LanguageServiceOptions::default(),
        );
        assert!(analysis.diagnostics.is_empty());
        let simulation = simulate_document(
            DocumentInput::new("counter.st", source),
            &LanguageServiceOptions::default(),
            2,
        );
        assert_eq!(simulation.cycles.len(), 2);
        assert!(simulation.generated_c.contains("counter_scan"));
    }
}
