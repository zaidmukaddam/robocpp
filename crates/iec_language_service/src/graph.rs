// SPDX-License-Identifier: MIT OR Apache-2.0

use std::collections::{BTreeMap, BTreeSet};

use iec_diagnostics::{json_escape, Diagnostic, DiagnosticCode};
use iec_ir::{ImplementationLanguage, LibraryElement, NetworkNode, Project, Sfc};

use crate::{DocumentAnalysis, WorkspaceAnalysis};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphModel {
    pub uri: String,
    pub pous: Vec<GraphPou>,
    pub plcopen_layout: PlcOpenLayout,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPou {
    pub name: String,
    pub language: String,
    pub networks: Vec<GraphNetwork>,
    pub sfc: Option<SfcGraph>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNetwork {
    pub id: String,
    pub label: Option<String>,
    pub language: String,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphNode {
    pub stable_id: String,
    pub kind: String,
    pub label: Option<String>,
    pub position: Option<GraphPoint>,
    pub size: Option<GraphSize>,
    pub attributes: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphEdge {
    pub connector_id: String,
    pub from: String,
    pub to: String,
    pub formal_parameter: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphPoint {
    pub x: String,
    pub y: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphSize {
    pub width: String,
    pub height: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcGraph {
    pub steps: Vec<SfcStepNode>,
    pub transitions: Vec<SfcTransitionNode>,
    pub actions: Vec<SfcActionNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcStepNode {
    pub stable_id: String,
    pub name: String,
    pub initial: bool,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcTransitionNode {
    pub stable_id: String,
    pub name: Option<String>,
    pub from: Vec<String>,
    pub to: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SfcActionNode {
    pub stable_id: String,
    pub name: String,
    pub qualifier: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlcOpenLayout {
    pub node_ids: Vec<String>,
    pub connector_ids: Vec<String>,
    pub branch_geometry: Vec<GraphEdge>,
    pub action_blocks: Vec<SfcActionNode>,
    pub vendor_add_data: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphValidation {
    pub valid: bool,
    pub diagnostics: Vec<Diagnostic>,
}

impl GraphModel {
    pub fn to_json(&self) -> String {
        let pous = self
            .pous
            .iter()
            .map(GraphPou::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"uri\":\"{}\",\"pous\":[{}],\"plcopenLayout\":{}}}",
            json_escape(&self.uri),
            pous,
            self.plcopen_layout.to_json()
        )
    }
}

impl GraphPou {
    pub fn to_json(&self) -> String {
        let networks = self
            .networks
            .iter()
            .map(GraphNetwork::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let sfc = self
            .sfc
            .as_ref()
            .map(SfcGraph::to_json)
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"name\":\"{}\",\"language\":\"{}\",\"networks\":[{}],\"sfc\":{}}}",
            json_escape(&self.name),
            json_escape(&self.language),
            networks,
            sfc
        )
    }
}

impl GraphNetwork {
    pub fn to_json(&self) -> String {
        let nodes = self
            .nodes
            .iter()
            .map(GraphNode::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let edges = self
            .edges
            .iter()
            .map(GraphEdge::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let label = self
            .label
            .as_ref()
            .map(|label| format!("\"{}\"", json_escape(label)))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"id\":\"{}\",\"label\":{},\"language\":\"{}\",\"nodes\":[{}],\"edges\":[{}]}}",
            json_escape(&self.id),
            label,
            json_escape(&self.language),
            nodes,
            edges
        )
    }
}

impl GraphNode {
    pub fn to_json(&self) -> String {
        let label = self
            .label
            .as_ref()
            .map(|label| format!("\"{}\"", json_escape(label)))
            .unwrap_or_else(|| "null".to_string());
        let position = self
            .position
            .as_ref()
            .map(GraphPoint::to_json)
            .unwrap_or_else(|| "null".to_string());
        let size = self
            .size
            .as_ref()
            .map(GraphSize::to_json)
            .unwrap_or_else(|| "null".to_string());
        let attributes = self
            .attributes
            .iter()
            .map(|(key, value)| format!("\"{}\":\"{}\"", json_escape(key), json_escape(value)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"stableId\":\"{}\",\"kind\":\"{}\",\"label\":{},\"position\":{},\"size\":{},\"attributes\":{{{}}}}}",
            json_escape(&self.stable_id),
            json_escape(&self.kind),
            label,
            position,
            size,
            attributes
        )
    }
}

impl GraphEdge {
    pub fn to_json(&self) -> String {
        let formal = self
            .formal_parameter
            .as_ref()
            .map(|formal| format!("\"{}\"", json_escape(formal)))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"connectorId\":\"{}\",\"from\":\"{}\",\"to\":\"{}\",\"formalParameter\":{}}}",
            json_escape(&self.connector_id),
            json_escape(&self.from),
            json_escape(&self.to),
            formal
        )
    }
}

impl GraphPoint {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"x\":\"{}\",\"y\":\"{}\"}}",
            json_escape(&self.x),
            json_escape(&self.y)
        )
    }
}

impl GraphSize {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"width\":\"{}\",\"height\":\"{}\"}}",
            json_escape(&self.width),
            json_escape(&self.height)
        )
    }
}

impl SfcGraph {
    pub fn to_json(&self) -> String {
        let steps = self
            .steps
            .iter()
            .map(SfcStepNode::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let transitions = self
            .transitions
            .iter()
            .map(SfcTransitionNode::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let actions = self
            .actions
            .iter()
            .map(SfcActionNode::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"steps\":[{}],\"transitions\":[{}],\"actions\":[{}]}}",
            steps, transitions, actions
        )
    }
}

impl SfcStepNode {
    pub fn to_json(&self) -> String {
        let actions = self
            .actions
            .iter()
            .map(|action| format!("\"{}\"", json_escape(action)))
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"stableId\":\"{}\",\"name\":\"{}\",\"initial\":{},\"actions\":[{}]}}",
            json_escape(&self.stable_id),
            json_escape(&self.name),
            self.initial,
            actions
        )
    }
}

impl SfcTransitionNode {
    pub fn to_json(&self) -> String {
        let name = self
            .name
            .as_ref()
            .map(|name| format!("\"{}\"", json_escape(name)))
            .unwrap_or_else(|| "null".to_string());
        format!(
            "{{\"stableId\":\"{}\",\"name\":{},\"from\":{},\"to\":{}}}",
            json_escape(&self.stable_id),
            name,
            json_string_array(&self.from),
            json_string_array(&self.to)
        )
    }
}

impl SfcActionNode {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"stableId\":\"{}\",\"name\":\"{}\",\"qualifier\":\"{}\"}}",
            json_escape(&self.stable_id),
            json_escape(&self.name),
            json_escape(&self.qualifier)
        )
    }
}

impl PlcOpenLayout {
    pub fn to_json(&self) -> String {
        let branch_geometry = self
            .branch_geometry
            .iter()
            .map(GraphEdge::to_json)
            .collect::<Vec<_>>()
            .join(",");
        let action_blocks = self
            .action_blocks
            .iter()
            .map(SfcActionNode::to_json)
            .collect::<Vec<_>>()
            .join(",");
        format!(
            "{{\"nodeIds\":{},\"connectorIds\":{},\"branchGeometry\":[{}],\"actionBlocks\":[{}],\"vendorAddData\":{}}}",
            json_string_array(&self.node_ids),
            json_string_array(&self.connector_ids),
            branch_geometry,
            action_blocks,
            json_string_array(&self.vendor_add_data)
        )
    }
}

impl GraphValidation {
    pub fn to_json(&self) -> String {
        format!(
            "{{\"valid\":{},\"diagnostics\":{}}}",
            self.valid,
            iec_diagnostics::diagnostics_to_json(&self.diagnostics)
        )
    }
}

pub fn document_graph_model(analysis: &DocumentAnalysis) -> GraphModel {
    project_graph_model(&analysis.uri, &analysis.project)
}

pub fn workspace_graph_model(analysis: &WorkspaceAnalysis) -> GraphModel {
    project_graph_model("workspace", &analysis.merged_project)
}

pub fn validate_graph_model(model: &GraphModel) -> GraphValidation {
    let mut diagnostics = Vec::new();
    for pou in &model.pous {
        for network in &pou.networks {
            if network.language == "Ladder Diagram" {
                validate_ld_network(network, &mut diagnostics);
            }
            if network.language == "Function Block Diagram" {
                validate_fbd_network(network, &mut diagnostics);
            }
        }
        if let Some(sfc) = &pou.sfc {
            validate_sfc_graph(sfc, &mut diagnostics);
        }
    }
    GraphValidation {
        valid: diagnostics.is_empty(),
        diagnostics,
    }
}

fn project_graph_model(uri: &str, project: &Project) -> GraphModel {
    let mut pous = Vec::new();
    let mut layout = PlcOpenLayout {
        node_ids: Vec::new(),
        connector_ids: Vec::new(),
        branch_geometry: Vec::new(),
        action_blocks: Vec::new(),
        vendor_add_data: project
            .metadata
            .get("plcopen.addData")
            .map(|value| vec![value.clone()])
            .unwrap_or_default(),
    };

    for element in &project.library_elements {
        let LibraryElement::Pou(pou) = element else {
            continue;
        };
        let mut networks = Vec::new();
        for (index, network) in pou.body.networks.iter().enumerate() {
            let nodes = network.nodes.iter().map(graph_node).collect::<Vec<_>>();
            let edges = graph_edges(&network.nodes);
            layout
                .node_ids
                .extend(nodes.iter().map(|node| node.stable_id.clone()));
            layout
                .connector_ids
                .extend(edges.iter().map(|edge| edge.connector_id.clone()));
            layout.branch_geometry.extend(edges.clone());
            networks.push(GraphNetwork {
                id: format!("{}:{index}", pou.name.original),
                label: network.label.clone(),
                language: language_label(network.language).to_string(),
                nodes,
                edges,
            });
        }
        let sfc = pou.body.sfc.as_ref().map(|sfc| {
            let graph = sfc_graph(sfc);
            layout.action_blocks.extend(graph.actions.iter().cloned());
            graph
        });
        pous.push(GraphPou {
            name: pou.name.original.clone(),
            language: language_label(pou.body.language).to_string(),
            networks,
            sfc,
        });
    }
    layout.node_ids.sort();
    layout.node_ids.dedup();
    layout.connector_ids.sort();
    layout.connector_ids.dedup();
    GraphModel {
        uri: uri.to_string(),
        pous,
        plcopen_layout: layout,
    }
}

fn graph_node(node: &NetworkNode) -> GraphNode {
    GraphNode {
        stable_id: node.id.clone(),
        kind: node.kind.clone(),
        label: node
            .attributes
            .get("name")
            .or_else(|| node.attributes.get("variable"))
            .or_else(|| node.attributes.get("expression"))
            .cloned(),
        position: node
            .attributes
            .get("positionX")
            .zip(node.attributes.get("positionY"))
            .map(|(x, y)| GraphPoint {
                x: x.clone(),
                y: y.clone(),
            }),
        size: node
            .attributes
            .get("width")
            .zip(node.attributes.get("height"))
            .map(|(width, height)| GraphSize {
                width: width.clone(),
                height: height.clone(),
            }),
        attributes: node.attributes.clone(),
    }
}

fn graph_edges(nodes: &[NetworkNode]) -> Vec<GraphEdge> {
    let mut edges = Vec::new();
    for node in nodes {
        for (index, reference) in connection_refs(node).into_iter().enumerate() {
            edges.push(GraphEdge {
                connector_id: format!("{}:{}:{index}", reference, node.id),
                from: reference,
                to: node.id.clone(),
                formal_parameter: None,
            });
        }
        if let Some(input_refs) = node.attributes.get("inputRefs") {
            for part in input_refs.split(';').filter(|part| !part.is_empty()) {
                let (formal, refs) = part.split_once('=').unwrap_or((part, ""));
                for reference in refs.split('|').filter(|reference| !reference.is_empty()) {
                    edges.push(GraphEdge {
                        connector_id: format!("{}:{}:{formal}", reference, node.id),
                        from: reference.to_string(),
                        to: node.id.clone(),
                        formal_parameter: Some(formal.to_string()),
                    });
                }
            }
        }
    }
    edges
}

fn sfc_graph(sfc: &Sfc) -> SfcGraph {
    SfcGraph {
        steps: sfc
            .steps
            .iter()
            .map(|step| SfcStepNode {
                stable_id: step.name.canonical.clone(),
                name: step.name.original.clone(),
                initial: step.initial,
                actions: step
                    .actions
                    .iter()
                    .map(|action| action.name.original.clone())
                    .collect(),
            })
            .collect(),
        transitions: sfc
            .transitions
            .iter()
            .enumerate()
            .map(|(index, transition)| SfcTransitionNode {
                stable_id: transition
                    .name
                    .as_ref()
                    .map(|name| name.canonical.clone())
                    .unwrap_or_else(|| format!("transition{index}")),
                name: transition.name.as_ref().map(|name| name.original.clone()),
                from: transition
                    .from
                    .iter()
                    .map(|step| step.original.clone())
                    .collect(),
                to: transition
                    .to
                    .iter()
                    .map(|step| step.original.clone())
                    .collect(),
            })
            .collect(),
        actions: sfc
            .actions
            .iter()
            .map(|action| SfcActionNode {
                stable_id: action.name.canonical.clone(),
                name: action.name.original.clone(),
                qualifier: action.qualifier.as_iec().to_string(),
            })
            .collect(),
    }
}

fn validate_ld_network(network: &GraphNetwork, diagnostics: &mut Vec<Diagnostic>) {
    let has_power = network
        .nodes
        .iter()
        .any(|node| node.kind == "leftPowerRail");
    for node in &network.nodes {
        if matches!(node.kind.as_str(), "contact" | "coil") {
            let has_incoming = network.edges.iter().any(|edge| edge.to == node.stable_id);
            if !has_incoming && has_power {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!(
                        "LD node '{}' is not connected to incoming power flow",
                        node.stable_id
                    ),
                    None,
                ));
            }
        }
    }
}

fn validate_fbd_network(network: &GraphNetwork, diagnostics: &mut Vec<Diagnostic>) {
    let graph =
        network
            .edges
            .iter()
            .fold(BTreeMap::<String, Vec<String>>::new(), |mut graph, edge| {
                graph
                    .entry(edge.from.clone())
                    .or_default()
                    .push(edge.to.clone());
                graph
            });
    for node in &network.nodes {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        if reaches_cycle(&node.stable_id, &graph, &mut visiting, &mut visited) {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!("FBD feedback cycle involving node '{}'", node.stable_id),
                None,
            ));
            break;
        }
    }
}

fn validate_sfc_graph(sfc: &SfcGraph, diagnostics: &mut Vec<Diagnostic>) {
    if !sfc.steps.iter().any(|step| step.initial) {
        diagnostics.push(Diagnostic::error(
            DiagnosticCode::Semantic,
            "SFC graph has no initial step",
            None,
        ));
    }
    let step_names = sfc
        .steps
        .iter()
        .map(|step| step.name.clone())
        .collect::<BTreeSet<_>>();
    for transition in &sfc.transitions {
        if transition.from.is_empty() || transition.to.is_empty() {
            diagnostics.push(Diagnostic::error(
                DiagnosticCode::Semantic,
                format!(
                    "SFC transition '{}' must have both source and target steps",
                    transition.name.as_deref().unwrap_or(&transition.stable_id)
                ),
                None,
            ));
        }
        for step in transition.from.iter().chain(transition.to.iter()) {
            if !step_names.contains(step) {
                diagnostics.push(Diagnostic::error(
                    DiagnosticCode::Semantic,
                    format!("SFC transition references unknown step '{step}'"),
                    None,
                ));
            }
        }
    }
}

fn reaches_cycle(
    node: &str,
    graph: &BTreeMap<String, Vec<String>>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
) -> bool {
    if visiting.contains(node) {
        return true;
    }
    if !visited.insert(node.to_string()) {
        return false;
    }
    visiting.insert(node.to_string());
    if let Some(neighbors) = graph.get(node) {
        for neighbor in neighbors {
            if reaches_cycle(neighbor, graph, visiting, visited) {
                return true;
            }
        }
    }
    visiting.remove(node);
    false
}

fn connection_refs(node: &NetworkNode) -> Vec<String> {
    node.attributes
        .get("connectionRefs")
        .map(|refs| {
            refs.split(',')
                .filter(|reference| !reference.is_empty())
                .map(ToString::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn language_label(language: ImplementationLanguage) -> &'static str {
    match language {
        ImplementationLanguage::StructuredText => "Structured Text",
        ImplementationLanguage::InstructionList => "Instruction List",
        ImplementationLanguage::SequentialFunctionChart => "Sequential Function Chart",
        ImplementationLanguage::LadderDiagram => "Ladder Diagram",
        ImplementationLanguage::FunctionBlockDiagram => "Function Block Diagram",
        ImplementationLanguage::External => "External",
    }
}

fn json_string_array(values: &[String]) -> String {
    let values = values
        .iter()
        .map(|value| format!("\"{}\"", json_escape(value)))
        .collect::<Vec<_>>()
        .join(",");
    format!("[{values}]")
}
