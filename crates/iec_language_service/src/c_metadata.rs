// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_c::generate_c;
use iec_diagnostics::{json_escape, Diagnostic};
use iec_ir::{DataTypeSpec, LibraryElement, PouKind, Project, RetainKind, VarBlockKind};

use crate::{analyze_document, has_error_diagnostics, DocumentInput, LanguageServiceOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedCArtifact {
    pub source: String,
    pub metadata: GeneratedCMetadata,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GeneratedCMetadata {
    pub filename_hint: String,
    pub scan_entrypoints: Vec<CEntrypoint>,
    pub state_layout: Vec<CStateField>,
    pub io_symbols: Vec<CIoSymbol>,
    pub access_paths: Vec<CAccessPath>,
    pub retained_fields: Vec<String>,
    pub target_hooks: Vec<String>,
    pub debug_symbols: Vec<CDebugSymbol>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CEntrypoint {
    pub name: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CStateField {
    pub name: String,
    pub type_name: String,
    pub retained: bool,
    pub source_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CIoSymbol {
    pub name: String,
    pub location: String,
    pub direction: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CAccessPath {
    pub name: String,
    pub target: String,
    pub direction: String,
    pub type_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CDebugSymbol {
    pub name: String,
    pub kind: String,
    pub type_name: String,
}

impl GeneratedCArtifact {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"source\":\"{}\",\"metadata\":{},\"diagnostics\":{}}}",
            json_escape(&self.source),
            self.metadata.to_json(),
            iec_diagnostics::diagnostics_to_json(&self.diagnostics)
        )
    }
}

impl GeneratedCMetadata {
    pub fn to_json(&self) -> String {
        let entrypoints = self
            .scan_entrypoints
            .iter()
            .map(CEntrypoint::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let state_layout = self
            .state_layout
            .iter()
            .map(CStateField::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let io_symbols = self
            .io_symbols
            .iter()
            .map(CIoSymbol::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let access_paths = self
            .access_paths
            .iter()
            .map(CAccessPath::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let retained_fields = json_string_array(&self.retained_fields);
        let target_hooks = json_string_array(&self.target_hooks);
        let debug_symbols = self
            .debug_symbols
            .iter()
            .map(CDebugSymbol::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"filenameHint\":\"{}\",\"scanEntrypoints\":[{}],\"stateLayout\":[{}],\"ioSymbols\":[{}],\"accessPaths\":[{}],\"retainedFields\":{},\"targetHooks\":{},\"debugSymbols\":[{}]}}",
            json_escape(&self.filename_hint),
            entrypoints,
            state_layout,
            io_symbols,
            access_paths,
            retained_fields,
            target_hooks,
            debug_symbols
        )
    }
}

impl CEntrypoint {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"name\":\"{}\",\"signature\":\"{}\"}}",
            json_escape(&self.name),
            json_escape(&self.signature)
        )
    }
}

impl CStateField {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"name\":\"{}\",\"typeName\":\"{}\",\"retained\":{},\"sourceName\":\"{}\"}}",
            json_escape(&self.name),
            json_escape(&self.type_name),
            self.retained,
            json_escape(&self.source_name)
        )
    }
}

impl CIoSymbol {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"name\":\"{}\",\"location\":\"{}\",\"direction\":\"{}\",\"typeName\":\"{}\"}}",
            json_escape(&self.name),
            json_escape(&self.location),
            json_escape(&self.direction),
            json_escape(&self.type_name)
        )
    }
}

impl CAccessPath {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"name\":\"{}\",\"target\":\"{}\",\"direction\":\"{}\",\"typeName\":\"{}\"}}",
            json_escape(&self.name),
            json_escape(&self.target),
            json_escape(&self.direction),
            json_escape(&self.type_name)
        )
    }
}

impl CDebugSymbol {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"name\":\"{}\",\"kind\":\"{}\",\"typeName\":\"{}\"}}",
            json_escape(&self.name),
            json_escape(&self.kind),
            json_escape(&self.type_name)
        )
    }
}

pub fn generate_c_artifact(
    input: DocumentInput,
    options: &LanguageServiceOptions,
) -> GeneratedCArtifact {
    let analysis = analyze_document(input, options);
    if has_error_diagnostics(&analysis.diagnostics) {
        return GeneratedCArtifact {
            source: String::new(),
            metadata: generated_c_metadata(&analysis.project, ""),
            diagnostics: analysis.diagnostics,
        };
    }
    match generate_c(&analysis.project, None) {
        Ok(output) => GeneratedCArtifact {
            metadata: generated_c_metadata(&analysis.project, &output.filename_hint),
            source: output.source,
            diagnostics: analysis.diagnostics,
        },
        Err(mut diagnostics) => {
            diagnostics.extend(analysis.diagnostics);
            GeneratedCArtifact {
                source: String::new(),
                metadata: generated_c_metadata(&analysis.project, ""),
                diagnostics,
            }
        }
    }
}

pub fn generated_c_metadata(project: &Project, filename_hint: &str) -> GeneratedCMetadata {
    let mut scan_entrypoints = Vec::new();
    let mut state_layout = Vec::new();
    let mut io_symbols = Vec::new();
    let mut access_paths = Vec::new();
    let mut retained_fields = Vec::new();
    let mut debug_symbols = Vec::new();

    if let Some(program) = project.first_program() {
        let program_ident = sanitize_c_ident(&program.name.original);
        let state_type = format!("{program_ident}_state");
        scan_entrypoints.push(CEntrypoint {
            name: format!("{program_ident}_init"),
            signature: format!("void {program_ident}_init({state_type} *s)"),
        });
        scan_entrypoints.push(CEntrypoint {
            name: format!("{program_ident}_scan"),
            signature: format!("void {program_ident}_scan({state_type} *s)"),
        });
        scan_entrypoints.push(CEntrypoint {
            name: format!("{program_ident}_warm_restart"),
            signature: format!("void {program_ident}_warm_restart({state_type} *s)"),
        });

        for block in &program.var_blocks {
            for var in &block.vars {
                if block.kind == VarBlockKind::Access {
                    if let Some(access) = &var.access {
                        access_paths.push(CAccessPath {
                            name: var.name.original.clone(),
                            target: access.path.clone(),
                            direction: match access.direction {
                                iec_ir::AccessDirection::ReadOnly => "READ_ONLY",
                                iec_ir::AccessDirection::ReadWrite => "READ_WRITE",
                            }
                            .to_string(),
                            type_name: type_detail(&var.type_spec),
                        });
                    }
                    continue;
                }
                let retained = block.retain == Some(RetainKind::Retain);
                let field = CStateField {
                    name: sanitize_c_ident(&var.name.original),
                    type_name: type_detail(&var.type_spec),
                    retained,
                    source_name: var.name.original.clone(),
                };
                if retained {
                    retained_fields.push(var.name.original.clone());
                }
                if let Some(location) = &var.location {
                    io_symbols.push(CIoSymbol {
                        name: var.name.original.clone(),
                        location: location.clone(),
                        direction: direct_location_direction(location).to_string(),
                        type_name: type_detail(&var.type_spec),
                    });
                }
                debug_symbols.push(CDebugSymbol {
                    name: var.name.original.clone(),
                    kind: "stateField".to_string(),
                    type_name: type_detail(&var.type_spec),
                });
                state_layout.push(field);
            }
        }
        if let Some(sfc) = &program.body.sfc {
            for step in &sfc.steps {
                let name = format!("$SFC_STEP_{}", step.name.canonical);
                debug_symbols.push(CDebugSymbol {
                    name: name.clone(),
                    kind: "sfcStep".to_string(),
                    type_name: "BOOL".to_string(),
                });
                state_layout.push(CStateField {
                    name: sanitize_c_ident(&name),
                    type_name: "BOOL".to_string(),
                    retained: false,
                    source_name: step.name.original.clone(),
                });
            }
        }
    }

    for element in &project.library_elements {
        if let LibraryElement::DataType(data_type) = element {
            debug_symbols.push(CDebugSymbol {
                name: data_type.name.original.clone(),
                kind: "dataType".to_string(),
                type_name: type_detail(&data_type.spec),
            });
        }
    }

    GeneratedCMetadata {
        filename_hint: filename_hint.to_string(),
        scan_entrypoints,
        state_layout,
        io_symbols,
        access_paths,
        retained_fields,
        target_hooks: vec![
            "io_read".to_string(),
            "io_write".to_string(),
            "retain_load".to_string(),
            "retain_save".to_string(),
            "time_ms".to_string(),
            "begin_scan".to_string(),
            "end_scan".to_string(),
            "watchdog_pet".to_string(),
        ],
        debug_symbols,
    }
}

fn direct_location_direction(location: &str) -> &'static str {
    if location.to_ascii_uppercase().starts_with("%I") {
        "input"
    } else if location.to_ascii_uppercase().starts_with("%Q") {
        "output"
    } else {
        "memory"
    }
}

fn sanitize_c_ident(input: &str) -> String {
    let mut output = String::new();
    for (index, ch) in input.chars().enumerate() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            if index == 0 && ch.is_ascii_digit() {
                output.push('_');
            }
            output.push(ch.to_ascii_lowercase());
        } else {
            output.push('_');
        }
    }
    if output.is_empty() {
        "_".to_string()
    } else {
        output
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

fn json_string_array(values: &[String]) -> String {
    let values = values
        .iter()
        .map(|value| format!("\"{}\"", json_escape(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{values}]")
}

#[allow(dead_code)]
fn _pou_kind_name(kind: &PouKind) -> &'static str {
    match kind {
        PouKind::Function { .. } => "function",
        PouKind::FunctionBlock => "functionBlock",
        PouKind::Program => "program",
    }
}
