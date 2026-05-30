import fs from "node:fs";
import path from "node:path";

const root = path.resolve("src");

const MODULE_PATHS = {
  types: "@/types",
  CodeEditor: "@/features/editor/CodeEditor",
  highlight: "@/features/editor/highlight",
  CommandPalette: "@/features/command-palette/CommandPalette",
  NewFileDialog: "@/features/dialogs/NewFileDialog",
  ProjectDialog: "@/features/dialogs/ProjectDialog",
  SettingsDialog: "@/features/dialogs/SettingsDialog",
  ProjectExplorer: "@/features/explorer/ProjectExplorer",
  explorerTree: "@/features/explorer/explorerTree",
  projectTreePaths: "@/features/explorer/projectTreePaths",
  openTabs: "@/features/explorer/openTabs",
  GraphCanvas: "@/features/graph/GraphCanvas",
  GraphDiagramView: "@/features/graph/GraphDiagramView",
  graphCache: "@/features/graph/graphCache",
  graphDocument: "@/features/graph/graphDocument",
  graphEdits: "@/features/graph/graphEdits",
  graphTrace: "@/features/graph/graphTrace",
  graphTypes: "@/features/graph/graphTypes",
  localGraphModel: "@/features/graph/localGraphModel",
  validateGraph: "@/features/graph/validateGraph",
  TargetInspectorPanel: "@/features/inspector/TargetInspectorPanel",
  ArtifactPanel: "@/features/panels/ArtifactPanel",
  SimulatorPanels: "@/features/panels/SimulatorPanels",
  WatchPanel: "@/features/panels/WatchPanel",
  projectStore: "@/features/project/projectStore",
  projectBundle: "@/features/project/projectBundle",
  projectTemplates: "@/features/project/projectTemplates",
  projectSnapshots: "@/features/project/projectSnapshots",
  fileTemplates: "@/features/project/fileTemplates",
  samples: "@/features/project/samples",
  plcopenIO: "@/features/project/plcopenIO",
  editHistory: "@/features/project/editHistory",
  buildSource: "@/features/project/buildSource",
  TargetConnectionBar: "@/features/target/TargetConnectionBar",
  TargetMappingEditor: "@/features/target/TargetMappingEditor",
  targetConnection: "@/features/target/targetConnection",
  targetDeployValidation: "@/features/target/targetDeployValidation",
  targetMapping: "@/features/target/targetMapping",
  deployClient: "@/features/target/deployClient",
  symbolCoverage: "@/features/target/symbolCoverage",
  workspaceLayout: "@/features/workspace/workspaceLayout",
  layoutPresets: "@/features/workspace/layoutPresets",
  editorTabsStore: "@/features/workspace/editorTabsStore",
  keyboardShortcuts: "@/lib/keyboardShortcuts",
  artifactLifecycle: "@/lib/artifactLifecycle",
  diagnosticQuickFixes: "@/lib/diagnosticQuickFixes",
  useMountEffect: "@/lib/hooks/useMountEffect",
  artifactStore: "@/stores/artifactStore",
  settingsStore: "@/stores/settingsStore",
  forcedValuesStore: "@/stores/forcedValuesStore",
  languageServiceBackend: "@/services/languageServiceBackend",
  localAnalysis: "@/services/localAnalysis",
  localDebug: "@/services/localDebug",
  localRunner: "@/services/localRunner",
  wasmClient: "@/services/wasmClient",
  telemetry: "@/services/telemetry",
  ErrorBoundary: "@/components/layout/ErrorBoundary",
  NarrowViewportGate: "@/components/layout/NarrowViewportGate"
};

function walk(dir, files = []) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      walk(full, files);
    } else if (/\.(ts|tsx)$/.test(entry.name)) {
      files.push(full);
    }
  }
  return files;
}

function rewriteImports(content) {
  return content.replace(/from (["'])(\.\.?\/[^"']+)\1/g, (match, quote, importPath) => {
    const base = path.basename(importPath).replace(/\.(ts|tsx)$/, "");
    if (MODULE_PATHS[base]) {
      return `from ${quote}${MODULE_PATHS[base]}${quote}`;
    }
    return match;
  });
}

for (const file of walk(root)) {
  const original = fs.readFileSync(file, "utf8");
  const updated = rewriteImports(original);
  if (updated !== original) {
    fs.writeFileSync(file, updated);
    console.log("updated", path.relative(root, file));
  }
}
