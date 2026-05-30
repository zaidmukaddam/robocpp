import { describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { BottomPanel } from "@/features/panels/BottomPanel";
import { createSampleProject } from "@/features/project/projectStore";

describe("accessibility structure", () => {
  it("exposes tablist semantics on the bottom panel", () => {
    render(
      <BottomPanel
        activePanel="Diagnostics"
        setActivePanel={() => undefined}
        diagnostics={[]}
        errorCount={0}
        warningCount={0}
        noteCount={0}
        debugTrace={null}
        cArtifact={null}
        generatedCOutputPath="generated/"
        commandLog={[]}
        logFilter="all"
        onLogFilterChange={() => undefined}
        watchVariables=""
        symbols={[]}
        forcedValues={[]}
        onAddWatch={() => undefined}
        onRemoveWatch={() => undefined}
        onForceWatchValue={() => undefined}
        onUnforceWatchValue={() => undefined}
        onApplyQuickFix={() => undefined}
        project={createSampleProject()}
        artifacts={[]}
        selectedArtifact={null}
        onSelectArtifact={() => undefined}
        onDeleteArtifact={() => undefined}
        onRenameArtifact={() => undefined}
        onClearArtifacts={() => undefined}
        onRevealSource={() => undefined}
        trendSeries={[]}
        trendRecording={false}
        onToggleTrendRecording={() => undefined}
        onClearTrends={() => undefined}
        onExportTrends={() => undefined}
        onJumpToDiagnostic={() => undefined}
        open
        onToggle={() => undefined}
      />
    );
    expect(screen.getByRole("tablist", { name: "Output views" })).toBeTruthy();
    expect(screen.getByRole("tab", { name: "Trends" })).toBeTruthy();
  });
});
