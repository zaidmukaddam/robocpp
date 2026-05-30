export type GraphPoint = {
  x: string;
  y: string;
};

export type GraphSize = {
  width: string;
  height: string;
};

export type GraphNode = {
  stableId: string;
  kind: string;
  label: string | null;
  position: GraphPoint | null;
  size: GraphSize | null;
  attributes: Record<string, string>;
};

export type GraphEdge = {
  connectorId: string;
  from: string;
  to: string;
  formalParameter: string | null;
};

export type GraphNetwork = {
  id: string;
  label: string | null;
  language: string;
  nodes: GraphNode[];
  edges: GraphEdge[];
};

export type SfcStepNode = {
  stableId: string;
  name: string;
  initial: boolean;
  actions: string[];
};

export type SfcTransitionNode = {
  stableId: string;
  name: string | null;
  from: string[];
  to: string[];
};

export type SfcActionNode = {
  stableId: string;
  name: string;
  qualifier: string;
};

export type SfcGraph = {
  steps: SfcStepNode[];
  transitions: SfcTransitionNode[];
  actions: SfcActionNode[];
};

export type GraphPou = {
  name: string;
  language: string;
  networks: GraphNetwork[];
  sfc: SfcGraph | null;
};

export type PlcOpenLayout = {
  nodeIds: string[];
  connectorIds: string[];
  branchGeometry: GraphEdge[];
  actionBlocks: SfcActionNode[];
  vendorAddData: string[];
};

export type GraphModel = {
  uri: string;
  pous: GraphPou[];
  plcopenLayout: PlcOpenLayout;
};

export type GraphSelection =
  | { kind: "node"; stableId: string; networkId?: string }
  | { kind: "step"; stableId: string }
  | { kind: "transition"; stableId: string }
  | { kind: "action"; stableId: string };

export type GraphPatchRecord = {
  action: string;
  payload?: string;
  at: string;
};
