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
    pub(crate) fn check_statement(
        &mut self,
        statement: &Statement,
        variables: &BTreeMap<String, DataTypeSpec>,
        constants: &BTreeSet<String>,
        project: &Project,
    ) {
        self.check_statement_in_context(statement, variables, constants, project, false);
    }

    pub(crate) fn check_statement_in_context(
        &mut self,
        statement: &Statement,
        variables: &BTreeMap<String, DataTypeSpec>,
        constants: &BTreeSet<String>,
        project: &Project,
        in_iteration: bool,
    ) {
        match statement {
            Statement::Assignment { target, value } => {
                self.check_variable(target, variables, project);
                if target
                    .root_name()
                    .is_some_and(|root| constants.contains(&root.canonical))
                {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("cannot assign to CONSTANT variable '{}'", target),
                        None,
                    ));
                }
                self.check_expr(value, variables, project);
                if let Some(target_type) = self.variable_type(target, variables, project) {
                    self.check_assignment_type(
                        &target_type,
                        value,
                        variables,
                        project,
                        format!("assignment to '{}'", target),
                    );
                    self.check_initialization_constraints(
                        &target_type,
                        value,
                        variables,
                        project,
                        format!("assignment to '{}'", target),
                    );
                }
            }
            Statement::FbCall { name, args } => {
                if let Some(root) = name.root_name() {
                    if is_standard_void_function(&root.original) {
                        self.check_standard_void_function_call_args(root, args, variables, project);
                        return;
                    }
                    if let Some(function) = project
                        .find_pou(&root.original)
                        .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))
                    {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "function '{}' returns a value and cannot be used as a statement",
                                root.original
                            ),
                            None,
                        ));
                        self.check_implicit_call_controls(
                            "function",
                            &root.original,
                            args,
                            variables,
                            project,
                        );
                        self.check_function_call_args(function, args, variables, project);
                        for arg in args {
                            if let Some(expr) = &arg.expr {
                                self.check_expr(expr, variables, project);
                            }
                            if let Some(variable) = &arg.variable {
                                self.check_variable(variable, variables, project);
                            }
                        }
                        return;
                    }
                    if is_standard_function(&root.original) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "function '{}' returns a value and cannot be used as a statement",
                                root.original
                            ),
                            None,
                        ));
                        self.check_implicit_call_controls(
                            "function",
                            &root.original,
                            args,
                            variables,
                            project,
                        );
                        self.check_standard_function_call_args(root, args, variables, project);
                        for arg in args {
                            if let Some(expr) = &arg.expr {
                                self.check_expr(expr, variables, project);
                            }
                            if let Some(variable) = &arg.variable {
                                self.check_variable(variable, variables, project);
                            }
                        }
                        return;
                    }
                    if !variables.contains_key(&root.canonical)
                        && !project
                            .find_pou(&root.original)
                            .is_some_and(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
                        && !is_standard_function_block(&root.original)
                    {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!("unknown function block instance '{}'", root.original),
                            None,
                        ));
                    }
                    if let Some(function_block) = variables
                        .get(&root.canonical)
                        .and_then(|spec| function_block_pou(project, spec))
                    {
                        self.check_function_block_call_args(
                            function_block,
                            args,
                            variables,
                            project,
                        );
                    } else if let Some(DataTypeSpec::Named(type_name)) =
                        variables.get(&root.canonical)
                    {
                        if is_standard_function_block(&type_name.original) {
                            self.check_standard_function_block_call_args(
                                type_name, args, variables, project,
                            );
                        }
                    }
                    self.check_implicit_call_controls(
                        "function block",
                        &root.original,
                        args,
                        variables,
                        project,
                    );
                }
                for arg in args {
                    if let Some(expr) = &arg.expr {
                        self.check_expr(expr, variables, project);
                    }
                    if let Some(variable) = &arg.variable {
                        self.check_variable(variable, variables, project);
                    }
                }
            }
            Statement::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    self.check_expr(condition, variables, project);
                    self.check_bool_expr(condition, variables, project, "IF condition");
                    for statement in body {
                        self.check_statement_in_context(
                            statement,
                            variables,
                            constants,
                            project,
                            in_iteration,
                        );
                    }
                }
                for statement in else_branch {
                    self.check_statement_in_context(
                        statement,
                        variables,
                        constants,
                        project,
                        in_iteration,
                    );
                }
            }
            Statement::Case {
                selector,
                cases,
                else_branch,
            } => {
                self.check_expr(selector, variables, project);
                let selector_type = self.type_of_expr(selector, variables, project);
                if !matches!(
                    selector_type,
                    SimpleType::Integer | SimpleType::Enum | SimpleType::Unknown
                ) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "CASE selector expects integer or enumerated, got {}",
                            selector_type.as_str()
                        ),
                        None,
                    ));
                }
                let selector_enum_type = self.enum_type_name_of_expr(selector, variables, project);
                let mut constant_ranges = Vec::<(i128, i128)>::new();
                for (labels, body) in cases {
                    for label in labels {
                        match label {
                            CaseLabel::Single(expr) => {
                                self.check_expr(expr, variables, project);
                                if selector_type == SimpleType::Enum {
                                    if let Some(enum_type) = &selector_enum_type {
                                        if let Some(value) =
                                            enum_case_label_ordinal(project, enum_type, expr)
                                        {
                                            constant_ranges.push((value, value));
                                        } else {
                                            self.diagnostics.push(Diagnostic::error(
                                                DiagnosticCode::Semantic,
                                                format!(
                                                    "CASE label expects value of enum type '{}'",
                                                    enum_type.original
                                                ),
                                                None,
                                            ));
                                        }
                                    }
                                } else {
                                    self.check_integer_expr(expr, variables, project, "CASE label");
                                    if let Some(value) =
                                        const_integer_i128(expr, variables, project, self)
                                    {
                                        constant_ranges.push((value, value));
                                    }
                                }
                            }
                            CaseLabel::Range(low, high) => {
                                self.check_expr(low, variables, project);
                                self.check_expr(high, variables, project);
                                if selector_type == SimpleType::Enum {
                                    self.diagnostics.push(Diagnostic::error(
                                        DiagnosticCode::Semantic,
                                        "CASE enumerated selector does not support range labels",
                                        None,
                                    ));
                                    continue;
                                }
                                self.check_integer_expr(
                                    low,
                                    variables,
                                    project,
                                    "CASE range lower bound",
                                );
                                self.check_integer_expr(
                                    high,
                                    variables,
                                    project,
                                    "CASE range upper bound",
                                );
                                if let (Some(low), Some(high)) = (
                                    const_integer_i128(low, variables, project, self),
                                    const_integer_i128(high, variables, project, self),
                                ) {
                                    if low > high {
                                        self.diagnostics.push(Diagnostic::error(
                                            DiagnosticCode::Semantic,
                                            format!("CASE range lower bound {low} exceeds upper bound {high}"),
                                            None,
                                        ));
                                    }
                                    constant_ranges.push((low, high));
                                }
                            }
                        }
                    }
                    for statement in body {
                        self.check_statement_in_context(
                            statement,
                            variables,
                            constants,
                            project,
                            in_iteration,
                        );
                    }
                }
                self.check_case_label_overlaps(&constant_ranges);
                for statement in else_branch {
                    self.check_statement_in_context(
                        statement,
                        variables,
                        constants,
                        project,
                        in_iteration,
                    );
                }
            }
            Statement::For {
                control,
                from,
                to,
                by,
                body,
            } => {
                if !variables.contains_key(&control.canonical) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("unknown FOR control variable '{}'", control.original),
                        None,
                    ));
                } else if let Some(control_type) = variables.get(&control.canonical) {
                    self.check_integer_spec(control_type, project, "FOR control variable");
                }
                self.check_expr(from, variables, project);
                self.check_integer_expr(from, variables, project, "FOR lower bound");
                self.check_expr(to, variables, project);
                self.check_integer_expr(to, variables, project, "FOR upper bound");
                if let Some(by) = by {
                    self.check_expr(by, variables, project);
                    self.check_integer_expr(by, variables, project, "FOR BY expression");
                    if const_i64(by, variables, project, self) == Some(0) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            "FOR BY value cannot be zero",
                            None,
                        ));
                    }
                }
                for statement in body {
                    self.check_statement_in_context(statement, variables, constants, project, true);
                }
            }
            Statement::While { condition, body } => {
                self.check_expr(condition, variables, project);
                self.check_bool_expr(condition, variables, project, "WHILE condition");
                for statement in body {
                    self.check_statement_in_context(statement, variables, constants, project, true);
                }
            }
            Statement::Repeat { body, until } => {
                for statement in body {
                    self.check_statement_in_context(statement, variables, constants, project, true);
                }
                self.check_expr(until, variables, project);
                self.check_bool_expr(until, variables, project, "REPEAT UNTIL condition");
            }
            Statement::Il { op, operand } => {
                if matches!(op, IlOp::Jmp | IlOp::Jmpc | IlOp::Jmpcn) {
                    return;
                }
                if matches!(op, IlOp::Cal | IlOp::Calc | IlOp::Calcn) {
                    self.check_il_call_operand(operand.as_ref(), variables, project);
                    return;
                }
                if matches!(op, IlOp::St | IlOp::Stn | IlOp::S | IlOp::R) {
                    self.check_il_store_operand(*op, operand.as_ref(), variables, project);
                    return;
                }
                if let Some(operand) = operand {
                    self.check_expr(operand, variables, project);
                }
            }
            Statement::Unsupported(text) => {
                self.diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::Unsupported,
                    format!("unsupported statement retained in IR: {text}"),
                    None,
                ));
            }
            Statement::Exit if !in_iteration => {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    "EXIT used outside of an iteration",
                    None,
                ));
            }
            Statement::Empty | Statement::IlLabel(_) | Statement::Exit | Statement::Return => {}
        }
    }

    pub(crate) fn check_sfc(
        &mut self,
        sfc: &Sfc,
        variables: &BTreeMap<String, DataTypeSpec>,
        constants: &BTreeSet<String>,
        project: &Project,
    ) {
        let mut steps = BTreeSet::new();
        let mut has_initial_step = false;
        for step in &sfc.steps {
            has_initial_step |= step.initial;
            if !steps.insert(step.name.canonical.clone()) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!("duplicate SFC step '{}'", step.name.original),
                    None,
                ));
            }
        }
        if !has_initial_step {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                "SFC requires at least one initial step",
                None,
            ));
        }

        let mut transitions = BTreeSet::new();
        for transition in &sfc.transitions {
            if let Some(name) = &transition.name {
                if !transitions.insert(name.canonical.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("duplicate SFC transition '{}'", name.original),
                        None,
                    ));
                }
            }
            if let Some(condition) = &transition.condition {
                self.check_expr(condition, variables, project);
                self.check_bool_expr(condition, variables, project, "SFC transition condition");
            } else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    "SFC transition requires a condition",
                    None,
                ));
            }
            if !transition.from.is_empty() || !transition.to.is_empty() {
                if transition.from.is_empty() || transition.to.is_empty() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        "SFC transition requires at least one FROM step and one TO step",
                        None,
                    ));
                }
                for from in &transition.from {
                    if !steps.contains(&from.canonical) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "SFC transition references unknown FROM step '{}'",
                                from.original
                            ),
                            None,
                        ));
                    }
                }
                for to in &transition.to {
                    if !steps.contains(&to.canonical) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "SFC transition references unknown TO step '{}'",
                                to.original
                            ),
                            None,
                        ));
                    }
                }
            }
        }

        let mut actions = BTreeSet::new();
        let mut action_defaults = BTreeMap::new();
        for action in &sfc.actions {
            if !actions.insert(action.name.canonical.clone()) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!("duplicate SFC action '{}'", action.name.original),
                    None,
                ));
            }
            self.check_sfc_action_control(
                &format!("SFC action '{}'", action.name.original),
                action.qualifier,
                action.duration.as_ref(),
            );
            action_defaults.insert(
                action.name.canonical.clone(),
                (action.name.original.clone(), action.qualifier),
            );
            for statement in &action.body {
                self.check_statement_limits(statement, 1);
                self.check_statement(statement, variables, constants, project);
            }
        }

        for step in &sfc.steps {
            let mut timed_associations = BTreeMap::<String, (String, usize)>::new();
            for association in &step.actions {
                if !actions.contains(&association.name.canonical) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "SFC step '{}' references unknown action '{}'",
                            step.name.original, association.name.original
                        ),
                        None,
                    ));
                }
                let effective_qualifier = association.qualifier.or_else(|| {
                    action_defaults
                        .get(&association.name.canonical)
                        .map(|(_, qualifier)| *qualifier)
                });
                if effective_qualifier.is_some_and(SfcActionQualifier::requires_duration) {
                    let entry = timed_associations
                        .entry(association.name.canonical.clone())
                        .or_insert_with(|| (association.name.original.clone(), 0));
                    entry.1 += 1;
                }
                self.check_sfc_action_control(
                    &format!(
                        "SFC step '{}' action association '{}'",
                        step.name.original, association.name.original
                    ),
                    effective_qualifier.unwrap_or(SfcActionQualifier::NonStored),
                    association.duration.as_ref(),
                );
            }
            for (_, (action_name, count)) in timed_associations {
                if count > 1 {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "SFC step '{}' has more than one time-related association for action '{}'",
                            step.name.original, action_name
                        ),
                        None,
                    ));
                }
            }
        }
    }

    pub(crate) fn check_sfc_action_control(
        &mut self,
        label: &str,
        qualifier: SfcActionQualifier,
        duration: Option<&Literal>,
    ) {
        if qualifier.requires_duration() && duration.is_none() {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "{label} qualifier {} requires a duration",
                    qualifier.as_iec()
                ),
                None,
            ));
        }
        if duration.is_some_and(|literal| !matches!(literal, Literal::DurationMs(_))) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("{label} qualifier duration must be TIME"),
                None,
            ));
        }
    }

    pub(crate) fn check_il_call_operand(
        &mut self,
        operand: Option<&Expr>,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        match operand {
            Some(Expr::Call { name, args }) => {
                self.check_variable(
                    &VariableRef::named(name.original.clone()),
                    variables,
                    project,
                );
                for arg in args {
                    if let Some(expr) = &arg.expr {
                        self.check_expr(expr, variables, project);
                    }
                    if let Some(variable) = &arg.variable {
                        self.check_variable(variable, variables, project);
                    }
                }
            }
            Some(Expr::Variable(variable)) => self.check_variable(variable, variables, project),
            Some(expr) => {
                self.check_expr(expr, variables, project);
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    "IL CAL instruction requires a function block instance operand",
                    None,
                ));
            }
            None => self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                "IL CAL instruction requires a function block instance operand",
                None,
            )),
        }
    }

    pub(crate) fn check_il_store_operand(
        &mut self,
        op: IlOp,
        operand: Option<&Expr>,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let op_name = il_op_name(op);
        match operand {
            Some(Expr::Variable(variable)) => {
                self.check_variable(variable, variables, project);
                if matches!(op, IlOp::Stn | IlOp::S | IlOp::R) {
                    let actual = self
                        .variable_type(variable, variables, project)
                        .map(|spec| self.type_of_spec(&spec, project))
                        .unwrap_or(SimpleType::Unknown);
                    if !types_are_assignable(SimpleType::Bool, actual) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!("IL {op_name} target expects BOOL, got {}", actual.as_str()),
                            None,
                        ));
                    }
                }
            }
            Some(expr) => {
                self.check_expr(expr, variables, project);
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!("IL {op_name} instruction requires a variable operand"),
                    None,
                ));
            }
            None => self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("IL {op_name} instruction requires a variable operand"),
                None,
            )),
        }
    }

    pub(crate) fn check_case_label_overlaps(&mut self, ranges: &[(i128, i128)]) {
        for (index, (low, high)) in ranges.iter().enumerate() {
            if low > high {
                continue;
            }
            for (other_low, other_high) in &ranges[..index] {
                if other_low > other_high {
                    continue;
                }
                if low <= other_high && high >= other_low {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "CASE label range {} overlaps previous range {}",
                            case_range_label(*low, *high),
                            case_range_label(*other_low, *other_high)
                        ),
                        None,
                    ));
                    break;
                }
            }
        }
    }

    pub(crate) fn enum_type_name_of_expr(
        &self,
        expr: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) -> Option<Identifier> {
        match expr {
            Expr::Variable(variable) => self
                .variable_type(variable, variables, project)
                .and_then(|spec| enum_type_name_for_spec(project, &spec)),
            Expr::Literal(Literal::Typed { type_name, .. }) => enum_type_root(project, type_name)
                .and_then(|_| {
                    project
                        .data_types()
                        .find(|data_type| data_type.name.canonical == type_name.canonical)
                        .map(|data_type| data_type.name.clone())
                }),
            _ => None,
        }
    }

    pub(crate) fn check_il_labels(&mut self, statements: &[Statement]) {
        let mut labels = BTreeSet::new();
        for statement in statements {
            if let Statement::IlLabel(label) = statement {
                if !labels.insert(label.canonical.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("duplicate IL label '{}'", label.original),
                        None,
                    ));
                }
            }
        }

        for statement in statements {
            if let Statement::Il {
                op: IlOp::Jmp | IlOp::Jmpc | IlOp::Jmpcn,
                operand,
            } = statement
            {
                let Some(label) = operand.as_ref().and_then(il_label_operand) else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        "IL jump instruction requires a label operand",
                        None,
                    ));
                    continue;
                };
                if !labels.contains(&label.canonical) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("unknown IL label '{}'", label.original),
                        None,
                    ));
                }
            }
        }
    }
}
