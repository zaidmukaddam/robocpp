// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

pub(crate) fn add_graphical_helper_vars(var_blocks: &mut Vec<VarBlock>, body: &PouBody) {
    let helpers = body
        .networks
        .iter()
        .flat_map(|network| network.nodes.iter())
        .filter(|node| node.kind == "contact")
        .filter_map(|node| {
            let block_type = ld_edge_block_type(node)?;
            Some((ld_edge_instance_name(&node.id), block_type.to_string()))
        })
        .collect::<Vec<_>>();
    if helpers.is_empty() {
        return;
    }

    let mut existing = var_blocks
        .iter()
        .flat_map(|block| block.vars.iter())
        .map(|var| var.name.canonical.clone())
        .collect::<BTreeSet<_>>();
    let new_vars = helpers
        .into_iter()
        .filter_map(|(name, block_type)| {
            let ident = Identifier::new(name);
            existing.insert(ident.canonical.clone()).then_some(VarDecl {
                name: ident,
                type_spec: DataTypeSpec::Named(Identifier::new(block_type)),
                initial_value: None,
                location: None,
                access: None,
                edge: None,
            })
        })
        .collect::<Vec<_>>();
    if new_vars.is_empty() {
        return;
    }

    if let Some(block) = var_blocks.iter_mut().find(|block| {
        block.kind == VarBlockKind::Local && !block.constant && block.retain.is_none()
    }) {
        block.vars.extend(new_vars);
    } else {
        var_blocks.push(VarBlock {
            kind: VarBlockKind::Local,
            constant: false,
            retain: None,
            vars: new_vars,
        });
    }
}

pub(crate) fn plcopen_graphical_body(
    source_name: &str,
    language: ImplementationLanguage,
    graph: Node<'_, '_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> PlcOpenGraphModel {
    let node_kinds = match language {
        ImplementationLanguage::LadderDiagram => &[
            "leftPowerRail",
            "rightPowerRail",
            "contact",
            "coil",
            "block",
            "inVariable",
            "outVariable",
            "connector",
            "continuation",
        ][..],
        ImplementationLanguage::FunctionBlockDiagram => &[
            "block",
            "inVariable",
            "outVariable",
            "connector",
            "continuation",
        ][..],
        _ => &[][..],
    };
    let mut nodes = Vec::new();
    for kind in node_kinds {
        for graph_node in descendant_elements(graph, kind) {
            let mut attributes = std::collections::BTreeMap::new();
            for attr_name in [
                "localId",
                "executionOrderId",
                "name",
                "typeName",
                "variable",
                "formalParameter",
                "negated",
                "edge",
                "storage",
                "width",
                "height",
            ] {
                if let Some(value) = graph_node.attribute(attr_name) {
                    attributes.insert(attr_name.to_string(), value.to_string());
                }
            }
            if let Some(position) = first_descendant_element(graph_node, "position") {
                if let Some(x) = position.attribute("x") {
                    attributes.insert("positionX".to_string(), x.to_string());
                }
                if let Some(y) = position.attribute("y") {
                    attributes.insert("positionY".to_string(), y.to_string());
                }
            }
            if let Some(expression) = first_descendant_element(graph_node, "expression")
                .map(|node| node_text_content(node).trim().to_string())
                .filter(|text| !text.is_empty())
            {
                attributes.insert("expression".to_string(), expression);
            }
            let connection_refs = connection_refs(graph_node);
            if !connection_refs.is_empty() {
                attributes.insert("connectionRefs".to_string(), connection_refs.join(","));
            }
            if *kind == "block" {
                let input_refs = block_input_refs(graph_node);
                if !input_refs.is_empty() {
                    attributes.insert(
                        "inputRefs".to_string(),
                        input_refs
                            .into_iter()
                            .map(|(formal, refs)| format!("{formal}={}", refs.join("|")))
                            .collect::<Vec<_>>()
                            .join(";"),
                    );
                }
            }
            nodes.push(NetworkNode {
                id: attributes
                    .get("localId")
                    .cloned()
                    .unwrap_or_else(|| format!("{}_{}", kind, nodes.len() + 1)),
                kind: (*kind).to_string(),
                attributes,
            });
        }
    }
    if nodes.is_empty() {
        let mut attributes = std::collections::BTreeMap::new();
        attributes.insert("raw_node".to_string(), graph.tag_name().name().to_string());
        nodes.push(NetworkNode {
            id: "placeholder".to_string(),
            kind: "raw-plcopen-network".to_string(),
            attributes,
        });
    }

    let statements = match language {
        ImplementationLanguage::LadderDiagram => lower_ld_network(&nodes),
        ImplementationLanguage::FunctionBlockDiagram => {
            lower_fbd_network(source_name, &nodes, diagnostics)
        }
        _ => Vec::new(),
    };

    PlcOpenGraphModel {
        language,
        statements,
        nodes,
    }
}

pub(crate) fn lower_ld_network(nodes: &[NetworkNode]) -> Vec<Statement> {
    let mut statements = nodes
        .iter()
        .filter(|node| node.kind == "contact" && ld_edge_block_type(node).is_some())
        .map(ld_edge_contact_call)
        .collect::<Vec<_>>();
    let mut coils = nodes
        .iter()
        .filter(|node| node.kind == "coil")
        .collect::<Vec<_>>();
    coils.sort_by_key(|node| graphical_order_key(node));

    statements.extend(
        coils
            .into_iter()
            .filter_map(|coil| {
                let coil_name = coil.attributes.get("variable")?;
                let mut visiting = Vec::new();
                let value = ld_node_expr(nodes, coil, &mut visiting).or_else(|| {
                    nodes
                        .iter()
                        .find(|node| node.kind == "contact")
                        .and_then(|node| {
                            let mut visiting = Vec::new();
                            ld_contact_expr(nodes, node, &mut visiting)
                        })
                })?;
                Some(ld_coil_statement(coil, coil_name, value))
            })
            .collect::<Vec<_>>(),
    );
    statements
}

pub(crate) fn ld_node_expr(
    nodes: &[NetworkNode],
    node: &NetworkNode,
    visiting: &mut Vec<String>,
) -> Option<Expr> {
    if visiting.iter().any(|id| id == &node.id) {
        return None;
    }
    visiting.push(node.id.clone());
    let refs = node_connection_refs(node);
    let result = if node.kind == "leftPowerRail" {
        Some(Expr::Literal(Literal::Bool(true)))
    } else if node.kind == "coil" || node.kind == "rightPowerRail" {
        (!refs.is_empty())
            .then(|| expr_or_refs_with_stack(nodes, &refs, visiting))
            .flatten()
    } else if node.kind == "contact" {
        ld_contact_expr(nodes, node, visiting)
    } else if node.kind == "continuation" && refs.is_empty() {
        node.attributes
            .get("name")
            .and_then(|name| {
                nodes.iter().find(|candidate| {
                    candidate.kind == "connector"
                        && candidate
                            .attributes
                            .get("name")
                            .is_some_and(|other| other == name)
                })
            })
            .and_then(|connector| ld_node_expr(nodes, connector, visiting))
    } else {
        expr_or_refs_with_stack(nodes, &refs, visiting)
    };
    visiting.pop();
    result
}

pub(crate) fn expr_or_refs_with_stack(
    nodes: &[NetworkNode],
    refs: &[String],
    visiting: &mut Vec<String>,
) -> Option<Expr> {
    let mut exprs = refs
        .iter()
        .filter_map(|id| nodes.iter().find(|node| &node.id == id))
        .filter_map(|node| ld_node_expr(nodes, node, visiting))
        .collect::<Vec<_>>();
    let first = exprs
        .is_empty()
        .then_some(Expr::Literal(Literal::Bool(true)))
        .or_else(|| {
            let first = exprs.remove(0);
            Some(exprs.into_iter().fold(first, |left, right| Expr::Binary {
                op: BinaryOp::Or,
                left: Box::new(left),
                right: Box::new(right),
            }))
        })?;
    Some(first)
}

pub(crate) fn ld_contact_expr(
    nodes: &[NetworkNode],
    node: &NetworkNode,
    visiting: &mut Vec<String>,
) -> Option<Expr> {
    let variable = node.attributes.get("variable")?;
    let mut expr = if ld_edge_block_type(node).is_some() {
        Expr::Variable(VariableRef {
            path: vec![
                Identifier::new(ld_edge_instance_name(&node.id)),
                Identifier::new("Q"),
            ],
            indices: vec![Vec::new(), Vec::new()],
            direct: None,
        })
    } else {
        Expr::Variable(VariableRef::named(variable.clone()))
    };
    if truthy_attr(node, "negated") {
        expr = Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(expr),
        };
    }
    let refs = node_connection_refs(node);
    if refs.is_empty() {
        Some(expr)
    } else {
        Some(Expr::Binary {
            op: BinaryOp::And,
            left: Box::new(expr_or_refs_with_stack(nodes, &refs, visiting)?),
            right: Box::new(expr),
        })
    }
}

pub(crate) fn ld_edge_contact_call(node: &NetworkNode) -> Statement {
    let variable = node
        .attributes
        .get("variable")
        .cloned()
        .unwrap_or_else(|| "FALSE".to_string());
    Statement::FbCall {
        name: VariableRef::named(ld_edge_instance_name(&node.id)),
        args: vec![ParamAssignment {
            name: Some(Identifier::new("CLK")),
            output: false,
            negated: false,
            expr: Some(Expr::Variable(VariableRef::named(variable))),
            variable: None,
        }],
    }
}

pub(crate) fn ld_edge_block_type(node: &NetworkNode) -> Option<&'static str> {
    let edge = node.attributes.get("edge")?.to_ascii_lowercase();
    match edge.as_str() {
        "rising" | "positive" | "p" | "true" | "1" => Some("R_TRIG"),
        "falling" | "negative" | "n" => Some("F_TRIG"),
        _ => None,
    }
}

pub(crate) fn ld_edge_instance_name(local_id: &str) -> String {
    format!("rbcpp_ld_edge_{}", sanitize_identifier_fragment(local_id))
}

pub(crate) fn sanitize_identifier_fragment(input: &str) -> String {
    let mut out = input
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    if out.is_empty() {
        out.push('x');
    }
    out
}

pub(crate) fn ld_coil_statement(coil: &NetworkNode, coil_name: &str, value: Expr) -> Statement {
    let target = VariableRef::named(coil_name.to_string());
    let mut value = value;
    if truthy_attr(coil, "negated") {
        value = Expr::Unary {
            op: UnaryOp::Not,
            expr: Box::new(value),
        };
    }

    match coil
        .attributes
        .get("storage")
        .map(|value| value.to_ascii_lowercase())
    {
        Some(storage) if matches!(storage.as_str(), "set" | "s" | "setstored") => Statement::If {
            branches: vec![(
                value,
                vec![Statement::Assignment {
                    target,
                    value: Expr::Literal(Literal::Bool(true)),
                }],
            )],
            else_branch: Vec::new(),
        },
        Some(storage) if matches!(storage.as_str(), "reset" | "r" | "resetstored") => {
            Statement::If {
                branches: vec![(
                    value,
                    vec![Statement::Assignment {
                        target,
                        value: Expr::Literal(Literal::Bool(false)),
                    }],
                )],
                else_branch: Vec::new(),
            }
        }
        _ => Statement::Assignment { target, value },
    }
}

pub(crate) fn lower_fbd_network(
    source_name: &str,
    nodes: &[NetworkNode],
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Statement> {
    let mut outputs = nodes
        .iter()
        .filter(|node| node.kind == "outVariable")
        .collect::<Vec<_>>();
    outputs.sort_by_key(|node| graphical_order_key(node));

    outputs
        .into_iter()
        .filter_map(|output| {
            let target = output.attributes.get("expression")?.clone();
            let refs = node_connection_refs(output);
            let value = if refs.is_empty() {
                fbd_unwired_output_expr(source_name, nodes, diagnostics)
            } else {
                fbd_expr_from_refs(source_name, nodes, &refs, diagnostics, &mut Vec::new())
            }?;
            Some(Statement::Assignment {
                target: VariableRef::named(target),
                value,
            })
        })
        .collect()
}

pub(crate) fn fbd_unwired_output_expr(
    source_name: &str,
    nodes: &[NetworkNode],
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    let block = nodes.iter().find(|node| node.kind == "block")?;
    let function_name = block.attributes.get("typeName")?;
    let mut args =
        fbd_block_input_exprs_with_stack(source_name, nodes, block, diagnostics, &mut Vec::new())
            .into_iter()
            .map(|(name, expr)| ParamAssignment {
                name: name.map(Identifier::new),
                output: false,
                negated: false,
                expr: Some(expr),
                variable: None,
            })
            .collect::<Vec<_>>();
    if args.is_empty() {
        args = nodes
            .iter()
            .filter(|node| node.kind == "inVariable")
            .filter_map(|node| node.attributes.get("expression"))
            .filter_map(|expression| parse_st_expression(source_name, expression, diagnostics))
            .map(|expr| ParamAssignment {
                name: None,
                output: false,
                negated: false,
                expr: Some(expr),
                variable: None,
            })
            .collect();
    }
    Some(Expr::Call {
        name: Identifier::new(function_name.clone()),
        args,
    })
}

pub(crate) fn fbd_block_input_exprs_with_stack(
    source_name: &str,
    nodes: &[NetworkNode],
    block: &NetworkNode,
    diagnostics: &mut Vec<Diagnostic>,
    visiting: &mut Vec<String>,
) -> Vec<(Option<String>, Expr)> {
    parse_input_refs(block)
        .into_iter()
        .filter_map(|(formal, refs)| {
            fbd_expr_from_refs(source_name, nodes, &refs, diagnostics, visiting)
                .map(|expr| (Some(formal), expr))
        })
        .collect()
}

pub(crate) fn fbd_expr_from_refs(
    source_name: &str,
    nodes: &[NetworkNode],
    refs: &[String],
    diagnostics: &mut Vec<Diagnostic>,
    visiting: &mut Vec<String>,
) -> Option<Expr> {
    refs.iter()
        .filter_map(|id| nodes.iter().find(|node| &node.id == id))
        .find_map(|node| fbd_node_expr_with_stack(source_name, nodes, node, diagnostics, visiting))
}

pub(crate) fn fbd_node_expr_with_stack(
    source_name: &str,
    nodes: &[NetworkNode],
    node: &NetworkNode,
    diagnostics: &mut Vec<Diagnostic>,
    visiting: &mut Vec<String>,
) -> Option<Expr> {
    if visiting.iter().any(|id| id == &node.id) {
        diagnostics.push(Diagnostic::warning(
            DiagnosticCode::Unsupported,
            format!(
                "FBD feedback path involving localId '{}' was preserved but not lowered",
                node.id
            ),
            None,
        ));
        return None;
    }
    visiting.push(node.id.clone());
    if let Some(expression) = node.attributes.get("expression") {
        let result = parse_st_expression(source_name, expression, diagnostics);
        visiting.pop();
        return result;
    }
    if node.kind == "block" {
        let Some(function_name) = node.attributes.get("typeName") else {
            visiting.pop();
            return None;
        };
        let args =
            fbd_block_input_exprs_with_stack(source_name, nodes, node, diagnostics, visiting)
                .into_iter()
                .map(|(name, expr)| ParamAssignment {
                    name: name.map(Identifier::new),
                    output: false,
                    negated: false,
                    expr: Some(expr),
                    variable: None,
                })
                .collect();
        visiting.pop();
        return Some(Expr::Call {
            name: Identifier::new(function_name.clone()),
            args,
        });
    }
    let refs = node_connection_refs(node);
    if node.kind == "continuation" && refs.is_empty() {
        let result = node
            .attributes
            .get("name")
            .and_then(|name| {
                nodes.iter().find(|candidate| {
                    candidate.kind == "connector"
                        && candidate
                            .attributes
                            .get("name")
                            .is_some_and(|other| other == name)
                })
            })
            .and_then(|connector| {
                fbd_node_expr_with_stack(source_name, nodes, connector, diagnostics, visiting)
            });
        visiting.pop();
        return result;
    }
    let result = fbd_expr_from_refs(source_name, nodes, &refs, diagnostics, visiting);
    visiting.pop();
    result
}

pub(crate) fn graphical_order_key(node: &NetworkNode) -> (i64, i64, String) {
    (
        node.attributes
            .get("executionOrderId")
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(i64::MAX),
        node.id.parse::<i64>().unwrap_or(i64::MAX),
        node.id.clone(),
    )
}

pub(crate) fn truthy_attr(node: &NetworkNode, name: &str) -> bool {
    node.attributes
        .get(name)
        .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "true" | "1"))
}

pub(crate) fn truthy_attr_text(value: Option<&str>) -> bool {
    value.is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "true" | "1"))
}

pub(crate) fn connection_refs(node: Node<'_, '_>) -> Vec<String> {
    descendant_elements(node, "connection")
        .into_iter()
        .filter_map(|connection| connection.attribute("refLocalId").map(ToString::to_string))
        .collect()
}

pub(crate) fn block_input_refs(node: Node<'_, '_>) -> Vec<(String, Vec<String>)> {
    descendant_elements(node, "variable")
        .into_iter()
        .filter_map(|variable| {
            let formal = variable.attribute("formalParameter")?.to_string();
            let refs = connection_refs(variable);
            (!refs.is_empty()).then_some((formal, refs))
        })
        .collect()
}

pub(crate) fn node_connection_refs(node: &NetworkNode) -> Vec<String> {
    node.attributes
        .get("connectionRefs")
        .map(|refs| {
            refs.split(',')
                .filter(|part| !part.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn parse_input_refs(node: &NetworkNode) -> Vec<(String, Vec<String>)> {
    node.attributes
        .get("inputRefs")
        .map(|refs| {
            refs.split(';')
                .filter_map(|binding| {
                    let (formal, ids) = binding.split_once('=')?;
                    let ids = ids
                        .split('|')
                        .filter(|id| !id.is_empty())
                        .map(ToString::to_string)
                        .collect::<Vec<_>>();
                    (!ids.is_empty()).then_some((formal.to_string(), ids))
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn graphical_networks_to_xml(tag: &str, networks: &[Network]) -> String {
    let mut out = String::new();
    out.push_str(&format!("          <{tag}>\n"));
    for network in networks {
        for node in &network.nodes {
            if node.kind == "raw-plcopen-network" {
                continue;
            }
            out.push_str(&format!("            <{}", node.kind));
            for (name, value) in &node.attributes {
                if matches!(
                    name.as_str(),
                    "expression" | "connectionRefs" | "inputRefs" | "positionX" | "positionY"
                ) {
                    continue;
                }
                out.push_str(&format!(" {}=\"{}\"", name, xml_escape(value)));
            }
            let connection_refs = node_connection_refs(node);
            let input_refs = parse_input_refs(node);
            let position = node
                .attributes
                .get("positionX")
                .zip(node.attributes.get("positionY"));
            if node.attributes.contains_key("expression")
                || !connection_refs.is_empty()
                || !input_refs.is_empty()
                || position.is_some()
            {
                out.push_str(">\n");
                if let Some((x, y)) = position {
                    out.push_str(&format!(
                        "              <position x=\"{}\" y=\"{}\" />\n",
                        xml_escape(x),
                        xml_escape(y)
                    ));
                }
                if let Some(expression) = node.attributes.get("expression") {
                    out.push_str("              <expression>");
                    out.push_str(&xml_escape(expression));
                    out.push_str("</expression>\n");
                }
                if !connection_refs.is_empty() {
                    out.push_str("              <connectionPointIn>\n");
                    for id in connection_refs {
                        out.push_str(&format!(
                            "                <connection refLocalId=\"{}\" />\n",
                            xml_escape(&id)
                        ));
                    }
                    out.push_str("              </connectionPointIn>\n");
                }
                if !input_refs.is_empty() {
                    out.push_str("              <inputVariables>\n");
                    for (formal, refs) in input_refs {
                        out.push_str(&format!(
                            "                <variable formalParameter=\"{}\">\n",
                            xml_escape(&formal)
                        ));
                        out.push_str("                  <connectionPointIn>\n");
                        for id in refs {
                            out.push_str(&format!(
                                "                    <connection refLocalId=\"{}\" />\n",
                                xml_escape(&id)
                            ));
                        }
                        out.push_str("                  </connectionPointIn>\n");
                        out.push_str("                </variable>\n");
                    }
                    out.push_str("              </inputVariables>\n");
                }
                out.push_str(&format!("            </{}>\n", node.kind));
            } else {
                out.push_str(" />\n");
            }
        }
    }
    out.push_str(&format!("          </{tag}>\n"));
    out
}
