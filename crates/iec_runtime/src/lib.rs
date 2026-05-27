// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(clippy::too_many_arguments)]

use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_stdlib::{
    eval_standard_function, is_communication_function_block, is_standard_function,
    is_standard_void_function, standard_function_input_index,
};

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

fn apply_program_instance_args(runtime: &mut Runtime<'_>, instance: &ProgramInstance) {
    for arg in &instance.args {
        if arg.output {
            continue;
        }
        let (Some(name), Some(expr)) = (&arg.name, &arg.expr) else {
            runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "program instance '{}' parameter requires a named input value",
                    instance.name.original
                ),
                None,
            ));
            continue;
        };
        let Some(value) = runtime.eval_expr(expr) else {
            runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "program instance '{}' parameter '{}' could not be evaluated",
                    instance.name.original, name.original
                ),
                None,
            ));
            continue;
        };
        runtime.assign(&VariableRef::named(name.original.clone()), value);
    }
}

fn program_instance_output_bindings(instance: &ProgramInstance) -> Vec<ProgramOutputBinding> {
    instance
        .args
        .iter()
        .filter(|arg| arg.output)
        .filter_map(|arg| {
            Some(ProgramOutputBinding {
                formal: arg.name.clone()?,
                target: arg.variable.clone()?,
            })
        })
        .collect()
}

fn collect_program_instance_output_writes(
    scheduled: &mut ScheduledProgram<'_>,
) -> Vec<ProgramOutputWrite> {
    let mut writes = Vec::new();
    for binding in scheduled.output_bindings.clone() {
        let Some(value) = scheduled
            .runtime
            .resolve(&VariableRef::named(binding.formal.original.clone()))
        else {
            scheduled.runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "program instance '{}' output binding '{}' could not be read",
                    scheduled.instance, binding.formal.original
                ),
                None,
            ));
            continue;
        };
        writes.push(ProgramOutputWrite {
            resource: scheduled.resource.clone(),
            instance: scheduled.instance.clone(),
            formal: binding.formal,
            target: binding.target,
            value,
        });
    }
    writes
}

fn apply_program_instance_output_writes(
    project: &Project,
    configuration: &Configuration,
    configuration_state: &mut GlobalState,
    resource_states: &mut BTreeMap<String, GlobalState>,
    direct_state: &mut BTreeMap<String, Value>,
    programs: &mut [ScheduledProgram<'_>],
    options: &RuntimeOptions,
    communication: &dyn CommunicationHooks,
    writes: Vec<ProgramOutputWrite>,
    diagnostics: &mut Vec<Diagnostic>,
) {
    for write in writes {
        let Some(target) = output_binding_access_target(&write.target) else {
            let resource_context = configuration
                .resources
                .iter()
                .find(|resource| resource.name.canonical == canonical_identifier(&write.resource));
            let access_name = format!(
                "program instance '{}.{}' output '{}'",
                write.resource, write.instance, write.formal.original
            );
            assign_configuration_output_variable_target(
                project,
                configuration,
                configuration_state,
                resource_states,
                direct_state,
                programs,
                resource_context,
                &write.target,
                &access_name,
                write.value,
                options,
                communication,
                diagnostics,
            );
            continue;
        };
        let resource_context = configuration
            .resources
            .iter()
            .find(|resource| resource.name.canonical == canonical_identifier(&write.resource));
        let access_name = format!(
            "program instance '{}.{}' output '{}'",
            write.resource, write.instance, write.formal.original
        );
        assign_configuration_access_target(
            project,
            configuration,
            configuration_state,
            resource_states,
            direct_state,
            programs,
            resource_context,
            &target,
            &access_name,
            write.value,
            diagnostics,
        );
    }
}

fn assign_configuration_output_variable_target(
    project: &Project,
    configuration: &Configuration,
    configuration_state: &mut GlobalState,
    resource_states: &mut BTreeMap<String, GlobalState>,
    direct_state: &mut BTreeMap<String, Value>,
    programs: &mut [ScheduledProgram<'_>],
    resource_context: Option<&Resource>,
    target: &VariableRef,
    access_name: &str,
    value: Value,
    options: &RuntimeOptions,
    communication: &dyn CommunicationHooks,
    diagnostics: &mut Vec<Diagnostic>,
) {
    if let Some(direct) = &target.direct {
        direct_state.insert(direct.clone(), value);
        return;
    }

    if let Some(resource) = resource_context {
        if target
            .root_name()
            .is_some_and(|root| root.canonical == resource.name.canonical)
        {
            if let Some(stripped) = strip_variable_root(target) {
                if assign_resource_output_variable_target(
                    project,
                    resource,
                    &stripped,
                    resource_states,
                    programs,
                    value.clone(),
                    options,
                    communication,
                    diagnostics,
                ) {
                    return;
                }
            }
        }
        if assign_resource_output_variable_target(
            project,
            resource,
            target,
            resource_states,
            programs,
            value.clone(),
            options,
            communication,
            diagnostics,
        ) {
            return;
        }
    }

    if assign_global_state_variable_target(
        project,
        configuration_state,
        target,
        value.clone(),
        options,
        communication,
        diagnostics,
    ) {
        return;
    }

    if let Some(root) = target.root_name() {
        if let Some(resource) = configuration
            .resources
            .iter()
            .find(|resource| resource.name.canonical == root.canonical)
        {
            if let Some(stripped) = strip_variable_root(target) {
                if assign_resource_output_variable_target(
                    project,
                    resource,
                    &stripped,
                    resource_states,
                    programs,
                    value,
                    options,
                    communication,
                    diagnostics,
                ) {
                    return;
                }
            }
        }
    }

    diagnostics.push(Diagnostic::error(
        DiagnosticCode::Runtime,
        format!(
            "VAR_ACCESS path '{access_name}' references unknown target '{}'",
            target
        ),
        None,
    ));
}

fn assign_resource_output_variable_target(
    project: &Project,
    resource: &Resource,
    target: &VariableRef,
    resource_states: &mut BTreeMap<String, GlobalState>,
    programs: &mut [ScheduledProgram<'_>],
    value: Value,
    options: &RuntimeOptions,
    communication: &dyn CommunicationHooks,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    if let Some(state) = resource_states.get_mut(&resource.name.canonical) {
        if assign_global_state_variable_target(
            project,
            state,
            target,
            value.clone(),
            options,
            communication,
            diagnostics,
        ) {
            return true;
        }
    }

    let Some(root) = target.root_name() else {
        return false;
    };
    let Some(path) = strip_variable_root(target) else {
        return false;
    };
    let Some(scheduled) = programs.iter_mut().find(|program| {
        canonical_identifier(&program.resource) == resource.name.canonical
            && canonical_identifier(&program.instance) == root.canonical
    }) else {
        return false;
    };
    scheduled.runtime.assign(&path, value);
    true
}

fn assign_global_state_variable_target(
    project: &Project,
    state: &mut GlobalState,
    target: &VariableRef,
    value: Value,
    options: &RuntimeOptions,
    communication: &dyn CommunicationHooks,
    diagnostics: &mut Vec<Diagnostic>,
) -> bool {
    let Some(root) = target.root_name() else {
        return false;
    };
    if !state.types.contains_key(&root.canonical) {
        return false;
    }
    let Some(program) = project.first_program() else {
        return false;
    };
    let mut runtime = Runtime {
        project,
        program,
        env: state.values.clone(),
        types: state.types.clone(),
        il_accumulator: Value::Unit,
        diagnostics: Vec::new(),
        options: options.clone(),
        call_depth: 0,
        communication,
    };
    if runtime.variable_spec(target).is_none() {
        return false;
    }
    runtime.assign(target, value);
    state.values = runtime.env;
    state.types = runtime.types;
    diagnostics.extend(runtime.diagnostics);
    true
}

fn strip_variable_root(variable: &VariableRef) -> Option<VariableRef> {
    if variable.direct.is_some() || variable.path.len() < 2 {
        return None;
    }
    Some(VariableRef {
        path: variable.path[1..].to_vec(),
        indices: variable
            .indices
            .get(1..)
            .map(|indices| indices.to_vec())
            .unwrap_or_default(),
        direct: None,
    })
}

fn output_binding_access_target(variable: &VariableRef) -> Option<String> {
    if variable.indices.iter().any(|indices| !indices.is_empty()) {
        return None;
    }
    variable
        .direct
        .clone()
        .or_else(|| (!variable.path.is_empty()).then(|| variable.to_string()))
}

fn find_program<'a>(project: &'a Project, program_name: Option<&str>) -> Option<&'a Pou> {
    if let Some(name) = program_name {
        project
            .find_pou(name)
            .filter(|pou| matches!(&pou.kind, PouKind::Program))
    } else {
        project.first_program()
    }
}

fn project_global_var_decls<'a>(
    project: &'a Project,
    current_pou: &str,
) -> Vec<(&'a VarDecl, Option<RetainKind>)> {
    let mut vars = Vec::new();
    for pou in project.pous() {
        if pou.name.canonical == current_pou {
            continue;
        }
        for block in &pou.var_blocks {
            if block.kind == VarBlockKind::Global {
                vars.extend(block.vars.iter().map(|var| (var, block.retain)));
            }
        }
    }
    for configuration in project.library_elements.iter().filter_map(|element| {
        if let LibraryElement::Configuration(configuration) = element {
            Some(configuration)
        } else {
            None
        }
    }) {
        for block in configuration.var_blocks.iter().chain(
            configuration
                .resources
                .iter()
                .flat_map(|resource| resource.var_blocks.iter()),
        ) {
            if block.kind == VarBlockKind::Global {
                vars.extend(block.vars.iter().map(|var| (var, block.retain)));
            }
        }
    }
    vars
}

fn find_configuration<'a>(
    project: &'a Project,
    configuration_name: Option<&str>,
) -> Option<&'a Configuration> {
    let expected = configuration_name.map(canonical_identifier);
    project.library_elements.iter().find_map(|element| {
        let LibraryElement::Configuration(configuration) = element else {
            return None;
        };
        if expected
            .as_ref()
            .map_or(true, |expected| *expected == configuration.name.canonical)
        {
            Some(configuration)
        } else {
            None
        }
    })
}

fn task_interval_ms(task: Option<&Task>, default_cycle_time_ms: i128) -> Option<i128> {
    match task {
        None => Some(default_cycle_time_ms.max(1)),
        Some(task) => match task.interval.as_ref() {
            Some(Expr::Literal(Literal::DurationMs(value))) => Some((*value).max(1)),
            Some(Expr::Literal(Literal::Int(value))) => Some((*value as i128).max(1)),
            _ if task.single.is_none() => Some(default_cycle_time_ms.max(1)),
            _ => None,
        },
    }
}

fn task_priority(task: Option<&Task>) -> u32 {
    let Some(priority) = task.and_then(|task| task.priority.as_ref()) else {
        return u32::MAX;
    };
    match priority {
        Expr::Literal(Literal::Int(value)) => u32::try_from(*value).unwrap_or(u32::MAX),
        _ => u32::MAX,
    }
}

fn scheduled_task_single_due(
    project: &Project,
    scheduled: &mut ScheduledProgram<'_>,
    configuration_state: &GlobalState,
    resource_states: &BTreeMap<String, GlobalState>,
    options: &RuntimeOptions,
    communication: &dyn CommunicationHooks,
) -> bool {
    let Some(single) = scheduled.single.clone() else {
        return false;
    };
    let resource_key = canonical_identifier(&scheduled.resource);
    let resource_state = resource_states.get(&resource_key);
    let (env, types) = task_event_environment(configuration_state, resource_state);
    let mut event_runtime = Runtime {
        project,
        program: scheduled.runtime.program,
        env,
        types,
        il_accumulator: Value::Unit,
        diagnostics: Vec::new(),
        options: options.clone(),
        call_depth: 0,
        communication,
    };
    let current = match event_runtime.eval_expr(&single) {
        Some(Value::Bool(value)) => value,
        Some(value) => {
            scheduled.runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "task '{}' SINGLE expects BOOL, got {}",
                    scheduled.task.as_deref().unwrap_or(&scheduled.instance),
                    runtime_value_label(&value)
                ),
                None,
            ));
            false
        }
        None => false,
    };
    scheduled
        .runtime
        .diagnostics
        .extend(event_runtime.diagnostics);
    let due = current && !scheduled.single_previous;
    scheduled.single_previous = current;
    due
}

fn task_event_environment(
    configuration_state: &GlobalState,
    resource_state: Option<&GlobalState>,
) -> (BTreeMap<String, Value>, BTreeMap<String, DataTypeSpec>) {
    let mut env = configuration_state.values.clone();
    let mut types = configuration_state.types.clone();
    if let Some(resource_state) = resource_state {
        env.extend(resource_state.values.clone());
        types.extend(resource_state.types.clone());
    }
    (env, types)
}

#[derive(Debug, Clone)]
struct AccessPathDeclaration {
    name: String,
    target: String,
    direction: AccessDirection,
    type_spec: DataTypeSpec,
}

fn access_declarations(blocks: &[VarBlock]) -> Vec<AccessPathDeclaration> {
    blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Access)
        .flat_map(|block| block.vars.iter())
        .filter_map(|var| {
            let access = var.access.as_ref()?;
            Some(AccessPathDeclaration {
                name: var.name.original.clone(),
                target: access.path.trim().to_string(),
                direction: access.direction,
                type_spec: var.type_spec.clone(),
            })
        })
        .collect()
}

fn variable_ref_from_access_path(path: &str) -> Option<VariableRef> {
    let target = path.trim();
    if target.starts_with('%') {
        return Some(VariableRef::direct(target.to_string()));
    }
    let mut identifiers = Vec::new();
    for part in target.split('.') {
        let part = part.trim();
        if !is_symbolic_access_part(part) {
            return None;
        }
        identifiers.push(Identifier::new(part));
    }
    if identifiers.is_empty() {
        return None;
    }
    Some(VariableRef {
        indices: vec![Vec::new(); identifiers.len()],
        path: identifiers,
        direct: None,
    })
}

fn is_symbolic_access_part(part: &str) -> bool {
    let mut chars = part.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

fn is_direct_location_key(location: &str) -> bool {
    location.trim_start().starts_with('%')
}

fn configuration_access_snapshot(
    project: &Project,
    configuration: &Configuration,
    configuration_state: &GlobalState,
    resource_states: &BTreeMap<String, GlobalState>,
    direct_state: &BTreeMap<String, Value>,
    programs: &mut [ScheduledProgram<'_>],
) -> Vec<AccessPathTrace> {
    let mut paths = Vec::new();
    for declaration in access_declarations(&configuration.var_blocks) {
        let value = configuration_access_value(
            project,
            configuration,
            None,
            &declaration.target,
            configuration_state,
            resource_states,
            direct_state,
            programs,
        );
        paths.push(AccessPathTrace {
            name: declaration.name,
            target: declaration.target,
            direction: declaration.direction,
            value,
        });
    }
    for resource in &configuration.resources {
        for declaration in access_declarations(&resource.var_blocks) {
            let value = configuration_access_value(
                project,
                configuration,
                Some(resource),
                &declaration.target,
                configuration_state,
                resource_states,
                direct_state,
                programs,
            );
            paths.push(AccessPathTrace {
                name: format!("{}.{}", resource.name.original, declaration.name),
                target: declaration.target,
                direction: declaration.direction,
                value,
            });
        }
    }
    paths
}

fn configuration_access_value(
    project: &Project,
    configuration: &Configuration,
    resource_context: Option<&Resource>,
    target: &str,
    configuration_state: &GlobalState,
    resource_states: &BTreeMap<String, GlobalState>,
    direct_state: &BTreeMap<String, Value>,
    programs: &mut [ScheduledProgram<'_>],
) -> Option<Value> {
    let target = target.trim();
    if target.starts_with('%') {
        return direct_state.get(target).cloned().or(Some(Value::Int(0)));
    }

    let parts = access_path_parts(target)?;
    if let Some(resource) = resource_context {
        if parts.first() == Some(&resource.name.canonical) {
            return resource_access_value(
                project,
                resource,
                &parts[1..],
                resource_states,
                programs,
            );
        }
        if let Some(value) =
            resource_access_value(project, resource, &parts, resource_states, programs)
        {
            return Some(value);
        }
    }

    if let Some(value) = configuration_state.access_value(project, &parts) {
        return Some(value);
    }

    let resource = configuration
        .resources
        .iter()
        .find(|resource| resource.name.canonical == parts[0])?;
    resource_access_value(project, resource, &parts[1..], resource_states, programs)
}

fn resource_access_value(
    project: &Project,
    resource: &Resource,
    parts: &[String],
    resource_states: &BTreeMap<String, GlobalState>,
    programs: &mut [ScheduledProgram<'_>],
) -> Option<Value> {
    if parts.is_empty() {
        return None;
    }
    if let Some(value) = resource_states
        .get(&resource.name.canonical)
        .and_then(|state| state.access_value(project, parts))
    {
        return Some(value);
    }

    let instance_name = &parts[0];
    let path = parts.get(1..)?;
    let scheduled = programs.iter_mut().find(|program| {
        canonical_identifier(&program.resource) == resource.name.canonical
            && canonical_identifier(&program.instance) == *instance_name
    })?;
    scheduled.runtime.access_path_value(&path.join("."))
}

fn access_path_parts(path: &str) -> Option<Vec<String>> {
    let parts = path
        .split('.')
        .map(str::trim)
        .map(|part| is_symbolic_access_part(part).then(|| canonical_identifier(part)))
        .collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then_some(parts)
}

#[derive(Debug, Clone)]
struct GlobalState {
    values: BTreeMap<String, Value>,
    types: BTreeMap<String, DataTypeSpec>,
}

impl GlobalState {
    fn from_blocks(project: &Project, blocks: &[VarBlock]) -> Self {
        let mut values = BTreeMap::new();
        let mut types = BTreeMap::new();
        for block in blocks
            .iter()
            .filter(|block| block.kind != VarBlockKind::Access)
        {
            for var in &block.vars {
                values.insert(
                    var.name.canonical.clone(),
                    global_initial_value_for_spec(
                        project,
                        &var.type_spec,
                        var.initial_value.as_ref(),
                    ),
                );
                types.insert(var.name.canonical.clone(), var.type_spec.clone());
            }
        }
        Self { values, types }
    }

    fn access_value(&self, project: &Project, parts: &[String]) -> Option<Value> {
        let root = parts.first()?;
        let mut value = self.values.get(root)?.clone();
        let mut spec = self.types.get(root)?.clone();
        for part in &parts[1..] {
            let DataTypeSpec::Struct { fields } = resolve_project_spec(project, &spec) else {
                return None;
            };
            let field = fields.iter().find(|field| field.name.canonical == *part)?;
            let Value::Struct(values) = value else {
                return None;
            };
            value = values.get(part)?.clone();
            spec = field.spec.clone();
        }
        Some(value)
    }

    fn assign(&mut self, project: &Project, parts: &[String], value: Value) -> bool {
        let Some(root) = parts.first() else {
            return false;
        };
        let Some(spec) = self.types.get(root).cloned() else {
            return false;
        };
        let Some(mut current) = self.values.get(root).cloned() else {
            return false;
        };
        if assign_into_global_value(project, &mut current, &spec, parts, 0, value) {
            self.values.insert(root.clone(), current);
            true
        } else {
            false
        }
    }
}

fn apply_configuration_access_writes(
    project: &Project,
    configuration: &Configuration,
    configuration_state: &mut GlobalState,
    resource_states: &mut BTreeMap<String, GlobalState>,
    direct_state: &mut BTreeMap<String, Value>,
    programs: &mut [ScheduledProgram<'_>],
    cycle: usize,
    writes: &[AccessPathWrite],
    diagnostics: &mut Vec<Diagnostic>,
) {
    let bindings = configuration_access_bindings(configuration);
    for write in writes.iter().filter(|write| write.cycle == cycle) {
        let requested = canonical_identifier(&write.name);
        let matches = bindings
            .iter()
            .filter(|binding| {
                binding.qualified_canonical == requested || binding.short_canonical == requested
            })
            .collect::<Vec<_>>();
        let Some(binding) = matches.first().copied() else {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("unknown VAR_ACCESS path '{}'", write.name),
                None,
            ));
            continue;
        };
        if matches.len() > 1 && binding.qualified_canonical != requested {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("ambiguous VAR_ACCESS path '{}'", write.name),
                None,
            ));
            continue;
        }
        if binding.direction != AccessDirection::ReadWrite {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("VAR_ACCESS path '{}' is READ_ONLY", binding.qualified_name),
                None,
            ));
            continue;
        }
        if !runtime_value_matches_project_spec(project, &write.value, &binding.type_spec) {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "VAR_ACCESS path '{}' expects {}, got {}",
                    binding.qualified_name,
                    runtime_spec_label(&resolve_project_spec(project, &binding.type_spec)),
                    runtime_value_label(&write.value)
                ),
                None,
            ));
            continue;
        }
        assign_configuration_access_target(
            project,
            configuration,
            configuration_state,
            resource_states,
            direct_state,
            programs,
            binding.resource_context,
            &binding.target,
            &binding.qualified_name,
            write.value.clone(),
            diagnostics,
        );
    }
}

#[derive(Debug, Clone)]
struct ConfigurationAccessBinding<'a> {
    qualified_name: String,
    qualified_canonical: String,
    short_canonical: String,
    target: String,
    direction: AccessDirection,
    type_spec: DataTypeSpec,
    resource_context: Option<&'a Resource>,
}

fn configuration_access_bindings(
    configuration: &Configuration,
) -> Vec<ConfigurationAccessBinding<'_>> {
    let mut bindings = access_declarations(&configuration.var_blocks)
        .into_iter()
        .map(|declaration| ConfigurationAccessBinding {
            qualified_canonical: canonical_identifier(&declaration.name),
            short_canonical: canonical_identifier(&declaration.name),
            qualified_name: declaration.name,
            target: declaration.target,
            direction: declaration.direction,
            type_spec: declaration.type_spec,
            resource_context: None,
        })
        .collect::<Vec<_>>();

    for resource in &configuration.resources {
        for declaration in access_declarations(&resource.var_blocks) {
            let qualified_name = format!("{}.{}", resource.name.original, declaration.name);
            bindings.push(ConfigurationAccessBinding {
                qualified_canonical: canonical_identifier(&qualified_name),
                short_canonical: canonical_identifier(&declaration.name),
                qualified_name,
                target: declaration.target,
                direction: declaration.direction,
                type_spec: declaration.type_spec,
                resource_context: Some(resource),
            });
        }
    }
    bindings
}

fn assign_configuration_access_target(
    project: &Project,
    configuration: &Configuration,
    configuration_state: &mut GlobalState,
    resource_states: &mut BTreeMap<String, GlobalState>,
    direct_state: &mut BTreeMap<String, Value>,
    programs: &mut [ScheduledProgram<'_>],
    resource_context: Option<&Resource>,
    target: &str,
    access_name: &str,
    value: Value,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let target = target.trim();
    if target.starts_with('%') {
        direct_state.insert(target.to_string(), value);
        return;
    }

    let Some(parts) = access_path_parts(target) else {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::Runtime,
            format!("VAR_ACCESS path '{access_name}' has invalid target '{target}'"),
            None,
        ));
        return;
    };

    if let Some(resource) = resource_context {
        if parts.first() == Some(&resource.name.canonical)
            && assign_resource_access_target(
                project,
                resource,
                &parts[1..],
                resource_states,
                programs,
                value.clone(),
            )
        {
            return;
        }
        if assign_resource_access_target(
            project,
            resource,
            &parts,
            resource_states,
            programs,
            value.clone(),
        ) {
            return;
        }
    }

    if configuration_state.assign(project, &parts, value.clone()) {
        return;
    }

    if let Some(resource) = configuration
        .resources
        .iter()
        .find(|resource| resource.name.canonical == parts[0])
    {
        if assign_resource_access_target(
            project,
            resource,
            &parts[1..],
            resource_states,
            programs,
            value,
        ) {
            return;
        }
    }

    diagnostics.push(Diagnostic::error(
        DiagnosticCode::Runtime,
        format!("VAR_ACCESS path '{access_name}' references unknown target '{target}'"),
        None,
    ));
}

fn assign_resource_access_target(
    project: &Project,
    resource: &Resource,
    parts: &[String],
    resource_states: &mut BTreeMap<String, GlobalState>,
    programs: &mut [ScheduledProgram<'_>],
    value: Value,
) -> bool {
    if parts.is_empty() {
        return false;
    }
    if resource_states
        .get_mut(&resource.name.canonical)
        .is_some_and(|state| state.assign(project, parts, value.clone()))
    {
        return true;
    }

    let instance_name = &parts[0];
    let Some(path) = parts.get(1..) else {
        return false;
    };
    let Some(scheduled) = programs.iter_mut().find(|program| {
        canonical_identifier(&program.resource) == resource.name.canonical
            && canonical_identifier(&program.instance) == *instance_name
    }) else {
        return false;
    };
    scheduled
        .runtime
        .assign_access_target("configuration VAR_ACCESS", &path.join("."), value)
}

fn global_initial_value_for_spec(
    project: &Project,
    spec: &DataTypeSpec,
    initial: Option<&Expr>,
) -> Value {
    match (resolve_project_spec(project, spec), initial) {
        (
            DataTypeSpec::Array {
                ranges: _,
                element_type,
            },
            Some(Expr::ArrayLiteral(elements)),
        ) => Value::Array(
            elements
                .iter()
                .map(|expr| global_initial_value_for_spec(project, &element_type, Some(expr)))
                .collect(),
        ),
        (
            DataTypeSpec::Array {
                ranges,
                element_type,
            },
            _,
        ) => Value::Array(
            (0..array_element_count(&ranges))
                .map(|_| global_initial_value_for_spec(project, &element_type, None))
                .collect(),
        ),
        (DataTypeSpec::Struct { fields }, Some(Expr::StructLiteral(initializers))) => {
            let mut values = BTreeMap::new();
            for field in fields {
                let initializer = initializers.iter().find(|initializer| {
                    initializer
                        .name
                        .as_ref()
                        .is_some_and(|name| name.canonical == field.name.canonical)
                });
                let value = initializer
                    .and_then(|initializer| initializer.expr.as_ref())
                    .or(field.initial_value.as_ref())
                    .map(|expr| global_initial_value_for_spec(project, &field.spec, Some(expr)))
                    .unwrap_or_else(|| global_initial_value_for_spec(project, &field.spec, None));
                values.insert(field.name.canonical.clone(), value);
            }
            Value::Struct(values)
        }
        (DataTypeSpec::Struct { fields }, _) => {
            let mut values = BTreeMap::new();
            for field in fields {
                let value = field
                    .initial_value
                    .as_ref()
                    .map(|expr| global_initial_value_for_spec(project, &field.spec, Some(expr)))
                    .unwrap_or_else(|| global_initial_value_for_spec(project, &field.spec, None));
                values.insert(field.name.canonical.clone(), value);
            }
            Value::Struct(values)
        }
        (DataTypeSpec::Enum { .. }, Some(expr)) => enum_ordinal_expr(project, expr)
            .map(Value::Int)
            .or_else(|| expr_literal_value(project, expr))
            .unwrap_or(Value::Int(0)),
        (resolved, Some(expr)) => {
            expr_literal_value(project, expr).unwrap_or_else(|| default_value_for_type(&resolved))
        }
        (resolved, None) => default_value_for_type(&resolved),
    }
}

fn assign_into_global_value(
    project: &Project,
    current: &mut Value,
    spec: &DataTypeSpec,
    parts: &[String],
    segment_index: usize,
    value: Value,
) -> bool {
    if segment_index + 1 >= parts.len() {
        *current = value;
        return true;
    }

    let DataTypeSpec::Struct { fields } = resolve_project_spec(project, spec) else {
        return false;
    };
    let next = &parts[segment_index + 1];
    let Some(field) = fields.iter().find(|field| field.name.canonical == *next) else {
        return false;
    };
    let Value::Struct(values) = current else {
        return false;
    };
    let Some(field_value) = values.get_mut(next) else {
        return false;
    };
    assign_into_global_value(
        project,
        field_value,
        &field.spec,
        parts,
        segment_index + 1,
        value,
    )
}

fn expr_literal_value(project: &Project, expr: &Expr) -> Option<Value> {
    match expr {
        Expr::Literal(literal) => Some(literal_to_value(project, literal)),
        Expr::ArrayLiteral(elements) => elements
            .iter()
            .map(|expr| expr_literal_value(project, expr))
            .collect::<Option<Vec<_>>>()
            .map(Value::Array),
        Expr::StructLiteral(initializers) => {
            let mut values = BTreeMap::new();
            for initializer in initializers {
                let name = initializer.name.as_ref()?;
                let value = initializer
                    .expr
                    .as_ref()
                    .and_then(|expr| expr_literal_value(project, expr))?;
                values.insert(name.canonical.clone(), value);
            }
            Some(Value::Struct(values))
        }
        Expr::Call { name, args } if is_standard_function(&name.original) => {
            let values = literal_standard_call_args(project, name, args)?;
            eval_standard_function(&name.original, &values)
        }
        _ => None,
    }
}

fn literal_standard_call_args(
    project: &Project,
    name: &Identifier,
    args: &[ParamAssignment],
) -> Option<Vec<Value>> {
    let mut positional = Vec::new();
    let mut named = BTreeMap::<usize, Value>::new();
    for arg in args {
        if arg.output {
            return None;
        }
        let value = expr_literal_value(project, arg.expr.as_ref()?)?;
        if let Some(arg_name) = &arg.name {
            let index = standard_function_input_index(&name.original, &arg_name.original)?;
            named.insert(index, value);
        } else {
            positional.push(value);
        }
    }
    if named.is_empty() {
        return Some(positional);
    }
    for (index, value) in positional.into_iter().enumerate() {
        named.entry(index).or_insert(value);
    }
    let max_index = *named.keys().max()?;
    (0..=max_index)
        .map(|index| named.remove(&index))
        .collect::<Option<Vec<_>>>()
}

fn resolve_project_spec(project: &Project, spec: &DataTypeSpec) -> DataTypeSpec {
    resolve_project_spec_inner(project, spec, &mut BTreeSet::new())
}

fn resolve_project_spec_inner(
    project: &Project,
    spec: &DataTypeSpec,
    seen: &mut BTreeSet<String>,
) -> DataTypeSpec {
    let DataTypeSpec::Named(name) = spec else {
        return spec.clone();
    };
    if !seen.insert(name.canonical.clone()) {
        return spec.clone();
    }
    let Some(data_type) = project
        .data_types()
        .find(|data_type| data_type.name.canonical == name.canonical)
    else {
        return spec.clone();
    };
    resolve_project_spec_inner(project, &data_type.spec, seen)
}

fn enum_ordinal_expr(project: &Project, expr: &Expr) -> Option<i64> {
    if let Expr::Literal(Literal::Typed { type_name, value }) = expr {
        return enum_ordinal_typed(project, type_name, value);
    }

    let Expr::Variable(variable) = expr else {
        return None;
    };
    if variable.direct.is_some()
        || variable.path.len() != 1
        || variable.indices.iter().any(|indices| !indices.is_empty())
    {
        return None;
    }
    enum_ordinal_name(project, &variable.root_name()?.canonical)
}

fn enum_ordinal_typed(project: &Project, type_name: &Identifier, value_name: &str) -> Option<i64> {
    project.data_types().find_map(|data_type| {
        if data_type.name.canonical != type_name.canonical {
            return None;
        }
        let DataTypeSpec::Enum { values } = &data_type.spec else {
            return None;
        };
        let value_name = canonical_identifier(value_name);
        values
            .iter()
            .position(|value| value.canonical == value_name)
            .map(|index| index as i64)
    })
}

fn enum_ordinal_name(project: &Project, canonical_name: &str) -> Option<i64> {
    for data_type in project.data_types() {
        if let DataTypeSpec::Enum { values } = &data_type.spec {
            if let Some(index) = values
                .iter()
                .position(|value| value.canonical == canonical_name)
            {
                return Some(index as i64);
            }
        }
    }
    None
}

fn runtime_value_matches_project_spec(
    project: &Project,
    value: &Value,
    spec: &DataTypeSpec,
) -> bool {
    match resolve_project_spec(project, spec) {
        DataTypeSpec::Elementary(elementary) => match elementary {
            ElementaryType::Bool => matches!(value, Value::Bool(_)),
            ElementaryType::Sint
            | ElementaryType::Int
            | ElementaryType::Dint
            | ElementaryType::Lint
            | ElementaryType::Usint
            | ElementaryType::Uint
            | ElementaryType::Udint
            | ElementaryType::Ulint
            | ElementaryType::Byte
            | ElementaryType::Word
            | ElementaryType::Dword
            | ElementaryType::Lword => matches!(value, Value::Int(_)),
            ElementaryType::Real | ElementaryType::Lreal => {
                matches!(value, Value::Int(_) | Value::Real(_))
            }
            ElementaryType::String => matches!(value, Value::String(_)),
            ElementaryType::WString => matches!(value, Value::WString(_)),
            ElementaryType::Time
            | ElementaryType::Date
            | ElementaryType::TimeOfDay
            | ElementaryType::DateAndTime => matches!(value, Value::TimeMs(_)),
        },
        DataTypeSpec::String { wide, .. } => {
            matches!(
                (wide, value),
                (false, Value::String(_)) | (true, Value::WString(_))
            )
        }
        DataTypeSpec::Subrange { range, .. } => value
            .as_i64()
            .is_some_and(|value| value >= range.low && value <= range.high),
        DataTypeSpec::Enum { .. } => matches!(value, Value::Int(_)),
        DataTypeSpec::Array {
            ranges,
            element_type,
        } => {
            let Value::Array(values) = value else {
                return false;
            };
            values.len() == array_element_count(&ranges)
                && values
                    .iter()
                    .all(|value| runtime_value_matches_project_spec(project, value, &element_type))
        }
        DataTypeSpec::Struct { fields } => {
            let Value::Struct(values) = value else {
                return false;
            };
            fields.iter().all(|field| {
                values.get(&field.name.canonical).is_some_and(|value| {
                    runtime_value_matches_project_spec(project, value, &field.spec)
                })
            })
        }
        DataTypeSpec::Named(_) => true,
    }
}

struct ScheduledProgram<'a> {
    resource: String,
    instance: String,
    task: Option<String>,
    priority: u32,
    interval_ms: Option<i128>,
    single: Option<Expr>,
    single_previous: bool,
    output_bindings: Vec<ProgramOutputBinding>,
    runtime: Runtime<'a>,
}

#[derive(Debug, Clone)]
struct ProgramOutputBinding {
    formal: Identifier,
    target: VariableRef,
}

#[derive(Debug, Clone)]
struct ProgramOutputWrite {
    resource: String,
    instance: String,
    formal: Identifier,
    target: VariableRef,
    value: Value,
}

struct Runtime<'a> {
    project: &'a Project,
    program: &'a Pou,
    env: BTreeMap<String, Value>,
    types: BTreeMap<String, DataTypeSpec>,
    il_accumulator: Value,
    diagnostics: Vec<Diagnostic>,
    options: RuntimeOptions,
    call_depth: usize,
    communication: &'a dyn CommunicationHooks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupKind {
    Cold,
    Warm,
}

impl Runtime<'_> {
    fn initialize(&mut self, startup: StartupKind) {
        if startup == StartupKind::Cold {
            self.env.clear();
            self.types.clear();
        }
        self.il_accumulator = Value::Unit;

        if let PouKind::Function { return_type } = &self.program.kind {
            let return_type = return_type.clone();
            self.types
                .insert(self.program.name.canonical.clone(), return_type.clone());
            if startup == StartupKind::Cold || !self.env.contains_key(&self.program.name.canonical)
            {
                let value = self.default_value(&return_type);
                self.env.insert(self.program.name.canonical.clone(), value);
            }
        }

        for (var, retain) in project_global_var_decls(self.project, &self.program.name.canonical) {
            self.types
                .insert(var.name.canonical.clone(), var.type_spec.clone());
            let preserve = startup == StartupKind::Warm
                && retain == Some(RetainKind::Retain)
                && self.env.contains_key(&var.name.canonical);
            if preserve {
                continue;
            }
            let value = self.initial_value_for_spec(&var.type_spec, var.initial_value.as_ref());
            self.env.insert(var.name.canonical.clone(), value);
            self.initialize_function_block_fields(var);
        }

        for block in &self.program.var_blocks {
            if matches!(block.kind, VarBlockKind::Access | VarBlockKind::External) {
                continue;
            }
            for var in &block.vars {
                self.types
                    .insert(var.name.canonical.clone(), var.type_spec.clone());
                let preserve = startup == StartupKind::Warm
                    && block.retain == Some(RetainKind::Retain)
                    && self.env.contains_key(&var.name.canonical);
                if preserve {
                    continue;
                }

                let value = self.initial_value_for_spec(&var.type_spec, var.initial_value.as_ref());
                self.env.insert(var.name.canonical.clone(), value);
                self.initialize_function_block_fields(var);
            }
        }
        if startup == StartupKind::Cold {
            self.initialize_sfc_steps();
        }
    }

    fn reset_temp_variables(&mut self) {
        let temp_vars = self
            .program
            .var_blocks
            .iter()
            .filter(|block| block.kind == VarBlockKind::Temp)
            .flat_map(|block| block.vars.iter().cloned())
            .collect::<Vec<_>>();
        for var in temp_vars {
            let value = self.initial_value_for_spec(&var.type_spec, var.initial_value.as_ref());
            self.env.insert(var.name.canonical.clone(), value);
            self.initialize_function_block_fields(&var);
        }
    }

    fn initialize_sfc_steps(&mut self) {
        let Some(sfc) = &self.program.body.sfc else {
            return;
        };
        for step in &sfc.steps {
            self.env
                .insert(sfc_step_key(&step.name), Value::Bool(step.initial));
        }
        for action in &sfc.actions {
            self.initialize_sfc_action_control(&sfc_action_control_key(&action.name));
        }
    }

    fn initialize_sfc_action_control(&mut self, key: &str) {
        self.env
            .insert(sfc_action_control_key_stored(key), Value::Bool(false));
        self.env
            .insert(sfc_action_control_key_previous(key), Value::Bool(false));
        self.env
            .insert(sfc_action_control_key_elapsed(key), Value::Int(0));
    }

    fn initial_value_for_spec(&mut self, spec: &DataTypeSpec, initial: Option<&Expr>) -> Value {
        match (self.resolve_named_spec(spec), initial) {
            (
                DataTypeSpec::Array {
                    ranges: _,
                    element_type,
                },
                Some(Expr::ArrayLiteral(elements)),
            ) => Value::Array(
                elements
                    .iter()
                    .map(|expr| self.initial_value_for_spec(&element_type, Some(expr)))
                    .collect::<Vec<_>>(),
            ),
            (
                DataTypeSpec::Array {
                    ranges,
                    element_type,
                },
                _,
            ) => Value::Array(
                (0..array_element_count(&ranges))
                    .map(|_| self.initial_value_for_spec(&element_type, None))
                    .collect(),
            ),
            (DataTypeSpec::Struct { fields }, Some(Expr::StructLiteral(initializers))) => {
                let mut values = BTreeMap::new();
                for field in fields {
                    let initializer = initializers.iter().find(|initializer| {
                        initializer
                            .name
                            .as_ref()
                            .is_some_and(|name| name.canonical == field.name.canonical)
                    });
                    let value = initializer
                        .and_then(|initializer| initializer.expr.as_ref())
                        .or(field.initial_value.as_ref())
                        .map(|expr| self.initial_value_for_spec(&field.spec, Some(expr)))
                        .unwrap_or_else(|| self.initial_value_for_spec(&field.spec, None));
                    values.insert(field.name.canonical.clone(), value);
                }
                Value::Struct(values)
            }
            (DataTypeSpec::Struct { fields }, _) => {
                let mut values = BTreeMap::new();
                for field in fields {
                    let value = field
                        .initial_value
                        .as_ref()
                        .map(|expr| self.initial_value_for_spec(&field.spec, Some(expr)))
                        .unwrap_or_else(|| self.initial_value_for_spec(&field.spec, None));
                    values.insert(field.name.canonical.clone(), value);
                }
                Value::Struct(values)
            }
            (DataTypeSpec::Enum { values: _ }, Some(expr)) => self
                .enum_ordinal_expr(expr)
                .map(Value::Int)
                .unwrap_or_else(|| self.eval_expr(expr).unwrap_or(Value::Int(0))),
            (DataTypeSpec::Enum { .. }, None) => Value::Int(0),
            (DataTypeSpec::Subrange { range, .. }, Some(expr)) => {
                let value = self.eval_expr(expr).unwrap_or(Value::Int(0));
                self.constrain_value(
                    &DataTypeSpec::Subrange {
                        base: ElementaryType::Int,
                        range,
                    },
                    value,
                )
            }
            (resolved, Some(expr)) => {
                let value = self
                    .eval_expr(expr)
                    .unwrap_or_else(|| self.default_value(&resolved));
                self.constrain_value(&resolved, value)
            }
            (resolved, None) => self.default_value(&resolved),
        }
    }

    fn default_value(&mut self, spec: &DataTypeSpec) -> Value {
        match self.resolve_named_spec(spec) {
            DataTypeSpec::Array {
                ranges,
                element_type,
            } => Value::Array(
                (0..array_element_count(&ranges))
                    .map(|_| self.default_value(&element_type))
                    .collect(),
            ),
            DataTypeSpec::Struct { fields } => {
                let mut values = BTreeMap::new();
                for field in fields {
                    let value = field
                        .initial_value
                        .as_ref()
                        .map(|expr| self.initial_value_for_spec(&field.spec, Some(expr)))
                        .unwrap_or_else(|| self.default_value(&field.spec));
                    values.insert(field.name.canonical.clone(), value);
                }
                Value::Struct(values)
            }
            DataTypeSpec::Enum { .. } => Value::Int(0),
            DataTypeSpec::Subrange { range, .. } => {
                if range.low <= 0 && range.high >= 0 {
                    Value::Int(0)
                } else {
                    Value::Int(range.low)
                }
            }
            resolved => default_value_for_type(&resolved),
        }
    }

    fn resolve_named_spec(&self, spec: &DataTypeSpec) -> DataTypeSpec {
        resolve_project_spec(self.project, spec)
    }

    fn initialize_function_block_fields(&mut self, var: &VarDecl) {
        self.initialize_function_block_instance(&var.name.canonical, &var.type_spec);
    }

    fn initialize_function_block_instance(&mut self, instance: &str, spec: &DataTypeSpec) {
        let DataTypeSpec::Named(type_name) = spec else {
            return;
        };

        match type_name.canonical.as_str() {
            "SR" | "RS" => {
                self.set_field(instance, "Q1", Value::Bool(false));
            }
            "R_TRIG" | "F_TRIG" => {
                self.set_field(instance, "Q", Value::Bool(false));
                self.set_field(instance, "M", Value::Bool(false));
            }
            "CTU" => {
                self.set_field(instance, "Q", Value::Bool(false));
                self.set_field(instance, "CV", Value::Int(0));
                self.set_field(instance, "_CU", Value::Bool(false));
            }
            "CTD" => {
                self.set_field(instance, "Q", Value::Bool(false));
                self.set_field(instance, "CV", Value::Int(0));
                self.set_field(instance, "_CD", Value::Bool(false));
            }
            "CTUD" => {
                self.set_field(instance, "QU", Value::Bool(false));
                self.set_field(instance, "QD", Value::Bool(false));
                self.set_field(instance, "CV", Value::Int(0));
                self.set_field(instance, "_CU", Value::Bool(false));
                self.set_field(instance, "_CD", Value::Bool(false));
            }
            "TON" | "TOF" | "TP" => {
                self.set_field(instance, "Q", Value::Bool(false));
                self.set_field(instance, "ET", Value::TimeMs(0));
                self.set_field(instance, "_IN", Value::Bool(false));
                self.set_field(instance, "_RUN", Value::Bool(false));
            }
            name if is_communication_function_block(name) => {
                self.initialize_communication_function_block_fields(instance);
            }
            _ => self.initialize_user_function_block_fields(instance, type_name),
        }
    }

    fn initialize_communication_function_block_fields(&mut self, instance: &str) {
        self.set_field(instance, "DONE", Value::Bool(false));
        self.set_field(instance, "NDR", Value::Bool(false));
        self.set_field(instance, "ERROR", Value::Bool(false));
        self.set_field(instance, "STATUS", Value::Int(0));
    }

    fn initialize_user_function_block_fields(&mut self, instance: &str, type_name: &Identifier) {
        let Some(function_block) = self
            .project
            .find_pou(&type_name.original)
            .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
        else {
            return;
        };

        for field in function_block.variable_declarations() {
            if function_block_field_specs(self.project, &field.type_spec).is_some() {
                self.initialize_function_block_instance(
                    &field_key(instance, &field.name.canonical),
                    &field.type_spec,
                );
            } else {
                let value =
                    self.initial_value_for_spec(&field.type_spec, field.initial_value.as_ref());
                self.set_field(instance, &field.name.canonical, value);
            }
            if field.edge.is_some() {
                self.set_field(
                    instance,
                    &edge_state_field_name(&field.name.canonical),
                    Value::Bool(false),
                );
            }
        }
    }

    fn snapshot(&self) -> Vec<(String, Value)> {
        self.env
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    fn sync_direct_state(&mut self, direct_state: &BTreeMap<String, Value>) {
        for (location, value) in direct_state {
            self.env.insert(location.clone(), value.clone());
        }
    }

    fn export_direct_state(&self, direct_state: &mut BTreeMap<String, Value>) {
        for (location, value) in self
            .env
            .iter()
            .filter(|(location, _)| is_direct_location_key(location))
        {
            direct_state.insert(location.clone(), value.clone());
        }
    }

    fn access_snapshot(&mut self) -> Vec<AccessPathTrace> {
        let declarations = access_declarations(&self.program.var_blocks);
        declarations
            .into_iter()
            .map(|declaration| AccessPathTrace {
                value: self.access_path_value(&declaration.target),
                name: declaration.name,
                target: declaration.target,
                direction: declaration.direction,
            })
            .collect()
    }

    fn access_path_value(&mut self, target: &str) -> Option<Value> {
        let variable = variable_ref_from_access_path(target)?;
        self.resolve(&variable)
    }

    fn apply_access_writes(&mut self, cycle: usize, writes: &[AccessPathWrite]) {
        let declarations = access_declarations(&self.program.var_blocks);
        for write in writes.iter().filter(|write| write.cycle == cycle) {
            let Some(declaration) = declarations.iter().find(|declaration| {
                canonical_identifier(&declaration.name) == canonical_identifier(&write.name)
            }) else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!("unknown VAR_ACCESS path '{}'", write.name),
                    None,
                ));
                continue;
            };
            if declaration.direction != AccessDirection::ReadWrite {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!("VAR_ACCESS path '{}' is READ_ONLY", declaration.name),
                    None,
                ));
                continue;
            }
            self.assign_access_target(&declaration.name, &declaration.target, write.value.clone());
        }
    }

    fn assign_access_target(&mut self, access_name: &str, target: &str, value: Value) -> bool {
        let Some(variable) = variable_ref_from_access_path(target) else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("VAR_ACCESS path '{access_name}' has invalid target '{target}'"),
                None,
            ));
            return false;
        };
        if let Some(spec) = self.variable_spec(&variable) {
            if !self.runtime_value_matches_spec(&value, &spec) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!(
                        "VAR_ACCESS path '{access_name}' expects {}, got {}",
                        runtime_spec_label(&self.resolve_named_spec(&spec)),
                        runtime_value_label(&value)
                    ),
                    None,
                ));
                return false;
            }
        }
        self.assign(&variable, value);
        true
    }

    fn variable_spec(&self, variable: &VariableRef) -> Option<DataTypeSpec> {
        if variable.direct.is_some() {
            return None;
        }
        let root = variable.root_name()?;
        let mut spec = self.types.get(&root.canonical).cloned()?;
        for segment in variable.path.iter().skip(1) {
            let resolved = self.resolve_named_spec(&spec);
            let DataTypeSpec::Struct { fields } = resolved else {
                return None;
            };
            spec = fields
                .iter()
                .find(|field| field.name.canonical == segment.canonical)
                .map(|field| field.spec.clone())?;
        }
        Some(spec)
    }

    fn runtime_value_matches_spec(&self, value: &Value, spec: &DataTypeSpec) -> bool {
        match self.resolve_named_spec(spec) {
            DataTypeSpec::Elementary(elementary) => match elementary {
                ElementaryType::Bool => matches!(value, Value::Bool(_)),
                ElementaryType::Sint
                | ElementaryType::Int
                | ElementaryType::Dint
                | ElementaryType::Lint
                | ElementaryType::Usint
                | ElementaryType::Uint
                | ElementaryType::Udint
                | ElementaryType::Ulint
                | ElementaryType::Byte
                | ElementaryType::Word
                | ElementaryType::Dword
                | ElementaryType::Lword => matches!(value, Value::Int(_)),
                ElementaryType::Real | ElementaryType::Lreal => {
                    matches!(value, Value::Int(_) | Value::Real(_))
                }
                ElementaryType::String => matches!(value, Value::String(_)),
                ElementaryType::WString => matches!(value, Value::WString(_)),
                ElementaryType::Time
                | ElementaryType::Date
                | ElementaryType::TimeOfDay
                | ElementaryType::DateAndTime => matches!(value, Value::TimeMs(_)),
            },
            DataTypeSpec::String { wide, .. } => {
                matches!(
                    (wide, value),
                    (false, Value::String(_)) | (true, Value::WString(_))
                )
            }
            DataTypeSpec::Subrange { range, .. } => {
                let Some(value) = value.as_i64() else {
                    return false;
                };
                value >= range.low && value <= range.high
            }
            DataTypeSpec::Enum { .. } => matches!(value, Value::Int(_)),
            DataTypeSpec::Array {
                ranges,
                element_type,
            } => {
                let Value::Array(values) = value else {
                    return false;
                };
                values.len() == array_element_count(&ranges)
                    && values
                        .iter()
                        .all(|value| self.runtime_value_matches_spec(value, &element_type))
            }
            DataTypeSpec::Struct { fields } => {
                let Value::Struct(values) = value else {
                    return false;
                };
                fields.iter().all(|field| {
                    values
                        .get(&field.name.canonical)
                        .is_some_and(|value| self.runtime_value_matches_spec(value, &field.spec))
                })
            }
            DataTypeSpec::Named(_) => true,
        }
    }

    fn execute_block(&mut self, body: &[Statement]) -> Control {
        for statement in body {
            match self.execute_statement(statement) {
                Control::Continue => {}
                control => return control,
            }
        }
        Control::Continue
    }

    fn execute_statement_list(&mut self, statements: &[Statement]) -> Control {
        let labels = statements
            .iter()
            .enumerate()
            .filter_map(|(index, statement)| {
                if let Statement::IlLabel(label) = statement {
                    Some((label.canonical.clone(), index))
                } else {
                    None
                }
            })
            .collect::<BTreeMap<_, _>>();

        let mut ip = 0_usize;
        let mut iterations = 0_usize;
        while ip < statements.len() {
            match self.execute_statement(&statements[ip]) {
                Control::Continue => ip += 1,
                Control::Jump(label) => {
                    let Some(target) = labels.get(&label) else {
                        return Control::Jump(label);
                    };
                    ip = *target;
                }
                control => return control,
            }
            iterations += 1;
            if iterations > self.options.max_loop_iterations {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    "maximum statement execution count exceeded",
                    None,
                ));
                break;
            }
        }
        Control::Continue
    }

    fn execute_program_cycle(&mut self) -> Control {
        if let Some(sfc) = self.program.body.sfc.clone() {
            self.execute_sfc(&sfc)
        } else {
            self.execute_statement_list(&self.program.body.statements.clone())
        }
    }

    fn execute_sfc(&mut self, sfc: &Sfc) -> Control {
        let active_steps = sfc
            .steps
            .iter()
            .filter(|step| {
                self.env
                    .get(&sfc_step_key(&step.name))
                    .and_then(Value::as_bool)
                    == Some(true)
            })
            .map(|step| step.name.canonical.clone())
            .collect::<Vec<_>>();

        for action in &sfc.actions {
            let control_key = sfc_action_control_key(&action.name);
            let inputs = sfc_action_inputs(sfc, action, &active_steps);
            if self.sfc_action_should_execute(&control_key, &action.name, &inputs) {
                match self.execute_statement_list(&action.body) {
                    Control::Continue | Control::Return => {}
                    control => return control,
                }
            }
        }

        let mut candidates = Vec::new();
        for (index, transition) in sfc.transitions.iter().enumerate() {
            let Some((from_steps, to_steps)) = sfc_transition_steps(sfc, transition, index) else {
                continue;
            };
            let from_active = from_steps.iter().all(|step| {
                self.env.get(&sfc_step_key(step)).and_then(Value::as_bool) == Some(true)
            });
            if !from_active {
                continue;
            }
            let condition = transition
                .condition
                .as_ref()
                .and_then(|condition| self.eval_expr(condition))
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if condition {
                candidates.push((
                    transition.priority.unwrap_or(i64::MAX),
                    index,
                    from_steps.into_iter().cloned().collect::<Vec<_>>(),
                    to_steps.into_iter().cloned().collect::<Vec<_>>(),
                ));
            }
        }
        candidates.sort_by_key(|(priority, index, _, _)| (*priority, *index));

        let mut consumed_steps = BTreeSet::new();
        for (_, _, from_steps, to_steps) in candidates {
            if from_steps
                .iter()
                .any(|step| consumed_steps.contains(&step.canonical))
            {
                continue;
            }
            for from in &from_steps {
                consumed_steps.insert(from.canonical.clone());
            }
            for from in from_steps {
                self.env.insert(sfc_step_key(&from), Value::Bool(false));
            }
            for to in to_steps {
                self.env.insert(sfc_step_key(&to), Value::Bool(true));
            }
        }

        Control::Continue
    }

    fn sfc_action_should_execute(
        &mut self,
        control_key: &str,
        action_name: &Identifier,
        inputs: &[SfcActionInput<'_>],
    ) -> bool {
        let active_time_inputs = inputs
            .iter()
            .filter(|input| input.active && input.qualifier.requires_duration())
            .count();
        if active_time_inputs > 1 {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "SFC action '{}' has more than one active time-related association",
                    action_name.original
                ),
                None,
            ));
            return false;
        }

        let reset_active = inputs
            .iter()
            .any(|input| input.active && input.qualifier == SfcActionQualifier::ResetStored);
        if reset_active {
            self.set_sfc_action_stored(control_key, false);
            self.set_sfc_action_elapsed(control_key, 0);
        }

        let non_stored = inputs
            .iter()
            .any(|input| input.active && input.qualifier == SfcActionQualifier::NonStored);

        let pulse_active = inputs
            .iter()
            .any(|input| input.active && input.qualifier == SfcActionQualifier::Pulse);
        let has_pulse_falling_input = inputs
            .iter()
            .any(|input| input.qualifier == SfcActionQualifier::PulseFalling);
        let pulse_falling_active = inputs
            .iter()
            .any(|input| input.active && input.qualifier == SfcActionQualifier::PulseFalling);
        let previous_key = sfc_action_control_key_previous(control_key);
        let was_active = self
            .env
            .get(&previous_key)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.env.insert(
            previous_key,
            Value::Bool(pulse_active || pulse_falling_active),
        );

        let mut should_execute = non_stored || (pulse_active && !was_active);
        should_execute |= has_pulse_falling_input && !pulse_falling_active && was_active;

        if inputs
            .iter()
            .any(|input| input.active && input.qualifier == SfcActionQualifier::SetStored)
        {
            self.set_sfc_action_stored(control_key, true);
        }

        let timed_input = inputs
            .iter()
            .find(|input| input.active && input.qualifier.requires_duration());

        if let Some(input) = timed_input {
            should_execute |= match input.qualifier {
                SfcActionQualifier::TimeLimited => {
                    let elapsed = self.advance_sfc_action_elapsed(control_key);
                    elapsed <= sfc_action_duration_ms(input.duration)
                }
                SfcActionQualifier::TimeDelayed => {
                    let elapsed = self.advance_sfc_action_elapsed(control_key);
                    elapsed >= sfc_action_duration_ms(input.duration)
                }
                SfcActionQualifier::StoredDelayed | SfcActionQualifier::DelayedStored => {
                    let elapsed = self.advance_sfc_action_elapsed(control_key);
                    if elapsed >= sfc_action_duration_ms(input.duration) {
                        self.set_sfc_action_stored(control_key, true);
                    }
                    false
                }
                SfcActionQualifier::StoredLimited => {
                    if !self.sfc_action_stored(control_key) {
                        self.set_sfc_action_stored(control_key, true);
                        self.set_sfc_action_elapsed(control_key, 0);
                    }
                    false
                }
                _ => false,
            };
        } else if !self.sfc_action_stored(control_key) {
            self.set_sfc_action_elapsed(control_key, 0);
        }

        if self.sfc_action_stored(control_key) {
            should_execute = true;
        }

        if inputs
            .iter()
            .any(|input| input.qualifier == SfcActionQualifier::StoredLimited)
            && self.sfc_action_stored(control_key)
        {
            let elapsed = self.advance_sfc_action_elapsed(control_key);
            let duration = inputs
                .iter()
                .find(|input| input.active && input.qualifier == SfcActionQualifier::StoredLimited)
                .and_then(|input| input.duration);
            if elapsed <= sfc_action_duration_ms(duration) {
                should_execute = true;
            } else {
                self.set_sfc_action_stored(control_key, false);
                should_execute = false;
            }
        }

        should_execute && !reset_active
    }

    fn sfc_action_stored(&self, control_key: &str) -> bool {
        self.env
            .get(&sfc_action_control_key_stored(control_key))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn set_sfc_action_stored(&mut self, control_key: &str, value: bool) {
        self.env.insert(
            sfc_action_control_key_stored(control_key),
            Value::Bool(value),
        );
    }

    fn sfc_action_elapsed(&self, control_key: &str) -> i128 {
        self.env
            .get(&sfc_action_control_key_elapsed(control_key))
            .and_then(Value::as_i64)
            .map(i128::from)
            .unwrap_or(0)
    }

    fn set_sfc_action_elapsed(&mut self, control_key: &str, elapsed: i128) {
        self.env.insert(
            sfc_action_control_key_elapsed(control_key),
            Value::Int(elapsed as i64),
        );
    }

    fn advance_sfc_action_elapsed(&mut self, control_key: &str) -> i128 {
        let elapsed = self.sfc_action_elapsed(control_key) + self.options.cycle_time_ms.max(1);
        self.set_sfc_action_elapsed(control_key, elapsed);
        elapsed
    }

    fn execute_statement(&mut self, statement: &Statement) -> Control {
        match statement {
            Statement::Empty => Control::Continue,
            Statement::Assignment { target, value } => {
                let Some(value) = self.eval_expr(value) else {
                    return Control::Continue;
                };
                self.assign(target, value);
                Control::Continue
            }
            Statement::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    if self.eval_expr(condition).and_then(|v| v.as_bool()) == Some(true) {
                        return self.execute_block(body);
                    }
                }
                self.execute_block(else_branch)
            }
            Statement::Case {
                selector,
                cases,
                else_branch,
            } => {
                let selector = self.eval_expr(selector);
                if let Some(selector) = selector {
                    for (labels, body) in cases {
                        if labels
                            .iter()
                            .any(|label| self.case_label_matches(label, &selector))
                        {
                            return self.execute_block(body);
                        }
                    }
                }
                self.execute_block(else_branch)
            }
            Statement::For {
                control,
                from,
                to,
                by,
                body,
            } => {
                let mut value = self.eval_expr(from).and_then(|v| v.as_i64()).unwrap_or(0);
                let end = self.eval_expr(to).and_then(|v| v.as_i64()).unwrap_or(0);
                let step = by
                    .as_ref()
                    .and_then(|expr| self.eval_expr(expr))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);
                if step == 0 {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        "FOR loop BY value cannot be zero",
                        None,
                    ));
                    return Control::Continue;
                }

                let mut iterations = 0;
                while if step > 0 { value <= end } else { value >= end } {
                    self.env
                        .insert(control.canonical.clone(), Value::Int(value));
                    match self.execute_block(body) {
                        Control::Continue => {}
                        Control::Exit => break,
                        Control::Return => return Control::Return,
                        Control::Jump(label) => return Control::Jump(label),
                    }
                    value += step;
                    iterations += 1;
                    if iterations > self.options.max_loop_iterations {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Runtime,
                            "maximum FOR loop iteration count exceeded",
                            None,
                        ));
                        break;
                    }
                }
                Control::Continue
            }
            Statement::While { condition, body } => {
                let mut iterations = 0;
                while self.eval_expr(condition).and_then(|v| v.as_bool()) == Some(true) {
                    match self.execute_block(body) {
                        Control::Continue => {}
                        Control::Exit => break,
                        Control::Return => return Control::Return,
                        Control::Jump(label) => return Control::Jump(label),
                    }
                    iterations += 1;
                    if iterations > self.options.max_loop_iterations {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Runtime,
                            "maximum WHILE loop iteration count exceeded",
                            None,
                        ));
                        break;
                    }
                }
                Control::Continue
            }
            Statement::Repeat { body, until } => {
                let mut iterations = 0;
                loop {
                    match self.execute_block(body) {
                        Control::Continue => {}
                        Control::Exit => break,
                        Control::Return => return Control::Return,
                        Control::Jump(label) => return Control::Jump(label),
                    }
                    if self.eval_expr(until).and_then(|v| v.as_bool()) == Some(true) {
                        break;
                    }
                    iterations += 1;
                    if iterations > self.options.max_loop_iterations {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Runtime,
                            "maximum REPEAT loop iteration count exceeded",
                            None,
                        ));
                        break;
                    }
                }
                Control::Continue
            }
            Statement::Il { op, operand } => self.execute_il_instruction(*op, operand.as_ref()),
            Statement::IlLabel(_) => Control::Continue,
            Statement::FbCall { name, .. } => {
                if let Some(root) = name.root_name() {
                    if is_standard_void_function(&root.original) {
                        self.execute_standard_void_call(root, statement);
                    } else {
                        self.execute_fb_call(name, statement);
                    }
                } else {
                    self.execute_fb_call(name, statement);
                }
                Control::Continue
            }
            Statement::Exit => Control::Exit,
            Statement::Return => Control::Return,
            Statement::Unsupported(text) => {
                self.diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::Unsupported,
                    format!("skipping unsupported statement: {text}"),
                    None,
                ));
                Control::Continue
            }
        }
    }

    fn eval_expr(&mut self, expr: &Expr) -> Option<Value> {
        match expr {
            Expr::Literal(literal) => Some(literal_to_value(self.project, literal)),
            Expr::Variable(variable) => self.resolve(variable),
            Expr::Unary { op, expr } => {
                let value = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Neg => match value {
                        Value::Real(value) => Some(Value::Real(-value)),
                        value => {
                            let value = value.as_i64()?;
                            value.checked_neg().map(Value::Int).or_else(|| {
                                self.push_overflow("integer negation");
                                None
                            })
                        }
                    },
                    UnaryOp::Not => match value {
                        Value::Bool(value) => Some(Value::Bool(!value)),
                        value => value.as_i64().map(|value| Value::Int(!value)),
                    },
                }
            }
            Expr::Binary { op, left, right } => {
                let left = self.eval_expr(left)?;
                if let Value::Bool(value) = left {
                    match op {
                        BinaryOp::And if !value => return Some(Value::Bool(false)),
                        BinaryOp::Or if value => return Some(Value::Bool(true)),
                        _ => {
                            let right = self.eval_expr(right)?;
                            return self.eval_binary(*op, Value::Bool(value), right);
                        }
                    }
                }
                let right = self.eval_expr(right)?;
                self.eval_binary(*op, left, right)
            }
            Expr::Call { name, args } => {
                let enabled = self.function_call_enabled(args);
                if !enabled {
                    self.assign_function_eno(args, false);
                    if is_standard_function(&name.original) {
                        return Some(self.disabled_standard_function_value(name, args));
                    }
                    if let Some(function) = self
                        .project
                        .find_pou(&name.original)
                        .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))
                    {
                        if let PouKind::Function { return_type } = &function.kind {
                            return Some(self.default_value(return_type));
                        }
                    }
                    return Some(Value::Int(0));
                }

                let standard_values = self.eval_standard_function_inputs(name, args);
                if let Some(value) = eval_standard_function(&name.original, &standard_values) {
                    self.assign_function_eno(args, true);
                    Some(value)
                } else if is_standard_function(&name.original) {
                    self.assign_function_eno(args, false);
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        format!(
                            "standard function '{}' failed for supplied arguments",
                            name.original
                        ),
                        None,
                    ));
                    None
                } else {
                    self.eval_user_function(name, args).or_else(|| {
                        self.diagnostics.push(Diagnostic::warning(
                            DiagnosticCode::Unsupported,
                            format!("function '{}' is not executable yet", name.original),
                            None,
                        ));
                        Some(Value::Unit)
                    })
                }
            }
            Expr::ArrayLiteral(elements) => {
                let values = elements
                    .iter()
                    .map(|element| self.eval_expr(element))
                    .collect::<Option<Vec<_>>>()?;
                Some(Value::Array(values))
            }
            Expr::StructLiteral(fields) => {
                let mut values = BTreeMap::new();
                for field in fields {
                    let Some(name) = &field.name else {
                        continue;
                    };
                    let value = field
                        .expr
                        .as_ref()
                        .and_then(|expr| self.eval_expr(expr))
                        .unwrap_or(Value::Unit);
                    values.insert(name.canonical.clone(), value);
                }
                Some(Value::Struct(values))
            }
        }
    }

    fn eval_user_function(&mut self, name: &Identifier, args: &[ParamAssignment]) -> Option<Value> {
        let function = self
            .project
            .find_pou(&name.original)
            .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))?;
        let PouKind::Function { return_type } = &function.kind else {
            return None;
        };

        if !self.function_call_enabled(args) {
            self.assign_function_eno(args, false);
            return Some(self.default_value(return_type));
        }

        if self.call_depth >= 64 {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "maximum function call depth exceeded at '{}'",
                    name.original
                ),
                None,
            ));
            return None;
        }

        let mut positional = Vec::new();
        let mut named = BTreeMap::new();
        for arg in args {
            if arg.output || arg.name.as_ref().is_some_and(is_implicit_en) {
                continue;
            }
            let value = arg
                .expr
                .as_ref()
                .and_then(|expr| self.eval_expr(expr))
                .unwrap_or(Value::Unit);
            if let Some(name) = &arg.name {
                named.insert(name.canonical.clone(), value);
            } else {
                positional.push(value);
            }
        }

        let mut runtime = Runtime {
            project: self.project,
            program: function,
            env: BTreeMap::new(),
            types: BTreeMap::new(),
            il_accumulator: Value::Unit,
            diagnostics: Vec::new(),
            options: self.options.clone(),
            call_depth: self.call_depth + 1,
            communication: self.communication,
        };
        runtime.initialize(StartupKind::Cold);
        runtime.bind_function_inputs(&positional, &named);

        match runtime.execute_statement_list(&function.body.statements) {
            Control::Continue | Control::Return => {}
            Control::Exit => runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                "EXIT used outside of an iteration",
                None,
            )),
            Control::Jump(label) => runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("jump to unknown IL label '{label}'"),
                None,
            )),
        }

        let result = runtime.env.get(&function.name.canonical).cloned();
        self.diagnostics.extend(runtime.diagnostics);
        if result.is_some() {
            self.assign_function_eno(args, true);
        }
        result
    }

    fn eval_standard_function_inputs(
        &mut self,
        name: &Identifier,
        args: &[ParamAssignment],
    ) -> Vec<Value> {
        let mut ordered = Vec::new();
        let mut positional_index = 0;
        let mut unknown_index = usize::MAX.saturating_sub(args.len());

        for arg in args {
            if arg.output || arg.name.as_ref().is_some_and(is_implicit_en) {
                continue;
            }
            let Some(expr) = arg.expr.as_ref() else {
                continue;
            };
            let Some(value) = self.eval_expr(expr) else {
                continue;
            };
            let index = if let Some(arg_name) = &arg.name {
                standard_function_input_index(&name.original, &arg_name.original).unwrap_or_else(
                    || {
                        let index = unknown_index;
                        unknown_index = unknown_index.saturating_add(1);
                        index
                    },
                )
            } else {
                let index = positional_index;
                positional_index += 1;
                index
            };
            ordered.push((index, value));
        }

        ordered.sort_by_key(|(index, _)| *index);
        ordered.into_iter().map(|(_, value)| value).collect()
    }

    fn disabled_standard_function_value(
        &self,
        name: &Identifier,
        args: &[ParamAssignment],
    ) -> Value {
        match name.canonical.as_str() {
            "GT" | "GE" | "EQ" | "NE" | "LE" | "LT" => Value::Bool(false),
            "SQRT" | "LN" | "LOG" | "EXP" | "SIN" | "COS" | "TAN" | "EXPT" => Value::Real(0.0),
            "LEFT" | "RIGHT" | "MID" | "CONCAT" | "INSERT" | "DELETE" | "REPLACE" => {
                if self.standard_string_call_is_wide(args) {
                    Value::WString(String::new())
                } else {
                    Value::String(String::new())
                }
            }
            name if name.ends_with("_TO_STRING") => Value::String(String::new()),
            name if name.ends_with("_TO_WSTRING") => Value::WString(String::new()),
            name if name.ends_with("_TO_BOOL") => Value::Bool(false),
            "ADD_TIME" | "SUB_TIME" | "ADD_TOD_TIME" | "SUB_TOD_TIME" | "ADD_DT_TIME"
            | "SUB_DT_TIME" | "CONCAT_DATE" | "CONCAT_TOD" | "CONCAT_DT" | "CONCAT_DATE_TOD"
            | "SUB_DATE_DATE" | "SUB_TOD_TOD" | "SUB_DT_DT" | "MUL_TIME" | "DIV_TIME"
            | "MULTIME" | "DIVTIME" => Value::TimeMs(0),
            _ => Value::Int(0),
        }
    }

    fn standard_string_call_is_wide(&self, args: &[ParamAssignment]) -> bool {
        args.iter()
            .filter(|arg| !arg.output)
            .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
            .filter_map(|arg| arg.expr.as_ref())
            .any(|expr| self.expr_is_wstring_like(expr))
    }

    fn expr_is_wstring_like(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(Literal::WString(_)) => true,
            Expr::Literal(Literal::Typed { type_name, .. }) => {
                let spec = ElementaryType::parse(&type_name.original)
                    .map(DataTypeSpec::Elementary)
                    .or_else(|| {
                        self.project
                            .data_types()
                            .find(|data_type| data_type.name.canonical == type_name.canonical)
                            .map(|data_type| data_type.spec.clone())
                    });
                spec.is_some_and(|spec| {
                    matches!(
                        self.resolve_named_spec(&spec),
                        DataTypeSpec::Elementary(ElementaryType::WString)
                            | DataTypeSpec::String { wide: true, .. }
                    )
                })
            }
            Expr::Variable(variable) => self.variable_spec(variable).is_some_and(|spec| {
                matches!(
                    self.resolve_named_spec(&spec),
                    DataTypeSpec::Elementary(ElementaryType::WString)
                        | DataTypeSpec::String { wide: true, .. }
                )
            }),
            _ => false,
        }
    }

    fn function_call_enabled(&mut self, args: &[ParamAssignment]) -> bool {
        args.iter()
            .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
            .and_then(|arg| arg.expr.as_ref())
            .and_then(|expr| self.eval_expr(expr))
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
    }

    fn assign_function_eno(&mut self, args: &[ParamAssignment], value: bool) {
        for arg in args {
            if !arg.output || !arg.name.as_ref().is_some_and(is_implicit_eno) {
                continue;
            }
            if let Some(variable) = &arg.variable {
                self.assign(
                    variable,
                    Value::Bool(if arg.negated { !value } else { value }),
                );
            }
        }
    }

    fn bind_function_inputs(&mut self, positional: &[Value], named: &BTreeMap<String, Value>) {
        let mut positional_index = 0;
        let inputs = self
            .function_inputs()
            .map(|var| var.name.canonical.clone())
            .collect::<Vec<_>>();
        for input in inputs {
            if let Some(value) = named.get(&input) {
                self.env.insert(input, value.clone());
            } else if let Some(value) = positional.get(positional_index) {
                self.env.insert(input, value.clone());
                positional_index += 1;
            }
        }
    }

    fn function_inputs(&self) -> impl Iterator<Item = &VarDecl> {
        self.program
            .var_blocks
            .iter()
            .filter(|block| block.kind == VarBlockKind::Input)
            .flat_map(|block| block.vars.iter())
    }

    fn eval_binary(&mut self, op: BinaryOp, left: Value, right: Value) -> Option<Value> {
        match op {
            BinaryOp::Or => bit_bool_binary(left, right, |a, b| a | b, |a, b| a || b),
            BinaryOp::Xor => bit_bool_binary(left, right, |a, b| a ^ b, |a, b| a ^ b),
            BinaryOp::And => bit_bool_binary(left, right, |a, b| a & b, |a, b| a && b),
            BinaryOp::Equal => Some(Value::Bool(compare_values(&left, &right) == Some(0))),
            BinaryOp::NotEqual => Some(Value::Bool(compare_values(&left, &right) != Some(0))),
            BinaryOp::Less => Some(Value::Bool(compare_values(&left, &right)? < 0)),
            BinaryOp::LessEqual => Some(Value::Bool(compare_values(&left, &right)? <= 0)),
            BinaryOp::Greater => Some(Value::Bool(compare_values(&left, &right)? > 0)),
            BinaryOp::GreaterEqual => Some(Value::Bool(compare_values(&left, &right)? >= 0)),
            BinaryOp::Add => self.time_or_numeric_binary(
                left,
                right,
                "addition",
                i128::checked_add,
                i64::checked_add,
                |a, b| a + b,
            ),
            BinaryOp::Sub => self.time_or_numeric_binary(
                left,
                right,
                "subtraction",
                i128::checked_sub,
                i64::checked_sub,
                |a, b| a - b,
            ),
            BinaryOp::Mul => {
                self.numeric_binary(left, right, "multiplication", i64::checked_mul, |a, b| {
                    a * b
                })
            }
            BinaryOp::Div => {
                if right.as_f64() == Some(0.0) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        "division by zero",
                        None,
                    ));
                    None
                } else {
                    self.numeric_binary(left, right, "division", i64::checked_div, |a, b| a / b)
                }
            }
            BinaryOp::Mod => {
                let right = right.as_i64()?;
                if right == 0 {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        "modulo by zero",
                        None,
                    ));
                    None
                } else {
                    left.as_i64()?
                        .checked_rem(right)
                        .map(Value::Int)
                        .or_else(|| {
                            self.push_overflow("modulo");
                            None
                        })
                }
            }
            BinaryOp::Power => Some(Value::Real(left.as_f64()?.powf(right.as_f64()?))),
        }
    }

    fn numeric_binary(
        &mut self,
        left: Value,
        right: Value,
        label: &str,
        int_op: fn(i64, i64) -> Option<i64>,
        real_op: fn(f64, f64) -> f64,
    ) -> Option<Value> {
        if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
            Some(Value::Real(real_op(left.as_f64()?, right.as_f64()?)))
        } else {
            int_op(left.as_i64()?, right.as_i64()?)
                .map(Value::Int)
                .or_else(|| {
                    self.push_overflow(label);
                    None
                })
        }
    }

    fn time_or_numeric_binary(
        &mut self,
        left: Value,
        right: Value,
        label: &str,
        time_op: fn(i128, i128) -> Option<i128>,
        numeric_op: fn(i64, i64) -> Option<i64>,
        real_op: fn(f64, f64) -> f64,
    ) -> Option<Value> {
        match (&left, &right) {
            (Value::TimeMs(left), Value::TimeMs(right)) => {
                time_op(*left, *right).map(Value::TimeMs).or_else(|| {
                    self.push_overflow(label);
                    None
                })
            }
            _ => self.numeric_binary(left, right, label, numeric_op, real_op),
        }
    }

    fn push_overflow(&mut self, operation: &str) {
        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::Runtime,
            format!("integer overflow during {operation}"),
            None,
        ));
    }

    fn resolve(&mut self, variable: &VariableRef) -> Option<Value> {
        if let Some(direct) = &variable.direct {
            return self.env.get(direct).cloned().or(Some(Value::Int(0)));
        }

        let root = variable.root_name()?;
        if let Some(ordinal) = self.enum_ordinal_name(&root.canonical) {
            return Some(Value::Int(ordinal));
        }
        if let Some(key) = flattened_field_key(variable) {
            if let Some(value) = self.env.get(&key) {
                return Some(value.clone());
            }
        }

        let mut value = self.env.get(&root.canonical).cloned().or_else(|| {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("variable '{}' has no runtime storage", variable),
                None,
            ));
            None
        })?;
        let mut spec = self.types.get(&root.canonical).cloned()?;
        (value, spec) = self.apply_indices_to_value(
            value,
            spec,
            variable.indices.first().map(Vec::as_slice).unwrap_or(&[]),
        )?;
        for (segment_index, segment) in variable.path.iter().enumerate().skip(1) {
            spec = self.resolve_named_spec(&spec);
            let DataTypeSpec::Struct { fields } = spec else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!("'{}' is not a structure", variable),
                    None,
                ));
                return None;
            };
            let Some(field) = fields
                .iter()
                .find(|field| field.name.canonical == segment.canonical)
            else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!("structure field '{}' does not exist", segment.original),
                    None,
                ));
                return None;
            };
            let Value::Struct(fields) = value else {
                return None;
            };
            value = fields.get(&segment.canonical).cloned()?;
            spec = field.spec.clone();
            (value, spec) = self.apply_indices_to_value(
                value,
                spec,
                variable
                    .indices
                    .get(segment_index)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            )?;
        }
        Some(value)
    }

    fn assign(&mut self, target: &VariableRef, value: Value) {
        if let Some(direct) = &target.direct {
            self.env.insert(direct.clone(), value);
            return;
        }
        let Some(root) = target.root_name() else {
            return;
        };
        if let Some(key) = flattened_field_key(target) {
            if let std::collections::btree_map::Entry::Occupied(mut e) = self.env.entry(key) {
                e.insert(value);
                return;
            }
        }
        if target.path.len() == 2
            && target.indices.iter().all(Vec::is_empty)
            && self
                .env
                .contains_key(&field_key(&root.canonical, &target.path[1].canonical))
        {
            self.env
                .insert(field_key(&root.canonical, &target.path[1].canonical), value);
            return;
        }
        let Some(spec) = self.types.get(&root.canonical).cloned() else {
            return;
        };
        let Some(mut root_value) = self.env.get(&root.canonical).cloned() else {
            return;
        };
        if self.assign_into_value(&mut root_value, &spec, target, 0, value) {
            self.env.insert(root.canonical.clone(), root_value);
        }
    }

    fn apply_indices_to_value(
        &mut self,
        mut value: Value,
        spec: DataTypeSpec,
        indices: &[Expr],
    ) -> Option<(Value, DataTypeSpec)> {
        if indices.is_empty() {
            return Some((value, spec));
        }
        let mut current_spec = self.resolve_named_spec(&spec);
        let mut remaining = indices;
        while !remaining.is_empty() {
            let DataTypeSpec::Array {
                ranges,
                element_type,
            } = current_spec
            else {
                return None;
            };
            if remaining.len() < ranges.len() {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!(
                        "array access expects {} index value(s), got {}",
                        ranges.len(),
                        remaining.len()
                    ),
                    None,
                ));
                return None;
            }
            let (current_indices, rest) = remaining.split_at(ranges.len());
            let offset = self.array_offset(&ranges, current_indices)?;
            let Value::Array(elements) = value else {
                return None;
            };
            value = elements.get(offset).cloned()?;
            current_spec = self.resolve_named_spec(&element_type);
            remaining = rest;
        }
        Some((value, current_spec))
    }

    fn assign_into_value(
        &mut self,
        current: &mut Value,
        spec: &DataTypeSpec,
        target: &VariableRef,
        segment_index: usize,
        value: Value,
    ) -> bool {
        let current_spec = self.resolve_named_spec(spec);
        if let Some(indices) = target.indices.get(segment_index) {
            if !indices.is_empty() {
                return self.assign_into_indexed_value(
                    current,
                    &current_spec,
                    indices,
                    target,
                    segment_index,
                    value,
                );
            }
        }

        if segment_index + 1 >= target.path.len() {
            *current = self.constrain_value(&current_spec, value);
            return true;
        }

        let DataTypeSpec::Struct { fields } = current_spec else {
            return false;
        };
        let next = &target.path[segment_index + 1];
        let Some(field) = fields
            .iter()
            .find(|field| field.name.canonical == next.canonical)
        else {
            return false;
        };
        let Value::Struct(values) = current else {
            return false;
        };
        let Some(field_value) = values.get_mut(&next.canonical) else {
            return false;
        };
        self.assign_into_value(field_value, &field.spec, target, segment_index + 1, value)
    }

    fn assign_into_indexed_value(
        &mut self,
        current: &mut Value,
        spec: &DataTypeSpec,
        indices: &[Expr],
        target: &VariableRef,
        segment_index: usize,
        value: Value,
    ) -> bool {
        if indices.is_empty() {
            if segment_index + 1 >= target.path.len() {
                *current = self.constrain_value(spec, value);
                return true;
            }
            return self.assign_into_value(current, spec, target, segment_index + 1, value);
        }

        let current_spec = self.resolve_named_spec(spec);
        let DataTypeSpec::Array {
            ranges,
            element_type,
        } = current_spec
        else {
            return false;
        };
        if indices.len() < ranges.len() {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "array access expects {} index value(s), got {}",
                    ranges.len(),
                    indices.len()
                ),
                None,
            ));
            return false;
        }
        let (current_indices, rest) = indices.split_at(ranges.len());
        let Some(offset) = self.array_offset(&ranges, current_indices) else {
            return false;
        };
        let Value::Array(elements) = current else {
            return false;
        };
        let Some(element) = elements.get_mut(offset) else {
            return false;
        };
        self.assign_into_indexed_value(element, &element_type, rest, target, segment_index, value)
    }

    fn array_offset(&mut self, ranges: &[Subrange], indices: &[Expr]) -> Option<usize> {
        if ranges.len() != indices.len() {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "array access expects {} index value(s), got {}",
                    ranges.len(),
                    indices.len()
                ),
                None,
            ));
            return None;
        }
        let mut offset = 0_usize;
        let mut stride = 1_usize;
        for (range, expr) in ranges.iter().rev().zip(indices.iter().rev()) {
            let index = self.eval_expr(expr).and_then(|value| value.as_i64())?;
            if index < range.low || index > range.high {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!(
                        "array index {index} is outside subrange {}..{}",
                        range.low, range.high
                    ),
                    None,
                ));
                return None;
            }
            offset += ((index - range.low) as usize) * stride;
            stride *= (range.high - range.low + 1).max(0) as usize;
        }
        Some(offset)
    }

    fn constrain_value(&mut self, spec: &DataTypeSpec, value: Value) -> Value {
        match self.resolve_named_spec(spec) {
            DataTypeSpec::Elementary(elementary) => {
                if let Some((type_name, low, high)) = elementary_integer_range(&elementary) {
                    if let Some(int_value) = value.as_i64() {
                        let int_value = i128::from(int_value);
                        if int_value < low || int_value > high {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::Runtime,
                                format!(
                                    "value {int_value} is outside {type_name} range {low}..{high}"
                                ),
                                None,
                            ));
                        }
                    }
                }
                value
            }
            DataTypeSpec::Subrange { range, .. } => {
                if let Some(int_value) = value.as_i64() {
                    if int_value < range.low || int_value > range.high {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Runtime,
                            format!(
                                "value {int_value} is outside subrange {}..{}",
                                range.low, range.high
                            ),
                            None,
                        ));
                    }
                }
                value
            }
            DataTypeSpec::String {
                length: Some(length),
                ..
            } => match value {
                Value::String(text) => Value::String(truncate_chars(&text, length)),
                Value::WString(text) => Value::WString(truncate_chars(&text, length)),
                value => value,
            },
            DataTypeSpec::String { .. } => value,
            DataTypeSpec::Array { element_type, .. } => {
                if let Value::Array(values) = value {
                    Value::Array(
                        values
                            .into_iter()
                            .map(|value| self.constrain_value(&element_type, value))
                            .collect(),
                    )
                } else {
                    value
                }
            }
            DataTypeSpec::Struct { fields } => {
                if let Value::Struct(mut values) = value {
                    for field in fields {
                        if let Some(field_value) = values.remove(&field.name.canonical) {
                            values.insert(
                                field.name.canonical.clone(),
                                self.constrain_value(&field.spec, field_value),
                            );
                        }
                    }
                    Value::Struct(values)
                } else {
                    value
                }
            }
            _ => value,
        }
    }

    fn enum_ordinal_expr(&self, expr: &Expr) -> Option<i64> {
        if let Expr::Literal(Literal::Typed { type_name, value }) = expr {
            return self.enum_ordinal_typed(type_name, value);
        }

        let Expr::Variable(variable) = expr else {
            return None;
        };
        if variable.direct.is_some()
            || variable.path.len() != 1
            || variable.indices.iter().any(|indices| !indices.is_empty())
        {
            return None;
        }
        self.enum_ordinal_name(&variable.root_name()?.canonical)
    }

    fn enum_ordinal_typed(&self, type_name: &Identifier, value_name: &str) -> Option<i64> {
        self.project.data_types().find_map(|data_type| {
            if data_type.name.canonical != type_name.canonical {
                return None;
            }
            let DataTypeSpec::Enum { values } = &data_type.spec else {
                return None;
            };
            let value_name = canonical_identifier(value_name);
            values
                .iter()
                .position(|value| value.canonical == value_name)
                .map(|index| index as i64)
        })
    }

    fn enum_ordinal_name(&self, canonical_name: &str) -> Option<i64> {
        for data_type in self.project.data_types() {
            if let DataTypeSpec::Enum { values } = &data_type.spec {
                if let Some(index) = values
                    .iter()
                    .position(|value| value.canonical == canonical_name)
                {
                    return Some(index as i64);
                }
            }
        }
        None
    }

    fn execute_il_instruction(&mut self, op: IlOp, operand: Option<&Expr>) -> Control {
        match op {
            IlOp::Ld | IlOp::Ldn => {
                let mut value = operand
                    .and_then(|expr| self.eval_expr(expr))
                    .unwrap_or(Value::Unit);
                if matches!(op, IlOp::Ldn) {
                    value = Value::Bool(!value.as_bool().unwrap_or(false));
                }
                self.il_accumulator = value;
            }
            IlOp::St | IlOp::Stn => {
                if let Some(Expr::Variable(target)) = operand {
                    let value = if matches!(op, IlOp::Stn) {
                        Value::Bool(!self.il_accumulator.as_bool().unwrap_or(false))
                    } else {
                        self.il_accumulator.clone()
                    };
                    self.assign(target, value);
                }
            }
            IlOp::S | IlOp::R => {
                if self.il_accumulator.as_bool().unwrap_or(false) {
                    if let Some(Expr::Variable(target)) = operand {
                        self.assign(target, Value::Bool(matches!(op, IlOp::S)));
                    }
                }
            }
            IlOp::Not => {
                self.il_accumulator = Value::Bool(!self.il_accumulator.as_bool().unwrap_or(false));
            }
            IlOp::And | IlOp::Andn | IlOp::Or | IlOp::Orn | IlOp::Xor | IlOp::Xorn => {
                let mut right = operand
                    .and_then(|expr| self.eval_expr(expr))
                    .unwrap_or(Value::Bool(false));
                if matches!(op, IlOp::Andn | IlOp::Orn | IlOp::Xorn) {
                    right = Value::Bool(!right.as_bool().unwrap_or(false));
                }
                let binary = match op {
                    IlOp::And | IlOp::Andn => BinaryOp::And,
                    IlOp::Or | IlOp::Orn => BinaryOp::Or,
                    IlOp::Xor | IlOp::Xorn => BinaryOp::Xor,
                    _ => unreachable!(),
                };
                if let Some(value) = self.eval_binary(binary, self.il_accumulator.clone(), right) {
                    self.il_accumulator = value;
                }
            }
            IlOp::Add
            | IlOp::Sub
            | IlOp::Mul
            | IlOp::Div
            | IlOp::Mod
            | IlOp::Gt
            | IlOp::Ge
            | IlOp::Eq
            | IlOp::Ne
            | IlOp::Le
            | IlOp::Lt => {
                let right = operand
                    .and_then(|expr| self.eval_expr(expr))
                    .unwrap_or(Value::Int(0));
                let binary = match op {
                    IlOp::Add => BinaryOp::Add,
                    IlOp::Sub => BinaryOp::Sub,
                    IlOp::Mul => BinaryOp::Mul,
                    IlOp::Div => BinaryOp::Div,
                    IlOp::Mod => BinaryOp::Mod,
                    IlOp::Gt => BinaryOp::Greater,
                    IlOp::Ge => BinaryOp::GreaterEqual,
                    IlOp::Eq => BinaryOp::Equal,
                    IlOp::Ne => BinaryOp::NotEqual,
                    IlOp::Le => BinaryOp::LessEqual,
                    IlOp::Lt => BinaryOp::Less,
                    _ => unreachable!(),
                };
                if let Some(value) = self.eval_binary(binary, self.il_accumulator.clone(), right) {
                    self.il_accumulator = value;
                }
            }
            IlOp::Jmp | IlOp::Jmpc | IlOp::Jmpcn => {
                let should_jump = match op {
                    IlOp::Jmp => true,
                    IlOp::Jmpc => self.il_accumulator.as_bool().unwrap_or(false),
                    IlOp::Jmpcn => !self.il_accumulator.as_bool().unwrap_or(false),
                    _ => false,
                };
                if should_jump {
                    if let Some(label) = operand.and_then(il_label_operand) {
                        return Control::Jump(label.canonical.clone());
                    }
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        "IL jump instruction requires a label operand",
                        None,
                    ));
                }
            }
            IlOp::Cal | IlOp::Calc | IlOp::Calcn => {
                let should_call = match op {
                    IlOp::Cal => true,
                    IlOp::Calc => self.il_accumulator.as_bool().unwrap_or(false),
                    IlOp::Calcn => !self.il_accumulator.as_bool().unwrap_or(false),
                    _ => false,
                };
                if should_call {
                    self.execute_il_call(operand);
                }
            }
            IlOp::Ret => return Control::Return,
            IlOp::Retc => {
                if self.il_accumulator.as_bool().unwrap_or(false) {
                    return Control::Return;
                }
            }
            IlOp::Retcn => {
                if !self.il_accumulator.as_bool().unwrap_or(false) {
                    return Control::Return;
                }
            }
        }

        Control::Continue
    }

    fn execute_il_call(&mut self, operand: Option<&Expr>) {
        match operand {
            Some(Expr::Call { name, args }) => {
                let variable = VariableRef::named(name.original.clone());
                let statement = Statement::FbCall {
                    name: variable.clone(),
                    args: args.clone(),
                };
                self.execute_fb_call(&variable, &statement);
            }
            Some(Expr::Variable(variable)) => {
                let statement = Statement::FbCall {
                    name: variable.clone(),
                    args: Vec::new(),
                };
                self.execute_fb_call(variable, &statement);
            }
            _ => self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                "IL CAL instruction requires a function block instance operand",
                None,
            )),
        }
    }

    fn execute_standard_void_call(&mut self, name: &Identifier, statement: &Statement) {
        let Statement::FbCall { args, .. } = statement else {
            return;
        };
        if !self.function_call_enabled(args) {
            self.assign_function_eno(args, false);
            return;
        }
        let Some(input) = split_input_expr(args).and_then(|expr| self.eval_expr(expr)) else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("standard function '{}' requires an IN value", name.original),
                None,
            ));
            self.assign_function_eno(args, false);
            return;
        };

        let fields = match name.canonical.as_str() {
            "SPLIT_DATE" => {
                let (year, month, day) = civil_from_days(input_time_value(&input));
                vec![("YEAR", year), ("MONTH", month), ("DATE", day)]
            }
            "SPLIT_TOD" => {
                let (hour, minute, second, millisecond) = tod_parts(input_time_value(&input));
                vec![
                    ("HOUR", hour),
                    ("MINUTE", minute),
                    ("SECOND", second),
                    ("MILLISECOND", millisecond),
                ]
            }
            "SPLIT_DT" => {
                let value = input_time_value(&input);
                let days = value.div_euclid(86_400_000);
                let tod = value.rem_euclid(86_400_000);
                let (year, month, day) = civil_from_days(days);
                let (hour, minute, second, millisecond) = tod_parts(tod);
                vec![
                    ("YEAR", year),
                    ("MONTH", month),
                    ("DATE", day),
                    ("HOUR", hour),
                    ("MINUTE", minute),
                    ("SECOND", second),
                    ("MILLISECOND", millisecond),
                ]
            }
            _ => return,
        };

        for (index, (field, value)) in fields.into_iter().enumerate() {
            if let Some(variable) = split_output_variable(args, field, index) {
                self.assign(variable, Value::Int(value));
            }
        }
        self.assign_function_eno(args, true);
    }

    fn execute_fb_call(&mut self, name: &VariableRef, statement: &Statement) {
        let Statement::FbCall { args, .. } = statement else {
            return;
        };
        let Some(root) = name.root_name() else {
            return;
        };
        let Some(DataTypeSpec::Named(type_name)) = self.types.get(&root.canonical).cloned() else {
            self.diagnostics.push(Diagnostic::warning(
                DiagnosticCode::Unsupported,
                format!(
                    "function block instance '{}' has no executable type",
                    root.original
                ),
                None,
            ));
            return;
        };

        if !self.function_call_enabled(args) {
            self.assign_function_eno(args, false);
            return;
        }

        let inputs = self.eval_fb_inputs(&type_name.original, args);
        let mut executed = true;
        match type_name.canonical.as_str() {
            "SR" => {
                let q = self.get_field_bool(&root.canonical, "Q1");
                let s1 = input_bool(&inputs, "S1");
                let r = input_bool(&inputs, "R");
                self.set_field(&root.canonical, "Q1", Value::Bool(s1 || (q && !r)));
            }
            "RS" => {
                let q = self.get_field_bool(&root.canonical, "Q1");
                let s = input_bool(&inputs, "S");
                let r1 = input_bool(&inputs, "R1");
                self.set_field(&root.canonical, "Q1", Value::Bool((q || s) && !r1));
            }
            "R_TRIG" => {
                let clk = input_bool(&inputs, "CLK");
                let old = self.get_field_bool(&root.canonical, "M");
                self.set_field(&root.canonical, "Q", Value::Bool(clk && !old));
                self.set_field(&root.canonical, "M", Value::Bool(clk));
            }
            "F_TRIG" => {
                let clk = input_bool(&inputs, "CLK");
                let old = self.get_field_bool(&root.canonical, "M");
                self.set_field(&root.canonical, "Q", Value::Bool(!clk && old));
                self.set_field(&root.canonical, "M", Value::Bool(clk));
            }
            "CTU" => self.execute_ctu(&root.canonical, &inputs),
            "CTD" => self.execute_ctd(&root.canonical, &inputs),
            "CTUD" => self.execute_ctud(&root.canonical, &inputs),
            "TON" => self.execute_ton(&root.canonical, &inputs),
            "TOF" => self.execute_tof(&root.canonical, &inputs),
            "TP" => self.execute_tp(&root.canonical, &inputs),
            _ => {
                if is_communication_function_block(&type_name.original) {
                    executed = self.execute_communication_function_block(
                        &root.canonical,
                        &type_name,
                        &inputs,
                    );
                } else if let Some(function_block) = self
                    .project
                    .find_pou(&type_name.original)
                    .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
                {
                    self.execute_user_function_block(&root.canonical, function_block, args);
                } else {
                    self.diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::Unsupported,
                        format!(
                            "function block type '{}' is not executable yet",
                            type_name.original
                        ),
                        None,
                    ));
                    executed = false;
                }
            }
        }
        if executed && is_standard_function_block_type(&type_name.original) {
            self.assign_standard_function_block_outputs(&root.canonical, &type_name.original, args);
        }
        self.assign_function_eno(args, executed);
    }

    fn assign_standard_function_block_outputs(
        &mut self,
        instance: &str,
        block_type: &str,
        args: &[ParamAssignment],
    ) {
        for arg in args {
            if !arg.output {
                continue;
            }
            let (Some(name), Some(variable)) = (&arg.name, &arg.variable) else {
                continue;
            };
            if is_implicit_eno(name)
                || !standard_function_block_output_names(block_type)
                    .iter()
                    .any(|field| canonical_identifier(field) == name.canonical)
            {
                continue;
            }
            let mut value = self
                .env
                .get(&field_key(instance, &name.canonical))
                .cloned()
                .unwrap_or(Value::Unit);
            if arg.negated {
                value = Value::Bool(!value.as_bool().unwrap_or(false));
            }
            self.assign(variable, value);
        }
    }

    fn execute_communication_function_block(
        &mut self,
        instance: &str,
        type_name: &Identifier,
        inputs: &BTreeMap<String, Value>,
    ) -> bool {
        let invocation = CommunicationInvocation {
            block: type_name.original.clone(),
            instance: instance.to_string(),
            inputs: inputs.clone(),
        };
        if let Some(outcome) = self.communication.execute(&invocation) {
            for (field, value) in outcome.outputs {
                self.set_field(instance, &canonical_identifier(&field), value);
            }
            return true;
        }

        self.set_field(instance, "DONE", Value::Bool(false));
        self.set_field(instance, "NDR", Value::Bool(false));
        self.set_field(instance, "ERROR", Value::Bool(true));
        self.set_field(instance, "STATUS", Value::Int(-1));
        self.diagnostics.push(Diagnostic::warning(
            DiagnosticCode::Unsupported,
            format!(
                "communication function block '{}' requires a runtime communication hook",
                type_name.original
            ),
            None,
        ));
        false
    }

    fn execute_user_function_block(
        &mut self,
        instance: &str,
        function_block: &Pou,
        args: &[ParamAssignment],
    ) {
        let input_fields = function_block
            .var_blocks
            .iter()
            .filter(|block| matches!(block.kind, VarBlockKind::Input | VarBlockKind::InOut))
            .flat_map(|block| {
                block
                    .vars
                    .iter()
                    .map(move |var| (block.kind, var.name.clone()))
            })
            .collect::<Vec<_>>();

        let mut positional_index = 0_usize;
        for arg in args {
            let Some((_, name)) = user_fb_input_target(&input_fields, arg, &mut positional_index)
            else {
                continue;
            };
            let value = arg
                .expr
                .as_ref()
                .and_then(|expr| self.eval_expr(expr))
                .unwrap_or(Value::Unit);
            let value = if let Some(edge) = user_fb_input_edge(function_block, &name.canonical) {
                self.edge_qualified_input_value(instance, &name.canonical, edge, value)
            } else {
                value
            };
            self.set_field(instance, &name.canonical, value);
        }

        let mut runtime = Runtime {
            project: self.project,
            program: function_block,
            env: BTreeMap::new(),
            types: BTreeMap::new(),
            il_accumulator: Value::Unit,
            diagnostics: Vec::new(),
            options: self.options.clone(),
            call_depth: self.call_depth + 1,
            communication: self.communication,
        };
        for field in function_block.variable_declarations() {
            runtime
                .types
                .insert(field.name.canonical.clone(), field.type_spec.clone());
            if function_block_field_specs(self.project, &field.type_spec).is_some() {
                self.copy_function_block_state_into_runtime(
                    &field_key(instance, &field.name.canonical),
                    &field.name.canonical,
                    &field.type_spec,
                    &mut runtime,
                );
            } else {
                let value = self
                    .env
                    .get(&field_key(instance, &field.name.canonical))
                    .cloned()
                    .unwrap_or_else(|| runtime.default_value(&field.type_spec));
                runtime.env.insert(field.name.canonical.clone(), value);
            }
        }

        match runtime.execute_statement_list(&function_block.body.statements) {
            Control::Continue | Control::Return => {}
            Control::Exit => runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                "EXIT used outside of an iteration",
                None,
            )),
            Control::Jump(label) => runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("jump to unknown IL label '{label}'"),
                None,
            )),
        }

        for field in function_block.variable_declarations() {
            if function_block_field_specs(self.project, &field.type_spec).is_some() {
                self.copy_function_block_state_from_runtime(
                    &runtime,
                    &field.name.canonical,
                    &field_key(instance, &field.name.canonical),
                    &field.type_spec,
                );
            } else if let Some(value) = runtime.env.get(&field.name.canonical) {
                self.set_field(instance, &field.name.canonical, value.clone());
            }
        }

        self.diagnostics.extend(runtime.diagnostics);
        let mut positional_index = 0_usize;
        for arg in args {
            let Some((kind, name)) =
                user_fb_input_target(&input_fields, arg, &mut positional_index)
            else {
                continue;
            };
            if kind != VarBlockKind::InOut {
                continue;
            }
            let Some(Expr::Variable(variable)) = &arg.expr else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!(
                        "VAR_IN_OUT parameter '{}' requires a variable actual",
                        name.original
                    ),
                    None,
                ));
                continue;
            };
            let value = self
                .env
                .get(&field_key(instance, &name.canonical))
                .cloned()
                .unwrap_or(Value::Unit);
            self.assign(variable, value);
        }
        for arg in args {
            if !arg.output {
                continue;
            }
            let (Some(name), Some(variable)) = (&arg.name, &arg.variable) else {
                continue;
            };
            if is_implicit_eno(name) {
                continue;
            }
            let value = self
                .env
                .get(&field_key(instance, &name.canonical))
                .cloned()
                .unwrap_or(Value::Unit);
            self.assign(variable, value);
        }
    }

    fn copy_function_block_state_into_runtime(
        &self,
        parent_prefix: &str,
        child_prefix: &str,
        spec: &DataTypeSpec,
        runtime: &mut Runtime<'_>,
    ) {
        let Some(fields) = function_block_field_specs(self.project, spec) else {
            return;
        };
        for field in fields {
            let parent_key = field_key(parent_prefix, &field.name);
            let child_key = field_key(child_prefix, &field.name);
            if function_block_field_specs(self.project, &field.spec).is_some() {
                self.copy_function_block_state_into_runtime(
                    &parent_key,
                    &child_key,
                    &field.spec,
                    runtime,
                );
            } else {
                let value = self
                    .env
                    .get(&parent_key)
                    .cloned()
                    .unwrap_or_else(|| runtime.default_value(&field.spec));
                runtime.env.insert(child_key, value);
            }
        }
    }

    fn copy_function_block_state_from_runtime(
        &mut self,
        runtime: &Runtime<'_>,
        child_prefix: &str,
        parent_prefix: &str,
        spec: &DataTypeSpec,
    ) {
        let Some(fields) = function_block_field_specs(self.project, spec) else {
            return;
        };
        for field in fields {
            let child_key = field_key(child_prefix, &field.name);
            let parent_key = field_key(parent_prefix, &field.name);
            if function_block_field_specs(self.project, &field.spec).is_some() {
                self.copy_function_block_state_from_runtime(
                    runtime,
                    &child_key,
                    &parent_key,
                    &field.spec,
                );
            } else if let Some(value) = runtime.env.get(&child_key) {
                self.env.insert(parent_key, value.clone());
            }
        }
    }

    fn edge_qualified_input_value(
        &mut self,
        instance: &str,
        input_name: &str,
        edge: EdgeQualifier,
        value: Value,
    ) -> Value {
        let current = value.as_bool().unwrap_or(false);
        let edge_field = edge_state_field_name(input_name);
        let previous = self.get_field_bool(instance, &edge_field);
        self.set_field(instance, &edge_field, Value::Bool(current));
        match edge {
            EdgeQualifier::Rising => Value::Bool(current && !previous),
            EdgeQualifier::Falling => Value::Bool(!current && previous),
        }
    }

    fn execute_ctu(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        if input_bool(inputs, "R") {
            self.set_field(instance, "CV", Value::Int(0));
        } else {
            let cu = input_bool(inputs, "CU");
            let old_cu = self.get_field_bool(instance, "_CU");
            if cu && !old_cu {
                let cv = self.get_field_i64(instance, "CV") + 1;
                self.set_field(instance, "CV", Value::Int(cv));
            }
            self.set_field(instance, "_CU", Value::Bool(cu));
        }
        let cv = self.get_field_i64(instance, "CV");
        let pv = input_i64(inputs, "PV");
        self.set_field(instance, "Q", Value::Bool(cv >= pv));
    }

    fn execute_ctd(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        if input_bool(inputs, "LD") {
            self.set_field(instance, "CV", Value::Int(input_i64(inputs, "PV")));
        } else {
            let cd = input_bool(inputs, "CD");
            let old_cd = self.get_field_bool(instance, "_CD");
            if cd && !old_cd {
                let cv = self.get_field_i64(instance, "CV") - 1;
                self.set_field(instance, "CV", Value::Int(cv));
            }
            self.set_field(instance, "_CD", Value::Bool(cd));
        }
        self.set_field(
            instance,
            "Q",
            Value::Bool(self.get_field_i64(instance, "CV") <= 0),
        );
    }

    fn execute_ctud(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        if input_bool(inputs, "R") {
            self.set_field(instance, "CV", Value::Int(0));
        } else if input_bool(inputs, "LD") {
            self.set_field(instance, "CV", Value::Int(input_i64(inputs, "PV")));
        } else {
            let cu = input_bool(inputs, "CU");
            let cd = input_bool(inputs, "CD");
            let old_cu = self.get_field_bool(instance, "_CU");
            let old_cd = self.get_field_bool(instance, "_CD");
            let mut cv = self.get_field_i64(instance, "CV");
            let cu_rising = cu && !old_cu;
            let cd_rising = cd && !old_cd;
            if cu_rising && !cd_rising {
                cv += 1;
            } else if cd_rising && !cu_rising {
                cv -= 1;
            }
            self.set_field(instance, "CV", Value::Int(cv));
            self.set_field(instance, "_CU", Value::Bool(cu));
            self.set_field(instance, "_CD", Value::Bool(cd));
        }
        let cv = self.get_field_i64(instance, "CV");
        let pv = input_i64(inputs, "PV");
        self.set_field(instance, "QU", Value::Bool(cv >= pv));
        self.set_field(instance, "QD", Value::Bool(cv <= 0));
    }

    fn execute_ton(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        let input = input_bool(inputs, "IN");
        let preset = input_time_ms(inputs, "PT").max(0);
        if !input {
            self.set_field(instance, "Q", Value::Bool(false));
            self.set_field(instance, "ET", Value::TimeMs(0));
        } else {
            let elapsed =
                (self.get_field_time_ms(instance, "ET") + self.options.cycle_time_ms).min(preset);
            self.set_field(instance, "ET", Value::TimeMs(elapsed));
            self.set_field(instance, "Q", Value::Bool(elapsed >= preset));
        }
        self.set_field(instance, "_IN", Value::Bool(input));
    }

    fn execute_tof(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        let input = input_bool(inputs, "IN");
        let preset = input_time_ms(inputs, "PT").max(0);
        if input {
            self.set_field(instance, "Q", Value::Bool(true));
            self.set_field(instance, "ET", Value::TimeMs(0));
        } else if self.get_field_bool(instance, "Q") {
            let elapsed =
                (self.get_field_time_ms(instance, "ET") + self.options.cycle_time_ms).min(preset);
            self.set_field(instance, "ET", Value::TimeMs(elapsed));
            if elapsed >= preset {
                self.set_field(instance, "Q", Value::Bool(false));
            }
        }
        self.set_field(instance, "_IN", Value::Bool(input));
    }

    fn execute_tp(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        let input = input_bool(inputs, "IN");
        let preset = input_time_ms(inputs, "PT").max(0);
        let old_input = self.get_field_bool(instance, "_IN");
        let mut running = self.get_field_bool(instance, "_RUN");

        if input && !old_input && !running {
            running = true;
            self.set_field(instance, "ET", Value::TimeMs(0));
            self.set_field(instance, "Q", Value::Bool(true));
        }

        if running {
            let elapsed =
                (self.get_field_time_ms(instance, "ET") + self.options.cycle_time_ms).min(preset);
            self.set_field(instance, "ET", Value::TimeMs(elapsed));
            if elapsed >= preset {
                running = false;
                self.set_field(instance, "Q", Value::Bool(false));
            } else {
                self.set_field(instance, "Q", Value::Bool(true));
            }
        } else {
            self.set_field(instance, "Q", Value::Bool(false));
        }

        self.set_field(instance, "_IN", Value::Bool(input));
        self.set_field(instance, "_RUN", Value::Bool(running));
    }

    fn eval_fb_inputs(
        &mut self,
        type_name: &str,
        args: &[ParamAssignment],
    ) -> BTreeMap<String, Value> {
        let mut inputs = BTreeMap::new();
        let input_names = standard_function_block_input_names(type_name);
        let mut positional_index = 0_usize;
        for arg in args {
            if arg.output || arg.name.as_ref().is_some_and(is_implicit_en) {
                continue;
            }
            let input_name = if let Some(name) = &arg.name {
                Some(name.canonical.clone())
            } else {
                let name = input_names
                    .get(positional_index)
                    .map(|name| (*name).to_string());
                positional_index += 1;
                name
            };
            let Some(input_name) = input_name else {
                continue;
            };
            let value = arg
                .expr
                .as_ref()
                .and_then(|expr| self.eval_expr(expr))
                .unwrap_or(Value::Unit);
            inputs.insert(canonical_identifier(&input_name), value);
        }
        inputs
    }

    fn get_field_bool(&self, instance: &str, field: &str) -> bool {
        self.env
            .get(&field_key(instance, field))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn get_field_i64(&self, instance: &str, field: &str) -> i64 {
        self.env
            .get(&field_key(instance, field))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    }

    fn get_field_time_ms(&self, instance: &str, field: &str) -> i128 {
        match self.env.get(&field_key(instance, field)) {
            Some(Value::TimeMs(value)) => *value,
            Some(value) => value.as_i64().unwrap_or(0) as i128,
            None => 0,
        }
    }

    fn set_field(&mut self, instance: &str, field: &str, value: Value) {
        self.env.insert(field_key(instance, field), value);
    }

    fn case_label_matches(&mut self, label: &CaseLabel, selector: &Value) -> bool {
        match label {
            CaseLabel::Single(expr) => self
                .eval_expr(expr)
                .is_some_and(|value| compare_values(&value, selector) == Some(0)),
            CaseLabel::Range(low, high) => {
                let low = self.eval_expr(low).and_then(|value| value.as_i64());
                let high = self.eval_expr(high).and_then(|value| value.as_i64());
                let selector = selector.as_i64();
                match (low, high, selector) {
                    (Some(low), Some(high), Some(selector)) => selector >= low && selector <= high,
                    _ => false,
                }
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Control {
    Continue,
    Exit,
    Return,
    Jump(String),
}

fn truncate_chars(text: &str, limit: usize) -> String {
    if text.chars().count() <= limit {
        text.to_string()
    } else {
        text.chars().take(limit).collect()
    }
}

fn literal_to_value(project: &Project, literal: &Literal) -> Value {
    match literal {
        Literal::Int(value) => Value::Int(*value),
        Literal::Real(value) => Value::Real(*value),
        Literal::Bool(value) => Value::Bool(*value),
        Literal::String(value) => Value::String(value.clone()),
        Literal::WString(value) => Value::WString(value.clone()),
        Literal::DurationMs(value) => Value::TimeMs(*value),
        Literal::Date(value) => Value::TimeMs(parse_date_days(value).unwrap_or(0) as i128),
        Literal::TimeOfDay(value) => Value::TimeMs(parse_time_of_day_ms(value).unwrap_or(0)),
        Literal::DateAndTime(value) => Value::TimeMs(parse_date_time_ms(value).unwrap_or(0)),
        Literal::Typed { type_name, value } => typed_literal_value(project, type_name, value)
            .unwrap_or_else(|| Value::String(value.clone())),
    }
}

fn typed_literal_value(project: &Project, type_name: &Identifier, value: &str) -> Option<Value> {
    if let Some(elementary) = ElementaryType::parse(&type_name.original) {
        return typed_literal_elementary_value(elementary, value);
    }
    let spec = project
        .data_types()
        .find(|data_type| data_type.name.canonical == type_name.canonical)
        .map(|data_type| data_type.spec.clone())?;
    typed_literal_spec_value(project, &spec, value, &mut BTreeSet::new())
}

fn typed_literal_spec_value(
    project: &Project,
    spec: &DataTypeSpec,
    value: &str,
    seen: &mut BTreeSet<String>,
) -> Option<Value> {
    match resolve_project_spec(project, spec) {
        DataTypeSpec::Elementary(elementary) => typed_literal_elementary_value(elementary, value),
        DataTypeSpec::Subrange { .. } => typed_literal_i128(value)
            .and_then(|value| i64::try_from(value).ok())
            .map(Value::Int),
        DataTypeSpec::Enum { values } => {
            let value = canonical_identifier(value);
            values
                .iter()
                .position(|candidate| candidate.canonical == value)
                .map(|index| Value::Int(index as i64))
        }
        DataTypeSpec::String { wide, .. } => {
            if wide {
                Some(Value::WString(value.to_string()))
            } else {
                Some(Value::String(value.to_string()))
            }
        }
        DataTypeSpec::Named(name) => {
            if !seen.insert(name.canonical.clone()) {
                return None;
            }
            let data_type = project
                .data_types()
                .find(|data_type| data_type.name.canonical == name.canonical)?;
            typed_literal_spec_value(project, &data_type.spec, value, seen)
        }
        DataTypeSpec::Array { .. } | DataTypeSpec::Struct { .. } => None,
    }
}

fn typed_literal_elementary_value(elementary: ElementaryType, value: &str) -> Option<Value> {
    match elementary {
        ElementaryType::Bool => parse_typed_bool(value).map(Value::Bool),
        ElementaryType::Sint
        | ElementaryType::Int
        | ElementaryType::Dint
        | ElementaryType::Lint
        | ElementaryType::Usint
        | ElementaryType::Uint
        | ElementaryType::Udint
        | ElementaryType::Ulint
        | ElementaryType::Byte
        | ElementaryType::Word
        | ElementaryType::Dword
        | ElementaryType::Lword => typed_literal_i128(value)
            .and_then(|value| i64::try_from(value).ok())
            .map(Value::Int),
        ElementaryType::Real | ElementaryType::Lreal => parse_typed_real(value).map(Value::Real),
        ElementaryType::Time => typed_literal_i128(value)
            .or_else(|| parse_duration_ms_checked(value))
            .map(Value::TimeMs),
        ElementaryType::Date => parse_date_days(value).map(|value| Value::TimeMs(value as i128)),
        ElementaryType::TimeOfDay => parse_time_of_day_ms(value).map(Value::TimeMs),
        ElementaryType::DateAndTime => parse_date_time_ms(value).map(Value::TimeMs),
        ElementaryType::String => Some(Value::String(value.to_string())),
        ElementaryType::WString => Some(Value::WString(value.to_string())),
    }
}

fn parse_typed_bool(value: &str) -> Option<bool> {
    match canonical_identifier(value).as_str() {
        "TRUE" | "1" => Some(true),
        "FALSE" | "0" => Some(false),
        _ => None,
    }
}

fn parse_typed_real(value: &str) -> Option<f64> {
    let value = value.trim().replace('_', "");
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

fn typed_literal_i128(value: &str) -> Option<i128> {
    if let Some((base, digits)) = value.split_once('#') {
        let base = match canonical_identifier(base).as_str() {
            "2" => 2,
            "8" => 8,
            "16" => 16,
            _ => return None,
        };
        if !valid_integer_underscore_placement(digits) {
            return None;
        }
        return i128::from_str_radix(&digits.replace('_', ""), base).ok();
    }
    let value = value.trim();
    if !valid_integer_underscore_placement(value) {
        return None;
    }
    value.replace('_', "").parse::<i128>().ok()
}

fn valid_integer_underscore_placement(raw: &str) -> bool {
    let raw = raw
        .strip_prefix('-')
        .or_else(|| raw.strip_prefix('+'))
        .unwrap_or(raw);
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.first() == Some(&b'_') || bytes.last() == Some(&b'_') {
        return false;
    }
    !bytes.windows(2).any(|pair| pair == b"__")
}

fn parse_duration_ms_checked(raw: &str) -> Option<i128> {
    let mut chars = raw.replace('_', "").to_ascii_lowercase();
    let sign = if chars.starts_with('-') {
        chars.remove(0);
        -1_i128
    } else {
        1_i128
    };
    let mut rest = chars.as_str();
    if rest.is_empty() {
        return None;
    }

    let mut total = 0.0_f64;
    let mut previous_rank = 6_u8;
    let mut saw_component = false;
    while !rest.is_empty() {
        let number_len = rest
            .chars()
            .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
            .map(char::len_utf8)
            .sum::<usize>();
        if number_len == 0 {
            return None;
        }
        let number_text = &rest[..number_len];
        if !valid_decimal_component(number_text) {
            return None;
        }
        let number = number_text.parse::<f64>().ok()?;
        rest = &rest[number_len..];
        let (factor, consumed, rank, unit) = if rest.starts_with("ms") {
            (1.0, 2, 1, "ms")
        } else if rest.starts_with('d') {
            (86_400_000.0, 1, 5, "d")
        } else if rest.starts_with('h') {
            (3_600_000.0, 1, 4, "h")
        } else if rest.starts_with('m') {
            (60_000.0, 1, 3, "m")
        } else if rest.starts_with('s') {
            (1_000.0, 1, 2, "s")
        } else {
            return None;
        };
        if rank >= previous_rank {
            return None;
        }
        let has_more = rest.get(consumed..).is_some_and(|tail| !tail.is_empty());
        if has_more && number_text.contains('.') {
            return None;
        }
        if has_more || previous_rank != 6 {
            match unit {
                "h" if number >= 24.0 => return None,
                "m" | "s" if number >= 60.0 => return None,
                "ms" if number >= 1000.0 => return None,
                _ => {}
            }
        }
        saw_component = true;
        previous_rank = rank;
        total += number * factor;
        rest = &rest[consumed..];
    }
    saw_component.then_some(sign * total.round() as i128)
}

fn valid_decimal_component(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.first() == Some(&b'_') || bytes.last() == Some(&b'_') {
        return false;
    }
    if bytes.windows(2).any(|pair| pair == b"__") {
        return false;
    }
    let mut dot_count = 0_u8;
    let mut digit_count = 0_usize;
    for ch in raw.chars() {
        if ch == '.' {
            dot_count += 1;
            if dot_count > 1 {
                return false;
            }
        } else if ch.is_ascii_digit() || ch == '_' {
            if ch.is_ascii_digit() {
                digit_count += 1;
            }
        } else {
            return false;
        }
    }
    digit_count > 0
}

fn parse_date_time_ms(input: &str) -> Option<i128> {
    if input.len() < 11 {
        return None;
    }
    let date = parse_date_days(input.get(..10)?)? as i128;
    let separator = input.as_bytes().get(10).copied()?;
    if separator != b'-' && separator != b'T' && separator != b't' {
        return None;
    }
    Some(date * 86_400_000 + parse_time_of_day_ms(input.get(11..)?)?)
}

fn parse_date_days(input: &str) -> Option<i64> {
    let mut parts = input.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<i64>().ok()?;
    let day = parts.next()?.parse::<i64>().ok()?;
    if parts.next().is_some()
        || !(1..=12).contains(&month)
        || !(1..=days_in_month(year, month)).contains(&day)
    {
        return None;
    }
    Some(days_from_civil(year, month, day))
}

fn parse_time_of_day_ms(input: &str) -> Option<i128> {
    let mut parts = input.split(':');
    let hour = parts.next()?.parse::<i128>().ok()?;
    let minute = parts.next()?.parse::<i128>().ok()?;
    let second_part = parts.next()?;
    if parts.next().is_some() || !(0..=23).contains(&hour) || !(0..=59).contains(&minute) {
        return None;
    }
    let (second, millis) = if let Some((seconds, fraction)) = second_part.split_once('.') {
        if fraction.is_empty() || !fraction.chars().all(|ch| ch.is_ascii_digit()) {
            return None;
        }
        let mut millis_text = fraction.chars().take(3).collect::<String>();
        while millis_text.len() < 3 {
            millis_text.push('0');
        }
        (
            seconds.parse::<i128>().ok()?,
            millis_text.parse::<i128>().ok()?,
        )
    } else {
        (second_part.parse::<i128>().ok()?, 0)
    };
    if !(0..=59).contains(&second) {
        return None;
    }
    Some(((hour * 60 + minute) * 60 + second) * 1000 + millis)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let adjusted_year = year - if month <= 2 { 1 } else { 0 };
    let era = if adjusted_year >= 0 {
        adjusted_year
    } else {
        adjusted_year - 399
    } / 400;
    let year_of_era = adjusted_year - era * 400;
    let month_prime = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * month_prime + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

fn array_element_count(ranges: &[Subrange]) -> usize {
    ranges.iter().fold(1_usize, |total, range| {
        total.saturating_mul((range.high - range.low + 1).max(0) as usize)
    })
}

fn runtime_spec_label(spec: &DataTypeSpec) -> &'static str {
    match spec {
        DataTypeSpec::Elementary(elementary) => match elementary {
            ElementaryType::Bool => "BOOL",
            ElementaryType::Sint
            | ElementaryType::Int
            | ElementaryType::Dint
            | ElementaryType::Lint
            | ElementaryType::Usint
            | ElementaryType::Uint
            | ElementaryType::Udint
            | ElementaryType::Ulint => "integer",
            ElementaryType::Real | ElementaryType::Lreal => "REAL",
            ElementaryType::Byte
            | ElementaryType::Word
            | ElementaryType::Dword
            | ElementaryType::Lword => "bit-string",
            ElementaryType::String => "STRING",
            ElementaryType::WString => "WSTRING",
            ElementaryType::Time => "TIME",
            ElementaryType::Date => "DATE",
            ElementaryType::TimeOfDay => "TIME_OF_DAY",
            ElementaryType::DateAndTime => "DATE_AND_TIME",
        },
        DataTypeSpec::String { wide, .. } => {
            if *wide {
                "WSTRING"
            } else {
                "STRING"
            }
        }
        DataTypeSpec::Subrange { .. } => "subrange",
        DataTypeSpec::Enum { .. } => "enumerated",
        DataTypeSpec::Array { .. } => "array",
        DataTypeSpec::Struct { .. } => "structure",
        DataTypeSpec::Named(_) => "value",
    }
}

fn runtime_value_label(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "BOOL",
        Value::Int(_) => "integer",
        Value::Real(_) => "REAL",
        Value::String(_) => "STRING",
        Value::WString(_) => "WSTRING",
        Value::TimeMs(_) => "TIME",
        Value::Array(_) => "array",
        Value::Struct(_) => "structure",
        Value::Unit => "unit",
    }
}

fn elementary_integer_range(elementary: &ElementaryType) -> Option<(&'static str, i128, i128)> {
    match elementary {
        ElementaryType::Sint => Some(("SINT", -128, 127)),
        ElementaryType::Usint | ElementaryType::Byte => Some((
            if matches!(elementary, ElementaryType::Byte) {
                "BYTE"
            } else {
                "USINT"
            },
            0,
            255,
        )),
        ElementaryType::Int => Some(("INT", -32_768, 32_767)),
        ElementaryType::Uint | ElementaryType::Word => Some((
            if matches!(elementary, ElementaryType::Word) {
                "WORD"
            } else {
                "UINT"
            },
            0,
            65_535,
        )),
        ElementaryType::Dint => Some(("DINT", -2_147_483_648, 2_147_483_647)),
        ElementaryType::Udint | ElementaryType::Dword => Some((
            if matches!(elementary, ElementaryType::Dword) {
                "DWORD"
            } else {
                "UDINT"
            },
            0,
            4_294_967_295,
        )),
        ElementaryType::Lint => Some(("LINT", i64::MIN as i128, i64::MAX as i128)),
        ElementaryType::Ulint | ElementaryType::Lword => Some((
            if matches!(elementary, ElementaryType::Lword) {
                "LWORD"
            } else {
                "ULINT"
            },
            0,
            i64::MAX as i128,
        )),
        _ => None,
    }
}

fn il_label_operand(expr: &Expr) -> Option<&Identifier> {
    let Expr::Variable(variable) = expr else {
        return None;
    };
    if variable.direct.is_some() || variable.path.len() != 1 {
        return None;
    }
    variable.root_name()
}

fn field_key(instance: &str, field: &str) -> String {
    format!(
        "{}.{}",
        canonical_identifier(instance),
        canonical_identifier(field)
    )
}

#[derive(Debug, Clone)]
struct FunctionBlockRuntimeField {
    name: String,
    spec: DataTypeSpec,
}

fn function_block_field_specs(
    project: &Project,
    spec: &DataTypeSpec,
) -> Option<Vec<FunctionBlockRuntimeField>> {
    let DataTypeSpec::Named(type_name) = spec else {
        return None;
    };
    let fields = match type_name.canonical.as_str() {
        "SR" | "RS" => vec![("Q1", DataTypeSpec::Elementary(ElementaryType::Bool))],
        "R_TRIG" | "F_TRIG" => vec![
            ("Q", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("M", DataTypeSpec::Elementary(ElementaryType::Bool)),
        ],
        "CTU" => vec![
            ("Q", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("CV", DataTypeSpec::Elementary(ElementaryType::Int)),
            ("_CU", DataTypeSpec::Elementary(ElementaryType::Bool)),
        ],
        "CTD" => vec![
            ("Q", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("CV", DataTypeSpec::Elementary(ElementaryType::Int)),
            ("_CD", DataTypeSpec::Elementary(ElementaryType::Bool)),
        ],
        "CTUD" => vec![
            ("QU", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("QD", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("CV", DataTypeSpec::Elementary(ElementaryType::Int)),
            ("_CU", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("_CD", DataTypeSpec::Elementary(ElementaryType::Bool)),
        ],
        "TON" | "TOF" | "TP" => vec![
            ("Q", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("ET", DataTypeSpec::Elementary(ElementaryType::Time)),
            ("_IN", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("_RUN", DataTypeSpec::Elementary(ElementaryType::Bool)),
        ],
        name if is_communication_function_block(name) => vec![
            ("DONE", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("NDR", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("ERROR", DataTypeSpec::Elementary(ElementaryType::Bool)),
            ("STATUS", DataTypeSpec::Elementary(ElementaryType::Int)),
        ],
        _ => {
            let function_block = project
                .find_pou(&type_name.original)
                .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))?;
            return Some(
                function_block
                    .variable_declarations()
                    .flat_map(|field| {
                        let mut fields = vec![FunctionBlockRuntimeField {
                            name: field.name.canonical.clone(),
                            spec: field.type_spec.clone(),
                        }];
                        if field.edge.is_some() {
                            fields.push(FunctionBlockRuntimeField {
                                name: edge_state_field_name(&field.name.canonical),
                                spec: DataTypeSpec::Elementary(ElementaryType::Bool),
                            });
                        }
                        fields
                    })
                    .collect(),
            );
        }
    };
    Some(
        fields
            .into_iter()
            .map(|(name, spec)| FunctionBlockRuntimeField {
                name: name.to_string(),
                spec,
            })
            .collect(),
    )
}

fn flattened_field_key(variable: &VariableRef) -> Option<String> {
    if variable.direct.is_some()
        || variable.path.len() < 2
        || variable.indices.iter().any(|indices| !indices.is_empty())
    {
        return None;
    }
    Some(
        variable
            .path
            .iter()
            .map(|part| part.canonical.as_str())
            .collect::<Vec<_>>()
            .join("."),
    )
}

fn sfc_step_key(step: &Identifier) -> String {
    format!("$SFC_STEP_{}", step.canonical)
}

fn sfc_transition_steps<'a>(
    sfc: &'a Sfc,
    transition: &'a SfcTransition,
    index: usize,
) -> Option<(Vec<&'a Identifier>, Vec<&'a Identifier>)> {
    if !transition.from.is_empty() || !transition.to.is_empty() {
        if transition.from.is_empty() || transition.to.is_empty() {
            return None;
        }
        return Some((
            transition.from.iter().collect(),
            transition.to.iter().collect(),
        ));
    }

    let from = &sfc.steps.get(index)?.name;
    let to = &sfc.steps.get(index + 1)?.name;
    Some((vec![from], vec![to]))
}

struct SfcActionInput<'a> {
    qualifier: SfcActionQualifier,
    duration: Option<&'a Literal>,
    active: bool,
}

fn sfc_action_inputs<'a>(
    sfc: &'a Sfc,
    action: &'a SfcAction,
    active_steps: &[String],
) -> Vec<SfcActionInput<'a>> {
    let mut inputs = Vec::new();
    for step in &sfc.steps {
        let active = active_steps.contains(&step.name.canonical);
        for association in &step.actions {
            if association.name.canonical != action.name.canonical {
                continue;
            }
            inputs.push(SfcActionInput {
                qualifier: association.qualifier.unwrap_or(action.qualifier),
                duration: association.duration.as_ref().or(action.duration.as_ref()),
                active,
            });
        }
    }

    if inputs.is_empty() {
        let active = active_steps.contains(&action.name.canonical);
        inputs.push(SfcActionInput {
            qualifier: action.qualifier,
            duration: action.duration.as_ref(),
            active,
        });
    }

    inputs
}

fn sfc_action_control_key(action: &Identifier) -> String {
    action.canonical.clone()
}

fn sfc_action_control_key_stored(key: &str) -> String {
    format!("$SFC_ACTION_{key}")
}

fn sfc_action_control_key_previous(key: &str) -> String {
    format!("$SFC_ACTION_PREVIOUS_{key}")
}

fn sfc_action_control_key_elapsed(key: &str) -> String {
    format!("$SFC_ACTION_ELAPSED_{key}")
}

fn sfc_action_duration_ms(duration: Option<&Literal>) -> i128 {
    match duration {
        Some(Literal::DurationMs(value)) => (*value).max(0),
        Some(Literal::Int(value)) => (*value as i128).max(0),
        _ => 0,
    }
}

fn is_implicit_en(name: &Identifier) -> bool {
    name.canonical == "EN"
}

fn is_implicit_eno(name: &Identifier) -> bool {
    name.canonical == "ENO"
}

fn split_input_expr(args: &[ParamAssignment]) -> Option<&Expr> {
    args.iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(|name| name.canonical == "IN"))
        .and_then(|arg| arg.expr.as_ref())
        .or_else(|| split_positional_args(args).first().copied())
}

fn split_positional_args(args: &[ParamAssignment]) -> Vec<&Expr> {
    args.iter()
        .filter(|arg| !arg.output)
        .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
        .filter(|arg| arg.name.is_none())
        .filter_map(|arg| arg.expr.as_ref())
        .collect()
}

fn split_formal_output<'a>(args: &'a [ParamAssignment], output: &str) -> Option<&'a VariableRef> {
    args.iter()
        .find(|arg| {
            arg.output
                && arg
                    .name
                    .as_ref()
                    .is_some_and(|name| name.canonical == output)
        })
        .and_then(|arg| arg.variable.as_ref())
}

fn split_output_variable<'a>(
    args: &'a [ParamAssignment],
    output: &str,
    positional_index: usize,
) -> Option<&'a VariableRef> {
    split_formal_output(args, output).or_else(|| {
        let positional = split_positional_args(args);
        let Expr::Variable(variable) = positional.get(positional_index + 1)? else {
            return None;
        };
        Some(variable)
    })
}

fn input_time_value(value: &Value) -> i128 {
    match value {
        Value::TimeMs(value) => *value,
        value => value.as_i64().unwrap_or(0) as i128,
    }
}

fn civil_from_days(days: i128) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 }.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096).div_euclid(365);
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let month_prime = (5 * doy + 2).div_euclid(153);
    let day = doy - (153 * month_prime + 2).div_euclid(5) + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    let year = year + if month <= 2 { 1 } else { 0 };
    (year as i64, month as i64, day as i64)
}

fn tod_parts(ms: i128) -> (i64, i64, i64, i64) {
    let ms = ms.rem_euclid(86_400_000);
    let hour = ms / 3_600_000;
    let minute = (ms % 3_600_000) / 60_000;
    let second = (ms % 60_000) / 1_000;
    let millisecond = ms % 1_000;
    (
        hour as i64,
        minute as i64,
        second as i64,
        millisecond as i64,
    )
}

fn is_standard_function_block_type(name: &str) -> bool {
    matches!(
        canonical_identifier(name).as_str(),
        "SR" | "RS" | "R_TRIG" | "F_TRIG" | "CTU" | "CTD" | "CTUD" | "TON" | "TOF" | "TP"
    ) || is_communication_function_block(name)
}

fn standard_function_block_output_names(name: &str) -> &'static [&'static str] {
    match canonical_identifier(name).as_str() {
        "SR" | "RS" => &["Q1"],
        "R_TRIG" | "F_TRIG" => &["Q"],
        "CTU" | "CTD" => &["Q", "CV"],
        "CTUD" => &["QU", "QD", "CV"],
        "TON" | "TOF" | "TP" => &["Q", "ET"],
        name if is_communication_function_block(name) => &["DONE", "NDR", "ERROR", "STATUS"],
        _ => &[],
    }
}

fn standard_function_block_input_names(name: &str) -> &'static [&'static str] {
    match canonical_identifier(name).as_str() {
        "SR" => &["S1", "R"],
        "RS" => &["S", "R1"],
        "R_TRIG" | "F_TRIG" => &["CLK"],
        "CTU" => &["CU", "R", "PV"],
        "CTD" => &["CD", "LD", "PV"],
        "CTUD" => &["CU", "CD", "R", "LD", "PV"],
        "TON" | "TOF" | "TP" => &["IN", "PT"],
        name if is_communication_function_block(name) => &["REQ", "EN_R", "ID", "LEN"],
        _ => &[],
    }
}

fn user_fb_input_target(
    input_fields: &[(VarBlockKind, Identifier)],
    arg: &ParamAssignment,
    positional_index: &mut usize,
) -> Option<(VarBlockKind, Identifier)> {
    if arg.output || arg.name.as_ref().is_some_and(is_implicit_en) {
        return None;
    }
    if let Some(name) = &arg.name {
        input_fields
            .iter()
            .find(|(_, field)| field.canonical == name.canonical)
            .cloned()
    } else {
        let target = input_fields.get(*positional_index).cloned();
        *positional_index += 1;
        target
    }
}

fn user_fb_input_edge(function_block: &Pou, input_name: &str) -> Option<EdgeQualifier> {
    function_block
        .var_blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Input)
        .flat_map(|block| block.vars.iter())
        .find(|var| var.name.canonical == input_name)
        .and_then(|var| var.edge)
}

fn edge_state_field_name(input_name: &str) -> String {
    format!("$EDGE_{}", canonical_identifier(input_name))
}

fn input_bool(inputs: &BTreeMap<String, Value>, name: &str) -> bool {
    inputs
        .get(&canonical_identifier(name))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn input_i64(inputs: &BTreeMap<String, Value>, name: &str) -> i64 {
    inputs
        .get(&canonical_identifier(name))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn input_time_ms(inputs: &BTreeMap<String, Value>, name: &str) -> i128 {
    match inputs.get(&canonical_identifier(name)) {
        Some(Value::TimeMs(value)) => *value,
        Some(value) => value.as_i64().unwrap_or(0) as i128,
        None => 0,
    }
}

fn bit_bool_binary(
    left: Value,
    right: Value,
    int_op: fn(i64, i64) -> i64,
    bool_op: fn(bool, bool) -> bool,
) -> Option<Value> {
    if matches!(left, Value::Bool(_)) && matches!(right, Value::Bool(_)) {
        Some(Value::Bool(bool_op(left.as_bool()?, right.as_bool()?)))
    } else {
        Some(Value::Int(int_op(left.as_i64()?, right.as_i64()?)))
    }
}

fn compare_values(left: &Value, right: &Value) -> Option<i8> {
    if value_text(left).is_some() || value_text(right).is_some() {
        let left = value_text(left).unwrap_or_else(|| left.to_string());
        let right = value_text(right).unwrap_or_else(|| right.to_string());
        return Some(match left.cmp(&right) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        });
    }

    let left = left.as_f64()?;
    let right = right.as_f64()?;
    if (left - right).abs() < f64::EPSILON {
        Some(0)
    } else if left < right {
        Some(-1)
    } else {
        Some(1)
    }
}

fn value_text(value: &Value) -> Option<String> {
    match value {
        Value::String(value) | Value::WString(value) => Some(value.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use iec_semantics::{check_project, CheckOptions};
    use iec_syntax::parse_project;

    use super::*;

    #[test]
    fn executes_counter_program() {
        let source = r#"
            PROGRAM Demo
            VAR Count : INT := 0; Done : BOOL := FALSE; END_VAR
            Count := Count + 1;
            IF Count >= 2 THEN Done := TRUE; END_IF;
            END_PROGRAM
        "#;
        let output = parse_project("test.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_var_external_against_project_global_state() {
        let source = r#"
            PROGRAM Globals
            VAR_GLOBAL
                Shared : INT := 5;
            END_VAR
            END_PROGRAM

            PROGRAM Demo
            VAR_EXTERNAL
                Shared : INT;
            END_VAR
            VAR
                Local : INT := 0;
            END_VAR
            Shared := Shared + 2;
            Local := Shared;
            END_PROGRAM
        "#;
        let output = parse_project("var_external_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SHARED" && *value == Value::Int(7)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "LOCAL" && *value == Value::Int(7)));
    }

    #[test]
    fn traces_program_access_paths() {
        let source = r#"
            TYPE
                Pair : STRUCT
                    Flag : BOOL;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                Count : INT := 0;
                Data : Pair;
                Edge : R_TRIG;
            END_VAR
            VAR_ACCESS
                PublicCount : Count : INT READ_WRITE;
                PublicFlag : Data.Flag : BOOL READ_ONLY;
                PublicEdge : Edge.Q : BOOL READ_ONLY;
            END_VAR
            Count := Count + 1;
            Data.Flag := Count >= 1;
            Edge(CLK := TRUE);
            END_PROGRAM
        "#;
        let output = parse_project("access_trace.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let access_paths = &trace.cycles[0].access_paths;
        assert!(access_paths.iter().any(|access| {
            access.name == "PublicCount"
                && access.target == "Count"
                && access.direction == AccessDirection::ReadWrite
                && access.value == Some(Value::Int(1))
        }));
        assert!(access_paths.iter().any(|access| {
            access.name == "PublicFlag"
                && access.target == "Data.Flag"
                && access.direction == AccessDirection::ReadOnly
                && access.value == Some(Value::Bool(true))
        }));
        assert!(access_paths.iter().any(|access| {
            access.name == "PublicEdge"
                && access.target == "Edge.Q"
                && access.value == Some(Value::Bool(true))
        }));
    }

    #[test]
    fn applies_program_access_path_writes_before_scan() {
        let source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR
            VAR_ACCESS
                PublicCount : Count : INT READ_WRITE;
                PublicFlag : Flag : BOOL READ_ONLY;
            END_VAR
            Count := Count + 1;
            END_PROGRAM
        "#;
        let output = parse_project("access_writes.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);

        let trace = run_program_with_access_writes(
            &output.project,
            Some("Demo"),
            1,
            &RuntimeOptions::default(),
            &[AccessPathWrite {
                cycle: 0,
                name: "PublicCount".to_string(),
                value: Value::Int(41),
            }],
        )
        .expect("program should run");
        let variables = &trace.cycles[0].variables;
        assert!(variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(42)));
        assert!(trace.cycles[0].access_paths.iter().any(|access| {
            access.name == "PublicCount" && access.value == Some(Value::Int(42))
        }));

        let error = run_program_with_access_writes(
            &output.project,
            Some("Demo"),
            1,
            &RuntimeOptions::default(),
            &[AccessPathWrite {
                cycle: 0,
                name: "PublicFlag".to_string(),
                value: Value::Bool(true),
            }],
        )
        .expect_err("READ_ONLY access write should fail");
        assert!(error
            .iter()
            .any(|diagnostic| diagnostic.message.contains("PublicFlag' is READ_ONLY")));

        let error = run_program_with_access_writes(
            &output.project,
            Some("Demo"),
            1,
            &RuntimeOptions::default(),
            &[AccessPathWrite {
                cycle: 0,
                name: "PublicCount".to_string(),
                value: Value::String("bad".to_string()),
            }],
        )
        .expect_err("wrong access write type should fail");
        assert!(error.iter().any(|diagnostic| diagnostic
            .message
            .contains("VAR_ACCESS path 'PublicCount' expects integer, got STRING")));
    }

    #[test]
    fn traces_configuration_access_paths_to_program_instances() {
        let source = r#"
            PROGRAM Demo
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
                RESOURCE Cpu ON PLC
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Demo;
                    VAR_ACCESS
                        ResourceCount : Main.Count : INT READ_ONLY;
                    END_VAR
                END_RESOURCE
                VAR_ACCESS
                    ConfigCount : Cpu.Main.Count : INT READ_ONLY;
                END_VAR
            END_CONFIGURATION
        "#;
        let output = parse_project("configuration_access_trace.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_configuration(
            &output.project,
            Some("Plant"),
            2,
            &RuntimeOptions::default(),
        )
        .expect("configuration should run");
        let cycle = &trace.cycles[1];
        assert!(cycle.access_paths.iter().any(|access| {
            access.name == "ConfigCount"
                && access.target == "Cpu.Main.Count"
                && access.value == Some(Value::Int(2))
        }));
        assert!(cycle.access_paths.iter().any(|access| {
            access.name == "Cpu.ResourceCount"
                && access.target == "Main.Count"
                && access.value == Some(Value::Int(2))
        }));
    }

    #[test]
    fn applies_configuration_program_instance_output_bindings() {
        let source = r#"
            PROGRAM Producer
            VAR_OUTPUT
                Count : INT := 0;
            END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
                VAR_GLOBAL
                    PlantObserved : INT := 0;
                END_VAR
                VAR_ACCESS
                    ConfigObserved : PlantObserved : INT READ_ONLY;
                END_VAR
                RESOURCE Cpu ON PLC
                    VAR_GLOBAL
                        ResourceObserved : INT := 0;
                    END_VAR
                    VAR_ACCESS
                        LocalObserved : ResourceObserved : INT READ_ONLY;
                    END_VAR
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Producer(Count => ResourceObserved);
                    PROGRAM ConfigMain WITH Fast : Producer(Count => PlantObserved);
                END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("configuration_program_output_bindings.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);

        let trace = run_configuration(
            &output.project,
            Some("Plant"),
            2,
            &RuntimeOptions::default(),
        )
        .expect("configuration should run");

        let cycle1 = &trace.cycles[1];
        assert!(cycle1.access_paths.iter().any(|access| {
            access.name == "ConfigObserved" && access.value == Some(Value::Int(2))
        }));
        assert!(cycle1.access_paths.iter().any(|access| {
            access.name == "Cpu.LocalObserved" && access.value == Some(Value::Int(2))
        }));
    }

    #[test]
    fn applies_program_instance_output_bindings_to_indexed_globals() {
        let source = r#"
            TYPE
                Slots : ARRAY [1..2] OF INT;
            END_TYPE

            PROGRAM Producer
            VAR_OUTPUT
                Count : INT := 0;
            END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
                VAR_GLOBAL
                    Values : Slots;
                END_VAR
                VAR_ACCESS
                    PublicValues : Values : Slots READ_ONLY;
                END_VAR
                RESOURCE Cpu ON PLC
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Producer(Count => Values[2]);
                END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("configuration_program_indexed_output_bindings.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);

        let trace = run_configuration(
            &output.project,
            Some("Plant"),
            2,
            &RuntimeOptions::default(),
        )
        .expect("configuration should run");

        assert!(trace.cycles[1].access_paths.iter().any(|access| {
            access.name == "PublicValues"
                && access.value == Some(Value::Array(vec![Value::Int(0), Value::Int(2)]))
        }));
    }

    #[test]
    fn applies_configuration_access_path_writes_to_globals_resources_and_programs() {
        let source = r#"
            PROGRAM Demo
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
                VAR_GLOBAL
                    Shared : INT := ADD(2, 3);
                END_VAR
                VAR_ACCESS
                    ConfigShared : Shared : INT READ_WRITE;
                    CpuProgramCount : Cpu.Main.Count : INT READ_WRITE;
                END_VAR
                RESOURCE Cpu ON PLC
                    VAR_GLOBAL
                        DeviceReady : BOOL := FALSE;
                    END_VAR
                    VAR_ACCESS
                        ResourceReady : DeviceReady : BOOL READ_WRITE;
                        ReadOnlyCount : Main.Count : INT READ_ONLY;
                    END_VAR
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Demo;
                END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("configuration_access_writes.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);

        let trace = run_configuration_with_access_writes(
            &output.project,
            Some("Plant"),
            2,
            &RuntimeOptions::default(),
            &[
                AccessPathWrite {
                    cycle: 1,
                    name: "ConfigShared".to_string(),
                    value: Value::Int(7),
                },
                AccessPathWrite {
                    cycle: 0,
                    name: "Cpu.ResourceReady".to_string(),
                    value: Value::Bool(true),
                },
                AccessPathWrite {
                    cycle: 0,
                    name: "CpuProgramCount".to_string(),
                    value: Value::Int(41),
                },
            ],
        )
        .expect("configuration should run");

        let cycle0 = &trace.cycles[0];
        assert!(cycle0.access_paths.iter().any(|access| {
            access.name == "ConfigShared" && access.value == Some(Value::Int(5))
        }));
        assert!(cycle0.access_paths.iter().any(|access| {
            access.name == "Cpu.ResourceReady" && access.value == Some(Value::Bool(true))
        }));
        assert!(cycle0.access_paths.iter().any(|access| {
            access.name == "CpuProgramCount" && access.value == Some(Value::Int(42))
        }));

        let cycle1 = &trace.cycles[1];
        assert!(cycle1.access_paths.iter().any(|access| {
            access.name == "ConfigShared" && access.value == Some(Value::Int(7))
        }));
        assert!(cycle1.access_paths.iter().any(|access| {
            access.name == "Cpu.ResourceReady" && access.value == Some(Value::Bool(true))
        }));
        assert!(cycle1.access_paths.iter().any(|access| {
            access.name == "CpuProgramCount" && access.value == Some(Value::Int(43))
        }));

        let error = run_configuration_with_access_writes(
            &output.project,
            Some("Plant"),
            1,
            &RuntimeOptions::default(),
            &[AccessPathWrite {
                cycle: 0,
                name: "Cpu.ReadOnlyCount".to_string(),
                value: Value::Int(10),
            }],
        )
        .expect_err("READ_ONLY configuration access write should fail");
        assert!(error.iter().any(|diagnostic| diagnostic
            .message
            .contains("Cpu.ReadOnlyCount' is READ_ONLY")));

        let error = run_configuration_with_access_writes(
            &output.project,
            Some("Plant"),
            1,
            &RuntimeOptions::default(),
            &[AccessPathWrite {
                cycle: 0,
                name: "ConfigShared".to_string(),
                value: Value::String("bad".to_string()),
            }],
        )
        .expect_err("wrong configuration access write type should fail");
        assert!(error.iter().any(|diagnostic| diagnostic
            .message
            .contains("VAR_ACCESS path 'ConfigShared' expects integer, got STRING")));
    }

    #[test]
    fn routes_configuration_direct_access_and_outputs_through_shared_state() {
        let output_source = r#"
            PROGRAM Producer
            VAR_OUTPUT
                Out : BOOL := FALSE;
            END_VAR
            Out := TRUE;
            END_PROGRAM

            PROGRAM Consumer
            VAR
                Seen : BOOL := FALSE;
            END_VAR
            Seen := %QX0.0;
            END_PROGRAM

            CONFIGURATION Plant
                RESOURCE Cpu ON PLC
                    VAR_ACCESS
                        DirectOut : %QX0.0 : BOOL READ_WRITE;
                    END_VAR
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM AProducer WITH Fast : Producer(Out => %QX0.0);
                    PROGRAM ZConsumer WITH Fast : Consumer;
                END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("configuration_direct_output.st", output_source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_configuration(
            &output.project,
            Some("Plant"),
            1,
            &RuntimeOptions::default(),
        )
        .expect("configuration should run");
        let cycle0 = &trace.cycles[0];
        assert!(cycle0.programs.iter().any(|program| {
            program.instance == "ZConsumer"
                && program
                    .variables
                    .iter()
                    .any(|(name, value)| name == "SEEN" && *value == Value::Bool(true))
        }));
        assert!(cycle0.access_paths.iter().any(|access| {
            access.name == "Cpu.DirectOut" && access.value == Some(Value::Bool(true))
        }));

        let access_source = r#"
            PROGRAM Consumer
            VAR
                Seen : BOOL := FALSE;
            END_VAR
            Seen := %QX0.1;
            END_PROGRAM

            CONFIGURATION Plant
                RESOURCE Cpu ON PLC
                    VAR_ACCESS
                        DirectOut : %QX0.1 : BOOL READ_WRITE;
                    END_VAR
                    TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                    PROGRAM Main WITH Fast : Consumer;
                END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("configuration_direct_access_write.st", access_source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_configuration_with_access_writes(
            &output.project,
            Some("Plant"),
            1,
            &RuntimeOptions::default(),
            &[AccessPathWrite {
                cycle: 0,
                name: "Cpu.DirectOut".to_string(),
                value: Value::Bool(true),
            }],
        )
        .expect("configuration should run");
        let cycle0 = &trace.cycles[0];
        assert!(cycle0.programs.iter().any(|program| {
            program.instance == "Main"
                && program
                    .variables
                    .iter()
                    .any(|(name, value)| name == "SEEN" && *value == Value::Bool(true))
        }));
        assert!(cycle0.access_paths.iter().any(|access| {
            access.name == "Cpu.DirectOut" && access.value == Some(Value::Bool(true))
        }));
    }

    #[test]
    fn enforces_max_scan_cycles() {
        let source = r#"
            PROGRAM Demo
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM
        "#;
        let output = parse_project("scan_limit.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let result = run_program(
            &output.project,
            Some("Demo"),
            3,
            &RuntimeOptions {
                max_scan_cycles: 2,
                ..RuntimeOptions::default()
            },
        );
        let diagnostics = result.expect_err("scan limit should reject the run");
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("scan cycle count 3")));
    }

    #[test]
    fn runs_configuration_tasks_by_interval_and_priority() {
        let source = r#"
            PROGRAM FastProgram
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            PROGRAM SlowProgram
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
            RESOURCE Cpu ON PLC
                TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                TASK Slow(INTERVAL := T#2ms, PRIORITY := 2);
                PROGRAM FastInstance WITH Fast : FastProgram(Count := 10);
                PROGRAM SlowInstance WITH Slow : SlowProgram(Count := ADD(20, 1));
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("configuration_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_configuration(
            &output.project,
            Some("Plant"),
            3,
            &RuntimeOptions::default(),
        )
        .expect("configuration should run");
        assert_eq!(trace.cycles.len(), 3);
        assert_eq!(trace.cycles[0].programs[0].instance, "FastInstance");
        assert_eq!(trace.cycles[0].programs[1].instance, "SlowInstance");
        assert_eq!(trace.cycles[1].programs.len(), 1);
        let fast_last = trace
            .cycles
            .last()
            .unwrap()
            .programs
            .iter()
            .find(|program| program.instance == "FastInstance")
            .unwrap();
        assert!(fast_last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(13)));
        let slow_last = trace.cycles[2]
            .programs
            .iter()
            .find(|program| program.instance == "SlowInstance")
            .unwrap();
        assert!(slow_last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(23)));
    }

    #[test]
    fn runs_configuration_single_tasks_on_rising_edges() {
        let source = r#"
            PROGRAM EventProgram
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                Trigger : BOOL := FALSE;
            END_VAR
            VAR_ACCESS
                PublicTrigger : Trigger : BOOL READ_WRITE;
            END_VAR
            RESOURCE Cpu ON PLC
                TASK OnTrigger(SINGLE := Trigger, PRIORITY := 1);
                PROGRAM EventInstance WITH OnTrigger : EventProgram;
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("configuration_single_task_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let writes = [
            AccessPathWrite {
                cycle: 0,
                name: "PublicTrigger".to_string(),
                value: Value::Bool(true),
            },
            AccessPathWrite {
                cycle: 2,
                name: "PublicTrigger".to_string(),
                value: Value::Bool(false),
            },
            AccessPathWrite {
                cycle: 3,
                name: "PublicTrigger".to_string(),
                value: Value::Bool(true),
            },
        ];
        let trace = run_configuration_with_access_writes(
            &output.project,
            Some("Plant"),
            5,
            &RuntimeOptions::default(),
            &writes,
        )
        .expect("configuration should run");
        assert_eq!(trace.cycles.len(), 5);
        assert_eq!(trace.cycles[0].programs.len(), 1);
        assert!(trace.cycles[1].programs.is_empty());
        assert!(trace.cycles[2].programs.is_empty());
        assert_eq!(trace.cycles[3].programs.len(), 1);
        assert!(trace.cycles[4].programs.is_empty());
        let second_fire = &trace.cycles[3].programs[0];
        assert!(second_fire
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
    }

    #[test]
    fn stress_schedules_interval_single_direct_globals_and_access_writes() {
        let source = r#"
            PROGRAM Producer
            VAR_OUTPUT
                Count : INT := 0;
                Pulse : BOOL := FALSE;
            END_VAR
            Count := Count + 1;
            Pulse := Count >= 2;
            END_PROGRAM

            PROGRAM Reader
            VAR_OUTPUT
                DirectSeen : BOOL := FALSE;
            END_VAR
            DirectSeen := %QX0.2;
            END_PROGRAM

            PROGRAM EventProgram
            VAR_OUTPUT
                EventTotal : INT := 0;
            END_VAR
            EventTotal := EventTotal + 10;
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                Trigger : BOOL := FALSE;
                Shared : INT := 0;
                DirectSeenGlobal : BOOL := FALSE;
                EventTotalGlobal : INT := 0;
            END_VAR
            VAR_ACCESS
                PublicTrigger : Trigger : BOOL READ_WRITE;
                PublicShared : Shared : INT READ_ONLY;
                PublicDirectSeen : DirectSeenGlobal : BOOL READ_ONLY;
                PublicEventTotal : EventTotalGlobal : INT READ_ONLY;
                PublicDirectOut : %QX0.2 : BOOL READ_ONLY;
            END_VAR
            RESOURCE Cpu ON PLC
                TASK OnTrigger(SINGLE := Trigger, PRIORITY := 0);
                TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                TASK Slow(INTERVAL := T#2ms, PRIORITY := 2);
                PROGRAM FastProducer WITH Fast : Producer(Count => Shared, Pulse => %QX0.2);
                PROGRAM SlowReader WITH Slow : Reader(DirectSeen => DirectSeenGlobal);
                PROGRAM EventInstance WITH OnTrigger : EventProgram(EventTotal => EventTotalGlobal);
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("configuration_scheduling_stress.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);

        let trace = run_configuration_with_access_writes(
            &output.project,
            Some("Plant"),
            5,
            &RuntimeOptions::default(),
            &[
                AccessPathWrite {
                    cycle: 1,
                    name: "PublicTrigger".to_string(),
                    value: Value::Bool(true),
                },
                AccessPathWrite {
                    cycle: 2,
                    name: "PublicTrigger".to_string(),
                    value: Value::Bool(false),
                },
                AccessPathWrite {
                    cycle: 3,
                    name: "PublicTrigger".to_string(),
                    value: Value::Bool(true),
                },
            ],
        )
        .expect("configuration should run");

        assert_eq!(trace.cycles.len(), 5);
        assert_eq!(trace.cycles[0].programs.len(), 2);
        assert_eq!(trace.cycles[1].programs.len(), 2);
        assert_eq!(trace.cycles[2].programs.len(), 2);
        assert_eq!(trace.cycles[3].programs.len(), 2);
        assert_eq!(trace.cycles[4].programs.len(), 2);

        let cycle4 = &trace.cycles[4];
        assert!(cycle4.access_paths.iter().any(|access| {
            access.name == "PublicShared" && access.value == Some(Value::Int(5))
        }));
        assert!(cycle4.access_paths.iter().any(|access| {
            access.name == "PublicDirectOut" && access.value == Some(Value::Bool(true))
        }));
        assert!(cycle4.access_paths.iter().any(|access| {
            access.name == "PublicDirectSeen" && access.value == Some(Value::Bool(true))
        }));
        assert!(cycle4.access_paths.iter().any(|access| {
            access.name == "PublicEventTotal" && access.value == Some(Value::Int(20))
        }));
    }

    #[test]
    fn executes_loops_case_and_standard_functions() {
        let source = r#"
            TYPE
                Mode : (Idle, Run, Fault);
            END_TYPE

            PROGRAM Demo
            VAR
                I : INT := 0;
                Total : INT := 0;
                Selected : INT := 0;
                Done : BOOL := FALSE;
                State : Mode := Run;
                EnumDone : BOOL := FALSE;
            END_VAR

            FOR I := 1 TO 3 DO
                Total := Total + I;
            END_FOR;

            WHILE Total < 8 DO
                Total := Total + 1;
            END_WHILE;

            REPEAT
                Total := Total - 1;
            UNTIL Total = 7
            END_REPEAT;

            Selected := MAX(Total, 3);

            CASE Selected OF
                7: Done := TRUE;
                ELSE Done := FALSE;
            END_CASE;
            CASE State OF
                Idle: EnumDone := FALSE;
                Run, Fault: EnumDone := TRUE;
            END_CASE;
            END_PROGRAM
        "#;
        let output = parse_project("test.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TOTAL" && *value == Value::Int(7)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SELECTED" && *value == Value::Int(7)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ENUMDONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_standard_power_precedence_and_associativity() {
        let source = r#"
            PROGRAM Demo
            VAR
                RightAssoc : REAL := 0.0;
                NegatedPower : REAL := 0.0;
                Positive : INT := 0;
            END_VAR
            RightAssoc := 2 ** 3 ** 2;
            NegatedPower := -2 ** 2;
            Positive := +2 + +(+3);
            END_PROGRAM
        "#;
        let output = parse_project("power_precedence.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "RIGHTASSOC" && *value == Value::Real(512.0)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "NEGATEDPOWER" && *value == Value::Real(-4.0)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "POSITIVE" && *value == Value::Int(5)));
    }

    #[test]
    fn executes_user_defined_functions() {
        let source = r#"
            FUNCTION Scale : INT
            VAR_INPUT
                Input : INT;
                Factor : INT;
            END_VAR
            Scale := Input * Factor;
            END_FUNCTION

            PROGRAM Demo
            VAR
                A : INT := 4;
                B : INT := 0;
                C : INT := 0;
            END_VAR
            B := Scale(A, 3);
            C := Scale(Input := B, Factor := 2);
            END_PROGRAM
        "#;
        let output = parse_project("functions.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "B" && *value == Value::Int(12)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "C" && *value == Value::Int(24)));
    }

    #[test]
    fn executes_function_en_eno_controls() {
        let source = r#"
            FUNCTION Scale : INT
            VAR_INPUT
                Input : INT;
            END_VAR
            Scale := Input * 2;
            END_FUNCTION

            PROGRAM Demo
            VAR
                EnabledResult : INT := 0;
                DisabledResult : INT := 5;
                EnabledOk : BOOL := FALSE;
                DisabledOk : BOOL := TRUE;
            END_VAR

            EnabledResult := Scale(EN := TRUE, Input := 3, ENO => EnabledOk);
            DisabledResult := Scale(EN := FALSE, Input := 10, ENO => DisabledOk);
            END_PROGRAM
        "#;
        let output = parse_project("function_controls.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ENABLEDRESULT" && *value == Value::Int(6)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DISABLEDRESULT" && *value == Value::Int(0)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ENABLEDOK" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DISABLEDOK" && *value == Value::Bool(false)));
    }

    #[test]
    fn executes_disabled_standard_function_defaults_by_return_family() {
        let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING[8] := 'keep';
                Wide : WSTRING[8] := "keep";
                Cmp : BOOL := TRUE;
                Root : REAL := 5.5;
                Delay : TIME := T#1s;
                TextOk : BOOL := TRUE;
                WideOk : BOOL := TRUE;
                CmpOk : BOOL := TRUE;
                RealOk : BOOL := TRUE;
                TimeOk : BOOL := TRUE;
            END_VAR

            Text := LEFT(EN := FALSE, IN := 'robot', L := 2, ENO => TextOk);
            Wide := LEFT(EN := FALSE, IN := "robot", L := 2, ENO => WideOk);
            Cmp := EQ(EN := FALSE, IN1 := 1, IN2 := 1, ENO => CmpOk);
            Root := SQRT(EN := FALSE, IN := 4.0, ENO => RealOk);
            Delay := ADD_TIME(EN := FALSE, IN1 := T#1s, IN2 := T#2s, ENO => TimeOk);
            END_PROGRAM
        "#;
        let output = parse_project("disabled_standard_defaults.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TEXT" && *value == Value::String(String::new())));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "WIDE" && *value == Value::WString(String::new())));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "CMP" && *value == Value::Bool(false)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ROOT" && *value == Value::Real(0.0)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DELAY" && *value == Value::TimeMs(0)));
        for flag in ["TEXTOK", "WIDEOK", "CMPOK", "REALOK", "TIMEOK"] {
            assert!(last
                .variables
                .iter()
                .any(|(name, value)| name == flag && *value == Value::Bool(false)));
        }
    }

    #[test]
    fn executes_disabled_user_function_defaults_for_named_returns() {
        let source = r#"
            TYPE
                ShortText : STRING[8];
                Pair : STRUCT
                    A : INT := 1;
                    B : BOOL := TRUE;
                END_STRUCT;
            END_TYPE

            FUNCTION Label : ShortText
            Label := 'live';
            END_FUNCTION

            FUNCTION MakePair : Pair
            MakePair := (A := 7, B := FALSE);
            END_FUNCTION

            PROGRAM Demo
            VAR
                Text : ShortText := 'keep';
                Item : Pair := (A := 9, B := TRUE);
                TextOk : BOOL := TRUE;
                PairOk : BOOL := TRUE;
            END_VAR

            Text := Label(EN := FALSE, ENO => TextOk);
            Item := MakePair(EN := FALSE, ENO => PairOk);
            END_PROGRAM
        "#;
        let output = parse_project("disabled_user_function_named_returns.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TEXT" && *value == Value::String(String::new())));
        let mut expected = BTreeMap::new();
        expected.insert("A".to_string(), Value::Int(1));
        expected.insert("B".to_string(), Value::Bool(true));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ITEM" && *value == Value::Struct(expected.clone())));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TEXTOK" && *value == Value::Bool(false)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "PAIROK" && *value == Value::Bool(false)));
    }

    #[test]
    fn executes_named_subrange_defaults_for_state_and_disabled_returns() {
        let source = r#"
            TYPE
                Positive : INT(5..10);
                Zeroable : INT(-1..3);
            END_TYPE

            FUNCTION Pick : Positive
            Pick := 9;
            END_FUNCTION

            FUNCTION_BLOCK Holder
            VAR_OUTPUT
                Out : Positive;
            END_VAR
            END_FUNCTION_BLOCK

            PROGRAM Globals
            VAR_GLOBAL
                Shared : Positive;
            END_VAR
            END_PROGRAM

            PROGRAM Demo
            VAR_EXTERNAL
                Shared : Positive;
            END_VAR
            VAR
                Direct : Positive;
                IncludesZero : Zeroable;
                Fb : Holder;
                Disabled : Positive := 6;
                Ok : BOOL := TRUE;
            END_VAR

            Disabled := Pick(EN := FALSE, ENO => Ok);
            END_PROGRAM
        "#;
        let output = parse_project("subrange_defaults_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SHARED" && *value == Value::Int(5)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DIRECT" && *value == Value::Int(5)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "INCLUDESZERO" && *value == Value::Int(0)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FB.OUT" && *value == Value::Int(5)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DISABLED" && *value == Value::Int(5)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OK" && *value == Value::Bool(false)));
    }

    #[test]
    fn executes_expanded_standard_functions() {
        let source = r#"
            PROGRAM Demo
            VAR
                Sum : INT := 0;
                Product : INT := 0;
                Choice : INT := 0;
                Shifted : INT := 0;
                Rotated : INT := 0;
                Ok : BOOL := FALSE;
            END_VAR

            Sum := ADD(1, 2, 3);
            Product := MUL(Sum, 2);
            Choice := MUX(2, 10, 20, 30);
            Shifted := SHL(1, 3);
            Rotated := ROL(1, 1);
            Ok := GT(Product, Sum) AND EQ(MOVE(Choice), 30) AND NE(Shifted, Rotated);
            END_PROGRAM
        "#;
        let output = parse_project("standard_functions.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SUM" && *value == Value::Int(6)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "PRODUCT" && *value == Value::Int(12)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "CHOICE" && *value == Value::Int(30)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SHIFTED" && *value == Value::Int(8)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ROTATED" && *value == Value::Int(2)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OK" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_out_of_order_standard_function_formal_inputs() {
        let source = r#"
            PROGRAM Demo
            VAR
                Limited : INT := 0;
                Selected : INT := 0;
                Muxed : INT := 0;
                Shifted : INT := 0;
                Text : STRING[8] := '';
                Ok : BOOL := FALSE;
            END_VAR

            Limited := LIMIT(IN := 12, MN := 0, MX := 10);
            Selected := SEL(IN1 := 20, G := FALSE, IN0 := 10);
            Muxed := MUX(IN1 := 200, K := 1, IN0 := 100);
            Shifted := SHL(N := 2, IN := 1);
            Text := LEFT(L := 3, IN := 'robot');
            Ok := EQ(IN2 := 10, IN1 := Limited);
            END_PROGRAM
        "#;
        let output = parse_project("standard_formal_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "LIMITED" && *value == Value::Int(10)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SELECTED" && *value == Value::Int(10)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "MUXED" && *value == Value::Int(200)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SHIFTED" && *value == Value::Int(4)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TEXT" && *value == Value::String("rob".to_string())));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OK" && *value == Value::Bool(true)));
    }

    #[test]
    fn rejects_negative_shift_counts_at_runtime() {
        let source = r#"
            PROGRAM Demo
            VAR
                Count : INT := -1;
                Shifted : INT := 0;
            END_VAR
            Shifted := SHL(1, Count);
            END_PROGRAM
        "#;
        let output = parse_project("negative_shift_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
        assert!(result
            .expect_err("negative shift should fail")
            .iter()
            .any(|diagnostic| diagnostic
                .message
                .contains("standard function 'SHL' failed for supplied arguments")));
    }

    #[test]
    fn executes_string_bit_and_time_standard_functions() {
        let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING := '';
                QuotedText : STRING[16] := 'A$"B$'';
                DateText : STRING[32] := '';
                TodText : WSTRING[32] := "";
                QuotedWide : WSTRING[16] := "A$'B$"";
                DtText : STRING[40] := '';
                Found : INT := 0;
                Mask : INT := 0;
                Flag : BOOL := FALSE;
                Delay : TIME := T#0ms;
                MinDelay : TIME := T#0ms;
                Span : TIME := T#0ms;
                Scale : TIME := T#0ms;
                TimeOfDay : TIME_OF_DAY := TOD#00:00:00;
                BuiltDate : DATE := D#1970-01-01;
                BuiltTod : TIME_OF_DAY := TOD#00:00:00;
                Stamp : DATE_AND_TIME := DT#1970-01-01-00:00:00;
                BuiltStamp : DATE_AND_TIME := DT#1970-01-01-00:00:00;
                Weekday : INT := 0;
                EscapedLen : INT := 0;
                Year : INT := 0;
                Month : INT := 0;
                DatePart : INT := 0;
                Hour : INT := 0;
                Minute : INT := 0;
                Second : INT := 0;
                Millisecond : INT := 0;
            END_VAR

            Text := CONCAT(LEFT('robot', 2), RIGHT('code', 2));
            DateText := DATE_TO_STRING(STRING_TO_DATE('D#1970-01-02'));
            TodText := TOD_TO_WSTRING(STRING_TO_TOD('TOD#01:02:03.004'));
            DtText := DATE_AND_TIME_TO_STRING(STRING_TO_DATE_AND_TIME('DT#1970-01-02-01:02:03.004'));
            Found := FIND(Text, 'de');
            Mask := OR(AND(15, 51), XOR(1, 3));
            Flag := XOR(TRUE, FALSE);
            Delay := ADD_TIME(T#1s, MUL_TIME(T#100ms, 2));
            MinDelay := MIN(T#2s, T#750ms);
            Span := SUB_DATE_DATE(D#1970-01-03, D#1970-01-01);
            Scale := DIVTIME(MULTIME(T#750ms, 4), 2);
            TimeOfDay := ADD_TOD_TIME(TOD#00:00:01, T#2s);
            BuiltDate := CONCAT_DATE(1970, 1, 3);
            BuiltTod := CONCAT_TOD(0, 0, 3, 250);
            Stamp := ADD_DT_TIME(DT#1970-01-01-00:00:01, T#2s);
            BuiltStamp := CONCAT_DATE_TOD(BuiltDate, BuiltTod);
            Weekday := DAY_OF_WEEK(D#1970-01-01);
            EscapedLen := LEN('A$0A$27$$');
            SPLIT_DT(
                IN := DT#1970-01-03-01:02:03.004,
                YEAR => Year,
                MONTH => Month,
                DATE => DatePart,
                HOUR => Hour,
                MINUTE => Minute,
                SECOND => Second,
                MILLISECOND => Millisecond);
            END_PROGRAM
        "#;
        let output = parse_project("standard_catalog.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TEXT" && *value == Value::String("rode".to_string())));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "QUOTEDTEXT" && *value == Value::String("A\"B'".to_string())
        }));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "DATETEXT" && *value == Value::String("D#1970-01-02".to_string())
        }));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "TODTEXT" && *value == Value::WString("TOD#01:02:03.004".to_string())
        }));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "QUOTEDWIDE" && *value == Value::WString("A'B\"".to_string())
        }));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "DTTEXT" && *value == Value::String("DT#1970-01-02-01:02:03.004".to_string())
        }));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FOUND" && *value == Value::Int(3)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "MASK" && *value == Value::Int(3)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DELAY" && *value == Value::TimeMs(1200)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "MINDELAY" && *value == Value::TimeMs(750)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SPAN" && *value == Value::TimeMs(172_800_000)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SCALE" && *value == Value::TimeMs(1_500)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TIMEOFDAY" && *value == Value::TimeMs(3_000)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "BUILTDATE" && *value == Value::TimeMs(2)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "BUILTTOD" && *value == Value::TimeMs(3_250)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "STAMP" && *value == Value::TimeMs(3_000)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| { name == "BUILTSTAMP" && *value == Value::TimeMs(172_803_250) }));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "WEEKDAY" && *value == Value::Int(4)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ESCAPEDLEN" && *value == Value::Int(4)));
        for (expected_name, expected_value) in [
            ("YEAR", 1970),
            ("MONTH", 1),
            ("DATEPART", 3),
            ("HOUR", 1),
            ("MINUTE", 2),
            ("SECOND", 3),
            ("MILLISECOND", 4),
        ] {
            assert!(last.variables.iter().any(|(name, value)| {
                name == expected_name && *value == Value::Int(expected_value)
            }));
        }
    }

    #[test]
    fn truncates_bounded_string_assignments_at_runtime() {
        let source = r#"
            PROGRAM Demo
            VAR
                Source : STRING[8] := 'abcdef';
                WideSource : WSTRING[8] := "abcdef";
                Text : STRING[3] := '';
                Wide : WSTRING[3] := "";
            END_VAR

            Text := CONCAT(LEFT(Source, 4), RIGHT(Source, 2));
            Wide := CONCAT(LEFT(WideSource, 4), RIGHT(WideSource, 2));
            END_PROGRAM
        "#;
        let output = parse_project("bounded_string_truncation_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TEXT" && *value == Value::String("abc".to_string())));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "WIDE" && *value == Value::WString("abc".to_string())));
    }

    #[test]
    fn executes_wstring_literals_and_string_functions() {
        let source = r#"
            PROGRAM Demo
            VAR
                Text : WSTRING[16] := "ro";
                Out : WSTRING[16] := "";
            END_VAR

            Out := CONCAT(Text, "bot");
            Text := LEFT(Out, 4);
            END_PROGRAM
        "#;
        let output = parse_project("wstring_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last.variables.iter().any(|(name, value)| {
            name == "OUT" && *value == Value::WString("robot".to_string())
        }));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "TEXT" && *value == Value::WString("robo".to_string())
        }));
    }

    #[test]
    fn executes_date_and_time_of_day_literals() {
        let source = r#"
            PROGRAM Demo
            VAR
                Today : DATE := D#1970-01-02;
                Leap : DATE := D#2024-02-29;
                Noon : TIME_OF_DAY := TOD#12:00:00.250;
                Stamp : DATE_AND_TIME := DT#1970-01-02-00:00:01;
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("date_literals.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TODAY" && *value == Value::TimeMs(1)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| { name == "LEAP" && *value == Value::TimeMs(19_782) }));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "NOON" && *value == Value::TimeMs(43_200_250)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "STAMP" && *value == Value::TimeMs(86_401_000)));
    }

    #[test]
    fn executes_typed_alias_literals_for_all_scalar_families() {
        let source = r#"
            TYPE
                MyBool : BOOL;
                MyBool2 : MyBool;
                MyReal : REAL;
                MyReal2 : MyReal;
                MyTime : TIME;
                MyTime2 : MyTime;
                MyDate : DATE;
                MyDate2 : MyDate;
                MyTod : TIME_OF_DAY;
                MyTod2 : MyTod;
                MyDt : DATE_AND_TIME;
                MyDt2 : MyDt;
                Small : INT(0..10);
                Small2 : Small;
                Mode : (Idle, Run, Fault);
                ModeAlias : Mode;
            END_TYPE

            PROGRAM Demo
            VAR
                Flag : MyBool2 := MyBool2#FALSE;
                RealValue : MyReal2 := MyReal2#0.0;
                Delay : MyTime2 := MyTime2#0ms;
                Today : MyDate2 := MyDate2#1970-01-01;
                Clock : MyTod2 := MyTod2#00:00:00;
                Stamp : MyDt2 := MyDt2#1970-01-01-00:00:00;
                SmallValue : Small2 := Small2#0;
                State : ModeAlias := ModeAlias#Idle;
                Text : STRING[64] := '';
            END_VAR

            Flag := MyBool2#TRUE;
            RealValue := MyReal2#1.5 + 0.5;
            Delay := MyTime2#1.5s;
            Today := MyDate2#1970-01-02;
            Clock := MyTod2#01:02:03.004;
            Stamp := MyDt2#1970-01-02-01:02:03.004;
            SmallValue := Small2#7;
            State := ModeAlias#Fault;
            Text := DATE_TO_STRING(Today);
            END_PROGRAM
        "#;
        let output = parse_project("typed_alias_literal_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "REALVALUE" && *value == Value::Real(2.0)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DELAY" && *value == Value::TimeMs(1500)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TODAY" && *value == Value::TimeMs(1)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "CLOCK" && *value == Value::TimeMs(3_723_004)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "STAMP" && *value == Value::TimeMs(90_123_004)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SMALLVALUE" && *value == Value::Int(7)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "STATE" && *value == Value::Int(2)));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "TEXT" && *value == Value::String("D#1970-01-02".to_string())
        }));
    }

    #[test]
    fn executes_expanded_conversion_functions() {
        let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING[32] := '';
                Parsed : INT := 0;
                Truncated : INT := 0;
                Bcd : WORD := 0;
                FromBcd : INT := 0;
                RealValue : REAL := 0.0;
                Flag : BOOL := FALSE;
                Delay : TIME := T#0ms;
                LongDelay : TIME := T#0ms;
                FractionalDelay : TIME := T#0ms;
            END_VAR

            Parsed := STRING_TO_INT('42');
            Truncated := TRUNC(-1.6);
            Bcd := INT_TO_BCD(369);
            FromBcd := BCD_TO_INT(Bcd) + WORD_BCD_TO_UINT(UINT_TO_BCD_WORD(25));
            RealValue := STRING_TO_REAL('2.5');
            Flag := STRING_TO_BOOL('TRUE');
            Delay := STRING_TO_TIME('T#250ms') + INT_TO_TIME(50);
            LongDelay := STRING_TO_TIME('T#1h2m3s4ms');
            FractionalDelay := STRING_TO_TIME('T#1.5s');
            Text := CONCAT(BOOL_TO_STRING(Flag), INT_TO_STRING(Parsed));
            END_PROGRAM
        "#;
        let output = parse_project("conversions.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "PARSED" && *value == Value::Int(42)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TRUNCATED" && *value == Value::Int(-1)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "BCD" && *value == Value::Int(0x369)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FROMBCD" && *value == Value::Int(394)));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "REALVALUE"
                && matches!(value, Value::Real(value) if (*value - 2.5).abs() < f64::EPSILON)
        }));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DELAY" && *value == Value::TimeMs(300)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "LONGDELAY" && *value == Value::TimeMs(3_723_004)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| { name == "FRACTIONALDELAY" && *value == Value::TimeMs(1_500) }));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TEXT" && *value == Value::String("TRUE42".to_string())));
    }

    #[test]
    fn executes_bool_and_bit_string_st_operators() {
        let source = r#"
            PROGRAM Demo
            VAR
                Mask : INT := 0;
                Inverted : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            Mask := (15 AND 51) OR (1 XOR 3);
            Inverted := NOT 15;
            Flag := (TRUE AND FALSE) OR TRUE;
            END_PROGRAM
        "#;
        let output = parse_project("st_bit_ops.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "MASK" && *value == Value::Int(3)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "INVERTED" && *value == Value::Int(!15)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
    }

    #[test]
    fn short_circuits_bool_and_or_operands() {
        let source = r#"
            PROGRAM Demo
            VAR
                Ok1 : BOOL := TRUE;
                Ok2 : BOOL := FALSE;
            END_VAR

            Ok1 := FALSE AND (1 / 0 = 0);
            Ok2 := TRUE OR (1 / 0 = 0);
            END_PROGRAM
        "#;
        let output = parse_project("short_circuit.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OK1" && *value == Value::Bool(false)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OK2" && *value == Value::Bool(true)));
    }

    #[test]
    fn reports_integer_overflow() {
        let source = r#"
            PROGRAM Demo
            VAR A : LINT := 9223372036854775807; END_VAR
            A := A + 1;
            END_PROGRAM
        "#;
        let output = parse_project("overflow.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
        let diagnostics = result.expect_err("overflow should reject the run");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("integer overflow during addition")));
    }

    #[test]
    fn executes_arrays_structs_enums_and_subrange_checks() {
        let source = r#"
            TYPE
                Small : INT(0..10);
                Mode : (Idle, Run, Fault);
                Pair : STRUCT
                    Low : Small := 1;
                    High : Small := 2;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                Values : ARRAY [1..3] OF Small := [1, 2, 3];
                Copy : ARRAY [1..3] OF Small := [0, 0, 0];
                Repeated : ARRAY [1..5] OF Small := [2(1), 3(2)];
                Window : Pair := (Low := 4, High := 6);
                Backup : Pair := (Low := 0, High := 0);
                State : Mode := Idle;
                Selected : Mode := Idle;
                Total : INT := 0;
                IsRun : BOOL := FALSE;
                IsNotIdle : BOOL := FALSE;
            END_VAR

            Values[2] := Values[1] + Window.High;
            Copy := Values;
            Window.Low := Values[2];
            Backup := Window;
            State := Run;
            Selected := MUX(1, Idle, Fault);
            IsRun := State = Run;
            IsNotIdle := NE(Selected, Idle);
            Total := Values[2] + Window.Low;
            END_PROGRAM
        "#;
        let output = parse_project("aggregates_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TOTAL" && *value == Value::Int(14)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ISRUN" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SELECTED" && *value == Value::Int(2)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ISNOTIDLE" && *value == Value::Bool(true)));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "COPY"
                && *value == Value::Array(vec![Value::Int(1), Value::Int(7), Value::Int(3)])
        }));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "REPEATED"
                && *value
                    == Value::Array(vec![
                        Value::Int(1),
                        Value::Int(1),
                        Value::Int(2),
                        Value::Int(2),
                        Value::Int(2),
                    ])
        }));
        let mut backup = BTreeMap::new();
        backup.insert("LOW".to_string(), Value::Int(7));
        backup.insert("HIGH".to_string(), Value::Int(6));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "BACKUP" && *value == Value::Struct(backup.clone())));
    }

    #[test]
    fn executes_nested_array_access_inside_structures() {
        let source = r#"
            TYPE
                Row : ARRAY [2..4] OF INT;
                Matrix : ARRAY [1..2] OF Row;
                Holder : STRUCT
                    Rows : Matrix;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                Box : Holder := (Rows := [[1, 2, 3], [4, 5, 6]]);
                RowCopy : Row := [0, 0, 0];
                Total : INT := 0;
            END_VAR

            Box.Rows[1][3] := 20;
            RowCopy := Box.Rows[1];
            Total := Box.Rows[1][2] + RowCopy[3] + Box.Rows[2][4];
            END_PROGRAM
        "#;
        let output = parse_project("nested_aggregates_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TOTAL" && *value == Value::Int(27)));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "ROWCOPY"
                && *value == Value::Array(vec![Value::Int(1), Value::Int(20), Value::Int(3)])
        }));
    }

    #[test]
    fn executes_nested_derived_aliases_for_aggregates_and_enums() {
        let source = r#"
            TYPE
                Small : INT(0..10);
                SmallAlias : Small;
                SmallAlias2 : SmallAlias;
                Row : ARRAY [1..2] OF SmallAlias2;
                RowAlias : Row;
                Holder : STRUCT
                    Values : RowAlias;
                END_STRUCT;
                HolderAlias : Holder;
                Mode : (Idle, Run);
                ModeAlias : Mode;
                ModeAlias2 : ModeAlias;
            END_TYPE

            PROGRAM Demo
            VAR
                Box : HolderAlias := (Values := [2, 3]);
                Copy : RowAlias := [0, 0];
                State : ModeAlias2 := Idle;
                Total : INT := 0;
                IsRun : BOOL := FALSE;
            END_VAR

            Box.Values[1] := 7;
            Copy := Box.Values;
            State := Run;
            IsRun := State = Run;
            Total := Copy[1] + Copy[2];
            END_PROGRAM
        "#;
        let output = parse_project("nested_alias_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TOTAL" && *value == Value::Int(10)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ISRUN" && *value == Value::Bool(true)));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "COPY" && *value == Value::Array(vec![Value::Int(7), Value::Int(3)])
        }));
    }

    #[test]
    fn rejects_runtime_subrange_violations() {
        let source = r#"
            TYPE Small : INT(0..10); END_TYPE
            PROGRAM Demo
            VAR Value : Small := 1; END_VAR
            Value := 11;
            END_PROGRAM
        "#;
        let output = parse_project("subrange_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("assignment to 'Value' value 11 is outside subrange 0..10")));
        let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
        let diagnostics = result.expect_err("runtime should reject subrange violation");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("value 11 is outside subrange 0..10")));
    }

    #[test]
    fn rejects_runtime_elementary_range_and_conversion_violations() {
        let elementary_source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 300;
                ByteValue : BYTE := 0;
            END_VAR
            ByteValue := Count;
            END_PROGRAM
        "#;
        let output = parse_project("elementary_range_runtime.st", elementary_source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
        let diagnostics = result.expect_err("runtime should reject BYTE range violation");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("value 300 is outside BYTE range 0..255")));

        let conversion_source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 300;
                Converted : USINT := 0;
            END_VAR
            Converted := INT_TO_USINT(Count);
            END_PROGRAM
        "#;
        let output = parse_project("conversion_range_runtime.st", conversion_source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
        let diagnostics = result.expect_err("runtime should reject conversion range violation");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'INT_TO_USINT' failed for supplied arguments")));
    }

    #[test]
    fn preserves_retain_variables_across_warm_restart() {
        let source = r#"
            PROGRAM Demo
            VAR RETAIN
                Kept : INT := 10;
            END_VAR
            VAR NON_RETAIN
                Reset : INT := 10;
            END_VAR
            VAR
                Plain : INT := 10;
            END_VAR

            Kept := Kept + 1;
            Reset := Reset + 1;
            Plain := Plain + 1;
            END_PROGRAM
        "#;
        let output = parse_project("retain_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let options = RuntimeOptions {
            warm_restart_before_cycles: vec![2],
            ..RuntimeOptions::default()
        };
        let trace =
            run_program(&output.project, Some("Demo"), 4, &options).expect("program should run");
        let before_restart = &trace.cycles[1].variables;
        let after_restart = &trace.cycles[2].variables;
        let last = &trace.cycles[3].variables;

        assert!(before_restart
            .iter()
            .any(|(name, value)| name == "KEPT" && *value == Value::Int(12)));
        assert!(after_restart
            .iter()
            .any(|(name, value)| name == "KEPT" && *value == Value::Int(13)));
        assert!(after_restart
            .iter()
            .any(|(name, value)| name == "RESET" && *value == Value::Int(11)));
        assert!(after_restart
            .iter()
            .any(|(name, value)| name == "PLAIN" && *value == Value::Int(11)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "KEPT" && *value == Value::Int(14)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "RESET" && *value == Value::Int(12)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "PLAIN" && *value == Value::Int(12)));
    }

    #[test]
    fn resets_program_var_temp_each_scan() {
        let source = r#"
            PROGRAM Demo
            VAR
                Total : INT := 0;
            END_VAR
            VAR_TEMP
                Scratch : INT := 5;
            END_VAR

            Scratch := Scratch + 1;
            Total := Total + Scratch;
            END_PROGRAM
        "#;
        let output = parse_project("var_temp_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 3, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TOTAL" && *value == Value::Int(18)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SCRATCH" && *value == Value::Int(6)));
    }

    #[test]
    fn executes_standard_counter_function_block() {
        let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                Pulse : BOOL := FALSE;
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            Pulse := NOT Pulse;
            Counter(CU := Pulse, R := FALSE, PV := 2);
            Count := Counter.CV;
            Done := Counter.Q;
            END_PROGRAM
        "#;
        let output = parse_project("fb_counter.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 3, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNTER.CV" && *value == Value::Int(2)));
    }

    #[test]
    fn executes_standard_function_block_positional_inputs() {
        let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                IlCounter : CTU;
                Count : INT := 0;
                IlCount : INT := 0;
                Done : BOOL := FALSE;
                IlDone : BOOL := FALSE;
            END_VAR

            Counter(TRUE, FALSE, 1);
            LD TRUE
            CAL IlCounter(TRUE, FALSE, 1)
            LD Counter.CV
            ST Count
            LD Counter.Q
            ST Done
            LD IlCounter.CV
            ST IlCount
            LD IlCounter.Q
            ST IlDone
            END_PROGRAM
        "#;
        let output = parse_project("fb_positional_inputs.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(1)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ILCOUNT" && *value == Value::Int(1)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ILDONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_function_block_en_eno_controls() {
        let source = r#"
            PROGRAM Demo
            VAR
                Disabled : CTU;
                Enabled : CTU;
                DisabledOk : BOOL := TRUE;
                EnabledOk : BOOL := FALSE;
            END_VAR

            Disabled(EN := FALSE, CU := TRUE, R := FALSE, PV := 1, ENO => DisabledOk);
            Enabled(EN := TRUE, CU := TRUE, R := FALSE, PV := 1, ENO => EnabledOk);
            END_PROGRAM
        "#;
        let output = parse_project("fb_controls.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DISABLED.CV" && *value == Value::Int(0)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ENABLED.CV" && *value == Value::Int(1)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DISABLEDOK" && *value == Value::Bool(false)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ENABLEDOK" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_communication_function_block_through_runtime_hook() {
        struct Hook;

        impl CommunicationHooks for Hook {
            fn execute(
                &self,
                invocation: &CommunicationInvocation,
            ) -> Option<CommunicationOutcome> {
                assert_eq!(invocation.block, "USEND");
                assert_eq!(invocation.instance, "SENDER");
                assert_eq!(invocation.inputs.get("REQ"), Some(&Value::Bool(true)));
                assert_eq!(invocation.inputs.get("ID"), Some(&Value::Int(7)));
                assert_eq!(invocation.inputs.get("LEN"), Some(&Value::Int(3)));
                Some(CommunicationOutcome {
                    outputs: BTreeMap::from([
                        ("done".to_string(), Value::Bool(true)),
                        ("error".to_string(), Value::Bool(false)),
                        ("status".to_string(), Value::Int(42)),
                    ]),
                })
            }
        }

        let source = r#"
            PROGRAM Demo
            VAR
                Sender : USEND;
                Done : BOOL := FALSE;
                Error : BOOL := TRUE;
                Status : INT := 0;
                Ok : BOOL := FALSE;
            END_VAR

            Sender(REQ := TRUE, ID := 7, LEN := 3, ENO => Ok);
            Done := Sender.DONE;
            Error := Sender.ERROR;
            Status := Sender.STATUS;
            END_PROGRAM
        "#;
        let output = parse_project("communication_hook.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let trace = run_program_with_communication_hooks(
            &output.project,
            Some("Demo"),
            1,
            &RuntimeOptions::default(),
            &Hook,
        )
        .expect("program should run with communication hook");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ERROR" && *value == Value::Bool(false)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "STATUS" && *value == Value::Int(42)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OK" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_user_defined_function_block_state() {
        let source = r#"
            FUNCTION_BLOCK Accumulator
            VAR_INPUT
                In : INT;
                Reset : BOOL;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            VAR
                Step : INT := 1;
            END_VAR

            IF Reset THEN
                Total := 0;
            ELSE
                Total := Total + In + Step;
            END_IF;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Acc : Accumulator;
                Out : INT := 0;
            END_VAR

            Acc(In := 2, Reset := FALSE, Total => Out);
            END_PROGRAM
        "#;
        let output = parse_project("user_fb.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ACC.TOTAL" && *value == Value::Int(6)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OUT" && *value == Value::Int(6)));
    }

    #[test]
    fn executes_user_function_block_positional_inputs_and_inouts() {
        let source = r#"
            FUNCTION_BLOCK Accumulate
            VAR_INPUT
                In : INT;
                Reset : BOOL;
            END_VAR
            VAR_IN_OUT
                Carry : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR

            IF Reset THEN
                Carry := 0;
            END_IF;
            Carry := Carry + In;
            Total := Carry;
            END_FUNCTION_BLOCK

            FUNCTION_BLOCK Wrapper
            VAR_INPUT
                In : INT;
                Reset : BOOL;
            END_VAR
            VAR_IN_OUT
                Carry : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            VAR
                Inner : Accumulate;
            END_VAR

            Inner(In, Reset, Carry, Total => Total);
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Block : Wrapper;
                Value : INT := 10;
                Out : INT := 0;
            END_VAR

            Block(2, FALSE, Value, Total => Out);
            END_PROGRAM
        "#;
        let output = parse_project("user_fb_positional.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "VALUE" && *value == Value::Int(12)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OUT" && *value == Value::Int(12)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "BLOCK.INNER.TOTAL" && *value == Value::Int(12)));
    }

    #[test]
    fn executes_user_function_block_input_edge_qualifiers() {
        let source = r#"
            FUNCTION_BLOCK EdgeCounter
            VAR_INPUT
                Start : BOOL R_EDGE;
                Stop : BOOL F_EDGE;
            END_VAR
            VAR_OUTPUT
                RiseCount : INT := 0;
                FallCount : INT := 0;
            END_VAR
            IF Start THEN
                RiseCount := RiseCount + 1;
            END_IF;
            IF Stop THEN
                FallCount := FallCount + 1;
            END_IF;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : EdgeCounter;
                Signal : BOOL := FALSE;
                Rises : INT := 0;
                Falls : INT := 0;
            END_VAR
            Fb(Start := Signal, Stop := Signal, RiseCount => Rises, FallCount => Falls);
            Signal := NOT Signal;
            END_PROGRAM
        "#;
        let output = parse_project("fb_edge_inputs_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 4, &RuntimeOptions::default())
            .expect("program should run");
        let last = &trace.cycles.last().unwrap().variables;
        assert!(last
            .iter()
            .any(|(name, value)| name == "RISES" && *value == Value::Int(2)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "FALLS" && *value == Value::Int(1)));
    }

    #[test]
    fn executes_user_function_block_return_control() {
        let source = r#"
            FUNCTION_BLOCK Gate
            VAR_INPUT
                Stop : BOOL;
            END_VAR
            VAR_OUTPUT
                Count : INT;
                Done : BOOL;
            END_VAR

            IF Stop THEN
                Done := TRUE;
                RETURN;
            END_IF;
            Count := Count + 1;
            Done := FALSE;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Gate;
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            Fb(Stop := TRUE, Count => Count, Done => Done);
            END_PROGRAM
        "#;
        let output = parse_project("user_fb_return.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(0)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_nested_user_defined_function_block_state() {
        let source = r#"
            FUNCTION_BLOCK Accumulator
            VAR_INPUT
                In : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            VAR
                Step : INT := 1;
            END_VAR
            Total := Total + In + Step;
            END_FUNCTION_BLOCK

            FUNCTION_BLOCK Wrapper
            VAR_INPUT
                In : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            VAR
                Inner : Accumulator;
            END_VAR
            Inner(In := In, Total => Total);
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Wrapper;
                Out : INT := 0;
                Mirror : INT := 0;
            END_VAR
            Fb(In := 2, Total => Out);
            Mirror := Fb.Inner.Total;
            END_PROGRAM
        "#;
        let output = parse_project("nested_user_fb_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FB.INNER.TOTAL" && *value == Value::Int(6)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FB.TOTAL" && *value == Value::Int(6)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OUT" && *value == Value::Int(6)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "MIRROR" && *value == Value::Int(6)));
    }

    #[test]
    fn executes_var_in_out_function_block_aliases() {
        let source = r#"
            FUNCTION_BLOCK Bump
            VAR_IN_OUT
                Value : INT;
            END_VAR
            Value := Value + 1;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Bump;
                Count : INT := 1;
            END_VAR

            Fb(Value := Count);
            END_PROGRAM
        "#;
        let output = parse_project("fb_inout.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(3)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FB.VALUE" && *value == Value::Int(3)));
    }

    #[test]
    fn rejects_non_variable_var_in_out_actuals() {
        let source = r#"
            FUNCTION_BLOCK Bump
            VAR_IN_OUT
                Value : INT;
            END_VAR
            Value := Value + 1;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Bump;
            END_VAR

            Fb(Value := 1);
            END_PROGRAM
        "#;
        let output = parse_project("bad_fb_inout.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("VAR_IN_OUT parameter 'Value' requires a variable actual")));
    }

    #[test]
    fn executes_bistable_and_edge_function_blocks() {
        let source = r#"
            PROGRAM Demo
            VAR
                Latch : SR;
                Edge : R_TRIG;
                Input : BOOL := FALSE;
                Latched : BOOL := FALSE;
                Rising : BOOL := FALSE;
            END_VAR

            Input := NOT Input;
            Latch(S1 := Input, R := FALSE);
            Edge(CLK := Input);
            Latched := Latch.Q1;
            Rising := Edge.Q;
            END_PROGRAM
        "#;
        let output = parse_project("fb_bits.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
            .expect("program should run");
        let first = &trace.cycles[0];
        assert!(first
            .variables
            .iter()
            .any(|(name, value)| name == "LATCH.Q1" && *value == Value::Bool(true)));
        assert!(first
            .variables
            .iter()
            .any(|(name, value)| name == "RISING" && *value == Value::Bool(true)));

        let second = &trace.cycles[1];
        assert!(second
            .variables
            .iter()
            .any(|(name, value)| name == "LATCH.Q1" && *value == Value::Bool(true)));
        assert!(second
            .variables
            .iter()
            .any(|(name, value)| name == "RISING" && *value == Value::Bool(false)));
    }

    #[test]
    fn executes_timer_function_blocks_with_cycle_time() {
        let source = r#"
            PROGRAM Demo
            VAR
                Delay : TON;
                Pulse : TP;
                Done : BOOL := FALSE;
                PulseDone : BOOL := FALSE;
                Elapsed : TIME := T#0ms;
            END_VAR

            Delay(IN := TRUE, PT := T#2ms);
            Pulse(IN := TRUE, PT := T#2ms);
            Done := Delay.Q;
            PulseDone := Pulse.Q;
            Elapsed := Delay.ET;
            END_PROGRAM
        "#;
        let output = parse_project("timers.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 3, &RuntimeOptions::default())
            .expect("program should run");

        let first = &trace.cycles[0];
        assert!(first
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(false)));
        assert!(first
            .variables
            .iter()
            .any(|(name, value)| name == "PULSEDONE" && *value == Value::Bool(true)));

        let second = &trace.cycles[1];
        assert!(second
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
        assert!(second
            .variables
            .iter()
            .any(|(name, value)| name == "ELAPSED" && *value == Value::TimeMs(2)));

        let third = &trace.cycles[2];
        assert!(third
            .variables
            .iter()
            .any(|(name, value)| name == "PULSEDONE" && *value == Value::Bool(false)));
    }

    #[test]
    fn executes_textual_sfc_scan_evolution() {
        let source = r#"
            PROGRAM Sequence
            VAR
                Ready : BOOL := TRUE;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start;
            STEP Run;
            TRANSITION Go := Ready;
            ACTION Run:
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("Sequence"),
            2,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let first = &trace.cycles[0].variables;
        assert!(first
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(false)));
        assert!(first
            .iter()
            .any(|(name, value)| name == "$SFC_STEP_RUN" && *value == Value::Bool(true)));
        let second = &trace.cycles[1].variables;
        assert!(second
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_textual_sfc_il_transition_body() {
        let source = r#"
            PROGRAM Sequence
            VAR
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start:
                Increment(N);
            END_STEP;
            STEP Run:
                Finish(N);
            END_STEP;
            TRANSITION FROM Start TO Run:
                LD Count
                GE 2
            END_TRANSITION;
            ACTION Increment:
                Count := Count + 1;
            END_ACTION;
            ACTION Finish:
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_il_transition_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);

        let trace = run_program(
            &output.project,
            Some("Sequence"),
            4,
            &RuntimeOptions::default(),
        )
        .expect("program should run");

        assert!(trace.cycles[0]
            .variables
            .iter()
            .any(|(name, value)| name == "$SFC_STEP_START" && *value == Value::Bool(true)));
        assert!(trace.cycles[1]
            .variables
            .iter()
            .any(|(name, value)| name == "$SFC_STEP_RUN" && *value == Value::Bool(true)));
        assert!(trace.cycles[2]
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_native_textual_ladder_body() {
        let source = r#"
            PROGRAM NativeLd
            VAR
                Start : BOOL := TRUE;
                Stop : BOOL := FALSE;
                Motor : BOOL := FALSE;
                Latched : BOOL := FALSE;
            END_VAR
            LADDER
            RUNG MotorRun:
                CONTACT Start;
                CONTACT_NOT Stop;
                COIL Motor;
            END_RUNG;
            RUNG Latch:
                CONTACT Start;
                SET Latched;
            END_RUNG;
            END_LADDER
            END_PROGRAM
        "#;
        let output = parse_project("native_ladder_runtime.ld", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);

        let trace = run_program(
            &output.project,
            Some("NativeLd"),
            1,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let cycle0 = &trace.cycles[0];
        assert!(cycle0
            .variables
            .iter()
            .any(|(name, value)| name == "MOTOR" && *value == Value::Bool(true)));
        assert!(cycle0
            .variables
            .iter()
            .any(|(name, value)| name == "LATCHED" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_native_textual_fbd_body() {
        let source = r#"
            PROGRAM NativeFbd
            VAR
                A : INT := 2;
                B : INT := 3;
                C : INT := 0;
                Ready : BOOL := FALSE;
            END_VAR
            FBD
            NETWORK Sum:
                OUT C := ADD(A, B);
                OUT Ready := C >= 5;
            END_NETWORK;
            END_FBD
            END_PROGRAM
        "#;
        let output = parse_project("native_fbd_runtime.fbd", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);

        let trace = run_program(
            &output.project,
            Some("NativeFbd"),
            1,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let cycle0 = &trace.cycles[0];
        assert!(cycle0
            .variables
            .iter()
            .any(|(name, value)| name == "C" && *value == Value::Int(5)));
        assert!(cycle0
            .variables
            .iter()
            .any(|(name, value)| name == "READY" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_native_ld_and_fbd_sfc_transition_bodies() {
        let ladder = r#"
            PROGRAM LdSequence
            VAR
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start:
                Increment(N);
            END_STEP;
            STEP Run:
                Finish(N);
            END_STEP;
            TRANSITION FROM Start TO Run:
                LADDER
                RUNG Ready:
                    CONTACT Count >= 2;
                END_RUNG;
                END_LADDER
            END_TRANSITION;
            ACTION Increment:
                Count := Count + 1;
            END_ACTION;
            ACTION Finish:
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_ld_transition_runtime.st", ladder);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("LdSequence"),
            4,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        assert!(trace.cycles[2]
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));

        let fbd = r#"
            PROGRAM FbdSequence
            VAR
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start:
                Increment(N);
            END_STEP;
            STEP Run:
                Finish(N);
            END_STEP;
            TRANSITION FROM Start TO Run:
                FBD
                NETWORK Ready:
                    OUT := Count >= 2;
                END_NETWORK;
                END_FBD
            END_TRANSITION;
            ACTION Increment:
                Count := Count + 1;
            END_ACTION;
            ACTION Finish:
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_fbd_transition_runtime.st", fbd);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("FbdSequence"),
            4,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        assert!(trace.cycles[2]
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_explicit_sfc_divergence_and_convergence() {
        let source = r#"
            PROGRAM Sequence
            VAR
                ACount : INT := 0;
                BCount : INT := 0;
                DoneCount : INT := 0;
            END_VAR

            INITIAL_STEP Start;
            STEP A;
            STEP B;
            STEP DoneStep;
            TRANSITION Split FROM Start TO (A, B) := TRUE;
            END_TRANSITION;
            TRANSITION Join FROM (A, B) TO DoneStep := TRUE;
            END_TRANSITION;
            ACTION A:
                ACount := ACount + 1;
            END_ACTION;
            ACTION B:
                BCount := BCount + 1;
            END_ACTION;
            ACTION DoneStep:
                DoneCount := DoneCount + 1;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_explicit_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("Sequence"),
            3,
            &RuntimeOptions::default(),
        )
        .expect("program should run");

        let first = &trace.cycles[0].variables;
        assert!(first
            .iter()
            .any(|(name, value)| name == "$SFC_STEP_A" && *value == Value::Bool(true)));
        assert!(first
            .iter()
            .any(|(name, value)| name == "$SFC_STEP_B" && *value == Value::Bool(true)));

        let second = &trace.cycles[1].variables;
        assert!(second
            .iter()
            .any(|(name, value)| name == "ACOUNT" && *value == Value::Int(1)));
        assert!(second
            .iter()
            .any(|(name, value)| name == "BCOUNT" && *value == Value::Int(1)));
        assert!(second
            .iter()
            .any(|(name, value)| name == "$SFC_STEP_DONESTEP" && *value == Value::Bool(true)));

        let third = &trace.cycles[2].variables;
        assert!(third
            .iter()
            .any(|(name, value)| name == "DONECOUNT" && *value == Value::Int(1)));
    }

    #[test]
    fn executes_sfc_transition_priority_conflicts() {
        let source = r#"
            PROGRAM Sequence
            VAR
                Selected : INT := 0;
            END_VAR

            INITIAL_STEP Start;
            STEP Low;
            STEP High;
            TRANSITION LowPriority (PRIORITY := 2) FROM Start TO Low := TRUE;
            END_TRANSITION;
            TRANSITION HighPriority (PRIORITY := 1) FROM Start TO High := TRUE;
            END_TRANSITION;
            ACTION Low:
                Selected := 1;
            END_ACTION;
            ACTION High:
                Selected := 2;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_priority_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("Sequence"),
            2,
            &RuntimeOptions::default(),
        )
        .expect("program should run");

        let first = &trace.cycles[0].variables;
        assert!(first
            .iter()
            .any(|(name, value)| name == "$SFC_STEP_LOW" && *value == Value::Bool(false)));
        assert!(first
            .iter()
            .any(|(name, value)| name == "$SFC_STEP_HIGH" && *value == Value::Bool(true)));

        let second = &trace.cycles[1].variables;
        assert!(second
            .iter()
            .any(|(name, value)| name == "SELECTED" && *value == Value::Int(2)));
    }

    #[test]
    fn executes_sfc_action_qualifiers_and_timers() {
        let source = r#"
            PROGRAM Qualifiers
            VAR
                PulseCount : INT := 0;
                DelayCount : INT := 0;
                LimitCount : INT := 0;
            END_VAR

            INITIAL_STEP Pulse;
            STEP Delay;
            STEP Limit;
            TRANSITION ToDelay := PulseCount >= 1;
            TRANSITION ToLimit := DelayCount >= 1;
            ACTION Pulse(P):
                PulseCount := PulseCount + 1;
            END_ACTION;
            ACTION Delay(D, T#2ms):
                DelayCount := DelayCount + 1;
            END_ACTION;
            ACTION Limit(L, T#2ms):
                LimitCount := LimitCount + 1;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_qualifiers.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("Qualifiers"),
            6,
            &RuntimeOptions {
                cycle_time_ms: 1,
                ..RuntimeOptions::default()
            },
        )
        .expect("program should run");
        let last = &trace.cycles.last().unwrap().variables;
        assert!(last
            .iter()
            .any(|(name, value)| name == "PULSECOUNT" && *value == Value::Int(1)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "DELAYCOUNT" && *value == Value::Int(1)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "LIMITCOUNT" && *value == Value::Int(2)));
    }

    #[test]
    fn executes_sfc_step_action_associations() {
        let source = r#"
            PROGRAM Sequence
            VAR
                Count : INT := 0;
                PulseCount : INT := 0;
                DelayCount : INT := 0;
            END_VAR

            INITIAL_STEP Start:
                CountAction(N);
                PulseAction(P);
            END_STEP;
            Running: STEP
                DelayAction(D, T#2ms);
            END_STEP;
            ToRun: TRANSITION FROM Start TO Running := Count >= 2;
            END_TRANSITION;
            CountAction: ACTION
                Count := Count + 1;
            END_ACTION;
            PulseAction: ACTION
                PulseCount := PulseCount + 1;
            END_ACTION;
            DelayAction: ACTION
                DelayCount := DelayCount + 1;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_associations.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("Sequence"),
            5,
            &RuntimeOptions {
                cycle_time_ms: 1,
                ..RuntimeOptions::default()
            },
        )
        .expect("program should run");
        let last = &trace.cycles.last().unwrap().variables;
        assert!(last
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "PULSECOUNT" && *value == Value::Int(1)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "DELAYCOUNT" && *value == Value::Int(2)));
    }

    #[test]
    fn executes_sfc_action_control_set_reset_across_steps() {
        let source = r#"
            PROGRAM Sequence
            VAR
                Count : INT := 0;
            END_VAR

            INITIAL_STEP SetStep:
                Shared(S);
            END_STEP;
            STEP ResetStep:
                Shared(R);
            END_STEP;
            TRANSITION LeaveSet FROM SetStep TO ResetStep := Count >= 2;
            END_TRANSITION;
            Shared: ACTION
                Count := Count + 1;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_action_control_reset.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("Sequence"),
            4,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let last = &trace.cycles.last().unwrap().variables;
        assert!(last
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
        assert!(last
            .iter()
            .any(|(name, value)| { name == "$SFC_ACTION_SHARED" && *value == Value::Bool(false) }));
    }

    #[test]
    fn executes_sfc_falling_pulse_action_qualifier() {
        let source = r#"
            PROGRAM Sequence
            VAR
                ExitCount : INT := 0;
            END_VAR
            INITIAL_STEP RunExit;
            STEP Done;
            TRANSITION Leave := TRUE;
            ACTION RunExit(P0):
                ExitCount := ExitCount + 1;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_p0.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("Sequence"),
            2,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let first = &trace.cycles[0].variables;
        let second = &trace.cycles[1].variables;
        assert!(first
            .iter()
            .any(|(name, value)| name == "EXITCOUNT" && *value == Value::Int(0)));
        assert!(second
            .iter()
            .any(|(name, value)| name == "EXITCOUNT" && *value == Value::Int(1)));
    }

    #[test]
    fn executes_basic_instruction_list() {
        let source = r#"
            PROGRAM IlDemo
            VAR
                A : INT := 3;
                B : INT := 4;
                C : INT := 0;
                Bigger : BOOL := FALSE;
                Complex : BOOL := FALSE;
            END_VAR

            LD A
            ADD B
            ST C
            GT 5
            ST Bigger
            LD TRUE
            AND (Bigger OR FALSE)
            ST Complex
            END_PROGRAM
        "#;
        let output = parse_project("il.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("IlDemo"),
            1,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "C" && *value == Value::Int(7)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "BIGGER" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COMPLEX" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_typed_instruction_list_operators() {
        let source = r#"
            PROGRAM TypedIlDemo
            VAR
                A : INT := 3;
                B : INT := 4;
                C : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            LD_INT A;
            ADD_INT B;
            ST_INT C;
            LD_BOOL TRUE;
            AND_BOOL (C = 7);
            ST_BOOL Flag;
            END_PROGRAM
        "#;
        let output = parse_project("typed_il_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("TypedIlDemo"),
            1,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "C" && *value == Value::Int(7)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_instruction_list_parenthesized_expression_lists() {
        let source = r#"
            PROGRAM NestedIlDemo
            VAR
                A : INT := 3;
                B : INT := 4;
                C : INT := 2;
                Total : INT := 0;
                Good : BOOL := FALSE;
            END_VAR

            LD A;
            ADD (
                LD B;
                MUL (
                    LD C;
                    ADD 1;
                );
            );
            ST Total;
            LD TRUE;
            AND (
                LD Total;
                EQ 15;
            );
            ST Good;
            END_PROGRAM
        "#;
        let output = parse_project("nested_il_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("NestedIlDemo"),
            1,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TOTAL" && *value == Value::Int(15)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "GOOD" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_instruction_list_jumps() {
        let source = r#"
            PROGRAM IlJumpDemo
            VAR
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            LD Count;
            GE 3;
            JMPC DoneLabel;
            LD Count;
            ADD 1;
            ST Count;
            JMP EndLabel;
            DoneLabel:
            LD TRUE;
            ST Done;
            EndLabel:
            END_PROGRAM
        "#;
        let output = parse_project("il_jump.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("IlJumpDemo"),
            4,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(3)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_instruction_list_calls_and_conditional_returns() {
        let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                Done : BOOL := FALSE;
                Cv : INT := 0;
                Skipped : INT := 0;
            END_VAR

            LD TRUE;
            CALC Counter(CU := TRUE, R := FALSE, PV := 1);
            LD Counter.Q;
            ST Done;
            LD Counter.CV;
            ST Cv;
            LD TRUE;
            RETC;
            Skipped := 1;
            END_PROGRAM
        "#;
        let output = parse_project("il_calls.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "CV" && *value == Value::Int(1)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SKIPPED" && *value == Value::Int(0)));
    }

    #[test]
    fn executes_instruction_list_simple_and_negated_call_forms() {
        let source = r#"
            PROGRAM Demo
            VAR
                CountUp : CTU;
                Skipped : CTU;
                Done : BOOL := FALSE;
                SkippedCv : INT := 0;
            END_VAR

            CountUp(CU := TRUE, R := FALSE, PV := 2);
            LD FALSE;
            CALCN CountUp;
            LD TRUE;
            CALCN Skipped(CU := TRUE, R := FALSE, PV := 1);
            LD CountUp.Q;
            ST Done;
            LD Skipped.CV;
            ST SkippedCv;
            END_PROGRAM
        "#;
        let output = parse_project("il_call_forms.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SKIPPEDCV" && *value == Value::Int(0)));
    }

    #[test]
    fn executes_instruction_list_user_fb_positional_call() {
        let source = r#"
            FUNCTION_BLOCK Accumulate
            VAR_INPUT
                In : INT;
                Reset : BOOL;
            END_VAR
            VAR_IN_OUT
                Carry : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR

            IF Reset THEN
                Carry := 0;
            END_IF;
            Carry := Carry + In;
            Total := Carry;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Accumulate;
                Value : INT := 10;
                Out : INT := 0;
            END_VAR

            LD TRUE
            CAL Fb(2, FALSE, Value, Total => Out)
            END_PROGRAM
        "#;
        let output = parse_project("il_user_fb_positional.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "VALUE" && *value == Value::Int(12)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OUT" && *value == Value::Int(12)));
    }
}
