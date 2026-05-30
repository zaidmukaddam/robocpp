import type { GraphModel } from "@/features/graph/graphTypes";
import type { DebugTrace, RunTrace } from "@/types";
import { buildTraceLabelSet } from "@/features/graph/graphTrace";

export type PowerFlowState = {
  nodeId: string;
  energized: boolean;
};

export function computeLdPowerFlow(
  model: GraphModel,
  runTrace: RunTrace | null,
  debugTrace: DebugTrace | null
): Map<string, boolean> {
  const energizedLabels = buildTraceLabelSet(runTrace, debugTrace);
  const states = new Map<string, boolean>();
  const pou = model.pous[0];
  if (!pou) {
    return states;
  }

  for (const network of pou.networks) {
    let flow = true;
    for (const node of network.nodes) {
      if (node.kind === "contact") {
        const negated = node.attributes.negated === "true";
        const labelActive = node.label ? energizedLabels.has(node.label) : false;
        flow = flow && (negated ? !labelActive : labelActive);
      }
      states.set(node.stableId, flow);
    }
  }

  return states;
}
