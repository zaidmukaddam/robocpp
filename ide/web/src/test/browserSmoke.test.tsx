import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { SafetyPolicyPanel } from "@/features/target/SafetyPolicyPanel";
import { DEFAULT_SAFETY_POLICY } from "@/features/target/safetyPolicy";
import { validateTargetDeployment } from "@/features/target/targetDeployValidation";
import { createSampleProject } from "@/features/project/projectStore";
import { buildDeployPackage } from "@/features/target/deployClient";
import { applyGraphEdit } from "@/features/graph/graphEdits";
import { CommandBar } from "@/features/layout/CommandBar";
import { analyzeLocally } from "@/services/localAnalysis";
import { buildSupportDiagnostics } from "@/lib/supportDiagnostics";
import { DEFAULT_SETTINGS } from "@/stores/settingsStore";
import { readTargetConnection } from "@/features/target/targetConnection";

const noop = vi.fn();

describe("browser smoke flows", () => {
  it("renders the safety policy panel", () => {
    render(<SafetyPolicyPanel policy={DEFAULT_SAFETY_POLICY} onChange={() => undefined} onSave={() => undefined} />);
    expect(screen.getByLabelText("Safety policy")).toBeTruthy();
    expect(screen.getByLabelText("Watchdog (ms)")).toBeTruthy();
  });

  it("validates deploy readiness for the sample project", () => {
    const project = createSampleProject();
    const { package: deployPackage } = buildDeployPackage(project, null);
    const issues = validateTargetDeployment(project, deployPackage.metadata);
    expect(issues.length).toBeGreaterThan(0);
  });

  it("applies a ladder graph edit to sample LD text", () => {
    const project = createSampleProject();
    const ladder = project.files.find((file) => file.languageId === "ld");
    expect(ladder).toBeTruthy();
    const next = applyGraphEdit(ladder!, "add-rung");
    expect(next).toContain("RUNG");
    expect(next).toContain("END_RUNG");
  });

  it("renders the command bar with project, build, and exchange menus", () => {
    render(
      <CommandBar
        runState="idle"
        onNewProject={noop}
        onOpenProject={noop}
        onSave={noop}
        onExportBundle={noop}
        onImportBundle={noop}
        onNewFile={noop}
        onCheck={noop}
        onRun={noop}
        onBuildC={noop}
        onImportPlcopen={noop}
        onExportPlcopen={noop}
        onDeploy={noop}
        onSettings={noop}
      />
    );
    expect(screen.getByRole("toolbar", { name: "Studio actions" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Project menu" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Build menu" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Exchange menu" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Run simulation" })).toBeTruthy();
  });

  it("checks a sample ST file through the local analysis path", () => {
    const project = createSampleProject();
    const stFile = project.files.find((file) => file.languageId === "st");
    expect(stFile).toBeTruthy();
    const analysis = analyzeLocally(stFile!);
    expect(analysis.symbols.length).toBeGreaterThan(0);
  });

  it("exports support diagnostics without project source text", () => {
    const project = createSampleProject();
    const exportData = buildSupportDiagnostics({
      project,
      settings: DEFAULT_SETTINGS,
      engineMode: "local",
      targetConnection: readTargetConnection(),
      commandLog: []
    });
    expect(exportData.files.length).toBe(project.files.length);
    expect(JSON.stringify(exportData)).not.toContain("Count := Count + 1");
  });
});
