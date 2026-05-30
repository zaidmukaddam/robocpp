import type { Project } from "@/types";

type SnapshotShape = {
  files?: Array<{ name: string; text: string }>;
};

export function dirtyFileNames(project: Project, savedProjectSnapshot: string): Set<string> {
  let savedFiles: Map<string, string>;
  try {
    const parsed = JSON.parse(savedProjectSnapshot) as SnapshotShape;
    savedFiles = new Map((parsed.files ?? []).map((file) => [file.name, file.text]));
  } catch {
    return new Set(project.files.map((file) => file.name));
  }

  const dirty = new Set<string>();
  for (const file of project.files) {
    if (savedFiles.get(file.name) !== file.text) {
      dirty.add(file.name);
    }
  }
  return dirty;
}
