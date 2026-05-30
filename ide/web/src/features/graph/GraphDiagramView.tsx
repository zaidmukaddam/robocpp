import { Fragment, memo, useMemo, useRef, useState, type KeyboardEvent } from "react";
import { Button } from "@/components/ui/button";
import { RenameDialog } from "@/features/dialogs/RenameDialog";
import { GraphCanvas } from "@/features/graph/GraphCanvas";
import type { DebugTrace, RunTrace, WorkspaceFile } from "@/types";
import type { GraphModel, GraphSelection } from "@/features/graph/graphTypes";
import type { GraphValidation } from "@/features/graph/validateGraph";
import { networkIndexFromSelection, selectionLabel, type GraphEditAction } from "@/features/graph/graphEdits";
import { GraphPropertyPanel } from "@/features/graph/GraphPropertyPanel";
import { createGraphDocument } from "@/features/graph/graphDocument";
import { activeSfcSteps, buildTraceLabelSet, buildTraceValueMap } from "@/features/graph/graphTrace";
import { graphStaleWarnings } from "@/lib/graphStaleSource";
import { enrichGraphModelForDisplay } from "@/features/graph/graphDisplayModel";
import { computeLdPowerFlow } from "@/features/graph/graphPowerFlow";

type GraphDiagramViewProps = {
  file: WorkspaceFile;
  model: GraphModel | null;
  validation: GraphValidation | null;
  runTrace: RunTrace | null;
  debugTrace: DebugTrace | null;
  canUndo: boolean;
  canRedo: boolean;
  onChange: (text: string) => void;
  onUndo: () => void;
  onRedo: () => void;
};

function traceValue(traceLabels: Set<string>, label: string | null): boolean {
  return Boolean(label && traceLabels.has(label));
}

export function GraphDiagramView({
  file,
  model,
  validation,
  runTrace,
  debugTrace,
  canUndo,
  canRedo,
  onChange,
  onUndo,
  onRedo
}: GraphDiagramViewProps) {
  const [selection, setSelection] = useState<GraphSelection | null>(null);
  const [connectSource, setConnectSource] = useState<GraphSelection | null>(null);
  const [renameLabel, setRenameLabel] = useState<string | null>(null);
  const clipboardRef = useRef<string | null>(null);
  const displayModel = useMemo(() => enrichGraphModelForDisplay(file, model), [file, model]);
  const pou = displayModel?.pous[0] ?? null;
  const traceLabels = useMemo(() => buildTraceLabelSet(runTrace, debugTrace), [runTrace, debugTrace]);
  const traceValues = useMemo(() => buildTraceValueMap(runTrace, debugTrace), [runTrace, debugTrace]);
  const activeSteps = useMemo(() => activeSfcSteps(debugTrace), [debugTrace]);
  const staleWarnings = useMemo(() => graphStaleWarnings(file, model), [file, model]);
  const powerFlow = useMemo(
    () => (model ? computeLdPowerFlow(model, runTrace, debugTrace) : new Map<string, boolean>()),
    [model, runTrace, debugTrace]
  );

  const selectedLabel = useMemo(() => {
    if (!model) {
      return null;
    }
    return selectionLabel(file, model, selection);
  }, [file, model, selection]);

  const selectedNetworkIndex = useMemo(
    () => (model ? networkIndexFromSelection(model, selection) : null),
    [model, selection]
  );

  if (!model || !pou) {
    return <p className="diagram-empty">No graphical model available for this document.</p>;
  }

  const apply = (action: GraphEditAction, payload?: string) => {
    const document = createGraphDocument(file, model, validation);
    const patch = document.apply(action, payload, selection);
    if (patch) {
      onChange(patch.nextText);
    }
  };

  const handleRename = () => {
    if (!selectedLabel) {
      return;
    }
    setRenameLabel(selectedLabel);
  };

  const submitRename = (nextLabel: string) => {
    if (!selectedLabel) {
      return;
    }
    apply("rename", `${selectedLabel}->${nextLabel}`);
    setRenameLabel(null);
  };

  const handleNodeSelect = (next: GraphSelection) => {
    if (
      connectSource &&
      connectSource.kind === "node" &&
      next.kind === "node" &&
      (file.languageId === "fbd" || file.languageId === "xml")
    ) {
      if (file.languageId === "xml") {
        if (connectSource.stableId !== next.stableId) {
          apply("connect", `${connectSource.stableId}->${next.stableId}`);
        }
      } else {
        const sourceLabel = selectionLabel(file, model, connectSource);
        const targetLabel = selectionLabel(file, model, next);
        if (sourceLabel && targetLabel && sourceLabel !== targetLabel) {
          apply("connect", `${sourceLabel}->${targetLabel}`);
        }
      }
      setConnectSource(null);
      setSelection(next);
      return;
    }
    setSelection(next);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const mod = event.metaKey || event.ctrlKey;
    if (mod && event.key.toLowerCase() === "z") {
      event.preventDefault();
      if (event.shiftKey) {
        onRedo();
      } else {
        onUndo();
      }
      return;
    }
    if (mod && event.key.toLowerCase() === "y") {
      event.preventDefault();
      onRedo();
      return;
    }
    if (mod && event.key.toLowerCase() === "c" && selectedLabel) {
      event.preventDefault();
      clipboardRef.current = selectedLabel;
      return;
    }
    if (mod && event.key.toLowerCase() === "v" && clipboardRef.current) {
      event.preventDefault();
      apply("duplicate-selected", clipboardRef.current);
      return;
    }
    if ((event.key === "Delete" || event.key === "Backspace") && selectedLabel) {
      event.preventDefault();
      apply("delete-selected", selectedLabel);
      setSelection(null);
      return;
    }
    if (event.key === "Escape") {
      setConnectSource(null);
      setSelection(null);
    }
  };

  const validationIssues = validation?.diagnostics ?? [];

  return (
    <div className="graph-diagram-shell" tabIndex={0} onKeyDown={onKeyDown} aria-label="Diagram editor">
      <RenameDialog
        open={renameLabel !== null}
        title="Rename diagram element"
        description="Update the label used in the source representation."
        currentName={renameLabel ?? ""}
        fieldLabel="Label"
        validate={(next) => (/^[A-Za-z_][A-Za-z0-9_]*$/.test(next) ? null : "Use a valid identifier.")}
        onOpenChange={(open) => {
          if (!open) {
            setRenameLabel(null);
          }
        }}
        onSubmit={submitRename}
      />
      <div className="graph-diagram-header">
        <div className="graph-toolbar" role="toolbar" aria-label="Diagram editing">
          {file.languageId === "ld" ? (
            <>
              <Button type="button" size="sm" variant="secondary" onClick={() => apply("add-rung")}>
                Add rung
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={!selectedLabel}
                onClick={() => apply("add-contact", selectedLabel ?? undefined)}
              >
                Add contact
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={!selectedLabel}
                onClick={() => apply("add-negated-contact", selectedLabel ?? undefined)}
              >
                Add NOT
              </Button>
              <Button type="button" size="sm" variant="ghost" onClick={() => apply("add-coil")}>
                Add coil
              </Button>
              <Button type="button" size="sm" variant="ghost" onClick={() => apply("add-set-coil")}>
                Add SET
              </Button>
              <Button type="button" size="sm" variant="ghost" onClick={() => apply("add-reset-coil")}>
                Add RESET
              </Button>
              <Button type="button" size="sm" variant="ghost" onClick={() => apply("add-branch")}>
                Add branch
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={!selectedLabel}
                onClick={() => selectedLabel && apply("toggle-edge-contact", selectedLabel)}
              >
                Toggle edge
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={selectedNetworkIndex === null}
                onClick={() => apply("move-rung", `${selectedNetworkIndex ?? 0}->up`)}
              >
                Rung up
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={selectedNetworkIndex === null}
                onClick={() => apply("move-rung", `${selectedNetworkIndex ?? 0}->down`)}
              >
                Rung down
              </Button>
            </>
          ) : null}
          {file.languageId === "fbd" || file.languageId === "xml" ? (
            <>
              <Button type="button" size="sm" variant="secondary" onClick={() => apply("add-network")}>
                Add network
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => setConnectSource(connectSource ? null : selection)}
              >
                {connectSource ? "Cancel connect" : "Connect"}
              </Button>
              <Button type="button" size="sm" variant="ghost" onClick={() => apply("add-fbd-literal")}>
                Add literal
              </Button>
            </>
          ) : null}
          {file.languageId === "sfc" ? (
            <>
              <Button type="button" size="sm" variant="secondary" onClick={() => apply("add-step")}>
                Add step
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                onClick={() => apply("add-transition", selectedLabel ?? undefined)}
              >
                Add transition
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={!selectedLabel || selection?.kind !== "step"}
                onClick={() => selectedLabel && apply("toggle-sfc-initial", selectedLabel)}
              >
                Set initial
              </Button>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                disabled={!selectedLabel || selection?.kind !== "step"}
                onClick={() => selectedLabel && apply("add-sfc-jump", selectedLabel)}
              >
                Add jump
              </Button>
            </>
          ) : null}
          <span className="graph-toolbar-divider" aria-hidden="true" />
          <Button type="button" size="sm" variant="ghost" disabled={!canUndo} onClick={onUndo}>
            Undo
          </Button>
          <Button type="button" size="sm" variant="ghost" disabled={!canRedo} onClick={onRedo}>
            Redo
          </Button>
          <Button type="button" size="sm" variant="ghost" disabled={!selectedLabel} onClick={handleRename}>
            Rename
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={!selectedLabel}
            onClick={() => selectedLabel && apply("duplicate-selected", selectedLabel)}
          >
            Duplicate
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            disabled={!selectedLabel}
            onClick={() => selectedLabel && apply("delete-selected", selectedLabel)}
          >
            Delete
          </Button>
        </div>
        {selectedLabel ? <span className="graph-selection-label">Selected: {selectedLabel}</span> : null}
      </div>

      {staleWarnings.length > 0 ? (
        <div className="graph-stale-banner" role="status">
          {staleWarnings.map((warning) => (
            <p key={warning.message} className={`graph-stale-${warning.severity}`}>
              {warning.message}
            </p>
          ))}
        </div>
      ) : null}

      {validationIssues.length > 0 ? (
        <div className="graph-validation-banner" role="status">
          {validationIssues[0]?.message}
          {validationIssues.length > 1 ? ` (+${validationIssues.length - 1} more in Diagnostics)` : null}
        </div>
      ) : null}

      <div className="graph-diagram-body">
        <GraphCanvas>
          {pou.sfc ? (
            <SfcGraphView
              sfc={pou.sfc}
              selection={selection}
              activeSteps={activeSteps}
              onSelect={setSelection}
            />
          ) : file.languageId === "xml" && model.plcopenLayout.nodeIds.length > 0 ? (
            <PlcopenGraphView
              model={model}
              selection={selection}
              traceLabels={traceLabels}
              onSelect={handleNodeSelect}
            />
          ) : (
            <NetworkGraphView
              file={file}
              networks={pou.networks}
              selection={selection}
              connectSource={connectSource}
              traceLabels={traceLabels}
              traceValues={traceValues}
              powerFlow={powerFlow}
              onSelect={handleNodeSelect}
              onReorder={(fromLabel, toLabel) => apply("reorder", `${fromLabel}->${toLabel}`)}
            />
          )}
        </GraphCanvas>
        <GraphPropertyPanel
          file={file}
          model={model}
          selection={selection}
          validation={validation}
          onApplyRename={(currentLabel, nextLabel) => apply("rename", `${currentLabel}->${nextLabel}`)}
          onApplyProperty={(property, value) => {
            if (!selectedLabel) {
              return;
            }
            if (property === "qualifier") {
              apply("rename", `${selectedLabel}->${value}`);
              return;
            }
            if (property === "comment" || property === "coil-mode") {
              apply("set-property", `${selectedLabel}:${property}:${value}`);
            }
          }}
          onMoveStep={(stepName, direction) => apply("move-step", `${stepName}->${direction}`)}
          onToggleLdContact={(label) => apply("toggle-edge-contact", label)}
        />
      </div>
    </div>
  );
}

const NetworkGraphView = memo(function NetworkGraphView({
  file,
  networks,
  selection,
  connectSource,
  traceLabels,
  traceValues,
  powerFlow,
  onSelect,
  onReorder
}: {
  file: WorkspaceFile;
  networks: GraphModel["pous"][number]["networks"];
  selection: GraphSelection | null;
  connectSource: GraphSelection | null;
  traceLabels: Set<string>;
  traceValues: Map<string, string>;
  powerFlow: Map<string, boolean>;
  onSelect: (selection: GraphSelection) => void;
  onReorder: (fromLabel: string, toLabel: string) => void;
}) {
  const dragLabelRef = useRef<string | null>(null);

  const handleDropOnNode = (targetLabel: string | null) => {
    const fromLabel = dragLabelRef.current;
    dragLabelRef.current = null;
    if (fromLabel && targetLabel && fromLabel !== targetLabel) {
      onReorder(fromLabel, targetLabel);
    }
  };

  return (
    <div className={`graph-preview-stack graph-${file.languageId}`}>
      {networks.map((network) => (
        <section key={network.id} className="graph-preview-network" aria-label={network.label ?? network.id}>
          {network.label ? <h3 className="graph-preview-network-title">{network.label}</h3> : null}
          {file.languageId === "ld" ? (
            <div className="ladder-view">
              <div className="ld-rung">
                <div className="ld-rail" aria-hidden="true" />
                {network.nodes
                  .filter((node) => node.kind !== "leftPowerRail" && node.kind !== "rightPowerRail")
                  .map((node, index, visibleNodes) => {
                  const energized = powerFlow.get(node.stableId) ?? traceValue(traceLabels, node.label);
                  const selected = selection?.kind === "node" && selection.stableId === node.stableId;
                  const connecting = connectSource?.kind === "node" && connectSource.stableId === node.stableId;
                  const className =
                    node.kind === "coil"
                      ? `ld-coil ${energized ? "energized" : ""} ${selected ? "selected" : ""} ${connecting ? "connecting" : ""}`
                      : `ld-contact ${energized ? "energized" : ""} ${selected ? "selected" : ""} ${connecting ? "connecting" : ""}`;
                  return (
                    <Fragment key={node.stableId}>
                      <button
                        type="button"
                        className={className}
                        draggable={Boolean(node.label)}
                        onDragStart={() => {
                          dragLabelRef.current = node.label;
                        }}
                        onDragOver={(event) => {
                          if (dragLabelRef.current) {
                            event.preventDefault();
                          }
                        }}
                        onDrop={(event) => {
                          event.preventDefault();
                          handleDropOnNode(node.label);
                        }}
                        onClick={() => onSelect({ kind: "node", stableId: node.stableId, networkId: network.id })}
                      >
                        {node.label}
                        {node.label && traceValues.has(node.label) ? (
                          <span className="graph-monitor-value">{traceValues.get(node.label)}</span>
                        ) : null}
                      </button>
                      {index < visibleNodes.length - 1 ? (
                        <div className={`ld-wire ${energized ? "energized" : ""}`} aria-hidden="true" />
                      ) : null}
                    </Fragment>
                  );
                })}
                <div className="ld-rail" aria-hidden="true" />
              </div>
            </div>
          ) : (
            <div className="fbd-view">
              <div className="fbd-row">
                {network.nodes.map((node, index) => {
                  const active = traceValue(traceLabels, node.label);
                  const selected = selection?.kind === "node" && selection.stableId === node.stableId;
                  const connecting = connectSource?.kind === "node" && connectSource.stableId === node.stableId;
                  const prevActive =
                    index > 0 ? traceValue(traceLabels, network.nodes[index - 1]?.label ?? null) : false;
                  return (
                    <Fragment key={node.stableId}>
                      {index > 0 ? <div className={`fbd-arrow ${prevActive && active ? "active" : ""}`} aria-hidden="true" /> : null}
                      <button
                        type="button"
                        className={`node-block ${node.kind} ${active ? "active" : ""} ${selected ? "selected" : ""} ${connecting ? "connecting" : ""}`}
                        draggable={Boolean(node.label)}
                        onDragStart={() => {
                          dragLabelRef.current = node.label;
                        }}
                        onDragOver={(event) => {
                          if (dragLabelRef.current) {
                            event.preventDefault();
                          }
                        }}
                        onDrop={(event) => {
                          event.preventDefault();
                          handleDropOnNode(node.label);
                        }}
                        onClick={() => onSelect({ kind: "node", stableId: node.stableId, networkId: network.id })}
                      >
                        <strong>{node.label}</strong>
                        {node.label && traceValues.has(node.label) ? (
                          <span className="graph-monitor-value">{traceValues.get(node.label)}</span>
                        ) : null}
                        <span>{node.kind}</span>
                      </button>
                    </Fragment>
                  );
                })}
              </div>
              {network.edges.length > 0 ? (
                <div className="graph-edge-list" aria-label="Connections">
                  {network.edges.map((edge) => (
                    <span key={edge.connectorId} className="graph-edge-chip">
                      {edge.from} → {edge.to}
                    </span>
                  ))}
                </div>
              ) : null}
            </div>
          )}
        </section>
      ))}
    </div>
  );
});

const SfcGraphView = memo(function SfcGraphView({
  sfc,
  selection,
  activeSteps,
  onSelect
}: {
  sfc: NonNullable<GraphModel["pous"][number]["sfc"]>;
  selection: GraphSelection | null;
  activeSteps: Set<string>;
  onSelect: (selection: GraphSelection) => void;
}) {
  const actionByName = useMemo(() => new Map(sfc.actions.map((action) => [action.name, action])), [sfc.actions]);
  const transitionsByStep = useMemo(() => {
    const map = new Map<string, typeof sfc.transitions>();
    for (const transition of sfc.transitions) {
      for (const stepName of transition.from) {
        const bucket = map.get(stepName) ?? [];
        bucket.push(transition);
        map.set(stepName, bucket);
      }
    }
    return map;
  }, [sfc.transitions]);

  return (
    <div className="sfc-view">
      {sfc.steps.map((step, stepIndex) => {
        const active = activeSteps.has(step.name) || (step.initial && activeSteps.size === 0);
        const selected = selection?.kind === "step" && selection.stableId === step.stableId;
        const stepActions = step.actions
          .map((actionName) => actionByName.get(actionName))
          .filter((action): action is NonNullable<typeof action> => Boolean(action));
        const stepTransitions = transitionsByStep.get(step.name) ?? [];

        return (
          <Fragment key={step.stableId}>
            {stepIndex > 0 ? <div className="sfc-arrow" aria-hidden="true" /> : null}
            <button
              type="button"
              className={`sfc-step ${step.initial ? "initial" : ""} ${active ? "active" : ""} ${selected ? "selected" : ""}`}
              onClick={() => onSelect({ kind: "step", stableId: step.stableId })}
            >
              {step.name}
            </button>
            {stepActions.map((action) => {
              const actionSelected = selection?.kind === "action" && selection.stableId === action.stableId;
              const actionActive = activeSteps.has(step.name);
              return (
                <Fragment key={action.stableId}>
                  <div className="sfc-arrow sfc-arrow-action" aria-hidden="true" />
                  <button
                    type="button"
                    className={`sfc-action ${actionActive ? "active" : ""} ${actionSelected ? "selected" : ""}`}
                    onClick={() => onSelect({ kind: "action", stableId: action.stableId })}
                  >
                    {action.name} / {action.qualifier}
                  </button>
                </Fragment>
              );
            })}
            {stepTransitions.map((transition) => {
              const transitionSelected =
                selection?.kind === "transition" && selection.stableId === transition.stableId;
              return (
                <Fragment key={transition.stableId}>
                  <div className="sfc-arrow" aria-hidden="true" />
                  <button
                    type="button"
                    className={`sfc-transition ${transitionSelected ? "selected" : ""}`}
                    onClick={() => onSelect({ kind: "transition", stableId: transition.stableId })}
                  >
                    {transition.name ?? transition.stableId}
                  </button>
                </Fragment>
              );
            })}
          </Fragment>
        );
      })}
    </div>
  );
});

const PlcopenGraphView = memo(function PlcopenGraphView({
  model,
  selection,
  traceLabels,
  onSelect
}: {
  model: GraphModel;
  selection: GraphSelection | null;
  traceLabels: Set<string>;
  onSelect: (selection: GraphSelection) => void;
}) {
  const pou = model.pous[0];
  if (!pou) {
    return null;
  }

  return (
    <div className="plcopen-view">
      {pou.networks.map((network) =>
        network.nodes.map((node) => {
          const active = traceValue(traceLabels, node.label);
          const selected = selection?.kind === "node" && selection.stableId === node.stableId;
          return (
            <button
              key={node.stableId}
              type="button"
              className={`xml-card ${active ? "active" : ""} ${selected ? "selected" : ""}`}
              onClick={() => onSelect({ kind: "node", stableId: node.stableId, networkId: network.id })}
            >
              <strong>{node.label}</strong>
              <span>{node.kind}</span>
              {node.attributes.localId ? <span>localId {node.attributes.localId}</span> : null}
            </button>
          );
        })
      )}
      {model.plcopenLayout.vendorAddData.length > 0 ? (
        <div className="plcopen-meta">
          {model.plcopenLayout.vendorAddData.map((entry) => (
            <span key={entry}>{entry}</span>
          ))}
        </div>
      ) : null}
    </div>
  );
});
