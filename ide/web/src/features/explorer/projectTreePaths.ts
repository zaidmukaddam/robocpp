import type { WorkspaceFile } from "@/types";
import { isTargetMappingFile, mappingFileName } from "@/features/target/targetMapping";
import type { ProjectArtifact } from "@/types";

export const APPLICATION_FOLDER = "Application";
export const PLCOPEN_FOLDER = "PLCopen";
export const TARGET_FOLDER = "Target";
export const GENERATED_FOLDER = "Generated";

const TREE_FOLDERS = [APPLICATION_FOLDER, PLCOPEN_FOLDER, TARGET_FOLDER, GENERATED_FOLDER] as const;

export function folderForLanguage(languageId: WorkspaceFile["languageId"]): string {
  return languageId === "xml" ? PLCOPEN_FOLDER : APPLICATION_FOLDER;
}

export function fileToTreePath(file: WorkspaceFile): string {
  if (isTargetMappingFile(file.name)) {
    return `${TARGET_FOLDER}/mapping.toml`;
  }
  return `${folderForLanguage(file.languageId)}/${file.name}`;
}

export function treePathToFileName(path: string): string {
  const segments = path.split("/").filter(Boolean);
  return segments.at(-1) ?? path;
}

export function isTargetMappingTreePath(path: string): boolean {
  return path === `${TARGET_FOLDER}/mapping.toml`;
}

export function treePathToProjectFileName(path: string, files: WorkspaceFile[]): string {
  if (isTargetMappingTreePath(path)) {
    return files.find((file) => isTargetMappingFile(file.name))?.name ?? mappingFileName();
  }
  const basename = treePathToFileName(path);
  return files.find((file) => file.name === basename)?.name ?? basename;
}

export function filesToTreePaths(files: WorkspaceFile[]): string[] {
  return files.map(fileToTreePath);
}

export function artifactToTreePath(artifact: ProjectArtifact): string {
  return `${GENERATED_FOLDER}/${artifact.name}`;
}

export function artifactsToTreePaths(artifacts: ProjectArtifact[]): string[] {
  return artifacts.map(artifactToTreePath);
}

/** Keep the newest artifact per display name for tree paths (store is newest-first). */
export function dedupeArtifactsByName(artifacts: ProjectArtifact[]): ProjectArtifact[] {
  const seen = new Set<string>();
  const unique: ProjectArtifact[] = [];
  for (const artifact of artifacts) {
    if (seen.has(artifact.name)) {
      continue;
    }
    seen.add(artifact.name);
    unique.push(artifact);
  }
  return unique;
}

export function findLatestArtifactByName(
  artifacts: ProjectArtifact[],
  name: string
): ProjectArtifact | undefined {
  return artifacts.find((artifact) => artifact.name === name);
}

export function isGeneratedArtifactPath(path: string): boolean {
  return path.startsWith(`${GENERATED_FOLDER}/`);
}

export function treePathToArtifactName(path: string): string {
  return path.slice(`${GENERATED_FOLDER}/`.length);
}

export function explorerPaths(files: WorkspaceFile[], artifacts: ProjectArtifact[]): string[] {
  return [...filesToTreePaths(files), ...artifactsToTreePaths(dedupeArtifactsByName(artifacts))];
}

export function treePathsSignature(files: WorkspaceFile[]): string {
  return files.map((file) => `${file.name}:${file.languageId}`).join("|");
}

export function folderFromTreePath(path: string): string | null {
  const segments = path.split("/").filter(Boolean);
  if (segments.length < 2) {
    return null;
  }
  const folder = segments[0];
  return TREE_FOLDERS.includes(folder as (typeof TREE_FOLDERS)[number]) ? folder : null;
}

export function isTreeFilePath(path: string): boolean {
  return folderFromTreePath(path) !== null && !path.endsWith("/");
}
