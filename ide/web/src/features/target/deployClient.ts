import type { GeneratedCMetadata, Project, WorkspaceFile } from "@/types";
import { parseTargetMapping, isTargetMappingFile } from "@/features/target/targetMapping";

export type DeployPackage = {
  project: string;
  generatedAt: string;
  sourceFiles: string[];
  mapping: ReturnType<typeof parseTargetMapping> | null;
  metadata: GeneratedCMetadata | null;
  adapterStubs: string[];
  adapterArtifacts: { name: string; content: string }[];
};

export type DeployIssue = {
  severity: "error" | "warning";
  message: string;
};

export function generateAdapterArtifacts(metadata: GeneratedCMetadata | null): { name: string; content: string }[] {
  if (!metadata) {
    return [];
  }
  const adapters: { name: string; content: string }[] = [];
  for (const hook of metadata.targetHooks) {
    adapters.push({
      name: `adapters/${hook}.c`,
      content: `// Generated adapter stub for ${hook}\n#include "scan.h"\n\nvoid ${hook}(void) {\n  /* bind transport-specific I/O here */\n}\n`
    });
  }
  for (const path of metadata.accessPaths) {
    adapters.push({
      name: `adapters/access_${path.name}.json`,
      content: JSON.stringify(path, null, 2)
    });
  }
  return adapters;
}

export function buildDeployPackage(
  project: Project,
  metadata: GeneratedCMetadata | null
): { package: DeployPackage; issues: DeployIssue[] } {
  const issues: DeployIssue[] = [];
  const mappingFile = project.files.find((file) => isTargetMappingFile(file.name));
  const mapping = mappingFile ? parseTargetMapping(mappingFile.text) : null;

  if (!mappingFile) {
    issues.push({ severity: "error", message: "Missing target/mapping.toml before deployment." });
  }
  if (!metadata) {
    issues.push({ severity: "error", message: "Build C first to produce scan metadata for deployment." });
  }

  const adapterStubs = (metadata?.targetHooks ?? []).map(
    (hook) => `// Adapter hook: ${hook}\nvoid ${hook}(void) { /* target-specific */ }\n`
  );
  const adapterArtifacts = generateAdapterArtifacts(metadata);

  return {
    package: {
      project: project.name,
      generatedAt: new Date().toISOString(),
      sourceFiles: project.files.map((file: WorkspaceFile) => file.name),
      mapping,
      metadata,
      adapterStubs,
      adapterArtifacts
    },
    issues
  };
}

export function serializeDeployPackage(pkg: DeployPackage): string {
  return JSON.stringify(pkg, null, 2);
}
