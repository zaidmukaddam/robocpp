// SPDX-License-Identifier: MIT OR Apache-2.0

#![allow(unused_imports)]

use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_stdlib::{
    eval_standard_function, is_communication_function_block, is_standard_function,
    is_standard_void_function, standard_function_input_index,
};

use crate::configuration::*;
use crate::state::*;
use crate::support::*;
use crate::*;

pub(crate) struct ScheduledProgram<'a> {
    pub(crate) resource: String,
    pub(crate) instance: String,
    pub(crate) task: Option<String>,
    pub(crate) priority: u32,
    pub(crate) interval_ms: Option<i128>,
    pub(crate) single: Option<Expr>,
    pub(crate) single_previous: bool,
    pub(crate) output_bindings: Vec<ProgramOutputBinding>,
    pub(crate) runtime: Runtime<'a>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProgramOutputBinding {
    pub(crate) formal: Identifier,
    pub(crate) target: VariableRef,
}

#[derive(Debug, Clone)]
pub(crate) struct ProgramOutputWrite {
    pub(crate) resource: String,
    pub(crate) instance: String,
    pub(crate) formal: Identifier,
    pub(crate) target: VariableRef,
    pub(crate) value: Value,
}

pub(crate) struct Runtime<'a> {
    pub(crate) project: &'a Project,
    pub(crate) program: &'a Pou,
    pub(crate) env: BTreeMap<String, Value>,
    pub(crate) types: BTreeMap<String, DataTypeSpec>,
    pub(crate) il_accumulator: Value,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) options: RuntimeOptions,
    pub(crate) call_depth: usize,
    pub(crate) communication: &'a dyn CommunicationHooks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartupKind {
    Cold,
    Warm,
}

impl Runtime<'_> {
    pub(crate) fn initialize(&mut self, startup: StartupKind) {
        if startup == StartupKind::Cold {
            self.env.clear();
            self.types.clear();
        }
        self.il_accumulator = Value::Unit;

        if let PouKind::Function { return_type } = &self.program.kind {
            let return_type = return_type.clone();
            self.types
                .insert(self.program.name.canonical.clone(), return_type.clone());
            if startup == StartupKind::Cold || !self.env.contains_key(&self.program.name.canonical)
            {
                let value = self.default_value(&return_type);
                self.env.insert(self.program.name.canonical.clone(), value);
            }
        }

        for (var, retain) in project_global_var_decls(self.project, &self.program.name.canonical) {
            self.types
                .insert(var.name.canonical.clone(), var.type_spec.clone());
            let preserve = startup == StartupKind::Warm
                && retain == Some(RetainKind::Retain)
                && self.env.contains_key(&var.name.canonical);
            if preserve {
                continue;
            }
            let value = self.initial_value_for_spec(&var.type_spec, var.initial_value.as_ref());
            self.env.insert(var.name.canonical.clone(), value);
            self.initialize_function_block_fields(var);
        }

        for block in &self.program.var_blocks {
            if matches!(block.kind, VarBlockKind::Access | VarBlockKind::External) {
                continue;
            }
            for var in &block.vars {
                self.types
                    .insert(var.name.canonical.clone(), var.type_spec.clone());
                let preserve = startup == StartupKind::Warm
                    && block.retain == Some(RetainKind::Retain)
                    && self.env.contains_key(&var.name.canonical);
                if preserve {
                    continue;
                }

                let value = self.initial_value_for_spec(&var.type_spec, var.initial_value.as_ref());
                self.env.insert(var.name.canonical.clone(), value);
                self.initialize_function_block_fields(var);
            }
        }
        if startup == StartupKind::Cold {
            self.initialize_sfc_steps();
        }
    }

    pub(crate) fn reset_temp_variables(&mut self) {
        let temp_vars = self
            .program
            .var_blocks
            .iter()
            .filter(|block| block.kind == VarBlockKind::Temp)
            .flat_map(|block| block.vars.iter().cloned())
            .collect::<Vec<_>>();
        for var in temp_vars {
            let value = self.initial_value_for_spec(&var.type_spec, var.initial_value.as_ref());
            self.env.insert(var.name.canonical.clone(), value);
            self.initialize_function_block_fields(&var);
        }
    }

    pub(crate) fn initialize_sfc_steps(&mut self) {
        let Some(sfc) = &self.program.body.sfc else {
            return;
        };
        for step in &sfc.steps {
            self.env
                .insert(sfc_step_key(&step.name), Value::Bool(step.initial));
        }
        for action in &sfc.actions {
            self.initialize_sfc_action_control(&sfc_action_control_key(&action.name));
        }
    }

    pub(crate) fn initialize_sfc_action_control(&mut self, key: &str) {
        self.env
            .insert(sfc_action_control_key_stored(key), Value::Bool(false));
        self.env
            .insert(sfc_action_control_key_previous(key), Value::Bool(false));
        self.env
            .insert(sfc_action_control_key_elapsed(key), Value::Int(0));
    }

    pub(crate) fn initial_value_for_spec(
        &mut self,
        spec: &DataTypeSpec,
        initial: Option<&Expr>,
    ) -> Value {
        match (self.resolve_named_spec(spec), initial) {
            (
                DataTypeSpec::Array {
                    ranges: _,
                    element_type,
                },
                Some(Expr::ArrayLiteral(elements)),
            ) => Value::Array(
                elements
                    .iter()
                    .map(|expr| self.initial_value_for_spec(&element_type, Some(expr)))
                    .collect::<Vec<_>>(),
            ),
            (
                DataTypeSpec::Array {
                    ranges,
                    element_type,
                },
                _,
            ) => Value::Array(
                (0..array_element_count(&ranges))
                    .map(|_| self.initial_value_for_spec(&element_type, None))
                    .collect(),
            ),
            (DataTypeSpec::Struct { fields }, Some(Expr::StructLiteral(initializers))) => {
                let mut values = BTreeMap::new();
                for field in fields {
                    let initializer = initializers.iter().find(|initializer| {
                        initializer
                            .name
                            .as_ref()
                            .is_some_and(|name| name.canonical == field.name.canonical)
                    });
                    let value = initializer
                        .and_then(|initializer| initializer.expr.as_ref())
                        .or(field.initial_value.as_ref())
                        .map(|expr| self.initial_value_for_spec(&field.spec, Some(expr)))
                        .unwrap_or_else(|| self.initial_value_for_spec(&field.spec, None));
                    values.insert(field.name.canonical.clone(), value);
                }
                Value::Struct(values)
            }
            (DataTypeSpec::Struct { fields }, _) => {
                let mut values = BTreeMap::new();
                for field in fields {
                    let value = field
                        .initial_value
                        .as_ref()
                        .map(|expr| self.initial_value_for_spec(&field.spec, Some(expr)))
                        .unwrap_or_else(|| self.initial_value_for_spec(&field.spec, None));
                    values.insert(field.name.canonical.clone(), value);
                }
                Value::Struct(values)
            }
            (DataTypeSpec::Enum { values: _ }, Some(expr)) => self
                .enum_ordinal_expr(expr)
                .map(Value::Int)
                .unwrap_or_else(|| self.eval_expr(expr).unwrap_or(Value::Int(0))),
            (DataTypeSpec::Enum { .. }, None) => Value::Int(0),
            (DataTypeSpec::Subrange { range, .. }, Some(expr)) => {
                let value = self.eval_expr(expr).unwrap_or(Value::Int(0));
                self.constrain_value(
                    &DataTypeSpec::Subrange {
                        base: ElementaryType::Int,
                        range,
                    },
                    value,
                )
            }
            (resolved, Some(expr)) => {
                let value = self
                    .eval_expr(expr)
                    .unwrap_or_else(|| self.default_value(&resolved));
                self.constrain_value(&resolved, value)
            }
            (resolved, None) => self.default_value(&resolved),
        }
    }

    pub(crate) fn default_value(&mut self, spec: &DataTypeSpec) -> Value {
        match self.resolve_named_spec(spec) {
            DataTypeSpec::Array {
                ranges,
                element_type,
            } => Value::Array(
                (0..array_element_count(&ranges))
                    .map(|_| self.default_value(&element_type))
                    .collect(),
            ),
            DataTypeSpec::Struct { fields } => {
                let mut values = BTreeMap::new();
                for field in fields {
                    let value = field
                        .initial_value
                        .as_ref()
                        .map(|expr| self.initial_value_for_spec(&field.spec, Some(expr)))
                        .unwrap_or_else(|| self.default_value(&field.spec));
                    values.insert(field.name.canonical.clone(), value);
                }
                Value::Struct(values)
            }
            DataTypeSpec::Enum { .. } => Value::Int(0),
            DataTypeSpec::Subrange { range, .. } => {
                if range.low <= 0 && range.high >= 0 {
                    Value::Int(0)
                } else {
                    Value::Int(range.low)
                }
            }
            resolved => default_value_for_type(&resolved),
        }
    }

    pub(crate) fn resolve_named_spec(&self, spec: &DataTypeSpec) -> DataTypeSpec {
        resolve_project_spec(self.project, spec)
    }

    pub(crate) fn initialize_function_block_fields(&mut self, var: &VarDecl) {
        self.initialize_function_block_instance(&var.name.canonical, &var.type_spec);
    }

    pub(crate) fn initialize_function_block_instance(
        &mut self,
        instance: &str,
        spec: &DataTypeSpec,
    ) {
        let DataTypeSpec::Named(type_name) = spec else {
            return;
        };

        match type_name.canonical.as_str() {
            "SR" | "RS" => {
                self.set_field(instance, "Q1", Value::Bool(false));
            }
            "R_TRIG" | "F_TRIG" => {
                self.set_field(instance, "Q", Value::Bool(false));
                self.set_field(instance, "M", Value::Bool(false));
            }
            "CTU" => {
                self.set_field(instance, "Q", Value::Bool(false));
                self.set_field(instance, "CV", Value::Int(0));
                self.set_field(instance, "_CU", Value::Bool(false));
            }
            "CTD" => {
                self.set_field(instance, "Q", Value::Bool(false));
                self.set_field(instance, "CV", Value::Int(0));
                self.set_field(instance, "_CD", Value::Bool(false));
            }
            "CTUD" => {
                self.set_field(instance, "QU", Value::Bool(false));
                self.set_field(instance, "QD", Value::Bool(false));
                self.set_field(instance, "CV", Value::Int(0));
                self.set_field(instance, "_CU", Value::Bool(false));
                self.set_field(instance, "_CD", Value::Bool(false));
            }
            "TON" | "TOF" | "TP" => {
                self.set_field(instance, "Q", Value::Bool(false));
                self.set_field(instance, "ET", Value::TimeMs(0));
                self.set_field(instance, "_IN", Value::Bool(false));
                self.set_field(instance, "_RUN", Value::Bool(false));
            }
            name if is_communication_function_block(name) => {
                self.initialize_communication_function_block_fields(instance);
            }
            _ => self.initialize_user_function_block_fields(instance, type_name),
        }
    }

    pub(crate) fn initialize_communication_function_block_fields(&mut self, instance: &str) {
        self.set_field(instance, "DONE", Value::Bool(false));
        self.set_field(instance, "NDR", Value::Bool(false));
        self.set_field(instance, "ERROR", Value::Bool(false));
        self.set_field(instance, "STATUS", Value::Int(0));
    }

    pub(crate) fn initialize_user_function_block_fields(
        &mut self,
        instance: &str,
        type_name: &Identifier,
    ) {
        let Some(function_block) = self
            .project
            .find_pou(&type_name.original)
            .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
        else {
            return;
        };

        for field in function_block.variable_declarations() {
            if function_block_field_specs(self.project, &field.type_spec).is_some() {
                self.initialize_function_block_instance(
                    &field_key(instance, &field.name.canonical),
                    &field.type_spec,
                );
            } else {
                let value =
                    self.initial_value_for_spec(&field.type_spec, field.initial_value.as_ref());
                self.set_field(instance, &field.name.canonical, value);
            }
            if field.edge.is_some() {
                self.set_field(
                    instance,
                    &edge_state_field_name(&field.name.canonical),
                    Value::Bool(false),
                );
            }
        }
    }

    pub(crate) fn snapshot(&self) -> Vec<(String, Value)> {
        self.env
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    pub(crate) fn sync_direct_state(&mut self, direct_state: &BTreeMap<String, Value>) {
        for (location, value) in direct_state {
            self.env.insert(location.clone(), value.clone());
        }
    }

    pub(crate) fn export_direct_state(&self, direct_state: &mut BTreeMap<String, Value>) {
        for (location, value) in self
            .env
            .iter()
            .filter(|(location, _)| is_direct_location_key(location))
        {
            direct_state.insert(location.clone(), value.clone());
        }
    }

    pub(crate) fn access_snapshot(&mut self) -> Vec<AccessPathTrace> {
        let declarations = access_declarations(&self.program.var_blocks);
        declarations
            .into_iter()
            .map(|declaration| AccessPathTrace {
                value: self.access_path_value(&declaration.target),
                name: declaration.name,
                target: declaration.target,
                direction: declaration.direction,
            })
            .collect()
    }

    pub(crate) fn access_path_value(&mut self, target: &str) -> Option<Value> {
        let variable = variable_ref_from_access_path(target)?;
        self.resolve(&variable)
    }

    pub(crate) fn apply_access_writes(&mut self, cycle: usize, writes: &[AccessPathWrite]) {
        let declarations = access_declarations(&self.program.var_blocks);
        for write in writes.iter().filter(|write| write.cycle == cycle) {
            let Some(declaration) = declarations.iter().find(|declaration| {
                canonical_identifier(&declaration.name) == canonical_identifier(&write.name)
            }) else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!("unknown VAR_ACCESS path '{}'", write.name),
                    None,
                ));
                continue;
            };
            if declaration.direction != AccessDirection::ReadWrite {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!("VAR_ACCESS path '{}' is READ_ONLY", declaration.name),
                    None,
                ));
                continue;
            }
            self.assign_access_target(&declaration.name, &declaration.target, write.value.clone());
        }
    }

    pub(crate) fn assign_access_target(
        &mut self,
        access_name: &str,
        target: &str,
        value: Value,
    ) -> bool {
        let Some(variable) = variable_ref_from_access_path(target) else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("VAR_ACCESS path '{access_name}' has invalid target '{target}'"),
                None,
            ));
            return false;
        };
        if let Some(spec) = self.variable_spec(&variable) {
            if !self.runtime_value_matches_spec(&value, &spec) {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!(
                        "VAR_ACCESS path '{access_name}' expects {}, got {}",
                        runtime_spec_label(&self.resolve_named_spec(&spec)),
                        runtime_value_label(&value)
                    ),
                    None,
                ));
                return false;
            }
        }
        self.assign(&variable, value);
        true
    }

    pub(crate) fn variable_spec(&self, variable: &VariableRef) -> Option<DataTypeSpec> {
        if variable.direct.is_some() {
            return None;
        }
        let root = variable.root_name()?;
        let mut spec = self.types.get(&root.canonical).cloned()?;
        for segment in variable.path.iter().skip(1) {
            let resolved = self.resolve_named_spec(&spec);
            let DataTypeSpec::Struct { fields } = resolved else {
                return None;
            };
            spec = fields
                .iter()
                .find(|field| field.name.canonical == segment.canonical)
                .map(|field| field.spec.clone())?;
        }
        Some(spec)
    }

    pub(crate) fn runtime_value_matches_spec(&self, value: &Value, spec: &DataTypeSpec) -> bool {
        match self.resolve_named_spec(spec) {
            DataTypeSpec::Elementary(elementary) => match elementary {
                ElementaryType::Bool => matches!(value, Value::Bool(_)),
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
                | ElementaryType::Lword => matches!(value, Value::Int(_)),
                ElementaryType::Real | ElementaryType::Lreal => {
                    matches!(value, Value::Int(_) | Value::Real(_))
                }
                ElementaryType::String => matches!(value, Value::String(_)),
                ElementaryType::WString => matches!(value, Value::WString(_)),
                ElementaryType::Time
                | ElementaryType::Date
                | ElementaryType::TimeOfDay
                | ElementaryType::DateAndTime => matches!(value, Value::TimeMs(_)),
            },
            DataTypeSpec::String { wide, .. } => {
                matches!(
                    (wide, value),
                    (false, Value::String(_)) | (true, Value::WString(_))
                )
            }
            DataTypeSpec::Subrange { range, .. } => {
                let Some(value) = value.as_i64() else {
                    return false;
                };
                value >= range.low && value <= range.high
            }
            DataTypeSpec::Enum { .. } => matches!(value, Value::Int(_)),
            DataTypeSpec::Array {
                ranges,
                element_type,
            } => {
                let Value::Array(values) = value else {
                    return false;
                };
                values.len() == array_element_count(&ranges)
                    && values
                        .iter()
                        .all(|value| self.runtime_value_matches_spec(value, &element_type))
            }
            DataTypeSpec::Struct { fields } => {
                let Value::Struct(values) = value else {
                    return false;
                };
                fields.iter().all(|field| {
                    values
                        .get(&field.name.canonical)
                        .is_some_and(|value| self.runtime_value_matches_spec(value, &field.spec))
                })
            }
            DataTypeSpec::Named(_) => true,
        }
    }

    pub(crate) fn execute_block(&mut self, body: &[Statement]) -> Control {
        for statement in body {
            match self.execute_statement(statement) {
                Control::Continue => {}
                control => return control,
            }
        }
        Control::Continue
    }

    pub(crate) fn execute_statement_list(&mut self, statements: &[Statement]) -> Control {
        let labels = statements
            .iter()
            .enumerate()
            .filter_map(|(index, statement)| {
                if let Statement::IlLabel(label) = statement {
                    Some((label.canonical.clone(), index))
                } else {
                    None
                }
            })
            .collect::<BTreeMap<_, _>>();

        let mut ip = 0_usize;
        let mut iterations = 0_usize;
        while ip < statements.len() {
            match self.execute_statement(&statements[ip]) {
                Control::Continue => ip += 1,
                Control::Jump(label) => {
                    let Some(target) = labels.get(&label) else {
                        return Control::Jump(label);
                    };
                    ip = *target;
                }
                control => return control,
            }
            iterations += 1;
            if iterations > self.options.max_loop_iterations {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    "maximum statement execution count exceeded",
                    None,
                ));
                break;
            }
        }
        Control::Continue
    }

    pub(crate) fn execute_program_cycle(&mut self) -> Control {
        if let Some(sfc) = self.program.body.sfc.clone() {
            self.execute_sfc(&sfc)
        } else {
            self.execute_statement_list(&self.program.body.statements.clone())
        }
    }

    pub(crate) fn execute_sfc(&mut self, sfc: &Sfc) -> Control {
        let active_steps = sfc
            .steps
            .iter()
            .filter(|step| {
                self.env
                    .get(&sfc_step_key(&step.name))
                    .and_then(Value::as_bool)
                    == Some(true)
            })
            .map(|step| step.name.canonical.clone())
            .collect::<Vec<_>>();

        for action in &sfc.actions {
            let control_key = sfc_action_control_key(&action.name);
            let inputs = sfc_action_inputs(sfc, action, &active_steps);
            if self.sfc_action_should_execute(&control_key, &action.name, &inputs) {
                match self.execute_statement_list(&action.body) {
                    Control::Continue | Control::Return => {}
                    control => return control,
                }
            }
        }

        let mut candidates = Vec::new();
        for (index, transition) in sfc.transitions.iter().enumerate() {
            let Some((from_steps, to_steps)) = sfc_transition_steps(sfc, transition, index) else {
                continue;
            };
            let from_active = from_steps.iter().all(|step| {
                self.env.get(&sfc_step_key(step)).and_then(Value::as_bool) == Some(true)
            });
            if !from_active {
                continue;
            }
            let condition = transition
                .condition
                .as_ref()
                .and_then(|condition| self.eval_expr(condition))
                .and_then(|value| value.as_bool())
                .unwrap_or(false);
            if condition {
                candidates.push((
                    transition.priority.unwrap_or(i64::MAX),
                    index,
                    from_steps.into_iter().cloned().collect::<Vec<_>>(),
                    to_steps.into_iter().cloned().collect::<Vec<_>>(),
                ));
            }
        }
        candidates.sort_by_key(|(priority, index, _, _)| (*priority, *index));

        let mut consumed_steps = BTreeSet::new();
        for (_, _, from_steps, to_steps) in candidates {
            if from_steps
                .iter()
                .any(|step| consumed_steps.contains(&step.canonical))
            {
                continue;
            }
            for from in &from_steps {
                consumed_steps.insert(from.canonical.clone());
            }
            for from in from_steps {
                self.env.insert(sfc_step_key(&from), Value::Bool(false));
            }
            for to in to_steps {
                self.env.insert(sfc_step_key(&to), Value::Bool(true));
            }
        }

        Control::Continue
    }

    pub(crate) fn sfc_action_should_execute(
        &mut self,
        control_key: &str,
        action_name: &Identifier,
        inputs: &[SfcActionInput<'_>],
    ) -> bool {
        let active_time_inputs = inputs
            .iter()
            .filter(|input| input.active && input.qualifier.requires_duration())
            .count();
        if active_time_inputs > 1 {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "SFC action '{}' has more than one active time-related association",
                    action_name.original
                ),
                None,
            ));
            return false;
        }

        let reset_active = inputs
            .iter()
            .any(|input| input.active && input.qualifier == SfcActionQualifier::ResetStored);
        if reset_active {
            self.set_sfc_action_stored(control_key, false);
            self.set_sfc_action_elapsed(control_key, 0);
        }

        let non_stored = inputs
            .iter()
            .any(|input| input.active && input.qualifier == SfcActionQualifier::NonStored);

        let pulse_active = inputs
            .iter()
            .any(|input| input.active && input.qualifier == SfcActionQualifier::Pulse);
        let has_pulse_falling_input = inputs
            .iter()
            .any(|input| input.qualifier == SfcActionQualifier::PulseFalling);
        let pulse_falling_active = inputs
            .iter()
            .any(|input| input.active && input.qualifier == SfcActionQualifier::PulseFalling);
        let previous_key = sfc_action_control_key_previous(control_key);
        let was_active = self
            .env
            .get(&previous_key)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.env.insert(
            previous_key,
            Value::Bool(pulse_active || pulse_falling_active),
        );

        let mut should_execute = non_stored || (pulse_active && !was_active);
        should_execute |= has_pulse_falling_input && !pulse_falling_active && was_active;

        if inputs
            .iter()
            .any(|input| input.active && input.qualifier == SfcActionQualifier::SetStored)
        {
            self.set_sfc_action_stored(control_key, true);
        }

        let timed_input = inputs
            .iter()
            .find(|input| input.active && input.qualifier.requires_duration());

        if let Some(input) = timed_input {
            should_execute |= match input.qualifier {
                SfcActionQualifier::TimeLimited => {
                    let elapsed = self.advance_sfc_action_elapsed(control_key);
                    elapsed <= sfc_action_duration_ms(input.duration)
                }
                SfcActionQualifier::TimeDelayed => {
                    let elapsed = self.advance_sfc_action_elapsed(control_key);
                    elapsed >= sfc_action_duration_ms(input.duration)
                }
                SfcActionQualifier::StoredDelayed | SfcActionQualifier::DelayedStored => {
                    let elapsed = self.advance_sfc_action_elapsed(control_key);
                    if elapsed >= sfc_action_duration_ms(input.duration) {
                        self.set_sfc_action_stored(control_key, true);
                    }
                    false
                }
                SfcActionQualifier::StoredLimited => {
                    if !self.sfc_action_stored(control_key) {
                        self.set_sfc_action_stored(control_key, true);
                        self.set_sfc_action_elapsed(control_key, 0);
                    }
                    false
                }
                _ => false,
            };
        } else if !self.sfc_action_stored(control_key) {
            self.set_sfc_action_elapsed(control_key, 0);
        }

        if self.sfc_action_stored(control_key) {
            should_execute = true;
        }

        if inputs
            .iter()
            .any(|input| input.qualifier == SfcActionQualifier::StoredLimited)
            && self.sfc_action_stored(control_key)
        {
            let elapsed = self.advance_sfc_action_elapsed(control_key);
            let duration = inputs
                .iter()
                .find(|input| input.active && input.qualifier == SfcActionQualifier::StoredLimited)
                .and_then(|input| input.duration);
            if elapsed <= sfc_action_duration_ms(duration) {
                should_execute = true;
            } else {
                self.set_sfc_action_stored(control_key, false);
                should_execute = false;
            }
        }

        should_execute && !reset_active
    }

    pub(crate) fn sfc_action_stored(&self, control_key: &str) -> bool {
        self.env
            .get(&sfc_action_control_key_stored(control_key))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub(crate) fn set_sfc_action_stored(&mut self, control_key: &str, value: bool) {
        self.env.insert(
            sfc_action_control_key_stored(control_key),
            Value::Bool(value),
        );
    }

    pub(crate) fn sfc_action_elapsed(&self, control_key: &str) -> i128 {
        self.env
            .get(&sfc_action_control_key_elapsed(control_key))
            .and_then(Value::as_i64)
            .map(i128::from)
            .unwrap_or(0)
    }

    pub(crate) fn set_sfc_action_elapsed(&mut self, control_key: &str, elapsed: i128) {
        self.env.insert(
            sfc_action_control_key_elapsed(control_key),
            Value::Int(elapsed as i64),
        );
    }

    pub(crate) fn advance_sfc_action_elapsed(&mut self, control_key: &str) -> i128 {
        let elapsed = self.sfc_action_elapsed(control_key) + self.options.cycle_time_ms.max(1);
        self.set_sfc_action_elapsed(control_key, elapsed);
        elapsed
    }

    pub(crate) fn execute_statement(&mut self, statement: &Statement) -> Control {
        match statement {
            Statement::Empty => Control::Continue,
            Statement::Assignment { target, value } => {
                let Some(value) = self.eval_expr(value) else {
                    return Control::Continue;
                };
                self.assign(target, value);
                Control::Continue
            }
            Statement::If {
                branches,
                else_branch,
            } => {
                for (condition, body) in branches {
                    if self.eval_expr(condition).and_then(|v| v.as_bool()) == Some(true) {
                        return self.execute_block(body);
                    }
                }
                self.execute_block(else_branch)
            }
            Statement::Case {
                selector,
                cases,
                else_branch,
            } => {
                let selector = self.eval_expr(selector);
                if let Some(selector) = selector {
                    for (labels, body) in cases {
                        if labels
                            .iter()
                            .any(|label| self.case_label_matches(label, &selector))
                        {
                            return self.execute_block(body);
                        }
                    }
                }
                self.execute_block(else_branch)
            }
            Statement::For {
                control,
                from,
                to,
                by,
                body,
            } => {
                let mut value = self.eval_expr(from).and_then(|v| v.as_i64()).unwrap_or(0);
                let end = self.eval_expr(to).and_then(|v| v.as_i64()).unwrap_or(0);
                let step = by
                    .as_ref()
                    .and_then(|expr| self.eval_expr(expr))
                    .and_then(|v| v.as_i64())
                    .unwrap_or(1);
                if step == 0 {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        "FOR loop BY value cannot be zero",
                        None,
                    ));
                    return Control::Continue;
                }

                let mut iterations = 0;
                while if step > 0 { value <= end } else { value >= end } {
                    self.env
                        .insert(control.canonical.clone(), Value::Int(value));
                    match self.execute_block(body) {
                        Control::Continue => {}
                        Control::Exit => break,
                        Control::Return => return Control::Return,
                        Control::Jump(label) => return Control::Jump(label),
                    }
                    value += step;
                    iterations += 1;
                    if iterations > self.options.max_loop_iterations {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Runtime,
                            "maximum FOR loop iteration count exceeded",
                            None,
                        ));
                        break;
                    }
                }
                Control::Continue
            }
            Statement::While { condition, body } => {
                let mut iterations = 0;
                while self.eval_expr(condition).and_then(|v| v.as_bool()) == Some(true) {
                    match self.execute_block(body) {
                        Control::Continue => {}
                        Control::Exit => break,
                        Control::Return => return Control::Return,
                        Control::Jump(label) => return Control::Jump(label),
                    }
                    iterations += 1;
                    if iterations > self.options.max_loop_iterations {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Runtime,
                            "maximum WHILE loop iteration count exceeded",
                            None,
                        ));
                        break;
                    }
                }
                Control::Continue
            }
            Statement::Repeat { body, until } => {
                let mut iterations = 0;
                loop {
                    match self.execute_block(body) {
                        Control::Continue => {}
                        Control::Exit => break,
                        Control::Return => return Control::Return,
                        Control::Jump(label) => return Control::Jump(label),
                    }
                    if self.eval_expr(until).and_then(|v| v.as_bool()) == Some(true) {
                        break;
                    }
                    iterations += 1;
                    if iterations > self.options.max_loop_iterations {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Runtime,
                            "maximum REPEAT loop iteration count exceeded",
                            None,
                        ));
                        break;
                    }
                }
                Control::Continue
            }
            Statement::Il { op, operand } => self.execute_il_instruction(*op, operand.as_ref()),
            Statement::IlLabel(_) => Control::Continue,
            Statement::FbCall { name, .. } => {
                if let Some(root) = name.root_name() {
                    if is_standard_void_function(&root.original) {
                        self.execute_standard_void_call(root, statement);
                    } else {
                        self.execute_fb_call(name, statement);
                    }
                } else {
                    self.execute_fb_call(name, statement);
                }
                Control::Continue
            }
            Statement::Exit => Control::Exit,
            Statement::Return => Control::Return,
            Statement::Unsupported(text) => {
                self.diagnostics.push(Diagnostic::warning(
                    DiagnosticCode::Unsupported,
                    format!("skipping unsupported statement: {text}"),
                    None,
                ));
                Control::Continue
            }
        }
    }

    pub(crate) fn eval_expr(&mut self, expr: &Expr) -> Option<Value> {
        match expr {
            Expr::Literal(literal) => Some(literal_to_value(self.project, literal)),
            Expr::Variable(variable) => self.resolve(variable),
            Expr::Unary { op, expr } => {
                let value = self.eval_expr(expr)?;
                match op {
                    UnaryOp::Neg => match value {
                        Value::Real(value) => Some(Value::Real(-value)),
                        value => {
                            let value = value.as_i64()?;
                            value.checked_neg().map(Value::Int).or_else(|| {
                                self.push_overflow("integer negation");
                                None
                            })
                        }
                    },
                    UnaryOp::Not => match value {
                        Value::Bool(value) => Some(Value::Bool(!value)),
                        value => value.as_i64().map(|value| Value::Int(!value)),
                    },
                }
            }
            Expr::Binary { op, left, right } => {
                let left = self.eval_expr(left)?;
                if let Value::Bool(value) = left {
                    match op {
                        BinaryOp::And if !value => return Some(Value::Bool(false)),
                        BinaryOp::Or if value => return Some(Value::Bool(true)),
                        _ => {
                            let right = self.eval_expr(right)?;
                            return self.eval_binary(*op, Value::Bool(value), right);
                        }
                    }
                }
                let right = self.eval_expr(right)?;
                self.eval_binary(*op, left, right)
            }
            Expr::Call { name, args } => {
                let enabled = self.function_call_enabled(args);
                if !enabled {
                    self.assign_function_eno(args, false);
                    if is_standard_function(&name.original) {
                        return Some(self.disabled_standard_function_value(name, args));
                    }
                    if let Some(function) = self
                        .project
                        .find_pou(&name.original)
                        .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))
                    {
                        if let PouKind::Function { return_type } = &function.kind {
                            return Some(self.default_value(return_type));
                        }
                    }
                    return Some(Value::Int(0));
                }

                let standard_values = self.eval_standard_function_inputs(name, args);
                if let Some(value) = eval_standard_function(&name.original, &standard_values) {
                    self.assign_function_eno(args, true);
                    Some(value)
                } else if is_standard_function(&name.original) {
                    self.assign_function_eno(args, false);
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        format!(
                            "standard function '{}' failed for supplied arguments",
                            name.original
                        ),
                        None,
                    ));
                    None
                } else {
                    self.eval_user_function(name, args).or_else(|| {
                        self.diagnostics.push(Diagnostic::warning(
                            DiagnosticCode::Unsupported,
                            format!("function '{}' is not executable yet", name.original),
                            None,
                        ));
                        Some(Value::Unit)
                    })
                }
            }
            Expr::ArrayLiteral(elements) => {
                let values = elements
                    .iter()
                    .map(|element| self.eval_expr(element))
                    .collect::<Option<Vec<_>>>()?;
                Some(Value::Array(values))
            }
            Expr::StructLiteral(fields) => {
                let mut values = BTreeMap::new();
                for field in fields {
                    let Some(name) = &field.name else {
                        continue;
                    };
                    let value = field
                        .expr
                        .as_ref()
                        .and_then(|expr| self.eval_expr(expr))
                        .unwrap_or(Value::Unit);
                    values.insert(name.canonical.clone(), value);
                }
                Some(Value::Struct(values))
            }
        }
    }

    pub(crate) fn eval_user_function(
        &mut self,
        name: &Identifier,
        args: &[ParamAssignment],
    ) -> Option<Value> {
        let function = self
            .project
            .find_pou(&name.original)
            .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))?;
        let PouKind::Function { return_type } = &function.kind else {
            return None;
        };

        if !self.function_call_enabled(args) {
            self.assign_function_eno(args, false);
            return Some(self.default_value(return_type));
        }

        if self.call_depth >= 64 {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "maximum function call depth exceeded at '{}'",
                    name.original
                ),
                None,
            ));
            return None;
        }

        let mut positional = Vec::new();
        let mut named = BTreeMap::new();
        for arg in args {
            if arg.output || arg.name.as_ref().is_some_and(is_implicit_en) {
                continue;
            }
            let value = arg
                .expr
                .as_ref()
                .and_then(|expr| self.eval_expr(expr))
                .unwrap_or(Value::Unit);
            if let Some(name) = &arg.name {
                named.insert(name.canonical.clone(), value);
            } else {
                positional.push(value);
            }
        }

        let mut runtime = Runtime {
            project: self.project,
            program: function,
            env: BTreeMap::new(),
            types: BTreeMap::new(),
            il_accumulator: Value::Unit,
            diagnostics: Vec::new(),
            options: self.options.clone(),
            call_depth: self.call_depth + 1,
            communication: self.communication,
        };
        runtime.initialize(StartupKind::Cold);
        runtime.bind_function_inputs(&positional, &named);

        match runtime.execute_statement_list(&function.body.statements) {
            Control::Continue | Control::Return => {}
            Control::Exit => runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                "EXIT used outside of an iteration",
                None,
            )),
            Control::Jump(label) => runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("jump to unknown IL label '{label}'"),
                None,
            )),
        }

        let result = runtime.env.get(&function.name.canonical).cloned();
        self.diagnostics.extend(runtime.diagnostics);
        if result.is_some() {
            self.assign_function_eno(args, true);
        }
        result
    }

    pub(crate) fn eval_standard_function_inputs(
        &mut self,
        name: &Identifier,
        args: &[ParamAssignment],
    ) -> Vec<Value> {
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
            let Some(value) = self.eval_expr(expr) else {
                continue;
            };
            let index = if let Some(arg_name) = &arg.name {
                standard_function_input_index(&name.original, &arg_name.original).unwrap_or_else(
                    || {
                        let index = unknown_index;
                        unknown_index = unknown_index.saturating_add(1);
                        index
                    },
                )
            } else {
                let index = positional_index;
                positional_index += 1;
                index
            };
            ordered.push((index, value));
        }

        ordered.sort_by_key(|(index, _)| *index);
        ordered.into_iter().map(|(_, value)| value).collect()
    }

    pub(crate) fn disabled_standard_function_value(
        &self,
        name: &Identifier,
        args: &[ParamAssignment],
    ) -> Value {
        match name.canonical.as_str() {
            "GT" | "GE" | "EQ" | "NE" | "LE" | "LT" => Value::Bool(false),
            "SQRT" | "LN" | "LOG" | "EXP" | "SIN" | "COS" | "TAN" | "EXPT" => Value::Real(0.0),
            "LEFT" | "RIGHT" | "MID" | "CONCAT" | "INSERT" | "DELETE" | "REPLACE" => {
                if self.standard_string_call_is_wide(args) {
                    Value::WString(String::new())
                } else {
                    Value::String(String::new())
                }
            }
            name if name.ends_with("_TO_STRING") => Value::String(String::new()),
            name if name.ends_with("_TO_WSTRING") => Value::WString(String::new()),
            name if name.ends_with("_TO_BOOL") => Value::Bool(false),
            "ADD_TIME" | "SUB_TIME" | "ADD_TOD_TIME" | "SUB_TOD_TIME" | "ADD_DT_TIME"
            | "SUB_DT_TIME" | "CONCAT_DATE" | "CONCAT_TOD" | "CONCAT_DT" | "CONCAT_DATE_TOD"
            | "SUB_DATE_DATE" | "SUB_TOD_TOD" | "SUB_DT_DT" | "MUL_TIME" | "DIV_TIME"
            | "MULTIME" | "DIVTIME" => Value::TimeMs(0),
            _ => Value::Int(0),
        }
    }

    pub(crate) fn standard_string_call_is_wide(&self, args: &[ParamAssignment]) -> bool {
        args.iter()
            .filter(|arg| !arg.output)
            .filter(|arg| !arg.name.as_ref().is_some_and(is_implicit_en))
            .filter_map(|arg| arg.expr.as_ref())
            .any(|expr| self.expr_is_wstring_like(expr))
    }

    pub(crate) fn expr_is_wstring_like(&self, expr: &Expr) -> bool {
        match expr {
            Expr::Literal(Literal::WString(_)) => true,
            Expr::Literal(Literal::Typed { type_name, .. }) => {
                let spec = ElementaryType::parse(&type_name.original)
                    .map(DataTypeSpec::Elementary)
                    .or_else(|| {
                        self.project
                            .data_types()
                            .find(|data_type| data_type.name.canonical == type_name.canonical)
                            .map(|data_type| data_type.spec.clone())
                    });
                spec.is_some_and(|spec| {
                    matches!(
                        self.resolve_named_spec(&spec),
                        DataTypeSpec::Elementary(ElementaryType::WString)
                            | DataTypeSpec::String { wide: true, .. }
                    )
                })
            }
            Expr::Variable(variable) => self.variable_spec(variable).is_some_and(|spec| {
                matches!(
                    self.resolve_named_spec(&spec),
                    DataTypeSpec::Elementary(ElementaryType::WString)
                        | DataTypeSpec::String { wide: true, .. }
                )
            }),
            _ => false,
        }
    }

    pub(crate) fn function_call_enabled(&mut self, args: &[ParamAssignment]) -> bool {
        args.iter()
            .find(|arg| !arg.output && arg.name.as_ref().is_some_and(is_implicit_en))
            .and_then(|arg| arg.expr.as_ref())
            .and_then(|expr| self.eval_expr(expr))
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
    }

    pub(crate) fn assign_function_eno(&mut self, args: &[ParamAssignment], value: bool) {
        for arg in args {
            if !arg.output || !arg.name.as_ref().is_some_and(is_implicit_eno) {
                continue;
            }
            if let Some(variable) = &arg.variable {
                self.assign(
                    variable,
                    Value::Bool(if arg.negated { !value } else { value }),
                );
            }
        }
    }

    pub(crate) fn bind_function_inputs(
        &mut self,
        positional: &[Value],
        named: &BTreeMap<String, Value>,
    ) {
        let mut positional_index = 0;
        let inputs = self
            .function_inputs()
            .map(|var| var.name.canonical.clone())
            .collect::<Vec<_>>();
        for input in inputs {
            if let Some(value) = named.get(&input) {
                self.env.insert(input, value.clone());
            } else if let Some(value) = positional.get(positional_index) {
                self.env.insert(input, value.clone());
                positional_index += 1;
            }
        }
    }

    pub(crate) fn function_inputs(&self) -> impl Iterator<Item = &VarDecl> {
        self.program
            .var_blocks
            .iter()
            .filter(|block| block.kind == VarBlockKind::Input)
            .flat_map(|block| block.vars.iter())
    }

    pub(crate) fn eval_binary(&mut self, op: BinaryOp, left: Value, right: Value) -> Option<Value> {
        match op {
            BinaryOp::Or => bit_bool_binary(left, right, |a, b| a | b, |a, b| a || b),
            BinaryOp::Xor => bit_bool_binary(left, right, |a, b| a ^ b, |a, b| a ^ b),
            BinaryOp::And => bit_bool_binary(left, right, |a, b| a & b, |a, b| a && b),
            BinaryOp::Equal => Some(Value::Bool(compare_values(&left, &right) == Some(0))),
            BinaryOp::NotEqual => Some(Value::Bool(compare_values(&left, &right) != Some(0))),
            BinaryOp::Less => Some(Value::Bool(compare_values(&left, &right)? < 0)),
            BinaryOp::LessEqual => Some(Value::Bool(compare_values(&left, &right)? <= 0)),
            BinaryOp::Greater => Some(Value::Bool(compare_values(&left, &right)? > 0)),
            BinaryOp::GreaterEqual => Some(Value::Bool(compare_values(&left, &right)? >= 0)),
            BinaryOp::Add => self.time_or_numeric_binary(
                left,
                right,
                "addition",
                i128::checked_add,
                i64::checked_add,
                |a, b| a + b,
            ),
            BinaryOp::Sub => self.time_or_numeric_binary(
                left,
                right,
                "subtraction",
                i128::checked_sub,
                i64::checked_sub,
                |a, b| a - b,
            ),
            BinaryOp::Mul => {
                self.numeric_binary(left, right, "multiplication", i64::checked_mul, |a, b| {
                    a * b
                })
            }
            BinaryOp::Div => {
                if right.as_f64() == Some(0.0) {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        "division by zero",
                        None,
                    ));
                    None
                } else {
                    self.numeric_binary(left, right, "division", i64::checked_div, |a, b| a / b)
                }
            }
            BinaryOp::Mod => {
                let right = right.as_i64()?;
                if right == 0 {
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        "modulo by zero",
                        None,
                    ));
                    None
                } else {
                    left.as_i64()?
                        .checked_rem(right)
                        .map(Value::Int)
                        .or_else(|| {
                            self.push_overflow("modulo");
                            None
                        })
                }
            }
            BinaryOp::Power => Some(Value::Real(left.as_f64()?.powf(right.as_f64()?))),
        }
    }

    pub(crate) fn numeric_binary(
        &mut self,
        left: Value,
        right: Value,
        label: &str,
        int_op: fn(i64, i64) -> Option<i64>,
        real_op: fn(f64, f64) -> f64,
    ) -> Option<Value> {
        if matches!(left, Value::Real(_)) || matches!(right, Value::Real(_)) {
            Some(Value::Real(real_op(left.as_f64()?, right.as_f64()?)))
        } else {
            int_op(left.as_i64()?, right.as_i64()?)
                .map(Value::Int)
                .or_else(|| {
                    self.push_overflow(label);
                    None
                })
        }
    }

    pub(crate) fn time_or_numeric_binary(
        &mut self,
        left: Value,
        right: Value,
        label: &str,
        time_op: fn(i128, i128) -> Option<i128>,
        numeric_op: fn(i64, i64) -> Option<i64>,
        real_op: fn(f64, f64) -> f64,
    ) -> Option<Value> {
        match (&left, &right) {
            (Value::TimeMs(left), Value::TimeMs(right)) => {
                time_op(*left, *right).map(Value::TimeMs).or_else(|| {
                    self.push_overflow(label);
                    None
                })
            }
            _ => self.numeric_binary(left, right, label, numeric_op, real_op),
        }
    }

    pub(crate) fn push_overflow(&mut self, operation: &str) {
        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::Runtime,
            format!("integer overflow during {operation}"),
            None,
        ));
    }

    pub(crate) fn resolve(&mut self, variable: &VariableRef) -> Option<Value> {
        if let Some(direct) = &variable.direct {
            return self.env.get(direct).cloned().or(Some(Value::Int(0)));
        }

        let root = variable.root_name()?;
        if let Some(ordinal) = self.enum_ordinal_name(&root.canonical) {
            return Some(Value::Int(ordinal));
        }
        if let Some(key) = flattened_field_key(variable) {
            if let Some(value) = self.env.get(&key) {
                return Some(value.clone());
            }
        }

        let mut value = self.env.get(&root.canonical).cloned().or_else(|| {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("variable '{}' has no runtime storage", variable),
                None,
            ));
            None
        })?;
        let mut spec = self.types.get(&root.canonical).cloned()?;
        (value, spec) = self.apply_indices_to_value(
            value,
            spec,
            variable.indices.first().map(Vec::as_slice).unwrap_or(&[]),
        )?;
        for (segment_index, segment) in variable.path.iter().enumerate().skip(1) {
            spec = self.resolve_named_spec(&spec);
            let DataTypeSpec::Struct { fields } = spec else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!("'{}' is not a structure", variable),
                    None,
                ));
                return None;
            };
            let Some(field) = fields
                .iter()
                .find(|field| field.name.canonical == segment.canonical)
            else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!("structure field '{}' does not exist", segment.original),
                    None,
                ));
                return None;
            };
            let Value::Struct(fields) = value else {
                return None;
            };
            value = fields.get(&segment.canonical).cloned()?;
            spec = field.spec.clone();
            (value, spec) = self.apply_indices_to_value(
                value,
                spec,
                variable
                    .indices
                    .get(segment_index)
                    .map(Vec::as_slice)
                    .unwrap_or(&[]),
            )?;
        }
        Some(value)
    }

    pub(crate) fn assign(&mut self, target: &VariableRef, value: Value) {
        if let Some(direct) = &target.direct {
            self.env.insert(direct.clone(), value);
            return;
        }
        let Some(root) = target.root_name() else {
            return;
        };
        if let Some(key) = flattened_field_key(target) {
            if let std::collections::btree_map::Entry::Occupied(mut e) = self.env.entry(key) {
                e.insert(value);
                return;
            }
        }
        if target.path.len() == 2
            && target.indices.iter().all(Vec::is_empty)
            && self
                .env
                .contains_key(&field_key(&root.canonical, &target.path[1].canonical))
        {
            self.env
                .insert(field_key(&root.canonical, &target.path[1].canonical), value);
            return;
        }
        let Some(spec) = self.types.get(&root.canonical).cloned() else {
            return;
        };
        let Some(mut root_value) = self.env.get(&root.canonical).cloned() else {
            return;
        };
        if self.assign_into_value(&mut root_value, &spec, target, 0, value) {
            self.env.insert(root.canonical.clone(), root_value);
        }
    }

    pub(crate) fn apply_indices_to_value(
        &mut self,
        mut value: Value,
        spec: DataTypeSpec,
        indices: &[Expr],
    ) -> Option<(Value, DataTypeSpec)> {
        if indices.is_empty() {
            return Some((value, spec));
        }
        let mut current_spec = self.resolve_named_spec(&spec);
        let mut remaining = indices;
        while !remaining.is_empty() {
            let DataTypeSpec::Array {
                ranges,
                element_type,
            } = current_spec
            else {
                return None;
            };
            if remaining.len() < ranges.len() {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!(
                        "array access expects {} index value(s), got {}",
                        ranges.len(),
                        remaining.len()
                    ),
                    None,
                ));
                return None;
            }
            let (current_indices, rest) = remaining.split_at(ranges.len());
            let offset = self.array_offset(&ranges, current_indices)?;
            let Value::Array(elements) = value else {
                return None;
            };
            value = elements.get(offset).cloned()?;
            current_spec = self.resolve_named_spec(&element_type);
            remaining = rest;
        }
        Some((value, current_spec))
    }

    pub(crate) fn assign_into_value(
        &mut self,
        current: &mut Value,
        spec: &DataTypeSpec,
        target: &VariableRef,
        segment_index: usize,
        value: Value,
    ) -> bool {
        let current_spec = self.resolve_named_spec(spec);
        if let Some(indices) = target.indices.get(segment_index) {
            if !indices.is_empty() {
                return self.assign_into_indexed_value(
                    current,
                    &current_spec,
                    indices,
                    target,
                    segment_index,
                    value,
                );
            }
        }

        if segment_index + 1 >= target.path.len() {
            *current = self.constrain_value(&current_spec, value);
            return true;
        }

        let DataTypeSpec::Struct { fields } = current_spec else {
            return false;
        };
        let next = &target.path[segment_index + 1];
        let Some(field) = fields
            .iter()
            .find(|field| field.name.canonical == next.canonical)
        else {
            return false;
        };
        let Value::Struct(values) = current else {
            return false;
        };
        let Some(field_value) = values.get_mut(&next.canonical) else {
            return false;
        };
        self.assign_into_value(field_value, &field.spec, target, segment_index + 1, value)
    }

    pub(crate) fn assign_into_indexed_value(
        &mut self,
        current: &mut Value,
        spec: &DataTypeSpec,
        indices: &[Expr],
        target: &VariableRef,
        segment_index: usize,
        value: Value,
    ) -> bool {
        if indices.is_empty() {
            if segment_index + 1 >= target.path.len() {
                *current = self.constrain_value(spec, value);
                return true;
            }
            return self.assign_into_value(current, spec, target, segment_index + 1, value);
        }

        let current_spec = self.resolve_named_spec(spec);
        let DataTypeSpec::Array {
            ranges,
            element_type,
        } = current_spec
        else {
            return false;
        };
        if indices.len() < ranges.len() {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "array access expects {} index value(s), got {}",
                    ranges.len(),
                    indices.len()
                ),
                None,
            ));
            return false;
        }
        let (current_indices, rest) = indices.split_at(ranges.len());
        let Some(offset) = self.array_offset(&ranges, current_indices) else {
            return false;
        };
        let Value::Array(elements) = current else {
            return false;
        };
        let Some(element) = elements.get_mut(offset) else {
            return false;
        };
        self.assign_into_indexed_value(element, &element_type, rest, target, segment_index, value)
    }

    pub(crate) fn array_offset(&mut self, ranges: &[Subrange], indices: &[Expr]) -> Option<usize> {
        if ranges.len() != indices.len() {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!(
                    "array access expects {} index value(s), got {}",
                    ranges.len(),
                    indices.len()
                ),
                None,
            ));
            return None;
        }
        let mut offset = 0_usize;
        let mut stride = 1_usize;
        for (range, expr) in ranges.iter().rev().zip(indices.iter().rev()) {
            let index = self.eval_expr(expr).and_then(|value| value.as_i64())?;
            if index < range.low || index > range.high {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!(
                        "array index {index} is outside subrange {}..{}",
                        range.low, range.high
                    ),
                    None,
                ));
                return None;
            }
            offset += ((index - range.low) as usize) * stride;
            stride *= (range.high - range.low + 1).max(0) as usize;
        }
        Some(offset)
    }

    pub(crate) fn constrain_value(&mut self, spec: &DataTypeSpec, value: Value) -> Value {
        match self.resolve_named_spec(spec) {
            DataTypeSpec::Elementary(elementary) => {
                if let Some((type_name, low, high)) = elementary_integer_range(&elementary) {
                    if let Some(int_value) = value.as_i64() {
                        let int_value = i128::from(int_value);
                        if int_value < low || int_value > high {
                            self.diagnostics.push(Diagnostic::error(
                                DiagnosticCode::Runtime,
                                format!(
                                    "value {int_value} is outside {type_name} range {low}..{high}"
                                ),
                                None,
                            ));
                        }
                    }
                }
                value
            }
            DataTypeSpec::Subrange { range, .. } => {
                if let Some(int_value) = value.as_i64() {
                    if int_value < range.low || int_value > range.high {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Runtime,
                            format!(
                                "value {int_value} is outside subrange {}..{}",
                                range.low, range.high
                            ),
                            None,
                        ));
                    }
                }
                value
            }
            DataTypeSpec::String {
                length: Some(length),
                ..
            } => match value {
                Value::String(text) => Value::String(truncate_chars(&text, length)),
                Value::WString(text) => Value::WString(truncate_chars(&text, length)),
                value => value,
            },
            DataTypeSpec::String { .. } => value,
            DataTypeSpec::Array { element_type, .. } => {
                if let Value::Array(values) = value {
                    Value::Array(
                        values
                            .into_iter()
                            .map(|value| self.constrain_value(&element_type, value))
                            .collect(),
                    )
                } else {
                    value
                }
            }
            DataTypeSpec::Struct { fields } => {
                if let Value::Struct(mut values) = value {
                    for field in fields {
                        if let Some(field_value) = values.remove(&field.name.canonical) {
                            values.insert(
                                field.name.canonical.clone(),
                                self.constrain_value(&field.spec, field_value),
                            );
                        }
                    }
                    Value::Struct(values)
                } else {
                    value
                }
            }
            _ => value,
        }
    }

    pub(crate) fn enum_ordinal_expr(&self, expr: &Expr) -> Option<i64> {
        if let Expr::Literal(Literal::Typed { type_name, value }) = expr {
            return self.enum_ordinal_typed(type_name, value);
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
        self.enum_ordinal_name(&variable.root_name()?.canonical)
    }

    pub(crate) fn enum_ordinal_typed(
        &self,
        type_name: &Identifier,
        value_name: &str,
    ) -> Option<i64> {
        self.project.data_types().find_map(|data_type| {
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

    pub(crate) fn enum_ordinal_name(&self, canonical_name: &str) -> Option<i64> {
        for data_type in self.project.data_types() {
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

    pub(crate) fn execute_il_instruction(&mut self, op: IlOp, operand: Option<&Expr>) -> Control {
        match op {
            IlOp::Ld | IlOp::Ldn => {
                let mut value = operand
                    .and_then(|expr| self.eval_expr(expr))
                    .unwrap_or(Value::Unit);
                if matches!(op, IlOp::Ldn) {
                    value = Value::Bool(!value.as_bool().unwrap_or(false));
                }
                self.il_accumulator = value;
            }
            IlOp::St | IlOp::Stn => {
                if let Some(Expr::Variable(target)) = operand {
                    let value = if matches!(op, IlOp::Stn) {
                        Value::Bool(!self.il_accumulator.as_bool().unwrap_or(false))
                    } else {
                        self.il_accumulator.clone()
                    };
                    self.assign(target, value);
                }
            }
            IlOp::S | IlOp::R => {
                if self.il_accumulator.as_bool().unwrap_or(false) {
                    if let Some(Expr::Variable(target)) = operand {
                        self.assign(target, Value::Bool(matches!(op, IlOp::S)));
                    }
                }
            }
            IlOp::Not => {
                self.il_accumulator = Value::Bool(!self.il_accumulator.as_bool().unwrap_or(false));
            }
            IlOp::And | IlOp::Andn | IlOp::Or | IlOp::Orn | IlOp::Xor | IlOp::Xorn => {
                let mut right = operand
                    .and_then(|expr| self.eval_expr(expr))
                    .unwrap_or(Value::Bool(false));
                if matches!(op, IlOp::Andn | IlOp::Orn | IlOp::Xorn) {
                    right = Value::Bool(!right.as_bool().unwrap_or(false));
                }
                let binary = match op {
                    IlOp::And | IlOp::Andn => BinaryOp::And,
                    IlOp::Or | IlOp::Orn => BinaryOp::Or,
                    IlOp::Xor | IlOp::Xorn => BinaryOp::Xor,
                    _ => unreachable!(),
                };
                if let Some(value) = self.eval_binary(binary, self.il_accumulator.clone(), right) {
                    self.il_accumulator = value;
                }
            }
            IlOp::Add
            | IlOp::Sub
            | IlOp::Mul
            | IlOp::Div
            | IlOp::Mod
            | IlOp::Gt
            | IlOp::Ge
            | IlOp::Eq
            | IlOp::Ne
            | IlOp::Le
            | IlOp::Lt => {
                let right = operand
                    .and_then(|expr| self.eval_expr(expr))
                    .unwrap_or(Value::Int(0));
                let binary = match op {
                    IlOp::Add => BinaryOp::Add,
                    IlOp::Sub => BinaryOp::Sub,
                    IlOp::Mul => BinaryOp::Mul,
                    IlOp::Div => BinaryOp::Div,
                    IlOp::Mod => BinaryOp::Mod,
                    IlOp::Gt => BinaryOp::Greater,
                    IlOp::Ge => BinaryOp::GreaterEqual,
                    IlOp::Eq => BinaryOp::Equal,
                    IlOp::Ne => BinaryOp::NotEqual,
                    IlOp::Le => BinaryOp::LessEqual,
                    IlOp::Lt => BinaryOp::Less,
                    _ => unreachable!(),
                };
                if let Some(value) = self.eval_binary(binary, self.il_accumulator.clone(), right) {
                    self.il_accumulator = value;
                }
            }
            IlOp::Jmp | IlOp::Jmpc | IlOp::Jmpcn => {
                let should_jump = match op {
                    IlOp::Jmp => true,
                    IlOp::Jmpc => self.il_accumulator.as_bool().unwrap_or(false),
                    IlOp::Jmpcn => !self.il_accumulator.as_bool().unwrap_or(false),
                    _ => false,
                };
                if should_jump {
                    if let Some(label) = operand.and_then(il_label_operand) {
                        return Control::Jump(label.canonical.clone());
                    }
                    self.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        "IL jump instruction requires a label operand",
                        None,
                    ));
                }
            }
            IlOp::Cal | IlOp::Calc | IlOp::Calcn => {
                let should_call = match op {
                    IlOp::Cal => true,
                    IlOp::Calc => self.il_accumulator.as_bool().unwrap_or(false),
                    IlOp::Calcn => !self.il_accumulator.as_bool().unwrap_or(false),
                    _ => false,
                };
                if should_call {
                    self.execute_il_call(operand);
                }
            }
            IlOp::Ret => return Control::Return,
            IlOp::Retc => {
                if self.il_accumulator.as_bool().unwrap_or(false) {
                    return Control::Return;
                }
            }
            IlOp::Retcn => {
                if !self.il_accumulator.as_bool().unwrap_or(false) {
                    return Control::Return;
                }
            }
        }

        Control::Continue
    }

    pub(crate) fn execute_il_call(&mut self, operand: Option<&Expr>) {
        match operand {
            Some(Expr::Call { name, args }) => {
                let variable = VariableRef::named(name.original.clone());
                let statement = Statement::FbCall {
                    name: variable.clone(),
                    args: args.clone(),
                };
                self.execute_fb_call(&variable, &statement);
            }
            Some(Expr::Variable(variable)) => {
                let statement = Statement::FbCall {
                    name: variable.clone(),
                    args: Vec::new(),
                };
                self.execute_fb_call(variable, &statement);
            }
            _ => self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                "IL CAL instruction requires a function block instance operand",
                None,
            )),
        }
    }

    pub(crate) fn execute_standard_void_call(&mut self, name: &Identifier, statement: &Statement) {
        let Statement::FbCall { args, .. } = statement else {
            return;
        };
        if !self.function_call_enabled(args) {
            self.assign_function_eno(args, false);
            return;
        }
        let Some(input) = split_input_expr(args).and_then(|expr| self.eval_expr(expr)) else {
            self.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("standard function '{}' requires an IN value", name.original),
                None,
            ));
            self.assign_function_eno(args, false);
            return;
        };

        let fields = match name.canonical.as_str() {
            "SPLIT_DATE" => {
                let (year, month, day) = civil_from_days(input_time_value(&input));
                vec![("YEAR", year), ("MONTH", month), ("DATE", day)]
            }
            "SPLIT_TOD" => {
                let (hour, minute, second, millisecond) = tod_parts(input_time_value(&input));
                vec![
                    ("HOUR", hour),
                    ("MINUTE", minute),
                    ("SECOND", second),
                    ("MILLISECOND", millisecond),
                ]
            }
            "SPLIT_DT" => {
                let value = input_time_value(&input);
                let days = value.div_euclid(86_400_000);
                let tod = value.rem_euclid(86_400_000);
                let (year, month, day) = civil_from_days(days);
                let (hour, minute, second, millisecond) = tod_parts(tod);
                vec![
                    ("YEAR", year),
                    ("MONTH", month),
                    ("DATE", day),
                    ("HOUR", hour),
                    ("MINUTE", minute),
                    ("SECOND", second),
                    ("MILLISECOND", millisecond),
                ]
            }
            _ => return,
        };

        for (index, (field, value)) in fields.into_iter().enumerate() {
            if let Some(variable) = split_output_variable(args, field, index) {
                self.assign(variable, Value::Int(value));
            }
        }
        self.assign_function_eno(args, true);
    }

    pub(crate) fn execute_fb_call(&mut self, name: &VariableRef, statement: &Statement) {
        let Statement::FbCall { args, .. } = statement else {
            return;
        };
        let Some(root) = name.root_name() else {
            return;
        };
        let Some(DataTypeSpec::Named(type_name)) = self.types.get(&root.canonical).cloned() else {
            self.diagnostics.push(Diagnostic::warning(
                DiagnosticCode::Unsupported,
                format!(
                    "function block instance '{}' has no executable type",
                    root.original
                ),
                None,
            ));
            return;
        };

        if !self.function_call_enabled(args) {
            self.assign_function_eno(args, false);
            return;
        }

        let inputs = self.eval_fb_inputs(&type_name.original, args);
        let mut executed = true;
        match type_name.canonical.as_str() {
            "SR" => {
                let q = self.get_field_bool(&root.canonical, "Q1");
                let s1 = input_bool(&inputs, "S1");
                let r = input_bool(&inputs, "R");
                self.set_field(&root.canonical, "Q1", Value::Bool(s1 || (q && !r)));
            }
            "RS" => {
                let q = self.get_field_bool(&root.canonical, "Q1");
                let s = input_bool(&inputs, "S");
                let r1 = input_bool(&inputs, "R1");
                self.set_field(&root.canonical, "Q1", Value::Bool((q || s) && !r1));
            }
            "R_TRIG" => {
                let clk = input_bool(&inputs, "CLK");
                let old = self.get_field_bool(&root.canonical, "M");
                self.set_field(&root.canonical, "Q", Value::Bool(clk && !old));
                self.set_field(&root.canonical, "M", Value::Bool(clk));
            }
            "F_TRIG" => {
                let clk = input_bool(&inputs, "CLK");
                let old = self.get_field_bool(&root.canonical, "M");
                self.set_field(&root.canonical, "Q", Value::Bool(!clk && old));
                self.set_field(&root.canonical, "M", Value::Bool(clk));
            }
            "CTU" => self.execute_ctu(&root.canonical, &inputs),
            "CTD" => self.execute_ctd(&root.canonical, &inputs),
            "CTUD" => self.execute_ctud(&root.canonical, &inputs),
            "TON" => self.execute_ton(&root.canonical, &inputs),
            "TOF" => self.execute_tof(&root.canonical, &inputs),
            "TP" => self.execute_tp(&root.canonical, &inputs),
            _ => {
                if is_communication_function_block(&type_name.original) {
                    executed = self.execute_communication_function_block(
                        &root.canonical,
                        &type_name,
                        &inputs,
                    );
                } else if let Some(function_block) = self
                    .project
                    .find_pou(&type_name.original)
                    .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
                {
                    self.execute_user_function_block(&root.canonical, function_block, args);
                } else {
                    self.diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::Unsupported,
                        format!(
                            "function block type '{}' is not executable yet",
                            type_name.original
                        ),
                        None,
                    ));
                    executed = false;
                }
            }
        }
        if executed && is_standard_function_block_type(&type_name.original) {
            self.assign_standard_function_block_outputs(&root.canonical, &type_name.original, args);
        }
        self.assign_function_eno(args, executed);
    }

    pub(crate) fn assign_standard_function_block_outputs(
        &mut self,
        instance: &str,
        block_type: &str,
        args: &[ParamAssignment],
    ) {
        for arg in args {
            if !arg.output {
                continue;
            }
            let (Some(name), Some(variable)) = (&arg.name, &arg.variable) else {
                continue;
            };
            if is_implicit_eno(name)
                || !standard_function_block_output_names(block_type)
                    .iter()
                    .any(|field| canonical_identifier(field) == name.canonical)
            {
                continue;
            }
            let mut value = self
                .env
                .get(&field_key(instance, &name.canonical))
                .cloned()
                .unwrap_or(Value::Unit);
            if arg.negated {
                value = Value::Bool(!value.as_bool().unwrap_or(false));
            }
            self.assign(variable, value);
        }
    }

    pub(crate) fn execute_communication_function_block(
        &mut self,
        instance: &str,
        type_name: &Identifier,
        inputs: &BTreeMap<String, Value>,
    ) -> bool {
        let invocation = CommunicationInvocation {
            block: type_name.original.clone(),
            instance: instance.to_string(),
            inputs: inputs.clone(),
        };
        if let Some(outcome) = self.communication.execute(&invocation) {
            for (field, value) in outcome.outputs {
                self.set_field(instance, &canonical_identifier(&field), value);
            }
            return true;
        }

        self.set_field(instance, "DONE", Value::Bool(false));
        self.set_field(instance, "NDR", Value::Bool(false));
        self.set_field(instance, "ERROR", Value::Bool(true));
        self.set_field(instance, "STATUS", Value::Int(-1));
        self.diagnostics.push(Diagnostic::warning(
            DiagnosticCode::Unsupported,
            format!(
                "communication function block '{}' requires a runtime communication hook",
                type_name.original
            ),
            None,
        ));
        false
    }

    pub(crate) fn execute_user_function_block(
        &mut self,
        instance: &str,
        function_block: &Pou,
        args: &[ParamAssignment],
    ) {
        let input_fields = function_block
            .var_blocks
            .iter()
            .filter(|block| matches!(block.kind, VarBlockKind::Input | VarBlockKind::InOut))
            .flat_map(|block| {
                block
                    .vars
                    .iter()
                    .map(move |var| (block.kind, var.name.clone()))
            })
            .collect::<Vec<_>>();

        let mut positional_index = 0_usize;
        for arg in args {
            let Some((_, name)) = user_fb_input_target(&input_fields, arg, &mut positional_index)
            else {
                continue;
            };
            let value = arg
                .expr
                .as_ref()
                .and_then(|expr| self.eval_expr(expr))
                .unwrap_or(Value::Unit);
            let value = if let Some(edge) = user_fb_input_edge(function_block, &name.canonical) {
                self.edge_qualified_input_value(instance, &name.canonical, edge, value)
            } else {
                value
            };
            self.set_field(instance, &name.canonical, value);
        }

        let mut runtime = Runtime {
            project: self.project,
            program: function_block,
            env: BTreeMap::new(),
            types: BTreeMap::new(),
            il_accumulator: Value::Unit,
            diagnostics: Vec::new(),
            options: self.options.clone(),
            call_depth: self.call_depth + 1,
            communication: self.communication,
        };
        for field in function_block.variable_declarations() {
            runtime
                .types
                .insert(field.name.canonical.clone(), field.type_spec.clone());
            if function_block_field_specs(self.project, &field.type_spec).is_some() {
                self.copy_function_block_state_into_runtime(
                    &field_key(instance, &field.name.canonical),
                    &field.name.canonical,
                    &field.type_spec,
                    &mut runtime,
                );
            } else {
                let value = self
                    .env
                    .get(&field_key(instance, &field.name.canonical))
                    .cloned()
                    .unwrap_or_else(|| runtime.default_value(&field.type_spec));
                runtime.env.insert(field.name.canonical.clone(), value);
            }
        }

        match runtime.execute_statement_list(&function_block.body.statements) {
            Control::Continue | Control::Return => {}
            Control::Exit => runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                "EXIT used outside of an iteration",
                None,
            )),
            Control::Jump(label) => runtime.diagnostics.push(Diagnostic::error(
                DiagnosticCode::Runtime,
                format!("jump to unknown IL label '{label}'"),
                None,
            )),
        }

        for field in function_block.variable_declarations() {
            if function_block_field_specs(self.project, &field.type_spec).is_some() {
                self.copy_function_block_state_from_runtime(
                    &runtime,
                    &field.name.canonical,
                    &field_key(instance, &field.name.canonical),
                    &field.type_spec,
                );
            } else if let Some(value) = runtime.env.get(&field.name.canonical) {
                self.set_field(instance, &field.name.canonical, value.clone());
            }
        }

        self.diagnostics.extend(runtime.diagnostics);
        let mut positional_index = 0_usize;
        for arg in args {
            let Some((kind, name)) =
                user_fb_input_target(&input_fields, arg, &mut positional_index)
            else {
                continue;
            };
            if kind != VarBlockKind::InOut {
                continue;
            }
            let Some(Expr::Variable(variable)) = &arg.expr else {
                self.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!(
                        "VAR_IN_OUT parameter '{}' requires a variable actual",
                        name.original
                    ),
                    None,
                ));
                continue;
            };
            let value = self
                .env
                .get(&field_key(instance, &name.canonical))
                .cloned()
                .unwrap_or(Value::Unit);
            self.assign(variable, value);
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
            let value = self
                .env
                .get(&field_key(instance, &name.canonical))
                .cloned()
                .unwrap_or(Value::Unit);
            self.assign(variable, value);
        }
    }

    pub(crate) fn copy_function_block_state_into_runtime(
        &self,
        parent_prefix: &str,
        child_prefix: &str,
        spec: &DataTypeSpec,
        runtime: &mut Runtime<'_>,
    ) {
        let Some(fields) = function_block_field_specs(self.project, spec) else {
            return;
        };
        for field in fields {
            let parent_key = field_key(parent_prefix, &field.name);
            let child_key = field_key(child_prefix, &field.name);
            if function_block_field_specs(self.project, &field.spec).is_some() {
                self.copy_function_block_state_into_runtime(
                    &parent_key,
                    &child_key,
                    &field.spec,
                    runtime,
                );
            } else {
                let value = self
                    .env
                    .get(&parent_key)
                    .cloned()
                    .unwrap_or_else(|| runtime.default_value(&field.spec));
                runtime.env.insert(child_key, value);
            }
        }
    }

    pub(crate) fn copy_function_block_state_from_runtime(
        &mut self,
        runtime: &Runtime<'_>,
        child_prefix: &str,
        parent_prefix: &str,
        spec: &DataTypeSpec,
    ) {
        let Some(fields) = function_block_field_specs(self.project, spec) else {
            return;
        };
        for field in fields {
            let child_key = field_key(child_prefix, &field.name);
            let parent_key = field_key(parent_prefix, &field.name);
            if function_block_field_specs(self.project, &field.spec).is_some() {
                self.copy_function_block_state_from_runtime(
                    runtime,
                    &child_key,
                    &parent_key,
                    &field.spec,
                );
            } else if let Some(value) = runtime.env.get(&child_key) {
                self.env.insert(parent_key, value.clone());
            }
        }
    }

    pub(crate) fn edge_qualified_input_value(
        &mut self,
        instance: &str,
        input_name: &str,
        edge: EdgeQualifier,
        value: Value,
    ) -> Value {
        let current = value.as_bool().unwrap_or(false);
        let edge_field = edge_state_field_name(input_name);
        let previous = self.get_field_bool(instance, &edge_field);
        self.set_field(instance, &edge_field, Value::Bool(current));
        match edge {
            EdgeQualifier::Rising => Value::Bool(current && !previous),
            EdgeQualifier::Falling => Value::Bool(!current && previous),
        }
    }

    pub(crate) fn execute_ctu(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        if input_bool(inputs, "R") {
            self.set_field(instance, "CV", Value::Int(0));
        } else {
            let cu = input_bool(inputs, "CU");
            let old_cu = self.get_field_bool(instance, "_CU");
            if cu && !old_cu {
                let cv = self.get_field_i64(instance, "CV") + 1;
                self.set_field(instance, "CV", Value::Int(cv));
            }
            self.set_field(instance, "_CU", Value::Bool(cu));
        }
        let cv = self.get_field_i64(instance, "CV");
        let pv = input_i64(inputs, "PV");
        self.set_field(instance, "Q", Value::Bool(cv >= pv));
    }

    pub(crate) fn execute_ctd(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        if input_bool(inputs, "LD") {
            self.set_field(instance, "CV", Value::Int(input_i64(inputs, "PV")));
        } else {
            let cd = input_bool(inputs, "CD");
            let old_cd = self.get_field_bool(instance, "_CD");
            if cd && !old_cd {
                let cv = self.get_field_i64(instance, "CV") - 1;
                self.set_field(instance, "CV", Value::Int(cv));
            }
            self.set_field(instance, "_CD", Value::Bool(cd));
        }
        self.set_field(
            instance,
            "Q",
            Value::Bool(self.get_field_i64(instance, "CV") <= 0),
        );
    }

    pub(crate) fn execute_ctud(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        if input_bool(inputs, "R") {
            self.set_field(instance, "CV", Value::Int(0));
        } else if input_bool(inputs, "LD") {
            self.set_field(instance, "CV", Value::Int(input_i64(inputs, "PV")));
        } else {
            let cu = input_bool(inputs, "CU");
            let cd = input_bool(inputs, "CD");
            let old_cu = self.get_field_bool(instance, "_CU");
            let old_cd = self.get_field_bool(instance, "_CD");
            let mut cv = self.get_field_i64(instance, "CV");
            let cu_rising = cu && !old_cu;
            let cd_rising = cd && !old_cd;
            if cu_rising && !cd_rising {
                cv += 1;
            } else if cd_rising && !cu_rising {
                cv -= 1;
            }
            self.set_field(instance, "CV", Value::Int(cv));
            self.set_field(instance, "_CU", Value::Bool(cu));
            self.set_field(instance, "_CD", Value::Bool(cd));
        }
        let cv = self.get_field_i64(instance, "CV");
        let pv = input_i64(inputs, "PV");
        self.set_field(instance, "QU", Value::Bool(cv >= pv));
        self.set_field(instance, "QD", Value::Bool(cv <= 0));
    }

    pub(crate) fn execute_ton(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        let input = input_bool(inputs, "IN");
        let preset = input_time_ms(inputs, "PT").max(0);
        if !input {
            self.set_field(instance, "Q", Value::Bool(false));
            self.set_field(instance, "ET", Value::TimeMs(0));
        } else {
            let elapsed =
                (self.get_field_time_ms(instance, "ET") + self.options.cycle_time_ms).min(preset);
            self.set_field(instance, "ET", Value::TimeMs(elapsed));
            self.set_field(instance, "Q", Value::Bool(elapsed >= preset));
        }
        self.set_field(instance, "_IN", Value::Bool(input));
    }

    pub(crate) fn execute_tof(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        let input = input_bool(inputs, "IN");
        let preset = input_time_ms(inputs, "PT").max(0);
        if input {
            self.set_field(instance, "Q", Value::Bool(true));
            self.set_field(instance, "ET", Value::TimeMs(0));
        } else if self.get_field_bool(instance, "Q") {
            let elapsed =
                (self.get_field_time_ms(instance, "ET") + self.options.cycle_time_ms).min(preset);
            self.set_field(instance, "ET", Value::TimeMs(elapsed));
            if elapsed >= preset {
                self.set_field(instance, "Q", Value::Bool(false));
            }
        }
        self.set_field(instance, "_IN", Value::Bool(input));
    }

    pub(crate) fn execute_tp(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
        let input = input_bool(inputs, "IN");
        let preset = input_time_ms(inputs, "PT").max(0);
        let old_input = self.get_field_bool(instance, "_IN");
        let mut running = self.get_field_bool(instance, "_RUN");

        if input && !old_input && !running {
            running = true;
            self.set_field(instance, "ET", Value::TimeMs(0));
            self.set_field(instance, "Q", Value::Bool(true));
        }

        if running {
            let elapsed =
                (self.get_field_time_ms(instance, "ET") + self.options.cycle_time_ms).min(preset);
            self.set_field(instance, "ET", Value::TimeMs(elapsed));
            if elapsed >= preset {
                running = false;
                self.set_field(instance, "Q", Value::Bool(false));
            } else {
                self.set_field(instance, "Q", Value::Bool(true));
            }
        } else {
            self.set_field(instance, "Q", Value::Bool(false));
        }

        self.set_field(instance, "_IN", Value::Bool(input));
        self.set_field(instance, "_RUN", Value::Bool(running));
    }

    pub(crate) fn eval_fb_inputs(
        &mut self,
        type_name: &str,
        args: &[ParamAssignment],
    ) -> BTreeMap<String, Value> {
        let mut inputs = BTreeMap::new();
        let input_names = standard_function_block_input_names(type_name);
        let mut positional_index = 0_usize;
        for arg in args {
            if arg.output || arg.name.as_ref().is_some_and(is_implicit_en) {
                continue;
            }
            let input_name = if let Some(name) = &arg.name {
                Some(name.canonical.clone())
            } else {
                let name = input_names
                    .get(positional_index)
                    .map(|name| (*name).to_string());
                positional_index += 1;
                name
            };
            let Some(input_name) = input_name else {
                continue;
            };
            let value = arg
                .expr
                .as_ref()
                .and_then(|expr| self.eval_expr(expr))
                .unwrap_or(Value::Unit);
            inputs.insert(canonical_identifier(&input_name), value);
        }
        inputs
    }

    pub(crate) fn get_field_bool(&self, instance: &str, field: &str) -> bool {
        self.env
            .get(&field_key(instance, field))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    pub(crate) fn get_field_i64(&self, instance: &str, field: &str) -> i64 {
        self.env
            .get(&field_key(instance, field))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    }

    pub(crate) fn get_field_time_ms(&self, instance: &str, field: &str) -> i128 {
        match self.env.get(&field_key(instance, field)) {
            Some(Value::TimeMs(value)) => *value,
            Some(value) => value.as_i64().unwrap_or(0) as i128,
            None => 0,
        }
    }

    pub(crate) fn set_field(&mut self, instance: &str, field: &str, value: Value) {
        self.env.insert(field_key(instance, field), value);
    }

    pub(crate) fn case_label_matches(&mut self, label: &CaseLabel, selector: &Value) -> bool {
        match label {
            CaseLabel::Single(expr) => self
                .eval_expr(expr)
                .is_some_and(|value| compare_values(&value, selector) == Some(0)),
            CaseLabel::Range(low, high) => {
                let low = self.eval_expr(low).and_then(|value| value.as_i64());
                let high = self.eval_expr(high).and_then(|value| value.as_i64());
                let selector = selector.as_i64();
                match (low, high, selector) {
                    (Some(low), Some(high), Some(selector)) => selector >= low && selector <= high,
                    _ => false,
                }
            }
        }
    }
}
