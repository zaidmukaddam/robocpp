import { describe, expect, it } from "vitest";
import { createSampleProject } from "@/features/project/projectStore";
import { persistEditorTabs, readEditorTabs, resolveEditorTabsState } from "@/features/workspace/editorTabsStore";

describe("editor tabs store", () => {
  it("resolves saved tabs against the current project files", () => {
    const project = createSampleProject();
    const fileNames = project.files.map((file) => file.name);
    const resolved = resolveEditorTabsState(
      {
        openTabNames: ["counter.st", "missing.st", "sequence.sfc"],
        activeFileName: "missing.st"
      },
      fileNames
    );
    expect(resolved.openTabNames).toEqual(["counter.st", "sequence.sfc"]);
    expect(resolved.activeFileName).toBe("counter.st");
  });

  it("round-trips tabs per project through localStorage", () => {
    const project = createSampleProject();
    persistEditorTabs(project.id, {
      openTabNames: ["counter.st", "sequence.sfc"],
      activeFileName: "sequence.sfc"
    });
    expect(readEditorTabs(project.id)).toEqual({
      openTabNames: ["counter.st", "sequence.sfc"],
      activeFileName: "sequence.sfc"
    });
  });
});
