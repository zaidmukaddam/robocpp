// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::hash_map::DefaultHasher;
use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};

use iec_diagnostics::json_escape;

use crate::symbols::document_symbol_index;
use crate::{
    analyze_document, analyze_workspace, merge_projects, DocumentAnalysis, DocumentInput,
    LanguageServiceOptions, SourceRange, SymbolKind, WorkspaceAnalysis,
};

#[derive(Debug, Clone)]
pub struct IncrementalCache {
    options: LanguageServiceOptions,
    documents: BTreeMap<String, CachedDocument>,
    dependency_edges: Vec<DependencyEdge>,
    workspace_analysis: Option<WorkspaceAnalysis>,
}

#[derive(Debug, Clone)]
pub struct CachedDocument {
    pub input: DocumentInput,
    pub content_hash: u64,
    pub analysis: DocumentAnalysis,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DependencyEdge {
    pub from_uri: String,
    pub to_uri: Option<String>,
    pub to_symbol: String,
    pub kind: String,
}

#[derive(Debug, Clone)]
pub struct IncrementalUpdate {
    pub changed_uris: Vec<String>,
    pub reused_uris: Vec<String>,
    pub affected_uris: Vec<String>,
    pub semantic_scope_recheck: bool,
    pub analysis: WorkspaceAnalysis,
    pub dependency_edges: Vec<DependencyEdge>,
}

impl IncrementalCache {
    pub fn new(options: LanguageServiceOptions) -> Self {
        Self {
            options,
            documents: BTreeMap::new(),
            dependency_edges: Vec::new(),
            workspace_analysis: None,
        }
    }

    pub fn upsert_document(&mut self, input: DocumentInput) -> IncrementalUpdate {
        self.update_documents(vec![input], Vec::new())
    }

    pub fn update_documents(
        &mut self,
        inputs: Vec<DocumentInput>,
        removed_uris: Vec<String>,
    ) -> IncrementalUpdate {
        let had_removed_documents = !removed_uris.is_empty();
        let previous_workspace = self.workspace_analysis.clone();
        let previous_dependency_edges = self.dependency_edges.clone();
        for uri in removed_uris {
            self.documents.remove(&uri);
        }

        let mut changed_uris = Vec::new();
        for input in inputs {
            let hash = content_hash(&input.text);
            let changed = self
                .documents
                .get(&input.uri)
                .map(|cached| cached.content_hash != hash)
                .unwrap_or(true);
            if changed {
                let analysis = analyze_document(input.clone(), &self.options);
                self.documents.insert(
                    input.uri.clone(),
                    CachedDocument {
                        input: input.clone(),
                        content_hash: hash,
                        analysis,
                    },
                );
                changed_uris.push(input.uri);
            }
        }

        let reused_uris = self
            .documents
            .keys()
            .filter(|uri| !changed_uris.contains(uri))
            .cloned()
            .collect::<Vec<_>>();
        self.dependency_edges = dependency_edges(
            &self
                .documents
                .values()
                .map(|cached| cached.analysis.clone())
                .collect::<Vec<_>>(),
        );
        let affected_uris = affected_uris(
            &changed_uris,
            &previous_dependency_edges,
            &self.dependency_edges,
        );
        let semantic_scope_recheck = can_recheck_affected_scopes(
            had_removed_documents,
            &changed_uris,
            &affected_uris,
            previous_workspace.as_ref(),
        );
        let analysis = if semantic_scope_recheck {
            workspace_from_cached_documents(
                &self.documents,
                previous_workspace.as_ref().expect("checked above"),
                &changed_uris,
                &self.options,
            )
        } else {
            analyze_workspace(
                self.documents
                    .values()
                    .map(|cached| cached.input.clone())
                    .collect(),
                &self.options,
            )
        };
        self.workspace_analysis = Some(analysis.clone());
        IncrementalUpdate {
            changed_uris,
            reused_uris,
            affected_uris,
            semantic_scope_recheck,
            analysis,
            dependency_edges: self.dependency_edges.clone(),
        }
    }

    pub fn analysis(&self, uri: &str) -> Option<&DocumentAnalysis> {
        self.documents.get(uri).map(|cached| &cached.analysis)
    }

    pub fn dependency_edges(&self) -> &[DependencyEdge] {
        &self.dependency_edges
    }
}

impl DependencyEdge {
    pub fn to_json(&self) -> String {
        let to_uri = self
            .to_uri
            .as_ref()
            .map(|uri| format!("\"{}\"", json_escape(uri)))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"fromUri\":\"{}\",\"toUri\":{},\"toSymbol\":\"{}\",\"kind\":\"{}\"}}",
            json_escape(&self.from_uri),
            to_uri,
            json_escape(&self.to_symbol),
            json_escape(&self.kind)
        )
    }
}

impl IncrementalUpdate {
    pub fn to_json(&self) -> String {
        let changed = json_string_array(&self.changed_uris);
        let reused = json_string_array(&self.reused_uris);
        let affected = json_string_array(&self.affected_uris);
        let edges = self
            .dependency_edges
            .iter()
            .map(DependencyEdge::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"changedUris\":{},\"reusedUris\":{},\"affectedUris\":{},\"semanticScopeRecheck\":{},\"analysis\":{},\"dependencyEdges\":[{}]}}",
            changed,
            reused,
            affected,
            self.semantic_scope_recheck,
            self.analysis.to_json(),
            edges
        )
    }
}

fn affected_uris(
    changed_uris: &[String],
    previous_edges: &[DependencyEdge],
    current_edges: &[DependencyEdge],
) -> Vec<String> {
    let changed = changed_uris.iter().cloned().collect::<BTreeSet<_>>();
    let mut affected = changed.clone();
    for edge in previous_edges.iter().chain(current_edges) {
        let target_changed = edge
            .to_uri
            .as_ref()
            .is_some_and(|to_uri| changed.contains(to_uri));
        if changed.contains(&edge.from_uri) || target_changed {
            affected.insert(edge.from_uri.clone());
            if let Some(to_uri) = &edge.to_uri {
                affected.insert(to_uri.clone());
            }
        }
    }
    affected.into_iter().collect()
}

fn can_recheck_affected_scopes(
    had_removed_documents: bool,
    changed_uris: &[String],
    affected_uris: &[String],
    previous_workspace: Option<&WorkspaceAnalysis>,
) -> bool {
    !had_removed_documents
        && previous_workspace.is_some()
        && changed_uris.len() == 1
        && affected_uris == changed_uris
}

fn workspace_from_cached_documents(
    documents: &BTreeMap<String, CachedDocument>,
    previous: &WorkspaceAnalysis,
    changed_uris: &[String],
    options: &LanguageServiceOptions,
) -> WorkspaceAnalysis {
    let document_analyses = documents
        .values()
        .map(|cached| cached.analysis.clone())
        .collect::<Vec<_>>();
    let mut diagnostics_by_uri = previous.diagnostics_by_uri.clone();
    diagnostics_by_uri.retain(|uri, _| documents.contains_key(uri));
    for uri in changed_uris {
        if let Some(cached) = documents.get(uri) {
            diagnostics_by_uri.insert(uri.clone(), cached.analysis.diagnostics.clone());
        }
    }
    for cached in documents.values() {
        diagnostics_by_uri
            .entry(cached.input.uri.clone())
            .or_insert_with(|| cached.analysis.diagnostics.clone());
    }

    let imported_plcopen_documents = documents
        .values()
        .filter(|cached| is_plcopen_uri(&cached.input.uri))
        .map(|cached| cached.input.uri.clone())
        .collect::<Vec<_>>();
    let include_groups = if imported_plcopen_documents.is_empty() {
        BTreeMap::new()
    } else {
        BTreeMap::from([("plcopen".to_string(), imported_plcopen_documents.clone())])
    };

    WorkspaceAnalysis {
        roots: previous.roots.clone(),
        merged_project: merge_projects(&document_analyses, options),
        documents: document_analyses,
        diagnostics_by_uri,
        include_groups,
        library_documents: Vec::new(),
        imported_plcopen_documents,
    }
}

fn dependency_edges(documents: &[DocumentAnalysis]) -> Vec<DependencyEdge> {
    let mut targets_by_name = BTreeMap::<String, Vec<DependencyTarget>>::new();
    for document in documents {
        for symbol in &document.symbols {
            targets_by_name
                .entry(canonical_symbol_name(&symbol.name))
                .or_default()
                .push(DependencyTarget {
                    uri: document.uri.clone(),
                    symbol: symbol.name.clone(),
                    kind: symbol.kind.clone(),
                    range: symbol.range.clone(),
                });
        }
    }

    let mut edges = Vec::new();
    for document in documents {
        let index = document_symbol_index(document);
        for reference in index.references {
            let Some(targets) = targets_by_name.get(&reference.canonical_name) else {
                continue;
            };
            for target in targets {
                if target.is_same_declaration(&document.uri, &reference.range) {
                    continue;
                }
                edges.push(DependencyEdge {
                    from_uri: document.uri.clone(),
                    to_uri: Some(target.uri.clone()),
                    to_symbol: target.symbol.clone(),
                    kind: dependency_edge_kind(target).to_string(),
                });
            }
        }

        for access_path in index.access_paths {
            edges.push(DependencyEdge {
                from_uri: document.uri.clone(),
                to_uri: Some(document.uri.clone()),
                to_symbol: access_path.target,
                kind: "accessPath".to_string(),
            });
        }

        if is_plcopen_uri(&document.uri) {
            for symbol in &document.symbols {
                edges.push(DependencyEdge {
                    from_uri: document.uri.clone(),
                    to_uri: Some(document.uri.clone()),
                    to_symbol: symbol.name.clone(),
                    kind: "plcopenImport".to_string(),
                });
            }
        }
    }
    edges.sort_by(|left, right| {
        left.from_uri
            .cmp(&right.from_uri)
            .then_with(|| left.to_uri.cmp(&right.to_uri))
            .then_with(|| left.to_symbol.cmp(&right.to_symbol))
            .then_with(|| left.kind.cmp(&right.kind))
    });
    edges.dedup();
    edges
}

#[derive(Clone)]
struct DependencyTarget {
    uri: String,
    symbol: String,
    kind: SymbolKind,
    range: Option<SourceRange>,
}

impl DependencyTarget {
    fn is_same_declaration(&self, uri: &str, range: &SourceRange) -> bool {
        self.uri == uri
            && self
                .range
                .as_ref()
                .is_some_and(|target_range| target_range.start == range.start)
    }
}

fn dependency_edge_kind(target: &DependencyTarget) -> &'static str {
    if is_plcopen_uri(&target.uri) {
        return "plcopenImport";
    }
    match target.kind {
        SymbolKind::DataType => "type",
        SymbolKind::Function | SymbolKind::FunctionBlock | SymbolKind::Program => "pou",
        SymbolKind::Configuration
        | SymbolKind::Resource
        | SymbolKind::Task
        | SymbolKind::ProgramInstance => "configuration",
        SymbolKind::AccessPath => "accessPath",
        SymbolKind::Variable => "variable",
        _ => "symbol",
    }
}

fn canonical_symbol_name(name: &str) -> String {
    name.to_ascii_uppercase()
}

fn is_plcopen_uri(uri: &str) -> bool {
    uri.rsplit('.')
        .next()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
}

fn content_hash(text: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    text.hash(&mut hasher);
    hasher.finish()
}

fn json_string_array(values: &[String]) -> String {
    let values = values
        .iter()
        .map(|value| format!("\"{}\"", json_escape(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{values}]")
}
