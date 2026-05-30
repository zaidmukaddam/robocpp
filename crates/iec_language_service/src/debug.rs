// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_diagnostics::{json_escape, Diagnostic};
use iec_ir::{canonical_identifier, AccessDirection, Value};
use iec_runtime::{run_program_with_access_writes, AccessPathWrite, RuntimeOptions};

use crate::{
    analyze_document, has_error_diagnostics, DocumentInput, LanguageServiceOptions,
    SimulationVariable,
};

#[derive(Debug, Clone)]
pub struct DebugOptions {
    pub cycles: usize,
    pub watches: Vec<String>,
    pub access_writes: Vec<DebugAccessWrite>,
    pub runtime_options: RuntimeOptions,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugAccessWrite {
    pub cycle: usize,
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugTrace {
    pub uri: String,
    pub program: String,
    pub cycles: Vec<DebugCycle>,
    pub diagnostics: Vec<Diagnostic>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugCycle {
    pub cycle: usize,
    pub recorded_at: String,
    pub watches: Vec<SimulationVariable>,
    pub variables: Vec<SimulationVariable>,
    pub access_paths: Vec<DebugAccessPath>,
    pub active_sfc_steps: Vec<String>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DebugAccessPath {
    pub name: String,
    pub target: String,
    pub direction: String,
    pub value: Option<String>,
}

impl Default for DebugOptions {
    fn default() -> Self {
        Self {
            cycles: 1,
            watches: Vec::new(),
            access_writes: Vec::new(),
            runtime_options: RuntimeOptions::default(),
        }
    }
}

impl DebugTrace {
    pub fn to_json(&self) -> String {
        let cycles = self
            .cycles
            .iter()
            .map(DebugCycle::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"uri\":\"{}\",\"program\":\"{}\",\"cycles\":[{}],\"diagnostics\":{}}}",
            json_escape(&self.uri),
            json_escape(&self.program),
            cycles,
            iec_diagnostics::diagnostics_to_json(&self.diagnostics)
        )
    }
}

impl DebugCycle {
    pub fn to_json(&self) -> String {
        let watches = variables_to_json(&self.watches);
        let variables = variables_to_json(&self.variables);
        let access_paths = self
            .access_paths
            .iter()
            .map(DebugAccessPath::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let active_sfc_steps = self
            .active_sfc_steps
            .iter()
            .map(|step| format!("\"{}\"", json_escape(step)))
            .collect::<Vec<_>>()
            .join(",");
        let events = self
            .events
            .iter()
            .map(|event| format!("\"{}\"", json_escape(event)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"cycle\":{},\"recordedAt\":\"{}\",\"watches\":{},\"variables\":{},\"accessPaths\":[{}],\"activeSfcSteps\":[{}],\"events\":[{}]}}",
            self.cycle,
            json_escape(&self.recorded_at),
            watches,
            variables,
            access_paths,
            active_sfc_steps,
            events
        )
    }
}

impl DebugAccessPath {
    pub fn to_json(&self) -> String {
        let value = self
            .value
            .as_ref()
            .cloned()
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"name\":\"{}\",\"target\":\"{}\",\"direction\":\"{}\",\"value\":{}}}",
            json_escape(&self.name),
            json_escape(&self.target),
            json_escape(&self.direction),
            value
        )
    }
}

pub fn debug_document(
    input: DocumentInput,
    options: &LanguageServiceOptions,
    debug: DebugOptions,
) -> DebugTrace {
    let analysis = analyze_document(input, options);
    let mut diagnostics = analysis.diagnostics;
    if has_error_diagnostics(&diagnostics) {
        return DebugTrace {
            uri: analysis.uri,
            program: String::new(),
            cycles: Vec::new(),
            diagnostics,
        };
    }

    let access_writes = debug
        .access_writes
        .iter()
        .map(|write| AccessPathWrite {
            cycle: write.cycle,
            name: write.name.clone(),
            value: parse_debug_value(&write.value),
        })
        .collect::<Vec<_>>();

    match run_program_with_access_writes(
        &analysis.project,
        None,
        debug.cycles,
        &debug.runtime_options,
        &access_writes,
    ) {
        Ok(trace) => {
            let cycles =
                trace
                    .cycles
                    .iter()
                    .map(|cycle| {
                        let variables = cycle
                            .variables
                            .iter()
                            .map(|(name, value)| SimulationVariable {
                                name: name.clone(),
                                value: value_to_json(value),
                            })
                            .collect::<Vec<_>>();
                        let watches = if debug.watches.is_empty() {
                            Vec::new()
                        } else {
                            let watch_names = debug
                                .watches
                                .iter()
                                .map(|watch| canonical_identifier(watch))
                                .collect::<std::collections::BTreeSet<_>>();
                            variables
                                .iter()
                                .filter(|variable| {
                                    watch_names.contains(&canonical_identifier(&variable.name))
                                })
                                .cloned()
                                .collect()
                        };
                        let active_sfc_steps = cycle
                            .variables
                            .iter()
                            .filter_map(|(name, value)| {
                                let step = name.strip_prefix("$SFC_STEP_")?;
                                (value.as_bool() == Some(true)).then(|| step.to_string())
                            })
                            .collect::<Vec<_>>();
                        let access_paths = cycle
                            .access_paths
                            .iter()
                            .map(|access| DebugAccessPath {
                                name: access.name.clone(),
                                target: access.target.clone(),
                                direction: match access.direction {
                                    AccessDirection::ReadOnly => "READ_ONLY",
                                    AccessDirection::ReadWrite => "READ_WRITE",
                                }
                                .to_string(),
                                value: access.value.as_ref().map(value_to_json),
                            })
                            .collect::<Vec<_>>();
                        let mut events = vec!["scan complete".to_string()];
                        events.extend(access_paths.iter().map(|access| {
                            format!("access path {} -> {}", access.name, access.target)
                        }));
                        DebugCycle {
                            cycle: cycle.cycle,
                            recorded_at: "1970-01-01T00:00:00.000Z".to_string(),
                            watches,
                            variables,
                            access_paths,
                            active_sfc_steps,
                            events,
                        }
                    })
                    .collect();
            DebugTrace {
                uri: analysis.uri,
                program: trace.program,
                cycles,
                diagnostics,
            }
        }
        Err(runtime_diagnostics) => {
            diagnostics.extend(runtime_diagnostics);
            DebugTrace {
                uri: analysis.uri,
                program: String::new(),
                cycles: Vec::new(),
                diagnostics,
            }
        }
    }
}

fn parse_debug_value(input: &str) -> Value {
    if input.eq_ignore_ascii_case("TRUE") || input == "1" {
        Value::Bool(true)
    } else if input.eq_ignore_ascii_case("FALSE") || input == "0" {
        Value::Bool(false)
    } else if let Ok(value) = input.parse::<i64>() {
        Value::Int(value)
    } else if let Ok(value) = input.parse::<f64>() {
        Value::Real(value)
    } else {
        Value::String(input.trim_matches('\'').trim_matches('"').to_string())
    }
}

fn value_to_json(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Real(value) => value.to_string(),
        Value::String(value) | Value::WString(value) => format!("\"{}\"", json_escape(value)),
        Value::TimeMs(value) => value.to_string(),
        Value::Array(values) => {
            let values = values
                .iter()
                .map(value_to_json)
                .collect::<Vec<_>>()
                .join(",");
            format!("[{values}]")
        }
        Value::Struct(fields) => {
            let fields = fields
                .iter()
                .map(|(name, value)| format!("\"{}\":{}", json_escape(name), value_to_json(value)))
                .collect::<Vec<_>>()
                .join(",");
            format!("{{{fields}}}")
        }
        Value::Unit => "null".to_string(),
    }
}

fn variables_to_json(variables: &[SimulationVariable]) -> String {
    let variables = variables
        .iter()
        .map(|variable| {
            format!(
                "{{\"name\":\"{}\",\"value\":{}}}",
                json_escape(&variable.name),
                variable.value
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{variables}]")
}
