import type { DeployPackage } from "@/features/target/deployClient";

export type DeployDiffEntry = {
  path: string;
  status: "added" | "removed" | "changed" | "unchanged";
  detail: string;
};

export function parseDeployPackageJson(text: string): DeployPackage | null {
  try {
    return JSON.parse(text) as DeployPackage;
  } catch {
    return null;
  }
}

export function diffDeployPackages(baseline: DeployPackage | null, current: DeployPackage): DeployDiffEntry[] {
  if (!baseline) {
    return [
      { path: "package", status: "added", detail: "No baseline deploy package saved yet." },
      { path: "sourceFiles", status: "added", detail: `${current.sourceFiles.length} source file(s)` },
      { path: "mapping", status: current.mapping ? "added" : "removed", detail: current.mapping ? "mapping present" : "missing" },
      { path: "metadata", status: current.metadata ? "added" : "removed", detail: current.metadata ? "metadata present" : "missing" },
      { path: "adapterStubs", status: "added", detail: `${current.adapterStubs.length} adapter stub(s)` }
    ];
  }

  const entries: DeployDiffEntry[] = [];

  if (baseline.project !== current.project) {
    entries.push({
      path: "project",
      status: "changed",
      detail: `${baseline.project} → ${current.project}`
    });
  }

  const baselineSources = new Set(baseline.sourceFiles);
  const currentSources = new Set(current.sourceFiles);
  for (const file of current.sourceFiles) {
    if (!baselineSources.has(file)) {
      entries.push({ path: `source:${file}`, status: "added", detail: "New project file in deploy package" });
    }
  }
  for (const file of baseline.sourceFiles) {
    if (!currentSources.has(file)) {
      entries.push({ path: `source:${file}`, status: "removed", detail: "Removed from deploy package" });
    }
  }

  const baselineMapping = JSON.stringify(baseline.mapping ?? null);
  const currentMapping = JSON.stringify(current.mapping ?? null);
  if (baselineMapping !== currentMapping) {
    entries.push({
      path: "mapping",
      status: "changed",
      detail: `${baseline.mapping?.entries.length ?? 0} → ${current.mapping?.entries.length ?? 0} binding(s)`
    });
  }

  const baselineMeta = JSON.stringify(baseline.metadata ?? null);
  const currentMeta = JSON.stringify(current.metadata ?? null);
  if (baselineMeta !== currentMeta) {
    entries.push({
      path: "metadata",
      status: "changed",
      detail: "Generated C metadata changed since baseline"
    });
  }

  if (baseline.adapterStubs.length !== current.adapterStubs.length) {
    entries.push({
      path: "adapterStubs",
      status: "changed",
      detail: `${baseline.adapterStubs.length} → ${current.adapterStubs.length} adapter stub(s)`
    });
  }

  const baselineArtifacts = JSON.stringify(baseline.adapterArtifacts ?? []);
  const currentArtifacts = JSON.stringify(current.adapterArtifacts ?? []);
  if (baselineArtifacts !== currentArtifacts) {
    entries.push({
      path: "adapterArtifacts",
      status: "changed",
      detail: `${baseline.adapterArtifacts?.length ?? 0} → ${current.adapterArtifacts?.length ?? 0} adapter artifact(s)`
    });
  }

  if (entries.length === 0) {
    entries.push({ path: "package", status: "unchanged", detail: "Deploy package matches the saved baseline." });
  }

  return entries;
}

export function deployDiffSummary(entries: DeployDiffEntry[]): string {
  const changed = entries.filter((entry) => entry.status !== "unchanged");
  if (changed.length === 0) {
    return "No deploy changes since baseline.";
  }
  return changed.map((entry) => `${entry.status.toUpperCase()} ${entry.path}: ${entry.detail}`).join("\n");
}
