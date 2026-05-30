import { applyGraphEdit, type GraphEditAction } from "@/features/graph/graphEdits";
import type { GraphModel, GraphSelection } from "@/features/graph/graphTypes";
import type { GraphValidation } from "@/features/graph/validateGraph";
import type { WorkspaceFile } from "@/types";

export type GraphPatch = {
  action: GraphEditAction;
  payload?: string;
  selection: GraphSelection | null;
  previousText: string;
  nextText: string;
};

/**
 * Editable graph view over a workspace file. The compiler graph model is derived
 * from source text; patches always flow through text serialization so WASM/local
 * parsers stay the single rebuild path.
 */
export class GraphDocument {
  readonly file: WorkspaceFile;
  readonly model: GraphModel;
  readonly validation: GraphValidation | null;

  constructor(file: WorkspaceFile, model: GraphModel, validation: GraphValidation | null) {
    this.file = file;
    this.model = model;
    this.validation = validation;
  }

  apply(action: GraphEditAction, payload?: string, selection?: GraphSelection | null): GraphPatch | null {
    const nextText = applyGraphEdit(this.file, action, payload, selection ?? null, this.model);
    if (nextText === this.file.text) {
      return null;
    }
    return {
      action,
      payload,
      selection: selection ?? null,
      previousText: this.file.text,
      nextText
    };
  }

  withFile(file: WorkspaceFile, model: GraphModel, validation: GraphValidation | null): GraphDocument {
    return new GraphDocument(file, model, validation);
  }

  summary(): string {
    const pou = this.model.pous[0];
    if (!pou) {
      return "Empty graph";
    }
    if (pou.sfc) {
      return `SFC ${pou.sfc.steps.length} steps, ${pou.sfc.transitions.length} transitions`;
    }
    const nodes = pou.networks.reduce((count, network) => count + network.nodes.length, 0);
    const edges = pou.networks.reduce((count, network) => count + network.edges.length, 0);
    return `${pou.language} ${pou.networks.length} network(s), ${nodes} nodes, ${edges} edges`;
  }
}

export function createGraphDocument(
  file: WorkspaceFile,
  model: GraphModel,
  validation: GraphValidation | null
): GraphDocument {
  return new GraphDocument(file, model, validation);
}
