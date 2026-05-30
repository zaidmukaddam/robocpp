import { describe, expect, it } from "vitest";
import { analyzeLocally } from "@/services/localAnalysis";
import { buildLocalGraphModel } from "@/features/graph/localGraphModel";
import { debugLocally } from "@/services/localDebug";
import { workspaceFiles } from "@/features/project/samples";

const OPEN_FILE_BUDGET_MS = 120;
const GRAPH_RENDER_BUDGET_MS = 40;
const SIMULATION_STEP_BUDGET_MS = 80;

const STARTUP_BUDGET_MS = 150;
const GENERATED_C_BUDGET_MS = 120;
const COMPLETION_BUDGET_MS = 20;

describe("performance budgets", () => {
  it("bootstraps sample analysis within startup budget", () => {
    const start = performance.now();
    for (const file of workspaceFiles.slice(0, 3)) {
      analyzeLocally(file);
    }
    expect(performance.now() - start).toBeLessThan(STARTUP_BUDGET_MS);
  });

  it("renders generated C-sized text within budget", () => {
    const file = workspaceFiles.find((entry) => entry.name === "counter.st");
    expect(file).toBeTruthy();
    const artifact = analyzeLocally(file!);
    const start = performance.now();
    JSON.stringify(artifact);
    expect(performance.now() - start).toBeLessThan(GENERATED_C_BUDGET_MS);
  });

  it("filters completions within budget", () => {
    const file = workspaceFiles.find((entry) => entry.name === "counter.st");
    expect(file).toBeTruthy();
    const analysis = analyzeLocally(file!);
    const start = performance.now();
    analysis.completions.filter((item) => item.label.startsWith("C")).slice(0, 12);
    expect(performance.now() - start).toBeLessThan(COMPLETION_BUDGET_MS);
  });
  it("analyzes counter.st within budget", () => {
    const file = workspaceFiles.find((entry) => entry.name === "counter.st");
    expect(file).toBeTruthy();
    const start = performance.now();
    analyzeLocally(file!);
    expect(performance.now() - start).toBeLessThan(OPEN_FILE_BUDGET_MS);
  });

  it("builds a local graph model within budget", () => {
    const file = workspaceFiles.find((entry) => entry.name === "native_fbd.fbd");
    expect(file).toBeTruthy();
    const start = performance.now();
    buildLocalGraphModel(file!);
    expect(performance.now() - start).toBeLessThan(GRAPH_RENDER_BUDGET_MS);
  });

  it("runs a debug trace within budget", () => {
    const file = workspaceFiles.find((entry) => entry.name === "counter.st");
    expect(file).toBeTruthy();
    const start = performance.now();
    debugLocally(file!, 3, "Count");
    expect(performance.now() - start).toBeLessThan(SIMULATION_STEP_BUDGET_MS);
  });
});
