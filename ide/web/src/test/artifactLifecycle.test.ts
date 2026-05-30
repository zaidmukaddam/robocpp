import { describe, expect, it } from "vitest";
import { createSampleProject } from "@/features/project/projectStore";
import { fingerprintFile, isArtifactStale, sourceTextHash } from "@/lib/artifactLifecycle";
import { saveGeneratedCArtifact } from "@/stores/artifactStore";

describe("artifact lifecycle", () => {
  it("marks artifacts stale when source text changes", () => {
    const project = createSampleProject();
    const source = project.files.find((file) => file.name === "counter.st")!;
    const artifact = saveGeneratedCArtifact(project.id, source.name, {
      source: "int scan(void) { return 0; }",
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
    }, source.text);
    expect(isArtifactStale(artifact, project)).toBe(false);
    const changed = {
      ...project,
      files: project.files.map((file) =>
        file.name === source.name ? { ...file, text: `${file.text}\n` } : file
      )
    };
    expect(isArtifactStale(artifact, changed)).toBe(true);
  });

  it("fingerprints file text consistently", () => {
    expect(sourceTextHash("abc")).toBe(sourceTextHash("abc"));
    expect(fingerprintFile({ name: "a.st", languageId: "st", text: "x" })).toContain("a.st:");
  });
});
