import { useMemo, useState } from "react";
import { Download, FileCode2, MoreHorizontal, Pencil, Trash2 } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger
} from "@/components/ui/dropdown-menu";
import { ScrollArea } from "@/components/ui/scroll-area";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue
} from "@/components/ui/select";
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

function formatArtifactTime(iso: string): string {
  return new Date(iso).toLocaleString(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit"
  });
}

function ArtifactRowMenu({
  artifact,
  onDownload,
  onRename,
  onDelete
}: {
  artifact: ProjectArtifact;
  onDownload: () => void;
  onRename: () => void;
  onDelete: () => void;
}) {
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className="artifact-row-menu-btn"
          aria-label={`Actions for ${artifact.name}`}
          onClick={(event) => event.stopPropagation()}
        >
          <MoreHorizontal aria-hidden="true" />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="artifact-row-menu">
        <DropdownMenuItem onClick={onDownload}>
          <Download aria-hidden="true" />
          Download
        </DropdownMenuItem>
        <DropdownMenuItem onClick={onRename}>
          <Pencil aria-hidden="true" />
          Rename
        </DropdownMenuItem>
        <DropdownMenuSeparator />
        <DropdownMenuItem variant="destructive" onClick={onDelete}>
          <Trash2 aria-hidden="true" />
          Delete
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

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

  const previewContent = useMemo(() => {
    if (!selectedArtifact) {
      return "";
    }
    if (compareLines.length > 0) {
      return compareLines
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
        .join("\n");
    }
    return selectedArtifact.content;
  }, [compareLines, selectedArtifact]);

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

      <header className="artifact-toolbar">
        <div className="artifact-toolbar-meta">
          <FileCode2 size={14} aria-hidden="true" />
          <span className="artifact-toolbar-title">Generated artifacts</span>
          <Badge variant="secondary">{artifacts.length}</Badge>
        </div>
        <div className="artifact-toolbar-actions">
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
      </header>

      {artifacts.length === 0 ? (
        <div className="artifact-empty">
          <FileCode2 size={28} strokeWidth={1.5} aria-hidden="true" />
          <p>No generated artifacts yet.</p>
          <small>Run Check, Build C, simulate, or export to create artifacts for this project.</small>
        </div>
      ) : (
        <div className="artifact-layout">
          <section className="artifact-list" aria-label="Artifact list">
            <div className="artifact-list-head">
              <span>Name</span>
              <span>Kind</span>
              <span>Status</span>
              <span className="sr-only">Actions</span>
            </div>
            <ScrollArea className="artifact-list-scroll">
              <div className="artifact-list-body">
                {artifacts.map((artifact) => {
                  const stale = isArtifactStale(artifact, project);
                  const selected = selectedArtifact?.id === artifact.id;
                  return (
                    <div
                      key={artifact.id}
                      className={`artifact-list-row${selected ? " selected" : ""}`}
                    >
                      <div className="artifact-list-primary">
                        <button
                          type="button"
                          className="artifact-list-name-btn"
                          aria-pressed={selected}
                          onClick={() => onSelectArtifact(artifact.id)}
                        >
                          {artifact.name}
                        </button>
                        <div className="artifact-list-meta">
                          <button
                            type="button"
                            className="artifact-source-link"
                            title={`Open source ${artifact.sourceFile}`}
                            onClick={() => onRevealSource(artifact.sourceFile)}
                          >
                            {artifact.sourceFile}
                          </button>
                          <span className="artifact-list-time">{formatArtifactTime(artifact.createdAt)}</span>
                        </div>
                      </div>
                      <span className="artifact-list-kind">{artifactKindLabel(artifact.kind)}</span>
                      <Badge
                        variant="outline"
                        className={stale ? "artifact-status-stale" : "artifact-status-current"}
                      >
                        {stale ? "stale" : "current"}
                      </Badge>
                      <ArtifactRowMenu
                        artifact={artifact}
                        onDownload={() => downloadArtifact(artifact)}
                        onRename={() => setRenameArtifactId(artifact.id)}
                        onDelete={() => onDeleteArtifact(artifact.id)}
                      />
                    </div>
                  );
                })}
              </div>
            </ScrollArea>
          </section>

          <section className="artifact-detail" aria-label="Artifact preview">
            {selectedArtifact ? (
              <>
                <header className="artifact-detail-head">
                  <div className="artifact-detail-title">
                    <strong title={selectedArtifact.name}>{selectedArtifact.name}</strong>
                    <span>{artifactKindLabel(selectedArtifact.kind)}</span>
                  </div>
                  <div className="artifact-detail-controls">
                    {comparable.length > 0 ? (
                      <Select
                        value={compareTargetId ?? "__none__"}
                        onValueChange={(value) => setCompareTargetId(value === "__none__" ? null : value)}
                      >
                        <SelectTrigger size="sm" className="artifact-compare-select" aria-label="Compare with">
                          <SelectValue placeholder="Compare with…" />
                        </SelectTrigger>
                        <SelectContent>
                          <SelectItem value="__none__">None</SelectItem>
                          {comparable.map((artifact) => (
                            <SelectItem key={artifact.id} value={artifact.id}>
                              {artifact.name}
                            </SelectItem>
                          ))}
                        </SelectContent>
                      </Select>
                    ) : null}
                    {compareTarget ? (
                      <span className="artifact-compare-summary">
                        {compareSummary(compareTarget, selectedArtifact)}
                      </span>
                    ) : null}
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() => downloadArtifact(selectedArtifact)}
                    >
                      <Download aria-hidden="true" />
                      Download
                    </Button>
                  </div>
                </header>
                <ScrollArea className="artifact-detail-scroll">
                  <pre
                    className={
                      compareLines.length > 0 ? "artifact-compare-view artifact-detail-code" : "artifact-detail-code"
                    }
                    aria-label={
                      compareLines.length > 0
                        ? "Artifact compare diff"
                        : `Artifact ${selectedArtifact.name}`
                    }
                  >
                    {previewContent}
                  </pre>
                </ScrollArea>
              </>
            ) : (
              <div className="artifact-detail-empty">
                <p>Select an artifact to preview its contents.</p>
              </div>
            )}
          </section>
        </div>
      )}
    </div>
  );
}
