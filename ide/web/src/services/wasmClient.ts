import { analyzeLocally } from "@/services/localAnalysis";
import { debugLocally } from "@/services/localDebug";
import { buildLocalGraphModel } from "@/features/graph/localGraphModel";
import { readEngineGraphCache, writeEngineGraphCache, type GraphSnapshot } from "@/features/graph/graphCache";
import { runLocally } from "@/services/localRunner";
import { parseWatchList } from "@/stores/settingsStore";
import { analyzeTargetMapping, isTargetMappingFile } from "@/features/target/targetMapping";
import type {
  Analysis,
  DebugTrace,
  GeneratedCArtifact,
  IdeSettings,
  RunTrace,
  ServiceCapabilities,
  WorkspaceFile
} from "@/types";
import type { GraphModel } from "@/features/graph/graphTypes";
import type { GraphValidation } from "@/features/graph/validateGraph";
import { validateGraphLocal } from "@/features/graph/validateGraph";

// IDE/compiler boundary: see crates/iec_language_service_wasm/IDE_CONTRACT.md before changing these exports.
type WasmModule = {
  default: () => Promise<unknown>;
  analyze_document_json: (uri: string, text: string, languageId?: string | null) => string;
  graph_model_json?: (uri: string, text: string, languageId?: string | null) => string;
  validate_graph_json?: (uri: string, text: string, languageId?: string | null) => string;
  run_document_json?: (uri: string, text: string, languageId?: string | null, cycles?: number | null) => string;
  debug_document_json?: (uri: string, text: string, languageId?: string | null, cycles?: number | null) => string;
  generated_c_artifact_json?: (uri: string, text: string, languageId?: string | null) => string;
  capabilities_json?: () => string;
};

export type EngineMode = "wasm" | "local";

let wasmModule: Promise<WasmModule | null> | null = null;
let engineMode: EngineMode | null = null;

async function loadWasm(): Promise<WasmModule | null> {
  if (!wasmModule) {
    wasmModule = import("../wasm/iec_language_service_wasm/iec_language_service_wasm.js")
      .then(async (mod: WasmModule) => {
        await mod.default();
        return mod;
      })
      .catch(() => null);
  }
  return wasmModule;
}

export async function getEngineMode(): Promise<EngineMode> {
  if (engineMode) {
    return engineMode;
  }
  const wasm = await loadWasm();
  engineMode = wasm ? "wasm" : "local";
  return engineMode;
}

export async function getCapabilities(): Promise<ServiceCapabilities | null> {
  const wasm = await loadWasm();
  if (!wasm?.capabilities_json) {
    return null;
  }
  return JSON.parse(wasm.capabilities_json()) as ServiceCapabilities;
}

export async function analyzeFile(file: WorkspaceFile): Promise<Analysis> {
  if (isTargetMappingFile(file.name) || file.languageId === "mapping") {
    return analyzeTargetMapping(file.name, file.text);
  }
  const wasm = await loadWasm();
  if (!wasm) {
    engineMode = "local";
    return analyzeLocally(file);
  }

  engineMode = "wasm";
  return JSON.parse(wasm.analyze_document_json(file.name, file.text, file.languageId)) as Analysis;
}

const EMPTY_GRAPH_MODEL: GraphModel = {
  uri: "",
  pous: [],
  plcopenLayout: { nodeIds: [], connectorIds: [], branchGeometry: [], actionBlocks: [], vendorAddData: [] }
};

async function loadEngineGraphSnapshot(file: WorkspaceFile): Promise<GraphSnapshot> {
  if (isTargetMappingFile(file.name) || file.languageId === "mapping") {
    return { model: { ...EMPTY_GRAPH_MODEL, uri: file.name }, validation: { valid: true, diagnostics: [] } };
  }

  const cached = readEngineGraphCache(file);
  if (cached) {
    return cached;
  }

  const wasm = await loadWasm();
  if (wasm?.graph_model_json && wasm.validate_graph_json) {
    try {
      const model = JSON.parse(wasm.graph_model_json(file.name, file.text, file.languageId)) as GraphModel;
      const validation = JSON.parse(
        wasm.validate_graph_json(file.name, file.text, file.languageId)
      ) as GraphValidation;
      const snapshot = { model, validation };
      writeEngineGraphCache(file, snapshot);
      return snapshot;
    } catch {
      const model = buildLocalGraphModel(file);
      const snapshot = { model, validation: validateGraphLocal(model) };
      writeEngineGraphCache(file, snapshot);
      return snapshot;
    }
  }

  const model = buildLocalGraphModel(file);
  const snapshot = { model, validation: validateGraphLocal(model) };
  writeEngineGraphCache(file, snapshot);
  return snapshot;
}

export async function graphModelForFile(file: WorkspaceFile): Promise<GraphModel> {
  return (await loadEngineGraphSnapshot(file)).model;
}

export async function validateGraphForFile(file: WorkspaceFile): Promise<GraphValidation> {
  return (await loadEngineGraphSnapshot(file)).validation;
}

export async function loadGraphForFile(file: WorkspaceFile): Promise<GraphSnapshot> {
  return loadEngineGraphSnapshot(file);
}

export async function runFile(file: WorkspaceFile, cycles = 5): Promise<RunTrace> {
  const wasm = await loadWasm();
  if (!wasm?.run_document_json) {
    return runLocally(file, cycles);
  }

  const trace = JSON.parse(
    wasm.run_document_json(file.name, file.text, file.languageId, cycles)
  ) as RunTrace & { generatedC?: string };

  if (trace.generatedC) {
    return trace;
  }

  const local = runLocally(file, cycles);
  return { ...trace, generatedC: local.generatedC };
}

export async function debugFile(
  file: WorkspaceFile,
  settings: Pick<IdeSettings, "simulationCycles" | "watchVariables">
): Promise<DebugTrace> {
  const cycles = settings.simulationCycles;
  const watches = parseWatchList(settings.watchVariables);
  const wasm = await loadWasm();

  if (!wasm?.debug_document_json) {
    return debugLocally(file, cycles, settings.watchVariables);
  }

  const trace = JSON.parse(
    wasm.debug_document_json(file.name, file.text, file.languageId, cycles)
  ) as DebugTrace;

  if (watches.length === 0) {
    return trace;
  }

  const watchSet = new Set(watches.map((name) => name.toLowerCase()));
  return {
    ...trace,
    cycles: trace.cycles.map((cycle) => ({
      ...cycle,
      watches: cycle.variables.filter((variable) => watchSet.has(variable.name.toLowerCase()))
    }))
  };
}

export async function buildCArtifact(file: WorkspaceFile): Promise<GeneratedCArtifact> {
  const wasm = await loadWasm();
  if (wasm?.generated_c_artifact_json) {
    return JSON.parse(wasm.generated_c_artifact_json(file.name, file.text, file.languageId)) as GeneratedCArtifact;
  }

  const runTrace = runLocally(file, 1);
  return {
    source: runTrace.generatedC,
    metadata: {
      filenameHint: file.name.replace(/\.[^.]+$/, ".c"),
      scanEntrypoints: [{ name: `${runTrace.program.toLowerCase()}_scan`, signature: "void scan(state_t *)" }],
      stateLayout: runTrace.cycles[0]?.variables.map((variable) => ({
        name: variable.name,
        typeName: typeof variable.value === "boolean" ? "BOOL" : "INT",
        retained: false,
        sourceName: variable.name
      })) ?? [],
      ioSymbols: [],
      accessPaths: [],
      retainedFields: [],
      targetHooks: [],
      debugSymbols: runTrace.cycles[0]?.variables.map((variable) => ({
        name: variable.name,
        kind: "variable",
        typeName: typeof variable.value === "boolean" ? "BOOL" : "INT"
      })) ?? []
    }
  };
}

export function engineStatusText(mode: EngineMode, analyzing: boolean): string {
  if (analyzing) {
    return "Analyzing…";
  }
  return mode === "wasm" ? "Language service" : "Local fallback";
}
