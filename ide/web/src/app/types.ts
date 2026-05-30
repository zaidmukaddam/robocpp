import { createSampleProject } from "@/features/project/projectStore";
import { readEditorTabs, resolveEditorTabsState } from "@/features/workspace/editorTabsStore";

export type InspectorTab = "symbols" | "completions" | "hover" | "target";
export type OutputPanel = "Diagnostics" | "Scan Trace" | "Trends" | "Watches" | "Generated C" | "Artifacts" | "Output";
export type DialogMode = "new" | "open" | null;

export type LogEntry = {
  time: string;
  message: string;
  kind: "info" | "action" | "error";
};

export function nowLabel(): string {
  return new Date().toLocaleTimeString(undefined, { hour: "2-digit", minute: "2-digit", second: "2-digit" });
}

function createBootstrappedAppState() {
  const project = createSampleProject();
  const editorTabs = resolveEditorTabsState(
    readEditorTabs(project.id),
    project.files.map((file) => file.name)
  );
  return { project, ...editorTabs };
}

export const bootstrappedApp = createBootstrappedAppState();
