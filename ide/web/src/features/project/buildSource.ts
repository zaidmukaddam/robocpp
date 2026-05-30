import { isTargetMappingFile } from "@/features/target/targetMapping";
import type { Project, WorkspaceFile } from "@/types";

const BUILD_LANGUAGE_ORDER: WorkspaceFile["languageId"][] = ["ld", "st", "fbd", "sfc", "il"];

export function resolveBuildSourceFile(
  project: Project,
  activeFile: WorkspaceFile | undefined
): WorkspaceFile | undefined {
  if (activeFile && isBuildableSource(activeFile)) {
    return activeFile;
  }
  for (const languageId of BUILD_LANGUAGE_ORDER) {
    const match = project.files.find((file) => file.languageId === languageId);
    if (match) {
      return match;
    }
  }
  return project.files.find((file) => isBuildableSource(file));
}

function isBuildableSource(file: WorkspaceFile): boolean {
  return !isTargetMappingFile(file.name) && file.languageId !== "xml" && file.languageId !== "mapping";
}
