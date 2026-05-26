use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticCode};
use iec_ir::*;
use iec_profile::{EditionProfile, ImplementationParameters};
use iec_stdlib::{
    is_communication_function_block, is_standard_function, is_standard_function_block,
    standard_symbols, StandardSymbolKind,
};

#[derive(Debug, Clone)]
pub struct CheckOptions {
    pub profile: EditionProfile,
    pub implementation: ImplementationParameters,
}

impl Default for CheckOptions {
    fn default() -> Self {
        Self {
            profile: EditionProfile::default(),
            implementation: ImplementationParameters::default(),
        }
    }
}

pub fn check_project(project: &Project, options: &CheckOptions) -> Vec<Diagnostic> {
    let mut checker = Checker {
        options: options.clone(),
        diagnostics: DiagnosticBag::new(),
    };
    checker.check(project);
    checker.diagnostics.into_vec()
}

struct Checker {
    options: CheckOptions,
    diagnostics: DiagnosticBag,
}

impl Checker {
    fn check(&mut self, project: &Project) {
        if !self.options.profile.is_claimable() {
            self.diagnostics.push(Diagnostic::warning(
                DiagnosticCode::Compliance,
                format!(
                    "profile '{}' is a placeholder and cannot be used for a compliance claim yet",
                    self.options.profile
                ),
                None,
            ));
        }

        self.check_library_duplicates(project);
        self.check_global_variable_duplicates(project);
        self.check_type_declarations(project);
        for pou in project.pous() {
            self.check_pou(project, pou);
        }
        for element in &project.library_elements {
            if let LibraryElement::Configuration(configuration) = element {
                self.check_configuration(project, configuration);
            }
        }
    }

    fn check_library_duplicates(&mut self, project: &Project) {
        let mut names = BTreeSet::new();
        for element in &project.library_elements {
            let name = element.name();
            self.check_identifier_profile(name, "library element name");
            if !names.insert(name.canonical.clone()) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!("duplicate library element '{}'", name.original),
                    None,
                ));
            }
        }
    }

    fn check_global_variable_duplicates(&mut self, project: &Project) {
        let mut names = BTreeMap::new();
        for pou in project.pous() {
            for block in &pou.var_blocks {
                if block.kind != VarBlockKind::Global {
                    continue;
                }
                for var in &block.vars {
                    if let Some(previous) =
                        names.insert(var.name.canonical.clone(), var.name.original.clone())
                    {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "duplicate global variable '{}' previously declared as '{}'",
                                var.name.original, previous
                            ),
                            None,
                        ));
                    }
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
                if block.kind != VarBlockKind::Global {
                    continue;
                }
                for var in &block.vars {
                    if let Some(previous) =
                        names.insert(var.name.canonical.clone(), var.name.original.clone())
                    {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "duplicate global variable '{}' previously declared as '{}'",
                                var.name.original, previous
                            ),
                            None,
                        ));
                    }
                }
            }
        }
    }

    fn check_type_declarations(&mut self, project: &Project) {
        let known_types = self.known_types(project);
        for data_type in project.data_types() {
            self.check_identifier_profile(&data_type.name, "type name");
            self.check_type_spec(&data_type.spec, &known_types);
        }
    }

    fn check_pou(&mut self, project: &Project, pou: &Pou) {
        let known_types = self.known_types(project);
        let mut variables = self.project_global_variables(project, &pou.name.canonical);
        let mut constants = BTreeSet::new();
        self.check_identifier_profile(&pou.name, "POU name");

        for block in &pou.var_blocks {
            self.check_var_block_qualifiers(pou, block);
            for var in &block.vars {
                if block.constant {
                    constants.insert(var.name.canonical.clone());
                }
                self.check_identifier_profile(&var.name, "variable name");
                if let Some(location) = &var.location {
                    self.check_direct_variable_location(location);
                }

                if variables
                    .insert(var.name.canonical.clone(), var.type_spec.clone())
                    .is_some()
                {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "duplicate variable '{}' in POU '{}'",
                            var.name.original, pou.name.original
                        ),
                        None,
                    ));
                }
                self.check_type_spec(&var.type_spec, &known_types);
                if let DataTypeSpec::Named(type_name) = &var.type_spec {
                    if is_communication_function_block(&type_name.original) {
                        self.diagnostics.push(Diagnostic::warning(
                            DiagnosticCode::Unsupported,
                            format!(
                                "communication function block '{}' is recognized but not simulated",
                                type_name.original
                            ),
                            None,
                        ));
                    }
                }
                if let Some(initial_value) = &var.initial_value {
                    self.check_expr_limit(initial_value, "variable initial value");
                    self.check_expr(initial_value, &variables, project);
                    self.check_assignment_type(
                        &var.type_spec,
                        initial_value,
                        &variables,
                        project,
                        format!("initial value for variable '{}'", var.name.original),
                    );
                    self.check_initialization_constraints(
                        &var.type_spec,
                        initial_value,
                        &variables,
                        project,
                        format!("initial value for variable '{}'", var.name.original),
                    );
                }
            }
        }

        if let PouKind::Function { return_type } = &pou.kind {
            self.check_type_spec(return_type, &known_types);
            variables.insert(pou.name.canonical.clone(), return_type.clone());
            if !statements_definitely_assign(&pou.body.statements, &pou.name.canonical) {
                self.diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::Semantic,
                    format!(
                        "function '{}' does not assign to its return variable on all paths",
                        pou.name.original
                    ),
                    None,
                ));
            }
        }

        self.check_il_labels(&pou.body.statements);
        for statement in &pou.body.statements {
            self.check_statement_limits(statement, 1);
            self.check_statement(statement, &variables, &constants, project);
        }
        if let Some(sfc) = &pou.body.sfc {
            self.check_sfc(sfc, &variables, &constants, project);
        }
    }

    fn project_global_variables(
        &self,
        project: &Project,
        current_pou: &str,
    ) -> BTreeMap<String, DataTypeSpec> {
        let mut variables = BTreeMap::new();
        for pou in project.pous() {
            if pou.name.canonical == current_pou {
                continue;
            }
            for block in &pou.var_blocks {
                if block.kind != VarBlockKind::Global {
                    continue;
                }
                for var in &block.vars {
                    variables
                        .entry(var.name.canonical.clone())
                        .or_insert_with(|| var.type_spec.clone());
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
                if block.kind != VarBlockKind::Global {
                    continue;
                }
                for var in &block.vars {
                    variables
                        .entry(var.name.canonical.clone())
                        .or_insert_with(|| var.type_spec.clone());
                }
            }
        }
        variables
    }

    fn check_var_block_qualifiers(&mut self, pou: &Pou, block: &VarBlock) {
        if let Some(retain) = block.retain {
            if matches!(&pou.kind, PouKind::Function { .. }) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "FUNCTION '{}' cannot declare {} variables",
                        pou.name.original,
                        retain_kind_label(retain)
                    ),
                    None,
                ));
            }

            if !matches!(
                block.kind,
                VarBlockKind::Local
                    | VarBlockKind::Input
                    | VarBlockKind::Output
                    | VarBlockKind::Global
            ) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "{} cannot be declared {}",
                        var_block_kind_label(block.kind),
                        retain_kind_label(retain)
                    ),
                    None,
                ));
            }

            if block.constant {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "{} CONSTANT cannot also be declared {}",
                        var_block_kind_label(block.kind),
                        retain_kind_label(retain)
                    ),
                    None,
                ));
            }
        }
    }

    fn check_identifier_profile(&mut self, identifier: &Identifier, context: &str) {
        if identifier.original.len() > self.options.implementation.max_identifier_length {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Compliance,
                format!(
                    "identifier '{}' exceeds maximum length {}",
                    identifier.original, self.options.implementation.max_identifier_length
                ),
                None,
            ));
        }

        if self.options.profile == EditionProfile::Iec61131_3_2003Strict
            && (identifier.original.contains("__") || identifier.original.ends_with('_'))
        {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Compliance,
                format!(
                    "{context} '{}' violates 2003-strict identifier underscore rules",
                    identifier.original
                ),
                None,
            ));
        }
    }

    fn check_statement_limits(&mut self, statement: &Statement, depth: usize) {
        if depth > self.options.implementation.max_statement_depth {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Compliance,
                format!(
                    "statement nesting depth {depth} exceeds maximum {}",
                    self.options.implementation.max_statement_depth
                ),
                None,
            ));
        }

        match statement {
            Statement::Assignment { value, .. } => {
                self.check_expr_limit(value, "assignment expression");
            }
            Statement::FbCall { args, .. } => {
                for arg in args {
                    if let Some(expr) = &arg.expr {
                        self.check_expr_limit(expr, "function block argument");
                    }
                }
            }
            Statement::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    self.check_expr_limit(condition, "IF condition");
                    for nested in body {
                        self.check_statement_limits(nested, depth + 1);
                    }
                }
                for nested in else_branch {
                    self.check_statement_limits(nested, depth + 1);
                }
            }
            Statement::Case {
                selector,
                cases,
                else_branch,
            } => {
                self.check_expr_limit(selector, "CASE selector");
                for (labels, body) in cases {
                    for label in labels {
                        match label {
                            CaseLabel::Single(expr) => {
                                self.check_expr_limit(expr, "CASE label");
                            }
                            CaseLabel::Range(low, high) => {
                                self.check_expr_limit(low, "CASE range label");
                                self.check_expr_limit(high, "CASE range label");
                            }
                        }
                    }
                    for nested in body {
                        self.check_statement_limits(nested, depth + 1);
                    }
                }
                for nested in else_branch {
                    self.check_statement_limits(nested, depth + 1);
                }
            }
            Statement::For {
                from, to, by, body, ..
            } => {
                self.check_expr_limit(from, "FOR start expression");
                self.check_expr_limit(to, "FOR end expression");
                if let Some(step) = by {
                    self.check_expr_limit(step, "FOR step expression");
                }
                for nested in body {
                    self.check_statement_limits(nested, depth + 1);
                }
            }
            Statement::While { condition, body } => {
                self.check_expr_limit(condition, "WHILE condition");
                for nested in body {
                    self.check_statement_limits(nested, depth + 1);
                }
            }
            Statement::Repeat { body, until } => {
                for nested in body {
                    self.check_statement_limits(nested, depth + 1);
                }
                self.check_expr_limit(until, "REPEAT condition");
            }
            Statement::Il { operand, .. } => {
                if let Some(expr) = operand {
                    self.check_expr_limit(expr, "IL operand");
                }
            }
            Statement::Empty
            | Statement::Return
            | Statement::Exit
            | Statement::IlLabel(_)
            | Statement::Unsupported(_) => {}
        }
    }

    fn check_expr_limit(&mut self, expr: &Expr, context: &str) {
        let depth = expr_depth(expr);
        if depth > self.options.implementation.max_expression_depth {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Compliance,
                format!(
                    "{context} depth {depth} exceeds maximum {}",
                    self.options.implementation.max_expression_depth
                ),
                None,
            ));
        }
    }

    fn known_types(&self, project: &Project) -> BTreeSet<String> {
        let mut known = BTreeSet::new();
        for name in [
            "BOOL",
            "SINT",
            "INT",
            "DINT",
            "LINT",
            "USINT",
            "UINT",
            "UDINT",
            "ULINT",
            "REAL",
            "LREAL",
            "BYTE",
            "WORD",
            "DWORD",
            "LWORD",
            "STRING",
            "WSTRING",
            "TIME",
            "DATE",
            "TIME_OF_DAY",
            "TOD",
            "DATE_AND_TIME",
            "DT",
        ] {
            known.insert(name.to_string());
        }
        for data_type in project.data_types() {
            known.insert(data_type.name.canonical.clone());
        }
        for pou in project.pous() {
            if matches!(&pou.kind, PouKind::FunctionBlock)
                || is_standard_function_block(&pou.name.original)
            {
                known.insert(pou.name.canonical.clone());
            }
        }
        for symbol in standard_symbols() {
            if symbol.kind == StandardSymbolKind::FunctionBlock {
                known.insert(symbol.name.to_string());
            }
        }
        known
    }

    fn check_type_spec(&mut self, spec: &DataTypeSpec, known_types: &BTreeSet<String>) {
        match spec {
            DataTypeSpec::Named(name) if !known_types.contains(&name.canonical) => {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!("unknown type '{}'", name.original),
                    None,
                ));
            }
            DataTypeSpec::Array {
                ranges,
                element_type,
            } => {
                let mut total = 1_usize;
                for range in ranges {
                    if range.low > range.high {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!("invalid subrange {}..{}", range.low, range.high),
                            None,
                        ));
                    }
                    total = total.saturating_mul((range.high - range.low + 1).max(0) as usize);
                }
                if total > self.options.implementation.max_array_elements {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Compliance,
                        format!(
                            "array size {total} exceeds maximum {}",
                            self.options.implementation.max_array_elements
                        ),
                        None,
                    ));
                }
                self.check_type_spec(element_type, known_types);
            }
            DataTypeSpec::Struct { fields } => {
                if fields.len() > self.options.implementation.max_structure_elements {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Compliance,
                        format!(
                            "structure has {} elements, exceeding maximum {}",
                            fields.len(),
                            self.options.implementation.max_structure_elements
                        ),
                        None,
                    ));
                }
                let mut seen = BTreeSet::new();
                for field in fields {
                    self.check_identifier_profile(&field.name, "structure field name");
                    if !seen.insert(field.name.canonical.clone()) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!("duplicate structure field '{}'", field.name.original),
                            None,
                        ));
                    }
                    self.check_type_spec(&field.spec, known_types);
                }
            }
            DataTypeSpec::Subrange { range, .. } if range.low > range.high => {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!("invalid subrange {}..{}", range.low, range.high),
                    None,
                ));
            }
            DataTypeSpec::String { length, .. } => {
                if length
                    .is_some_and(|length| length > self.options.implementation.max_string_length)
                {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Compliance,
                        format!(
                            "string length exceeds maximum {}",
                            self.options.implementation.max_string_length
                        ),
                        None,
                    ));
                }
            }
            _ => {}
        }
    }

    fn check_statement(
        &mut self,
        statement: &Statement,
        variables: &BTreeMap<String, DataTypeSpec>,
        constants: &BTreeSet<String>,
        project: &Project,
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
                        self.check_statement(statement, variables, constants, project);
                    }
                }
                for statement in else_branch {
                    self.check_statement(statement, variables, constants, project);
                }
            }
            Statement::Case {
                selector,
                cases,
                else_branch,
            } => {
                self.check_expr(selector, variables, project);
                for (labels, body) in cases {
                    for label in labels {
                        match label {
                            CaseLabel::Single(expr) => self.check_expr(expr, variables, project),
                            CaseLabel::Range(low, high) => {
                                self.check_expr(low, variables, project);
                                self.check_expr(high, variables, project);
                            }
                        }
                    }
                    for statement in body {
                        self.check_statement(statement, variables, constants, project);
                    }
                }
                for statement in else_branch {
                    self.check_statement(statement, variables, constants, project);
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
                }
                for statement in body {
                    self.check_statement(statement, variables, constants, project);
                }
            }
            Statement::While { condition, body } => {
                self.check_expr(condition, variables, project);
                self.check_bool_expr(condition, variables, project, "WHILE condition");
                for statement in body {
                    self.check_statement(statement, variables, constants, project);
                }
            }
            Statement::Repeat { body, until } => {
                for statement in body {
                    self.check_statement(statement, variables, constants, project);
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
            Statement::Empty | Statement::IlLabel(_) | Statement::Exit | Statement::Return => {}
        }
    }

    fn check_sfc(
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
        }

        let mut actions = BTreeSet::new();
        for action in &sfc.actions {
            if !actions.insert(action.name.canonical.clone()) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!("duplicate SFC action '{}'", action.name.original),
                    None,
                ));
            }
            if action.qualifier.requires_duration() && action.duration.is_none() {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "SFC action '{}' qualifier {} requires a duration",
                        action.name.original,
                        action.qualifier.as_iec()
                    ),
                    None,
                ));
            }
            if action
                .duration
                .as_ref()
                .is_some_and(|literal| !matches!(literal, Literal::DurationMs(_)))
            {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "SFC action '{}' qualifier duration must be TIME",
                        action.name.original
                    ),
                    None,
                ));
            }
            for statement in &action.body {
                self.check_statement_limits(statement, 1);
                self.check_statement(statement, variables, constants, project);
            }
        }
    }

    fn check_il_call_operand(
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

    fn check_il_labels(&mut self, statements: &[Statement]) {
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

    fn check_expr(
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
                if !is_standard_function(&name.original) && user_function.is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("unknown function '{}'", name.original),
                        None,
                    ));
                }
                self.check_implicit_function_controls(name, args, variables, project);
                if let Some(function) = user_function {
                    self.check_function_call_args(function, args, variables, project);
                }
                self.check_conversion_range(name, args, variables, project);
                self.check_bcd_conversion_range(name, args, variables, project);
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
            Expr::Literal(_) => {}
        }
    }

    fn check_unary_operator(
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

    fn check_binary_operator(
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
                left_type != SimpleType::Aggregate && right_type != SimpleType::Aggregate
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

    fn check_conversion_range(
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
            .filter(|arg| !arg.name.as_ref().is_some_and(|name| is_implicit_en(name)))
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

    fn check_bcd_conversion_range(
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
            .filter(|arg| !arg.name.as_ref().is_some_and(|name| is_implicit_en(name)))
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

    fn check_assignment_type(
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

    fn check_initialization_constraints(
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
                if let Some(value) = const_i64(value, variables, project, self) {
                    if value < range.low || value > range.high {
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
                    if let Some(value) = const_i64(value, variables, project, self) {
                        let value = i128::from(value);
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
                let valid = enum_expr_name(value)
                    .is_some_and(|name| values.iter().any(|value| value.canonical == name));
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
            DataTypeSpec::String {
                length: Some(length),
                ..
            } => {
                if let Expr::Literal(Literal::String(value)) = value {
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
                let Expr::ArrayLiteral(elements) = value else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("{context} expects an array initializer"),
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
                let Expr::StructLiteral(initializers) = value else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("{context} expects a structure initializer"),
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

    fn check_bool_expr(
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

    fn check_integer_expr(
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

    fn check_integer_spec(&mut self, spec: &DataTypeSpec, project: &Project, context: &str) {
        let actual = self.type_of_spec(spec, project);
        if !matches!(actual, SimpleType::Integer | SimpleType::Unknown) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("{context} expects integer, got {}", actual.as_str()),
                None,
            ));
        }
    }

    fn check_function_call_args(
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
            if arg.name.as_ref().is_some_and(|name| is_implicit_en(name)) {
                continue;
            }
            if arg.output {
                if arg.name.as_ref().is_some_and(|name| is_implicit_eno(name)) {
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

    fn check_implicit_function_controls(
        &mut self,
        function_name: &Identifier,
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
                        format!(
                            "function '{}' EN is bound more than once",
                            function_name.original
                        ),
                        None,
                    ));
                }
                if arg.output {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "function '{}' EN must use input binding",
                            function_name.original
                        ),
                        None,
                    ));
                }
                if let Some(expr) = &arg.expr {
                    self.check_bool_expr(expr, variables, project, "function EN input");
                }
            } else if is_implicit_eno(name) {
                if !eno_seen {
                    eno_seen = true;
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "function '{}' ENO is bound more than once",
                            function_name.original
                        ),
                        None,
                    ));
                }
                if !arg.output {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "function '{}' ENO must use output binding",
                            function_name.original
                        ),
                        None,
                    ));
                }
                if let Some(variable) = &arg.variable {
                    if let Some(spec) = self.variable_type(variable, variables, project) {
                        if self.type_of_spec(&spec, project) != SimpleType::Bool {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::Semantic,
                                format!(
                                    "function '{}' ENO expects BOOL output",
                                    function_name.original
                                ),
                                None,
                            ));
                        }
                    }
                }
            }
        }
    }

    fn variable_type(
        &self,
        variable: &VariableRef,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) -> Option<DataTypeSpec> {
        if variable.direct.is_some() {
            return None;
        }
        let mut spec = variables.get(&variable.root_name()?.canonical)?.clone();
        spec = self.apply_indices_to_type(
            spec,
            variable.indices.first().map(Vec::as_slice).unwrap_or(&[]),
            project,
        )?;
        for (segment_index, segment) in variable.path.iter().enumerate().skip(1) {
            if let Some(field_spec) = standard_fb_field_type(&spec, &segment.canonical) {
                spec = field_spec;
                continue;
            }
            spec = self.resolve_named_spec(&spec, project);
            let DataTypeSpec::Struct { fields } = spec else {
                return None;
            };
            spec = fields
                .iter()
                .find(|field| field.name.canonical == segment.canonical)?
                .spec
                .clone();
            spec = self.apply_indices_to_type(
                spec,
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

    fn apply_indices_to_type(
        &self,
        spec: DataTypeSpec,
        indices: &[Expr],
        project: &Project,
    ) -> Option<DataTypeSpec> {
        if indices.is_empty() {
            return Some(spec);
        }
        let resolved = self.resolve_named_spec(&spec, project);
        let DataTypeSpec::Array { element_type, .. } = resolved else {
            return None;
        };
        Some(*element_type)
    }

    fn resolve_named_spec(&self, spec: &DataTypeSpec, project: &Project) -> DataTypeSpec {
        let DataTypeSpec::Named(name) = spec else {
            return spec.clone();
        };
        project
            .data_types()
            .find(|data_type| data_type.name.canonical == name.canonical)
            .map(|data_type| data_type.spec.clone())
            .unwrap_or_else(|| spec.clone())
    }

    fn type_of_expr(
        &self,
        expr: &Expr,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) -> SimpleType {
        match expr {
            Expr::Literal(literal) => literal_type(literal, project),
            Expr::Variable(variable) => self
                .variable_type(variable, variables, project)
                .map(|spec| self.type_of_spec(&spec, project))
                .or_else(|| {
                    variable.root_name().and_then(|root| {
                        enum_value_exists(project, &root.canonical).then_some(SimpleType::Integer)
                    })
                })
                .unwrap_or(SimpleType::Unknown),
            Expr::Unary { op, expr } => match op {
                UnaryOp::Not => match self.type_of_expr(expr, variables, project) {
                    SimpleType::Bool => SimpleType::Bool,
                    SimpleType::Unknown => SimpleType::Unknown,
                    _ => SimpleType::BitString,
                },
                UnaryOp::Neg => self
                    .type_of_expr(expr, variables, project)
                    .numeric_or_unknown(),
            },
            Expr::Binary { op, left, right } => match op {
                BinaryOp::Or | BinaryOp::Xor | BinaryOp::And => {
                    let left = self.type_of_expr(left, variables, project);
                    let right = self.type_of_expr(right, variables, project);
                    if matches!(left, SimpleType::Bool) && matches!(right, SimpleType::Bool) {
                        SimpleType::Bool
                    } else if matches!(left, SimpleType::Unknown)
                        || matches!(right, SimpleType::Unknown)
                    {
                        SimpleType::Unknown
                    } else {
                        SimpleType::BitString
                    }
                }
                BinaryOp::Equal
                | BinaryOp::NotEqual
                | BinaryOp::Less
                | BinaryOp::LessEqual
                | BinaryOp::Greater
                | BinaryOp::GreaterEqual => SimpleType::Bool,
                BinaryOp::Power => SimpleType::Real,
                BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => {
                    let left = self.type_of_expr(left, variables, project);
                    let right = self.type_of_expr(right, variables, project);
                    if matches!(op, BinaryOp::Add | BinaryOp::Sub)
                        && matches!(left, SimpleType::Time)
                        && matches!(right, SimpleType::Time)
                    {
                        SimpleType::Time
                    } else if matches!(left, SimpleType::Real) || matches!(right, SimpleType::Real)
                    {
                        SimpleType::Real
                    } else if matches!(left, SimpleType::Unknown)
                        || matches!(right, SimpleType::Unknown)
                    {
                        SimpleType::Unknown
                    } else {
                        SimpleType::Integer
                    }
                }
            },
            Expr::Call { name, args } => {
                if let Some(function) = project.find_pou(&name.original).and_then(|pou| match &pou
                    .kind
                {
                    PouKind::Function { return_type } => Some(return_type),
                    _ => None,
                }) {
                    return self.type_of_spec(function, project);
                }
                standard_function_return_type(name, args, variables, project, self)
            }
            Expr::ArrayLiteral(_) | Expr::StructLiteral(_) => SimpleType::Aggregate,
        }
    }

    fn type_of_spec(&self, spec: &DataTypeSpec, project: &Project) -> SimpleType {
        self.type_of_spec_inner(spec, project, &mut BTreeSet::new())
    }

    fn type_of_spec_inner(
        &self,
        spec: &DataTypeSpec,
        project: &Project,
        seen: &mut BTreeSet<String>,
    ) -> SimpleType {
        match spec {
            DataTypeSpec::Elementary(elementary) => elementary_type(elementary),
            DataTypeSpec::String { .. } => SimpleType::String,
            DataTypeSpec::Subrange { base, .. } => elementary_type(base),
            DataTypeSpec::Enum { .. } => SimpleType::Integer,
            DataTypeSpec::Named(name) => {
                if !seen.insert(name.canonical.clone()) {
                    return SimpleType::Unknown;
                }
                project
                    .data_types()
                    .find(|data_type| data_type.name.canonical == name.canonical)
                    .map(|data_type| self.type_of_spec_inner(&data_type.spec, project, seen))
                    .unwrap_or(SimpleType::Unknown)
            }
            DataTypeSpec::Array { .. } | DataTypeSpec::Struct { .. } => SimpleType::Aggregate,
        }
    }

    fn check_variable(
        &mut self,
        variable: &VariableRef,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        if let Some(direct) = &variable.direct {
            self.check_direct_variable_location(direct);
            return;
        }
        let Some(root) = variable.root_name() else {
            return;
        };
        if enum_value_exists(project, &root.canonical)
            && variable.path.len() == 1
            && variable.indices.iter().all(Vec::is_empty)
        {
            return;
        }
        if !variables.contains_key(&root.canonical) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("unknown variable '{}'", root.original),
                None,
            ));
            return;
        }
        for index in variable.indices.iter().flatten() {
            self.check_expr(index, variables, project);
            self.check_integer_expr(index, variables, project, "array index");
        }
        if variable.path.len() > 1 || variable.indices.iter().any(|indices| !indices.is_empty()) {
            if self.variable_type(variable, variables, project).is_none() {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!("invalid field or array access '{}'", variable),
                    None,
                ));
            }
        }
    }

    fn check_direct_variable_location(&mut self, location: &str) {
        if let Some(message) = validate_direct_variable_location(location) {
            self.diagnostics
                .push(Diagnostic::error(DiagnosticCode::Semantic, message, None));
        }
    }

    fn check_configuration(&mut self, project: &Project, configuration: &Configuration) {
        let known_types = self.known_types(project);
        self.check_identifier_profile(&configuration.name, "configuration name");
        self.check_configuration_var_blocks(
            &configuration.var_blocks,
            format!("configuration '{}'", configuration.name.original),
            &known_types,
        );
        for resource in &configuration.resources {
            self.check_identifier_profile(&resource.name, "resource name");
            self.check_configuration_var_blocks(
                &resource.var_blocks,
                format!(
                    "resource '{}' in configuration '{}'",
                    resource.name.original, configuration.name.original
                ),
                &known_types,
            );
            let mut tasks = BTreeSet::new();
            for task in &resource.tasks {
                self.check_identifier_profile(&task.name, "task name");
                if !tasks.insert(task.name.canonical.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "duplicate task '{}' in resource '{}'",
                            task.name.original, resource.name.original
                        ),
                        None,
                    ));
                }
            }

            let mut program_instances = BTreeSet::new();
            for instance in &resource.program_instances {
                self.check_identifier_profile(&instance.name, "program instance name");
                if !program_instances.insert(instance.name.canonical.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "duplicate program instance '{}' in resource '{}'",
                            instance.name.original, resource.name.original
                        ),
                        None,
                    ));
                }

                if !project
                    .find_pou(&instance.program_type.original)
                    .is_some_and(|pou| matches!(&pou.kind, PouKind::Program))
                {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "program instance '{}' references unknown PROGRAM type '{}'",
                            instance.name.original, instance.program_type.original
                        ),
                        None,
                    ));
                }

                if let Some(task) = &instance.task {
                    if !tasks.contains(&task.canonical) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "program instance '{}' references unknown task '{}'",
                                instance.name.original, task.original
                            ),
                            None,
                        ));
                    }
                }
            }
        }
    }

    fn check_configuration_var_blocks(
        &mut self,
        blocks: &[VarBlock],
        context: String,
        known_types: &BTreeSet<String>,
    ) {
        let mut names = BTreeSet::new();
        for block in blocks {
            if !matches!(
                block.kind,
                VarBlockKind::Global | VarBlockKind::Config | VarBlockKind::Access
            ) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "{} cannot contain {}",
                        context,
                        var_block_kind_label(block.kind)
                    ),
                    None,
                ));
            }
            for var in &block.vars {
                self.check_identifier_profile(&var.name, "configuration variable name");
                if !names.insert(var.name.canonical.clone()) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("duplicate variable '{}' in {context}", var.name.original),
                        None,
                    ));
                }
                if let Some(location) = &var.location {
                    self.check_direct_variable_location(location);
                }
                self.check_type_spec(&var.type_spec, known_types);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleType {
    Bool,
    Integer,
    Real,
    BitString,
    String,
    Time,
    Date,
    TimeOfDay,
    DateAndTime,
    Aggregate,
    Unknown,
}

impl SimpleType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "BOOL",
            Self::Integer => "integer",
            Self::Real => "REAL",
            Self::BitString => "bit-string",
            Self::String => "STRING",
            Self::Time => "TIME",
            Self::Date => "DATE",
            Self::TimeOfDay => "TIME_OF_DAY",
            Self::DateAndTime => "DATE_AND_TIME",
            Self::Aggregate => "aggregate",
            Self::Unknown => "unknown",
        }
    }

    fn numeric_or_unknown(self) -> Self {
        match self {
            Self::Integer | Self::Real | Self::Unknown => self,
            _ => Self::Unknown,
        }
    }
}

fn types_are_assignable(expected: SimpleType, actual: SimpleType) -> bool {
    match (expected, actual) {
        (SimpleType::Unknown, _) | (_, SimpleType::Unknown) => true,
        (left, right) if left == right => true,
        (SimpleType::Real, SimpleType::Integer) => true,
        (SimpleType::BitString, SimpleType::Integer) => true,
        (SimpleType::Integer, SimpleType::BitString) => true,
        _ => false,
    }
}

fn literal_type(literal: &Literal, project: &Project) -> SimpleType {
    match literal {
        Literal::Int(_) => SimpleType::Integer,
        Literal::Real(_) => SimpleType::Real,
        Literal::Bool(_) => SimpleType::Bool,
        Literal::String(_) => SimpleType::String,
        Literal::DurationMs(_) => SimpleType::Time,
        Literal::Date(_) => SimpleType::Date,
        Literal::TimeOfDay(_) => SimpleType::TimeOfDay,
        Literal::DateAndTime(_) => SimpleType::DateAndTime,
        Literal::Typed { type_name, .. } => ElementaryType::parse(&type_name.original)
            .map(|elementary| elementary_type(&elementary))
            .or_else(|| {
                project
                    .data_types()
                    .find(|data_type| data_type.name.canonical == type_name.canonical)
                    .map(|data_type| match &data_type.spec {
                        DataTypeSpec::Elementary(elementary) => elementary_type(elementary),
                        DataTypeSpec::Subrange { base, .. } => elementary_type(base),
                        DataTypeSpec::Enum { .. } => SimpleType::Integer,
                        DataTypeSpec::String { .. } => SimpleType::String,
                        DataTypeSpec::Array { .. } | DataTypeSpec::Struct { .. } => {
                            SimpleType::Aggregate
                        }
                        DataTypeSpec::Named(_) => SimpleType::Unknown,
                    })
            })
            .unwrap_or(SimpleType::Unknown),
    }
}

fn is_numeric_simple(simple: SimpleType) -> bool {
    matches!(simple, SimpleType::Integer | SimpleType::Real)
}

fn is_bitwise_simple(simple: SimpleType) -> bool {
    matches!(simple, SimpleType::Integer | SimpleType::BitString)
}

fn unary_op_name(op: UnaryOp) -> &'static str {
    match op {
        UnaryOp::Neg => "-",
        UnaryOp::Not => "NOT",
    }
}

fn binary_op_name(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Or => "OR",
        BinaryOp::Xor => "XOR",
        BinaryOp::And => "AND",
        BinaryOp::Equal => "=",
        BinaryOp::NotEqual => "<>",
        BinaryOp::Less => "<",
        BinaryOp::LessEqual => "<=",
        BinaryOp::Greater => ">",
        BinaryOp::GreaterEqual => ">=",
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::Mod => "MOD",
        BinaryOp::Power => "**",
    }
}

fn elementary_type(elementary: &ElementaryType) -> SimpleType {
    match elementary {
        ElementaryType::Bool => SimpleType::Bool,
        ElementaryType::Sint
        | ElementaryType::Int
        | ElementaryType::Dint
        | ElementaryType::Lint
        | ElementaryType::Usint
        | ElementaryType::Uint
        | ElementaryType::Udint
        | ElementaryType::Ulint => SimpleType::Integer,
        ElementaryType::Real | ElementaryType::Lreal => SimpleType::Real,
        ElementaryType::Byte
        | ElementaryType::Word
        | ElementaryType::Dword
        | ElementaryType::Lword => SimpleType::BitString,
        ElementaryType::String | ElementaryType::WString => SimpleType::String,
        ElementaryType::Time => SimpleType::Time,
        ElementaryType::Date => SimpleType::Date,
        ElementaryType::TimeOfDay => SimpleType::TimeOfDay,
        ElementaryType::DateAndTime => SimpleType::DateAndTime,
    }
}

fn standard_function_return_type(
    name: &Identifier,
    args: &[ParamAssignment],
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> SimpleType {
    let arg_types = args
        .iter()
        .filter(|arg| !arg.output)
        .filter(|arg| !arg.name.as_ref().is_some_and(|name| is_implicit_en(name)))
        .filter_map(|arg| arg.expr.as_ref())
        .map(|expr| checker.type_of_expr(expr, variables, project))
        .collect::<Vec<_>>();

    match name.canonical.as_str() {
        name if bcd_conversion_return_type(name).is_some() => {
            bcd_conversion_return_type(name).unwrap()
        }
        name if conversion_return_type(name).is_some() => conversion_return_type(name).unwrap(),
        "MOVE" => arg_types.first().copied().unwrap_or(SimpleType::Unknown),
        "ABS" => arg_types
            .first()
            .copied()
            .map(SimpleType::numeric_or_unknown)
            .unwrap_or(SimpleType::Unknown),
        "TRUNC" => SimpleType::Integer,
        "SQRT" | "LN" | "LOG" | "EXP" | "SIN" | "COS" | "TAN" => SimpleType::Real,
        "EXPT" => SimpleType::Real,
        "ADD" | "SUB" | "MUL" | "DIV" | "MOD" | "MIN" | "MAX" => {
            if arg_types
                .iter()
                .any(|arg_type| *arg_type == SimpleType::Real)
            {
                SimpleType::Real
            } else if arg_types
                .iter()
                .any(|arg_type| *arg_type == SimpleType::Unknown)
            {
                SimpleType::Unknown
            } else {
                SimpleType::Integer
            }
        }
        "LIMIT" => arg_types.get(1).copied().unwrap_or(SimpleType::Unknown),
        "SEL" => arg_types.get(2).copied().unwrap_or(SimpleType::Unknown),
        "MUX" => arg_types.get(1).copied().unwrap_or(SimpleType::Unknown),
        "GT" | "GE" | "EQ" | "NE" | "LE" | "LT" => SimpleType::Bool,
        "SHL" | "SHR" | "ROL" | "ROR" => SimpleType::Integer,
        "AND" | "OR" | "XOR" => {
            if arg_types
                .iter()
                .all(|arg_type| *arg_type == SimpleType::Bool)
            {
                SimpleType::Bool
            } else {
                SimpleType::BitString
            }
        }
        "NOT" => arg_types.first().copied().unwrap_or(SimpleType::Unknown),
        "LEN" | "FIND" => SimpleType::Integer,
        "LEFT" | "RIGHT" | "MID" | "CONCAT" | "INSERT" | "DELETE" | "REPLACE" => SimpleType::String,
        "ADD_TIME" | "SUB_TIME" | "MUL_TIME" | "DIV_TIME" => SimpleType::Time,
        "BOOL_TO_INT" | "REAL_TO_INT" => SimpleType::Integer,
        "INT_TO_BOOL" => SimpleType::Bool,
        "INT_TO_REAL" => SimpleType::Real,
        _ => SimpleType::Unknown,
    }
}

fn conversion_return_type(name: &str) -> Option<SimpleType> {
    let (_, target) = name.split_once("_TO_")?;
    match target {
        "BOOL" => Some(SimpleType::Bool),
        "SINT" | "INT" | "DINT" | "LINT" | "USINT" | "UINT" | "UDINT" | "ULINT" => {
            Some(SimpleType::Integer)
        }
        "BYTE" | "WORD" | "DWORD" | "LWORD" => Some(SimpleType::BitString),
        "REAL" | "LREAL" => Some(SimpleType::Real),
        "STRING" | "WSTRING" => Some(SimpleType::String),
        "TIME" => Some(SimpleType::Time),
        _ => None,
    }
}

fn bcd_conversion_return_type(name: &str) -> Option<SimpleType> {
    if name == "BCD_TO_INT" {
        return Some(SimpleType::Integer);
    }
    if name == "INT_TO_BCD" {
        return Some(SimpleType::BitString);
    }
    if name.split_once("_BCD_TO_").is_some() {
        return Some(SimpleType::Integer);
    }
    if name.split_once("_TO_BCD_").is_some() {
        return Some(SimpleType::BitString);
    }
    None
}

#[derive(Debug, Clone, Copy)]
enum BcdConversionKind {
    BcdToInt { digits: Option<u32> },
    IntToBcd { digits: Option<u32> },
}

fn bcd_conversion_kind(name: &str) -> Option<BcdConversionKind> {
    if name == "BCD_TO_INT" {
        return Some(BcdConversionKind::BcdToInt { digits: None });
    }
    if name == "INT_TO_BCD" {
        return Some(BcdConversionKind::IntToBcd { digits: None });
    }
    if let Some((source, _target)) = name.split_once("_BCD_TO_") {
        return Some(BcdConversionKind::BcdToInt {
            digits: bcd_digit_capacity(source),
        });
    }
    if let Some((_source, target)) = name.split_once("_TO_BCD_") {
        return Some(BcdConversionKind::IntToBcd {
            digits: bcd_digit_capacity(target),
        });
    }
    None
}

fn bcd_digit_capacity(name: &str) -> Option<u32> {
    match name {
        "BYTE" => Some(2),
        "WORD" => Some(4),
        "DWORD" => Some(8),
        "LWORD" => Some(16),
        _ => None,
    }
}

fn bcd_decode_i128(value: i128, digits: Option<u32>) -> Option<i128> {
    if value < 0 {
        return None;
    }
    let mut raw = value as u128;
    if let Some(digits) = digits {
        let bits = digits.saturating_mul(4);
        let mask = if bits >= 128 {
            u128::MAX
        } else {
            (1_u128 << bits) - 1
        };
        if raw & !mask != 0 {
            return None;
        }
    }
    let mut result = 0_i128;
    let mut place = 1_i128;
    while raw != 0 {
        let digit = (raw & 0x0f) as i128;
        if digit > 9 {
            return None;
        }
        result = result.checked_add(digit.checked_mul(place)?)?;
        place = place.checked_mul(10)?;
        raw >>= 4;
    }
    Some(result)
}

fn bcd_encode_i128(value: i128, digits: Option<u32>) -> Option<i128> {
    if value < 0 {
        return None;
    }
    let max_digits = digits.unwrap_or(16);
    let mut decimal = value;
    let mut raw = 0_i128;
    let mut used_digits = 0_u32;
    if decimal == 0 {
        return Some(0);
    }
    while decimal != 0 {
        if used_digits >= max_digits {
            return None;
        }
        let digit = decimal % 10;
        raw |= digit << (used_digits * 4);
        decimal /= 10;
        used_digits += 1;
    }
    Some(raw)
}

fn conversion_target_integer_range(name: &str) -> Option<(&'static str, i128, i128)> {
    let (_, target) = name.split_once("_TO_")?;
    match target {
        "SINT" => Some(("SINT", -128, 127)),
        "USINT" | "BYTE" => Some((if target == "BYTE" { "BYTE" } else { "USINT" }, 0, 255)),
        "INT" => Some(("INT", -32_768, 32_767)),
        "UINT" | "WORD" => Some((if target == "WORD" { "WORD" } else { "UINT" }, 0, 65_535)),
        "DINT" => Some(("DINT", -2_147_483_648, 2_147_483_647)),
        "UDINT" | "DWORD" => Some((
            if target == "DWORD" { "DWORD" } else { "UDINT" },
            0,
            4_294_967_295,
        )),
        "LINT" => Some(("LINT", i64::MIN as i128, i64::MAX as i128)),
        "ULINT" | "LWORD" => Some((
            if target == "LWORD" { "LWORD" } else { "ULINT" },
            0,
            i64::MAX as i128,
        )),
        _ => None,
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

fn enum_expr_name(expr: &Expr) -> Option<String> {
    let Expr::Variable(variable) = expr else {
        return None;
    };
    if variable.direct.is_some()
        || variable.path.len() != 1
        || variable.indices.iter().any(|indices| !indices.is_empty())
    {
        return None;
    }
    variable.root_name().map(|name| name.canonical.clone())
}

fn il_label_operand(expr: &Expr) -> Option<&Identifier> {
    let Expr::Variable(variable) = expr else {
        return None;
    };
    if variable.direct.is_some()
        || variable.path.len() != 1
        || variable.indices.iter().any(|indices| !indices.is_empty())
    {
        return None;
    }
    variable.root_name()
}

fn enum_value_exists(project: &Project, canonical_name: &str) -> bool {
    project.data_types().any(|data_type| {
        if let DataTypeSpec::Enum { values } = &data_type.spec {
            values.iter().any(|value| value.canonical == canonical_name)
        } else {
            false
        }
    })
}

fn standard_fb_field_type(spec: &DataTypeSpec, field: &str) -> Option<DataTypeSpec> {
    let DataTypeSpec::Named(type_name) = spec else {
        return None;
    };
    let spec = match type_name.canonical.as_str() {
        "SR" | "RS" if field == "Q1" => DataTypeSpec::Elementary(ElementaryType::Bool),
        "R_TRIG" | "F_TRIG" if matches!(field, "Q" | "M") => {
            DataTypeSpec::Elementary(ElementaryType::Bool)
        }
        "CTU" | "CTD" => match field {
            "Q" | "_CU" | "_CD" => DataTypeSpec::Elementary(ElementaryType::Bool),
            "CV" => DataTypeSpec::Elementary(ElementaryType::Int),
            _ => return None,
        },
        "CTUD" => match field {
            "QU" | "QD" | "_CU" | "_CD" => DataTypeSpec::Elementary(ElementaryType::Bool),
            "CV" => DataTypeSpec::Elementary(ElementaryType::Int),
            _ => return None,
        },
        "TON" | "TOF" | "TP" => match field {
            "Q" | "_IN" | "_RUN" => DataTypeSpec::Elementary(ElementaryType::Bool),
            "ET" => DataTypeSpec::Elementary(ElementaryType::Time),
            _ => return None,
        },
        _ => return None,
    };
    Some(spec)
}

fn expr_depth(expr: &Expr) -> usize {
    match expr {
        Expr::Literal(_) | Expr::Variable(_) => 1,
        Expr::Unary { expr, .. } => 1 + expr_depth(expr),
        Expr::Binary { left, right, .. } => 1 + expr_depth(left).max(expr_depth(right)),
        Expr::Call { args, .. } => {
            1 + args
                .iter()
                .filter_map(|arg| arg.expr.as_ref())
                .map(expr_depth)
                .max()
                .unwrap_or(0)
        }
        Expr::ArrayLiteral(elements) => 1 + elements.iter().map(expr_depth).max().unwrap_or(0),
        Expr::StructLiteral(fields) => {
            1 + fields
                .iter()
                .filter_map(|field| field.expr.as_ref())
                .map(expr_depth)
                .max()
                .unwrap_or(0)
        }
    }
}

fn const_i64(
    expr: &Expr,
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> Option<i64> {
    match expr {
        Expr::Literal(Literal::Int(value)) => Some(*value),
        Expr::Literal(Literal::Bool(value)) => Some(if *value { 1 } else { 0 }),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => const_i64(expr, variables, project, checker).and_then(i64::checked_neg),
        Expr::Binary { op, left, right } => {
            let left = const_i64(left, variables, project, checker)?;
            let right = const_i64(right, variables, project, checker)?;
            match op {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Sub => left.checked_sub(right),
                BinaryOp::Mul => left.checked_mul(right),
                BinaryOp::Div if right != 0 => left.checked_div(right),
                BinaryOp::Mod if right != 0 => left.checked_rem(right),
                _ => None,
            }
        }
        Expr::Call { name, args } => {
            let values = args
                .iter()
                .filter_map(|arg| arg.expr.as_ref())
                .map(|expr| {
                    const_i64(expr, variables, project, checker).map(|value| Value::Int(value))
                })
                .collect::<Option<Vec<_>>>()?;
            iec_stdlib::eval_standard_function(&name.original, &values)
                .and_then(|value| value.as_i64())
        }
        _ => {
            let _ = checker.type_of_expr(expr, variables, project);
            None
        }
    }
}

fn const_conversion_i128(
    expr: &Expr,
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Int(value)) => Some(*value as i128),
        Expr::Literal(Literal::Bool(value)) => Some(if *value { 1 } else { 0 }),
        Expr::Literal(Literal::Real(value)) if value.is_finite() => Some(*value as i128),
        Expr::Literal(Literal::String(value)) => value.trim().parse::<i128>().ok(),
        Expr::Literal(Literal::DurationMs(value)) => Some(*value),
        Expr::Literal(Literal::Typed { value, .. }) => value.trim().parse::<i128>().ok(),
        _ => const_i64(expr, variables, project, checker).map(i128::from),
    }
}

fn retain_kind_label(kind: RetainKind) -> &'static str {
    match kind {
        RetainKind::Retain => "RETAIN",
        RetainKind::NonRetain => "NON_RETAIN",
    }
}

fn var_block_kind_label(kind: VarBlockKind) -> &'static str {
    match kind {
        VarBlockKind::Local => "VAR",
        VarBlockKind::Input => "VAR_INPUT",
        VarBlockKind::Output => "VAR_OUTPUT",
        VarBlockKind::InOut => "VAR_IN_OUT",
        VarBlockKind::External => "VAR_EXTERNAL",
        VarBlockKind::Global => "VAR_GLOBAL",
        VarBlockKind::Temp => "VAR_TEMP",
        VarBlockKind::Access => "VAR_ACCESS",
        VarBlockKind::Config => "VAR_CONFIG",
    }
}

fn validate_direct_variable_location(location: &str) -> Option<String> {
    if !location.starts_with('%') {
        return Some(format!(
            "direct variable location '{location}' must start with '%'"
        ));
    }

    let mut chars = location[1..].chars().peekable();
    let Some(area) = chars.next() else {
        return Some("direct variable location '%' is missing an area".to_string());
    };
    if !matches!(area.to_ascii_uppercase(), 'I' | 'Q' | 'M') {
        return Some(format!(
            "direct variable location '{location}' has invalid area '{area}'"
        ));
    }

    if chars
        .peek()
        .is_some_and(|ch| matches!(ch.to_ascii_uppercase(), 'X' | 'B' | 'W' | 'D' | 'L'))
    {
        chars.next();
    }

    let address = chars.collect::<String>();
    if address.is_empty() {
        return Some(format!(
            "direct variable location '{location}' is missing an address"
        ));
    }

    if address.starts_with('.') || address.ends_with('.') || address.contains("..") {
        return Some(format!(
            "direct variable location '{location}' has malformed address '{address}'"
        ));
    }

    if !address.chars().all(|ch| ch.is_ascii_digit() || ch == '.') {
        return Some(format!(
            "direct variable location '{location}' has invalid address '{address}'"
        ));
    }

    None
}

fn array_element_count(ranges: &[Subrange]) -> usize {
    ranges.iter().fold(1_usize, |total, range| {
        total.saturating_mul((range.high - range.low + 1).max(0) as usize)
    })
}

fn is_implicit_en(name: &Identifier) -> bool {
    name.canonical == "EN"
}

fn is_implicit_eno(name: &Identifier) -> bool {
    name.canonical == "ENO"
}

fn statements_definitely_assign(statements: &[Statement], canonical_name: &str) -> bool {
    for statement in statements {
        if statement_definitely_assigns(statement, canonical_name) {
            return true;
        }
        if matches!(statement, Statement::Return | Statement::Exit) {
            return false;
        }
    }
    false
}

fn statement_definitely_assigns(statement: &Statement, canonical_name: &str) -> bool {
    match statement {
        Statement::Assignment { target, .. } => target
            .root_name()
            .is_some_and(|name| name.canonical == canonical_name),
        Statement::If {
            branches,
            else_branch,
        } => {
            !else_branch.is_empty()
                && branches
                    .iter()
                    .all(|(_, body)| statements_definitely_assign(body, canonical_name))
                && statements_definitely_assign(else_branch, canonical_name)
        }
        Statement::Case {
            cases, else_branch, ..
        } => {
            !else_branch.is_empty()
                && !cases.is_empty()
                && cases
                    .iter()
                    .all(|(_, body)| statements_definitely_assign(body, canonical_name))
                && statements_definitely_assign(else_branch, canonical_name)
        }
        Statement::Repeat { body, .. } => statements_definitely_assign(body, canonical_name),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use iec_diagnostics::diagnostics_to_json;
    use iec_profile::ImplementationParameters;
    use iec_syntax::parse_project;

    use super::*;

    #[test]
    fn flags_unknown_variable() {
        let source = r#"
            PROGRAM Demo
            VAR A : INT; END_VAR
            B := A + 1;
            END_PROGRAM
        "#;
        let output = parse_project("test.st", source);
        assert!(output.diagnostics.is_empty());
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown variable 'B'")));
    }

    #[test]
    fn accepts_derived_type_reference_and_standard_function() {
        let source = r#"
            TYPE
                MyInt : INT;
            END_TYPE

            PROGRAM Demo
            VAR
                A : MyInt := 1;
                B : INT := 0;
            END_VAR
            B := ABS(A);
            END_PROGRAM
        "#;
        let output = parse_project("test.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    }

    #[test]
    fn flags_duplicate_variable() {
        let source = r#"
            PROGRAM Demo
            VAR
                A : INT;
                A : BOOL;
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("test.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate variable 'A'")));
    }

    #[test]
    fn checks_user_function_call_parameters() {
        let source = r#"
            FUNCTION Add : INT
            VAR_INPUT
                A : INT;
                B : INT;
            END_VAR
            Add := A + B;
            END_FUNCTION

            PROGRAM Demo
            VAR X : INT; END_VAR
            X := Add(A := 1, C := 2);
            END_PROGRAM
        "#;
        let output = parse_project("functions.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("no input parameter 'C'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("missing input parameter 'B'")));
    }

    #[test]
    fn checks_function_en_eno_and_return_paths() {
        let source = r#"
            FUNCTION Maybe : INT
            VAR_INPUT
                A : INT;
            END_VAR
            IF A > 0 THEN
                Maybe := A;
            END_IF;
            END_FUNCTION

            PROGRAM Demo
            VAR
                X : INT := 0;
                Ok : BOOL := FALSE;
                BadEno : INT := 0;
            END_VAR
            X := Maybe(EN := TRUE, A := 1, ENO => Ok);
            X := Maybe(EN := 1, A := 1);
            X := Maybe(A := 1, ENO := TRUE);
            X := Maybe(A := 1, ENO => BadEno);
            END_PROGRAM
        "#;
        let output = parse_project("function_controls.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function 'Maybe' does not assign to its return variable on all paths")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function EN input expects BOOL")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function 'Maybe' ENO must use output binding")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function 'Maybe' ENO expects BOOL output")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("has no input parameter 'EN'")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("has no input parameter 'ENO'")));
    }

    #[test]
    fn recognizes_communication_function_blocks_with_diagnostics() {
        let source = r#"
            PROGRAM Demo
            VAR
                Sender : USEND;
            END_VAR
            Sender(REQ := TRUE);
            END_PROGRAM
        "#;
        let output = parse_project("communication_fb.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown type 'USEND'")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("unknown function block instance")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("communication function block 'USEND' is recognized but not simulated")));
    }

    #[test]
    fn flags_elementary_type_mismatches() {
        let source = r#"
            FUNCTION Scale : INT
            VAR_INPUT
                Input : INT;
                Enabled : BOOL;
            END_VAR
            Scale := Input;
            END_FUNCTION

            PROGRAM Demo
            VAR
                X : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            Flag := 1;
            IF X THEN
                X := 1;
            END_IF;
            X := Scale(Input := TRUE, Enabled := 1);
            END_PROGRAM
        "#;
        let output = parse_project("types.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("assignment to 'Flag' expects BOOL, got integer")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("IF condition expects BOOL")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function 'Scale' parameter 'Input' expects integer, got BOOL")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function 'Scale' parameter 'Enabled' expects BOOL, got integer")));
    }

    #[test]
    fn flags_invalid_st_operator_operands() {
        let source = r#"
            PROGRAM Demo
            VAR
                A : INT := 0;
                R : REAL := 0.0;
                Flag : BOOL := FALSE;
                Text : STRING[8] := 'x';
            END_VAR
            A := TRUE + 1;
            Flag := Text AND TRUE;
            A := Text MOD 2;
            R := 2 ** TRUE;
            END_PROGRAM
        "#;
        let output = parse_project("operator_types.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("operator + cannot be applied to BOOL and integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("operator AND cannot be applied to STRING and BOOL")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("operator MOD cannot be applied to STRING and integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("operator ** cannot be applied to integer and BOOL")));
    }

    #[test]
    fn validates_derived_type_initializers() {
        let source = r#"
            TYPE
                Small : INT(0..10);
                Mode : (Idle, Run, Fault);
                ShortText : STRING[3];
            END_TYPE

            PROGRAM Demo
            VAR
                GoodSmall : Small := 5;
                BadSmall : Small := 11;
                GoodMode : Mode := Run;
                BadMode : Mode := 1;
                GoodText : ShortText := 'abc';
                BadText : ShortText := 'abcd';
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("derived_init.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'BadSmall' value 11 is outside subrange 0..10")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'BadMode' expects one of: Idle, Run, Fault")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'BadText' exceeds string length 3")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown variable 'Run'")));
    }

    #[test]
    fn validates_array_and_structure_initializers() {
        let source = r#"
            TYPE
                Small : INT(0..10);
                Pair : STRUCT
                    Low : Small;
                    Flag : BOOL;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                GoodArray : ARRAY [1..3] OF Small := [1, 2, 3];
                BadArrayLength : ARRAY [1..3] OF Small := [1, 2];
                BadArrayElement : ARRAY [1..2] OF Small := [1, 11];
                GoodPair : Pair := (Low := 5, Flag := TRUE);
                UnknownField : Pair := (Low := 5, Missing := TRUE);
                DuplicateField : Pair := (Low := 5, Low := 6, Flag := TRUE);
                BadFieldType : Pair := (Low := TRUE, Flag := 1);
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("aggregates.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'BadArrayLength' expects 3 array element(s), got 2"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'BadArrayElement' element 2 value 11 is outside subrange 0..10")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'UnknownField' has unknown structure field 'Missing'"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'DuplicateField' initializes field 'Low' more than once")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'BadFieldType' field 'Low' expects integer, got BOOL"
            )));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'BadFieldType' field 'Flag' expects BOOL, got integer"
            )));
    }

    #[test]
    fn rejects_writes_to_constant_variables() {
        let source = r#"
            PROGRAM Demo
            VAR CONSTANT
                Limit : INT := 5;
            END_VAR
            VAR
                Count : INT := 0;
            END_VAR

            Count := Limit;
            Limit := 6;
            END_PROGRAM
        "#;
        let output = parse_project("constant.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("cannot assign to CONSTANT variable 'Limit'")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("assignment to 'Count'")));
    }

    #[test]
    fn validates_retain_qualifiers() {
        let source = r#"
            FUNCTION BadFunction : INT
            VAR RETAIN
                Saved : INT := 0;
            END_VAR
            BadFunction := Saved;
            END_FUNCTION

            PROGRAM Demo
            VAR RETAIN
                Kept : INT := 1;
            END_VAR
            VAR NON_RETAIN
                Reset : INT := 1;
            END_VAR
            VAR_TEMP RETAIN
                TempSaved : INT;
            END_VAR
            VAR CONSTANT RETAIN
                BadConstant : INT := 1;
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("retain.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("FUNCTION 'BadFunction' cannot declare RETAIN variables")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("VAR_TEMP cannot be declared RETAIN")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("VAR CONSTANT cannot also be declared RETAIN")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Kept")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Reset")));
    }

    #[test]
    fn enforces_expression_and_statement_depth_limits() {
        let source = r#"
            PROGRAM Demo
            VAR
                A : INT := 0;
            END_VAR

            A := 1 + (2 * (3 + 4));
            IF TRUE THEN
                IF TRUE THEN
                    A := 1;
                END_IF;
            END_IF;
            END_PROGRAM
        "#;
        let output = parse_project("depth_limits.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(
            &output.project,
            &CheckOptions {
                implementation: ImplementationParameters {
                    max_expression_depth: 2,
                    max_statement_depth: 1,
                    ..ImplementationParameters::default()
                },
                ..CheckOptions::default()
            },
        );
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("assignment expression depth")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("statement nesting depth 2")));
    }

    #[test]
    fn validates_direct_variable_locations() {
        let good = r#"
            PROGRAM GoodIo
            VAR
                Sensor AT %IX0.0 : BOOL;
                OutputWord AT %QW2 : INT;
                MemoryDint AT %MD10 : DINT;
            END_VAR
            Sensor := %IX0.1;
            END_PROGRAM
        "#;
        let output = parse_project("good_io.st", good);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);

        let bad = r#"
            PROGRAM BadIo
            VAR
                BadArea AT %ZX0.0 : BOOL;
                MissingAddress AT %IX : BOOL;
                BadAddress AT %QW1-A : INT;
                NotDirect AT Symbolic : INT;
            END_VAR
            %Q.1 := TRUE;
            END_PROGRAM
        "#;
        let output = parse_project("bad_io.st", bad);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("invalid area 'Z'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("'%IX' is missing an address")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("invalid address '1-A'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("must start with '%'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("malformed address '.1'")));
    }

    #[test]
    fn annex_e_style_negative_cases_emit_stable_diagnostics() {
        let cases = [
            (
                "duplicate-variable",
                r#"
                PROGRAM BadDuplicate
                VAR
                    A : INT;
                    A : BOOL;
                END_VAR
                END_PROGRAM
                "#,
                "duplicate variable 'A'",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "unknown-variable",
                r#"
                PROGRAM BadUnknown
                VAR A : INT; END_VAR
                B := A + 1;
                END_PROGRAM
                "#,
                "unknown variable 'B'",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "type-mismatch",
                r#"
                PROGRAM BadTypes
                VAR Flag : BOOL; END_VAR
                Flag := 1;
                END_PROGRAM
                "#,
                "assignment to 'Flag' expects BOOL, got integer",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "bad-direct-variable",
                r#"
                PROGRAM BadDirectVariable
                VAR Broken AT %ZX0.0 : BOOL; END_VAR
                END_PROGRAM
                "#,
                "invalid area 'Z'",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "strict-identifier",
                r#"
                PROGRAM BadIdentifier
                VAR Bad__Name : INT; END_VAR
                END_PROGRAM
                "#,
                "violates 2003-strict identifier underscore rules",
                "\"stableCode\":\"RBCPP-COMPLIANCE\"",
            ),
            (
                "bad-configuration-reference",
                r#"
                CONFIGURATION Plant
                RESOURCE Cpu ON PLC
                    PROGRAM Main WITH MissingTask : MissingProgram;
                END_RESOURCE
                END_CONFIGURATION
                "#,
                "unknown PROGRAM type 'MissingProgram'",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "bad-sfc-transition",
                r#"
                PROGRAM BadSequence
                VAR Ready : INT := 1; END_VAR
                INITIAL_STEP Start;
                STEP Start;
                TRANSITION Go := Ready;
                END_PROGRAM
                "#,
                "SFC transition condition expects BOOL, got integer",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
        ];

        for (name, source, expected_message, expected_stable_code) in cases {
            let output = parse_project(format!("annex_e_{name}.st"), source);
            assert!(
                output.diagnostics.is_empty(),
                "{name}: parse diagnostics: {:?}",
                output.diagnostics
            );
            let diagnostics = check_project(&output.project, &CheckOptions::default());
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains(expected_message)),
                "{name}: expected '{expected_message}', got {diagnostics:?}"
            );
            let json = diagnostics_to_json(&diagnostics);
            assert!(
                json.contains("\"stableCode\""),
                "{name}: expected stableCode in {json}"
            );
            assert!(
                json.contains(expected_stable_code),
                "{name}: expected {expected_stable_code} in {json}"
            );
        }
    }

    #[test]
    fn enforces_strict_profile_identifier_underscore_rules() {
        let source = r#"
            PROGRAM Demo
            VAR
                Bad__Name : INT := 0;
                BadTrailing_ : INT := 0;
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("profile_identifiers.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let strict = check_project(&output.project, &CheckOptions::default());
        assert!(strict.iter().any(|diagnostic| diagnostic
            .message
            .contains("Bad__Name' violates 2003-strict identifier underscore rules")));
        assert!(strict.iter().any(|diagnostic| diagnostic
            .message
            .contains("BadTrailing_' violates 2003-strict identifier underscore rules")));

        let plus = check_project(
            &output.project,
            &CheckOptions {
                profile: EditionProfile::Iec61131_3_2003PlusExtensions,
                ..CheckOptions::default()
            },
        );
        assert!(!plus
            .iter()
            .any(|diagnostic| diagnostic.message.contains("underscore rules")));
    }

    #[test]
    fn later_edition_profiles_are_non_claimable() {
        let source = "PROGRAM Demo END_PROGRAM";
        let output = parse_project("placeholder_profile.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(
            &output.project,
            &CheckOptions {
                profile: EditionProfile::Iec61131_3_2025Placeholder,
                ..CheckOptions::default()
            },
        );
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("profile '2025-placeholder' is a placeholder")));
    }

    #[test]
    fn handles_overflowing_constant_expressions_without_panic() {
        let source = r#"
            TYPE Small : INT(0..10); END_TYPE

            PROGRAM Demo
            VAR
                Value : Small := 9223372036854775807 + 1;
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("constant_overflow.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let _ = check_project(&output.project, &CheckOptions::default());
    }

    #[test]
    fn flags_constant_conversion_target_range_errors() {
        let source = r#"
            PROGRAM Demo
            VAR
                Bad : INT := 0;
                BadByte : BYTE := 300;
                BadSint : SINT := -129;
            END_VAR
            Bad := INT_TO_USINT(300);
            Bad := WORD_BCD_TO_UINT(16#1A);
            Bad := INT_TO_BCD_BYTE(123);
            END_PROGRAM
        "#;
        let output = parse_project("conversion_range.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("conversion 'INT_TO_USINT' value 300 is outside target range 0..255")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("conversion 'WORD_BCD_TO_UINT' value 26 is not valid BCD")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("conversion 'INT_TO_BCD_BYTE' value 123 cannot be represented as BCD")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'BadByte' value 300 is outside BYTE range 0..255"
            )));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'BadSint' value -129 is outside SINT range -128..127"
            )));
    }

    #[test]
    fn validates_textual_sfc_elements() {
        let valid = r#"
            PROGRAM Sequence
            VAR
                Ready : BOOL := TRUE;
                Done : BOOL := FALSE;
            END_VAR
            INITIAL_STEP Start;
            STEP Run;
            TRANSITION Go := Ready;
            ACTION MarkDone:
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("valid_sfc.st", valid);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);

        let invalid = r#"
            PROGRAM BadSequence
            VAR
                Ready : INT := 1;
                Done : BOOL := FALSE;
            END_VAR
            INITIAL_STEP Start;
            STEP Start;
            TRANSITION Go := Ready;
            ACTION MarkDone:
                Done := TRUE;
            END_ACTION;
            ACTION MarkDone:
                Done := FALSE;
            END_ACTION;
            ACTION Delay(D):
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("invalid_sfc.st", invalid);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate SFC step 'Start'")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SFC transition condition expects BOOL, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("duplicate SFC action 'MarkDone'")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SFC action 'Delay' qualifier D requires a duration")));
    }

    #[test]
    fn resolves_global_variables_across_pous() {
        let source = r#"
            PROGRAM Globals
            VAR_GLOBAL
                Shared : INT := 1;
            END_VAR
            END_PROGRAM

            PROGRAM Main
            VAR
                Local : INT := 0;
            END_VAR
            Local := Shared + 1;
            Local := ConfigShared + ResourceShared;
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                ConfigShared : INT := 2;
            END_VAR
            RESOURCE Cpu ON PLC
                VAR_GLOBAL
                    ResourceShared : INT := 3;
                END_VAR
                VAR_CONFIG
                    Tunable AT %MW10 : INT := 4;
                END_VAR
                VAR_ACCESS
                    AccessPoint AT %MX0.0 : BOOL;
                END_VAR
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("globals.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown variable 'Shared'")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("unknown variable 'ConfigShared'")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("unknown variable 'ResourceShared'")));
    }

    #[test]
    fn checks_instruction_list_labels() {
        let source = r#"
            PROGRAM BadIl
            VAR A : INT := 0; END_VAR
            JMP Missing;
            Start:
            Start:
            LD A;
            END_PROGRAM
        "#;
        let output = parse_project("bad_il.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown IL label 'Missing'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate IL label 'Start'")));
    }

    #[test]
    fn checks_configuration_program_and_task_references() {
        let source = r#"
            PROGRAM Demo
            END_PROGRAM

            CONFIGURATION Plant
            RESOURCE Cpu ON PLC
                TASK Fast(INTERVAL := T#10ms, PRIORITY := 1);
                PROGRAM Main WITH Fast : Demo;
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("config.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    }

    #[test]
    fn flags_bad_configuration_references() {
        let source = r#"
            CONFIGURATION Plant
            RESOURCE Cpu ON PLC
                PROGRAM Main WITH MissingTask : MissingProgram;
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("config.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("unknown PROGRAM type 'MissingProgram'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown task 'MissingTask'")));
    }
}
