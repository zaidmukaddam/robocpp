// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::fmt::{self, Write};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_profile::ImplementationParameters;
use iec_stdlib::{is_standard_function, standard_function_input_index};

use crate::addressing::*;
use crate::fb::*;
use crate::functions::*;
use crate::state::*;
use crate::*;

pub(crate) fn expr_to_c(expr: &Expr) -> String {
    match expr {
        Expr::Literal(literal) => literal_to_c(literal),
        Expr::Variable(variable) => format!("s->{}", var_to_c(variable)),
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg => format!("(-{})", expr_to_c(expr)),
            UnaryOp::Not => format!("(!{})", expr_to_c(expr)),
        },
        Expr::Binary { op, left, right } => {
            if *op == BinaryOp::Power {
                return format!("pow({}, {})", expr_to_c(left), expr_to_c(right));
            }
            format!(
                "({} {} {})",
                expr_to_c(left),
                binary_op_to_c(*op),
                expr_to_c(right)
            )
        }
        Expr::Call { name, args } => {
            let call_args = if is_standard_function(&name.original) {
                standard_call_input_args_to_c(&name.original, args)
            } else {
                call_input_args_to_c(args)
            };
            let call = standard_call_to_c(&name.original, &call_args).unwrap_or_else(|| {
                format!(
                    "{}({})",
                    sanitize_c_ident(&name.original),
                    call_args.join(", ")
                )
            });
            wrap_call_controls_to_c(
                call,
                args,
                false,
                disabled_call_default_to_c(&name.original),
            )
        }
        Expr::ArrayLiteral(_) | Expr::StructLiteral(_) => "0".to_string(),
    }
}

pub(crate) fn expr_to_c_state(
    expr: &Expr,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
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
            let rendered = format!("s->{}", var_to_c_state(variable, var_types, project));
            if variable_spec(variable, var_types, project)
                .and_then(|spec| c_text_info(project, &spec))
                .is_some_and(|info| info.wide)
            {
                format!("rbcpp_wstr_to_utf8({rendered})")
            } else {
                rendered
            }
        }
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg => format!("-({})", expr_to_c_state(expr, var_types, project)),
            UnaryOp::Not => {
                let expr_c = expr_to_c_state(expr, var_types, project);
                if expr_is_bool_for_c(expr, var_types, project) {
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
                    expr_to_c_state(left, var_types, project),
                    expr_to_c_state(right, var_types, project)
                );
            }
            let op_c = binary_op_to_c_state(*op, left, right, var_types, project);
            format!(
                "({} {} {})",
                expr_to_c_state(left, var_types, project),
                op_c,
                expr_to_c_state(right, var_types, project)
            )
        }
        Expr::Call { name, args } => {
            let call_args = if is_standard_function(&name.original) {
                standard_call_input_args_to_c_state(&name.original, args, var_types, project)
            } else {
                user_function_call_input_args_to_c_state(project, &name.original, args, var_types)
            };
            let call =
                standard_call_to_c_state(&name.original, args, &call_args, var_types, project)
                    .unwrap_or_else(|| {
                        format!(
                            "{}({})",
                            sanitize_c_ident(&name.original),
                            call_args.join(", ")
                        )
                    });
            let disabled_default = disabled_call_default_to_c_project(project, &name.original);
            wrap_call_controls_to_c_state(call, args, var_types, project, &disabled_default)
        }
        Expr::ArrayLiteral(_) | Expr::StructLiteral(_) => "0".to_string(),
    }
}

pub(crate) fn expr_to_c_local(expr: &Expr) -> String {
    match expr {
        Expr::Literal(literal) => literal_to_c(literal),
        Expr::Variable(variable) => local_var_to_c(variable),
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg => format!("(-{})", expr_to_c_local(expr)),
            UnaryOp::Not => format!("(!{})", expr_to_c_local(expr)),
        },
        Expr::Binary { op, left, right } => {
            if *op == BinaryOp::Power {
                return format!("pow({}, {})", expr_to_c_local(left), expr_to_c_local(right));
            }
            format!(
                "({} {} {})",
                expr_to_c_local(left),
                binary_op_to_c(*op),
                expr_to_c_local(right)
            )
        }
        Expr::Call { name, args } => {
            let call_args = if is_standard_function(&name.original) {
                standard_call_input_args_to_c_local(&name.original, args)
            } else {
                call_input_args_to_c_local(args)
            };
            let call = standard_call_to_c(&name.original, &call_args).unwrap_or_else(|| {
                format!(
                    "{}({})",
                    sanitize_c_ident(&name.original),
                    call_args.join(", ")
                )
            });
            wrap_call_controls_to_c(call, args, true, disabled_call_default_to_c(&name.original))
        }
        Expr::ArrayLiteral(_) | Expr::StructLiteral(_) => "0".to_string(),
    }
}

pub(crate) fn expr_to_c_local_typed(
    expr: &Expr,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
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
            let rendered = local_var_to_c_typed(variable, var_types, project);
            if variable_spec(variable, var_types, project)
                .and_then(|spec| c_text_info(project, &spec))
                .is_some_and(|info| info.wide)
            {
                format!("rbcpp_wstr_to_utf8({rendered})")
            } else {
                rendered
            }
        }
        Expr::Unary { op, expr } => match op {
            UnaryOp::Neg => format!("-({})", expr_to_c_local_typed(expr, var_types, project)),
            UnaryOp::Not => {
                let expr_c = expr_to_c_local_typed(expr, var_types, project);
                if expr_is_bool_for_c(expr, var_types, project) {
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
                    expr_to_c_local_typed(left, var_types, project),
                    expr_to_c_local_typed(right, var_types, project)
                );
            }
            let op_c = binary_op_to_c_state(*op, left, right, var_types, project);
            format!(
                "({} {} {})",
                expr_to_c_local_typed(left, var_types, project),
                op_c,
                expr_to_c_local_typed(right, var_types, project)
            )
        }
        Expr::Call { name, args } => {
            let call_args = if is_standard_function(&name.original) {
                ordered_call_input_args(&name.original, args, |expr| {
                    expr_to_c_local_typed(expr, var_types, project)
                })
            } else {
                user_function_call_input_args_to_c_local_typed(
                    project,
                    &name.original,
                    args,
                    var_types,
                )
            };
            let call =
                standard_call_to_c_state(&name.original, args, &call_args, var_types, project)
                    .unwrap_or_else(|| {
                        format!(
                            "{}({})",
                            sanitize_c_ident(&name.original),
                            call_args.join(", ")
                        )
                    });
            let disabled_default = disabled_call_default_to_c_project(project, &name.original);
            wrap_call_controls_to_c(call, args, true, &disabled_default)
        }
        Expr::ArrayLiteral(_) | Expr::StructLiteral(_) => "0".to_string(),
    }
}

pub(crate) fn call_input_args_to_c(args: &[ParamAssignment]) -> Vec<String> {
    args.iter()
        .filter(|arg| !arg.output)
        .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
        .filter_map(|arg| arg.expr.as_ref())
        .map(expr_to_c)
        .collect()
}

pub(crate) fn user_function_call_input_args_to_c_state(
    project: &Project,
    function_name: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
) -> Vec<String> {
    ordered_user_function_call_input_args(project, function_name, args, |expr| {
        expr_to_c_state(expr, var_types, project)
    })
}

pub(crate) fn user_function_call_input_args_to_c_local_typed(
    project: &Project,
    function_name: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
) -> Vec<String> {
    ordered_user_function_call_input_args(project, function_name, args, |expr| {
        expr_to_c_local_typed(expr, var_types, project)
    })
}

pub(crate) fn ordered_user_function_call_input_args(
    project: &Project,
    function_name: &str,
    args: &[ParamAssignment],
    render: impl Fn(&Expr) -> String,
) -> Vec<String> {
    let Some(function) = project
        .find_pou(function_name)
        .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))
    else {
        return args
            .iter()
            .filter(|arg| !arg.output)
            .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
            .filter_map(|arg| arg.expr.as_ref())
            .map(render)
            .collect();
    };
    let inputs = function
        .var_blocks
        .iter()
        .filter(|block| block.kind == VarBlockKind::Input)
        .flat_map(|block| block.vars.iter())
        .map(|var| var.name.canonical.clone())
        .collect::<Vec<_>>();

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
        let index = if let Some(arg_name) = &arg.name {
            inputs
                .iter()
                .position(|input| input == &arg_name.canonical)
                .unwrap_or_else(|| {
                    let index = unknown_index;
                    unknown_index = unknown_index.saturating_add(1);
                    index
                })
        } else {
            let index = positional_index;
            positional_index += 1;
            index
        };
        ordered.push((index, render(expr)));
    }
    ordered.sort_by_key(|(index, _)| *index);
    ordered.into_iter().map(|(_, value)| value).collect()
}

pub(crate) fn call_input_args_to_c_local(args: &[ParamAssignment]) -> Vec<String> {
    args.iter()
        .filter(|arg| !arg.output)
        .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
        .filter_map(|arg| arg.expr.as_ref())
        .map(expr_to_c_local)
        .collect()
}

pub(crate) fn standard_call_input_args_to_c(
    function_name: &str,
    args: &[ParamAssignment],
) -> Vec<String> {
    ordered_call_input_args(function_name, args, expr_to_c)
}

pub(crate) fn standard_call_input_args_to_c_state(
    function_name: &str,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> Vec<String> {
    ordered_call_input_args(function_name, args, |expr| {
        expr_to_c_state(expr, var_types, project)
    })
}

pub(crate) fn standard_call_input_args_to_c_local(
    function_name: &str,
    args: &[ParamAssignment],
) -> Vec<String> {
    ordered_call_input_args(function_name, args, expr_to_c_local)
}

pub(crate) fn ordered_call_input_args(
    function_name: &str,
    args: &[ParamAssignment],
    render: impl Fn(&Expr) -> String,
) -> Vec<String> {
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
        let index = if let Some(arg_name) = &arg.name {
            standard_function_input_index(function_name, &arg_name.original).unwrap_or_else(|| {
                let index = unknown_index;
                unknown_index = unknown_index.saturating_add(1);
                index
            })
        } else {
            let index = positional_index;
            positional_index += 1;
            index
        };
        ordered.push((index, render(expr)));
    }

    ordered.sort_by_key(|(index, _)| *index);
    ordered.into_iter().map(|(_, value)| value).collect()
}

pub(crate) fn wrap_call_controls_to_c(
    call: String,
    args: &[ParamAssignment],
    local: bool,
    disabled_default: &str,
) -> String {
    let en = args
        .iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
        .and_then(|arg| arg.expr.as_ref())
        .map(|expr| {
            if local {
                expr_to_c_local(expr)
            } else {
                expr_to_c(expr)
            }
        });
    let eno = args.iter().find(|arg| {
        arg.output && arg.name.as_ref().is_some_and(is_implicit_eno) && arg.variable.is_some()
    });

    match (en, eno) {
        (None, None) => call,
        (Some(en), None) => format!("(({en}) ? ({call}) : {disabled_default})"),
        (None, Some(eno)) => {
            let variable = eno_output_to_c(eno, local);
            format!("({variable} = {}, ({call}))", eno_bool_value(eno, true))
        }
        (Some(en), Some(eno)) => {
            let variable = eno_output_to_c(eno, local);
            let true_value = eno_bool_value(eno, true);
            let false_value = eno_bool_value(eno, false);
            format!(
                "(({en}) ? ({variable} = {true_value}, ({call})) : ({variable} = {false_value}, {disabled_default}))"
            )
        }
    }
}

pub(crate) fn wrap_call_controls_to_c_state(
    call: String,
    args: &[ParamAssignment],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
    disabled_default: &str,
) -> String {
    let en = args
        .iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
        .and_then(|arg| arg.expr.as_ref())
        .map(|expr| expr_to_c_state(expr, var_types, project));
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
                .map(|variable| format!("s->{}", var_to_c_state(variable, var_types, project)))
                .expect("ENO output has a variable");
            format!("({variable} = {}, ({call}))", eno_bool_value(eno, true))
        }
        (Some(en), Some(eno)) => {
            let variable = eno
                .variable
                .as_ref()
                .map(|variable| format!("s->{}", var_to_c_state(variable, var_types, project)))
                .expect("ENO output has a variable");
            let true_value = eno_bool_value(eno, true);
            let false_value = eno_bool_value(eno, false);
            format!(
                "(({en}) ? ({variable} = {true_value}, ({call})) : ({variable} = {false_value}, {disabled_default}))"
            )
        }
    }
}

pub(crate) fn disabled_call_default_to_c(function_name: &str) -> &'static str {
    let canonical = canonical_identifier(function_name);
    match canonical.as_str() {
        "LEFT" | "RIGHT" | "MID" | "CONCAT" | "INSERT" | "DELETE" | "REPLACE" => "\"\"",
        name if name.ends_with("_TO_STRING") || name.ends_with("_TO_WSTRING") => "\"\"",
        _ => "0",
    }
}

pub(crate) fn disabled_call_default_to_c_project(project: &Project, function_name: &str) -> String {
    if let Some(function) = project
        .find_pou(function_name)
        .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))
    {
        if let PouKind::Function { return_type } = &function.kind {
            return default_expr_to_c(project, return_type);
        }
    }
    disabled_call_default_to_c(function_name).to_string()
}

pub(crate) fn eno_output_to_c(arg: &ParamAssignment, local: bool) -> String {
    let variable = arg.variable.as_ref().expect("ENO output has a variable");
    if local {
        local_var_to_c(variable)
    } else {
        format!("s->{}", var_to_c(variable))
    }
}

pub(crate) fn eno_bool_value(arg: &ParamAssignment, success: bool) -> &'static str {
    match success ^ arg.negated {
        true => "true",
        false => "false",
    }
}

pub(crate) fn is_implicit_en(name: &Identifier) -> bool {
    name.canonical == "EN"
}

pub(crate) fn is_implicit_eno(name: &Identifier) -> bool {
    name.canonical == "ENO"
}

pub(crate) fn split_input_expr(args: &[ParamAssignment]) -> Option<&Expr> {
    args.iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(|name| name.canonical == "IN"))
        .and_then(|arg| arg.expr.as_ref())
        .or_else(|| split_positional_args(args).first().copied())
}

pub(crate) fn split_positional_args(args: &[ParamAssignment]) -> Vec<&Expr> {
    args.iter()
        .filter(|arg| !arg.output)
        .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
        .filter(|arg| arg.name.is_none())
        .filter_map(|arg| arg.expr.as_ref())
        .collect()
}

pub(crate) fn split_formal_output<'a>(
    args: &'a [ParamAssignment],
    output: &str,
) -> Option<&'a VariableRef> {
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

pub(crate) fn split_output_variable<'a>(
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

pub(crate) fn bcd_conversion_call_to_c(name: &str, args: &[String]) -> Option<String> {
    let arg = args.first()?;
    if name == "BCD_TO_INT" || name.split_once("_BCD_TO_").is_some() {
        return Some(format!("rbcpp_bcd_to_int({arg})"));
    }
    if name == "INT_TO_BCD" || name.split_once("_TO_BCD_").is_some() {
        return Some(format!("rbcpp_int_to_bcd({arg})"));
    }
    None
}

pub(crate) fn standard_call_to_c(name: &str, args: &[String]) -> Option<String> {
    let name = canonical_identifier(name);
    if let Some(conversion) = bcd_conversion_call_to_c(&name, args) {
        return Some(conversion);
    }
    if let Some(conversion) = conversion_call_to_c(&name, args) {
        return Some(conversion);
    }
    match name.as_str() {
        "ABS" => args.first().map(|arg| format!("RBCPP_ABS({arg})")),
        "SQRT" => args.first().map(|arg| format!("sqrt({arg})")),
        "LN" => args.first().map(|arg| format!("log({arg})")),
        "LOG" => args.first().map(|arg| format!("log10({arg})")),
        "EXP" => args.first().map(|arg| format!("exp({arg})")),
        "SIN" => args.first().map(|arg| format!("sin({arg})")),
        "COS" => args.first().map(|arg| format!("cos({arg})")),
        "TAN" => args.first().map(|arg| format!("tan({arg})")),
        "MOVE" => args.first().cloned(),
        "ADD" => fold_binary_operator("+", args),
        "SUB" if args.len() == 2 => Some(format!("({} - {})", args[0], args[1])),
        "MUL" => fold_binary_operator("*", args),
        "DIV" if args.len() == 2 => Some(format!("({} / {})", args[0], args[1])),
        "MOD" if args.len() == 2 => Some(format!("({} % {})", args[0], args[1])),
        "EXPT" if args.len() == 2 => Some(format!("pow({}, {})", args[0], args[1])),
        "TRUNC" if args.len() == 1 => Some(format!("((int64_t)({}))", args[0])),
        "MIN" => fold_binary_macro("RBCPP_MIN", args),
        "MAX" => fold_binary_macro("RBCPP_MAX", args),
        "LIMIT" if args.len() == 3 => Some(format!(
            "RBCPP_LIMIT({}, {}, {})",
            args[0], args[1], args[2]
        )),
        "SEL" if args.len() == 3 => {
            Some(format!("RBCPP_SEL({}, {}, {})", args[0], args[1], args[2]))
        }
        "MUX" if args.len() >= 2 => mux_to_c(args),
        "GT" => compare_chain_to_c(">", args),
        "GE" => compare_chain_to_c(">=", args),
        "EQ" => compare_chain_to_c("==", args),
        "NE" => compare_chain_to_c("!=", args),
        "LE" => compare_chain_to_c("<=", args),
        "LT" => compare_chain_to_c("<", args),
        "SHL" if args.len() == 2 => Some(format!("rbcpp_shl64({}, {})", args[0], args[1])),
        "SHR" if args.len() == 2 => Some(format!("rbcpp_shr64({}, {})", args[0], args[1])),
        "ROL" if args.len() == 2 => Some(format!("rbcpp_rol64({}, {})", args[0], args[1])),
        "ROR" if args.len() == 2 => Some(format!("rbcpp_ror64({}, {})", args[0], args[1])),
        "AND" => fold_binary_operator("&", args),
        "OR" => fold_binary_operator("|", args),
        "XOR" => fold_binary_operator("^", args),
        "NOT" => args.first().map(|arg| format!("(~({arg}))")),
        "ADD_TIME" | "ADD_TOD_TIME" | "ADD_DT_TIME" if args.len() == 2 => {
            Some(format!("({} + {})", args[0], args[1]))
        }
        "SUB_TIME" | "SUB_TOD_TIME" | "SUB_DT_TIME" | "SUB_TOD_TOD" | "SUB_DT_DT"
            if args.len() == 2 =>
        {
            Some(format!("({} - {})", args[0], args[1]))
        }
        "SUB_DATE_DATE" if args.len() == 2 => {
            Some(format!("(({} - {}) * 86400000LL)", args[0], args[1]))
        }
        "CONCAT_DATE" if args.len() == 3 => Some(format!(
            "rbcpp_concat_date({}, {}, {})",
            args[0], args[1], args[2]
        )),
        "CONCAT_TOD" if args.len() == 4 => Some(format!(
            "rbcpp_concat_tod({}, {}, {}, {})",
            args[0], args[1], args[2], args[3]
        )),
        "CONCAT_DT" if args.len() == 7 => Some(format!(
            "rbcpp_concat_dt({}, {}, {}, {}, {}, {}, {})",
            args[0], args[1], args[2], args[3], args[4], args[5], args[6]
        )),
        "CONCAT_DATE_TOD" if args.len() == 2 => {
            Some(format!("rbcpp_concat_date_tod({}, {})", args[0], args[1]))
        }
        "DAY_OF_WEEK" if args.len() == 1 => Some(format!("rbcpp_day_of_week({})", args[0])),
        "MUL_TIME" | "MULTIME" if args.len() == 2 => Some(format!("({} * {})", args[0], args[1])),
        "DIV_TIME" | "DIVTIME" if args.len() == 2 => Some(format!("({} / {})", args[0], args[1])),
        "LEN" if args.len() == 1 => Some(format!("rbcpp_utf8_len({})", args[0])),
        "LEFT" if args.len() == 2 => Some(format!("rbcpp_left({}, {})", args[0], args[1])),
        "RIGHT" if args.len() == 2 => Some(format!("rbcpp_right({}, {})", args[0], args[1])),
        "MID" if args.len() == 3 => {
            Some(format!("rbcpp_mid({}, {}, {})", args[0], args[1], args[2]))
        }
        "CONCAT" => fold_binary_function("rbcpp_concat2", args),
        "INSERT" if args.len() == 3 => Some(format!(
            "rbcpp_insert({}, {}, {})",
            args[0], args[1], args[2]
        )),
        "DELETE" if args.len() == 3 => Some(format!(
            "rbcpp_delete({}, {}, {})",
            args[0], args[1], args[2]
        )),
        "REPLACE" if args.len() == 4 => Some(format!(
            "rbcpp_replace({}, {}, {}, {})",
            args[0], args[1], args[2], args[3]
        )),
        "FIND" if args.len() == 2 => Some(format!("rbcpp_find({}, {})", args[0], args[1])),
        _ => None,
    }
}

pub(crate) fn standard_call_to_c_state(
    name: &str,
    source_args: &[ParamAssignment],
    args: &[String],
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> Option<String> {
    let canonical = canonical_identifier(name);
    if canonical == "NOT" && args.len() == 1 {
        let input_is_bool = first_ordered_standard_input_expr(name, source_args)
            .is_some_and(|expr| expr_is_bool_for_c(expr, var_types, project));
        let op = if input_is_bool { "!" } else { "~" };
        return Some(format!("({op}({}))", args[0]));
    }
    standard_call_to_c(name, args)
}

pub(crate) fn first_ordered_standard_input_expr<'a>(
    function_name: &str,
    args: &'a [ParamAssignment],
) -> Option<&'a Expr> {
    let mut positional_index = 0;
    let mut unknown_index = usize::MAX.saturating_sub(args.len());
    let mut ordered = Vec::new();

    for arg in args {
        if arg.output || arg.name.as_ref().is_some_and(is_implicit_en) {
            continue;
        }
        let Some(expr) = arg.expr.as_ref() else {
            continue;
        };
        let index = if let Some(arg_name) = &arg.name {
            standard_function_input_index(function_name, &arg_name.original).unwrap_or_else(|| {
                let index = unknown_index;
                unknown_index = unknown_index.saturating_add(1);
                index
            })
        } else {
            let index = positional_index;
            positional_index += 1;
            index
        };
        ordered.push((index, expr));
    }

    ordered.sort_by_key(|(index, _)| *index);
    ordered.into_iter().map(|(_, expr)| expr).next()
}

pub(crate) fn conversion_call_to_c(name: &str, args: &[String]) -> Option<String> {
    let (source, target) = name.split_once("_TO_")?;
    let arg = args.first()?;
    match target {
        "BOOL" => match source {
            "STRING" | "WSTRING" => Some(format!("rbcpp_string_to_bool({arg})")),
            _ => Some(format!("(({arg}) != 0)")),
        },
        "SINT" | "INT" | "DINT" | "LINT" | "USINT" | "UINT" | "UDINT" | "ULINT" | "BYTE"
        | "WORD" | "DWORD" | "LWORD" => match source {
            "STRING" | "WSTRING" => Some(format!("rbcpp_string_to_int({arg})")),
            _ => Some(format!("((int64_t)({arg}))")),
        },
        "REAL" | "LREAL" => match source {
            "STRING" | "WSTRING" => Some(format!("rbcpp_string_to_real({arg})")),
            _ => Some(format!("((double)({arg}))")),
        },
        "STRING" | "WSTRING" => match source {
            "BOOL" => Some(format!("rbcpp_bool_to_string({arg})")),
            "REAL" | "LREAL" => Some(format!("rbcpp_real_to_string({arg})")),
            "TIME" => Some(format!("rbcpp_time_to_string({arg})")),
            "DATE" => Some(format!("rbcpp_date_to_string({arg})")),
            "TOD" | "TIME_OF_DAY" => Some(format!("rbcpp_tod_to_string({arg})")),
            "DT" | "DATE_AND_TIME" => Some(format!("rbcpp_dt_to_string({arg})")),
            "STRING" | "WSTRING" => Some(arg.clone()),
            _ => Some(format!("rbcpp_int_to_string({arg})")),
        },
        "TIME" => match source {
            "STRING" | "WSTRING" => Some(format!("rbcpp_string_to_time({arg})")),
            _ => Some(format!("((int64_t)({arg}))")),
        },
        "DATE" => match source {
            "STRING" | "WSTRING" => Some(format!("rbcpp_string_to_date({arg})")),
            _ => Some(format!("((int64_t)({arg}))")),
        },
        "TOD" | "TIME_OF_DAY" => match source {
            "STRING" | "WSTRING" => Some(format!("rbcpp_string_to_tod({arg})")),
            _ => Some(format!("((int64_t)({arg}))")),
        },
        "DT" | "DATE_AND_TIME" => match source {
            "STRING" | "WSTRING" => Some(format!("rbcpp_string_to_dt({arg})")),
            _ => Some(format!("((int64_t)({arg}))")),
        },
        _ => None,
    }
}

pub(crate) fn fold_binary_operator(operator: &str, args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    let first = iter.next()?.clone();
    Some(iter.fold(first, |acc, arg| format!("({acc} {operator} {arg})")))
}

pub(crate) fn fold_binary_macro(macro_name: &str, args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    let first = iter.next()?.clone();
    Some(iter.fold(first, |acc, arg| format!("{macro_name}({acc}, {arg})")))
}

pub(crate) fn fold_binary_function(function_name: &str, args: &[String]) -> Option<String> {
    let mut iter = args.iter();
    let first = iter.next()?.clone();
    Some(iter.fold(first, |acc, arg| format!("{function_name}({acc}, {arg})")))
}

pub(crate) fn compare_chain_to_c(operator: &str, args: &[String]) -> Option<String> {
    if args.len() < 2 {
        return None;
    }
    Some(
        args.windows(2)
            .map(|pair| format!("({} {operator} {})", pair[0], pair[1]))
            .collect::<Vec<_>>()
            .join(" && "),
    )
}

pub(crate) fn mux_to_c(args: &[String]) -> Option<String> {
    let index = args.first()?;
    let inputs = &args[1..];
    let mut expr = inputs.last()?.clone();
    for (position, input) in inputs.iter().enumerate().rev().skip(1) {
        expr = format!("(({index}) == {position} ? {input} : {expr})");
    }
    Some(expr)
}

pub(crate) fn initializer_expr_to_c(
    expr: &Expr,
    expected: &DataTypeSpec,
    project: &Project,
) -> String {
    if let Some(value) = enum_ordinal_expr(project, expr) {
        return value.to_string();
    }

    match resolve_named_spec(project, expected) {
        resolved if matches!(expr, Expr::Literal(Literal::Typed { .. })) => {
            if let Expr::Literal(literal) = expr {
                literal_to_c_for_spec(project, literal, &resolved)
                    .unwrap_or_else(|| literal_to_c_project(project, literal))
            } else {
                unreachable!()
            }
        }
        DataTypeSpec::Enum { .. } => enum_ordinal_expr(project, expr)
            .map(|value| value.to_string())
            .unwrap_or_else(|| expr_to_c(expr)),
        _ => expr_to_c(expr),
    }
}

pub(crate) fn initializer_expr_to_c_local_typed(
    expr: &Expr,
    expected: &DataTypeSpec,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
    project: &Project,
) -> String {
    if let Some(value) = enum_ordinal_expr(project, expr) {
        return value.to_string();
    }

    match resolve_named_spec(project, expected) {
        resolved if matches!(expr, Expr::Literal(Literal::Typed { .. })) => {
            if let Expr::Literal(literal) = expr {
                literal_to_c_for_spec(project, literal, &resolved)
                    .unwrap_or_else(|| literal_to_c_project(project, literal))
            } else {
                unreachable!()
            }
        }
        DataTypeSpec::Enum { .. } => enum_ordinal_expr(project, expr)
            .map(|value| value.to_string())
            .unwrap_or_else(|| expr_to_c_local_typed(expr, var_types, project)),
        _ => expr_to_c_local_typed(expr, var_types, project),
    }
}

pub(crate) fn default_expr_to_c(project: &Project, spec: &DataTypeSpec) -> String {
    if let Some(value) = struct_compound_literal_to_c(project, spec, None) {
        return value;
    }
    if c_text_info(project, spec).is_some() {
        return "\"\"".to_string();
    }
    match resolve_named_spec(project, spec) {
        DataTypeSpec::Subrange { range, .. } if range.low > 0 || range.high < 0 => {
            range.low.to_string()
        }
        resolved => c_default(&resolved).to_string(),
    }
}

pub(crate) fn struct_compound_literal_to_c(
    project: &Project,
    spec: &DataTypeSpec,
    initial: Option<&Expr>,
) -> Option<String> {
    let type_ident = named_struct_type_ident(project, spec)?;
    let DataTypeSpec::Struct { fields } = resolve_named_spec(project, spec) else {
        return None;
    };
    let initializers = match initial {
        Some(Expr::StructLiteral(initializers)) => initializers.as_slice(),
        _ => &[],
    };
    let values = fields
        .iter()
        .map(|field| {
            let explicit = initializers
                .iter()
                .find(|initializer| {
                    initializer
                        .name
                        .as_ref()
                        .is_some_and(|name| name.canonical == field.name.canonical)
                })
                .and_then(|initializer| initializer.expr.as_ref());
            let initializer = explicit.or(field.initial_value.as_ref());
            let value = initializer
                .map(|expr| {
                    struct_compound_literal_to_c(project, &field.spec, Some(expr))
                        .unwrap_or_else(|| initializer_expr_to_c(expr, &field.spec, project))
                })
                .unwrap_or_else(|| default_expr_to_c(project, &field.spec));
            format!(".{} = {value}", sanitize_c_ident(&field.name.original))
        })
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("({type_ident}){{{values}}}"))
}

pub(crate) fn struct_compound_literal_to_c_local(
    project: &Project,
    spec: &DataTypeSpec,
    initial: Option<&Expr>,
    var_types: &std::collections::BTreeMap<String, DataTypeSpec>,
) -> Option<String> {
    let type_ident = named_struct_type_ident(project, spec)?;
    let DataTypeSpec::Struct { fields } = resolve_named_spec(project, spec) else {
        return None;
    };
    let initializers = match initial {
        Some(Expr::StructLiteral(initializers)) => initializers.as_slice(),
        _ => &[],
    };
    let values = fields
        .iter()
        .map(|field| {
            let explicit = initializers
                .iter()
                .find(|initializer| {
                    initializer
                        .name
                        .as_ref()
                        .is_some_and(|name| name.canonical == field.name.canonical)
                })
                .and_then(|initializer| initializer.expr.as_ref());
            let initializer = explicit.or(field.initial_value.as_ref());
            let value = initializer
                .map(|expr| {
                    struct_compound_literal_to_c_local(project, &field.spec, Some(expr), var_types)
                        .unwrap_or_else(|| {
                            initializer_expr_to_c_local_typed(expr, &field.spec, var_types, project)
                        })
                })
                .unwrap_or_else(|| default_expr_to_c(project, &field.spec));
            format!(".{} = {value}", sanitize_c_ident(&field.name.original))
        })
        .collect::<Vec<_>>()
        .join(", ");
    Some(format!("({type_ident}){{{values}}}"))
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

pub(crate) fn resolve_named_spec(project: &Project, spec: &DataTypeSpec) -> DataTypeSpec {
    resolve_named_spec_inner(project, spec, &mut std::collections::BTreeSet::new())
}

pub(crate) fn resolve_named_spec_inner(
    project: &Project,
    spec: &DataTypeSpec,
    seen: &mut std::collections::BTreeSet<String>,
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
    resolve_named_spec_inner(project, &data_type.spec, seen)
}

pub(crate) fn is_aggregate_spec(spec: &DataTypeSpec, project: &Project) -> bool {
    matches!(
        resolve_named_spec(project, spec),
        DataTypeSpec::Array { .. } | DataTypeSpec::Struct { .. }
    )
}

pub(crate) fn is_string_spec(project: &Project, spec: &DataTypeSpec) -> bool {
    c_string_capacity(project, spec).is_some()
}

pub(crate) fn c_string_capacity(project: &Project, spec: &DataTypeSpec) -> Option<usize> {
    c_text_info(project, spec).map(|info| info.capacity)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CTextInfo {
    pub(crate) wide: bool,
    pub(crate) capacity: usize,
}

pub(crate) fn c_text_info(project: &Project, spec: &DataTypeSpec) -> Option<CTextInfo> {
    match resolve_named_spec(project, spec) {
        DataTypeSpec::Elementary(ElementaryType::String) => Some(CTextInfo {
            wide: false,
            capacity: RBCPP_DEFAULT_STRING_CAP,
        }),
        DataTypeSpec::Elementary(ElementaryType::WString) => Some(CTextInfo {
            wide: true,
            capacity: RBCPP_DEFAULT_STRING_CAP,
        }),
        DataTypeSpec::String { wide, length } => Some(CTextInfo {
            wide,
            capacity: length
                .unwrap_or(RBCPP_DEFAULT_STRING_CAP - 1)
                .saturating_add(1),
        }),
        _ => None,
    }
}

pub(crate) fn array_storage_compatible(
    project: &Project,
    expected: &DataTypeSpec,
    actual: &DataTypeSpec,
) -> bool {
    let expected = resolve_named_spec(project, expected);
    let actual = resolve_named_spec(project, actual);
    match (expected, actual) {
        (
            DataTypeSpec::Array {
                ranges: expected_ranges,
                element_type: expected_element,
            },
            DataTypeSpec::Array {
                ranges: actual_ranges,
                element_type: actual_element,
            },
        ) => {
            expected_ranges == actual_ranges
                && data_storage_compatible(project, &expected_element, &actual_element)
        }
        _ => false,
    }
}

pub(crate) fn struct_storage_compatible(
    project: &Project,
    expected: &DataTypeSpec,
    actual: &DataTypeSpec,
) -> bool {
    let expected = resolve_named_spec(project, expected);
    let actual = resolve_named_spec(project, actual);
    let (
        DataTypeSpec::Struct {
            fields: expected_fields,
        },
        DataTypeSpec::Struct {
            fields: actual_fields,
        },
    ) = (expected, actual)
    else {
        return false;
    };
    expected_fields.len() == actual_fields.len()
        && expected_fields
            .iter()
            .zip(actual_fields.iter())
            .all(|(expected, actual)| {
                expected.name.canonical == actual.name.canonical
                    && data_storage_compatible(project, &expected.spec, &actual.spec)
            })
}

pub(crate) fn data_storage_compatible(
    project: &Project,
    expected: &DataTypeSpec,
    actual: &DataTypeSpec,
) -> bool {
    let expected = resolve_named_spec(project, expected);
    let actual = resolve_named_spec(project, actual);
    match (&expected, &actual) {
        (DataTypeSpec::Array { .. }, DataTypeSpec::Array { .. }) => {
            array_storage_compatible(project, &expected, &actual)
        }
        (DataTypeSpec::Struct { .. }, DataTypeSpec::Struct { .. }) => {
            struct_storage_compatible(project, &expected, &actual)
        }
        _ => {
            let expected_text = c_text_info(project, &expected);
            let actual_text = c_text_info(project, &actual);
            if expected_text.is_some() || actual_text.is_some() {
                return expected_text == actual_text;
            }
            c_storage_type(project, &expected) == c_storage_type(project, &actual)
        }
    }
}

pub(crate) fn named_struct_type_ident(project: &Project, spec: &DataTypeSpec) -> Option<String> {
    let DataTypeSpec::Named(name) = spec else {
        return None;
    };
    project
        .data_types()
        .find(|data_type| data_type.name.canonical == name.canonical)
        .and_then(|data_type| match &data_type.spec {
            DataTypeSpec::Struct { .. } => Some(type_c_ident(name)),
            _ => None,
        })
}

pub(crate) fn named_array_type_ident(project: &Project, spec: &DataTypeSpec) -> Option<String> {
    let DataTypeSpec::Named(name) = spec else {
        return None;
    };
    project
        .data_types()
        .find(|data_type| data_type.name.canonical == name.canonical)
        .and_then(|data_type| match &data_type.spec {
            DataTypeSpec::Array { .. } => Some(type_c_ident(name)),
            _ => None,
        })
}

pub(crate) fn peel_array_dimensions(
    project: &Project,
    spec: &DataTypeSpec,
) -> (DataTypeSpec, Vec<Subrange>) {
    let mut current = spec.clone();
    let mut dimensions = Vec::new();
    loop {
        match current {
            DataTypeSpec::Array {
                ranges,
                element_type,
            } => {
                dimensions.extend(ranges);
                current = *element_type;
            }
            DataTypeSpec::Named(_) => {
                let resolved = resolve_named_spec(project, &current);
                if matches!(resolved, DataTypeSpec::Array { .. }) {
                    current = resolved;
                } else {
                    return (current, dimensions);
                }
            }
            _ => return (current, dimensions),
        }
    }
}

pub(crate) fn dimensions_to_c(ranges: &[Subrange]) -> String {
    ranges
        .iter()
        .map(|range| format!("[{}]", array_range_len(range)))
        .collect::<Vec<_>>()
        .join("")
}

pub(crate) fn c_zero_based_indices(offset: usize, ranges: &[Subrange]) -> String {
    let mut remaining = offset;
    let mut indices = Vec::with_capacity(ranges.len());
    for position in 0..ranges.len() {
        let stride = ranges[position + 1..].iter().fold(1_usize, |total, range| {
            total.saturating_mul(array_range_len(range))
        });
        let index = remaining.checked_div(stride).unwrap_or(0);
        remaining = remaining.checked_rem(stride).unwrap_or(0);
        indices.push(index);
    }
    indices
        .into_iter()
        .map(|index| format!("[{index}]"))
        .collect::<Vec<_>>()
        .join("")
}

pub(crate) fn array_element_count(ranges: &[Subrange]) -> usize {
    ranges.iter().fold(1_usize, |total, range| {
        total.saturating_mul(array_range_len(range))
    })
}

pub(crate) fn array_range_len(range: &Subrange) -> usize {
    (range.high - range.low + 1).max(0) as usize
}

pub(crate) fn c_storage_type(project: &Project, spec: &DataTypeSpec) -> String {
    if let Some(type_ident) = named_struct_type_ident(project, spec) {
        return type_ident;
    }

    match resolve_named_spec(project, spec) {
        DataTypeSpec::Named(_) => c_type(spec).to_string(),
        DataTypeSpec::Array { element_type, .. } => c_storage_type(project, &element_type),
        DataTypeSpec::Struct { .. } => "int64_t".to_string(),
        resolved => c_type(&resolved).to_string(),
    }
}

pub(crate) fn type_c_ident(name: &Identifier) -> String {
    format!("rbcpp_type_{}", sanitize_c_ident(&name.original))
}

pub(crate) fn literal_to_c(literal: &Literal) -> String {
    match literal {
        Literal::Int(value) => value.to_string(),
        Literal::Real(value) => value.to_string(),
        Literal::Bool(value) => {
            if *value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }
        Literal::String(value) | Literal::WString(value) => {
            format!("\"{}\"", c_string_escape(value))
        }
        Literal::DurationMs(value) => value.to_string(),
        Literal::Date(value) => parse_date_days(value)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "0".to_string()),
        Literal::TimeOfDay(value) => parse_time_of_day_ms(value)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "0".to_string()),
        Literal::DateAndTime(value) => parse_date_time_ms(value)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "0".to_string()),
        Literal::Typed { value, .. } => value.clone(),
    }
}

pub(crate) fn literal_to_c_project(project: &Project, literal: &Literal) -> String {
    match literal {
        Literal::Typed { .. } => {
            literal_to_c_for_project_type(project, literal).unwrap_or_else(|| literal_to_c(literal))
        }
        _ => literal_to_c(literal),
    }
}

pub(crate) fn literal_to_c_for_project_type(
    project: &Project,
    literal: &Literal,
) -> Option<String> {
    let Literal::Typed { type_name, value } = literal else {
        return None;
    };
    if let Some(elementary) = ElementaryType::parse(&type_name.original) {
        return typed_literal_elementary_to_c(elementary, value);
    }
    let spec = project
        .data_types()
        .find(|data_type| data_type.name.canonical == type_name.canonical)
        .map(|data_type| data_type.spec.clone())?;
    literal_to_c_for_spec(project, literal, &resolve_named_spec(project, &spec))
}

pub(crate) fn literal_to_c_for_spec(
    project: &Project,
    literal: &Literal,
    spec: &DataTypeSpec,
) -> Option<String> {
    let Literal::Typed { value, .. } = literal else {
        return None;
    };
    match resolve_named_spec(project, spec) {
        DataTypeSpec::Elementary(elementary) => typed_literal_elementary_to_c(elementary, value),
        DataTypeSpec::Subrange { .. } => typed_literal_i128(value).map(|value| value.to_string()),
        DataTypeSpec::Enum { values } => {
            let value = canonical_identifier(value);
            values
                .iter()
                .position(|candidate| candidate.canonical == value)
                .map(|index| index.to_string())
        }
        DataTypeSpec::String { .. } => Some(format!("\"{}\"", c_string_escape(value))),
        DataTypeSpec::Named(name) => {
            let data_type = project
                .data_types()
                .find(|data_type| data_type.name.canonical == name.canonical)?;
            literal_to_c_for_spec(project, literal, &data_type.spec)
        }
        DataTypeSpec::Array { .. } | DataTypeSpec::Struct { .. } => None,
    }
}

pub(crate) fn typed_literal_elementary_to_c(
    elementary: ElementaryType,
    value: &str,
) -> Option<String> {
    match elementary {
        ElementaryType::Bool => parse_typed_bool(value).map(|value| {
            if value {
                "true".to_string()
            } else {
                "false".to_string()
            }
        }),
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
        | ElementaryType::Lword => typed_literal_i128(value).map(|value| value.to_string()),
        ElementaryType::Real | ElementaryType::Lreal => parse_typed_real(value).map(|value| {
            let rendered = value.to_string();
            if rendered.contains('.') || rendered.contains('e') || rendered.contains('E') {
                rendered
            } else {
                format!("{rendered}.0")
            }
        }),
        ElementaryType::Time => typed_literal_i128(value)
            .or_else(|| parse_duration_ms_checked(value))
            .map(|value| value.to_string()),
        ElementaryType::Date => parse_date_days(value).map(|value| value.to_string()),
        ElementaryType::TimeOfDay => parse_time_of_day_ms(value).map(|value| value.to_string()),
        ElementaryType::DateAndTime => parse_date_time_ms(value).map(|value| value.to_string()),
        ElementaryType::String | ElementaryType::WString => {
            Some(format!("\"{}\"", c_string_escape(value)))
        }
    }
}

pub(crate) fn parse_typed_bool(value: &str) -> Option<bool> {
    match canonical_identifier(value).as_str() {
        "TRUE" | "1" => Some(true),
        "FALSE" | "0" => Some(false),
        _ => None,
    }
}

pub(crate) fn parse_typed_real(value: &str) -> Option<f64> {
    let value = value.trim().replace('_', "");
    value.parse::<f64>().ok().filter(|value| value.is_finite())
}

pub(crate) fn typed_literal_i128(value: &str) -> Option<i128> {
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

pub(crate) fn valid_integer_underscore_placement(raw: &str) -> bool {
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

pub(crate) fn parse_duration_ms_checked(raw: &str) -> Option<i128> {
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

pub(crate) fn valid_decimal_component(raw: &str) -> bool {
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

pub(crate) fn parse_date_time_ms(input: &str) -> Option<i128> {
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

pub(crate) fn parse_date_days(input: &str) -> Option<i64> {
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

pub(crate) fn parse_time_of_day_ms(input: &str) -> Option<i128> {
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

pub(crate) fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

pub(crate) fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub(crate) fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
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

pub(crate) fn binary_op_to_c(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Or => "|",
        BinaryOp::Xor => "^",
        BinaryOp::And => "&",
        BinaryOp::Equal => "==",
        BinaryOp::NotEqual => "!=",
        BinaryOp::Less => "<",
        BinaryOp::LessEqual => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "%",
        BinaryOp::Power => "pow",
    }
}
