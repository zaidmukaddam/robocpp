import type { Project } from "@/types";

export type ProjectBundle = {
  format: "robocpp-studio-project";
  version: 1;
  exportedAt: string;
  project: Project;
};

export function exportProjectBundle(project: Project): void {
  const bundle: ProjectBundle = {
    format: "robocpp-studio-project",
    version: 1,
    exportedAt: new Date().toISOString(),
    project: { ...project, builtIn: undefined }
  };
  const blob = new Blob([JSON.stringify(bundle, null, 2)], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `${project.name}.robocpp-project.json`;
  anchor.click();
  URL.revokeObjectURL(url);
}

export async function pickProjectBundle(): Promise<Project | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".json,application/json";
    input.onchange = async () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        return;
      }
      try {
        const parsed = JSON.parse(await file.text()) as ProjectBundle | Project;
        if ("project" in parsed && parsed.project?.files) {
          resolve(parsed.project);
          return;
        }
        if ("files" in parsed && Array.isArray(parsed.files)) {
          resolve(parsed as Project);
          return;
        }
        resolve(null);
      } catch {
        resolve(null);
      }
    };
    input.click();
  });
}

export function projectSnapshot(project: Project): string {
  return JSON.stringify({
    name: project.name,
    files: project.files.map((file) => ({ name: file.name, languageId: file.languageId, text: file.text }))
  });
}
