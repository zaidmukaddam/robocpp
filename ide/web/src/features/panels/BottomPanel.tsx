import { ChevronDown, ChevronUp } from "lucide-react";
import { ArtifactPanel } from "@/features/panels/ArtifactPanel";
import { GeneratedCView, SimulatorTrace } from "@/features/panels/SimulatorPanels";
import { TraceTrendPanel } from "@/features/panels/TraceTrendPanel";
import { WatchPanel } from "@/features/panels/WatchPanel";
import type { TrendSeries } from "@/lib/traceTrend";
import { quickFixesForDiagnostic } from "@/lib/diagnosticQuickFixes";
import type { ForcedValue } from "@/stores/forcedValuesStore";
import type { LogEntry, OutputPanel } from "@/app/types";
import type {
  Analysis,
  DebugTrace,
  Diagnostic,
  DocumentSymbol,
  GeneratedCArtifact,
  Project,
  ProjectArtifact
} from "@/types";

import type { TargetIoValue } from "@/features/target/targetBridgeClient";

export type BottomPanelProps = {
  activePanel: OutputPanel;
  setActivePanel: (panel: OutputPanel) => void;
  diagnostics: Analysis["diagnostics"];
  errorCount: number;
  warningCount: number;
  noteCount: number;
  debugTrace: DebugTrace | null;
  cArtifact: GeneratedCArtifact | null;
  generatedCOutputPath: string;
  commandLog: LogEntry[];
  logFilter: "all" | LogEntry["kind"];
  onLogFilterChange: (filter: "all" | LogEntry["kind"]) => void;
  watchVariables: string;
  symbols: DocumentSymbol[];
  forcedValues: ForcedValue[];
  liveIoBySymbol?: Map<string, TargetIoValue>;
  hardwareOnline?: boolean;
  onAddWatch: (name: string) => void;
  onRemoveWatch: (name: string) => void;
  onForceWatchValue: (name: string, value: string, persistent: boolean) => void;
  onUnforceWatchValue: (name: string) => void;
  onApplyQuickFix: (diagnostic: Diagnostic, fixId: string) => void;
  project: Project;
  artifacts: ProjectArtifact[];
  selectedArtifact: ProjectArtifact | null;
  onSelectArtifact: (artifactId: string | null) => void;
  onDeleteArtifact: (artifactId: string) => void;
  onClearArtifacts: () => void;
  onRevealSource: (fileName: string) => void;
  onRenameArtifact: (artifactId: string, nextName: string) => void;
  trendSeries: TrendSeries[];
  trendRecording: boolean;
  onToggleTrendRecording: () => void;
  onClearTrends: () => void;
  onExportTrends: () => void;
  onJumpToDiagnostic: (diagnostic: Diagnostic) => void;
  onExportTrace?: () => void;
  open: boolean;
  onToggle: () => void;
};

export function BottomPanel({
  activePanel,
  setActivePanel,
  diagnostics,
  errorCount,
  warningCount,
  noteCount,
  debugTrace,
  cArtifact,
  generatedCOutputPath,
  commandLog,
  logFilter,
  onLogFilterChange,
  watchVariables,
  symbols,
  forcedValues,
  liveIoBySymbol,
  hardwareOnline,
  onAddWatch,
  onRemoveWatch,
  onForceWatchValue,
  onUnforceWatchValue,
  onApplyQuickFix,
  project,
  artifacts,
  selectedArtifact,
  onSelectArtifact,
  onDeleteArtifact,
  onClearArtifacts,
  onRevealSource,
  onRenameArtifact,
  trendSeries,
  trendRecording,
  onToggleTrendRecording,
  onClearTrends,
  onExportTrends,
  onJumpToDiagnostic,
  onExportTrace,
  open,
  onToggle
}: BottomPanelProps) {
  const panels: OutputPanel[] = ["Diagnostics", "Scan Trace", "Trends", "Watches", "Generated C", "Artifacts", "Output"];
  const filteredLog =
    logFilter === "all" ? commandLog : commandLog.filter((entry) => entry.kind === logFilter);

  return (
    <section className={`bottom-panel ${open ? "" : "collapsed"}`} aria-label="Output panel">
      <div className="panel-header">
        <div className="panel-tabs" role="tablist" aria-label="Output views">
          {panels.map((panel) => (
            <button
              key={panel}
              type="button"
              role="tab"
              aria-selected={panel === activePanel}
              className={panel === activePanel ? "selected" : ""}
              onClick={() => {
                setActivePanel(panel);
                if (!open) {
                  onToggle();
                }
              }}
            >
              {panel}
            </button>
          ))}
        </div>
        <div className="panel-header-actions">
          {activePanel === "Diagnostics" ? (
            <div className="diagnostic-chips">
              <span className="diagnostic-chip">{diagnostics.length}</span>
              {errorCount > 0 ? <span className="diagnostic-chip error">{errorCount}</span> : null}
              {warningCount > 0 ? <span className="diagnostic-chip warning">{warningCount}</span> : null}
              {noteCount > 0 ? <span className="diagnostic-chip note">{noteCount}</span> : null}
            </div>
          ) : null}
          {activePanel === "Output" ? (
            <label className="log-filter">
              <span className="sr-only">Filter output log</span>
              <select value={logFilter} onChange={(event) => onLogFilterChange(event.target.value as typeof logFilter)}>
                <option value="all">All</option>
                <option value="action">Actions</option>
                <option value="info">Info</option>
                <option value="error">Errors</option>
              </select>
            </label>
          ) : null}
          <button
            type="button"
            className="panel-toggle-btn"
            aria-label={open ? "Collapse panel" : "Expand panel"}
            aria-expanded={open}
            onClick={onToggle}
          >
            {open ? <ChevronDown size={14} aria-hidden="true" /> : <ChevronUp size={14} aria-hidden="true" />}
          </button>
        </div>
      </div>
      {open ? (
        activePanel === "Diagnostics" ? (
          <div className="diagnostics-table">
            <div className="table-head">
              <span>Severity</span>
              <span>Code</span>
              <span>Message</span>
              <span>Location</span>
            </div>
            {diagnostics.length === 0 ? (
              <div className="empty-row">No diagnostics. Run Check (F7) to analyze the active file.</div>
            ) : (
              diagnostics.map((diagnostic) => {
                const fixes = quickFixesForDiagnostic(diagnostic);
                return (
                  <div
                    className="diagnostic-row-wrap"
                    key={`${diagnostic.stableCode}-${diagnostic.message}-${diagnostic.span?.line ?? 0}`}
                  >
                    <button
                      type="button"
                      className="table-row diagnostic-row"
                      onClick={() => onJumpToDiagnostic(diagnostic)}
                      disabled={!diagnostic.span}
                    >
                      <span className={`severity ${diagnostic.severity}`}>{diagnostic.severity}</span>
                      <span>{diagnostic.stableCode}</span>
                      <span>{diagnostic.message}</span>
                      <span>{diagnostic.span ? `${diagnostic.span.line}:${diagnostic.span.column}` : "—"}</span>
                      {diagnostic.help ? <span className="help-text">{diagnostic.help}</span> : null}
                    </button>
                    {fixes.length > 0 ? (
                      <div className="diagnostic-quick-fixes">
                        {fixes.map((fix) => (
                          <button
                            key={fix.id}
                            type="button"
                            className="diagnostic-quick-fix-btn"
                            onClick={() => onApplyQuickFix(diagnostic, fix.id)}
                          >
                            {fix.label}
                          </button>
                        ))}
                      </div>
                    ) : null}
                  </div>
                );
              })
            )}
          </div>
        ) : activePanel === "Scan Trace" ? (
          <div className="trace-table">
            {onExportTrace ? (
              <div className="panel-inline-actions">
                <button type="button" className="panel-action-btn" onClick={onExportTrace}>
                  Export trace JSON
                </button>
              </div>
            ) : null}
            <SimulatorTrace trace={debugTrace} />
          </div>
        ) : activePanel === "Trends" ? (
          <TraceTrendPanel
            series={trendSeries}
            recording={trendRecording}
            onToggleRecording={onToggleTrendRecording}
            onClear={onClearTrends}
            onExport={onExportTrends}
          />
        ) : activePanel === "Watches" ? (
          <WatchPanel
            watchVariables={watchVariables}
            symbols={symbols}
            debugTrace={debugTrace}
            liveIoBySymbol={liveIoBySymbol}
            hardwareOnline={hardwareOnline}
            forcedValues={forcedValues}
            onAddWatch={onAddWatch}
            onRemoveWatch={onRemoveWatch}
            onForceValue={onForceWatchValue}
            onUnforceValue={onUnforceWatchValue}
          />
        ) : activePanel === "Generated C" ? (
          <GeneratedCView artifact={cArtifact} outputPath={generatedCOutputPath} />
        ) : activePanel === "Artifacts" ? (
          <ArtifactPanel
            project={project}
            artifacts={artifacts}
            selectedArtifact={selectedArtifact}
            onSelectArtifact={onSelectArtifact}
            onDeleteArtifact={onDeleteArtifact}
            onClearArtifacts={onClearArtifacts}
            onRevealSource={onRevealSource}
            onRenameArtifact={onRenameArtifact}
          />
        ) : (
          <pre className="output-log" aria-live="polite">
            {filteredLog.map((entry) => `[${entry.time}] ${entry.message}`).join("\n")}
          </pre>
        )
      ) : null}
    </section>
  );
}
