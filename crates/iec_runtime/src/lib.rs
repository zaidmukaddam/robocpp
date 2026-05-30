// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(clippy::too_many_arguments)]

mod configuration;
mod runtime;
mod state;
mod support;

#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;

use configuration::*;
use runtime::*;
use state::*;
use support::*;

#[derive(Debug, Clone)]
pub struct RuntimeTrace {
    pub program: String,
    pub cycles: Vec<CycleTrace>,
}

#[derive(Debug, Clone)]
pub struct ConfigurationTrace {
    pub configuration: String,
    pub cycles: Vec<ConfigurationCycleTrace>,
}

#[derive(Debug, Clone)]
pub struct ConfigurationCycleTrace {
    pub cycle: usize,
    pub programs: Vec<ProgramInstanceTrace>,
    pub access_paths: Vec<AccessPathTrace>,
}

#[derive(Debug, Clone)]
pub struct ProgramInstanceTrace {
    pub resource: String,
    pub instance: String,
    pub program: String,
    pub variables: Vec<(String, Value)>,
    pub access_paths: Vec<AccessPathTrace>,
}

#[derive(Debug, Clone)]
pub struct CycleTrace {
    pub cycle: usize,
    pub variables: Vec<(String, Value)>,
    pub access_paths: Vec<AccessPathTrace>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccessPathTrace {
    pub name: String,
    pub target: String,
    pub direction: AccessDirection,
    pub value: Option<Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AccessPathWrite {
    pub cycle: usize,
    pub name: String,
    pub value: Value,
}

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub max_loop_iterations: usize,
    pub max_scan_cycles: usize,
    pub cycle_time_ms: i128,
    pub warm_restart_before_cycles: Vec<usize>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            max_loop_iterations: 10_000,
            max_scan_cycles: 10_000,
            cycle_time_ms: 1,
            warm_restart_before_cycles: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommunicationInvocation {
    pub block: String,
    pub instance: String,
    pub inputs: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommunicationOutcome {
    pub outputs: BTreeMap<String, Value>,
}

pub trait CommunicationHooks {
    fn execute(&self, invocation: &CommunicationInvocation) -> Option<CommunicationOutcome>;
}

struct NoCommunicationHooks;

impl CommunicationHooks for NoCommunicationHooks {
    fn execute(&self, _invocation: &CommunicationInvocation) -> Option<CommunicationOutcome> {
        None
    }
}

pub fn run_program(
    project: &Project,
    program_name: Option<&str>,
    cycles: usize,
    options: &RuntimeOptions,
) -> Result<RuntimeTrace, Vec<Diagnostic>> {
    run_program_with_access_writes(project, program_name, cycles, options, &[])
}

pub fn run_program_with_access_writes(
    project: &Project,
    program_name: Option<&str>,
    cycles: usize,
    options: &RuntimeOptions,
    access_writes: &[AccessPathWrite],
) -> Result<RuntimeTrace, Vec<Diagnostic>> {
    run_program_with_runtime_services(
        project,
        program_name,
        cycles,
        options,
        &NoCommunicationHooks,
        access_writes,
    )
}

pub fn run_program_with_communication_hooks<'a>(
    project: &'a Project,
    program_name: Option<&str>,
    cycles: usize,
    options: &RuntimeOptions,
    communication: &'a dyn CommunicationHooks,
) -> Result<RuntimeTrace, Vec<Diagnostic>> {
    run_program_with_runtime_services(project, program_name, cycles, options, communication, &[])
}

fn run_program_with_runtime_services<'a>(
    project: &'a Project,
    program_name: Option<&str>,
    cycles: usize,
    options: &RuntimeOptions,
    communication: &'a dyn CommunicationHooks,
    access_writes: &[AccessPathWrite],
) -> Result<RuntimeTrace, Vec<Diagnostic>> {
    if cycles > options.max_scan_cycles {
        return Err(vec![Diagnostic::error(
            DiagnosticCode::Compliance,
            format!(
                "scan cycle count {cycles} exceeds maximum {}",
                options.max_scan_cycles
            ),
            None,
        )]);
    }

    let Some(program) = find_program(project, program_name) else {
        return Err(vec![Diagnostic::error(
            DiagnosticCode::Runtime,
            "no PROGRAM POU found to execute",
            None,
        )]);
    };

    let mut runtime = Runtime {
        project,
        program,
        env: BTreeMap::new(),
        types: BTreeMap::new(),
        il_accumulator: Value::Unit,
        diagnostics: Vec::new(),
        options: options.clone(),
        call_depth: 0,
        communication,
    };
    runtime.initialize(StartupKind::Cold);
    if !runtime.diagnostics.is_empty() {
        return Err(runtime.diagnostics);
    }

    let mut trace = RuntimeTrace {
        program: program.name.original.clone(),
        cycles: Vec::new(),
    };

    for cycle in 0..cycles {
        if options.warm_restart_before_cycles.contains(&cycle) {
            runtime.initialize(StartupKind::Warm);
        }
        runtime.apply_access_writes(cycle, access_writes);
        if !runtime.diagnostics.is_empty() {
            return Err(runtime.diagnostics);
        }
        runtime.reset_temp_variables();
        match runtime.execute_program_cycle() {
            Control::Continue | Control::Return => {}
            Control::Exit => {
                runtime.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    "EXIT used outside of an iteration",
                    None,
                ));
                return Err(runtime.diagnostics);
            }
            Control::Jump(label) => {
                runtime.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!("jump to unknown IL label '{label}'"),
                    None,
                ));
                return Err(runtime.diagnostics);
            }
        }
        trace.cycles.push(CycleTrace {
            cycle,
            variables: runtime.snapshot(),
            access_paths: runtime.access_snapshot(),
        });
    }

    if runtime.diagnostics.is_empty() {
        Ok(trace)
    } else {
        Err(runtime.diagnostics)
    }
}

pub fn run_configuration(
    project: &Project,
    configuration_name: Option<&str>,
    cycles: usize,
    options: &RuntimeOptions,
) -> Result<ConfigurationTrace, Vec<Diagnostic>> {
    run_configuration_with_runtime_services(
        project,
        configuration_name,
        cycles,
        options,
        &NoCommunicationHooks,
        &[],
    )
}

pub fn run_configuration_with_access_writes(
    project: &Project,
    configuration_name: Option<&str>,
    cycles: usize,
    options: &RuntimeOptions,
    access_writes: &[AccessPathWrite],
) -> Result<ConfigurationTrace, Vec<Diagnostic>> {
    run_configuration_with_runtime_services(
        project,
        configuration_name,
        cycles,
        options,
        &NoCommunicationHooks,
        access_writes,
    )
}

pub fn run_configuration_with_communication_hooks<'a>(
    project: &'a Project,
    configuration_name: Option<&str>,
    cycles: usize,
    options: &RuntimeOptions,
    communication: &'a dyn CommunicationHooks,
) -> Result<ConfigurationTrace, Vec<Diagnostic>> {
    run_configuration_with_runtime_services(
        project,
        configuration_name,
        cycles,
        options,
        communication,
        &[],
    )
}

fn run_configuration_with_runtime_services<'a>(
    project: &'a Project,
    configuration_name: Option<&str>,
    cycles: usize,
    options: &RuntimeOptions,
    communication: &'a dyn CommunicationHooks,
    access_writes: &[AccessPathWrite],
) -> Result<ConfigurationTrace, Vec<Diagnostic>> {
    if cycles > options.max_scan_cycles {
        return Err(vec![Diagnostic::error(
            DiagnosticCode::Compliance,
            format!(
                "scan cycle count {cycles} exceeds maximum {}",
                options.max_scan_cycles
            ),
            None,
        )]);
    }

    let Some(configuration) = find_configuration(project, configuration_name) else {
        return Err(vec![Diagnostic::error(
            DiagnosticCode::Runtime,
            "no CONFIGURATION found to execute",
            None,
        )]);
    };

    let mut programs = Vec::new();
    let mut configuration_state = GlobalState::from_blocks(project, &configuration.var_blocks);
    let mut resource_states = configuration
        .resources
        .iter()
        .map(|resource| {
            (
                resource.name.canonical.clone(),
                GlobalState::from_blocks(project, &resource.var_blocks),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let mut direct_state = BTreeMap::<String, Value>::new();
    let mut diagnostics = Vec::new();
    for resource in &configuration.resources {
        for instance in &resource.program_instances {
            let Some(program) = project
                .find_pou(&instance.program_type.original)
                .filter(|pou| matches!(&pou.kind, PouKind::Program))
            else {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!(
                        "program instance '{}' references unknown PROGRAM type '{}'",
                        instance.name.original, instance.program_type.original
                    ),
                    None,
                ));
                continue;
            };
            let task = instance.task.as_ref().and_then(|task| {
                resource
                    .tasks
                    .iter()
                    .find(|candidate| candidate.name.canonical == task.canonical)
            });
            let mut runtime = Runtime {
                project,
                program,
                env: BTreeMap::new(),
                types: BTreeMap::new(),
                il_accumulator: Value::Unit,
                diagnostics: Vec::new(),
                options: options.clone(),
                call_depth: 0,
                communication,
            };
            runtime.initialize(StartupKind::Cold);
            apply_program_instance_args(&mut runtime, instance);
            if !runtime.diagnostics.is_empty() {
                diagnostics.extend(runtime.diagnostics.clone());
                continue;
            }
            programs.push(ScheduledProgram {
                resource: resource.name.original.clone(),
                instance: instance.name.original.clone(),
                task: task.map(|task| task.name.original.clone()),
                priority: task_priority(task),
                interval_ms: task_interval_ms(task, options.cycle_time_ms),
                single: task.and_then(|task| task.single.clone()),
                single_previous: false,
                output_bindings: program_instance_output_bindings(instance),
                runtime,
            });
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    programs.sort_by_key(|program| (program.priority, program.instance.clone()));
    let mut trace = ConfigurationTrace {
        configuration: configuration.name.original.clone(),
        cycles: Vec::new(),
    };

    for cycle in 0..cycles {
        let mut cycle_trace = ConfigurationCycleTrace {
            cycle,
            programs: Vec::new(),
            access_paths: Vec::new(),
        };
        let elapsed_ms = cycle as i128 * options.cycle_time_ms;
        apply_configuration_access_writes(
            project,
            configuration,
            &mut configuration_state,
            &mut resource_states,
            &mut direct_state,
            &mut programs,
            cycle,
            access_writes,
            &mut diagnostics,
        );
        if !diagnostics.is_empty() {
            return Err(diagnostics);
        }
        for index in 0..programs.len() {
            let output_writes = {
                let scheduled = &mut programs[index];
                let interval_due = scheduled
                    .interval_ms
                    .is_some_and(|interval_ms| elapsed_ms % interval_ms == 0);
                let single_due = scheduled_task_single_due(
                    project,
                    scheduled,
                    &configuration_state,
                    &resource_states,
                    options,
                    communication,
                );
                if !scheduled.runtime.diagnostics.is_empty() {
                    return Err(scheduled.runtime.diagnostics.clone());
                }
                if !interval_due && !single_due {
                    None
                } else {
                    scheduled.runtime.sync_direct_state(&direct_state);
                    scheduled.runtime.reset_temp_variables();
                    match scheduled.runtime.execute_program_cycle() {
                        Control::Continue | Control::Return => {}
                        Control::Exit => {
                            scheduled.runtime.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::Runtime,
                                "EXIT used outside of an iteration",
                                None,
                            ));
                        }
                        Control::Jump(label) => {
                            scheduled.runtime.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::Runtime,
                                format!("jump to unknown IL label '{label}'"),
                                None,
                            ));
                        }
                    }
                    if !scheduled.runtime.diagnostics.is_empty() {
                        return Err(scheduled.runtime.diagnostics.clone());
                    }
                    scheduled.runtime.export_direct_state(&mut direct_state);
                    let output_writes = collect_program_instance_output_writes(scheduled);
                    if !scheduled.runtime.diagnostics.is_empty() {
                        return Err(scheduled.runtime.diagnostics.clone());
                    }
                    cycle_trace.programs.push(ProgramInstanceTrace {
                        resource: scheduled.resource.clone(),
                        instance: scheduled.instance.clone(),
                        program: scheduled.runtime.program.name.original.clone(),
                        variables: scheduled.runtime.snapshot(),
                        access_paths: scheduled.runtime.access_snapshot(),
                    });
                    Some(output_writes)
                }
            };
            let Some(output_writes) = output_writes else {
                continue;
            };
            apply_program_instance_output_writes(
                project,
                configuration,
                &mut configuration_state,
                &mut resource_states,
                &mut direct_state,
                &mut programs,
                options,
                communication,
                output_writes,
                &mut diagnostics,
            );
            if !diagnostics.is_empty() {
                return Err(diagnostics);
            }
        }
        cycle_trace.access_paths = configuration_access_snapshot(
            project,
            configuration,
            &configuration_state,
            &resource_states,
            &direct_state,
            &mut programs,
        );
        trace.cycles.push(cycle_trace);
    }

    Ok(trace)
}
