import { startTransition, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { usePanelRef } from "react-resizable-panels";
import { Activity, AlertTriangle, CheckCircle2, Gauge } from "lucide-react";
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from "@/components/ui/resizable";
import { CommandPalette, buildFilePaletteItems, type CommandPaletteItem } from "@/features/command-palette/CommandPalette";
import { NewFileDialog } from "@/features/dialogs/NewFileDialog";
import { ProjectDialog } from "@/features/dialogs/ProjectDialog";
import { FindReplaceDialog, type FindReplaceMode } from "@/features/dialogs/FindReplaceDialog";
import { RenameDialog } from "@/features/dialogs/RenameDialog";
import { TargetConnectionBar } from "@/features/target/TargetConnectionBar";
import { NarrowViewportGate } from "@/components/layout/NarrowViewportGate";
import { CommandBar } from "@/features/layout/CommandBar";
import { EditorTabBar, tabPanelId } from "@/features/editor/EditorTabBar";
import { InspectorPanel } from "@/features/inspector/InspectorPanel";
import { useMountEffect } from "@/lib/hooks/useMountEffect";
import { dirtyFileNames as computeDirtyFileNames } from "@/lib/projectDirty";
import {
  addProjectFile,
  createEmptyProject,
  createProjectFromTemplate,
  deleteSavedProject,
  listOpenableProjects,
  persistProject,
  renameProjectFile,
  removeProjectFile,
  reorderProjectFile,
  updateProjectFile
} from "@/features/project/projectStore";
import {
  addOpenTab,
  activeFileAfterClose,
  removeOpenTab,
  renameOpenTab,
  syncOpenTabsWithProject
} from "@/features/explorer/openTabs";
import { persistEditorTabs, readEditorTabs, resolveEditorTabsState } from "@/features/workspace/editorTabsStore";
import { ProjectExplorer, type ProjectExplorerHandle } from "@/features/explorer/ProjectExplorer";
import { resolveBuildSourceFile } from "@/features/project/buildSource";
import { createEditHistory } from "@/features/project/editHistory";
import { validateTargetDeployment } from "@/features/target/targetDeployValidation";
import { treePathsSignature } from "@/features/explorer/projectTreePaths";
import {
  downloadArtifact,
  clearProjectArtifacts,
  listProjectArtifacts,
  removeProjectArtifact,
  renameProjectArtifact,
  saveDiagnosticReport,
  saveGeneratedCArtifact,
  savePlcopenExportArtifact,
  saveProjectArtifact,
  saveTraceArtifact
} from "@/stores/artifactStore";
import { exportProjectBundle, pickProjectBundle, projectSnapshot } from "@/features/project/projectBundle";
import { sourceTextHash } from "@/lib/artifactLifecycle";
import {
  readForcedValues,
  removeForcedValue,
  upsertForcedValue,
  type ForcedValue
} from "@/stores/forcedValuesStore";
import { saveProjectSnapshot } from "@/features/project/projectSnapshots";
import type { ProjectTemplateId } from "@/features/project/projectTemplates";
import { LAYOUT_PRESETS } from "@/features/workspace/layoutPresets";
import { persistTargetConnection, readTargetConnection, type TargetConnection } from "@/features/target/targetConnection";
import { buildDeployPackage, generateAdapterArtifacts, serializeDeployPackage } from "@/features/target/deployClient";
import type { TargetIoValue } from "@/features/target/targetBridgeClient";
import {
  coerceWriteValue,
  fetchTargetSession,
  indexTargetIo,
  readTargetIo,
  targetBridgeUrl,
  writeTargetIo
} from "@/features/target/targetBridgeClient";
import { diffDeployPackages, parseDeployPackageJson } from "@/features/target/deployDiff";
import {
  DEFAULT_SAFETY_POLICY,
  parseSafetyPolicyFromMapping,
  upsertSafetyPolicyInMapping,
  type SafetyPolicy
} from "@/features/target/safetyPolicy";
import {
  findAllReferences,
  goToDefinitionTarget,
  renameSymbolInSource,
  symbolAtCursor
} from "@/lib/symbolNavigation";
import { recordTrendSeries, type TrendSeries } from "@/lib/traceTrend";
import { buildCallHierarchy, flattenCallHierarchy } from "@/lib/callHierarchy";
import { installGlobalErrorHandlers } from "@/services/telemetry";
import { graphSnapshotLocal, isGraphicalLanguage } from "@/features/graph/graphCache";
import { quickFixesForDiagnostic } from "@/lib/diagnosticQuickFixes";
import type { CodeEditorHandle } from "@/features/editor/CodeEditor";
import { isTargetMappingFile, mappingFileName as defaultMappingFilePath, parseTargetMapping } from "@/features/target/targetMapping";
import { SettingsDialog } from "@/features/dialogs/SettingsDialog";
import { exportPlcopenXml, importPlcopenXml, pickPlcopenFile } from "@/features/project/plcopenIO";
import { loadSettings, parseWatchList, saveSettings } from "@/stores/settingsStore";
import {
  DEFAULT_WORKSPACE_LAYOUT,
  panelIsOpen,
  persistWorkspaceLayout,
  readWorkspaceLayout
} from "@/features/workspace/workspaceLayout";
import {
  analyzeFile,
  buildCArtifact,
  debugFile,
  engineStatusText,
  getEngineMode,
  loadGraphForFile,
  type EngineMode
} from "@/services/languageServiceBackend";
import type { GraphValidation } from "@/features/graph/validateGraph";
import type {
  Analysis,
  DebugTrace,
  Diagnostic,
  DocumentSymbol,
  GeneratedCArtifact,
  IdeSettings,
  Project,
  RunTrace,
  WorkspaceFile
} from "@/types";
import type { GraphModel } from "@/features/graph/graphTypes";
import { PaneHeader } from "@/components/layout/PaneHeader";
import { EditorSurface } from "@/app/EditorSurface";
import { BottomPanel } from "@/features/panels/BottomPanel";
import { bootstrappedApp, nowLabel, type DialogMode, type InspectorTab, type LogEntry, type OutputPanel } from "@/app/types";

export function App() {
  const initialWorkspaceLayout = useMemo(() => readWorkspaceLayout() ?? DEFAULT_WORKSPACE_LAYOUT, []);
  const [project, setProject] = useState<Project>(bootstrappedApp.project);
  const [settings, setSettings] = useState<IdeSettings>(() => loadSettings());
  const [activeFileName, setActiveFileName] = useState(bootstrappedApp.activeFileName);
  const [openTabNames, setOpenTabNames] = useState<string[]>(bootstrappedApp.openTabNames);
  const [analysis, setAnalysis] = useState<Analysis | null>(null);
  const [graphModel, setGraphModel] = useState<GraphModel | null>(null);
  const [graphValidation, setGraphValidation] = useState<GraphValidation | null>(null);
  const [historyTick, setHistoryTick] = useState(0);
  const [runTrace, setRunTrace] = useState<RunTrace | null>(null);
  const [debugTrace, setDebugTrace] = useState<DebugTrace | null>(null);
  const [cArtifact, setCArtifact] = useState<GeneratedCArtifact | null>(null);
  const [artifactTick, setArtifactTick] = useState(0);
  const [selectedArtifactId, setSelectedArtifactId] = useState<string | null>(null);
  const [activePanel, setActivePanel] = useState<OutputPanel>("Diagnostics");
  const [runState, setRunState] = useState<"idle" | "running" | "complete">("idle");
  const [leftOpen, setLeftOpen] = useState(() => panelIsOpen(initialWorkspaceLayout.explorer));
  const [rightOpen, setRightOpen] = useState(() => panelIsOpen(initialWorkspaceLayout.inspector));
  const explorerPanelRef = usePanelRef();
  const inspectorPanelRef = usePanelRef();
  const [bottomOpen, setBottomOpen] = useState(true);
  const [inspectorTab, setInspectorTab] = useState<InspectorTab>("symbols");
  const [symbolQuery, setSymbolQuery] = useState("");
  const [selectedSymbol, setSelectedSymbol] = useState<DocumentSymbol | null>(null);
  const [analyzing, setAnalyzing] = useState(false);
  const [engineMode, setEngineMode] = useState<EngineMode>("local");
  const [dialogMode, setDialogMode] = useState<DialogMode>(null);
  const [showNewFile, setShowNewFile] = useState(false);
  const [showSettings, setShowSettings] = useState(false);
  const [openableProjects, setOpenableProjects] = useState<Project[]>(() => listOpenableProjects());
  const [commandLog, setCommandLog] = useState<LogEntry[]>([
    { time: nowLabel(), message: "RoboC++ Studio ready.", kind: "info" }
  ]);
  const [targetConnection, setTargetConnection] = useState<TargetConnection>(() => readTargetConnection());
  const [targetIoValues, setTargetIoValues] = useState<TargetIoValue[]>([]);
  const [commandPaletteOpen, setCommandPaletteOpen] = useState(false);
  const [commandPaletteQuery, setCommandPaletteQuery] = useState("");
  const [findReplaceDialog, setFindReplaceDialog] = useState<{ mode: FindReplaceMode; query?: string } | null>(
    null
  );
  const [renameSymbolToken, setRenameSymbolToken] = useState<string | null>(null);
  const [logFilter, setLogFilter] = useState<"all" | LogEntry["kind"]>("all");
  const [savedProjectSnapshot, setSavedProjectSnapshot] = useState(() => projectSnapshot(bootstrappedApp.project));
  const [autosaveState, setAutosaveState] = useState<"saved" | "unsaved" | "autosaving" | "error">("saved");
  const [deployPreview, setDeployPreview] = useState<string | null>(null);
  const [deployBaseline, setDeployBaseline] = useState<string | null>(() => {
    try {
      return localStorage.getItem(`deploy-baseline:${bootstrappedApp.project.id}`);
    } catch {
      return null;
    }
  });
  const [safetyPolicy, setSafetyPolicy] = useState<SafetyPolicy>(DEFAULT_SAFETY_POLICY);
  const [breakpointsByFile, setBreakpointsByFile] = useState<Record<string, number[]>>({});
  const [trendSeries, setTrendSeries] = useState<TrendSeries[]>([]);
  const [trendRecording, setTrendRecording] = useState(false);
  const [forcedValues, setForcedValues] = useState<ForcedValue[]>(() => readForcedValues(bootstrappedApp.project.id));
  const [editorJumpLine, setEditorJumpLine] = useState<number | null>(null);
  const explorerRef = useRef<ProjectExplorerHandle | null>(null);
  const codeEditorRef = useRef<CodeEditorHandle | null>(null);
  const editHistoryRef = useRef(createEditHistory());

  const projectArtifacts = useMemo(() => {
    void artifactTick;
    return listProjectArtifacts(project.id);
  }, [artifactTick, project.id]);

  const selectedArtifact = useMemo(
    () => projectArtifacts.find((entry) => entry.id === selectedArtifactId) ?? null,
    [projectArtifacts, selectedArtifactId]
  );

  const refreshArtifacts = useCallback(() => {
    setArtifactTick((tick) => tick + 1);
  }, []);

  useEffect(() => installGlobalErrorHandlers(), []);

  useMountEffect(() => {
    const media = window.matchMedia("(max-width: 1100px)");
    const collapseIfCompact = () => {
      if (media.matches) {
        setLeftOpen(false);
        setRightOpen(false);
      }
    };
    collapseIfCompact();
    media.addEventListener("change", collapseIfCompact);
    return () => media.removeEventListener("change", collapseIfCompact);
  });

  useEffect(() => {
    const panel = explorerPanelRef.current;
    if (!panel) {
      return;
    }
    if (leftOpen && panel.isCollapsed()) {
      panel.expand();
    } else if (!leftOpen && !panel.isCollapsed()) {
      panel.collapse();
    }
  }, [leftOpen, explorerPanelRef]);

  useEffect(() => {
    const panel = inspectorPanelRef.current;
    if (!panel) {
      return;
    }
    if (rightOpen && panel.isCollapsed()) {
      panel.expand();
    } else if (!rightOpen && !panel.isCollapsed()) {
      panel.collapse();
    }
  }, [rightOpen, inspectorPanelRef]);

  const projectFileNamesKey = project.files.map((file) => file.name).join("|");
  const openTabNamesKey = openTabNames.join("\0");

  useEffect(() => {
    const fileNames = project.files.map((file) => file.name);
    const resolved = resolveEditorTabsState({ openTabNames, activeFileName }, fileNames);
    if (resolved.openTabNames.join("\0") !== openTabNamesKey) {
      setOpenTabNames(resolved.openTabNames);
    }
    if (resolved.activeFileName !== activeFileName) {
      setActiveFileName(resolved.activeFileName);
    }
  }, [projectFileNamesKey]);

  useEffect(() => {
    if (!project.id || openTabNames.length === 0) {
      return;
    }
    persistEditorTabs(project.id, { openTabNames, activeFileName });
  }, [activeFileName, openTabNames, openTabNamesKey, project.id]);

  const openTabs = useMemo(() => {
    const filesByName = new Map(project.files.map((file) => [file.name, file]));
    return openTabNames.flatMap((name) => {
      const file = filesByName.get(name);
      return file ? [file] : [];
    });
  }, [openTabNames, project.files]);

  const activeFile = useMemo(
    () => project.files.find((file) => file.name === activeFileName) ?? project.files[0],
    [project.files, activeFileName]
  );

  const localGraphSnapshot = useMemo(() => {
    if (!activeFile || !isGraphicalLanguage(activeFile.languageId)) {
      return null;
    }
    return graphSnapshotLocal(activeFile);
  }, [activeFile]);

  const displayedGraphModel = graphModel ?? localGraphSnapshot?.model ?? null;
  const displayedGraphValidation = graphValidation ?? localGraphSnapshot?.validation ?? null;

  const appendLog = useCallback((message: string, kind: LogEntry["kind"] = "action") => {
    setCommandLog((entries) => [...entries.slice(-99), { time: nowLabel(), message, kind }]);
  }, []);

  const refreshAnalysis = useCallback(
    async (file: WorkspaceFile, reason: string) => {
      setAnalyzing(true);
      appendLog(`${reason}: ${file.name}`);
      const nextAnalysis = await analyzeFile(file);
      setAnalysis(nextAnalysis);
      setAnalyzing(false);
      const errors = nextAnalysis.diagnostics.filter((d) => d.severity === "error").length;
      const warnings = nextAnalysis.diagnostics.filter((d) => d.severity === "warning").length;
      appendLog(`Check finished: ${errors} error(s), ${warnings} warning(s).`);
      return nextAnalysis;
    },
    [appendLog]
  );

  useEffect(() => {
    let cancelled = false;
    getEngineMode().then((mode) => {
      if (!cancelled) {
        setEngineMode(mode);
        appendLog(mode === "wasm" ? "WASM language service loaded." : "Using local analysis fallback.");
      }
    });
    return () => {
      cancelled = true;
    };
  }, [appendLog]);

  useEffect(() => {
    if (!activeFile) {
      return;
    }
    setRunTrace(null);
    setDebugTrace(null);
    setCArtifact(null);
    setGraphModel(null);
    setGraphValidation(null);
    setRunState("idle");
    setSelectedSymbol(null);
  }, [activeFile?.name]);

  useEffect(() => {
    if (!activeFile) {
      return;
    }
    let cancelled = false;
    setAnalyzing(true);
    const timer = window.setTimeout(() => {
      analyzeFile(activeFile).then((nextAnalysis) => {
        if (!cancelled) {
          setAnalysis(nextAnalysis);
          setAnalyzing(false);
        }
      });
    }, 180);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [activeFile?.name, activeFile?.text, activeFile?.languageId]);

  useEffect(() => {
    if (!activeFile || !isGraphicalLanguage(activeFile.languageId)) {
      return;
    }
    let cancelled = false;
    const timer = window.setTimeout(() => {
      loadGraphForFile(activeFile).then((snapshot) => {
        if (!cancelled) {
          startTransition(() => {
            setGraphModel(snapshot.model);
            setGraphValidation(snapshot.validation);
          });
        }
      });
    }, 80);
    return () => {
      cancelled = true;
      window.clearTimeout(timer);
    };
  }, [activeFile?.name, activeFile?.text, activeFile?.languageId]);

  useEffect(() => {
    if (!activeFile) {
      return;
    }
    const history = editHistoryRef.current;
    if (!history.has(activeFile.name)) {
      history.init(activeFile.name, activeFile.text);
    }
  }, [activeFile?.name, activeFile?.text]);

  const handleCheck = useCallback(async () => {
    if (!activeFile) {
      return;
    }
    setActivePanel("Diagnostics");
    setBottomOpen(true);
    const nextAnalysis = await refreshAnalysis(activeFile, "Check");
    if (nextAnalysis) {
      saveDiagnosticReport(project.id, activeFile.name, nextAnalysis);
      refreshArtifacts();
    }
  }, [activeFile, project.id, refreshAnalysis, refreshArtifacts]);

  const handleRun = useCallback(async () => {
    if (!activeFile || runState === "running") {
      return;
    }
    setRunState("running");
    setActivePanel("Scan Trace");
    setBottomOpen(true);
    appendLog(`Run: ${activeFile.name}`);
    const nextDebug = await debugFile(activeFile, settings);
    setDebugTrace(nextDebug);
    setRunTrace({
      program: nextDebug.program,
      source: nextDebug.uri,
      cycles: nextDebug.cycles.map((cycle) => ({
        cycle: cycle.cycle,
        variables: cycle.variables,
        events: cycle.events
      })),
      generatedC: cArtifact?.source ?? ""
    });
    setRunState("complete");
    saveTraceArtifact(project.id, activeFile.name, nextDebug);
    if (trendRecording) {
      setTrendSeries((current) => recordTrendSeries(current, nextDebug, parseWatchList(settings.watchVariables)));
      setActivePanel("Trends");
    }
    refreshArtifacts();
    appendLog(`Simulation complete: ${nextDebug.cycles.length} cycle(s).`);
  }, [activeFile, appendLog, cArtifact?.source, project.id, refreshArtifacts, runState, settings, trendRecording]);

  const handleBuildC = useCallback(async () => {
    const sourceFile = resolveBuildSourceFile(project, activeFile);
    if (!sourceFile) {
      appendLog("Build C: no PLC program file available.");
      return;
    }
    setActivePanel("Generated C");
    setBottomOpen(true);
    appendLog(`Build C: generating artifact for ${sourceFile.name}…`);
    const artifact = await buildCArtifact(sourceFile);
    setCArtifact(artifact);
    saveGeneratedCArtifact(project.id, sourceFile.name, artifact, sourceFile.text);
    refreshArtifacts();
    setRunTrace({
      program: sourceFile.name,
      source: sourceFile.name,
      cycles: [],
      generatedC: artifact.source
    });
    appendLog("Generated C preview opened.");
  }, [activeFile, appendLog, project, refreshArtifacts]);

  const handleFileChange = useCallback(
    (text: string) => {
      if (!activeFile) {
        return;
      }
      editHistoryRef.current.push(activeFile.name, text);
      setHistoryTick((tick) => tick + 1);
      setProject((current) => updateProjectFile(current, activeFile.name, text));
    },
    [activeFile]
  );

  const handleUndo = useCallback(() => {
    if (!activeFile) {
      return;
    }
    const previous = editHistoryRef.current.undo(activeFile.name);
    if (previous === null) {
      return;
    }
    setHistoryTick((tick) => tick + 1);
    setProject((current) => updateProjectFile(current, activeFile.name, previous));
  }, [activeFile]);

  const handleRedo = useCallback(() => {
    if (!activeFile) {
      return;
    }
    const next = editHistoryRef.current.redo(activeFile.name);
    if (next === null) {
      return;
    }
    setHistoryTick((tick) => tick + 1);
    setProject((current) => updateProjectFile(current, activeFile.name, next));
  }, [activeFile]);

  const openProject = useCallback(
    (nextProject: Project) => {
      const editorTabs = resolveEditorTabsState(
        readEditorTabs(nextProject.id),
        nextProject.files.map((file) => file.name)
      );
      setProject(nextProject);
      setActiveFileName(editorTabs.activeFileName);
      setOpenTabNames(editorTabs.openTabNames);
      setSavedProjectSnapshot(projectSnapshot(nextProject));
      setAutosaveState("saved");
      setForcedValues(readForcedValues(nextProject.id));
      setDeployPreview(null);
      setSelectedArtifactId(null);
      refreshArtifacts();
      setDialogMode(null);
      appendLog(`Opened project: ${nextProject.name}`);
    },
    [appendLog, refreshArtifacts]
  );

  const handleCreateProject = useCallback(
    (name: string, template: ProjectTemplateId) => {
      const nextProject =
        template === "empty" ? createEmptyProject(name) : createProjectFromTemplate(name, template);
      persistProject(nextProject);
      setOpenableProjects(listOpenableProjects());
      openProject(nextProject);
      appendLog(`Created project: ${name}`);
    },
    [appendLog, openProject]
  );

  const handleDeleteProject = useCallback(
    (id: string) => {
      deleteSavedProject(id);
      setOpenableProjects(listOpenableProjects());
      appendLog("Deleted saved project.");
    },
    [appendLog]
  );

  const handleCreateFile = useCallback(
    (name: string, languageId: WorkspaceFile["languageId"]) => {
      const result = addProjectFile(project, name, languageId);
      if (!result) {
        appendLog(`Could not add file: ${name}`);
        return;
      }
      setProject(result.project);
      setActiveFileName(result.file.name);
      setOpenTabNames((tabs) => addOpenTab(tabs, result.file.name));
      setShowNewFile(false);
      appendLog(`Added file: ${result.file.name}`);
    },
    [appendLog, project]
  );

  const diagnostics = analysis?.diagnostics ?? [];
  const allDiagnostics = useMemo(() => {
    const graphDiags = displayedGraphValidation?.diagnostics ?? [];
    if (graphDiags.length === 0) {
      return diagnostics;
    }
    const seen = new Set(diagnostics.map((diagnostic) => diagnostic.message));
    const merged = [...diagnostics];
    for (const diagnostic of graphDiags) {
      if (!seen.has(diagnostic.message)) {
        merged.push(diagnostic);
      }
    }
    return merged;
  }, [diagnostics, displayedGraphValidation?.diagnostics]);
  const symbols = analysis?.symbols ?? [];
  const completions = analysis?.completions ?? [];
  const statusText = engineStatusText(engineMode, analyzing);
  const errorCount = allDiagnostics.filter((d) => d.severity === "error").length;
  const warningCount = allDiagnostics.filter((d) => d.severity === "warning").length;
  const noteCount = allDiagnostics.filter((d) => d.severity === "note").length;

  const isProjectDirty = useMemo(
    () => projectSnapshot(project) !== savedProjectSnapshot,
    [project, savedProjectSnapshot]
  );

  const dirtyFiles = useMemo(
    () => computeDirtyFileNames(project, savedProjectSnapshot),
    [project, savedProjectSnapshot]
  );

  const programHash = useMemo(() => sourceTextHash(projectSnapshot(project)), [project]);

  useEffect(() => {
    if (!isProjectDirty || project.builtIn) {
      return;
    }
    setAutosaveState("unsaved");
    const timer = window.setTimeout(() => {
      setAutosaveState("autosaving");
      try {
        persistProject(project);
        setSavedProjectSnapshot(projectSnapshot(project));
        setAutosaveState("saved");
      } catch {
        setAutosaveState("error");
      }
    }, 3000);
    return () => window.clearTimeout(timer);
  }, [isProjectDirty, project]);

  useEffect(() => {
    setTargetConnection((current) => {
      const editorMatchesTarget = !isProjectDirty && current.deployHash !== null;
      if (current.programHash === programHash && current.editorMatchesTarget === editorMatchesTarget) {
        return current;
      }
      const next = { ...current, programHash, editorMatchesTarget };
      persistTargetConnection(next);
      return next;
    });
  }, [isProjectDirty, programHash]);

  const mappingSymbolSuggestions = useMemo(() => {
    const fromMetadata = [
      ...(cArtifact?.metadata?.ioSymbols.map((symbol) => symbol.name) ?? []),
      ...(cArtifact?.metadata?.stateLayout.map((field) => field.name) ?? [])
    ];
    const fromSymbols = symbols.filter((symbol) => symbol.kind === "variable").map((symbol) => symbol.name);
    return Array.from(new Set([...fromMetadata, ...fromSymbols]));
  }, [cArtifact?.metadata, symbols]);

  const activeDiagnostics = useMemo(
    () =>
      allDiagnostics.filter(
        (diagnostic) =>
          !diagnostic.span ||
          diagnostic.span.source === activeFile?.name ||
          diagnostic.span.source.endsWith(`/${activeFile?.name ?? ""}`)
      ),
    [activeFile?.name, allDiagnostics]
  );

  const jumpToDiagnostic = useCallback((diagnostic: Diagnostic) => {
    if (!diagnostic.span) {
      return;
    }
    setEditorJumpLine(diagnostic.span.line);
    codeEditorRef.current?.scrollToLine(diagnostic.span.line);
  }, []);

  useEffect(() => {
    if (editorJumpLine === null) {
      return;
    }
    codeEditorRef.current?.scrollToLine(editorJumpLine);
  }, [editorJumpLine, activeFile?.name]);

  const handleSave = useCallback(() => {
    persistProject(project);
    setSavedProjectSnapshot(projectSnapshot(project));
    appendLog(`Saved project: ${project.name}`);
  }, [appendLog, project]);

  const programNames = useMemo(
    () => symbols.filter((symbol) => symbol.kind === "program").map((symbol) => symbol.name),
    [symbols]
  );

  const handleImport = useCallback(async () => {
    const picked = await pickPlcopenFile();
    if (!picked) {
      return;
    }
    const nextProject = importPlcopenXml(project, picked.name, picked.text);
    if (!nextProject) {
      appendLog(`Could not import PLCopen file: ${picked.name}`);
      return;
    }
    setProject(nextProject);
    const imported = nextProject.files.find((file) => file.name.endsWith(".xml"));
    if (imported) {
      setActiveFileName(imported.name);
      setOpenTabNames((tabs) => addOpenTab(tabs, imported.name));
    }
    appendLog(`Imported PLCopen XML: ${picked.name}`);
  }, [appendLog, project]);

  const handleExport = useCallback(() => {
    if (!activeFile) {
      return;
    }
    const xmlFile =
      activeFile.languageId === "xml"
        ? activeFile
        : project.files.find((file) => file.languageId === "xml");
    if (!xmlFile) {
      appendLog("No PLCopen XML file found in the project.");
      return;
    }
    exportPlcopenXml(xmlFile, displayedGraphModel);
    savePlcopenExportArtifact(project.id, xmlFile.name, xmlFile.text);
    refreshArtifacts();
    appendLog(`Exported PLCopen XML: ${xmlFile.name}`);
  }, [activeFile, appendLog, displayedGraphModel, project.files, project.id, refreshArtifacts]);

  const handleDeploy = useCallback(() => {
    const { package: deployPackage, issues } = buildDeployPackage(project, cArtifact?.metadata ?? null);
    if (issues.some((issue) => issue.severity === "error")) {
      appendLog(`Deploy blocked: ${issues.map((issue) => issue.message).join(" ")}`);
      setInspectorTab("target");
      return;
    }
    const serialized = serializeDeployPackage(deployPackage);
    saveProjectArtifact(project.id, {
      kind: "deploy-package",
      name: `${project.name}.deploy.json`,
      sourceFile: activeFile?.name ?? "workspace",
      content: serialized,
      mimeType: "application/json"
    });
    refreshArtifacts();
    downloadArtifact({
      id: "download",
      kind: "deploy-package",
      name: `${project.name}.deploy.json`,
      sourceFile: activeFile?.name ?? "workspace",
      content: serialized,
      mimeType: "application/json",
      createdAt: new Date().toISOString()
    });
    appendLog(`Deploy package prepared for ${project.name}.`);
  }, [activeFile?.name, appendLog, cArtifact?.metadata, project, refreshArtifacts]);

  const handleCompliance = useCallback(() => {
    setActivePanel("Diagnostics");
    setBottomOpen(true);
    const complianceNotes = diagnostics.filter(
      (diagnostic) => diagnostic.code === "compliance" || diagnostic.stableCode.includes("COMPLIANCE")
    );
    if (complianceNotes.length > 0) {
      appendLog(`Compliance: ${complianceNotes[0]?.message ?? "profile report available."}`);
    } else {
      appendLog("Compliance: active document has no open compliance findings.");
    }
  }, [appendLog, diagnostics]);

  const handleRenameFile = useCallback(
    (oldName: string, newName: string) => {
      const result = renameProjectFile(project, oldName, newName);
      if (!result) {
        appendLog(`Could not rename file: ${oldName}`);
        return;
      }
      setProject(result.project);
      if (activeFileName === oldName) {
        setActiveFileName(result.fileName);
      }
      setOpenTabNames((tabs) => renameOpenTab(tabs, oldName, result.fileName));
      appendLog(`Renamed file: ${oldName} → ${result.fileName}`);
    },
    [activeFileName, appendLog, project]
  );

  const handleDeleteFile = useCallback(
    (fileName: string) => {
      const nextProject = removeProjectFile(project, fileName);
      if (!nextProject) {
        appendLog("Cannot delete the last project file.");
        return;
      }
      setProject(nextProject);
      setOpenTabNames((tabs) => {
        const nextTabs = removeOpenTab(tabs, fileName);
        if (activeFileName === fileName) {
          setActiveFileName(activeFileAfterClose(tabs, fileName, activeFileName));
        }
        return syncOpenTabsWithProject(nextTabs, nextProject.files.map((file) => file.name));
      });
      appendLog(`Deleted file: ${fileName}`);
    },
    [activeFileName, appendLog, project]
  );

  const handleReorderFile = useCallback(
    (fileName: string, beforeFileName: string) => {
      setProject((current) => reorderProjectFile(current, fileName, beforeFileName));
      appendLog(`Reordered ${fileName} before ${beforeFileName}.`);
    },
    [appendLog]
  );

  const selectFile = useCallback((fileName: string) => {
    setSelectedArtifactId(null);
    setOpenTabNames((tabs) => addOpenTab(tabs, fileName));
    setActiveFileName(fileName);
    if (isTargetMappingFile(fileName)) {
      setInspectorTab("target");
      setRightOpen(true);
    }
  }, []);

  const closeTab = useCallback(
    (fileName: string) => {
      setOpenTabNames((tabs) => {
        if (tabs.length <= 1 || !tabs.includes(fileName)) {
          return tabs;
        }
        const nextTabs = removeOpenTab(tabs, fileName);
        setActiveFileName((current) => activeFileAfterClose(tabs, fileName, current));
        return nextTabs;
      });
    },
    []
  );

  const handleSelectArtifact = useCallback(
    (artifactId: string | null) => {
      setSelectedArtifactId(artifactId);
      if (!artifactId) {
        return;
      }
      setBottomOpen(true);
      const artifact = listProjectArtifacts(project.id).find((entry) => entry.id === artifactId);
      if (!artifact) {
        return;
      }
      if (artifact.kind === "trace-export") {
        try {
          const parsed = JSON.parse(artifact.content) as DebugTrace;
          setDebugTrace(parsed);
          setRunTrace({
            program: parsed.program,
            source: parsed.uri,
            cycles: parsed.cycles.map((cycle) => ({
              cycle: cycle.cycle,
              variables: cycle.variables,
              events: cycle.events
            })),
            generatedC: cArtifact?.source ?? ""
          });
          setActivePanel("Scan Trace");
        } catch {
          setActivePanel("Artifacts");
        }
      } else {
        setActivePanel("Artifacts");
      }
      appendLog(`Opened artifact: ${artifact.name}`);
    },
    [appendLog, cArtifact?.source, project.id]
  );

  const handleSaveSettings = useCallback(
    (nextSettings: IdeSettings) => {
      setSettings(nextSettings);
      saveSettings(nextSettings);
      appendLog("Settings saved.");
    },
    [appendLog]
  );

  const addWatch = useCallback(
    (name: string) => {
      const current = parseWatchList(settings.watchVariables);
      if (current.includes(name)) {
        return;
      }
      handleSaveSettings({ ...settings, watchVariables: [...current, name].join(", ") });
    },
    [handleSaveSettings, settings]
  );

  const removeWatch = useCallback(
    (name: string) => {
      const next = parseWatchList(settings.watchVariables)
        .filter((entry) => entry !== name)
        .join(", ");
      handleSaveSettings({ ...settings, watchVariables: next });
    },
    [handleSaveSettings, settings]
  );

  const forceWatchValue = useCallback(
    async (name: string, value: string, persistent: boolean) => {
      const hardwareOnline =
        targetConnection.kind === "hardware" &&
        targetConnection.state !== "offline" &&
        targetConnection.state !== "error" &&
        targetConnection.state !== "connecting";
      const mappingTextForWatch =
        project.files.find((file) => isTargetMappingFile(file.name))?.text ?? "";
      const mapped = parseTargetMapping(mappingTextForWatch).entries.some(
        (entry) => entry.symbol.toLowerCase() === name.toLowerCase()
      );
      if (hardwareOnline && mapped) {
        try {
          const bridge = targetBridgeUrl(settings.targetBridgeUrl);
          const current = targetIoValues.find(
            (entry) => entry.symbol.toUpperCase() === name.toUpperCase()
          )?.value;
          await writeTargetIo(bridge, name, coerceWriteValue(value, current));
          const values = await readTargetIo(bridge);
          setTargetIoValues(values);
          appendLog(`${persistent ? "Forced" : "Wrote"} ${name} = ${value} on target`);
          if (persistent) {
            const next = upsertForcedValue(project.id, { name, preparedValue: value, persistent });
            setForcedValues(next);
          }
        } catch (error) {
          appendLog(
            `Target write failed for ${name}: ${error instanceof Error ? error.message : "unknown error"}`,
            "error"
          );
        }
        return;
      }
      const next = upsertForcedValue(project.id, { name, preparedValue: value, persistent });
      setForcedValues(next);
      appendLog(`${persistent ? "Forced" : "Wrote"} ${name} = ${value}`);
    },
    [
      appendLog,
      project.files,
      project.id,
      settings.targetBridgeUrl,
      targetConnection.kind,
      targetConnection.state,
      targetIoValues
    ]
  );

  const unforceWatchValue = useCallback(
    (name: string) => {
      const next = removeForcedValue(project.id, name);
      setForcedValues(next);
      appendLog(`Unforced ${name}`);
    },
    [appendLog, project.id]
  );

  const handlePreviewDeploy = useCallback(() => {
    const { package: deployPackage } = buildDeployPackage(project, cArtifact?.metadata ?? null);
    setDeployPreview(serializeDeployPackage(deployPackage));
    appendLog("Deploy package preview updated.");
  }, [appendLog, cArtifact?.metadata, project]);

  const handleRevalidateDeploy = useCallback(() => {
    const { package: deployPackage, issues } = buildDeployPackage(project, cArtifact?.metadata ?? null);
    setDeployPreview(serializeDeployPackage(deployPackage));
    if (issues.length > 0) {
      appendLog(`Deploy revalidation found ${issues.length} issue(s).`, "error");
      return;
    }
    appendLog("Deploy package revalidated successfully.");
  }, [appendLog, cArtifact?.metadata, project]);

  const handleSaveDeployBaseline = useCallback(() => {
    if (!deployPreview) {
      return;
    }
    try {
      localStorage.setItem(`deploy-baseline:${project.id}`, deployPreview);
      setDeployBaseline(deployPreview);
      appendLog("Saved deploy package baseline for diff review.");
    } catch {
      appendLog("Could not save deploy baseline.", "error");
    }
  }, [appendLog, deployPreview, project.id]);

  const handleSaveSafetyPolicy = useCallback(() => {
    const mappingFile = project.files.find((file) => isTargetMappingFile(file.name));
    if (!mappingFile) {
      appendLog("Create a target mapping file before saving safety policy.", "info");
      return;
    }
    const nextText = upsertSafetyPolicyInMapping(mappingFile.text, safetyPolicy);
    setProject((current) => updateProjectFile(current, mappingFile.name, nextText));
    appendLog("Saved safety policy to target mapping.");
  }, [appendLog, project.files, safetyPolicy]);

  const handleGoToDefinition = useCallback(() => {
    if (!activeFile) {
      return;
    }
    const offset = codeEditorRef.current?.getCursorOffset() ?? 0;
    const target = goToDefinitionTarget(symbols, activeFile.text, offset);
    if (!target) {
      appendLog("No definition found for symbol at cursor.", "info");
      return;
    }
    setEditorJumpLine(target.line);
    codeEditorRef.current?.goToOffset(target.offset);
    appendLog(`Go to definition: line ${target.line}`);
  }, [activeFile, appendLog, symbols]);

  const handleFindReferences = useCallback(() => {
    if (!activeFile) {
      return;
    }
    const offset = codeEditorRef.current?.getCursorOffset() ?? 0;
    const symbol = symbolAtCursor(symbols, activeFile.text, offset);
    const token = symbol?.name ?? activeFile.text.slice(offset).match(/[A-Za-z_][A-Za-z0-9_]*/)?.[0];
    if (!token) {
      appendLog("No symbol at cursor for find references.", "info");
      return;
    }
    const references = findAllReferences(activeFile.text, token);
    if (references.length === 0) {
      appendLog(`No references found for ${token}.`, "info");
      return;
    }
    setActivePanel("Diagnostics");
    setBottomOpen(true);
    appendLog(`Found ${references.length} reference(s) for ${token}. Jumping to first.`);
    const first = references[0];
    if (first) {
      setEditorJumpLine(first.line);
      codeEditorRef.current?.goToOffset(first.offset);
    }
  }, [activeFile, appendLog, symbols]);

  const openFindReplaceDialog = useCallback((mode: FindReplaceMode, query = "") => {
    if (!activeFile) {
      appendLog("Open a file before searching.", "info");
      return;
    }
    setFindReplaceDialog({ mode, query });
  }, [activeFile, appendLog]);

  const handleRenameSymbol = useCallback(() => {
    if (!activeFile) {
      return;
    }
    const offset = codeEditorRef.current?.getCursorOffset() ?? 0;
    const symbol = symbolAtCursor(symbols, activeFile.text, offset);
    const token = symbol?.name ?? activeFile.text.slice(offset).match(/[A-Za-z_][A-Za-z0-9_]*/)?.[0];
    if (!token) {
      appendLog("No symbol at cursor to rename.", "info");
      return;
    }
    setRenameSymbolToken(token);
  }, [activeFile, appendLog, symbols]);

  const submitRenameSymbol = useCallback(
    (nextName: string) => {
      if (!activeFile || !renameSymbolToken) {
        return;
      }
      const nextText = renameSymbolInSource(activeFile.text, renameSymbolToken, nextName);
      if (!nextText) {
        appendLog(`Could not rename ${renameSymbolToken}.`, "error");
        return;
      }
      setProject((current) => updateProjectFile(current, activeFile.name, nextText));
      appendLog(`Renamed ${renameSymbolToken} to ${nextName} in ${activeFile.name}.`);
      setRenameSymbolToken(null);
    },
    [activeFile, appendLog, renameSymbolToken]
  );

  const toggleBreakpoint = useCallback(
    (line: number) => {
      if (!activeFile) {
        return;
      }
      setBreakpointsByFile((current) => {
        const existing = new Set(current[activeFile.name] ?? []);
        if (existing.has(line)) {
          existing.delete(line);
        } else {
          existing.add(line);
        }
        return { ...current, [activeFile.name]: [...existing].sort((a, b) => a - b) };
      });
    },
    [activeFile]
  );

  const handleRenameArtifact = useCallback(
    (artifactId: string, nextName: string) => {
      const updated = renameProjectArtifact(project.id, artifactId, nextName);
      if (!updated) {
        appendLog("Could not rename artifact.", "error");
        return;
      }
      refreshArtifacts();
      appendLog(`Renamed artifact to ${updated.name}`);
    },
    [appendLog, project.id, refreshArtifacts]
  );

  const handleExportTrends = useCallback(() => {
    const blob = new Blob([JSON.stringify(trendSeries, null, 2)], { type: "application/json" });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = `${project.name}.trends.json`;
    anchor.click();
    URL.revokeObjectURL(url);
    appendLog("Exported trend series.");
  }, [appendLog, project.name, trendSeries]);

  const handleApplyQuickFix = useCallback(
    (diagnostic: Diagnostic, fixId: string) => {
      if (!activeFile) {
        return;
      }
      const fix = quickFixesForDiagnostic(diagnostic).find((entry) => entry.id === fixId);
      if (!fix) {
        return;
      }
      const next = fix.apply(activeFile.text);
      if (!next) {
        appendLog("Quick fix could not be applied.", "info");
        return;
      }
      setProject((current) => updateProjectFile(current, activeFile.name, next));
      appendLog(`Applied quick fix: ${fix.label}`);
    },
    [activeFile, appendLog]
  );

  const handleSaveSnapshot = useCallback(() => {
    const snapshot = saveProjectSnapshot(project);
    appendLog(`Saved backup snapshot: ${snapshot.name}`);
  }, [appendLog, project]);

  const handleCreateMapping = useCallback(() => {
    const name = defaultMappingFilePath();
    if (project.files.some((file) => file.name === name)) {
      selectFile(name);
      return;
    }
    const result = addProjectFile(project, name, "mapping");
    if (!result) {
      appendLog("Could not create target mapping file.", "error");
      return;
    }
    setProject(result.project);
    selectFile(result.file.name);
    appendLog(`Created ${result.file.name}`);
  }, [appendLog, project, selectFile]);

  const handleImportBundle = useCallback(async () => {
    const imported = await pickProjectBundle();
    if (!imported) {
      appendLog("Project import cancelled.", "info");
      return;
    }
    openProject({ ...imported, id: imported.id || crypto.randomUUID(), updatedAt: new Date().toISOString() });
    appendLog(`Imported project bundle: ${imported.name}`);
  }, [appendLog, openProject]);

  const commandPaletteItems = useMemo<CommandPaletteItem[]>(() => {
    const fileItems = buildFilePaletteItems(project.files, selectFile);
    const symbolItems: CommandPaletteItem[] = symbols.slice(0, 80).map((symbol) => ({
      id: `symbol:${symbol.kind}:${symbol.name}`,
      label: symbol.name,
      detail: symbol.detail,
      group: "Navigation" as const,
      run: () => {
        selectSymbol(symbol);
        setRightOpen(true);
        setInspectorTab("symbols");
      }
    }));
    const commands: CommandPaletteItem[] = [
      { id: "cmd:palette", label: "Show command palette", group: "Commands", shortcut: "Cmd+Shift+P", run: () => setCommandPaletteOpen(true) },
      {
        id: "cmd:find-in-file",
        label: "Find in active file",
        group: "Commands",
        shortcut: "Cmd+F",
        run: () => openFindReplaceDialog("find")
      },
      {
        id: "cmd:replace-in-file",
        label: "Replace in active file",
        group: "Commands",
        shortcut: "Cmd+Shift+F",
        run: () => openFindReplaceDialog("replace")
      },
      {
        id: "cmd:go-to-definition",
        label: "Go to definition",
        group: "Commands",
        shortcut: "F12",
        run: handleGoToDefinition
      },
      {
        id: "cmd:find-references",
        label: "Find all references",
        group: "Commands",
        shortcut: "Shift+F12",
        run: handleFindReferences
      },
      {
        id: "cmd:rename-symbol",
        label: "Rename symbol",
        group: "Commands",
        shortcut: "F2",
        run: handleRenameSymbol
      },
      ...flattenCallHierarchy(buildCallHierarchy(symbols))
        .slice(0, 40)
        .map((symbol) => ({
          id: `hierarchy:${symbol.kind}:${symbol.name}`,
          label: `Call hierarchy: ${symbol.name}`,
          detail: symbol.detail,
          group: "Navigation" as const,
          run: () => {
            selectSymbol(symbol);
            setRightOpen(true);
            setInspectorTab("hover");
            if (symbol.range) {
              setEditorJumpLine(symbol.range.startPosition.line + 1);
              codeEditorRef.current?.goToOffset(symbol.range.start);
            }
          }
        })),
      {
        id: "cmd:add-watch-selection",
        label: "Add selection to watches",
        group: "Commands",
        run: () => {
          const selected = codeEditorRef.current?.getSelectedText();
          if (!selected) {
            appendLog("Select an identifier in the editor first.", "info");
            return;
          }
          addWatch(selected);
        }
      },
      { id: "cmd:check", label: "Check project", group: "Commands", shortcut: "F7", run: () => void handleCheck() },
      { id: "cmd:run", label: "Run simulation", group: "Commands", shortcut: "F5", run: () => void handleRun() },
      { id: "cmd:build-c", label: "Build C", group: "Commands", run: () => void handleBuildC() },
      { id: "cmd:save", label: "Save project", group: "Commands", shortcut: "Cmd+S", run: handleSave },
      { id: "cmd:save-snapshot", label: "Save project backup snapshot", group: "Commands", run: handleSaveSnapshot },
      { id: "cmd:settings", label: "Open settings", group: "Commands", run: () => setShowSettings(true) },
      { id: "cmd:export-bundle", label: "Export project bundle", group: "Commands", run: () => exportProjectBundle(project) },
      { id: "cmd:import-bundle", label: "Import project bundle", group: "Commands", run: () => void handleImportBundle() },
      {
        id: "nav:diagnostics",
        label: "Show diagnostics panel",
        group: "Navigation",
        run: () => {
          setActivePanel("Diagnostics");
          setBottomOpen(true);
        }
      },
      {
        id: "nav:watches",
        label: "Show watches panel",
        group: "Navigation",
        run: () => {
          setActivePanel("Watches");
          setBottomOpen(true);
        }
      },
      {
        id: "nav:artifacts",
        label: "Show artifacts panel",
        group: "Navigation",
        run: () => {
          setActivePanel("Artifacts");
          setBottomOpen(true);
        }
      },
      ...LAYOUT_PRESETS.map((preset) => ({
        id: `layout:${preset.id}`,
        label: `Layout: ${preset.label}`,
        detail: preset.description,
        group: "Navigation" as const,
        run: () => {
          persistWorkspaceLayout(preset.layout);
          setLeftOpen(preset.leftOpen);
          setRightOpen(preset.rightOpen);
          setBottomOpen(preset.bottomOpen);
        }
      }))
    ];
    return [...fileItems, ...symbolItems, ...commands];
  }, [
    activeFile?.name,
    addWatch,
    appendLog,
    handleBuildC,
    handleCheck,
    handleFindReferences,
    handleGoToDefinition,
    handleImportBundle,
    handleRenameSymbol,
    handleRun,
    handleSave,
    handleSaveSnapshot,
    openFindReplaceDialog,
    project,
    selectFile,
    symbols
  ]);

  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "F5") {
        event.preventDefault();
        void handleRun();
      }
      if (event.key === "F7" && !event.shiftKey) {
        event.preventDefault();
        void handleCheck();
      }
      if (event.key === "F7" && event.shiftKey) {
        event.preventDefault();
        handleCompliance();
      }
      if ((event.metaKey || event.ctrlKey) && event.key === "s") {
        event.preventDefault();
        handleSave();
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "f") {
        event.preventDefault();
        openFindReplaceDialog(event.shiftKey ? "replace" : "find");
      }
      if ((event.metaKey || event.ctrlKey) && event.shiftKey && event.key.toLowerCase() === "p") {
        event.preventDefault();
        setCommandPaletteOpen(true);
      }
      if (event.key === "F12" && !event.shiftKey) {
        event.preventDefault();
        handleGoToDefinition();
      }
      if (event.key === "F12" && event.shiftKey) {
        event.preventDefault();
        handleFindReferences();
      }
      if (event.key === "F2") {
        event.preventDefault();
        handleRenameSymbol();
      }
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [
    activeFile?.name,
    appendLog,
    handleCheck,
    handleCompliance,
    handleFindReferences,
    handleGoToDefinition,
    handleRenameSymbol,
    handleRun,
    handleSave,
    openFindReplaceDialog
  ]);

  const filteredSymbols = useMemo(() => {
    const query = symbolQuery.trim().toLowerCase();
    if (!query) {
      return symbols;
    }
    return symbols.filter(
      (symbol) =>
        symbol.name.toLowerCase().includes(query) ||
        symbol.detail.toLowerCase().includes(query) ||
        (symbol.containerName?.toLowerCase().includes(query) ?? false)
    );
  }, [symbols, symbolQuery]);

  const hoverSymbol = selectedSymbol ?? symbols[0] ?? null;
  const explorerKey = `${project.id}:${treePathsSignature(project.files)}:${projectArtifacts.length}`;
  const canUndo = activeFile ? editHistoryRef.current.canUndo(activeFile.name) : false;
  const canRedo = activeFile ? editHistoryRef.current.canRedo(activeFile.name) : false;
  void historyTick;
  const deployIssues = useMemo(
    () => validateTargetDeployment(project, cArtifact?.metadata ?? null),
    [project, cArtifact?.metadata]
  );
  const mappingEntries = useMemo(() => {
    const mappingFile = project.files.find((file) => isTargetMappingFile(file.name));
    return parseTargetMapping(mappingFile?.text ?? "").entries;
  }, [project.files]);

  const staleMappedSymbols = useMemo(() => {
    if (!cArtifact?.metadata) {
      return new Set<string>();
    }
    const ioNames = new Set(cArtifact.metadata.ioSymbols.map((symbol) => symbol.name.toLowerCase()));
    const stateNames = new Set(cArtifact.metadata.stateLayout.map((field) => field.name.toLowerCase()));
    return new Set(
      mappingEntries
        .filter((entry) => !ioNames.has(entry.symbol.toLowerCase()) && !stateNames.has(entry.symbol.toLowerCase()))
        .map((entry) => entry.symbol.toLowerCase())
    );
  }, [cArtifact?.metadata, mappingEntries]);
  const mappingFileName =
    project.files.find((file) => isTargetMappingFile(file.name))?.name ?? "target/mapping.toml";
  const mappingFile = project.files.find((file) => isTargetMappingFile(file.name));
  const mappingText = mappingFile?.text ?? "";
  const hardwareOnline =
    targetConnection.kind === "hardware" &&
    targetConnection.state !== "offline" &&
    targetConnection.state !== "error" &&
    targetConnection.state !== "connecting";
  const targetIoBySymbol = useMemo(() => indexTargetIo(targetIoValues), [targetIoValues]);
  const serializedDeployPackage = useMemo(() => {
    const { package: deployPackage } = buildDeployPackage(project, cArtifact?.metadata ?? null);
    return serializeDeployPackage(deployPackage);
  }, [project, cArtifact?.metadata]);
  const deployDiff = useMemo(() => {
    const { package: currentPackage } = buildDeployPackage(project, cArtifact?.metadata ?? null);
    const baseline = deployBaseline ? parseDeployPackageJson(deployBaseline) : null;
    return diffDeployPackages(baseline, currentPackage);
  }, [cArtifact?.metadata, deployBaseline, project]);
  const adapterArtifacts = useMemo(
    () => generateAdapterArtifacts(cArtifact?.metadata ?? null),
    [cArtifact?.metadata]
  );
  const activeBreakpoints = useMemo(
    () => new Set(breakpointsByFile[activeFile?.name ?? ""] ?? []),
    [activeFile?.name, breakpointsByFile]
  );
  useEffect(() => {
    if (mappingFile) {
      setSafetyPolicy(parseSafetyPolicyFromMapping(mappingFile.text));
    }
  }, [mappingFile?.text]);

  useMountEffect(() => {
    if (targetConnection.kind !== "hardware") {
      return;
    }
    void (async () => {
      try {
        const bridge = targetBridgeUrl(settings.targetBridgeUrl);
        const session = await fetchTargetSession(bridge);
        if (!session.ok) {
          return;
        }
        setTargetConnection((current) => ({
          ...current,
          state:
            session.running || session.state === "running"
              ? "running"
              : session.state === "stopped"
                ? "stopped"
                : "online",
          runtimeVersion: session.runtimeVersion,
          programHash: session.programHash ?? current.programHash,
          deployHash: session.deployHash,
          editorMatchesTarget: session.editorMatchesTarget,
          lastError: null
        }));
        const values = await readTargetIo(bridge);
        setTargetIoValues(values);
      } catch {
        // Bridge not running; keep offline state.
      }
    })();
  });

  const buildSourceFile = useMemo(
    () => resolveBuildSourceFile(project, activeFile),
    [project, activeFile]
  );

  const workspaceClass = ["workspace", !leftOpen ? "no-left" : "", !rightOpen ? "no-right" : ""]
    .filter(Boolean)
    .join(" ");

  const handleWorkspaceLayoutChanged = useCallback((layout: Record<string, number>) => {
    persistWorkspaceLayout(layout);
    const explorerOpen = panelIsOpen(layout.explorer);
    const inspectorOpen = panelIsOpen(layout.inspector);
    setLeftOpen(explorerOpen);
    setRightOpen(inspectorOpen);
  }, []);

  const selectSymbol = (symbol: DocumentSymbol) => {
    setSelectedSymbol(symbol);
    setInspectorTab("hover");
  };

  if (!activeFile) {
    return null;
  }

  return (
    <>
      <NarrowViewportGate />
      <main className="studio-shell">
      <header className="topbar">
        <div className="brand-block">
          <span className="brand-text" translate="no">
            RoboC++ Studio
          </span>
          <nav className="breadcrumb" aria-label="Project path">
            <span>{project.name}</span>
            <span className="breadcrumb-sep" aria-hidden="true">
              /
            </span>
            <span className="breadcrumb-current">{activeFile.name}</span>
          </nav>
        </div>
        <TargetConnectionBar
          connection={targetConnection}
          onChange={setTargetConnection}
          runState={runState}
          bridgeUrl={settings.targetBridgeUrl}
          modbusPort={settings.targetModbusPort}
          projectId={project.id}
          mappingText={mappingText}
          workspaceRoot={settings.targetWorkspaceRoot || undefined}
          programHash={programHash}
          generatedC={cArtifact?.source ?? null}
          deployPackage={serializedDeployPackage}
          onIoSnapshot={setTargetIoValues}
        />
      </header>

      <CommandBar
        runState={runState}
        onNewProject={() => setDialogMode("new")}
        onOpenProject={() => {
          setOpenableProjects(listOpenableProjects());
          setDialogMode("open");
        }}
        onSave={handleSave}
        onExportBundle={() => exportProjectBundle(project)}
        onImportBundle={() => void handleImportBundle()}
        onNewFile={() => setShowNewFile(true)}
        onCheck={() => void handleCheck()}
        onRun={() => void handleRun()}
        onBuildC={() => void handleBuildC()}
        onImportPlcopen={() => void handleImport()}
        onExportPlcopen={handleExport}
        onDeploy={handleDeploy}
        onSettings={() => setShowSettings(true)}
      />

      <ResizablePanelGroup
        id="studio-workspace"
        orientation="horizontal"
        className={workspaceClass}
        defaultLayout={initialWorkspaceLayout}
        onLayoutChanged={handleWorkspaceLayoutChanged}
      >
        <ResizablePanel
          id="explorer"
          panelRef={explorerPanelRef}
          defaultSize="14%"
          minSize={152}
          maxSize="42%"
          collapsible
          collapsedSize={0}
          className="project-pane"
          aria-label="Project files"
        >
          <PaneHeader
            title="Explorer"
            count={String(project.files.length + projectArtifacts.length)}
            onClose={() => setLeftOpen(false)}
            closeLabel="Hide explorer"
            action={
              <button
                type="button"
                className="pane-header-btn"
                aria-label="New file"
                onClick={() => setShowNewFile(true)}
              >
                +
              </button>
            }
          />
          <div className="pane-body project-explorer-body">
            <ProjectExplorer
              key={explorerKey}
              ref={explorerRef}
              project={project}
              artifacts={projectArtifacts}
              activeFileName={activeFileName}
              onSelectFile={selectFile}
              onSelectArtifact={handleSelectArtifact}
              onRenameFile={handleRenameFile}
              onDeleteFile={handleDeleteFile}
              onReorderFile={handleReorderFile}
              onNewFile={() => setShowNewFile(true)}
            />
          </div>
          <div className="project-meta-grid">
            <div className="project-meta">
              <span>Profile</span>
              <strong>{settings.compilerProfile}</strong>
            </div>
            <div className="project-meta">
              <span>Engine</span>
              <strong>{statusText}</strong>
            </div>
          </div>
        </ResizablePanel>

        <ResizableHandle withHandle className="workspace-resize-handle" />

        <ResizablePanel id="editor" minSize="35%" className="editor-column">
          <EditorTabBar
            openTabs={openTabs}
            activeFileName={activeFile.name}
            dirtyFileNames={dirtyFiles}
            leftOpen={leftOpen}
            rightOpen={rightOpen}
            onSelectFile={selectFile}
            onCloseTab={closeTab}
            onOpenLeft={() => setLeftOpen(true)}
            onOpenRight={() => setRightOpen(true)}
          />
          <div
            id={tabPanelId(activeFile.name)}
            role="tabpanel"
            aria-labelledby={`tab-${tabPanelId(activeFile.name)}`}
            className="editor-tab-panel"
          >
            <EditorSurface
              ref={codeEditorRef}
              file={activeFile}
              graphModel={displayedGraphModel}
              graphValidation={displayedGraphValidation}
              runTrace={runTrace}
              debugTrace={debugTrace}
              diagnostics={activeDiagnostics}
              completions={completions}
              symbols={symbols}
              mappingSymbolSuggestions={mappingSymbolSuggestions}
              targetBindings={mappingEntries}
              currentLine={editorJumpLine}
              breakpoints={activeBreakpoints}
              onToggleBreakpoint={toggleBreakpoint}
              onDiagnosticClick={jumpToDiagnostic}
              onAddWatch={addWatch}
              canUndo={canUndo}
              canRedo={canRedo}
              onChange={handleFileChange}
              onUndo={handleUndo}
              onRedo={handleRedo}
            />
          </div>
          <BottomPanel
            activePanel={activePanel}
            setActivePanel={setActivePanel}
            diagnostics={allDiagnostics}
            errorCount={errorCount}
            warningCount={warningCount}
            noteCount={noteCount}
            debugTrace={debugTrace}
            cArtifact={cArtifact}
            generatedCOutputPath={settings.generatedCOutputPath}
            commandLog={commandLog}
            logFilter={logFilter}
            onLogFilterChange={setLogFilter}
            watchVariables={settings.watchVariables}
            symbols={symbols}
            forcedValues={forcedValues}
            liveIoBySymbol={targetIoBySymbol}
            hardwareOnline={hardwareOnline}
            onAddWatch={addWatch}
            onRemoveWatch={removeWatch}
            onForceWatchValue={forceWatchValue}
            onUnforceWatchValue={unforceWatchValue}
            onApplyQuickFix={handleApplyQuickFix}
            project={project}
            artifacts={projectArtifacts}
            selectedArtifact={selectedArtifact}
            onSelectArtifact={handleSelectArtifact}
            onDeleteArtifact={(artifactId) => {
              removeProjectArtifact(project.id, artifactId);
              refreshArtifacts();
              if (selectedArtifactId === artifactId) {
                setSelectedArtifactId(null);
              }
            }}
            onClearArtifacts={() => {
              clearProjectArtifacts(project.id);
              refreshArtifacts();
              setSelectedArtifactId(null);
            }}
            onRevealSource={selectFile}
            onRenameArtifact={handleRenameArtifact}
            trendSeries={trendSeries}
            trendRecording={trendRecording}
            onToggleTrendRecording={() => setTrendRecording((value) => !value)}
            onClearTrends={() => setTrendSeries([])}
            onExportTrends={handleExportTrends}
            onJumpToDiagnostic={jumpToDiagnostic}
            onExportTrace={
              debugTrace
                ? () => {
                    if (!activeFile) {
                      return;
                    }
                    const artifact = saveTraceArtifact(project.id, activeFile.name, debugTrace);
                    refreshArtifacts();
                    downloadArtifact(artifact);
                    appendLog(`Exported trace: ${artifact.name}`);
                  }
                : undefined
            }
            open={bottomOpen}
            onToggle={() => setBottomOpen((value) => !value)}
          />
        </ResizablePanel>

        <ResizableHandle withHandle className="workspace-resize-handle" />

        <ResizablePanel
          id="inspector"
          panelRef={inspectorPanelRef}
          defaultSize="20%"
          minSize="15%"
          maxSize="42%"
          collapsible
          collapsedSize="0%"
          className="inspector"
          aria-label="Inspector"
        >
            <PaneHeader title="Inspector" onClose={() => setRightOpen(false)} closeLabel="Hide inspector" />
            <div className="pane-body">
              <InspectorPanel
                tab={inspectorTab}
                onTabChange={setInspectorTab}
                symbolQuery={symbolQuery}
                onSymbolQueryChange={setSymbolQuery}
                filteredSymbols={filteredSymbols}
                selectedSymbol={selectedSymbol}
                onSelectSymbol={selectSymbol}
                onAddWatch={addWatch}
                completions={completions}
                hoverSymbol={hoverSymbol}
                targetProps={{
                  entries: mappingEntries,
                  issues: deployIssues,
                  metadata: cArtifact?.metadata ?? null,
                  mappingFileName,
                  buildSourceName: buildSourceFile?.name ?? null,
                  deployPreview,
                  deployDiff,
                  adapterArtifacts,
                  safetyPolicy,
                  staleMappedSymbols,
                  programHash,
                  editorMatchesTarget: targetConnection.editorMatchesTarget,
                  liveIoValues: targetIoValues,
                  hardwareConnected: hardwareOnline,
                  onOpenMapping: () => selectFile(mappingFileName),
                  onCreateMapping: handleCreateMapping,
                  onBuildC: () => void handleBuildC(),
                  onPreviewDeploy: handlePreviewDeploy,
                  onRevalidateDeploy: handleRevalidateDeploy,
                  onSaveDeployBaseline: handleSaveDeployBaseline,
                  onSafetyPolicyChange: setSafetyPolicy,
                  onSaveSafetyPolicy: handleSaveSafetyPolicy
                }}
              />
            </div>
        </ResizablePanel>
      </ResizablePanelGroup>

      <footer className="statusbar" aria-live="polite">
        <span title={`Language service: ${engineMode}`}>
          <CheckCircle2 size={12} aria-hidden="true" />
          {analyzing ? "Analyzing…" : errorCount === 0 ? "Ready" : `${errorCount} error${errorCount === 1 ? "" : "s"}`}
        </span>
        {warningCount > 0 ? (
          <span className="status-warn-item">
            <AlertTriangle size={12} aria-hidden="true" />
            {warningCount} warning{warningCount === 1 ? "" : "s"}
          </span>
        ) : null}
        <span title={runState === "running" ? "Simulation in progress" : "Simulation state"}>
          <Activity size={12} aria-hidden="true" />
          {runState === "idle" ? "Idle" : runState === "running" ? "Running…" : "Done"}
        </span>
        <span>
          <Gauge size={12} aria-hidden="true" />
          {settings.cycleTimeMs.toFixed(2)} ms
        </span>
        {autosaveState === "autosaving" ? (
          <span className="status-saving">Autosaving…</span>
        ) : autosaveState === "error" ? (
          <span className="status-error">Save failed</span>
        ) : isProjectDirty ? (
          <span className="status-dirty">Unsaved</span>
        ) : (
          <span className="status-saved">Saved</span>
        )}
        <span className="status-target-hash" title={`Program hash ${programHash}`}>
          #{programHash}
        </span>
        <span className="statusbar-spacer" />
        <span translate="no">{activeFile.name}</span>
      </footer>

      {dialogMode ? (
        <ProjectDialog
          open
          mode={dialogMode}
          projects={openableProjects}
          onOpenChange={(nextOpen) => {
            if (!nextOpen) {
              setDialogMode(null);
            }
          }}
          onCreate={handleCreateProject}
          onOpen={openProject}
          onDelete={handleDeleteProject}
        />
      ) : null}
      <NewFileDialog
        open={showNewFile}
        onOpenChange={setShowNewFile}
        existingNames={project.files.map((file) => file.name)}
        onCreate={handleCreateFile}
      />
      <SettingsDialog
        open={showSettings}
        settings={settings}
        programNames={programNames}
        supportDiagnostics={{
          project,
          settings,
          engineMode,
          targetConnection,
          commandLog
        }}
        onOpenChange={setShowSettings}
        onSave={handleSaveSettings}
      />
      <FindReplaceDialog
        open={findReplaceDialog !== null}
        mode={findReplaceDialog?.mode ?? "find"}
        fileName={activeFile?.name}
        initialQuery={findReplaceDialog?.query}
        onOpenChange={(open) => {
          if (!open) {
            setFindReplaceDialog(null);
          }
        }}
        onFindNext={(query) => codeEditorRef.current?.findNext(query) ?? false}
        onReplaceNext={(query, replacement) => codeEditorRef.current?.replaceNext(query, replacement) ?? false}
        onReplaceAll={(query, replacement) => codeEditorRef.current?.replaceAll(query, replacement) ?? 0}
        onStatus={(message, level = "info") => appendLog(message, level)}
      />
      <RenameDialog
        open={renameSymbolToken !== null}
        title="Rename symbol"
        description="Rename the identifier across the active file."
        currentName={renameSymbolToken ?? ""}
        fieldLabel="Symbol"
        validate={(next) => (/^[A-Za-z_][A-Za-z0-9_]*$/.test(next) ? null : "Use a valid IEC identifier.")}
        onOpenChange={(open) => {
          if (!open) {
            setRenameSymbolToken(null);
          }
        }}
        onSubmit={submitRenameSymbol}
      />
      <CommandPalette
        open={commandPaletteOpen}
        query={commandPaletteQuery}
        items={commandPaletteItems}
        onOpenChange={setCommandPaletteOpen}
        onQueryChange={setCommandPaletteQuery}
      />
    </main>
    </>
  );
}
