import { describe, expect, it } from "vitest";
import { workspaceFiles } from "@/features/project/samples";
import { buildLocalGraphModel } from "@/features/graph/localGraphModel";
import { applyGraphEdit } from "@/features/graph/graphEdits";
import { mergePlcopenMetadata, readEmbeddedPlcopenMetadata } from "@/features/graph/plcopenMetadata";
import { connectPlcopenWire, plcopenMetadataIntact, renamePlcopenExpression } from "@/features/graph/plcopenGraphEdits";
import type { WorkspaceFile } from "@/types";

function fileWithText(base: WorkspaceFile, text: string): WorkspaceFile {
  return { ...base, text };
}

describe("plcopen round trip", () => {
  it("parses PLCopen nodes with expressions and connector edges", () => {
    const file = workspaceFiles.find((entry) => entry.name === "plcopen_fbd.xml");
    expect(file).toBeTruthy();
    const model = buildLocalGraphModel(file!);
    const network = model.pous[0]?.networks[0];
    expect(network?.nodes.some((node) => node.label === "A" && node.kind === "inVariable")).toBe(true);
    expect(network?.nodes.some((node) => node.label === "C" && node.kind === "outVariable")).toBe(true);
    expect(network?.edges.some((edge) => edge.from === "1" && edge.to === "3")).toBe(true);
    expect(model.plcopenLayout.nodeIds).toEqual(["1", "2", "3", "4"]);
  });

  it("renames expressions without dropping localIds", () => {
    const file = workspaceFiles.find((entry) => entry.name === "plcopen_fbd.xml");
    expect(file).toBeTruthy();
    const renamed = renamePlcopenExpression(file!.text, "C", "Sum");
    expect(renamed).toContain("<expression>Sum</expression>");
    expect(renamed).toContain('localId="4"');
    expect(plcopenMetadataIntact(file!.text, renamed)).toBe(true);
  });

  it("connects PLCopen blocks by localId and preserves metadata on export", () => {
    const file = workspaceFiles.find((entry) => entry.name === "plcopen_fbd.xml");
    expect(file).toBeTruthy();
    const connected = connectPlcopenWire(file!.text, "2", "4");
    expect(connected).toContain('refLocalId="2"');
    expect(plcopenMetadataIntact(file!.text, connected)).toBe(true);

    const connectedModel = buildLocalGraphModel(fileWithText(file!, connected));
    const exported = mergePlcopenMetadata(connected, connectedModel);
    const metadata = readEmbeddedPlcopenMetadata(exported);
    expect(metadata?.nodeIds).toEqual(["1", "2", "3", "4"]);
    expect(metadata?.connectorIds.length).toBeGreaterThan(0);
    expect(exported).toContain("robocpp-plcopen-metadata");
  });

  it("applies rename and delete through the shared graph edit path", () => {
    const file = workspaceFiles.find((entry) => entry.name === "plcopen_fbd.xml");
    expect(file).toBeTruthy();
    const model = buildLocalGraphModel(file!);
    const block = model.pous[0]?.networks[0]?.nodes.find((node) => node.kind === "block");
    expect(block).toBeTruthy();

    const renamed = applyGraphEdit(file!, "rename", "A->InputA", null, model);
    expect(renamed).toContain("<expression>InputA</expression>");

    const deleted = applyGraphEdit(
      fileWithText(file!, renamed),
      "delete-selected",
      block!.label ?? "",
      { kind: "node", stableId: block!.stableId, networkId: model.pous[0]?.networks[0]?.id },
      model
    );
    expect(deleted).not.toContain(`localId="${block!.stableId}"`);
    expect(deleted).toContain('localId="1"');
    expect(deleted).toContain('localId="4"');
  });
});
