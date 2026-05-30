import { describe, expect, it } from "vitest";
import { workspaceFiles } from "@/features/project/samples";
import { buildLocalGraphModel } from "@/features/graph/localGraphModel";
import { buildDeployPackage } from "@/features/target/deployClient";
import { createGraphDocument } from "@/features/graph/graphDocument";
import { applyGraphEdit } from "@/features/graph/graphEdits";
import { listProjectArtifacts, saveGeneratedCArtifact } from "@/stores/artifactStore";
import { reportError } from "@/services/telemetry";
import { createEditHistory } from "@/features/project/editHistory";
import { validateGraphLocalFile } from "@/features/graph/validateGraph";
import { createSampleProject, reorderProjectFile } from "@/features/project/projectStore";
import { resolveBuildSourceFile } from "@/features/project/buildSource";
import { isTargetMappingFile } from "@/features/target/targetMapping";
import { validateTargetDeployment } from "@/features/target/targetDeployValidation";
import {
  DEFAULT_TARGET_MAPPING_TEXT,
  analyzeTargetMapping,
  parseTargetMapping,
  serializeTargetMapping,
  validateTargetMapping
} from "@/features/target/targetMapping";

describe("golden graph fixtures", () => {
  it("matches ladder graph shape for native_ladder.ld", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(file).toBeTruthy();
    const model = buildLocalGraphModel(file!);
    expect(model.pous[0]?.name).toBe("NativeLd");
    const labels = model.pous[0]?.networks[0]?.nodes
      .filter((node) => node.kind === "contact" || node.kind === "coil")
      .map((node) => node.label);
    expect(labels).toEqual(["Start", "Motor"]);
    expect(model.pous[0]?.networks[0]?.nodes.some((node) => node.kind === "leftPowerRail")).toBe(true);
  });

  it("matches FBD graph shape for native_fbd.fbd", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_fbd.fbd");
    expect(file).toBeTruthy();
    const model = buildLocalGraphModel(file!);
    expect(model.pous[0]?.networks[0]?.nodes).toHaveLength(1);
    expect(model.pous[0]?.networks[0]?.nodes[0]?.kind).toBe("outVariable");
    expect(model.pous[0]?.networks[0]?.nodes.some((node) => node.label === "MotorCmd")).toBe(true);
  });

  it("matches SFC graph shape for sequence.sfc", () => {
    const file = workspaceFiles.find((entry) => entry.name === "sequence.sfc");
    expect(file).toBeTruthy();
    const model = buildLocalGraphModel(file!);
    expect(model.pous[0]?.sfc?.steps.some((step) => step.initial)).toBe(true);
    expect(model.pous[0]?.sfc?.actions.some((action) => action.name === "Run")).toBe(true);
    const go = model.pous[0]?.sfc?.transitions.find((transition) => transition.name === "Go");
    expect(go?.from).toEqual(["Start"]);
    expect(go?.to).toEqual(["Run"]);
  });
});

describe("target mapping fixtures", () => {
  it("round-trips the default mapping sample", () => {
    const parsed = parseTargetMapping(DEFAULT_TARGET_MAPPING_TEXT);
    expect(parsed.entries).toHaveLength(2);
    expect(serializeTargetMapping(parsed)).toContain("Motor, io/motor.txt, bool");
    expect(validateTargetMapping(parsed)).toEqual([]);
  });

  it("does not run IEC syntax analysis on mapping.toml", () => {
    const analysis = analyzeTargetMapping("target/mapping.toml", DEFAULT_TARGET_MAPPING_TEXT);
    expect(analysis.diagnostics).toEqual([]);
    expect(analysis.symbols).toHaveLength(2);
    expect(analysis.symbols.some((symbol) => symbol.name === "Motor")).toBe(true);
  });
});

describe("graph edit fixtures", () => {
  it("appends a ladder rung to native ladder source", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(file).toBeTruthy();
    const next = applyGraphEdit(file!, "add-rung");
    expect(next).toContain("NewInput");
    expect(next).toContain("NewOutput");
  });
});

describe("project reorder fixtures", () => {
  it("reorders files within the project list", () => {
    const project = createSampleProject();
    const first = project.files[0]?.name;
    const second = project.files[1]?.name;
    expect(first).toBeTruthy();
    expect(second).toBeTruthy();
    const next = reorderProjectFile(project, second!, first!);
    expect(next.files[0]?.name).toBe(second);
    expect(next.files[1]?.name).toBe(first);
  });
});

describe("edit history fixtures", () => {
  it("supports undo and redo for a single file", () => {
    const history = createEditHistory();
    history.init("demo.st", "A");
    history.push("demo.st", "B");
    history.push("demo.st", "C");
    expect(history.undo("demo.st")).toBe("B");
    expect(history.redo("demo.st")).toBe("C");
    expect(history.canUndo("demo.st")).toBe(true);
    expect(history.canRedo("demo.st")).toBe(false);
  });
});

describe("graph validation fixtures", () => {
  it("validates native ladder samples without errors", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(file).toBeTruthy();
    const validation = validateGraphLocalFile(file!);
    expect(validation.valid).toBe(true);
  });

  it("validates sequence.sfc transition wiring", () => {
    const file = workspaceFiles.find((entry) => entry.name === "sequence.sfc");
    expect(file).toBeTruthy();
    const model = buildLocalGraphModel(file!);
    const go = model.pous[0]?.sfc?.transitions.find((transition) => transition.name === "Go");
    expect(go?.from.length).toBeGreaterThan(0);
    expect(go?.to.length).toBeGreaterThan(0);
  });
});

describe("deployment validation fixtures", () => {
  it("flags missing mapping files in empty projects", () => {
    const project = createSampleProject();
    const stripped = {
      ...project,
      files: project.files.filter((file) => file.name !== "target/mapping.toml")
    };
    const issues = validateTargetDeployment(stripped, null);
    expect(issues.some((issue) => issue.message.includes("Missing target/mapping.toml"))).toBe(true);
  });

  it("prompts for Build C when metadata is absent", () => {
    const project = createSampleProject();
    const issues = validateTargetDeployment(project, null);
    expect(issues.some((issue) => issue.severity === "note" && issue.message.includes("Build C"))).toBe(true);
  });

  it("prefers a PLC program over mapping.toml for Build C", () => {
    const project = createSampleProject();
    const mappingFile = project.files.find((file) => isTargetMappingFile(file.name))!;
    const source = resolveBuildSourceFile(project, mappingFile);
    expect(source?.name).not.toBe(mappingFile.name);
    expect(source?.languageId).not.toBe("mapping");
  });
});

describe("graph document fixtures", () => {
  it("applies ladder edits through the shared graph document", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(file).toBeTruthy();
    const model = buildLocalGraphModel(file!);
    const document = createGraphDocument(file!, model, { valid: true, diagnostics: [] });
    const patch = document.apply("add-contact", "Start");
    expect(patch?.nextText).toContain("NewContact");
  });

  it("supports negated contacts and set coils in native ladder text", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_ladder.ld");
    expect(file).toBeTruthy();
    const negated = applyGraphEdit(file!, "add-negated-contact", "Start");
    expect(negated).toContain("CONTACT NOT NewContact");
    const setCoil = applyGraphEdit({ ...file!, text: negated }, "add-set-coil");
    expect(setCoil).toContain("SET NewCoil");
  });
});

describe("artifact fixtures", () => {
  it("stores generated C artifacts per project", () => {
    const project = createSampleProject();
    const artifact = saveGeneratedCArtifact(project.id, "counter.st", {
      source: "int main() { return 0; }",
      metadata: {
        filenameHint: "counter.c",
        scanEntrypoints: [],
        stateLayout: [],
        ioSymbols: [],
        accessPaths: [],
        retainedFields: [],
        targetHooks: [],
        debugSymbols: []
      }
    });
    const listed = listProjectArtifacts(project.id);
    expect(listed.some((entry) => entry.id === artifact.id)).toBe(true);
  });

  it("builds deploy packages when metadata is present", () => {
    const project = createSampleProject();
    const { issues } = buildDeployPackage(project, {
      filenameHint: "demo.c",
      scanEntrypoints: [{ name: "demo_scan", signature: "void demo_scan(void)" }],
      stateLayout: [],
      ioSymbols: [],
      accessPaths: [],
      retainedFields: [],
      targetHooks: ["target_init"],
      debugSymbols: []
    });
    expect(issues.some((issue) => issue.severity === "error")).toBe(false);
  });
});

describe("telemetry fixtures", () => {
  it("records local error reports without throwing", () => {
    const report = reportError(new Error("fixture failure"), { area: "test" });
    expect(report?.message).toBe("fixture failure");
  });
});
