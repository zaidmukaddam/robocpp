import { describe, expect, it } from "vitest";
import { workspaceFiles } from "@/features/project/samples";
import { buildLocalGraphModel } from "@/features/graph/localGraphModel";
import { createGraphDocument } from "@/features/graph/graphDocument";
import {
  applyGraphEdit,
  connectFbdWire,
  deleteFbdLabel,
  networkIndexFromSelection
} from "@/features/graph/graphEdits";
import { createEditHistory } from "@/features/project/editHistory";
import type { GraphSelection } from "@/features/graph/graphTypes";
import type { WorkspaceFile } from "@/types";

function fileWithText(base: WorkspaceFile, text: string): WorkspaceFile {
  return { ...base, text };
}

function selectionForLabel(
  file: WorkspaceFile,
  model: ReturnType<typeof buildLocalGraphModel>,
  label: string
): GraphSelection | null {
  const pou = model.pous[0];
  if (!pou) {
    return null;
  }
  for (const network of pou.networks) {
    const node = network.nodes.find((entry) => entry.label === label);
    if (node) {
      return { kind: "node", stableId: node.stableId, networkId: network.id };
    }
  }
  const step = pou.sfc?.steps.find((entry) => entry.name === label);
  if (step) {
    return { kind: "step", stableId: step.stableId };
  }
  const transition = pou.sfc?.transitions.find((entry) => entry.name === label);
  if (transition) {
    return { kind: "transition", stableId: transition.stableId };
  }
  return null;
}

describe("graph edit acceptance", () => {
  it("moves the selected ladder rung instead of always rung zero", () => {
    const base = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(base).toBeTruthy();
    const twoRungs = fileWithText(
      base!,
      `${base!.text.replace("END_LADDER", `RUNG
    CONTACT Aux;
    COIL AuxOut;
END_RUNG
END_LADDER`)}`
    );
    const model = buildLocalGraphModel(twoRungs);
    const selection = selectionForLabel(twoRungs, model, "Aux");
    expect(selection).toBeTruthy();
    expect(networkIndexFromSelection(model, selection)).toBe(1);

    const moved = applyGraphEdit(twoRungs, "move-rung", "1->up", selection, model);
    const auxIndex = moved.indexOf("CONTACT Aux");
    const startIndex = moved.indexOf("CONTACT Start");
    expect(auxIndex).toBeGreaterThan(-1);
    expect(startIndex).toBeGreaterThan(-1);
    expect(auxIndex).toBeLessThan(startIndex);
  });

  it("wires an FBD source into the selected target expression", () => {
    const base = workspaceFiles.find((entry) => entry.name === "native_fbd.fbd");
    expect(base).toBeTruthy();
    const extended = fileWithText(
      base!,
      base!.text.replace(
        "END_NETWORK",
        `    OUT Ready := NOT(MotorCmd);
END_NETWORK`
      )
    );
    const wired = connectFbdWire(extended.text, "Enable", "Ready", 0);
    expect(wired).toContain("OUT Ready := AND(NOT(MotorCmd), Enable)");
    expect(wired).not.toContain("NewInput");
  });

  it("deletes an FBD output block without leaving Removed placeholders", () => {
    const base = workspaceFiles.find((entry) => entry.name === "native_fbd.fbd");
    expect(base).toBeTruthy();
    const next = deleteFbdLabel(base!.text, "MotorCmd", 0);
    expect(next).not.toMatch(/\bRemoved\b/);
    expect(next).not.toContain("OUT MotorCmd");
    expect(next).toContain("Enable");
    expect(next).toContain("Interlock");
  });

  it("round-trips FBD graph edges after connect and delete edits", () => {
    const base = workspaceFiles.find((entry) => entry.name === "native_fbd.fbd");
    expect(base).toBeTruthy();
    const model = buildLocalGraphModel(base!);
    const motorSelection = selectionForLabel(base!, model, "MotorCmd");
    const connected = applyGraphEdit(base!, "connect", "Enable->MotorCmd", motorSelection, model);
    const connectedModel = buildLocalGraphModel(fileWithText(base!, connected));
    const edges = connectedModel.pous[0]?.networks[0]?.edges ?? [];
    expect(edges.some((edge) => edge.from === "Enable" && edge.to === "MotorCmd")).toBe(true);

    const deleted = deleteFbdLabel(connected, "MotorCmd", 0);
    const deletedModel = buildLocalGraphModel(fileWithText(base!, deleted));
    expect(deletedModel.pous[0]?.networks[0]?.nodes.some((node) => node.label === "MotorCmd")).toBe(false);
    expect(deleted).not.toMatch(/\bRemoved\b/);
  });

  it("adds FBD networks with declared unique placeholder variables", () => {
    const base = workspaceFiles.find((entry) => entry.name === "native_fbd.fbd");
    expect(base).toBeTruthy();

    const first = applyGraphEdit(base!, "add-network");
    expect(first).toContain("NewOutput : BOOL;");
    expect(first).toContain("NewInputA : BOOL;");
    expect(first).toContain("NewInputB : BOOL;");
    expect(first).toContain("OUT NewOutput := AND(NewInputA, NewInputB);");

    const second = applyGraphEdit(fileWithText(base!, first), "add-network");
    expect(second).toContain("NewOutput1 : BOOL;");
    expect(second).toContain("NewInputA1 : BOOL;");
    expect(second).toContain("NewInputB1 : BOOL;");
    expect(second).toContain("OUT NewOutput1 := AND(NewInputA1, NewInputB1);");
  });

  it("does not append native FBD text to PLCopen XML graph edits", () => {
    const base = workspaceFiles.find((entry) => entry.name === "plcopen_fbd.xml");
    expect(base).toBeTruthy();

    expect(applyGraphEdit(base!, "add-network")).toBe(base!.text);
    expect(applyGraphEdit(base!, "add-fbd-literal")).toBe(base!.text);
  });

  it("adds and deletes SFC transitions through the graph document", () => {
    const base = workspaceFiles.find((entry) => entry.name === "sequence.sfc");
    expect(base).toBeTruthy();
    const model = buildLocalGraphModel(base!);
    const document = createGraphDocument(base!, model, { valid: true, diagnostics: [] });
    const startSelection = selectionForLabel(base!, model, "Start");
    const added = document.apply("add-transition", "Start", startSelection);
    expect(added?.nextText).toContain("TRANSITION");
    expect(added?.nextText).toContain("FROM Start");

    const addedModel = buildLocalGraphModel(fileWithText(base!, added!.nextText));
    const transition = addedModel.pous[0]?.sfc?.transitions.find((entry) => entry.from.includes("Start"));
    expect(transition).toBeTruthy();
    const transitionSelection: GraphSelection = {
      kind: "transition",
      stableId: transition!.stableId
    };
    const deleted = document
      .withFile(fileWithText(base!, added!.nextText), addedModel, { valid: true, diagnostics: [] })
      .apply("delete-selected", transition!.name ?? transition!.stableId, transitionSelection);
    expect(deleted?.nextText).not.toContain(`TRANSITION ${transition!.name}`);
  });

  it("generates unique SFC step labels", () => {
    const base = workspaceFiles.find((entry) => entry.name === "sequence.sfc");
    expect(base).toBeTruthy();

    const first = applyGraphEdit(base!, "add-step");
    const second = applyGraphEdit(fileWithText(base!, first), "add-step");
    expect(first).toContain("STEP NewStep;");
    expect(second).toContain("STEP NewStep1;");
    expect(second.match(/STEP NewStep;/g)).toHaveLength(1);
  });

  it("serializes LD element comments into source and re-parses them", () => {
    const base = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(base).toBeTruthy();
    const commented = applyGraphEdit(base!, "set-property", "Motor:comment:Motor output coil");
    expect(commented).toContain("(* Motor output coil *)");
    expect(commented).toContain("COIL Motor;");

    const model = buildLocalGraphModel(fileWithText(base!, commented));
    const coil = model.pous[0]?.networks[0]?.nodes.find((node) => node.label === "Motor");
    expect(coil?.attributes.comment).toBe("Motor output coil");
  });

  it("changes coil mode between COIL, SET, and RESET", () => {
    const base = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(base).toBeTruthy();

    const setMode = applyGraphEdit(base!, "set-property", "Motor:coil-mode:set");
    expect(setMode).toContain("SET Motor;");

    const resetMode = applyGraphEdit(fileWithText(base!, setMode), "set-property", "Motor:coil-mode:reset");
    expect(resetMode).toContain("RESET Motor;");
    expect(resetMode).not.toMatch(/^\s*SET\s+Motor;/m);

    const normalMode = applyGraphEdit(fileWithText(base!, resetMode), "set-property", "Motor:coil-mode:normal");
    expect(normalMode).toContain("COIL Motor;");
    expect(normalMode).not.toMatch(/^\s*RESET\s+Motor;/m);
  });

  it("supports undo after graph property edits", () => {
    const base = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(base).toBeTruthy();
    const history = createEditHistory();
    history.init(base!.name, base!.text);

    const commented = applyGraphEdit(base!, "set-property", "Motor:comment:Hold output");
    history.push(base!.name, commented);
    expect(history.canUndo(base!.name)).toBe(true);
    expect(history.undo(base!.name)).toBe(base!.text);
    expect(history.redo(base!.name)).toBe(commented);
  });
});
