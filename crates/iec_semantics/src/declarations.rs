// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_profile::EditionProfile;
use iec_stdlib::{
    is_communication_function_block, is_standard_function, is_standard_function_block,
    is_standard_void_function, standard_function_input_index, standard_symbols, StandardSymbolKind,
};

use crate::support::*;
use crate::Checker;

impl Checker {
    pub(crate) fn check(&mut self, project: &Project) {
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

        self.check_project_limits(project);
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

    pub(crate) fn check_project_limits(&mut self, project: &Project) {
        let pou_count = project.pous().count();
        if pou_count > self.options.implementation.max_pous {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Compliance,
                format!(
                    "POU count {pou_count} exceeds maximum {}",
                    self.options.implementation.max_pous
                ),
                None,
            ));
        }

        let variable_count = count_project_variables(project);
        if variable_count > self.options.implementation.max_variables {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Compliance,
                format!(
                    "variable declaration count {variable_count} exceeds maximum {}",
                    self.options.implementation.max_variables
                ),
                None,
            ));
        }

        let symbol_count = count_project_symbols(project);
        if symbol_count > self.options.implementation.max_symbols {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Compliance,
                format!(
                    "named symbol count {symbol_count} exceeds maximum {}",
                    self.options.implementation.max_symbols
                ),
                None,
            ));
        }
    }

    pub(crate) fn check_library_duplicates(&mut self, project: &Project) {
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

    pub(crate) fn check_global_variable_duplicates(&mut self, project: &Project) {
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

    pub(crate) fn check_function_recursion(&mut self, project: &Project) {
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

    pub(crate) fn check_type_declarations(&mut self, project: &Project) {
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

    pub(crate) fn check_enum_declaration(
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

    pub(crate) fn check_pou(&mut self, project: &Project, pou: &Pou) {
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

    pub(crate) fn project_global_variables(
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

    pub(crate) fn project_global_constant_variables(
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

    pub(crate) fn check_var_block_qualifiers(&mut self, pou: &Pou, block: &VarBlock) {
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

    pub(crate) fn check_identifier_profile(&mut self, identifier: &Identifier, context: &str) {
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

    pub(crate) fn check_statement_limits(&mut self, statement: &Statement, depth: usize) {
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

    pub(crate) fn check_expr_limit(&mut self, expr: &Expr, context: &str) {
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

    pub(crate) fn known_types(&self, project: &Project) -> BTreeSet<String> {
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

    pub(crate) fn check_type_spec(&mut self, spec: &DataTypeSpec, known_types: &BTreeSet<String>) {
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
}
