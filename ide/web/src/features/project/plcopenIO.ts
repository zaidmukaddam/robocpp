import { addProjectFile } from "@/features/project/projectStore";
import type { Project, WorkspaceFile } from "@/types";

export function importPlcopenXml(project: Project, fileName: string, text: string): Project | null {
  const normalized = fileName.endsWith(".xml") ? fileName : `${fileName}.xml`;
  const result = addProjectFile(project, normalized, "xml");
  if (!result) {
    return null;
  }
  return {
    ...result.project,
    files: result.project.files.map((file) =>
      file.name === result.file.name ? { ...file, text } : file
    )
  };
}

import { mergePlcopenMetadata } from "@/features/graph/plcopenMetadata";
import type { GraphModel } from "@/features/graph/graphTypes";

export function exportPlcopenXml(file: WorkspaceFile, model?: GraphModel | null): void {
  const payload = model ? mergePlcopenMetadata(file.text, model) : file.text;
  const blob = new Blob([payload], { type: "application/xml" });
  const url = URL.createObjectURL(blob);
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = file.name.endsWith(".xml") ? file.name : `${file.name}.xml`;
  anchor.click();
  URL.revokeObjectURL(url);
}

export function pickPlcopenFile(): Promise<{ name: string; text: string } | null> {
  return new Promise((resolve) => {
    const input = document.createElement("input");
    input.type = "file";
    input.accept = ".xml,application/xml,text/xml";
    input.addEventListener("change", async () => {
      const file = input.files?.[0];
      if (!file) {
        resolve(null);
        return;
      }
      const text = await file.text();
      resolve({ name: file.name, text });
    });
    input.click();
  });
}
