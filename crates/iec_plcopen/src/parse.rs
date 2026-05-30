// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

pub(crate) fn parse_plcopen_var_blocks(
    node: Node<'_, '_>,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<VarBlock> {
    [
        ("inputVars", VarBlockKind::Input),
        ("outputVars", VarBlockKind::Output),
        ("inOutVars", VarBlockKind::InOut),
        ("externalVars", VarBlockKind::External),
        ("globalVars", VarBlockKind::Global),
        ("tempVars", VarBlockKind::Temp),
        ("accessVars", VarBlockKind::Access),
        ("configVars", VarBlockKind::Config),
        ("localVars", VarBlockKind::Local),
    ]
    .into_iter()
    .filter_map(|(tag, kind)| {
        let vars = child_elements(node, tag)
            .into_iter()
            .flat_map(|child| {
                if kind == VarBlockKind::Access {
                    parse_plcopen_access_variables(child)
                } else if kind == VarBlockKind::Config {
                    parse_plcopen_config_variables(child, implementation, diagnostics)
                } else {
                    parse_plcopen_variables(child, implementation, diagnostics)
                }
            })
            .collect::<Vec<_>>();
        (!vars.is_empty()).then_some(VarBlock {
            kind,
            constant: false,
            retain: None,
            vars,
        })
    })
    .collect()
}

pub(crate) fn parse_plcopen_variables(
    node: Node<'_, '_>,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<VarDecl> {
    child_elements(node, "variable")
        .into_iter()
        .filter_map(|variable| {
            let name = variable.attribute("name")?;
            let type_spec = parse_plcopen_type(variable)
                .unwrap_or(DataTypeSpec::Elementary(ElementaryType::Bool));
            Some(VarDecl {
                name: Identifier::new(name),
                location: variable
                    .attribute("address")
                    .or_else(|| variable.attribute("location"))
                    .map(ToString::to_string),
                access: None,
                edge: None,
                type_spec,
                initial_value: parse_plcopen_initial_value(variable, implementation, diagnostics),
            })
        })
        .collect()
}

pub(crate) fn parse_plcopen_access_variables(node: Node<'_, '_>) -> Vec<VarDecl> {
    child_elements(node, "accessVariable")
        .into_iter()
        .filter_map(|variable| {
            let alias = variable.attribute("alias")?;
            let type_spec = parse_plcopen_type(variable)
                .unwrap_or(DataTypeSpec::Elementary(ElementaryType::Bool));
            let direction = match variable.attribute("direction") {
                Some("readWrite") => AccessDirection::ReadWrite,
                _ => AccessDirection::ReadOnly,
            };
            Some(VarDecl {
                name: Identifier::new(alias),
                location: None,
                access: Some(AccessSpec {
                    path: variable.attribute("instancePathAndName")?.to_string(),
                    direction,
                }),
                edge: None,
                type_spec,
                initial_value: None,
            })
        })
        .collect()
}

pub(crate) fn parse_plcopen_config_variables(
    node: Node<'_, '_>,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<VarDecl> {
    let vars = child_elements(node, "configVariable")
        .into_iter()
        .filter_map(|variable| {
            let name = variable.attribute("instancePathAndName")?;
            let type_spec = parse_plcopen_type(variable)
                .unwrap_or(DataTypeSpec::Elementary(ElementaryType::Bool));
            Some(VarDecl {
                name: Identifier::new(name),
                location: variable.attribute("address").map(ToString::to_string),
                access: None,
                edge: None,
                type_spec,
                initial_value: parse_plcopen_initial_value(variable, implementation, diagnostics),
            })
        })
        .collect::<Vec<_>>();
    if vars.is_empty() {
        parse_plcopen_variables(node, implementation, diagnostics)
    } else {
        vars
    }
}

pub(crate) fn parse_plcopen_initial_value(
    node: Node<'_, '_>,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    let initial = first_child_element(node, "initialValue")?;
    parse_plcopen_value(initial, implementation, diagnostics)
}

pub(crate) fn parse_plcopen_value(
    node: Node<'_, '_>,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    if let Some(body) = first_child_element(node, "arrayValue") {
        let mut elements = Vec::new();
        for value_node in child_elements(body, "value") {
            let repeat = parse_plcopen_repetition_value(value_node, diagnostics);
            if let Some(value) = parse_plcopen_value(value_node, implementation, diagnostics) {
                extend_plcopen_array_value(
                    &mut elements,
                    repeat,
                    value,
                    implementation,
                    diagnostics,
                );
            }
        }
        return Some(Expr::ArrayLiteral(elements));
    }

    if let Some(body) = first_child_element(node, "structValue") {
        let fields = child_elements(body, "value")
            .into_iter()
            .filter_map(|value_node| {
                Some(ParamAssignment {
                    name: Some(Identifier::new(value_node.attribute("member")?)),
                    output: false,
                    negated: false,
                    expr: parse_plcopen_value(value_node, implementation, diagnostics),
                    variable: None,
                })
            })
            .collect::<Vec<_>>();
        return Some(Expr::StructLiteral(fields));
    }

    if let Some(simple) = first_child_element(node, "simpleValue") {
        let value = simple.attribute("value")?;
        let mut diagnostics = Vec::new();
        return parse_st_expression("plcopen.xml", value, &mut diagnostics);
    }

    None
}

pub(crate) fn parse_plcopen_repetition_value(
    node: Node<'_, '_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> usize {
    let Some(value) = node.attribute("repetitionValue") else {
        return 1;
    };
    value.parse::<usize>().unwrap_or_else(|_| {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::Syntax,
            format!("invalid PLCopen array repetitionValue '{value}'"),
            None,
        ));
        0
    })
}

pub(crate) fn extend_plcopen_array_value(
    elements: &mut Vec<Expr>,
    repeat: usize,
    value: Expr,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) {
    let max = implementation.max_array_elements;
    if repeat > max {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::Compliance,
            format!("PLCopen array repetitionValue {repeat} exceeds maximum {max}"),
            None,
        ));
        return;
    }
    let Some(new_len) = elements.len().checked_add(repeat) else {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::Compliance,
            format!("PLCopen array value exceeds maximum {max} elements"),
            None,
        ));
        return;
    };
    if new_len > max {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::Compliance,
            format!("PLCopen array value has {new_len} elements, exceeding maximum {max}"),
            None,
        ));
        return;
    }
    if elements.try_reserve(repeat).is_err() {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::Compliance,
            "PLCopen array value element storage exhausted",
            None,
        ));
        return;
    }
    elements.extend((0..repeat).map(|_| value.clone()));
}

pub(crate) fn parse_plcopen_type(node: Node<'_, '_>) -> Option<DataTypeSpec> {
    if let Some(derived) = first_descendant_element(node, "derived") {
        return derived
            .attribute("name")
            .map(|name| type_spec_from_name(name.to_string()));
    }
    if let Some(string) = first_descendant_element(node, "string") {
        return Some(DataTypeSpec::String {
            wide: false,
            length: string
                .attribute("length")
                .and_then(|value| value.parse().ok()),
        });
    }
    if let Some(wstring) = first_descendant_element(node, "wstring") {
        return Some(DataTypeSpec::String {
            wide: true,
            length: wstring
                .attribute("length")
                .and_then(|value| value.parse().ok()),
        });
    }
    [
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
        "TIME",
        "DATE",
        "TOD",
        "DT",
        "TIME_OF_DAY",
        "DATE_AND_TIME",
    ]
    .into_iter()
    .find(|name| first_descendant_element(node, name).is_some())
    .and_then(ElementaryType::parse)
    .map(DataTypeSpec::Elementary)
}

pub(crate) fn type_spec_from_name(name: String) -> DataTypeSpec {
    ElementaryType::parse(&name)
        .map(DataTypeSpec::Elementary)
        .unwrap_or_else(|| DataTypeSpec::Named(Identifier::new(name)))
}

pub(crate) fn parse_plcopen_data_types(
    node: Node<'_, '_>,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<PlcOpenDataTypeModel> {
    child_elements(node, "dataType")
        .into_iter()
        .filter_map(|data_type| {
            let name = data_type.attribute("name")?;
            let spec = parse_plcopen_base_type(data_type, implementation, diagnostics)?;
            Some(PlcOpenDataTypeModel {
                declaration: DataTypeDeclaration {
                    name: Identifier::new(name),
                    spec,
                },
            })
        })
        .collect()
}

pub(crate) fn parse_plcopen_base_type(
    node: Node<'_, '_>,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<DataTypeSpec> {
    if let Some(subrange) = first_descendant_element(node, "subrange") {
        return Some(DataTypeSpec::Subrange {
            base: subrange
                .attribute("baseType")
                .and_then(ElementaryType::parse)
                .unwrap_or(ElementaryType::Int),
            range: Subrange {
                low: subrange
                    .attribute("lower")
                    .and_then(|value| value.parse().ok())?,
                high: subrange
                    .attribute("upper")
                    .and_then(|value| value.parse().ok())?,
            },
        });
    }
    if let Some(subrange_body) = first_descendant_element(node, "subrangeSigned")
        .or_else(|| first_descendant_element(node, "subrangeUnsigned"))
    {
        let range = first_child_element(subrange_body, "range")?;
        let base = first_child_element(subrange_body, "baseType")
            .and_then(parse_plcopen_type)
            .and_then(|spec| {
                if let DataTypeSpec::Elementary(elementary) = spec {
                    Some(elementary)
                } else {
                    None
                }
            })
            .unwrap_or(ElementaryType::Int);
        return Some(DataTypeSpec::Subrange {
            base,
            range: Subrange {
                low: range
                    .attribute("lower")
                    .and_then(|value| value.parse().ok())?,
                high: range
                    .attribute("upper")
                    .and_then(|value| value.parse().ok())?,
            },
        });
    }
    if let Some(enum_body) = first_descendant_element(node, "enum") {
        return Some(DataTypeSpec::Enum {
            values: child_elements(enum_body, "value")
                .into_iter()
                .filter_map(|value| value.attribute("name"))
                .map(Identifier::new)
                .collect(),
        });
    }
    if let Some(struct_body) = first_descendant_element(node, "struct") {
        return Some(DataTypeSpec::Struct {
            fields: parse_plcopen_variables(struct_body, implementation, diagnostics)
                .into_iter()
                .map(|var| StructField {
                    name: var.name,
                    spec: var.type_spec,
                    initial_value: None,
                })
                .collect(),
        });
    }
    if let Some(array_body) = first_descendant_element(node, "array") {
        let ranges = child_elements(array_body, "dimension")
            .into_iter()
            .filter_map(|dimension| {
                Some(Subrange {
                    low: dimension
                        .attribute("lower")
                        .and_then(|value| value.parse().ok())?,
                    high: dimension
                        .attribute("upper")
                        .and_then(|value| value.parse().ok())?,
                })
            })
            .collect::<Vec<_>>();
        let element_type = first_child_element(array_body, "baseType")
            .or_else(|| first_child_element(array_body, "elementType"))
            .and_then(parse_plcopen_type)
            .unwrap_or(DataTypeSpec::Elementary(ElementaryType::Int));
        return Some(DataTypeSpec::Array {
            ranges,
            element_type: Box::new(element_type),
        });
    }
    parse_plcopen_type(node)
}

pub(crate) fn parse_plcopen_configurations(
    node: Node<'_, '_>,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<PlcOpenConfigurationModel> {
    child_elements(node, "configuration")
        .into_iter()
        .filter_map(|configuration| {
            let name = configuration.attribute("name")?;
            let mut var_blocks = Vec::new();
            let mut resources = Vec::new();
            for child in configuration.children().filter(|child| child.is_element()) {
                let tag_name = child.tag_name().name();
                if tag_name == "resource" {
                    if let Some(resource) =
                        parse_plcopen_resource(child, implementation, diagnostics)
                    {
                        resources.push(resource);
                    }
                } else if let Some(kind) = plcopen_var_block_kind(tag_name) {
                    let vars = if kind == VarBlockKind::Access {
                        parse_plcopen_access_variables(child)
                    } else if kind == VarBlockKind::Config {
                        parse_plcopen_config_variables(child, implementation, diagnostics)
                    } else {
                        parse_plcopen_variables(child, implementation, diagnostics)
                    };
                    if !vars.is_empty() {
                        var_blocks.push(VarBlock {
                            kind,
                            constant: false,
                            retain: None,
                            vars,
                        });
                    }
                }
            }
            Some(PlcOpenConfigurationModel {
                configuration: Configuration {
                    name: Identifier::new(name),
                    var_blocks,
                    resources,
                },
            })
        })
        .collect()
}

pub(crate) fn parse_plcopen_resource(
    node: Node<'_, '_>,
    implementation: &ImplementationParameters,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Resource> {
    let name = node.attribute("name")?;
    let mut var_blocks = Vec::new();
    let mut tasks = Vec::new();
    let mut program_instances = Vec::new();
    for child in node.children().filter(|child| child.is_element()) {
        let tag_name = child.tag_name().name();
        match tag_name {
            "task" => {
                if let Some(task) = parse_plcopen_task(child) {
                    let task_name = task.name.clone();
                    for task_child in child.children().filter(|child| child.is_element()) {
                        let task_tag_name = task_child.tag_name().name();
                        if matches!(task_tag_name, "pouInstance" | "program") {
                            if let Some(program) =
                                parse_plcopen_program_instance(task_child, Some(&task_name))
                            {
                                program_instances.push(program);
                            }
                        }
                    }
                    tasks.push(task);
                }
            }
            "pouInstance" | "program" => {
                if let Some(program) = parse_plcopen_program_instance(child, None) {
                    program_instances.push(program);
                }
            }
            _ => {
                if let Some(kind) = plcopen_var_block_kind(tag_name) {
                    let vars = if kind == VarBlockKind::Access {
                        parse_plcopen_access_variables(child)
                    } else if kind == VarBlockKind::Config {
                        parse_plcopen_config_variables(child, implementation, diagnostics)
                    } else {
                        parse_plcopen_variables(child, implementation, diagnostics)
                    };
                    if !vars.is_empty() {
                        var_blocks.push(VarBlock {
                            kind,
                            constant: false,
                            retain: None,
                            vars,
                        });
                    }
                }
            }
        }
    }
    Some(Resource {
        name: Identifier::new(name),
        var_blocks,
        tasks,
        program_instances,
    })
}

pub(crate) fn parse_plcopen_task(node: Node<'_, '_>) -> Option<Task> {
    let mut diagnostics = Vec::new();
    Some(Task {
        name: Identifier::new(node.attribute("name")?),
        single: node
            .attribute("single")
            .and_then(|value| parse_st_expression("plcopen.xml", value, &mut diagnostics)),
        interval: node
            .attribute("interval")
            .map(|value| Expr::Literal(parse_plcopen_time_literal(value))),
        priority: node.attribute("priority").and_then(|value| {
            value
                .parse()
                .ok()
                .map(|value| Expr::Literal(Literal::Int(value)))
        }),
    })
}

pub(crate) fn parse_plcopen_program_instance(
    node: Node<'_, '_>,
    task_override: Option<&Identifier>,
) -> Option<ProgramInstance> {
    Some(ProgramInstance {
        name: Identifier::new(node.attribute("name")?),
        program_type: Identifier::new(node.attribute("typeName")?),
        task: task_override
            .cloned()
            .or_else(|| node.attribute("task").map(Identifier::new)),
        args: parse_plcopen_program_instance_args(node),
    })
}

pub(crate) fn parse_plcopen_program_instance_args(node: Node<'_, '_>) -> Vec<ParamAssignment> {
    let sources = if let Some(add_data) = first_child_element(node, "addData") {
        let data_sources = child_elements(add_data, "data")
            .into_iter()
            .filter(|data| {
                data.attribute("name").is_some_and(|name| {
                    matches!(
                        name,
                        "RoboCpp.ProgramInstanceParameters"
                            | "RoboC++.ProgramInstanceParameters"
                            | "RoboCPP.ProgramInstanceParameters"
                    )
                })
            })
            .collect::<Vec<_>>();
        if data_sources.is_empty() {
            vec![add_data]
        } else {
            data_sources
        }
    } else {
        vec![node]
    };

    let mut diagnostics = Vec::new();
    let mut args = Vec::new();
    for source in sources {
        for parameter in descendant_elements(source, "parameter") {
            let name = parameter
                .attribute("name")
                .or_else(|| parameter.attribute("formal"))
                .map(Identifier::new);
            let direction = parameter.attribute("direction").unwrap_or("input");
            let negated = truthy_attr_text(parameter.attribute("negated"));
            if direction.eq_ignore_ascii_case("output") {
                let variable = parameter
                    .attribute("target")
                    .or_else(|| parameter.attribute("variable"))
                    .and_then(|target| parse_st_expression("plcopen.xml", target, &mut diagnostics))
                    .and_then(|expr| {
                        if let Expr::Variable(variable) = expr {
                            Some(variable)
                        } else {
                            None
                        }
                    });
                args.push(ParamAssignment {
                    name,
                    output: true,
                    negated,
                    expr: None,
                    variable,
                });
            } else {
                let expr = parameter
                    .attribute("expression")
                    .or_else(|| parameter.attribute("value"))
                    .and_then(|expression| {
                        parse_st_expression("plcopen.xml", expression, &mut diagnostics)
                    });
                args.push(ParamAssignment {
                    name,
                    output: false,
                    negated,
                    expr,
                    variable: None,
                });
            }
        }
    }
    args
}

pub(crate) fn parse_plcopen_time_literal(value: &str) -> Literal {
    let mut diagnostics = Vec::new();
    parse_st_expression("plcopen.xml", value, &mut diagnostics)
        .and_then(|expr| {
            if let Expr::Literal(literal) = expr {
                Some(literal)
            } else {
                None
            }
        })
        .unwrap_or_else(|| Literal::Typed {
            type_name: Identifier::new("TIME"),
            value: value.to_string(),
        })
}

pub(crate) fn plcopen_var_block_kind(tag: &str) -> Option<VarBlockKind> {
    match tag {
        "inputVars" => Some(VarBlockKind::Input),
        "outputVars" => Some(VarBlockKind::Output),
        "inOutVars" => Some(VarBlockKind::InOut),
        "externalVars" => Some(VarBlockKind::External),
        "globalVars" => Some(VarBlockKind::Global),
        "tempVars" => Some(VarBlockKind::Temp),
        "accessVars" => Some(VarBlockKind::Access),
        "configVars" => Some(VarBlockKind::Config),
        "localVars" => Some(VarBlockKind::Local),
        _ => None,
    }
}

pub(crate) fn wrap_st_body(name: &str, kind: &PouKind, body: &str) -> String {
    match kind {
        PouKind::Function { .. } => format!("FUNCTION {name} : INT\n{body}\nEND_FUNCTION"),
        PouKind::FunctionBlock => format!("FUNCTION_BLOCK {name}\n{body}\nEND_FUNCTION_BLOCK"),
        PouKind::Program => format!("PROGRAM {name}\n{body}\nEND_PROGRAM"),
    }
}
