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
    pub(crate) fn check_function_call_args(
        &mut self,
        function: &Pou,
        args: &[ParamAssignment],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let inputs = function
            .var_blocks
            .iter()
            .filter(|block| block.kind == VarBlockKind::Input)
            .flat_map(|block| block.vars.iter())
            .collect::<Vec<_>>();
        let input_names = inputs
            .iter()
            .map(|var| var.name.canonical.clone())
            .collect::<BTreeSet<_>>();
        let mut bound = BTreeSet::new();
        let mut positional_index = 0_usize;

        for arg in args {
            if arg.name.as_ref().is_some_and(is_implicit_en) {
                continue;
            }
            if arg.output {
                if arg.name.as_ref().is_some_and(is_implicit_eno) {
                    continue;
                }
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "function '{}' does not accept output parameter bindings",
                        function.name.original
                    ),
                    None,
                ));
                continue;
            }

            if let Some(name) = &arg.name {
                if is_implicit_eno(name) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "function '{}' ENO must use output binding",
                            function.name.original
                        ),
                        None,
                    ));
                    continue;
                }
                if !input_names.contains(&name.canonical) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "function '{}' has no input parameter '{}'",
                            function.name.original, name.original
                        ),
                        None,
                    ));
                    continue;
                }
                if let Some(input) = inputs
                    .iter()
                    .find(|input| input.name.canonical == name.canonical)
                {
                    if let Some(expr) = &arg.expr {
                        self.check_assignment_type(
                            &input.type_spec,
                            expr,
                            variables,
                            project,
                            format!(
                                "function '{}' parameter '{}'",
                                function.name.original, name.original
                            ),
                        );
                        self.check_initialization_constraints(
                            &input.type_spec,
                            expr,
                            variables,
                            project,
                            format!(
                                "function '{}' parameter '{}'",
                                function.name.original, name.original
                            ),
                        );
                    }
                }
                if !bound.insert(name.canonical.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "function '{}' parameter '{}' is bound more than once",
                            function.name.original, name.original
                        ),
                        None,
                    ));
                }
            } else if let Some(input) = inputs.get(positional_index) {
                if !bound.insert(input.name.canonical.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "function '{}' parameter '{}' is bound more than once",
                            function.name.original, input.name.original
                        ),
                        None,
                    ));
                }
                if let Some(expr) = &arg.expr {
                    self.check_assignment_type(
                        &input.type_spec,
                        expr,
                        variables,
                        project,
                        format!(
                            "function '{}' parameter '{}'",
                            function.name.original, input.name.original
                        ),
                    );
                    self.check_initialization_constraints(
                        &input.type_spec,
                        expr,
                        variables,
                        project,
                        format!(
                            "function '{}' parameter '{}'",
                            function.name.original, input.name.original
                        ),
                    );
                }
                positional_index += 1;
            } else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "function '{}' expects {} input parameter(s)",
                        function.name.original,
                        inputs.len()
                    ),
                    None,
                ));
            }
        }

        for input in inputs {
            if !bound.contains(&input.name.canonical) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "function '{}' missing input parameter '{}'",
                        function.name.original, input.name.original
                    ),
                    None,
                ));
            }
        }
    }

    pub(crate) fn check_function_block_call_args(
        &mut self,
        function_block: &Pou,
        args: &[ParamAssignment],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let input_fields = function_block
            .var_blocks
            .iter()
            .filter(|block| matches!(block.kind, VarBlockKind::Input | VarBlockKind::InOut))
            .flat_map(|block| block.vars.iter().map(move |var| (block.kind, var)))
            .collect::<Vec<_>>();
        let output_fields = function_block
            .var_blocks
            .iter()
            .filter(|block| matches!(block.kind, VarBlockKind::Output | VarBlockKind::InOut))
            .flat_map(|block| block.vars.iter().map(move |var| (block.kind, var)))
            .collect::<Vec<_>>();
        let mut bound_inputs = BTreeSet::new();
        let mut bound_outputs = BTreeSet::new();
        let mut positional_index = 0_usize;

        for arg in args {
            if let Some(name) = &arg.name {
                if is_implicit_en(name) || is_implicit_eno(name) {
                    continue;
                }
                if arg.output {
                    let Some((_, field)) = output_fields
                        .iter()
                        .find(|(_, field)| field.name.canonical == name.canonical)
                    else {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "function block '{}' has no output parameter '{}'",
                                function_block.name.original, name.original
                            ),
                            None,
                        ));
                        continue;
                    };
                    if !bound_outputs.insert(name.canonical.clone()) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "function block '{}' output parameter '{}' is bound more than once",
                                function_block.name.original, name.original
                            ),
                            None,
                        ));
                    }
                    if let Some(variable) = &arg.variable {
                        if let Some(actual) = self.variable_type(variable, variables, project) {
                            self.check_assignment_type(
                                &actual,
                                &Expr::Variable(VariableRef::named(&field.name.original)),
                                &BTreeMap::from([(
                                    field.name.canonical.clone(),
                                    field.type_spec.clone(),
                                )]),
                                project,
                                format!(
                                    "function block '{}' output parameter '{}'",
                                    function_block.name.original, name.original
                                ),
                            );
                        }
                    }
                    continue;
                }

                let Some((kind, field)) = input_fields
                    .iter()
                    .find(|(_, field)| field.name.canonical == name.canonical)
                else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "function block '{}' has no input parameter '{}'",
                            function_block.name.original, name.original
                        ),
                        None,
                    ));
                    continue;
                };
                if !bound_inputs.insert(name.canonical.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "function block '{}' input parameter '{}' is bound more than once",
                            function_block.name.original, name.original
                        ),
                        None,
                    ));
                }
                if let Some(expr) = &arg.expr {
                    self.check_assignment_type(
                        &field.type_spec,
                        expr,
                        variables,
                        project,
                        format!(
                            "function block '{}' parameter '{}'",
                            function_block.name.original, name.original
                        ),
                    );
                    self.check_initialization_constraints(
                        &field.type_spec,
                        expr,
                        variables,
                        project,
                        format!(
                            "function block '{}' parameter '{}'",
                            function_block.name.original, name.original
                        ),
                    );
                    if *kind == VarBlockKind::InOut && !matches!(expr, Expr::Variable(_)) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "function block '{}' VAR_IN_OUT parameter '{}' requires a variable actual",
                                function_block.name.original, name.original
                            ),
                            None,
                        ));
                    }
                }
            } else if let Some((kind, field)) = input_fields.get(positional_index) {
                if !bound_inputs.insert(field.name.canonical.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "function block '{}' input parameter '{}' is bound more than once",
                            function_block.name.original, field.name.original
                        ),
                        None,
                    ));
                }
                if let Some(expr) = &arg.expr {
                    self.check_assignment_type(
                        &field.type_spec,
                        expr,
                        variables,
                        project,
                        format!(
                            "function block '{}' parameter '{}'",
                            function_block.name.original, field.name.original
                        ),
                    );
                    self.check_initialization_constraints(
                        &field.type_spec,
                        expr,
                        variables,
                        project,
                        format!(
                            "function block '{}' parameter '{}'",
                            function_block.name.original, field.name.original
                        ),
                    );
                    if *kind == VarBlockKind::InOut && !matches!(expr, Expr::Variable(_)) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "function block '{}' VAR_IN_OUT parameter '{}' requires a variable actual",
                                function_block.name.original, field.name.original
                            ),
                            None,
                        ));
                    }
                }
                positional_index += 1;
            } else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "function block '{}' has no parameter for positional argument {}",
                        function_block.name.original,
                        positional_index + 1
                    ),
                    None,
                ));
            }
        }
    }

    pub(crate) fn check_standard_function_block_call_args(
        &mut self,
        type_name: &Identifier,
        args: &[ParamAssignment],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let inputs = standard_function_block_inputs(&type_name.original);
        let outputs = standard_function_block_outputs(&type_name.original);
        let mut bound_inputs = BTreeSet::new();
        let mut bound_outputs = BTreeSet::new();
        let mut positional_index = 0_usize;

        for arg in args {
            if let Some(name) = &arg.name {
                if is_implicit_en(name) || is_implicit_eno(name) {
                    continue;
                }
                if arg.output {
                    let Some((_, spec)) = outputs
                        .iter()
                        .find(|(field, _)| canonical_identifier(field) == name.canonical)
                    else {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "standard function block '{}' has no output parameter '{}'",
                                type_name.original, name.original
                            ),
                            None,
                        ));
                        continue;
                    };
                    if !bound_outputs.insert(name.canonical.clone()) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "standard function block '{}' output parameter '{}' is bound more than once",
                                type_name.original, name.original
                            ),
                            None,
                        ));
                    }
                    if arg.negated && self.type_of_spec(spec, project) != SimpleType::Bool {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "standard function block '{}' output parameter '{}' cannot be negated",
                                type_name.original, name.original
                            ),
                            None,
                        ));
                    }
                    if let Some(variable) = &arg.variable {
                        if let Some(actual) = self.variable_type(variable, variables, project) {
                            self.check_assignment_type(
                                &actual,
                                &Expr::Variable(VariableRef::named(name.original.clone())),
                                &BTreeMap::from([(name.canonical.clone(), spec.clone())]),
                                project,
                                format!(
                                    "standard function block '{}' output parameter '{}'",
                                    type_name.original, name.original
                                ),
                            );
                        }
                    }
                    continue;
                }

                let Some((_, spec)) = inputs
                    .iter()
                    .find(|(field, _)| canonical_identifier(field) == name.canonical)
                else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "standard function block '{}' has no input parameter '{}'",
                            type_name.original, name.original
                        ),
                        None,
                    ));
                    continue;
                };
                if !bound_inputs.insert(name.canonical.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "standard function block '{}' input parameter '{}' is bound more than once",
                            type_name.original, name.original
                        ),
                        None,
                    ));
                }
                if let Some(expr) = &arg.expr {
                    self.check_assignment_type(
                        spec,
                        expr,
                        variables,
                        project,
                        format!(
                            "standard function block '{}' parameter '{}'",
                            type_name.original, name.original
                        ),
                    );
                    self.check_initialization_constraints(
                        spec,
                        expr,
                        variables,
                        project,
                        format!(
                            "standard function block '{}' parameter '{}'",
                            type_name.original, name.original
                        ),
                    );
                }
            } else if let Some((field, spec)) = inputs.get(positional_index) {
                if !bound_inputs.insert(canonical_identifier(field)) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "standard function block '{}' input parameter '{}' is bound more than once",
                            type_name.original, field
                        ),
                        None,
                    ));
                }
                if let Some(expr) = &arg.expr {
                    self.check_assignment_type(
                        spec,
                        expr,
                        variables,
                        project,
                        format!(
                            "standard function block '{}' parameter '{}'",
                            type_name.original, field
                        ),
                    );
                    self.check_initialization_constraints(
                        spec,
                        expr,
                        variables,
                        project,
                        format!(
                            "standard function block '{}' parameter '{}'",
                            type_name.original, field
                        ),
                    );
                }
                positional_index += 1;
            } else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "standard function block '{}' has no parameter for positional argument {}",
                        type_name.original,
                        positional_index + 1
                    ),
                    None,
                ));
            }
        }
    }

    pub(crate) fn check_implicit_call_controls(
        &mut self,
        call_kind: &str,
        call_name: &str,
        args: &[ParamAssignment],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let mut en_seen = false;
        let mut eno_seen = false;
        for arg in args {
            let Some(name) = &arg.name else {
                continue;
            };
            if is_implicit_en(name) {
                if !en_seen {
                    en_seen = true;
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("{call_kind} '{call_name}' EN is bound more than once"),
                        None,
                    ));
                }
                if arg.output {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("{call_kind} '{call_name}' EN must use input binding"),
                        None,
                    ));
                }
                if let Some(expr) = &arg.expr {
                    self.check_bool_expr(
                        expr,
                        variables,
                        project,
                        &format!("{call_kind} EN input"),
                    );
                }
            } else if is_implicit_eno(name) {
                if !eno_seen {
                    eno_seen = true;
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("{call_kind} '{call_name}' ENO is bound more than once"),
                        None,
                    ));
                }
                if !arg.output {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("{call_kind} '{call_name}' ENO must use output binding"),
                        None,
                    ));
                }
                if let Some(variable) = &arg.variable {
                    if let Some(spec) = self.variable_type(variable, variables, project) {
                        if self.type_of_spec(&spec, project) != SimpleType::Bool {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::Semantic,
                                format!("{call_kind} '{call_name}' ENO expects BOOL output"),
                                None,
                            ));
                        }
                    }
                }
            }
        }
    }
}
