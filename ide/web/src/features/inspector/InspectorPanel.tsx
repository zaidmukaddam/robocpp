import { Braces, Crosshair, Cpu, Search, Sparkles } from "lucide-react";
import type { InspectorTab } from "@/app/types";
import type { CompletionItem, DocumentSymbol } from "@/types";
import { TargetInspectorPanel, type TargetInspectorPanelProps } from "@/features/inspector/TargetInspectorPanel";

type InspectorPanelProps = {
  tab: InspectorTab;
  onTabChange: (tab: InspectorTab) => void;
  symbolQuery: string;
  onSymbolQueryChange: (query: string) => void;
  filteredSymbols: DocumentSymbol[];
  selectedSymbol: DocumentSymbol | null;
  onSelectSymbol: (symbol: DocumentSymbol) => void;
  onAddWatch: (name: string) => void;
  completions: CompletionItem[];
  hoverSymbol: DocumentSymbol | null;
  targetProps: TargetInspectorPanelProps;
};

const INSPECTOR_TABS: {
  id: InspectorTab;
  label: string;
  shortLabel: string;
  icon: typeof Braces;
  panelId: string;
}[] = [
  { id: "symbols", label: "Symbols", shortLabel: "Sym", icon: Braces, panelId: "inspector-panel-symbols" },
  { id: "completions", label: "Completions", shortLabel: "Cmp", icon: Sparkles, panelId: "inspector-panel-completions" },
  { id: "hover", label: "Hover", shortLabel: "Hov", icon: Crosshair, panelId: "inspector-panel-hover" },
  { id: "target", label: "Target", shortLabel: "Tgt", icon: Cpu, panelId: "inspector-panel-target" }
];

function kindTone(kind: string): string {
  const normalized = kind.toLowerCase();
  if (normalized.includes("program")) return "kind-program";
  if (normalized.includes("function")) return "kind-function";
  if (normalized.includes("var")) return "kind-variable";
  if (normalized.includes("type")) return "kind-type";
  return "kind-default";
}

export function InspectorPanel({
  tab,
  onTabChange,
  symbolQuery,
  onSymbolQueryChange,
  filteredSymbols,
  selectedSymbol,
  onSelectSymbol,
  onAddWatch,
  completions,
  hoverSymbol,
  targetProps
}: InspectorPanelProps) {
  const activeTab = INSPECTOR_TABS.find((entry) => entry.id === tab) ?? INSPECTOR_TABS[0];

  return (
    <div className="inspector-shell">
      <div className="inspector-tabs" role="tablist" aria-label="Inspector views">
        {INSPECTOR_TABS.map((entry) => {
          const Icon = entry.icon;
          const selected = tab === entry.id;
          const count =
            entry.id === "symbols"
              ? filteredSymbols.length
              : entry.id === "completions"
                ? completions.length
                : null;

          return (
            <button
              key={entry.id}
              type="button"
              role="tab"
              id={`tab-${entry.panelId}`}
              aria-selected={selected}
              aria-controls={entry.panelId}
              tabIndex={selected ? 0 : -1}
              className={selected ? "selected" : ""}
              title={entry.label}
              onClick={() => onTabChange(entry.id)}
            >
              <Icon size={13} aria-hidden="true" />
              <span className="inspector-tab-label">{entry.label}</span>
              <span className="inspector-tab-short">{entry.shortLabel}</span>
              {count !== null ? <span className="inspector-tab-count tabular-nums">{count}</span> : null}
            </button>
          );
        })}
      </div>

      {tab === "symbols" ? (
        <div
          id={activeTab.panelId}
          role="tabpanel"
          aria-labelledby={`tab-${activeTab.panelId}`}
          className="inspector-panel-body"
        >
          <div className="inspector-search-wrap">
            <Search size={13} className="inspector-search-icon" aria-hidden="true" />
            <label className="sr-only" htmlFor="symbol-search">
              Search symbols
            </label>
            <input
              id="symbol-search"
              className="search-box inspector-search"
              type="search"
              name="symbol-search"
              placeholder="Filter symbols…"
              spellCheck={false}
              autoComplete="off"
              enterKeyHint="search"
              value={symbolQuery}
              onChange={(event) => onSymbolQueryChange(event.target.value)}
            />
          </div>
          <section aria-label="Symbol list" className="inspector-section">
            <h2>Symbols</h2>
            <div className="symbol-list">
              {filteredSymbols.length === 0 ? (
                <div className="empty-row inspector-empty">
                  {symbolQuery.trim() ? "No symbols match your filter." : "No symbols in this document."}
                </div>
              ) : (
                filteredSymbols.map((symbol) => {
                  const typeText = symbol.detail.includes(" : ")
                    ? symbol.detail.slice(symbol.detail.lastIndexOf(" : ") + 3)
                    : symbol.detail;
                  const line = (symbol.range?.startPosition.line ?? 0) + 1;

                  return (
                    <div className="symbol-row-wrap" key={`${symbol.kind}-${symbol.name}`}>
                      <button
                        type="button"
                        className={`symbol-row ${selectedSymbol?.name === symbol.name ? "selected" : ""}`}
                        onClick={() => onSelectSymbol(symbol)}
                      >
                        <span className="symbol-row-main">
                          <span className={`symbol-kind ${kindTone(symbol.kind)}`}>{symbol.kind}</span>
                          <span className="symbol-name">{symbol.name}</span>
                        </span>
                        <span className="symbol-type">{typeText}</span>
                        <small className="symbol-meta">
                          {symbol.containerName ? `${symbol.containerName} · ` : ""}
                          <span className="tabular-nums">L{line}</span>
                        </small>
                      </button>
                      <button
                        type="button"
                        className="symbol-watch-btn"
                        aria-label={`Add ${symbol.name} to watches`}
                        title="Add to watches"
                        onClick={() => onAddWatch(symbol.name)}
                      >
                        +
                      </button>
                    </div>
                  );
                })
              )}
            </div>
          </section>
        </div>
      ) : null}

      {tab === "completions" ? (
        <div
          id="inspector-panel-completions"
          role="tabpanel"
          aria-labelledby="tab-inspector-panel-completions"
          className="inspector-panel-body"
        >
          <section aria-label="Completions" className="inspector-section">
            <h2>Completions</h2>
            <div className="completion-list">
              {completions.length === 0 ? (
                <div className="empty-row inspector-empty">No completions for this document.</div>
              ) : (
                completions.slice(0, 40).map((item) => (
                  <div className="completion-row" key={`${item.kind}-${item.label}`}>
                    <span className="completion-row-main">
                      <span className={`symbol-kind ${kindTone(item.kind)}`}>{item.kind}</span>
                      <span className="completion-label">{item.label}</span>
                    </span>
                    <span className="completion-detail">{item.detail}</span>
                  </div>
                ))
              )}
            </div>
          </section>
        </div>
      ) : null}

      {tab === "hover" ? (
        <div
          id="inspector-panel-hover"
          role="tabpanel"
          aria-labelledby="tab-inspector-panel-hover"
          className="inspector-panel-body"
        >
          <section aria-label="Hover preview" className="inspector-section">
            <h2>Hover</h2>
            <div className="hover-card">
              {hoverSymbol ? (
                <>
                  <div className="hover-card-head">
                    <span className={`symbol-kind ${kindTone(hoverSymbol.kind)}`}>{hoverSymbol.kind}</span>
                    <strong>{hoverSymbol.name}</strong>
                  </div>
                  <span className="hover-type">{hoverSymbol.detail}</span>
                  <small>
                    {hoverSymbol.containerName ? `${hoverSymbol.containerName} · ` : ""}
                    Line <span className="tabular-nums">{(hoverSymbol.range?.startPosition.line ?? 0) + 1}</span>
                  </small>
                </>
              ) : (
                <span className="inspector-empty-inline">Place the cursor on a symbol to preview details.</span>
              )}
            </div>
          </section>
        </div>
      ) : null}

      {tab === "target" ? (
        <div
          id="inspector-panel-target"
          role="tabpanel"
          aria-labelledby="tab-inspector-panel-target"
          className="inspector-panel-body inspector-target-body"
        >
          <TargetInspectorPanel {...targetProps} />
        </div>
      ) : null}
    </div>
  );
}
