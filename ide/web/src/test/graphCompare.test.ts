import { describe, expect, it } from "vitest";
import { workspaceFiles } from "@/features/project/samples";
import { buildLocalGraphModel } from "@/features/graph/localGraphModel";
import { graphModelsSemanticallyEqual, shapeGraphModelLikeEngine } from "@/features/graph/graphCompare";
import { enrichGraphModelForDisplay } from "@/features/graph/graphDisplayModel";
import { graphStaleWarnings } from "@/lib/graphStaleSource";

describe("graph compare", () => {
  it("treats rebuilt local models as equivalent to themselves", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(file).toBeTruthy();
    const model = buildLocalGraphModel(file!);
    expect(graphModelsSemanticallyEqual(model, buildLocalGraphModel(file!))).toBe(true);
    expect(graphStaleWarnings(file!, model)).toEqual([]);
  });

  it("treats WASM-shaped engine metadata as equivalent for sample LD and FBD", () => {
    for (const name of ["native_ladder.ld", "native_fbd.fbd"] as const) {
      const file = workspaceFiles.find((entry) => entry.name === name);
      expect(file).toBeTruthy();
      const local = buildLocalGraphModel(file!);
      const engineShaped = shapeGraphModelLikeEngine(local);
      expect(graphModelsSemanticallyEqual(local, engineShaped), name).toBe(true);
      expect(graphStaleWarnings(file!, engineShaped), name).toEqual([]);
    }
  });

  it("treats WASM FBD placeholder expressions like AND(...) as equivalent to full source", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_fbd.fbd");
    expect(file).toBeTruthy();
    const local = buildLocalGraphModel(file!);
    const engine: typeof local = {
      uri: "native_fbd.fbd",
      pous: [
        {
          name: "NativeFbd",
          language: "Function Block Diagram",
          networks: [
            {
              id: "NativeFbd:0",
              label: null,
              language: "Function Block Diagram",
              nodes: [
                {
                  stableId: "1",
                  kind: "outVariable",
                  label: "MotorCmd",
                  position: null,
                  size: null,
                  attributes: {
                    expression: "MotorCmd",
                    localId: "1",
                    value: "AND(...)"
                  }
                }
              ],
              edges: []
            }
          ],
          sfc: null
        }
      ],
      plcopenLayout: {
        nodeIds: ["1"],
        connectorIds: [],
        branchGeometry: [],
        actionBlocks: [],
        vendorAddData: []
      }
    };
    expect(graphModelsSemanticallyEqual(local, engine)).toBe(true);
    expect(graphStaleWarnings(file!, engine)).toEqual([]);
  });

  it("does not warn for sample FBD, LD, and PLCopen XML when only layout ids differ", () => {
    for (const name of ["native_ladder.ld", "native_fbd.fbd", "plcopen_fbd.xml"] as const) {
      const file = workspaceFiles.find((entry) => entry.name === name);
      expect(file).toBeTruthy();
      const local = buildLocalGraphModel(file!);
      const engineShaped = structuredClone(local);
      for (const network of engineShaped.pous[0]?.networks ?? []) {
        network.id = `${network.id}:engine`;
      }
      expect(graphStaleWarnings(file!, engineShaped), name).toEqual([]);
    }
  });

  it("expands FBD outVariable nodes for diagram display", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_fbd.fbd");
    expect(file).toBeTruthy();
    const model = buildLocalGraphModel(file!);
    const display = enrichGraphModelForDisplay(file!, model);
    const nodes = display?.pous[0]?.networks[0]?.nodes ?? [];
    expect(nodes.some((node) => node.kind === "block" && node.label === "AND")).toBe(true);
    expect(nodes.some((node) => node.kind === "input" && node.label === "Enable")).toBe(true);
    expect(nodes.some((node) => node.kind === "output" && node.label === "MotorCmd")).toBe(true);
  });
});
