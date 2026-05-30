// SPDX-License-Identifier: MIT OR Apache-2.0

use iec_c::generate_c;
use iec_diagnostics::{json_escape, Diagnostic};
use iec_ir::Value;
use iec_runtime::{run_program, RuntimeOptions, RuntimeTrace};

use crate::{analyze_document, has_error_diagnostics, DocumentInput, LanguageServiceOptions};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationVariable {
    pub name: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimulationCycle {
    pub cycle: usize,
    pub variables: Vec<SimulationVariable>,
    pub events: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentSimulation {
    pub uri: String,
    pub program: String,
    pub cycles: Vec<SimulationCycle>,
    pub generated_c: String,
    pub diagnostics: Vec<Diagnostic>,
}

impl DocumentSimulation {
    pub fn to_json(&self) -> String {
        let cycles = self
            .cycles
            .iter()
            .map(SimulationCycle::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let diagnostics = iec_diagnostics::diagnostics_to_json(&self.diagnostics);
        format!(
            "{{\"program\":\"{}\",\"source\":\"{}\",\"cycles\":[{}],\"generatedC\":\"{}\",\"diagnostics\":{}}}",
            json_escape(&self.program),
            json_escape(&self.uri),
            cycles,
            json_escape(&self.generated_c),
            diagnostics
        )
    }
}

impl SimulationCycle {
    pub fn to_json(&self) -> String {
        let variables = self
            .variables
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
        let events = self
            .events
            .iter()
            .map(|event| format!("\"{}\"", json_escape(event)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"cycle\":{},\"variables\":[{}],\"events\":[{}]}}",
            self.cycle, variables, events
        )
    }
}

pub fn simulate_document(
    input: DocumentInput,
    options: &LanguageServiceOptions,
    cycles: usize,
) -> DocumentSimulation {
    let analysis = analyze_document(input.clone(), options);
    let mut diagnostics = analysis.diagnostics;

    if has_error_diagnostics(&diagnostics) {
        return DocumentSimulation {
            uri: analysis.uri,
            program: String::new(),
            cycles: Vec::new(),
            generated_c: String::new(),
            diagnostics,
        };
    }

    let generated_c = generate_document_c(&analysis.project).unwrap_or_default();

    match run_program(&analysis.project, None, cycles, &RuntimeOptions::default()) {
        Ok(trace) => DocumentSimulation {
            uri: analysis.uri,
            program: trace.program.clone(),
            cycles: trace_to_cycles(&trace),
            generated_c,
            diagnostics,
        },
        Err(runtime_diagnostics) => {
            diagnostics.extend(runtime_diagnostics);
            DocumentSimulation {
                uri: analysis.uri,
                program: String::new(),
                cycles: Vec::new(),
                generated_c,
                diagnostics,
            }
        }
    }
}

pub fn generate_document_c_from_input(
    input: DocumentInput,
    options: &LanguageServiceOptions,
) -> Result<String, Vec<Diagnostic>> {
    let analysis = analyze_document(input, options);
    if has_error_diagnostics(&analysis.diagnostics) {
        return Err(analysis.diagnostics);
    }
    generate_document_c(&analysis.project)
}

fn generate_document_c(project: &iec_ir::Project) -> Result<String, Vec<Diagnostic>> {
    generate_c(project, None).map(|output| output.source)
}

fn trace_to_cycles(trace: &RuntimeTrace) -> Vec<SimulationCycle> {
    trace
        .cycles
        .iter()
        .map(|cycle| SimulationCycle {
            cycle: cycle.cycle,
            variables: cycle
                .variables
                .iter()
                .map(|(name, value)| SimulationVariable {
                    name: name.clone(),
                    value: value_to_json(value),
                })
                .collect(),
            events: vec!["scan complete".to_string()],
        })
        .collect()
}

fn value_to_json(value: &Value) -> String {
    match value {
        Value::Bool(value) => value.to_string(),
        Value::Int(value) => value.to_string(),
        Value::Real(value) => value.to_string(),
        Value::String(value) | Value::WString(value) => format!("\"{}\"", json_escape(value)),
        Value::TimeMs(value) => value.to_string(),
        Value::Array(_) | Value::Struct(_) | Value::Unit => "\"\"".to_string(),
    }
}
