import type { Project } from "@/types";

export type ProjectSnapshot = {
  id: string;
  projectId: string;
  name: string;
  createdAt: string;
  project: Project;
};

const STORAGE_KEY = "robocpp-studio-snapshots-v1";
const MAX_SNAPSHOTS_PER_PROJECT = 8;

function readSnapshots(): ProjectSnapshot[] {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      return [];
    }
    const parsed = JSON.parse(raw) as ProjectSnapshot[];
    return Array.isArray(parsed) ? parsed : [];
  } catch {
    return [];
  }
}

function writeSnapshots(snapshots: ProjectSnapshot[]): void {
  localStorage.setItem(STORAGE_KEY, JSON.stringify(snapshots));
}

export function listProjectSnapshots(projectId: string): ProjectSnapshot[] {
  return readSnapshots()
    .filter((snapshot) => snapshot.projectId === projectId)
    .sort((left, right) => right.createdAt.localeCompare(left.createdAt));
}

export function saveProjectSnapshot(project: Project, label?: string): ProjectSnapshot {
  const snapshot: ProjectSnapshot = {
    id: crypto.randomUUID(),
    projectId: project.id,
    name: label?.trim() || `${project.name} @ ${new Date().toLocaleString()}`,
    createdAt: new Date().toISOString(),
    project: { ...project, builtIn: undefined }
  };
  const otherProjects = readSnapshots().filter((entry) => entry.projectId !== project.id);
  const projectSnapshots = [snapshot, ...listProjectSnapshots(project.id)].slice(0, MAX_SNAPSHOTS_PER_PROJECT);
  writeSnapshots([...projectSnapshots, ...otherProjects]);
  return snapshot;
}

export function restoreProjectSnapshot(snapshotId: string): Project | null {
  const snapshot = readSnapshots().find((entry) => entry.id === snapshotId);
  return snapshot ? snapshot.project : null;
}

export function deleteProjectSnapshot(snapshotId: string): void {
  writeSnapshots(readSnapshots().filter((entry) => entry.id !== snapshotId));
}
