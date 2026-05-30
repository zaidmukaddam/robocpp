import { describe, expect, it } from "vitest";
import { diffDeployPackages } from "@/features/target/deployDiff";
import type { DeployPackage } from "@/features/target/deployClient";

const basePackage: DeployPackage = {
  project: "Demo",
  generatedAt: "2026-01-01T00:00:00.000Z",
  sourceFiles: ["counter.st"],
  mapping: { entries: [] },
  metadata: null,
  adapterStubs: [],
  adapterArtifacts: []
};

describe("deployDiff", () => {
  it("reports added files when no baseline exists", () => {
    const entries = diffDeployPackages(null, basePackage);
    expect(entries.some((entry) => entry.status === "added")).toBe(true);
  });

  it("detects source file changes", () => {
    const current = { ...basePackage, sourceFiles: ["counter.st", "sequence.sfc"] };
    const entries = diffDeployPackages(basePackage, current);
    expect(entries.some((entry) => entry.path === "source:sequence.sfc" && entry.status === "added")).toBe(true);
  });
});
