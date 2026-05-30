// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use iec_diagnostics::{json_escape, Diagnostic};
use iec_ir::Project;
use iec_semantics::{check_project, CheckOptions};

use crate::{
    analyze_document, DocumentAnalysis, DocumentInput, LanguageServiceOptions, SourceRange,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceRoot {
    pub uri: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceFile {
    pub input: DocumentInput,
    pub kind: WorkspaceFileKind,
    pub include_group: Option<String>,
    pub library: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceFileKind {
    Source,
    Library,
    PlcOpenImport,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkspaceModel {
    pub roots: Vec<WorkspaceRoot>,
    pub files: Vec<WorkspaceFile>,
}

#[derive(Debug, Clone)]
pub struct WorkspaceAnalysis {
    pub roots: Vec<WorkspaceRoot>,
    pub documents: Vec<DocumentAnalysis>,
    pub merged_project: Project,
    pub diagnostics_by_uri: BTreeMap<String, Vec<Diagnostic>>,
    pub include_groups: BTreeMap<String, Vec<String>>,
    pub library_documents: Vec<String>,
    pub imported_plcopen_documents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkspaceDiagnosticEntry {
    pub uri: String,
    pub diagnostics: Vec<Diagnostic>,
}

impl WorkspaceModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_root(mut self, uri: impl Into<String>) -> Self {
        self.roots.push(WorkspaceRoot { uri: uri.into() });
        self
    }

    pub fn add_source(&mut self, input: DocumentInput) {
        self.files.push(WorkspaceFile {
            input,
            kind: WorkspaceFileKind::Source,
            include_group: None,
            library: false,
        });
    }

    pub fn add_library(&mut self, input: DocumentInput, group: impl Into<String>) {
        self.files.push(WorkspaceFile {
            input,
            kind: WorkspaceFileKind::Library,
            include_group: Some(group.into()),
            library: true,
        });
    }

    pub fn add_plcopen_import(&mut self, input: DocumentInput) {
        self.files.push(WorkspaceFile {
            input,
            kind: WorkspaceFileKind::PlcOpenImport,
            include_group: Some("plcopen".to_string()),
            library: false,
        });
    }

    pub fn from_inputs(inputs: Vec<DocumentInput>) -> Self {
        let mut model = Self::new();
        for input in inputs {
            if input.uri.ends_with(".xml") || input.language_id.as_deref() == Some("xml") {
                model.add_plcopen_import(input);
            } else {
                model.add_source(input);
            }
        }
        model
    }

    pub fn load_from_root(root: impl AsRef<Path>) -> Result<Self, String> {
        let root = root.as_ref();
        let mut model = WorkspaceModel::new().with_root(root.to_string_lossy());
        for path in discover_project_files(root)? {
            let text = fs::read_to_string(&path).map_err(|err| err.to_string())?;
            let uri = path.to_string_lossy().to_string();
            let input = DocumentInput::new(uri, text);
            if path
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("xml"))
            {
                model.add_plcopen_import(input.with_language_id("xml"));
            } else if path.components().any(|component| {
                component
                    .as_os_str()
                    .to_string_lossy()
                    .eq_ignore_ascii_case("lib")
            }) {
                model.add_library(input, "lib");
            } else {
                model.add_source(input);
            }
        }
        Ok(model)
    }
}

impl WorkspaceAnalysis {
    pub fn diagnostics(&self) -> Vec<WorkspaceDiagnosticEntry> {
        self.diagnostics_by_uri
            .iter()
            .map(|(uri, diagnostics)| WorkspaceDiagnosticEntry {
                uri: uri.clone(),
                diagnostics: diagnostics.clone(),
            })
            .collect()
    }

    pub fn document(&self, uri: &str) -> Option<&DocumentAnalysis> {
        self.documents.iter().find(|document| document.uri == uri)
    }

    pub fn to_json(&self) -> String {
        let roots = self
            .roots
            .iter()
            .map(|root| format!("\"{}\"", json_escape(&root.uri)))
            .collect::<Vec<_>>()
            .join(",");
        let documents = self
            .documents
            .iter()
            .map(DocumentAnalysis::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let diagnostics = self
            .diagnostics()
            .iter()
            .map(WorkspaceDiagnosticEntry::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let library_documents = json_string_array(&self.library_documents);
        let imported_plcopen_documents = json_string_array(&self.imported_plcopen_documents);
        let include_groups = self
            .include_groups
            .iter()
            .map(|(group, uris)| format!("\"{}\":{}", json_escape(group), json_string_array(uris)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"roots\":[{}],\"documents\":[{}],\"diagnostics\":[{}],\"includeGroups\":{{{}}},\"libraryDocuments\":{},\"importedPlcopenDocuments\":{}}}",
            roots,
            documents,
            diagnostics,
            include_groups,
            library_documents,
            imported_plcopen_documents
        )
    }
}

impl WorkspaceDiagnosticEntry {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"uri\":\"{}\",\"diagnostics\":{}}}",
            json_escape(&self.uri),
            iec_diagnostics::diagnostics_to_json(&self.diagnostics)
        )
    }
}

pub fn analyze_workspace(
    inputs: Vec<DocumentInput>,
    options: &LanguageServiceOptions,
) -> WorkspaceAnalysis {
    analyze_workspace_model(WorkspaceModel::from_inputs(inputs), options)
}

pub fn analyze_workspace_model(
    model: WorkspaceModel,
    options: &LanguageServiceOptions,
) -> WorkspaceAnalysis {
    let mut documents = Vec::new();
    let mut diagnostics_by_uri = BTreeMap::new();
    let mut merged_project = Project::new(options.profile);
    let mut include_groups = BTreeMap::<String, Vec<String>>::new();
    let mut library_documents = Vec::new();
    let mut imported_plcopen_documents = Vec::new();

    for file in &model.files {
        let mut input = file.input.clone();
        if matches!(file.kind, WorkspaceFileKind::PlcOpenImport) {
            input.language_id = Some("xml".to_string());
            imported_plcopen_documents.push(input.uri.clone());
        }
        if file.library {
            library_documents.push(input.uri.clone());
        }
        if let Some(group) = &file.include_group {
            include_groups
                .entry(group.clone())
                .or_default()
                .push(input.uri.clone());
        }

        let analysis = analyze_document(input, options);
        merged_project
            .library_elements
            .extend(analysis.project.library_elements.clone());
        merged_project
            .metadata
            .extend(analysis.project.metadata.clone());
        diagnostics_by_uri.insert(analysis.uri.clone(), analysis.diagnostics.clone());
        documents.push(analysis);
    }

    let workspace_diagnostics = check_project(
        &merged_project,
        &CheckOptions {
            profile: options.profile,
            implementation: options.implementation.clone(),
        },
    );
    distribute_workspace_diagnostics(&workspace_diagnostics, &documents, &mut diagnostics_by_uri);

    WorkspaceAnalysis {
        roots: model.roots,
        documents,
        merged_project,
        diagnostics_by_uri,
        include_groups,
        library_documents,
        imported_plcopen_documents,
    }
}

pub fn workspace_document_count(analysis: &WorkspaceAnalysis) -> usize {
    analysis.documents.len()
}

pub fn workspace_diagnostics_json(analysis: &WorkspaceAnalysis) -> String {
    let entries = analysis
        .diagnostics()
        .iter()
        .map(WorkspaceDiagnosticEntry::to_json)
        .collect::<Vec<_>>()
        .join(",");
    format!("[{entries}]")
}

pub fn merge_projects(documents: &[DocumentAnalysis], options: &LanguageServiceOptions) -> Project {
    let mut project = Project::new(options.profile);
    for document in documents {
        project
            .library_elements
            .extend(document.project.library_elements.clone());
        project.metadata.extend(document.project.metadata.clone());
    }
    project
}

fn distribute_workspace_diagnostics(
    diagnostics: &[Diagnostic],
    documents: &[DocumentAnalysis],
    diagnostics_by_uri: &mut BTreeMap<String, Vec<Diagnostic>>,
) {
    let mut seen = BTreeSet::<(String, String)>::new();
    for diagnostic in diagnostics {
        let uri = diagnostic
            .span
            .as_ref()
            .map(|span| span.source.clone())
            .or_else(|| infer_diagnostic_uri(diagnostic, documents))
            .unwrap_or_else(|| "workspace".to_string());
        let key = (uri.clone(), diagnostic.message.clone());
        if seen.insert(key) {
            diagnostics_by_uri
                .entry(uri)
                .or_default()
                .push(diagnostic.clone());
        }
    }
}

fn infer_diagnostic_uri(diagnostic: &Diagnostic, documents: &[DocumentAnalysis]) -> Option<String> {
    documents
        .iter()
        .find(|document| {
            document.symbols.iter().any(|symbol| {
                diagnostic.message.contains(&format!("'{}'", symbol.name))
                    || diagnostic.message.contains(&symbol.name)
            })
        })
        .map(|document| document.uri.clone())
}

fn discover_project_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    discover_project_files_inner(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn discover_project_files_inner(root: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    if root
        .file_name()
        .is_some_and(|name| name == "target" || name == "node_modules" || name == ".git")
    {
        return Ok(());
    }
    for entry in fs::read_dir(root).map_err(|err| err.to_string())? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        if path.is_dir() {
            discover_project_files_inner(&path, files)?;
        } else if is_workspace_source_file(&path) {
            files.push(path);
        }
    }
    Ok(())
}

fn is_workspace_source_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| {
        matches!(
            extension.to_string_lossy().to_ascii_lowercase().as_str(),
            "st" | "il" | "ld" | "fbd" | "sfc" | "xml"
        )
    })
}

fn json_string_array(values: &[String]) -> String {
    let values = values
        .iter()
        .map(|value| format!("\"{}\"", json_escape(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{values}]")
}

#[allow(dead_code)]
fn _range_for_docs(_range: &SourceRange) {}
