// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_stdlib::{
    eval_standard_function, is_communication_function_block, is_standard_function,
    is_standard_void_function, standard_function_input_index,
};

use crate::runtime::*;
use crate::state::*;
use crate::support::*;
use crate::*;

pub(crate) fn apply_program_instance_args(runtime: &mut Runtime<'_>, instance: &ProgramInstance) {
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

pub(crate) fn program_instance_output_bindings(
    instance: &ProgramInstance,
) -> Vec<ProgramOutputBinding> {
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

pub(crate) fn collect_program_instance_output_writes(
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

pub(crate) fn apply_program_instance_output_writes(
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

pub(crate) fn assign_configuration_output_variable_target(
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

pub(crate) fn assign_resource_output_variable_target(
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

pub(crate) fn assign_global_state_variable_target(
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

pub(crate) fn strip_variable_root(variable: &VariableRef) -> Option<VariableRef> {
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

pub(crate) fn output_binding_access_target(variable: &VariableRef) -> Option<String> {
    if variable.indices.iter().any(|indices| !indices.is_empty()) {
        return None;
    }
    variable
        .direct
        .clone()
        .or_else(|| (!variable.path.is_empty()).then(|| variable.to_string()))
}

pub(crate) fn find_program<'a>(
    project: &'a Project,
    program_name: Option<&str>,
) -> Option<&'a Pou> {
    if let Some(name) = program_name {
        project
            .find_pou(name)
            .filter(|pou| matches!(&pou.kind, PouKind::Program))
    } else {
        project.first_program()
    }
}

pub(crate) fn project_global_var_decls<'a>(
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

pub(crate) fn find_configuration<'a>(
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

pub(crate) fn task_interval_ms(task: Option<&Task>, default_cycle_time_ms: i128) -> Option<i128> {
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

pub(crate) fn task_priority(task: Option<&Task>) -> u32 {
    let Some(priority) = task.and_then(|task| task.priority.as_ref()) else {
        return u32::MAX;
    };
    match priority {
        Expr::Literal(Literal::Int(value)) => u32::try_from(*value).unwrap_or(u32::MAX),
        _ => u32::MAX,
    }
}

pub(crate) fn scheduled_task_single_due(
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

pub(crate) fn task_event_environment(
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
pub(crate) struct AccessPathDeclaration {
    pub(crate) name: String,
    pub(crate) target: String,
    pub(crate) direction: AccessDirection,
    pub(crate) type_spec: DataTypeSpec,
}

pub(crate) fn access_declarations(blocks: &[VarBlock]) -> Vec<AccessPathDeclaration> {
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

pub(crate) fn variable_ref_from_access_path(path: &str) -> Option<VariableRef> {
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

pub(crate) fn is_symbolic_access_part(part: &str) -> bool {
    let mut chars = part.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && chars.all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
}

pub(crate) fn is_direct_location_key(location: &str) -> bool {
    location.trim_start().starts_with('%')
}

pub(crate) fn configuration_access_snapshot(
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

pub(crate) fn configuration_access_value(
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

pub(crate) fn resource_access_value(
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

pub(crate) fn access_path_parts(path: &str) -> Option<Vec<String>> {
    let parts = path
        .split('.')
        .map(str::trim)
        .map(|part| is_symbolic_access_part(part).then(|| canonical_identifier(part)))
        .collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then_some(parts)
}
