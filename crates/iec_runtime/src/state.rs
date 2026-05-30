// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_stdlib::{
    eval_standard_function, is_communication_function_block, is_standard_function,
    is_standard_void_function, standard_function_input_index,
};

use crate::configuration::*;
use crate::runtime::*;
use crate::support::*;
use crate::*;

pub(crate) struct GlobalState {
    pub(crate) values: BTreeMap<String, Value>,
    pub(crate) types: BTreeMap<String, DataTypeSpec>,
}

impl GlobalState {
    pub(crate) fn from_blocks(project: &Project, blocks: &[VarBlock]) -> Self {
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

    pub(crate) fn access_value(&self, project: &Project, parts: &[String]) -> Option<Value> {
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

    pub(crate) fn assign(&mut self, project: &Project, parts: &[String], value: Value) -> bool {
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

pub(crate) fn apply_configuration_access_writes(
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
pub(crate) struct ConfigurationAccessBinding<'a> {
    pub(crate) qualified_name: String,
    pub(crate) qualified_canonical: String,
    pub(crate) short_canonical: String,
    pub(crate) target: String,
    pub(crate) direction: AccessDirection,
    pub(crate) type_spec: DataTypeSpec,
    pub(crate) resource_context: Option<&'a Resource>,
}

pub(crate) fn configuration_access_bindings(
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

pub(crate) fn assign_configuration_access_target(
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

pub(crate) fn assign_resource_access_target(
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

pub(crate) fn global_initial_value_for_spec(
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

pub(crate) fn assign_into_global_value(
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

pub(crate) fn expr_literal_value(project: &Project, expr: &Expr) -> Option<Value> {
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

pub(crate) fn literal_standard_call_args(
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

pub(crate) fn resolve_project_spec(project: &Project, spec: &DataTypeSpec) -> DataTypeSpec {
    resolve_project_spec_inner(project, spec, &mut BTreeSet::new())
}

pub(crate) fn resolve_project_spec_inner(
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

pub(crate) fn enum_ordinal_expr(project: &Project, expr: &Expr) -> Option<i64> {
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

pub(crate) fn enum_ordinal_typed(
    project: &Project,
    type_name: &Identifier,
    value_name: &str,
) -> Option<i64> {
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

pub(crate) fn enum_ordinal_name(project: &Project, canonical_name: &str) -> Option<i64> {
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

pub(crate) fn runtime_value_matches_project_spec(
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
