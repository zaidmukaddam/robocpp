import { describe, expect, it } from "vitest";
import { createSampleProject } from "@/features/project/projectStore";
import { dirtyFileNames } from "@/lib/projectDirty";
import { projectSnapshot } from "@/features/project/projectBundle";

describe("dirtyFileNames", () => {
  it("returns empty when snapshot matches", () => {
    const project = createSampleProject();
    const snapshot = projectSnapshot(project);
    expect(dirtyFileNames(project, snapshot).size).toBe(0);
  });

  it("flags only changed files", () => {
    const project = createSampleProject();
    const snapshot = projectSnapshot(project);
    const target = project.files[0]?.name;
    expect(target).toBeTruthy();
    const edited = {
      ...project,
      files: project.files.map((file) =>
        file.name === target ? { ...file, text: `${file.text}\n` } : file
      )
    };
    const dirty = dirtyFileNames(edited, snapshot);
    expect(dirty.has(target!)).toBe(true);
    expect(dirty.size).toBe(1);
  });
});
