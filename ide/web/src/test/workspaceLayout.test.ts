import { describe, expect, it } from "vitest";
import { DEFAULT_WORKSPACE_LAYOUT, panelIsOpen, readWorkspaceLayout } from "@/features/workspace/workspaceLayout";

describe("workspace layout", () => {
  it("defaults explorer and inspector to open", () => {
    expect(panelIsOpen(DEFAULT_WORKSPACE_LAYOUT.explorer)).toBe(true);
    expect(panelIsOpen(DEFAULT_WORKSPACE_LAYOUT.inspector)).toBe(true);
    expect(panelIsOpen(0)).toBe(false);
  });

  it("reads saved layout from localStorage when present", () => {
    localStorage.setItem(
      "robocpp-studio-workspace-layout",
      JSON.stringify({ explorer: 24, editor: 56, inspector: 20 })
    );
    expect(readWorkspaceLayout()?.explorer).toBe(24);
    localStorage.removeItem("robocpp-studio-workspace-layout");
  });
});
