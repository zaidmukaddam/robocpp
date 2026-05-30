// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::fmt::{self, Write};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_profile::ImplementationParameters;
use iec_stdlib::{is_standard_function, standard_function_input_index};

use crate::addressing::*;
use crate::expressions::*;
use crate::functions::*;
use crate::state::*;
use crate::*;

pub(crate) fn emit_fb_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    name: &VariableRef,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let Some(root) = name.root_name() else {
        c_writeln!(out, "{pad}/* invalid function block call */");
        return;
    };
    if is_standard_void_call_name(&root.original) {
        emit_standard_void_call(out, pad, root, args, var_types, project);
        return;
    }
    let Some(DataTypeSpec::Named(type_name)) = var_types.get(&root.canonical) else {
        c_writeln!(out, "{pad}/* unknown function block instance {name} */");
        return;
    };
    let instance = &root.original;
    let en_expr = args
        .iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
        .and_then(|arg| arg.expr.as_ref())
        .map(|expr| expr_to_c_state(expr, var_types, project));
    let eno_arg = args.iter().find(|arg| {
        arg.output && arg.name.as_ref().is_some_and(is_implicit_eno) && arg.variable.is_some()
    });
    let body_pad_storage;
    let outer_pad = pad;
    let pad = if let Some(en_expr) = &en_expr {
        c_writeln!(out, "{outer_pad}if ({en_expr}) {{");
        body_pad_storage = format!("{outer_pad}    ");
        body_pad_storage.as_str()
    } else {
        outer_pad
    };

    match type_name.canonical.as_str() {
        "SR" => {
            let s1 = fb_arg_for_block(args, "SR", "S1", var_types, project)
                .unwrap_or_else(|| "false".to_string());
            let r = fb_arg_for_block(args, "SR", "R", var_types, project)
                .unwrap_or_else(|| "false".to_string());
            c_writeln!(
                out,
                "{pad}s->{} = ({s1}) || (s->{} && !({r}));",
                fb_field_ident(instance, "Q1"),
                fb_field_ident(instance, "Q1")
            );
        }
        "RS" => {
            let s = fb_arg_for_block(args, "RS", "S", var_types, project)
                .unwrap_or_else(|| "false".to_string());
            let r1 = fb_arg_for_block(args, "RS", "R1", var_types, project)
                .unwrap_or_else(|| "false".to_string());
            c_writeln!(
                out,
                "{pad}s->{} = (s->{} || ({s})) && !({r1});",
                fb_field_ident(instance, "Q1"),
                fb_field_ident(instance, "Q1")
            );
        }
        "R_TRIG" => {
            let clk = fb_arg_for_block(args, "R_TRIG", "CLK", var_types, project)
                .unwrap_or_else(|| "false".to_string());
            let tmp = format!("rbcpp_{}_clk", sanitize_c_ident(instance));
            c_writeln!(out, "{pad}bool {tmp} = ({clk});");
            c_writeln!(
                out,
                "{pad}s->{} = {tmp} && !s->{};",
                fb_field_ident(instance, "Q"),
                fb_field_ident(instance, "M")
            );
            c_writeln!(out, "{pad}s->{} = {tmp};", fb_field_ident(instance, "M"));
        }
        "F_TRIG" => {
            let clk = fb_arg_for_block(args, "F_TRIG", "CLK", var_types, project)
                .unwrap_or_else(|| "false".to_string());
            let tmp = format!("rbcpp_{}_clk", sanitize_c_ident(instance));
            c_writeln!(out, "{pad}bool {tmp} = ({clk});");
            c_writeln!(
                out,
                "{pad}s->{} = !{tmp} && s->{};",
                fb_field_ident(instance, "Q"),
                fb_field_ident(instance, "M")
            );
            c_writeln!(out, "{pad}s->{} = {tmp};", fb_field_ident(instance, "M"));
        }
        "CTU" => emit_ctu_call(out, pad, instance, args, var_types, project),
        "CTD" => emit_ctd_call(out, pad, instance, args, var_types, project),
        "CTUD" => emit_ctud_call(out, pad, instance, args, var_types, project),
        "TON" => emit_ton_call(out, pad, instance, args, var_types, project),
        "TOF" => emit_tof_call(out, pad, instance, args, var_types, project),
        "TP" => emit_tp_call(out, pad, instance, args, var_types, project),
        _ => {
            if is_communication_fb_name(&type_name.original) {
                emit_communication_call(
                    out,
                    pad,
                    instance,
                    &type_name.original,
                    args,
                    var_types,
                    project,
                );
            } else if let Some(function_block) = project
                .find_pou(&type_name.original)
                .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
            {
                emit_user_fb_call(out, pad, instance, args, function_block, var_types, project);
            } else {
                c_writeln!(
                    out,
                    "{pad}/* function block type {} is not emitted yet */",
                    type_name.original
                );
            }
        }
    }
    if standard_fb_fields(&DataTypeSpec::Named(type_name.clone())).is_some() {
        emit_standard_fb_output_bindings_state(
            out,
            pad,
            instance,
            &type_name.original,
            args,
            var_types,
            project,
        );
    }
    if let Some(eno_arg) = eno_arg {
        let variable = eno_arg
            .variable
            .as_ref()
            .map(|variable| format!("s->{}", var_to_c_state(variable, var_types, project)))
            .expect("ENO output has a variable");
        c_writeln!(out, "{pad}{variable} = {};", eno_bool_value(eno_arg, true));
    }
    if en_expr.is_some() {
        c_writeln!(out, "{outer_pad}}}");
        if let Some(eno_arg) = eno_arg {
            let variable = eno_arg
                .variable
                .as_ref()
                .map(|variable| format!("s->{}", var_to_c_state(variable, var_types, project)))
                .expect("ENO output has a variable");
            c_writeln!(
                out,
                "{outer_pad}else {{ {variable} = {}; }}",
                eno_bool_value(eno_arg, false)
            );
        }
    }
}

pub(crate) fn is_communication_fb_name(name: &str) -> bool {
    matches!(
        canonical_identifier(name).as_str(),
        "USEND" | "URCV" | "BSEND" | "BRCV" | "SEND" | "RCV"
    )
}

pub(crate) fn user_fb_input_fields(function_block: &Pou) -> Vec<(VarBlockKind, Identifier)> {
    function_block
        .var_blocks
        .iter()
        .filter(|block| matches!(block.kind, VarBlockKind::Input | VarBlockKind::InOut))
        .flat_map(|block| {
            block
                .vars
                .iter()
                .map(move |var| (block.kind, var.name.clone()))
        })
        .collect()
}

pub(crate) fn user_fb_input_target(
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

pub(crate) fn user_fb_input_edge(function_block: &Pou, input_name: &str) -> Option<EdgeQualifier> {
    function_block
        .var_blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Input)
        .flat_map(|block| block.vars.iter())
        .find(|var| var.name.canonical == input_name)
        .and_then(|var| var.edge)
}

pub(crate) fn edge_state_field_name(input_name: &str) -> String {
    format!("$EDGE_{}", canonical_identifier(input_name))
}

pub(crate) fn emit_communication_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    block: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let req = fb_arg_for_block(args, block, "REQ", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let en_r = fb_arg_for_block(args, block, "EN_R", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let id =
        fb_arg_for_block(args, block, "ID", var_types, project).unwrap_or_else(|| "0".to_string());
    let length =
        fb_arg_for_block(args, block, "LEN", var_types, project).unwrap_or_else(|| "0".to_string());
    let ident = sanitize_c_ident(instance);
    c_writeln!(
        out,
        "{pad}rbcpp_comm_request rbcpp_{ident}_request = {{ \"{block}\", \"{instance}\", ({req}), ({en_r}), (int64_t)({id}), (int64_t)({length}) }};"
    );
    c_writeln!(
        out,
        "{pad}rbcpp_comm_response rbcpp_{ident}_response = {{ false, false, true, -1 }};"
    );
    c_writeln!(
        out,
        "{pad}if (s->rbcpp_comm && s->rbcpp_comm(s->rbcpp_comm_ctx, &rbcpp_{ident}_request, &rbcpp_{ident}_response)) {{"
    );
    c_writeln!(
        out,
        "{pad}    s->{} = rbcpp_{ident}_response.done;",
        fb_field_ident(instance, "DONE")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = rbcpp_{ident}_response.ndr;",
        fb_field_ident(instance, "NDR")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = rbcpp_{ident}_response.error;",
        fb_field_ident(instance, "ERROR")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = rbcpp_{ident}_response.status;",
        fb_field_ident(instance, "STATUS")
    );
    c_writeln!(out, "{pad}}} else {{");
    c_writeln!(
        out,
        "{pad}    s->{} = false;",
        fb_field_ident(instance, "DONE")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = false;",
        fb_field_ident(instance, "NDR")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = true;",
        fb_field_ident(instance, "ERROR")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = -1;",
        fb_field_ident(instance, "STATUS")
    );
    c_writeln!(out, "{pad}}}");
}

pub(crate) fn emit_user_fb_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    args: &[ParamAssignment],
    function_block: &Pou,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let input_fields = user_fb_input_fields(function_block);
    let field_types = function_block
        .variable_declarations()
        .map(|var| (var.name.canonical.clone(), var.type_spec.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut positional_index = 0_usize;
    for arg in args {
        let (Some((_, name)), Some(expr)) = (
            user_fb_input_target(&input_fields, arg, &mut positional_index),
            &arg.expr,
        ) else {
            continue;
        };
        let target_field = fb_field_ident(instance, &name.original);
        if let Some(edge) = user_fb_input_edge(function_block, &name.canonical) {
            let current = format!(
                "rbcpp_edge_current_{}_{}",
                sanitize_c_ident(instance),
                sanitize_c_ident(&name.original)
            );
            let previous = fb_field_ident(instance, &edge_state_field_name(&name.canonical));
            let expr_c = expr_to_c_state(expr, var_types, project);
            c_writeln!(out, "{pad}bool {current} = ({expr_c});");
            let edge_expr = match edge {
                EdgeQualifier::Rising => format!("{current} && !s->{previous}"),
                EdgeQualifier::Falling => format!("!{current} && s->{previous}"),
            };
            c_writeln!(out, "{pad}s->{target_field} = {edge_expr};");
            c_writeln!(out, "{pad}s->{previous} = {current};");
            continue;
        }
        if let Some(target_spec) = field_types.get(&name.canonical) {
            if let Some(info) = c_text_info(project, target_spec) {
                let assign = if info.wide {
                    "rbcpp_wstrassign_utf8"
                } else {
                    "rbcpp_strassign"
                };
                c_writeln!(
                    out,
                    "{pad}{assign}(s->{target_field}, {}, {});",
                    info.capacity,
                    expr_to_c_state(expr, var_types, project)
                );
                continue;
            }
            if let Expr::Variable(source) = expr {
                if let Some(source_spec) = variable_spec(source, var_types, project) {
                    if array_storage_compatible(project, target_spec, &source_spec) {
                        c_writeln!(
                            out,
                            "{pad}memcpy(s->{target_field}, {}, sizeof(s->{target_field}));",
                            expr_to_c_state(expr, var_types, project)
                        );
                        continue;
                    }
                    if struct_storage_compatible(project, target_spec, &source_spec) {
                        c_writeln!(
                            out,
                            "{pad}memcpy(&s->{target_field}, &{}, sizeof(s->{target_field}));",
                            expr_to_c_state(expr, var_types, project)
                        );
                        continue;
                    }
                }
            }
        }
        c_writeln!(
            out,
            "{pad}s->{} = {};",
            target_field,
            expr_to_c_state(expr, var_types, project)
        );
    }

    c_writeln!(out, "{pad}do {{");
    let body_pad = format!("{pad}    ");
    let return_label = format!("rbcpp_return_{}_{}", sanitize_c_ident(instance), out.len());
    if statements_need_il_accumulator(&function_block.body.statements) {
        c_writeln!(out, "{body_pad}int64_t rbcpp_acc = 0;");
    }
    for statement in &function_block.body.statements {
        emit_user_fb_statement(
            out,
            &body_pad,
            instance,
            statement,
            &field_types,
            project,
            &return_label,
        );
    }
    c_writeln!(out, "{body_pad}{return_label}:;");
    c_writeln!(out, "{pad}}} while (0);");

    let mut positional_index = 0_usize;
    for arg in args {
        let Some((kind, name)) = user_fb_input_target(&input_fields, arg, &mut positional_index)
        else {
            continue;
        };
        if kind != VarBlockKind::InOut {
            continue;
        }
        let Some(Expr::Variable(variable)) = &arg.expr else {
            continue;
        };
        if let (Some(target_spec), Some(source_spec)) = (
            variable_spec(variable, var_types, project),
            field_types.get(&name.canonical),
        ) {
            if let Some(info) = c_text_info(project, &target_spec) {
                let assign = if info.wide {
                    "rbcpp_wstrassign_utf8"
                } else {
                    "rbcpp_strassign"
                };
                c_writeln!(
                    out,
                    "{pad}{assign}(s->{}, {}, s->{});",
                    var_to_c_state(variable, var_types, project),
                    info.capacity,
                    fb_field_ident(instance, &name.original)
                );
                continue;
            }
            if array_storage_compatible(project, &target_spec, source_spec) {
                let target = var_to_c_state(variable, var_types, project);
                let source = fb_field_ident(instance, &name.original);
                c_writeln!(
                    out,
                    "{pad}memcpy(s->{target}, s->{source}, sizeof(s->{target}));"
                );
                continue;
            }
            if struct_storage_compatible(project, &target_spec, source_spec) {
                let target = var_to_c_state(variable, var_types, project);
                let source = fb_field_ident(instance, &name.original);
                c_writeln!(
                    out,
                    "{pad}memcpy(&s->{target}, &s->{source}, sizeof(s->{target}));"
                );
                continue;
            }
        }
        c_writeln!(
            out,
            "{pad}s->{} = s->{};",
            var_to_c_state(variable, var_types, project),
            fb_field_ident(instance, &name.original)
        );
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
        if let (Some(target_spec), Some(source_spec)) = (
            variable_spec(variable, var_types, project),
            field_types.get(&name.canonical),
        ) {
            if let Some(info) = c_text_info(project, &target_spec) {
                let assign = if info.wide {
                    "rbcpp_wstrassign_utf8"
                } else {
                    "rbcpp_strassign"
                };
                c_writeln!(
                    out,
                    "{pad}{assign}(s->{}, {}, s->{});",
                    var_to_c_state(variable, var_types, project),
                    info.capacity,
                    fb_field_ident(instance, &name.original)
                );
                continue;
            }
            if array_storage_compatible(project, &target_spec, source_spec) {
                let target = var_to_c_state(variable, var_types, project);
                let source = fb_field_ident(instance, &name.original);
                c_writeln!(
                    out,
                    "{pad}memcpy(s->{target}, s->{source}, sizeof(s->{target}));"
                );
                continue;
            }
            if struct_storage_compatible(project, &target_spec, source_spec) {
                let target = var_to_c_state(variable, var_types, project);
                let source = fb_field_ident(instance, &name.original);
                c_writeln!(
                    out,
                    "{pad}memcpy(&s->{target}, &s->{source}, sizeof(s->{target}));"
                );
                continue;
            }
        }
        c_writeln!(
            out,
            "{pad}s->{} = s->{};",
            var_to_c_state(variable, var_types, project),
            fb_field_ident(instance, &name.original)
        );
    }
}

pub(crate) fn emit_user_fb_statement(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    statement: &Statement,
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
    return_label: &str,
) {
    match statement {
        Statement::Empty => {}
        Statement::Assignment { target, value } => {
            let target_c = user_fb_var_to_c_typed(instance, target, field_types, project);
            if let Some(target_spec) = variable_spec(target, field_types, project) {
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
                        expr_to_c_for_user_fb(value, instance, field_types, project)
                    );
                    return;
                }
                if let Expr::Variable(source) = value {
                    if let Some(source_spec) = variable_spec(source, field_types, project) {
                        if array_storage_compatible(project, &target_spec, &source_spec) {
                            c_writeln!(
                                out,
                                "{pad}memcpy(s->{target_c}, {}, sizeof(s->{target_c}));",
                                expr_to_c_for_user_fb(value, instance, field_types, project)
                            );
                            return;
                        }
                        if struct_storage_compatible(project, &target_spec, &source_spec) {
                            c_writeln!(
                                out,
                                "{pad}memcpy(&s->{target_c}, &{}, sizeof(s->{target_c}));",
                                expr_to_c_for_user_fb(value, instance, field_types, project)
                            );
                            return;
                        }
                    }
                }
            }
            c_writeln!(
                out,
                "{pad}s->{} = {};",
                target_c,
                expr_to_c_for_user_fb(value, instance, field_types, project)
            );
        }
        Statement::If {
            branches,
            else_branch,
        } => {
            for (index, (condition, body)) in branches.iter().enumerate() {
                let condition = expr_to_c_for_user_fb(condition, instance, field_types, project);
                if index == 0 {
                    c_writeln!(out, "{pad}if ({condition}) {{");
                } else {
                    c_writeln!(out, "{pad}else if ({condition}) {{");
                }
                let nested_pad = format!("{pad}    ");
                for statement in body {
                    emit_user_fb_statement(
                        out,
                        &nested_pad,
                        instance,
                        statement,
                        field_types,
                        project,
                        return_label,
                    );
                }
                c_writeln!(out, "{pad}}}");
            }
            if !else_branch.is_empty() {
                c_writeln!(out, "{pad}else {{");
                let nested_pad = format!("{pad}    ");
                for statement in else_branch {
                    emit_user_fb_statement(
                        out,
                        &nested_pad,
                        instance,
                        statement,
                        field_types,
                        project,
                        return_label,
                    );
                }
                c_writeln!(out, "{pad}}}");
            }
        }
        Statement::Case {
            selector,
            cases,
            else_branch,
        } => emit_user_fb_case_statement(
            out,
            pad,
            instance,
            selector,
            cases,
            else_branch,
            field_types,
            project,
            return_label,
        ),
        Statement::For {
            control,
            from,
            to,
            by,
            body,
        } => {
            let control_c = user_fb_var_to_c_typed(
                instance,
                &VariableRef::named(control.original.clone()),
                field_types,
                project,
            );
            let step = by
                .as_ref()
                .map(|expr| expr_to_c_for_user_fb(expr, instance, field_types, project))
                .unwrap_or_else(|| "1".to_string());
            c_writeln!(out, "{pad}{{");
            c_writeln!(out, "{pad}    int64_t rbcpp_step = {step};");
            c_writeln!(
                out,
                "{pad}    for (s->{control_c} = {}; (rbcpp_step >= 0) ? (s->{control_c} <= {}) : (s->{control_c} >= {}); s->{control_c} += rbcpp_step) {{",
                expr_to_c_for_user_fb(from, instance, field_types, project),
                expr_to_c_for_user_fb(to, instance, field_types, project),
                expr_to_c_for_user_fb(to, instance, field_types, project)
            );
            let nested_pad = format!("{pad}        ");
            for statement in body {
                emit_user_fb_statement(
                    out,
                    &nested_pad,
                    instance,
                    statement,
                    field_types,
                    project,
                    return_label,
                );
            }
            c_writeln!(out, "{pad}    }}");
            c_writeln!(out, "{pad}}}");
        }
        Statement::While { condition, body } => {
            c_writeln!(
                out,
                "{pad}while ({}) {{",
                expr_to_c_for_user_fb(condition, instance, field_types, project)
            );
            let nested_pad = format!("{pad}    ");
            for statement in body {
                emit_user_fb_statement(
                    out,
                    &nested_pad,
                    instance,
                    statement,
                    field_types,
                    project,
                    return_label,
                );
            }
            c_writeln!(out, "{pad}}}");
        }
        Statement::Repeat { body, until } => {
            c_writeln!(out, "{pad}do {{");
            let nested_pad = format!("{pad}    ");
            for statement in body {
                emit_user_fb_statement(
                    out,
                    &nested_pad,
                    instance,
                    statement,
                    field_types,
                    project,
                    return_label,
                );
            }
            c_writeln!(
                out,
                "{pad}}} while (!({}));",
                expr_to_c_for_user_fb(until, instance, field_types, project)
            );
        }
        Statement::FbCall { name, args } => {
            if let Some(root) = name.root_name() {
                if is_standard_void_call_name(&root.original) {
                    emit_standard_void_call_user_fb(
                        out,
                        pad,
                        root,
                        args,
                        instance,
                        field_types,
                        project,
                    );
                } else {
                    emit_nested_user_fb_call(out, pad, instance, name, args, field_types, project);
                }
            } else {
                emit_nested_user_fb_call(out, pad, instance, name, args, field_types, project);
            }
        }
        Statement::Il { op, operand } => emit_user_fb_il_instruction(
            out,
            pad,
            *op,
            operand.as_ref(),
            instance,
            field_types,
            project,
            return_label,
        ),
        Statement::IlLabel(label) => {
            c_writeln!(out, "{}:;", user_fb_il_label_to_c(return_label, label));
        }
        Statement::Exit => {
            c_writeln!(out, "{pad}break;");
        }
        Statement::Return => {
            c_writeln!(out, "{pad}goto {return_label};");
        }
        _ => {
            c_writeln!(out, "{pad}/* function block statement not emitted yet */");
        }
    }
}

pub(crate) fn emit_user_fb_case_statement(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    selector: &Expr,
    cases: &[(Vec<CaseLabel>, Vec<Statement>)],
    else_branch: &[Statement],
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
    return_label: &str,
) {
    let selector_name = format!("rbcpp_case_{}", out.len());
    c_writeln!(
        out,
        "{pad}int64_t {selector_name} = {};",
        expr_to_c_for_user_fb(selector, instance, field_types, project)
    );
    for (index, (labels, body)) in cases.iter().enumerate() {
        let condition = labels
            .iter()
            .map(|label| {
                user_fb_case_label_to_c(label, &selector_name, instance, field_types, project)
            })
            .collect::<Vec<_>>()
            .join(" || ");
        if index == 0 {
            c_writeln!(out, "{pad}if ({condition}) {{");
        } else {
            c_writeln!(out, "{pad}else if ({condition}) {{");
        }
        let nested_pad = format!("{pad}    ");
        for statement in body {
            emit_user_fb_statement(
                out,
                &nested_pad,
                instance,
                statement,
                field_types,
                project,
                return_label,
            );
        }
        c_writeln!(out, "{pad}}}");
    }
    if !else_branch.is_empty() {
        c_writeln!(out, "{pad}else {{");
        let nested_pad = format!("{pad}    ");
        for statement in else_branch {
            emit_user_fb_statement(
                out,
                &nested_pad,
                instance,
                statement,
                field_types,
                project,
                return_label,
            );
        }
        c_writeln!(out, "{pad}}}");
    }
}

pub(crate) fn user_fb_case_label_to_c(
    label: &CaseLabel,
    selector_name: &str,
    instance: &str,
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> String {
    match label {
        CaseLabel::Single(expr) => {
            format!(
                "({selector_name} == {})",
                expr_to_c_for_user_fb(expr, instance, field_types, project)
            )
        }
        CaseLabel::Range(low, high) => format!(
            "({selector_name} >= {} && {selector_name} <= {})",
            expr_to_c_for_user_fb(low, instance, field_types, project),
            expr_to_c_for_user_fb(high, instance, field_types, project)
        ),
    }
}

pub(crate) fn emit_user_fb_il_instruction(
    out: &mut CEmitter<'_>,
    pad: &str,
    op: IlOp,
    operand: Option<&Expr>,
    instance: &str,
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
    return_label: &str,
) {
    let operand_c = operand
        .map(|expr| expr_to_c_for_user_fb(expr, instance, field_types, project))
        .unwrap_or_else(|| "0".to_string());
    match op {
        IlOp::Ld => c_writeln!(out, "{pad}rbcpp_acc = {operand_c};"),
        IlOp::Ldn => c_writeln!(out, "{pad}rbcpp_acc = !({operand_c});"),
        IlOp::St => {
            if let Some(Expr::Variable(target)) = operand {
                c_writeln!(
                    out,
                    "{pad}s->{} = rbcpp_acc;",
                    user_fb_var_to_c_typed(instance, target, field_types, project)
                );
            }
        }
        IlOp::Stn => {
            if let Some(Expr::Variable(target)) = operand {
                c_writeln!(
                    out,
                    "{pad}s->{} = !rbcpp_acc;",
                    user_fb_var_to_c_typed(instance, target, field_types, project)
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
                    user_fb_var_to_c_typed(instance, target, field_types, project)
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
                let label = user_fb_il_label_to_c(return_label, label);
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
                    IlOp::Cal => emit_nested_user_fb_call(
                        out,
                        pad,
                        instance,
                        &name,
                        &args,
                        field_types,
                        project,
                    ),
                    IlOp::Calc => {
                        c_writeln!(out, "{pad}if (rbcpp_acc) {{");
                        emit_nested_user_fb_call(
                            out,
                            &format!("{pad}    "),
                            instance,
                            &name,
                            &args,
                            field_types,
                            project,
                        );
                        c_writeln!(out, "{pad}}}");
                    }
                    IlOp::Calcn => {
                        c_writeln!(out, "{pad}if (!rbcpp_acc) {{");
                        emit_nested_user_fb_call(
                            out,
                            &format!("{pad}    "),
                            instance,
                            &name,
                            &args,
                            field_types,
                            project,
                        );
                        c_writeln!(out, "{pad}}}");
                    }
                    _ => {}
                }
            }
        }
        IlOp::Ret => c_writeln!(out, "{pad}goto {return_label};"),
        IlOp::Retc => c_writeln!(out, "{pad}if (rbcpp_acc) {{ goto {return_label}; }}"),
        IlOp::Retcn => c_writeln!(out, "{pad}if (!rbcpp_acc) {{ goto {return_label}; }}"),
    }
}

pub(crate) fn user_fb_il_label_to_c(prefix: &str, label: &Identifier) -> String {
    format!(
        "rbcpp_label_{}_{}",
        sanitize_c_ident(prefix),
        sanitize_c_ident(&label.original)
    )
}

pub(crate) fn emit_nested_user_fb_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    outer_instance: &str,
    name: &VariableRef,
    args: &[ParamAssignment],
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let Some(root) = name.root_name() else {
        c_writeln!(out, "{pad}/* invalid nested function block call */");
        return;
    };
    let Some(DataTypeSpec::Named(type_name)) = field_types.get(&root.canonical) else {
        c_writeln!(
            out,
            "{pad}/* unknown nested function block instance {} */",
            root.original
        );
        return;
    };
    let nested_instance = field_key_for_c(outer_instance, &root.original);
    let en_expr = args
        .iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
        .and_then(|arg| arg.expr.as_ref())
        .map(|expr| expr_to_c_for_user_fb(expr, outer_instance, field_types, project));
    let eno_arg = args.iter().find(|arg| {
        arg.output && arg.name.as_ref().is_some_and(is_implicit_eno) && arg.variable.is_some()
    });
    let outer_pad = pad;
    let body_pad_storage;
    let pad = if let Some(en_expr) = &en_expr {
        c_writeln!(out, "{outer_pad}if ({en_expr}) {{");
        body_pad_storage = format!("{outer_pad}    ");
        body_pad_storage.as_str()
    } else {
        outer_pad
    };

    if emit_standard_fb_body_with(
        out,
        pad,
        &nested_instance,
        &type_name.original,
        args,
        &|expr| expr_to_c_for_user_fb(expr, outer_instance, field_types, project),
    ) {
        emit_standard_fb_output_bindings_user_fb(
            out,
            pad,
            outer_instance,
            &nested_instance,
            &type_name.original,
            args,
            field_types,
            project,
        );
        emit_nested_fb_eno(
            out,
            pad,
            outer_pad,
            en_expr.is_some(),
            eno_arg,
            outer_instance,
            field_types,
            project,
            true,
        );
        return;
    }

    let Some(function_block) = project
        .find_pou(&type_name.original)
        .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
    else {
        c_writeln!(
            out,
            "{pad}/* nested function block type {} is not emitted yet */",
            type_name.original
        );
        emit_nested_fb_eno(
            out,
            pad,
            outer_pad,
            en_expr.is_some(),
            eno_arg,
            outer_instance,
            field_types,
            project,
            false,
        );
        return;
    };
    let input_fields = user_fb_input_fields(function_block);
    let nested_field_types = function_block
        .variable_declarations()
        .map(|var| (var.name.canonical.clone(), var.type_spec.clone()))
        .collect::<std::collections::BTreeMap<_, _>>();

    let mut positional_index = 0_usize;
    for arg in args {
        let (Some((_, name)), Some(expr)) = (
            user_fb_input_target(&input_fields, arg, &mut positional_index),
            &arg.expr,
        ) else {
            continue;
        };
        let target_field = fb_field_ident(&nested_instance, &name.original);
        if let Some(target_spec) = nested_field_types.get(&name.canonical) {
            if let Some(info) = c_text_info(project, target_spec) {
                let assign = if info.wide {
                    "rbcpp_wstrassign_utf8"
                } else {
                    "rbcpp_strassign"
                };
                c_writeln!(
                    out,
                    "{pad}{assign}(s->{target_field}, {}, {});",
                    info.capacity,
                    expr_to_c_for_user_fb(expr, outer_instance, field_types, project)
                );
                continue;
            }
            if let Expr::Variable(source) = expr {
                if let Some(source_spec) = variable_spec(source, field_types, project) {
                    if array_storage_compatible(project, target_spec, &source_spec) {
                        c_writeln!(
                            out,
                            "{pad}memcpy(s->{target_field}, {}, sizeof(s->{target_field}));",
                            expr_to_c_for_user_fb(expr, outer_instance, field_types, project)
                        );
                        continue;
                    }
                    if struct_storage_compatible(project, target_spec, &source_spec) {
                        c_writeln!(
                            out,
                            "{pad}memcpy(&s->{target_field}, &{}, sizeof(s->{target_field}));",
                            expr_to_c_for_user_fb(expr, outer_instance, field_types, project)
                        );
                        continue;
                    }
                }
            }
        }
        c_writeln!(
            out,
            "{pad}s->{} = {};",
            target_field,
            expr_to_c_for_user_fb(expr, outer_instance, field_types, project)
        );
    }

    c_writeln!(out, "{pad}do {{");
    let body_pad = format!("{pad}    ");
    let return_label = format!(
        "rbcpp_return_{}_{}",
        sanitize_c_ident(&nested_instance),
        out.len()
    );
    if statements_need_il_accumulator(&function_block.body.statements) {
        c_writeln!(out, "{body_pad}int64_t rbcpp_acc = 0;");
    }
    for statement in &function_block.body.statements {
        emit_user_fb_statement(
            out,
            &body_pad,
            &nested_instance,
            statement,
            &nested_field_types,
            project,
            &return_label,
        );
    }
    c_writeln!(out, "{body_pad}{return_label}:;");
    c_writeln!(out, "{pad}}} while (0);");

    let mut positional_index = 0_usize;
    for arg in args {
        let Some((kind, name)) = user_fb_input_target(&input_fields, arg, &mut positional_index)
        else {
            continue;
        };
        if kind != VarBlockKind::InOut {
            continue;
        }
        let Some(Expr::Variable(variable)) = &arg.expr else {
            continue;
        };
        if let (Some(target_spec), Some(source_spec)) = (
            variable_spec(variable, field_types, project),
            nested_field_types.get(&name.canonical),
        ) {
            if let Some(info) = c_text_info(project, &target_spec) {
                let assign = if info.wide {
                    "rbcpp_wstrassign_utf8"
                } else {
                    "rbcpp_strassign"
                };
                c_writeln!(
                    out,
                    "{pad}{assign}(s->{}, {}, s->{});",
                    user_fb_var_to_c_typed(outer_instance, variable, field_types, project),
                    info.capacity,
                    fb_field_ident(&nested_instance, &name.original)
                );
                continue;
            }
            if array_storage_compatible(project, &target_spec, source_spec) {
                let target = user_fb_var_to_c_typed(outer_instance, variable, field_types, project);
                let source = fb_field_ident(&nested_instance, &name.original);
                c_writeln!(
                    out,
                    "{pad}memcpy(s->{target}, s->{source}, sizeof(s->{target}));"
                );
                continue;
            }
            if struct_storage_compatible(project, &target_spec, source_spec) {
                let target = user_fb_var_to_c_typed(outer_instance, variable, field_types, project);
                let source = fb_field_ident(&nested_instance, &name.original);
                c_writeln!(
                    out,
                    "{pad}memcpy(&s->{target}, &s->{source}, sizeof(s->{target}));"
                );
                continue;
            }
        }
        c_writeln!(
            out,
            "{pad}s->{} = s->{};",
            user_fb_var_to_c_typed(outer_instance, variable, field_types, project),
            fb_field_ident(&nested_instance, &name.original)
        );
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
        if let (Some(target_spec), Some(source_spec)) = (
            variable_spec(variable, field_types, project),
            nested_field_types.get(&name.canonical),
        ) {
            if let Some(info) = c_text_info(project, &target_spec) {
                let assign = if info.wide {
                    "rbcpp_wstrassign_utf8"
                } else {
                    "rbcpp_strassign"
                };
                c_writeln!(
                    out,
                    "{pad}{assign}(s->{}, {}, s->{});",
                    user_fb_var_to_c_typed(outer_instance, variable, field_types, project),
                    info.capacity,
                    fb_field_ident(&nested_instance, &name.original)
                );
                continue;
            }
            if array_storage_compatible(project, &target_spec, source_spec) {
                let target = user_fb_var_to_c_typed(outer_instance, variable, field_types, project);
                let source = fb_field_ident(&nested_instance, &name.original);
                c_writeln!(
                    out,
                    "{pad}memcpy(s->{target}, s->{source}, sizeof(s->{target}));"
                );
                continue;
            }
            if struct_storage_compatible(project, &target_spec, source_spec) {
                let target = user_fb_var_to_c_typed(outer_instance, variable, field_types, project);
                let source = fb_field_ident(&nested_instance, &name.original);
                c_writeln!(
                    out,
                    "{pad}memcpy(&s->{target}, &s->{source}, sizeof(s->{target}));"
                );
                continue;
            }
        }
        c_writeln!(
            out,
            "{pad}s->{} = s->{};",
            user_fb_var_to_c_typed(outer_instance, variable, field_types, project),
            fb_field_ident(&nested_instance, &name.original)
        );
    }
    emit_nested_fb_eno(
        out,
        pad,
        outer_pad,
        en_expr.is_some(),
        eno_arg,
        outer_instance,
        field_types,
        project,
        true,
    );
}

pub(crate) fn emit_nested_fb_eno(
    out: &mut CEmitter<'_>,
    pad: &str,
    outer_pad: &str,
    gated: bool,
    eno_arg: Option<&ParamAssignment>,
    outer_instance: &str,
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
    success: bool,
) {
    if let Some(eno_arg) = eno_arg {
        let variable = eno_arg
            .variable
            .as_ref()
            .map(|variable| {
                format!(
                    "s->{}",
                    user_fb_var_to_c_typed(outer_instance, variable, field_types, project)
                )
            })
            .expect("ENO output has a variable");
        c_writeln!(
            out,
            "{pad}{variable} = {};",
            eno_bool_value(eno_arg, success)
        );
    }
    if gated {
        c_writeln!(out, "{outer_pad}}}");
        if let Some(eno_arg) = eno_arg {
            let variable = eno_arg
                .variable
                .as_ref()
                .map(|variable| {
                    format!(
                        "s->{}",
                        user_fb_var_to_c_typed(outer_instance, variable, field_types, project)
                    )
                })
                .expect("ENO output has a variable");
            c_writeln!(
                out,
                "{outer_pad}else {{ {variable} = {}; }}",
                eno_bool_value(eno_arg, false)
            );
        }
    }
}

pub(crate) fn expr_to_c_for_user_fb(
    expr: &Expr,
    instance: &str,
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> String {
    match expr {
        Expr::Literal(literal) => literal_to_c_project(project, literal),
        Expr::Variable(variable) => {
            if let Some(root) = variable.root_name() {
                if variable.path.len() == 1 {
                    if let Some(ordinal) = enum_ordinal_name(project, &root.canonical) {
                        return ordinal.to_string();
                    }
                }
            }
            let rendered = format!(
                "s->{}",
                user_fb_var_to_c_typed(instance, variable, field_types, project)
            );
            if variable_spec(variable, field_types, project)
                .and_then(|spec| c_text_info(project, &spec))
                .is_some_and(|info| info.wide)
            {
                format!("rbcpp_wstr_to_utf8({rendered})")
            } else {
                rendered
            }
        }
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg => {
                format!(
                    "(-{})",
                    expr_to_c_for_user_fb(expr, instance, field_types, project)
                )
            }
            UnaryOp::Not => {
                let expr_c = expr_to_c_for_user_fb(expr, instance, field_types, project);
                if expr_is_bool_for_c(expr, field_types, project) {
                    format!("!({expr_c})")
                } else {
                    format!("~({expr_c})")
                }
            }
        },
        Expr::Binary { op, left, right } => {
            if *op == BinaryOp::Power {
                return format!(
                    "pow({}, {})",
                    expr_to_c_for_user_fb(left, instance, field_types, project),
                    expr_to_c_for_user_fb(right, instance, field_types, project)
                );
            }
            let op_c = binary_op_to_c_state(*op, left, right, field_types, project);
            format!(
                "({} {} {})",
                expr_to_c_for_user_fb(left, instance, field_types, project),
                op_c,
                expr_to_c_for_user_fb(right, instance, field_types, project)
            )
        }
        Expr::Call { name, args } => {
            let call_args = if is_standard_function(&name.original) {
                ordered_call_input_args(&name.original, args, |expr| {
                    expr_to_c_for_user_fb(expr, instance, field_types, project)
                })
            } else {
                ordered_user_function_call_input_args(project, &name.original, args, |expr| {
                    expr_to_c_for_user_fb(expr, instance, field_types, project)
                })
            };
            let call =
                standard_call_to_c_state(&name.original, args, &call_args, field_types, project)
                    .unwrap_or_else(|| {
                        format!(
                            "{}({})",
                            sanitize_c_ident(&name.original),
                            call_args.join(", ")
                        )
                    });
            let disabled_default = disabled_call_default_to_c_project(project, &name.original);
            wrap_call_controls_to_c_user_fb(
                call,
                args,
                instance,
                field_types,
                project,
                &disabled_default,
            )
        }
        Expr::ArrayLiteral(_) | Expr::StructLiteral(_) => "0".to_string(),
    }
}

pub(crate) fn wrap_call_controls_to_c_user_fb(
    call: String,
    args: &[ParamAssignment],
    instance: &str,
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
    disabled_default: &str,
) -> String {
    let en = args
        .iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
        .and_then(|arg| arg.expr.as_ref())
        .map(|expr| expr_to_c_for_user_fb(expr, instance, field_types, project));
    let eno = args.iter().find(|arg| {
        arg.output && arg.name.as_ref().is_some_and(is_implicit_eno) && arg.variable.is_some()
    });

    match (en, eno) {
        (None, None) => call,
        (Some(en), None) => format!("(({en}) ? ({call}) : {disabled_default})"),
        (None, Some(eno)) => {
            let variable = eno
                .variable
                .as_ref()
                .map(|variable| {
                    format!(
                        "s->{}",
                        user_fb_var_to_c_typed(instance, variable, field_types, project)
                    )
                })
                .expect("ENO output has a variable");
            format!("({variable} = {}, ({call}))", eno_bool_value(eno, true))
        }
        (Some(en), Some(eno)) => {
            let variable = eno
                .variable
                .as_ref()
                .map(|variable| {
                    format!(
                        "s->{}",
                        user_fb_var_to_c_typed(instance, variable, field_types, project)
                    )
                })
                .expect("ENO output has a variable");
            let true_value = eno_bool_value(eno, true);
            let false_value = eno_bool_value(eno, false);
            format!(
                "(({en}) ? ({variable} = {true_value}, ({call})) : ({variable} = {false_value}, {disabled_default}))"
            )
        }
    }
}

pub(crate) fn user_fb_var_to_c(instance: &str, variable: &VariableRef) -> String {
    if variable.direct.is_some() {
        return var_to_c(variable);
    }
    if variable.path.len() == 1 {
        fb_field_ident(instance, &variable.path[0].original)
    } else {
        let field_path = variable
            .path
            .iter()
            .map(|part| sanitize_c_ident(&part.original))
            .collect::<Vec<_>>()
            .join(".");
        fb_field_ident(instance, &field_path)
    }
}

pub(crate) fn user_fb_var_to_c_typed(
    instance: &str,
    variable: &VariableRef,
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> String {
    if variable.direct.is_some() {
        return var_to_c(variable);
    }
    let Some(root) = variable.root_name() else {
        return "_".to_string();
    };
    let Some(root_spec) = field_types.get(&root.canonical) else {
        return user_fb_var_to_c(instance, variable);
    };

    if variable.path.len() > 1
        && (standard_fb_fields(root_spec).is_some()
            || user_function_block(project, root_spec).is_some())
    {
        return user_fb_var_to_c(instance, variable);
    }

    let mut text = fb_field_ident(instance, &root.original);
    let mut current_spec = root_spec.clone();
    current_spec = append_indices_to_c(
        &mut text,
        &current_spec,
        variable.indices.first().map(Vec::as_slice).unwrap_or(&[]),
        project,
        &|expr| expr_to_c_for_user_fb(expr, instance, field_types, project),
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
        current_spec = append_indices_to_c(
            &mut text,
            &field_spec,
            variable
                .indices
                .get(segment_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]),
            project,
            &|expr| expr_to_c_for_user_fb(expr, instance, field_types, project),
        );
    }

    text
}

pub(crate) fn emit_ton_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let input = fb_arg_for_block(args, "TON", "IN", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let pt =
        fb_arg_for_block(args, "TON", "PT", var_types, project).unwrap_or_else(|| "0".to_string());
    c_writeln!(out, "{pad}if (!({input})) {{");
    c_writeln!(
        out,
        "{pad}    s->{} = false;",
        fb_field_ident(instance, "Q")
    );
    c_writeln!(out, "{pad}    s->{} = 0;", fb_field_ident(instance, "ET"));
    c_writeln!(out, "{pad}}} else {{");
    c_writeln!(
        out,
        "{pad}    s->{} = RBCPP_MIN(s->{} + RBCPP_CYCLE_MS, ({pt}));",
        fb_field_ident(instance, "ET"),
        fb_field_ident(instance, "ET")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = s->{} >= ({pt});",
        fb_field_ident(instance, "Q"),
        fb_field_ident(instance, "ET")
    );
    c_writeln!(out, "{pad}}}");
    c_writeln!(
        out,
        "{pad}s->{} = ({input});",
        fb_field_ident(instance, "_IN")
    );
}

pub(crate) fn emit_tof_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let input = fb_arg_for_block(args, "TOF", "IN", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let pt =
        fb_arg_for_block(args, "TOF", "PT", var_types, project).unwrap_or_else(|| "0".to_string());
    c_writeln!(out, "{pad}if ({input}) {{");
    c_writeln!(out, "{pad}    s->{} = true;", fb_field_ident(instance, "Q"));
    c_writeln!(out, "{pad}    s->{} = 0;", fb_field_ident(instance, "ET"));
    c_writeln!(
        out,
        "{pad}}} else if (s->{}) {{",
        fb_field_ident(instance, "Q")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = RBCPP_MIN(s->{} + RBCPP_CYCLE_MS, ({pt}));",
        fb_field_ident(instance, "ET"),
        fb_field_ident(instance, "ET")
    );
    c_writeln!(
        out,
        "{pad}    if (s->{} >= ({pt})) {{",
        fb_field_ident(instance, "ET")
    );
    c_writeln!(
        out,
        "{pad}        s->{} = false;",
        fb_field_ident(instance, "Q")
    );
    c_writeln!(out, "{pad}    }}");
    c_writeln!(out, "{pad}}}");
    c_writeln!(
        out,
        "{pad}s->{} = ({input});",
        fb_field_ident(instance, "_IN")
    );
}

pub(crate) fn emit_tp_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let input = fb_arg_for_block(args, "TP", "IN", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let pt =
        fb_arg_for_block(args, "TP", "PT", var_types, project).unwrap_or_else(|| "0".to_string());
    let tmp = format!("rbcpp_{}_in", sanitize_c_ident(instance));
    c_writeln!(out, "{pad}bool {tmp} = ({input});");
    c_writeln!(
        out,
        "{pad}if ({tmp} && !s->{} && !s->{}) {{",
        fb_field_ident(instance, "_IN"),
        fb_field_ident(instance, "_RUN")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = true;",
        fb_field_ident(instance, "_RUN")
    );
    c_writeln!(out, "{pad}    s->{} = 0;", fb_field_ident(instance, "ET"));
    c_writeln!(out, "{pad}    s->{} = true;", fb_field_ident(instance, "Q"));
    c_writeln!(out, "{pad}}}");
    c_writeln!(out, "{pad}if (s->{}) {{", fb_field_ident(instance, "_RUN"));
    c_writeln!(
        out,
        "{pad}    s->{} = RBCPP_MIN(s->{} + RBCPP_CYCLE_MS, ({pt}));",
        fb_field_ident(instance, "ET"),
        fb_field_ident(instance, "ET")
    );
    c_writeln!(
        out,
        "{pad}    if (s->{} >= ({pt})) {{",
        fb_field_ident(instance, "ET")
    );
    c_writeln!(
        out,
        "{pad}        s->{} = false;",
        fb_field_ident(instance, "_RUN")
    );
    c_writeln!(
        out,
        "{pad}        s->{} = false;",
        fb_field_ident(instance, "Q")
    );
    c_writeln!(out, "{pad}    }} else {{");
    c_writeln!(
        out,
        "{pad}        s->{} = true;",
        fb_field_ident(instance, "Q")
    );
    c_writeln!(out, "{pad}    }}");
    c_writeln!(out, "{pad}}} else {{");
    c_writeln!(
        out,
        "{pad}    s->{} = false;",
        fb_field_ident(instance, "Q")
    );
    c_writeln!(out, "{pad}}}");
    c_writeln!(out, "{pad}s->{} = {tmp};", fb_field_ident(instance, "_IN"));
}

pub(crate) fn emit_ctu_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let cu = fb_arg_for_block(args, "CTU", "CU", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let r = fb_arg_for_block(args, "CTU", "R", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let pv =
        fb_arg_for_block(args, "CTU", "PV", var_types, project).unwrap_or_else(|| "0".to_string());
    let tmp = format!("rbcpp_{}_cu", sanitize_c_ident(instance));
    c_writeln!(out, "{pad}bool {tmp} = ({cu});");
    c_writeln!(out, "{pad}if ({r}) {{");
    c_writeln!(out, "{pad}    s->{} = 0;", fb_field_ident(instance, "CV"));
    c_writeln!(out, "{pad}}} else {{");
    c_writeln!(
        out,
        "{pad}    if ({tmp} && !s->{}) {{ s->{} += 1; }}",
        fb_field_ident(instance, "_CU"),
        fb_field_ident(instance, "CV")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = {tmp};",
        fb_field_ident(instance, "_CU")
    );
    c_writeln!(out, "{pad}}}");
    c_writeln!(
        out,
        "{pad}s->{} = s->{} >= ({pv});",
        fb_field_ident(instance, "Q"),
        fb_field_ident(instance, "CV")
    );
}

pub(crate) fn emit_ctd_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let cd = fb_arg_for_block(args, "CTD", "CD", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let ld = fb_arg_for_block(args, "CTD", "LD", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let pv =
        fb_arg_for_block(args, "CTD", "PV", var_types, project).unwrap_or_else(|| "0".to_string());
    let tmp = format!("rbcpp_{}_cd", sanitize_c_ident(instance));
    c_writeln!(out, "{pad}bool {tmp} = ({cd});");
    c_writeln!(out, "{pad}if ({ld}) {{");
    c_writeln!(
        out,
        "{pad}    s->{} = ({pv});",
        fb_field_ident(instance, "CV")
    );
    c_writeln!(out, "{pad}}} else {{");
    c_writeln!(
        out,
        "{pad}    if ({tmp} && !s->{}) {{ s->{} -= 1; }}",
        fb_field_ident(instance, "_CD"),
        fb_field_ident(instance, "CV")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = {tmp};",
        fb_field_ident(instance, "_CD")
    );
    c_writeln!(out, "{pad}}}");
    c_writeln!(
        out,
        "{pad}s->{} = s->{} <= 0;",
        fb_field_ident(instance, "Q"),
        fb_field_ident(instance, "CV")
    );
}

pub(crate) fn emit_ctud_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let cu = fb_arg_for_block(args, "CTUD", "CU", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let cd = fb_arg_for_block(args, "CTUD", "CD", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let r = fb_arg_for_block(args, "CTUD", "R", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let ld = fb_arg_for_block(args, "CTUD", "LD", var_types, project)
        .unwrap_or_else(|| "false".to_string());
    let pv =
        fb_arg_for_block(args, "CTUD", "PV", var_types, project).unwrap_or_else(|| "0".to_string());
    let tmp_cu = format!("rbcpp_{}_cu", sanitize_c_ident(instance));
    let tmp_cd = format!("rbcpp_{}_cd", sanitize_c_ident(instance));
    c_writeln!(out, "{pad}bool {tmp_cu} = ({cu});");
    c_writeln!(out, "{pad}bool {tmp_cd} = ({cd});");
    c_writeln!(out, "{pad}if ({r}) {{");
    c_writeln!(out, "{pad}    s->{} = 0;", fb_field_ident(instance, "CV"));
    c_writeln!(out, "{pad}}} else if ({ld}) {{");
    c_writeln!(
        out,
        "{pad}    s->{} = ({pv});",
        fb_field_ident(instance, "CV")
    );
    c_writeln!(out, "{pad}}} else {{");
    c_writeln!(
        out,
        "{pad}    if ({tmp_cu} && !s->{} && !({tmp_cd} && !s->{})) {{ s->{} += 1; }}",
        fb_field_ident(instance, "_CU"),
        fb_field_ident(instance, "_CD"),
        fb_field_ident(instance, "CV")
    );
    c_writeln!(
        out,
        "{pad}    else if ({tmp_cd} && !s->{} && !({tmp_cu} && !s->{})) {{ s->{} -= 1; }}",
        fb_field_ident(instance, "_CD"),
        fb_field_ident(instance, "_CU"),
        fb_field_ident(instance, "CV")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = {tmp_cu};",
        fb_field_ident(instance, "_CU")
    );
    c_writeln!(
        out,
        "{pad}    s->{} = {tmp_cd};",
        fb_field_ident(instance, "_CD")
    );
    c_writeln!(out, "{pad}}}");
    c_writeln!(
        out,
        "{pad}s->{} = s->{} >= ({pv});",
        fb_field_ident(instance, "QU"),
        fb_field_ident(instance, "CV")
    );
    c_writeln!(
        out,
        "{pad}s->{} = s->{} <= 0;",
        fb_field_ident(instance, "QD"),
        fb_field_ident(instance, "CV")
    );
}

pub(crate) fn fb_arg_for_block(
    args: &[ParamAssignment],
    block: &str,
    name: &str,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> Option<String> {
    fb_arg_render_for_block(args, block, name, &|expr| {
        expr_to_c_state(expr, var_types, project)
    })
}

pub(crate) fn fb_arg_render_for_block<F>(
    args: &[ParamAssignment],
    block: &str,
    name: &str,
    render: &F,
) -> Option<String>
where
    F: Fn(&Expr) -> String,
{
    let canonical = canonical_identifier(name);
    if let Some(value) = args
        .iter()
        .find(|arg| {
            !arg.output
                && arg
                    .name
                    .as_ref()
                    .is_some_and(|arg_name| arg_name.canonical == canonical)
        })
        .and_then(|arg| arg.expr.as_ref())
        .map(render)
    {
        return Some(value);
    }

    let index = standard_fb_input_names(block)
        .iter()
        .position(|field| canonical_identifier(field) == canonical)?;
    args.iter()
        .filter(|arg| !arg.output && arg.name.is_none())
        .nth(index)
        .and_then(|arg| arg.expr.as_ref())
        .map(render)
}

pub(crate) fn emit_standard_fb_body_with<F>(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    block_type: &str,
    args: &[ParamAssignment],
    render: &F,
) -> bool
where
    F: Fn(&Expr) -> String,
{
    match canonical_identifier(block_type).as_str() {
        "SR" => {
            let s1 = fb_arg_render_for_block(args, block_type, "S1", render)
                .unwrap_or_else(|| "false".to_string());
            let r = fb_arg_render_for_block(args, block_type, "R", render)
                .unwrap_or_else(|| "false".to_string());
            c_writeln!(
                out,
                "{pad}s->{} = ({s1}) || (s->{} && !({r}));",
                fb_field_ident(instance, "Q1"),
                fb_field_ident(instance, "Q1")
            );
        }
        "RS" => {
            let s = fb_arg_render_for_block(args, block_type, "S", render)
                .unwrap_or_else(|| "false".to_string());
            let r1 = fb_arg_render_for_block(args, block_type, "R1", render)
                .unwrap_or_else(|| "false".to_string());
            c_writeln!(
                out,
                "{pad}s->{} = (s->{} || ({s})) && !({r1});",
                fb_field_ident(instance, "Q1"),
                fb_field_ident(instance, "Q1")
            );
        }
        "R_TRIG" => {
            let clk = fb_arg_render_for_block(args, block_type, "CLK", render)
                .unwrap_or_else(|| "false".to_string());
            let tmp = format!("rbcpp_{}_clk", sanitize_c_ident(instance));
            c_writeln!(out, "{pad}bool {tmp} = ({clk});");
            c_writeln!(
                out,
                "{pad}s->{} = {tmp} && !s->{};",
                fb_field_ident(instance, "Q"),
                fb_field_ident(instance, "M")
            );
            c_writeln!(out, "{pad}s->{} = {tmp};", fb_field_ident(instance, "M"));
        }
        "F_TRIG" => {
            let clk = fb_arg_render_for_block(args, block_type, "CLK", render)
                .unwrap_or_else(|| "false".to_string());
            let tmp = format!("rbcpp_{}_clk", sanitize_c_ident(instance));
            c_writeln!(out, "{pad}bool {tmp} = ({clk});");
            c_writeln!(
                out,
                "{pad}s->{} = !{tmp} && s->{};",
                fb_field_ident(instance, "Q"),
                fb_field_ident(instance, "M")
            );
            c_writeln!(out, "{pad}s->{} = {tmp};", fb_field_ident(instance, "M"));
        }
        "CTU" => {
            let cu = fb_arg_render_for_block(args, block_type, "CU", render)
                .unwrap_or_else(|| "false".to_string());
            let r = fb_arg_render_for_block(args, block_type, "R", render)
                .unwrap_or_else(|| "false".to_string());
            let pv = fb_arg_render_for_block(args, block_type, "PV", render)
                .unwrap_or_else(|| "0".to_string());
            let tmp = format!("rbcpp_{}_cu", sanitize_c_ident(instance));
            c_writeln!(out, "{pad}bool {tmp} = ({cu});");
            c_writeln!(out, "{pad}if ({r}) {{");
            c_writeln!(out, "{pad}    s->{} = 0;", fb_field_ident(instance, "CV"));
            c_writeln!(out, "{pad}}} else {{");
            c_writeln!(
                out,
                "{pad}    if ({tmp} && !s->{}) {{ s->{} += 1; }}",
                fb_field_ident(instance, "_CU"),
                fb_field_ident(instance, "CV")
            );
            c_writeln!(
                out,
                "{pad}    s->{} = {tmp};",
                fb_field_ident(instance, "_CU")
            );
            c_writeln!(out, "{pad}}}");
            c_writeln!(
                out,
                "{pad}s->{} = s->{} >= ({pv});",
                fb_field_ident(instance, "Q"),
                fb_field_ident(instance, "CV")
            );
        }
        "CTD" => {
            let cd = fb_arg_render_for_block(args, block_type, "CD", render)
                .unwrap_or_else(|| "false".to_string());
            let ld = fb_arg_render_for_block(args, block_type, "LD", render)
                .unwrap_or_else(|| "false".to_string());
            let pv = fb_arg_render_for_block(args, block_type, "PV", render)
                .unwrap_or_else(|| "0".to_string());
            let tmp = format!("rbcpp_{}_cd", sanitize_c_ident(instance));
            c_writeln!(out, "{pad}bool {tmp} = ({cd});");
            c_writeln!(out, "{pad}if ({ld}) {{");
            c_writeln!(
                out,
                "{pad}    s->{} = ({pv});",
                fb_field_ident(instance, "CV")
            );
            c_writeln!(out, "{pad}}} else {{");
            c_writeln!(
                out,
                "{pad}    if ({tmp} && !s->{}) {{ s->{} -= 1; }}",
                fb_field_ident(instance, "_CD"),
                fb_field_ident(instance, "CV")
            );
            c_writeln!(
                out,
                "{pad}    s->{} = {tmp};",
                fb_field_ident(instance, "_CD")
            );
            c_writeln!(out, "{pad}}}");
            c_writeln!(
                out,
                "{pad}s->{} = s->{} <= 0;",
                fb_field_ident(instance, "Q"),
                fb_field_ident(instance, "CV")
            );
        }
        "CTUD" => {
            let cu = fb_arg_render_for_block(args, block_type, "CU", render)
                .unwrap_or_else(|| "false".to_string());
            let cd = fb_arg_render_for_block(args, block_type, "CD", render)
                .unwrap_or_else(|| "false".to_string());
            let r = fb_arg_render_for_block(args, block_type, "R", render)
                .unwrap_or_else(|| "false".to_string());
            let ld = fb_arg_render_for_block(args, block_type, "LD", render)
                .unwrap_or_else(|| "false".to_string());
            let pv = fb_arg_render_for_block(args, block_type, "PV", render)
                .unwrap_or_else(|| "0".to_string());
            let tmp_cu = format!("rbcpp_{}_cu", sanitize_c_ident(instance));
            let tmp_cd = format!("rbcpp_{}_cd", sanitize_c_ident(instance));
            c_writeln!(out, "{pad}bool {tmp_cu} = ({cu});");
            c_writeln!(out, "{pad}bool {tmp_cd} = ({cd});");
            c_writeln!(out, "{pad}if ({r}) {{");
            c_writeln!(out, "{pad}    s->{} = 0;", fb_field_ident(instance, "CV"));
            c_writeln!(out, "{pad}}} else if ({ld}) {{");
            c_writeln!(
                out,
                "{pad}    s->{} = ({pv});",
                fb_field_ident(instance, "CV")
            );
            c_writeln!(out, "{pad}}} else {{");
            c_writeln!(
                out,
                "{pad}    if ({tmp_cu} && !s->{} && !({tmp_cd} && !s->{})) {{ s->{} += 1; }}",
                fb_field_ident(instance, "_CU"),
                fb_field_ident(instance, "_CD"),
                fb_field_ident(instance, "CV")
            );
            c_writeln!(
                out,
                "{pad}    else if ({tmp_cd} && !s->{} && !({tmp_cu} && !s->{})) {{ s->{} -= 1; }}",
                fb_field_ident(instance, "_CD"),
                fb_field_ident(instance, "_CU"),
                fb_field_ident(instance, "CV")
            );
            c_writeln!(
                out,
                "{pad}    s->{} = {tmp_cu};",
                fb_field_ident(instance, "_CU")
            );
            c_writeln!(
                out,
                "{pad}    s->{} = {tmp_cd};",
                fb_field_ident(instance, "_CD")
            );
            c_writeln!(out, "{pad}}}");
            c_writeln!(
                out,
                "{pad}s->{} = s->{} >= ({pv});",
                fb_field_ident(instance, "QU"),
                fb_field_ident(instance, "CV")
            );
            c_writeln!(
                out,
                "{pad}s->{} = s->{} <= 0;",
                fb_field_ident(instance, "QD"),
                fb_field_ident(instance, "CV")
            );
        }
        "TON" => {
            let input = fb_arg_render_for_block(args, block_type, "IN", render)
                .unwrap_or_else(|| "false".to_string());
            let pt = fb_arg_render_for_block(args, block_type, "PT", render)
                .unwrap_or_else(|| "0".to_string());
            c_writeln!(out, "{pad}if (!({input})) {{");
            c_writeln!(
                out,
                "{pad}    s->{} = false;",
                fb_field_ident(instance, "Q")
            );
            c_writeln!(out, "{pad}    s->{} = 0;", fb_field_ident(instance, "ET"));
            c_writeln!(out, "{pad}}} else {{");
            c_writeln!(
                out,
                "{pad}    s->{} = RBCPP_MIN(s->{} + RBCPP_CYCLE_MS, ({pt}));",
                fb_field_ident(instance, "ET"),
                fb_field_ident(instance, "ET")
            );
            c_writeln!(
                out,
                "{pad}    s->{} = s->{} >= ({pt});",
                fb_field_ident(instance, "Q"),
                fb_field_ident(instance, "ET")
            );
            c_writeln!(out, "{pad}}}");
            c_writeln!(
                out,
                "{pad}s->{} = ({input});",
                fb_field_ident(instance, "_IN")
            );
        }
        "TOF" => {
            let input = fb_arg_render_for_block(args, block_type, "IN", render)
                .unwrap_or_else(|| "false".to_string());
            let pt = fb_arg_render_for_block(args, block_type, "PT", render)
                .unwrap_or_else(|| "0".to_string());
            c_writeln!(out, "{pad}if ({input}) {{");
            c_writeln!(out, "{pad}    s->{} = true;", fb_field_ident(instance, "Q"));
            c_writeln!(out, "{pad}    s->{} = 0;", fb_field_ident(instance, "ET"));
            c_writeln!(
                out,
                "{pad}}} else if (s->{}) {{",
                fb_field_ident(instance, "Q")
            );
            c_writeln!(
                out,
                "{pad}    s->{} = RBCPP_MIN(s->{} + RBCPP_CYCLE_MS, ({pt}));",
                fb_field_ident(instance, "ET"),
                fb_field_ident(instance, "ET")
            );
            c_writeln!(
                out,
                "{pad}    if (s->{} >= ({pt})) {{",
                fb_field_ident(instance, "ET")
            );
            c_writeln!(
                out,
                "{pad}        s->{} = false;",
                fb_field_ident(instance, "Q")
            );
            c_writeln!(out, "{pad}    }}");
            c_writeln!(out, "{pad}}}");
            c_writeln!(
                out,
                "{pad}s->{} = ({input});",
                fb_field_ident(instance, "_IN")
            );
        }
        "TP" => {
            let input = fb_arg_render_for_block(args, block_type, "IN", render)
                .unwrap_or_else(|| "false".to_string());
            let pt = fb_arg_render_for_block(args, block_type, "PT", render)
                .unwrap_or_else(|| "0".to_string());
            let tmp = format!("rbcpp_{}_in", sanitize_c_ident(instance));
            c_writeln!(out, "{pad}bool {tmp} = ({input});");
            c_writeln!(
                out,
                "{pad}if ({tmp} && !s->{} && !s->{}) {{",
                fb_field_ident(instance, "_IN"),
                fb_field_ident(instance, "_RUN")
            );
            c_writeln!(
                out,
                "{pad}    s->{} = true;",
                fb_field_ident(instance, "_RUN")
            );
            c_writeln!(out, "{pad}    s->{} = 0;", fb_field_ident(instance, "ET"));
            c_writeln!(out, "{pad}    s->{} = true;", fb_field_ident(instance, "Q"));
            c_writeln!(out, "{pad}}}");
            c_writeln!(out, "{pad}if (s->{}) {{", fb_field_ident(instance, "_RUN"));
            c_writeln!(
                out,
                "{pad}    s->{} = RBCPP_MIN(s->{} + RBCPP_CYCLE_MS, ({pt}));",
                fb_field_ident(instance, "ET"),
                fb_field_ident(instance, "ET")
            );
            c_writeln!(
                out,
                "{pad}    if (s->{} >= ({pt})) {{",
                fb_field_ident(instance, "ET")
            );
            c_writeln!(
                out,
                "{pad}        s->{} = false;",
                fb_field_ident(instance, "_RUN")
            );
            c_writeln!(
                out,
                "{pad}        s->{} = false;",
                fb_field_ident(instance, "Q")
            );
            c_writeln!(out, "{pad}    }} else {{");
            c_writeln!(
                out,
                "{pad}        s->{} = true;",
                fb_field_ident(instance, "Q")
            );
            c_writeln!(out, "{pad}    }}");
            c_writeln!(out, "{pad}}} else {{");
            c_writeln!(
                out,
                "{pad}    s->{} = false;",
                fb_field_ident(instance, "Q")
            );
            c_writeln!(out, "{pad}}}");
            c_writeln!(out, "{pad}s->{} = {tmp};", fb_field_ident(instance, "_IN"));
        }
        _ => return false,
    }
    true
}

pub(crate) fn standard_fb_public_output(block_type: &str, field: &str) -> bool {
    let field = canonical_identifier(field);
    match canonical_identifier(block_type).as_str() {
        "SR" | "RS" => field == "Q1",
        "R_TRIG" | "F_TRIG" => field == "Q",
        "CTU" | "CTD" => matches!(field.as_str(), "Q" | "CV"),
        "CTUD" => matches!(field.as_str(), "QU" | "QD" | "CV"),
        "TON" | "TOF" | "TP" => matches!(field.as_str(), "Q" | "ET"),
        name if is_communication_fb_name(name) => {
            matches!(field.as_str(), "DONE" | "NDR" | "ERROR" | "STATUS")
        }
        _ => false,
    }
}

pub(crate) fn emit_standard_fb_output_bindings_state(
    out: &mut CEmitter<'_>,
    pad: &str,
    instance: &str,
    block_type: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    for arg in args {
        if !arg.output {
            continue;
        }
        let (Some(name), Some(variable)) = (&arg.name, &arg.variable) else {
            continue;
        };
        if is_implicit_eno(name) || !standard_fb_public_output(block_type, &name.original) {
            continue;
        }
        let mut value = format!("s->{}", fb_field_ident(instance, &name.original));
        if arg.negated {
            value = format!("!({value})");
        }
        c_writeln!(
            out,
            "{pad}s->{} = {value};",
            var_to_c_state(variable, var_types, project)
        );
    }
}

pub(crate) fn emit_standard_fb_output_bindings_user_fb(
    out: &mut CEmitter<'_>,
    pad: &str,
    outer_instance: &str,
    nested_instance: &str,
    block_type: &str,
    args: &[ParamAssignment],
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    for arg in args {
        if !arg.output {
            continue;
        }
        let (Some(name), Some(variable)) = (&arg.name, &arg.variable) else {
            continue;
        };
        if is_implicit_eno(name) || !standard_fb_public_output(block_type, &name.original) {
            continue;
        }
        let mut value = format!("s->{}", fb_field_ident(nested_instance, &name.original));
        if arg.negated {
            value = format!("!({value})");
        }
        c_writeln!(
            out,
            "{pad}s->{} = {value};",
            user_fb_var_to_c_typed(outer_instance, variable, field_types, project)
        );
    }
}

pub(crate) fn emit_standard_void_call(
    out: &mut CEmitter<'_>,
    pad: &str,
    name: &Identifier,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let Some(input) = split_input_expr(args) else {
        c_writeln!(out, "{pad}/* missing input for {} */", name.original);
        return;
    };
    let input_c = expr_to_c_state(input, var_types, project);
    let outputs: &[&str] = match name.canonical.as_str() {
        "SPLIT_DATE" => &["YEAR", "MONTH", "DATE"],
        "SPLIT_TOD" => &["HOUR", "MINUTE", "SECOND", "MILLISECOND"],
        "SPLIT_DT" => &[
            "YEAR",
            "MONTH",
            "DATE",
            "HOUR",
            "MINUTE",
            "SECOND",
            "MILLISECOND",
        ],
        _ => return,
    };
    let helper = match name.canonical.as_str() {
        "SPLIT_DATE" => format!("rbcpp_civil_from_days({input_c})"),
        "SPLIT_TOD" => format!("rbcpp_split_tod_parts({input_c})"),
        "SPLIT_DT" => format!("rbcpp_split_dt_parts({input_c})"),
        _ => unreachable!(),
    };
    let en_expr = args
        .iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
        .and_then(|arg| arg.expr.as_ref())
        .map(|expr| expr_to_c_state(expr, var_types, project));
    let eno_arg = args.iter().find(|arg| {
        arg.output && arg.name.as_ref().is_some_and(is_implicit_eno) && arg.variable.is_some()
    });
    let outer_pad = pad;
    let body_pad_storage;
    let pad = if let Some(en_expr) = &en_expr {
        c_writeln!(out, "{outer_pad}if ({en_expr}) {{");
        body_pad_storage = format!("{outer_pad}    ");
        body_pad_storage.as_str()
    } else {
        outer_pad
    };
    c_writeln!(out, "{pad}{{");
    c_writeln!(out, "{pad}    rbcpp_datetime_parts rbcpp_split = {helper};");
    for (index, output) in outputs.iter().enumerate() {
        let Some(variable) = split_output_variable(args, output, index) else {
            continue;
        };
        c_writeln!(
            out,
            "{pad}    s->{} = rbcpp_split.{};",
            var_to_c_state(variable, var_types, project),
            split_output_field(output)
        );
    }
    c_writeln!(out, "{pad}}}");
    if let Some(eno_arg) = eno_arg {
        let variable = eno_arg
            .variable
            .as_ref()
            .map(|variable| format!("s->{}", var_to_c_state(variable, var_types, project)))
            .expect("ENO output has a variable");
        c_writeln!(out, "{pad}{variable} = {};", eno_bool_value(eno_arg, true));
    }
    if en_expr.is_some() {
        c_writeln!(out, "{outer_pad}}}");
        if let Some(eno_arg) = eno_arg {
            let variable = eno_arg
                .variable
                .as_ref()
                .map(|variable| format!("s->{}", var_to_c_state(variable, var_types, project)))
                .expect("ENO output has a variable");
            c_writeln!(
                out,
                "{outer_pad}else {{ {variable} = {}; }}",
                eno_bool_value(eno_arg, false)
            );
        }
    }
}

pub(crate) fn emit_standard_void_call_user_fb(
    out: &mut CEmitter<'_>,
    pad: &str,
    name: &Identifier,
    args: &[ParamAssignment],
    instance: &str,
    field_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) {
    let Some(input) = split_input_expr(args) else {
        c_writeln!(out, "{pad}/* missing input for {} */", name.original);
        return;
    };
    let input_c = expr_to_c_for_user_fb(input, instance, field_types, project);
    let outputs: &[&str] = match name.canonical.as_str() {
        "SPLIT_DATE" => &["YEAR", "MONTH", "DATE"],
        "SPLIT_TOD" => &["HOUR", "MINUTE", "SECOND", "MILLISECOND"],
        "SPLIT_DT" => &[
            "YEAR",
            "MONTH",
            "DATE",
            "HOUR",
            "MINUTE",
            "SECOND",
            "MILLISECOND",
        ],
        _ => return,
    };
    let helper = match name.canonical.as_str() {
        "SPLIT_DATE" => format!("rbcpp_civil_from_days({input_c})"),
        "SPLIT_TOD" => format!("rbcpp_split_tod_parts({input_c})"),
        "SPLIT_DT" => format!("rbcpp_split_dt_parts({input_c})"),
        _ => unreachable!(),
    };
    let en_expr = args
        .iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
        .and_then(|arg| arg.expr.as_ref())
        .map(|expr| expr_to_c_for_user_fb(expr, instance, field_types, project));
    let eno_arg = args.iter().find(|arg| {
        arg.output && arg.name.as_ref().is_some_and(is_implicit_eno) && arg.variable.is_some()
    });
    let outer_pad = pad;
    let body_pad_storage;
    let pad = if let Some(en_expr) = &en_expr {
        c_writeln!(out, "{outer_pad}if ({en_expr}) {{");
        body_pad_storage = format!("{outer_pad}    ");
        body_pad_storage.as_str()
    } else {
        outer_pad
    };
    c_writeln!(out, "{pad}{{");
    c_writeln!(out, "{pad}    rbcpp_datetime_parts rbcpp_split = {helper};");
    for (index, output) in outputs.iter().enumerate() {
        let Some(variable) = split_output_variable(args, output, index) else {
            continue;
        };
        c_writeln!(
            out,
            "{pad}    s->{} = rbcpp_split.{};",
            user_fb_var_to_c_typed(instance, variable, field_types, project),
            split_output_field(output)
        );
    }
    c_writeln!(out, "{pad}}}");
    emit_nested_fb_eno(
        out,
        pad,
        outer_pad,
        en_expr.is_some(),
        eno_arg,
        instance,
        field_types,
        project,
        true,
    );
}

pub(crate) fn emit_standard_void_call_local(
    out: &mut CEmitter<'_>,
    pad: &str,
    name: &Identifier,
    args: &[ParamAssignment],
    context: &FunctionCContext,
) {
    let Some(input) = split_input_expr(args) else {
        c_writeln!(out, "{pad}/* missing input for {} */", name.original);
        return;
    };
    let input_c = expr_to_c_local_typed(input, &context.var_types, context.project);
    let outputs: &[&str] = match name.canonical.as_str() {
        "SPLIT_DATE" => &["YEAR", "MONTH", "DATE"],
        "SPLIT_TOD" => &["HOUR", "MINUTE", "SECOND", "MILLISECOND"],
        "SPLIT_DT" => &[
            "YEAR",
            "MONTH",
            "DATE",
            "HOUR",
            "MINUTE",
            "SECOND",
            "MILLISECOND",
        ],
        _ => return,
    };
    let helper = match name.canonical.as_str() {
        "SPLIT_DATE" => format!("rbcpp_civil_from_days({input_c})"),
        "SPLIT_TOD" => format!("rbcpp_split_tod_parts({input_c})"),
        "SPLIT_DT" => format!("rbcpp_split_dt_parts({input_c})"),
        _ => unreachable!(),
    };
    let en_expr = args
        .iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
        .and_then(|arg| arg.expr.as_ref())
        .map(|expr| expr_to_c_local_typed(expr, &context.var_types, context.project));
    let eno_arg = args.iter().find(|arg| {
        arg.output && arg.name.as_ref().is_some_and(is_implicit_eno) && arg.variable.is_some()
    });
    let outer_pad = pad;
    let body_pad_storage;
    let pad = if let Some(en_expr) = &en_expr {
        c_writeln!(out, "{outer_pad}if ({en_expr}) {{");
        body_pad_storage = format!("{outer_pad}    ");
        body_pad_storage.as_str()
    } else {
        outer_pad
    };
    c_writeln!(out, "{pad}{{");
    c_writeln!(out, "{pad}    rbcpp_datetime_parts rbcpp_split = {helper};");
    for (index, output) in outputs.iter().enumerate() {
        let Some(variable) = split_output_variable(args, output, index) else {
            continue;
        };
        c_writeln!(
            out,
            "{pad}    {} = rbcpp_split.{};",
            local_var_to_c_typed(variable, &context.var_types, context.project),
            split_output_field(output)
        );
    }
    c_writeln!(out, "{pad}}}");
    if let Some(eno_arg) = eno_arg {
        let variable = eno_arg
            .variable
            .as_ref()
            .map(|variable| local_var_to_c_typed(variable, &context.var_types, context.project))
            .expect("ENO output has a variable");
        c_writeln!(out, "{pad}{variable} = {};", eno_bool_value(eno_arg, true));
    }
    if en_expr.is_some() {
        c_writeln!(out, "{outer_pad}}}");
        if let Some(eno_arg) = eno_arg {
            let variable = eno_arg
                .variable
                .as_ref()
                .map(|variable| local_var_to_c_typed(variable, &context.var_types, context.project))
                .expect("ENO output has a variable");
            c_writeln!(
                out,
                "{outer_pad}else {{ {variable} = {}; }}",
                eno_bool_value(eno_arg, false)
            );
        }
    }
}

pub(crate) fn is_standard_void_call_name(name: &str) -> bool {
    matches!(
        canonical_identifier(name).as_str(),
        "SPLIT_DATE" | "SPLIT_TOD" | "SPLIT_DT"
    )
}

pub(crate) fn split_output_field(output: &str) -> &'static str {
    match output {
        "YEAR" => "year",
        "MONTH" => "month",
        "DATE" => "day",
        "HOUR" => "hour",
        "MINUTE" => "minute",
        "SECOND" => "second",
        "MILLISECOND" => "millisecond",
        _ => "year",
    }
}
