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
    pub(crate) fn check_configuration(&mut self, project: &Project, configuration: &Configuration) {
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

    pub(crate) fn check_program_instance_args(
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

    pub(crate) fn check_configuration_var_blocks(
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

    pub(crate) fn configuration_task_variables(
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

    pub(crate) fn check_configuration_access_declaration(
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
