import type { TrendSeries } from "@/lib/traceTrend";
import { trendSparkline } from "@/lib/traceTrend";

type TraceTrendPanelProps = {
  series: TrendSeries[];
  recording: boolean;
  onToggleRecording: () => void;
  onClear: () => void;
  onExport: () => void;
};

export function TraceTrendPanel({
  series,
  recording,
  onToggleRecording,
  onClear,
  onExport
}: TraceTrendPanelProps) {
  return (
    <div className="trace-trend-panel">
      <div className="panel-inline-actions">
        <button type="button" className={`panel-action-btn${recording ? " active" : ""}`} onClick={onToggleRecording}>
          {recording ? "Stop recording" : "Start recording"}
        </button>
        <button type="button" className="panel-action-btn" disabled={series.length === 0} onClick={onExport}>
          Export trends JSON
        </button>
        <button type="button" className="panel-action-btn" disabled={series.length === 0} onClick={onClear}>
          Clear trends
        </button>
      </div>
      {series.length === 0 ? (
        <div className="empty-row">Record simulation runs to build trend series for watched variables.</div>
      ) : (
        <div className="trend-table">
          <div className="table-head trend-table-head">
            <span>Variable</span>
            <span>Points</span>
            <span>Latest</span>
            <span>Trend</span>
          </div>
          {series.map((entry) => {
            const latest = entry.points.at(-1);
            const spark = trendSparkline(entry.points);
            return (
              <div className="table-row trend-table-row" key={entry.name}>
                <span>{entry.name}</span>
                <span>{entry.points.length}</span>
                <span>{latest ? String(latest.value) : "—"}</span>
                <span className="trend-sparkline-cell">
                  {spark ? (
                    <svg viewBox="0 0 120 24" width="120" height="24" aria-hidden="true">
                      <polyline fill="none" stroke="currentColor" strokeWidth="1.5" points={spark} />
                    </svg>
                  ) : (
                    "—"
                  )}
                </span>
              </div>
            );
          })}
        </div>
      )}
    </div>
  );
}
