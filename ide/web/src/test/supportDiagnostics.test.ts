import { describe, expect, it } from "vitest";
import { buildSupportDiagnostics } from "@/lib/supportDiagnostics";
import { createSampleProject } from "@/features/project/projectStore";
import { DEFAULT_SETTINGS } from "@/stores/settingsStore";
import { readTargetConnection } from "@/features/target/targetConnection";

describe("support diagnostics export", () => {
  it("builds a source-free diagnostics bundle", () => {
    const project = createSampleProject();
    const exportData = buildSupportDiagnostics({
      project,
      settings: DEFAULT_SETTINGS,
      engineMode: "local",
      targetConnection: readTargetConnection(),
      commandLog: [{ time: "12:00", message: "Opened project", kind: "info" }]
    });

    expect(exportData.privacy).toBe("source-excluded");
    expect(exportData.project.fileCount).toBeGreaterThan(0);
    expect(exportData.files.every((file) => !("text" in file))).toBe(true);
    expect(JSON.stringify(exportData)).not.toContain("PROGRAM Counter");
    expect(exportData.files.some((file) => file.name.endsWith(".st"))).toBe(true);
  });
});
