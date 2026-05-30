import type { GraphEdge, GraphModel, GraphNetwork, GraphNode } from "@/features/graph/graphTypes";
import { nodeDisplayLabel } from "@/features/graph/graphCompare";
import type { WorkspaceFile } from "@/types";

function edge(from: string, to: string, connectorId: string): GraphEdge {
  return { connectorId, from, to, formalParameter: null };
}

function expandFbdOutVariable(
  networkId: string,
  node: GraphNode,
  nodeIndex: number
): { nodes: GraphNode[]; edges: GraphEdge[] } {
  const value = node.attributes.value ?? "";
  const fnMatch = value.match(/^([A-Za-z_]\w*)\(([^)]*)\)$/);
  if (!fnMatch) {
    return { nodes: [node], edges: [] };
  }

  const operator = fnMatch[1] ?? "AND";
  const inputs = (fnMatch[2] ?? "")
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
  const output = nodeDisplayLabel(node) || "Output";
  const prefix = `${networkId}:${node.stableId}`;

  const inputNodes = inputs.map((label, inputIndex) => ({
    stableId: `${prefix}:in${inputIndex}`,
    kind: "input",
    label,
    position: null,
    size: null,
    attributes: { variable: label }
  }));

  const logicNode: GraphNode = {
    stableId: `${prefix}:logic`,
    kind: "block",
    label: operator,
    position: null,
    size: null,
    attributes: { expression: operator }
  };

  const outputNode: GraphNode = {
    stableId: `${prefix}:out`,
    kind: "output",
    label: output,
    position: null,
    size: null,
    attributes: { variable: output }
  };

  const nodes = [...inputNodes, logicNode, outputNode];
  const edges = inputNodes.map((inputNode, inputIndex) =>
    edge(inputNode.stableId, logicNode.stableId, `${prefix}:in${inputIndex}`)
  );
  edges.push(edge(logicNode.stableId, outputNode.stableId, `${prefix}:out`));

  return { nodes, edges };
}

function expandNetworkForDisplay(network: GraphNetwork, languageId: WorkspaceFile["languageId"]): GraphNetwork {
  if (languageId !== "fbd") {
    return network;
  }

  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];
  let outIndex = 0;

  for (const node of network.nodes) {
    if (node.kind !== "outVariable") {
      nodes.push(node);
      continue;
    }
    const expanded = expandFbdOutVariable(network.id, node, outIndex);
    outIndex += 1;
    nodes.push(...expanded.nodes);
    edges.push(...expanded.edges);
  }

  if (edges.length === 0) {
    return { ...network, nodes };
  }

  return { ...network, nodes, edges };
}

export function enrichGraphModelForDisplay(file: WorkspaceFile, model: GraphModel | null): GraphModel | null {
  if (!model) {
    return null;
  }

  return {
    ...model,
    pous: model.pous.map((pou) => ({
      ...pou,
      networks: pou.networks.map((network) => expandNetworkForDisplay(network, file.languageId))
    }))
  };
}
