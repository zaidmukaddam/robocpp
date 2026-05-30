import { useState } from "react";
import { Button } from "@/components/ui/button";
import { WatchValueDialog, type WatchDialogMode } from "@/features/dialogs/WatchValueDialog";
import type { DebugTrace, DocumentSymbol } from "@/types";
import type { ForcedValue } from "@/stores/forcedValuesStore";
import type { TargetIoValue } from "@/features/target/targetBridgeClient";
import { formatIoValue } from "@/features/target/targetBridgeClient";
import { parseWatchList } from "@/stores/settingsStore";

type WatchPanelProps = {
  watchVariables: string;
  symbols: DocumentSymbol[];
  debugTrace: DebugTrace | null;
  liveIoBySymbol?: Map<string, TargetIoValue>;
  hardwareOnline?: boolean;
  forcedValues: ForcedValue[];
  onAddWatch: (name: string) => void;
  onRemoveWatch: (name: string) => void;
  onForceValue: (name: string, value: string, persistent: boolean) => void;
  onUnforceValue: (name: string) => void;
};

function symbolType(symbols: DocumentSymbol[], name: string): string {
  const symbol = symbols.find((entry) => entry.name === name);
  if (!symbol) {
    return "—";
  }
  const match = symbol.detail.match(/:\s*([A-Za-z0-9_]+)/);
  return match?.[1] ?? symbol.kind;
}

export function WatchPanel({
  watchVariables,
  symbols,
  debugTrace,
  liveIoBySymbol,
  hardwareOnline = false,
  forcedValues,
  onAddWatch,
  onRemoveWatch,
  onForceValue,
  onUnforceValue
}: WatchPanelProps) {
  const watches = parseWatchList(watchVariables);
  const latestCycle = debugTrace?.cycles.at(-1);
  const forcedNames = new Set(forcedValues.map((entry) => entry.name));
  const [dialogMode, setDialogMode] = useState<WatchDialogMode | null>(null);
  const [dialogName, setDialogName] = useState("");
  const [dialogValue, setDialogValue] = useState("");

  const openDialog = (mode: WatchDialogMode, name = "", value = "") => {
    setDialogMode(mode);
    setDialogName(name);
    setDialogValue(value);
  };

  const closeDialog = () => {
    setDialogMode(null);
    setDialogName("");
    setDialogValue("");
  };

  return (
    <div className="watch-panel">
      <WatchValueDialog
        open={dialogMode !== null}
        mode={dialogMode ?? "add"}
        variableName={dialogName}
        iecType={symbolType(symbols, dialogName)}
        initialValue={dialogValue}
        symbols={symbols}
        onOpenChange={(open) => {
          if (!open) {
            closeDialog();
          }
        }}
        onSubmit={(name, value, persistent) => {
          if (dialogMode === "add") {
            onAddWatch(name);
            if (value) {
              onForceValue(name, value, persistent);
            }
            return;
          }
          onForceValue(name, value, persistent);
        }}
      />
      <div className="panel-inline-actions">
        <button type="button" className="panel-action-btn" onClick={() => openDialog("add")}>
          Add watch
        </button>
        {forcedValues.length > 0 ? (
          <span className="watch-forced-summary">{forcedValues.length} forced</span>
        ) : null}
      </div>
      <div className="watch-table">
        <div className="table-head watch-table-head">
          <span>Name</span>
          <span>Type</span>
          <span>Value</span>
          <span>Prepared</span>
          <span>Quality</span>
          <span>Updated</span>
          <span />
        </div>
        {watches.length === 0 ? (
          <div className="empty-row">
            {hardwareOnline
              ? "No watches yet. Add mapped symbols to monitor live target I/O."
              : "No watches yet. Run F5, then add variables from the inspector or right-click an identifier in the editor."}
          </div>
        ) : (
          watches.map((name) => {
            const liveIo = liveIoBySymbol?.get(name.toUpperCase());
            const watch = latestCycle?.watches.find((entry) => entry.name === name);
            const variable = latestCycle?.variables.find((entry) => entry.name === name);
            const forced = forcedValues.find((entry) => entry.name === name);
            const value = forced
              ? forced.preparedValue
              : liveIo
                ? formatIoValue(liveIo.value)
                : watch?.value ?? variable?.value;
            const quality = hardwareOnline
              ? liveIo
                ? "good"
                : "stale"
              : latestCycle
                ? watch || variable
                  ? "good"
                  : "stale"
                : "idle";
            const updatedAt = hardwareOnline
              ? "live"
              : latestCycle
                ? new Date(latestCycle.recordedAt).toLocaleTimeString(undefined, {
                    hour: "2-digit",
                    minute: "2-digit",
                    second: "2-digit"
                  })
                : "—";
            return (
              <div className={`table-row watch-table-row${forced ? " forced" : ""}`} key={name}>
                <span>{name}</span>
                <span>{symbolType(symbols, name)}</span>
                <span>{value === undefined ? "—" : String(value)}</span>
                <span>{forced?.preparedValue ?? "—"}</span>
                <span className={`watch-quality watch-quality-${quality}`}>{quality}</span>
                <span>{updatedAt}</span>
                <div className="watch-row-actions">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => openDialog("write", name, String(value ?? ""))}
                  >
                    Write
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => openDialog("force", name, String(value ?? ""))}
                  >
                    Force
                  </Button>
                  {forcedNames.has(name) ? (
                    <Button type="button" variant="ghost" size="sm" onClick={() => onUnforceValue(name)}>
                      Unforce
                    </Button>
                  ) : null}
                  <Button type="button" variant="ghost" size="sm" onClick={() => onRemoveWatch(name)}>
                    Remove
                  </Button>
                </div>
              </div>
            );
          })
        )}
      </div>
      {forcedValues.length > 0 ? (
        <div className="forced-values-block" aria-label="All forced values">
          <h3>Forced values</h3>
          <ul>
            {forcedValues.map((entry) => (
              <li key={entry.name}>
                <span>{entry.name}</span>
                <span>{entry.preparedValue}</span>
                <span>{entry.persistent ? "persistent" : "one-shot"}</span>
                <button type="button" className="panel-action-btn" onClick={() => onUnforceValue(entry.name)}>
                  Unforce
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </div>
  );
}
