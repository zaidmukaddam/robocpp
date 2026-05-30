// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::fmt::{self, Write};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_profile::ImplementationParameters;
use iec_stdlib::{is_standard_function, standard_function_input_index};

use crate::expressions::*;
use crate::fb::*;
use crate::functions::*;
use crate::state::*;
use crate::*;

pub(crate) fn binary_op_to_c_state(
    op: BinaryOp,
    left: &Expr,
    right: &Expr,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> &'static str {
    let bool_operands = expr_is_bool_for_c(left, var_types, project)
        && expr_is_bool_for_c(right, var_types, project);
    match op {
        BinaryOp::Or if bool_operands => "||",
        BinaryOp::Xor if bool_operands => "!=",
        BinaryOp::And if bool_operands => "&&",
        _ => binary_op_to_c(op),
    }
}

pub(crate) fn var_to_c(variable: &VariableRef) -> String {
    if let Some(direct) = &variable.direct {
        sanitize_c_ident(direct)
    } else {
        let mut text = variable
            .path
            .iter()
            .map(|part| sanitize_c_ident(&part.original))
            .collect::<Vec<_>>()
            .join("_");
        for indices in &variable.indices {
            for index in indices {
                c_write!(text, "[({}) - 1]", expr_to_c(index));
            }
        }
        text
    }
}

pub(crate) fn variable_spec(
    variable: &VariableRef,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> Option<DataTypeSpec> {
    if variable.direct.is_some() {
        return None;
    }
    let root = variable.root_name()?;
    let mut spec = var_types.get(&root.canonical)?.clone();
    spec = apply_indices_to_spec(
        spec,
        variable.indices.first().map(Vec::as_slice).unwrap_or(&[]),
        project,
    )?;

    for (segment_index, segment) in variable.path.iter().enumerate().skip(1) {
        let DataTypeSpec::Struct { fields } = resolve_named_spec(project, &spec) else {
            return None;
        };
        let field = fields
            .iter()
            .find(|field| field.name.canonical == segment.canonical)?;
        spec = apply_indices_to_spec(
            field.spec.clone(),
            variable
                .indices
                .get(segment_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            project,
        )?;
    }
    Some(spec)
}

pub(crate) fn expr_is_bool_for_c(
    expr: &Expr,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> bool {
    match expr {
        Expr::Literal(Literal::Bool(_)) => true,
        Expr::Variable(variable) => {
            variable_spec(variable, var_types, project).is_some_and(|spec| {
                matches!(
                    resolve_named_spec(project, &spec),
                    DataTypeSpec::Elementary(ElementaryType::Bool)
                )
            })
        }
        Expr::Unary {
            op: UnaryOp::Not,
            expr,
        } => expr_is_bool_for_c(expr, var_types, project),
        Expr::Binary { op, left, right } => match op {
            BinaryOp::Equal
            | BinaryOp::NotEqual
            | BinaryOp::Less
            | BinaryOp::LessEqual
            | BinaryOp::Greater
            | BinaryOp::GreaterEqual => true,
            BinaryOp::And | BinaryOp::Or | BinaryOp::Xor => {
                expr_is_bool_for_c(left, var_types, project)
                    && expr_is_bool_for_c(right, var_types, project)
            }
            _ => false,
        },
        Expr::Call { name, args } => {
            matches!(
                name.canonical.as_str(),
                "GT" | "GE" | "EQ" | "NE" | "LE" | "LT" | "INT_TO_BOOL" | "STRING_TO_BOOL"
            ) || name.canonical.ends_with("_TO_BOOL")
                || (name.canonical == "NOT"
                    && first_ordered_standard_input_expr(&name.original, args)
                        .is_some_and(|expr| expr_is_bool_for_c(expr, var_types, project)))
        }
        _ => false,
    }
}

pub(crate) fn apply_indices_to_spec(
    spec: DataTypeSpec,
    indices: &[Expr],
    project: &Project,
) -> Option<DataTypeSpec> {
    if indices.is_empty() {
        return Some(spec);
    }
    let mut current = resolve_named_spec(project, &spec);
    let mut consumed = 0_usize;
    loop {
        let DataTypeSpec::Array {
            ranges,
            element_type,
        } = current
        else {
            return None;
        };
        consumed += ranges.len();
        current = *element_type;
        if consumed >= indices.len() {
            return Some(current);
        }
        current = resolve_named_spec(project, &current);
    }
}

pub(crate) fn sfc_step_field(step: &Identifier) -> String {
    format!("rbcpp_sfc_step_{}", sanitize_c_ident(&step.original))
}

pub(crate) fn sfc_transition_steps<'a>(
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

pub(crate) fn sfc_transition_application_order(sfc: &Sfc) -> Vec<usize> {
    let mut order = sfc
        .transitions
        .iter()
        .enumerate()
        .map(|(index, transition)| (transition.priority.unwrap_or(i64::MAX), index))
        .collect::<Vec<_>>();
    order.sort_by_key(|(priority, index)| (*priority, *index));
    order.into_iter().map(|(_, index)| index).collect()
}

pub(crate) struct SfcActionControl<'a> {
    pub(crate) key: String,
    pub(crate) action: &'a SfcAction,
    pub(crate) inputs: Vec<SfcActionControlInput<'a>>,
}

pub(crate) struct SfcActionControlInput<'a> {
    pub(crate) qualifier: SfcActionQualifier,
    pub(crate) duration: Option<&'a Literal>,
    pub(crate) active_step: Option<&'a Identifier>,
}

pub(crate) fn sfc_action_controls(sfc: &Sfc) -> Vec<SfcActionControl<'_>> {
    let mut controls = Vec::new();
    for action in &sfc.actions {
        let mut inputs = Vec::new();
        for step in &sfc.steps {
            for association in &step.actions {
                if association.name.canonical != action.name.canonical {
                    continue;
                }
                inputs.push(SfcActionControlInput {
                    qualifier: association.qualifier.unwrap_or(action.qualifier),
                    duration: association.duration.as_ref().or(action.duration.as_ref()),
                    active_step: Some(&step.name),
                });
            }
        }
        if inputs.is_empty() {
            let active_step = sfc
                .steps
                .iter()
                .find(|step| step.name.canonical == action.name.canonical)
                .map(|step| &step.name);
            if active_step.is_none() {
                continue;
            }
            inputs.push(SfcActionControlInput {
                qualifier: action.qualifier,
                duration: action.duration.as_ref(),
                active_step,
            });
        }
        controls.push(SfcActionControl {
            key: sfc_action_control_key(&action.name),
            action,
            inputs,
        });
    }
    controls
}

pub(crate) fn sfc_action_control_keys(sfc: &Sfc) -> Vec<String> {
    sfc_action_controls(sfc)
        .into_iter()
        .map(|control| control.key)
        .collect()
}

pub(crate) fn sfc_action_control_qualifier_label(control: &SfcActionControl<'_>) -> String {
    let mut qualifiers = control
        .inputs
        .iter()
        .map(|input| input.qualifier.as_iec())
        .collect::<Vec<_>>();
    qualifiers.sort_unstable();
    qualifiers.dedup();
    qualifiers.join("_")
}

pub(crate) fn sfc_action_control_key(action: &Identifier) -> String {
    action.canonical.clone()
}

pub(crate) fn sfc_action_field_from_key(key: &str) -> String {
    format!("rbcpp_sfc_action_{}", sanitize_c_ident(key))
}

pub(crate) fn sfc_action_previous_field_from_key(key: &str) -> String {
    format!("rbcpp_sfc_action_previous_{}", sanitize_c_ident(key))
}

pub(crate) fn sfc_action_elapsed_field_from_key(key: &str) -> String {
    format!("rbcpp_sfc_action_elapsed_{}", sanitize_c_ident(key))
}

pub(crate) fn sfc_action_duration_ms(duration: Option<&Literal>) -> i128 {
    match duration {
        Some(Literal::DurationMs(value)) => (*value).max(0),
        Some(Literal::Int(value)) => (*value as i128).max(0),
        _ => 0,
    }
}

pub(crate) fn var_to_c_state(
    variable: &VariableRef,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> String {
    if let Some(direct) = &variable.direct {
        return sanitize_c_ident(direct);
    }
    let Some(root) = variable.root_name() else {
        return "_".to_string();
    };
    let Some(root_spec) = var_types.get(&root.canonical) else {
        return var_to_c(variable);
    };

    if variable.path.len() > 1
        && (standard_fb_fields(root_spec).is_some()
            || user_function_block(project, root_spec).is_some())
    {
        return var_to_c(variable);
    }

    let mut text = sanitize_c_ident(&root.original);
    let mut current_spec = root_spec.clone();
    current_spec = append_indices_to_c_state(
        &mut text,
        &current_spec,
        variable.indices.first().map(Vec::as_slice).unwrap_or(&[]),
        var_types,
        project,
    );

    for (segment_index, segment) in variable.path.iter().enumerate().skip(1) {
        current_spec = resolve_named_spec(project, &current_spec);
        let DataTypeSpec::Struct { fields } = current_spec else {
            text.push('_');
            text.push_str(&sanitize_c_ident(&segment.original));
            continue;
        };
        let field_spec = fields
            .iter()
            .find(|field| field.name.canonical == segment.canonical)
            .map(|field| field.spec.clone())
            .unwrap_or_else(|| DataTypeSpec::Elementary(ElementaryType::Int));
        text.push('.');
        text.push_str(&sanitize_c_ident(&segment.original));
        current_spec = append_indices_to_c_state(
            &mut text,
            &field_spec,
            variable
                .indices
                .get(segment_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            var_types,
            project,
        );
    }
    text
}

pub(crate) fn append_indices_to_c_state(
    out: &mut String,
    spec: &DataTypeSpec,
    indices: &[Expr],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> DataTypeSpec {
    append_indices_to_c(out, spec, indices, project, &|expr| {
        expr_to_c_state(expr, var_types, project)
    })
}

pub(crate) fn append_indices_to_c_local(
    out: &mut String,
    spec: &DataTypeSpec,
    indices: &[Expr],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> DataTypeSpec {
    append_indices_to_c(out, spec, indices, project, &|expr| {
        expr_to_c_local_typed(expr, var_types, project)
    })
}

pub(crate) fn append_indices_to_c(
    out: &mut String,
    spec: &DataTypeSpec,
    indices: &[Expr],
    project: &Project,
    render_index: &dyn Fn(&Expr) -> String,
) -> DataTypeSpec {
    if indices.is_empty() {
        return spec.clone();
    }

    let mut current_spec = resolve_named_spec(project, spec);
    let mut index_iter = indices.iter();
    loop {
        current_spec = resolve_named_spec(project, &current_spec);
        match current_spec {
            DataTypeSpec::Array {
                ranges,
                element_type,
            } => {
                for range in ranges {
                    let Some(index) = index_iter.next() else {
                        return DataTypeSpec::Array {
                            ranges: vec![range],
                            element_type,
                        };
                    };
                    let index_c = render_index(index);
                    c_write!(out, "[({index_c}) - {}]", range.low);
                }
                current_spec = *element_type;
                if index_iter.as_slice().is_empty() {
                    return current_spec;
                }
            }
            _ => {
                for index in index_iter {
                    let index_c = render_index(index);
                    c_write!(out, "[({index_c}) - 1]");
                }
                return current_spec;
            }
        }
    }
}

pub(crate) fn local_var_to_c(variable: &VariableRef) -> String {
    var_to_c(variable)
}

pub(crate) fn local_var_to_c_typed(
    variable: &VariableRef,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> String {
    local_var_to_c_typed_inner(variable, var_types, project, None)
}

pub(crate) fn function_local_var_to_c(
    variable: &VariableRef,
    context: &FunctionCContext<'_>,
) -> String {
    local_var_to_c_typed_inner(
        variable,
        &context.var_types,
        context.project,
        context.array_return_name.as_ref(),
    )
}

pub(crate) fn local_var_to_c_typed_inner(
    variable: &VariableRef,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
    array_return_name: Option<&String>,
) -> String {
    if let Some(direct) = &variable.direct {
        return sanitize_c_ident(direct);
    }
    let Some(root) = variable.root_name() else {
        return "_".to_string();
    };
    let Some(root_spec) = var_types.get(&root.canonical) else {
        return local_var_to_c(variable);
    };

    let mut text = sanitize_c_ident(&root.original);
    if array_return_name.is_some_and(|name| name == &root.canonical) {
        text.push_str(".value");
    }

    let mut current_spec = root_spec.clone();
    current_spec = append_indices_to_c_local(
        &mut text,
        &current_spec,
        variable.indices.first().map(Vec::as_slice).unwrap_or(&[]),
        var_types,
        project,
    );

    for (segment_index, segment) in variable.path.iter().enumerate().skip(1) {
        current_spec = resolve_named_spec(project, &current_spec);
        let DataTypeSpec::Struct { fields } = current_spec else {
            text.push('_');
            text.push_str(&sanitize_c_ident(&segment.original));
            continue;
        };
        let field_spec = fields
            .iter()
            .find(|field| field.name.canonical == segment.canonical)
            .map(|field| field.spec.clone())
            .unwrap_or_else(|| DataTypeSpec::Elementary(ElementaryType::Int));
        text.push('.');
        text.push_str(&sanitize_c_ident(&segment.original));
        current_spec = append_indices_to_c_local(
            &mut text,
            &field_spec,
            variable
                .indices
                .get(segment_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            var_types,
            project,
        );
    }

    text
}

pub(crate) fn il_call_operand(expr: &Expr) -> Option<(VariableRef, Vec<ParamAssignment>)> {
    match expr {
        Expr::Call { name, args } => {
            Some((VariableRef::named(name.original.clone()), args.clone()))
        }
        Expr::Variable(variable) => Some((variable.clone(), Vec::new())),
        _ => None,
    }
}

pub(crate) fn il_label_operand(expr: &Expr) -> Option<&Identifier> {
    let Expr::Variable(variable) = expr else {
        return None;
    };
    if variable.direct.is_some() || variable.path.len() != 1 {
        return None;
    }
    variable.root_name()
}

pub(crate) fn il_label_to_c(label: &Identifier) -> String {
    format!("rbcpp_label_{}", sanitize_c_ident(&label.original))
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct FbField {
    pub(crate) name: &'static str,
    pub(crate) c_type: &'static str,
    pub(crate) default: &'static str,
}

pub(crate) fn standard_fb_fields(spec: &DataTypeSpec) -> Option<&'static [FbField]> {
    let DataTypeSpec::Named(type_name) = spec else {
        return None;
    };

    match type_name.canonical.as_str() {
        "SR" | "RS" => Some(&[FbField {
            name: "Q1",
            c_type: "bool",
            default: "false",
        }]),
        "R_TRIG" | "F_TRIG" => Some(&[
            FbField {
                name: "Q",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "M",
                c_type: "bool",
                default: "false",
            },
        ]),
        "CTU" => Some(&[
            FbField {
                name: "Q",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "CV",
                c_type: "int64_t",
                default: "0",
            },
            FbField {
                name: "_CU",
                c_type: "bool",
                default: "false",
            },
        ]),
        "CTD" => Some(&[
            FbField {
                name: "Q",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "CV",
                c_type: "int64_t",
                default: "0",
            },
            FbField {
                name: "_CD",
                c_type: "bool",
                default: "false",
            },
        ]),
        "CTUD" => Some(&[
            FbField {
                name: "QU",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "QD",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "CV",
                c_type: "int64_t",
                default: "0",
            },
            FbField {
                name: "_CU",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "_CD",
                c_type: "bool",
                default: "false",
            },
        ]),
        "TON" | "TOF" | "TP" => Some(&[
            FbField {
                name: "Q",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "ET",
                c_type: "int64_t",
                default: "0",
            },
            FbField {
                name: "_IN",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "_RUN",
                c_type: "bool",
                default: "false",
            },
        ]),
        name if is_communication_fb_name(name) => Some(&[
            FbField {
                name: "DONE",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "NDR",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "ERROR",
                c_type: "bool",
                default: "false",
            },
            FbField {
                name: "STATUS",
                c_type: "int64_t",
                default: "0",
            },
        ]),
        _ => None,
    }
}

pub(crate) fn standard_fb_input_names(block: &str) -> &'static [&'static str] {
    match canonical_identifier(block).as_str() {
        "SR" => &["S1", "R"],
        "RS" => &["S", "R1"],
        "R_TRIG" | "F_TRIG" => &["CLK"],
        "CTU" => &["CU", "R", "PV"],
        "CTD" => &["CD", "LD", "PV"],
        "CTUD" => &["CU", "CD", "R", "LD", "PV"],
        "TON" | "TOF" | "TP" => &["IN", "PT"],
        name if is_communication_fb_name(name) => &["REQ", "EN_R", "ID", "LEN"],
        _ => &[],
    }
}

pub(crate) fn fb_field_ident(instance: &str, field: &str) -> String {
    format!("{}_{}", sanitize_c_ident(instance), sanitize_c_ident(field))
}

pub(crate) fn field_key_for_c(instance: &str, field: &str) -> String {
    format!("{instance}.{field}")
}

pub(crate) fn c_type(spec: &DataTypeSpec) -> &'static str {
    match spec {
        DataTypeSpec::Elementary(ElementaryType::Bool) => "bool",
        DataTypeSpec::Elementary(ElementaryType::Real | ElementaryType::Lreal) => "double",
        DataTypeSpec::Elementary(
            ElementaryType::Sint
            | ElementaryType::Int
            | ElementaryType::Dint
            | ElementaryType::Lint
            | ElementaryType::Usint
            | ElementaryType::Uint
            | ElementaryType::Udint
            | ElementaryType::Ulint,
        )
        | DataTypeSpec::Subrange { .. } => "int64_t",
        DataTypeSpec::Elementary(
            ElementaryType::Byte
            | ElementaryType::Word
            | ElementaryType::Dword
            | ElementaryType::Lword,
        ) => "uint64_t",
        DataTypeSpec::Elementary(
            ElementaryType::String
            | ElementaryType::WString
            | ElementaryType::Time
            | ElementaryType::Date
            | ElementaryType::TimeOfDay
            | ElementaryType::DateAndTime,
        )
        | DataTypeSpec::String { .. }
        | DataTypeSpec::Named(_)
        | DataTypeSpec::Array { .. }
        | DataTypeSpec::Struct { .. }
        | DataTypeSpec::Enum { .. } => "int64_t",
    }
}

pub(crate) fn function_c_type(project: &Project, spec: &DataTypeSpec) -> String {
    if let Some(type_ident) = named_struct_type_ident(project, spec) {
        return type_ident;
    }
    if let Some(type_ident) = named_array_type_ident(project, spec) {
        return type_ident;
    }
    if c_text_info(project, spec).is_some() {
        return "const char *".to_string();
    }
    c_type(&resolve_named_spec(project, spec)).to_string()
}

pub(crate) fn function_parameter_decl(project: &Project, var: &VarDecl) -> String {
    let name = sanitize_c_ident(&var.name.original);
    if matches!(
        resolve_named_spec(project, &var.type_spec),
        DataTypeSpec::Array { .. }
    ) {
        return c_parameter_declaration(project, &name, &var.type_spec);
    }
    format!("{} {name}", function_c_type(project, &var.type_spec))
}

pub(crate) fn c_parameter_declaration(
    project: &Project,
    name: &str,
    spec: &DataTypeSpec,
) -> String {
    let (base, dimensions) = peel_array_dimensions(project, spec);
    let dimensions = dimensions_to_c(&dimensions);

    if let Some(info) = c_text_info(project, &base) {
        let element_type = if info.wide { "uint32_t" } else { "char" };
        return format!("{element_type} {name}{dimensions}[{}]", info.capacity);
    }

    if let Some(type_ident) = named_struct_type_ident(project, &base) {
        return format!("{type_ident} {name}{dimensions}");
    }

    format!(
        "{} {name}{dimensions}",
        c_storage_type(project, &resolve_named_spec(project, &base))
    )
}

pub(crate) fn c_default(spec: &DataTypeSpec) -> &'static str {
    match spec {
        DataTypeSpec::Elementary(ElementaryType::Bool) => "false",
        _ => "0",
    }
}

pub(crate) fn sanitize_c_ident(input: &str) -> String {
    let mut out = String::new();
    for (index, ch) in input.chars().enumerate() {
        let valid = ch.is_ascii_alphanumeric() || ch == '_';
        if index == 0 && ch.is_ascii_digit() {
            out.push('_');
        }
        out.push(if valid { ch.to_ascii_lowercase() } else { '_' });
    }
    if out.is_empty() {
        "_".to_string()
    } else {
        out
    }
}
