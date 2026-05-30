// SPDX-License-Identifier: MIT OR Apache-2.0

use super::*;

pub(crate) fn parse_sfc_body(
    source_name: &str,
    sfc: Node<'_, '_>,
    diagnostics: &mut Vec<Diagnostic>,
) -> Sfc {
    let mut steps = Vec::new();
    let mut transitions = Vec::new();
    let mut actions = Vec::new();
    let mut step_ids = std::collections::BTreeMap::new();
    let mut node_inputs = std::collections::BTreeMap::<String, Vec<String>>::new();
    let mut node_step_targets = std::collections::BTreeMap::<String, Identifier>::new();

    for (element, kind) in [
        ("step", SfcStepKind::Step),
        ("macroStep", SfcStepKind::MacroStep),
    ] {
        for node in descendant_elements(sfc, element) {
            let name = node
                .attribute("name")
                .map(ToString::to_string)
                .unwrap_or_else(|| format!("Step{}", steps.len()));
            let step_name = Identifier::new(&name);
            let local_id = node
                .attribute("localId")
                .map(ToString::to_string)
                .unwrap_or_else(|| (steps.len() + 1).to_string());
            let initial = node
                .attribute("initialStep")
                .or_else(|| node.attribute("initial"))
                .is_some_and(|value| matches!(value.to_ascii_lowercase().as_str(), "true" | "1"));
            let actions = first_child_element(node, "actionBlock")
                .map(|action_block| {
                    child_elements(action_block, "action")
                        .into_iter()
                        .filter_map(|action| {
                            let name = action
                                .attribute("referenceName")
                                .or_else(|| action.attribute("name"))
                                .or_else(|| action.attribute("actionName"))?;
                            let qualifier = action
                                .attribute("qualifier")
                                .and_then(SfcActionQualifier::parse)
                                .unwrap_or(SfcActionQualifier::NonStored);
                            let duration =
                                action.attribute("duration").map(parse_plcopen_time_literal);
                            Some(SfcActionAssociation {
                                name: Identifier::new(name),
                                qualifier: Some(qualifier),
                                duration,
                            })
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default();
            step_ids.insert(local_id.clone(), step_name.clone());
            node_inputs.insert(local_id, connection_refs(node));
            steps.push(SfcStep {
                name: step_name,
                initial,
                kind,
                actions,
            });
        }
    }

    let mut transition_ids = std::collections::BTreeMap::new();
    for node in descendant_elements(sfc, "transition") {
        let name = node.attribute("name").map(Identifier::new);
        let local_id = node
            .attribute("localId")
            .map(ToString::to_string)
            .unwrap_or_else(|| (steps.len() + transitions.len() + 1).to_string());
        node_inputs.insert(local_id.clone(), connection_refs(node));
        let condition = first_descendant_element(node, "ST")
            .map(node_text_content)
            .and_then(|text| parse_sfc_condition(source_name, &text, diagnostics));
        let priority = node
            .attribute("priority")
            .and_then(|value| value.parse::<i64>().ok());
        transition_ids.insert(local_id, transitions.len());
        transitions.push(SfcTransition {
            name,
            from: Vec::new(),
            to: Vec::new(),
            condition,
            priority,
        });
    }
    for kind in [
        "selectionDivergence",
        "selectionConvergence",
        "simultaneousDivergence",
        "simultaneousConvergence",
    ] {
        for node in descendant_elements(sfc, kind) {
            if let Some(local_id) = node.attribute("localId") {
                node_inputs.insert(local_id.to_string(), connection_refs(node));
            }
        }
    }
    for jump_kind in ["jumpStep", "jump"] {
        for node in descendant_elements(sfc, jump_kind) {
            if let Some(local_id) = node.attribute("localId") {
                node_inputs.insert(local_id.to_string(), connection_refs(node));
                if let Some(target) = node
                    .attribute("targetName")
                    .or_else(|| node.attribute("target"))
                    .or_else(|| node.attribute("targetNameRef"))
                {
                    node_step_targets.insert(local_id.to_string(), Identifier::new(target));
                }
            }
        }
    }
    let node_outputs = sfc_node_outputs(&node_inputs);
    for (local_id, index) in &transition_ids {
        transitions[*index].from = collect_sfc_reachable_steps(
            local_id,
            &node_inputs,
            &node_outputs,
            &step_ids,
            &node_step_targets,
            true,
        );
        transitions[*index].to = collect_sfc_reachable_steps(
            local_id,
            &node_inputs,
            &node_outputs,
            &step_ids,
            &node_step_targets,
            false,
        );
    }

    for node in descendant_elements(sfc, "action") {
        if node.attribute("referenceName").is_some() || node.attribute("actionName").is_some() {
            continue;
        }
        let name = node
            .attribute("name")
            .map(ToString::to_string)
            .unwrap_or_else(|| format!("Action{}", actions.len()));
        let qualifier = node
            .attribute("qualifier")
            .and_then(SfcActionQualifier::parse)
            .unwrap_or(SfcActionQualifier::NonStored);
        let duration = node.attribute("duration").map(parse_plcopen_time_literal);
        let action_body = first_descendant_element(node, "ST")
            .map(node_text_content)
            .map(|text| parse_sfc_action_body(source_name, &name, &text, diagnostics))
            .unwrap_or_default();
        actions.push(SfcAction {
            name: Identifier::new(name),
            qualifier,
            duration,
            body: action_body,
        });
    }

    Sfc {
        steps,
        transitions,
        actions,
    }
}

pub(crate) fn sfc_node_outputs(
    node_inputs: &std::collections::BTreeMap<String, Vec<String>>,
) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut outputs = std::collections::BTreeMap::<String, Vec<String>>::new();
    for (node_id, inputs) in node_inputs {
        for input in inputs {
            outputs
                .entry(input.clone())
                .or_default()
                .push(node_id.clone());
        }
    }
    outputs
}

pub(crate) fn collect_sfc_reachable_steps(
    start: &str,
    node_inputs: &std::collections::BTreeMap<String, Vec<String>>,
    node_outputs: &std::collections::BTreeMap<String, Vec<String>>,
    step_ids: &std::collections::BTreeMap<String, Identifier>,
    node_step_targets: &std::collections::BTreeMap<String, Identifier>,
    reverse: bool,
) -> Vec<Identifier> {
    fn visit(
        node_id: &str,
        node_inputs: &std::collections::BTreeMap<String, Vec<String>>,
        node_outputs: &std::collections::BTreeMap<String, Vec<String>>,
        step_ids: &std::collections::BTreeMap<String, Identifier>,
        node_step_targets: &std::collections::BTreeMap<String, Identifier>,
        reverse: bool,
        visited: &mut std::collections::BTreeSet<String>,
        steps: &mut Vec<Identifier>,
    ) {
        if !visited.insert(node_id.to_string()) {
            return;
        }
        let neighbors = if reverse {
            node_inputs.get(node_id)
        } else {
            node_outputs.get(node_id)
        };
        let Some(neighbors) = neighbors else { return };
        for neighbor in neighbors {
            if let Some(step) = step_ids
                .get(neighbor)
                .or_else(|| node_step_targets.get(neighbor))
            {
                if !steps
                    .iter()
                    .any(|existing| existing.canonical == step.canonical)
                {
                    steps.push(step.clone());
                }
            } else {
                visit(
                    neighbor,
                    node_inputs,
                    node_outputs,
                    step_ids,
                    node_step_targets,
                    reverse,
                    visited,
                    steps,
                );
            }
        }
    }

    let mut visited = std::collections::BTreeSet::new();
    let mut steps = Vec::new();
    visit(
        start,
        node_inputs,
        node_outputs,
        step_ids,
        node_step_targets,
        reverse,
        &mut visited,
        &mut steps,
    );
    steps
}

pub(crate) fn parse_sfc_condition(
    source_name: &str,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    let condition = text.trim().trim_end_matches(';');
    if condition.is_empty() {
        return None;
    }
    parse_st_expression(source_name, condition, diagnostics)
}

pub(crate) fn parse_st_expression(
    source_name: &str,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Option<Expr> {
    let wrapped = format!(
        "PROGRAM RbcppExpression\nVAR RbcppValue : INT; END_VAR\nRbcppValue := {text};\nEND_PROGRAM"
    );
    let parsed = parse_project(source_name, &wrapped);
    diagnostics.extend(parsed.diagnostics);
    parsed
        .project
        .first_program()
        .and_then(|pou| pou.body.statements.first())
        .and_then(|statement| {
            if let Statement::Assignment { value, .. } = statement {
                Some(value.clone())
            } else {
                None
            }
        })
}

pub(crate) fn parse_sfc_action_body(
    source_name: &str,
    action_name: &str,
    text: &str,
    diagnostics: &mut Vec<Diagnostic>,
) -> Vec<Statement> {
    let wrapped = format!("PROGRAM RbcppSfcAction{action_name}\n{text}\nEND_PROGRAM");
    let parsed = parse_project(source_name, &wrapped);
    diagnostics.extend(parsed.diagnostics);
    parsed
        .project
        .first_program()
        .map(|pou| pou.body.statements.clone())
        .unwrap_or_default()
}
