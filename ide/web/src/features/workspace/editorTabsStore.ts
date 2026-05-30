import { syncOpenTabsWithProject } from "@/features/explorer/openTabs";

const EDITOR_TABS_KEY = "robocpp-studio-editor-tabs";

export type EditorTabsState = {
  openTabNames: string[];
  activeFileName: string;
};

type EditorTabsStore = Record<string, EditorTabsState>;

function readStore(): EditorTabsStore {
  try {
    const raw = localStorage.getItem(EDITOR_TABS_KEY);
    if (!raw) {
      return {};
    }
    const parsed = JSON.parse(raw) as EditorTabsStore;
    return parsed && typeof parsed === "object" ? parsed : {};
  } catch {
    return {};
  }
}

function writeStore(store: EditorTabsStore): void {
  localStorage.setItem(EDITOR_TABS_KEY, JSON.stringify(store));
}

export function readEditorTabs(projectId: string): EditorTabsState | undefined {
  const saved = readStore()[projectId];
  if (!saved || !Array.isArray(saved.openTabNames)) {
    return undefined;
  }
  return {
    openTabNames: saved.openTabNames.filter((name) => typeof name === "string"),
    activeFileName: typeof saved.activeFileName === "string" ? saved.activeFileName : ""
  };
}

export function persistEditorTabs(projectId: string, state: EditorTabsState): void {
  const store = readStore();
  store[projectId] = {
    openTabNames: state.openTabNames,
    activeFileName: state.activeFileName
  };
  writeStore(store);
}

export function resolveEditorTabsState(
  saved: EditorTabsState | undefined,
  projectFileNames: string[]
): EditorTabsState {
  const openTabNames = syncOpenTabsWithProject(saved?.openTabNames ?? [], projectFileNames);
  const activeFileName =
    saved?.activeFileName &&
    projectFileNames.includes(saved.activeFileName) &&
    openTabNames.includes(saved.activeFileName)
      ? saved.activeFileName
      : openTabNames[0] ?? projectFileNames[0] ?? "";

  return { openTabNames, activeFileName };
}
