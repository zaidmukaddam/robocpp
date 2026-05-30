import type {
  GraphEdge,
  GraphModel,
  GraphNetwork,
  GraphNode,
  GraphPou,
  SfcActionNode,
  GraphPoint,
  GraphSize,
  SfcGraph,
  SfcStepNode,
  SfcTransitionNode
} from "@/features/graph/graphTypes";
import type { WorkspaceFile } from "@/types";
import { readEmbeddedPlcopenMetadata } from "@/features/graph/plcopenMetadata";

function node(
  id: string,
  kind: string,
  label: string | null,
  attributes: Record<string, string> = {}
): GraphNode {
  return {
    stableId: id,
    kind,
    label,
    position: null,
    size: null,
    attributes
  };
}

function edge(from: string, to: string, connectorId: string): GraphEdge {
  return { connectorId, from, to, formalParameter: null };
}

function stripCommentLine(line: string): string | null {
  const match = line.match(/^\s*\(\*([\s\S]*?)\*\)\s*$/);
  return match ? match[1]?.trim() ?? "" : null;
}

function parseLdBodyElements(body: string): Array<{
  kind: "contact" | "coil";
  variable: string;
  negated: boolean;
  storage?: "set" | "reset";
  comment?: string;
}> {
  const elements: Array<{
    kind: "contact" | "coil";
    variable: string;
    negated: boolean;
    storage?: "set" | "reset";
    comment?: string;
  }> = [];
  let pendingComment = "";

  for (const rawLine of body.split("\n")) {
    const comment = stripCommentLine(rawLine);
    if (comment !== null) {
      pendingComment = comment;
      continue;
    }

    const line = rawLine.trim();
    if (!line) {
      continue;
    }

    const contactNot = line.match(/^CONTACT_NOT\s+(\w+)\s*;$/i);
    const contactNegated = line.match(/^CONTACT\s+NOT\s+(\w+)\s*;$/i);
    const contact = line.match(/^CONTACT\s+(\w+)\s*;$/i);
    const coilNot = line.match(/^COIL_NOT\s+(\w+)\s*;$/i);
    const coil = line.match(/^COIL\s+(\w+)\s*;$/i);
    const setCoil = line.match(/^SET\s+(\w+)\s*;$/i);
    const resetCoil = line.match(/^RESET\s+(\w+)\s*;$/i);

    const commentValue = pendingComment || undefined;
    pendingComment = "";

    if (contactNot || contactNegated || contact) {
      elements.push({
        kind: "contact",
        variable: contactNot?.[1] ?? contactNegated?.[1] ?? contact?.[1] ?? "Contact",
        negated: Boolean(contactNot || contactNegated),
        ...(commentValue ? { comment: commentValue } : {})
      });
      continue;
    }

    if (coilNot || coil || setCoil || resetCoil) {
      elements.push({
        kind: "coil",
        variable: coilNot?.[1] ?? coil?.[1] ?? setCoil?.[1] ?? resetCoil?.[1] ?? "Coil",
        negated: Boolean(coilNot),
        storage: setCoil ? "set" : resetCoil ? "reset" : undefined,
        ...(commentValue ? { comment: commentValue } : {})
      });
    }
  }

  return elements;
}

function parseLdGraph(file: WorkspaceFile): GraphPou | null {
  const pouMatch = file.text.match(/PROGRAM\s+(\w+)/i);
  const pouName = pouMatch?.[1] ?? "Program";
  const rungBlocks = [...file.text.matchAll(/RUNG\s*([\s\S]*?)\s*END_RUNG/gi)];
  const networks: GraphNetwork[] = rungBlocks.map((match, index) => {
    const body = match[1] ?? "";
    const nodes: GraphNode[] = [];
    const edges: GraphEdge[] = [];
    let nodeIndex = 1;
    const powerId = String(nodeIndex);
    nodes.push(node(powerId, "leftPowerRail", null));
    let lastId = powerId;

    for (const entry of parseLdBodyElements(body)) {
      nodeIndex += 1;
      const stableId = String(nodeIndex);
      if (entry.kind === "contact") {
        nodes.push(
          node(stableId, "contact", entry.variable, {
            connectionRefs: lastId,
            variable: entry.variable,
            ...(entry.negated ? { negated: "true" } : {}),
            ...(entry.comment ? { comment: entry.comment } : {})
          })
        );
      } else {
        nodes.push(
          node(stableId, "coil", entry.variable, {
            connectionRefs: lastId,
            variable: entry.variable,
            ...(entry.storage ? { storage: entry.storage } : {}),
            ...(entry.negated ? { negated: "true" } : {}),
            ...(entry.comment ? { comment: entry.comment } : {})
          })
        );
      }
      edges.push(edge(lastId, stableId, `${lastId}:${stableId}:0`));
      lastId = stableId;
    }

    return {
      id: `${pouName}:rung:${index}`,
      label: `Rung ${index + 1}`,
      language: "LD",
      nodes,
      edges
    };
  });
  if (networks.length === 0) {
    return null;
  }
  return { name: pouName, language: "LD", networks, sfc: null };
}

function parseFbdEdges(assignments: Array<{ target: string; value: string }>, nodes: GraphNode[]): GraphEdge[] {
  const edges: GraphEdge[] = [];
  for (const entry of assignments) {
    const target = entry.target;
    const value = entry.value.trim();
    const targetNode = nodes.find((node) => node.label === target);
    if (!targetNode) {
      continue;
    }
    const funcMatch = value.match(/^(\w+)\(([^)]*)\)$/);
    if (funcMatch) {
      for (const arg of splitFbdArgList(funcMatch[2] ?? "")) {
        edges.push(edge(arg, target, `${arg}->${target}`));
      }
      continue;
    }
    const simpleRef = value.match(/^(\w+)$/);
    if (simpleRef?.[1]) {
      edges.push(edge(simpleRef[1], target, `${simpleRef[1]}->${target}`));
    }
  }
  return edges;
}

function splitFbdArgList(args: string): string[] {
  return args
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean);
}

function parseFbdGraph(file: WorkspaceFile): GraphPou | null {
  const pouMatch = file.text.match(/PROGRAM\s+(\w+)/i);
  const pouName = pouMatch?.[1] ?? "Program";
  const networkBlocks = [...file.text.matchAll(/NETWORK\s*([\s\S]*?)\s*END_NETWORK/gi)];
  const networks: GraphNetwork[] = networkBlocks.map((match, index) => {
    const body = match[1] ?? "";
    const assignments: Array<{ target: string; value: string; comment?: string }> = [];
    let pendingComment = "";
    for (const rawLine of body.split("\n")) {
      const comment = stripCommentLine(rawLine);
      if (comment !== null) {
        pendingComment = comment;
        continue;
      }
      const assign = rawLine.match(/OUT\s+(\w+)\s*:=\s*([^;]+);/i);
      if (assign) {
        assignments.push({
          target: assign[1] ?? "Output",
          value: (assign[2] ?? "").trim(),
          ...(pendingComment ? { comment: pendingComment } : {})
        });
        pendingComment = "";
      }
    }
    const nodes: GraphNode[] = assignments.map((entry, outIndex) =>
      node(String(outIndex + 1), "outVariable", entry.target, {
        expression: entry.target,
        value: entry.value,
        ...(entry.comment ? { comment: entry.comment } : {})
      })
    );
    const edges = parseFbdEdges(assignments, nodes);
    return {
      id: `${pouName}:network:${index}`,
      label: `Network ${index + 1}`,
      language: "FBD",
      nodes,
      edges
    };
  });
  if (networks.length === 0) {
    return null;
  }
  return { name: pouName, language: "FBD", networks, sfc: null };
}

function parseSfcGraph(file: WorkspaceFile): GraphPou | null {
  const pouMatch = file.text.match(/PROGRAM\s+(\w+)/i);
  const pouName = pouMatch?.[1] ?? "Program";
  const steps: SfcStepNode[] = [];
  const initialSteps = [...file.text.matchAll(/INITIAL_STEP\s+(\w+)/gi)];
  for (const match of initialSteps) {
    steps.push({
      stableId: match[1] ?? "Start",
      name: match[1] ?? "Start",
      initial: true,
      actions: []
    });
  }
  for (const match of file.text.matchAll(/^\s*STEP\s+(\w+)/gim)) {
    const name = match[1] ?? "Step";
    if (!steps.some((step) => step.name === name)) {
      steps.push({ stableId: name, name, initial: false, actions: [] });
    }
  }
  const transitions: SfcTransitionNode[] = [];
  const transitionPatterns = [
    /TRANSITION\s+(\w+)\s+FROM\s+([\w,\s]+)\s+TO\s+([\w,\s]+)\s*:=\s*([^;]+);/gi,
    /TRANSITION\s+FROM\s+([\w,\s]+)\s+TO\s+([\w,\s]+)\s*:=\s*([^;]+);/gi,
    /TRANSITION\s+(\w+)?\s*:=\s*([^;]+);/gi
  ];

  const splitStepList = (value: string): string[] =>
    value
      .split(",")
      .map((part) => part.trim())
      .filter(Boolean);

  for (const match of file.text.matchAll(transitionPatterns[0]!)) {
    transitions.push({
      stableId: match[1] ?? `transition${transitions.length}`,
      name: match[1] ?? null,
      from: splitStepList(match[2] ?? ""),
      to: splitStepList(match[3] ?? "")
    });
  }
  if (transitions.length === 0) {
    for (const match of file.text.matchAll(transitionPatterns[1]!)) {
      transitions.push({
        stableId: `transition${transitions.length}`,
        name: null,
        from: splitStepList(match[1] ?? ""),
        to: splitStepList(match[2] ?? "")
      });
    }
  }
  if (transitions.length === 0) {
    for (const match of file.text.matchAll(transitionPatterns[2]!)) {
      transitions.push({
        stableId: match[1] ?? `transition${transitions.length}`,
        name: match[1] ?? null,
        from: steps.length > 0 ? [steps[0]?.name ?? "Start"] : [],
        to: steps.length > 1 ? [steps[1]?.name ?? "Run"] : []
      });
    }
  }
  const actions: SfcActionNode[] = [...file.text.matchAll(/ACTION\s+(\w+)\s*:\s*([\s\S]*?)\s*END_ACTION/gi)].map(
    (match) => ({
      stableId: match[1] ?? "Action",
      name: match[1] ?? "Action",
      qualifier: "N"
    })
  );
  for (const step of steps) {
    step.actions = actions.filter((action) => action.name === step.name).map((action) => action.name);
  }
  if (steps.length === 0) {
    return null;
  }
  const sfc: SfcGraph = { steps, transitions, actions };
  return { name: pouName, language: "SFC", networks: [], sfc };
}

function parsePosition(fragment: string): { position: GraphPoint | null; size: GraphSize | null } {
  const positionMatch = fragment.match(/<position[^>]*x="([^"]+)"[^>]*y="([^"]+)"/i);
  const sizeMatch = fragment.match(/<size[^>]*width="([^"]+)"[^>]*height="([^"]+)"/i);
  return {
    position: positionMatch ? { x: positionMatch[1]!, y: positionMatch[2]! } : null,
    size: sizeMatch ? { width: sizeMatch[1]!, height: sizeMatch[2]! } : null
  };
}

function parseXmlGraph(file: WorkspaceFile): GraphPou | null {
  const pouMatch = file.text.match(/<pou\s+name="([^"]+)"/i);
  const pouName = pouMatch?.[1] ?? "PLCopen";
  const nodes: GraphNode[] = [];
  const edges: GraphEdge[] = [];

  for (const match of file.text.matchAll(
    /<inVariable\s+localId="(\d+)"([^>]*)>([\s\S]*?)<\/inVariable>/gi
  )) {
    const localId = match[1] ?? "";
    const layout = parsePosition(`${match[2] ?? ""}${match[3] ?? ""}`);
    const expression = match[3]?.match(/<expression>([^<]*)<\/expression>/i)?.[1] ?? `In${localId}`;
    nodes.push({
      stableId: localId,
      kind: "inVariable",
      label: expression,
      position: layout.position,
      size: layout.size,
      attributes: { localId, expression }
    });
  }

  for (const match of file.text.matchAll(/<block\s+localId="(\d+)"\s+typeName="([^"]+)"([^>]*)>/gi)) {
    const localId = match[1] ?? "";
    const typeName = match[2] ?? "block";
    const layout = parsePosition(match[3] ?? "");
    nodes.push({
      stableId: localId,
      kind: "block",
      label: typeName,
      position: layout.position,
      size: layout.size,
      attributes: { localId, typeName }
    });
  }

  for (const match of file.text.matchAll(
    /<outVariable\s+localId="(\d+)"([^>]*)>([\s\S]*?)<\/outVariable>/gi
  )) {
    const localId = match[1] ?? "";
    const layout = parsePosition(`${match[2] ?? ""}${match[3] ?? ""}`);
    const expression = match[3]?.match(/<expression>([^<]*)<\/expression>/i)?.[1] ?? `Out${localId}`;
    nodes.push({
      stableId: localId,
      kind: "outVariable",
      label: expression,
      position: layout.position,
      size: layout.size,
      attributes: { localId, expression }
    });
  }

  for (const match of file.text.matchAll(
    /<(?:inVariable|outVariable|block)\s+localId="(\d+)"[^>]*>([\s\S]*?)<\/(?:inVariable|outVariable|block)>/gi
  )) {
    const targetId = match[1] ?? "";
    for (const ref of match[2]?.matchAll(/refLocalId="(\d+)"/g) ?? []) {
      const sourceId = ref[1] ?? "";
      edges.push(edge(sourceId, targetId, `plc:${sourceId}->${targetId}`));
    }
  }

  return {
    name: pouName,
    language: "FBD",
    networks: [
      {
        id: `${pouName}:plcopen`,
        label: "PLCopen network",
        language: "FBD",
        nodes,
        edges
      }
    ],
    sfc: null
  };
}

export function buildLocalGraphModel(file: WorkspaceFile): GraphModel {
  let pou: GraphPou | null = null;
  if (file.languageId === "ld") {
    pou = parseLdGraph(file);
  } else if (file.languageId === "fbd") {
    pou = parseFbdGraph(file);
  } else if (file.languageId === "sfc") {
    pou = parseSfcGraph(file);
  } else if (file.languageId === "xml") {
    pou = parseXmlGraph(file);
  }

  const nodeIds = pou?.networks.flatMap((network) => network.nodes.map((entry) => entry.stableId)) ?? [];
  const connectorIds = pou?.networks.flatMap((network) => network.edges.map((entry) => entry.connectorId)) ?? [];
  const plcopenMetadata = file.languageId === "xml" ? readEmbeddedPlcopenMetadata(file.text) : null;

  return {
    uri: file.name,
    pous: pou ? [pou] : [],
    plcopenLayout: {
      nodeIds: plcopenMetadata?.nodeIds.length ? plcopenMetadata.nodeIds : nodeIds,
      connectorIds: plcopenMetadata?.connectorIds.length ? plcopenMetadata.connectorIds : connectorIds,
      branchGeometry: pou?.networks.flatMap((network) => network.edges) ?? [],
      actionBlocks: pou?.sfc?.actions ?? [],
      vendorAddData: plcopenMetadata?.vendorAddData ?? []
    }
  };
}
