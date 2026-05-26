use std::collections::BTreeMap;

use iec_diagnostics::{Diagnostic, DiagnosticCode};
use iec_ir::*;
use iec_stdlib::{eval_standard_function, is_communication_function_block, is_standard_function};

#[derive(Debug, Clone)]
pub struct RuntimeTrace {
    pub program: String,
    pub cycles: Vec<CycleTrace>,
}

#[derive(Debug, Clone)]
pub struct ConfigurationTrace {
    pub configuration: String,
    pub cycles: Vec<ConfigurationCycleTrace>,
}

#[derive(Debug, Clone)]
pub struct ConfigurationCycleTrace {
    pub cycle: usize,
    pub programs: Vec<ProgramInstanceTrace>,
}

#[derive(Debug, Clone)]
pub struct ProgramInstanceTrace {
    pub resource: String,
    pub instance: String,
    pub program: String,
    pub variables: Vec<(String, Value)>,
}

#[derive(Debug, Clone)]
pub struct CycleTrace {
    pub cycle: usize,
    pub variables: Vec<(String, Value)>,
}

#[derive(Debug, Clone)]
pub struct RuntimeOptions {
    pub max_loop_iterations: usize,
    pub max_scan_cycles: usize,
    pub cycle_time_ms: i128,
    pub warm_restart_before_cycles: Vec<usize>,
}

impl Default for RuntimeOptions {
    fn default() -> Self {
        Self {
            max_loop_iterations: 10_000,
            max_scan_cycles: 10_000,
            cycle_time_ms: 1,
            warm_restart_before_cycles: Vec::new(),
        }
    }
}

pub fn run_program(
    project: &Project,
    program_name: Option<&str>,
    cycles: usize,
    options: &RuntimeOptions,
) -> Result<RuntimeTrace, Vec<Diagnostic>> {
    if cycles > options.max_scan_cycles {
        return Err(vec![Diagnostic::error(
            DiagnosticCode::Compliance,
            format!(
                "scan cycle count {cycles} exceeds maximum {}",
                options.max_scan_cycles
            ),
            None,
        )]);
    }

    let Some(program) = find_program(project, program_name) else {
        return Err(vec![Diagnostic::error(
            DiagnosticCode::Runtime,
            "no PROGRAM POU found to execute",
            None,
        )]);
    };

    let mut runtime = Runtime {
        project,
        program,
        env: BTreeMap::new(),
        types: BTreeMap::new(),
        il_accumulator: Value::Unit,
        diagnostics: Vec::new(),
        options: options.clone(),
        call_depth: 0,
    };
    runtime.initialize(StartupKind::Cold);
    if !runtime.diagnostics.is_empty() {
        return Err(runtime.diagnostics);
    }

    let mut trace = RuntimeTrace {
        program: program.name.original.clone(),
        cycles: Vec::new(),
    };

    for cycle in 0..cycles {
        if options.warm_restart_before_cycles.contains(&cycle) {
            runtime.initialize(StartupKind::Warm);
        }
        match runtime.execute_program_cycle() {
            Control::Continue | Control::Return => {}
            Control::Exit => {
                runtime.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    "EXIT used outside of an iteration",
                    None,
                ));
                return Err(runtime.diagnostics);
            }
            Control::Jump(label) => {
                runtime.diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!("jump to unknown IL label '{label}'"),
                    None,
                ));
                return Err(runtime.diagnostics);
            }
        }
        trace.cycles.push(CycleTrace {
            cycle,
            variables: runtime.snapshot(),
        });
    }

    if runtime.diagnostics.is_empty() {
        Ok(trace)
    } else {
        Err(runtime.diagnostics)
    }
}

pub fn run_configuration(
    project: &Project,
    configuration_name: Option<&str>,
    cycles: usize,
    options: &RuntimeOptions,
) -> Result<ConfigurationTrace, Vec<Diagnostic>> {
    if cycles > options.max_scan_cycles {
        return Err(vec![Diagnostic::error(
            DiagnosticCode::Compliance,
            format!(
                "scan cycle count {cycles} exceeds maximum {}",
                options.max_scan_cycles
            ),
            None,
        )]);
    }

    let Some(configuration) = find_configuration(project, configuration_name) else {
        return Err(vec![Diagnostic::error(
            DiagnosticCode::Runtime,
            "no CONFIGURATION found to execute",
            None,
        )]);
    };

    let mut programs = Vec::new();
    let mut diagnostics = Vec::new();
    for resource in &configuration.resources {
        for instance in &resource.program_instances {
            let Some(program) = project
                .find_pou(&instance.program_type.original)
                .filter(|pou| matches!(&pou.kind, PouKind::Program))
            else {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Runtime,
                    format!(
                        "program instance '{}' references unknown PROGRAM type '{}'",
                        instance.name.original, instance.program_type.original
                    ),
                    None,
                ));
                continue;
            };
            let task = instance.task.as_ref().and_then(|task| {
                resource
                    .tasks
                    .iter()
                    .find(|candidate| candidate.name.canonical == task.canonical)
            });
            let mut runtime = Runtime {
                project,
                program,
                env: BTreeMap::new(),
                types: BTreeMap::new(),
                il_accumulator: Value::Unit,
                diagnostics: Vec::new(),
                options: options.clone(),
                call_depth: 0,
            };
            runtime.initialize(StartupKind::Cold);
            programs.push(ScheduledProgram {
                resource: resource.name.original.clone(),
                instance: instance.name.original.clone(),
                priority: task.and_then(|task| task.priority).unwrap_or(u32::MAX),
                interval_ms: task_interval_ms(task, options.cycle_time_ms),
                runtime,
            });
        }
    }
    if !diagnostics.is_empty() {
        return Err(diagnostics);
    }

    programs.sort_by_key(|program| (program.priority, program.instance.clone()));
    let mut trace = ConfigurationTrace {
        configuration: configuration.name.original.clone(),
        cycles: Vec::new(),
    };

    for cycle in 0..cycles {
        let mut cycle_trace = ConfigurationCycleTrace {
            cycle,
            programs: Vec::new(),
        };
        let elapsed_ms = cycle as i128 * options.cycle_time_ms;
        for scheduled in &mut programs {
            if scheduled.interval_ms > 0 && elapsed_ms % scheduled.interval_ms != 0 {
                continue;
            }
            match scheduled.runtime.execute_program_cycle() {
                Control::Continue | Control::Return => {}
                Control::Exit => {
                    scheduled.runtime.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        "EXIT used outside of an iteration",
                        None,
                    ));
                }
                Control::Jump(label) => {
                    scheduled.runtime.diagnostics.push(Diagnostic::error(
                        DiagnosticCode::Runtime,
                        format!("jump to unknown IL label '{label}'"),
                        None,
                    ));
                }
            }
            if !scheduled.runtime.diagnostics.is_empty() {
                return Err(scheduled.runtime.diagnostics.clone());
            }
            cycle_trace.programs.push(ProgramInstanceTrace {
                resource: scheduled.resource.clone(),
                instance: scheduled.instance.clone(),
                program: scheduled.runtime.program.name.original.clone(),
                variables: scheduled.runtime.snapshot(),
            });
        }
        trace.cycles.push(cycle_trace);
    }

    Ok(trace)
}

fn find_program<'a>(project: &'a Project, program_name: Option<&str>) -> Option<&'a Pou> {
    if let Some(name) = program_name {
        project
            .find_pou(name)
            .filter(|pou| matches!(&pou.kind, PouKind::Program))
    } else {
        project.first_program()
    }
}

fn find_configuration<'a>(
    project: &'a Project,
    configuration_name: Option<&str>,
) -> Option<&'a Configuration> {
    let expected = configuration_name.map(canonical_identifier);
    project.library_elements.iter().find_map(|element| {
        let LibraryElement::Configuration(configuration) = element else {
            return None;
        };
        if expected
            .as_ref()
            .is_none_or(|expected| *expected == configuration.name.canonical)
        {
            Some(configuration)
        } else {
            None
        }
    })
}

fn task_interval_ms(task: Option<&Task>, default_cycle_time_ms: i128) -> i128 {
    match task.and_then(|task| task.interval.as_ref()) {
        Some(Literal::DurationMs(value)) => (*value).max(1),
        Some(Literal::Int(value)) => (*value as i128).max(1),
        _ => default_cycle_time_ms.max(1),
    }
}

struct ScheduledProgram<'a> {
    resource: String,
    instance: String,
    priority: u32,
    interval_ms: i128,
    runtime: Runtime<'a>,
}

struct Runtime<'a> {
    project: &'a Project,
    program: &'a Pou,
    env: BTreeMap<String, Value>,
    types: BTreeMap<String, DataTypeSpec>,
    il_accumulator: Value,
    diagnostics: Vec<Diagnostic>,
    options: RuntimeOptions,
    call_depth: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StartupKind {
    Cold,
    Warm,
}

impl Runtime<'_> {
    fn initialize(&mut self, startup: StartupKind) {
        if startup == StartupKind::Cold {
            self.env.clear();
            self.types.clear();
        }
        self.il_accumulator = Value::Unit;

        if let PouKind::Function { return_type } = &self.program.kind {
            self.types
                .insert(self.program.name.canonical.clone(), return_type.clone());
            if startup == StartupKind::Cold || !self.env.contains_key(&self.program.name.canonical)
            {
                self.env.insert(
                    self.program.name.canonical.clone(),
                    default_value_for_type(return_type),
                );
            }
        }

        for block in &self.program.var_blocks {
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

    fn initialize_sfc_steps(&mut self) {
        let Some(sfc) = &self.program.body.sfc else {
            return;
        };
        for step in &sfc.steps {
            self.env
                .insert(sfc_step_key(&step.name), Value::Bool(step.initial));
        }
        for action in &sfc.actions {
            self.env
                .insert(sfc_action_key(&action.name), Value::Bool(false));
            self.env
                .insert(sfc_action_previous_key(&action.name), Value::Bool(false));
            self.env
                .insert(sfc_action_elapsed_key(&action.name), Value::Int(0));
        }
    }

    fn initial_value_for_spec(&mut self, spec: &DataTypeSpec, initial: Option<&Expr>) -> Value {
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

    fn default_value(&mut self, spec: &DataTypeSpec) -> Value {
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

    fn resolve_named_spec(&self, spec: &DataTypeSpec) -> DataTypeSpec {
        let DataTypeSpec::Named(name) = spec else {
            return spec.clone();
        };
        self.project
            .data_types()
            .find(|data_type| data_type.name.canonical == name.canonical)
            .map(|data_type| data_type.spec.clone())
            .unwrap_or_else(|| spec.clone())
    }

    fn initialize_function_block_fields(&mut self, var: &VarDecl) {
        let DataTypeSpec::Named(type_name) = &var.type_spec else {
            return;
        };

        match type_name.canonical.as_str() {
            "SR" | "RS" => {
                self.set_field(&var.name.canonical, "Q1", Value::Bool(false));
            }
            "R_TRIG" | "F_TRIG" => {
                self.set_field(&var.name.canonical, "Q", Value::Bool(false));
                self.set_field(&var.name.canonical, "M", Value::Bool(false));
            }
            "CTU" => {
                self.set_field(&var.name.canonical, "Q", Value::Bool(false));
                self.set_field(&var.name.canonical, "CV", Value::Int(0));
                self.set_field(&var.name.canonical, "_CU", Value::Bool(false));
            }
            "CTD" => {
                self.set_field(&var.name.canonical, "Q", Value::Bool(false));
                self.set_field(&var.name.canonical, "CV", Value::Int(0));
                self.set_field(&var.name.canonical, "_CD", Value::Bool(false));
            }
            "CTUD" => {
                self.set_field(&var.name.canonical, "QU", Value::Bool(false));
                self.set_field(&var.name.canonical, "QD", Value::Bool(false));
                self.set_field(&var.name.canonical, "CV", Value::Int(0));
                self.set_field(&var.name.canonical, "_CU", Value::Bool(false));
                self.set_field(&var.name.canonical, "_CD", Value::Bool(false));
            }
            "TON" | "TOF" | "TP" => {
                self.set_field(&var.name.canonical, "Q", Value::Bool(false));
                self.set_field(&var.name.canonical, "ET", Value::TimeMs(0));
                self.set_field(&var.name.canonical, "_IN", Value::Bool(false));
                self.set_field(&var.name.canonical, "_RUN", Value::Bool(false));
            }
            _ => self.initialize_user_function_block_fields(&var.name.canonical, type_name),
        }
    }

    fn initialize_user_function_block_fields(&mut self, instance: &str, type_name: &Identifier) {
        let Some(function_block) = self
            .project
            .find_pou(&type_name.original)
            .filter(|pou| matches!(&pou.kind, PouKind::FunctionBlock))
        else {
            return;
        };

        for field in function_block.variable_declarations() {
            let value = self.initial_value_for_spec(&field.type_spec, field.initial_value.as_ref());
            self.set_field(instance, &field.name.canonical, value);
        }
    }

    fn snapshot(&self) -> Vec<(String, Value)> {
        self.env
            .iter()
            .map(|(name, value)| (name.clone(), value.clone()))
            .collect()
    }

    fn execute_block(&mut self, body: &[Statement]) -> Control {
        for statement in body {
            match self.execute_statement(statement) {
                Control::Continue => {}
                control => return control,
            }
        }
        Control::Continue
    }

    fn execute_statement_list(&mut self, statements: &[Statement]) -> Control {
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

    fn execute_program_cycle(&mut self) -> Control {
        if let Some(sfc) = self.program.body.sfc.clone() {
            self.execute_sfc(&sfc)
        } else {
            self.execute_statement_list(&self.program.body.statements.clone())
        }
    }

    fn execute_sfc(&mut self, sfc: &Sfc) -> Control {
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
            let active = active_steps
                .iter()
                .any(|step| *step == action.name.canonical);
            if !self.sfc_action_should_execute(action, active) {
                continue;
            }
            match self.execute_statement_list(&action.body) {
                Control::Continue | Control::Return => {}
                control => return control,
            }
        }

        let mut fired = Vec::new();
        for (index, transition) in sfc.transitions.iter().enumerate() {
            let Some(from_step) = sfc.steps.get(index) else {
                continue;
            };
            let Some(to_step) = sfc.steps.get(index + 1) else {
                continue;
            };
            let from_active = self
                .env
                .get(&sfc_step_key(&from_step.name))
                .and_then(Value::as_bool)
                == Some(true);
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
                fired.push((from_step.name.clone(), to_step.name.clone()));
            }
        }

        for (from, to) in fired {
            self.env.insert(sfc_step_key(&from), Value::Bool(false));
            self.env.insert(sfc_step_key(&to), Value::Bool(true));
        }

        Control::Continue
    }

    fn sfc_action_should_execute(&mut self, action: &SfcAction, active: bool) -> bool {
        let previous_key = sfc_action_previous_key(&action.name);
        let was_active = self
            .env
            .get(&previous_key)
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.env.insert(previous_key, Value::Bool(active));

        match action.qualifier {
            SfcActionQualifier::NonStored => active,
            SfcActionQualifier::Pulse => active && !was_active,
            SfcActionQualifier::SetStored => {
                if active {
                    self.set_sfc_action_stored(&action.name, true);
                }
                self.sfc_action_stored(&action.name)
            }
            SfcActionQualifier::ResetStored => {
                if active {
                    self.set_sfc_action_stored(&action.name, false);
                    self.set_sfc_action_elapsed(&action.name, 0);
                }
                false
            }
            SfcActionQualifier::TimeLimited => {
                if !active {
                    self.set_sfc_action_elapsed(&action.name, 0);
                    return false;
                }
                let elapsed = self.advance_sfc_action_elapsed(&action.name);
                elapsed <= sfc_action_duration_ms(action)
            }
            SfcActionQualifier::TimeDelayed => {
                if !active {
                    self.set_sfc_action_elapsed(&action.name, 0);
                    return false;
                }
                let elapsed = self.advance_sfc_action_elapsed(&action.name);
                elapsed >= sfc_action_duration_ms(action)
            }
            SfcActionQualifier::StoredDelayed | SfcActionQualifier::DelayedStored => {
                if active {
                    let elapsed = self.advance_sfc_action_elapsed(&action.name);
                    if elapsed >= sfc_action_duration_ms(action) {
                        self.set_sfc_action_stored(&action.name, true);
                    }
                } else {
                    self.set_sfc_action_elapsed(&action.name, 0);
                }
                self.sfc_action_stored(&action.name)
            }
            SfcActionQualifier::StoredLimited => {
                if active && !self.sfc_action_stored(&action.name) {
                    self.set_sfc_action_stored(&action.name, true);
                    self.set_sfc_action_elapsed(&action.name, 0);
                }
                if !self.sfc_action_stored(&action.name) {
                    return false;
                }
                let elapsed = self.advance_sfc_action_elapsed(&action.name);
                if elapsed <= sfc_action_duration_ms(action) {
                    true
                } else {
                    self.set_sfc_action_stored(&action.name, false);
                    false
                }
            }
        }
    }

    fn sfc_action_stored(&self, action: &Identifier) -> bool {
        self.env
            .get(&sfc_action_key(action))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn set_sfc_action_stored(&mut self, action: &Identifier, value: bool) {
        self.env.insert(sfc_action_key(action), Value::Bool(value));
    }

    fn sfc_action_elapsed(&self, action: &Identifier) -> i128 {
        self.env
            .get(&sfc_action_elapsed_key(action))
            .and_then(Value::as_i64)
            .map(i128::from)
            .unwrap_or(0)
    }

    fn set_sfc_action_elapsed(&mut self, action: &Identifier, elapsed: i128) {
        self.env
            .insert(sfc_action_elapsed_key(action), Value::Int(elapsed as i64));
    }

    fn advance_sfc_action_elapsed(&mut self, action: &Identifier) -> i128 {
        let elapsed = self.sfc_action_elapsed(action) + self.options.cycle_time_ms.max(1);
        self.set_sfc_action_elapsed(action, elapsed);
        elapsed
    }

    fn execute_statement(&mut self, statement: &Statement) -> Control {
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
                self.execute_fb_call(name, statement);
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

    fn eval_expr(&mut self, expr: &Expr) -> Option<Value> {
        match expr {
            Expr::Literal(literal) => Some(literal_to_value(literal)),
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
                    if let Some(function) = self
                        .project
                        .find_pou(&name.original)
                        .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))
                    {
                        if let PouKind::Function { return_type } = &function.kind {
                            return Some(default_value_for_type(return_type));
                        }
                    }
                    return Some(Value::Int(0));
                }

                let mut values = Vec::new();
                for arg in args {
                    if arg.output || arg.name.as_ref().is_some_and(|name| is_implicit_en(name)) {
                        continue;
                    }
                    if let Some(expr) = &arg.expr {
                        if let Some(value) = self.eval_expr(expr) {
                            values.push(value);
                        }
                    }
                }
                if let Some(value) = eval_standard_function(&name.original, &values) {
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

    fn eval_user_function(&mut self, name: &Identifier, args: &[ParamAssignment]) -> Option<Value> {
        let function = self
            .project
            .find_pou(&name.original)
            .filter(|pou| matches!(&pou.kind, PouKind::Function { .. }))?;
        let PouKind::Function { return_type } = &function.kind else {
            return None;
        };

        if !self.function_call_enabled(args) {
            self.assign_function_eno(args, false);
            return Some(default_value_for_type(return_type));
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
            if arg.output || arg.name.as_ref().is_some_and(|name| is_implicit_en(name)) {
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

    fn function_call_enabled(&mut self, args: &[ParamAssignment]) -> bool {
        args.iter()
            .find(|arg| !arg.output && arg.name.as_ref().is_some_and(|name| is_implicit_en(name)))
            .and_then(|arg| arg.expr.as_ref())
            .and_then(|expr| self.eval_expr(expr))
            .and_then(|value| value.as_bool())
            .unwrap_or(true)
    }

    fn assign_function_eno(&mut self, args: &[ParamAssignment], value: bool) {
        for arg in args {
            if !arg.output || !arg.name.as_ref().is_some_and(|name| is_implicit_eno(name)) {
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

    fn bind_function_inputs(&mut self, positional: &[Value], named: &BTreeMap<String, Value>) {
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

    fn function_inputs(&self) -> impl Iterator<Item = &VarDecl> {
        self.program
            .var_blocks
            .iter()
            .filter(|block| block.kind == VarBlockKind::Input)
            .flat_map(|block| block.vars.iter())
    }

    fn eval_binary(&mut self, op: BinaryOp, left: Value, right: Value) -> Option<Value> {
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

    fn numeric_binary(
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

    fn time_or_numeric_binary(
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

    fn push_overflow(&mut self, operation: &str) {
        self.diagnostics.push(Diagnostic::error(
            DiagnosticCode::Runtime,
            format!("integer overflow during {operation}"),
            None,
        ));
    }

    fn resolve(&mut self, variable: &VariableRef) -> Option<Value> {
        if let Some(direct) = &variable.direct {
            return self.env.get(direct).cloned().or(Some(Value::Int(0)));
        }

        let Some(root) = variable.root_name() else {
            return None;
        };
        if let Some(ordinal) = self.enum_ordinal_name(&root.canonical) {
            return Some(Value::Int(ordinal));
        }
        if variable.path.len() == 2
            && variable.indices.iter().all(Vec::is_empty)
            && self
                .env
                .contains_key(&field_key(&root.canonical, &variable.path[1].canonical))
        {
            return self
                .env
                .get(&field_key(&root.canonical, &variable.path[1].canonical))
                .cloned();
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

    fn assign(&mut self, target: &VariableRef, value: Value) {
        if let Some(direct) = &target.direct {
            self.env.insert(direct.clone(), value);
            return;
        }
        let Some(root) = target.root_name() else {
            return;
        };
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

    fn apply_indices_to_value(
        &mut self,
        value: Value,
        spec: DataTypeSpec,
        indices: &[Expr],
    ) -> Option<(Value, DataTypeSpec)> {
        if indices.is_empty() {
            return Some((value, spec));
        }
        let resolved = self.resolve_named_spec(&spec);
        let DataTypeSpec::Array {
            ranges,
            element_type,
        } = resolved
        else {
            return None;
        };
        let offset = self.array_offset(&ranges, indices)?;
        let Value::Array(elements) = value else {
            return None;
        };
        elements
            .get(offset)
            .cloned()
            .map(|value| (value, *element_type))
    }

    fn assign_into_value(
        &mut self,
        current: &mut Value,
        spec: &DataTypeSpec,
        target: &VariableRef,
        segment_index: usize,
        value: Value,
    ) -> bool {
        let mut current_spec = self.resolve_named_spec(spec);
        if let Some(indices) = target.indices.get(segment_index) {
            if !indices.is_empty() {
                let DataTypeSpec::Array {
                    ranges,
                    element_type,
                } = current_spec
                else {
                    return false;
                };
                let Some(offset) = self.array_offset(&ranges, indices) else {
                    return false;
                };
                let Value::Array(elements) = current else {
                    return false;
                };
                let Some(element) = elements.get_mut(offset) else {
                    return false;
                };
                current_spec = *element_type;
                if segment_index + 1 >= target.path.len() {
                    *element = self.constrain_value(&current_spec, value);
                    return true;
                }
                return self.assign_into_value(
                    element,
                    &current_spec,
                    target,
                    segment_index + 1,
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

    fn array_offset(&mut self, ranges: &[Subrange], indices: &[Expr]) -> Option<usize> {
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

    fn constrain_value(&mut self, spec: &DataTypeSpec, value: Value) -> Value {
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
            } => {
                if let Value::String(text) = &value {
                    if text.chars().count() > length {
                        self.diagnostics.push(Diagnostic::error(
                            DiagnosticCode::Runtime,
                            format!(
                                "string value exceeds length {length} with {} character(s)",
                                text.chars().count()
                            ),
                            None,
                        ));
                    }
                }
                value
            }
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

    fn enum_ordinal_expr(&self, expr: &Expr) -> Option<i64> {
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

    fn enum_ordinal_name(&self, canonical_name: &str) -> Option<i64> {
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

    fn execute_il_instruction(&mut self, op: IlOp, operand: Option<&Expr>) -> Control {
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

    fn execute_il_call(&mut self, operand: Option<&Expr>) {
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

    fn execute_fb_call(&mut self, name: &VariableRef, statement: &Statement) {
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

        let inputs = self.eval_fb_inputs(args);
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
                    self.diagnostics.push(Diagnostic::warning(
                        DiagnosticCode::Unsupported,
                        format!(
                            "communication function block '{}' is not simulated",
                            type_name.original
                        ),
                        None,
                    ));
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
                }
            }
        }
    }

    fn execute_user_function_block(
        &mut self,
        instance: &str,
        function_block: &Pou,
        args: &[ParamAssignment],
    ) {
        for arg in args {
            if arg.output {
                continue;
            }
            let Some(name) = &arg.name else {
                continue;
            };
            let value = arg
                .expr
                .as_ref()
                .and_then(|expr| self.eval_expr(expr))
                .unwrap_or(Value::Unit);
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
        };
        for field in function_block.variable_declarations() {
            runtime
                .types
                .insert(field.name.canonical.clone(), field.type_spec.clone());
            let value = self
                .env
                .get(&field_key(instance, &field.name.canonical))
                .cloned()
                .unwrap_or_else(|| runtime.default_value(&field.type_spec));
            runtime.env.insert(field.name.canonical.clone(), value);
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
            if let Some(value) = runtime.env.get(&field.name.canonical) {
                self.set_field(instance, &field.name.canonical, value.clone());
            }
        }

        self.diagnostics.extend(runtime.diagnostics);
        for arg in args {
            if !arg.output {
                continue;
            }
            let (Some(name), Some(variable)) = (&arg.name, &arg.variable) else {
                continue;
            };
            let value = self
                .env
                .get(&field_key(instance, &name.canonical))
                .cloned()
                .unwrap_or(Value::Unit);
            self.assign(variable, value);
        }
    }

    fn execute_ctu(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
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

    fn execute_ctd(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
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

    fn execute_ctud(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
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
            if cu && !old_cu && !(cd && !old_cd) {
                cv += 1;
            } else if cd && !old_cd && !(cu && !old_cu) {
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

    fn execute_ton(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
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

    fn execute_tof(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
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

    fn execute_tp(&mut self, instance: &str, inputs: &BTreeMap<String, Value>) {
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

    fn eval_fb_inputs(&mut self, args: &[ParamAssignment]) -> BTreeMap<String, Value> {
        let mut inputs = BTreeMap::new();
        for arg in args {
            if arg.output {
                continue;
            }
            let Some(name) = &arg.name else {
                continue;
            };
            let value = arg
                .expr
                .as_ref()
                .and_then(|expr| self.eval_expr(expr))
                .unwrap_or(Value::Unit);
            inputs.insert(name.canonical.clone(), value);
        }
        inputs
    }

    fn get_field_bool(&self, instance: &str, field: &str) -> bool {
        self.env
            .get(&field_key(instance, field))
            .and_then(Value::as_bool)
            .unwrap_or(false)
    }

    fn get_field_i64(&self, instance: &str, field: &str) -> i64 {
        self.env
            .get(&field_key(instance, field))
            .and_then(Value::as_i64)
            .unwrap_or(0)
    }

    fn get_field_time_ms(&self, instance: &str, field: &str) -> i128 {
        match self.env.get(&field_key(instance, field)) {
            Some(Value::TimeMs(value)) => *value,
            Some(value) => value.as_i64().unwrap_or(0) as i128,
            None => 0,
        }
    }

    fn set_field(&mut self, instance: &str, field: &str, value: Value) {
        self.env.insert(field_key(instance, field), value);
    }

    fn case_label_matches(&mut self, label: &CaseLabel, selector: &Value) -> bool {
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

#[derive(Debug, Clone, PartialEq, Eq)]
enum Control {
    Continue,
    Exit,
    Return,
    Jump(String),
}

fn literal_to_value(literal: &Literal) -> Value {
    match literal {
        Literal::Int(value) => Value::Int(*value),
        Literal::Real(value) => Value::Real(*value),
        Literal::Bool(value) => Value::Bool(*value),
        Literal::String(value) => Value::String(value.clone()),
        Literal::DurationMs(value) => Value::TimeMs(*value),
        Literal::Date(_) | Literal::TimeOfDay(_) | Literal::DateAndTime(_) => Value::TimeMs(0),
        Literal::Typed { value, .. } => value
            .parse::<i64>()
            .map(Value::Int)
            .unwrap_or_else(|_| Value::String(value.clone())),
    }
}

fn array_element_count(ranges: &[Subrange]) -> usize {
    ranges.iter().fold(1_usize, |total, range| {
        total.saturating_mul((range.high - range.low + 1).max(0) as usize)
    })
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

fn il_label_operand(expr: &Expr) -> Option<&Identifier> {
    let Expr::Variable(variable) = expr else {
        return None;
    };
    if variable.direct.is_some() || variable.path.len() != 1 {
        return None;
    }
    variable.root_name()
}

fn field_key(instance: &str, field: &str) -> String {
    format!(
        "{}.{}",
        canonical_identifier(instance),
        canonical_identifier(field)
    )
}

fn sfc_step_key(step: &Identifier) -> String {
    format!("$SFC_STEP_{}", step.canonical)
}

fn sfc_action_key(action: &Identifier) -> String {
    format!("$SFC_ACTION_{}", action.canonical)
}

fn sfc_action_previous_key(action: &Identifier) -> String {
    format!("$SFC_ACTION_PREVIOUS_{}", action.canonical)
}

fn sfc_action_elapsed_key(action: &Identifier) -> String {
    format!("$SFC_ACTION_ELAPSED_{}", action.canonical)
}

fn sfc_action_duration_ms(action: &SfcAction) -> i128 {
    match action.duration.as_ref() {
        Some(Literal::DurationMs(value)) => (*value).max(0),
        Some(Literal::Int(value)) => (*value as i128).max(0),
        _ => 0,
    }
}

fn is_implicit_en(name: &Identifier) -> bool {
    name.canonical == "EN"
}

fn is_implicit_eno(name: &Identifier) -> bool {
    name.canonical == "ENO"
}

fn input_bool(inputs: &BTreeMap<String, Value>, name: &str) -> bool {
    inputs
        .get(&canonical_identifier(name))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn input_i64(inputs: &BTreeMap<String, Value>, name: &str) -> i64 {
    inputs
        .get(&canonical_identifier(name))
        .and_then(Value::as_i64)
        .unwrap_or(0)
}

fn input_time_ms(inputs: &BTreeMap<String, Value>, name: &str) -> i128 {
    match inputs.get(&canonical_identifier(name)) {
        Some(Value::TimeMs(value)) => *value,
        Some(value) => value.as_i64().unwrap_or(0) as i128,
        None => 0,
    }
}

fn bit_bool_binary(
    left: Value,
    right: Value,
    int_op: fn(i64, i64) -> i64,
    bool_op: fn(bool, bool) -> bool,
) -> Option<Value> {
    if matches!(left, Value::Bool(_)) && matches!(right, Value::Bool(_)) {
        Some(Value::Bool(bool_op(left.as_bool()?, right.as_bool()?)))
    } else {
        Some(Value::Int(int_op(left.as_i64()?, right.as_i64()?)))
    }
}

fn compare_values(left: &Value, right: &Value) -> Option<i8> {
    if matches!(left, Value::String(_)) || matches!(right, Value::String(_)) {
        let left = left.to_string();
        let right = right.to_string();
        return Some(match left.cmp(&right) {
            std::cmp::Ordering::Less => -1,
            std::cmp::Ordering::Equal => 0,
            std::cmp::Ordering::Greater => 1,
        });
    }

    let left = left.as_f64()?;
    let right = right.as_f64()?;
    if (left - right).abs() < f64::EPSILON {
        Some(0)
    } else if left < right {
        Some(-1)
    } else {
        Some(1)
    }
}

#[cfg(test)]
mod tests {
    use iec_semantics::{check_project, CheckOptions};
    use iec_syntax::parse_project;

    use super::*;

    #[test]
    fn executes_counter_program() {
        let source = r#"
            PROGRAM Demo
            VAR Count : INT := 0; Done : BOOL := FALSE; END_VAR
            Count := Count + 1;
            IF Count >= 2 THEN Done := TRUE; END_IF;
            END_PROGRAM
        "#;
        let output = parse_project("test.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn enforces_max_scan_cycles() {
        let source = r#"
            PROGRAM Demo
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM
        "#;
        let output = parse_project("scan_limit.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let result = run_program(
            &output.project,
            Some("Demo"),
            3,
            &RuntimeOptions {
                max_scan_cycles: 2,
                ..RuntimeOptions::default()
            },
        );
        let diagnostics = result.expect_err("scan limit should reject the run");
        assert!(diagnostics
            .iter()
            .any(|diagnostic| diagnostic.message.contains("scan cycle count 3")));
    }

    #[test]
    fn runs_configuration_tasks_by_interval_and_priority() {
        let source = r#"
            PROGRAM FastProgram
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            PROGRAM SlowProgram
            VAR Count : INT := 0; END_VAR
            Count := Count + 1;
            END_PROGRAM

            CONFIGURATION Plant
            RESOURCE Cpu ON PLC
                TASK Fast(INTERVAL := T#1ms, PRIORITY := 1);
                TASK Slow(INTERVAL := T#2ms, PRIORITY := 2);
                PROGRAM FastInstance WITH Fast : FastProgram;
                PROGRAM SlowInstance WITH Slow : SlowProgram;
            END_RESOURCE
            END_CONFIGURATION
        "#;
        let output = parse_project("configuration_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_configuration(
            &output.project,
            Some("Plant"),
            3,
            &RuntimeOptions::default(),
        )
        .expect("configuration should run");
        assert_eq!(trace.cycles.len(), 3);
        assert_eq!(trace.cycles[0].programs[0].instance, "FastInstance");
        assert_eq!(trace.cycles[0].programs[1].instance, "SlowInstance");
        assert_eq!(trace.cycles[1].programs.len(), 1);
        let fast_last = trace
            .cycles
            .last()
            .unwrap()
            .programs
            .iter()
            .find(|program| program.instance == "FastInstance")
            .unwrap();
        assert!(fast_last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(3)));
        let slow_last = trace.cycles[2]
            .programs
            .iter()
            .find(|program| program.instance == "SlowInstance")
            .unwrap();
        assert!(slow_last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
    }

    #[test]
    fn executes_loops_case_and_standard_functions() {
        let source = r#"
            PROGRAM Demo
            VAR
                I : INT := 0;
                Total : INT := 0;
                Selected : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            FOR I := 1 TO 3 DO
                Total := Total + I;
            END_FOR;

            WHILE Total < 8 DO
                Total := Total + 1;
            END_WHILE;

            REPEAT
                Total := Total - 1;
            UNTIL Total = 7
            END_REPEAT;

            Selected := MAX(Total, 3);

            CASE Selected OF
                7: Done := TRUE;
                ELSE Done := FALSE;
            END_CASE;
            END_PROGRAM
        "#;
        let output = parse_project("test.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TOTAL" && *value == Value::Int(7)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SELECTED" && *value == Value::Int(7)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_standard_power_precedence_and_associativity() {
        let source = r#"
            PROGRAM Demo
            VAR
                RightAssoc : REAL := 0.0;
                NegatedPower : REAL := 0.0;
            END_VAR
            RightAssoc := 2 ** 3 ** 2;
            NegatedPower := -2 ** 2;
            END_PROGRAM
        "#;
        let output = parse_project("power_precedence.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "RIGHTASSOC" && *value == Value::Real(512.0)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "NEGATEDPOWER" && *value == Value::Real(-4.0)));
    }

    #[test]
    fn executes_user_defined_functions() {
        let source = r#"
            FUNCTION Scale : INT
            VAR_INPUT
                Input : INT;
                Factor : INT;
            END_VAR
            Scale := Input * Factor;
            END_FUNCTION

            PROGRAM Demo
            VAR
                A : INT := 4;
                B : INT := 0;
                C : INT := 0;
            END_VAR
            B := Scale(A, 3);
            C := Scale(Input := B, Factor := 2);
            END_PROGRAM
        "#;
        let output = parse_project("functions.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "B" && *value == Value::Int(12)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "C" && *value == Value::Int(24)));
    }

    #[test]
    fn executes_function_en_eno_controls() {
        let source = r#"
            FUNCTION Scale : INT
            VAR_INPUT
                Input : INT;
            END_VAR
            Scale := Input * 2;
            END_FUNCTION

            PROGRAM Demo
            VAR
                EnabledResult : INT := 0;
                DisabledResult : INT := 5;
                EnabledOk : BOOL := FALSE;
                DisabledOk : BOOL := TRUE;
            END_VAR

            EnabledResult := Scale(EN := TRUE, Input := 3, ENO => EnabledOk);
            DisabledResult := Scale(EN := FALSE, Input := 10, ENO => DisabledOk);
            END_PROGRAM
        "#;
        let output = parse_project("function_controls.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ENABLEDRESULT" && *value == Value::Int(6)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DISABLEDRESULT" && *value == Value::Int(0)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ENABLEDOK" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DISABLEDOK" && *value == Value::Bool(false)));
    }

    #[test]
    fn executes_expanded_standard_functions() {
        let source = r#"
            PROGRAM Demo
            VAR
                Sum : INT := 0;
                Product : INT := 0;
                Choice : INT := 0;
                Shifted : INT := 0;
                Rotated : INT := 0;
                Ok : BOOL := FALSE;
            END_VAR

            Sum := ADD(1, 2, 3);
            Product := MUL(Sum, 2);
            Choice := MUX(2, 10, 20, 30);
            Shifted := SHL(1, 3);
            Rotated := ROL(1, 1);
            Ok := GT(Product, Sum) AND EQ(MOVE(Choice), 30) AND NE(Shifted, Rotated);
            END_PROGRAM
        "#;
        let output = parse_project("standard_functions.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SUM" && *value == Value::Int(6)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "PRODUCT" && *value == Value::Int(12)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "CHOICE" && *value == Value::Int(30)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SHIFTED" && *value == Value::Int(8)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ROTATED" && *value == Value::Int(2)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OK" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_string_bit_and_time_standard_functions() {
        let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING := '';
                Found : INT := 0;
                Mask : INT := 0;
                Flag : BOOL := FALSE;
                Delay : TIME := T#0ms;
            END_VAR

            Text := CONCAT(LEFT('robot', 2), RIGHT('code', 2));
            Found := FIND(Text, 'de');
            Mask := OR(AND(15, 51), XOR(1, 3));
            Flag := XOR(TRUE, FALSE);
            Delay := ADD_TIME(T#1s, MUL_TIME(T#100ms, 2));
            END_PROGRAM
        "#;
        let output = parse_project("standard_catalog.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TEXT" && *value == Value::String("rode".to_string())));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FOUND" && *value == Value::Int(3)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "MASK" && *value == Value::Int(3)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DELAY" && *value == Value::TimeMs(1200)));
    }

    #[test]
    fn executes_expanded_conversion_functions() {
        let source = r#"
            PROGRAM Demo
            VAR
                Text : STRING[32] := '';
                Parsed : INT := 0;
                Truncated : INT := 0;
                Bcd : WORD := 0;
                FromBcd : INT := 0;
                RealValue : REAL := 0.0;
                Flag : BOOL := FALSE;
                Delay : TIME := T#0ms;
            END_VAR

            Parsed := STRING_TO_INT('42');
            Truncated := TRUNC(-1.6);
            Bcd := INT_TO_BCD(369);
            FromBcd := BCD_TO_INT(Bcd) + WORD_BCD_TO_UINT(UINT_TO_BCD_WORD(25));
            RealValue := STRING_TO_REAL('2.5');
            Flag := STRING_TO_BOOL('TRUE');
            Delay := STRING_TO_TIME('T#250ms') + INT_TO_TIME(50);
            Text := CONCAT(BOOL_TO_STRING(Flag), INT_TO_STRING(Parsed));
            END_PROGRAM
        "#;
        let output = parse_project("conversions.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "PARSED" && *value == Value::Int(42)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TRUNCATED" && *value == Value::Int(-1)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "BCD" && *value == Value::Int(0x369)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FROMBCD" && *value == Value::Int(394)));
        assert!(last.variables.iter().any(|(name, value)| {
            name == "REALVALUE"
                && matches!(value, Value::Real(value) if (*value - 2.5).abs() < f64::EPSILON)
        }));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DELAY" && *value == Value::TimeMs(300)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TEXT" && *value == Value::String("TRUE42".to_string())));
    }

    #[test]
    fn executes_bool_and_bit_string_st_operators() {
        let source = r#"
            PROGRAM Demo
            VAR
                Mask : INT := 0;
                Inverted : INT := 0;
                Flag : BOOL := FALSE;
            END_VAR

            Mask := (15 AND 51) OR (1 XOR 3);
            Inverted := NOT 15;
            Flag := (TRUE AND FALSE) OR TRUE;
            END_PROGRAM
        "#;
        let output = parse_project("st_bit_ops.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "MASK" && *value == Value::Int(3)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "INVERTED" && *value == Value::Int(!15)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "FLAG" && *value == Value::Bool(true)));
    }

    #[test]
    fn short_circuits_bool_and_or_operands() {
        let source = r#"
            PROGRAM Demo
            VAR
                Ok1 : BOOL := TRUE;
                Ok2 : BOOL := FALSE;
            END_VAR

            Ok1 := FALSE AND (1 / 0 = 0);
            Ok2 := TRUE OR (1 / 0 = 0);
            END_PROGRAM
        "#;
        let output = parse_project("short_circuit.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OK1" && *value == Value::Bool(false)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OK2" && *value == Value::Bool(true)));
    }

    #[test]
    fn reports_integer_overflow() {
        let source = r#"
            PROGRAM Demo
            VAR A : LINT := 9223372036854775807; END_VAR
            A := A + 1;
            END_PROGRAM
        "#;
        let output = parse_project("overflow.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
        let diagnostics = result.expect_err("overflow should reject the run");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("integer overflow during addition")));
    }

    #[test]
    fn executes_arrays_structs_enums_and_subrange_checks() {
        let source = r#"
            TYPE
                Small : INT(0..10);
                Mode : (Idle, Run, Fault);
                Pair : STRUCT
                    Low : Small := 1;
                    High : Small := 2;
                END_STRUCT;
            END_TYPE

            PROGRAM Demo
            VAR
                Values : ARRAY [1..3] OF Small := [1, 2, 3];
                Window : Pair := (Low := 4, High := 6);
                State : Mode := Idle;
                Total : INT := 0;
                IsRun : BOOL := FALSE;
            END_VAR

            Values[2] := Values[1] + Window.High;
            Window.Low := Values[2];
            State := Run;
            IsRun := State = Run;
            Total := Values[2] + Window.Low;
            END_PROGRAM
        "#;
        let output = parse_project("aggregates_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "TOTAL" && *value == Value::Int(14)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ISRUN" && *value == Value::Bool(true)));
    }

    #[test]
    fn rejects_runtime_subrange_violations() {
        let source = r#"
            TYPE Small : INT(0..10); END_TYPE
            PROGRAM Demo
            VAR Value : Small := 1; END_VAR
            Value := 11;
            END_PROGRAM
        "#;
        let output = parse_project("subrange_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("assignment to 'Value' value 11 is outside subrange 0..10")));
        let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
        let diagnostics = result.expect_err("runtime should reject subrange violation");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("value 11 is outside subrange 0..10")));
    }

    #[test]
    fn rejects_runtime_elementary_range_and_conversion_violations() {
        let elementary_source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 300;
                ByteValue : BYTE := 0;
            END_VAR
            ByteValue := Count;
            END_PROGRAM
        "#;
        let output = parse_project("elementary_range_runtime.st", elementary_source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
        let diagnostics = result.expect_err("runtime should reject BYTE range violation");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("value 300 is outside BYTE range 0..255")));

        let conversion_source = r#"
            PROGRAM Demo
            VAR
                Count : INT := 300;
                Converted : USINT := 0;
            END_VAR
            Converted := INT_TO_USINT(Count);
            END_PROGRAM
        "#;
        let output = parse_project("conversion_range_runtime.st", conversion_source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let result = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default());
        let diagnostics = result.expect_err("runtime should reject conversion range violation");
        assert!(diagnostics.iter().any(|diagnostic| diagnostic
            .message
            .contains("standard function 'INT_TO_USINT' failed for supplied arguments")));
    }

    #[test]
    fn preserves_retain_variables_across_warm_restart() {
        let source = r#"
            PROGRAM Demo
            VAR RETAIN
                Kept : INT := 10;
            END_VAR
            VAR NON_RETAIN
                Reset : INT := 10;
            END_VAR
            VAR
                Plain : INT := 10;
            END_VAR

            Kept := Kept + 1;
            Reset := Reset + 1;
            Plain := Plain + 1;
            END_PROGRAM
        "#;
        let output = parse_project("retain_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let options = RuntimeOptions {
            warm_restart_before_cycles: vec![2],
            ..RuntimeOptions::default()
        };
        let trace =
            run_program(&output.project, Some("Demo"), 4, &options).expect("program should run");
        let before_restart = &trace.cycles[1].variables;
        let after_restart = &trace.cycles[2].variables;
        let last = &trace.cycles[3].variables;

        assert!(before_restart
            .iter()
            .any(|(name, value)| name == "KEPT" && *value == Value::Int(12)));
        assert!(after_restart
            .iter()
            .any(|(name, value)| name == "KEPT" && *value == Value::Int(13)));
        assert!(after_restart
            .iter()
            .any(|(name, value)| name == "RESET" && *value == Value::Int(11)));
        assert!(after_restart
            .iter()
            .any(|(name, value)| name == "PLAIN" && *value == Value::Int(11)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "KEPT" && *value == Value::Int(14)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "RESET" && *value == Value::Int(12)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "PLAIN" && *value == Value::Int(12)));
    }

    #[test]
    fn executes_standard_counter_function_block() {
        let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                Pulse : BOOL := FALSE;
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            Pulse := NOT Pulse;
            Counter(CU := Pulse, R := FALSE, PV := 2);
            Count := Counter.CV;
            Done := Counter.Q;
            END_PROGRAM
        "#;
        let output = parse_project("fb_counter.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 3, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(2)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNTER.CV" && *value == Value::Int(2)));
    }

    #[test]
    fn executes_user_defined_function_block_state() {
        let source = r#"
            FUNCTION_BLOCK Accumulator
            VAR_INPUT
                In : INT;
                Reset : BOOL;
            END_VAR
            VAR_OUTPUT
                Total : INT;
            END_VAR
            VAR
                Step : INT := 1;
            END_VAR

            IF Reset THEN
                Total := 0;
            ELSE
                Total := Total + In + Step;
            END_IF;
            END_FUNCTION_BLOCK

            PROGRAM Demo
            VAR
                Acc : Accumulator;
                Out : INT := 0;
            END_VAR

            Acc(In := 2, Reset := FALSE, Total => Out);
            END_PROGRAM
        "#;
        let output = parse_project("user_fb.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "ACC.TOTAL" && *value == Value::Int(6)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "OUT" && *value == Value::Int(6)));
    }

    #[test]
    fn executes_bistable_and_edge_function_blocks() {
        let source = r#"
            PROGRAM Demo
            VAR
                Latch : SR;
                Edge : R_TRIG;
                Input : BOOL := FALSE;
                Latched : BOOL := FALSE;
                Rising : BOOL := FALSE;
            END_VAR

            Input := NOT Input;
            Latch(S1 := Input, R := FALSE);
            Edge(CLK := Input);
            Latched := Latch.Q1;
            Rising := Edge.Q;
            END_PROGRAM
        "#;
        let output = parse_project("fb_bits.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 2, &RuntimeOptions::default())
            .expect("program should run");
        let first = &trace.cycles[0];
        assert!(first
            .variables
            .iter()
            .any(|(name, value)| name == "LATCH.Q1" && *value == Value::Bool(true)));
        assert!(first
            .variables
            .iter()
            .any(|(name, value)| name == "RISING" && *value == Value::Bool(true)));

        let second = &trace.cycles[1];
        assert!(second
            .variables
            .iter()
            .any(|(name, value)| name == "LATCH.Q1" && *value == Value::Bool(true)));
        assert!(second
            .variables
            .iter()
            .any(|(name, value)| name == "RISING" && *value == Value::Bool(false)));
    }

    #[test]
    fn executes_timer_function_blocks_with_cycle_time() {
        let source = r#"
            PROGRAM Demo
            VAR
                Delay : TON;
                Pulse : TP;
                Done : BOOL := FALSE;
                PulseDone : BOOL := FALSE;
                Elapsed : TIME := T#0ms;
            END_VAR

            Delay(IN := TRUE, PT := T#2ms);
            Pulse(IN := TRUE, PT := T#2ms);
            Done := Delay.Q;
            PulseDone := Pulse.Q;
            Elapsed := Delay.ET;
            END_PROGRAM
        "#;
        let output = parse_project("timers.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 3, &RuntimeOptions::default())
            .expect("program should run");

        let first = &trace.cycles[0];
        assert!(first
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(false)));
        assert!(first
            .variables
            .iter()
            .any(|(name, value)| name == "PULSEDONE" && *value == Value::Bool(true)));

        let second = &trace.cycles[1];
        assert!(second
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
        assert!(second
            .variables
            .iter()
            .any(|(name, value)| name == "ELAPSED" && *value == Value::TimeMs(2)));

        let third = &trace.cycles[2];
        assert!(third
            .variables
            .iter()
            .any(|(name, value)| name == "PULSEDONE" && *value == Value::Bool(false)));
    }

    #[test]
    fn executes_textual_sfc_scan_evolution() {
        let source = r#"
            PROGRAM Sequence
            VAR
                Ready : BOOL := TRUE;
                Done : BOOL := FALSE;
            END_VAR

            INITIAL_STEP Start;
            STEP Run;
            TRANSITION Go := Ready;
            ACTION Run:
                Done := TRUE;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_runtime.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("Sequence"),
            2,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let first = &trace.cycles[0].variables;
        assert!(first
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(false)));
        assert!(first
            .iter()
            .any(|(name, value)| name == "$SFC_STEP_RUN" && *value == Value::Bool(true)));
        let second = &trace.cycles[1].variables;
        assert!(second
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_sfc_action_qualifiers_and_timers() {
        let source = r#"
            PROGRAM Qualifiers
            VAR
                PulseCount : INT := 0;
                DelayCount : INT := 0;
                LimitCount : INT := 0;
            END_VAR

            INITIAL_STEP Pulse;
            STEP Delay;
            STEP Limit;
            TRANSITION ToDelay := PulseCount >= 1;
            TRANSITION ToLimit := DelayCount >= 1;
            ACTION Pulse(P):
                PulseCount := PulseCount + 1;
            END_ACTION;
            ACTION Delay(D, T#2ms):
                DelayCount := DelayCount + 1;
            END_ACTION;
            ACTION Limit(L, T#2ms):
                LimitCount := LimitCount + 1;
            END_ACTION;
            END_PROGRAM
        "#;
        let output = parse_project("sfc_qualifiers.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("Qualifiers"),
            6,
            &RuntimeOptions {
                cycle_time_ms: 1,
                ..RuntimeOptions::default()
            },
        )
        .expect("program should run");
        let last = &trace.cycles.last().unwrap().variables;
        assert!(last
            .iter()
            .any(|(name, value)| name == "PULSECOUNT" && *value == Value::Int(1)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "DELAYCOUNT" && *value == Value::Int(1)));
        assert!(last
            .iter()
            .any(|(name, value)| name == "LIMITCOUNT" && *value == Value::Int(2)));
    }

    #[test]
    fn executes_basic_instruction_list() {
        let source = r#"
            PROGRAM IlDemo
            VAR
                A : INT := 3;
                B : INT := 4;
                C : INT := 0;
                Bigger : BOOL := FALSE;
                Complex : BOOL := FALSE;
            END_VAR

            LD A;
            ADD B;
            ST C;
            GT 5;
            ST Bigger;
            LD TRUE;
            AND (Bigger OR FALSE);
            ST Complex;
            END_PROGRAM
        "#;
        let output = parse_project("il.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("IlDemo"),
            1,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "C" && *value == Value::Int(7)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "BIGGER" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COMPLEX" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_instruction_list_jumps() {
        let source = r#"
            PROGRAM IlJumpDemo
            VAR
                Count : INT := 0;
                Done : BOOL := FALSE;
            END_VAR

            LD Count;
            GE 3;
            JMPC DoneLabel;
            LD Count;
            ADD 1;
            ST Count;
            JMP EndLabel;
            DoneLabel:
            LD TRUE;
            ST Done;
            EndLabel:
            END_PROGRAM
        "#;
        let output = parse_project("il_jump.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(
            &output.project,
            Some("IlJumpDemo"),
            4,
            &RuntimeOptions::default(),
        )
        .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "COUNT" && *value == Value::Int(3)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
    }

    #[test]
    fn executes_instruction_list_calls_and_conditional_returns() {
        let source = r#"
            PROGRAM Demo
            VAR
                Counter : CTU;
                Done : BOOL := FALSE;
                Cv : INT := 0;
                Skipped : INT := 0;
            END_VAR

            LD TRUE;
            CALC Counter(CU := TRUE, R := FALSE, PV := 1);
            LD Counter.Q;
            ST Done;
            LD Counter.CV;
            ST Cv;
            LD TRUE;
            RETC;
            Skipped := 1;
            END_PROGRAM
        "#;
        let output = parse_project("il_calls.st", source);
        assert!(output.diagnostics.is_empty(), "{:?}", output.diagnostics);
        let diagnostics = check_project(&output.project, &CheckOptions::default());
        assert!(diagnostics.is_empty(), "{:?}", diagnostics);
        let trace = run_program(&output.project, Some("Demo"), 1, &RuntimeOptions::default())
            .expect("program should run");
        let last = trace.cycles.last().unwrap();
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "DONE" && *value == Value::Bool(true)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "CV" && *value == Value::Int(1)));
        assert!(last
            .variables
            .iter()
            .any(|(name, value)| name == "SKIPPED" && *value == Value::Int(0)));
    }
}
