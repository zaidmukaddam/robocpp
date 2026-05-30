export type Severity = "error" | "warning" | "note";

export type SourcePosition = {
  line: number;
  character: number;
};

export type SourceRange = {
  uri: string;
  start: number;
  end: number;
  startPosition: SourcePosition;
  endPosition: SourcePosition;
};

export type Diagnostic = {
  severity: Severity;
  code: string;
  stableCode: string;
  message: string;
  span: {
    source: string;
    start: number;
    end: number;
    line: number;
    column: number;
  } | null;
  help: string | null;
};

export type DocumentSymbol = {
  name: string;
  kind: string;
  detail: string;
  containerName: string | null;
  range: SourceRange | null;
};

export type CompletionItem = {
  label: string;
  kind: string;
  detail: string;
};

export type Analysis = {
  uri: string;
  diagnostics: Diagnostic[];
  symbols: DocumentSymbol[];
  completions: CompletionItem[];
};

export type WorkspaceFile = {
  name: string;
  languageId: "st" | "il" | "ld" | "fbd" | "sfc" | "xml" | "mapping";
  text: string;
};

export type ProjectArtifact = {
  id: string;
  kind: "generated-c" | "trace-export" | "diagnostic-report" | "plcopen-export" | "deploy-package";
  name: string;
  sourceFile: string;
  content: string;
  mimeType: string;
  createdAt: string;
  sourceTextHash?: string;
};

export type Project = {
  id: string;
  name: string;
  files: WorkspaceFile[];
  updatedAt: string;
  builtIn?: boolean;
};

export type TraceVariable = {
  name: string;
  value: string | number | boolean;
};

export type TraceCycle = {
  cycle: number;
  variables: TraceVariable[];
  events: string[];
};

export type RunTrace = {
  program: string;
  source: string;
  cycles: TraceCycle[];
  generatedC: string;
};

export type DebugAccessPath = {
  name: string;
  target: string;
  direction: string;
  value: string | number | boolean | null;
};

export type DebugCycle = {
  cycle: number;
  recordedAt: string;
  watches: TraceVariable[];
  variables: TraceVariable[];
  accessPaths: DebugAccessPath[];
  activeSfcSteps: string[];
  events: string[];
};

export type DebugTrace = {
  uri: string;
  program: string;
  cycles: DebugCycle[];
};

export type CEntrypoint = {
  name: string;
  signature: string;
};

export type CStateField = {
  name: string;
  typeName: string;
  retained: boolean;
  sourceName: string;
};

export type CIoSymbol = {
  name: string;
  location: string;
  direction: string;
  typeName: string;
};

export type CAccessPathMeta = {
  name: string;
  target: string;
  direction: string;
  typeName: string;
};

export type CDebugSymbol = {
  name: string;
  kind: string;
  typeName: string;
};

export type GeneratedCMetadata = {
  filenameHint: string;
  scanEntrypoints: CEntrypoint[];
  stateLayout: CStateField[];
  ioSymbols: CIoSymbol[];
  accessPaths: CAccessPathMeta[];
  retainedFields: string[];
  targetHooks: string[];
  debugSymbols: CDebugSymbol[];
};

export type GeneratedCArtifact = {
  source: string;
  metadata: GeneratedCMetadata;
};

export type CompilerProfile = "2003-strict" | "2003-extended";

export type IdeSettings = {
  compilerProfile: CompilerProfile;
  cycleTimeMs: number;
  selectedProgram: string;
  selectedConfiguration: string;
  generatedCOutputPath: string;
  targetMappingPath: string;
  simulationCycles: number;
  watchVariables: string;
  targetBridgeUrl: string;
  targetWorkspaceRoot: string;
  targetModbusPort: number;
};

export type ServiceCapabilities = {
  profile: string;
  sourceFormats: string[];
  features: Record<string, boolean>;
};
