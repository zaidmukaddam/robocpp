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
    pub(crate) fn variable_type(
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

    pub(crate) fn apply_indices_to_type(
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

    pub(crate) fn resolve_named_spec(
        &self,
        spec: &DataTypeSpec,
        project: &Project,
    ) -> DataTypeSpec {
        self.resolve_named_spec_inner(spec, project, &mut BTreeSet::new())
    }

    pub(crate) fn resolve_named_spec_inner(
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

    pub(crate) fn type_of_expr(
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

    pub(crate) fn type_of_spec(&self, spec: &DataTypeSpec, project: &Project) -> SimpleType {
        self.type_of_spec_inner(spec, project, &mut BTreeSet::new())
    }

    pub(crate) fn type_of_spec_inner(
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

    pub(crate) fn check_variable(
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

    pub(crate) fn check_array_index_constraints(
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

    pub(crate) fn check_array_indices_for_spec(
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

    pub(crate) fn check_declared_direct_variable_location(&mut self, location: &str) {
        if let Some(message) = validate_direct_variable_location(location, true) {
            self.diagnostics
                .push(Diagnostic::error(DiagnosticCode::Semantic, message, None));
        }
    }

    pub(crate) fn check_direct_variable_reference(&mut self, location: &str) {
        if let Some(message) = validate_direct_variable_location(location, false) {
            self.diagnostics
                .push(Diagnostic::error(DiagnosticCode::Semantic, message, None));
        }
    }

    pub(crate) fn check_access_declaration(
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

    pub(crate) fn check_access_type_matches(
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
}
