import { describe, expect, it } from "vitest";
import { analyzeLocally } from "@/services/localAnalysis";
import { debugLocally } from "@/services/localDebug";
import { createSampleProject, renameProjectFile, removeProjectFile, updateProjectFile } from "@/features/project/projectStore";
import { buildExplorerTree, filterExplorerTree } from "@/features/explorer/explorerTree";
import { addOpenTab, activeFileAfterClose, removeOpenTab, syncOpenTabsWithProject } from "@/features/explorer/openTabs";
import {
  filesToTreePaths,
  folderForLanguage,
  treePathToFileName,
  treePathToProjectFileName
} from "@/features/explorer/projectTreePaths";
import { DEFAULT_SETTINGS, loadSettings, parseWatchList, saveSettings } from "@/stores/settingsStore";
import type { WorkspaceFile } from "@/types";

const counterFile: WorkspaceFile = {
  name: "counter.st",
  languageId: "st",
  text: `PROGRAM Counter
VAR
    Count : INT := 0;
    Done : BOOL := FALSE;
END_VAR
END_PROGRAM
`
};

describe("project store", () => {
  it("creates the packaging line sample with all language files", () => {
    const project = createSampleProject();
    expect(project.files.length).toBeGreaterThanOrEqual(6);
    expect(project.files.some((file) => file.languageId === "xml")).toBe(true);
    expect(project.files.some((file) => file.name === "target/mapping.toml")).toBe(true);
  });

  it("updates file text in place", () => {
    const project = createSampleProject();
    const next = updateProjectFile(project, "counter.st", "PROGRAM Counter\nEND_PROGRAM\n");
    const file = next.files.find((entry) => entry.name === "counter.st");
    expect(file?.text).toContain("END_PROGRAM");
  });
});

describe("settings store", () => {
  it("round-trips settings through localStorage", () => {
    const custom = { ...DEFAULT_SETTINGS, cycleTimeMs: 8, simulationCycles: 3 };
    saveSettings(custom);
    expect(loadSettings()).toEqual(custom);
  });

  it("parses watch variable lists", () => {
    expect(parseWatchList("Count, Done\nMotor")).toEqual(["Count", "Done", "Motor"]);
  });
});

describe("local analysis", () => {
  it("returns symbols and sample diagnostics for counter.st", () => {
    const analysis = analyzeLocally(counterFile);
    expect(analysis.symbols.some((symbol) => symbol.name === "Counter")).toBe(true);
    expect(analysis.completions.some((item) => item.label === "PROGRAM")).toBe(true);
    expect(analysis.diagnostics.length).toBeGreaterThan(0);
  });
});

describe("project tree paths", () => {
  it("maps PLC files into Application and PLCopen folders", () => {
    const project = createSampleProject();
    const paths = filesToTreePaths(project.files);
    expect(paths).toContain("Application/counter.st");
    expect(paths.some((path) => path.startsWith("PLCopen/"))).toBe(true);
    expect(treePathToFileName("Application/counter.st")).toBe("counter.st");
    expect(folderForLanguage("xml")).toBe("PLCopen");
    expect(treePathToProjectFileName("Target/mapping.toml", project.files)).toBe("target/mapping.toml");
    expect(treePathToProjectFileName("Application/counter.st", project.files)).toBe("counter.st");
  });

  it("builds and filters the explorer tree", () => {
    const project = createSampleProject();
    const tree = buildExplorerTree(filesToTreePaths(project.files));
    expect(tree.some((folder) => folder.name === "Application")).toBe(true);
    expect(tree.find((folder) => folder.name === "Application")?.children.some((file) => file.name === "counter.st")).toBe(
      true
    );
    const filtered = filterExplorerTree(tree, "counter");
    expect(filtered).toHaveLength(1);
    expect(filtered[0]?.children).toHaveLength(1);
  });
});

describe("open tabs", () => {
  it("adds, closes, and syncs editor tabs", () => {
    expect(addOpenTab(["counter.st"], "sequence.sfc")).toEqual(["counter.st", "sequence.sfc"]);
    expect(addOpenTab(["counter.st"], "counter.st")).toEqual(["counter.st"]);
    expect(activeFileAfterClose(["a.st", "b.st", "c.st"], "b.st", "b.st")).toBe("c.st");
    expect(activeFileAfterClose(["a.st", "b.st"], "b.st", "a.st")).toBe("a.st");
    expect(removeOpenTab(["a.st", "b.st"], "a.st")).toEqual(["b.st"]);
    expect(syncOpenTabsWithProject(["missing.st"], ["counter.st", "sequence.sfc"])).toEqual(["counter.st"]);
  });
});

describe("project file mutations", () => {
  it("renames a file and returns the resolved unique name", () => {
    const project = createSampleProject();
    const result = renameProjectFile(project, "counter.st", "counter_v2.st");
    expect(result?.fileName).toBe("counter_v2.st");
    expect(result?.project.files.some((file) => file.name === "counter_v2.st")).toBe(true);
  });

  it("removes a file when more than one exists", () => {
    const project = createSampleProject();
    const next = removeProjectFile(project, "counter.st");
    expect(next?.files.some((file) => file.name === "counter.st")).toBe(false);
  });
});

describe("local debug runner", () => {
  it("produces scan cycles and filtered watches", () => {
    const trace = debugLocally(counterFile, 3, "Count");
    expect(trace.cycles).toHaveLength(3);
    expect(trace.cycles[0]?.watches.some((watch) => watch.name === "Count")).toBe(true);
  });
});
