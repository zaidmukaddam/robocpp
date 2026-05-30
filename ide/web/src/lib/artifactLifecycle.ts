import type { Project, ProjectArtifact, WorkspaceFile } from "@/types";

export function sourceTextHash(text: string): string {
  let hash = 0;
  for (let index = 0; index < text.length; index += 1) {
    hash = (hash * 31 + text.charCodeAt(index)) | 0;
  }
  return String(hash);
}

export function fingerprintFile(file: WorkspaceFile): string {
  return `${file.name}:${sourceTextHash(file.text)}`;
}

export function isArtifactStale(artifact: ProjectArtifact, project: Project): boolean {
  const source = project.files.find((file) => file.name === artifact.sourceFile);
  if (!source) {
    return true;
  }
  if (artifact.sourceTextHash) {
    return artifact.sourceTextHash !== sourceTextHash(source.text);
  }
  return new Date(artifact.createdAt).getTime() < new Date(project.updatedAt).getTime();
}

export function artifactKindLabel(kind: ProjectArtifact["kind"]): string {
  switch (kind) {
    case "generated-c":
      return "Generated C";
    case "trace-export":
      return "Trace export";
    case "diagnostic-report":
      return "Diagnostics";
    case "plcopen-export":
      return "PLCopen export";
    case "deploy-package":
      return "Deploy package";
  }
}
