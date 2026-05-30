import type { DebugTrace, GeneratedCArtifact } from "@/types";

export function SimulatorTrace({ trace }: { trace: DebugTrace | null }) {
  if (!trace || trace.cycles.length === 0) {
    return <div className="empty-row">Press Run (F5) to simulate the active document.</div>;
  }

  return (
    <div className="simulator-layout">
      {trace.cycles.map((cycle) => (
        <div className="trace-cycle" key={cycle.cycle}>
          <div className="trace-cycle-head">
            <strong>Cycle {cycle.cycle}</strong>
            {cycle.activeSfcSteps.length > 0 ? (
              <span className="trace-chip sfc">SFC: {cycle.activeSfcSteps.join(", ")}</span>
            ) : null}
          </div>

          {cycle.watches.length > 0 ? (
            <div className="trace-section">
              <span className="trace-section-label">Watches</span>
              <div className="trace-pill-row">
                {cycle.watches.map((variable) => (
                  <span className="trace-pill watch" key={`watch-${variable.name}`}>
                    {variable.name}={formatValue(variable.value)}
                  </span>
                ))}
              </div>
            </div>
          ) : null}

          <div className="trace-section">
            <span className="trace-section-label">Variables</span>
            <div className="trace-pill-row">
              {cycle.variables.map((variable) => (
                <span className="trace-pill" key={variable.name}>
                  {variable.name}={formatValue(variable.value)}
                </span>
              ))}
            </div>
          </div>

          {cycle.accessPaths.length > 0 ? (
            <div className="trace-section">
              <span className="trace-section-label">Access paths</span>
              <div className="access-path-table">
                <div className="table-head access-path-head">
                  <span>Name</span>
                  <span>Target</span>
                  <span>Direction</span>
                  <span>Value</span>
                </div>
                {cycle.accessPaths.map((access) => (
                  <div className="table-row access-path-row" key={access.name}>
                    <span>{access.name}</span>
                    <span>{access.target}</span>
                    <span>{access.direction}</span>
                    <span>{access.value === null ? "—" : formatValue(access.value)}</span>
                  </div>
                ))}
              </div>
            </div>
          ) : null}

          {cycle.events.length > 0 ? <small className="trace-events">{cycle.events.join(", ")}</small> : null}
        </div>
      ))}
    </div>
  );
}

export function GeneratedCView({
  artifact,
  outputPath
}: {
  artifact: GeneratedCArtifact | null;
  outputPath: string;
}) {
  if (!artifact) {
    return (
      <div className="generated-c-layout">
        <div className="empty-row">Run Build C or simulate first to populate generated output.</div>
      </div>
    );
  }

  const { metadata } = artifact;

  return (
    <div className="generated-c-layout">
      <aside className="generated-c-meta" aria-label="Generated C metadata">
        <MetaSection title="Output" rows={[[outputPath || metadata.filenameHint, "path"]]} />
        <MetaSection
          title="Scan entrypoints"
          rows={metadata.scanEntrypoints.map((entry) => [entry.name, entry.signature])}
        />
        <MetaSection
          title="State layout"
          rows={metadata.stateLayout.map((field) => [
            field.name,
            `${field.typeName}${field.retained ? " · retained" : ""}`
          ])}
        />
        <MetaSection
          title="I/O symbols"
          rows={metadata.ioSymbols.map((symbol) => [
            symbol.name,
            `${symbol.direction} @ ${symbol.location}`
          ])}
        />
        <MetaSection
          title="Access paths"
          rows={metadata.accessPaths.map((path) => [path.name, `${path.direction} → ${path.target}`])}
        />
        {metadata.retainedFields.length > 0 ? (
          <MetaSection title="Retained fields" rows={metadata.retainedFields.map((field) => [field, ""])} />
        ) : null}
        {metadata.targetHooks.length > 0 ? (
          <MetaSection title="Target hooks" rows={metadata.targetHooks.map((hook) => [hook, ""])} />
        ) : null}
      </aside>
      <pre className="generated-c">{artifact.source}</pre>
    </div>
  );
}

function MetaSection({ title, rows }: { title: string; rows: [string, string][] }) {
  if (rows.length === 0) {
    return null;
  }

  return (
    <section className="meta-section">
      <h3>{title}</h3>
      <div className="meta-rows">
        {rows.map(([primary, secondary]) => (
          <div className="meta-row" key={`${title}-${primary}`}>
            <span>{primary}</span>
            {secondary ? <small>{secondary}</small> : null}
          </div>
        ))}
      </div>
    </section>
  );
}

function formatValue(value: string | number | boolean): string {
  if (typeof value === "string" && (value.startsWith("\"") || value === "true" || value === "false")) {
    return value;
  }
  return String(value);
}
