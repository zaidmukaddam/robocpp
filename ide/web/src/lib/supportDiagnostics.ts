import { analyzeLocally } from "@/services/localAnalysis";
import { listTelemetryReports } from "@/services/telemetry";
import type { TargetConnection } from "@/features/target/targetConnection";
import type { LogEntry } from "@/app/types";
import type { IdeSettings, Project } from "@/types";

export type SupportDiagnosticsExport = {
  exportedAt: string;
  appVersion: string;
  privacy: "source-excluded";
  environment: {
    userAgent: string;
    language: string;
    viewport: string;
  };
  engineMode: string;
  settings: IdeSettings;
  project: {
    id: string;
    name: string;
    fileCount: number;
    builtIn: boolean;
    updatedAt: string;
  };
  files: Array<{
    name: string;
    languageId: string;
    lineCount: number;
    byteLength: number;
    errorCount: number;
    warningCount: number;
    noteCount: number;
  }>;
  target: Pick<
    TargetConnection,
    "kind" | "state" | "label" | "runtimeVersion" | "programHash" | "deployHash" | "editorMatchesTarget"
  >;
  commandLog: LogEntry[];
  telemetryReports: ReturnType<typeof listTelemetryReports>;
};

export type SupportDiagnosticsInput = {
  project: Project;
  settings: IdeSettings;
  engineMode: string;
  targetConnection: TargetConnection;
  commandLog: LogEntry[];
};

export function buildSupportDiagnostics(input: SupportDiagnosticsInput): SupportDiagnosticsExport {
  const files = input.project.files.map((file) => {
    const analysis = analyzeLocally(file);
    const counts = { error: 0, warning: 0, note: 0 };
    for (const diagnostic of analysis.diagnostics) {
      if (diagnostic.severity === "error") {
        counts.error += 1;
      } else if (diagnostic.severity === "warning") {
        counts.warning += 1;
      } else {
        counts.note += 1;
      }
    }
    return {
      name: file.name,
      languageId: file.languageId,
      lineCount: file.text.split("\n").length,
      byteLength: file.text.length,
      errorCount: counts.error,
      warningCount: counts.warning,
      noteCount: counts.note
    };
  });

  return {
    exportedAt: new Date().toISOString(),
    appVersion: "0.1.0",
    privacy: "source-excluded",
    environment: {
      userAgent: typeof navigator !== "undefined" ? navigator.userAgent : "unknown",
      language: typeof navigator !== "undefined" ? navigator.language : "unknown",
      viewport:
        typeof window !== "undefined" ? `${window.innerWidth}x${window.innerHeight}` : "unknown"
    },
    engineMode: input.engineMode,
    settings: input.settings,
    project: {
      id: input.project.id,
      name: input.project.name,
      fileCount: input.project.files.length,
      builtIn: Boolean(input.project.builtIn),
      updatedAt: input.project.updatedAt
    },
    files,
    target: {
      kind: input.targetConnection.kind,
      state: input.targetConnection.state,
      label: input.targetConnection.label,
      runtimeVersion: input.targetConnection.runtimeVersion,
      programHash: input.targetConnection.programHash,
      deployHash: input.targetConnection.deployHash,
      editorMatchesTarget: input.targetConnection.editorMatchesTarget
    },
    commandLog: input.commandLog.slice(-30),
    telemetryReports: listTelemetryReports().slice(0, 20)
  };
}

export function downloadSupportDiagnostics(exportData: SupportDiagnosticsExport, projectName: string): void {
  const slug = projectName.trim().replace(/\s+/g, "-").toLowerCase() || "project";
  const blob = new Blob([JSON.stringify(exportData, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `robocpp-support-${slug}-${Date.now()}.json`;
  anchor.click();
  URL.revokeObjectURL(url);
}
