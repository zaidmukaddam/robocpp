import { sourceTextHash } from "@/lib/artifactLifecycle";
import type { Analysis, DebugTrace, GeneratedCArtifact, ProjectArtifact } from "@/types";

const STORAGE_KEY = "robocpp-studio-artifacts-v1";
const MAX_ARTIFACTS_PER_PROJECT = 40;

function readStore(): Record<string, ProjectArtifact[]> {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return {};
    }
    const parsed = JSON.parse(raw) as Record<string, ProjectArtifact[]>;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function writeStore(store: Record<string, ProjectArtifact[]>): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(store));
}

export function listProjectArtifacts(projectId: string): ProjectArtifact[] {
  return readStore()[projectId] ?? [];
}

export function saveProjectArtifact(
  projectId: string,
  artifact: Omit<ProjectArtifact, "id" | "createdAt">,
  options?: { sourceText?: string }
): ProjectArtifact {
  const entry: ProjectArtifact = {
    ...artifact,
    sourceTextHash: options?.sourceText ? sourceTextHash(options.sourceText) : artifact.sourceTextHash,
    id: crypto.randomUUID(),
    createdAt: new Date().toISOString()
  };
  const store = readStore();
  const current = store[projectId] ?? [];
  store[projectId] = [entry, ...current].slice(0, MAX_ARTIFACTS_PER_PROJECT);
  writeStore(store);
  return entry;
}

export function removeProjectArtifact(projectId: string, artifactId: string): void {
  const store = readStore();
  store[projectId] = (store[projectId] ?? []).filter((entry) => entry.id !== artifactId);
  writeStore(store);
}

export function renameProjectArtifact(projectId: string, artifactId: string, nextName: string): ProjectArtifact | null {
  const trimmed = nextName.trim();
  if (!trimmed) {
    return null;
  }
  const store = readStore();
  const current = store[projectId] ?? [];
  const target = current.find((entry) => entry.id === artifactId);
  if (!target) {
    return null;
  }
  const updated: ProjectArtifact = { ...target, name: trimmed };
  store[projectId] = current.map((entry) => (entry.id === artifactId ? updated : entry));
  writeStore(store);
  return updated;
}

export function clearProjectArtifacts(projectId: string): void {
  const store = readStore();
  delete store[projectId];
  writeStore(store);
}

export function artifactFileName(artifact: ProjectArtifact): string {
  return artifact.name;
}

export function saveGeneratedCArtifact(
  projectId: string,
  sourceFile: string,
  artifact: GeneratedCArtifact,
  sourceText?: string
): ProjectArtifact {
  const base = sourceFile.includes(".") ? sourceFile.slice(0, sourceFile.lastIndexOf(".")) : sourceFile;
  return saveProjectArtifact(
    projectId,
    {
      kind: "generated-c",
      name: `${base}.generated.c`,
      sourceFile,
      content: artifact.source,
      mimeType: "text/x-c"
    },
    { sourceText }
  );
}

export function saveTraceArtifact(projectId: string, sourceFile: string, trace: DebugTrace): ProjectArtifact {
  const base = sourceFile.includes(".") ? sourceFile.slice(0, sourceFile.lastIndexOf(".")) : sourceFile;
  return saveProjectArtifact(projectId, {
    kind: "trace-export",
    name: `${base}.trace.json`,
    sourceFile,
    content: JSON.stringify(trace, null, 2),
    mimeType: "application/json"
  });
}

export function saveDiagnosticReport(
  projectId: string,
  sourceFile: string,
  analysis: Analysis
): ProjectArtifact {
  const base = sourceFile.includes(".") ? sourceFile.slice(0, sourceFile.lastIndexOf(".")) : sourceFile;
  const report = {
    uri: analysis.uri,
    generatedAt: new Date().toISOString(),
    diagnostics: analysis.diagnostics
  };
  return saveProjectArtifact(projectId, {
    kind: "diagnostic-report",
    name: `${base}.diagnostics.json`,
    sourceFile,
    content: JSON.stringify(report, null, 2),
    mimeType: "application/json"
  });
}

export function savePlcopenExportArtifact(projectId: string, sourceFile: string, xml: string): ProjectArtifact {
  const base = sourceFile.includes(".") ? sourceFile.slice(0, sourceFile.lastIndexOf(".")) : sourceFile;
  return saveProjectArtifact(projectId, {
    kind: "plcopen-export",
    name: `${base}.export.xml`,
    sourceFile,
    content: xml,
    mimeType: "application/xml"
  });
}

export function downloadArtifact(artifact: ProjectArtifact): void {
  const blob = new Blob([artifact.content], { type: artifact.mimeType });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = artifact.name;
  anchor.click();
  URL.revokeObjectURL(url);
}
