import { projectTemplateById, type ProjectTemplateId } from "@/features/project/projectTemplates";
import { workspaceFiles } from "@/features/project/samples";
import { DEFAULT_TARGET_MAPPING_TEXT, isTargetMappingFile } from "@/features/target/targetMapping";
import type { Project, WorkspaceFile } from "@/types";
import { createFileTemplate, defaultFileName, uniqueFileName, type FileLanguageId, languageFromFileName } from "@/features/project/fileTemplates";

const STORAGE_KEY = "robocpp-studio-projects-v1";

export const SAMPLE_PROJECT_ID = "sample-packaging-line";

function cloneFiles(files: WorkspaceFile[]): WorkspaceFile[] {
  return files.map((file) => normalizeProjectFile({ ...file }));
}

function normalizeProjectFile(file: WorkspaceFile): WorkspaceFile {
  if (isTargetMappingFile(file.name)) {
    return { ...file, languageId: "mapping" };
  }
  return file;
}

export function createSampleProject(): Project {
  return {
    id: SAMPLE_PROJECT_ID,
    name: "PackagingLine",
    files: [
      ...cloneFiles(workspaceFiles),
      {
        name: "target/mapping.toml",
        languageId: "mapping",
        text: DEFAULT_TARGET_MAPPING_TEXT
      }
    ],
    updatedAt: new Date().toISOString(),
    builtIn: true
  };
}

export function createEmptyProject(name: string): Project {
  return {
    id: crypto.randomUUID(),
    name,
    files: [
      {
        name: "main.st",
        languageId: "st",
        text: `PROGRAM Main
VAR
END_VAR
END_PROGRAM
`
      }
    ],
    updatedAt: new Date().toISOString()
  };
}

export function createProjectFromTemplate(name: string, templateId: ProjectTemplateId = "sample"): Project {
  return {
    id: crypto.randomUUID(),
    name,
    files: projectTemplateById(templateId).buildFiles(),
    updatedAt: new Date().toISOString()
  };
}

export function loadSavedProjects(): Project[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw) as Project[];
    return Array.isArray(parsed)
      ? parsed.filter((project) => !project.builtIn).map((project) => ({
          ...project,
          files: project.files.map((file) => normalizeProjectFile(file))
        }))
      : [];
  } catch {
    return [];
  }
}

export function persistProject(project: Project): void {
  if (project.builtIn) {
    return;
  }
  const saved = loadSavedProjects().filter((entry) => entry.id !== project.id);
  saved.unshift({ ...project, updatedAt: new Date().toISOString() });
  localStorage.setItem(STORAGE_KEY, JSON.stringify(saved.slice(0, 20)));
}

export function deleteSavedProject(id: string): void {
  const saved = loadSavedProjects().filter((entry) => entry.id !== id);
  localStorage.setItem(STORAGE_KEY, JSON.stringify(saved));
}

export function listOpenableProjects(): Project[] {
  return [createSampleProject(), ...loadSavedProjects()];
}

export function updateProjectFile(project: Project, fileName: string, text: string): Project {
  return {
    ...project,
    files: project.files.map((file) => (file.name === fileName ? { ...file, text } : file)),
    updatedAt: new Date().toISOString()
  };
}

export function addProjectFile(
  project: Project,
  requestedName: string,
  languageId: FileLanguageId
): { project: Project; file: WorkspaceFile } | null {
  const existingNames = project.files.map((file) => file.name);
  let name = requestedName.trim();
  const detected = languageFromFileName(name);
  const resolvedLanguage = detected ?? languageId;

  if (!name.includes(".")) {
    name = defaultFileName(resolvedLanguage, existingNames);
  }

  const unique = uniqueFileName(name, existingNames);
  if (!unique) {
    return null;
  }

  const file = createFileTemplate(resolvedLanguage, unique);
  return {
    project: {
      ...project,
      files: [...project.files, file],
      updatedAt: new Date().toISOString()
    },
    file
  };
}

export function removeProjectFile(project: Project, fileName: string): Project | null {
  if (project.files.length <= 1) {
    return null;
  }
  return {
    ...project,
    files: project.files.filter((file) => file.name !== fileName),
    updatedAt: new Date().toISOString()
  };
}

export function renameProjectFile(
  project: Project,
  oldName: string,
  newName: string
): { project: Project; fileName: string } | null {
  const file = project.files.find((entry) => entry.name === oldName);
  if (!file) {
    return null;
  }

  const trimmed = newName.trim();
  if (!trimmed) {
    return null;
  }

  const detected = languageFromFileName(trimmed);
  if (!detected) {
    return null;
  }

  const existingNames = project.files.map((entry) => entry.name).filter((name) => name !== oldName);
  const unique = uniqueFileName(trimmed, existingNames);
  if (!unique) {
    return null;
  }

  return {
    fileName: unique,
    project: {
      ...project,
      files: project.files.map((entry) =>
        entry.name === oldName ? { ...entry, name: unique, languageId: detected } : entry
      ),
      updatedAt: new Date().toISOString()
    }
  };
}

export function reorderProjectFile(project: Project, fileName: string, beforeFileName: string): Project {
  if (fileName === beforeFileName) {
    return project;
  }
  const files = [...project.files];
  const fromIndex = files.findIndex((file) => file.name === fileName);
  const beforeIndex = files.findIndex((file) => file.name === beforeFileName);
  if (fromIndex < 0 || beforeIndex < 0) {
    return project;
  }
  const [moved] = files.splice(fromIndex, 1);
  const insertAt = fromIndex < beforeIndex ? beforeIndex - 1 : beforeIndex;
  files.splice(insertAt, 0, moved);
  return {
    ...project,
    files,
    updatedAt: new Date().toISOString()
  };
}
