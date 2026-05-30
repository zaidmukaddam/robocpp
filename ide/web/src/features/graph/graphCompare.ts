import type { GraphModel, GraphNode } from "@/features/graph/graphTypes";

const INFRASTRUCTURE_KINDS = new Set(["leftPowerRail", "rightPowerRail"]);

export function nodeDisplayLabel(node: GraphNode): string {
  if (node.kind === "outVariable") {
    return node.label ?? node.attributes.expression ?? node.attributes.name ?? node.attributes.variable ?? "";
  }
  if (node.kind === "inVariable") {
    return node.label ?? node.attributes.expression ?? node.attributes.variable ?? node.attributes.name ?? "";
  }
  return (
    node.label ??
    node.attributes.name ??
    node.attributes.variable ??
    node.attributes.typeName ??
    ""
  );
}

function normalizeSemanticToken(value: string): string {
  return value.replace(/\s+/g, "").toUpperCase();
}

function normalizeOutVariableValue(value: string): string {
  const trimmed = value.trim();
  const placeholder = trimmed.match(/^([A-Za-z_][\w]*)\(\.\.\.\)$/);
  if (placeholder) {
    return normalizeSemanticToken(placeholder[1]!);
  }
  const call = trimmed.match(/^([A-Za-z_][\w]*)\(/);
  if (call) {
    return normalizeSemanticToken(call[1]!);
  }
  return normalizeSemanticToken(trimmed);
}

function nodeSemanticValue(node: GraphNode): string {
  if (node.kind === "outVariable") {
    return normalizeOutVariableValue(node.attributes.value ?? "");
  }
  if (node.kind === "inVariable") {
    return normalizeSemanticToken(node.attributes.expression ?? node.attributes.variable ?? "");
  }
  return normalizeSemanticToken(node.attributes.value ?? "");
}

export function graphSemanticFingerprint(model: GraphModel): string {
  const parts: string[] = [];

  for (const pou of model.pous) {
    pou.networks.forEach((network, networkIndex) => {
      for (const node of network.nodes) {
        if (INFRASTRUCTURE_KINDS.has(node.kind)) {
          continue;
        }
        const label = nodeDisplayLabel(node);
        const value = nodeSemanticValue(node);
        const negated = node.attributes.negated ?? "";
        const storage = node.attributes.storage ?? "";
        parts.push(`${networkIndex}\0${node.kind}\0${label}\0${value}\0${negated}\0${storage}`);
      }
    });

    if (pou.sfc) {
      for (const step of pou.sfc.steps) {
        parts.push(`sfc-step\0${step.name}\0${step.initial ? "1" : "0"}`);
      }
      for (const transition of pou.sfc.transitions) {
        parts.push(
          `sfc-transition\0${transition.from.join(",")}\0${transition.to.join(",")}\0${transition.name ?? ""}`
        );
      }
      for (const action of pou.sfc.actions) {
        parts.push(`sfc-action\0${action.name}\0${action.qualifier}`);
      }
    }
  }

  parts.sort();
  return parts.join("\n");
}

export function graphModelsSemanticallyEqual(left: GraphModel, right: GraphModel): boolean {
  return graphSemanticFingerprint(left) === graphSemanticFingerprint(right);
}

export function graphSemanticNodeCount(model: GraphModel): number {
  let count = 0;
  for (const pou of model.pous) {
    for (const network of pou.networks) {
      count += network.nodes.filter((node) => !INFRASTRUCTURE_KINDS.has(node.kind)).length;
    }
    if (pou.sfc) {
      count += pou.sfc.steps.length + pou.sfc.transitions.length + pou.sfc.actions.length;
    }
  }
  return count;
}

/** Shape a local graph model like the WASM engine output for parity tests. */
export function shapeGraphModelLikeEngine(model: GraphModel): GraphModel {
  const shaped = structuredClone(model);
  for (const pou of shaped.pous) {
    pou.networks.forEach((network, index) => {
      network.id = `${pou.name}:${index}`;
      for (const node of network.nodes) {
        node.attributes.localId = node.stableId;
        if (node.attributes.variable && !node.label) {
          node.label = node.attributes.variable;
        }
        if (node.kind === "contact" || node.kind === "coil") {
          node.attributes.connectionRefs = String(Number(node.stableId) - 1);
        }
        if (node.kind === "outVariable") {
          node.attributes.expression = node.label ?? node.attributes.expression ?? "";
          node.attributes.value = "AND(...)";
        }
      }
    });
  }
  return shaped;
}
