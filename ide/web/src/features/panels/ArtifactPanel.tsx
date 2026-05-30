import { useMemo, useState } from "react";
import { Button } from "@/components/ui/button";
import { ConfirmDialog } from "@/features/dialogs/ConfirmDialog";
import { RenameDialog } from "@/features/dialogs/RenameDialog";
import { artifactKindLabel, isArtifactStale } from "@/lib/artifactLifecycle";
import { compareSummary, diffLines, findComparableArtifacts } from "@/lib/artifactCompare";
import { downloadArtifact } from "@/stores/artifactStore";
import type { Project, ProjectArtifact } from "@/types";

type ArtifactPanelProps = {
  project: Project;
  artifacts: ProjectArtifact[];
  selectedArtifact: ProjectArtifact | null;
  onSelectArtifact: (artifactId: string | null) => void;
  onDeleteArtifact: (artifactId: string) => void;
  onRenameArtifact: (artifactId: string, nextName: string) => void;
  onClearArtifacts: () => void;
  onRevealSource: (fileName: string) => void;
};

export function ArtifactPanel({
  project,
  artifacts,
  selectedArtifact,
  onSelectArtifact,
  onDeleteArtifact,
  onRenameArtifact,
  onClearArtifacts,
  onRevealSource
}: ArtifactPanelProps) {
  const [compareTargetId, setCompareTargetId] = useState<string | null>(null);
  const [clearConfirmOpen, setClearConfirmOpen] = useState(false);
  const [renameArtifactId, setRenameArtifactId] = useState<string | null>(null);

  const renameArtifact = useMemo(
    () => artifacts.find((entry) => entry.id === renameArtifactId) ?? null,
    [artifacts, renameArtifactId]
  );

  const compareTarget = useMemo(
    () => artifacts.find((entry) => entry.id === compareTargetId) ?? null,
    [artifacts, compareTargetId]
  );

  const compareLines = useMemo(() => {
    if (!selectedArtifact || !compareTarget) {
      return [];
    }
    return diffLines(compareTarget.content, selectedArtifact.content).filter((row) => row.kind !== "same");
  }, [compareTarget, selectedArtifact]);

  const comparable = selectedArtifact ? findComparableArtifacts(artifacts, selectedArtifact) : [];

  return (
    <div className="artifact-panel">
      <ConfirmDialog
        open={clearConfirmOpen}
        title="Clear generated artifacts"
        description="Remove all generated artifacts for this project. Source files are not affected."
        confirmLabel="Clear all"
        destructive
        onOpenChange={setClearConfirmOpen}
        onConfirm={onClearArtifacts}
      />
      <RenameDialog
        open={renameArtifact !== null}
        title="Rename artifact"
        description="Choose a display name for the generated artifact."
        currentName={renameArtifact?.name ?? ""}
        fieldLabel="Artifact name"
        validate={(next) => (next.length > 0 ? null : "Name is required.")}
        onOpenChange={(open) => {
          if (!open) {
            setRenameArtifactId(null);
          }
        }}
        onSubmit={(nextName) => {
          if (renameArtifact) {
            onRenameArtifact(renameArtifact.id, nextName);
            setRenameArtifactId(null);
          }
        }}
      />
      <div className="panel-inline-actions">
        <button
          type="button"
          className="panel-action-btn"
          disabled={artifacts.length === 0}
          onClick={() => {
            for (const artifact of artifacts) {
              downloadArtifact(artifact);
            }
          }}
        >
          Download all
        </button>
        <button
          type="button"
          className="panel-action-btn destructive"
          disabled={artifacts.length === 0}
          onClick={() => setClearConfirmOpen(true)}
        >
          Clear generated
        </button>
      </div>
      <div className="artifact-table">
        <div className="table-head artifact-table-head">
          <span>Name</span>
          <span>Kind</span>
          <span>Source</span>
          <span>Status</span>
          <span>Actions</span>
        </div>
        {artifacts.length === 0 ? (
          <div className="empty-row">No generated artifacts yet. Run Check, Build C, or Export to create artifacts.</div>
        ) : (
          artifacts.map((artifact) => {
            const stale = isArtifactStale(artifact, project);
            const selected = selectedArtifact?.id === artifact.id;
            return (
              <div className={`table-row artifact-table-row${selected ? " selected" : ""}`} key={artifact.id}>
                <button type="button" className="artifact-name-btn" onClick={() => onSelectArtifact(artifact.id)}>
                  {artifact.name}
                </button>
                <span>{artifactKindLabel(artifact.kind)}</span>
                <button type="button" className="artifact-link-btn" onClick={() => onRevealSource(artifact.sourceFile)}>
                  {artifact.sourceFile}
                </button>
                <span className={stale ? "artifact-stale" : "artifact-current"}>{stale ? "stale" : "current"}</span>
                <div className="artifact-actions">
                  <Button type="button" variant="ghost" size="sm" onClick={() => downloadArtifact(artifact)}>
                    Download
                  </Button>
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() => setRenameArtifactId(artifact.id)}
                  >
                    Rename
                  </Button>
                  <Button type="button" variant="ghost" size="sm" onClick={() => onDeleteArtifact(artifact.id)}>
                    Delete
                  </Button>
                </div>
              </div>
            );
          })
        )}
      </div>
      {selectedArtifact ? (
        <>
          {comparable.length > 0 ? (
            <div className="artifact-compare-controls">
              <label>
                Compare with
                <select
                  value={compareTargetId ?? ""}
                  onChange={(event) => setCompareTargetId(event.target.value || null)}
                >
                  <option value="">Select artifact…</option>
                  {comparable.map((artifact) => (
                    <option key={artifact.id} value={artifact.id}>
                      {artifact.name}
                    </option>
                  ))}
                </select>
              </label>
              {compareTarget ? <span className="artifact-compare-summary">{compareSummary(compareTarget, selectedArtifact)}</span> : null}
            </div>
          ) : null}
          {compareLines.length > 0 ? (
            <pre className="artifact-compare-view" aria-label="Artifact compare diff">
              {compareLines
                .slice(0, 120)
                .map((row) => {
                  if (row.kind === "add") {
                    return `+ ${row.line}: ${row.right}`;
                  }
                  if (row.kind === "remove") {
                    return `- ${row.line}: ${row.left}`;
                  }
                  return `~ ${row.line}: ${row.left} -> ${row.right}`;
                })
                .join("\n")}
            </pre>
          ) : (
            <pre className="output-log artifact-preview" aria-label={`Artifact ${selectedArtifact.name}`}>
              {selectedArtifact.content}
            </pre>
          )}
        </>
      ) : null}
    </div>
  );
}
