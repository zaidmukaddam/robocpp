// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_stdlib::{
    is_communication_function_block, is_standard_function, is_standard_function_block,
    is_standard_void_function, standard_function_input_index, standard_symbols, StandardSymbolKind,
};

use crate::support::*;
use crate::Checker;

impl Checker {
    pub(crate) fn check_expr(
        &mut self,
        expr: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        match expr {
            Expr::Variable(variable) => self.check_variable(variable, variables, project),
            Expr::Unary { op, expr } => {
                self.check_expr(expr, variables, project);
                self.check_unary_operator(*op, expr, variables, project);
            }
            Expr::Binary { op, left, right } => {
                self.check_expr(left, variables, project);
                self.check_expr(right, variables, project);
                self.check_binary_operator(*op, left, right, variables, project);
            }
            Expr::Call { name, args } => {
                let user_function = project
                    .find_pou(&name.original)
                    .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }));
                let standard_function = is_standard_function(&name.original);
                if !standard_function && user_function.is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("unknown function '{}'", name.original),
                        None,
                    ));
                }
                self.check_implicit_call_controls(
                    "function",
                    &name.original,
                    args,
                    variables,
                    project,
                );
                if let Some(function) = user_function {
                    self.check_function_call_args(function, args, variables, project);
                }
                if standard_function {
                    self.check_standard_function_call_args(name, args, variables, project);
                }
                self.check_conversion_range(name, args, variables, project);
                self.check_bcd_conversion_range(name, args, variables, project);
                self.check_constant_conversion_result(name, args, variables, project);
                for arg in args {
                    if let Some(expr) = &arg.expr {
                        self.check_expr(expr, variables, project);
                    }
                    if let Some(variable) = &arg.variable {
                        self.check_variable(variable, variables, project);
                    }
                }
            }
            Expr::ArrayLiteral(elements) => {
                for element in elements {
                    self.check_expr(element, variables, project);
                }
            }
            Expr::StructLiteral(fields) => {
                for field in fields {
                    if let Some(expr) = &field.expr {
                        self.check_expr(expr, variables, project);
                    }
                }
            }
            Expr::Literal(literal) => self.check_literal(literal, project),
        }
    }

    pub(crate) fn check_literal(&mut self, literal: &Literal, project: &Project) {
        let Literal::Typed { type_name, value } = literal else {
            return;
        };

        if let Some(elementary) = ElementaryType::parse(&type_name.original) {
            self.check_typed_literal_elementary(type_name, value, &elementary);
            return;
        }

        if let Some(data_type) = project
            .data_types()
            .find(|data_type| data_type.name.canonical == type_name.canonical)
        {
            self.check_typed_literal_spec(
                type_name,
                value,
                &data_type.spec,
                project,
                &mut BTreeSet::new(),
            );
        } else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("unknown typed literal type '{}'", type_name.original),
                None,
            ));
        }
    }

    pub(crate) fn check_standard_function_call_args(
        &mut self,
        name: &Identifier,
        args: &[ParamAssignment],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        self.check_standard_function_outputs(name, args);
        let input_exprs = self.standard_function_input_exprs(name, args);
        let input_types = input_exprs
            .iter()
            .map(|expr| self.type_of_expr(expr, variables, project))
            .collect::<Vec<_>>();

        if let Some(family) = bcd_conversion_source_family(&name.canonical) {
            self.check_standard_exact_args(name, &input_types, 1);
            self.check_standard_arg_family(name, &input_types, 0, family);
            return;
        }

        if let Some(family) = conversion_source_family(&name.canonical) {
            self.check_standard_exact_args(name, &input_types, 1);
            self.check_standard_arg_family(name, &input_types, 0, family);
            return;
        }

        match name.canonical.as_str() {
            "ABS" => {
                self.check_standard_exact_args(name, &input_types, 1);
                self.check_standard_family(name, &input_types, GenericFamily::AnyNum);
            }
            "SQRT" | "LN" | "LOG" | "EXP" | "SIN" | "COS" | "TAN" | "TRUNC" => {
                self.check_standard_exact_args(name, &input_types, 1);
                self.check_standard_family(name, &input_types, GenericFamily::AnyReal);
            }
            "ADD" | "SUB" | "MUL" | "DIV" => {
                self.check_standard_min_args(name, &input_types, 2);
                self.check_standard_family(name, &input_types, GenericFamily::AnyNum);
                if matches!(name.canonical.as_str(), "SUB" | "DIV") {
                    self.check_standard_exact_args(name, &input_types, 2);
                }
            }
            "MIN" | "MAX" => {
                self.check_standard_min_args(name, &input_types, 2);
                self.check_standard_family(name, &input_types, GenericFamily::AnyMagnitude);
                self.check_standard_compatible_data_args(name, &input_types, 0);
            }
            "MOD" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_family(name, &input_types, GenericFamily::AnyInt);
            }
            "EXPT" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::AnyReal);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::AnyNum);
            }
            "MOVE" => {
                self.check_standard_exact_args(name, &input_types, 1);
                self.check_standard_family(name, &input_types, GenericFamily::Any);
            }
            "LIMIT" => {
                self.check_standard_exact_args(name, &input_types, 3);
                self.check_standard_family(name, &input_types, GenericFamily::AnyMagnitude);
                self.check_standard_compatible_data_args(name, &input_types, 0);
            }
            "SEL" => {
                self.check_standard_exact_args(name, &input_types, 3);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::Bool);
                self.check_standard_compatible_data_args(name, &input_types, 1);
            }
            "MUX" => {
                self.check_standard_min_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::AnyInt);
                self.check_standard_compatible_data_args(name, &input_types, 1);
                self.check_standard_mux_selector(name, &input_exprs, variables, project);
            }
            "GT" | "GE" | "EQ" | "NE" | "LE" | "LT" => {
                self.check_standard_min_args(name, &input_types, 2);
                self.check_standard_family(name, &input_types, GenericFamily::AnyElementary);
                self.check_standard_compatible_data_args(name, &input_types, 0);
                if name.canonical == "NE" {
                    self.check_standard_exact_args(name, &input_types, 2);
                }
            }
            "SHL" | "SHR" | "ROL" | "ROR" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::AnyBit);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::AnyInt);
                self.check_standard_non_negative_arg(name, &input_exprs, 1, variables, project);
            }
            "AND" | "OR" | "XOR" => {
                self.check_standard_min_args(name, &input_types, 2);
                self.check_standard_family(name, &input_types, GenericFamily::AnyBit);
            }
            "NOT" => {
                self.check_standard_exact_args(name, &input_types, 1);
                self.check_standard_family(name, &input_types, GenericFamily::AnyBit);
            }
            "LEN" => {
                self.check_standard_exact_args(name, &input_types, 1);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::AnyString);
            }
            "LEFT" | "RIGHT" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::AnyString);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::AnyInt);
                self.check_standard_non_negative_arg(name, &input_exprs, 1, variables, project);
                self.check_standard_string_bounds(name, &input_exprs, variables, project);
            }
            "MID" => {
                self.check_standard_exact_args(name, &input_types, 3);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::AnyString);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::AnyInt);
                self.check_standard_arg_family(name, &input_types, 2, GenericFamily::AnyInt);
                self.check_standard_non_negative_arg(name, &input_exprs, 1, variables, project);
                self.check_standard_non_negative_arg(name, &input_exprs, 2, variables, project);
                self.check_standard_string_bounds(name, &input_exprs, variables, project);
            }
            "CONCAT" => {
                self.check_standard_min_args(name, &input_types, 2);
                self.check_standard_family(name, &input_types, GenericFamily::AnyString);
            }
            "INSERT" | "REPLACE" => {
                self.check_standard_exact_args(
                    name,
                    &input_types,
                    if name.canonical == "REPLACE" { 4 } else { 3 },
                );
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::AnyString);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::AnyString);
                self.check_standard_arg_family(name, &input_types, 2, GenericFamily::AnyInt);
                self.check_standard_non_negative_arg(name, &input_exprs, 2, variables, project);
                if name.canonical == "REPLACE" {
                    self.check_standard_non_negative_arg(name, &input_exprs, 3, variables, project);
                }
                self.check_standard_string_bounds(name, &input_exprs, variables, project);
            }
            "DELETE" => {
                self.check_standard_exact_args(name, &input_types, 3);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::AnyString);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::AnyInt);
                self.check_standard_arg_family(name, &input_types, 2, GenericFamily::AnyInt);
                self.check_standard_non_negative_arg(name, &input_exprs, 1, variables, project);
                self.check_standard_non_negative_arg(name, &input_exprs, 2, variables, project);
                self.check_standard_string_bounds(name, &input_exprs, variables, project);
            }
            "FIND" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::AnyString);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::AnyString);
            }
            "ADD_TIME" | "SUB_TIME" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::Time);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::Time);
            }
            "ADD_TOD_TIME" | "SUB_TOD_TIME" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::TimeOfDay);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::Time);
            }
            "ADD_DT_TIME" | "SUB_DT_TIME" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::DateAndTime);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::Time);
            }
            "CONCAT_DATE" => {
                self.check_standard_exact_args(name, &input_types, 3);
                self.check_standard_family(name, &input_types, GenericFamily::AnyInt);
            }
            "CONCAT_TOD" => {
                self.check_standard_exact_args(name, &input_types, 4);
                self.check_standard_family(name, &input_types, GenericFamily::AnyInt);
            }
            "CONCAT_DT" => {
                self.check_standard_exact_args(name, &input_types, 7);
                self.check_standard_family(name, &input_types, GenericFamily::AnyInt);
            }
            "CONCAT_DATE_TOD" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::Date);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::TimeOfDay);
            }
            "DAY_OF_WEEK" => {
                self.check_standard_exact_args(name, &input_types, 1);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::Date);
            }
            "SUB_DATE_DATE" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::Date);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::Date);
            }
            "SUB_TOD_TOD" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::TimeOfDay);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::TimeOfDay);
            }
            "SUB_DT_DT" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::DateAndTime);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::DateAndTime);
            }
            "MUL_TIME" | "DIV_TIME" | "MULTIME" | "DIVTIME" => {
                self.check_standard_exact_args(name, &input_types, 2);
                self.check_standard_arg_family(name, &input_types, 0, GenericFamily::Time);
                self.check_standard_arg_family(name, &input_types, 1, GenericFamily::AnyNum);
            }
            _ => {}
        }
    }

    pub(crate) fn check_standard_function_outputs(
        &mut self,
        name: &Identifier,
        args: &[ParamAssignment],
    ) {
        for arg in args.iter().filter(|arg| arg.output) {
            let Some(arg_name) = &arg.name else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "standard function '{}' does not accept positional output arguments",
                        name.original
                    ),
                    None,
                ));
                continue;
            };
            if !is_implicit_eno(arg_name) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "standard function '{}' has no output parameter '{}'",
                        name.original, arg_name.original
                    ),
                    None,
                ));
            }
        }
    }

    pub(crate) fn standard_function_input_exprs<'b>(
        &mut self,
        name: &Identifier,
        args: &'b [ParamAssignment],
    ) -> Vec<&'b Expr> {
        let mut ordered = Vec::new();
        let mut seen = BTreeMap::new();
        let mut positional_index = 0;
        let mut unknown_index = usize::MAX.saturating_sub(args.len());

        for arg in args {
            if arg.output || arg.name.as_ref().is_some_and(is_implicit_en) {
                continue;
            }
            let Some(expr) = arg.expr.as_ref() else {
                continue;
            };

            let (index, label) = if let Some(arg_name) = &arg.name {
                if let Some(index) =
                    standard_function_input_index(&name.original, &arg_name.original)
                {
                    (index, arg_name.original.clone())
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "standard function '{}' has no input parameter '{}'",
                            name.original, arg_name.original
                        ),
                        None,
                    ));
                    let index = unknown_index;
                    unknown_index = unknown_index.saturating_add(1);
                    (index, arg_name.original.clone())
                }
            } else {
                let index = positional_index;
                positional_index += 1;
                (index, format!("positional argument {}", positional_index))
            };

            if let Some(previous) = seen.insert(index, label.clone()) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "standard function '{}' input parameter '{}' duplicates '{}'",
                        name.original, label, previous
                    ),
                    None,
                ));
            }
            ordered.push((index, expr));
        }

        ordered.sort_by_key(|(index, _)| *index);
        ordered.into_iter().map(|(_, expr)| expr).collect()
    }

    pub(crate) fn check_standard_void_function_call_args(
        &mut self,
        name: &Identifier,
        args: &[ParamAssignment],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        for arg in args {
            if let Some(expr) = &arg.expr {
                self.check_expr(expr, variables, project);
            }
            if let Some(variable) = &arg.variable {
                self.check_variable(variable, variables, project);
            }
        }
        self.check_implicit_call_controls("function", &name.original, args, variables, project);

        let (input_family, outputs): (GenericFamily, &[&str]) = match name.canonical.as_str() {
            "SPLIT_DATE" => (GenericFamily::Date, &["YEAR", "MONTH", "DATE"]),
            "SPLIT_TOD" => (
                GenericFamily::TimeOfDay,
                &["HOUR", "MINUTE", "SECOND", "MILLISECOND"],
            ),
            "SPLIT_DT" => (
                GenericFamily::DateAndTime,
                &[
                    "YEAR",
                    "MONTH",
                    "DATE",
                    "HOUR",
                    "MINUTE",
                    "SECOND",
                    "MILLISECOND",
                ],
            ),
            _ => return,
        };

        let Some(input) = split_input_expr(args) else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "standard function '{}' expects an IN input argument",
                    name.original
                ),
                None,
            ));
            return;
        };
        let input_type = self.type_of_expr(input, variables, project);
        self.check_standard_arg_family(name, &[input_type], 0, input_family);

        if uses_formal_split_outputs(args) {
            for output in outputs {
                if let Some(variable) = split_formal_output(args, output) {
                    self.check_split_output_type(name, output, variable, variables, project);
                }
            }
            for arg in args.iter().filter(|arg| arg.output) {
                let Some(arg_name) = &arg.name else {
                    continue;
                };
                if is_implicit_eno(arg_name)
                    || outputs.iter().any(|output| *output == arg_name.canonical)
                {
                    continue;
                }
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "standard function '{}' has no output parameter '{}'",
                        name.original, arg_name.original
                    ),
                    None,
                ));
            }
        } else {
            let positional = split_positional_args(args);
            let expected = outputs.len() + 1;
            if positional.len() != expected {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "standard function '{}' expects exactly {expected} positional argument(s), got {}",
                        name.original,
                        positional.len()
                    ),
                    None,
                ));
            }
            for (index, output) in outputs.iter().enumerate() {
                let Some(expr) = positional.get(index + 1) else {
                    continue;
                };
                let Expr::Variable(variable) = expr else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "standard function '{}' output '{}' requires a variable actual",
                            name.original, output
                        ),
                        None,
                    ));
                    continue;
                };
                self.check_split_output_type(name, output, variable, variables, project);
            }
        }
    }

    pub(crate) fn check_split_output_type(
        &mut self,
        name: &Identifier,
        output: &str,
        variable: &VariableRef,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let actual = self
            .variable_type(variable, variables, project)
            .map(|spec| self.type_of_spec(&spec, project))
            .unwrap_or(SimpleType::Unknown);
        if !matches!(actual, SimpleType::Integer | SimpleType::Unknown) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "standard function '{}' output '{}' expects INT-compatible variable, got {}",
                    name.original,
                    output,
                    actual.as_str()
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_standard_min_args(
        &mut self,
        name: &Identifier,
        input_types: &[SimpleType],
        min: usize,
    ) {
        if input_types.len() < min {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "standard function '{}' expects at least {min} input argument(s), got {}",
                    name.original,
                    input_types.len()
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_standard_exact_args(
        &mut self,
        name: &Identifier,
        input_types: &[SimpleType],
        expected: usize,
    ) {
        if input_types.len() != expected {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "standard function '{}' expects exactly {expected} input argument(s), got {}",
                    name.original,
                    input_types.len()
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_standard_family(
        &mut self,
        name: &Identifier,
        input_types: &[SimpleType],
        family: GenericFamily,
    ) {
        for (index, actual) in input_types.iter().copied().enumerate() {
            self.check_standard_arg_type(name, index, actual, family);
        }
    }

    pub(crate) fn check_standard_arg_family(
        &mut self,
        name: &Identifier,
        input_types: &[SimpleType],
        index: usize,
        family: GenericFamily,
    ) {
        if let Some(actual) = input_types.get(index).copied() {
            self.check_standard_arg_type(name, index, actual, family);
        }
    }

    pub(crate) fn check_standard_non_negative_arg(
        &mut self,
        name: &Identifier,
        input_exprs: &[&Expr],
        index: usize,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let Some(value) = input_exprs
            .get(index)
            .and_then(|expr| const_i64(expr, variables, project, self))
        else {
            return;
        };
        if value < 0 {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "standard function '{}' argument {} must be non-negative, got {value}",
                    name.original,
                    index + 1
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_standard_mux_selector(
        &mut self,
        name: &Identifier,
        input_exprs: &[&Expr],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let input_count = input_exprs.len().saturating_sub(1);
        if input_count == 0 {
            return;
        }
        let Some(selector) = input_exprs
            .first()
            .and_then(|expr| const_i64(expr, variables, project, self))
        else {
            return;
        };
        if selector < 0 || selector >= input_count as i64 {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "standard function '{}' selector must be in range 0..{}, got {selector}",
                    name.original,
                    input_count - 1
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_standard_string_bounds(
        &mut self,
        name: &Identifier,
        input_exprs: &[&Expr],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let Some(input) = input_exprs
            .first()
            .and_then(|expr| const_string_expr(expr, variables, project, self))
        else {
            return;
        };
        let input_len = input.chars().count() as i64;
        let int_arg = |index: usize| {
            input_exprs
                .get(index)
                .and_then(|expr| const_i64(expr, variables, project, self))
        };

        match name.canonical.as_str() {
            "LEFT" | "RIGHT" => {
                if let Some(length) = int_arg(1) {
                    if length > input_len {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "standard function '{}' length {length} exceeds string length {input_len}",
                                name.original
                            ),
                            None,
                        ));
                    }
                }
            }
            "MID" => {
                if let (Some(length), Some(position)) = (int_arg(1), int_arg(2)) {
                    self.check_string_position(name, position, input_len);
                    self.check_string_range(name, position, length, input_len);
                }
            }
            "INSERT" => {
                if let Some(position) = int_arg(2) {
                    if position > input_len {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "standard function '{}' insert position {position} is outside range 0..{input_len}",
                                name.original
                            ),
                            None,
                        ));
                    }
                }
            }
            "DELETE" => {
                if let (Some(length), Some(position)) = (int_arg(1), int_arg(2)) {
                    self.check_string_position(name, position, input_len);
                    self.check_string_range(name, position, length, input_len);
                }
            }
            "REPLACE" => {
                if let (Some(length), Some(position)) = (int_arg(2), int_arg(3)) {
                    self.check_string_position(name, position, input_len);
                    self.check_string_range(name, position, length, input_len);
                }
            }
            _ => {}
        }
    }

    pub(crate) fn check_string_position(
        &mut self,
        name: &Identifier,
        position: i64,
        input_len: i64,
    ) {
        if position <= 0 || position > input_len {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "standard function '{}' position {position} is outside string positions 1..{input_len}",
                    name.original
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_string_range(
        &mut self,
        name: &Identifier,
        position: i64,
        length: i64,
        input_len: i64,
    ) {
        if position > 0 && length >= 0 && position - 1 + length > input_len {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "standard function '{}' length {length} from position {position} exceeds string length {input_len}",
                    name.original
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_standard_arg_type(
        &mut self,
        name: &Identifier,
        index: usize,
        actual: SimpleType,
        family: GenericFamily,
    ) {
        if !family.contains(actual) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "standard function '{}' argument {} expects {}, got {}",
                    name.original,
                    index + 1,
                    family.as_str(),
                    actual.as_str()
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_standard_compatible_data_args(
        &mut self,
        name: &Identifier,
        input_types: &[SimpleType],
        start: usize,
    ) {
        let Some(first) = input_types.get(start).copied() else {
            return;
        };
        if first == SimpleType::Unknown {
            return;
        }
        for actual in input_types.iter().copied().skip(start + 1) {
            if actual != SimpleType::Unknown && !types_have_common_value_type(first, actual) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "standard function '{}' data arguments must have compatible types, got {} and {}",
                        name.original,
                        first.as_str(),
                        actual.as_str()
                    ),
                    None,
                ));
                return;
            }
        }
    }

    pub(crate) fn check_typed_literal_spec(
        &mut self,
        type_name: &Identifier,
        value: &str,
        spec: &DataTypeSpec,
        project: &Project,
        seen: &mut BTreeSet<String>,
    ) {
        match spec {
            DataTypeSpec::Elementary(elementary) => {
                self.check_typed_literal_elementary(type_name, value, elementary);
            }
            DataTypeSpec::Enum { values } => {
                let value_name = canonical_identifier(value);
                if !values.iter().any(|value| value.canonical == value_name) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "typed enum literal '{}#{}' is not a value of '{}'",
                            type_name.original, value, type_name.original
                        ),
                        None,
                    ));
                }
            }
            DataTypeSpec::Subrange { range, .. } => {
                if let Some(value) = typed_literal_i128(value) {
                    if value < i128::from(range.low) || value > i128::from(range.high) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "typed literal value {value} is outside subrange {}..{}",
                                range.low, range.high
                            ),
                            None,
                        ));
                    }
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "typed literal '{}#{}' is not a valid integer value",
                            type_name.original, value
                        ),
                        None,
                    ));
                }
            }
            DataTypeSpec::Named(name) => {
                if !seen.insert(name.canonical.clone()) {
                    return;
                }
                if let Some(data_type) = project
                    .data_types()
                    .find(|data_type| data_type.name.canonical == name.canonical)
                {
                    self.check_typed_literal_spec(type_name, value, &data_type.spec, project, seen);
                }
            }
            DataTypeSpec::String { .. }
            | DataTypeSpec::Array { .. }
            | DataTypeSpec::Struct { .. } => {}
        }
    }

    pub(crate) fn check_typed_literal_elementary(
        &mut self,
        type_name: &Identifier,
        value: &str,
        elementary: &ElementaryType,
    ) {
        match elementary {
            ElementaryType::Bool => {
                if !matches!(
                    canonical_identifier(value).as_str(),
                    "0" | "1" | "FALSE" | "TRUE"
                ) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "typed literal '{}#{}' is not a valid BOOL value",
                            type_name.original, value
                        ),
                        None,
                    ));
                }
            }
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
            | ElementaryType::Lword => {
                let Some(value) = typed_literal_i128(value) else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "typed literal '{}#{}' is not a valid integer value",
                            type_name.original, value
                        ),
                        None,
                    ));
                    return;
                };
                if let Some((range_name, low, high)) = elementary_integer_range(elementary) {
                    if value < low || value > high {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "typed literal value {value} is outside {range_name} range {low}..{high}"
                            ),
                            None,
                        ));
                    }
                }
            }
            ElementaryType::Real | ElementaryType::Lreal => {
                let parsed = real_literal_f64(value);
                if parsed.is_none() {
                    let type_label = match elementary {
                        ElementaryType::Real => "REAL",
                        ElementaryType::Lreal => "LREAL",
                        _ => unreachable!(),
                    };
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "typed literal '{}#{}' is not a valid {type_label} value",
                            type_name.original, value
                        ),
                        None,
                    ));
                }
                if matches!(elementary, ElementaryType::Real)
                    && parsed.is_some_and(|value| value.abs() > f64::from(f32::MAX))
                {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "typed literal value {value} is outside REAL range -{}..{}",
                            f32::MAX,
                            f32::MAX
                        ),
                        None,
                    ));
                }
            }
            ElementaryType::Time => {
                if parse_duration_ms_checked(value).is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "typed literal '{}#{}' is not a valid TIME value",
                            type_name.original, value
                        ),
                        None,
                    ));
                }
            }
            ElementaryType::Date => {
                if parse_date_days(value).is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "typed literal '{}#{}' is not a valid DATE value",
                            type_name.original, value
                        ),
                        None,
                    ));
                }
            }
            ElementaryType::TimeOfDay => {
                if parse_time_of_day_ms(value).is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "typed literal '{}#{}' is not a valid TIME_OF_DAY value",
                            type_name.original, value
                        ),
                        None,
                    ));
                }
            }
            ElementaryType::DateAndTime => {
                if parse_date_time_ms(value).is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "typed literal '{}#{}' is not a valid DATE_AND_TIME value",
                            type_name.original, value
                        ),
                        None,
                    ));
                }
            }
            ElementaryType::String | ElementaryType::WString => {}
        }
    }

    pub(crate) fn check_unary_operator(
        &mut self,
        op: UnaryOp,
        expr: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let actual = self.type_of_expr(expr, variables, project);
        let valid = match op {
            UnaryOp::Neg => matches!(
                actual,
                SimpleType::Integer | SimpleType::Real | SimpleType::Unknown
            ),
            UnaryOp::Not => matches!(
                actual,
                SimpleType::Bool
                    | SimpleType::Integer
                    | SimpleType::BitString
                    | SimpleType::Unknown
            ),
        };
        if !valid {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "operator {} cannot be applied to {}",
                    unary_op_name(op),
                    actual.as_str()
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_binary_operator(
        &mut self,
        op: BinaryOp,
        left: &Expr,
        right: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let left_type = self.type_of_expr(left, variables, project);
        let right_type = self.type_of_expr(right, variables, project);
        if matches!(left_type, SimpleType::Unknown) || matches!(right_type, SimpleType::Unknown) {
            return;
        }

        let valid = match op {
            BinaryOp::Add | BinaryOp::Sub => {
                (is_numeric_simple(left_type) && is_numeric_simple(right_type))
                    || (left_type == SimpleType::Time && right_type == SimpleType::Time)
            }
            BinaryOp::Mul | BinaryOp::Div => {
                is_numeric_simple(left_type) && is_numeric_simple(right_type)
            }
            BinaryOp::Mod => left_type == SimpleType::Integer && right_type == SimpleType::Integer,
            BinaryOp::Power => is_numeric_simple(left_type) && is_numeric_simple(right_type),
            BinaryOp::And | BinaryOp::Or | BinaryOp::Xor => {
                (left_type == SimpleType::Bool && right_type == SimpleType::Bool)
                    || (is_bitwise_simple(left_type) && is_bitwise_simple(right_type))
            }
            BinaryOp::Equal | BinaryOp::NotEqual => {
                left_type != SimpleType::Aggregate
                    && right_type != SimpleType::Aggregate
                    && types_have_common_value_type(left_type, right_type)
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                (is_numeric_simple(left_type) && is_numeric_simple(right_type))
                    || left_type == right_type
                        && matches!(
                            left_type,
                            SimpleType::String
                                | SimpleType::Time
                                | SimpleType::Date
                                | SimpleType::TimeOfDay
                                | SimpleType::DateAndTime
                        )
            }
        };

        if !valid {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "operator {} cannot be applied to {} and {}",
                    binary_op_name(op),
                    left_type.as_str(),
                    right_type.as_str()
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_conversion_range(
        &mut self,
        name: &Identifier,
        args: &[ParamAssignment],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let Some((target, low, high)) = conversion_target_integer_range(&name.canonical) else {
            return;
        };
        let Some(value) = args
            .iter()
            .filter(|arg| !arg.output)
            .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
            .filter_map(|arg| arg.expr.as_ref())
            .next()
            .and_then(|expr| const_conversion_i128(expr, variables, project, self))
        else {
            return;
        };
        if value < low || value > high {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "conversion '{}' value {value} is outside target range {low}..{high} for {target}",
                    name.original
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_bcd_conversion_range(
        &mut self,
        name: &Identifier,
        args: &[ParamAssignment],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let Some(kind) = bcd_conversion_kind(&name.canonical) else {
            return;
        };
        let Some(value) = args
            .iter()
            .filter(|arg| !arg.output)
            .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
            .filter_map(|arg| arg.expr.as_ref())
            .next()
            .and_then(|expr| const_conversion_i128(expr, variables, project, self))
        else {
            return;
        };
        match kind {
            BcdConversionKind::BcdToInt { digits } => {
                if bcd_decode_i128(value, digits).is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "conversion '{}' value {value} is not valid BCD",
                            name.original
                        ),
                        None,
                    ));
                }
            }
            BcdConversionKind::IntToBcd { digits } => {
                if bcd_encode_i128(value, digits).is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "conversion '{}' value {value} cannot be represented as BCD",
                            name.original
                        ),
                        None,
                    ));
                }
            }
        }
    }

    pub(crate) fn check_constant_conversion_result(
        &mut self,
        name: &Identifier,
        args: &[ParamAssignment],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let Some(source_family) = conversion_source_family(&name.canonical)
            .or_else(|| bcd_conversion_source_family(&name.canonical))
        else {
            return;
        };
        let Some(input) = ordered_standard_function_input_exprs(name, args)
            .into_iter()
            .next()
        else {
            return;
        };
        let input_type = self.type_of_expr(input, variables, project);
        if !source_family.contains(input_type) {
            return;
        }

        let Some(input_value) = const_standard_value(input, variables, project, self) else {
            return;
        };

        if conversion_target_integer_range(&name.canonical).is_some()
            && const_conversion_i128(input, variables, project, self).is_some()
        {
            return;
        }
        if bcd_conversion_kind(&name.canonical).is_some()
            && const_conversion_i128(input, variables, project, self).is_some()
        {
            return;
        }

        match iec_stdlib::eval_standard_function(&name.original, &[input_value]) {
            Some(Value::Real(value)) if !value.is_finite() => {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "conversion '{}' produced non-finite REAL from constant input",
                        name.original
                    ),
                    None,
                ));
            }
            Some(_) => {}
            None => {
                let target = name
                    .canonical
                    .split_once("_TO_")
                    .map(|(_, target)| target)
                    .unwrap_or("target type");
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "conversion '{}' cannot convert constant input to {target}",
                        name.original
                    ),
                    None,
                ));
            }
        }
    }

    pub(crate) fn check_assignment_type(
        &mut self,
        expected: &DataTypeSpec,
        value: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
        context: String,
    ) {
        let expected = self.type_of_spec(expected, project);
        let actual = self.type_of_expr(value, variables, project);
        if !types_are_assignable(expected, actual) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "{context} expects {}, got {}",
                    expected.as_str(),
                    actual.as_str()
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_initialization_constraints(
        &mut self,
        expected: &DataTypeSpec,
        value: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
        context: String,
    ) {
        match expected {
            DataTypeSpec::Named(name) => {
                if let Some(data_type) = project
                    .data_types()
                    .find(|data_type| data_type.name.canonical == name.canonical)
                {
                    if let DataTypeSpec::Enum { values } = &data_type.spec {
                        self.check_enum_initialization_constraints(
                            Some(&data_type.name),
                            values,
                            value,
                            variables,
                            project,
                            context,
                        );
                        return;
                    }
                    self.check_initialization_constraints(
                        &data_type.spec,
                        value,
                        variables,
                        project,
                        context,
                    );
                }
            }
            DataTypeSpec::Subrange { range, .. } => {
                if let Some(value) = const_integer_i128(value, variables, project, self) {
                    if value < i128::from(range.low) || value > i128::from(range.high) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "{context} value {value} is outside subrange {}..{}",
                                range.low, range.high
                            ),
                            None,
                        ));
                    }
                }
            }
            DataTypeSpec::Elementary(elementary) => {
                if let Some((type_name, low, high)) = elementary_integer_range(elementary) {
                    if let Some(value) = const_integer_i128(value, variables, project, self) {
                        if value < low || value > high {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::Semantic,
                                format!(
                                    "{context} value {value} is outside {type_name} range {low}..{high}"
                                ),
                                None,
                            ));
                        }
                    }
                }
            }
            DataTypeSpec::Enum { values } => {
                self.check_enum_initialization_constraints(
                    None, values, value, variables, project, context,
                );
            }
            DataTypeSpec::String {
                length: Some(length),
                ..
            } => {
                if let Some(value) = const_string_expr(value, variables, project, self) {
                    if value.chars().count() > *length {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "{context} exceeds string length {length} with {} character(s)",
                                value.chars().count()
                            ),
                            None,
                        ));
                    }
                }
            }
            DataTypeSpec::Array {
                ranges,
                element_type,
            } => {
                let expected = DataTypeSpec::Array {
                    ranges: ranges.clone(),
                    element_type: element_type.clone(),
                };
                let Expr::ArrayLiteral(elements) = value else {
                    if self
                        .expr_data_spec(value, variables, project)
                        .is_some_and(|actual| {
                            self.array_specs_assignable(&expected, &actual, project)
                        })
                    {
                        return;
                    }
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("{context} expects a compatible array value"),
                        None,
                    ));
                    return;
                };
                let expected_len = array_element_count(ranges);
                if elements.len() != expected_len {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "{context} expects {expected_len} array element(s), got {}",
                            elements.len()
                        ),
                        None,
                    ));
                }
                for (index, element) in elements.iter().enumerate() {
                    self.check_assignment_type(
                        element_type,
                        element,
                        variables,
                        project,
                        format!("{context} element {}", index + 1),
                    );
                    self.check_initialization_constraints(
                        element_type,
                        element,
                        variables,
                        project,
                        format!("{context} element {}", index + 1),
                    );
                }
            }
            DataTypeSpec::Struct { fields } => {
                let expected = DataTypeSpec::Struct {
                    fields: fields.clone(),
                };
                let Expr::StructLiteral(initializers) = value else {
                    if self
                        .expr_data_spec(value, variables, project)
                        .is_some_and(|actual| {
                            self.struct_specs_assignable(&expected, &actual, project)
                        })
                    {
                        return;
                    }
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("{context} expects a compatible structure value"),
                        None,
                    ));
                    return;
                };
                let mut seen = BTreeSet::new();
                for initializer in initializers {
                    let Some(name) = &initializer.name else {
                        continue;
                    };
                    if !seen.insert(name.canonical.clone()) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "{context} initializes field '{}' more than once",
                                name.original
                            ),
                            None,
                        ));
                    }
                    let Some(field) = fields
                        .iter()
                        .find(|field| field.name.canonical == name.canonical)
                    else {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!("{context} has unknown structure field '{}'", name.original),
                            None,
                        ));
                        continue;
                    };
                    if let Some(expr) = &initializer.expr {
                        self.check_assignment_type(
                            &field.spec,
                            expr,
                            variables,
                            project,
                            format!("{context} field '{}'", name.original),
                        );
                        self.check_initialization_constraints(
                            &field.spec,
                            expr,
                            variables,
                            project,
                            format!("{context} field '{}'", name.original),
                        );
                    }
                }
            }
            _ => {}
        }
    }

    pub(crate) fn expr_data_spec(
        &self,
        expr: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) -> Option<DataTypeSpec> {
        match expr {
            Expr::Variable(variable) => self.variable_type(variable, variables, project),
            Expr::Call { name, .. } => project.find_pou(&name.original).and_then(|pou| {
                if let PouKind::Function { return_type } = &pou.kind {
                    Some(return_type.clone())
                } else {
                    None
                }
            }),
            _ => None,
        }
    }

    pub(crate) fn array_specs_assignable(
        &self,
        expected: &DataTypeSpec,
        actual: &DataTypeSpec,
        project: &Project,
    ) -> bool {
        let expected = self.resolve_named_spec(expected, project);
        let actual = self.resolve_named_spec(actual, project);
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
                    && self.array_specs_assignable(&expected_element, &actual_element, project)
            }
            (expected, actual) => self.data_specs_assignable(&expected, &actual, project),
        }
    }

    pub(crate) fn struct_specs_assignable(
        &self,
        expected: &DataTypeSpec,
        actual: &DataTypeSpec,
        project: &Project,
    ) -> bool {
        let expected = self.resolve_named_spec(expected, project);
        let actual = self.resolve_named_spec(actual, project);
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
                        && self.data_specs_assignable(&expected.spec, &actual.spec, project)
                })
    }

    pub(crate) fn data_specs_assignable(
        &self,
        expected: &DataTypeSpec,
        actual: &DataTypeSpec,
        project: &Project,
    ) -> bool {
        let expected = self.resolve_named_spec(expected, project);
        let actual = self.resolve_named_spec(actual, project);
        match (&expected, &actual) {
            (DataTypeSpec::Array { .. }, DataTypeSpec::Array { .. }) => {
                self.array_specs_assignable(&expected, &actual, project)
            }
            (DataTypeSpec::Struct { .. }, DataTypeSpec::Struct { .. }) => {
                self.struct_specs_assignable(&expected, &actual, project)
            }
            _ => types_are_assignable(
                self.type_of_spec(&expected, project),
                self.type_of_spec(&actual, project),
            ),
        }
    }

    pub(crate) fn check_enum_initialization_constraints(
        &mut self,
        expected_type: Option<&Identifier>,
        values: &[Identifier],
        value: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
        context: String,
    ) {
        if let Expr::Literal(Literal::Typed {
            type_name,
            value: literal_value,
        }) = value
        {
            if let Some(expected_type) = expected_type {
                let expected_root = enum_type_root(project, expected_type);
                let actual_root = enum_type_root(project, type_name);
                if expected_root.is_none() || actual_root.is_none() || expected_root != actual_root
                {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "{context} expects enum type '{}', got typed enum literal '{}#{}'",
                            expected_type.original, type_name.original, literal_value
                        ),
                        None,
                    ));
                    return;
                }
            }
        }

        let valid =
            self.enum_expr_matches_expected(expected_type, values, value, variables, project);
        if !valid {
            let allowed = values
                .iter()
                .map(|value| value.original.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("{context} expects one of: {allowed}"),
                None,
            ));
        }
    }

    pub(crate) fn enum_expr_matches_expected(
        &self,
        expected_type: Option<&Identifier>,
        values: &[Identifier],
        expr: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) -> bool {
        if enum_expr_name(expr)
            .is_some_and(|name| values.iter().any(|value| value.canonical == name))
        {
            return true;
        }
        if let Some(expected_type) = expected_type {
            if let Expr::Variable(variable) = expr {
                if self
                    .variable_type(variable, variables, project)
                    .and_then(|spec| enum_type_name_for_spec(project, &spec))
                    .and_then(|type_name| enum_type_root(project, &type_name))
                    == enum_type_root(project, expected_type)
                {
                    return true;
                }
            }
            if let Expr::Call { name, args } = expr {
                if let Some(data_args) = enum_selection_data_args(name, args) {
                    return !data_args.is_empty()
                        && data_args.iter().all(|arg| {
                            self.enum_expr_matches_expected(
                                Some(expected_type),
                                values,
                                arg,
                                variables,
                                project,
                            )
                        });
                }
            }
        }
        false
    }

    pub(crate) fn check_bool_expr(
        &mut self,
        expr: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
        context: &str,
    ) {
        let actual = self.type_of_expr(expr, variables, project);
        if !types_are_assignable(SimpleType::Bool, actual) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("{context} expects BOOL, got {}", actual.as_str()),
                None,
            ));
        }
    }

    pub(crate) fn check_integer_expr(
        &mut self,
        expr: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
        context: &str,
    ) {
        let actual = self.type_of_expr(expr, variables, project);
        if !matches!(actual, SimpleType::Integer | SimpleType::Unknown) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("{context} expects integer, got {}", actual.as_str()),
                None,
            ));
        }
    }

    pub(crate) fn check_integer_spec(
        &mut self,
        spec: &DataTypeSpec,
        project: &Project,
        context: &str,
    ) {
        let actual = self.type_of_spec(spec, project);
        if !matches!(actual, SimpleType::Integer | SimpleType::Unknown) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("{context} expects integer, got {}", actual.as_str()),
                None,
            ));
        }
    }
}
