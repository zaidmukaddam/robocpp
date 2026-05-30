import { describe, expect, it, vi } from "vitest";
import { renderToStaticMarkup } from "react-dom/server";
import { CommandBar } from "@/features/layout/CommandBar";

const noop = vi.fn();

const props = {
  runState: "idle" as const,
  onNewProject: noop,
  onOpenProject: noop,
  onSave: noop,
  onExportBundle: noop,
  onImportBundle: noop,
  onNewFile: noop,
  onCheck: noop,
  onRun: noop,
  onBuildC: noop,
  onImportPlcopen: noop,
  onExportPlcopen: noop,
  onDeploy: noop,
  onSettings: noop
};

describe("CommandBar layout", () => {
  it("renders shadcn dropdown menus for project, build, and exchange", () => {
    const html = renderToStaticMarkup(<CommandBar {...props} />);

    expect(html).toContain("Project");
    expect(html).toContain("Build");
    expect(html).toContain("Exchange");
    expect(html).toContain('data-slot="dropdown-menu-trigger"');
    expect(html).toContain("Run");
    expect(html).toContain('aria-label="Settings"');
  });
});
