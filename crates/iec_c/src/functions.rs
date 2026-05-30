// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::fmt::{self, Write};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_profile::ImplementationParameters;
use iec_stdlib::{is_standard_function, standard_function_input_index};

use crate::addressing::*;
use crate::expressions::*;
use crate::fb::*;
use crate::state::*;
use crate::*;

pub(crate) fn emit_function_pou(out: &mut CEmitter<'_>, function: &Pou, project: &Project) {
    let PouKind::Function { return_type } = &function.kind else {
        return;
    };
    let parameters = function
        .var_blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Input)
        .flat_map(|block| block.vars.iter())
        .map(|var| function_parameter_decl(project, var))
        .collect::<Vec<_>>();
    let parameter_list = if parameters.is_empty() {
        "void".to_string()
    } else {
        parameters.join(", ")
    };
    let return_name = sanitize_c_ident(&function.name.original);
    let return_text = c_text_info(project, return_type);
    let return_array_type = named_array_type_ident(project, return_type);
    let return_is_aggregate = is_aggregate_spec(return_type, project);
    let mut function_var_types = std::collections::BTreeMap::new();
    function_var_types.insert(function.name.canonical.clone(), return_type.clone());
    for var in function.variable_declarations() {
        function_var_types.insert(var.name.canonical.clone(), var.type_spec.clone());
    }
    c_writeln!(
        out,
        "static {} {}({}) {{",
        function_c_type(project, return_type),
        return_name,
        parameter_list
    );
    if let Some(info) = return_text {
        c_writeln!(out, "    static char {return_name}[{}];", info.capacity);
        c_writeln!(
            out,
            "    rbcpp_strassign({return_name}, {}, \"\");",
            info.capacity
        );
    } else if let Some(type_ident) = &return_array_type {
        c_writeln!(out, "    {type_ident} {return_name};");
        emit_initializer(
            out,
            &format!("{return_name}.value"),
            return_type,
            None,
            project,
        );
    } else if return_is_aggregate {
        emit_c_declaration(out, "    ", &return_name, return_type, project);
        emit_local_initializer(
            out,
            1,
            &return_name,
            return_type,
            None,
            &function_var_types,
            project,
        );
    } else {
        let resolved_return = resolve_named_spec(project, return_type);
        c_writeln!(
            out,
            "    {} {} = {};",
            c_storage_type(project, return_type),
            return_name,
            default_expr_to_c(project, &resolved_return)
        );
    }

    let input_names = function
        .var_blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Input)
        .flat_map(|block| block.vars.iter())
        .map(|var| var.name.canonical.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for var in function.variable_declarations() {
        if input_names.contains(&var.name.canonical) {
            continue;
        }
        if is_aggregate_spec(&var.type_spec, project) || is_string_spec(project, &var.type_spec) {
            let var_name = sanitize_c_ident(&var.name.original);
            emit_c_declaration(out, "    ", &var_name, &var.type_spec, project);
            emit_local_initializer(
                out,
                1,
                &var_name,
                &var.type_spec,
                var.initial_value.as_ref(),
                &function_var_types,
                project,
            );
            continue;
        }
        let initial = var
            .initial_value
            .as_ref()
            .map(|expr| expr_to_c_local_typed(expr, &function_var_types, project))
            .unwrap_or_else(|| default_expr_to_c(project, &var.type_spec));
        c_writeln!(
            out,
            "    {} {} = {};",
            c_storage_type(project, &var.type_spec),
            sanitize_c_ident(&var.name.original),
            initial
        );
    }

    if statements_need_il_accumulator(&function.body.statements) {
        c_writeln!(out, "    int64_t rbcpp_acc = 0;");
    }
    let function_context = FunctionCContext {
        return_name,
        return_text,
        array_return_name: return_array_type.map(|_| function.name.canonical.clone()),
        var_types: function_var_types,
        project,
    };
    for statement in &function.body.statements {
        emit_function_statement(out, statement, 1, &function_context);
    }
    c_writeln!(out, "    return {};", function_context.return_name);
    c_writeln!(out, "}}");
}

#[derive(Debug, Clone)]
pub(crate) struct FunctionCContext<'a> {
    pub(crate) return_name: String,
    pub(crate) return_text: Option<CTextInfo>,
    pub(crate) array_return_name: Option<String>,
    pub(crate) var_types: std::collections::BTreeMap<String, DataTypeSpec>,
    pub(crate) project: &'a Project,
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

pub(crate) fn emit_statement(
    out: &mut CEmitter<'_>,
    statement: &Statement,
    indent: usize,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let pad = "    ".repeat(indent);
    match statement {
        Statement::Assignment { target, value } => {
            let target_c = var_to_c_state(target, var_types, project);
            if let Some(target_spec) = variable_spec(target, var_types, project) {
                if emit_array_function_return_assignment(
                    out,
                    &pad,
                    &target_c,
                    &target_spec,
                    value,
                    var_types,
                    project,
                ) {
                    return;
                }
                if let Expr::Variable(source) = value {
                    if let Some(source_spec) = variable_spec(source, var_types, project) {
                        if array_storage_compatible(project, &target_spec, &source_spec) {
                            c_writeln!(
                                out,
                                "{pad}memcpy(s->{target_c}, {}, sizeof(s->{target_c}));",
                                expr_to_c_state(value, var_types, project)
                            );
                            return;
                        }
                    }
                }
                if let Expr::Variable(source) = value {
                    if let Some(source_spec) = variable_spec(source, var_types, project) {
                        if struct_storage_compatible(project, &target_spec, &source_spec) {
                            c_writeln!(
                                out,
                                "{pad}memcpy(&s->{target_c}, &{}, sizeof(s->{target_c}));",
                                expr_to_c_state(value, var_types, project)
                            );
                            return;
                        }
                    }
                }
                if let Some(info) = c_text_info(project, &target_spec) {
                    let assign = if info.wide {
                        "rbcpp_wstrassign_utf8"
                    } else {
                        "rbcpp_strassign"
                    };
                    c_writeln!(
                        out,
                        "{pad}{assign}(s->{target_c}, {}, {});",
                        info.capacity,
                        expr_to_c_state(value, var_types, project)
                    );
                    return;
                }
            }
            c_writeln!(
                out,
                "{pad}s->{} = {};",
                target_c,
                expr_to_c_state(value, var_types, project)
            );
        }
        Statement::If {
            branches,
            else_branch,
        } => {
            for (index, (condition, body)) in branches.iter().enumerate() {
                if index == 0 {
                    c_writeln!(
                        out,
                        "{pad}if ({}) {{",
                        expr_to_c_state(condition, var_types, project)
                    );
                } else {
                    c_writeln!(
                        out,
                        "{pad}else if ({}) {{",
                        expr_to_c_state(condition, var_types, project)
                    );
                }
                for statement in body {
                    emit_statement(out, statement, indent + 1, var_types, project);
                }
                c_writeln!(out, "{pad}}}");
            }
            if !else_branch.is_empty() {
                c_writeln!(out, "{pad}else {{");
                for statement in else_branch {
                    emit_statement(out, statement, indent + 1, var_types, project);
                }
                c_writeln!(out, "{pad}}}");
            }
        }
        Statement::For {
            control,
            from,
            to,
            by,
            body,
        } => {
            let step = by
                .as_ref()
                .map(|expr| expr_to_c_state(expr, var_types, project))
                .unwrap_or_else(|| "1".to_string());
            let control_name = sanitize_c_ident(&control.original);
            c_writeln!(out, "{pad}{{");
            c_writeln!(out, "{pad}    int64_t rbcpp_step = {step};");
            c_writeln!(
                out,
                "{pad}    for (s->{control_name} = {}; (rbcpp_step >= 0) ? (s->{control_name} <= {}) : (s->{control_name} >= {}); s->{control_name} += rbcpp_step) {{",
                expr_to_c_state(from, var_types, project),
                expr_to_c_state(to, var_types, project),
                expr_to_c_state(to, var_types, project)
            );
            for statement in body {
                emit_statement(out, statement, indent + 2, var_types, project);
            }
            c_writeln!(out, "{pad}    }}");
            c_writeln!(out, "{pad}}}");
        }
        Statement::While { condition, body } => {
            c_writeln!(
                out,
                "{pad}while ({}) {{",
                expr_to_c_state(condition, var_types, project)
            );
            for statement in body {
                emit_statement(out, statement, indent + 1, var_types, project);
            }
            c_writeln!(out, "{pad}}}");
        }
        Statement::Repeat { body, until } => {
            c_writeln!(out, "{pad}do {{");
            for statement in body {
                emit_statement(out, statement, indent + 1, var_types, project);
            }
            c_writeln!(
                out,
                "{pad}}} while (!({}));",
                expr_to_c_state(until, var_types, project)
            );
        }
        Statement::Il { op, operand } => {
            emit_il_instruction(out, &pad, *op, operand.as_ref(), var_types, project)
        }
        Statement::IlLabel(label) => {
            c_writeln!(out, "{}:;", il_label_to_c(label));
        }
        Statement::Exit => {
            c_writeln!(out, "{pad}break;");
        }
        Statement::Return => {
            c_writeln!(out, "{pad}return;");
        }
        Statement::FbCall { name, args } => {
            emit_fb_call(out, &pad, name, args, var_types, project);
        }
        Statement::Case {
            selector,
            cases,
            else_branch,
        } => emit_case_statement(
            out,
            &pad,
            selector,
            cases,
            else_branch,
            indent,
            var_types,
            project,
        ),
        Statement::Unsupported(_) | Statement::Empty => {
            c_writeln!(out, "{pad}/* statement not emitted yet */");
        }
    }
}

pub(crate) fn emit_array_function_return_assignment(
    out: &mut CEmitter<'_>,
    pad: &str,
    target_c: &str,
    target_spec: &DataTypeSpec,
    value: &Expr,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> bool {
    let Expr::Call { name, args } = value else {
        return false;
    };
    let Some(function) = project
        .find_pou(&name.original)
        .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))
    else {
        return false;
    };
    let PouKind::Function { return_type } = &function.kind else {
        return false;
    };
    let Some(type_ident) = named_array_type_ident(project, return_type) else {
        return false;
    };
    if !array_storage_compatible(project, target_spec, return_type) {
        return false;
    }

    let call_args =
        user_function_call_input_args_to_c_state(project, &name.original, args, var_types);
    let call = format!(
        "{}({})",
        sanitize_c_ident(&name.original),
        call_args.join(", ")
    );
    let en = args
        .iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
        .and_then(|arg| arg.expr.as_ref())
        .map(|expr| expr_to_c_state(expr, var_types, project));
    let eno = args.iter().find(|arg| {
        arg.output && arg.name.as_ref().is_some_and(is_implicit_eno) && arg.variable.is_some()
    });

    c_writeln!(out, "{pad}{{");
    c_writeln!(out, "{pad}    {type_ident} rbcpp_array_result;");
    match en {
        Some(en) => {
            c_writeln!(out, "{pad}    if ({en}) {{");
            c_writeln!(out, "{pad}        rbcpp_array_result = {call};");
            emit_eno_assignment_state(out, pad, eno, true, var_types, project, 2);
            c_writeln!(out, "{pad}    }} else {{");
            emit_initializer(out, "rbcpp_array_result.value", return_type, None, project);
            emit_eno_assignment_state(out, pad, eno, false, var_types, project, 2);
            c_writeln!(out, "{pad}    }}");
        }
        None => {
            c_writeln!(out, "{pad}    rbcpp_array_result = {call};");
            emit_eno_assignment_state(out, pad, eno, true, var_types, project, 1);
        }
    }
    c_writeln!(
        out,
        "{pad}    memcpy(s->{target_c}, rbcpp_array_result.value, sizeof(s->{target_c}));"
    );
    c_writeln!(out, "{pad}}}");
    true
}

pub(crate) fn emit_eno_assignment_state(
    out: &mut CEmitter<'_>,
    pad: &str,
    eno: Option<&ParamAssignment>,
    success: bool,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
    indent: usize,
) {
    let Some(eno) = eno else {
        return;
    };
    let Some(variable) = &eno.variable else {
        return;
    };
    let pad = format!("{pad}{}", "    ".repeat(indent));
    c_writeln!(
        out,
        "{pad}s->{} = {};",
        var_to_c_state(variable, var_types, project),
        eno_bool_value(eno, success)
    );
}

pub(crate) fn emit_case_statement(
    out: &mut CEmitter<'_>,
    pad: &str,
    selector: &Expr,
    cases: &[(Vec<CaseLabel>, Vec<Statement>)],
    else_branch: &[Statement],
    indent: usize,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let selector_c = expr_to_c_state(selector, var_types, project);
    for (index, (labels, body)) in cases.iter().enumerate() {
        let condition = labels
            .iter()
            .map(|label| case_label_to_c(&selector_c, label, var_types, project))
            .collect::<Vec<_>>()
            .join(" || ");
        if index == 0 {
            c_writeln!(out, "{pad}if ({condition}) {{");
        } else {
            c_writeln!(out, "{pad}else if ({condition}) {{");
        }
        for statement in body {
            emit_statement(out, statement, indent + 1, var_types, project);
        }
        c_writeln!(out, "{pad}}}");
    }

    if !else_branch.is_empty() {
        c_writeln!(out, "{pad}else {{");
        for statement in else_branch {
            emit_statement(out, statement, indent + 1, var_types, project);
        }
        c_writeln!(out, "{pad}}}");
    }
}

pub(crate) fn case_label_to_c(
    selector_c: &str,
    label: &CaseLabel,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> String {
    match label {
        CaseLabel::Single(expr) => {
            format!(
                "{selector_c} == {}",
                expr_to_c_state(expr, var_types, project)
            )
        }
        CaseLabel::Range(low, high) => format!(
            "({selector_c} >= {}) && ({selector_c} <= {})",
            expr_to_c_state(low, var_types, project),
            expr_to_c_state(high, var_types, project)
        ),
    }
}

pub(crate) fn emit_sfc_initialization(out: &mut CEmitter<'_>, sfc: &Sfc) {
    for step in &sfc.steps {
        c_writeln!(
            out,
            "    s->{} = {};",
            sfc_step_field(&step.name),
            if step.initial { "true" } else { "false" }
        );
    }
    for key in sfc_action_control_keys(sfc) {
        c_writeln!(out, "    s->{} = false;", sfc_action_field_from_key(&key));
        c_writeln!(
            out,
            "    s->{} = false;",
            sfc_action_previous_field_from_key(&key)
        );
        c_writeln!(
            out,
            "    s->{} = 0;",
            sfc_action_elapsed_field_from_key(&key)
        );
    }
}

pub(crate) fn emit_sfc_scan(
    out: &mut CEmitter<'_>,
    sfc: &Sfc,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    for control in sfc_action_controls(sfc) {
        emit_sfc_action_execution(out, &control, var_types, project);
    }

    for (index, transition) in sfc.transitions.iter().enumerate() {
        let Some((from_steps, _to_steps)) = sfc_transition_steps(sfc, transition, index) else {
            continue;
        };
        let condition = transition
            .condition
            .as_ref()
            .map(|expr| expr_to_c_state(expr, var_types, project))
            .unwrap_or_else(|| "false".to_string());
        let active_condition = from_steps
            .iter()
            .map(|step| format!("s->{}", sfc_step_field(step)))
            .collect::<Vec<_>>()
            .join(" && ");
        c_writeln!(
            out,
            "    bool rbcpp_sfc_fire_{index} = ({active_condition}) && ({});",
            condition
        );
    }

    for step in &sfc.steps {
        c_writeln!(
            out,
            "    bool rbcpp_sfc_consumed_{} = false;",
            sfc_step_field(&step.name)
        );
    }

    for index in sfc_transition_application_order(sfc) {
        let transition = &sfc.transitions[index];
        let Some((from_steps, to_steps)) = sfc_transition_steps(sfc, transition, index) else {
            continue;
        };
        let not_consumed = from_steps
            .iter()
            .map(|step| format!("!rbcpp_sfc_consumed_{}", sfc_step_field(step)))
            .collect::<Vec<_>>()
            .join(" && ");
        c_writeln!(out, "    if (rbcpp_sfc_fire_{index} && {not_consumed}) {{");
        for from_step in &from_steps {
            c_writeln!(
                out,
                "        rbcpp_sfc_consumed_{} = true;",
                sfc_step_field(from_step)
            );
        }
        for from_step in from_steps {
            c_writeln!(out, "        s->{} = false;", sfc_step_field(from_step));
        }
        for to_step in to_steps {
            c_writeln!(out, "        s->{} = true;", sfc_step_field(to_step));
        }
        c_writeln!(out, "    }}");
    }
}

pub(crate) fn emit_sfc_action_execution(
    out: &mut CEmitter<'_>,
    control: &SfcActionControl<'_>,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let stored = sfc_action_field_from_key(&control.key);
    let previous = sfc_action_previous_field_from_key(&control.key);
    let elapsed = sfc_action_elapsed_field_from_key(&control.key);
    let local = sanitize_c_ident(&control.key);
    let active_for = |input: &SfcActionControlInput<'_>| {
        input
            .active_step
            .map(|step| format!("s->{}", sfc_step_field(step)))
            .unwrap_or_else(|| "false".to_string())
    };
    let qualifier_expr = |qualifier: SfcActionQualifier| {
        let active = control
            .inputs
            .iter()
            .filter(|input| input.qualifier == qualifier)
            .map(active_for)
            .collect::<Vec<_>>();
        if active.is_empty() {
            "false".to_string()
        } else {
            active.join(" || ")
        }
    };
    let duration_for = |qualifier: SfcActionQualifier| {
        control
            .inputs
            .iter()
            .find(|input| input.qualifier == qualifier)
            .and_then(|input| input.duration)
    };

    let n_active = qualifier_expr(SfcActionQualifier::NonStored);
    let s_active = qualifier_expr(SfcActionQualifier::SetStored);
    let r_active = qualifier_expr(SfcActionQualifier::ResetStored);
    let p_active = qualifier_expr(SfcActionQualifier::Pulse);
    let p0_active = qualifier_expr(SfcActionQualifier::PulseFalling);
    let has_p0_input = control
        .inputs
        .iter()
        .any(|input| input.qualifier == SfcActionQualifier::PulseFalling);
    let l_active = qualifier_expr(SfcActionQualifier::TimeLimited);
    let d_active = qualifier_expr(SfcActionQualifier::TimeDelayed);
    let sd_active = qualifier_expr(SfcActionQualifier::StoredDelayed);
    let ds_active = qualifier_expr(SfcActionQualifier::DelayedStored);
    let sl_active = qualifier_expr(SfcActionQualifier::StoredLimited);
    let timed_active = [
        l_active.as_str(),
        d_active.as_str(),
        sd_active.as_str(),
        ds_active.as_str(),
        sl_active.as_str(),
    ]
    .into_iter()
    .filter(|expr| *expr != "false")
    .collect::<Vec<_>>()
    .join(" || ");
    let timed_active = if timed_active.is_empty() {
        "false".to_string()
    } else {
        timed_active
    };

    c_writeln!(out, "    bool rbcpp_sfc_execute_{local} = false;");
    c_writeln!(out, "    if ({r_active}) {{");
    c_writeln!(out, "        s->{stored} = false;");
    c_writeln!(out, "        s->{elapsed} = 0;");
    c_writeln!(out, "    }}");
    c_writeln!(out, "    if ({s_active}) {{ s->{stored} = true; }}");
    c_writeln!(
        out,
        "    if ({n_active}) {{ rbcpp_sfc_execute_{local} = true; }}"
    );
    c_writeln!(
        out,
        "    if (({p_active}) && !s->{previous}) {{ rbcpp_sfc_execute_{local} = true; }}"
    );
    if has_p0_input {
        c_writeln!(
            out,
            "    if (!({p0_active}) && s->{previous}) {{ rbcpp_sfc_execute_{local} = true; }}"
        );
    }
    c_writeln!(out, "    s->{previous} = ({p_active}) || ({p0_active});");

    if l_active != "false" {
        let duration = sfc_action_duration_ms(duration_for(SfcActionQualifier::TimeLimited));
        c_writeln!(out, "    if ({l_active}) {{");
        c_writeln!(out, "        s->{elapsed} += RBCPP_CYCLE_MS;");
        c_writeln!(
            out,
            "        if (s->{elapsed} <= {duration}) {{ rbcpp_sfc_execute_{local} = true; }}"
        );
        c_writeln!(out, "    }}");
    }
    if d_active != "false" {
        let duration = sfc_action_duration_ms(duration_for(SfcActionQualifier::TimeDelayed));
        c_writeln!(out, "    if ({d_active}) {{");
        c_writeln!(out, "        s->{elapsed} += RBCPP_CYCLE_MS;");
        c_writeln!(
            out,
            "        if (s->{elapsed} >= {duration}) {{ rbcpp_sfc_execute_{local} = true; }}"
        );
        c_writeln!(out, "    }}");
    }
    for (active, qualifier) in [
        (&sd_active, SfcActionQualifier::StoredDelayed),
        (&ds_active, SfcActionQualifier::DelayedStored),
    ] {
        if active == "false" {
            continue;
        }
        let duration = sfc_action_duration_ms(duration_for(qualifier));
        c_writeln!(out, "    if ({active}) {{");
        c_writeln!(out, "        s->{elapsed} += RBCPP_CYCLE_MS;");
        c_writeln!(
            out,
            "        if (s->{elapsed} >= {duration}) {{ s->{stored} = true; }}"
        );
        c_writeln!(out, "    }}");
    }
    if sl_active != "false" {
        let duration = sfc_action_duration_ms(duration_for(SfcActionQualifier::StoredLimited));
        c_writeln!(out, "    if ({sl_active} && !s->{stored}) {{");
        c_writeln!(out, "        s->{stored} = true;");
        c_writeln!(out, "        s->{elapsed} = 0;");
        c_writeln!(out, "    }}");
        c_writeln!(out, "    if (s->{stored}) {{");
        c_writeln!(out, "        s->{elapsed} += RBCPP_CYCLE_MS;");
        c_writeln!(
            out,
            "        if (s->{elapsed} <= {duration}) {{ rbcpp_sfc_execute_{local} = true; }} else {{ s->{stored} = false; }}"
        );
        c_writeln!(out, "    }}");
    } else {
        c_writeln!(
            out,
            "    if (s->{stored}) {{ rbcpp_sfc_execute_{local} = true; }}"
        );
    }
    c_writeln!(
        out,
        "    if (!({timed_active}) && !s->{stored}) {{ s->{elapsed} = 0; }}"
    );
    c_writeln!(
        out,
        "    if ({r_active}) {{ rbcpp_sfc_execute_{local} = false; }}"
    );
    c_writeln!(out, "    if (rbcpp_sfc_execute_{local}) {{");
    emit_sfc_action_body(out, control.action, 2, var_types, project);
    c_writeln!(out, "    }}");
}

pub(crate) fn emit_sfc_action_body(
    out: &mut CEmitter<'_>,
    action: &SfcAction,
    indent: usize,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    for statement in &action.body {
        emit_statement(out, statement, indent, var_types, project);
    }
}

pub(crate) fn emit_function_statement(
    out: &mut CEmitter<'_>,
    statement: &Statement,
    indent: usize,
    context: &FunctionCContext,
) {
    let pad = "    ".repeat(indent);
    match statement {
        Statement::Assignment { target, value } => {
            let target_c = function_local_var_to_c(target, context);
            if let Some(info) = context.return_text {
                if target_c == context.return_name {
                    c_writeln!(
                        out,
                        "{pad}rbcpp_strassign({}, {}, {});",
                        context.return_name,
                        info.capacity,
                        expr_to_c_local_typed(value, &context.var_types, context.project)
                    );
                    return;
                }
            }
            if let Some(target_spec) = variable_spec(target, &context.var_types, context.project) {
                if emit_local_array_function_return_assignment(
                    out,
                    &pad,
                    &target_c,
                    &target_spec,
                    value,
                    context,
                ) {
                    return;
                }
                if let Expr::Variable(source) = value {
                    if let Some(source_spec) =
                        variable_spec(source, &context.var_types, context.project)
                    {
                        if array_storage_compatible(context.project, &target_spec, &source_spec) {
                            c_writeln!(
                                out,
                                "{pad}memcpy({target_c}, {}, sizeof({target_c}));",
                                expr_to_c_local_typed(value, &context.var_types, context.project)
                            );
                            return;
                        }
                        if struct_storage_compatible(context.project, &target_spec, &source_spec) {
                            c_writeln!(
                                out,
                                "{pad}memcpy(&{target_c}, &{}, sizeof({target_c}));",
                                expr_to_c_local_typed(value, &context.var_types, context.project)
                            );
                            return;
                        }
                    }
                }
                if emit_local_initializer(
                    out,
                    indent,
                    &target_c,
                    &target_spec,
                    Some(value),
                    &context.var_types,
                    context.project,
                ) {
                    return;
                }
            }
            c_writeln!(
                out,
                "{pad}{} = {};",
                target_c,
                expr_to_c_local_typed(value, &context.var_types, context.project)
            );
        }
        Statement::If {
            branches,
            else_branch,
        } => {
            for (index, (condition, body)) in branches.iter().enumerate() {
                if index == 0 {
                    c_writeln!(
                        out,
                        "{pad}if ({}) {{",
                        expr_to_c_local_typed(condition, &context.var_types, context.project)
                    );
                } else {
                    c_writeln!(
                        out,
                        "{pad}else if ({}) {{",
                        expr_to_c_local_typed(condition, &context.var_types, context.project)
                    );
                }
                for statement in body {
                    emit_function_statement(out, statement, indent + 1, context);
                }
                c_writeln!(out, "{pad}}}");
            }
            if !else_branch.is_empty() {
                c_writeln!(out, "{pad}else {{");
                for statement in else_branch {
                    emit_function_statement(out, statement, indent + 1, context);
                }
                c_writeln!(out, "{pad}}}");
            }
        }
        Statement::Case {
            selector,
            cases,
            else_branch,
        } => emit_function_case_statement(out, &pad, selector, cases, else_branch, indent, context),
        Statement::For {
            control,
            from,
            to,
            by,
            body,
        } => {
            let step = by
                .as_ref()
                .map(|expr| expr_to_c_local_typed(expr, &context.var_types, context.project))
                .unwrap_or_else(|| "1".to_string());
            let control_name = sanitize_c_ident(&control.original);
            c_writeln!(out, "{pad}{{");
            c_writeln!(out, "{pad}    int64_t rbcpp_step = {step};");
            c_writeln!(
                out,
                "{pad}    for ({control_name} = {}; (rbcpp_step >= 0) ? ({control_name} <= {}) : ({control_name} >= {}); {control_name} += rbcpp_step) {{",
                expr_to_c_local_typed(from, &context.var_types, context.project),
                expr_to_c_local_typed(to, &context.var_types, context.project),
                expr_to_c_local_typed(to, &context.var_types, context.project)
            );
            for statement in body {
                emit_function_statement(out, statement, indent + 2, context);
            }
            c_writeln!(out, "{pad}    }}");
            c_writeln!(out, "{pad}}}");
        }
        Statement::While { condition, body } => {
            c_writeln!(
                out,
                "{pad}while ({}) {{",
                expr_to_c_local_typed(condition, &context.var_types, context.project)
            );
            for statement in body {
                emit_function_statement(out, statement, indent + 1, context);
            }
            c_writeln!(out, "{pad}}}");
        }
        Statement::Repeat { body, until } => {
            c_writeln!(out, "{pad}do {{");
            for statement in body {
                emit_function_statement(out, statement, indent + 1, context);
            }
            c_writeln!(
                out,
                "{pad}}} while (!({}));",
                expr_to_c_local_typed(until, &context.var_types, context.project)
            );
        }
        Statement::Il { op, operand } => {
            emit_local_il_instruction(out, &pad, *op, operand.as_ref())
        }
        Statement::IlLabel(label) => {
            c_writeln!(out, "{}:;", il_label_to_c(label));
        }
        Statement::Exit => {
            c_writeln!(out, "{pad}break;");
        }
        Statement::Return => {
            c_writeln!(out, "{pad}return {};", context.return_name);
        }
        Statement::FbCall { name, args } => {
            if let Some(root) = name.root_name() {
                if is_standard_void_call_name(&root.original) {
                    emit_standard_void_call_local(out, &pad, root, args, context);
                } else {
                    c_writeln!(out, "{pad}/* statement not emitted yet */");
                }
            } else {
                c_writeln!(out, "{pad}/* statement not emitted yet */");
            }
        }
        Statement::Unsupported(_) | Statement::Empty => {
            c_writeln!(out, "{pad}/* statement not emitted yet */");
        }
    }
}

pub(crate) fn emit_local_array_function_return_assignment(
    out: &mut CEmitter<'_>,
    pad: &str,
    target_c: &str,
    target_spec: &DataTypeSpec,
    value: &Expr,
    context: &FunctionCContext,
) -> bool {
    let Expr::Call { name, args } = value else {
        return false;
    };
    let Some(function) = context
        .project
        .find_pou(&name.original)
        .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))
    else {
        return false;
    };
    let PouKind::Function { return_type } = &function.kind else {
        return false;
    };
    let Some(type_ident) = named_array_type_ident(context.project, return_type) else {
        return false;
    };
    if !array_storage_compatible(context.project, target_spec, return_type) {
        return false;
    }

    let call_args = user_function_call_input_args_to_c_local_typed(
        context.project,
        &name.original,
        args,
        &context.var_types,
    );
    let call = format!(
        "{}({})",
        sanitize_c_ident(&name.original),
        call_args.join(", ")
    );
    let en = args
        .iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
        .and_then(|arg| arg.expr.as_ref())
        .map(|expr| expr_to_c_local_typed(expr, &context.var_types, context.project));
    let eno = args.iter().find(|arg| {
        arg.output && arg.name.as_ref().is_some_and(is_implicit_eno) && arg.variable.is_some()
    });

    c_writeln!(out, "{pad}{{");
    c_writeln!(out, "{pad}    {type_ident} rbcpp_array_result;");
    match en {
        Some(en) => {
            c_writeln!(out, "{pad}    if ({en}) {{");
            c_writeln!(out, "{pad}        rbcpp_array_result = {call};");
            emit_eno_assignment_local(out, pad, eno, true, context, 2);
            c_writeln!(out, "{pad}    }} else {{");
            emit_initializer(
                out,
                "rbcpp_array_result.value",
                return_type,
                None,
                context.project,
            );
            emit_eno_assignment_local(out, pad, eno, false, context, 2);
            c_writeln!(out, "{pad}    }}");
        }
        None => {
            c_writeln!(out, "{pad}    rbcpp_array_result = {call};");
            emit_eno_assignment_local(out, pad, eno, true, context, 1);
        }
    }
    c_writeln!(
        out,
        "{pad}    memcpy({target_c}, rbcpp_array_result.value, sizeof({target_c}));"
    );
    c_writeln!(out, "{pad}}}");
    true
}

pub(crate) fn emit_eno_assignment_local(
    out: &mut CEmitter<'_>,
    pad: &str,
    eno: Option<&ParamAssignment>,
    enabled: bool,
    context: &FunctionCContext,
    indent_offset: usize,
) {
    let Some(eno) = eno else {
        return;
    };
    let Some(variable) = eno.variable.as_ref() else {
        return;
    };
    let indent = "    ".repeat(indent_offset);
    c_writeln!(
        out,
        "{pad}{indent}{} = {};",
        function_local_var_to_c(variable, context),
        eno_bool_value(eno, enabled)
    );
}

pub(crate) fn emit_function_case_statement(
    out: &mut CEmitter<'_>,
    pad: &str,
    selector: &Expr,
    cases: &[(Vec<CaseLabel>, Vec<Statement>)],
    else_branch: &[Statement],
    indent: usize,
    context: &FunctionCContext,
) {
    let selector_c = expr_to_c_local_typed(selector, &context.var_types, context.project);
    for (index, (labels, body)) in cases.iter().enumerate() {
        let condition = labels
            .iter()
            .map(|label| {
                case_label_to_c_local_typed(&selector_c, label, &context.var_types, context.project)
            })
            .collect::<Vec<_>>()
            .join(" || ");
        if index == 0 {
            c_writeln!(out, "{pad}if ({condition}) {{");
        } else {
            c_writeln!(out, "{pad}else if ({condition}) {{");
        }
        for statement in body {
            emit_function_statement(out, statement, indent + 1, context);
        }
        c_writeln!(out, "{pad}}}");
    }

    if !else_branch.is_empty() {
        c_writeln!(out, "{pad}else {{");
        for statement in else_branch {
            emit_function_statement(out, statement, indent + 1, context);
        }
        c_writeln!(out, "{pad}}}");
    }
}

pub(crate) fn case_label_to_c_local_typed(
    selector_c: &str,
    label: &CaseLabel,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> String {
    match label {
        CaseLabel::Single(expr) => {
            format!(
                "{selector_c} == {}",
                expr_to_c_local_typed(expr, var_types, project)
            )
        }
        CaseLabel::Range(low, high) => format!(
            "({selector_c} >= {}) && ({selector_c} <= {})",
            expr_to_c_local_typed(low, var_types, project),
            expr_to_c_local_typed(high, var_types, project)
        ),
    }
}

pub(crate) fn statements_need_il_accumulator(statements: &[Statement]) -> bool {
    statements.iter().any(statement_needs_il_accumulator)
}

pub(crate) fn statement_needs_il_accumulator(statement: &Statement) -> bool {
    match statement {
        Statement::Il { .. } => true,
        Statement::If {
            branches,
            else_branch,
        } => {
            branches
                .iter()
                .any(|(_, body)| statements_need_il_accumulator(body))
                || statements_need_il_accumulator(else_branch)
        }
        Statement::Case {
            cases, else_branch, ..
        } => {
            cases
                .iter()
                .any(|(_, body)| statements_need_il_accumulator(body))
                || statements_need_il_accumulator(else_branch)
        }
        Statement::For { body, .. }
        | Statement::While { body, .. }
        | Statement::Repeat { body, .. } => statements_need_il_accumulator(body),
        _ => false,
    }
}

pub(crate) fn emit_il_instruction(
    out: &mut CEmitter<'_>,
    pad: &str,
    op: IlOp,
    operand: Option<&Expr>,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let operand_c = operand
        .map(|expr| expr_to_c_state(expr, var_types, project))
        .unwrap_or_else(|| "0".to_string());
    match op {
        IlOp::Ld => c_writeln!(out, "{pad}rbcpp_acc = {operand_c};"),
        IlOp::Ldn => c_writeln!(out, "{pad}rbcpp_acc = !({operand_c});"),
        IlOp::St => {
            if let Some(Expr::Variable(target)) = operand {
                c_writeln!(
                    out,
                    "{pad}s->{} = rbcpp_acc;",
                    var_to_c_state(target, var_types, project)
                );
            }
        }
        IlOp::Stn => {
            if let Some(Expr::Variable(target)) = operand {
                c_writeln!(
                    out,
                    "{pad}s->{} = !rbcpp_acc;",
                    var_to_c_state(target, var_types, project)
                );
            }
        }
        IlOp::S | IlOp::R => {
            if let Some(Expr::Variable(target)) = operand {
                let value = if matches!(op, IlOp::S) {
                    "true"
                } else {
                    "false"
                };
                c_writeln!(
                    out,
                    "{pad}if (rbcpp_acc) {{ s->{} = {value}; }}",
                    var_to_c_state(target, var_types, project)
                );
            }
        }
        IlOp::Not => c_writeln!(out, "{pad}rbcpp_acc = !rbcpp_acc;"),
        IlOp::And => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc && ({operand_c});"),
        IlOp::Andn => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc && !({operand_c});"),
        IlOp::Or => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc || ({operand_c});"),
        IlOp::Orn => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc || !({operand_c});"),
        IlOp::Xor => c_writeln!(out, "{pad}rbcpp_acc = !!rbcpp_acc != !!({operand_c});"),
        IlOp::Xorn => c_writeln!(out, "{pad}rbcpp_acc = !!rbcpp_acc != !({operand_c});"),
        IlOp::Add => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc + ({operand_c});"),
        IlOp::Sub => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc - ({operand_c});"),
        IlOp::Mul => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc * ({operand_c});"),
        IlOp::Div => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc / ({operand_c});"),
        IlOp::Mod => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc % ({operand_c});"),
        IlOp::Gt => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc > ({operand_c});"),
        IlOp::Ge => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc >= ({operand_c});"),
        IlOp::Eq => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc == ({operand_c});"),
        IlOp::Ne => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc != ({operand_c});"),
        IlOp::Le => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc <= ({operand_c});"),
        IlOp::Lt => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc < ({operand_c});"),
        IlOp::Jmp | IlOp::Jmpc | IlOp::Jmpcn => {
            if let Some(label) = operand.and_then(il_label_operand) {
                let label = il_label_to_c(label);
                match op {
                    IlOp::Jmp => c_writeln!(out, "{pad}goto {label};"),
                    IlOp::Jmpc => c_writeln!(out, "{pad}if (rbcpp_acc) {{ goto {label}; }}"),
                    IlOp::Jmpcn => {
                        c_writeln!(out, "{pad}if (!rbcpp_acc) {{ goto {label}; }}");
                    }
                    _ => {}
                }
            }
        }
        IlOp::Cal | IlOp::Calc | IlOp::Calcn => {
            if let Some((name, args)) = operand.and_then(il_call_operand) {
                match op {
                    IlOp::Cal => emit_fb_call(out, pad, &name, &args, var_types, project),
                    IlOp::Calc => {
                        c_writeln!(out, "{pad}if (rbcpp_acc) {{");
                        emit_fb_call(out, &format!("{pad}    "), &name, &args, var_types, project);
                        c_writeln!(out, "{pad}}}");
                    }
                    IlOp::Calcn => {
                        c_writeln!(out, "{pad}if (!rbcpp_acc) {{");
                        emit_fb_call(out, &format!("{pad}    "), &name, &args, var_types, project);
                        c_writeln!(out, "{pad}}}");
                    }
                    _ => {}
                }
            }
        }
        IlOp::Ret => c_writeln!(out, "{pad}return;"),
        IlOp::Retc => c_writeln!(out, "{pad}if (rbcpp_acc) {{ return; }}"),
        IlOp::Retcn => c_writeln!(out, "{pad}if (!rbcpp_acc) {{ return; }}"),
    }
}

pub(crate) fn emit_local_il_instruction(
    out: &mut CEmitter<'_>,
    pad: &str,
    op: IlOp,
    operand: Option<&Expr>,
) {
    let operand_c = operand
        .map(expr_to_c_local)
        .unwrap_or_else(|| "0".to_string());
    match op {
        IlOp::Ld => c_writeln!(out, "{pad}rbcpp_acc = {operand_c};"),
        IlOp::Ldn => c_writeln!(out, "{pad}rbcpp_acc = !({operand_c});"),
        IlOp::St => {
            if let Some(Expr::Variable(target)) = operand {
                c_writeln!(out, "{pad}{} = rbcpp_acc;", local_var_to_c(target));
            }
        }
        IlOp::Stn => {
            if let Some(Expr::Variable(target)) = operand {
                c_writeln!(out, "{pad}{} = !rbcpp_acc;", local_var_to_c(target));
            }
        }
        IlOp::S | IlOp::R => {
            if let Some(Expr::Variable(target)) = operand {
                let value = if matches!(op, IlOp::S) {
                    "true"
                } else {
                    "false"
                };
                c_writeln!(
                    out,
                    "{pad}if (rbcpp_acc) {{ {} = {value}; }}",
                    local_var_to_c(target)
                );
            }
        }
        IlOp::Not => c_writeln!(out, "{pad}rbcpp_acc = !rbcpp_acc;"),
        IlOp::And => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc && ({operand_c});"),
        IlOp::Andn => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc && !({operand_c});"),
        IlOp::Or => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc || ({operand_c});"),
        IlOp::Orn => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc || !({operand_c});"),
        IlOp::Xor => c_writeln!(out, "{pad}rbcpp_acc = !!rbcpp_acc != !!({operand_c});"),
        IlOp::Xorn => c_writeln!(out, "{pad}rbcpp_acc = !!rbcpp_acc != !({operand_c});"),
        IlOp::Add => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc + ({operand_c});"),
        IlOp::Sub => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc - ({operand_c});"),
        IlOp::Mul => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc * ({operand_c});"),
        IlOp::Div => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc / ({operand_c});"),
        IlOp::Mod => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc % ({operand_c});"),
        IlOp::Gt => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc > ({operand_c});"),
        IlOp::Ge => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc >= ({operand_c});"),
        IlOp::Eq => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc == ({operand_c});"),
        IlOp::Ne => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc != ({operand_c});"),
        IlOp::Le => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc <= ({operand_c});"),
        IlOp::Lt => c_writeln!(out, "{pad}rbcpp_acc = rbcpp_acc < ({operand_c});"),
        IlOp::Jmp | IlOp::Jmpc | IlOp::Jmpcn => {
            if let Some(label) = operand.and_then(il_label_operand) {
                let label = il_label_to_c(label);
                match op {
                    IlOp::Jmp => c_writeln!(out, "{pad}goto {label};"),
                    IlOp::Jmpc => c_writeln!(out, "{pad}if (rbcpp_acc) {{ goto {label}; }}"),
                    IlOp::Jmpcn => {
                        c_writeln!(out, "{pad}if (!rbcpp_acc) {{ goto {label}; }}");
                    }
                    _ => {}
                }
            }
        }
        IlOp::Ret => c_writeln!(out, "{pad}return;"),
        IlOp::Retc => c_writeln!(out, "{pad}if (rbcpp_acc) {{ return; }}"),
        IlOp::Retcn => c_writeln!(out, "{pad}if (!rbcpp_acc) {{ return; }}"),
        IlOp::Cal | IlOp::Calc | IlOp::Calcn => {
            c_writeln!(
                out,
                "{pad}/* IL function block call not emitted in functions */"
            );
        }
    }
}
