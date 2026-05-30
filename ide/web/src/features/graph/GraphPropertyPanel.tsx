import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
import { useMemo } from "react";
import type { GraphModel, GraphSelection } from "@/features/graph/graphTypes";
import { selectionLabel } from "@/features/graph/graphEdits";
import type { GraphValidation } from "@/features/graph/validateGraph";
import type { WorkspaceFile } from "@/types";

type GraphPropertyPanelProps = {
  file: WorkspaceFile;
  model: GraphModel;
  selection: GraphSelection | null;
  validation?: GraphValidation | null;
  onApplyRename: (currentLabel: string, nextLabel: string) => void;
  onApplyProperty: (property: string, value: string) => void;
  onMoveStep: (stepName: string, direction: "up" | "down") => void;
  onToggleLdContact?: (label: string) => void;
};

export function GraphPropertyPanel({
  file,
  model,
  selection,
  validation,
  onApplyRename,
  onApplyProperty,
  onMoveStep,
  onToggleLdContact
}: GraphPropertyPanelProps) {
  const selectedLabel = selectionLabel(file, model, selection);
  const pou = model.pous[0];
  const selectionIssues = useMemo(() => {
    if (!validation?.diagnostics.length || !selectedLabel) {
      return [];
    }
    const needle = selectedLabel.toLowerCase();
    return validation.diagnostics.filter((issue) => issue.message.toLowerCase().includes(needle));
  }, [selectedLabel, validation?.diagnostics]);

  if (!selection || !pou) {
    return (
      <aside className="graph-property-panel" aria-label="Graph properties">
        <p className="graph-property-empty">Select a rung element, network node, or SFC step to edit properties.</p>
      </aside>
    );
  }

  let title = "Element";
  let elementKind = "element";
  let qualifier = "";
  let comment = "";
  let variableName = "";
  let iecType = "";
  let formalParameter = "";
  let negated = false;
  let coilMode = "normal";
  let transitionFrom = "";
  let transitionTo = "";
  let selectedNode: (typeof pou.networks)[number]["nodes"][number] | null = null;

  if (selection.kind === "node") {
    const network = pou.networks.find((entry) => entry.id === selection.networkId) ?? pou.networks[0];
    const node = network?.nodes.find((entry) => entry.stableId === selection.stableId);
    selectedNode = node ?? null;
    title = node?.label ?? "Node";
    elementKind = node?.kind ?? "node";
    comment = node?.attributes.comment ?? "";
    variableName = node?.attributes.variable ?? node?.label ?? "";
    iecType = node?.attributes.type ?? "—";
    formalParameter = node?.attributes.formalParameter ?? node?.attributes.typeName ?? "";
    negated = node?.attributes.negated === "true";
    coilMode = node?.attributes.storage ?? "normal";
  } else if (selection.kind === "step") {
    const step = pou.sfc?.steps.find((entry) => entry.stableId === selection.stableId);
    title = step?.name ?? "Step";
    elementKind = step?.initial ? "initial step" : "step";
  } else if (selection.kind === "transition") {
    const transition = pou.sfc?.transitions.find((entry) => entry.stableId === selection.stableId);
    title = transition?.name ?? "Transition";
    elementKind = "transition";
    transitionFrom = transition?.from.join(", ") ?? "";
    transitionTo = transition?.to.join(", ") ?? "";
  } else if (selection.kind === "action") {
    const action = pou.sfc?.actions.find((entry) => entry.stableId === selection.stableId);
    title = action?.name ?? "Action";
    elementKind = "action";
    qualifier = action?.qualifier ?? "";
  }

  return (
    <aside className="graph-property-panel" aria-label="Graph properties">
      <h3>Properties</h3>
      <div className="graph-property-grid">
        <Label htmlFor="graph-prop-kind">Kind</Label>
        <Input id="graph-prop-kind" value={elementKind} readOnly />
        <Label htmlFor="graph-prop-label">Label</Label>
        <Input
          id="graph-prop-label"
          defaultValue={selectedLabel ?? title}
          key={selectedLabel ?? title}
          onBlur={(event) => {
            const next = event.target.value.trim();
            if (selectedLabel && next && next !== selectedLabel) {
              onApplyRename(selectedLabel, next);
            }
          }}
        />
        {selection.kind === "node" && file.languageId === "ld" ? (
          <>
            <Label htmlFor="graph-prop-variable">Variable</Label>
            <Input id="graph-prop-variable" value={variableName} readOnly />
            <Label htmlFor="graph-prop-polarity">Contact polarity</Label>
            <Input id="graph-prop-polarity" value={negated ? "Negated (NOT)" : "Normal"} readOnly />
            {selectedNode?.kind === "contact" && selectedLabel && onToggleLdContact ? (
              <Button type="button" size="sm" variant="secondary" onClick={() => onToggleLdContact(selectedLabel)}>
                Toggle NOT
              </Button>
            ) : null}
            {selectedNode?.kind === "coil" ? (
              <>
                <Label htmlFor="graph-prop-coil-mode">Coil mode</Label>
                <Select
                  value={coilMode}
                  onValueChange={(value) => onApplyProperty("coil-mode", value)}
                >
                  <SelectTrigger id="graph-prop-coil-mode" size="sm" className="w-full">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="normal">Normal (COIL)</SelectItem>
                    <SelectItem value="set">Set (SET)</SelectItem>
                    <SelectItem value="reset">Reset (RESET)</SelectItem>
                  </SelectContent>
                </Select>
              </>
            ) : null}
          </>
        ) : null}
        {selection.kind === "node" && (file.languageId === "fbd" || file.languageId === "xml") ? (
          <>
            <Label htmlFor="graph-prop-expression">Expression</Label>
            <Input id="graph-prop-expression" value={selectedNode?.attributes.value ?? selectedNode?.attributes.expression ?? ""} readOnly />
            {formalParameter ? (
              <>
                <Label htmlFor="graph-prop-formal">Formal parameter</Label>
                <Input id="graph-prop-formal" value={formalParameter} readOnly />
              </>
            ) : null}
            {selectedNode?.attributes.localId ? (
              <>
                <Label htmlFor="graph-prop-local-id">PLCopen localId</Label>
                <Input id="graph-prop-local-id" value={selectedNode.attributes.localId} readOnly />
              </>
            ) : null}
          </>
        ) : null}
        {selection.kind === "node" && iecType !== "—" ? (
          <>
            <Label htmlFor="graph-prop-type">IEC type</Label>
            <Input id="graph-prop-type" value={iecType} readOnly />
          </>
        ) : null}
        {selection.kind === "action" ? (
          <>
            <Label htmlFor="graph-prop-qualifier">Qualifier</Label>
            <Input
              id="graph-prop-qualifier"
              defaultValue={qualifier}
              key={`${title}-${qualifier}`}
              onBlur={(event) => onApplyProperty("qualifier", event.target.value.trim())}
            />
          </>
        ) : null}
        {selection.kind === "transition" ? (
          <>
            <Label htmlFor="graph-prop-from">From steps</Label>
            <Input id="graph-prop-from" value={transitionFrom} readOnly />
            <Label htmlFor="graph-prop-to">To steps</Label>
            <Input id="graph-prop-to" value={transitionTo} readOnly />
          </>
        ) : null}
        {selectionIssues.length > 0 ? (
          <div className="graph-property-validation" role="status">
            {selectionIssues.map((issue) => (
              <p key={`${issue.message}:${issue.severity}`} className={`graph-property-issue-${issue.severity}`}>
                {issue.message}
              </p>
            ))}
          </div>
        ) : null}
        <Label htmlFor="graph-prop-comment">Comment</Label>
        <Input
          id="graph-prop-comment"
          defaultValue={comment}
          placeholder="Optional note"
          key={`${title}-comment`}
          onBlur={(event) => onApplyProperty("comment", event.target.value.trim())}
        />
      </div>
      {selection.kind === "step" && selectedLabel ? (
        <div className="graph-property-actions">
          <Button type="button" size="sm" variant="secondary" onClick={() => onMoveStep(selectedLabel, "up")}>
            Move step up
          </Button>
          <Button type="button" size="sm" variant="secondary" onClick={() => onMoveStep(selectedLabel, "down")}>
            Move step down
          </Button>
        </div>
      ) : null}
    </aside>
  );
}
