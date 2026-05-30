import type { Layout } from "react-resizable-panels";

const WORKSPACE_LAYOUT_KEY = "robocpp-studio-workspace-layout";

export const DEFAULT_WORKSPACE_LAYOUT: Layout = {
  explorer: 14,
  editor: 66,
  inspector: 20
};

export function readWorkspaceLayout(): Layout | undefined {
  try {
    const raw = localStorage.getItem(WORKSPACE_LAYOUT_KEY);
    if (!raw) {
      return undefined;
    }
    const parsed = JSON.parse(raw) as Layout;
    if (parsed.explorer == null || parsed.editor == null) {
      return undefined;
    }
    return {
      explorer: parsed.explorer > 0 ? Math.max(parsed.explorer, 12) : 0,
      editor: Math.max(parsed.editor, 35),
      inspector: parsed.inspector != null && parsed.inspector > 0 ? Math.max(parsed.inspector, 12) : 0
    };
  } catch {
    return undefined;
  }
}

export function persistWorkspaceLayout(layout: Layout) {
  localStorage.setItem(WORKSPACE_LAYOUT_KEY, JSON.stringify(layout));
}

export function panelIsOpen(layoutValue: number | undefined, fallback = true): boolean {
  return (layoutValue ?? (fallback ? 1 : 0)) > 0;
}
