import { buildLocalGraphModel } from "@/features/graph/localGraphModel";
import { graphModelsSemanticallyEqual, graphSemanticNodeCount } from "@/features/graph/graphCompare";
import { isGraphicalLanguage } from "@/features/graph/graphCache";
import type { GraphModel } from "@/features/graph/graphTypes";
import type { WorkspaceFile } from "@/types";

export type GraphStaleWarning = {
  severity: "warning" | "note";
  message: string;
};

export function graphStaleWarnings(file: WorkspaceFile, model: GraphModel | null): GraphStaleWarning[] {
  if (!model || !isGraphicalLanguage(file.languageId)) {
    return [];
  }

  const warnings: GraphStaleWarning[] = [];
  const rebuilt = buildLocalGraphModel(file);
  const rebuiltPou = rebuilt.pous[0];
  const modelPou = model.pous[0];

  if (!rebuiltPou || !modelPou) {
    return [{ severity: "warning", message: "Graph model could not be rebuilt from current source text." }];
  }

  if (!graphModelsSemanticallyEqual(model, rebuilt)) {
    const rebuiltNodes = graphSemanticNodeCount(rebuilt);
    const modelNodes = graphSemanticNodeCount(model);
    const countSuffix =
      rebuiltNodes === modelNodes
        ? `${modelNodes} nodes, but wiring or labels differ`
        : `${modelNodes} nodes vs ${rebuiltNodes} parsed from source`;
    warnings.push({
      severity: "warning",
      message: `Graph structure (${countSuffix}). Text edits may not round-trip losslessly.`
    });
  }

  if (file.languageId === "xml" && model.plcopenLayout.vendorAddData.length > 0) {
    const preserved = model.plcopenLayout.vendorAddData.every((chunk) => file.text.includes(chunk.slice(0, 40)));
    if (!preserved) {
      warnings.push({
        severity: "warning",
        message: "PLCopen vendor metadata in the graph is no longer present in the XML source."
      });
    }
  }

  if (file.text.includes("(* graph-only") || file.text.includes("<!-- graph-only")) {
    warnings.push({
      severity: "note",
      message: "Source contains graph-only constructs that may not appear in the diagram view."
    });
  }

  return warnings;
}
