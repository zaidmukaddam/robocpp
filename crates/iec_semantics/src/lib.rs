// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticCode};
use iec_ir::*;
use iec_profile::{EditionProfile, ImplementationParameters};
use iec_stdlib::{
    is_communication_function_block, is_standard_function, is_standard_function_block,
    is_standard_void_function, standard_function_input_index, standard_symbols, StandardSymbolKind,
};

#[derive(Debug, Clone, Default)]
pub struct CheckOptions {
    pub profile: EditionProfile,
    pub implementation: ImplementationParameters,
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
        self.check_function_recursion(project);
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

    fn check_function_recursion(&mut self, project: &Project) {
        let functions = project
            .pous()
            .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))
            .map(|pou| (pou.name.canonical.clone(), pou.name.original.clone()))
            .collect::<BTreeMap<_, _>>();
        if functions.is_empty() {
            return;
        }

        let mut graph = BTreeMap::<String, BTreeSet<String>>::new();
        for pou in project
            .pous()
            .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))
        {
            let mut calls = BTreeSet::new();
            collect_function_calls_in_statements(&pou.body.statements, project, &mut calls);
            graph.insert(pou.name.canonical.clone(), calls);
        }

        let mut reported = BTreeSet::new();
        for function in functions.keys() {
            let mut path = Vec::new();
            if function_reaches_itself(function, function, &graph, &mut path, &mut BTreeSet::new())
                && reported.insert(function.clone())
            {
                let name = functions
                    .get(function)
                    .cloned()
                    .unwrap_or_else(|| function.clone());
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!("recursive function call cycle involving '{name}' is not supported"),
                    None,
                ));
            }
        }
    }

    fn check_type_declarations(&mut self, project: &Project) {
        let known_types = self.known_types(project);
        let mut enum_value_owners = BTreeMap::<String, Vec<String>>::new();
        for data_type in project.data_types() {
            self.check_identifier_profile(&data_type.name, "type name");
            self.check_type_spec(&data_type.spec, &known_types);
            if let DataTypeSpec::Enum { values } = &data_type.spec {
                self.check_enum_declaration(data_type, values, &mut enum_value_owners);
            }
        }

        for (value, owners) in enum_value_owners {
            if owners.len() > 1 {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "enumerated value '{}' is declared by multiple enum types: {}; unqualified uses are ambiguous",
                        value,
                        owners.join(", ")
                    ),
                    None,
                ));
            }
        }
    }

    fn check_enum_declaration(
        &mut self,
        data_type: &DataTypeDeclaration,
        values: &[Identifier],
        owners: &mut BTreeMap<String, Vec<String>>,
    ) {
        let mut seen = BTreeSet::new();
        for value in values {
            self.check_identifier_profile(value, "enumerated value name");
            if !seen.insert(value.canonical.clone()) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "duplicate enumerated value '{}' in enum type '{}'",
                        value.original, data_type.name.original
                    ),
                    None,
                ));
                continue;
            }
            owners
                .entry(value.canonical.clone())
                .or_default()
                .push(data_type.name.original.clone());
        }
    }

    fn check_pou(&mut self, project: &Project, pou: &Pou) {
        let known_types = self.known_types(project);
        let mut variables = self.project_global_variables(project, &pou.name.canonical);
        let global_constants = self.project_global_constant_variables(project, &pou.name.canonical);
        let variable_kinds = pou
            .var_blocks
            .iter()
            .flat_map(|block| {
                block
                    .vars
                    .iter()
                    .map(move |var| (var.name.canonical.clone(), block.kind))
            })
            .collect::<BTreeMap<_, _>>();
        let mut access_decls = Vec::new();
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
                    self.check_declared_direct_variable_location(location);
                }
                if let Some(edge) = var.edge {
                    if !matches!(&pou.kind, PouKind::FunctionBlock)
                        || block.kind != VarBlockKind::Input
                    {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "{} edge qualifier on variable '{}' is only valid on FUNCTION_BLOCK VAR_INPUT declarations",
                                edge_qualifier_label(edge),
                                var.name.original
                            ),
                            None,
                        ));
                    }
                    if self.type_of_spec(&var.type_spec, project) != SimpleType::Bool {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "{} edge qualifier on variable '{}' requires BOOL, got {}",
                                edge_qualifier_label(edge),
                                var.name.original,
                                self.type_of_spec(&var.type_spec, project).as_str()
                            ),
                            None,
                        ));
                    }
                }
                if var.access.is_some() {
                    access_decls.push(var);
                }

                if block.kind == VarBlockKind::External {
                    if let Some(global_spec) = variables.get(&var.name.canonical).cloned() {
                        if !self.data_specs_assignable(&global_spec, &var.type_spec, project) {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::Semantic,
                                format!(
                                    "VAR_EXTERNAL variable '{}' type does not match its VAR_GLOBAL declaration",
                                    var.name.original
                                ),
                                None,
                            ));
                        }
                        if let Some(global_constant) = global_constants.get(&var.name.canonical) {
                            if *global_constant && !block.constant {
                                self.diagnostics.push(Diagnostic::error(
                                    DiagnosticCode::Semantic,
                                    format!(
                                        "VAR_EXTERNAL variable '{}' must be declared CONSTANT because its VAR_GLOBAL declaration is CONSTANT",
                                        var.name.original
                                    ),
                                    None,
                                ));
                            } else if !*global_constant && block.constant {
                                self.diagnostics.push(Diagnostic::error(
                                    DiagnosticCode::Semantic,
                                    format!(
                                        "VAR_EXTERNAL variable '{}' cannot be declared CONSTANT because its VAR_GLOBAL declaration is not CONSTANT",
                                        var.name.original
                                    ),
                                    None,
                                ));
                            }
                        }
                    } else {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "VAR_EXTERNAL variable '{}' has no matching VAR_GLOBAL declaration",
                                var.name.original
                            ),
                            None,
                        ));
                        variables.insert(var.name.canonical.clone(), var.type_spec.clone());
                    }
                } else if block.kind != VarBlockKind::Access
                    && variables
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
                                "communication function block '{}' requires a target runtime hook",
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
        for var in access_decls {
            self.check_access_declaration(var, &variables, &variable_kinds, project);
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

    fn project_global_constant_variables(
        &self,
        project: &Project,
        current_pou: &str,
    ) -> BTreeMap<String, bool> {
        let mut constants = BTreeMap::new();
        for pou in project.pous() {
            if pou.name.canonical == current_pou {
                continue;
            }
            for block in &pou.var_blocks {
                if block.kind != VarBlockKind::Global {
                    continue;
                }
                for var in &block.vars {
                    constants
                        .entry(var.name.canonical.clone())
                        .or_insert(block.constant);
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
                    constants
                        .entry(var.name.canonical.clone())
                        .or_insert(block.constant);
                }
            }
        }
        constants
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
            DataTypeSpec::Subrange { base, range } => {
                if range.low > range.high {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!("invalid subrange {}..{}", range.low, range.high),
                        None,
                    ));
                }
                if !base.is_integer() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "subrange base type '{}' must be an integer type",
                            base.as_iec()
                        ),
                        None,
                    ));
                } else if let Some((type_name, low, high)) = elementary_integer_range(base) {
                    if i128::from(range.low) < low || i128::from(range.high) > high {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "subrange {}..{} is outside {type_name} range {low}..{high}",
                                range.low, range.high
                            ),
                            None,
                        ));
                    }
                }
            }
            DataTypeSpec::String { length, .. } => {
                if length.is_some_and(|length| length == 0) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        "string length must be at least 1",
                        None,
                    ));
                }
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
        self.check_statement_in_context(statement, variables, constants, project, false);
    }

    fn check_statement_in_context(
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

    fn check_sfc_action_control(
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

    fn check_il_store_operand(
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

    fn check_case_label_overlaps(&mut self, ranges: &[(i128, i128)]) {
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

    fn enum_type_name_of_expr(
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

    fn check_literal(&mut self, literal: &Literal, project: &Project) {
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

    fn check_standard_function_call_args(
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

    fn check_standard_function_outputs(&mut self, name: &Identifier, args: &[ParamAssignment]) {
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

    fn standard_function_input_exprs<'b>(
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

    fn check_standard_void_function_call_args(
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

    fn check_split_output_type(
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

    fn check_standard_min_args(
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

    fn check_standard_exact_args(
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

    fn check_standard_family(
        &mut self,
        name: &Identifier,
        input_types: &[SimpleType],
        family: GenericFamily,
    ) {
        for (index, actual) in input_types.iter().copied().enumerate() {
            self.check_standard_arg_type(name, index, actual, family);
        }
    }

    fn check_standard_arg_family(
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

    fn check_standard_non_negative_arg(
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

    fn check_standard_mux_selector(
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

    fn check_standard_string_bounds(
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

    fn check_string_position(&mut self, name: &Identifier, position: i64, input_len: i64) {
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

    fn check_string_range(
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

    fn check_standard_arg_type(
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

    fn check_standard_compatible_data_args(
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

    fn check_typed_literal_spec(
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

    fn check_typed_literal_elementary(
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

    fn check_constant_conversion_result(
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

    fn expr_data_spec(
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

    fn array_specs_assignable(
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

    fn struct_specs_assignable(
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

    fn data_specs_assignable(
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

    fn check_enum_initialization_constraints(
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

    fn enum_expr_matches_expected(
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

    fn check_function_block_call_args(
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

    fn check_standard_function_block_call_args(
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

    fn check_implicit_call_controls(
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
            } else if let Some(function_block) = function_block_pou(project, &spec) {
                spec = function_block
                    .variable_declarations()
                    .find(|field| field.name.canonical == segment.canonical)?
                    .type_spec
                    .clone();
            } else {
                spec = self.resolve_named_spec(&spec, project);
                let DataTypeSpec::Struct { fields } = spec else {
                    return None;
                };
                spec = fields
                    .iter()
                    .find(|field| field.name.canonical == segment.canonical)?
                    .spec
                    .clone();
            };
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
        let mut current = self.resolve_named_spec(&spec, project);
        let mut remaining = indices.len();
        while remaining > 0 {
            let DataTypeSpec::Array {
                ranges,
                element_type,
            } = current
            else {
                return None;
            };
            if remaining < ranges.len() {
                return None;
            }
            remaining -= ranges.len();
            current = self.resolve_named_spec(&element_type, project);
            if remaining == 0 {
                return Some(current);
            }
        }
        Some(current)
    }

    fn resolve_named_spec(&self, spec: &DataTypeSpec, project: &Project) -> DataTypeSpec {
        self.resolve_named_spec_inner(spec, project, &mut BTreeSet::new())
    }

    fn resolve_named_spec_inner(
        &self,
        spec: &DataTypeSpec,
        project: &Project,
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
        self.resolve_named_spec_inner(&data_type.spec, project, seen)
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
                        enum_value_exists(project, &root.canonical).then_some(SimpleType::Enum)
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
            DataTypeSpec::String { wide, .. } => {
                if *wide {
                    SimpleType::WString
                } else {
                    SimpleType::String
                }
            }
            DataTypeSpec::Subrange { base, .. } => elementary_type(base),
            DataTypeSpec::Enum { .. } => SimpleType::Enum,
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
            self.check_direct_variable_reference(direct);
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
        self.check_array_index_constraints(variable, variables, project);
        if (variable.path.len() > 1 || variable.indices.iter().any(|indices| !indices.is_empty()))
            && self.variable_type(variable, variables, project).is_none()
        {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("invalid field or array access '{}'", variable),
                None,
            ));
        }
    }

    fn check_array_index_constraints(
        &mut self,
        variable: &VariableRef,
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) {
        let Some(root) = variable.root_name() else {
            return;
        };
        let Some(mut spec) = variables.get(&root.canonical).cloned() else {
            return;
        };

        for (segment_index, segment) in variable.path.iter().enumerate() {
            if segment_index > 0 {
                if let Some(field_spec) = standard_fb_field_type(&spec, &segment.canonical) {
                    spec = field_spec;
                } else if let Some(function_block) = function_block_pou(project, &spec) {
                    let Some(field) = function_block
                        .variable_declarations()
                        .find(|field| field.name.canonical == segment.canonical)
                    else {
                        return;
                    };
                    spec = field.type_spec.clone();
                } else {
                    spec = self.resolve_named_spec(&spec, project);
                    let DataTypeSpec::Struct { fields } = &spec else {
                        return;
                    };
                    let Some(field) = fields
                        .iter()
                        .find(|field| field.name.canonical == segment.canonical)
                    else {
                        return;
                    };
                    spec = field.spec.clone();
                }
            }

            let indices = variable
                .indices
                .get(segment_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            if indices.is_empty() {
                continue;
            }

            spec = self.check_array_indices_for_spec(variable, spec, indices, variables, project);
        }
    }

    fn check_array_indices_for_spec(
        &mut self,
        variable: &VariableRef,
        mut spec: DataTypeSpec,
        indices: &[Expr],
        variables: &BTreeMap<String, DataTypeSpec>,
        project: &Project,
    ) -> DataTypeSpec {
        let mut remaining = indices;
        while !remaining.is_empty() {
            let resolved = self.resolve_named_spec(&spec, project);
            let DataTypeSpec::Array {
                ranges,
                element_type,
            } = resolved
            else {
                return spec;
            };
            if remaining.len() < ranges.len() {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "array access '{}' expects {} index(es), got {}",
                        variable,
                        ranges.len(),
                        remaining.len()
                    ),
                    None,
                ));
                return *element_type;
            }
            let (current_indices, rest) = remaining.split_at(ranges.len());
            for (index_expr, range) in current_indices.iter().zip(ranges.iter()) {
                if let Some(value) = const_i64(index_expr, variables, project, self) {
                    if value < range.low || value > range.high {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "array index {value} in '{}' is outside range {}..{}",
                                variable, range.low, range.high
                            ),
                            None,
                        ));
                    }
                }
            }
            spec = *element_type;
            remaining = rest;
        }
        spec
    }

    fn check_declared_direct_variable_location(&mut self, location: &str) {
        if let Some(message) = validate_direct_variable_location(location, true) {
            self.diagnostics
                .push(Diagnostic::error(DiagnosticCode::Semantic, message, None));
        }
    }

    fn check_direct_variable_reference(&mut self, location: &str) {
        if let Some(message) = validate_direct_variable_location(location, false) {
            self.diagnostics
                .push(Diagnostic::error(DiagnosticCode::Semantic, message, None));
        }
    }

    fn check_access_declaration(
        &mut self,
        var: &VarDecl,
        variables: &BTreeMap<String, DataTypeSpec>,
        variable_kinds: &BTreeMap<String, VarBlockKind>,
        project: &Project,
    ) {
        let Some(access) = &var.access else {
            return;
        };
        if access.path.trim().is_empty() {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("access path '{}' is missing a target", var.name.original),
                None,
            ));
            return;
        }
        if access.path.starts_with('%') {
            self.check_direct_variable_reference(&access.path);
            return;
        }

        let Some(parts) = access_path_parts(&access.path) else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "access path '{}' has invalid target '{}'",
                    var.name.original, access.path
                ),
                None,
            ));
            return;
        };
        let root = &parts[0];
        if matches!(
            variable_kinds.get(root),
            Some(VarBlockKind::Temp | VarBlockKind::External | VarBlockKind::InOut)
        ) {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "access path '{}' cannot target VAR_TEMP, VAR_EXTERNAL, or VAR_IN_OUT variable '{}'",
                    var.name.original, access.path
                ),
                None,
            ));
        }
        let Some(target_type) = variables
            .get(root)
            .and_then(|spec| resolve_access_path_from_spec(spec, &parts[1..], project))
        else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "access path '{}' references unknown target '{}'",
                    var.name.original, access.path
                ),
                None,
            ));
            return;
        };
        self.check_access_type_matches(var, &target_type, project);
    }

    fn check_access_type_matches(
        &mut self,
        var: &VarDecl,
        target_type: &DataTypeSpec,
        project: &Project,
    ) {
        let expected = self.type_of_spec(&var.type_spec, project);
        let actual = self.type_of_spec(target_type, project);
        if expected != actual {
            let target = var
                .access
                .as_ref()
                .map(|access| access.path.as_str())
                .unwrap_or("");
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "access path '{}' type does not match target '{}'",
                    var.name.original, target
                ),
                None,
            ));
        }
    }

    fn check_configuration(&mut self, project: &Project, configuration: &Configuration) {
        let known_types = self.known_types(project);
        self.check_identifier_profile(&configuration.name, "configuration name");
        self.check_configuration_var_blocks(
            project,
            configuration,
            None,
            &configuration.var_blocks,
            format!("configuration '{}'", configuration.name.original),
            &known_types,
        );
        for resource in &configuration.resources {
            self.check_identifier_profile(&resource.name, "resource name");
            self.check_configuration_var_blocks(
                project,
                configuration,
                Some(resource),
                &resource.var_blocks,
                format!(
                    "resource '{}' in configuration '{}'",
                    resource.name.original, configuration.name.original
                ),
                &known_types,
            );
            let mut tasks = BTreeSet::new();
            let task_variables =
                self.configuration_task_variables(project, configuration, resource);
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
                if let Some(single) = &task.single {
                    self.check_expr_limit(single, "task SINGLE expression");
                    self.check_expr(single, &task_variables, project);
                    self.check_bool_expr(
                        single,
                        &task_variables,
                        project,
                        &format!("task '{}' SINGLE", task.name.original),
                    );
                }
                if let Some(interval) = &task.interval {
                    self.check_expr_limit(interval, "task INTERVAL expression");
                    self.check_expr(interval, &task_variables, project);
                    let actual = self.type_of_expr(interval, &task_variables, project);
                    if !matches!(
                        actual,
                        SimpleType::Time | SimpleType::Integer | SimpleType::Unknown
                    ) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "task '{}' INTERVAL expects TIME duration or integer milliseconds",
                                task.name.original
                            ),
                            None,
                        ));
                    } else if !matches!(actual, SimpleType::Unknown)
                        && !matches!(
                            interval,
                            Expr::Literal(Literal::DurationMs(_)) | Expr::Literal(Literal::Int(_))
                        )
                    {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "task '{}' INTERVAL must be a constant TIME duration or integer milliseconds",
                                task.name.original
                            ),
                            None,
                        ));
                    }
                }
                if let Some(priority) = &task.priority {
                    self.check_expr_limit(priority, "task PRIORITY expression");
                    self.check_expr(priority, &task_variables, project);
                    let actual = self.type_of_expr(priority, &task_variables, project);
                    if !matches!(actual, SimpleType::Integer | SimpleType::Unknown) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!("task '{}' PRIORITY expects integer", task.name.original),
                            None,
                        ));
                    } else if !matches!(actual, SimpleType::Unknown) {
                        match const_i64(priority, &task_variables, project, self) {
                            Some(value) if value >= 0 => {}
                            Some(_) => self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::Semantic,
                                format!(
                                    "task '{}' PRIORITY must be non-negative",
                                    task.name.original
                                ),
                                None,
                            )),
                            None => self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::Semantic,
                                format!(
                                    "task '{}' PRIORITY must be a constant integer",
                                    task.name.original
                                ),
                                None,
                            )),
                        }
                    }
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

                let program_type = project
                    .find_pou(&instance.program_type.original)
                    .filter(|pou| matches!(&pou.kind, PouKind::Program));
                if program_type.is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "program instance '{}' references unknown PROGRAM type '{}'",
                            instance.name.original, instance.program_type.original
                        ),
                        None,
                    ));
                } else if let Some(program_type) = program_type {
                    self.check_program_instance_args(
                        project,
                        configuration,
                        resource,
                        instance,
                        program_type,
                        &task_variables,
                    );
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

    fn check_program_instance_args(
        &mut self,
        project: &Project,
        configuration: &Configuration,
        resource: &Resource,
        instance: &ProgramInstance,
        program_type: &Pou,
        variables: &BTreeMap<String, DataTypeSpec>,
    ) {
        let mut seen = BTreeSet::new();
        for (position, arg) in instance.args.iter().enumerate() {
            let Some(name) = &arg.name else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "program instance '{}' parameter {} must name a PROGRAM variable",
                        instance.name.original,
                        position + 1
                    ),
                    None,
                ));
                continue;
            };
            if !seen.insert(name.canonical.clone()) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "program instance '{}' initializes parameter '{}' more than once",
                        instance.name.original, name.original
                    ),
                    None,
                ));
            }
            if arg.output {
                let Some(variable) = &arg.variable else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "program instance '{}' output binding '{}' requires a target variable",
                            instance.name.original, name.original
                        ),
                        None,
                    ));
                    continue;
                };
                let Some((target, block_kind)) = program_variable_with_kind(program_type, name)
                else {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "program instance '{}' references unknown PROGRAM variable '{}'",
                            instance.name.original, name.original
                        ),
                        None,
                    ));
                    continue;
                };
                if block_kind != VarBlockKind::Output {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "program instance '{}' output binding '{}' must reference a VAR_OUTPUT variable",
                            instance.name.original, name.original
                        ),
                        None,
                    ));
                    continue;
                }
                self.check_variable(variable, variables, project);
                let actual_spec = self
                    .variable_type(variable, variables, project)
                    .or_else(|| {
                        output_binding_access_target(variable).and_then(|target| {
                            resolve_configuration_access_target(
                                configuration,
                                Some(resource),
                                &target,
                                project,
                            )
                        })
                    });
                if let Some(actual_spec) = actual_spec {
                    let expected = self.type_of_spec(&actual_spec, project);
                    let actual = self.type_of_spec(&target.type_spec, project);
                    if !types_are_assignable(expected, actual) {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Semantic,
                            format!(
                                "program instance '{}' output binding '{}' expects {} target, got {}",
                                instance.name.original,
                                name.original,
                                actual.as_str(),
                                expected.as_str()
                            ),
                            None,
                        ));
                    }
                } else if variable.direct.is_none() {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Semantic,
                        format!(
                            "program instance '{}' output binding '{}' references unknown target '{}'",
                            instance.name.original, name.original, variable
                        ),
                        None,
                    ));
                }
                continue;
            }
            let Some((target, block_kind)) = program_variable_with_kind(program_type, name) else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "program instance '{}' references unknown PROGRAM variable '{}'",
                        instance.name.original, name.original
                    ),
                    None,
                ));
                continue;
            };
            if matches!(
                block_kind,
                VarBlockKind::Access
                    | VarBlockKind::External
                    | VarBlockKind::InOut
                    | VarBlockKind::Temp
            ) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "program instance '{}' cannot initialize {} variable '{}'",
                        instance.name.original,
                        var_block_kind_label(block_kind),
                        name.original
                    ),
                    None,
                ));
                continue;
            }
            let Some(expr) = &arg.expr else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "program instance '{}' parameter '{}' requires an input value",
                        instance.name.original, name.original
                    ),
                    None,
                ));
                continue;
            };
            self.check_expr_limit(
                expr,
                &format!(
                    "program instance '{}' parameter '{}'",
                    instance.name.original, name.original
                ),
            );
            self.check_expr(expr, variables, project);
            if const_standard_value(expr, variables, project, self).is_none() {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "program instance '{}' parameter '{}' must be a constant expression",
                        instance.name.original, name.original
                    ),
                    None,
                ));
            }
            self.check_assignment_type(
                &target.type_spec,
                expr,
                variables,
                project,
                format!(
                    "program instance '{}' parameter '{}'",
                    instance.name.original, name.original
                ),
            );
            self.check_initialization_constraints(
                &target.type_spec,
                expr,
                variables,
                project,
                format!(
                    "program instance '{}' parameter '{}'",
                    instance.name.original, name.original
                ),
            );
        }
    }

    fn check_configuration_var_blocks(
        &mut self,
        project: &Project,
        configuration: &Configuration,
        resource: Option<&Resource>,
        blocks: &[VarBlock],
        context: String,
        known_types: &BTreeSet<String>,
    ) {
        let mut names = BTreeSet::new();
        let mut variables = self.project_global_variables(project, "");
        for block in &configuration.var_blocks {
            if block.kind != VarBlockKind::Access {
                for var in &block.vars {
                    variables.insert(var.name.canonical.clone(), var.type_spec.clone());
                }
            }
        }
        if let Some(resource) = resource {
            for block in &resource.var_blocks {
                if block.kind != VarBlockKind::Access {
                    for var in &block.vars {
                        variables.insert(var.name.canonical.clone(), var.type_spec.clone());
                    }
                }
            }
        }
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
                    self.check_declared_direct_variable_location(location);
                }
                if var.access.is_some() {
                    self.check_configuration_access_declaration(
                        var,
                        project,
                        configuration,
                        resource,
                    );
                }
                self.check_type_spec(&var.type_spec, known_types);
                if let Some(initial_value) = &var.initial_value {
                    self.check_expr_limit(initial_value, "configuration variable initial value");
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
    }

    fn configuration_task_variables(
        &self,
        project: &Project,
        configuration: &Configuration,
        resource: &Resource,
    ) -> BTreeMap<String, DataTypeSpec> {
        let mut variables = self.project_global_variables(project, "");
        for block in configuration
            .var_blocks
            .iter()
            .chain(resource.var_blocks.iter())
        {
            if block.kind == VarBlockKind::Access {
                continue;
            }
            for var in &block.vars {
                variables
                    .entry(var.name.canonical.clone())
                    .or_insert_with(|| var.type_spec.clone());
            }
        }
        variables
    }

    fn check_configuration_access_declaration(
        &mut self,
        var: &VarDecl,
        project: &Project,
        configuration: &Configuration,
        resource: Option<&Resource>,
    ) {
        let Some(access) = &var.access else {
            return;
        };
        if access.path.trim().is_empty() {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("access path '{}' is missing a target", var.name.original),
                None,
            ));
            return;
        }
        if access.path.starts_with('%') {
            self.check_direct_variable_reference(&access.path);
            return;
        }
        let Some(target_type) =
            resolve_configuration_access_target(configuration, resource, &access.path, project)
        else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "access path '{}' references unknown target '{}'",
                    var.name.original, access.path
                ),
                None,
            ));
            return;
        };
        self.check_access_type_matches(var, &target_type, project);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SimpleType {
    Bool,
    Integer,
    Real,
    BitString,
    String,
    WString,
    Time,
    Date,
    TimeOfDay,
    DateAndTime,
    Enum,
    Aggregate,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GenericFamily {
    Any,
    #[allow(dead_code)]
    AnyDerived,
    AnyElementary,
    AnyMagnitude,
    AnyNum,
    AnyReal,
    AnyInt,
    AnyBit,
    AnyString,
    #[allow(dead_code)]
    AnyDate,
    BitString,
    Bool,
    String,
    WString,
    Time,
    Date,
    TimeOfDay,
    DateAndTime,
}

impl GenericFamily {
    fn as_str(self) -> &'static str {
        match self {
            Self::Any => "ANY",
            Self::AnyDerived => "ANY_DERIVED",
            Self::AnyElementary => "ANY_ELEMENTARY",
            Self::AnyMagnitude => "ANY_MAGNITUDE",
            Self::AnyNum => "ANY_NUM",
            Self::AnyReal => "ANY_REAL",
            Self::AnyInt => "ANY_INT",
            Self::AnyBit => "ANY_BIT",
            Self::AnyString => "ANY_STRING",
            Self::AnyDate => "ANY_DATE",
            Self::BitString => "bit-string",
            Self::Bool => "BOOL",
            Self::String => "STRING",
            Self::WString => "WSTRING",
            Self::Time => "TIME",
            Self::Date => "DATE",
            Self::TimeOfDay => "TIME_OF_DAY",
            Self::DateAndTime => "DATE_AND_TIME",
        }
    }

    fn contains(self, actual: SimpleType) -> bool {
        if actual == SimpleType::Unknown || self == Self::Any {
            return true;
        }
        match self {
            Self::Any => true,
            Self::AnyDerived => matches!(actual, SimpleType::Enum | SimpleType::Aggregate),
            Self::AnyElementary => !matches!(actual, SimpleType::Aggregate),
            Self::AnyMagnitude => matches!(
                actual,
                SimpleType::Integer | SimpleType::Real | SimpleType::Time
            ),
            Self::AnyNum => matches!(actual, SimpleType::Integer | SimpleType::Real),
            Self::AnyReal => actual == SimpleType::Real,
            Self::AnyInt => actual == SimpleType::Integer,
            Self::AnyBit => matches!(
                actual,
                SimpleType::Bool | SimpleType::Integer | SimpleType::BitString
            ),
            Self::AnyString => matches!(actual, SimpleType::String | SimpleType::WString),
            Self::AnyDate => matches!(
                actual,
                SimpleType::Date | SimpleType::TimeOfDay | SimpleType::DateAndTime
            ),
            Self::BitString => actual == SimpleType::BitString,
            Self::Bool => actual == SimpleType::Bool,
            Self::String => actual == SimpleType::String,
            Self::WString => actual == SimpleType::WString,
            Self::Time => actual == SimpleType::Time,
            Self::Date => actual == SimpleType::Date,
            Self::TimeOfDay => actual == SimpleType::TimeOfDay,
            Self::DateAndTime => actual == SimpleType::DateAndTime,
        }
    }
}

impl SimpleType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Bool => "BOOL",
            Self::Integer => "integer",
            Self::Real => "REAL",
            Self::BitString => "bit-string",
            Self::String => "STRING",
            Self::WString => "WSTRING",
            Self::Time => "TIME",
            Self::Date => "DATE",
            Self::TimeOfDay => "TIME_OF_DAY",
            Self::DateAndTime => "DATE_AND_TIME",
            Self::Enum => "enumerated",
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

fn types_have_common_value_type(left: SimpleType, right: SimpleType) -> bool {
    types_are_assignable(left, right) || types_are_assignable(right, left)
}

fn literal_type(literal: &Literal, project: &Project) -> SimpleType {
    match literal {
        Literal::Int(_) => SimpleType::Integer,
        Literal::Real(_) => SimpleType::Real,
        Literal::Bool(_) => SimpleType::Bool,
        Literal::String(_) => SimpleType::String,
        Literal::WString(_) => SimpleType::WString,
        Literal::DurationMs(_) => SimpleType::Time,
        Literal::Date(_) => SimpleType::Date,
        Literal::TimeOfDay(_) => SimpleType::TimeOfDay,
        Literal::DateAndTime(_) => SimpleType::DateAndTime,
        Literal::Typed { type_name, .. } => ElementaryType::parse(&type_name.original)
            .map(|elementary| elementary_type(&elementary))
            .or_else(|| typed_literal_named_type(project, type_name))
            .unwrap_or(SimpleType::Unknown),
    }
}

fn typed_literal_named_type(project: &Project, type_name: &Identifier) -> Option<SimpleType> {
    typed_literal_named_type_inner(project, type_name, &mut BTreeSet::new())
}

fn typed_literal_named_type_inner(
    project: &Project,
    type_name: &Identifier,
    seen: &mut BTreeSet<String>,
) -> Option<SimpleType> {
    if !seen.insert(type_name.canonical.clone()) {
        return Some(SimpleType::Unknown);
    }
    let data_type = project
        .data_types()
        .find(|data_type| data_type.name.canonical == type_name.canonical)?;
    match &data_type.spec {
        DataTypeSpec::Elementary(elementary) => Some(elementary_type(elementary)),
        DataTypeSpec::Subrange { base, .. } => Some(elementary_type(base)),
        DataTypeSpec::Enum { .. } => Some(SimpleType::Enum),
        DataTypeSpec::String { wide, .. } => Some(if *wide {
            SimpleType::WString
        } else {
            SimpleType::String
        }),
        DataTypeSpec::Array { .. } | DataTypeSpec::Struct { .. } => Some(SimpleType::Aggregate),
        DataTypeSpec::Named(next) => typed_literal_named_type_inner(project, next, seen),
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

fn il_op_name(op: IlOp) -> &'static str {
    match op {
        IlOp::Ld => "LD",
        IlOp::Ldn => "LDN",
        IlOp::St => "ST",
        IlOp::Stn => "STN",
        IlOp::S => "S",
        IlOp::R => "R",
        IlOp::And => "AND",
        IlOp::Andn => "ANDN",
        IlOp::Or => "OR",
        IlOp::Orn => "ORN",
        IlOp::Xor => "XOR",
        IlOp::Xorn => "XORN",
        IlOp::Not => "NOT",
        IlOp::Add => "ADD",
        IlOp::Sub => "SUB",
        IlOp::Mul => "MUL",
        IlOp::Div => "DIV",
        IlOp::Mod => "MOD",
        IlOp::Gt => "GT",
        IlOp::Ge => "GE",
        IlOp::Eq => "EQ",
        IlOp::Ne => "NE",
        IlOp::Le => "LE",
        IlOp::Lt => "LT",
        IlOp::Jmp => "JMP",
        IlOp::Jmpc => "JMPC",
        IlOp::Jmpcn => "JMPCN",
        IlOp::Cal => "CAL",
        IlOp::Calc => "CALC",
        IlOp::Calcn => "CALCN",
        IlOp::Ret => "RET",
        IlOp::Retc => "RETC",
        IlOp::Retcn => "RETCN",
    }
}

fn case_range_label(low: i128, high: i128) -> String {
    if low == high {
        low.to_string()
    } else {
        format!("{low}..{high}")
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
        ElementaryType::String => SimpleType::String,
        ElementaryType::WString => SimpleType::WString,
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
    let arg_types = ordered_standard_function_input_exprs(name, args)
        .into_iter()
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
        "ADD" | "SUB" | "MUL" | "DIV" | "MOD" => {
            if arg_types.contains(&SimpleType::Real) {
                SimpleType::Real
            } else if arg_types.contains(&SimpleType::Unknown) {
                SimpleType::Unknown
            } else {
                SimpleType::Integer
            }
        }
        "MIN" | "MAX" => {
            if arg_types.contains(&SimpleType::Real) {
                SimpleType::Real
            } else if arg_types.contains(&SimpleType::Unknown) {
                SimpleType::Unknown
            } else {
                arg_types.first().copied().unwrap_or(SimpleType::Unknown)
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
        "LEFT" | "RIGHT" | "MID" | "CONCAT" | "INSERT" | "DELETE" | "REPLACE" => {
            if arg_types.contains(&SimpleType::WString) {
                SimpleType::WString
            } else {
                SimpleType::String
            }
        }
        "ADD_TIME" | "SUB_TIME" | "MUL_TIME" | "DIV_TIME" | "MULTIME" | "DIVTIME"
        | "SUB_DATE_DATE" | "SUB_TOD_TOD" | "SUB_DT_DT" => SimpleType::Time,
        "ADD_TOD_TIME" | "SUB_TOD_TIME" => SimpleType::TimeOfDay,
        "ADD_DT_TIME" | "SUB_DT_TIME" => SimpleType::DateAndTime,
        "CONCAT_DATE" => SimpleType::Date,
        "CONCAT_TOD" => SimpleType::TimeOfDay,
        "CONCAT_DT" | "CONCAT_DATE_TOD" => SimpleType::DateAndTime,
        "DAY_OF_WEEK" => SimpleType::Integer,
        "BOOL_TO_INT" | "REAL_TO_INT" => SimpleType::Integer,
        "INT_TO_BOOL" => SimpleType::Bool,
        "INT_TO_REAL" => SimpleType::Real,
        _ => SimpleType::Unknown,
    }
}

fn ordered_standard_function_input_exprs<'a>(
    name: &Identifier,
    args: &'a [ParamAssignment],
) -> Vec<&'a Expr> {
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
            standard_function_input_index(&name.original, &arg_name.original).unwrap_or_else(|| {
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
    ordered.into_iter().map(|(_, expr)| expr).collect()
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
        "STRING" => Some(SimpleType::String),
        "WSTRING" => Some(SimpleType::WString),
        "TIME" => Some(SimpleType::Time),
        "DATE" => Some(SimpleType::Date),
        "TOD" | "TIME_OF_DAY" => Some(SimpleType::TimeOfDay),
        "DT" | "DATE_AND_TIME" => Some(SimpleType::DateAndTime),
        _ => None,
    }
}

fn conversion_source_family(name: &str) -> Option<GenericFamily> {
    let (source, _) = name.split_once("_TO_")?;
    conversion_type_family(source)
}

fn conversion_type_family(name: &str) -> Option<GenericFamily> {
    match name {
        "BOOL" => Some(GenericFamily::Bool),
        "SINT" | "INT" | "DINT" | "LINT" | "USINT" | "UINT" | "UDINT" | "ULINT" => {
            Some(GenericFamily::AnyInt)
        }
        "BYTE" | "WORD" | "DWORD" | "LWORD" => Some(GenericFamily::BitString),
        "REAL" | "LREAL" => Some(GenericFamily::AnyReal),
        "STRING" => Some(GenericFamily::String),
        "WSTRING" => Some(GenericFamily::WString),
        "TIME" => Some(GenericFamily::Time),
        "DATE" => Some(GenericFamily::Date),
        "TOD" | "TIME_OF_DAY" => Some(GenericFamily::TimeOfDay),
        "DT" | "DATE_AND_TIME" => Some(GenericFamily::DateAndTime),
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

fn bcd_conversion_source_family(name: &str) -> Option<GenericFamily> {
    if name == "BCD_TO_INT" {
        return Some(GenericFamily::BitString);
    }
    if name == "INT_TO_BCD" {
        return Some(GenericFamily::AnyInt);
    }
    if name.split_once("_BCD_TO_").is_some() {
        return Some(GenericFamily::BitString);
    }
    if name.split_once("_TO_BCD_").is_some() {
        return Some(GenericFamily::AnyInt);
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
    if let Expr::Literal(Literal::Typed { value, .. }) = expr {
        return Some(canonical_identifier(value));
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
    variable.root_name().map(|name| name.canonical.clone())
}

fn enum_type_root(project: &Project, type_name: &Identifier) -> Option<String> {
    let mut current = type_name.canonical.clone();
    let mut seen = BTreeSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return None;
        }
        let data_type = project
            .data_types()
            .find(|data_type| data_type.name.canonical == current)?;
        match &data_type.spec {
            DataTypeSpec::Enum { .. } => return Some(data_type.name.canonical.clone()),
            DataTypeSpec::Named(next) => current = next.canonical.clone(),
            _ => return None,
        }
    }
}

fn enum_type_name_for_spec(project: &Project, spec: &DataTypeSpec) -> Option<Identifier> {
    match spec {
        DataTypeSpec::Named(name) => {
            let data_type = project
                .data_types()
                .find(|data_type| data_type.name.canonical == name.canonical)?;
            match &data_type.spec {
                DataTypeSpec::Enum { .. } => Some(data_type.name.clone()),
                nested => enum_type_name_for_spec(project, nested),
            }
        }
        _ => None,
    }
}

fn enum_case_label_ordinal(
    project: &Project,
    expected_type: &Identifier,
    expr: &Expr,
) -> Option<i128> {
    let expected_root = enum_type_root(project, expected_type)?;
    match expr {
        Expr::Literal(Literal::Typed { type_name, value }) => (enum_type_root(project, type_name)?
            == expected_root)
            .then(|| enum_ordinal_in_root(project, &expected_root, &canonical_identifier(value)))
            .flatten(),
        Expr::Variable(variable)
            if variable.direct.is_none()
                && variable.path.len() == 1
                && variable.indices.iter().all(Vec::is_empty) =>
        {
            enum_ordinal_in_root(project, &expected_root, &variable.root_name()?.canonical)
        }
        _ => None,
    }
}

fn enum_ordinal_in_root(project: &Project, root_type: &str, value_name: &str) -> Option<i128> {
    project.data_types().find_map(|data_type| {
        if data_type.name.canonical != root_type {
            return None;
        }
        let DataTypeSpec::Enum { values } = &data_type.spec else {
            return None;
        };
        values
            .iter()
            .position(|value| value.canonical == value_name)
            .map(|index| index as i128)
    })
}

fn enum_selection_data_args<'a>(
    name: &Identifier,
    args: &'a [ParamAssignment],
) -> Option<Vec<&'a Expr>> {
    match name.canonical.as_str() {
        "SEL" => {
            let formal = ["IN0", "IN1"]
                .into_iter()
                .filter_map(|param| {
                    args.iter()
                        .find(|arg| {
                            !arg.output
                                && arg
                                    .name
                                    .as_ref()
                                    .is_some_and(|name| name.canonical == param)
                        })
                        .and_then(|arg| arg.expr.as_ref())
                })
                .collect::<Vec<_>>();
            if !formal.is_empty() {
                return Some(formal);
            }
            Some(positional_input_exprs(args).into_iter().skip(1).collect())
        }
        "MUX" => {
            let mut formal = args
                .iter()
                .filter_map(|arg| {
                    let name = arg.name.as_ref()?;
                    let suffix = name.canonical.strip_prefix("IN")?;
                    let index = suffix.parse::<usize>().ok()?;
                    (!arg.output).then_some((index, arg.expr.as_ref()?))
                })
                .collect::<Vec<_>>();
            if !formal.is_empty() {
                formal.sort_by_key(|(index, _)| *index);
                return Some(formal.into_iter().map(|(_, expr)| expr).collect());
            }
            Some(positional_input_exprs(args).into_iter().skip(1).collect())
        }
        _ => None,
    }
}

fn positional_input_exprs(args: &[ParamAssignment]) -> Vec<&Expr> {
    args.iter()
        .filter(|arg| !arg.output)
        .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
        .filter(|arg| arg.name.is_none())
        .filter_map(|arg| arg.expr.as_ref())
        .collect()
}

fn typed_literal_i128(value: &str) -> Option<i128> {
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

fn valid_integer_underscore_placement(raw: &str) -> bool {
    let raw = raw
        .strip_prefix('-')
        .or_else(|| raw.strip_prefix('+'))
        .unwrap_or(raw);
    valid_underscore_placement(raw)
}

fn valid_underscore_placement(raw: &str) -> bool {
    let bytes = raw.as_bytes();
    if bytes.is_empty() || bytes.first() == Some(&b'_') || bytes.last() == Some(&b'_') {
        return false;
    }
    !bytes.windows(2).any(|pair| pair == b"__")
}

fn real_literal_f64(raw: &str) -> Option<f64> {
    let trimmed = raw.trim();
    let unsigned = trimmed
        .strip_prefix('-')
        .or_else(|| trimmed.strip_prefix('+'))
        .unwrap_or(trimmed);
    if !valid_underscore_placement(unsigned) {
        return None;
    }
    let value = trimmed.replace('_', "").parse::<f64>().ok()?;
    value.is_finite().then_some(value)
}

fn parse_duration_ms_checked(raw: &str) -> Option<i128> {
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

fn valid_decimal_component(raw: &str) -> bool {
    if !valid_underscore_placement(raw) {
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

fn parse_date_time_ms(input: &str) -> Option<i128> {
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

fn parse_date_days(input: &str) -> Option<i64> {
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

fn parse_time_of_day_ms(input: &str) -> Option<i128> {
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

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
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

fn function_block_pou<'a>(project: &'a Project, spec: &DataTypeSpec) -> Option<&'a Pou> {
    let DataTypeSpec::Named(type_name) = spec else {
        return None;
    };
    project
        .find_pou(&type_name.original)
        .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
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
        name if is_communication_function_block(name) => match field {
            "DONE" | "NDR" | "ERROR" => DataTypeSpec::Elementary(ElementaryType::Bool),
            "STATUS" => DataTypeSpec::Elementary(ElementaryType::Int),
            _ => return None,
        },
        _ => return None,
    };
    Some(spec)
}

fn standard_function_block_inputs(name: &str) -> Vec<(&'static str, DataTypeSpec)> {
    let bool_spec = || DataTypeSpec::Elementary(ElementaryType::Bool);
    let int_spec = || DataTypeSpec::Elementary(ElementaryType::Int);
    let time_spec = || DataTypeSpec::Elementary(ElementaryType::Time);
    match canonical_identifier(name).as_str() {
        "SR" => vec![("S1", bool_spec()), ("R", bool_spec())],
        "RS" => vec![("S", bool_spec()), ("R1", bool_spec())],
        "R_TRIG" | "F_TRIG" => vec![("CLK", bool_spec())],
        "CTU" => vec![("CU", bool_spec()), ("R", bool_spec()), ("PV", int_spec())],
        "CTD" => vec![("CD", bool_spec()), ("LD", bool_spec()), ("PV", int_spec())],
        "CTUD" => vec![
            ("CU", bool_spec()),
            ("CD", bool_spec()),
            ("R", bool_spec()),
            ("LD", bool_spec()),
            ("PV", int_spec()),
        ],
        "TON" | "TOF" | "TP" => vec![("IN", bool_spec()), ("PT", time_spec())],
        name if is_communication_function_block(name) => vec![
            ("REQ", bool_spec()),
            ("EN_R", bool_spec()),
            ("ID", int_spec()),
            ("LEN", int_spec()),
        ],
        _ => Vec::new(),
    }
}

fn standard_function_block_outputs(name: &str) -> Vec<(&'static str, DataTypeSpec)> {
    let bool_spec = || DataTypeSpec::Elementary(ElementaryType::Bool);
    let int_spec = || DataTypeSpec::Elementary(ElementaryType::Int);
    let time_spec = || DataTypeSpec::Elementary(ElementaryType::Time);
    match canonical_identifier(name).as_str() {
        "SR" | "RS" => vec![("Q1", bool_spec())],
        "R_TRIG" | "F_TRIG" => vec![("Q", bool_spec())],
        "CTU" | "CTD" => vec![("Q", bool_spec()), ("CV", int_spec())],
        "CTUD" => vec![("QU", bool_spec()), ("QD", bool_spec()), ("CV", int_spec())],
        "TON" | "TOF" | "TP" => vec![("Q", bool_spec()), ("ET", time_spec())],
        name if is_communication_function_block(name) => vec![
            ("DONE", bool_spec()),
            ("NDR", bool_spec()),
            ("ERROR", bool_spec()),
            ("STATUS", int_spec()),
        ],
        _ => Vec::new(),
    }
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
        Expr::Literal(Literal::Typed { value, .. }) => {
            typed_literal_i128(value).and_then(|value| i64::try_from(value).ok())
        }
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
        Expr::Call { .. } => {
            const_standard_value(expr, variables, project, checker).and_then(|value| value.as_i64())
        }
        _ => {
            let _ = checker.type_of_expr(expr, variables, project);
            None
        }
    }
}

fn const_integer_i128(
    expr: &Expr,
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> Option<i128> {
    match expr {
        Expr::Literal(Literal::Int(value)) => Some(i128::from(*value)),
        Expr::Literal(Literal::Bool(value)) => Some(if *value { 1 } else { 0 }),
        Expr::Literal(Literal::Typed { value, .. }) => typed_literal_i128(value),
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => const_integer_i128(expr, variables, project, checker).and_then(i128::checked_neg),
        Expr::Binary { op, left, right } => {
            let left = const_integer_i128(left, variables, project, checker)?;
            let right = const_integer_i128(right, variables, project, checker)?;
            match op {
                BinaryOp::Add => left.checked_add(right),
                BinaryOp::Sub => left.checked_sub(right),
                BinaryOp::Mul => left.checked_mul(right),
                BinaryOp::Div if right != 0 => left.checked_div(right),
                BinaryOp::Mod if right != 0 => left.checked_rem(right),
                _ => None,
            }
        }
        _ => const_i64(expr, variables, project, checker).map(i128::from),
    }
}

fn const_string_expr(
    expr: &Expr,
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> Option<String> {
    match const_standard_value(expr, variables, project, checker)? {
        Value::String(value) | Value::WString(value) => Some(value),
        _ => None,
    }
}

fn const_standard_value(
    expr: &Expr,
    variables: &BTreeMap<String, DataTypeSpec>,
    project: &Project,
    checker: &Checker,
) -> Option<Value> {
    match expr {
        Expr::Literal(Literal::Int(value)) => Some(Value::Int(*value)),
        Expr::Literal(Literal::Real(value)) if value.is_finite() => Some(Value::Real(*value)),
        Expr::Literal(Literal::Bool(value)) => Some(Value::Bool(*value)),
        Expr::Literal(Literal::String(value)) => Some(Value::String(value.clone())),
        Expr::Literal(Literal::WString(value)) => Some(Value::WString(value.clone())),
        Expr::Literal(Literal::DurationMs(value)) => Some(Value::TimeMs(*value)),
        Expr::Literal(Literal::Typed { type_name, value }) => {
            typed_literal_const_value(project, type_name, value)
        }
        Expr::Unary {
            op: UnaryOp::Neg,
            expr,
        } => match const_standard_value(expr, variables, project, checker)? {
            Value::Int(value) => value.checked_neg().map(Value::Int),
            Value::Real(value) => Some(Value::Real(-value)),
            Value::TimeMs(value) => value.checked_neg().map(Value::TimeMs),
            _ => None,
        },
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
            .map(Value::Int)
        }
        Expr::Call { name, args } => {
            let values = ordered_standard_function_input_exprs(name, args)
                .into_iter()
                .map(|expr| const_standard_value(expr, variables, project, checker))
                .collect::<Option<Vec<_>>>()?;
            iec_stdlib::eval_standard_function(&name.original, &values)
        }
        _ => {
            let _ = checker.type_of_expr(expr, variables, project);
            None
        }
    }
}

fn typed_literal_const_value(
    project: &Project,
    type_name: &Identifier,
    value: &str,
) -> Option<Value> {
    if let Some(elementary) = ElementaryType::parse(&type_name.original) {
        return typed_literal_elementary_value(elementary, value);
    }
    let spec = project
        .data_types()
        .find(|data_type| data_type.name.canonical == type_name.canonical)
        .map(|data_type| data_type.spec.clone())?;
    typed_literal_spec_value(project, &spec, value, &mut BTreeSet::new())
}

fn typed_literal_spec_value(
    project: &Project,
    spec: &DataTypeSpec,
    value: &str,
    seen: &mut BTreeSet<String>,
) -> Option<Value> {
    match spec {
        DataTypeSpec::Elementary(elementary) => {
            typed_literal_elementary_value(elementary.clone(), value)
        }
        DataTypeSpec::Subrange { .. } => typed_literal_i128(value)
            .and_then(|value| i64::try_from(value).ok())
            .map(Value::Int),
        DataTypeSpec::Enum { values } => {
            let value = canonical_identifier(value);
            values
                .iter()
                .position(|candidate| candidate.canonical == value)
                .map(|index| Value::Int(index as i64))
        }
        DataTypeSpec::String { wide, .. } => {
            if *wide {
                Some(Value::WString(value.to_string()))
            } else {
                Some(Value::String(value.to_string()))
            }
        }
        DataTypeSpec::Named(name) => {
            if !seen.insert(name.canonical.clone()) {
                return None;
            }
            let data_type = project
                .data_types()
                .find(|data_type| data_type.name.canonical == name.canonical)?;
            typed_literal_spec_value(project, &data_type.spec, value, seen)
        }
        DataTypeSpec::Array { .. } | DataTypeSpec::Struct { .. } => None,
    }
}

fn typed_literal_elementary_value(elementary: ElementaryType, value: &str) -> Option<Value> {
    match elementary {
        ElementaryType::Bool => parse_typed_bool(value).map(Value::Bool),
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
        | ElementaryType::Lword => typed_literal_i128(value)
            .and_then(|value| i64::try_from(value).ok())
            .map(Value::Int),
        ElementaryType::Real | ElementaryType::Lreal => real_literal_f64(value).map(Value::Real),
        ElementaryType::Time => typed_literal_i128(value)
            .or_else(|| parse_duration_ms_checked(value))
            .map(Value::TimeMs),
        ElementaryType::Date => parse_date_days(value).map(|value| Value::TimeMs(value as i128)),
        ElementaryType::TimeOfDay => parse_time_of_day_ms(value).map(Value::TimeMs),
        ElementaryType::DateAndTime => parse_date_time_ms(value).map(Value::TimeMs),
        ElementaryType::String => Some(Value::String(value.to_string())),
        ElementaryType::WString => Some(Value::WString(value.to_string())),
    }
}

fn parse_typed_bool(value: &str) -> Option<bool> {
    match canonical_identifier(value).as_str() {
        "TRUE" | "1" => Some(true),
        "FALSE" | "0" => Some(false),
        _ => None,
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
        Expr::Literal(Literal::String(value) | Literal::WString(value)) => {
            value.trim().parse::<i128>().ok()
        }
        Expr::Literal(Literal::DurationMs(value)) => Some(*value),
        Expr::Literal(Literal::Typed { type_name, value }) => {
            typed_literal_const_value(project, type_name, value).and_then(|value| match value {
                Value::Bool(value) => Some(if value { 1 } else { 0 }),
                Value::Int(value) => Some(i128::from(value)),
                Value::Real(value) if value.is_finite() => Some(value as i128),
                Value::Real(_) => None,
                Value::String(value) | Value::WString(value) => value.trim().parse::<i128>().ok(),
                Value::TimeMs(value) => Some(value),
                Value::Array(_) | Value::Struct(_) | Value::Unit => None,
            })
        }
        _ => const_i64(expr, variables, project, checker).map(i128::from),
    }
}

fn retain_kind_label(kind: RetainKind) -> &'static str {
    match kind {
        RetainKind::Retain => "RETAIN",
        RetainKind::NonRetain => "NON_RETAIN",
    }
}

fn edge_qualifier_label(kind: EdgeQualifier) -> &'static str {
    match kind {
        EdgeQualifier::Rising => "R_EDGE",
        EdgeQualifier::Falling => "F_EDGE",
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

fn collect_function_calls_in_statements(
    statements: &[Statement],
    project: &Project,
    calls: &mut BTreeSet<String>,
) {
    for statement in statements {
        collect_function_calls_in_statement(statement, project, calls);
    }
}

fn collect_function_calls_in_statement(
    statement: &Statement,
    project: &Project,
    calls: &mut BTreeSet<String>,
) {
    match statement {
        Statement::Assignment { value, .. } => {
            collect_function_calls_in_expr(value, project, calls)
        }
        Statement::FbCall { args, .. } => collect_function_calls_in_args(args, project, calls),
        Statement::If {
            branches,
            else_branch,
        } => {
            for (condition, body) in branches {
                collect_function_calls_in_expr(condition, project, calls);
                collect_function_calls_in_statements(body, project, calls);
            }
            collect_function_calls_in_statements(else_branch, project, calls);
        }
        Statement::Case {
            selector,
            cases,
            else_branch,
        } => {
            collect_function_calls_in_expr(selector, project, calls);
            for (labels, body) in cases {
                for label in labels {
                    match label {
                        CaseLabel::Single(expr) => {
                            collect_function_calls_in_expr(expr, project, calls)
                        }
                        CaseLabel::Range(low, high) => {
                            collect_function_calls_in_expr(low, project, calls);
                            collect_function_calls_in_expr(high, project, calls);
                        }
                    }
                }
                collect_function_calls_in_statements(body, project, calls);
            }
            collect_function_calls_in_statements(else_branch, project, calls);
        }
        Statement::For {
            from, to, by, body, ..
        } => {
            collect_function_calls_in_expr(from, project, calls);
            collect_function_calls_in_expr(to, project, calls);
            if let Some(by) = by {
                collect_function_calls_in_expr(by, project, calls);
            }
            collect_function_calls_in_statements(body, project, calls);
        }
        Statement::While { condition, body } => {
            collect_function_calls_in_expr(condition, project, calls);
            collect_function_calls_in_statements(body, project, calls);
        }
        Statement::Repeat { body, until } => {
            collect_function_calls_in_statements(body, project, calls);
            collect_function_calls_in_expr(until, project, calls);
        }
        Statement::Il { operand, .. } => {
            if let Some(operand) = operand {
                collect_function_calls_in_expr(operand, project, calls);
            }
        }
        Statement::Empty
        | Statement::IlLabel(_)
        | Statement::Exit
        | Statement::Return
        | Statement::Unsupported(_) => {}
    }
}

fn collect_function_calls_in_args(
    args: &[ParamAssignment],
    project: &Project,
    calls: &mut BTreeSet<String>,
) {
    for arg in args {
        if let Some(expr) = &arg.expr {
            collect_function_calls_in_expr(expr, project, calls);
        }
    }
}

fn collect_function_calls_in_expr(expr: &Expr, project: &Project, calls: &mut BTreeSet<String>) {
    match expr {
        Expr::Call { name, args } => {
            if project
                .find_pou(&name.original)
                .is_some_and(|pou| matches!(&pou.kind, PouKind::Function { .. }))
            {
                calls.insert(name.canonical.clone());
            }
            collect_function_calls_in_args(args, project, calls);
        }
        Expr::Unary { expr, .. } => collect_function_calls_in_expr(expr, project, calls),
        Expr::Binary { left, right, .. } => {
            collect_function_calls_in_expr(left, project, calls);
            collect_function_calls_in_expr(right, project, calls);
        }
        Expr::ArrayLiteral(elements) => {
            for element in elements {
                collect_function_calls_in_expr(element, project, calls);
            }
        }
        Expr::StructLiteral(fields) => {
            for field in fields {
                if let Some(expr) = &field.expr {
                    collect_function_calls_in_expr(expr, project, calls);
                }
            }
        }
        Expr::Literal(_) | Expr::Variable(_) => {}
    }
}

fn function_reaches_itself(
    start: &str,
    current: &str,
    graph: &BTreeMap<String, BTreeSet<String>>,
    path: &mut Vec<String>,
    visited: &mut BTreeSet<String>,
) -> bool {
    if !visited.insert(current.to_string()) {
        return false;
    }
    path.push(current.to_string());
    let result = graph.get(current).is_some_and(|calls| {
        calls
            .iter()
            .any(|next| next == start || function_reaches_itself(start, next, graph, path, visited))
    });
    path.pop();
    result
}

fn access_path_parts(path: &str) -> Option<Vec<String>> {
    let parts = path
        .split('.')
        .map(str::trim)
        .map(|part| {
            let mut chars = part.chars();
            let first = chars.next()?;
            if !(first.is_ascii_alphabetic() || first == '_') {
                return None;
            }
            chars
                .all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                .then(|| canonical_identifier(part))
        })
        .collect::<Option<Vec<_>>>()?;
    (!parts.is_empty()).then_some(parts)
}

fn output_binding_access_target(variable: &VariableRef) -> Option<String> {
    if variable.indices.iter().any(|indices| !indices.is_empty()) {
        return None;
    }
    variable
        .direct
        .clone()
        .or_else(|| (!variable.path.is_empty()).then(|| variable.to_string()))
}

fn resolve_access_path_from_spec(
    spec: &DataTypeSpec,
    parts: &[String],
    project: &Project,
) -> Option<DataTypeSpec> {
    resolve_access_path_from_spec_inner(spec, parts, project, 0)
}

fn resolve_access_path_from_spec_inner(
    spec: &DataTypeSpec,
    parts: &[String],
    project: &Project,
    depth: usize,
) -> Option<DataTypeSpec> {
    if depth > 32 {
        return None;
    }
    if parts.is_empty() {
        return Some(spec.clone());
    }

    if let Some(field_spec) = standard_fb_field_type(spec, &parts[0]) {
        return resolve_access_path_from_spec_inner(&field_spec, &parts[1..], project, depth + 1);
    }

    if let Some(function_block) = function_block_pou(project, spec) {
        if let Some(field) = function_block
            .variable_declarations()
            .find(|field| field.name.canonical == parts[0])
        {
            return resolve_access_path_from_spec_inner(
                &field.type_spec,
                &parts[1..],
                project,
                depth + 1,
            );
        }
    }

    let resolved = match spec {
        DataTypeSpec::Named(name) => project
            .data_types()
            .find(|data_type| data_type.name.canonical == name.canonical)
            .map(|data_type| data_type.spec.clone())?,
        other => other.clone(),
    };

    match resolved {
        DataTypeSpec::Struct { fields } => {
            let field = fields
                .iter()
                .find(|field| field.name.canonical == parts[0])?;
            resolve_access_path_from_spec_inner(&field.spec, &parts[1..], project, depth + 1)
        }
        DataTypeSpec::Named(_) if &resolved != spec => {
            resolve_access_path_from_spec_inner(&resolved, parts, project, depth + 1)
        }
        _ => None,
    }
}

fn resolve_configuration_access_target(
    configuration: &Configuration,
    resource: Option<&Resource>,
    path: &str,
    project: &Project,
) -> Option<DataTypeSpec> {
    let parts = access_path_parts(path)?;

    if let Some(resource) = resource {
        if parts.first() == Some(&resource.name.canonical) {
            return resolve_resource_access_target(resource, &parts[1..], project);
        }
        if let Some(spec) = resolve_resource_access_target(resource, &parts, project) {
            return Some(spec);
        }
    }

    if let Some(spec) = variable_spec_in_blocks(&configuration.var_blocks, &parts[0])
        .and_then(|spec| resolve_access_path_from_spec(&spec, &parts[1..], project))
    {
        return Some(spec);
    }

    let resource = configuration
        .resources
        .iter()
        .find(|resource| resource.name.canonical == parts[0])?;
    resolve_resource_access_target(resource, &parts[1..], project)
}

fn resolve_resource_access_target(
    resource: &Resource,
    parts: &[String],
    project: &Project,
) -> Option<DataTypeSpec> {
    let root = parts.first()?;

    if let Some(spec) = variable_spec_in_blocks(&resource.var_blocks, root)
        .and_then(|spec| resolve_access_path_from_spec(&spec, &parts[1..], project))
    {
        return Some(spec);
    }

    let instance = resource
        .program_instances
        .iter()
        .find(|instance| instance.name.canonical == *root)?;
    let field = parts.get(1)?;
    let program = project
        .find_pou(&instance.program_type.original)
        .filter(|pou| matches!(&pou.kind, PouKind::Program))?;
    let spec = program
        .variable_declarations()
        .find(|var| var.name.canonical == *field)
        .map(|var| var.type_spec.clone())?;
    resolve_access_path_from_spec(&spec, &parts[2..], project)
}

fn variable_spec_in_blocks(blocks: &[VarBlock], name: &str) -> Option<DataTypeSpec> {
    blocks
        .iter()
        .filter(|block| block.kind != VarBlockKind::Access)
        .flat_map(|block| block.vars.iter())
        .find(|var| var.name.canonical == name)
        .map(|var| var.type_spec.clone())
}

fn program_variable_with_kind<'a>(
    program: &'a Pou,
    name: &Identifier,
) -> Option<(&'a VarDecl, VarBlockKind)> {
    program.var_blocks.iter().find_map(|block| {
        block
            .vars
            .iter()
            .find(|var| var.name.canonical == name.canonical)
            .map(|var| (var, block.kind))
    })
}

fn validate_direct_variable_location(location: &str, allow_incomplete: bool) -> Option<String> {
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
    if address == "*" {
        return if allow_incomplete {
            None
        } else {
            Some(format!(
                "incomplete direct variable location '{location}' is only valid in a declaration"
            ))
        };
    }

    if address.is_empty() {
        return Some(format!(
            "direct variable location '{location}' is missing an address"
        ));
    }

    if address.contains('*') {
        return Some(format!(
            "direct variable location '{location}' has invalid address '{address}'"
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

fn split_input_expr(args: &[ParamAssignment]) -> Option<&Expr> {
    args.iter()
        .find(|arg| !arg.output && arg.name.as_ref().is_some_and(|name| name.canonical == "IN"))
        .and_then(|arg| arg.expr.as_ref())
        .or_else(|| split_positional_args(args).first().copied())
}

fn split_positional_args(args: &[ParamAssignment]) -> Vec<&Expr> {
    args.iter()
        .filter(|arg| !arg.output)
        .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
        .filter(|arg| arg.name.is_none())
        .filter_map(|arg| arg.expr.as_ref())
        .collect()
}

fn uses_formal_split_outputs(args: &[ParamAssignment]) -> bool {
    args.iter()
        .any(|arg| arg.output && !arg.name.as_ref().is_some_and(is_implicit_eno))
}

fn split_formal_output<'a>(args: &'a [ParamAssignment], output: &str) -> Option<&'a VariableRef> {
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
    fn rejects_value_returning_function_calls_as_statements() {
        let source = r#"
            FUNCTION Sum2 : INT
            VAR_INPUT
                A : INT;
                B : INT;
            END_VAR
            Sum2 := A + B;
            END_FUNCTION

            PROGRAM Demo
            Sum2(A := 1, B := 2);
            ADD(1, 2);
            ABS(1);
            END_PROGRAM
        "#;
        let output = parse_project("function_statement_calls.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function 'Sum2' returns a value and cannot be used as a statement")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function 'ABS' returns a value and cannot be used as a statement")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function 'ADD' returns a value and cannot be used as a statement")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("unknown function block instance 'ABS'")));
    }

    #[test]
    fn checks_user_call_parameter_range_and_length_constraints() {
        let source = r#"
            TYPE
                Small : INT(0..10);
            END_TYPE

            FUNCTION UseSmall : INT
            VAR_INPUT
                X : Small;
                Label : STRING[3];
            END_VAR
            UseSmall := X;
            END_FUNCTION

            FUNCTION_BLOCK Capture
            VAR_INPUT
                X : Small;
                Label : STRING[3];
            END_VAR
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Out : INT := 0;
                Fb : Capture;
            END_VAR

            Out := UseSmall(X := 11, Label := CONCAT('abc', 'd'));
            Out := UseSmall(12, 'abcde');
            Fb(X := 11, Label := CONCAT('abc', 'd'));
            Fb(12, 'abcde');
            END_PROGRAM
        "#;
        let output = parse_project("call_parameter_constraints.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function 'UseSmall' parameter 'X' value 11 is outside subrange 0..10")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function 'UseSmall' parameter 'X' value 12 is outside subrange 0..10")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "function 'UseSmall' parameter 'Label' exceeds string length 3 with 4 character(s)"
            )));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "function 'UseSmall' parameter 'Label' exceeds string length 3 with 5 character(s)"
            )));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "function block 'Capture' parameter 'X' value 11 is outside subrange 0..10"
            )));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "function block 'Capture' parameter 'X' value 12 is outside subrange 0..10"
            )));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
            "function block 'Capture' parameter 'Label' exceeds string length 3 with 4 character(s)"
        )
        }));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
            "function block 'Capture' parameter 'Label' exceeds string length 3 with 5 character(s)"
        )
        }));
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
    fn rejects_recursive_function_cycles() {
        let source = r#"
            FUNCTION A : INT
            VAR_INPUT
                X : INT;
            END_VAR
            A := B(X := X);
            END_FUNCTION

            FUNCTION B : INT
            VAR_INPUT
                X : INT;
            END_VAR
            B := A(X := X);
            END_FUNCTION

            PROGRAM Demo
            VAR
                Out : INT := 0;
            END_VAR
            Out := A(X := 1);
            END_PROGRAM
        "#;
        let output = parse_project("recursive_functions.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("recursive function call cycle involving 'A' is not supported")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("recursive function call cycle involving 'B' is not supported")));
    }

    #[test]
    fn recognizes_communication_function_blocks_with_diagnostics() {
        let source = r#"
            PROGRAM Demo
            VAR
                Sender : USEND;
                Done : BOOL;
                Status : INT;
            END_VAR
            Sender(REQ := TRUE);
            Done := Sender.DONE;
            Status := Sender.STATUS;
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
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown field")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("communication function block 'USEND' requires a target runtime hook")));
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
            Flag := A = Text;
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
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("operator = cannot be applied to integer and STRING")));
    }

    #[test]
    fn rejects_exit_outside_iteration() {
        let source = r#"
            PROGRAM Demo
            VAR
                I : INT := 0;
                Done : BOOL := FALSE;
            END_VAR
            IF Done THEN
                EXIT;
            END_IF;
            WHILE I < 2 DO
                I := I + 1;
                IF I = 1 THEN
                    EXIT;
                END_IF;
            END_WHILE;
            FOR I := 0 TO 2 BY 0 DO
                Done := TRUE;
            END_FOR;
            END_PROGRAM
        "#;
        let output = parse_project("exit_context.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic
                    .message
                    .contains("EXIT used outside of an iteration"))
                .count(),
            1
        );
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("FOR BY value cannot be zero")));
    }

    #[test]
    fn checks_case_selector_and_constant_labels() {
        let source = r#"
            TYPE
                Mode : (Idle, Run, Fault);
                OtherMode : (Cold, Hot);
            END_TYPE

            PROGRAM Demo
            VAR
                I : INT := 0;
                Text : STRING[8] := 'x';
                State : Mode := Idle;
            END_VAR
            CASE Text OF
                'x': I := 1;
            END_CASE;
            CASE I OF
                1, 1: I := 2;
                2..4: I := 3;
                3: I := 4;
                7..5: I := 5;
            ELSE
                I := 6;
            END_CASE;
            CASE State OF
                Idle, Mode#Run, Idle: I := 7;
                Fault..Run: I := 8;
                OtherMode#Cold: I := 9;
                1: I := 10;
            END_CASE;
            END_PROGRAM
        "#;
        let output = parse_project("case_semantics.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("CASE selector expects integer or enumerated, got STRING")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("CASE label range 1 overlaps previous range 1")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("CASE label range 3 overlaps previous range 2..4")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("CASE range lower bound 7 exceeds upper bound 5")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("CASE label range 0 overlaps previous range 0")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("CASE enumerated selector does not support range labels")));
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic
                    .message
                    .contains("CASE label expects value of enum type 'Mode'"))
                .count(),
            2
        );
    }

    #[test]
    fn checks_standard_function_generic_families() {
        let source = r#"
            TYPE
                Mode : (Idle, Run);
            END_TYPE

            PROGRAM Demo
            VAR
                A : INT := 0;
                Shifted : INT := 0;
                Text : STRING[8] := 'x';
                Delay : TIME := T#0ms;
                R : REAL := 0.0;
                Today : DATE := D#1970-01-01;
                Clock : TIME_OF_DAY := TOD#00:00:00;
                Stamp : DATE_AND_TIME := DT#1970-01-01-00:00:00;
                State : Mode := Idle;
                Other : Mode := Run;
                Selected : Mode := Idle;
                Same : BOOL := FALSE;
                Flag : BOOL := FALSE;
            END_VAR
            A := ADD(Text, 1);
            A := LEN(1);
            A := SEL(TRUE, 1, Text);
            Shifted := SHL(Text, 1);
            Text := CONCAT('a', 1);
            Delay := ADD_TIME(T#1s, 1);
            Delay := MIN(T#2s, T#1s);
            Delay := MIN(T#1s, 2);
            Delay := MIN(Today, Today);
            R := SQRT(4);
            R := EXPT(2, 3);
            A := TRUNC(1);
            A := LIMIT(0.0, 1, 2.0);
            Clock := ADD_TOD_TIME(TOD#00:00:01, T#2s);
            Stamp := ADD_DT_TIME(DT#1970-01-01-00:00:01, T#2s);
            Delay := SUB_DATE_DATE(Today, Today);
            Today := CONCAT_DATE(1970, 1, 1);
            Clock := CONCAT_TOD(0, 0, 1, 0);
            Stamp := CONCAT_DT(1970, 1, 1, 0, 0, 1, 0);
            Stamp := CONCAT_DATE_TOD(Today, Clock);
            A := DAY_OF_WEEK(Today);
            Delay := SUB_TOD_TOD(Clock, T#1s);
            Stamp := ADD_DT_TIME(Stamp, 1);
            A := DAY_OF_WEEK(1);
            Selected := SEL(TRUE, State, Other);
            Same := EQ(State, Other);
            A := ADD(State, 1);
            Shifted := SHL(State, 1);
            Same := EQ(State, Text);
            Shifted := SHL(1, -1);
            SPLIT_DATE(IN := 1, YEAR => A);
            SPLIT_DT(IN := Stamp, YEAR => Flag);
            A := ABS(1, 2);
            A := SUB(1, 2, 3);
            A := MUX(3, 10, 20);
            Same := NE(1, 2, 3);
            Text := REPLACE('abc', 'x', 1);
            END_PROGRAM
        "#;
        let output = parse_project("standard_generic_types.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'ADD' argument 1 expects ANY_NUM, got STRING")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'LEN' argument 1 expects ANY_STRING, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
            "standard function 'SEL' data arguments must have compatible types, got integer and STRING"
        )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'SHL' argument 1 expects ANY_BIT, got STRING")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'CONCAT' argument 2 expects ANY_STRING, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'ADD_TIME' argument 2 expects TIME, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
            "standard function 'MIN' data arguments must have compatible types, got TIME and integer"
        )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'MIN' argument 1 expects ANY_MAGNITUDE, got DATE")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'SQRT' argument 1 expects ANY_REAL, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'EXPT' argument 1 expects ANY_REAL, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'TRUNC' argument 1 expects ANY_REAL, got integer")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("assignment to 'A' expects integer, got REAL")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'SUB_TOD_TOD' argument 2 expects TIME_OF_DAY, got TIME")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'ADD_DT_TIME' argument 2 expects TIME, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'DAY_OF_WEEK' argument 1 expects DATE, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'SPLIT_DATE' argument 1 expects DATE, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
            "standard function 'SPLIT_DT' output 'YEAR' expects INT-compatible variable, got BOOL"
        )
        }));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'ADD' argument 1 expects ANY_NUM, got enumerated")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'SHL' argument 1 expects ANY_BIT, got enumerated")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
            "standard function 'EQ' data arguments must have compatible types, got enumerated and STRING"
        )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'SHL' argument 2 must be non-negative, got -1")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'ABS' expects exactly 1 input argument(s), got 2")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'SUB' expects exactly 2 input argument(s), got 3")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'MUX' selector must be in range 0..1, got 3")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'NE' expects exactly 2 input argument(s), got 3")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'REPLACE' expects exactly 4 input argument(s), got 3")));
    }

    #[test]
    fn checks_standard_function_formal_input_names_and_duplicates() {
        let source = r#"
            PROGRAM Demo
            VAR
                A : INT := 0;
                B : BOOL := FALSE;
            END_VAR
            A := LIMIT(IN := 5, MN := 0, MX := 10);
            A := SEL(IN1 := 2, G := FALSE, IN0 := 1);
            A := MUX(IN1 := 20, K := 1, IN0 := 10);
            A := SHL(N := 2, IN := 1);
            A := LIMIT(IN := 5, BAD := 0, MX := 10);
            A := LIMIT(0, 5, IN := 6);
            A := ADD(1, 2, OUT => A);
            B := SEL(IN1 := TRUE, G := FALSE, IN0 := FALSE);
            END_PROGRAM
        "#;
        let output = parse_project("standard_formals.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'LIMIT' has no input parameter 'BAD'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "standard function 'LIMIT' input parameter 'IN' duplicates 'positional argument 2'"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'ADD' has no output parameter 'OUT'")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'SHL' argument 1 expects ANY_BIT")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("standard function 'SEL' data")));
    }

    #[test]
    fn orders_standard_formals_before_inferring_return_type() {
        let source = r#"
            PROGRAM Demo
            VAR
                A : INT := 5;
                B : BOOL := FALSE;
            END_VAR
            A := LIMIT(MX := 10.0, MN := 0.0, IN := A);
            B := EQ(IN2 := A, IN1 := LIMIT(MX := 10.0, MN := 0.0, IN := A));
            END_PROGRAM
        "#;
        let output = parse_project("standard_return_formals.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(
            diagnostics.is_empty(),
            "formal input ordering should drive return type inference: {diagnostics:?}"
        );
    }

    #[test]
    fn generic_family_models_table_11_hierarchy() {
        assert!(GenericFamily::Any.contains(SimpleType::Aggregate));
        assert!(GenericFamily::AnyDerived.contains(SimpleType::Enum));
        assert!(GenericFamily::AnyDerived.contains(SimpleType::Aggregate));
        assert!(!GenericFamily::AnyDerived.contains(SimpleType::Integer));
        assert!(GenericFamily::AnyElementary.contains(SimpleType::Integer));
        assert!(GenericFamily::AnyElementary.contains(SimpleType::DateAndTime));
        assert!(!GenericFamily::AnyElementary.contains(SimpleType::Aggregate));
        assert!(GenericFamily::AnyMagnitude.contains(SimpleType::Integer));
        assert!(GenericFamily::AnyMagnitude.contains(SimpleType::Real));
        assert!(GenericFamily::AnyMagnitude.contains(SimpleType::Time));
        assert!(!GenericFamily::AnyMagnitude.contains(SimpleType::Date));
        assert!(GenericFamily::AnyDate.contains(SimpleType::Date));
        assert!(GenericFamily::AnyDate.contains(SimpleType::TimeOfDay));
        assert!(GenericFamily::AnyDate.contains(SimpleType::DateAndTime));
        assert!(!GenericFamily::AnyDate.contains(SimpleType::Time));
        assert!(!GenericFamily::AnyDate.contains(SimpleType::Integer));
        assert_eq!(GenericFamily::AnyDerived.as_str(), "ANY_DERIVED");
        assert_eq!(GenericFamily::AnyDate.as_str(), "ANY_DATE");
    }

    #[test]
    fn checks_string_function_bounds() {
        let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING[8] := '';
            END_VAR
            Text := LEFT('ABC', 4);
            Text := RIGHT('ABC', -1);
            Text := MID('ABC', 2, 3);
            Text := DELETE('ABC', 1, 0);
            Text := INSERT('ABC', 'X', 4);
            Text := REPLACE('ABC', 'X', 2, 3);
            END_PROGRAM
        "#;
        let output = parse_project("string_bounds.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'LEFT' length 4 exceeds string length 3")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'RIGHT' argument 2 must be non-negative, got -1")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'MID' length 2 from position 3 exceeds string length 3")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'DELETE' position 0 is outside string positions 1..3")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'INSERT' insert position 4 is outside range 0..3")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "standard function 'REPLACE' length 2 from position 3 exceeds string length 3"
            )));
    }

    #[test]
    fn checks_bounded_string_constant_expression_lengths() {
        let source = r#"
            PROGRAM Demo
            VAR
                Short : STRING[3] := '';
                Wide : WSTRING[3] := "";
            END_VAR
            Short := CONCAT('ab', 'cd');
            Short := LEFT('abcd', 3);
            Wide := CONCAT("ab", "cd");
            END_PROGRAM
        "#;
        let output = parse_project("string_constant_expression_bounds.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("assignment to 'Short' exceeds string length 3 with 4 character(s)")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("assignment to 'Wide' exceeds string length 3 with 4 character(s)")));
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("LEFT")),
            "{diagnostics:?}"
        );
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
                BadSmallFromFormalMux : Small := MUX(IN1 := 11, K := 1, IN0 := 0);
                BadTextFromFormalLeft : ShortText := LEFT(L := 4, IN := 'abcd');
                GoodTextFromFormalLeft : ShortText := LEFT(L := 3, IN := 'abcd');
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
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
            "initial value for variable 'BadSmallFromFormalMux' value 11 is outside subrange 0..10"
        )
        }));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'BadTextFromFormalLeft' exceeds string length 3"
            )));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("unknown variable 'Run'")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodTextFromFormalLeft")));
    }

    #[test]
    fn validates_nested_derived_alias_initializers_and_access() {
        let source = r#"
            TYPE
                Small : INT(0..10);
                SmallAlias : Small;
                SmallAlias2 : SmallAlias;
                Text3 : STRING[3];
                TextAlias : Text3;
                Row : ARRAY [1..2] OF SmallAlias2;
                RowAlias : Row;
                Holder : STRUCT
                    Values : RowAlias;
                    Label : TextAlias;
                END_STRUCT;
                HolderAlias : Holder;
                Mode : (Idle, Run);
                ModeAlias : Mode;
                ModeAlias2 : ModeAlias;
            END_TYPE

            PROGRAM Demo
            VAR
                GoodHolder : HolderAlias := (Values := [1, 2], Label := 'abc');
                BadSubrange : SmallAlias2 := 11;
                BadText : TextAlias := 'abcd';
                GoodMode : ModeAlias2 := Mode#Run;
                BadMode : ModeAlias2 := 1;
                BadFromFormalMux : SmallAlias2 := MUX(K := 1, IN0 := 0, IN1 := 11);
            END_VAR
            GoodHolder.Values[1] := GoodHolder.Values[2];
            END_PROGRAM
        "#;
        let output = parse_project("nested_derived_aliases.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'BadSubrange' value 11 is outside subrange 0..10"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'BadText' exceeds string length 3")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'BadMode' expects one of: Idle, Run")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'BadFromFormalMux' value 11 is outside subrange 0..10"
            )));
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("GoodHolder.Values")),
            "{diagnostics:?}"
        );
        assert!(
            !diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message.contains("GoodMode")),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn validates_subrange_base_type_and_bounds() {
        let source = r#"
            TYPE
                GoodSint : SINT(-128..127);
                GoodUint : UINT(0..65535);
                BadOrder : INT(10..0);
                BadSint : SINT(-129..127);
                BadUsint : USINT(-1..256);
                BadReal : REAL(0..10);
                BadByte : BYTE(0..10);
            END_TYPE

            PROGRAM Demo
            END_PROGRAM
        "#;
        let output = parse_project("subrange_bounds.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("invalid subrange 10..0")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("subrange -129..127 is outside SINT range -128..127")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("subrange -1..256 is outside USINT range 0..255")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("subrange base type 'REAL' must be an integer type")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("subrange base type 'BYTE' must be an integer type")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodSint")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodUint")));
    }

    #[test]
    fn diagnoses_enum_duplicate_values_and_cross_type_ambiguity() {
        let source = r#"
            TYPE
                ModeA : (Idle, Run);
                ModeB : (Run, Fault);
                BadEnum : (Repeat, Repeat);
            END_TYPE

            PROGRAM Demo
            VAR
                StateA : ModeA := ModeA#Idle;
                StateB : ModeB := ModeB#Fault;
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("enum_ambiguity.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("duplicate enumerated value 'Repeat' in enum type 'BadEnum'")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("enumerated value 'RUN' is declared by multiple enum types: ModeA, ModeB")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("StateA")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("StateB")));
    }

    #[test]
    fn rejects_typed_enum_literals_from_incompatible_enum_types() {
        let source = r#"
            TYPE
                ModeA : (Idle, Run);
                AliasA : ModeA;
                ModeB : (Run, Fault);
            END_TYPE

            PROGRAM Demo
            VAR
                GoodA : ModeA := ModeA#Run;
                GoodAliasA : AliasA := AliasA#Idle;
                GoodAliasBase : AliasA := ModeA#Idle;
                BadA : ModeA := ModeB#Run;
                BadAliasA : AliasA := ModeB#Run;
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("enum_typed_type_mismatch.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'BadA' expects enum type 'ModeA', got typed enum literal 'ModeB#Run'"
        )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic.message.contains(
            "initial value for variable 'BadAliasA' expects enum type 'ModeA', got typed enum literal 'ModeB#Run'"
        )));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodA")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodAliasA")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodAliasBase")));
    }

    #[test]
    fn rejects_zero_length_string_types() {
        let source = r#"
            PROGRAM Demo
            VAR
                EmptyText : STRING[0];
                EmptyWide : WSTRING[0];
                TooLong : STRING[5];
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("zero_strings.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(
            &output.project,
            &CheckOptions {
                implementation: ImplementationParameters {
                    max_string_length: 4,
                    ..ImplementationParameters::default()
                },
                ..CheckOptions::default()
            },
        );
        assert_eq!(
            diagnostics
                .iter()
                .filter(|diagnostic| diagnostic
                    .message
                    .contains("string length must be at least 1"))
                .count(),
            2
        );
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("string length exceeds maximum 4")));
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
                OtherPair : STRUCT
                    Low : Small;
                    Flag : INT;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                GoodArray : ARRAY [1..3] OF Small := [1, 2, 3];
                GoodArrayCopy : ARRAY [1..3] OF Small := [0, 0, 0];
                GoodRepeat : ARRAY [1..5] OF Small := [2(1), 3(2)];
                BadArrayLength : ARRAY [1..3] OF Small := [1, 2];
                BadArrayElement : ARRAY [1..2] OF Small := [1, 11];
                BadRepeatedElement : ARRAY [1..3] OF Small := [2(11), 0];
                BadArrayCopy : ARRAY [1..2] OF Small := [0, 0];
                GoodPair : Pair := (Low := 5, Flag := TRUE);
                GoodPairCopy : Pair := (Low := 0, Flag := FALSE);
                BadPairCopy : OtherPair := (Low := 0, Flag := 0);
                UnknownField : Pair := (Low := 5, Missing := TRUE);
                DuplicateField : Pair := (Low := 5, Low := 6, Flag := TRUE);
                BadFieldType : Pair := (Low := TRUE, Flag := 1);
            END_VAR
            GoodArrayCopy := GoodArray;
            BadArrayCopy := GoodArray;
            GoodPairCopy := GoodPair;
            BadPairCopy := GoodPair;
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
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'BadRepeatedElement' element 1 value 11 is outside subrange 0..10")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("assignment to 'BadArrayCopy' expects a compatible array value")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("assignment to 'BadPairCopy' expects a compatible structure value")));
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
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodArrayCopy")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodRepeat")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodPairCopy")));
    }

    #[test]
    fn checks_array_index_arity_and_constant_bounds() {
        let source = r#"
            TYPE
                Row : ARRAY [2..4] OF INT;
                Matrix : ARRAY [1..2] OF Row;
                Holder : STRUCT
                    Rows : Matrix;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                Values : ARRAY [1..3, 0..1] OF INT;
                Pair : STRUCT
                    Nested : ARRAY [2..4] OF INT;
                END_STRUCT;
                Box : Holder;
                RowCopy : Row;
                Ok : INT := 0;
                BadLow : INT := 0;
                BadHigh : INT := 0;
                BadArity : INT := 0;
                BadNested : INT := 0;
                GoodNested : INT := 0;
                BadNestedOuter : INT := 0;
                BadNestedInner : INT := 0;
            END_VAR

            Ok := Values[1, 0];
            BadLow := Values[0, 0];
            BadHigh := Values[1, 2];
            BadArity := Values[1];
            BadNested := Pair.Nested[5];
            GoodNested := Box.Rows[1][2];
            RowCopy := Box.Rows[2];
            BadNestedOuter := Box.Rows[0][2];
            BadNestedInner := Box.Rows[1][5];
            END_PROGRAM
        "#;
        let output = parse_project("array_indices.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("array index 0 in 'Values[0, 0]' is outside range 1..3")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("array index 2 in 'Values[1, 2]' is outside range 0..1")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("array access 'Values[1]' expects 2 index(es), got 1")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("array index 5 in 'Pair.Nested[5]' is outside range 2..4")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("array index 0 in 'Box.Rows[0, 2]' is outside range 1..2")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("array index 5 in 'Box.Rows[1, 5]' is outside range 2..4")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Ok")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodNested")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("RowCopy")));
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
                IncompleteInput AT %IX* : BOOL;
                IncompleteOutput AT %QW* : INT;
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
                BadWildcard AT %MX1.* : BOOL;
                NotDirect AT Symbolic : INT;
            END_VAR
            %Q.1 := TRUE;
            %IX* := TRUE;
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
            .any(|diagnostic| diagnostic.message.contains("invalid address '1.*'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("must start with '%'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("malformed address '.1'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "incomplete direct variable location '%IX*' is only valid in a declaration"
            )));
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
            (
                "write-to-constant",
                r#"
                PROGRAM BadConstantWrite
                VAR CONSTANT
                    Limit : INT := 5;
                END_VAR
                Limit := 6;
                END_PROGRAM
                "#,
                "cannot assign to CONSTANT variable 'Limit'",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "exit-outside-iteration",
                r#"
                PROGRAM BadExit
                VAR Done : BOOL := FALSE; END_VAR
                IF Done THEN
                    EXIT;
                END_IF;
                END_PROGRAM
                "#,
                "EXIT used outside of an iteration",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "case-label-overlap",
                r#"
                PROGRAM BadCase
                VAR Selected : INT := 0; END_VAR
                CASE Selected OF
                    1, 1: Selected := 2;
                ELSE
                    Selected := 3;
                END_CASE;
                END_PROGRAM
                "#,
                "CASE label range 1 overlaps previous range 1",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "subrange-out-of-base-range",
                r#"
                TYPE
                    BadSmall : SINT(-129..127);
                END_TYPE
                PROGRAM BadSubrange
                VAR Value : BadSmall := 0; END_VAR
                END_PROGRAM
                "#,
                "subrange -129..127 is outside SINT range -128..127",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "unknown-il-label",
                r#"
                PROGRAM BadIl
                VAR A : INT := 0; END_VAR
                JMP Missing;
                END_PROGRAM
                "#,
                "unknown IL label 'Missing'",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "missing-function-return",
                r#"
                FUNCTION Maybe : INT
                VAR_INPUT
                    Flag : BOOL;
                END_VAR
                IF Flag THEN
                    Maybe := 1;
                END_IF;
                END_FUNCTION
                "#,
                "function 'Maybe' does not assign to its return variable on all paths",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "duplicate-enumerated-value",
                r#"
                TYPE
                    BadEnum : (Repeat, Repeat);
                END_TYPE
                PROGRAM BadEnumProgram
                VAR State : BadEnum := Repeat; END_VAR
                END_PROGRAM
                "#,
                "duplicate enumerated value 'Repeat' in enum type 'BadEnum'",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "array-index-bounds",
                r#"
                PROGRAM BadArray
                VAR
                    Values : ARRAY [1..3, 0..1] OF INT;
                    Out : INT := 0;
                END_VAR
                Out := Values[0, 0];
                END_PROGRAM
                "#,
                "array index 0 in 'Values[0, 0]' is outside range 1..3",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "bad-access-path-target",
                r#"
                PROGRAM BadAccess
                VAR
                    Local : INT := 1;
                END_VAR
                VAR_TEMP
                    Scratch : INT;
                END_VAR
                VAR_ACCESS
                    BadType : Local : BOOL READ_ONLY;
                    BadTemp : Scratch : INT READ_ONLY;
                END_VAR
                END_PROGRAM
                "#,
                "access path 'BadType' type does not match target 'Local'",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "standard-function-arity",
                r#"
                PROGRAM BadArity
                VAR A : INT := 0; END_VAR
                A := ABS(1, 2);
                END_PROGRAM
                "#,
                "standard function 'ABS' expects exactly 1 input argument(s), got 2",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "conversion-range",
                r#"
                PROGRAM BadConversion
                VAR A : USINT := 0; END_VAR
                A := INT_TO_USINT(300);
                END_PROGRAM
                "#,
                "conversion 'INT_TO_USINT' value 300 is outside target range 0..255",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "bad-retain-qualifier",
                r#"
                FUNCTION BadFunction : INT
                VAR RETAIN
                    Saved : INT := 0;
                END_VAR
                BadFunction := Saved;
                END_FUNCTION
                "#,
                "FUNCTION 'BadFunction' cannot declare RETAIN variables",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "duplicate-il-label",
                r#"
                PROGRAM BadIlLabel
                VAR A : INT := 0; END_VAR
                Start:
                Start:
                LD A;
                END_PROGRAM
                "#,
                "duplicate IL label 'Start'",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "recursive-function-cycle",
                r#"
                FUNCTION A : INT
                A := B();
                END_FUNCTION
                FUNCTION B : INT
                B := A();
                END_FUNCTION
                "#,
                "recursive function call cycle involving 'A' is not supported",
                "\"stableCode\":\"RBCPP-SEMANTIC\"",
            ),
            (
                "non-variable-var-in-out-actual",
                r#"
                FUNCTION_BLOCK Mutate
                VAR_IN_OUT
                    X : INT;
                END_VAR
                X := X + 1;
                END_FUNCTION_BLOCK

                PROGRAM BadInOut
                VAR Fb : Mutate; END_VAR
                Fb(X := 1);
                END_PROGRAM
                "#,
                "function block 'Mutate' VAR_IN_OUT parameter 'X' requires a variable actual",
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
                Huge : LINT := LINT#9223372036854775807 + DINT#1;
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("constant_overflow.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'Value' value 9223372036854775808 is outside subrange 0..10")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'Huge' value 9223372036854775808 is outside LINT range"
            )));
    }

    #[test]
    fn flags_constant_conversion_target_range_errors() {
        let source = r#"
            PROGRAM Demo
            VAR
                Bad : INT := 0;
                BadByte : BYTE := 300;
                BadSint : SINT := -129;
                BadReal : REAL := REAL#1e39;
                BadLreal : LREAL := LREAL#1e5000;
            END_VAR
            Bad := INT_TO_USINT(300);
            Bad := WORD_BCD_TO_UINT(WORD#16#1A);
            Bad := INT_TO_BCD_BYTE(123);
            Bad := REAL_TO_INT(1);
            Bad := BOOL_TO_INT(1);
            Bad := STRING_TO_INT("wide");
            Bad := WSTRING_TO_INT('narrow');
            Bad := WORD_TO_UINT(1);
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
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'REAL_TO_INT' argument 1 expects ANY_REAL, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'BOOL_TO_INT' argument 1 expects BOOL, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'STRING_TO_INT' argument 1 expects STRING, got WSTRING")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "standard function 'WSTRING_TO_INT' argument 1 expects WSTRING, got STRING"
            )));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "standard function 'WORD_TO_UINT' argument 1 expects bit-string, got integer"
            )));
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
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("typed literal value 1e39 is outside REAL range")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("typed literal 'LREAL#1e5000' is not a valid LREAL value")));
    }

    #[test]
    fn flags_invalid_constant_conversion_inputs() {
        let source = r#"
            PROGRAM Demo
            VAR
                BadInt : INT := 0;
                BadBool : BOOL := FALSE;
                BadReal : REAL := 0.0;
                BadTime : TIME := T#0s;
                BadDate : DATE := D#1970-01-01;
                BadTod : TIME_OF_DAY := TOD#00:00:00;
                BadDt : DATE_AND_TIME := DT#1970-01-01-00:00:00;
            END_VAR

            BadInt := STRING_TO_INT('not-an-int');
            BadBool := STRING_TO_BOOL('maybe');
            BadReal := STRING_TO_REAL('NaN');
            BadTime := STRING_TO_TIME('no-time');
            BadDate := STRING_TO_DATE('2024-02-30');
            BadTod := STRING_TO_TOD('25:00:00');
            BadDt := STRING_TO_DATE_AND_TIME('2024-02-30-00:00:00');
            END_PROGRAM
        "#;
        let output = parse_project("invalid_constant_conversions.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        for expected in [
            "conversion 'STRING_TO_INT' cannot convert constant input to INT",
            "conversion 'STRING_TO_BOOL' cannot convert constant input to BOOL",
            "conversion 'STRING_TO_REAL' produced non-finite REAL from constant input",
            "conversion 'STRING_TO_TIME' cannot convert constant input to TIME",
            "conversion 'STRING_TO_DATE' cannot convert constant input to DATE",
            "conversion 'STRING_TO_TOD' cannot convert constant input to TOD",
            "conversion 'STRING_TO_DATE_AND_TIME' cannot convert constant input to DATE_AND_TIME",
        ] {
            assert!(
                diagnostics
                    .iter()
                    .any(|diagnostic| diagnostic.message.contains(expected)),
                "missing {expected}; diagnostics: {diagnostics:?}"
            );
        }
    }

    #[test]
    fn checks_date_time_conversion_function_families() {
        let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING[40] := '';
                Wide : WSTRING[40] := "";
                Today : DATE := D#1970-01-01;
                Clock : TIME_OF_DAY := TOD#00:00:00;
                Stamp : DATE_AND_TIME := DT#1970-01-01-00:00:00;
            END_VAR

            Today := STRING_TO_DATE('D#1970-01-02');
            Clock := STRING_TO_TOD('TOD#01:02:03.004');
            Stamp := STRING_TO_DT('DT#1970-01-02-01:02:03.004');
            Text := DATE_TO_STRING(Today);
            Wide := TOD_TO_WSTRING(Clock);
            Text := DATE_AND_TIME_TO_STRING(Stamp);
            Text := DATE_TO_STRING(Clock);
            Today := STRING_TO_DATE(T#1s);
            END_PROGRAM
        "#;
        let output = parse_project("date_time_conversions.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "standard function 'DATE_TO_STRING' argument 1 expects DATE, got TIME_OF_DAY"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'STRING_TO_DATE' argument 1 expects STRING, got TIME")));
    }

    #[test]
    fn checks_typed_alias_literal_families_in_standard_calls() {
        let source = r#"
            TYPE
                MyInt : INT;
                MyInt2 : MyInt;
                MyReal : REAL;
                MyReal2 : MyReal;
                MyTod : TIME_OF_DAY;
                MyTod2 : MyTod;
            END_TYPE

            PROGRAM Demo
            VAR
                RealOut : REAL := 0.0;
                Text : STRING[32] := '';
            END_VAR

            RealOut := SIN(MyReal2#1.5);
            RealOut := SIN(MyInt2#1);
            Text := DATE_TO_STRING(MyTod2#01:02:03.004);
            END_PROGRAM
        "#;
        let output = parse_project("typed_alias_literal_families.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'SIN' argument 1 expects ANY_REAL, got integer")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "standard function 'DATE_TO_STRING' argument 1 expects DATE, got TIME_OF_DAY"
            )));
    }

    #[test]
    fn validates_typed_literal_ranges_and_enum_values() {
        let source = r#"
            TYPE
                Small : INT(0..10);
                Mode : (Idle, Run, Fault);
                AliasInt : INT;
                AliasWord : WORD;
                AliasTime : TIME;
                AliasDate : DATE;
                AliasTod : TIME_OF_DAY;
                AliasDt : DATE_AND_TIME;
                AliasBool : BOOL;
                AliasAliasDate : AliasDate;
            END_TYPE

            PROGRAM Demo
            VAR
                BadSmall : Small := Small#11;
                BadMode : Mode := Mode#Missing;
                GoodMode : Mode := Mode#Run;
                BadByte : BYTE := BYTE#16#100;
                BadAliasInt : AliasInt := AliasInt#40000;
                BadAliasWord : AliasWord := AliasWord#16#1_0000;
                BadAliasTime : AliasTime := AliasTime#1m_75s;
                BadAliasDate : AliasDate := AliasDate#2023-02-29;
                BadAliasTod : AliasTod := AliasTod#24:00:00;
                BadAliasDt : AliasDt := AliasDt#2024-02-29-25:00:00;
                BadAliasBool : AliasBool := AliasBool#maybe;
                BadNestedAliasDate : AliasAliasDate := AliasAliasDate#2023-02-29;
                BadUnknownType : INT := MissingType#1;
                GoodAliasTime : AliasTime := AliasTime#1m_30s;
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("typed_literals.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("value 11 is outside subrange 0..10")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("typed enum literal 'Mode#Missing' is not a value of 'Mode'")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("value 256 is outside BYTE range 0..255")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("value 40000 is outside INT range -32768..32767")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("value 65536 is outside WORD range 0..65535")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("typed literal 'AliasTime#1m_75s' is not a valid TIME value")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("typed literal 'AliasDate#2023-02-29' is not a valid DATE value")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("typed literal 'AliasTod#24:00:00' is not a valid TIME_OF_DAY value")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "typed literal 'AliasDt#2024-02-29-25:00:00' is not a valid DATE_AND_TIME value"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("typed literal 'AliasBool#maybe' is not a valid BOOL value")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("typed literal 'AliasAliasDate#2023-02-29' is not a valid DATE value")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("unknown typed literal type 'MissingType'")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodAliasTime")));
    }

    #[test]
    fn distinguishes_string_and_wstring_assignments() {
        let source = r#"
            PROGRAM TextTypes
            VAR
                Narrow : STRING[8] := "wide";
                Wide : WSTRING[8] := 'narrow';
                GoodWide : WSTRING[8] := "ok";
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("text_types.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'Narrow' expects STRING, got WSTRING")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'Wide' expects WSTRING, got STRING")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'GoodWide'")));
    }

    #[test]
    fn validates_textual_sfc_elements() {
        let valid = r#"
            PROGRAM Sequence
            VAR
                Ready : BOOL := TRUE;
                Done : BOOL := FALSE;
            END_VAR
            INITIAL_STEP Start:
                MarkDone(N);
            END_STEP;
            STEP Run;
            Go: TRANSITION FROM Start TO Run := Ready;
            END_TRANSITION;
            MarkDone: ACTION
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
            STEP Other:
                Unknown(D);
                Delay(L, T#1ms);
                Delay(D, T#2ms);
            END_STEP;
            TRANSITION Go FROM Missing TO Done := Ready;
            END_TRANSITION;
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
            .contains("SFC transition references unknown FROM step 'Missing'")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SFC transition references unknown TO step 'Done'")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("duplicate SFC action 'MarkDone'")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SFC action 'Delay' qualifier D requires a duration")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("SFC step 'Other' references unknown action 'Unknown'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "SFC step 'Other' action association 'Unknown' qualifier D requires a duration"
            )));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "SFC step 'Other' has more than one time-related association for action 'Delay'"
            )));
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
    fn validates_var_external_against_global_declarations() {
        let source = r#"
            PROGRAM Globals
            VAR_GLOBAL
                Shared : INT := 1;
                Flag : BOOL := TRUE;
            END_VAR
            VAR_GLOBAL CONSTANT
                ConstShared : INT := 2;
            END_VAR
            END_PROGRAM

            PROGRAM Main
            VAR_EXTERNAL
                Shared : INT;
                Flag : INT;
                Missing : INT;
                ConstShared : INT;
            END_VAR
            VAR_EXTERNAL CONSTANT
                Shared : INT;
            END_VAR
            VAR
                Local : INT := 0;
            END_VAR
            Local := Shared + Missing;
            END_PROGRAM
        "#;
        let output = parse_project("var_external_semantics.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("VAR_EXTERNAL variable 'Flag' type does not match")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("VAR_EXTERNAL variable 'Missing' has no matching VAR_GLOBAL")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("VAR_EXTERNAL variable 'ConstShared' must be declared CONSTANT")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("VAR_EXTERNAL variable 'Shared' cannot be declared CONSTANT")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("duplicate variable 'Shared'")));
    }

    #[test]
    fn validates_function_block_input_edge_qualifiers() {
        let source = r#"
            FUNCTION_BLOCK EdgeOk
            VAR_INPUT
                Start : BOOL R_EDGE;
                Stop : BOOL F_EDGE;
            END_VAR
            END_FUNCTION_BLOCK

            FUNCTION BadFunction : BOOL
            VAR_INPUT
                Start : BOOL R_EDGE;
            END_VAR
            BadFunction := Start;
            END_FUNCTION

            FUNCTION_BLOCK BadType
            VAR_INPUT
                Count : INT R_EDGE;
            END_VAR
            END_FUNCTION_BLOCK
        "#;
        let output = parse_project("edge_qualifiers_semantics.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| {
            diagnostic.message.contains(
            "R_EDGE edge qualifier on variable 'Start' is only valid on FUNCTION_BLOCK VAR_INPUT"
        )
        }));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("R_EDGE edge qualifier on variable 'Count' requires BOOL")));
        assert!(!diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("edge qualifier on variable 'Stop'")));
    }

    #[test]
    fn checks_program_access_paths() {
        let source = r#"
            TYPE
                Pair : STRUCT
                    Low : INT;
                    Flag : BOOL;
                END_STRUCT;
            END_TYPE

            PROGRAM AccessDemo
            VAR
                Local : INT := 1;
                Counter : CTU;
                PairValue : Pair;
            END_VAR
            VAR_TEMP
                Scratch : INT;
            END_VAR
            VAR_ACCESS
                GoodLocal : Local : INT READ_WRITE;
                GoodFbField : Counter.CV : INT READ_ONLY;
                GoodStructField : PairValue.Flag : BOOL READ_ONLY;
                GoodDirect : %IX1.1 : BOOL READ_ONLY;
                BadType : Local : BOOL READ_ONLY;
                BadNested : PairValue.Missing : INT READ_ONLY;
                BadTemp : Scratch : INT READ_ONLY;
            END_VAR
            END_PROGRAM
        "#;
        let output = parse_project("access_paths.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("access path 'BadType' type does not match target 'Local'")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("access path 'BadNested' references unknown target 'PairValue.Missing'")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("access path 'BadTemp' cannot target VAR_TEMP")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodLocal")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodDirect")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodFbField")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodStructField")));
    }

    #[test]
    fn checks_function_block_positional_and_named_duplicate_inputs() {
        let source = r#"
            FUNCTION_BLOCK Capture
            VAR_INPUT
                X : INT;
                Y : INT;
            END_VAR
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Capture;
            END_VAR

            Fb(1, X := 2);
            Fb(1, 2, Y := 3);
            END_PROGRAM
        "#;
        let output = parse_project("fb_duplicate_inputs.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function block 'Capture' input parameter 'X' is bound more than once")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function block 'Capture' input parameter 'Y' is bound more than once")));
    }

    #[test]
    fn checks_function_block_en_eno_controls() {
        let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                BadEno : INT := 0;
                GoodEno : BOOL := FALSE;
            END_VAR

            Counter(EN := 1, CU := TRUE, R := FALSE, PV := 1);
            Counter(EN := TRUE, CU := TRUE, R := FALSE, PV := 1, ENO => BadEno);
            Counter(EN := TRUE, CU := TRUE, R := FALSE, PV := 1, ENO => GoodEno);
            END_PROGRAM
        "#;
        let output = parse_project("fb_controls_semantics.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function block EN input expects BOOL")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("function block 'Counter' ENO expects BOOL output")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodEno")));
    }

    #[test]
    fn checks_standard_function_block_parameter_bindings() {
        let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                Flag : BOOL := FALSE;
                Count : INT := 0;
                BadCount : BOOL := FALSE;
            END_VAR

            Counter(CU := TRUE, R := FALSE, PV := 1, Q => Flag, CV => Count);
            Counter(CU := TRUE, R := FALSE, PV := 1, Missing => Flag);
            Counter(CU := TRUE, R := FALSE, PV := 1, CV => BadCount);
            Counter(CU := TRUE, R := FALSE, PV := 1, NOT CV => Count);
            Counter(CU := TRUE, R := FALSE, PV := 1, Q => Flag, Q => Flag);
            Counter(CU := TRUE, BadInput := FALSE, PV := 1);
            END_PROGRAM
        "#;
        let output = parse_project("standard_fb_bindings_semantics.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function block 'CTU' has no output parameter 'Missing'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "standard function block 'CTU' output parameter 'CV' expects BOOL, got integer"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function block 'CTU' output parameter 'CV' cannot be negated")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "standard function block 'CTU' output parameter 'Q' is bound more than once"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function block 'CTU' has no input parameter 'BadInput'")));
    }

    #[test]
    fn checks_instruction_list_labels() {
        let source = r#"
            PROGRAM BadIl
            VAR
                A : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR
            JMP Missing;
            Start:
            Start:
            LD A;
            ST 1;
            STN A;
            S A;
            R Flag;
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
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("IL ST instruction requires a variable operand")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("IL STN target expects BOOL, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("IL S target expects BOOL, got integer")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("IL R target expects BOOL")));
    }

    #[test]
    fn checks_configuration_program_and_task_references() {
        let source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 0;
            END_VAR
            END_PROGRAM

            CONFIGURATION Plant
            RESOURCE Cpu ON PLC
                TASK Fast(INTERVAL := T#10ms, PRIORITY := 1);
                PROGRAM Main WITH Fast : Demo(Count := 5);
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("config.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    }

    #[test]
    fn checks_configuration_variable_initializers() {
        let source = r#"
            TYPE
                Small : INT(0..10);
            END_TYPE

            CONFIGURATION Plant
            VAR_GLOBAL
                BadGlobal : Small := 11;
                BadBool : BOOL := 1;
            END_VAR
            RESOURCE Cpu ON PLC
                VAR_CONFIG
                    BadResource : Small := 12;
                END_VAR
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("config_initializers.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'BadGlobal' value 11 is outside subrange 0..10"
            )));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "initial value for variable 'BadResource' value 12 is outside subrange 0..10"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("initial value for variable 'BadBool' expects BOOL, got integer")));
    }

    #[test]
    fn checks_configuration_program_instance_initializers() {
        let source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR
            VAR_OUTPUT
                OutCount : INT := 0;
            END_VAR
            VAR_TEMP
                Scratch : INT := 0;
            END_VAR
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                Observed : INT := 0;
                WrongFlag : BOOL := FALSE;
            END_VAR
            RESOURCE Cpu ON PLC
                PROGRAM Good : Demo(Count := ADD(2, 3), Flag := TRUE);
                PROGRAM GoodOutput : Demo(OutCount => Observed);
                PROGRAM BadUnknown : Demo(Missing := 1);
                PROGRAM BadType : Demo(Flag := 1);
                PROGRAM BadTemp : Demo(Scratch := 1);
                PROGRAM BadDuplicate : Demo(Count := 1, Count := 2);
                PROGRAM BadDynamic : Demo(Count := MissingConfig);
                PROGRAM BadOutputKind : Demo(Count => Observed);
                PROGRAM BadOutputType : Demo(OutCount => WrongFlag);
                PROGRAM BadOutputUnknown : Demo(OutCount => MissingTarget);
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("config_program_instance_initializers.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "program instance 'BadUnknown' references unknown PROGRAM variable 'Missing'"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("program instance 'BadType' parameter 'Flag' expects BOOL, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("program instance 'BadTemp' cannot initialize VAR_TEMP variable 'Scratch'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "program instance 'BadDuplicate' initializes parameter 'Count' more than once"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains(
                "program instance 'BadOutputKind' output binding 'Count' must reference a VAR_OUTPUT variable"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains(
                "program instance 'BadOutputType' output binding 'OutCount' expects integer target, got BOOL"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains(
                "program instance 'BadOutputUnknown' output binding 'OutCount' references unknown target 'MissingTarget'"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("unknown variable 'MissingConfig'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "program instance 'BadDynamic' parameter 'Count' must be a constant expression"
            )));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("Good")));
        assert!(!diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("GoodOutput")));
    }

    #[test]
    fn checks_configuration_single_task_expressions() {
        let source = r#"
            PROGRAM Demo
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                Trigger : BOOL;
                Count : INT;
            END_VAR
            RESOURCE Cpu ON PLC
                TASK Good(SINGLE := Trigger, PRIORITY := 1);
                TASK BadType(SINGLE := Count, PRIORITY := 2);
                TASK BadUnknown(SINGLE := MissingTrigger, PRIORITY := 3);
                TASK BadInterval(INTERVAL := Trigger, PRIORITY := 4);
                TASK BadPriority(PRIORITY := Trigger);
                TASK BadPriorityNegative(PRIORITY := -1);
                PROGRAM Main WITH Good : Demo;
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("config_single_task.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("task 'BadType' SINGLE expects BOOL, got integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("unknown variable 'MissingTrigger'")));
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "task 'BadInterval' INTERVAL expects TIME duration or integer milliseconds"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("task 'BadPriority' PRIORITY expects integer")));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("task 'BadPriorityNegative' PRIORITY must be non-negative")));
    }

    #[test]
    fn checks_nested_user_function_block_fields() {
        let source = r#"
            FUNCTION_BLOCK Accumulator
            VAR_INPUT
                In : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            Total := Total + In;
            END_FUNCTION_BLOCK

            FUNCTION_BLOCK Wrapper
            VAR_INPUT
                In : INT;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            VAR
                Inner : Accumulator;
            END_VAR
            Inner(In := In, Total => Total);
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Fb : Wrapper;
                Out : INT := 0;
                Mirror : INT := 0;
            END_VAR
            Fb(In := 2, Total => Out);
            Mirror := Fb.Inner.Total;
            END_PROGRAM
        "#;
        let output = parse_project("nested_user_fb_semantics.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
    }

    #[test]
    fn checks_configuration_access_paths() {
        let source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 0;
                Counter : CTU;
            END_VAR
            END_PROGRAM

            CONFIGURATION Plant
            VAR_GLOBAL
                ConfigValue : INT;
            END_VAR
            VAR_ACCESS
                ConfigAccess : ConfigValue : INT READ_ONLY;
                ProgramAccess : Cpu.Main.Count : INT READ_ONLY;
                ProgramFbAccess : Cpu.Main.Counter.CV : INT READ_ONLY;
                BadProgramAccess : Cpu.Main.Missing : INT READ_ONLY;
            END_VAR
            RESOURCE Cpu ON PLC
                VAR_GLOBAL
                    ResourceFlag : BOOL;
                END_VAR
                VAR_ACCESS
                    ResourceAccess : ResourceFlag : BOOL READ_ONLY;
                    LocalProgramAccess : Main.Count : INT READ_ONLY;
                    BadResourceAccess : Main.Count : BOOL READ_ONLY;
                END_VAR
                PROGRAM Main : Demo;
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("config_access_paths.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains(
                "access path 'BadProgramAccess' references unknown target 'Cpu.Main.Missing'"
            )));
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("access path 'BadResourceAccess' type does not match target 'Main.Count'")));
        for good in [
            "ConfigAccess",
            "ProgramAccess",
            "ProgramFbAccess",
            "ResourceAccess",
            "LocalProgramAccess",
        ] {
            assert!(
                !diagnostics.iter().any(|diagnostic| diagnostic
                    .message
                    .contains(&format!("access path '{good}'"))),
                "{good} should not produce diagnostics: {diagnostics:?}"
            );
        }
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
